use std::{cell::RefCell, collections::BTreeMap, fmt, rc::Rc};

use wasm_parser::ValueType;
use wasm_runtime::{HostCapabilities, HostError, HostRegistry, HostRegistryError, Value};

use crate::{
    ERRNO_BADF, ERRNO_EXIST, ERRNO_FAULT, ERRNO_FBIG, ERRNO_INVAL, ERRNO_IO, ERRNO_MFILE,
    ERRNO_NAMETOOLONG, ERRNO_NOENT, ERRNO_NOSPC, ERRNO_NOTCAPABLE, ERRNO_NOTDIR, ERRNO_NOTEMPTY,
    ERRNO_NOTSUP, ERRNO_OVERFLOW, ERRNO_SUCCESS, FILETYPE_DIRECTORY, FILETYPE_REGULAR_FILE,
    FILETYPE_SYMBOLIC_LINK, LOOKUPFLAGS_SYMLINK_FOLLOW, OFLAGS_CREAT, OFLAGS_DIRECTORY,
    RIGHTS_FD_FILESTAT_GET, RIGHTS_FD_FILESTAT_SET_SIZE, RIGHTS_FD_FILESTAT_SET_TIMES,
    RIGHTS_FD_READ, RIGHTS_FD_READDIR, RIGHTS_FD_SEEK, RIGHTS_FD_TELL, RIGHTS_FD_WRITE,
    RIGHTS_PATH_CREATE_DIRECTORY, RIGHTS_PATH_CREATE_FILE, RIGHTS_PATH_FILESTAT_GET,
    RIGHTS_PATH_FILESTAT_SET_TIMES, RIGHTS_PATH_LINK_SOURCE, RIGHTS_PATH_LINK_TARGET,
    RIGHTS_PATH_OPEN, RIGHTS_PATH_READLINK, RIGHTS_PATH_REMOVE_DIRECTORY,
    RIGHTS_PATH_RENAME_SOURCE, RIGHTS_PATH_RENAME_TARGET, RIGHTS_PATH_SYMLINK,
    RIGHTS_PATH_UNLINK_FILE,
};

