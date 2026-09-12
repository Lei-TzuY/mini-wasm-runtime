from pathlib import Path

runtime = Path('crates/wasm-runtime/src/lib.rs')
s = runtime.read_text()
old = '''        259 => {
            // f32x4.relaxed_madd permits either fused or unfused evaluation.
            // Use ordinary multiply followed by add for a portable deterministic lowering.
            let c = numeric::v128_from_stack(stack)?;
            let b = numeric::v128_from_stack(stack)?;
            let a = numeric::v128_from_stack(stack)?;
            let mut result = [0u8; 16];
            for (lane, output) in result.chunks_exact_mut(4).enumerate() {
                let start = lane * 4;
                let lhs =
                    f32::from_le_bytes(a[start..start + 4].try_into().expect("f32x4 lane width"));
                let rhs =
                    f32::from_le_bytes(b[start..start + 4].try_into().expect("f32x4 lane width"));
                let addend =
                    f32::from_le_bytes(c[start..start + 4].try_into().expect("f32x4 lane width"));
                output.copy_from_slice(&(lhs * rhs + addend).to_le_bytes());
            }
            stack.push(Value::V128(Rc::new(result)));
        }
'''
new = '''        259 => {
            // i32x4.relaxed_trunc_f64x2_s_zero permits a saturating signed result
            // for non-deterministic lanes. Rust's f64-to-i32 cast truncates and
            // saturates (NaN -> 0), providing a valid deterministic lowering.
            // The upper two i32 lanes are required to be zero.
            let value = numeric::v128_from_stack(stack)?;
            let mut result = [0u8; 16];
            for lane in 0..2 {
                let start = lane * 8;
                let input = f64::from_bits(u64::from_le_bytes(
                    value[start..start + 8]
                        .try_into()
                        .expect("f64x2 lane width"),
                ));
                let output_start = lane * 4;
                result[output_start..output_start + 4]
                    .copy_from_slice(&(input as i32).to_le_bytes());
            }
            stack.push(Value::V128(Rc::new(result)));
        }
'''
if old not in s:
    raise SystemExit('runtime 259 block not found')
runtime.write_text(s.replace(old, new, 1))

validator = Path('crates/wasm-validator/src/typed.rs')
s = validator.read_text()
old = '''                    259 => {
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        stack.push(ValueType::V128);
                    }
'''
new = '''                    259 => {
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        stack.push(ValueType::V128);
                    }
'''
if old not in s:
    raise SystemExit('validator 259 block not found')
validator.write_text(s.replace(old, new, 1))

binary = Path('crates/wasm-runtime/tests/simd_f32x4_binary.rs')
s = binary.read_text()
start = s.index('fn relaxed_madd_lane_bits(')
end = s.index('#[test]\nfn adjacent_f32x4_f64x2_min_frontier_remains_fail_closed()', start)
s = s[:start] + s[end:]
s = s.replace('push_simd(&mut instructions, 259);\n    let parsed = parse_module(&module(&instructions)).expect("fixture parses");', 'push_simd(&mut instructions, 260);\n    let parsed = parse_module(&module(&instructions)).expect("fixture parses");', 1)
s = s.replace('subopcode: 259,', 'subopcode: 260,', 1)
binary.write_text(s)

runtime_test = Path('crates/wasm-runtime/tests/simd_relaxed_trunc_f64x2_s_zero.rs')
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

fn push_f64x2_const(code: &mut Vec<u8>, lanes: [f64; 2]) {
    code.extend_from_slice(&[0xfd, 0x0c]);
    for lane in lanes { code.extend_from_slice(&lane.to_bits().to_le_bytes()); }
}

fn simd(code: &mut Vec<u8>, subopcode: u32) {
    code.push(0xfd);
    push_u32(code, subopcode);
}

