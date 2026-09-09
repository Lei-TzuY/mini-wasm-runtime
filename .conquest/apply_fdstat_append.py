from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one anchor, found {count}")
    return text.replace(old, new, 1)

root = Path("crates/wasm-wasi/src/root.rs")
s = root.read_text()
s = replace_once(
    s,
    "pub const RIGHTS_FD_SEEK: u64 = 1 << 2;\n",
    "pub const RIGHTS_FD_SEEK: u64 = 1 << 2;\npub const RIGHTS_FD_FDSTAT_SET_FLAGS: u64 = 1 << 3;\npub const FDFLAGS_APPEND: u16 = 1 << 0;\n",
    "root constants",
)
s = replace_once(
    s,
    "            RIGHTS_FD_READ\n                | RIGHTS_FD_WRITE\n                | RIGHTS_FD_SEEK\n",
    "            RIGHTS_FD_READ\n                | RIGHTS_FD_WRITE\n                | RIGHTS_FD_FDSTAT_SET_FLAGS\n                | RIGHTS_FD_SEEK\n",
    "writable inheriting rights",
)
root.write_text(s)

fs = Path("crates/wasm-wasi/src/filesystem.rs")
s = fs.read_text()
s = replace_once(
    s,
    "    FILETYPE_SYMBOLIC_LINK, LOOKUPFLAGS_SYMLINK_FOLLOW, OFLAGS_CREAT, OFLAGS_DIRECTORY,\n    RIGHTS_FD_FILESTAT_GET, RIGHTS_FD_FILESTAT_SET_SIZE, RIGHTS_FD_FILESTAT_SET_TIMES,\n",
    "    FDFLAGS_APPEND, FILETYPE_SYMBOLIC_LINK, LOOKUPFLAGS_SYMLINK_FOLLOW, OFLAGS_CREAT,\n    OFLAGS_DIRECTORY, RIGHTS_FD_FDSTAT_SET_FLAGS, RIGHTS_FD_FILESTAT_GET,\n    RIGHTS_FD_FILESTAT_SET_SIZE, RIGHTS_FD_FILESTAT_SET_TIMES,\n",
    "filesystem imports",
)
s = replace_once(
    s,
    'const FD_READDIR_NAME: &str = "fd_readdir";\n',
    'const FD_READDIR_NAME: &str = "fd_readdir";\nconst FD_FDSTAT_SET_FLAGS_NAME: &str = "fd_fdstat_set_flags";\n',
    "host name",
)
s = replace_once(
    s,
    "    offset: u64,\n    rights_base: u64,\n}\n",
    "    offset: u64,\n    rights_base: u64,\n    flags: u16,\n}\n",
    "open file flags field",
)
s = replace_once(
    s,
    "pub(crate) enum DescriptorWriteError {\n    BadFd,\n    NotCapable,\n    FileTooLarge,\n}\n",
    "pub(crate) enum DescriptorWriteError {\n    BadFd,\n    NotCapable,\n    FileTooLarge,\n}\n\n#[derive(Debug, Clone, Copy, PartialEq, Eq)]\nenum DescriptorFlagsError {\n    BadFd,\n    NotCapable,\n    InvalidFlags,\n}\n",
    "flags error type",
)
s = replace_once(
    s,
    "        let len = u64::try_from(len).map_err(|_| DescriptorWriteError::FileTooLarge)?;\n        let end = file\n            .offset\n            .checked_add(len)\n            .ok_or(DescriptorWriteError::FileTooLarge)?;\n",
    "        let len = u64::try_from(len).map_err(|_| DescriptorWriteError::FileTooLarge)?;\n        let start = if file.flags & FDFLAGS_APPEND != 0 {\n            file.bytes.borrow().len() as u64\n        } else {\n            file.offset\n        };\n        let end = start\n            .checked_add(len)\n            .ok_or(DescriptorWriteError::FileTooLarge)?;\n",
    "append prepare write",
)
s = replace_once(
    s,
    "        let start = usize::try_from(file.offset).map_err(|_| DescriptorWriteError::FileTooLarge)?;\n        let end = start\n            .checked_add(bytes.len())\n            .ok_or(DescriptorWriteError::FileTooLarge)?;\n        if end > MAX_FILE_BYTES {\n            return Err(DescriptorWriteError::FileTooLarge);\n        }\n        {\n            let mut file_bytes = file.bytes.borrow_mut();\n            if file_bytes.len() < start {\n                file_bytes.resize(start, 0);\n            }\n            if file_bytes.len() < end {\n                file_bytes.resize(end, 0);\n            }\n            file_bytes[start..end].copy_from_slice(bytes);\n        }\n",
    "        let mut file_bytes = file.bytes.borrow_mut();\n        let start = if file.flags & FDFLAGS_APPEND != 0 {\n            file_bytes.len()\n        } else {\n            usize::try_from(file.offset).map_err(|_| DescriptorWriteError::FileTooLarge)?\n        };\n        let end = start\n            .checked_add(bytes.len())\n            .ok_or(DescriptorWriteError::FileTooLarge)?;\n        if end > MAX_FILE_BYTES {\n            return Err(DescriptorWriteError::FileTooLarge);\n        }\n        if file_bytes.len() < start {\n            file_bytes.resize(start, 0);\n        }\n        if file_bytes.len() < end {\n            file_bytes.resize(end, 0);\n        }\n        file_bytes[start..end].copy_from_slice(bytes);\n        drop(file_bytes);\n",
    "append write",
)
s = replace_once(
    s,
    "    pub(crate) fn fdstat(&self, fd: u32) -> Option<(u8, u64, u64)> {\n        let state = self.state.borrow();\n        if let Some(file) = state.open_files.get(&fd) {\n            return Some((FILETYPE_REGULAR_FILE, file.rights_base, 0));\n        }\n        state\n            .open_directories\n            .get(&fd)\n            .map(|directory| (FILETYPE_DIRECTORY, directory.rights_base, 0))\n    }\n",
    "    pub(crate) fn fdstat(&self, fd: u32) -> Option<(u8, u16, u64, u64)> {\n        let state = self.state.borrow();\n        if let Some(file) = state.open_files.get(&fd) {\n            return Some((FILETYPE_REGULAR_FILE, file.flags, file.rights_base, 0));\n        }\n        state\n            .open_directories\n            .get(&fd)\n            .map(|directory| (FILETYPE_DIRECTORY, 0, directory.rights_base, 0))\n    }\n\n    fn set_flags(&self, fd: u32, flags: u16) -> Result<(), DescriptorFlagsError> {\n        let mut state = self.state.borrow_mut();\n        let Some(file) = state.open_files.get_mut(&fd) else {\n            return Err(DescriptorFlagsError::BadFd);\n        };\n        if file.rights_base & RIGHTS_FD_FDSTAT_SET_FLAGS == 0 {\n            return Err(DescriptorFlagsError::NotCapable);\n        }\n        if flags & !FDFLAGS_APPEND != 0 {\n            return Err(DescriptorFlagsError::InvalidFlags);\n        }\n        file.flags = flags;\n        Ok(())\n    }\n",
    "fdstat flags",
)
s = replace_once(
    s,
    "                let open_flags = *open_flags as u32;\n                let supported_flags = OFLAGS_CREAT | OFLAGS_DIRECTORY;\n                if *dir_flags != 0 || *fd_flags != 0 || open_flags & !supported_flags != 0 {\n                    return Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]);\n                }\n",
    "                let open_flags = *open_flags as u32;\n                let fd_flags = *fd_flags as u32;\n                let supported_flags = OFLAGS_CREAT | OFLAGS_DIRECTORY;\n                if *dir_flags != 0 || open_flags & !supported_flags != 0 {\n                    return Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]);\n                }\n                if fd_flags > u16::MAX as u32 || (fd_flags as u16) & !FDFLAGS_APPEND != 0 {\n                    return Ok(vec![Value::I32(ERRNO_INVAL)]);\n                }\n",
    "path open flags validation",
)
s = replace_once(
    s,
    "                let allowed_file_base = RIGHTS_FD_READ | RIGHTS_FD_WRITE | RIGHTS_FD_SEEK\n                    | RIGHTS_FD_TELL | RIGHTS_FD_FILESTAT_GET | RIGHTS_FD_FILESTAT_SET_SIZE\n                    | RIGHTS_FD_FILESTAT_SET_TIMES;\n",
    "                let allowed_file_base = RIGHTS_FD_READ | RIGHTS_FD_WRITE | RIGHTS_FD_SEEK\n                    | RIGHTS_FD_TELL | RIGHTS_FD_FDSTAT_SET_FLAGS | RIGHTS_FD_FILESTAT_GET\n                    | RIGHTS_FD_FILESTAT_SET_SIZE | RIGHTS_FD_FILESTAT_SET_TIMES;\n",
    "allowed file rights",
)
s = replace_once(
    s,
    "                let opened = if directory {\n                    open_filesystem.open_directory(dir_fd, &path, requested_base)\n                } else {\n                    open_filesystem.open(dir_fd, &path, requested_base, create)\n                };\n",
    "                let opened = if directory {\n                    if fd_flags != 0 {\n                        return Ok(vec![Value::I32(ERRNO_INVAL)]);\n                    }\n                    open_filesystem.open_directory(dir_fd, &path, requested_base)\n                } else {\n                    open_filesystem.open(dir_fd, &path, requested_base, create, fd_flags as u16)\n                };\n",
    "open call flags",
)
s = replace_once(
    s,
    "        create: bool,\n    ) -> Result<OpenedFile, OpenError> {\n",
    "        create: bool,\n        flags: u16,\n    ) -> Result<OpenedFile, OpenError> {\n",
    "open signature",
)
s = replace_once(
    s,
    "        let mutation_rights =\n            RIGHTS_FD_WRITE | RIGHTS_FD_FILESTAT_SET_SIZE | RIGHTS_FD_FILESTAT_SET_TIMES;\n",
    "        let mutation_rights = RIGHTS_FD_WRITE\n            | RIGHTS_FD_FDSTAT_SET_FLAGS\n            | RIGHTS_FD_FILESTAT_SET_SIZE\n            | RIGHTS_FD_FILESTAT_SET_TIMES;\n",
    "mutation rights",
)
s = replace_once(
    s,
    "                offset: 0,\n                rights_base,\n            },\n",
    "                offset: 0,\n                rights_base,\n                flags,\n            },\n",
    "store open flags",
)
anchor = "        let readdir_filesystem = self.clone();\n"
registration = '''        let set_flags_filesystem = self.clone();\n        registry.register_values(\n            WASI_MODULE,\n            FD_FDSTAT_SET_FLAGS_NAME,\n            vec![ValueType::I32, ValueType::I32],\n            vec![ValueType::I32],\n            HostCapabilities::NONE,\n            move |_context, args| {\n                let [Value::I32(fd), Value::I32(flags)] = args else {\n                    return Err(HostError::message(\n                        "validated wasi fd_fdstat_set_flags signature received invalid arguments",\n                    ));\n                };\n                let fd = *fd as u32;\n                if set_flags_filesystem.is_known_non_file(fd) {\n                    return Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]);\n                }\n                let flags = *flags as u32;\n                if flags > u16::MAX as u32 {\n                    return Ok(vec![Value::I32(ERRNO_INVAL)]);\n                }\n                match set_flags_filesystem.set_flags(fd, flags as u16) {\n                    Ok(()) => Ok(vec![Value::I32(ERRNO_SUCCESS)]),\n                    Err(DescriptorFlagsError::BadFd) => Ok(vec![Value::I32(ERRNO_BADF)]),\n                    Err(DescriptorFlagsError::NotCapable) => {\n                        Ok(vec![Value::I32(ERRNO_NOTCAPABLE)])\n                    }\n                    Err(DescriptorFlagsError::InvalidFlags) => Ok(vec![Value::I32(ERRNO_INVAL)]),\n                }\n            },\n        )?;\n\n'''
s = replace_once(s, anchor, registration + anchor, "set flags registration")
fs.write_text(s)

