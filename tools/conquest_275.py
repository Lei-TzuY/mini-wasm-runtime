from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one match, found {count}: {old[:80]!r}")
    path.write_text(text.replace(old, new, 1))


runtime = ROOT / "crates/wasm-runtime/src/lib.rs"
insert = '''        275 => {
            // Deterministic Relaxed SIMD profile: interpret both byte vectors as
            // signed, saturate adjacent byte dot products to i16, pairwise-add
            // them to i32, then wrap-add the i32x4 accumulator.
            let addend = numeric::v128_from_stack(stack)?;
            let rhs = numeric::v128_from_stack(stack)?;
            let lhs = numeric::v128_from_stack(stack)?;
            let mut result = [0u8; 16];
            for lane in 0..4 {
                let byte = lane * 4;
                let pair_dot = |offset: usize| -> i16 {
                    let lhs0 = i32::from(lhs[byte + offset] as i8);
                    let lhs1 = i32::from(lhs[byte + offset + 1] as i8);
                    let rhs0 = i32::from(rhs[byte + offset] as i8);
                    let rhs1 = i32::from(rhs[byte + offset + 1] as i8);
                    (lhs0 * rhs0 + lhs1 * rhs1)
                        .clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
                };
                let dot = i32::from(pair_dot(0)) + i32::from(pair_dot(2));
                let accumulator = i32::from_le_bytes(
                    addend[byte..byte + 4]
                        .try_into()
                        .expect("i32x4 lane width"),
                );
                let value = dot.wrapping_add(accumulator);
                result[byte..byte + 4].copy_from_slice(&value.to_le_bytes());
            }
            stack.push(Value::V128(Rc::new(result)));
        }
'''
replace_once(runtime, "        256 => {\n", insert + "        256 => {\n")
replace_once(runtime, "                    | 240..=274\n", "                    | 240..=275\n")

validator = ROOT / "crates/wasm-validator/src/typed.rs"
validator_old = '''                    269..=274 => {
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        stack.push(ValueType::V128);
                    }
'''
validator_new = validator_old + '''                    275 => {
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        stack.push(ValueType::V128);
                    }
'''
replace_once(validator, validator_old, validator_new)

# Advance existing fail-closed frontier assertions without weakening them.
for path in (ROOT / "crates/wasm-runtime/tests").glob("*.rs"):
    text = path.read_text()
    updated = text.replace("subopcode: 275", "subopcode: 276")
    updated = updated.replace(", 275)", ", 276)")
    updated = updated.replace("frontier remains outside this slice", "frontier remains outside this slice")
    if updated != text:
        path.write_text(updated)

runtime_test = r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value};
use wasm_validator::{validate, ValidationError};

