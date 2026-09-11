from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    if text.count(old) != 1:
        raise SystemExit(f"expected one match in {path}: {old!r}, found {text.count(old)}")
    p.write_text(text.replace(old, new, 1))


runtime = "crates/wasm-runtime/src/lib.rs"
insert = r'''        236 | 237 | 239 => {
            let value = numeric::v128_from_stack(stack)?;
            let mut result = [0u8; 16];
            for lane in 0..2 {
                let start = lane * 8;
                let bits = u64::from_le_bytes(
                    value[start..start + 8]
                        .try_into()
                        .expect("f64x2 lane width"),
                );
                let output = match subopcode {
                    236 => bits & 0x7fff_ffff_ffff_ffff,
                    237 => bits ^ 0x8000_0000_0000_0000,
                    239 => f64::from_bits(bits).sqrt().to_bits(),
                    _ => unreachable!("matched f64x2 unary opcode"),
                };
                result[start..start + 8].copy_from_slice(&output.to_le_bytes());
            }
            stack.push(Value::V128(Rc::new(result)));
        }
'''
replace_once(runtime, "        228..=235 => {", insert + "        228..=235 => {")
replace_once(
    runtime,
    "                    | 228..=235\n                    | 142",
    "                    | 228..=235\n                    | 236\n                    | 237\n                    | 239\n                    | 142",
)

validator = "crates/wasm-validator/src/typed.rs"
replace_once(
    validator,
    "                    96 | 97 | 98 | 128 | 129 | 224 | 225 | 227 => {",
    "                    96 | 97 | 98 | 128 | 129 | 224 | 225 | 227 | 236 | 237 | 239 => {",
)

frontier = "crates/wasm-runtime/tests/simd_f32x4_unary.rs"
replace_once(
    frontier,
    "fn adjacent_f32x4_f64x2_abs_frontier_remains_fail_closed()",
    "fn adjacent_f64x2_add_frontier_remains_fail_closed()",
)
replace_once(frontier, "push_simd(&mut instructions, 236);", "push_simd(&mut instructions, 240);")
replace_once(frontier, "subopcode: 236,", "subopcode: 240,")

Path("crates/wasm-runtime/tests/simd_f64x2_unary.rs").write_text(r'''use wasm_parser::parse_module;
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
fn push_simd(i: &mut Vec<u8>, sub: u32) { i.push(0xfd); push_u32(i, sub); }
fn push_f64x2_const(i: &mut Vec<u8>, lanes: [f64; 2]) {
    push_simd(i, 12);
    for lane in lanes { i.extend_from_slice(&lane.to_bits().to_le_bytes()); }
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
        if done { break; }
    }
}
fn push_v128_store(i: &mut Vec<u8>) { push_simd(i, 11); i.extend_from_slice(&[4, 0]); }
fn push_i64_load(i: &mut Vec<u8>, offset: u32) { i.push(0x29); i.push(3); push_u32(i, offset); }
fn lane_bits(input: [f64; 2], subopcode: u32, lane: u32) -> u64 {
    let mut instructions = Vec::new();
    instructions.extend_from_slice(&[0x02, 0x40]);
    push_f64x2_const(&mut instructions, input);
    push_simd(&mut instructions, subopcode);
    instructions.push(0x1a);
    instructions.push(0x0b);
    push_i32_const(&mut instructions, 0);
    push_f64x2_const(&mut instructions, input);
    push_simd(&mut instructions, subopcode);
    push_v128_store(&mut instructions);
    push_i32_const(&mut instructions, 0);
    push_i64_load(&mut instructions, lane * 8);
    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    let mut instance = Instance::new(parsed).expect("fixture validates");
    match instance.invoke_export_values("run", &[]).expect("fixture executes").as_slice() {
        [Value::I64(value)] => *value as u64,
        other => panic!("unexpected f64x2 unary result: {other:?}"),
    }
}

#[test]
fn f64x2_abs_neg_and_sqrt_are_lane_exact() {
    assert_eq!(lane_bits([-0.0, -9.0], 236, 0), 0.0f64.to_bits());
    assert_eq!(lane_bits([-0.0, -9.0], 236, 1), 9.0f64.to_bits());
    assert_eq!(lane_bits([0.0, 4.0], 237, 0), (-0.0f64).to_bits());
    assert_eq!(lane_bits([0.0, 4.0], 237, 1), (-4.0f64).to_bits());
    assert_eq!(lane_bits([1.0, 81.0], 239, 0), 1.0f64.to_bits());
    assert_eq!(lane_bits([1.0, 81.0], 239, 1), 9.0f64.to_bits());
}

#[test]
fn f64x2_abs_and_neg_preserve_nan_payload_bits_except_sign() {
    let nan = f64::from_bits(0xfff8_1234_5678_9abc);
    assert_eq!(lane_bits([nan, 1.0], 236, 0), 0x7ff8_1234_5678_9abc);
    assert_eq!(lane_bits([nan, 1.0], 237, 0), 0x7ff8_1234_5678_9abc);
}

#[test]
fn validator_rejects_f64x2_unary_type_confusion() {
    let instructions = vec![0x41, 0x01, 0xfd, 0xec, 0x01];
    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(ValidationError::TypeMismatch { .. }))
    ));
}

#[test]
fn adjacent_f64x2_add_frontier_remains_fail_closed() {
    let mut instructions = Vec::new();
    push_f64x2_const(&mut instructions, [1.0, 2.0]);
    push_f64x2_const(&mut instructions, [3.0, 4.0]);
    push_simd(&mut instructions, 240);
    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(ValidationError::UnsupportedPrefixedOpcode {
            prefix: 0xfd, subopcode: 240, ..
        }))
    ));
}
''')

