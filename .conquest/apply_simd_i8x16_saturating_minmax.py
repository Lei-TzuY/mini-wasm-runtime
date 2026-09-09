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
    """        111 | 112 | 114 | 115 | 118 | 119 | 120 | 121 | 123 => {
            let rhs = numeric::v128_from_stack(stack)?;
            let lhs = numeric::v128_from_stack(stack)?;
            let mut result = [0u8; 16];
            for ((output, lhs_lane), rhs_lane) in result
                .iter_mut()
                .zip(lhs.iter().copied())
                .zip(rhs.iter().copied())
            {
                *output = match subopcode {
                    111 => (lhs_lane as i8).saturating_add(rhs_lane as i8) as u8,
                    112 => lhs_lane.saturating_add(rhs_lane),
                    114 => (lhs_lane as i8).saturating_sub(rhs_lane as i8) as u8,
                    115 => lhs_lane.saturating_sub(rhs_lane),
                    118 => (lhs_lane as i8).min(rhs_lane as i8) as u8,
                    119 => lhs_lane.min(rhs_lane),
                    120 => (lhs_lane as i8).max(rhs_lane as i8) as u8,
                    121 => lhs_lane.max(rhs_lane),
                    123 => ((u16::from(lhs_lane) + u16::from(rhs_lane) + 1) / 2) as u8,
                    _ => unreachable!("matched i8x16 saturating/min-max opcode"),
                };
            }
            stack.push(Value::V128(Rc::new(result)));
        }
        128 | 129 => {
""",
)
replace_once(
    runtime,
    "                    | 109\n                    | 128\n",
    """                    | 109
                    | 111
                    | 112
                    | 114
                    | 115
                    | 118
                    | 119
                    | 120
                    | 121
                    | 123
                    | 128
""",
)

validator = "crates/wasm-validator/src/typed.rs"
replace_once(
    validator,
    "                    139..=141 => {\n",
    """                    111 | 112 | 114 | 115 | 118 | 119 | 120 | 121 | 123 => {
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        stack.push(ValueType::V128);
                    }
                    139..=141 => {
""",
)

Path("crates/wasm-runtime/tests/simd_i8x16_saturating_minmax.rs").write_text(r'''use wasm_parser::parse_module;
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
        let sign_bit_set = byte & 0x40 != 0;
        let done = (value == 0 && !sign_bit_set) || (value == -1 && sign_bit_set);
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

fn push_splat(instructions: &mut Vec<u8>, value: i32) {
    push_i32_const(instructions, value);
    push_simd(instructions, 15); // i8x16.splat
}

fn push_extract_s(instructions: &mut Vec<u8>, lane: u8) {
    push_simd(instructions, 21); // i8x16.extract_lane_s
    instructions.push(lane);
}

fn push_extract_u(instructions: &mut Vec<u8>, lane: u8) {
    push_simd(instructions, 22); // i8x16.extract_lane_u
    instructions.push(lane);
}

fn run_i32(instructions: &[u8]) -> i32 {
    let parsed = parse_module(&module(instructions)).expect("i8x16 ALU fixture must parse");
    let mut instance = Instance::new(parsed).expect("i8x16 ALU fixture must validate");
    match instance
        .invoke_export_values("run", &[])
        .expect("i8x16 ALU fixture must execute")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected i8x16 ALU result: {other:?}"),
    }
}

#[test]
fn i8x16_signed_saturating_add_and_sub_clamp() {
    let mut add_hi = Vec::new();
    push_splat(&mut add_hi, 127);
    push_splat(&mut add_hi, 1);
    push_simd(&mut add_hi, 111);
    push_extract_s(&mut add_hi, 0);
    assert_eq!(run_i32(&add_hi), 127);

    let mut add_lo = Vec::new();
    push_splat(&mut add_lo, -128);
    push_splat(&mut add_lo, -1);
    push_simd(&mut add_lo, 111);
    push_extract_s(&mut add_lo, 15);
    assert_eq!(run_i32(&add_lo), -128);

    let mut sub_lo = Vec::new();
    push_splat(&mut sub_lo, -128);
    push_splat(&mut sub_lo, 1);
    push_simd(&mut sub_lo, 114);
    push_extract_s(&mut sub_lo, 7);
    assert_eq!(run_i32(&sub_lo), -128);

    let mut sub_hi = Vec::new();
    push_splat(&mut sub_hi, 127);
    push_splat(&mut sub_hi, -1);
    push_simd(&mut sub_hi, 114);
    push_extract_s(&mut sub_hi, 4);
    assert_eq!(run_i32(&sub_hi), 127);
}

#[test]
fn i8x16_unsigned_saturating_add_and_sub_clamp() {
    let mut add = Vec::new();
    push_splat(&mut add, 255);
    push_splat(&mut add, 1);
    push_simd(&mut add, 112);
    push_extract_u(&mut add, 3);
    assert_eq!(run_i32(&add), 255);

    let mut sub = Vec::new();
    push_splat(&mut sub, 0);
    push_splat(&mut sub, 1);
    push_simd(&mut sub, 115);
    push_extract_u(&mut sub, 12);
    assert_eq!(run_i32(&sub), 0);
}

#[test]
fn i8x16_min_max_and_average_observe_lane_signedness() {
    let cases = [
        (118, true, -1, 1, -1),
        (120, true, -1, 1, 1),
        (119, false, 255, 1, 1),
        (121, false, 255, 1, 255),
        (123, false, 10, 13, 12),
    ];

    for (opcode, signed, lhs, rhs, expected) in cases {
        let mut instructions = Vec::new();
        push_splat(&mut instructions, lhs);
        push_splat(&mut instructions, rhs);
        push_simd(&mut instructions, opcode);
        if signed {
            push_extract_s(&mut instructions, 5);
        } else {
            push_extract_u(&mut instructions, 5);
        }
        assert_eq!(run_i32(&instructions), expected, "subopcode {opcode}");
    }
}

#[test]
fn i8x16_alu_executes_inside_structured_control() {
    let mut instructions = vec![0x02, 0x7f]; // block (result i32)
    push_splat(&mut instructions, 100);
    push_splat(&mut instructions, 40);
    push_simd(&mut instructions, 120); // i8x16.max_s
    push_extract_s(&mut instructions, 9);
    instructions.push(0x0b);
    assert_eq!(run_i32(&instructions), 100);
}

#[test]
fn validator_rejects_i8x16_alu_type_confusion() {
    let mut instructions = Vec::new();
    push_splat(&mut instructions, 1);
    push_i32_const(&mut instructions, 2); // wrong rhs type
    push_simd(&mut instructions, 111);
    push_extract_u(&mut instructions, 0);

    let parsed = parse_module(&module(&instructions)).expect("type-confusion fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}

#[test]
fn adjacent_i8x16_wrapping_add_remains_fail_closed() {
    let mut instructions = Vec::new();
    push_splat(&mut instructions, 1);
    push_splat(&mut instructions, 2);
    push_simd(&mut instructions, 110); // i8x16.add remains outside this slice
    push_extract_u(&mut instructions, 0);

    let parsed = parse_module(&module(&instructions)).expect("unsupported-SIMD fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::UnsupportedPrefixedOpcode {
                prefix: 0xfd,
                subopcode: 110,
                ..
            }
        ))
    ));
}
''')