const WASI_MODULE: &str = "wasi_snapshot_preview1";
const PATH_CREATE_DIRECTORY_NAME: &str = "path_create_directory";
const PATH_FILESTAT_GET_NAME: &str = "path_filestat_get";
const PATH_FILESTAT_SET_TIMES_NAME: &str = "path_filestat_set_times";
const PATH_REMOVE_DIRECTORY_NAME: &str = "path_remove_directory";
const PATH_OPEN_NAME: &str = "path_open";
const PATH_LINK_NAME: &str = "path_link";
const PATH_RENAME_NAME: &str = "path_rename";
const PATH_READLINK_NAME: &str = "path_readlink";
const PATH_SYMLINK_NAME: &str = "path_symlink";
const PATH_UNLINK_FILE_NAME: &str = "path_unlink_file";
const FD_READDIR_NAME: &str = "fd_readdir";
const FD_CLOSE_NAME: &str = "fd_close";
const FD_SEEK_NAME: &str = "fd_seek";
const FD_TELL_NAME: &str = "fd_tell";
const FD_PWRITE_NAME: &str = "fd_pwrite";
const FD_FILESTAT_SET_SIZE_NAME: &str = "fd_filestat_set_size";
const FD_FILESTAT_SET_TIMES_NAME: &str = "fd_filestat_set_times";
const FSTFLAGS_ATIM: u32 = 1 << 0;
const FSTFLAGS_ATIM_NOW: u32 = 1 << 1;
const FSTFLAGS_MTIM: u32 = 1 << 2;
const FSTFLAGS_MTIM_NOW: u32 = 1 << 3;
const FSTFLAGS_ALL: u32 = FSTFLAGS_ATIM | FSTFLAGS_ATIM_NOW | FSTFLAGS_MTIM | FSTFLAGS_MTIM_NOW;
const WHENCE_SET: u32 = 0;
const WHENCE_CUR: u32 = 1;
const WHENCE_END: u32 = 2;
const FIRST_DYNAMIC_FD: u32 = 3;
const MAX_MOUNTED_FILES: usize = 4_096;
const MAX_RELATIVE_PATH_BYTES: usize = 4 * 1024;
const MAX_FILE_BYTES: usize = 16 * 1024 * 1024;
const MAX_OPEN_FILES: usize = 256;
const MAX_PWRITE_IOVECS: u32 = 1_024;
const DIRENT_SIZE: usize = 24;
const SYNTHETIC_DEVICE_ID: u64 = 1;

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct FileTimes {
    atim: u64,
    mtim: u64,
    ctim: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TimeUpdate {
    atim: Option<u64>,
    mtim: Option<u64>,
}

#[derive(Debug, Clone)]
struct MountedFile {
    preopen_fd: u32,
    relative_path: Vec<u8>,
    inode: u64,
    bytes: Rc<RefCell<Vec<u8>>>,
    link_count: Rc<RefCell<u64>>,
    times: Rc<RefCell<FileTimes>>,
    writable: bool,
}

#[derive(Debug, Clone)]
struct OpenFile {
    inode: u64,
    bytes: Rc<RefCell<Vec<u8>>>,
    link_count: Rc<RefCell<u64>>,
    times: Rc<RefCell<FileTimes>>,
    offset: u64,
    rights_base: u64,
}

#[derive(Debug, Clone)]
struct MountedDirectory {
    preopen_fd: u32,
    relative_path: Vec<u8>,
    inode: u64,
    times: FileTimes,
}

#[derive(Debug, Clone)]
struct MountedSymlink {
    preopen_fd: u32,
    relative_path: Vec<u8>,
    inode: u64,
    target: Vec<u8>,
    times: FileTimes,
}

#[derive(Debug, Clone)]
struct OpenDirectory {
    preopen_fd: u32,
    relative_path: Vec<u8>,
    inode: u64,
    parent_inode: u64,
    rights_base: u64,
}

#[derive(Debug, Default)]
struct FilesystemState {
    next_inode: u64,
    reserved_preopens: Vec<u32>,
    writable_preopens: Vec<u32>,
    mounted_files: Vec<MountedFile>,
    mounted_directories: Vec<MountedDirectory>,
    mounted_symlinks: Vec<MountedSymlink>,
    open_files: BTreeMap<u32, OpenFile>,
    open_directories: BTreeMap<u32, OpenDirectory>,
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
pub(crate) enum DescriptorWriteError {
    BadFd,
    NotCapable,
    FileTooLarge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DescriptorFilestatError {
    BadFd,
    NotCapable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DescriptorFilestat {
    pub(crate) dev: u64,
    pub(crate) ino: u64,
    pub(crate) filetype: u8,
    pub(crate) nlink: u64,
    pub(crate) size: u64,
    pub(crate) atim: u64,
    pub(crate) mtim: u64,
    pub(crate) ctim: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenError {
    NotFound,
    NotCapable,
    NotDirectory,
    NameTooLong,
    TooManyOpenFiles,
    TooManyFiles,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinkError {
    BadFd,
    NotFound,
    NotCapable,
    NameTooLong,
    TargetExists,
    TooManyFiles,
    LinkCountOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnlinkError {
    BadFd,
    NotFound,
    NotCapable,
    NameTooLong,
    InvalidLinkCount,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenameError {
    NotFound,
    NotCapable,
    InvalidLinkCount,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReaddirError {
    BadFd,
    NotCapable,
    NotSupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathFilestatError {
    BadFd,
    NotFound,
    NotCapable,
    NameTooLong,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolveDirectoryError {
    BadFd,
    NotCapable,
    NameTooLong,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DirectoryMutationError {
    BadFd,
    NotFound,
    NotCapable,
    NameTooLong,
    Exists,
    NotDirectory,
    NotEmpty,
    TooManyFiles,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SymlinkError {
    BadFd,
    NotFound,
    NotCapable,
    NameTooLong,
    Exists,
    TooManyFiles,
    NotLink,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReaddirEntry {
    next: u64,
    inode: u64,
    filetype: u8,
    name: Vec<u8>,
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

        let inode = state
            .next_inode
            .checked_add(1)
            .expect("bounded mounted-file count prevents inode exhaustion");
        state.next_inode = inode;
        let link_count = Rc::new(RefCell::new(1));
        let times = Rc::new(RefCell::new(FileTimes::default()));
        state.mounted_files.push(MountedFile {
            preopen_fd,
            relative_path: relative_path.to_vec(),
            inode,
            bytes: Rc::new(RefCell::new(bytes.to_vec())),
            link_count,
            times,
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

    pub(crate) fn ensure_preadable(&self, fd: u32) -> Result<(), DescriptorReadError> {
        let state = self.state.borrow();
        let Some(file) = state.open_files.get(&fd) else {
            return Err(DescriptorReadError::BadFd);
        };
        let required = RIGHTS_FD_READ | RIGHTS_FD_SEEK;
        if file.rights_base & required != required {
            return Err(DescriptorReadError::NotCapable);
        }
        Ok(())
    }

    pub(crate) fn pread(
        &self,
        fd: u32,
        offset: u64,
        max_len: usize,
    ) -> Result<Vec<u8>, DescriptorReadError> {
        let state = self.state.borrow();
        let Some(file) = state.open_files.get(&fd) else {
            return Err(DescriptorReadError::BadFd);
        };
        let required = RIGHTS_FD_READ | RIGHTS_FD_SEEK;
        if file.rights_base & required != required {
            return Err(DescriptorReadError::NotCapable);
        }
        let Ok(start) = usize::try_from(offset) else {
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

    pub(crate) fn ensure_writable(&self, fd: u32) -> Result<(), DescriptorWriteError> {
        let state = self.state.borrow();
        let Some(file) = state.open_files.get(&fd) else {
            return Err(DescriptorWriteError::BadFd);
        };
        if file.rights_base & RIGHTS_FD_WRITE == 0 {
            return Err(DescriptorWriteError::NotCapable);
        }
        Ok(())
    }

    pub(crate) fn prepare_write(&self, fd: u32, len: usize) -> Result<(), DescriptorWriteError> {
        let state = self.state.borrow();
        let Some(file) = state.open_files.get(&fd) else {
            return Err(DescriptorWriteError::BadFd);
        };
        if file.rights_base & RIGHTS_FD_WRITE == 0 {
            return Err(DescriptorWriteError::NotCapable);
        }
        let len = u64::try_from(len).map_err(|_| DescriptorWriteError::FileTooLarge)?;
        let end = file
            .offset
            .checked_add(len)
            .ok_or(DescriptorWriteError::FileTooLarge)?;
        if end > MAX_FILE_BYTES as u64 {
            return Err(DescriptorWriteError::FileTooLarge);
        }
        Ok(())
    }

    pub(crate) fn write(&self, fd: u32, bytes: &[u8]) -> Result<(), DescriptorWriteError> {
        self.prepare_write(fd, bytes.len())?;
        if bytes.is_empty() {
            return Ok(());
        }

        let mut state = self.state.borrow_mut();
        let Some(file) = state.open_files.get_mut(&fd) else {
            return Err(DescriptorWriteError::BadFd);
        };
        if file.rights_base & RIGHTS_FD_WRITE == 0 {
            return Err(DescriptorWriteError::NotCapable);
        }
        let start = usize::try_from(file.offset).map_err(|_| DescriptorWriteError::FileTooLarge)?;
        let end = start
            .checked_add(bytes.len())
            .ok_or(DescriptorWriteError::FileTooLarge)?;
        if end > MAX_FILE_BYTES {
            return Err(DescriptorWriteError::FileTooLarge);
        }
        {
            let mut file_bytes = file.bytes.borrow_mut();
            if file_bytes.len() < start {
                file_bytes.resize(start, 0);
            }
            if file_bytes.len() < end {
                file_bytes.resize(end, 0);
            }
            file_bytes[start..end].copy_from_slice(bytes);
        }
        file.offset = end as u64;
        Ok(())
    }

    pub(crate) fn fdstat(&self, fd: u32) -> Option<(u8, u64, u64)> {
        let state = self.state.borrow();
        if let Some(file) = state.open_files.get(&fd) {
            return Some((FILETYPE_REGULAR_FILE, file.rights_base, 0));
        }
        state
            .open_directories
            .get(&fd)
            .map(|directory| (FILETYPE_DIRECTORY, directory.rights_base, 0))
    }

    pub(crate) fn filestat(&self, fd: u32) -> Result<DescriptorFilestat, DescriptorFilestatError> {
        let state = self.state.borrow();
        let Some(file) = state.open_files.get(&fd) else {
            return Err(DescriptorFilestatError::BadFd);
        };
        if file.rights_base & RIGHTS_FD_FILESTAT_GET == 0 {
            return Err(DescriptorFilestatError::NotCapable);
        }
        let size = file.bytes.borrow().len() as u64;
        let nlink = *file.link_count.borrow();
        let times = *file.times.borrow();
        Ok(DescriptorFilestat {
            dev: SYNTHETIC_DEVICE_ID,
            ino: file.inode,
            filetype: FILETYPE_REGULAR_FILE,
            nlink,
            size,
            atim: times.atim,
            mtim: times.mtim,
            ctim: times.ctim,
        })
    }

    pub(crate) fn set_size(&self, fd: u32, size: u64) -> Result<(), DescriptorWriteError> {
        let state = self.state.borrow();
        let Some(file) = state.open_files.get(&fd) else {
            return Err(DescriptorWriteError::BadFd);
        };
        if file.rights_base & RIGHTS_FD_FILESTAT_SET_SIZE == 0 {
            return Err(DescriptorWriteError::NotCapable);
        }
        let size = usize::try_from(size).map_err(|_| DescriptorWriteError::FileTooLarge)?;
        if size > MAX_FILE_BYTES {
            return Err(DescriptorWriteError::FileTooLarge);
        }
        file.bytes.borrow_mut().resize(size, 0);
        Ok(())
    }

    fn set_times(&self, fd: u32, update: TimeUpdate) -> Result<(), DescriptorFilestatError> {
        let state = self.state.borrow();
        let Some(file) = state.open_files.get(&fd) else {
            return Err(DescriptorFilestatError::BadFd);
        };
        if file.rights_base & RIGHTS_FD_FILESTAT_SET_TIMES == 0 {
            return Err(DescriptorFilestatError::NotCapable);
        }
        let mut times = file.times.borrow_mut();
        apply_time_update(&mut times, update);
        Ok(())
    }

    fn path_set_times(
        &self,
        dir_fd: u32,
        path: &[u8],
        update: TimeUpdate,
    ) -> Result<(), PathFilestatError> {
        let (preopen_fd, full_path, _) = self
            .resolve_directory_path(dir_fd, path, RIGHTS_PATH_FILESTAT_SET_TIMES)
            .map_err(|error| match error {
                ResolveDirectoryError::BadFd => PathFilestatError::BadFd,
                ResolveDirectoryError::NotCapable => PathFilestatError::NotCapable,
                ResolveDirectoryError::NameTooLong => PathFilestatError::NameTooLong,
            })?;
        let mut state = self.state.borrow_mut();
        if let Some(file) = state.mounted_files.iter_mut().find(|file| {
            file.preopen_fd == preopen_fd && file.relative_path.as_slice() == full_path.as_slice()
        }) {
            if !file.writable {
                return Err(PathFilestatError::NotCapable);
            }
            let mut times = file.times.borrow_mut();
            apply_time_update(&mut times, update);
            return Ok(());
        }
        if let Some(directory) = state.mounted_directories.iter_mut().find(|directory| {
            directory.preopen_fd == preopen_fd
                && directory.relative_path.as_slice() == full_path.as_slice()
        }) {
            apply_time_update(&mut directory.times, update);
            return Ok(());
        }
        if let Some(symlink) = state.mounted_symlinks.iter_mut().find(|symlink| {
            symlink.preopen_fd == preopen_fd
                && symlink.relative_path.as_slice() == full_path.as_slice()
        }) {
            apply_time_update(&mut symlink.times, update);
            return Ok(());
        }
        Err(PathFilestatError::NotFound)
    }

    fn path_filestat(
        &self,
        dir_fd: u32,
        path: &[u8],
    ) -> Result<DescriptorFilestat, PathFilestatError> {
        let (preopen_fd, full_path, _) = self
            .resolve_directory_path(dir_fd, path, RIGHTS_PATH_FILESTAT_GET)
            .map_err(|error| match error {
                ResolveDirectoryError::BadFd => PathFilestatError::BadFd,
                ResolveDirectoryError::NotCapable => PathFilestatError::NotCapable,
                ResolveDirectoryError::NameTooLong => PathFilestatError::NameTooLong,
            })?;
        let state = self.state.borrow();
        if let Some(file) = state.mounted_files.iter().find(|file| {
            file.preopen_fd == preopen_fd && file.relative_path.as_slice() == full_path.as_slice()
        }) {
            let size = file.bytes.borrow().len() as u64;
            let nlink = *file.link_count.borrow();
            let times = *file.times.borrow();
            return Ok(DescriptorFilestat {
                dev: SYNTHETIC_DEVICE_ID,
                ino: file.inode,
                filetype: FILETYPE_REGULAR_FILE,
                nlink,
                size,
                atim: times.atim,
                mtim: times.mtim,
                ctim: times.ctim,
            });
        }
        if let Some(directory) = state.mounted_directories.iter().find(|directory| {
            directory.preopen_fd == preopen_fd
                && directory.relative_path.as_slice() == full_path.as_slice()
        }) {
            return Ok(DescriptorFilestat {
                dev: SYNTHETIC_DEVICE_ID,
                ino: directory.inode,
                filetype: FILETYPE_DIRECTORY,
                nlink: 1,
                size: 0,
                atim: directory.times.atim,
                mtim: directory.times.mtim,
                ctim: directory.times.ctim,
            });
        }
        if let Some(symlink) = state.mounted_symlinks.iter().find(|symlink| {
            symlink.preopen_fd == preopen_fd
                && symlink.relative_path.as_slice() == full_path.as_slice()
        }) {
            return Ok(DescriptorFilestat {
                dev: SYNTHETIC_DEVICE_ID,
                ino: symlink.inode,
                filetype: FILETYPE_SYMBOLIC_LINK,
                nlink: 1,
                size: symlink.target.len() as u64,
                atim: symlink.times.atim,
                mtim: symlink.times.mtim,
                ctim: symlink.times.ctim,
            });
        }
        Err(PathFilestatError::NotFound)
    }

    fn readdir_snapshot(&self, fd: u32, cookie: u64) -> Result<Vec<ReaddirEntry>, ReaddirError> {
        let state = self.state.borrow();
        let (preopen_fd, base, directory_inode, parent_inode, rights) =
            if state.reserved_preopens.contains(&fd) {
                let inode = (1u64 << 63) | u64::from(fd);
                (
                    fd,
                    Vec::new(),
                    inode,
                    inode,
                    directory_rights(&state, fd).expect("reserved preopen has rights"),
                )
            } else if let Some(directory) = state.open_directories.get(&fd) {
                (
                    directory.preopen_fd,
                    directory.relative_path.clone(),
                    directory.inode,
                    directory.parent_inode,
                    directory.rights_base,
                )
            } else {
                if fd <= 2 || state.open_files.contains_key(&fd) {
                    return Err(ReaddirError::NotCapable);
                }
                return Err(ReaddirError::BadFd);
            };
        if rights & RIGHTS_FD_READDIR == 0 {
            return Err(ReaddirError::NotCapable);
        }

        for file in state
            .mounted_files
            .iter()
            .filter(|file| file.preopen_fd == preopen_fd)
        {
            let Some(remainder) = relative_to_directory(&base, &file.relative_path) else {
                continue;
            };
            let Some(separator) = remainder.iter().position(|byte| *byte == b'/') else {
                continue;
            };
            let first = &remainder[..separator];
            let expected_directory = join_relative(&base, first);
            if !state.mounted_directories.iter().any(|directory| {
                directory.preopen_fd == preopen_fd
                    && directory.relative_path.as_slice() == expected_directory.as_slice()
            }) {
                return Err(ReaddirError::NotSupported);
            }
        }

        let mut children = Vec::new();
        for directory in state
            .mounted_directories
            .iter()
            .filter(|directory| directory.preopen_fd == preopen_fd)
        {
            if let Some(name) = immediate_child_name(&base, &directory.relative_path) {
                children.push((name.to_vec(), directory.inode, FILETYPE_DIRECTORY));
            }
        }
        for file in state
            .mounted_files
            .iter()
            .filter(|file| file.preopen_fd == preopen_fd)
        {
            if let Some(name) = immediate_child_name(&base, &file.relative_path) {
                children.push((name.to_vec(), file.inode, FILETYPE_REGULAR_FILE));
            }
        }
        for symlink in state
            .mounted_symlinks
            .iter()
            .filter(|symlink| symlink.preopen_fd == preopen_fd)
        {
            if let Some(name) = immediate_child_name(&base, &symlink.relative_path) {
                children.push((name.to_vec(), symlink.inode, FILETYPE_SYMBOLIC_LINK));
            }
        }
        children.sort_by(|left, right| left.0.cmp(&right.0));

        let mut entries = Vec::with_capacity(children.len() + 2);
        entries.push(ReaddirEntry {
            next: 1,
            inode: directory_inode,
            filetype: FILETYPE_DIRECTORY,
            name: b".".to_vec(),
        });
        entries.push(ReaddirEntry {
            next: 2,
            inode: parent_inode,
            filetype: FILETYPE_DIRECTORY,
            name: b"..".to_vec(),
        });
        entries.extend(
            children
                .into_iter()
                .enumerate()
                .map(|(index, (name, inode, filetype))| ReaddirEntry {
                    next: (index + 3) as u64,
                    inode,
                    filetype,
                    name,
                }),
        );

        let Ok(start) = usize::try_from(cookie) else {
            return Ok(Vec::new());
        };
        if start >= entries.len() {
            return Ok(Vec::new());
        }
        Ok(entries.into_iter().skip(start).collect())
    }

    pub(crate) fn register(
        &self,
        registry: &mut HostRegistry,
        realtime_now: Option<u64>,
    ) -> Result<(), HostRegistryError> {
        let create_directory_filesystem = self.clone();
        registry.register_values(
            WASI_MODULE,
            PATH_CREATE_DIRECTORY_NAME,
            vec![ValueType::I32, ValueType::I32, ValueType::I32],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ,
            move |context, args| {
                let [Value::I32(dir_fd), Value::I32(path_ptr), Value::I32(path_len)] = args else {
                    return Err(HostError::message(
                        "validated wasi path_create_directory signature received invalid arguments",
                    ));
                };
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
                    Err(GuestPathError::TooLong) => return Ok(vec![Value::I32(ERRNO_NAMETOOLONG)]),
                    Err(GuestPathError::Unsafe) => return Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]),
                }
                match create_directory_filesystem.create_directory(*dir_fd as u32, &path) {
                    Ok(()) => Ok(vec![Value::I32(ERRNO_SUCCESS)]),
                    Err(error) => Ok(vec![Value::I32(directory_errno(error))]),
                }
            },
        )?;

        let remove_directory_filesystem = self.clone();
        registry.register_values(
            WASI_MODULE,
            PATH_REMOVE_DIRECTORY_NAME,
            vec![ValueType::I32, ValueType::I32, ValueType::I32],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ,
            move |context, args| {
                let [Value::I32(dir_fd), Value::I32(path_ptr), Value::I32(path_len)] = args else {
                    return Err(HostError::message(
                        "validated wasi path_remove_directory signature received invalid arguments",
                    ));
                };
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
                    Err(GuestPathError::TooLong) => return Ok(vec![Value::I32(ERRNO_NAMETOOLONG)]),
                    Err(GuestPathError::Unsafe) => return Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]),
                }
                match remove_directory_filesystem.remove_directory(*dir_fd as u32, &path) {
                    Ok(()) => Ok(vec![Value::I32(ERRNO_SUCCESS)]),
                    Err(error) => Ok(vec![Value::I32(directory_errno(error))]),
                }
            },
        )?;

        let symlink_filesystem = self.clone();
        registry.register_values(
            WASI_MODULE,
            PATH_SYMLINK_NAME,
            vec![
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
            ],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ,
            move |context, args| {
                let [
                    Value::I32(target_ptr),
                    Value::I32(target_len),
                    Value::I32(dir_fd),
                    Value::I32(path_ptr),
                    Value::I32(path_len),
                ] = args
                else {
                    return Err(HostError::message(
                        "validated wasi path_symlink signature received invalid arguments",
                    ));
                };
                let target_len = *target_len as u32 as usize;
                let path_len = *path_len as u32 as usize;
                if target_len > MAX_RELATIVE_PATH_BYTES || path_len > MAX_RELATIVE_PATH_BYTES {
                    return Ok(vec![Value::I32(ERRNO_NAMETOOLONG)]);
                }
                let target = match context.read_memory(*target_ptr as u32, target_len) {
                    Ok(target) => target,
                    Err(_) => return Ok(vec![Value::I32(ERRNO_FAULT)]),
                };
                let path = match context.read_memory(*path_ptr as u32, path_len) {
                    Ok(path) => path,
                    Err(_) => return Ok(vec![Value::I32(ERRNO_FAULT)]),
                };
                match validate_guest_path(&path) {
                    Ok(()) => {}
                    Err(GuestPathError::Empty) => return Ok(vec![Value::I32(ERRNO_INVAL)]),
                    Err(GuestPathError::TooLong) => return Ok(vec![Value::I32(ERRNO_NAMETOOLONG)]),
                    Err(GuestPathError::Unsafe) => return Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]),
                }
                match symlink_filesystem.symlink(*dir_fd as u32, &path, &target) {
                    Ok(()) => Ok(vec![Value::I32(ERRNO_SUCCESS)]),
                    Err(error) => Ok(vec![Value::I32(symlink_errno(error))]),
                }
            },
        )?;

        let readlink_filesystem = self.clone();
        registry.register_values(
            WASI_MODULE,
            PATH_READLINK_NAME,
            vec![
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
            ],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ_WRITE,
            move |context, args| {
                let [
                    Value::I32(dir_fd),
                    Value::I32(path_ptr),
                    Value::I32(path_len),
                    Value::I32(buf),
                    Value::I32(buf_len),
                    Value::I32(bufused),
                ] = args
                else {
                    return Err(HostError::message(
                        "validated wasi path_readlink signature received invalid arguments",
                    ));
                };
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
                    Err(GuestPathError::TooLong) => return Ok(vec![Value::I32(ERRNO_NAMETOOLONG)]),
                    Err(GuestPathError::Unsafe) => return Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]),
                }
                let target = match readlink_filesystem.readlink(*dir_fd as u32, &path) {
                    Ok(target) => target,
                    Err(error) => return Ok(vec![Value::I32(symlink_errno(error))]),
                };
                let buf_len = *buf_len as u32 as usize;
                if context.read_memory(*buf as u32, buf_len).is_err()
                    || context.read_memory(*bufused as u32, 4).is_err()
                {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }
                let copied = target.len().min(buf_len);
                if context.write_memory(*buf as u32, &target[..copied]).is_err()
                    || context
                        .write_memory(*bufused as u32, &(copied as u32).to_le_bytes())
                        .is_err()
                {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }
                Ok(vec![Value::I32(ERRNO_SUCCESS)])
            },
        )?;

        let path_filestat_filesystem = self.clone();
        registry.register_values(
            WASI_MODULE,
            PATH_FILESTAT_GET_NAME,
            vec![
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
            ],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ_WRITE,
            move |context, args| {
                let [
                    Value::I32(dir_fd),
                    Value::I32(flags),
                    Value::I32(path_ptr),
                    Value::I32(path_len),
                    Value::I32(filestat_ptr),
                ] = args
                else {
                    return Err(HostError::message(
                        "validated wasi path_filestat_get signature received invalid arguments",
                    ));
                };

                let flags = *flags as u32;
                if flags & !LOOKUPFLAGS_SYMLINK_FOLLOW != 0 {
                    return Ok(vec![Value::I32(ERRNO_INVAL)]);
                }
                if flags & LOOKUPFLAGS_SYMLINK_FOLLOW != 0 {
                    return Ok(vec![Value::I32(ERRNO_NOTSUP)]);
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

                let stat = match path_filestat_filesystem.path_filestat(*dir_fd as u32, &path) {
                    Ok(stat) => stat,
                    Err(PathFilestatError::BadFd) => return Ok(vec![Value::I32(ERRNO_BADF)]),
                    Err(PathFilestatError::NotFound) => return Ok(vec![Value::I32(ERRNO_NOENT)]),
                    Err(PathFilestatError::NotCapable) => {
                        return Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]);
                    }
                    Err(PathFilestatError::NameTooLong) => {
                        return Ok(vec![Value::I32(ERRNO_NAMETOOLONG)]);
                    }
                };
                if context.read_memory(*filestat_ptr as u32, 64).is_err() {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }
                let mut encoded = [0u8; 64];
                encoded[0..8].copy_from_slice(&stat.dev.to_le_bytes());
                encoded[8..16].copy_from_slice(&stat.ino.to_le_bytes());
                encoded[16] = stat.filetype;
                encoded[24..32].copy_from_slice(&stat.nlink.to_le_bytes());
                encoded[32..40].copy_from_slice(&stat.size.to_le_bytes());
                encoded[40..48].copy_from_slice(&stat.atim.to_le_bytes());
                encoded[48..56].copy_from_slice(&stat.mtim.to_le_bytes());
                encoded[56..64].copy_from_slice(&stat.ctim.to_le_bytes());
                if context.write_memory(*filestat_ptr as u32, &encoded).is_err() {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }
                Ok(vec![Value::I32(ERRNO_SUCCESS)])
            },
        )?;

        let path_set_times_filesystem = self.clone();
        registry.register_values(
            WASI_MODULE,
            PATH_FILESTAT_SET_TIMES_NAME,
            vec![
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
                ValueType::I64,
                ValueType::I64,
                ValueType::I32,
            ],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ,
            move |context, args| {
                let [
                    Value::I32(dir_fd),
                    Value::I32(flags),
                    Value::I32(path_ptr),
                    Value::I32(path_len),
                    Value::I64(atim),
                    Value::I64(mtim),
                    Value::I32(fst_flags),
                ] = args
                else {
                    return Err(HostError::message(
                        "validated wasi path_filestat_set_times signature received invalid arguments",
                    ));
                };
                let flags = *flags as u32;
                if flags & !LOOKUPFLAGS_SYMLINK_FOLLOW != 0 {
                    return Ok(vec![Value::I32(ERRNO_INVAL)]);
                }
                if flags & LOOKUPFLAGS_SYMLINK_FOLLOW != 0 {
                    return Ok(vec![Value::I32(ERRNO_NOTSUP)]);
                }
                let update = match resolve_time_update(*atim as u64, *mtim as u64, *fst_flags as u32, realtime_now) {
                    Ok(update) => update,
                    Err(()) => return Ok(vec![Value::I32(ERRNO_INVAL)]),
                };
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
                    Err(GuestPathError::TooLong) => return Ok(vec![Value::I32(ERRNO_NAMETOOLONG)]),
                    Err(GuestPathError::Unsafe) => return Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]),
                }
                match path_set_times_filesystem.path_set_times(*dir_fd as u32, &path, update) {
                    Ok(()) => Ok(vec![Value::I32(ERRNO_SUCCESS)]),
                    Err(PathFilestatError::BadFd) => Ok(vec![Value::I32(ERRNO_BADF)]),
                    Err(PathFilestatError::NotFound) => Ok(vec![Value::I32(ERRNO_NOENT)]),
                    Err(PathFilestatError::NotCapable) => Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]),
                    Err(PathFilestatError::NameTooLong) => Ok(vec![Value::I32(ERRNO_NAMETOOLONG)]),
                }
            },
        )?;

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
                    Value::I32(dir_fd), Value::I32(dir_flags), Value::I32(path_ptr),
                    Value::I32(path_len), Value::I32(open_flags), Value::I64(rights_base),
                    Value::I64(rights_inheriting), Value::I32(fd_flags), Value::I32(opened_fd_ptr),
                ] = args else {
                    return Err(HostError::message(
                        "validated wasi path_open signature received invalid arguments",
                    ));
                };
                let dir_fd = *dir_fd as u32;
                if !open_filesystem.has_directory_descriptor(dir_fd) {
                    return Ok(vec![Value::I32(ERRNO_BADF)]);
                }
                let open_flags = *open_flags as u32;
                let supported_flags = OFLAGS_CREAT | OFLAGS_DIRECTORY;
                if *dir_flags != 0 || *fd_flags != 0 || open_flags & !supported_flags != 0 {
                    return Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]);
                }
                let create = open_flags & OFLAGS_CREAT != 0;
                let directory = open_flags & OFLAGS_DIRECTORY != 0;
                if create && directory {
                    return Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]);
                }
                let requested_base = *rights_base as u64;
                let requested_inheriting = *rights_inheriting as u64;
                let allowed_file_base = RIGHTS_FD_READ | RIGHTS_FD_WRITE | RIGHTS_FD_SEEK
                    | RIGHTS_FD_TELL | RIGHTS_FD_FILESTAT_GET | RIGHTS_FD_FILESTAT_SET_SIZE
                    | RIGHTS_FD_FILESTAT_SET_TIMES;
                    let allowed_directory_base = RIGHTS_FD_READDIR
                        | RIGHTS_PATH_OPEN
                        | RIGHTS_PATH_FILESTAT_GET
                        | RIGHTS_PATH_FILESTAT_SET_TIMES
                        | RIGHTS_PATH_CREATE_DIRECTORY
                        | RIGHTS_PATH_CREATE_FILE
                        | RIGHTS_PATH_LINK_SOURCE
                        | RIGHTS_PATH_LINK_TARGET
                        | RIGHTS_PATH_READLINK
                        | RIGHTS_PATH_SYMLINK
                        | RIGHTS_PATH_REMOVE_DIRECTORY
                        | RIGHTS_PATH_UNLINK_FILE;
                let allowed_base = if directory { allowed_directory_base } else { allowed_file_base };
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
                    Err(GuestPathError::TooLong) => return Ok(vec![Value::I32(ERRNO_NAMETOOLONG)]),
                    Err(GuestPathError::Unsafe) => return Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]),
                }
                if context.read_memory(*opened_fd_ptr as u32, 4).is_err() {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }
                let opened = if directory {
                    open_filesystem.open_directory(dir_fd, &path, requested_base)
                } else {
                    open_filesystem.open(dir_fd, &path, requested_base, create)
                };
                let opened = match opened {
                    Ok(opened) => opened,
                    Err(OpenError::NotFound) => return Ok(vec![Value::I32(ERRNO_NOENT)]),
                    Err(OpenError::NotCapable) => return Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]),
                    Err(OpenError::NotDirectory) => return Ok(vec![Value::I32(ERRNO_NOTDIR)]),
                    Err(OpenError::NameTooLong) => return Ok(vec![Value::I32(ERRNO_NAMETOOLONG)]),
                    Err(OpenError::TooManyOpenFiles) => return Ok(vec![Value::I32(ERRNO_MFILE)]),
                    Err(OpenError::TooManyFiles) => return Ok(vec![Value::I32(ERRNO_NOSPC)]),
                };
                if context.write_memory(*opened_fd_ptr as u32, &opened.fd.to_le_bytes()).is_err() {
                    open_filesystem.rollback_open(dir_fd, &path, opened);
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }
                Ok(vec![Value::I32(ERRNO_SUCCESS)])
            },
        )?;

        let readdir_filesystem = self.clone();
        registry.register_values(
            WASI_MODULE,
            FD_READDIR_NAME,
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
                    Value::I32(buf),
                    Value::I32(buf_len),
                    Value::I64(cookie),
                    Value::I32(bufused),
                ] = args
                else {
                    return Err(HostError::message(
                        "validated wasi fd_readdir signature received invalid arguments",
                    ));
                };

                let fd = *fd as u32;
                let entries = match readdir_filesystem.readdir_snapshot(fd, *cookie as u64) {
                    Ok(entries) => entries,
                    Err(ReaddirError::BadFd) => return Ok(vec![Value::I32(ERRNO_BADF)]),
                    Err(ReaddirError::NotCapable) => {
                        return Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]);
                    }
                    Err(ReaddirError::NotSupported) => {
                        return Ok(vec![Value::I32(ERRNO_NOTSUP)]);
                    }
                };

                let buf_len = *buf_len as u32 as usize;
                if context.read_memory(*buf as u32, buf_len).is_err()
                    || context.read_memory(*bufused as u32, 4).is_err()
                {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }

                let mut payload = Vec::with_capacity(buf_len.min(4096));
                for entry in entries {
                    if payload.len() == buf_len {
                        break;
                    }
                    let mut header = [0u8; DIRENT_SIZE];
                    header[0..8].copy_from_slice(&entry.next.to_le_bytes());
                    header[8..16].copy_from_slice(&entry.inode.to_le_bytes());
                    let name_len = u32::try_from(entry.name.len())
                        .expect("bounded WASI path length fits in a u32");
                    header[16..20].copy_from_slice(&name_len.to_le_bytes());
                    header[20] = entry.filetype;

                    for chunk in [&header[..], entry.name.as_slice()] {
                        let remaining = buf_len.saturating_sub(payload.len());
                        if remaining == 0 {
                            break;
                        }
                        let take = remaining.min(chunk.len());
                        payload.extend_from_slice(&chunk[..take]);
                    }
                }

                let used = payload.len() as u32;
                if context.write_memory(*buf as u32, &payload).is_err()
                    || context
                        .write_memory(*bufused as u32, &used.to_le_bytes())
                        .is_err()
                {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }
                Ok(vec![Value::I32(ERRNO_SUCCESS)])
            },
        )?;

        let link_filesystem = self.clone();
        registry.register_values(
            WASI_MODULE,
            PATH_LINK_NAME,
            vec![
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
            ],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ,
            move |context, args| {
                let [
                    Value::I32(old_fd),
                    Value::I32(old_flags),
                    Value::I32(old_path_ptr),
                    Value::I32(old_path_len),
                    Value::I32(new_fd),
                    Value::I32(new_path_ptr),
                    Value::I32(new_path_len),
                ] = args
                else {
                    return Err(HostError::message(
                        "validated wasi path_link signature received invalid arguments",
                    ));
                };

                let old_fd = *old_fd as u32;
                let new_fd = *new_fd as u32;
                if !link_filesystem.has_directory_descriptor(old_fd)
                    || !link_filesystem.has_directory_descriptor(new_fd)
                {
                    return Ok(vec![Value::I32(ERRNO_BADF)]);
                }
                if *old_flags != 0 {
                    return Ok(vec![Value::I32(ERRNO_INVAL)]);
                }

                let old_path_len = *old_path_len as u32 as usize;
                let new_path_len = *new_path_len as u32 as usize;
                if old_path_len > MAX_RELATIVE_PATH_BYTES || new_path_len > MAX_RELATIVE_PATH_BYTES {
                    return Ok(vec![Value::I32(ERRNO_NAMETOOLONG)]);
                }
                let old_path = match context.read_memory(*old_path_ptr as u32, old_path_len) {
                    Ok(path) => path,
                    Err(_) => return Ok(vec![Value::I32(ERRNO_FAULT)]),
                };
                let new_path = match context.read_memory(*new_path_ptr as u32, new_path_len) {
                    Ok(path) => path,
                    Err(_) => return Ok(vec![Value::I32(ERRNO_FAULT)]),
                };
                for path in [&old_path, &new_path] {
                    match validate_guest_path(path) {
                        Ok(()) => {}
                        Err(GuestPathError::Empty) => {
                            return Ok(vec![Value::I32(ERRNO_INVAL)]);
                        }
                        Err(GuestPathError::TooLong) => {
                            return Ok(vec![Value::I32(ERRNO_NAMETOOLONG)]);
                        }
                        Err(GuestPathError::Unsafe) => {
                            return Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]);
                        }
                    }
                }

                match link_filesystem.link(old_fd, &old_path, new_fd, &new_path) {
                    Ok(()) => Ok(vec![Value::I32(ERRNO_SUCCESS)]),
                    Err(LinkError::BadFd) => Ok(vec![Value::I32(ERRNO_BADF)]),
                    Err(LinkError::NotFound) => Ok(vec![Value::I32(ERRNO_NOENT)]),
                    Err(LinkError::NotCapable) => Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]),
                    Err(LinkError::NameTooLong) => Ok(vec![Value::I32(ERRNO_NAMETOOLONG)]),
                    Err(LinkError::TargetExists) => Ok(vec![Value::I32(ERRNO_EXIST)]),
                    Err(LinkError::TooManyFiles) => Ok(vec![Value::I32(ERRNO_NOSPC)]),
                    Err(LinkError::LinkCountOverflow) => Ok(vec![Value::I32(ERRNO_OVERFLOW)]),
                }
            },
        )?;

        let rename_filesystem = self.clone();
        registry.register_values(
            WASI_MODULE,
            PATH_RENAME_NAME,
            vec![
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
            ],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ,
            move |context, args| {
                let [
                    Value::I32(old_fd),
                    Value::I32(old_path_ptr),
                    Value::I32(old_path_len),
                    Value::I32(new_fd),
                    Value::I32(new_path_ptr),
                    Value::I32(new_path_len),
                ] = args
                else {
                    return Err(HostError::message(
                        "validated wasi path_rename signature received invalid arguments",
                    ));
                };

                let old_fd = *old_fd as u32;
                let new_fd = *new_fd as u32;
                if !rename_filesystem.has_preopen(old_fd)
                    || !rename_filesystem.has_preopen(new_fd)
                {
                    return Ok(vec![Value::I32(ERRNO_BADF)]);
                }
                if !rename_filesystem.is_writable_preopen(old_fd)
                    || !rename_filesystem.is_writable_preopen(new_fd)
                {
                    return Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]);
                }

                let old_path_len = *old_path_len as u32 as usize;
                let new_path_len = *new_path_len as u32 as usize;
                if old_path_len > MAX_RELATIVE_PATH_BYTES || new_path_len > MAX_RELATIVE_PATH_BYTES {
                    return Ok(vec![Value::I32(ERRNO_NAMETOOLONG)]);
                }
                let old_path = match context.read_memory(*old_path_ptr as u32, old_path_len) {
                    Ok(path) => path,
                    Err(_) => return Ok(vec![Value::I32(ERRNO_FAULT)]),
                };
                let new_path = match context.read_memory(*new_path_ptr as u32, new_path_len) {
                    Ok(path) => path,
                    Err(_) => return Ok(vec![Value::I32(ERRNO_FAULT)]),
                };
                for path in [&old_path, &new_path] {
                    match validate_guest_path(path) {
                        Ok(()) => {}
                        Err(GuestPathError::Empty) => {
                            return Ok(vec![Value::I32(ERRNO_INVAL)]);
                        }
                        Err(GuestPathError::TooLong) => {
                            return Ok(vec![Value::I32(ERRNO_NAMETOOLONG)]);
                        }
                        Err(GuestPathError::Unsafe) => {
                            return Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]);
                        }
                    }
                }

                match rename_filesystem.rename(old_fd, &old_path, new_fd, &new_path) {
                    Ok(()) => Ok(vec![Value::I32(ERRNO_SUCCESS)]),
                    Err(RenameError::NotFound) => Ok(vec![Value::I32(ERRNO_NOENT)]),
                    Err(RenameError::NotCapable) => Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]),
                    Err(RenameError::InvalidLinkCount) => Ok(vec![Value::I32(ERRNO_IO)]),
                }
            },
        )?;

        let unlink_filesystem = self.clone();
        registry.register_values(
            WASI_MODULE,
            PATH_UNLINK_FILE_NAME,
            vec![ValueType::I32, ValueType::I32, ValueType::I32],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ,
            move |context, args| {
                let [Value::I32(dir_fd), Value::I32(path_ptr), Value::I32(path_len)] = args else {
                    return Err(HostError::message(
                        "validated wasi path_unlink_file signature received invalid arguments",
                    ));
                };

                let dir_fd = *dir_fd as u32;
                if !unlink_filesystem.has_directory_descriptor(dir_fd) {
                    return Ok(vec![Value::I32(ERRNO_BADF)]);
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

                match unlink_filesystem.unlink(dir_fd, &path) {
                    Ok(()) => Ok(vec![Value::I32(ERRNO_SUCCESS)]),
                    Err(UnlinkError::BadFd) => Ok(vec![Value::I32(ERRNO_BADF)]),
                    Err(UnlinkError::NotFound) => Ok(vec![Value::I32(ERRNO_NOENT)]),
                    Err(UnlinkError::NotCapable) => Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]),
                    Err(UnlinkError::NameTooLong) => Ok(vec![Value::I32(ERRNO_NAMETOOLONG)]),
                    Err(UnlinkError::InvalidLinkCount) => Ok(vec![Value::I32(ERRNO_IO)]),
                }
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

        let resize_filesystem = self.clone();
        registry.register_values(
            WASI_MODULE,
            FD_FILESTAT_SET_SIZE_NAME,
            vec![ValueType::I32, ValueType::I64],
            vec![ValueType::I32],
            HostCapabilities::NONE,
            move |_context, args| {
                let [Value::I32(fd), Value::I64(size)] = args else {
                    return Err(HostError::message(
                        "validated wasi fd_filestat_set_size signature received invalid arguments",
                    ));
                };

                let fd = *fd as u32;
                if resize_filesystem.is_known_non_file(fd) {
                    return Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]);
                }
                match resize_filesystem.set_size(fd, *size as u64) {
                    Ok(()) => Ok(vec![Value::I32(ERRNO_SUCCESS)]),
                    Err(error) => Ok(vec![Value::I32(write_errno(error))]),
                }
            },
        )?;

        let set_times_filesystem = self.clone();
        registry.register_values(
            WASI_MODULE,
            FD_FILESTAT_SET_TIMES_NAME,
            vec![
                ValueType::I32,
                ValueType::I64,
                ValueType::I64,
                ValueType::I32,
            ],
            vec![ValueType::I32],
            HostCapabilities::NONE,
            move |_context, args| {
                let [Value::I32(fd), Value::I64(atim), Value::I64(mtim), Value::I32(fst_flags)] =
                    args
                else {
                    return Err(HostError::message(
                        "validated wasi fd_filestat_set_times signature received invalid arguments",
                    ));
                };
                let update = match resolve_time_update(
                    *atim as u64,
                    *mtim as u64,
                    *fst_flags as u32,
                    realtime_now,
                ) {
                    Ok(update) => update,
                    Err(()) => return Ok(vec![Value::I32(ERRNO_INVAL)]),
                };
                let fd = *fd as u32;
                if set_times_filesystem.is_known_non_file(fd) {
                    return Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]);
                }
                match set_times_filesystem.set_times(fd, update) {
                    Ok(()) => Ok(vec![Value::I32(ERRNO_SUCCESS)]),
                    Err(DescriptorFilestatError::BadFd) => Ok(vec![Value::I32(ERRNO_BADF)]),
                    Err(DescriptorFilestatError::NotCapable) => {
                        Ok(vec![Value::I32(ERRNO_NOTCAPABLE)])
                    }
                }
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
        fd <= 2 || self.has_preopen(fd) || self.state.borrow().open_directories.contains_key(&fd)
    }

    fn has_directory_descriptor(&self, fd: u32) -> bool {
        let state = self.state.borrow();
        state.reserved_preopens.contains(&fd) || state.open_directories.contains_key(&fd)
    }

    fn resolve_directory_path(
        &self,
        fd: u32,
        path: &[u8],
        required_right: u64,
    ) -> Result<(u32, Vec<u8>, u64), ResolveDirectoryError> {
        let state = self.state.borrow();
        let Some((preopen_fd, base, rights)) = directory_context(&state, fd) else {
            return Err(ResolveDirectoryError::BadFd);
        };
        if rights & required_right != required_right {
            return Err(ResolveDirectoryError::NotCapable);
        }
        let full_path = join_relative(&base, path);
        if full_path.len() > MAX_RELATIVE_PATH_BYTES {
            return Err(ResolveDirectoryError::NameTooLong);
        }
        Ok((preopen_fd, full_path, rights))
    }

    fn allocate_dynamic_fd(state: &FilesystemState) -> Result<u32, OpenError> {
        if state.open_files.len() + state.open_directories.len() >= MAX_OPEN_FILES {
            return Err(OpenError::TooManyOpenFiles);
        }
        let mut candidate = FIRST_DYNAMIC_FD;
        loop {
            if !state.reserved_preopens.contains(&candidate)
                && !state.open_files.contains_key(&candidate)
                && !state.open_directories.contains_key(&candidate)
            {
                return Ok(candidate);
            }
            candidate = candidate
                .checked_add(1)
                .ok_or(OpenError::TooManyOpenFiles)?;
        }
    }

    fn open_directory(
        &self,
        dir_fd: u32,
        path: &[u8],
        rights_base: u64,
    ) -> Result<OpenedFile, OpenError> {
        let (preopen_fd, full_path, parent_rights) = self
            .resolve_directory_path(dir_fd, path, RIGHTS_PATH_OPEN)
            .map_err(open_resolve_error)?;
        if rights_base & !parent_rights != 0 {
            return Err(OpenError::NotCapable);
        }
        let mut state = self.state.borrow_mut();
        if state.mounted_files.iter().any(|file| {
            file.preopen_fd == preopen_fd && file.relative_path.as_slice() == full_path.as_slice()
        }) || state.mounted_symlinks.iter().any(|symlink| {
            symlink.preopen_fd == preopen_fd
                && symlink.relative_path.as_slice() == full_path.as_slice()
        }) {
            return Err(OpenError::NotDirectory);
        }
        let Some(inode) = state
            .mounted_directories
            .iter()
            .find(|directory| {
                directory.preopen_fd == preopen_fd
                    && directory.relative_path.as_slice() == full_path.as_slice()
            })
            .map(|directory| directory.inode)
        else {
            return Err(OpenError::NotFound);
        };
        let Some(parent_inode) = directory_inode(&state, preopen_fd, parent_path(&full_path))
        else {
            return Err(OpenError::NotFound);
        };
        let fd = Self::allocate_dynamic_fd(&state)?;
        state.open_directories.insert(
            fd,
            OpenDirectory {
                preopen_fd,
                relative_path: full_path,
                inode,
                parent_inode,
                rights_base,
            },
        );
        Ok(OpenedFile { fd, created: false })
    }

    fn open(
        &self,
        dir_fd: u32,
        path: &[u8],
        rights_base: u64,
        create: bool,
    ) -> Result<OpenedFile, OpenError> {
        let (preopen_fd, full_path, parent_rights) = self
            .resolve_directory_path(dir_fd, path, RIGHTS_PATH_OPEN)
            .map_err(open_resolve_error)?;
        if create && parent_rights & RIGHTS_PATH_CREATE_FILE == 0 {
            return Err(OpenError::NotCapable);
        }
        let mut state = self.state.borrow_mut();
        let candidate = Self::allocate_dynamic_fd(&state)?;
        if state.mounted_directories.iter().any(|directory| {
            directory.preopen_fd == preopen_fd
                && directory.relative_path.as_slice() == full_path.as_slice()
        }) {
            return Err(OpenError::NotDirectory);
        }
        if state.mounted_symlinks.iter().any(|symlink| {
            symlink.preopen_fd == preopen_fd
                && symlink.relative_path.as_slice() == full_path.as_slice()
        }) {
            return Err(OpenError::NotCapable);
        }
        let existing = state
            .mounted_files
            .iter()
            .find(|file| {
                file.preopen_fd == preopen_fd
                    && file.relative_path.as_slice() == full_path.as_slice()
            })
            .map(|file| {
                (
                    file.inode,
                    file.bytes.clone(),
                    file.link_count.clone(),
                    file.times.clone(),
                    file.writable,
                )
            });
        let (inode, bytes, link_count, times, writable, created) = if let Some(existing) = existing
        {
            (
                existing.0, existing.1, existing.2, existing.3, existing.4, false,
            )
        } else {
            if !create {
                return Err(OpenError::NotFound);
            }
            if !state.writable_preopens.contains(&preopen_fd) {
                return Err(OpenError::NotCapable);
            }
            if namespace_entry_count(&state) >= MAX_MOUNTED_FILES {
                return Err(OpenError::TooManyFiles);
            }
            if !parent_directory_exists(&state, preopen_fd, &full_path) {
                return Err(OpenError::NotFound);
            }
            let inode = state
                .next_inode
                .checked_add(1)
                .ok_or(OpenError::TooManyFiles)?;
            state.next_inode = inode;
            let bytes = Rc::new(RefCell::new(Vec::new()));
            let link_count = Rc::new(RefCell::new(1));
            let times = Rc::new(RefCell::new(FileTimes::default()));
            state.mounted_files.push(MountedFile {
                preopen_fd,
                relative_path: full_path.clone(),
                inode,
                bytes: bytes.clone(),
                link_count: link_count.clone(),
                times: times.clone(),
                writable: true,
            });
            (inode, bytes, link_count, times, true, true)
        };
        let mutation_rights =
            RIGHTS_FD_WRITE | RIGHTS_FD_FILESTAT_SET_SIZE | RIGHTS_FD_FILESTAT_SET_TIMES;
        if rights_base & mutation_rights != 0
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
                inode,
                bytes,
                link_count,
                times,
                offset: 0,
                rights_base,
            },
        );
        Ok(OpenedFile {
            fd: candidate,
            created,
        })
    }

    fn create_directory(&self, fd: u32, path: &[u8]) -> Result<(), DirectoryMutationError> {
        let (preopen_fd, full_path, _) = self
            .resolve_directory_path(fd, path, RIGHTS_PATH_CREATE_DIRECTORY)
            .map_err(directory_resolve_error)?;
        let mut state = self.state.borrow_mut();
        if namespace_entry_exists(&state, preopen_fd, &full_path) {
            return Err(DirectoryMutationError::Exists);
        }
        if namespace_entry_count(&state) >= MAX_MOUNTED_FILES {
            return Err(DirectoryMutationError::TooManyFiles);
        }
        if !parent_directory_exists(&state, preopen_fd, &full_path) {
            return Err(DirectoryMutationError::NotFound);
        }
        let inode = state
            .next_inode
            .checked_add(1)
            .ok_or(DirectoryMutationError::TooManyFiles)?;
        state.next_inode = inode;
        state.mounted_directories.push(MountedDirectory {
            preopen_fd,
            relative_path: full_path,
            inode,
            times: FileTimes::default(),
        });
        Ok(())
    }

    fn remove_directory(&self, fd: u32, path: &[u8]) -> Result<(), DirectoryMutationError> {
        let (preopen_fd, full_path, _) = self
            .resolve_directory_path(fd, path, RIGHTS_PATH_REMOVE_DIRECTORY)
            .map_err(directory_resolve_error)?;
        let mut state = self.state.borrow_mut();
        if state.mounted_files.iter().any(|file| {
            file.preopen_fd == preopen_fd && file.relative_path.as_slice() == full_path.as_slice()
        }) || state.mounted_symlinks.iter().any(|symlink| {
            symlink.preopen_fd == preopen_fd
                && symlink.relative_path.as_slice() == full_path.as_slice()
        }) {
            return Err(DirectoryMutationError::NotDirectory);
        }
        let Some(index) = state.mounted_directories.iter().position(|directory| {
            directory.preopen_fd == preopen_fd
                && directory.relative_path.as_slice() == full_path.as_slice()
        }) else {
            return Err(DirectoryMutationError::NotFound);
        };
        let prefix = directory_prefix(&full_path);
        let has_child_file = state
            .mounted_files
            .iter()
            .any(|file| file.preopen_fd == preopen_fd && file.relative_path.starts_with(&prefix));
        let has_child_directory =
            state
                .mounted_directories
                .iter()
                .enumerate()
                .any(|(other, directory)| {
                    other != index
                        && directory.preopen_fd == preopen_fd
                        && directory.relative_path.starts_with(&prefix)
                });
        let has_child_symlink = state.mounted_symlinks.iter().any(|symlink| {
            symlink.preopen_fd == preopen_fd && symlink.relative_path.starts_with(&prefix)
        });
        if has_child_file || has_child_directory || has_child_symlink {
            return Err(DirectoryMutationError::NotEmpty);
        }
        state.mounted_directories.remove(index);
        Ok(())
    }

    fn symlink(&self, dir_fd: u32, path: &[u8], target: &[u8]) -> Result<(), SymlinkError> {
        let (preopen_fd, full_path, _) = self
            .resolve_directory_path(dir_fd, path, RIGHTS_PATH_SYMLINK)
            .map_err(symlink_resolve_error)?;
        let mut state = self.state.borrow_mut();
        if namespace_entry_exists(&state, preopen_fd, &full_path) {
            return Err(SymlinkError::Exists);
        }
        if !parent_directory_exists(&state, preopen_fd, &full_path) {
            return Err(SymlinkError::NotFound);
        }
        if namespace_entry_count(&state) >= MAX_MOUNTED_FILES {
            return Err(SymlinkError::TooManyFiles);
        }
        let inode = state
            .next_inode
            .checked_add(1)
            .ok_or(SymlinkError::TooManyFiles)?;
        state.next_inode = inode;
        state.mounted_symlinks.push(MountedSymlink {
            preopen_fd,
            relative_path: full_path,
            inode,
            target: target.to_vec(),
            times: FileTimes::default(),
        });
        Ok(())
    }

    fn readlink(&self, dir_fd: u32, path: &[u8]) -> Result<Vec<u8>, SymlinkError> {
        let (preopen_fd, full_path, _) = self
            .resolve_directory_path(dir_fd, path, RIGHTS_PATH_READLINK)
            .map_err(symlink_resolve_error)?;
        let state = self.state.borrow();
        if let Some(symlink) = state.mounted_symlinks.iter().find(|symlink| {
            symlink.preopen_fd == preopen_fd
                && symlink.relative_path.as_slice() == full_path.as_slice()
        }) {
            return Ok(symlink.target.clone());
        }
        if namespace_entry_exists(&state, preopen_fd, &full_path) {
            return Err(SymlinkError::NotLink);
        }
        Err(SymlinkError::NotFound)
    }

    fn link(
        &self,
        old_dir_fd: u32,
        old_path: &[u8],
        new_dir_fd: u32,
        new_path: &[u8],
    ) -> Result<(), LinkError> {
        let (old_preopen_fd, old_full_path, _) = self
            .resolve_directory_path(old_dir_fd, old_path, RIGHTS_PATH_LINK_SOURCE)
            .map_err(link_resolve_error)?;
        let (new_preopen_fd, new_full_path, _) = self
            .resolve_directory_path(new_dir_fd, new_path, RIGHTS_PATH_LINK_TARGET)
            .map_err(link_resolve_error)?;

        let mut state = self.state.borrow_mut();
        if namespace_entry_count(&state) >= MAX_MOUNTED_FILES {
            return Err(LinkError::TooManyFiles);
        }
        if namespace_entry_exists(&state, new_preopen_fd, &new_full_path) {
            return Err(LinkError::TargetExists);
        }
        if !parent_directory_exists(&state, new_preopen_fd, &new_full_path) {
            return Err(LinkError::NotFound);
        }
        let Some((inode, bytes, link_count, times, writable)) = state
            .mounted_files
            .iter()
            .find(|file| {
                file.preopen_fd == old_preopen_fd
                    && file.relative_path.as_slice() == old_full_path.as_slice()
            })
            .map(|file| {
                (
                    file.inode,
                    file.bytes.clone(),
                    file.link_count.clone(),
                    file.times.clone(),
                    file.writable,
                )
            })
        else {
            return Err(LinkError::NotFound);
        };
        let current_links = *link_count.borrow();
        let new_links = current_links
            .checked_add(1)
            .ok_or(LinkError::LinkCountOverflow)?;
        if current_links == 0 {
            return Err(LinkError::NotFound);
        }

        state.mounted_files.push(MountedFile {
            preopen_fd: new_preopen_fd,
            relative_path: new_full_path,
            inode,
            bytes,
            link_count: link_count.clone(),
            times,
            writable,
        });
        *link_count.borrow_mut() = new_links;
        Ok(())
    }

    fn rename(
        &self,
        old_preopen_fd: u32,
        old_path: &[u8],
        new_preopen_fd: u32,
        new_path: &[u8],
    ) -> Result<(), RenameError> {
        let mut state = self.state.borrow_mut();
        if !state.writable_preopens.contains(&old_preopen_fd)
            || !state.writable_preopens.contains(&new_preopen_fd)
        {
            return Err(RenameError::NotCapable);
        }
        if state.mounted_symlinks.iter().any(|symlink| {
            (symlink.preopen_fd == old_preopen_fd && symlink.relative_path.as_slice() == old_path)
                || (symlink.preopen_fd == new_preopen_fd
                    && symlink.relative_path.as_slice() == new_path)
        }) {
            return Err(RenameError::NotCapable);
        }

        let Some(source_index) = state.mounted_files.iter().position(|file| {
            file.preopen_fd == old_preopen_fd && file.relative_path.as_slice() == old_path
        }) else {
            return Err(RenameError::NotFound);
        };

        if old_preopen_fd == new_preopen_fd && old_path == new_path {
            return Ok(());
        }

        let target_index = state.mounted_files.iter().position(|file| {
            file.preopen_fd == new_preopen_fd && file.relative_path.as_slice() == new_path
        });

        if let Some(target_index) = target_index {
            if state.mounted_files[target_index].inode == state.mounted_files[source_index].inode {
                return Ok(());
            }
            let target_link_count = state.mounted_files[target_index].link_count.clone();
            let target_links = *target_link_count.borrow();
            let Some(new_target_links) = target_links.checked_sub(1) else {
                return Err(RenameError::InvalidLinkCount);
            };

            state.mounted_files[source_index].preopen_fd = new_preopen_fd;
            state.mounted_files[source_index].relative_path = new_path.to_vec();
            state.mounted_files.remove(target_index);
            *target_link_count.borrow_mut() = new_target_links;
            return Ok(());
        }

        state.mounted_files[source_index].preopen_fd = new_preopen_fd;
        state.mounted_files[source_index].relative_path = new_path.to_vec();
        Ok(())
    }

    fn unlink(&self, dir_fd: u32, path: &[u8]) -> Result<(), UnlinkError> {
        let (preopen_fd, full_path, _) =
            match self.resolve_directory_path(dir_fd, path, RIGHTS_PATH_UNLINK_FILE) {
                Ok(resolved) => resolved,
                Err(ResolveDirectoryError::BadFd) => return Err(UnlinkError::BadFd),
                Err(ResolveDirectoryError::NotCapable) => return Err(UnlinkError::NotCapable),
                Err(ResolveDirectoryError::NameTooLong) => return Err(UnlinkError::NameTooLong),
            };
        let mut state = self.state.borrow_mut();
        if let Some(index) = state.mounted_files.iter().position(|file| {
            file.preopen_fd == preopen_fd && file.relative_path.as_slice() == full_path.as_slice()
        }) {
            let link_count = state.mounted_files[index].link_count.clone();
            let current_links = *link_count.borrow();
            let Some(new_links) = current_links.checked_sub(1) else {
                return Err(UnlinkError::InvalidLinkCount);
            };
            state.mounted_files.remove(index);
            *link_count.borrow_mut() = new_links;
            return Ok(());
        }
        if let Some(index) = state.mounted_symlinks.iter().position(|symlink| {
            symlink.preopen_fd == preopen_fd
                && symlink.relative_path.as_slice() == full_path.as_slice()
        }) {
            state.mounted_symlinks.remove(index);
            return Ok(());
        }
        Err(UnlinkError::NotFound)
    }

    fn rollback_open(&self, dir_fd: u32, path: &[u8], opened: OpenedFile) {
        let resolved = if opened.created {
            self.resolve_directory_path(dir_fd, path, RIGHTS_PATH_OPEN)
                .ok()
        } else {
            None
        };
        let mut state = self.state.borrow_mut();
        state.open_files.remove(&opened.fd);
        state.open_directories.remove(&opened.fd);
        if let Some((preopen_fd, full_path, _)) = resolved {
            if let Some(index) = state.mounted_files.iter().position(|file| {
                file.preopen_fd == preopen_fd
                    && file.relative_path.as_slice() == full_path.as_slice()
            }) {
                let inode = state.mounted_files[index].inode;
                state.mounted_files.remove(index);
                if state.next_inode == inode {
                    state.next_inode = inode.saturating_sub(1);
                }
            }
        }
    }

    fn close(&self, fd: u32) -> bool {
        let mut state = self.state.borrow_mut();
        state.open_files.remove(&fd).is_some() || state.open_directories.remove(&fd).is_some()
    }

    fn prepare_pwrite(&self, fd: u32, offset: u64, len: usize) -> Result<(), DescriptorWriteError> {
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

    fn pwrite(&self, fd: u32, offset: u64, bytes: &[u8]) -> Result<(), DescriptorWriteError> {
        self.prepare_pwrite(fd, offset, bytes.len())?;
        if bytes.is_empty() {
            return Ok(());
        }
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

fn directory_inode(state: &FilesystemState, preopen_fd: u32, path: &[u8]) -> Option<u64> {
    if path.is_empty() {
        return Some((1u64 << 63) | u64::from(preopen_fd));
    }
    state
        .mounted_directories
        .iter()
        .find(|directory| {
            directory.preopen_fd == preopen_fd && directory.relative_path.as_slice() == path
        })
        .map(|directory| directory.inode)
}

fn directory_rights(state: &FilesystemState, fd: u32) -> Option<u64> {
    if state.reserved_preopens.contains(&fd) {
        let mut rights = RIGHTS_FD_READDIR | RIGHTS_PATH_OPEN | RIGHTS_PATH_FILESTAT_GET;
        if state.writable_preopens.contains(&fd) {
            rights |= RIGHTS_PATH_FILESTAT_SET_TIMES
                | RIGHTS_PATH_CREATE_DIRECTORY
                | RIGHTS_PATH_CREATE_FILE
                | RIGHTS_PATH_LINK_SOURCE
                | RIGHTS_PATH_LINK_TARGET
                | RIGHTS_PATH_READLINK
                | RIGHTS_PATH_RENAME_SOURCE
                | RIGHTS_PATH_RENAME_TARGET
                | RIGHTS_PATH_SYMLINK
                | RIGHTS_PATH_REMOVE_DIRECTORY
                | RIGHTS_PATH_UNLINK_FILE;
        }
        if !state.writable_preopens.contains(&fd) {
            rights |= RIGHTS_PATH_READLINK;
        }
        return Some(rights);
    }
    state
        .open_directories
        .get(&fd)
        .map(|directory| directory.rights_base)
}

fn directory_context(state: &FilesystemState, fd: u32) -> Option<(u32, Vec<u8>, u64)> {
    if state.reserved_preopens.contains(&fd) {
        return Some((fd, Vec::new(), directory_rights(state, fd)?));
    }
    state.open_directories.get(&fd).map(|directory| {
        (
            directory.preopen_fd,
            directory.relative_path.clone(),
            directory.rights_base,
        )
    })
}

fn join_relative(base: &[u8], child: &[u8]) -> Vec<u8> {
    if base.is_empty() {
        return child.to_vec();
    }
    let mut joined = Vec::with_capacity(base.len() + 1 + child.len());
    joined.extend_from_slice(base);
    joined.push(b'/');
    joined.extend_from_slice(child);
    joined
}

fn directory_prefix(path: &[u8]) -> Vec<u8> {
    let mut prefix = path.to_vec();
    prefix.push(b'/');
    prefix
}

fn parent_path(path: &[u8]) -> &[u8] {
    path.iter()
        .rposition(|byte| *byte == b'/')
        .map_or(&[], |separator| &path[..separator])
}

fn parent_directory_exists(state: &FilesystemState, preopen_fd: u32, path: &[u8]) -> bool {
    let parent = parent_path(path);
    parent.is_empty()
        || state.mounted_directories.iter().any(|directory| {
            directory.preopen_fd == preopen_fd && directory.relative_path.as_slice() == parent
        })
}

fn namespace_entry_count(state: &FilesystemState) -> usize {
    state.mounted_files.len() + state.mounted_directories.len() + state.mounted_symlinks.len()
}

fn namespace_entry_exists(state: &FilesystemState, preopen_fd: u32, path: &[u8]) -> bool {
    state
        .mounted_files
        .iter()
        .any(|file| file.preopen_fd == preopen_fd && file.relative_path.as_slice() == path)
        || state.mounted_directories.iter().any(|directory| {
            directory.preopen_fd == preopen_fd && directory.relative_path.as_slice() == path
        })
        || state.mounted_symlinks.iter().any(|symlink| {
            symlink.preopen_fd == preopen_fd && symlink.relative_path.as_slice() == path
        })
}

fn relative_to_directory<'a>(base: &[u8], path: &'a [u8]) -> Option<&'a [u8]> {
    if base.is_empty() {
        return Some(path);
    }
    if path.len() <= base.len() || !path.starts_with(base) || path[base.len()] != b'/' {
        return None;
    }
    Some(&path[base.len() + 1..])
}

