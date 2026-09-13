from pathlib import Path

root = Path('.')

lib = root / 'crates/wasm-runtime/src/lib.rs'
text = lib.read_text()
anchor = '''        266 => {
            // Deterministic-profile lowering: use v128.bitselect-equivalent semantics.
            let mask = numeric::v128_from_stack(stack)?;
            let b = numeric::v128_from_stack(stack)?;
            let a = numeric::v128_from_stack(stack)?;
            let mut result = [0u8; 16];
            for index in 0..16 {
                result[index] = (a[index] & mask[index]) | (b[index] & !mask[index]);
            }
            stack.push(Value::V128(Rc::new(result)));
        }
'''
if anchor not in text:
    raise SystemExit('runtime 266 anchor missing')
insert = anchor + '''        267 => {
            // Deterministic-profile lowering: use v128.bitselect-equivalent semantics.
            let mask = numeric::v128_from_stack(stack)?;
            let b = numeric::v128_from_stack(stack)?;
            let a = numeric::v128_from_stack(stack)?;
            let mut result = [0u8; 16];
            for index in 0..16 {
                result[index] = (a[index] & mask[index]) | (b[index] & !mask[index]);
            }
            stack.push(Value::V128(Rc::new(result)));
        }
'''
text = text.replace(anchor, insert, 1)
if '240..=266' not in text:
    raise SystemExit('control-map frontier anchor missing')
text = text.replace('240..=266', '240..=267', 1)
lib.write_text(text)

validator = root / 'crates/wasm-validator/src/typed.rs'
text = validator.read_text()
if '265 | 266 =>' not in text:
    raise SystemExit('validator anchor missing')
text = text.replace('265 | 266 =>', '265 | 266 | 267 =>', 1)
validator.write_text(text)

changed_frontiers = []
for path in sorted((root / 'crates/wasm-runtime/tests').glob('*.rs')):
    content = path.read_text()
    if 'subopcode: 267' not in content:
        continue
    content2 = content.replace('267', '268')
    if content2 == content:
        raise SystemExit(f'frontier replacement failed: {path}')
    path.write_text(content2)
    changed_frontiers.append(str(path))
if len(changed_frontiers) < 20:
    raise SystemExit(f'expected broad fail-closed frontier coverage, changed only {len(changed_frontiers)} files')

runtime_test = root / 'crates/wasm-runtime/tests/simd_relaxed_laneselect_i32x4.rs'
runtime_test.write_text(r'''use wasm_parser::parse_module;
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
    simd(&mut code, 267);
    simd(&mut code, 27);
    code.push(0);
    let parsed = parse_module(&module(&code)).expect("lane-select fixture parses");
    let mut instance = Instance::new(parsed).expect("lane-select fixture validates");
    match instance
        .invoke_export_values("run", &[])
        .expect("lane-select executes")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected lane-select result: {other:?}"),
    }
}

#[test]
fn relaxed_laneselect_handles_deterministic_and_mixed_masks() {
    let a = [0xaa; 16];
    let b = [0x55; 16];
    assert_eq!(execute_lane0(a, b, [0xff; 16]), -1431655766);
    assert_eq!(execute_lane0(a, b, [0; 16]), 1431655765);
    let mut mixed = [0u8; 16];
    mixed[..4].fill(0xf0);
    assert_eq!(execute_lane0(a, b, mixed), -1515870811);
}

#[test]
fn relaxed_laneselect_validates_three_v128_operands() {
    let mut code = Vec::new();
    v128_const(&mut code, [1; 16]);
    v128_const(&mut code, [2; 16]);
    code.extend_from_slice(&[0x41, 0x00]);
    simd(&mut code, 267);
    simd(&mut code, 27);
    code.push(0);
    let parsed = parse_module(&module(&code)).expect("type-confusion fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}

#[test]
fn next_relaxed_simd_subopcode_remains_fail_closed() {
    let mut code = Vec::new();
    v128_const(&mut code, [1; 16]);
    v128_const(&mut code, [2; 16]);
    v128_const(&mut code, [0xff; 16]);
    simd(&mut code, 268);
    simd(&mut code, 27);
    code.push(0);
    let parsed = parse_module(&module(&code)).expect("frontier fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::UnsupportedPrefixedOpcode {
                prefix: 0xfd,
                subopcode: 268,
                ..
            }
        ))
    ));
}
''')

diff_test = root / 'differential/tests/simd_relaxed_laneselect_i32x4.rs'
diff_test.write_text(r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "all_a") (result i32)
    v128.const i32x4 -1431655766 -1431655766 -1431655766 -1431655766
    v128.const i32x4 1431655765 1431655765 1431655765 1431655765
    v128.const i32x4 -1 -1 -1 -1
    i32x4.relaxed_laneselect
    i32x4.extract_lane 0)
  (func (export "all_b") (result i32)
    v128.const i32x4 -1431655766 -1431655766 -1431655766 -1431655766
    v128.const i32x4 1431655765 1431655765 1431655765 1431655765
    v128.const i32x4 0 0 0 0
    i32x4.relaxed_laneselect
    i32x4.extract_lane 0))
"#;
const EXPORTS: [&str; 2] = ["all_a", "all_b"];

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini runtime parses relaxed lane-select fixture");
    let mut instance =
        MiniInstance::new(module).expect("mini runtime instantiates relaxed lane-select fixture");
    EXPORTS
        .into_iter()
        .map(|export| {
            match instance
                .invoke_export_values(export, &[])
                .expect("mini lane-select executes")
                .as_slice()
            {
                [Value::I32(value)] => *value,
                other => panic!("unexpected mini result for {export}: {other:?}"),
            }
        })
        .collect()
}

fn reference_trace(bytes: &[u8]) -> Vec<i32> {
    let mut config = Config::new();
    config.wasm_simd(true);
    config.wasm_relaxed_simd(true);
    let engine = Engine::new(&config).expect("Wasmtime engine initializes");
    let module = ReferenceModule::new(&engine, bytes)
        .expect("Wasmtime compiles relaxed lane-select fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime instantiates relaxed lane-select fixture");
    EXPORTS
        .into_iter()
        .map(|export| {
            instance
                .get_typed_func::<(), i32>(&mut store, export)
                .expect("lane-select export is [] -> [i32]")
                .call(&mut store, ())
                .expect("Wasmtime lane-select executes")
        })
        .collect()
}

#[test]
fn relaxed_i32x4_laneselect_matches_wasmtime_for_deterministic_masks() {
    let bytes = wat::parse_str(FIXTURE).expect("relaxed lane-select WAT parses");
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, vec![-1431655766, 1431655765]);
    assert_eq!(reference, vec![-1431655766, 1431655765]);
    assert_eq!(mini, reference);
}
''')

print(f'updated {len(changed_frontiers)} existing frontier test files')
