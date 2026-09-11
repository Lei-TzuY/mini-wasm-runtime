from pathlib import Path

runtime = Path("crates/wasm-runtime/src/lib.rs")
text = runtime.read_text()
marker = "        246..=247 => {\n"
if marker not in text:
    raise SystemExit("runtime insertion marker missing")
insert = r'''        248..=251 => {
            let value = numeric::v128_from_stack(stack)?;
            let mut result = [0u8; 16];
            for lane in 0..4 {
                let start = lane * 4;
                match subopcode {
                    248 => {
                        let input = f32::from_bits(u32::from_le_bytes(
                            value[start..start + 4]
                                .try_into()
                                .expect("f32x4 lane width"),
                        ));
                        result[start..start + 4].copy_from_slice(&(input as i32).to_le_bytes());
                    }
                    249 => {
                        let input = f32::from_bits(u32::from_le_bytes(
                            value[start..start + 4]
                                .try_into()
                                .expect("f32x4 lane width"),
                        ));
                        result[start..start + 4].copy_from_slice(&(input as u32).to_le_bytes());
                    }
                    250 => {
                        let input = i32::from_le_bytes(
                            value[start..start + 4]
                                .try_into()
                                .expect("i32x4 lane width"),
                        );
                        result[start..start + 4]
                            .copy_from_slice(&(input as f32).to_bits().to_le_bytes());
                    }
                    251 => {
                        let input = u32::from_le_bytes(
                            value[start..start + 4]
                                .try_into()
                                .expect("i32x4 lane width"),
                        );
                        result[start..start + 4]
                            .copy_from_slice(&(input as f32).to_bits().to_le_bytes());
                    }
                    _ => unreachable!("matched f32x4 conversion opcode"),
                }
            }
            stack.push(Value::V128(Rc::new(result)));
        }
        252..=255 => {
            let value = numeric::v128_from_stack(stack)?;
            let mut result = [0u8; 16];
            match subopcode {
                252 | 253 => {
                    for lane in 0..2 {
                        let input_start = lane * 8;
                        let output_start = lane * 4;
                        let input = f64::from_bits(u64::from_le_bytes(
                            value[input_start..input_start + 8]
                                .try_into()
                                .expect("f64x2 lane width"),
                        ));
                        let output = if subopcode == 252 {
                            (input as i32).to_le_bytes()
                        } else {
                            (input as u32).to_le_bytes()
                        };
                        result[output_start..output_start + 4].copy_from_slice(&output);
                    }
                }
                254 | 255 => {
                    for lane in 0..2 {
                        let input_start = lane * 4;
                        let output_start = lane * 8;
                        let output = if subopcode == 254 {
                            let input = i32::from_le_bytes(
                                value[input_start..input_start + 4]
                                    .try_into()
                                    .expect("i32x4 lane width"),
                            );
                            (input as f64).to_bits()
                        } else {
                            let input = u32::from_le_bytes(
                                value[input_start..input_start + 4]
                                    .try_into()
                                    .expect("i32x4 lane width"),
                            );
                            (input as f64).to_bits()
                        };
                        result[output_start..output_start + 8]
                            .copy_from_slice(&output.to_le_bytes());
                    }
                }
                _ => unreachable!("matched terminal SIMD conversion opcode"),
            }
            stack.push(Value::V128(Rc::new(result)));
        }
'''
text = text.replace(marker, insert + marker, 1)
old_scan = "                    | 240..=247\n"
if old_scan not in text:
    raise SystemExit("control-map SIMD frontier marker missing")
text = text.replace(old_scan, "                    | 240..=255\n", 1)
runtime.write_text(text)

validator = Path("crates/wasm-validator/src/typed.rs")
text = validator.read_text()
old = "                    96 | 97 | 98 | 128 | 129 | 224 | 225 | 227 | 236 | 237 | 239 => {\n"
if old not in text:
    raise SystemExit("validator unary SIMD marker missing")
new = "                    96 | 97 | 98 | 128 | 129 | 224 | 225 | 227 | 236 | 237 | 239\n                    | 248..=255 => {\n"
text = text.replace(old, new, 1)
validator.write_text(text)

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

fn push_i32_const(bytes: &mut Vec<u8>, mut value: i32) {
    bytes.push(0x41);
    loop {
        let byte = (value as u8) & 0x7f;
        value >>= 7;
        let sign = byte & 0x40 != 0;
        let done = (value == 0 && !sign) || (value == -1 && sign);
        bytes.push(if done { byte } else { byte | 0x80 });
        if done {
            break;
        }
    }
}

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    push_u32(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn module(result_type: u8, instructions: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0, 0, 0];
    push_section(&mut bytes, 1, &[1, 0x60, 0, 1, result_type]);
    push_section(&mut bytes, 3, &[1, 0]);
    push_section(&mut bytes, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);
    let mut body = vec![0];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code = vec![1];
    push_u32(&mut code, body.len() as u32);
    code.extend(body);
    push_section(&mut bytes, 10, &code);
    bytes
}

fn simd(bytes: &mut Vec<u8>, subopcode: u32) {
    bytes.push(0xfd);
    push_u32(bytes, subopcode);
}

