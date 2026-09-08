from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    if text.count(old) != 1:
        raise SystemExit(f"anchor mismatch in {path}: {text.count(old)} matches")
    p.write_text(text.replace(old, new, 1))

runtime = "crates/wasm-runtime/src/lib.rs"
replace_once(
    runtime,
    "        128 | 129 => {\n",
    """        107..=109 => {
            let shift = (numeric::i32_from_stack(stack)? as u32) & 7;
            let value = numeric::v128_from_stack(stack)?;
            let mut result = [0u8; 16];
            for (output, lane_unsigned) in result.iter_mut().zip(value.iter().copied()) {
                *output = match subopcode {
                    107 => lane_unsigned.wrapping_shl(shift),
                    108 => ((lane_unsigned as i8) >> shift) as u8,
                    109 => lane_unsigned >> shift,
                    _ => unreachable!(\"matched i8x16 shift opcode\"),
                };
            }
            stack.push(Value::V128(Rc::new(result)));
        }
        128 | 129 => {
""",
)
replace_once(
    runtime,
    "                    | 100\n                    | 128\n",
    "                    | 100\n                    | 107\n                    | 108\n                    | 109\n                    | 128\n",
)

validator = "crates/wasm-validator/src/lib.rs"
replace_once(
    validator,
    "                    139..=141 => {\n",
    """                    107..=109 => {
                        pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        stack.push(ValueType::V128);
                    }
                    139..=141 => {
""",
)

Path("crates/wasm-runtime/tests/simd_i8x16_shifts.rs").write_text(r'''use wasm_parser::parse_module;
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

fn run_i32(instructions: &[u8]) -> i32 {
    let parsed = parse_module(&module(instructions)).expect("i8x16 shift fixture must parse");
    let mut instance = Instance::new(parsed).expect("i8x16 shift fixture must validate");
    match instance.invoke_export_values("run", &[]).expect("i8x16 shift fixture must execute").as_slice() {
        [Value::I32(value)] => *value,
        other => panic!("unexpected i8x16 shift result: {other:?}"),
    }
}

#[test]
fn i8x16_shift_family_masks_counts_and_preserves_signedness() {
    assert_eq!(run_i32(&[0x41, 0x40, 0xfd, 0x0f, 0x41, 0x09, 0xfd, 0x6b, 0xfd, 0x15, 0x00]), -128);
    assert_eq!(run_i32(&[0x41, 0x7e, 0xfd, 0x0f, 0x41, 0x01, 0xfd, 0x6c, 0xfd, 0x15, 0x00]), -1);
    assert_eq!(run_i32(&[0x41, 0x80, 0x7f, 0xfd, 0x0f, 0x41, 0x01, 0xfd, 0x6d, 0xfd, 0x16, 0x00]), 64);
}

#[test]
fn i8x16_shifts_execute_inside_structured_control() {
    assert_eq!(run_i32(&[0x02, 0x7f, 0x41, 0x01, 0xfd, 0x0f, 0x41, 0x03, 0xfd, 0x6b, 0xfd, 0x16, 0x00, 0x0b]), 8);
}

#[test]
fn validator_rejects_i8x16_shift_type_confusion() {
    let bytes = module(&[0x41, 0x00, 0x41, 0x01, 0xfd, 0x6b, 0xfd, 0x16, 0x00]);
    let parsed = parse_module(&bytes).expect("type-confusion fixture must parse");
    assert!(matches!(Instance::new(parsed), Err(RuntimeError::Validation(ValidationError::TypeMismatch { .. }))));
}

#[test]
fn adjacent_i8x16_arithmetic_remains_fail_closed() {
    let bytes = module(&[0x41, 0x01, 0xfd, 0x0f, 0x41, 0x02, 0xfd, 0x0f, 0xfd, 0x6e, 0xfd, 0x16, 0x00]);
    let parsed = parse_module(&bytes).expect("adjacent arithmetic fixture must parse");
    assert!(Instance::new(parsed).is_err());
}
''')

Path("differential/tests/simd_i8x16_shifts.rs").write_text(r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "shl") (result i32)
    i32.const 64 i8x16.splat i32.const 9 i8x16.shl i8x16.extract_lane_s 0)
  (func (export "shr_s") (result i32)
    i32.const -2 i8x16.splat i32.const 1 i8x16.shr_s i8x16.extract_lane_s 0)
  (func (export "shr_u") (result i32)
    i32.const -128 i8x16.splat i32.const 1 i8x16.shr_u i8x16.extract_lane_u 0))
"#;
const EXPORTS: [&str; 3] = ["shl", "shr_s", "shr_u"];

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini must parse i8x16 shift fixture");
    let mut instance = MiniInstance::new(module).expect("mini must instantiate i8x16 shift fixture");
    EXPORTS.into_iter().map(|export| match instance.invoke_export_values(export, &[]).expect("mini execution").as_slice() {
        [Value::I32(value)] => *value,
        other => panic!("unexpected mini result for {export}: {other:?}"),
    }).collect()
}

fn reference_trace(bytes: &[u8]) -> Vec<i32> {
    let mut config = Config::new();
    config.wasm_simd(true);
    let engine = Engine::new(&config).expect("SIMD Wasmtime engine");
    let module = ReferenceModule::new(&engine, bytes).expect("Wasmtime compile");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[]).expect("Wasmtime instantiate");
    EXPORTS.into_iter().map(|export| instance.get_typed_func::<(), i32>(&mut store, export).expect("signature").call(&mut store, ()).expect("reference execution")).collect()
}

#[test]
fn i8x16_shifts_match_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("fixture parse");
    let expected = vec![-128, -1, 64];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, expected);
    assert_eq!(reference, expected);
    assert_eq!(mini, reference);
}
''')

roadmap = Path("docs/roadmap.md")
text = roadmap.read_text()
anchor = "- [ ] broaden SIMD lane/arithmetic/memory semantics as subsequent bounded executable slices"
if text.count(anchor) != 1:
    raise SystemExit("roadmap anchor mismatch")
text = text.replace(anchor, "- [x] SIMD `i8x16.shl` / `i8x16.shr_s` / `i8x16.shr_u` scalar-count shifts with exact `v128, i32 -> v128` validation, 3-bit masked counts, signed/unsigned byte-lane semantics, structured-control scanning, deterministic regressions, adjacent arithmetic fail-closed coverage, and pinned Wasmtime differential evidence\n" + anchor, 1)
roadmap.write_text(text)
