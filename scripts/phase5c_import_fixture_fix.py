from pathlib import Path

path = Path("crates/wasm-runtime/tests/host_boundary.rs")
text = path.read_text()
text = text.replace(
    "DataSegment, Export, ExportKind, FuncType, FunctionBody, Import, Limits, MemoryType, Module,\n    ValueType,",
    "DataSegment, Export, ExportKind, FuncType, FunctionBody, Import, ImportDesc, Limits, MemoryType,\n    Module, ValueType,",
    1,
)
old = '''        imports: vec![Import {
            module: "env".into(),
            name: "read_first".into(),
            type_index: 0,
        }],'''
new = '''        imports: vec![Import {
            module: "env".into(),
            name: "read_first".into(),
            desc: ImportDesc::Function(0),
        }],'''
assert old in text
path.write_text(text.replace(old, new, 1))
