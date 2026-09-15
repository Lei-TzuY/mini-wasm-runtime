from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one anchor, found {count}: {old[:80]!r}")
    p.write_text(text.replace(old, new, 1))

root = "crates/wasm-wasi/src/root.rs"
fs = "crates/wasm-wasi/src/filesystem.rs"
fdstat = "crates/wasm-wasi/tests/fd_fdstat_get.rs"
acceptance = "crates/wasm-wasi/tests/fd_allocate.rs"

replace_once(
    root,
    "pub const RIGHTS_FD_TELL: u64 = 1 << 5;\npub const RIGHTS_FD_READDIR: u64 = 1 << 14;",
    "pub const RIGHTS_FD_TELL: u64 = 1 << 5;\npub const RIGHTS_FD_ALLOCATE: u64 = 1 << 8;\npub const RIGHTS_FD_READDIR: u64 = 1 << 14;",
)
replace_once(
    root,
    "                | RIGHTS_FD_FILESTAT_GET\n                | RIGHTS_FD_FILESTAT_SET_SIZE\n                | RIGHTS_FD_FILESTAT_SET_TIMES",
    "                | RIGHTS_FD_FILESTAT_GET\n                | RIGHTS_FD_ALLOCATE\n                | RIGHTS_FD_FILESTAT_SET_SIZE\n                | RIGHTS_FD_FILESTAT_SET_TIMES",
)

replace_once(
    fs,
    "    RIGHTS_FD_FILESTAT_GET, RIGHTS_FD_FILESTAT_SET_SIZE, RIGHTS_FD_FILESTAT_SET_TIMES,\n    RIGHTS_FD_READ, RIGHTS_FD_READDIR, RIGHTS_FD_SEEK, RIGHTS_FD_TELL, RIGHTS_FD_WRITE,",
    "    RIGHTS_FD_ALLOCATE, RIGHTS_FD_FILESTAT_GET, RIGHTS_FD_FILESTAT_SET_SIZE,\n    RIGHTS_FD_FILESTAT_SET_TIMES, RIGHTS_FD_READ, RIGHTS_FD_READDIR, RIGHTS_FD_SEEK,\n    RIGHTS_FD_TELL, RIGHTS_FD_WRITE,",
)
replace_once(
    fs,
    'const FD_PWRITE_NAME: &str = "fd_pwrite";\nconst FD_FILESTAT_SET_SIZE_NAME: &str = "fd_filestat_set_size";',
    'const FD_PWRITE_NAME: &str = "fd_pwrite";\nconst FD_ALLOCATE_NAME: &str = "fd_allocate";\nconst FD_FILESTAT_SET_SIZE_NAME: &str = "fd_filestat_set_size";',
)
replace_once(
    fs,
    "                let allowed_file_base = RIGHTS_FD_READ | RIGHTS_FD_WRITE | RIGHTS_FD_SEEK\n                    | RIGHTS_FD_TELL | RIGHTS_FD_FILESTAT_GET | RIGHTS_FD_FILESTAT_SET_SIZE\n                    | RIGHTS_FD_FILESTAT_SET_TIMES;",
    "                let allowed_file_base = RIGHTS_FD_READ | RIGHTS_FD_WRITE | RIGHTS_FD_SEEK\n                    | RIGHTS_FD_TELL | RIGHTS_FD_ALLOCATE | RIGHTS_FD_FILESTAT_GET\n                    | RIGHTS_FD_FILESTAT_SET_SIZE | RIGHTS_FD_FILESTAT_SET_TIMES;",
)
replace_once(
    fs,
    "        let mutation_rights =\n            RIGHTS_FD_WRITE | RIGHTS_FD_FILESTAT_SET_SIZE | RIGHTS_FD_FILESTAT_SET_TIMES;",
    "        let mutation_rights = RIGHTS_FD_WRITE\n            | RIGHTS_FD_ALLOCATE\n            | RIGHTS_FD_FILESTAT_SET_SIZE\n            | RIGHTS_FD_FILESTAT_SET_TIMES;",
)