fn u32leb(mut n: u32, out: &mut Vec<u8>) {
    loop {
        let mut b = (n & 0x7f) as u8;
        n >>= 7;
        if n != 0 { b |= 0x80; }
        out.push(b);
        if n == 0 { break; }
    }
}
fn simd(out: &mut Vec<u8>, op: u32) { out.push(0xfd); u32leb(op, out); }
fn v128(out: &mut Vec<u8>, bytes: [u8; 16]) { simd(out, 12); out.extend_from_slice(&bytes); }
fn module(code: &[u8]) -> Vec<u8> {
    let mut body = vec![0x00]; body.extend_from_slice(code); body.push(0x0b);
    let type_sec = vec![0x01, 0x60, 0x00, 0x01, 0x7b];
    let func_sec = vec![0x01, 0x00];
    let export = vec![0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00];
    let mut code_sec = vec![0x01]; u32leb(body.len() as u32, &mut code_sec); code_sec.extend(body);
    let mut m = b"\0asm\x01\0\0\0".to_vec();
    for (id, sec) in [(1, type_sec), (3, func_sec), (7, export), (10, code_sec)] {
        m.push(id); u32leb(sec.len() as u32, &mut m); m.extend(sec);
    }
    m
}
fn run(a: [u8; 16], b: [u8; 16], c: [u8; 16]) -> [u8; 16] {
    let mut code = Vec::new();
    v128(&mut code, a); v128(&mut code, b); v128(&mut code, c); simd(&mut code, 275);
    let parsed = parse_module(&module(&code)).unwrap(); validate(&parsed).unwrap();
    let mut inst = Instance::new(parsed).unwrap();
    match inst.invoke_export("run", &[]).unwrap().as_slice() {
        [Value::V128(v)] => **v,
        x => panic!("{x:?}"),
    }
}
fn i32x4(values: [i32; 4]) -> [u8; 16] {
    let mut out = [0u8; 16];
    for (lane, value) in values.into_iter().enumerate() {
        out[lane * 4..lane * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    out
}
#[test]
fn relaxed_dot_add_executes_defined_signed_products_and_accumulator() {
    let mut a = [0u8; 16]; let mut b = [0u8; 16];
    a[..4].copy_from_slice(&[2, (-3i8) as u8, 4, 5]);
    b[..4].copy_from_slice(&[4, 5, 6, 7]);
    let r = run(a, b, i32x4([10, 0, 0, 0]));
    assert_eq!(i32::from_le_bytes(r[..4].try_into().unwrap()), 62);
}
#[test]
fn relaxed_dot_add_uses_signed_rhs_and_saturating_pair_profile() {
    let a = [i8::MIN as u8; 16];
    let b = [i8::MIN as u8; 16];
    let r = run(a, b, i32x4([1, 2, 3, 4]));
    for lane in 0..4 {
        let start = lane * 4;
        let got = i32::from_le_bytes(r[start..start + 4].try_into().unwrap());
        assert_eq!(got, 2 * i32::from(i16::MAX) + (lane as i32 + 1));
    }
}
#[test]
fn relaxed_dot_add_requires_three_v128_operands() {
    let mut code = Vec::new(); v128(&mut code, [0; 16]); v128(&mut code, [0; 16]); simd(&mut code, 275);
    let parsed = parse_module(&module(&code)).unwrap();
    assert!(matches!(validate(&parsed), Err(ValidationError::StackUnderflow { .. })));
}
#[test]
fn next_relaxed_simd_opcode_remains_fail_closed() {
    let mut code = Vec::new();
    v128(&mut code, [0; 16]); v128(&mut code, [0; 16]); v128(&mut code, [0; 16]); simd(&mut code, 276);
    let parsed = parse_module(&module(&code)).unwrap();
    assert!(validate(&parsed).is_err());
}
'''
(ROOT / "crates/wasm-runtime/tests/simd_relaxed_dot_i8x16_i7x16_add.rs").write_text(runtime_test)

differential_test = r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"(module
  (func (export "run") (result i32)
    v128.const i8x16 2 -3 4 5 0 0 0 0 0 0 0 0 0 0 0 0
    v128.const i8x16 4 5 6 7 0 0 0 0 0 0 0 0 0 0 0 0
    v128.const i32x4 10 0 0 0
    i32x4.relaxed_dot_i8x16_i7x16_add_s
    i32x4.extract_lane 0))"#;

#[test]
fn relaxed_dot_add_matches_wasmtime_for_defined_lanes() {
    let bytes = wat::parse_str(FIXTURE).unwrap();
    let parsed = parse_module(&bytes).unwrap();
    let mut mini = MiniInstance::new(parsed).unwrap();
    let got = match mini.invoke_export("run", &[]).unwrap().as_slice() {
        [Value::I32(v)] => *v,
        _ => panic!(),
    };
    let mut cfg = Config::new(); cfg.wasm_relaxed_simd(true);
    let engine = Engine::new(&cfg).unwrap();
    let module = ReferenceModule::new(&engine, &bytes).unwrap();
    let mut store = Store::new(&engine, ());
    let inst = ReferenceInstance::new(&mut store, &module, &[]).unwrap();
    let expected = inst.get_typed_func::<(), i32>(&mut store, "run").unwrap().call(&mut store, ()).unwrap();
    assert_eq!(got, expected);
}
'''
(ROOT / "differential/tests/simd_relaxed_dot_i8x16_i7x16_add.rs").write_text(differential_test)
