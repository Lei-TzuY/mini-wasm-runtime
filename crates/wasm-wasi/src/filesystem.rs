use std::{cell::RefCell, collections::BTreeMap, fmt, rc::Rc};

use wasm_parser::ValueType;
use wasm_runtime::{HostCapabilities, HostError, HostRegistry, HostRegistryError, Value};

use crate::{
    ERRNO_BADF, ERRNO_FAULT, ERRNO_INVAL, ERRNO_MFILE, ERRNO_NAMETOOLONG, ERRNO_NOENT,
    ERRNO_NOTCAPABLE, ERRNO_SUCCESS, FILETYPE_REGULAR_FILE, RIGHTS_FD_READ,
};

const WASI_MODULE: &str = "wasi_snapshot_preview1";
const PATH_OPEN_NAME: &str = "path_open";
const FD_CLOSE_NAME: &str = "fd_close";
const FIRST_DYNAMIC_FD: u32 = 3;
const MAX_MOUNTED_FILES: usize = 4_096;
const MAX_RELATIVE_PATH_BYTES: usize = 4 * 1024;
const MAX_FILE_BYTES: usize = 16 * 1024 * 1024;
const MAX_OPEN_FILES: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WasiFilesystemError {
    UnknownPreopen { guest_path: String },
    EmptyRelativePath,
    RelativePathTooLong { length: usize, limit: usize },
    UnsafeRelativePath,
    FileTooLarge { length: usize, limit: usize },
    TooManyFiles { limit: usize },
    DuplicateFile,
}

impl fmt::Display for WasiFilesystemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownPreopen { guest_path } => {
                write!(f, "WASI preopen {guest_path:?} is not configured")
            }
            Self::EmptyRelativePath => write!(f, "WASI mounted file path must not be empty"),
            Self::RelativePathTooLong { length, limit } => write!(
                f,
                "WASI mounted file path is {length} bytes, exceeding the {limit}-byte limit"
            ),
            Self::UnsafeRelativePath => write!(
                f,
                "WASI mounted file path must be a traversal-safe relative path"
            ),
            Self::FileTooLarge { length, limit } => write!(
                f,
                "WASI mounted file is {length} bytes, exceeding the {limit}-byte limit"
            ),
            Self::TooManyFiles { limit } => {
                write!(
                    f,
                    "WASI mounted file count exceeds the fixed limit of {limit}"
                )
            }
            Self::DuplicateFile => write!(f, "WASI mounted file path is already configured"),
        }
    }
}

impl std::error::Error for WasiFilesystemError {}

#[derive(Debug, Clone)]
struct MountedFile {
    preopen_fd: u32,
    relative_path: Vec<u8>,
    bytes: Rc<Vec<u8>>,
}

#[derive(Debug, Clone)]
struct OpenFile {
    bytes: Rc<Vec<u8>>,
    offset: usize,
    rights_base: u64,
}

