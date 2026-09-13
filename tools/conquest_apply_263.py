from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]


def clone_match_arm(path: Path, marker: str, new_opcode: int, transform=None):
    text = path.read_text()
    idx = text.find(marker)
    if idx < 0:
        raise SystemExit(f"missing marker {marker!r} in {path}")
    line_start = text.rfind("\n", 0, idx) + 1
    brace = text.find("{", idx)
    depth = 0
    end = None
    for pos in range(brace, len(text)):
        if text[pos] == "{":
            depth += 1
        elif text[pos] == "}":
            depth -= 1
            if depth == 0:
                end = pos + 1
                break
    if end is None:
        raise SystemExit(f"unterminated arm in {path}")
    while end < len(text) and text[end] in " \t":
        end += 1
    if end < len(text) and text[end] == ",":
        end += 1
    if end < len(text) and text[end] == "\n":
        end += 1
    block = text[line_start:end].replace(marker, f"{new_opcode} => {{", 1)
    if transform:
        block = transform(block)
    path.write_text(text[:end] + block + text[end:])


runtime = ROOT / "crates/wasm-runtime/src/lib.rs"


def runtime_transform(block: str) -> str:
    replacements = [
        ("f32x4.relaxed_madd", "f64x2.relaxed_madd"),
        ("f32x4", "f64x2"),
        ("f32::from_le_bytes", "f64::from_le_bytes"),
        ("chunks_exact_mut(4)", "chunks_exact_mut(8)"),
        ("lane * 4", "lane * 8"),
    ]
    for old, new in replacements:
        if old not in block:
            raise SystemExit(f"runtime transform anchor missing: {old}")
        block = block.replace(old, new)
    return block


clone_match_arm(runtime, "261 => {", 263, runtime_transform)
text = runtime.read_text()
if "240..=262" not in text:
    raise SystemExit("runtime scanner frontier anchor missing")
runtime.write_text(text.replace("240..=262", "240..=263", 1))

validator = ROOT / "crates/wasm-validator/src/typed.rs"
clone_match_arm(validator, "261 => {", 263)

# Advance every independent relaxed-SIMD fail-closed sentinel from 263 to 264.
changed = 0
for path in (ROOT / "crates/wasm-runtime/tests").glob("simd_*.rs"):
    text = path.read_text()
    original = text
    text, n1 = re.subn(r"((?:push_simd|simd)\(\s*&mut\s+\w+,\s*)263(\s*\))", r"\g<1>264\g<2>", text)
    text, n2 = re.subn(r"(subopcode:\s*)263\b", r"\g<1>264", text)
    if text != original:
        path.write_text(text)
        changed += n1 + n2
if changed < 6:
    raise SystemExit(f"unexpectedly few frontier migrations: {changed}")

binary = ROOT / "crates/wasm-runtime/tests/simd_f64x2_binary.rs"
text = binary.read_text()
anchor = "#[test]\nfn adjacent_f64x2_min_frontier_remains_fail_closed() {"
if anchor not in text:
    raise SystemExit("f64x2 frontier test anchor missing")
insert = r'''fn relaxed_madd_lane_bits(a: [f64; 2], b: [f64; 2], c: [f64; 2], lane: u32) -> u64 {
    let mut instructions = Vec::new();
    push_i32_const(&mut instructions, 0);
    push_f64x2_const(&mut instructions, a);
    push_f64x2_const(&mut instructions, b);
    push_f64x2_const(&mut instructions, c);
    push_simd(&mut instructions, 263);
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
        other => panic!("unexpected relaxed madd result: {other:?}"),
    }
}

#[test]
fn relaxed_madd_executes_unfused_lane_semantics() {
    assert_eq!(
        relaxed_madd_lane_bits([2.0, -3.0], [3.0, 2.0], [1.0, 8.0], 0),
        7.0f64.to_bits()
    );
    assert_eq!(
        relaxed_madd_lane_bits([2.0, -3.0], [3.0, 2.0], [1.0, 8.0], 1),
        2.0f64.to_bits()
    );
}

#[test]
fn relaxed_madd_validator_rejects_missing_third_v128_operand() {
    let mut instructions = Vec::new();
    push_f64x2_const(&mut instructions, [1.0; 2]);
    push_f64x2_const(&mut instructions, [2.0; 2]);
    push_simd(&mut instructions, 263);
    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::OperandStackUnderflow { .. }
        ))
    ));
}

'''
binary.write_text(text.replace(anchor, insert + anchor, 1))

differential = ROOT / "differential/tests/simd_relaxed_madd_f64x2.rs"
if differential.exists():
    raise SystemExit("differential f64x2 madd fixture already exists")
differential.write_text(r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "a") (result i64)
    v128.const f64x2 2 -3
    v128.const f64x2 3 2
    v128.const f64x2 1 8
    f64x2.relaxed_madd
    i64x2.extract_lane 0)
  (func (export "b") (result i64)
    v128.const f64x2 2 -3
    v128.const f64x2 3 2
    v128.const f64x2 1 8
    f64x2.relaxed_madd
    i64x2.extract_lane 1))
"#;
const EXPORTS: [&str; 2] = ["a", "b"];

fn mini_trace(bytes: &[u8]) -> Vec<i64> {
    let module = parse_module(bytes).expect("mini runtime must parse relaxed f64x2 madd fixture");
    let mut instance = MiniInstance::new(module).expect("mini runtime must instantiate relaxed f64x2 madd fixture");
    EXPORTS.into_iter().map(|export| match instance.invoke_export_values(export, &[]).expect("mini relaxed f64x2 madd execution must succeed").as_slice() {
        [Value::I64(value)] => *value,
        other => panic!("unexpected mini result for {export}: {other:?}"),
    }).collect()
}

fn reference_trace(bytes: &[u8]) -> Vec<i64> {
    let mut config = Config::new();
    config.wasm_simd(true);
    config.wasm_relaxed_simd(true);
    let engine = Engine::new(&config).expect("relaxed-SIMD Wasmtime engine must initialize");
    let module = ReferenceModule::new(&engine, bytes).expect("Wasmtime must compile relaxed f64x2 madd fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[]).expect("Wasmtime must instantiate relaxed f64x2 madd fixture");
    EXPORTS.into_iter().map(|export| instance.get_typed_func::<(), i64>(&mut store, export).expect("relaxed f64x2 madd export must be [] -> [i64]").call(&mut store, ()).expect("Wasmtime relaxed f64x2 madd execution must succeed")).collect()
}

#[test]
fn relaxed_f64x2_madd_matches_wasmtime_on_exact_finite_lanes() {
    let bytes = wat::parse_str(FIXTURE).expect("relaxed f64x2 madd WAT fixture must parse");
    let expected = vec![7.0f64.to_bits() as i64, 2.0f64.to_bits() as i64];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, expected);
    assert_eq!(reference, expected);
    assert_eq!(mini, reference);
}
''')

print(f"advanced {changed} relaxed-SIMD frontier assertions")
