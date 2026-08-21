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

# 3) The pre-multi-value Phase 5C regression intentionally rejected type-index
# blocks with two results. Once this slice opens defined-Wasm multi-value execution,
# preserve the coverage by converting it into a positive ordered-result regression.
test_path = Path("crates/wasm-runtime/tests/phase5c.rs")
test_text = test_path.read_text()
old_test = '''#[test]
fn multi_result_block_signature_remains_fail_closed() {
    let module = build_module(
        &[ty(&[], &[]), ty(&[], &[I32, I32])],
        0,
        &[0x02, 0x01, 0x0b],
    );
    let error =
        Instance::new(parse_module(&module).unwrap()).expect_err("multi-result block must fail");
    assert!(matches!(
        error,
        RuntimeError::Validation(ValidationError::UnsupportedBlockResultArity {
            type_index: 1,
            results: 2,
            ..
        })
    ));
}
'''
new_test = '''#[test]
fn multi_result_block_signature_executes_in_order() {
    let module = build_module(
        &[ty(&[], &[I32, I32]), ty(&[], &[I32, I32])],
        0,
        &[0x02, 0x01, 0x41, 0x07, 0x41, 0x09, 0x0b],
    );
    assert_eq!(
        instance(&module).invoke_export_values("run", &[]).unwrap(),
        vec![Value::I32(7), Value::I32(9)]
    );
}
'''
if test_text.count(old_test) != 1:
    raise SystemExit("expected exactly one legacy multi-result block regression")
test_path.write_text(test_text.replace(old_test, new_test, 1))

# 4) The validator's pre-feature unit test rejected every defined function with
# multiple results before checking its body. Replace it with a valid two-result
# body so the unit suite now locks acceptance of defined-Wasm multi-value while
# the integration corpus continues to reject wrong result order and host ABI
# multi-result imports.
validator_path = Path("crates/wasm-validator/src/lib.rs")
validator_text = validator_path.read_text()
old_validator_test = '''    #[test]
    fn rejects_multi_value_results() {
        let mut module = valid_module();
        module.types[0].results.push(ValueType::I32);
        assert_eq!(
            validate(&module),
            Err(ValidationError::UnsupportedResultArity {
                function: 0,
                results: 2,
            })
        );
    }
'''
new_validator_test = '''    #[test]
    fn accepts_defined_multi_value_results() {
        let mut module = valid_module();
        module.types[0].results.push(ValueType::I32);
        module.code[0].code = vec![0x20, 0x00, 0x20, 0x00, 0x0b];
        assert_eq!(validate(&module), Ok(()));
    }
'''
if validator_text.count(old_validator_test) != 1:
    raise SystemExit("expected exactly one legacy defined multi-value validator regression")
validator_path.write_text(validator_text.replace(old_validator_test, new_validator_test, 1))
