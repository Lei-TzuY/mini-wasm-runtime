from pathlib import Path

source_path = Path("scripts/phase5c_multi_value_gate.py")
source = source_path.read_text()

start_marker = "# Three structured-control frame constructors.\n"
next_marker = '''replace_once(
    "crates/wasm-runtime/src/lib.rs",
    """                    if let Some(result) =
'''
start = source.find(start_marker)
end = source.find(next_marker, start)
if start < 0 or end < 0 or end <= start:
    raise SystemExit("could not isolate repeated structured-frame staging block")

old = """                        param_types: signature.params,\n                        result_type: signature.result,\n"""
new = """                        param_types: signature.params,\n                        result_types: signature.results,\n"""
replacement = f'''# Two structured-control frame constructors share one exact source anchor.
_runtime_path = Path("crates/wasm-runtime/src/lib.rs")
_runtime_text = _runtime_path.read_text()
_runtime_old = {old!r}
_runtime_new = {new!r}
_runtime_count = _runtime_text.count(_runtime_old)
if _runtime_count != 2:
    raise SystemExit(f"expected exactly two structured-frame anchors, found {{_runtime_count}}")
_runtime_path.write_text(_runtime_text.replace(_runtime_old, _runtime_new))

'''

patched = source[:start] + replacement + source[end:]
exec(compile(patched, str(source_path), "exec"), {"__name__": "__main__"})
