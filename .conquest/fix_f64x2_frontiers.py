from pathlib import Path

root = Path("crates/wasm-runtime/tests")
updated = []
for path in sorted(root.glob("simd_*.rs")):
    if path.name == "simd_f64x2_unary.rs":
        continue
    text = path.read_text()
    if "subopcode: 236" not in text:
        continue
    original = text
    text = text.replace("f64x2_abs_frontier", "f64x2_add_frontier")
    text = text.replace("push_simd(&mut instructions, 236);", "push_simd(&mut instructions, 240);")
    text = text.replace("subopcode: 236,", "subopcode: 240,")
    if text == original:
        raise SystemExit(f"stale 236 frontier not rewritten in {path}")
    if "subopcode: 236" in text:
        raise SystemExit(f"residual stale 236 frontier in {path}")
    path.write_text(text)
    updated.append(path.as_posix())

if not updated:
    raise SystemExit("expected at least one stale SIMD 236 frontier")
print("updated stale frontiers:")
for path in updated:
    print(path)
