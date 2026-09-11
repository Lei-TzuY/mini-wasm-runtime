from pathlib import Path
import re

runtime_path = Path("crates/wasm-runtime/src/lib.rs")
runtime = runtime_path.read_text()
anchor = "        228..=235 => {\n"
assert runtime.count(anchor) == 1
binary_block = '''        240..=243 => {
            let rhs = numeric::v128_from_stack(stack)?;
            let lhs = numeric::v128_from_stack(stack)?;
            let mut result = [0u8; 16];
            for lane in 0..2 {
                let start = lane * 8;
                let lhs_lane = f64::from_bits(u64::from_le_bytes(
                    lhs[start..start + 8].try_into().expect("f64x2 lane width"),
                ));
                let rhs_lane = f64::from_bits(u64::from_le_bytes(
                    rhs[start..start + 8].try_into().expect("f64x2 lane width"),
                ));
                let output = match subopcode {
                    240 => lhs_lane + rhs_lane,
                    241 => lhs_lane - rhs_lane,
                    242 => lhs_lane * rhs_lane,
                    243 => lhs_lane / rhs_lane,
                    _ => unreachable!("matched f64x2 binary opcode"),
                };
                result[start..start + 8].copy_from_slice(&output.to_bits().to_le_bytes());
            }
            stack.push(Value::V128(Rc::new(result)));
        }
'''
runtime = runtime.replace(anchor, binary_block + anchor)
control_anchor = "                    | 239\n                    | 142\n"
assert runtime.count(control_anchor) == 1
runtime = runtime.replace(control_anchor, "                    | 239\n                    | 240..=243\n                    | 142\n")
runtime_path.write_text(runtime)

typed_path = Path("crates/wasm-validator/src/typed.rs")
typed = typed_path.read_text()
typed_anchor = "                    | 220..=223\n                    | 228..=235 => {\n"
assert typed.count(typed_anchor) == 1
typed = typed.replace(typed_anchor, "                    | 220..=223\n                    | 228..=235\n                    | 240..=243 => {\n")
typed_path.write_text(typed)

# Advance every existing SIMD fail-closed sentinel from f64x2.add (240) to f64x2.min (244).
# Do this before creating the new binary tests, where 240 is intentionally executable.
for path in Path("crates/wasm-runtime/tests").glob("simd_*.rs"):
    text = path.read_text()
    if "subopcode: 240" not in text:
        continue
    text = re.sub(r"push_simd\(([^\n]+?), 240\)", r"push_simd(\1, 244)", text)
    text = text.replace("subopcode: 240", "subopcode: 244")
    text = text.replace("f64x2_add_frontier", "f64x2_min_frontier")
    text = text.replace("f32x4_f64x2_add_frontier", "f32x4_f64x2_min_frontier")
    text = text.replace("next f32x4 opcode remains outside this slice", "next f64x2 opcode remains outside this slice")
    path.write_text(text)

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
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0, 0, 0];
    push_section(&mut bytes, 1, &[0x01, 0x60, 0x00, 0x01, 0x7e]);
    push_section(&mut bytes, 3, &[0x01, 0x00]);
    push_section(&mut bytes, 5, &[0x01, 0x00, 0x01]);
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

fn push_simd(i: &mut Vec<u8>, subopcode: u32) {
    i.push(0xfd);
    push_u32(i, subopcode);
}

fn push_f64x2_const(i: &mut Vec<u8>, lanes: [f64; 2]) {
    push_simd(i, 12);
    for lane in lanes {
        i.extend_from_slice(&lane.to_bits().to_le_bytes());
    }
}

fn push_i32_const(i: &mut Vec<u8>, value: i32) {
    i.push(0x41);
    let mut value = value;
    loop {
        let byte = (value as u8) & 0x7f;
        value >>= 7;
        let sign = byte & 0x40 != 0;
        let done = (value == 0 && !sign) || (value == -1 && sign);
        i.push(if done { byte } else { byte | 0x80 });
        if done {
            break;
        }
    }
}

fn push_v128_store(i: &mut Vec<u8>) {
    push_simd(i, 11);
    i.extend_from_slice(&[4, 0]);
}

fn push_i64_load(i: &mut Vec<u8>, offset: u32) {
    i.push(0x29);
    i.push(3);
    push_u32(i, offset);
}

fn lane_bits(lhs: [f64; 2], rhs: [f64; 2], subopcode: u32, lane: u32) -> u64 {
    let mut instructions = Vec::new();
    instructions.extend_from_slice(&[0x02, 0x40]);
    push_f64x2_const(&mut instructions, lhs);
    push_f64x2_const(&mut instructions, rhs);
    push_simd(&mut instructions, subopcode);
    instructions.push(0x1a);
    instructions.push(0x0b);
    push_i32_const(&mut instructions, 0);
    push_f64x2_const(&mut instructions, lhs);
    push_f64x2_const(&mut instructions, rhs);
    push_simd(&mut instructions, subopcode);
    push_v128_store(&mut instructions);
    push_i32_const(&mut instructions, 0);
    push_i64_load(&mut instructions, lane * 8);

    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    let mut instance = Instance::new(parsed).expect("fixture validates");
    match instance
        .invoke_export_values("run", &[])
        .expect("fixture executes")
        .as_slice()
    {
        [Value::I64(value)] => *value as u64,
        other => panic!("unexpected f64x2 binary result: {other:?}"),
    }
}

