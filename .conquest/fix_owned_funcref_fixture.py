from pathlib import Path

path = Path("crates/wasm-runtime/tests/owned_funcref_embedding.rs")
text = path.read_text()
old = '''    section(&mut module, 7, &exports);

    let target = [0, 0x0b];
'''
new = '''    section(&mut module, 7, &exports);
    // A declarative legacy element segment declares function 0 as a valid ref.func target.
    section(&mut module, 9, &[1, 3, 0, 1, 0]);

    let target = [0, 0x0b];
'''
if text.count(old) != 1:
    raise SystemExit(f"fixture declaration anchor: expected one, found {text.count(old)}")
path.write_text(text.replace(old, new, 1))