fn v128_const(bytes: &mut Vec<u8>, value: [u8; 16]) {
    simd(bytes, 12);
    bytes.extend_from_slice(&value);
}

fn run_v128(instructions: &[u8]) -> [u8; 16] {
    let parsed = parse_module(&module(0x7b, instructions)).expect("SIMD conversion fixture parses");
    let mut instance = Instance::new(parsed).expect("SIMD conversion fixture validates");
    match instance
        .invoke_export_values("run", &[])
        .expect("SIMD conversion fixture executes")
        .as_slice()
    {
        [Value::V128(value)] => **value,
        other => panic!("unexpected SIMD conversion result: {other:?}"),
    }
}

fn f32x4(values: [f32; 4]) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    for (lane, value) in values.into_iter().enumerate() {
        let start = lane * 4;
        bytes[start..start + 4].copy_from_slice(&value.to_bits().to_le_bytes());
    }
    bytes
}

fn f64x2(values: [f64; 2]) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    for (lane, value) in values.into_iter().enumerate() {
        let start = lane * 8;
        bytes[start..start + 8].copy_from_slice(&value.to_bits().to_le_bytes());
    }
    bytes
}

fn i32x4(values: [i32; 4]) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    for (lane, value) in values.into_iter().enumerate() {
        let start = lane * 4;
        bytes[start..start + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn u32x4(values: [u32; 4]) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    for (lane, value) in values.into_iter().enumerate() {
        let start = lane * 4;
        bytes[start..start + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn read_i32x4(bytes: [u8; 16]) -> [i32; 4] {
    std::array::from_fn(|lane| {
        let start = lane * 4;
        i32::from_le_bytes(bytes[start..start + 4].try_into().unwrap())
    })
}

fn read_u32x4(bytes: [u8; 16]) -> [u32; 4] {
    std::array::from_fn(|lane| {
        let start = lane * 4;
        u32::from_le_bytes(bytes[start..start + 4].try_into().unwrap())
    })
}

fn read_f32x4(bytes: [u8; 16]) -> [f32; 4] {
    std::array::from_fn(|lane| {
        let start = lane * 4;
        f32::from_bits(u32::from_le_bytes(
            bytes[start..start + 4].try_into().unwrap(),
        ))
    })
}

fn read_f64x2(bytes: [u8; 16]) -> [f64; 2] {
    std::array::from_fn(|lane| {
        let start = lane * 8;
        f64::from_bits(u64::from_le_bytes(
            bytes[start..start + 8].try_into().unwrap(),
        ))
    })
}

#[test]
fn f32x4_trunc_sat_signed_and_unsigned_match_wasm_saturation() {
    let mut signed = Vec::new();
    v128_const(&mut signed, f32x4([f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 42.9]));
    simd(&mut signed, 248);
    assert_eq!(read_i32x4(run_v128(&signed)), [0, i32::MAX, i32::MIN, 42]);

    let mut unsigned = Vec::new();
    v128_const(&mut unsigned, f32x4([-1.0, f32::NAN, f32::INFINITY, 42.9]));
    simd(&mut unsigned, 249);
    assert_eq!(read_u32x4(run_v128(&unsigned)), [0, 0, u32::MAX, 42]);
}

#[test]
fn f32x4_integer_conversions_preserve_signed_and_unsigned_interpretation() {
    let mut signed = Vec::new();
    v128_const(&mut signed, i32x4([-16_777_216, -7, 0, 16_777_216]));
    simd(&mut signed, 250);
    assert_eq!(read_f32x4(run_v128(&signed)), [-16_777_216.0, -7.0, 0.0, 16_777_216.0]);

    let mut unsigned = Vec::new();
    v128_const(&mut unsigned, u32x4([0, 7, 16_777_216, u32::MAX]));
    simd(&mut unsigned, 251);
    let actual = read_f32x4(run_v128(&unsigned));
    assert_eq!(actual[0..3], [0.0, 7.0, 16_777_216.0]);
    assert_eq!(actual[3], u32::MAX as f32);
}

#[test]
fn f64x2_trunc_sat_zeroes_upper_i32_lanes() {
    let mut signed = Vec::new();
    v128_const(&mut signed, f64x2([f64::NEG_INFINITY, 19.75]));
    simd(&mut signed, 252);
    assert_eq!(read_i32x4(run_v128(&signed)), [i32::MIN, 19, 0, 0]);

    let mut unsigned = Vec::new();
    v128_const(&mut unsigned, f64x2([f64::NAN, f64::INFINITY]));
    simd(&mut unsigned, 253);
    assert_eq!(read_u32x4(run_v128(&unsigned)), [0, u32::MAX, 0, 0]);
}

#[test]
fn f64x2_convert_low_uses_only_low_i32x4_lanes() {
    let mut signed = Vec::new();
    v128_const(&mut signed, i32x4([-9, 17, 1234, -5678]));
    simd(&mut signed, 254);
    assert_eq!(read_f64x2(run_v128(&signed)), [-9.0, 17.0]);

    let mut unsigned = Vec::new();
    v128_const(&mut unsigned, u32x4([u32::MAX, 17, 1234, 5678]));
    simd(&mut unsigned, 255);
    assert_eq!(read_f64x2(run_v128(&unsigned)), [u32::MAX as f64, 17.0]);
}

#[test]
fn conversion_validates_and_scans_structured_control() {
    let mut instructions = vec![0x02, 0x7f];
    v128_const(&mut instructions, f32x4([5.9, 0.0, 0.0, 0.0]));
    simd(&mut instructions, 248);
    simd(&mut instructions, 27);
    instructions.push(0);
    instructions.push(0x0b);
    let parsed = parse_module(&module(0x7f, &instructions)).expect("structured fixture parses");
    let mut instance = Instance::new(parsed).expect("structured fixture validates");
    assert_eq!(instance.invoke_export("run", &[]).unwrap(), Some(Value::I32(5)));

    let mut bad = Vec::new();
    push_i32_const(&mut bad, 1);
    simd(&mut bad, 248);
    let parsed = parse_module(&module(0x7b, &bad)).expect("type-confusion fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(ValidationError::TypeMismatch { .. }))
    ));
}

#[test]
fn relaxed_simd_frontier_remains_fail_closed_at_256() {
    let mut instructions = Vec::new();
    v128_const(&mut instructions, [0; 16]);
    simd(&mut instructions, 256);
    let parsed = parse_module(&module(0x7b, &instructions)).expect("frontier fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::UnsupportedPrefixedOpcode {
                prefix: 0xfd,
                subopcode: 256,
                ..
            }
        ))
    ));
}
'''
Path("crates/wasm-runtime/tests/simd_conversions.rs").write_text(runtime_test)

differential = r'''use wasm_runtime::{Instance, Value};
use wasmtime::{Engine, Instance as WasmtimeInstance, Module as WasmtimeModule, Store};