base = Path("crates/wasm-wasi/src/lib.rs")
s = base.read_text()
s = replace_once(
    s,
    "                let (filetype, rights_base, rights_inheriting) = match *fd {\n                    0 => (FILETYPE_CHARACTER_DEVICE, RIGHTS_FD_READ, 0),\n                    1 | 2 => (FILETYPE_CHARACTER_DEVICE, RIGHTS_FD_WRITE, 0),\n",
    "                let (filetype, flags, rights_base, rights_inheriting) = match *fd {\n                    0 => (FILETYPE_CHARACTER_DEVICE, 0, RIGHTS_FD_READ, 0),\n                    1 | 2 => (FILETYPE_CHARACTER_DEVICE, 0, RIGHTS_FD_WRITE, 0),\n",
    "base fdstat tuple",
)
s = replace_once(
    s,
    "                            (entry.filetype, entry.rights_base, entry.rights_inheriting)\n",
    "                            (entry.filetype, 0, entry.rights_base, entry.rights_inheriting)\n",
    "extra fdstat flags",
)
s = replace_once(
    s,
    "                bytes[0] = filetype;\n                bytes[8..16].copy_from_slice(&rights_base.to_le_bytes());\n",
    "                bytes[0] = filetype;\n                bytes[2..4].copy_from_slice(&flags.to_le_bytes());\n                bytes[8..16].copy_from_slice(&rights_base.to_le_bytes());\n",
    "encode fdstat flags",
)
base.write_text(s)