#[derive(Debug, Default)]
struct FilesystemState {
    reserved_preopens: Vec<u32>,
    mounted_files: Vec<MountedFile>,
    open_files: BTreeMap<u32, OpenFile>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Filesystem {
    state: Rc<RefCell<FilesystemState>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DescriptorReadError {
    BadFd,
    NotCapable,
}

impl Filesystem {
    pub(crate) fn reserve_preopen(&self, fd: u32) {
        let mut state = self.state.borrow_mut();
        if !state.reserved_preopens.contains(&fd) {
            state.reserved_preopens.push(fd);
            state.reserved_preopens.sort_unstable();
        }
    }

    pub(crate) fn mount_file(
        &self,
        preopen_fd: u32,
        relative_path: &[u8],
        bytes: &[u8],
    ) -> Result<(), WasiFilesystemError> {
        validate_configured_path(relative_path)?;
        if bytes.len() > MAX_FILE_BYTES {
            return Err(WasiFilesystemError::FileTooLarge {
                length: bytes.len(),
                limit: MAX_FILE_BYTES,
            });
        }

        let mut state = self.state.borrow_mut();
        if state.mounted_files.len() >= MAX_MOUNTED_FILES {
            return Err(WasiFilesystemError::TooManyFiles {
                limit: MAX_MOUNTED_FILES,
            });
        }
        if state.mounted_files.iter().any(|file| {
            file.preopen_fd == preopen_fd && file.relative_path.as_slice() == relative_path
        }) {
            return Err(WasiFilesystemError::DuplicateFile);
        }

        state.mounted_files.push(MountedFile {
            preopen_fd,
            relative_path: relative_path.to_vec(),
            bytes: Rc::new(bytes.to_vec()),
        });
        Ok(())
    }

    pub(crate) fn ensure_readable(&self, fd: u32) -> Result<(), DescriptorReadError> {
        let state = self.state.borrow();
        let Some(file) = state.open_files.get(&fd) else {
            return Err(DescriptorReadError::BadFd);
        };
        if file.rights_base & RIGHTS_FD_READ == 0 {
            return Err(DescriptorReadError::NotCapable);
        }
        Ok(())
    }

    pub(crate) fn peek(&self, fd: u32, max_len: usize) -> Result<Vec<u8>, DescriptorReadError> {
        let state = self.state.borrow();
        let Some(file) = state.open_files.get(&fd) else {
            return Err(DescriptorReadError::BadFd);
        };
        if file.rights_base & RIGHTS_FD_READ == 0 {
            return Err(DescriptorReadError::NotCapable);
        }
        let remaining = file.bytes.len().saturating_sub(file.offset);
        let len = remaining.min(max_len);
        Ok(file.bytes[file.offset..file.offset + len].to_vec())
    }

    pub(crate) fn advance(&self, fd: u32, len: usize) -> Result<(), DescriptorReadError> {
        let mut state = self.state.borrow_mut();
        let Some(file) = state.open_files.get_mut(&fd) else {
            return Err(DescriptorReadError::BadFd);
        };
        if file.rights_base & RIGHTS_FD_READ == 0 {
            return Err(DescriptorReadError::NotCapable);
        }
        file.offset = file.offset.saturating_add(len).min(file.bytes.len());
        Ok(())
    }

    pub(crate) fn fdstat(&self, fd: u32) -> Option<(u8, u64, u64)> {
        let state = self.state.borrow();
        state
            .open_files
            .get(&fd)
            .map(|file| (FILETYPE_REGULAR_FILE, file.rights_base, 0))
    }

    pub(crate) fn register(&self, registry: &mut HostRegistry) -> Result<(), HostRegistryError> {
        let open_filesystem = self.clone();
        registry.register_values(
            WASI_MODULE,
            PATH_OPEN_NAME,
            vec![
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
                ValueType::I64,
                ValueType::I64,
                ValueType::I32,
                ValueType::I32,
            ],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ_WRITE,
            move |context, args| {
                let [
                    Value::I32(dir_fd),
                    Value::I32(dir_flags),
                    Value::I32(path_ptr),
                    Value::I32(path_len),
                    Value::I32(open_flags),
                    Value::I64(rights_base),
                    Value::I64(rights_inheriting),
                    Value::I32(fd_flags),
                    Value::I32(opened_fd_ptr),
                ] = args
                else {
                    return Err(HostError::message(
                        "validated wasi path_open signature received invalid arguments",
                    ));
                };

                let dir_fd = *dir_fd as u32;
                if !open_filesystem.has_preopen(dir_fd) {
                    return Ok(vec![Value::I32(ERRNO_BADF)]);
                }
                if *dir_flags != 0 || *open_flags != 0 || *fd_flags != 0 {
                    return Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]);
                }

                let requested_base = *rights_base as u64;
                let requested_inheriting = *rights_inheriting as u64;
                if requested_base & !RIGHTS_FD_READ != 0 || requested_inheriting != 0 {
                    return Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]);
                }

                let path_len = *path_len as u32 as usize;
                if path_len > MAX_RELATIVE_PATH_BYTES {
                    return Ok(vec![Value::I32(ERRNO_NAMETOOLONG)]);
                }
                let path = match context.read_memory(*path_ptr as u32, path_len) {
                    Ok(path) => path,
                    Err(_) => return Ok(vec![Value::I32(ERRNO_FAULT)]),
                };
                match validate_guest_path(&path) {
                    Ok(()) => {}
                    Err(GuestPathError::Empty) => return Ok(vec![Value::I32(ERRNO_INVAL)]),
                    Err(GuestPathError::TooLong) => {
                        return Ok(vec![Value::I32(ERRNO_NAMETOOLONG)]);
                    }
                    Err(GuestPathError::Unsafe) => {
                        return Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]);
                    }
                }

                if context.read_memory(*opened_fd_ptr as u32, 4).is_err() {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }

                let fd = match open_filesystem.open(dir_fd, &path, requested_base) {
                    Ok(Some(fd)) => fd,
                    Ok(None) => return Ok(vec![Value::I32(ERRNO_NOENT)]),
                    Err(()) => return Ok(vec![Value::I32(ERRNO_MFILE)]),
                };

                if context
                    .write_memory(*opened_fd_ptr as u32, &fd.to_le_bytes())
                    .is_err()
                {
                    open_filesystem.close(fd);
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }

                Ok(vec![Value::I32(ERRNO_SUCCESS)])
            },
        )?;

        let close_filesystem = self.clone();
        registry.register_values(
            WASI_MODULE,
            FD_CLOSE_NAME,
            vec![ValueType::I32],
            vec![ValueType::I32],
            HostCapabilities::NONE,
            move |_context, args| {
                let [Value::I32(fd)] = args else {
                    return Err(HostError::message(
                        "validated wasi fd_close signature received invalid arguments",
                    ));
                };

                if close_filesystem.close(*fd as u32) {
                    Ok(vec![Value::I32(ERRNO_SUCCESS)])
                } else {
                    Ok(vec![Value::I32(ERRNO_BADF)])
                }
            },
        )
    }

    fn has_preopen(&self, fd: u32) -> bool {
        self.state.borrow().reserved_preopens.contains(&fd)
    }

    fn open(&self, preopen_fd: u32, path: &[u8], rights_base: u64) -> Result<Option<u32>, ()> {
        let mut state = self.state.borrow_mut();
        let Some(bytes) = state
            .mounted_files
            .iter()
            .find(|file| file.preopen_fd == preopen_fd && file.relative_path.as_slice() == path)
            .map(|file| file.bytes.clone())
        else {
            return Ok(None);
        };

        if state.open_files.len() >= MAX_OPEN_FILES {
            return Err(());
        }

        let mut candidate = FIRST_DYNAMIC_FD;
        loop {
            if !state.reserved_preopens.contains(&candidate)
                && !state.open_files.contains_key(&candidate)
            {
                break;
            }
            candidate = candidate.checked_add(1).ok_or(())?;
        }

        state.open_files.insert(
            candidate,
            OpenFile {
                bytes,
                offset: 0,
                rights_base,
            },
        );
        Ok(Some(candidate))
    }

    fn close(&self, fd: u32) -> bool {
        self.state.borrow_mut().open_files.remove(&fd).is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GuestPathError {
    Empty,
    TooLong,
    Unsafe,
}

fn validate_configured_path(path: &[u8]) -> Result<(), WasiFilesystemError> {
    match validate_guest_path(path) {
        Ok(()) => Ok(()),
        Err(GuestPathError::Empty) => Err(WasiFilesystemError::EmptyRelativePath),
        Err(GuestPathError::TooLong) => Err(WasiFilesystemError::RelativePathTooLong {
            length: path.len(),
            limit: MAX_RELATIVE_PATH_BYTES,
        }),
        Err(GuestPathError::Unsafe) => Err(WasiFilesystemError::UnsafeRelativePath),
    }
}

fn validate_guest_path(path: &[u8]) -> Result<(), GuestPathError> {
    if path.is_empty() {
        return Err(GuestPathError::Empty);
    }
    if path.len() > MAX_RELATIVE_PATH_BYTES {
        return Err(GuestPathError::TooLong);
    }
    if path[0] == b'/' || path[path.len() - 1] == b'/' || path.contains(&0) {
        return Err(GuestPathError::Unsafe);
    }
    for component in path.split(|byte| *byte == b'/') {
        if component.is_empty() || component == b"." || component == b".." {
            return Err(GuestPathError::Unsafe);
        }
    }
    Ok(())
}
