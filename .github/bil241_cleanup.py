from pathlib import Path

path = Path('crates/wasm-runtime/src/lib.rs')
s = path.read_text()
for old in (
    '    table: Option<TableHandle>,\n',
    '        let table = tables.first().cloned();\n',
    '            table,\n            tables,\n',
):
    if s.count(old) != 1:
        raise SystemExit(f'expected exactly one legacy table alias anchor: {old!r}; found {s.count(old)}')
s = s.replace('    table: Option<TableHandle>,\n', '', 1)
s = s.replace('        let table = tables.first().cloned();\n', '', 1)
s = s.replace('            table,\n            tables,\n', '            tables,\n', 1)
path.write_text(s)
