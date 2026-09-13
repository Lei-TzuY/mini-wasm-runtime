from pathlib import Path
import re

lib = Path("crates/wasm-runtime/src/lib.rs")
text = lib.read_text()
if "        265 => {" not in text:
    arm = """        265 => {
            // i8x16.relaxed_laneselect permits either lane-sign selection or bit selection
            // for mixed masks. Choose deterministic bit-selection semantics for every byte.
            let mask = numeric::v128_from_stack(stack)?;
            let b = numeric::v128_from_stack(stack)?;
            let a = numeric::v128_from_stack(stack)?;
            let mut result = [0u8; 16];
            for index in 0..16 {
                result[index] = (a[index] & mask[index]) | (b[index] & !mask[index]);
            }
            stack.push(Value::V128(Rc::new(result)));
        }
"""
    anchor = "        256 => {"
    if anchor not in text:
        raise SystemExit("runtime 256 anchor missing")
    text = text.replace(anchor, arm + anchor, 1)
if "240..=264" in text:
    text = text.replace("240..=264", "240..=265")
elif "240..=265" not in text:
    raise SystemExit("control-map frontier anchor missing")
lib.write_text(text)

validator = Path("crates/wasm-validator/src/typed.rs")
text = validator.read_text()
if re.search(r"^\s+265 => \{", text, re.M) is None:
    pattern = re.compile(r"(                    264 => \{.*?                        stack\.push\(ValueType::V128\);\n                    \}\n)", re.S)
    match = pattern.search(text)
    if not match:
        raise SystemExit("validator 264 anchor missing")
    block = """                    265 => {
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        stack.push(ValueType::V128);
                    }
"""
    text = text[:match.end()] + block + text[match.end():]
validator.write_text(text)

for path in Path("crates/wasm-runtime/tests").glob("*.rs"):
    text = path.read_text()
    original = text
    text = re.sub(r"((?:push_simd|simd)\([^\n]*,\s*)265(\s*\))", r"\g<1>266\2", text)
    text = text.replace("subopcode: 265", "subopcode: 266")
    if text != original:
        path.write_text(text)

Path("crates/wasm-runtime/tests/simd_relaxed_laneselect_i8x16.rs").write_text(r'''use wasm_parser::parse_module;
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

fn v128_const(code: &mut Vec<u8>, value: [u8; 16]) {
    code.extend_from_slice(&[0xfd, 0x0c]);
    code.extend_from_slice(&value);
}

fn simd(code: &mut Vec<u8>, subopcode: u32) {
    code.push(0xfd);
    push_u32(code, subopcode);
}

fn execute_lane0(a: [u8; 16], b: [u8; 16], mask: [u8; 16]) -> i32 {
    let mut code = Vec::new();
    v128_const(&mut code, a);
    v128_const(&mut code, b);
    v128_const(&mut code, mask);
    simd(&mut code, 265);
    code.extend_from_slice(&[0xfd, 0x16, 0x00]);
    let parsed = parse_module(&module(&code)).expect("lane-select fixture parses");
    let mut instance = Instance::new(parsed).expect("lane-select fixture validates");
    match instance.invoke_export_values("run", &[]).expect("lane-select executes").as_slice() {
        [Value::I32(value)] => *value,
        other => panic!("unexpected lane-select result: {other:?}"),
    }
}

#[test]
fn relaxed_laneselect_handles_deterministic_and_mixed_masks() {
    let a = [0xaa; 16];
    let b = [0x55; 16];
    let mut all_a = [0u8; 16];
    all_a[0] = 0xff;
    assert_eq!(execute_lane0(a, b, all_a), 0xaa);
    assert_eq!(execute_lane0(a, b, [0u8; 16]), 0x55);
    let mut mixed = [0u8; 16];
    mixed[0] = 0xf0;
    assert_eq!(execute_lane0(a, b, mixed), 0xa5);
}

#[test]
fn relaxed_laneselect_validates_three_v128_operands() {
    let mut code = Vec::new();
    v128_const(&mut code, [1; 16]);
    v128_const(&mut code, [2; 16]);
    code.extend_from_slice(&[0x41, 0x00]);
    simd(&mut code, 265);
    code.extend_from_slice(&[0xfd, 0x16, 0x00]);
    let parsed = parse_module(&module(&code)).expect("type-confusion fixture parses");
    assert!(matches!(Instance::new(parsed), Err(RuntimeError::Validation(ValidationError::TypeMismatch { .. }))));
}

#[test]
fn next_relaxed_simd_subopcode_remains_fail_closed() {
    let mut code = Vec::new();
    v128_const(&mut code, [1; 16]);
    v128_const(&mut code, [2; 16]);
    v128_const(&mut code, [0xff; 16]);
    simd(&mut code, 266);
    code.extend_from_slice(&[0xfd, 0x16, 0x00]);
    let parsed = parse_module(&module(&code)).expect("frontier fixture parses");
    assert!(matches!(Instance::new(parsed), Err(RuntimeError::Validation(ValidationError::UnsupportedPrefixedOpcode { prefix: 0xfd, subopcode: 266, .. }))));
}
''')

Path("differential/tests/simd_relaxed_laneselect_i8x16.rs").write_text(r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "all_a") (result i32)
    v128.const i8x16 -86 -86 -86 -86 -86 -86 -86 -86 -86 -86 -86 -86 -86 -86 -86 -86
    v128.const i8x16 85 85 85 85 85 85 85 85 85 85 85 85 85 85 85 85
    v128.const i8x16 -1 -1 -1 -1 -1 -1 -1 -1 -1 -1 -1 -1 -1 -1 -1 -1
    i8x16.relaxed_laneselect
    i8x16.extract_lane_u 0)
  (func (export "all_b") (result i32)
    v128.const i8x16 -86 -86 -86 -86 -86 -86 -86 -86 -86 -86 -86 -86 -86 -86 -86 -86
    v128.const i8x16 85 85 85 85 85 85 85 85 85 85 85 85 85 85 85 85
    v128.const i8x16 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0
    i8x16.relaxed_laneselect
    i8x16.extract_lane_u 0))
"#;
const EXPORTS: [&str; 2] = ["all_a", "all_b"];

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini runtime parses relaxed lane-select fixture");
    let mut instance = MiniInstance::new(module).expect("mini runtime instantiates relaxed lane-select fixture");
    EXPORTS.into_iter().map(|export| match instance.invoke_export_values(export, &[]).expect("mini lane-select executes").as_slice() {
        [Value::I32(value)] => *value,
        other => panic!("unexpected mini result for {export}: {other:?}"),
    }).collect()
}

fn reference_trace(bytes: &[u8]) -> Vec<i32> {
    let mut config = Config::new();
    config.wasm_simd(true);
    config.wasm_relaxed_simd(true);
    let engine = Engine::new(&config).expect("Wasmtime engine initializes");
    let module = ReferenceModule::new(&engine, bytes).expect("Wasmtime compiles relaxed lane-select fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[]).expect("Wasmtime instantiates relaxed lane-select fixture");
    EXPORTS.into_iter().map(|export| instance.get_typed_func::<(), i32>(&mut store, export).expect("lane-select export is [] -> [i32]").call(&mut store, ()).expect("Wasmtime lane-select executes")).collect()
}

#[test]
fn relaxed_i8x16_laneselect_matches_wasmtime_for_deterministic_masks() {
    let bytes = wat::parse_str(FIXTURE).expect("relaxed lane-select WAT parses");
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, vec![170, 85]);
    assert_eq!(reference, vec![170, 85]);
    assert_eq!(mini, reference);
}
''')
