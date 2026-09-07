from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected one {label} anchor, found {count}")
    return text.replace(old, new, 1)


fs_path = Path("crates/wasm-wasi/src/filesystem.rs")
fs = fs_path.read_text()
if "FD_FILESTAT_SET_SIZE_NAME" not in fs:
    fs = replace_once(
        fs,
        "    OFLAGS_CREAT, RIGHTS_FD_FILESTAT_GET, RIGHTS_FD_READ, RIGHTS_FD_SEEK, RIGHTS_FD_TELL,\n    RIGHTS_FD_WRITE,\n",
        "    OFLAGS_CREAT, RIGHTS_FD_FILESTAT_GET, RIGHTS_FD_FILESTAT_SET_SIZE, RIGHTS_FD_READ,\n    RIGHTS_FD_SEEK, RIGHTS_FD_TELL, RIGHTS_FD_WRITE,\n",
        "filesystem rights import",
    )
    fs = replace_once(
        fs,
        'const FD_PWRITE_NAME: &str = "fd_pwrite";\n',
        'const FD_PWRITE_NAME: &str = "fd_pwrite";\nconst FD_FILESTAT_SET_SIZE_NAME: &str = "fd_filestat_set_size";\n',
        "set-size hostcall constant",
    )
    fs = replace_once(
        fs,
        "    pub(crate) fn register(&self, registry: &mut HostRegistry) -> Result<(), HostRegistryError> {\n",
        "    pub(crate) fn set_size(\n        &self,\n        fd: u32,\n        size: u64,\n    ) -> Result<(), DescriptorWriteError> {\n        let state = self.state.borrow();\n        let Some(file) = state.open_files.get(&fd) else {\n            return Err(DescriptorWriteError::BadFd);\n        };\n        if file.rights_base & RIGHTS_FD_FILESTAT_SET_SIZE == 0 {\n            return Err(DescriptorWriteError::NotCapable);\n        }\n        let size = usize::try_from(size).map_err(|_| DescriptorWriteError::FileTooLarge)?;\n        if size > MAX_FILE_BYTES {\n            return Err(DescriptorWriteError::FileTooLarge);\n        }\n        file.bytes.borrow_mut().resize(size, 0);\n        Ok(())\n    }\n\n    pub(crate) fn register(&self, registry: &mut HostRegistry) -> Result<(), HostRegistryError> {\n",
        "set-size state mutation",
    )
    fs = replace_once(
        fs,
        "                let allowed_base = RIGHTS_FD_READ\n                    | RIGHTS_FD_WRITE\n                    | RIGHTS_FD_SEEK\n                    | RIGHTS_FD_TELL\n                    | RIGHTS_FD_FILESTAT_GET;\n",
        "                let allowed_base = RIGHTS_FD_READ\n                    | RIGHTS_FD_WRITE\n                    | RIGHTS_FD_SEEK\n                    | RIGHTS_FD_TELL\n                    | RIGHTS_FD_FILESTAT_GET\n                    | RIGHTS_FD_FILESTAT_SET_SIZE;\n",
        "path_open allowed rights",
    )
    fs = replace_once(
        fs,
        "        let close_filesystem = self.clone();\n",
        "        let resize_filesystem = self.clone();\n        registry.register_values(\n            WASI_MODULE,\n            FD_FILESTAT_SET_SIZE_NAME,\n            vec![ValueType::I32, ValueType::I64],\n            vec![ValueType::I32],\n            HostCapabilities::NONE,\n            move |_context, args| {\n                let [Value::I32(fd), Value::I64(size)] = args else {\n                    return Err(HostError::message(\n                        \"validated wasi fd_filestat_set_size signature received invalid arguments\",\n                    ));\n                };\n\n                let fd = *fd as u32;\n                if resize_filesystem.is_known_non_file(fd) {\n                    return Ok(vec![Value::I32(ERRNO_NOTCAPABLE)]);\n                }\n                match resize_filesystem.set_size(fd, *size as u64) {\n                    Ok(()) => Ok(vec![Value::I32(ERRNO_SUCCESS)]),\n                    Err(error) => Ok(vec![Value::I32(write_errno(error))]),\n                }\n            },\n        )?;\n\n        let close_filesystem = self.clone();\n",
        "set-size host registration",
    )
    fs = replace_once(
        fs,
        "        if rights_base & RIGHTS_FD_WRITE != 0\n            && (!writable || !state.writable_preopens.contains(&preopen_fd))\n        {\n",
        "        let mutation_rights = RIGHTS_FD_WRITE | RIGHTS_FD_FILESTAT_SET_SIZE;\n        if rights_base & mutation_rights != 0\n            && (!writable || !state.writable_preopens.contains(&preopen_fd))\n        {\n",
        "mutation rights policy",
    )
    fs_path.write_text(fs)