fn immediate_child_name<'a>(base: &[u8], path: &'a [u8]) -> Option<&'a [u8]> {
    let remainder = relative_to_directory(base, path)?;
    if remainder.is_empty() || remainder.contains(&b'/') {
        return None;
    }
    Some(remainder)
}

fn open_resolve_error(error: ResolveDirectoryError) -> OpenError {
    match error {
        ResolveDirectoryError::BadFd | ResolveDirectoryError::NotCapable => OpenError::NotCapable,
        ResolveDirectoryError::NameTooLong => OpenError::NameTooLong,
    }
}

fn link_resolve_error(error: ResolveDirectoryError) -> LinkError {
    match error {
        ResolveDirectoryError::BadFd => LinkError::BadFd,
        ResolveDirectoryError::NotCapable => LinkError::NotCapable,
        ResolveDirectoryError::NameTooLong => LinkError::NameTooLong,
    }
}

fn symlink_resolve_error(error: ResolveDirectoryError) -> SymlinkError {
    match error {
        ResolveDirectoryError::BadFd => SymlinkError::BadFd,
        ResolveDirectoryError::NotCapable => SymlinkError::NotCapable,
        ResolveDirectoryError::NameTooLong => SymlinkError::NameTooLong,
    }
}

fn symlink_errno(error: SymlinkError) -> i32 {
    match error {
        SymlinkError::BadFd => ERRNO_BADF,
        SymlinkError::NotFound => ERRNO_NOENT,
        SymlinkError::NotCapable => ERRNO_NOTCAPABLE,
        SymlinkError::NameTooLong => ERRNO_NAMETOOLONG,
        SymlinkError::Exists => ERRNO_EXIST,
        SymlinkError::TooManyFiles => ERRNO_NOSPC,
        SymlinkError::NotLink => ERRNO_INVAL,
    }
}