#[test]
fn f64x2_add_sub_mul_div_are_lane_exact_for_finite_values() {
    let lhs = [1.5, -8.0];
    let rhs = [2.5, 2.0];
    assert_eq!(lane_bits(lhs, rhs, 240, 0), 4.0f64.to_bits());
    assert_eq!(lane_bits(lhs, rhs, 241, 1), (-10.0f64).to_bits());
    assert_eq!(lane_bits([6.0, -9.0], [-0.5, 3.0], 242, 0), (-3.0f64).to_bits());
    assert_eq!(lane_bits([6.0, -9.0], [-0.5, 3.0], 243, 1), (-3.0f64).to_bits());
}

#[test]
fn f64x2_binary_preserves_ieee_zero_infinity_and_nan_behavior() {
    assert_eq!(lane_bits([-0.0, 1.0], [0.0, 1.0], 240, 0), 0.0f64.to_bits());
    assert_eq!(lane_bits([1.0, 0.0], [0.0, 0.0], 243, 0), f64::INFINITY.to_bits());
    assert!(f64::from_bits(lane_bits([1.0, 0.0], [1.0, 0.0], 243, 1)).is_nan());
}

#[test]
fn validator_rejects_f64x2_binary_type_confusion() {
    let mut instructions = Vec::new();
    push_f64x2_const(&mut instructions, [1.0; 2]);
    push_i32_const(&mut instructions, 1);
    push_simd(&mut instructions, 240);
    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(ValidationError::TypeMismatch { .. }))
    ));
}

#[test]
fn adjacent_f64x2_min_frontier_remains_fail_closed() {
    let mut instructions = Vec::new();
    push_f64x2_const(&mut instructions, [1.0, 2.0]);
    push_f64x2_const(&mut instructions, [3.0, 4.0]);
    push_simd(&mut instructions, 244);
    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::UnsupportedPrefixedOpcode {
                prefix: 0xfd,
                subopcode: 244,
                ..
            }
        ))
    ));
}
'''
Path("crates/wasm-runtime/tests/simd_f64x2_binary.rs").write_text(runtime_test)

differential_test = r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"(module
  (memory 1)
  (func (export "add") (result i64) i32.const 0 v128.const f64x2 1.5 -8 v128.const f64x2 2.5 2 f64x2.add v128.store i32.const 0 i64.load)
  (func (export "sub") (result i64) i32.const 0 v128.const f64x2 1.5 -8 v128.const f64x2 2.5 2 f64x2.sub v128.store i32.const 0 i64.load offset=8)
  (func (export "mul") (result i64) i32.const 0 v128.const f64x2 6 -9 v128.const f64x2 -0.5 3 f64x2.mul v128.store i32.const 0 i64.load)
  (func (export "div") (result i64) i32.const 0 v128.const f64x2 6 -9 v128.const f64x2 -0.5 3 f64x2.div v128.store i32.const 0 i64.load offset=8))"#;
const EXPORTS: [&str; 4] = ["add", "sub", "mul", "div"];

fn mini_trace(bytes: &[u8]) -> Vec<i64> {
    let module = parse_module(bytes).expect("mini parse");
    let mut instance = MiniInstance::new(module).expect("mini instantiate");
    EXPORTS
        .into_iter()
        .map(|export| match instance
            .invoke_export_values(export, &[])
            .expect("mini execution")
            .as_slice()
        {
            [Value::I64(value)] => *value,
            other => panic!("unexpected mini result for {export}: {other:?}"),
        })
        .collect()
}

fn reference_trace(bytes: &[u8]) -> Vec<i64> {
    let mut config = Config::new();
    config.wasm_simd(true);
    let engine = Engine::new(&config).expect("SIMD engine");
    let module = ReferenceModule::new(&engine, bytes).expect("Wasmtime compile");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[]).expect("instantiate");
    EXPORTS
        .into_iter()
        .map(|export| {
            instance
                .get_typed_func::<(), i64>(&mut store, export)
                .expect("signature")
                .call(&mut store, ())
                .expect("reference execution")
        })
        .collect()
}

#[test]
fn f64x2_binary_arithmetic_matches_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("fixture parse");
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, reference);
    assert_eq!(mini[0] as u64, 4.0f64.to_bits());
    assert_eq!(mini[1] as u64, (-10.0f64).to_bits());
    assert_eq!(mini[2] as u64, (-3.0f64).to_bits());
    assert_eq!(mini[3] as u64, (-3.0f64).to_bits());
}
'''
Path("differential/tests/simd_f64x2_binary.rs").write_text(differential_test)

roadmap_path = Path("docs/roadmap.md")
roadmap = roadmap_path.read_text()
old = "the adjacent `f64x2.add` opcode remains fail-closed."
assert roadmap.count(old) == 1
new = "`f64x2.add`, `f64x2.sub`, `f64x2.mul`, and `f64x2.div` are executable with typed validation, structured-control handling, IEEE-754 edge regressions, and Wasmtime differential coverage; the adjacent `f64x2.min` opcode remains fail-closed."
roadmap_path.write_text(roadmap.replace(old, new))
