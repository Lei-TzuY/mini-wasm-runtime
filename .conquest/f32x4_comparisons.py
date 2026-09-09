from pathlib import Path

runtime = Path("crates/wasm-runtime/src/lib.rs")
text = runtime.read_text()
marker = "        77 => {"
insert = '''        65..=70 => {
            let rhs = numeric::v128_from_stack(stack)?;
            let lhs = numeric::v128_from_stack(stack)?;
            let mut result = [0u8; 16];
            for lane in 0..4 {
                let start = lane * 4;
                let lhs_value = f32::from_le_bytes(
                    lhs[start..start + 4]
                        .try_into()
                        .expect("f32x4 lane is four bytes"),
                );
                let rhs_value = f32::from_le_bytes(
                    rhs[start..start + 4]
                        .try_into()
                        .expect("f32x4 lane is four bytes"),
                );
                let predicate = match subopcode {
                    65 => lhs_value == rhs_value,
                    66 => lhs_value != rhs_value,
                    67 => lhs_value < rhs_value,
                    68 => lhs_value > rhs_value,
                    69 => lhs_value <= rhs_value,
                    70 => lhs_value >= rhs_value,
                    _ => unreachable!("matched f32x4 comparison opcode"),
                };
                let mask = if predicate { u32::MAX } else { 0 };
                result[start..start + 4].copy_from_slice(&mask.to_le_bytes());
            }
            stack.push(Value::V128(result));
        }
'''
head, sep, tail = text.partition("fn execute_simd(")
assert sep, "execute_simd not found"
assert tail.count(marker) >= 1, "runtime insertion marker missing"
tail = tail.replace(marker, insert + marker, 1)
text = head + sep + tail
old_scan = "                    | 55..=64\n                    | 77..=83"
new_scan = "                    | 55..=64\n                    | 65..=70\n                    | 77..=83"
assert old_scan in text, "control-map SIMD range marker missing"
text = text.replace(old_scan, new_scan, 1)
runtime.write_text(text)

validator = Path("crates/wasm-validator/src/typed.rs")
text = validator.read_text()
old = '''                    55..=64 => {
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        stack.push(ValueType::V128);
                    }
                    77 => {'''
new = '''                    55..=64 => {
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        stack.push(ValueType::V128);
                    }
                    65..=70 => {
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        stack.push(ValueType::V128);
                    }
                    77 => {'''
assert old in text, "typed validator SIMD range marker missing"
validator.write_text(text.replace(old, new, 1))

roadmap = Path("docs/roadmap.md")
text = roadmap.read_text()
anchor = "- SIMD cross-width widening is executable for `i16x8.extend_low/high_i8x16_{s,u}`, with signed/unsigned low/high lane semantics, typed validation, structured-control scanning, regressions, and Wasmtime differential coverage."
addition = anchor + "\n- SIMD floating-point comparisons are executable for `f32x4.{eq,ne,lt,gt,le,ge}`, including canonical i32 lane masks, NaN behavior, typed validation, structured-control scanning, regressions, and Wasmtime differential coverage."
assert anchor in text, "roadmap widening anchor missing"
roadmap.write_text(text.replace(anchor, addition, 1))

runtime_test = r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};
use wasm_validator::ValidationError;

fn push_u32(bytes: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        bytes.push(byte);
        if value == 0 {
            break;
        }
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

fn push_simd(instructions: &mut Vec<u8>, subopcode: u32) {
    instructions.push(0xfd);
    push_u32(instructions, subopcode);
}

fn push_v128_f32(instructions: &mut Vec<u8>, lanes: [f32; 4]) {
    push_simd(instructions, 12);
    for lane in lanes {
        instructions.extend_from_slice(&lane.to_bits().to_le_bytes());
    }
}

fn run_i32(instructions: &[u8]) -> i32 {
    let parsed = parse_module(&module(instructions)).expect("f32x4 comparison fixture must parse");
    let mut instance = Instance::new(parsed).expect("f32x4 comparison fixture must validate");
    match instance
        .invoke_export_values("run", &[])
        .expect("f32x4 comparison fixture must execute")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected f32x4 comparison result: {other:?}"),
    }
}

fn comparison_mask(subopcode: u32) -> i32 {
    let mut instructions = Vec::new();
    push_v128_f32(&mut instructions, [-1.0, 0.0, f32::NAN, 4.0]);
    push_v128_f32(&mut instructions, [0.0, -0.0, 1.0, 4.0]);
    push_simd(&mut instructions, subopcode);
    push_simd(&mut instructions, 164); // i32x4.bitmask
    run_i32(&instructions)
}

#[test]
fn f32x4_comparisons_produce_canonical_lane_masks() {
    let cases = [(65, 10), (66, 5), (67, 1), (68, 0), (69, 11), (70, 10)];
    for (subopcode, expected) in cases {
        assert_eq!(comparison_mask(subopcode), expected, "subopcode {subopcode}");
    }
}

