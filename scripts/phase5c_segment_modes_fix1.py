from pathlib import Path

p = Path("crates/wasm-runtime/tests/host_boundary.rs")
text = p.read_text()
old_import = '''use wasm_parser::{
    DataSegment, Export, ExportKind, FuncType, FunctionBody, Import, ImportDesc, Limits,
    MemoryType, Module, ValueType,
};
'''
new_import = '''use wasm_parser::{
    DataMode, DataSegment, Export, ExportKind, FuncType, FunctionBody, Import, ImportDesc, Limits,
    MemoryType, Module, ValueType,
};
'''
if text.count(old_import) != 1:
    raise SystemExit("host-boundary import anchor mismatch")
text = text.replace(old_import, new_import, 1)
old_segment = '''        data: vec![DataSegment {
            memory_index: 0,
            offset: 0,
            bytes: b"wasm".to_vec(),
        }],
'''
new_segment = '''        data: vec![DataSegment {
            mode: DataMode::Active {
                memory_index: 0,
                offset: 0,
            },
            bytes: b"wasm".to_vec(),
        }],
'''
if text.count(old_segment) != 1:
    raise SystemExit("host-boundary data anchor mismatch")
p.write_text(text.replace(old_segment, new_segment, 1))