Path("differential/tests/simd_f64x2_unary.rs").write_text(r'''use wasm_runtime::{Instance, Value};
use wasmtime::{Engine, Instance as WasmtimeInstance, Module as WasmtimeModule, Store};

const FIXTURE: &str = r#"(module
  (memory 1)
  (func (export "abs") (result i64) i32.const 0 v128.const f64x2 -3.5 -0 f64x2.abs v128.store i32.const 0 i64.load)
  (func (export "neg") (result i64) i32.const 0 v128.const f64x2 3.5 2 f64x2.neg v128.store i32.const 0 i64.load offset=8)
  (func (export "sqrt") (result i64) i32.const 0 v128.const f64x2 4 81 f64x2.sqrt v128.store i32.const 0 i64.load offset=8))"#;

#[test]
fn f64x2_unary_matches_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("wat");
    let parsed = wasm_parser::parse_module(&bytes).expect("parse");
    let mut mini = Instance::new(parsed).expect("mini");
    let mini_values = ["abs", "neg", "sqrt"].map(|name| match mini
        .invoke_export_values(name, &[]).unwrap().as_slice() {
        [Value::I64(v)] => *v,
        _ => panic!(),
    });

    let engine = Engine::default();
    let module = WasmtimeModule::new(&engine, &bytes).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = WasmtimeInstance::new(&mut store, &module, &[]).unwrap();
    let reference = ["abs", "neg", "sqrt"].map(|name| instance
        .get_typed_func::<(), i64>(&mut store, name).unwrap()
        .call(&mut store, ()).unwrap());

    assert_eq!(mini_values, reference);
}
''')

roadmap = Path("docs/roadmap.md")
text = roadmap.read_text()
old = "the adjacent `f64x2.abs` opcode remains fail-closed."
new = "`f64x2.abs`, `f64x2.neg`, and `f64x2.sqrt` are executable with typed validation, structured-control handling, focused regressions, and Wasmtime differential coverage; the adjacent `f64x2.add` opcode remains fail-closed."
if old not in text:
    raise SystemExit("roadmap frontier sentence not found")
roadmap.write_text(text.replace(old, new, 1))
