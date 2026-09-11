from pathlib import Path
import re

root = Path("crates/wasm-runtime/tests")
changed = 0
for path in root.glob("simd_*.rs"):
    if path.name == "simd_conversions.rs":
        continue
    text = path.read_text()
    original = text
    text = re.sub(
        r"((?:push_simd|simd)\(&mut\s+[A-Za-z_][A-Za-z0-9_]*,\s*)248(\s*\);)",
        r"\g<1>256\2",
        text,
    )
    text = text.replace("subopcode: 248", "subopcode: 256")
    text = text.replace("248); // f32x4 frontier remains outside this slice", "256); // relaxed-SIMD frontier remains outside this slice")
    if text != original:
        path.write_text(text)
        changed += 1

if changed == 0:
    raise SystemExit("no stale SIMD frontier tests were advanced")
print(f"advanced fail-closed frontier in {changed} SIMD test files")
