from pathlib import Path

p = Path("crates/wasm-runtime/tests/phase5c_imported_memory.rs")
text = p.read_text()
old_import = '''use wasm_parser::{
    parse_module, DataSegment, Export, ExportKind, FuncType, Import, ImportDesc, Limits,
    MemoryType, Module, ValueType,
};
'''
new_import = '''use wasm_parser::{
    parse_module, DataMode, DataSegment, Export, ExportKind, FuncType, Import, ImportDesc, Limits,
    MemoryType, Module, ValueType,
};
'''
if text.count(old_import) != 1:
    raise SystemExit("imported-memory import anchor mismatch")
text = text.replace(old_import, new_import, 1)
old_segments = '''    module.data = vec![
        DataSegment {
            memory_index: 0,
            offset: 32,
            bytes: b"MUTATE".to_vec(),
        },
        DataSegment {
            memory_index: 0,
            offset: 131_071,
            bytes: vec![0xaa, 0xbb],
        },
    ];
'''
new_segments = '''    module.data = vec![
        DataSegment {
            mode: DataMode::Active {
                memory_index: 0,
                offset: 32,
            },
            bytes: b"MUTATE".to_vec(),
        },
        DataSegment {
            mode: DataMode::Active {
                memory_index: 0,
                offset: 131_071,
            },
            bytes: vec![0xaa, 0xbb],
        },
    ];
'''
if text.count(old_segments) != 1:
    raise SystemExit("imported-memory data fixture anchor mismatch")
p.write_text(text.replace(old_segments, new_segments, 1))