Path("differential/tests/simd_i8x16_saturating_minmax.rs").write_text(r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "add_sat_s") (result i32)
    i32.const 127 i8x16.splat i32.const 1 i8x16.splat
    i8x16.add_sat_s i8x16.extract_lane_s 0)
  (func (export "sub_sat_s") (result i32)
    i32.const -128 i8x16.splat i32.const 1 i8x16.splat
    i8x16.sub_sat_s i8x16.extract_lane_s 0)
  (func (export "add_sat_u") (result i32)
    i32.const 255 i8x16.splat i32.const 1 i8x16.splat
    i8x16.add_sat_u i8x16.extract_lane_u 0)
  (func (export "sub_sat_u") (result i32)
    i32.const 0 i8x16.splat i32.const 1 i8x16.splat
    i8x16.sub_sat_u i8x16.extract_lane_u 0)
  (func (export "min_s") (result i32)
    i32.const -1 i8x16.splat i32.const 1 i8x16.splat
    i8x16.min_s i8x16.extract_lane_s 0)
  (func (export "max_s") (result i32)
    i32.const -1 i8x16.splat i32.const 1 i8x16.splat
    i8x16.max_s i8x16.extract_lane_s 0)
  (func (export "min_u") (result i32)
    i32.const 255 i8x16.splat i32.const 1 i8x16.splat
    i8x16.min_u i8x16.extract_lane_u 0)
  (func (export "max_u") (result i32)
    i32.const 255 i8x16.splat i32.const 1 i8x16.splat
    i8x16.max_u i8x16.extract_lane_u 0)
  (func (export "avgr_u") (result i32)
    i32.const 10 i8x16.splat i32.const 13 i8x16.splat
    i8x16.avgr_u i8x16.extract_lane_u 0))
"#;

const EXPORTS: [&str; 9] = [
    "add_sat_s",
    "sub_sat_s",
    "add_sat_u",
    "sub_sat_u",
    "min_s",
    "max_s",
    "min_u",
    "max_u",
    "avgr_u",
];

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini must parse i8x16 ALU fixture");
    let mut instance = MiniInstance::new(module).expect("mini must instantiate i8x16 ALU fixture");
    EXPORTS
        .into_iter()
        .map(|export| {
            match instance
                .invoke_export_values(export, &[])
                .expect("mini execution")
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
    let engine = Engine::new(&config).expect("SIMD Wasmtime engine");
    let module = ReferenceModule::new(&engine, bytes).expect("Wasmtime compile");
    let mut store = Store::new(&engine, ());
    let instance =
        ReferenceInstance::new(&mut store, &module, &[]).expect("Wasmtime instantiate");
    EXPORTS
        .into_iter()
        .map(|export| {
            instance
                .get_typed_func::<(), i32>(&mut store, export)
                .expect("signature")
                .call(&mut store, ())
                .expect("reference execution")
        })
        .collect()
}

#[test]
fn i8x16_saturating_minmax_matches_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("fixture parse");
    let expected = vec![127, -128, 255, 0, -1, 1, 1, 255, 12];
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
entry = "- [x] SIMD `i8x16` signed/unsigned saturating add/sub, min/max, and unsigned rounded average with exact `v128, v128 -> v128` validation, byte-lane semantics, structured-control scanning, focused fail-closed regressions, and Wasmtime differential evidence\n"
roadmap.write_text(text.replace(anchor, entry + anchor, 1))
