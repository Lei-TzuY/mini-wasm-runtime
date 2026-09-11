from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected exactly one match in {path}, got {count}: {old[:80]!r}")
    p.write_text(text.replace(old, new, 1))


runtime = "crates/wasm-runtime/src/lib.rs"
validator = "crates/wasm-validator/src/typed.rs"

old_runtime = '''        14 => {
            let indices = numeric::v128_from_stack(stack)?;
            let input = numeric::v128_from_stack(stack)?;
            let mut result = [0u8; 16];
            for index in 0..16 {
                let lane = indices[index];
                result[index] = if lane < 16 {
                    input[usize::from(lane)]
                } else {
                    0
                };
            }
            stack.push(Value::V128(Rc::new(result)));
        }
'''
new_runtime = old_runtime + '''        256 => {
            // Relaxed swizzle permits implementation-defined results for selectors 16..=127,
            // while selectors >= 128 must produce zero. Choosing zero for every selector >= 16
            // is a deterministic lowering that is valid on every host.
            let indices = numeric::v128_from_stack(stack)?;
            let input = numeric::v128_from_stack(stack)?;
            let mut result = [0u8; 16];
            for index in 0..16 {
                let lane = indices[index];
                result[index] = if lane < 16 {
                    input[usize::from(lane)]
                } else {
                    0
                };
            }
            stack.push(Value::V128(Rc::new(result)));
        }
'''
replace_once(runtime, old_runtime, new_runtime)
replace_once(runtime, "                    | 240..=255\n", "                    | 240..=256\n")
replace_once(
    validator,
    '''                    14 => {
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        stack.push(ValueType::V128);
                    }
''',
    '''                    14 | 256 => {
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        stack.push(ValueType::V128);
                    }
''',
)

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

fn push_v128_const(code: &mut Vec<u8>, lanes: [u8; 16]) {
    code.extend_from_slice(&[0xfd, 0x0c]);
    code.extend_from_slice(&lanes);
}

fn simd(code: &mut Vec<u8>, subopcode: u32) {
    code.push(0xfd);
    push_u32(code, subopcode);
}

fn run_i32(code: &[u8]) -> i32 {
    let parsed = parse_module(&module(code)).expect("relaxed swizzle fixture must parse");
    let mut instance = Instance::new(parsed).expect("relaxed swizzle fixture must validate");
    match instance.invoke_export_values("run", &[]).expect("relaxed swizzle must execute").as_slice() {
        [Value::I32(value)] => *value,
        other => panic!("unexpected relaxed swizzle result: {other:?}"),
    }
}

fn table() -> [u8; 16] {
    [10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25]
}