#[test]
fn f32x4_comparison_executes_inside_structured_control() {
    let mut instructions = vec![0x02, 0x7f];
    push_v128_f32(&mut instructions, [1.0, 2.0, 3.0, 4.0]);
    push_v128_f32(&mut instructions, [1.0, 0.0, 3.0, 5.0]);
    push_simd(&mut instructions, 65);
    push_simd(&mut instructions, 164);
    instructions.push(0x0b);
    assert_eq!(run_i32(&instructions), 5);
}

#[test]
fn validator_rejects_f32x4_comparison_type_confusion() {
    let mut instructions = Vec::new();
    push_v128_f32(&mut instructions, [1.0; 4]);
    instructions.extend_from_slice(&[0x41, 0x01]);
    push_simd(&mut instructions, 65);
    push_simd(&mut instructions, 164);
    let parsed = parse_module(&module(&instructions)).expect("type-confusion fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(ValidationError::TypeMismatch { .. }))
    ));
}

#[test]
fn adjacent_f64x2_comparison_remains_fail_closed() {
    let mut instructions = Vec::new();
    push_v128_f32(&mut instructions, [0.0; 4]);
    push_v128_f32(&mut instructions, [0.0; 4]);
    push_simd(&mut instructions, 71); // f64x2.eq
    instructions.push(0x1a);
    instructions.extend_from_slice(&[0x41, 0x00]);
    let parsed = parse_module(&module(&instructions)).expect("unsupported SIMD fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::UnsupportedPrefixedOpcode {
                prefix: 0xfd,
                subopcode: 71,
                ..
            }
        ))
    ));
}
'''
Path("crates/wasm-runtime/tests/simd_f32x4_comparisons.rs").write_text(runtime_test)

differential_test = r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "eq") (result i32)
    v128.const f32x4 -1 0 nan 4
    v128.const f32x4 0 -0 1 4
    f32x4.eq
    i32x4.bitmask)
  (func (export "ne") (result i32)
    v128.const f32x4 -1 0 nan 4
    v128.const f32x4 0 -0 1 4
    f32x4.ne
    i32x4.bitmask)
  (func (export "lt") (result i32)
    v128.const f32x4 -1 0 nan 4
    v128.const f32x4 0 -0 1 4
    f32x4.lt
    i32x4.bitmask)
  (func (export "gt") (result i32)
    v128.const f32x4 -1 0 nan 4
    v128.const f32x4 0 -0 1 4
    f32x4.gt
    i32x4.bitmask)
  (func (export "le") (result i32)
    v128.const f32x4 -1 0 nan 4
    v128.const f32x4 0 -0 1 4
    f32x4.le
    i32x4.bitmask)
  (func (export "ge") (result i32)
    v128.const f32x4 -1 0 nan 4
    v128.const f32x4 0 -0 1 4
    f32x4.ge
    i32x4.bitmask)
)
"#;

const EXPORTS: [&str; 6] = ["eq", "ne", "lt", "gt", "le", "ge"];

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini runtime must parse f32x4 comparison fixture");
    let mut instance = MiniInstance::new(module).expect("mini runtime must instantiate f32x4 fixture");
    EXPORTS
        .into_iter()
        .map(|export| match instance
            .invoke_export_values(export, &[])
            .expect("mini f32x4 comparison must execute")
            .as_slice()
        {
            [Value::I32(value)] => *value,
            other => panic!("unexpected mini result for {export}: {other:?}"),
        })
        .collect()
}

fn reference_trace(bytes: &[u8]) -> Vec<i32> {
    let mut config = Config::new();
    config.wasm_simd(true);
    let engine = Engine::new(&config).expect("SIMD-enabled Wasmtime engine must initialize");
    let module = ReferenceModule::new(&engine, bytes).expect("Wasmtime must compile f32x4 fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime must instantiate f32x4 fixture");
    EXPORTS
        .into_iter()
        .map(|export| {
            instance
                .get_typed_func::<(), i32>(&mut store, export)
                .expect("f32x4 comparison export must be [] -> [i32]")
                .call(&mut store, ())
                .expect("Wasmtime f32x4 comparison must execute")
        })
        .collect()
}

#[test]
fn f32x4_comparisons_match_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("f32x4 comparison WAT must parse");
    let expected = vec![10, 5, 1, 0, 11, 10];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, expected, "mini f32x4 comparison trace drifted");
    assert_eq!(reference, expected, "Wasmtime f32x4 comparison trace drifted");
    assert_eq!(mini, reference, "f32x4 comparison traces diverged");
}
'''
Path("differential/tests/simd_f32x4_comparisons.rs").write_text(differential_test)