lib_path = Path("crates/wasm-wasi/src/lib.rs")
lib = lib_path.read_text()
if "pub const RIGHTS_FD_FILESTAT_SET_SIZE" not in lib:
    lib = replace_once(
        lib,
        "pub const RIGHTS_FD_FILESTAT_GET: u64 = 1 << 21;\n",
        "pub const RIGHTS_FD_FILESTAT_GET: u64 = 1 << 21;\npub const RIGHTS_FD_FILESTAT_SET_SIZE: u64 = 1 << 22;\n",
        "public set-size right",
    )
    lib_path.write_text(lib)

root_path = Path("crates/wasm-wasi/src/root.rs")
root = root_path.read_text()
if "RIGHTS_FD_FILESTAT_SET_SIZE" not in root:
    root = replace_once(
        root,
        "    FILETYPE_CHARACTER_DEVICE, FILETYPE_DIRECTORY, FILETYPE_REGULAR_FILE, RIGHTS_FD_FILESTAT_GET,\n    RIGHTS_FD_READ, RIGHTS_FD_WRITE, RIGHTS_PATH_OPEN,\n",
        "    FILETYPE_CHARACTER_DEVICE, FILETYPE_DIRECTORY, FILETYPE_REGULAR_FILE, RIGHTS_FD_FILESTAT_GET,\n    RIGHTS_FD_FILESTAT_SET_SIZE, RIGHTS_FD_READ, RIGHTS_FD_WRITE, RIGHTS_PATH_OPEN,\n",
        "root set-size re-export",
    )
    root = replace_once(
        root,
        "                | RIGHTS_FD_TELL\n                | RIGHTS_FD_FILESTAT_GET\n",
        "                | RIGHTS_FD_TELL\n                | RIGHTS_FD_FILESTAT_GET\n                | RIGHTS_FD_FILESTAT_SET_SIZE\n",
        "writable preopen inheriting rights",
    )
    root_path.write_text(root)

fdstat_path = Path("crates/wasm-wasi/tests/fd_fdstat_get.rs")
fdstat = fdstat_path.read_text()
if "writable_preopen_resize_right" not in fdstat:
    fdstat = replace_once(
        fdstat,
        "    FILETYPE_DIRECTORY, RIGHTS_FD_FILESTAT_GET, RIGHTS_FD_READ, RIGHTS_FD_SEEK, RIGHTS_FD_TELL,\n    RIGHTS_FD_WRITE, RIGHTS_PATH_OPEN,\n",
        "    FILETYPE_DIRECTORY, RIGHTS_FD_FILESTAT_GET, RIGHTS_FD_FILESTAT_SET_SIZE, RIGHTS_FD_READ,\n    RIGHTS_FD_SEEK, RIGHTS_FD_TELL, RIGHTS_FD_WRITE, RIGHTS_PATH_CREATE_FILE, RIGHTS_PATH_OPEN,\n",
        "fdstat test imports",
    )
    anchor = "\n#[test]\nfn fd_fdstat_get_bad_fd_does_not_mutate_guest_memory() {\n"
    test = "\n#[test]\nfn fd_fdstat_get_reports_writable_preopen_resize_right() {\n    let memory = MemoryHandle::new(1, Some(1)).unwrap();\n    let wasi = WasiPreview1::new().with_writable_preopen(\"/sandbox\").unwrap();\n    let mut vm = instantiate(3, 128, &memory, &wasi);\n\n    assert_eq!(\n        vm.invoke_export(\"run\", &[]).unwrap(),\n        Some(Value::I32(ERRNO_SUCCESS))\n    );\n    let fdstat = memory.read(128, 24).unwrap();\n    assert_eq!(fdstat[0], FILETYPE_DIRECTORY);\n    assert_eq!(\n        u64::from_le_bytes(fdstat[8..16].try_into().unwrap()),\n        RIGHTS_PATH_OPEN | RIGHTS_PATH_CREATE_FILE\n    );\n    assert_eq!(\n        u64::from_le_bytes(fdstat[16..24].try_into().unwrap()),\n        RIGHTS_FD_READ\n            | RIGHTS_FD_WRITE\n            | RIGHTS_FD_SEEK\n            | RIGHTS_FD_TELL\n            | RIGHTS_FD_FILESTAT_GET\n            | RIGHTS_FD_FILESTAT_SET_SIZE\n    );\n}\n"
    fdstat = replace_once(fdstat, anchor, test + anchor, "writable preopen fdstat test")
    fdstat_path.write_text(fdstat)

roadmap_path = Path("docs/roadmap.md")
roadmap = roadmap_path.read_text()
if "fd_filestat_set_size" not in roadmap:
    anchor = "- [x] stable regular-file metadata identity via `fd_filestat_get` with `FD_FILESTAT_GET` attenuation, deterministic synthetic device/inode identity, live shared size, fixed logical-epoch timestamps, and 64-byte fail-closed guest-memory preflight\n"
    roadmap = replace_once(
        roadmap,
        anchor,
        anchor + "- [x] bounded regular-file resizing via `fd_filestat_set_size` with `FD_FILESTAT_SET_SIZE` attenuation, shrink/zero-fill extension semantics, cursor preservation, writable-file policy enforcement, and the existing fixed 16 MiB file-size ceiling\n",
        "roadmap resize milestone",
    )
    roadmap_path.write_text(roadmap)
