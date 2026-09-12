from pathlib import Path
import re


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text()
    if old not in text:
        raise SystemExit(f"missing marker in {path}: {old[:80]!r}")
    if text.count(old) != 1:
        raise SystemExit(f"non-unique marker in {path}: {old[:80]!r}")
    path.write_text(text.replace(old, new, 1))


runtime = Path("crates/wasm-runtime/src/lib.rs")
marker = "        256 => {\n"
relaxed_trunc_arm = '''        257 => {
            // i32x4.relaxed_trunc_f32x4_s allows the saturating result for every
            // non-deterministic lane. Rust's f32-to-i32 cast is truncating and
            // saturating (NaN -> 0), so it is a valid deterministic lowering.
            let value = numeric::v128_from_stack(stack)?;
            let mut result = [0u8; 16];
            for lane in 0..4 {
                let start = lane * 4;
                let input = f32::from_bits(u32::from_le_bytes(
                    value[start..start + 4]
                        .try_into()
                        .expect("f32x4 lane width"),
                ));
                result[start..start + 4].copy_from_slice(&(input as i32).to_le_bytes());
            }
            stack.push(Value::V128(Rc::new(result)));
        }
'''
replace_once(runtime, marker, relaxed_trunc_arm + marker)
replace_once(runtime, "                    | 240..=256\n", "                    | 240..=257\n")

validator = Path("crates/wasm-validator/src/typed.rs")
validator_marker = '''                    14 | 256 => {
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        stack.push(ValueType::V128);
                    }'''
validator_replacement = '''                    257 => {
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        stack.push(ValueType::V128);
                    }
                    14 | 256 => {
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        stack.push(ValueType::V128);
                    }'''
replace_once(validator, validator_marker, validator_replacement)

# Advance legacy fail-closed frontier assertions/call sites from 257 to 258.
for path in Path("crates/wasm-runtime/tests").glob("simd_*.rs"):
    text = path.read_text()
    updated = text.replace("subopcode: 257", "subopcode: 258")
    updated = re.sub(r"(simd\([^;\n]*,\s*)257(\s*\);)", r"\g<1>258\2", updated)
    updated = re.sub(r"(push_simd\([^;\n]*,\s*)257(\s*\);)", r"\g<1>258\2", updated)
    updated = re.sub(r"(push_u32\([^;\n]*,\s*)257(\s*\);)", r"\g<1>258\2", updated)
    updated = re.sub(r"(push_leb_u32\([^;\n]*,\s*)257(\s*\);)", r"\g<1>258\2", updated)
    if updated != text:
        path.write_text(updated)

runtime_test = Path("crates/wasm-runtime/tests/simd_relaxed_trunc_f32x4_s.rs")
runtime_test.write_text(r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};
use wasm_validator::ValidationError;

fn push_u32(bytes: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 { byte |= 0x80; }
        bytes.push(byte);
        if value == 0 { break; }
    }
}

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    push_u32(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn module(instructions: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut bytes, 1, &[0x01, 0x60, 0x00, 0x01, 0x7f]);
    push_section(&mut bytes, 3, &[0x01, 0x00]);
    push_section(&mut bytes, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);
    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend(body);
    push_section(&mut bytes, 10, &code);
    bytes
}

fn push_f32x4_const(code: &mut Vec<u8>, lanes: [f32; 4]) {
    code.extend_from_slice(&[0xfd, 0x0c]);
    for lane in lanes { code.extend_from_slice(&lane.to_bits().to_le_bytes()); }
}

fn simd(code: &mut Vec<u8>, subopcode: u32) {
    code.push(0xfd);
    push_u32(code, subopcode);
}

fn run_lane(lanes: [f32; 4], lane: u8) -> i32 {
    let mut code = Vec::new();
    push_f32x4_const(&mut code, lanes);
    simd(&mut code, 257);
    code.extend_from_slice(&[0xfd, 0x1b, lane]);
    let parsed = parse_module(&module(&code)).expect("relaxed trunc fixture must parse");
    let mut instance = Instance::new(parsed).expect("relaxed trunc fixture must validate");
    match instance.invoke_export_values("run", &[]).expect("relaxed trunc must execute").as_slice() {
        [Value::I32(value)] => *value,
        other => panic!("unexpected relaxed trunc result: {other:?}"),
    }
}

#[test]
fn relaxed_trunc_executes_deterministic_in_range_lanes() {
    let lanes = [1.75, -2.75, 0.0, 12345.5];
    assert_eq!(run_lane(lanes, 0), 1);
    assert_eq!(run_lane(lanes, 1), -2);
    assert_eq!(run_lane(lanes, 2), 0);
    assert_eq!(run_lane(lanes, 3), 12345);
}

