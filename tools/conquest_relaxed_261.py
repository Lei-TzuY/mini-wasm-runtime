from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    return (ROOT / path).read_text()


def write(path: str, content: str) -> None:
    (ROOT / path).write_text(content)


# Advance every existing fail-closed Relaxed SIMD frontier from 261 to 262.
# At the base commit, opcode 261 is intentionally unsupported, so frontier
# fixtures are the only runtime-test call sites that may reference it.
for path in (ROOT / "crates/wasm-runtime/tests").glob("*.rs"):
    text = path.read_text()
    if "subopcode: 261" not in text:
        continue
    text = text.replace("subopcode: 261", "subopcode: 262")
    text = re.sub(r"((?:push_simd|simd)\([^\n]*?,\s*)261(\s*\))", r"\g<1>262\2", text)
    path.write_text(text)

# Runtime execution + control-map scanner.
lib_path = "crates/wasm-runtime/src/lib.rs"
lib = read(lib_path)
if "        261 => {" not in lib:
    marker = "        256 => {\n"
    assert marker in lib, "SIMD insertion marker missing"
    arm = '''        261 => {
            // f32x4.relaxed_madd permits either fused or unfused evaluation.
            // Use ordinary multiply followed by add for a portable deterministic lowering.
            let c = numeric::v128_from_stack(stack)?;
            let b = numeric::v128_from_stack(stack)?;
            let a = numeric::v128_from_stack(stack)?;
            let mut result = [0u8; 16];
            for (lane, output) in result.chunks_exact_mut(4).enumerate() {
                let start = lane * 4;
                let lhs =
                    f32::from_le_bytes(a[start..start + 4].try_into().expect("f32x4 lane width"));
                let rhs =
                    f32::from_le_bytes(b[start..start + 4].try_into().expect("f32x4 lane width"));
                let addend =
                    f32::from_le_bytes(c[start..start + 4].try_into().expect("f32x4 lane width"));
                output.copy_from_slice(&(lhs * rhs + addend).to_le_bytes());
            }
            stack.push(Value::V128(Rc::new(result)));
        }
'''
    lib = lib.replace(marker, arm + marker, 1)
old_range = "                    | 240..=260\n"
new_range = "                    | 240..=261\n"
assert old_range in lib or new_range in lib, "SIMD scanner frontier missing"
lib = lib.replace(old_range, new_range, 1)
write(lib_path, lib)

# Typed validator: three v128 operands -> one v128 result.
typed_path = "crates/wasm-validator/src/typed.rs"
typed = read(typed_path)
if "                    261 => {" not in typed:
    marker = "                    14 | 256 => {\n"
    assert marker in typed, "validator insertion marker missing"
    arm = '''                    261 => {
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        stack.push(ValueType::V128);
                    }
'''
    typed = typed.replace(marker, arm + marker, 1)
write(typed_path, typed)

# Focused executable and validation regressions.
test_path = "crates/wasm-runtime/tests/simd_f32x4_binary.rs"
test = read(test_path)
if "fn relaxed_madd_lane_bits" not in test:
    marker = "#[test]\nfn adjacent_f32x4_f64x2_min_frontier_remains_fail_closed()"
    assert marker in test, "f32x4 frontier test marker missing"
    cases = r'''fn relaxed_madd_lane_bits(a: [f32; 4], b: [f32; 4], c: [f32; 4], lane: u32) -> u32 {
    let mut instructions = Vec::new();
    push_i32_const(&mut instructions, 0);
    push_f32x4_const(&mut instructions, a);
    push_f32x4_const(&mut instructions, b);
    push_f32x4_const(&mut instructions, c);
    push_simd(&mut instructions, 261);
    push_v128_store(&mut instructions);
    push_i32_const(&mut instructions, 0);
    push_i32_load(&mut instructions, lane * 4);
    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    let mut instance = Instance::new(parsed).expect("fixture validates");
    match instance
        .invoke_export_values("run", &[])
        .expect("fixture executes")
        .as_slice()
    {
        [Value::I32(value)] => *value as u32,
        other => panic!("unexpected relaxed madd result: {other:?}"),
    }
}

#[test]
fn relaxed_madd_executes_unfused_lane_semantics() {
    assert_eq!(
        relaxed_madd_lane_bits(
            [2.0, -3.0, 0.5, 4.0],
            [3.0, 2.0, 8.0, -0.5],
            [1.0, 1.0, -1.0, 5.0],
            0,
        ),
        7.0f32.to_bits()
    );
    assert_eq!(
        relaxed_madd_lane_bits(
            [2.0, -3.0, 0.5, 4.0],
            [3.0, 2.0, 8.0, -0.5],
            [1.0, 1.0, -1.0, 5.0],
            3,
        ),
        3.0f32.to_bits()
    );
}

#[test]
fn relaxed_madd_validator_rejects_missing_third_v128_operand() {
    let mut instructions = Vec::new();
    push_f32x4_const(&mut instructions, [1.0; 4]);
    push_f32x4_const(&mut instructions, [2.0; 4]);
    push_simd(&mut instructions, 261);
    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::OperandStackUnderflow { .. }
        ))
    ));
}

'''
    test = test.replace(marker, cases + marker, 1)
write(test_path, test)

# Reference differential on exact finite lanes, where fused/unfused choices agree.
diff_path = ROOT / "differential/tests/simd_relaxed_madd_f32x4.rs"
diff_path.write_text(r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "a") (result i32)
    v128.const f32x4 2 -3 0.5 4
    v128.const f32x4 3 2 8 -0.5
    v128.const f32x4 1 1 -1 5
    f32x4.relaxed_madd
    i32x4.extract_lane 0)
  (func (export "b") (result i32)
    v128.const f32x4 2 -3 0.5 4
    v128.const f32x4 3 2 8 -0.5
    v128.const f32x4 1 1 -1 5
    f32x4.relaxed_madd
    i32x4.extract_lane 3))
"#;
const EXPORTS: [&str; 2] = ["a", "b"];

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini runtime must parse relaxed madd fixture");
    let mut instance =
        MiniInstance::new(module).expect("mini runtime must instantiate relaxed madd fixture");
    EXPORTS
        .into_iter()
        .map(|export| {
            match instance
                .invoke_export_values(export, &[])
                .expect("mini relaxed madd execution must succeed")
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
    let engine = Engine::new(&config).expect("relaxed-SIMD Wasmtime engine must initialize");
    let module =
        ReferenceModule::new(&engine, bytes).expect("Wasmtime must compile relaxed madd fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime must instantiate relaxed madd fixture");
    EXPORTS
        .into_iter()
        .map(|export| {
            instance
                .get_typed_func::<(), i32>(&mut store, export)
                .expect("relaxed madd export must be [] -> [i32]")
                .call(&mut store, ())
                .expect("Wasmtime relaxed madd execution must succeed")
        })
        .collect()
}

#[test]
fn relaxed_madd_matches_wasmtime_on_exact_finite_lanes() {
    let bytes = wat::parse_str(FIXTURE).expect("relaxed madd WAT fixture must parse");
    let expected = vec![7.0f32.to_bits() as i32, 3.0f32.to_bits() as i32];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, expected);
    assert_eq!(reference, expected);
    assert_eq!(mini, reference);
}
''')