#[test]
fn relaxed_swizzle_selects_in_range_lanes_and_zeroes_high_indices() {
    let mut code = Vec::new();
    push_v128_const(&mut code, table());
    push_v128_const(&mut code, [15, 16, 31, 127, 128, 255, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    simd(&mut code, 256);
    code.extend_from_slice(&[0xfd, 0x16, 0x00]);
    assert_eq!(run_i32(&code), 25);

    let mut high = Vec::new();
    push_v128_const(&mut high, table());
    push_v128_const(&mut high, [128, 255, 16, 31, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
    simd(&mut high, 256);
    high.extend_from_slice(&[0xfd, 0x16, 0x00]);
    assert_eq!(run_i32(&high), 0);
}

#[test]
fn relaxed_swizzle_executes_inside_structured_control() {
    let mut code = vec![0x02, 0x7f];
    push_v128_const(&mut code, table());
    push_v128_const(&mut code, [3, 0, 1, 2, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
    simd(&mut code, 256);
    code.extend_from_slice(&[0xfd, 0x16, 0x00, 0x0b]);
    assert_eq!(run_i32(&code), 13);
}

#[test]
fn validator_rejects_relaxed_swizzle_type_confusion() {
    let mut code = Vec::new();
    push_v128_const(&mut code, table());
    code.extend_from_slice(&[0x41, 0x00]);
    simd(&mut code, 256);
    code.extend_from_slice(&[0xfd, 0x16, 0x00]);
    let parsed = parse_module(&module(&code)).expect("type-confusion fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(ValidationError::TypeMismatch { .. }))
    ));
}

#[test]
fn next_relaxed_simd_subopcode_remains_fail_closed() {
    let mut code = Vec::new();
    push_v128_const(&mut code, table());
    push_v128_const(&mut code, table());
    simd(&mut code, 257);
    code.extend_from_slice(&[0xfd, 0x16, 0x00]);
    let parsed = parse_module(&module(&code)).expect("257 frontier fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::UnsupportedPrefixedOpcode {
                prefix: 0xfd,
                subopcode: 257,
                ..
            }
        ))
    ));
}
'''
Path("crates/wasm-runtime/tests/simd_relaxed_swizzle.rs").write_text(runtime_test)

differential = r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "in_range") (result i32)
    v128.const i8x16 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25
    v128.const i8x16 15 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14
    i8x16.relaxed_swizzle
    i8x16.extract_lane_u 0)
  (func (export "high_oob") (result i32)
    v128.const i8x16 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25
    v128.const i8x16 -128 -1 0 1 2 3 4 5 6 7 8 9 10 11 12 13
    i8x16.relaxed_swizzle
    i8x16.extract_lane_u 0))
"#;

const EXPORTS: [&str; 2] = ["in_range", "high_oob"];

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini runtime must parse relaxed swizzle fixture");
    let mut instance = MiniInstance::new(module).expect("mini runtime must instantiate relaxed swizzle fixture");
    EXPORTS.into_iter().map(|export| {
        match instance.invoke_export_values(export, &[]).expect("mini relaxed swizzle execution must succeed").as_slice() {
            [Value::I32(value)] => *value,
            other => panic!("unexpected mini result for {export}: {other:?}"),
        }
    }).collect()
}

fn reference_trace(bytes: &[u8]) -> Vec<i32> {
    let mut config = Config::new();
    config.wasm_simd(true);
    config.wasm_relaxed_simd(true);
    let engine = Engine::new(&config).expect("relaxed-SIMD Wasmtime engine must initialize");
    let module = ReferenceModule::new(&engine, bytes).expect("Wasmtime must compile relaxed swizzle fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[]).expect("Wasmtime must instantiate relaxed swizzle fixture");
    EXPORTS.into_iter().map(|export| {
        instance.get_typed_func::<(), i32>(&mut store, export).expect("relaxed swizzle export must be [] -> [i32]").call(&mut store, ()).expect("Wasmtime relaxed swizzle execution must succeed")
    }).collect()
}

#[test]
fn relaxed_swizzle_matches_wasmtime_on_deterministic_lanes() {
    let bytes = wat::parse_str(FIXTURE).expect("relaxed swizzle WAT fixture must parse");
    let expected = vec![25, 0];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, expected, "mini relaxed swizzle deterministic lanes drifted");
    assert_eq!(reference, expected, "Wasmtime relaxed swizzle deterministic lanes drifted");
    assert_eq!(mini, reference, "relaxed swizzle deterministic traces diverged");
}
'''
Path("differential/tests/simd_relaxed_swizzle.rs").write_text(differential)

roadmap = Path("docs/roadmap.md")
roadmap_text = roadmap.read_text()
needle = "- SIMD terminal conversions (subopcodes 248-255) are executable across saturating f32/f64-to-i32 lanes, signed/unsigned i32-to-f32/f64 lanes, structured-control scanning, focused regressions, and Wasmtime differential coverage; relaxed-SIMD subopcode 256 remains fail-closed."
replacement = needle.replace("; relaxed-SIMD subopcode 256 remains fail-closed.", ".\n- Relaxed SIMD `i8x16.relaxed_swizzle` (subopcode 256) is executable with the portable deterministic zero-on-out-of-range lowering, typed validation, structured-control coverage, deterministic-lane Wasmtime differential evidence, and subopcode 257 retained as the fail-closed frontier.")
if needle not in roadmap_text:
    raise SystemExit("roadmap frontier marker not found")
roadmap.write_text(roadmap_text.replace(needle, replacement, 1))