#[test]
fn relaxed_trunc_uses_permitted_saturating_choices_for_special_lanes() {
    let lanes = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 42.9];
    assert_eq!(run_lane(lanes, 0), 0);
    assert_eq!(run_lane(lanes, 1), i32::MAX);
    assert_eq!(run_lane(lanes, 2), i32::MIN);
    assert_eq!(run_lane(lanes, 3), 42);
}

#[test]
fn validator_rejects_relaxed_trunc_type_confusion() {
    let mut code = vec![0x41, 0x00];
    simd(&mut code, 257);
    code.extend_from_slice(&[0xfd, 0x1b, 0x00]);
    let parsed = parse_module(&module(&code)).expect("type-confusion fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(ValidationError::TypeMismatch { .. }))
    ));
}

#[test]
fn next_relaxed_simd_subopcode_remains_fail_closed() {
    let mut code = Vec::new();
    push_f32x4_const(&mut code, [1.0; 4]);
    simd(&mut code, 258);
    code.extend_from_slice(&[0xfd, 0x1b, 0x00]);
    let parsed = parse_module(&module(&code)).expect("258 frontier fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(ValidationError::UnsupportedPrefixedOpcode {
            prefix: 0xfd,
            subopcode: 258,
            ..
        }))
    ));
}
''')

diff_test = Path("differential/tests/simd_relaxed_trunc_f32x4_s.rs")
diff_test.write_text(r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "a") (result i32)
    v128.const f32x4 1.75 -2.75 0 12345.5
    i32x4.relaxed_trunc_f32x4_s
    i32x4.extract_lane 0)
  (func (export "b") (result i32)
    v128.const f32x4 1.75 -2.75 0 12345.5
    i32x4.relaxed_trunc_f32x4_s
    i32x4.extract_lane 1)
  (func (export "c") (result i32)
    v128.const f32x4 1.75 -2.75 0 12345.5
    i32x4.relaxed_trunc_f32x4_s
    i32x4.extract_lane 3))
"#;
const EXPORTS: [&str; 3] = ["a", "b", "c"];

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini runtime must parse relaxed trunc fixture");
    let mut instance = MiniInstance::new(module).expect("mini runtime must instantiate relaxed trunc fixture");
    EXPORTS.into_iter().map(|export| match instance.invoke_export_values(export, &[]).expect("mini relaxed trunc execution must succeed").as_slice() {
        [Value::I32(value)] => *value,
        other => panic!("unexpected mini result for {export}: {other:?}"),
    }).collect()
}

fn reference_trace(bytes: &[u8]) -> Vec<i32> {
    let mut config = Config::new();
    config.wasm_simd(true);
    config.wasm_relaxed_simd(true);
    let engine = Engine::new(&config).expect("relaxed-SIMD Wasmtime engine must initialize");
    let module = ReferenceModule::new(&engine, bytes).expect("Wasmtime must compile relaxed trunc fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[]).expect("Wasmtime must instantiate relaxed trunc fixture");
    EXPORTS.into_iter().map(|export| instance.get_typed_func::<(), i32>(&mut store, export).expect("relaxed trunc export must be [] -> [i32]").call(&mut store, ()).expect("Wasmtime relaxed trunc execution must succeed")).collect()
}

#[test]
fn relaxed_trunc_matches_wasmtime_on_deterministic_lanes() {
    let bytes = wat::parse_str(FIXTURE).expect("relaxed trunc WAT fixture must parse");
    let expected = vec![1, -2, 12345];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, expected);
    assert_eq!(reference, expected);
    assert_eq!(mini, reference);
}
''')

roadmap = Path("docs/roadmap.md")
lines = roadmap.read_text().splitlines()
needle = "Relaxed SIMD `i8x16.relaxed_swizzle`"
indices = [i for i, line in enumerate(lines) if needle in line]
if len(indices) != 1:
    raise SystemExit(f"expected one relaxed swizzle roadmap marker, found {len(indices)}")
new_line = "- Relaxed SIMD `i32x4.relaxed_trunc_f32x4_s` (subopcode 257 / 0x101) is executable with a deterministic saturating lowering permitted by relaxed semantics, unary typed validation, in-range and special-value regressions, Wasmtime differential evidence on deterministic lanes, and fail-closed frontier advancement to subopcode 258."
if new_line not in lines:
    lines.insert(indices[0] + 1, new_line)
roadmap.write_text("\n".join(lines) + "\n")
