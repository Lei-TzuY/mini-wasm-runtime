from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    if text.count(old) != 1:
        raise SystemExit(f"expected exactly one anchor in {path}: {old!r}, got {text.count(old)}")
    p.write_text(text.replace(old, new, 1))


runtime_arm = '''        171..=173 => {
            let shift = (numeric::i32_from_stack(stack)? as u32) & 31;
            let value = numeric::v128_from_stack(stack)?;
            let mut result = [0u8; 16];
            for (output, lane_bytes) in result.chunks_exact_mut(4).zip(value.chunks_exact(4)) {
                let lane_unsigned = u32::from_le_bytes([
                    lane_bytes[0],
                    lane_bytes[1],
                    lane_bytes[2],
                    lane_bytes[3],
                ]);
                let lane = match subopcode {
                    171 => lane_unsigned.wrapping_shl(shift),
                    172 => ((lane_unsigned as i32) >> shift) as u32,
                    173 => lane_unsigned >> shift,
                    _ => unreachable!("matched i32x4 shift opcode"),
                };
                output.copy_from_slice(&lane.to_le_bytes());
            }
            stack.push(Value::V128(Rc::new(result)));
        }
'''
replace_once(
    "crates/wasm-runtime/src/lib.rs",
    "        167..=170 => {\n            let value = numeric::v128_from_stack(stack)?;",
    runtime_arm + "        167..=170 => {\n            let value = numeric::v128_from_stack(stack)?;",
)
replace_once(
    "crates/wasm-runtime/src/lib.rs",
    "                    | 167..=170\n",
    "                    | 167..=170\n                    | 171..=173\n",
)

validator_arm = '''                    171..=173 => {
                        pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        stack.push(ValueType::V128);
                    }
'''
replace_once(
    "crates/wasm-validator/src/typed.rs",
    "                    135..=138 | 167..=170 => {",
    validator_arm + "                    135..=138 | 167..=170 => {",
)

widening = Path("crates/wasm-runtime/tests/simd_i32x4_widening.rs")
text = widening.read_text()
text = text.replace("fn adjacent_i32x4_shift_remains_fail_closed()", "fn adjacent_i64x2_shift_remains_fail_closed()")
text = text.replace("push_simd(&mut instructions, 171); // i32x4.shl remains outside this slice", "push_simd(&mut instructions, 203); // i64x2.shl remains outside this slice")
text = text.replace("subopcode: 171,", "subopcode: 203,")
widening.write_text(text)

