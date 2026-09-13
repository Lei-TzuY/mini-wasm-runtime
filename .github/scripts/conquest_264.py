from pathlib import Path
import re

runtime = Path('crates/wasm-runtime/src/lib.rs')
text = runtime.read_text()
assert '        264 => {' not in text
start = text.index('        263 => {')
insert_at = text.index('        256 => {', start)
block = '''        264 => {
            // f64x2.relaxed_nmadd permits either fused or unfused evaluation.
            // Use ordinary multiply, negate, then add for a portable deterministic lowering.
            let c = numeric::v128_from_stack(stack)?;
            let b = numeric::v128_from_stack(stack)?;
            let a = numeric::v128_from_stack(stack)?;
            let mut result = [0u8; 16];
            for (lane, output) in result.chunks_exact_mut(8).enumerate() {
                let start = lane * 8;
                let lhs =
                    f64::from_le_bytes(a[start..start + 8].try_into().expect("f64x2 lane width"));
                let rhs =
                    f64::from_le_bytes(b[start..start + 8].try_into().expect("f64x2 lane width"));
                let addend =
                    f64::from_le_bytes(c[start..start + 8].try_into().expect("f64x2 lane width"));
                output.copy_from_slice(&(-(lhs * rhs) + addend).to_le_bytes());
            }
            stack.push(Value::V128(Rc::new(result)));
        }
'''
text = text[:insert_at] + block + text[insert_at:]
assert text.count('| 240..=263') == 1
text = text.replace('| 240..=263', '| 240..=264')
runtime.write_text(text)

validator = Path('crates/wasm-validator/src/typed.rs')
text = validator.read_text()
assert '                    264 => {' not in text
start = text.index('                    263 => {')
end = text.index('                    }\n', start) + len('                    }\n')
old = text[start:end]
assert old.count('pop_expect') == 3 and 'stack.push(ValueType::V128);' in old
text = text[:end] + old.replace('263 =>', '264 =>', 1) + text[end:]
validator.write_text(text)

for path in Path('crates/wasm-runtime/tests').glob('simd_*.rs'):
    text = path.read_text()
    text = re.sub(r'\b(push_simd|simd)\(([^\n]*?),\s*264\)', r'\1(\2, 265)', text)
    text = text.replace('subopcode: 264', 'subopcode: 265')
    path.write_text(text)

test = Path('crates/wasm-runtime/tests/simd_f64x2_binary.rs')
text = test.read_text()
marker = '#[test]\nfn adjacent_f64x2_min_frontier_remains_fail_closed() {'
assert marker in text and 'fn relaxed_nmadd_lane_bits' not in text
addition = r'''fn relaxed_nmadd_lane_bits(a: [f64; 2], b: [f64; 2], c: [f64; 2], lane: u32) -> u64 {
    let mut instructions = Vec::new();
    push_i32_const(&mut instructions, 0);
    push_f64x2_const(&mut instructions, a);
    push_f64x2_const(&mut instructions, b);
    push_f64x2_const(&mut instructions, c);
    push_simd(&mut instructions, 264);
    push_v128_store(&mut instructions);
    push_i32_const(&mut instructions, 0);
    push_i64_load(&mut instructions, lane * 8);
    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    let mut instance = Instance::new(parsed).expect("fixture validates");
    match instance
        .invoke_export_values("run", &[])
        .expect("fixture executes")
        .as_slice()
    {
        [Value::I64(value)] => *value as u64,
        other => panic!("unexpected relaxed nmadd result: {other:?}"),
    }
}

#[test]
fn relaxed_nmadd_executes_unfused_lane_semantics() {
    assert_eq!(
        relaxed_nmadd_lane_bits([2.0, -3.0], [3.0, 2.0], [1.0, 8.0], 0),
        (-5.0f64).to_bits()
    );
    assert_eq!(
        relaxed_nmadd_lane_bits([2.0, -3.0], [3.0, 2.0], [1.0, 8.0], 1),
        14.0f64.to_bits()
    );
}

#[test]
fn relaxed_nmadd_validator_rejects_missing_third_v128_operand() {
    let mut instructions = Vec::new();
    push_f64x2_const(&mut instructions, [1.0; 2]);
    push_f64x2_const(&mut instructions, [2.0; 2]);
    push_simd(&mut instructions, 264);
    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::OperandStackUnderflow { .. }
        ))
    ));
}

'''
text = text.replace(marker, addition + marker, 1)
test.write_text(text)

diff = Path('differential/tests/simd_relaxed_nmadd_f64x2.rs')
assert not diff.exists()
diff.write_text(r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (memory 1)
  (func (export "a") (result i64)
    i32.const 0
    v128.const f64x2 2 -3
    v128.const f64x2 3 2
    v128.const f64x2 1 8
    f64x2.relaxed_nmadd
    v128.store
    i32.const 0
    i64.load)
  (func (export "b") (result i64)
    i32.const 0
    v128.const f64x2 2 -3
    v128.const f64x2 3 2
    v128.const f64x2 1 8
    f64x2.relaxed_nmadd
    v128.store
    i32.const 8
    i64.load))
"#;
const EXPORTS: [&str; 2] = ["a", "b"];

fn mini_trace(bytes: &[u8]) -> Vec<i64> {
    let module = parse_module(bytes).expect("mini runtime must parse relaxed f64x2 nmadd fixture");
    let mut instance = MiniInstance::new(module)
        .expect("mini runtime must instantiate relaxed f64x2 nmadd fixture");
    EXPORTS
        .into_iter()
        .map(|export| {
            match instance
                .invoke_export_values(export, &[])
                .expect("mini relaxed f64x2 nmadd execution must succeed")
                .as_slice()
            {
                [Value::I64(value)] => *value,
                other => panic!("unexpected mini result for {export}: {other:?}"),
            }
        })
        .collect()
}

fn reference_trace(bytes: &[u8]) -> Vec<i64> {
    let mut config = Config::new();
    config.wasm_simd(true);
    config.wasm_relaxed_simd(true);
    let engine = Engine::new(&config).expect("relaxed-SIMD Wasmtime engine must initialize");
    let module = ReferenceModule::new(&engine, bytes)
        .expect("Wasmtime must compile relaxed f64x2 nmadd fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime must instantiate relaxed f64x2 nmadd fixture");
    EXPORTS
        .into_iter()
        .map(|export| {
            instance
                .get_typed_func::<(), i64>(&mut store, export)
                .expect("relaxed f64x2 nmadd export must be [] -> [i64]")
                .call(&mut store, ())
                .expect("Wasmtime relaxed f64x2 nmadd execution must succeed")
        })
        .collect()
}

#[test]
fn relaxed_f64x2_nmadd_matches_wasmtime_on_exact_finite_lanes() {
    let bytes = wat::parse_str(FIXTURE).expect("relaxed f64x2 nmadd WAT fixture must parse");
    let expected = vec![(-5.0f64).to_bits() as i64, 14.0f64.to_bits() as i64];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, expected);
    assert_eq!(reference, expected);
    assert_eq!(mini, reference);
}
''')
