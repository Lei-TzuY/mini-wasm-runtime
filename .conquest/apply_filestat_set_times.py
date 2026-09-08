from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one anchor, found {count}")
    return text.replace(old, new, 1)


root_path = Path("crates/wasm-wasi/src/root.rs")
root = root_path.read_text()
root = replace_once(
    root,
    "pub const RIGHTS_PATH_FILESTAT_GET: u64 = 1 << 18;\n",
    "pub const RIGHTS_PATH_FILESTAT_GET: u64 = 1 << 18;\n"
    "pub const RIGHTS_PATH_FILESTAT_SET_TIMES: u64 = 1 << 20;\n"
    "pub const RIGHTS_FD_FILESTAT_SET_TIMES: u64 = 1 << 23;\n",
    "root rights constants",
)
root = replace_once(
    root,
    "                | RIGHTS_PATH_FILESTAT_GET\n                | RIGHTS_FD_READDIR\n",
    "                | RIGHTS_PATH_FILESTAT_GET\n                | RIGHTS_PATH_FILESTAT_SET_TIMES\n                | RIGHTS_FD_READDIR\n",
    "writable preopen pathname rights",
)
root = replace_once(
    root,
    "                | RIGHTS_FD_FILESTAT_GET\n                | RIGHTS_FD_FILESTAT_SET_SIZE\n",
    "                | RIGHTS_FD_FILESTAT_GET\n                | RIGHTS_FD_FILESTAT_SET_SIZE\n                | RIGHTS_FD_FILESTAT_SET_TIMES\n",
    "writable preopen inheriting rights",
)
root = replace_once(
    root,
    "        self.filesystem.register(registry)?;\n",
    "        self.filesystem\n            .register(registry, self.clocks.realtime_time())?;\n",
    "filesystem register clock injection",
)
root_path.write_text(root)

clock_path = Path("crates/wasm-wasi/src/clock.rs")
clock = clock_path.read_text()
clock = replace_once(
    clock,
    "    fn snapshot(&self, raw_id: i32) -> Option<ClockSnapshot> {\n",
    "    pub(crate) fn realtime_time(&self) -> Option<u64> {\n"
    "        self.snapshots[WasiClockId::Realtime.index()].map(|snapshot| snapshot.time_ns)\n"
    "    }\n\n"
    "    fn snapshot(&self, raw_id: i32) -> Option<ClockSnapshot> {\n",
    "clock realtime accessor",
)
clock_path.write_text(clock)

