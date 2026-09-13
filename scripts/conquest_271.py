from pathlib import Path


def replace_exact(path, old, new, count=None):
    p = Path(path)
    s = p.read_text()
    actual = s.count(old)
    if actual == 0:
        raise SystemExit(f"missing expected text in {path}: {old!r}")
    if count is not None and actual != count:
        raise SystemExit(f"unexpected count in {path}: {actual} != {count} for {old!r}")
    p.write_text(s.replace(old, new))


runtime = Path("crates/wasm-runtime/src/lib.rs")
s = runtime.read_text()
marker = "        256 => {\n            // Relaxed swizzle permits implementation-defined results for selectors 16..=127,"
if marker not in s:
    raise SystemExit("runtime insertion marker not found")
block = '''        271 => {
            // f64x2.relaxed_min permits implementation-defined choice for NaN and
            // signed-zero ties. Reuse deterministic f64x2.min-compatible semantics.
            let rhs = numeric::v128_from_stack(stack)?;
            let lhs = numeric::v128_from_stack(stack)?;
            let mut result = [0u8; 16];
            for lane in 0..2 {
                let start = lane * 8;
                let lhs_lane =
                    f64::from_le_bytes(lhs[start..start + 8].try_into().expect("f64x2 lane width"));
                let rhs_lane =
                    f64::from_le_bytes(rhs[start..start + 8].try_into().expect("f64x2 lane width"));
                let value = if lhs_lane.is_nan() || rhs_lane.is_nan() {
                    f64::NAN
                } else if lhs_lane == rhs_lane {
                    if lhs_lane == 0.0 {
                        f64::from_bits(lhs_lane.to_bits() | rhs_lane.to_bits())
                    } else {
                        lhs_lane
                    }
                } else {
                    lhs_lane.min(rhs_lane)
                };
                result[start..start + 8].copy_from_slice(&value.to_le_bytes());
            }
            stack.push(Value::V128(Rc::new(result)));
        }
'''
s = s.replace(marker, block + marker, 1)
s = s.replace("                    | 240..=270\n", "                    | 240..=271\n", 1)
runtime.write_text(s)

replace_exact(
    "crates/wasm-validator/src/typed.rs",
    "                    269 | 270 => {",
    "                    269 | 270 | 271 => {",
    1,
)

frontier_files = [
    "crates/wasm-runtime/tests/simd_conversions.rs",
    "crates/wasm-runtime/tests/simd_f32x4_binary.rs",
    "crates/wasm-runtime/tests/simd_f32x4_minmax.rs",
    "crates/wasm-runtime/tests/simd_f32x4_unary.rs",
    "crates/wasm-runtime/tests/simd_f64x2_binary.rs",
    "crates/wasm-runtime/tests/simd_f64x2_minmax.rs",
    "crates/wasm-runtime/tests/simd_f64x2_unary.rs",
    "crates/wasm-runtime/tests/simd_i16x8_alu_closure.rs",
    "crates/wasm-runtime/tests/simd_i32x4_shifts.rs",
    "crates/wasm-runtime/tests/simd_i32x4_widening.rs",
    "crates/wasm-runtime/tests/simd_i64x2_arithmetic.rs",
    "crates/wasm-runtime/tests/simd_i64x2_comparisons.rs",
    "crates/wasm-runtime/tests/simd_i64x2_extmul.rs",
    "crates/wasm-runtime/tests/simd_i64x2_shifts.rs",
    "crates/wasm-runtime/tests/simd_relaxed_laneselect_i16x8.rs",
    "crates/wasm-runtime/tests/simd_relaxed_laneselect_i32x4.rs",
    "crates/wasm-runtime/tests/simd_relaxed_laneselect_i64x2.rs",
    "crates/wasm-runtime/tests/simd_relaxed_laneselect_i8x16.rs",
    "crates/wasm-runtime/tests/simd_relaxed_swizzle.rs",
    "crates/wasm-runtime/tests/simd_relaxed_trunc_f32x4_s.rs",
    "crates/wasm-runtime/tests/simd_relaxed_trunc_f32x4_u.rs",
    "crates/wasm-runtime/tests/simd_relaxed_trunc_f64x2_s_zero.rs",
    "crates/wasm-runtime/tests/simd_relaxed_trunc_f64x2_u_zero.rs",
]
for path in frontier_files:
    p = Path(path)
    lines = p.read_text().splitlines(True)
    changed = False
    out = []
    for line in lines:
        if "271" in line and (
            "push_simd" in line
            or "simd(&mut" in line
            or "subopcode:" in line
            or "frontier" in line
        ):
            line = line.replace("271", "272")
            changed = True
        out.append(line)
    if not changed:
        raise SystemExit(f"no frontier 271 updated in {path}")
    p.write_text("".join(out))

test_file = Path("crates/wasm-runtime/tests/simd_f64x2_minmax.rs")
t = test_file.read_text()
extra = r'''

#[test]
fn relaxed_f64x2_min_executes_ordered_lanes() {
    assert_eq!(
        lane_bits([3.0, -2.0], [4.0, -5.0], 271, 0),
        3.0f64.to_bits()
    );
    assert_eq!(
        lane_bits([3.0, -2.0], [4.0, -5.0], 271, 1),
        (-5.0f64).to_bits()
    );
}

#[test]
fn relaxed_f64x2_min_selects_negative_zero() {
    assert_eq!(
        lane_bits([0.0, -0.0], [-0.0, 0.0], 271, 0),
        (-0.0f64).to_bits()
    );
}

#[test]
fn validator_rejects_relaxed_f64x2_min_type_confusion() {
    let mut instructions = Vec::new();
    push_f64x2_const(&mut instructions, [1.0; 2]);
    push_i32_const(&mut instructions, 1);
    push_simd(&mut instructions, 271);
    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::TypeMismatch { .. }
        ))
    ));
}
'''
if "relaxed_f64x2_min_executes_ordered_lanes" in t:
    raise SystemExit("relaxed f64x2 min tests already exist")
test_file.write_text(t + extra)

differential = Path("differential/tests/simd_relaxed_f64x2_min.rs")
if differential.exists():
    raise SystemExit("differential test already exists")
differential.write_text(r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (memory 1)
  (func (export "ordered") (result i64)
    i32.const 0
    v128.const f64x2 3 -2
    v128.const f64x2 4 -5
    f64x2.relaxed_min
    v128.store
    i32.const 0
    i64.load))
"#;

#[test]
fn relaxed_f64x2_min_matches_wasmtime_for_ordered_lanes() {
    let bytes = wat::parse_str(FIXTURE).expect("relaxed min WAT parses");
    let parsed = parse_module(&bytes).expect("mini parses relaxed min fixture");
    let mut mini = MiniInstance::new(parsed).expect("mini instantiates relaxed min fixture");
    let mini_value = match mini
        .invoke_export_values("ordered", &[])
        .unwrap()
        .as_slice()
    {
        [Value::I64(v)] => *v,
        other => panic!("unexpected mini result: {other:?}"),
    };

    let mut config = Config::new();
    config.wasm_simd(true);
    config.wasm_relaxed_simd(true);
    let engine = Engine::new(&config).expect("Wasmtime engine initializes");
    let module = ReferenceModule::new(&engine, &bytes).expect("Wasmtime compiles relaxed min");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[]).expect("Wasmtime instantiates");
    let reference = instance
        .get_typed_func::<(), i64>(&mut store, "ordered")
        .unwrap()
        .call(&mut store, ())
        .unwrap();

    assert_eq!(mini_value, 3.0f64.to_bits() as i64);
    assert_eq!(mini_value, reference);
}
''')