Path("crates/wasm-runtime/tests/simd_i32x4_shifts.rs").write_text(r'''use wasm_parser::parse_module;
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

fn push_i32x4_const(instructions: &mut Vec<u8>, lanes: [i32; 4]) {
    push_simd(instructions, 12);
    for lane in lanes {
        instructions.extend_from_slice(&lane.to_le_bytes());
    }
}

fn push_i32x4_extract(instructions: &mut Vec<u8>, lane: u8) {
    push_simd(instructions, 27);
    instructions.push(lane);
}

fn run_i32(instructions: &[u8]) -> i32 {
    let parsed = parse_module(&module(instructions)).expect("i32x4 shift fixture must parse");
    let mut instance = Instance::new(parsed).expect("i32x4 shift fixture must validate");
    match instance
        .invoke_export_values("run", &[])
        .expect("i32x4 shift fixture must execute")
        .as_slice()
    {
        [Value::I32(value)] => *value,
        other => panic!("unexpected i32x4 shift result: {other:?}"),
    }
}

#[test]
fn i32x4_shift_family_masks_counts_and_preserves_signedness() {
    let mut shl = Vec::new();
    push_i32x4_const(&mut shl, [0x4000_0000, 0, 0, 0]);
    push_i32_const(&mut shl, 33);
    push_simd(&mut shl, 171);
    push_i32x4_extract(&mut shl, 0);
    assert_eq!(run_i32(&shl), i32::MIN);

    let mut shr_s = Vec::new();
    push_i32x4_const(&mut shr_s, [-2, 0, 0, 0]);
    push_i32_const(&mut shr_s, 1);
    push_simd(&mut shr_s, 172);
    push_i32x4_extract(&mut shr_s, 0);
    assert_eq!(run_i32(&shr_s), -1);

    let mut shr_u = Vec::new();
    push_i32x4_const(&mut shr_u, [i32::MIN, 0, 0, 0]);
    push_i32_const(&mut shr_u, 1);
    push_simd(&mut shr_u, 173);
    push_i32x4_extract(&mut shr_u, 0);
    assert_eq!(run_i32(&shr_u), 0x4000_0000);
}

#[test]
fn i32x4_shifts_execute_inside_structured_control() {
    let mut instructions = vec![0x02, 0x7f];
    push_i32x4_const(&mut instructions, [1, 0, 0, 0]);
    push_i32_const(&mut instructions, 3);
    push_simd(&mut instructions, 171);
    push_i32x4_extract(&mut instructions, 0);
    instructions.push(0x0b);
    assert_eq!(run_i32(&instructions), 8);
}

#[test]
fn validator_rejects_i32x4_shift_type_confusion() {
    let mut instructions = Vec::new();
    push_i32_const(&mut instructions, 0);
    push_i32_const(&mut instructions, 1);
    push_simd(&mut instructions, 171);
    push_i32x4_extract(&mut instructions, 0);
    let parsed = parse_module(&module(&instructions)).expect("type-confusion fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(ValidationError::TypeMismatch { .. }))
    ));
}

#[test]
fn adjacent_i64x2_shift_remains_fail_closed() {
    let mut instructions = Vec::new();
    push_i32x4_const(&mut instructions, [1, 0, 0, 0]);
    push_i32_const(&mut instructions, 1);
    push_simd(&mut instructions, 203);
    push_i32x4_extract(&mut instructions, 0);
    let parsed = parse_module(&module(&instructions)).expect("unsupported-SIMD fixture must parse");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::UnsupportedPrefixedOpcode {
                prefix: 0xfd,
                subopcode: 203,
                ..
            }
        ))
    ));
}
''')

Path("differential/tests/simd_i32x4_shifts.rs").write_text(r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "shl") (result i32)
    i32.const 1073741824 i32x4.splat i32.const 33 i32x4.shl i32x4.extract_lane 0)
  (func (export "shr_s") (result i32)
    i32.const -2 i32x4.splat i32.const 1 i32x4.shr_s i32x4.extract_lane 0)
  (func (export "shr_u") (result i32)
    i32.const -2147483648 i32x4.splat i32.const 1 i32x4.shr_u i32x4.extract_lane 0))
"#;
const EXPORTS: [&str; 3] = ["shl", "shr_s", "shr_u"];

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini must parse i32x4 shift fixture");
    let mut instance = MiniInstance::new(module).expect("mini must instantiate i32x4 shift fixture");
    EXPORTS
        .into_iter()
        .map(|export| match instance
            .invoke_export_values(export, &[])
            .expect("mini execution")
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
    let engine = Engine::new(&config).expect("SIMD Wasmtime engine");
    let module = ReferenceModule::new(&engine, bytes).expect("Wasmtime compile");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[]).expect("Wasmtime instantiate");
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
fn i32x4_shifts_match_wasmtime_reference() {
    let bytes = wat::parse_str(FIXTURE).expect("fixture parse");
    let expected = vec![i32::MIN, -1, 0x4000_0000];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, expected);
    assert_eq!(reference, expected);
    assert_eq!(mini, reference);
}
''')

roadmap = Path("docs/roadmap.md")
text = roadmap.read_text()
anchor = "- SIMD cross-width widening is executable for `i16x8.extend_low/high_i8x16_{s,u}`, with signed/unsigned low/high lane semantics, typed validation, structured-control scanning, regressions, and Wasmtime differential coverage."
if anchor not in text:
    raise SystemExit("roadmap SIMD widening anchor missing")
line = "\n- SIMD `i32x4.{shl,shr_s,shr_u}` shifts are executable with modulo-32 shift-count masking, signed/unsigned right-shift semantics, typed validation, structured-control scanning, regressions, and Wasmtime differential coverage."
roadmap.write_text(text.replace(anchor, anchor + line, 1))
