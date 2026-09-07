use std::{cell::RefCell, collections::BTreeMap, fmt, rc::Rc};

use wasm_parser::ValueType;
use wasm_runtime::{HostCapabilities, HostError, HostRegistry, HostRegistryError, Value};

use crate::{
    ERRNO_BADF, ERRNO_FAULT, ERRNO_FBIG, ERRNO_INVAL, ERRNO_MFILE, ERRNO_NAMETOOLONG,
    ERRNO_NOENT, ERRNO_NOSPC, ERRNO_NOTCAPABLE, ERRNO_OVERFLOW, ERRNO_SUCCESS,
    FILETYPE_REGULAR_FILE, OFLAGS_CREAT, RIGHTS_FD_READ, RIGHTS_FD_SEEK, RIGHTS_FD_TELL,
    RIGHTS_FD_WRITE,
};

const WASI_MODULE: &str = "wasi_snapshot_preview1";
const PATH_OPEN_NAME: &str = "path_open";
const FD_CLOSE_NAME: &str = "fd_close";
const FD_SEEK_NAME: &str = "fd_seek";
const FD_TELL_NAME: &str = "fd_tell";
const FD_PWRITE_NAME: &str = "fd_pwrite";
const WHENCE_SET: u32 = 0;
const WHENCE_CUR: u32 = 1;
const WHENCE_END: u32 = 2;
const FIRST_DYNAMIC_FD: u32 = 3;
const MAX_MOUNTED_FILES: usize = 4_096;
const MAX_RELATIVE_PATH_BYTES: usize = 4 * 1024;
const MAX_FILE_BYTES: usize = 16 * 1024 * 1024;
const MAX_OPEN_FILES: usize = 256;
const MAX_PWRITE_IOVECS: u32 = 1_024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WasiFilesystemError {
    UnknownPreopen { guest_path: String },
    EmptyRelativePath,
    RelativePathTooLong { length: usize, limit: usize },
    UnsafeRelativePath,
    FileTooLarge { length: usize, limit: usize },
    TooManyFiles { limit: usize },
    DuplicateFile,
    ReadOnlyPreopen,
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
            Self::ReadOnlyPreopen => write!(f, "WASI writable files require a writable preopen"),
        }
    }
}

impl std::error::Error for WasiFilesystemError {}

#[derive(Debug, Clone)]
struct MountedFile {
    preopen_fd: u32,
    relative_path: Vec<u8>,
    bytes: Rc<RefCell<Vec<u8>>>,
    writable: bool,
}

#[derive(Debug, Clone)]
struct OpenFile {
    bytes: Rc<RefCell<Vec<u8>>>,
    offset: u64,
    rights_base: u64,
}