set_size_anchor = """    pub(crate) fn set_size(&self, fd: u32, size: u64) -> Result<(), DescriptorWriteError> {
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
"""
allocate_method = set_size_anchor + """
    pub(crate) fn allocate(
        &self,
        fd: u32,
        offset: u64,
        len: u64,
    ) -> Result<(), DescriptorWriteError> {
        let state = self.state.borrow();
        let Some(file) = state.open_files.get(&fd) else {
            return Err(DescriptorWriteError::BadFd);
        };
        if file.rights_base & RIGHTS_FD_ALLOCATE == 0 {
            return Err(DescriptorWriteError::NotCapable);
        }
        let end = offset
            .checked_add(len)
            .ok_or(DescriptorWriteError::FileTooLarge)?;
        if end > MAX_FILE_BYTES as u64 {
            return Err(DescriptorWriteError::FileTooLarge);
        }
        let end = usize::try_from(end).map_err(|_| DescriptorWriteError::FileTooLarge)?;
        let mut bytes = file.bytes.borrow_mut();
        if bytes.len() < end {
            bytes.resize(end, 0);
        }
        Ok(())
    }
"""
replace_once(fs, set_size_anchor, allocate_method)

resize_anchor = """        let resize_filesystem = self.clone();
        registry.register_values(
            WASI_MODULE,
            FD_FILESTAT_SET_SIZE_NAME,
"""
allocate_registration = """        let allocate_filesystem = self.clone();
        registry.register_values(
            WASI_MODULE,
            FD_ALLOCATE_NAME,
            vec![ValueType::I32, ValueType::I64, ValueType::I64],
            vec![ValueType::I32],
            HostCapabilities::NONE,
            move |_context, args| {
                let [Value::I32(fd), Value::I64(offset), Value::I64(len)] = args else {
                    return Err(HostError::message(
                        "validated wasi fd_allocate signature received invalid arguments",
                    ));
                };
                let fd = *fd as u32;
                if allocate_filesystem.is_known_non_file(fd) {
                    return Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]);
                }
                match allocate_filesystem.allocate(fd, *offset as u64, *len as u64) {
                    Ok(()) => Ok(vec![Value::I32(ERRNO_SUCCESS)]),
                    Err(error) => Ok(vec![Value::I32(write_errno(error))]),
                }
            },
        )?;

""" + resize_anchor
replace_once(fs, resize_anchor, allocate_registration)

replace_once(
    fdstat,
    "    FILETYPE_DIRECTORY, RIGHTS_FD_FILESTAT_GET, RIGHTS_FD_FILESTAT_SET_SIZE,\n",
    "    FILETYPE_DIRECTORY, RIGHTS_FD_ALLOCATE, RIGHTS_FD_FILESTAT_GET, RIGHTS_FD_FILESTAT_SET_SIZE,\n",
)
replace_once(
    fdstat,
    "            | RIGHTS_FD_FILESTAT_GET\n            | RIGHTS_FD_FILESTAT_SET_SIZE\n",
    "            | RIGHTS_FD_ALLOCATE\n            | RIGHTS_FD_FILESTAT_GET\n            | RIGHTS_FD_FILESTAT_SET_SIZE\n",
)

replace_once(
    acceptance,
    "    WasiPreview1, ERRNO_BADF, ERRNO_FBIG, ERRNO_NOTCAPABLE, ERRNO_SUCCESS, OFLAGS_CREAT,\n    RIGHTS_FD_SEEK, RIGHTS_FD_TELL,\n};\n\nconst RIGHTS_FD_ALLOCATE: u64 = 1 << 8;",
    "    WasiPreview1, ERRNO_BADF, ERRNO_FBIG, ERRNO_NOTCAPABLE, ERRNO_SUCCESS, OFLAGS_CREAT,\n    RIGHTS_FD_ALLOCATE, RIGHTS_FD_SEEK, RIGHTS_FD_TELL,\n};",
)
