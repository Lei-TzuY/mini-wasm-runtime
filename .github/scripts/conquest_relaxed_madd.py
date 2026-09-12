from pathlib import Path

p = Path('crates/wasm-runtime/src/lib.rs')
s = p.read_text()
needle = '''        256 => {
            // Relaxed swizzle permits implementation-defined results for selectors 16..=127,'''
insert = '''        259 => {
            // f32x4.relaxed_madd permits either fused or unfused evaluation.
            // Use ordinary multiply followed by add for a portable deterministic lowering.
            let c = numeric::v128_from_stack(stack)?;
            let b = numeric::v128_from_stack(stack)?;
            let a = numeric::v128_from_stack(stack)?;
            let mut result = [0u8; 16];
            for (lane, output) in result.chunks_exact_mut(4).enumerate() {
                let start = lane * 4;
                let lhs = f32::from_le_bytes(
                    a[start..start + 4].try_into().expect("f32x4 lane width"),
                );
                let rhs = f32::from_le_bytes(
                    b[start..start + 4].try_into().expect("f32x4 lane width"),
                );
                let addend = f32::from_le_bytes(
                    c[start..start + 4].try_into().expect("f32x4 lane width"),
                );
                output.copy_from_slice(&(lhs * rhs + addend).to_le_bytes());
            }
            stack.push(Value::V128(Rc::new(result)));
        }
        256 => {
            // Relaxed swizzle permits implementation-defined results for selectors 16..=127,'''
assert needle in s
s = s.replace(needle, insert, 1)
assert '240..=258' in s
s = s.replace('240..=258', '240..=259', 1)
p.write_text(s)

p = Path('crates/wasm-validator/src/typed.rs')
s = p.read_text()
needle = '''                    258 => {
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        stack.push(ValueType::V128);
                    }
                    14 | 256 => {'''
insert = '''                    258 => {
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        stack.push(ValueType::V128);
                    }
                    259 => {
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        stack.push(ValueType::V128);
                    }
                    14 | 256 => {'''
assert needle in s
p.write_text(s.replace(needle, insert, 1))

# Before this slice every 259 reference outside the target file is a fail-closed frontier sentinel.
for test_path in Path('crates/wasm-runtime/tests').glob('simd_*.rs'):
    if test_path.name == 'simd_f32x4_binary.rs':
        continue
    text = test_path.read_text()
    updated = text.replace(
        'push_simd(&mut instructions, 259);',
        'push_simd(&mut instructions, 260);',
    ).replace('subopcode: 259,', 'subopcode: 260,')
    if updated != text:
        test_path.write_text(updated)

p = Path('crates/wasm-runtime/tests/simd_f32x4_binary.rs')
s = p.read_text()
marker = '''#[test]
fn adjacent_f32x4_f64x2_min_frontier_remains_fail_closed() {'''
tests = '''fn relaxed_madd_lane_bits(a: [f32; 4], b: [f32; 4], c: [f32; 4], lane: u32) -> u32 {
    let mut instructions = Vec::new();
    push_i32_const(&mut instructions, 0);
    push_f32x4_const(&mut instructions, a);
    push_f32x4_const(&mut instructions, b);
    push_f32x4_const(&mut instructions, c);
    push_simd(&mut instructions, 259);
    push_v128_store(&mut instructions);
    push_i32_const(&mut instructions, 0);
    push_i32_load(&mut instructions, lane * 4);
    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    let mut instance = Instance::new(parsed).expect("fixture validates");
    match instance.invoke_export_values("run", &[]).expect("fixture executes").as_slice() {
        [Value::I32(value)] => *value as u32,
        other => panic!("unexpected relaxed madd result: {other:?}"),
    }
}

#[test]
fn relaxed_madd_executes_unfused_lane_semantics() {
    assert_eq!(
        relaxed_madd_lane_bits([2.0, -3.0, 0.5, 4.0], [3.0, 2.0, 8.0, -0.5], [1.0, 1.0, -1.0, 5.0], 0),
        7.0f32.to_bits()
    );
    assert_eq!(
        relaxed_madd_lane_bits([2.0, -3.0, 0.5, 4.0], [3.0, 2.0, 8.0, -0.5], [1.0, 1.0, -1.0, 5.0], 3),
        3.0f32.to_bits()
    );
}

#[test]
fn relaxed_madd_validator_rejects_missing_third_v128_operand() {
    let mut instructions = Vec::new();
    push_f32x4_const(&mut instructions, [1.0; 4]);
    push_f32x4_const(&mut instructions, [2.0; 4]);
    push_simd(&mut instructions, 259);
    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::OperandStackUnderflow { .. }
        ))
    ));
}

#[test]
fn adjacent_f32x4_f64x2_min_frontier_remains_fail_closed() {'''
assert marker in s
s = s.replace(marker, tests, 1)
s = s.replace(
    '''push_simd(&mut instructions, 259);
    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::UnsupportedPrefixedOpcode {
                prefix: 0xfd,
                subopcode: 259,''',
    '''push_simd(&mut instructions, 260);
    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::UnsupportedPrefixedOpcode {
                prefix: 0xfd,
                subopcode: 260,''',
    1,
)
p.write_text(s)
