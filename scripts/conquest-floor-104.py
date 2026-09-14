from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected exactly one match in {path}, found {count}")
    p.write_text(text.replace(old, new, 1))


replace_once(
    "crates/wasm-runtime/src/lib.rs",
    '''        103 => {
            let value = numeric::v128_from_stack(stack)?;
            let mut result = [0u8; 16];
            for lane in 0..4 {
                let start = lane * 4;
                let input = f32::from_bits(u32::from_le_bytes(
                    value[start..start + 4]
                        .try_into()
                        .expect("f32x4 lane width"),
                ));
                result[start..start + 4].copy_from_slice(&input.ceil().to_bits().to_le_bytes());
            }
            stack.push(Value::V128(Rc::new(result)));
        }
        224 | 225 | 227 => {''',
    '''        103 | 104 => {
            let value = numeric::v128_from_stack(stack)?;
            let mut result = [0u8; 16];
            for lane in 0..4 {
                let start = lane * 4;
                let input = f32::from_bits(u32::from_le_bytes(
                    value[start..start + 4]
                        .try_into()
                        .expect("f32x4 lane width"),
                ));
                let output = match subopcode {
                    103 => input.ceil(),
                    104 => input.floor(),
                    _ => unreachable!("matched f32x4 rounding opcode"),
                };
                result[start..start + 4].copy_from_slice(&output.to_bits().to_le_bytes());
            }
            stack.push(Value::V128(Rc::new(result)));
        }
        224 | 225 | 227 => {''',
)
replace_once(
    "crates/wasm-runtime/src/lib.rs",
    '''                    | 103
                    | 99''',
    '''                    | 103
                    | 104
                    | 99''',
)
replace_once(
    "crates/wasm-validator/src/typed.rs",
    '''                    | 103
                    | 128''',
    '''                    | 103
                    | 104
                    | 128''',
)
replace_once(
    "crates/wasm-runtime/tests/simd_f32x4_unary.rs",
    '''#[test]
fn validator_rejects_f32x4_ceil_type_confusion() {''',
    '''#[test]
fn f32x4_floor_rounds_each_lane_toward_negative_infinity() {
    let input = [-1.5, -0.0, 1.25, 2.0];
    assert_eq!(lane_bits(input, 104, 0), (-2.0f32).to_bits());
    assert_eq!(lane_bits(input, 104, 1), (-0.0f32).to_bits());
    assert_eq!(lane_bits(input, 104, 2), 1.0f32.to_bits());
    assert_eq!(lane_bits(input, 104, 3), 2.0f32.to_bits());
}

#[test]
fn validator_rejects_f32x4_floor_type_confusion() {
    let instructions = vec![0x41, 0x01, 0xfd, 0x68];
    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}

#[test]
fn validator_rejects_f32x4_ceil_type_confusion() {''',
)
replace_once(
    "crates/wasm-runtime/tests/simd_i32x4_core.rs",
    '''fn next_simd_lane_memory_subopcode_remains_fail_closed() {
    let bytes = module(&[
        0xfd, 0x68, // f32x4.floor is the next unsupported SIMD capability
    ]);''',
    '''fn next_simd_rounding_subopcode_remains_fail_closed() {
    let bytes = module(&[
        0xfd, 0x69, // f32x4.trunc is the next unsupported SIMD capability
    ]);''',
)
replace_once(
    "crates/wasm-runtime/tests/simd_i32x4_core.rs",
    '''                subopcode: 104,''',
    '''                subopcode: 105,''',
)
replace_once(
    "differential/tests/simd_f32x4_unary.rs",
    '''  (func (export "ceil") (result i32) i32.const 0 v128.const f32x4 -1.5 -0.0 1.25 2 f32x4.ceil v128.store i32.const 0 i32.load))"#;
const EXPORTS: [&str; 4] = ["abs", "neg", "sqrt", "ceil"];''',
    '''  (func (export "ceil") (result i32) i32.const 0 v128.const f32x4 -1.5 -0.0 1.25 2 f32x4.ceil v128.store i32.const 0 i32.load)
  (func (export "floor") (result i32) i32.const 0 v128.const f32x4 -1.5 -0.0 1.25 2 f32x4.floor v128.store i32.const 0 i32.load))"#;
const EXPORTS: [&str; 5] = ["abs", "neg", "sqrt", "ceil", "floor"];''',
)
replace_once(
    "differential/tests/simd_f32x4_unary.rs",
    '''    assert_eq!(mini[3] as u32, (-1.0f32).to_bits());
}''',
    '''    assert_eq!(mini[3] as u32, (-1.0f32).to_bits());
    assert_eq!(mini[4] as u32, (-2.0f32).to_bits());
}''',
)