fn directory_resolve_error(error: ResolveDirectoryError) -> DirectoryMutationError {
    match error {
        ResolveDirectoryError::BadFd => DirectoryMutationError::BadFd,
        ResolveDirectoryError::NotCapable => DirectoryMutationError::NotCapable,
        ResolveDirectoryError::NameTooLong => DirectoryMutationError::NameTooLong,
    }
}

fn directory_errno(error: DirectoryMutationError) -> i32 {
    match error {
        DirectoryMutationError::BadFd => ERRNO_BADF,
        DirectoryMutationError::NotFound => ERRNO_NOENT,
        DirectoryMutationError::NotCapable => ERRNO_NOTCAPABLE,
        DirectoryMutationError::NameTooLong => ERRNO_NAMETOOLONG,
        DirectoryMutationError::Exists => ERRNO_EXIST,
        DirectoryMutationError::NotDirectory => ERRNO_NOTDIR,
        DirectoryMutationError::NotEmpty => ERRNO_NOTEMPTY,
        DirectoryMutationError::TooManyFiles => ERRNO_NOSPC,
    }
}

fn resolve_time_update(
    atim: u64,
    mtim: u64,
    flags: u32,
    realtime_now: Option<u64>,
) -> Result<TimeUpdate, ()> {
    if flags & !FSTFLAGS_ALL != 0
        || flags & FSTFLAGS_ATIM != 0 && flags & FSTFLAGS_ATIM_NOW != 0
        || flags & FSTFLAGS_MTIM != 0 && flags & FSTFLAGS_MTIM_NOW != 0
    {
        return Err(());
    }
    let resolved_atim = if flags & FSTFLAGS_ATIM != 0 {
        Some(atim)
    } else if flags & FSTFLAGS_ATIM_NOW != 0 {
        Some(realtime_now.ok_or(())?)
    } else {
        None
    };
    let resolved_mtim = if flags & FSTFLAGS_MTIM != 0 {
        Some(mtim)
    } else if flags & FSTFLAGS_MTIM_NOW != 0 {
        Some(realtime_now.ok_or(())?)
    } else {
        None
    };
    Ok(TimeUpdate {
        atim: resolved_atim,
        mtim: resolved_mtim,
    })
}

fn apply_time_update(times: &mut FileTimes, update: TimeUpdate) {
    if let Some(atim) = update.atim {
        times.atim = atim;
    }
    if let Some(mtim) = update.mtim {
        times.mtim = mtim;
    }
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