fs_path = Path("crates/wasm-wasi/src/filesystem.rs")
fs = fs_path.read_text()
fs = replace_once(
    fs,
    "    RIGHTS_FD_FILESTAT_GET, RIGHTS_FD_FILESTAT_SET_SIZE, RIGHTS_FD_READ, RIGHTS_FD_READDIR,\n"
    "    RIGHTS_FD_SEEK, RIGHTS_FD_TELL, RIGHTS_FD_WRITE, RIGHTS_PATH_CREATE_DIRECTORY,\n"
    "    RIGHTS_PATH_CREATE_FILE, RIGHTS_PATH_FILESTAT_GET, RIGHTS_PATH_LINK_SOURCE,\n",
    "    RIGHTS_FD_FILESTAT_GET, RIGHTS_FD_FILESTAT_SET_SIZE, RIGHTS_FD_FILESTAT_SET_TIMES,\n"
    "    RIGHTS_FD_READ, RIGHTS_FD_READDIR, RIGHTS_FD_SEEK, RIGHTS_FD_TELL, RIGHTS_FD_WRITE,\n"
    "    RIGHTS_PATH_CREATE_DIRECTORY, RIGHTS_PATH_CREATE_FILE, RIGHTS_PATH_FILESTAT_GET,\n"
    "    RIGHTS_PATH_FILESTAT_SET_TIMES, RIGHTS_PATH_LINK_SOURCE,\n",
    "filesystem rights imports",
)
fs = replace_once(
    fs,
    "const PATH_FILESTAT_GET_NAME: &str = \"path_filestat_get\";\n",
    "const PATH_FILESTAT_GET_NAME: &str = \"path_filestat_get\";\n"
    "const PATH_FILESTAT_SET_TIMES_NAME: &str = \"path_filestat_set_times\";\n",
    "path set times name",
)
fs = replace_once(
    fs,
    "const FD_FILESTAT_SET_SIZE_NAME: &str = \"fd_filestat_set_size\";\n",
    "const FD_FILESTAT_SET_SIZE_NAME: &str = \"fd_filestat_set_size\";\n"
    "const FD_FILESTAT_SET_TIMES_NAME: &str = \"fd_filestat_set_times\";\n"
    "const FSTFLAGS_ATIM: u32 = 1 << 0;\n"
    "const FSTFLAGS_ATIM_NOW: u32 = 1 << 1;\n"
    "const FSTFLAGS_MTIM: u32 = 1 << 2;\n"
    "const FSTFLAGS_MTIM_NOW: u32 = 1 << 3;\n"
    "const FSTFLAGS_ALL: u32 = FSTFLAGS_ATIM | FSTFLAGS_ATIM_NOW | FSTFLAGS_MTIM | FSTFLAGS_MTIM_NOW;\n",
    "fd set times constants",
)
fs = replace_once(
    fs,
    "#[derive(Debug, Clone)]\nstruct MountedFile {\n",
    "#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]\n"
    "struct FileTimes {\n"
    "    atim: u64,\n"
    "    mtim: u64,\n"
    "    ctim: u64,\n"
    "}\n\n"
    "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n"
    "struct TimeUpdate {\n"
    "    atim: Option<u64>,\n"
    "    mtim: Option<u64>,\n"
    "}\n\n"
    "#[derive(Debug, Clone)]\nstruct MountedFile {\n",
    "timestamp structs",
)
fs = replace_once(fs, "    link_count: Rc<RefCell<u64>>,\n    writable: bool,\n}\n\n#[derive(Debug, Clone)]\nstruct OpenFile {", "    link_count: Rc<RefCell<u64>>,\n    times: Rc<RefCell<FileTimes>>,\n    writable: bool,\n}\n\n#[derive(Debug, Clone)]\nstruct OpenFile {", "mounted file timestamp field")
fs = replace_once(fs, "    link_count: Rc<RefCell<u64>>,\n    offset: u64,\n    rights_base: u64,\n}\n\n#[derive(Debug, Clone)]\nstruct MountedDirectory {", "    link_count: Rc<RefCell<u64>>,\n    times: Rc<RefCell<FileTimes>>,\n    offset: u64,\n    rights_base: u64,\n}\n\n#[derive(Debug, Clone)]\nstruct MountedDirectory {", "open file timestamp field")
fs = replace_once(fs, "struct MountedDirectory {\n    preopen_fd: u32,\n    relative_path: Vec<u8>,\n    inode: u64,\n}\n", "struct MountedDirectory {\n    preopen_fd: u32,\n    relative_path: Vec<u8>,\n    inode: u64,\n    times: FileTimes,\n}\n", "directory timestamp field")
fs = replace_once(fs, "struct MountedSymlink {\n    preopen_fd: u32,\n    relative_path: Vec<u8>,\n    inode: u64,\n    target: Vec<u8>,\n}\n", "struct MountedSymlink {\n    preopen_fd: u32,\n    relative_path: Vec<u8>,\n    inode: u64,\n    target: Vec<u8>,\n    times: FileTimes,\n}\n", "symlink timestamp field")
fs = replace_once(
    fs,
    "        let link_count = Rc::new(RefCell::new(1));\n        state.mounted_files.push(MountedFile {\n",
    "        let link_count = Rc::new(RefCell::new(1));\n        let times = Rc::new(RefCell::new(FileTimes::default()));\n        state.mounted_files.push(MountedFile {\n",
    "mounted file times allocation",
)
fs = replace_once(fs, "            link_count,\n            writable,\n        });\n", "            link_count,\n            times,\n            writable,\n        });\n", "mounted file times assignment")
fs = replace_once(
    fs,
    "        let size = file.bytes.borrow().len() as u64;\n        let nlink = *file.link_count.borrow();\n        Ok(DescriptorFilestat {\n",
    "        let size = file.bytes.borrow().len() as u64;\n        let nlink = *file.link_count.borrow();\n        let times = *file.times.borrow();\n        Ok(DescriptorFilestat {\n",
    "fd filestat times read",
)
fs = replace_once(fs, "            atim: LOGICAL_EPOCH_NS,\n            mtim: LOGICAL_EPOCH_NS,\n            ctim: LOGICAL_EPOCH_NS,\n        })\n    }\n\n    pub(crate) fn set_size", "            atim: times.atim,\n            mtim: times.mtim,\n            ctim: times.ctim,\n        })\n    }\n\n    pub(crate) fn set_size", "fd filestat encoded times")
fs = replace_once(
    fs,
    "    fn path_filestat(\n",
    "    fn set_times(&self, fd: u32, update: TimeUpdate) -> Result<(), DescriptorFilestatError> {\n"
    "        let state = self.state.borrow();\n"
    "        let Some(file) = state.open_files.get(&fd) else {\n"
    "            return Err(DescriptorFilestatError::BadFd);\n"
    "        };\n"
    "        if file.rights_base & RIGHTS_FD_FILESTAT_SET_TIMES == 0 {\n"
    "            return Err(DescriptorFilestatError::NotCapable);\n"
    "        }\n"
    "        let mut times = file.times.borrow_mut();\n"
    "        apply_time_update(&mut times, update);\n"
    "        Ok(())\n"
    "    }\n\n"
    "    fn path_set_times(\n"
    "        &self,\n"
    "        dir_fd: u32,\n"
    "        path: &[u8],\n"
    "        update: TimeUpdate,\n"
    "    ) -> Result<(), PathFilestatError> {\n"
    "        let (preopen_fd, full_path, _) = self\n"
    "            .resolve_directory_path(dir_fd, path, RIGHTS_PATH_FILESTAT_SET_TIMES)\n"
    "            .map_err(|error| match error {\n"
    "                ResolveDirectoryError::BadFd => PathFilestatError::BadFd,\n"
    "                ResolveDirectoryError::NotCapable => PathFilestatError::NotCapable,\n"
    "                ResolveDirectoryError::NameTooLong => PathFilestatError::NameTooLong,\n"
    "            })?;\n"
    "        let mut state = self.state.borrow_mut();\n"
    "        if let Some(file) = state.mounted_files.iter_mut().find(|file| {\n"
    "            file.preopen_fd == preopen_fd && file.relative_path.as_slice() == full_path.as_slice()\n"
    "        }) {\n"
    "            if !file.writable {\n"
    "                return Err(PathFilestatError::NotCapable);\n"
    "            }\n"
    "            let mut times = file.times.borrow_mut();\n"
    "            apply_time_update(&mut times, update);\n"
    "            return Ok(());\n"
    "        }\n"
    "        if let Some(directory) = state.mounted_directories.iter_mut().find(|directory| {\n"
    "            directory.preopen_fd == preopen_fd && directory.relative_path.as_slice() == full_path.as_slice()\n"
    "        }) {\n"
    "            apply_time_update(&mut directory.times, update);\n"
    "            return Ok(());\n"
    "        }\n"
    "        if let Some(symlink) = state.mounted_symlinks.iter_mut().find(|symlink| {\n"
    "            symlink.preopen_fd == preopen_fd && symlink.relative_path.as_slice() == full_path.as_slice()\n"
    "        }) {\n"
    "            apply_time_update(&mut symlink.times, update);\n"
    "            return Ok(());\n"
    "        }\n"
    "        Err(PathFilestatError::NotFound)\n"
    "    }\n\n"
    "    fn path_filestat(\n",
    "timestamp mutation methods",
)
fs = replace_once(
    fs,
    "            let size = file.bytes.borrow().len() as u64;\n            let nlink = *file.link_count.borrow();\n            return Ok(DescriptorFilestat {\n",
    "            let size = file.bytes.borrow().len() as u64;\n            let nlink = *file.link_count.borrow();\n            let times = *file.times.borrow();\n            return Ok(DescriptorFilestat {\n",
    "path file times read",
)
fs = replace_once(fs, "                atim: LOGICAL_EPOCH_NS,\n                mtim: LOGICAL_EPOCH_NS,\n                ctim: LOGICAL_EPOCH_NS,\n            });\n        }\n        if let Some(directory)", "                atim: times.atim,\n                mtim: times.mtim,\n                ctim: times.ctim,\n            });\n        }\n        if let Some(directory)", "path file times encode")
fs = replace_once(fs, "                atim: LOGICAL_EPOCH_NS,\n                mtim: LOGICAL_EPOCH_NS,\n                ctim: LOGICAL_EPOCH_NS,\n            });\n        }\n        if let Some(symlink)", "                atim: directory.times.atim,\n                mtim: directory.times.mtim,\n                ctim: directory.times.ctim,\n            });\n        }\n        if let Some(symlink)", "directory times encode")
fs = replace_once(fs, "                atim: LOGICAL_EPOCH_NS,\n                mtim: LOGICAL_EPOCH_NS,\n                ctim: LOGICAL_EPOCH_NS,\n            });\n        }\n        Err(PathFilestatError::NotFound)", "                atim: symlink.times.atim,\n                mtim: symlink.times.mtim,\n                ctim: symlink.times.ctim,\n            });\n        }\n        Err(PathFilestatError::NotFound)", "symlink times encode")
fs = replace_once(
    fs,
    "    pub(crate) fn register(&self, registry: &mut HostRegistry) -> Result<(), HostRegistryError> {\n",
    "    pub(crate) fn register(\n        &self,\n        registry: &mut HostRegistry,\n        realtime_now: Option<u64>,\n    ) -> Result<(), HostRegistryError> {\n",
    "filesystem register signature",
)
path_set_host = '''
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

'''
fs = replace_once(fs, "        let open_filesystem = self.clone();\n", path_set_host + "        let open_filesystem = self.clone();\n", "path set times hostcall")
fd_set_host = '''
        let set_times_filesystem = self.clone();
        registry.register_values(
            WASI_MODULE,
            FD_FILESTAT_SET_TIMES_NAME,
            vec![ValueType::I32, ValueType::I64, ValueType::I64, ValueType::I32],
            vec![ValueType::I32],
            HostCapabilities::NONE,
            move |_context, args| {
                let [Value::I32(fd), Value::I64(atim), Value::I64(mtim), Value::I32(fst_flags)] = args else {
                    return Err(HostError::message(
                        "validated wasi fd_filestat_set_times signature received invalid arguments",
                    ));
                };
                let update = match resolve_time_update(*atim as u64, *mtim as u64, *fst_flags as u32, realtime_now) {
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
                    Err(DescriptorFilestatError::NotCapable) => Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]),
                }
            },
        )?;

'''
fs = replace_once(fs, "        let close_filesystem = self.clone();\n", fd_set_host + "        let close_filesystem = self.clone();\n", "fd set times hostcall")
fs = replace_once(
    fs,
    "                    | RIGHTS_FD_FILESTAT_GET | RIGHTS_FD_FILESTAT_SET_SIZE;\n",
    "                    | RIGHTS_FD_FILESTAT_GET | RIGHTS_FD_FILESTAT_SET_SIZE\n                    | RIGHTS_FD_FILESTAT_SET_TIMES;\n",
    "path open file rights",
)
fs = replace_once(
    fs,
    "                        | RIGHTS_PATH_FILESTAT_GET\n                        | RIGHTS_PATH_CREATE_DIRECTORY\n",
    "                        | RIGHTS_PATH_FILESTAT_GET\n                        | RIGHTS_PATH_FILESTAT_SET_TIMES\n                        | RIGHTS_PATH_CREATE_DIRECTORY\n",
    "path open directory rights",
)
fs = replace_once(
    fs,
    "                    file.link_count.clone(),\n                    file.writable,\n",
    "                    file.link_count.clone(),\n                    file.times.clone(),\n                    file.writable,\n",
    "open existing times tuple",
)
fs = replace_once(
    fs,
    "        let (inode, bytes, link_count, writable, created) = if let Some(existing) = existing {\n            (existing.0, existing.1, existing.2, existing.3, false)\n",
    "        let (inode, bytes, link_count, times, writable, created) = if let Some(existing) = existing {\n            (existing.0, existing.1, existing.2, existing.3, existing.4, false)\n",
    "open tuple shape",
)
fs = replace_once(
    fs,
    "            let link_count = Rc::new(RefCell::new(1));\n            state.mounted_files.push(MountedFile {\n",
    "            let link_count = Rc::new(RefCell::new(1));\n            let times = Rc::new(RefCell::new(FileTimes::default()));\n            state.mounted_files.push(MountedFile {\n",
    "created file times allocation",
)
fs = replace_once(
    fs,
    "                link_count: link_count.clone(),\n                writable: true,\n            });\n            (inode, bytes, link_count, true, true)\n",
    "                link_count: link_count.clone(),\n                times: times.clone(),\n                writable: true,\n            });\n            (inode, bytes, link_count, times, true, true)\n",
    "created file timestamp assignment",
)
fs = replace_once(fs, "        let mutation_rights = RIGHTS_FD_WRITE | RIGHTS_FD_FILESTAT_SET_SIZE;\n", "        let mutation_rights =\n            RIGHTS_FD_WRITE | RIGHTS_FD_FILESTAT_SET_SIZE | RIGHTS_FD_FILESTAT_SET_TIMES;\n", "file mutation rights")
fs = replace_once(fs, "                link_count,\n                offset: 0,\n", "                link_count,\n                times,\n                offset: 0,\n", "open file times assignment")
fs = replace_once(fs, "            relative_path: full_path,\n            inode,\n        });\n        Ok(())\n    }\n\n    fn remove_directory", "            relative_path: full_path,\n            inode,\n            times: FileTimes::default(),\n        });\n        Ok(())\n    }\n\n    fn remove_directory", "directory times constructor")
fs = replace_once(fs, "            inode,\n            target: target.to_vec(),\n        });\n", "            inode,\n            target: target.to_vec(),\n            times: FileTimes::default(),\n        });\n", "symlink times constructor")
fs = replace_once(
    fs,
    "        let Some((inode, bytes, link_count, writable)) = state\n",
    "        let Some((inode, bytes, link_count, times, writable)) = state\n",
    "link tuple pattern",
)
fs = replace_once(
    fs,
    "                    file.link_count.clone(),\n                    file.writable,\n                )\n",
    "                    file.link_count.clone(),\n                    file.times.clone(),\n                    file.writable,\n                )\n",
    "link times clone",
)
fs = replace_once(fs, "            link_count: link_count.clone(),\n            writable,\n        });\n", "            link_count: link_count.clone(),\n            times,\n            writable,\n        });\n", "link mounted timestamp assignment")
helper = '''
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

'''
fs = replace_once(fs, "fn validate_configured_path(path: &[u8]) -> Result<(), WasiFilesystemError> {\n", helper + "fn validate_configured_path(path: &[u8]) -> Result<(), WasiFilesystemError> {\n", "timestamp helpers")
fs_path.write_text(fs)