const FIXTURE: &str = r#"(module
  (memory 1)
  (func (export "trunc_s") (result i32)
    i32.const 0 v128.const f32x4 42.9 nan inf -inf i32x4.trunc_sat_f32x4_s v128.store
    i32.const 0 i32.load)
  (func (export "trunc_u") (result i32)
    i32.const 0 v128.const f32x4 -1 42.9 nan inf i32x4.trunc_sat_f32x4_u v128.store
    i32.const 4 i32.load)
  (func (export "convert_s_bits") (result i32)
    i32.const 0 v128.const i32x4 -7 9 0 1 f32x4.convert_i32x4_s v128.store
    i32.const 0 i32.load)
  (func (export "trunc_f64_zero") (result i32)
    i32.const 0 v128.const f64x2 -19.75 33.5 i32x4.trunc_sat_f64x2_s_zero v128.store
    i32.const 8 i32.load)
  (func (export "convert_low_u_bits") (result i64)
    i32.const 0 v128.const i32x4 4294967295 17 9 11 f64x2.convert_low_i32x4_u v128.store
    i32.const 0 i64.load))"#;

#[test]
fn terminal_simd_conversions_match_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("wat");
    let parsed = wasm_parser::parse_module(&bytes).expect("parse");
    let mut mini = Instance::new(parsed).expect("mini");

    let mini_i32 = ["trunc_s", "trunc_u", "convert_s_bits", "trunc_f64_zero"]
        .map(|name| match mini.invoke_export_values(name, &[]).unwrap().as_slice() {
            [Value::I32(value)] => *value,
            other => panic!("unexpected mini i32 result for {name}: {other:?}"),
        });
    let mini_i64 = match mini
        .invoke_export_values("convert_low_u_bits", &[])
        .unwrap()
        .as_slice()
    {
        [Value::I64(value)] => *value,
        other => panic!("unexpected mini i64 result: {other:?}"),
    };

    let engine = Engine::default();
    let module = WasmtimeModule::new(&engine, &bytes).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = WasmtimeInstance::new(&mut store, &module, &[]).unwrap();
    let reference_i32 = ["trunc_s", "trunc_u", "convert_s_bits", "trunc_f64_zero"].map(|name| {
        instance
            .get_typed_func::<(), i32>(&mut store, name)
            .unwrap()
            .call(&mut store, ())
            .unwrap()
    });
    let reference_i64 = instance
        .get_typed_func::<(), i64>(&mut store, "convert_low_u_bits")
        .unwrap()
        .call(&mut store, ())
        .unwrap();

    assert_eq!(mini_i32, reference_i32);
    assert_eq!(mini_i64, reference_i64);
}
'''
Path("differential/tests/simd_conversions.rs").write_text(differential)

roadmap = Path("docs/roadmap.md")
roadmap_text = roadmap.read_text()
entry = "- SIMD terminal conversions (subopcodes 248-255) are executable across saturating f32/f64-to-i32 lanes, signed/unsigned integer-to-float lanes, the f64x2 low-lane conversion forms, typed validation, structured-control scanning, focused regressions, and Wasmtime differential coverage; relaxed-SIMD subopcode 256 remains fail-closed.\n"
if entry not in roadmap_text:
    roadmap.write_text(roadmap_text.rstrip() + "\n\n" + entry)
