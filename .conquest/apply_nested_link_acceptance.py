from pathlib import Path

path = Path("crates/wasm-wasi/tests/nested_directories.rs")
text = path.read_text()

replacements = [
    (
        '''    RIGHTS_FD_READDIR, RIGHTS_PATH_CREATE_FILE, RIGHTS_PATH_OPEN, RIGHTS_PATH_UNLINK_FILE,\n''',
        '''    RIGHTS_FD_READDIR, RIGHTS_PATH_CREATE_FILE, RIGHTS_PATH_LINK_SOURCE,\n    RIGHTS_PATH_LINK_TARGET, RIGHTS_PATH_OPEN, RIGHTS_PATH_UNLINK_FILE,\n''',
        "link rights imports",
    ),
    (
        '''    let mut types = vec![3];\n''',
        '''    let mut types = vec![4];\n''',
        "type count",
    ),
    (
        '''    function_type(&mut types, &[0x7f, 0x7f, 0x7f, 0x7f, 0x7f]);\n    section(&mut module, 1, &types);\n''',
        '''    function_type(&mut types, &[0x7f, 0x7f, 0x7f, 0x7f, 0x7f]);\n    function_type(\n        &mut types,\n        &[0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f],\n    );\n    section(&mut module, 1, &types);\n''',
        "path_link function type",
    ),
    (
        '''    let mut imports = vec![6];\n''',
        '''    let mut imports = vec![7];\n''',
        "import count",
    ),
    (
        '''        ("path_unlink_file", 0),\n''',
        '''        ("path_unlink_file", 0),\n        ("path_link", 3),\n''',
        "path_link import",
    ),
    (
        '''    section(&mut module, 3, &[5, 0, 0, 1, 2, 0]);\n\n    let mut exports = vec![5];\n    for (export_name, function_index) in [\n        ("mkdir", 5u8),\n        ("rmdir", 6),\n        ("open", 7),\n        ("readdir", 8),\n        ("unlink", 9),\n    ] {\n''',
        '''    section(&mut module, 3, &[6, 0, 0, 1, 2, 0, 3]);\n\n    let mut exports = vec![6];\n    for (export_name, function_index) in [\n        ("mkdir", 6u8),\n        ("rmdir", 7),\n        ("open", 8),\n        ("readdir", 9),\n        ("unlink", 10),\n        ("link", 11),\n    ] {\n''',
        "defined/export function indices",
    ),
    (
        '''        wrapper_body(5, 3),\n        wrapper_body(3, 4),\n    ];\n''',
        '''        wrapper_body(5, 3),\n        wrapper_body(3, 4),\n        wrapper_body(7, 5),\n    ];\n''',
        "path_link wrapper body",
    ),
    (
        '''fn readdir_args(fd: u32, buf: u32, buf_len: u32, cookie: u64, bufused: u32) -> [Value; 5] {\n''',
        '''fn link_args(\n    old_fd: u32,\n    old_path_ptr: u32,\n    old_path_len: u32,\n    new_fd: u32,\n    new_path_ptr: u32,\n    new_path_len: u32,\n) -> [Value; 7] {\n    [\n        Value::I32(old_fd as i32),\n        Value::I32(0),\n        Value::I32(old_path_ptr as i32),\n        Value::I32(old_path_len as i32),\n        Value::I32(new_fd as i32),\n        Value::I32(new_path_ptr as i32),\n        Value::I32(new_path_len as i32),\n    ]\n}\n\nfn readdir_args(fd: u32, buf: u32, buf_len: u32, cookie: u64, bufused: u32) -> [Value; 5] {\n''',
        "path_link args helper",
    ),
]

for old, new, label in replacements:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label} anchor drifted: expected 1 match, found {count}")
    text = text.replace(old, new, 1)

marker = '''#[test]\nfn directory_mutation_failures_are_atomic_and_capability_scoped() {\n'''
if text.count(marker) != 1:
    raise SystemExit("nested link test insertion anchor drifted")

test = r'''#[test]
fn hard_link_resolves_source_and_target_directory_descriptors() {
    let memory = MemoryHandle::new(1, Some(1)).unwrap();
    memory.write(96, b"left").unwrap();
    memory.write(112, b"right").unwrap();
    memory.write(128, b"source.txt").unwrap();
    memory.write(144, b"alias.txt").unwrap();

    let wasi = WasiPreview1::new()
        .with_writable_preopen("/sandbox")
        .unwrap();
    let mut vm = instantiate(&memory, &wasi);

    assert_eq!(errno(&mut vm, "mkdir", &path_args(3, 96, 4)), ERRNO_SUCCESS);
    assert_eq!(errno(&mut vm, "mkdir", &path_args(3, 112, 5)), ERRNO_SUCCESS);

    let source_rights = RIGHTS_FD_READDIR
        | RIGHTS_PATH_OPEN
        | RIGHTS_PATH_CREATE_FILE
        | RIGHTS_PATH_LINK_SOURCE
        | RIGHTS_PATH_UNLINK_FILE;
    let target_rights = RIGHTS_FD_READDIR | RIGHTS_PATH_OPEN | RIGHTS_PATH_LINK_TARGET;
    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(3, 96, 4, OFLAGS_DIRECTORY, source_rights, 32),
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(3, 112, 5, OFLAGS_DIRECTORY, target_rights, 36),
        ),
        ERRNO_SUCCESS
    );
    let source_dir_fd = u32::from_le_bytes(memory.read(32, 4).unwrap().try_into().unwrap());
    let target_dir_fd = u32::from_le_bytes(memory.read(36, 4).unwrap().try_into().unwrap());

    assert_eq!(
        errno(
            &mut vm,
            "open",
            &open_args(source_dir_fd, 128, 10, OFLAGS_CREAT, 0, 40),
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(
        wasi.file_snapshot("/sandbox", "left/source.txt"),
        Some(Vec::new())
    );

    assert_eq!(
        errno(
            &mut vm,
            "link",
            &link_args(source_dir_fd, 128, 10, source_dir_fd, 144, 9),
        ),
        ERRNO_NOTCAPABLE,
        "a source-only directory descriptor must not be usable as a hard-link target"
    );
    assert_eq!(wasi.file_snapshot("/sandbox", "left/alias.txt"), None);

    assert_eq!(
        errno(
            &mut vm,
            "link",
            &link_args(source_dir_fd, 128, 10, target_dir_fd, 144, 9),
        ),
        ERRNO_SUCCESS
    );
    assert_eq!(
        wasi.file_snapshot("/sandbox", "right/alias.txt"),
        Some(Vec::new())
    );

    assert_eq!(
        errno(&mut vm, "unlink", &path_args(source_dir_fd, 128, 10)),
        ERRNO_SUCCESS
    );
    assert_eq!(wasi.file_snapshot("/sandbox", "left/source.txt"), None);
    assert_eq!(
        wasi.file_snapshot("/sandbox", "right/alias.txt"),
        Some(Vec::new())
    );

    assert_eq!(
        read_dir(&memory, &mut vm, source_dir_fd, 2048, 48)
            .into_iter()
            .map(|entry| entry.name)
            .collect::<Vec<_>>(),
        vec![b".".to_vec(), b"..".to_vec()]
    );
    assert_eq!(
        read_dir(&memory, &mut vm, target_dir_fd, 2560, 52)
            .into_iter()
            .map(|entry| entry.name)
            .collect::<Vec<_>>(),
        vec![b".".to_vec(), b"..".to_vec(), b"alias.txt".to_vec()]
    );
}

'''
text = text.replace(marker, test + marker, 1)
path.write_text(text)
