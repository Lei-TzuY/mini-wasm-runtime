from pathlib import Path

source_path = Path("scripts/phase5c_multi_value_gate.py")
source = source_path.read_text()

# 1) Structured-control constructors: v1 contains a loop plus an extra duplicate
# top-level replacement, while the runtime source has exactly two matching anchors.
start_marker = "# Three structured-control frame constructors.\n"
start = source.find(start_marker)
if start < 0:
    raise SystemExit("could not find repeated structured-frame staging block")
first_top_level = source.find("\nreplace_once(", start + len(start_marker))
if first_top_level < 0:
    raise SystemExit("could not find duplicate top-level structured-frame replacement")
end = source.find("\nreplace_once(", first_top_level + 1)
if end < 0 or end <= first_top_level:
    raise SystemExit("could not isolate complete structured-frame staging block")

frame_old = """                        param_types: signature.params,\n                        result_type: signature.result,\n"""
frame_new = """                        param_types: signature.params,\n                        result_types: signature.results,\n"""
frame_replacement = f'''# Two structured-control frame constructors share one exact source anchor.
_runtime_path = Path("crates/wasm-runtime/src/lib.rs")
_runtime_text = _runtime_path.read_text()
_runtime_old = {frame_old!r}
_runtime_new = {frame_new!r}
_runtime_count = _runtime_text.count(_runtime_old)
if _runtime_count != 2:
    raise SystemExit(f"expected exactly two structured-frame anchors, found {{_runtime_count}}")
_runtime_path.write_text(_runtime_text.replace(_runtime_old, _runtime_new))
'''
patched = source[:start] + frame_replacement + source[end:]

# 2) Direct-call and call_indirect result propagation use the same runtime source
# anchor twice. Replace both together only when the source has exactly two matches;
# all unrelated v1 replacements remain strict replace_once operations.
call_old = """                    if let Some(result) =\n                        self.invoke_function(callee, &call_args, depth + 1, budget)?\n                    {\n                        stack.push(result);\n                    }\n"""
call_new = """                    let results = self.invoke_function(callee, &call_args, depth + 1, budget)?;\n                    stack.extend(results);\n"""
anchor = repr(call_old)[1:-1]
pos = patched.find('"""                    if let Some(result) =\\n')
if pos < 0:
    raise SystemExit("could not find duplicated call-result staging anchors")
call_start = patched.rfind("\nreplace_once(", 0, pos)
if call_start < 0:
    raise SystemExit("could not find first call-result replacement start")
call_start += 1
second_call = patched.find("\nreplace_once(", pos)
if second_call < 0:
    raise SystemExit("could not find second call-result replacement")
call_end = patched.find("\nreplace_once(", second_call + 1)
if call_end < 0:
    raise SystemExit("could not isolate duplicated call-result replacement block")

call_replacement = f'''# Direct and indirect calls share one exact source anchor.
_runtime_path = Path("crates/wasm-runtime/src/lib.rs")
_runtime_text = _runtime_path.read_text()
_call_old = {call_old!r}
_call_new = {call_new!r}
_call_count = _runtime_text.count(_call_old)
if _call_count != 2:
    raise SystemExit(f"expected exactly two call-result anchors, found {{_call_count}}")
_runtime_path.write_text(_runtime_text.replace(_call_old, _call_new))
'''
patched = patched[:call_start] + call_replacement + patched[call_end:]

exec(compile(patched, str(source_path), "exec"), {"__name__": "__main__"})