#[derive(Debug, Default)]
struct FilesystemState {
    reserved_preopens: Vec<u32>,
    writable_preopens: Vec<u32>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DescriptorPositionError {
    BadFd,
    NotCapable,
    InvalidWhence,
    InvalidOffset,
    Overflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DescriptorWriteError {
    BadFd,
    NotCapable,
    FileTooLarge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenError {
    NotFound,
    NotCapable,
    TooManyOpenFiles,
    TooManyFiles,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OpenedFile {
    fd: u32,
    created: bool,
}

impl Filesystem {
    pub(crate) fn reserve_preopen(&self, fd: u32, writable: bool) {
        let mut state = self.state.borrow_mut();
        if !state.reserved_preopens.contains(&fd) {
            state.reserved_preopens.push(fd);
            state.reserved_preopens.sort_unstable();
        }
        if writable && !state.writable_preopens.contains(&fd) {
            state.writable_preopens.push(fd);
            state.writable_preopens.sort_unstable();
        }
    }

    pub(crate) fn mount_file(
        &self,
        preopen_fd: u32,
        relative_path: &[u8],
        bytes: &[u8],
        writable: bool,
    ) -> Result<(), WasiFilesystemError> {
        validate_configured_path(relative_path)?;
        if writable && !self.is_writable_preopen(preopen_fd) {
            return Err(WasiFilesystemError::ReadOnlyPreopen);
        }
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
            bytes: Rc::new(RefCell::new(bytes.to_vec())),
            writable,
        });
        Ok(())
    }

    pub(crate) fn snapshot(&self, preopen_fd: u32, relative_path: &[u8]) -> Option<Vec<u8>> {
        let state = self.state.borrow();
        let file = state.mounted_files.iter().find(|file| {
            file.preopen_fd == preopen_fd && file.relative_path.as_slice() == relative_path
        })?;
        let bytes = file.bytes.borrow().clone();
        Some(bytes)
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
        let Ok(start) = usize::try_from(file.offset) else {
            return Ok(Vec::new());
        };
        let bytes = file.bytes.borrow();
        if start >= bytes.len() {
            return Ok(Vec::new());
        }
        let remaining = bytes.len() - start;
        let len = remaining.min(max_len);
        Ok(bytes[start..start + len].to_vec())
    }

    pub(crate) fn advance(&self, fd: u32, len: usize) -> Result<(), DescriptorReadError> {
        let mut state = self.state.borrow_mut();
        let Some(file) = state.open_files.get_mut(&fd) else {
            return Err(DescriptorReadError::BadFd);
        };
        if file.rights_base & RIGHTS_FD_READ == 0 {
            return Err(DescriptorReadError::NotCapable);
        }
        file.offset = file.offset.saturating_add(len as u64);
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
                let open_flags = *open_flags as u32;
                if *dir_flags != 0 || *fd_flags != 0 || open_flags & !OFLAGS_CREAT != 0 {
                    return Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]);
                }
                let create = open_flags & OFLAGS_CREAT != 0;
                if create && !open_filesystem.is_writable_preopen(dir_fd) {
                    return Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]);
                }

                let requested_base = *rights_base as u64;
                let requested_inheriting = *rights_inheriting as u64;
                let allowed_base =
                    RIGHTS_FD_READ | RIGHTS_FD_WRITE | RIGHTS_FD_SEEK | RIGHTS_FD_TELL;
                if requested_base & !allowed_base != 0 || requested_inheriting != 0 {
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

                let opened = match open_filesystem.open(dir_fd, &path, requested_base, create) {
                    Ok(opened) => opened,
                    Err(OpenError::NotFound) => return Ok(vec![Value::I32(ERRNO_NOENT)]),
                    Err(OpenError::NotCapable) => return Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]),
                    Err(OpenError::TooManyOpenFiles) => return Ok(vec![Value::I32(ERRNO_MFILE)]),
                    Err(OpenError::TooManyFiles) => return Ok(vec![Value::I32(ERRNO_NOSPC)]),
                };

                if context
                    .write_memory(*opened_fd_ptr as u32, &opened.fd.to_le_bytes())
                    .is_err()
                {
                    open_filesystem.rollback_open(dir_fd, &path, opened);
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }

                Ok(vec![Value::I32(ERRNO_SUCCESS)])
            },
        )?;

        let pwrite_filesystem = self.clone();
        registry.register_values(
            WASI_MODULE,
            FD_PWRITE_NAME,
            vec![
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
                ValueType::I64,
                ValueType::I32,
            ],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ_WRITE,
            move |context, args| {
                let [
                    Value::I32(fd),
                    Value::I32(iovs),
                    Value::I32(iovs_len),
                    Value::I64(offset),
                    Value::I32(nwritten),
                ] = args
                else {
                    return Err(HostError::message(
                        "validated wasi fd_pwrite signature received invalid arguments",
                    ));
                };

                let fd = *fd as u32;
                let offset = *offset as u64;
                let iovs_len = *iovs_len as u32;
                if iovs_len > MAX_PWRITE_IOVECS {
                    return Ok(vec![Value::I32(ERRNO_INVAL)]);
                }

                let mut payload = Vec::new();
                for index in 0..iovs_len {
                    let Some(entry_offset) = index.checked_mul(8) else {
                        return Ok(vec![Value::I32(ERRNO_FAULT)]);
                    };
                    let Some(entry_address) = (*iovs as u32).checked_add(entry_offset) else {
                        return Ok(vec![Value::I32(ERRNO_FAULT)]);
                    };
                    let header = match context.read_memory(entry_address, 8) {
                        Ok(header) => header,
                        Err(_) => return Ok(vec![Value::I32(ERRNO_FAULT)]),
                    };
                    let pointer =
                        u32::from_le_bytes(header[0..4].try_into().expect("fixed ciovec header"));
                    let length =
                        u32::from_le_bytes(header[4..8].try_into().expect("fixed ciovec header"))
                            as usize;
                    let Some(next_len) = payload.len().checked_add(length) else {
                        return Ok(vec![Value::I32(ERRNO_INVAL)]);
                    };
                    if next_len > MAX_FILE_BYTES || next_len > u32::MAX as usize {
                        return Ok(vec![Value::I32(ERRNO_FBIG)]);
                    }
                    let bytes = match context.read_memory(pointer, length) {
                        Ok(bytes) => bytes,
                        Err(_) => return Ok(vec![Value::I32(ERRNO_FAULT)]),
                    };
                    payload.extend_from_slice(&bytes);
                }

                match pwrite_filesystem.prepare_pwrite(fd, offset, payload.len()) {
                    Ok(()) => {}
                    Err(error) => return Ok(vec![Value::I32(write_errno(error))]),
                }

                if context.read_memory(*nwritten as u32, 4).is_err() {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }
                let written = payload.len() as u32;
                if context
                    .write_memory(*nwritten as u32, &written.to_le_bytes())
                    .is_err()
                {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }

                if let Err(error) = pwrite_filesystem.pwrite(fd, offset, &payload) {
                    return Err(HostError::message(format!(
                        "WASI writable descriptor changed during fd_pwrite: {error:?}"
                    )));
                }

                Ok(vec![Value::I32(ERRNO_SUCCESS)])
            },
        )?;

        let seek_filesystem = self.clone();
        registry.register_values(
            WASI_MODULE,
            FD_SEEK_NAME,
            vec![ValueType::I32, ValueType::I64, ValueType::I32, ValueType::I32],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ_WRITE,
            move |context, args| {
                let [
                    Value::I32(fd),
                    Value::I64(offset),
                    Value::I32(whence),
                    Value::I32(newoffset_ptr),
                ] = args
                else {
                    return Err(HostError::message(
                        "validated wasi fd_seek signature received invalid arguments",
                    ));
                };

                let fd = *fd as u32;
                if seek_filesystem.is_known_non_file(fd) {
                    return Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]);
                }
                let newoffset = match seek_filesystem.prepare_seek(fd, *offset, *whence as u32) {
                    Ok(newoffset) => newoffset,
                    Err(error) => return Ok(vec![Value::I32(position_errno(error))]),
                };

                if context.read_memory(*newoffset_ptr as u32, 8).is_err() {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }
                if context
                    .write_memory(*newoffset_ptr as u32, &newoffset.to_le_bytes())
                    .is_err()
                {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }
                if !seek_filesystem.commit_seek(fd, newoffset) {
                    return Err(HostError::message("WASI descriptor changed during fd_seek"));
                }

                Ok(vec![Value::I32(ERRNO_SUCCESS)])
            },
        )?;

        let tell_filesystem = self.clone();
        registry.register_values(
            WASI_MODULE,
            FD_TELL_NAME,
            vec![ValueType::I32, ValueType::I32],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ_WRITE,
            move |context, args| {
                let [Value::I32(fd), Value::I32(offset_ptr)] = args else {
                    return Err(HostError::message(
                        "validated wasi fd_tell signature received invalid arguments",
                    ));
                };

                let fd = *fd as u32;
                if tell_filesystem.is_known_non_file(fd) {
                    return Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]);
                }
                let offset = match tell_filesystem.tell(fd) {
                    Ok(offset) => offset,
                    Err(error) => return Ok(vec![Value::I32(position_errno(error))]),
                };

                if context.read_memory(*offset_ptr as u32, 8).is_err() {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }
                if context
                    .write_memory(*offset_ptr as u32, &offset.to_le_bytes())
                    .is_err()
                {
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

    pub(crate) fn is_writable_preopen(&self, fd: u32) -> bool {
        self.state.borrow().writable_preopens.contains(&fd)
    }

    fn is_known_non_file(&self, fd: u32) -> bool {
        fd <= 2 || self.has_preopen(fd)
    }

    fn open(
        &self,
        preopen_fd: u32,
        path: &[u8],
        rights_base: u64,
        create: bool,
    ) -> Result<OpenedFile, OpenError> {
        let mut state = self.state.borrow_mut();
        if state.open_files.len() >= MAX_OPEN_FILES {
            return Err(OpenError::TooManyOpenFiles);
        }

        let mut candidate = FIRST_DYNAMIC_FD;
        loop {
            if !state.reserved_preopens.contains(&candidate)
                && !state.open_files.contains_key(&candidate)
            {
                break;
            }
            candidate = candidate
                .checked_add(1)
                .ok_or(OpenError::TooManyOpenFiles)?;
        }

        let existing = state
            .mounted_files
            .iter()
            .find(|file| file.preopen_fd == preopen_fd && file.relative_path.as_slice() == path)
            .map(|file| (file.bytes.clone(), file.writable));

        let (bytes, writable, created) = if let Some((bytes, writable)) = existing {
            (bytes, writable, false)
        } else {
            if !create {
                return Err(OpenError::NotFound);
            }
            if !state.writable_preopens.contains(&preopen_fd) {
                return Err(OpenError::NotCapable);
            }
            if state.mounted_files.len() >= MAX_MOUNTED_FILES {
                return Err(OpenError::TooManyFiles);
            }
            let bytes = Rc::new(RefCell::new(Vec::new()));
            state.mounted_files.push(MountedFile {
                preopen_fd,
                relative_path: path.to_vec(),
                bytes: bytes.clone(),
                writable: true,
            });
            (bytes, true, true)
        };

        if rights_base & RIGHTS_FD_WRITE != 0
            && (!writable || !state.writable_preopens.contains(&preopen_fd))
        {
            if created {
                state.mounted_files.pop();
            }
            return Err(OpenError::NotCapable);
        }

        state.open_files.insert(
            candidate,
            OpenFile {
                bytes,
                offset: 0,
                rights_base,
            },
        );
        Ok(OpenedFile {
            fd: candidate,
            created,
        })
    }

    fn rollback_open(&self, preopen_fd: u32, path: &[u8], opened: OpenedFile) {
        let mut state = self.state.borrow_mut();
        state.open_files.remove(&opened.fd);
        if opened.created {
            if let Some(index) = state.mounted_files.iter().position(|file| {
                file.preopen_fd == preopen_fd && file.relative_path.as_slice() == path
            }) {
                state.mounted_files.remove(index);
            }
        }
    }

    fn close(&self, fd: u32) -> bool {
        self.state.borrow_mut().open_files.remove(&fd).is_some()
    }

    fn prepare_pwrite(
        &self,
        fd: u32,
        offset: u64,
        len: usize,
    ) -> Result<(), DescriptorWriteError> {
        let state = self.state.borrow();
        let Some(file) = state.open_files.get(&fd) else {
            return Err(DescriptorWriteError::BadFd);
        };
        if file.rights_base & RIGHTS_FD_WRITE == 0 || file.rights_base & RIGHTS_FD_SEEK == 0 {
            return Err(DescriptorWriteError::NotCapable);
        }
        let len = u64::try_from(len).map_err(|_| DescriptorWriteError::FileTooLarge)?;
        let end = offset
            .checked_add(len)
            .ok_or(DescriptorWriteError::FileTooLarge)?;
        if end > MAX_FILE_BYTES as u64 {
            return Err(DescriptorWriteError::FileTooLarge);
        }
        Ok(())
    }

    fn pwrite(
        &self,
        fd: u32,
        offset: u64,
        bytes: &[u8],
    ) -> Result<(), DescriptorWriteError> {
        self.prepare_pwrite(fd, offset, bytes.len())?;
        let state = self.state.borrow();
        let file = state
            .open_files
            .get(&fd)
            .ok_or(DescriptorWriteError::BadFd)?;
        let start = usize::try_from(offset).map_err(|_| DescriptorWriteError::FileTooLarge)?;
        let end = start
            .checked_add(bytes.len())
            .ok_or(DescriptorWriteError::FileTooLarge)?;
        let mut file_bytes = file.bytes.borrow_mut();
        if file_bytes.len() < start {
            file_bytes.resize(start, 0);
        }
        if file_bytes.len() < end {
            file_bytes.resize(end, 0);
        }
        file_bytes[start..end].copy_from_slice(bytes);
        Ok(())
    }

    fn prepare_seek(
        &self,
        fd: u32,
        delta: i64,
        whence: u32,
    ) -> Result<u64, DescriptorPositionError> {
        let state = self.state.borrow();
        let Some(file) = state.open_files.get(&fd) else {
            return Err(DescriptorPositionError::BadFd);
        };

        let base = match whence {
            WHENCE_SET => 0u64,
            WHENCE_CUR => file.offset,
            WHENCE_END => file.bytes.borrow().len() as u64,
            _ => return Err(DescriptorPositionError::InvalidWhence),
        };
        let tell_only_operation = whence == WHENCE_CUR && delta == 0;
        if tell_only_operation {
            if file.rights_base & (RIGHTS_FD_SEEK | RIGHTS_FD_TELL) == 0 {
                return Err(DescriptorPositionError::NotCapable);
            }
        } else if file.rights_base & RIGHTS_FD_SEEK == 0 {
            return Err(DescriptorPositionError::NotCapable);
        }

        let target = i128::from(base) + i128::from(delta);
        if target < 0 {
            return Err(DescriptorPositionError::InvalidOffset);
        }
        if target > i128::from(u64::MAX) {
            return Err(DescriptorPositionError::Overflow);
        }
        Ok(target as u64)
    }

    fn commit_seek(&self, fd: u32, offset: u64) -> bool {
        let mut state = self.state.borrow_mut();
        let Some(file) = state.open_files.get_mut(&fd) else {
            return false;
        };
        file.offset = offset;
        true
    }

    fn tell(&self, fd: u32) -> Result<u64, DescriptorPositionError> {
        let state = self.state.borrow();
        let Some(file) = state.open_files.get(&fd) else {
            return Err(DescriptorPositionError::BadFd);
        };
        if file.rights_base & (RIGHTS_FD_SEEK | RIGHTS_FD_TELL) == 0 {
            return Err(DescriptorPositionError::NotCapable);
        }
        Ok(file.offset)
    }
}

fn position_errno(error: DescriptorPositionError) -> i32 {
    match error {
        DescriptorPositionError::BadFd => ERRNO_BADF,
        DescriptorPositionError::NotCapable => ERRNO_NOTCAPABLE,
        DescriptorPositionError::InvalidWhence | DescriptorPositionError::InvalidOffset => {
            ERRNO_INVAL
        }
        DescriptorPositionError::Overflow => ERRNO_OVERFLOW,
    }
}

fn write_errno(error: DescriptorWriteError) -> i32 {
    match error {
        DescriptorWriteError::BadFd => ERRNO_BADF,
        DescriptorWriteError::NotCapable => ERRNO_NOTCAPABLE,
        DescriptorWriteError::FileTooLarge => ERRNO_FBIG,
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