fn run_lane(lanes: [f64; 2], lane: u8) -> i32 {
    let mut code = Vec::new();
    push_f64x2_const(&mut code, lanes);
    simd(&mut code, 259);
    code.extend_from_slice(&[0xfd, 0x1b, lane]);
    let parsed = parse_module(&module(&code)).expect("fixture parses");
    let mut instance = Instance::new(parsed).expect("fixture validates");
    match instance.invoke_export_values("run", &[]).expect("execution succeeds").as_slice() {
        [Value::I32(value)] => *value,
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn executes_in_range_and_zeroes_upper_lanes() {
    let lanes = [1.75, -12345.75];
    assert_eq!(run_lane(lanes, 0), 1);
    assert_eq!(run_lane(lanes, 1), -12345);
    assert_eq!(run_lane(lanes, 2), 0);
    assert_eq!(run_lane(lanes, 3), 0);
}

#[test]
fn uses_permitted_saturating_choices_for_special_lanes() {
    assert_eq!(run_lane([f64::NAN, f64::INFINITY], 0), 0);
    assert_eq!(run_lane([f64::NAN, f64::INFINITY], 1), i32::MAX);
    assert_eq!(run_lane([f64::NEG_INFINITY, 42.9], 0), i32::MIN);
    assert_eq!(run_lane([f64::NEG_INFINITY, 42.9], 1), 42);
}

#[test]
fn validator_rejects_type_confusion() {
    let mut code = vec![0x41, 0x00];
    simd(&mut code, 259);
    code.extend_from_slice(&[0xfd, 0x1b, 0x00]);
    let parsed = parse_module(&module(&code)).expect("fixture parses");
    assert!(matches!(Instance::new(parsed), Err(RuntimeError::Validation(ValidationError::TypeMismatch { .. }))));
}

#[test]
fn next_subopcode_remains_fail_closed() {
    let mut code = Vec::new();
    push_f64x2_const(&mut code, [1.0; 2]);
    simd(&mut code, 260);
    code.extend_from_slice(&[0xfd, 0x1b, 0x00]);
    let parsed = parse_module(&module(&code)).expect("fixture parses");
    assert!(matches!(Instance::new(parsed), Err(RuntimeError::Validation(ValidationError::UnsupportedPrefixedOpcode { prefix: 0xfd, subopcode: 260, .. }))));
}
''')

old_diff = Path('differential/tests/simd_relaxed_madd_f32x4.rs')
if old_diff.exists(): old_diff.unlink()
Path('differential/tests/simd_relaxed_trunc_f64x2_s_zero.rs').write_text(r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "a") (result i32)
    v128.const f64x2 1.75 -12345.75
    i32x4.relaxed_trunc_f64x2_s_zero
    i32x4.extract_lane 0)
  (func (export "b") (result i32)
    v128.const f64x2 1.75 -12345.75
    i32x4.relaxed_trunc_f64x2_s_zero
    i32x4.extract_lane 1)
  (func (export "z2") (result i32)
    v128.const f64x2 1.75 -12345.75
    i32x4.relaxed_trunc_f64x2_s_zero
    i32x4.extract_lane 2)
  (func (export "z3") (result i32)
    v128.const f64x2 1.75 -12345.75
    i32x4.relaxed_trunc_f64x2_s_zero
    i32x4.extract_lane 3))
"#;
const EXPORTS: [&str; 4] = ["a", "b", "z2", "z3"];

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini parses fixture");
    let mut instance = MiniInstance::new(module).expect("mini instantiates fixture");
    EXPORTS.into_iter().map(|export| match instance.invoke_export_values(export, &[]).expect("mini executes").as_slice() {
        [Value::I32(value)] => *value,
        other => panic!("unexpected mini result for {export}: {other:?}"),
    }).collect()
}

fn reference_trace(bytes: &[u8]) -> Vec<i32> {
    let mut config = Config::new();
    config.wasm_simd(true);
    config.wasm_relaxed_simd(true);
    let engine = Engine::new(&config).expect("engine initializes");
    let module = ReferenceModule::new(&engine, bytes).expect("Wasmtime compiles fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[]).expect("Wasmtime instantiates");
    EXPORTS.into_iter().map(|export| instance.get_typed_func::<(), i32>(&mut store, export).expect("typed export").call(&mut store, ()).expect("Wasmtime executes")).collect()
}

#[test]
fn matches_wasmtime_on_deterministic_lanes() {
    let bytes = wat::parse_str(FIXTURE).expect("WAT parses");
    let expected = vec![1, -12345, 0, 0];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, expected);
    assert_eq!(reference, expected);
    assert_eq!(mini, reference);
}
''')
