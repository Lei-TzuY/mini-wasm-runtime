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
        ch = text[pos]
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                end = pos + 1
                break
    if end is None:
        raise SystemExit(f"unterminated arm in {path}")
    while end < len(text) and text[end] in " \t":
        end += 1
    if end < len(text) and text[end] == ',':
        end += 1
    if end < len(text) and text[end] == '\n':
        end += 1
    block = text[line_start:end]
    block = block.replace(marker, f"{new_opcode} => {{", 1)
    if transform:
        block = transform(block)
    text = text[:end] + block + text[end:]
    path.write_text(text)


runtime = ROOT / "crates/wasm-runtime/src/lib.rs"

def runtime_transform(block: str) -> str:
    block = block.replace("f32x4.relaxed_madd", "f32x4.relaxed_nmadd")
    block = block.replace(
        "Use ordinary multiply followed by add for a portable deterministic lowering.",
        "Use ordinary multiply, negate, then add for a portable deterministic lowering.",
    )
    old = "output.copy_from_slice(&(lhs * rhs + addend).to_le_bytes());"
    new = "output.copy_from_slice(&(-(lhs * rhs) + addend).to_le_bytes());"
    if old not in block:
        raise SystemExit("runtime madd expression anchor missing")
    return block.replace(old, new, 1)

clone_match_arm(runtime, "261 => {", 262, runtime_transform)
text = runtime.read_text()
if "240..=261" not in text:
    raise SystemExit("runtime scanner frontier anchor missing")
runtime.write_text(text.replace("240..=261", "240..=262", 1))

validator = ROOT / "crates/wasm-validator/src/typed.rs"
clone_match_arm(validator, "261 => {", 262)

# Advance every independent relaxed-SIMD fail-closed sentinel from 262 to 263.
changed = 0
for path in (ROOT / "crates/wasm-runtime/tests").glob("simd_*.rs"):
    text = path.read_text()
    original = text
    text, n1 = re.subn(r"((?:push_simd|simd)\(\s*&mut\s+\w+,\s*)262(\s*\))", r"\g<1>263\g<2>", text)
    text, n2 = re.subn(r"(subopcode:\s*)262\b", r"\g<1>263", text)
    if text != original:
        path.write_text(text)
        changed += n1 + n2
if changed < 6:
    raise SystemExit(f"unexpectedly few frontier migrations: {changed}")

binary = ROOT / "crates/wasm-runtime/tests/simd_f32x4_binary.rs"
text = binary.read_text()
anchor = "#[test]\nfn adjacent_f32x4_f64x2_min_frontier_remains_fail_closed() {"
if anchor not in text:
    raise SystemExit("f32x4 frontier test anchor missing")
insert = r'''fn relaxed_nmadd_lane_bits(a: [f32; 4], b: [f32; 4], c: [f32; 4], lane: u32) -> u32 {
    let mut instructions = Vec::new();
    push_i32_const(&mut instructions, 0);
    push_f32x4_const(&mut instructions, a);
    push_f32x4_const(&mut instructions, b);
    push_f32x4_const(&mut instructions, c);
    push_simd(&mut instructions, 262);
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
        other => panic!("unexpected relaxed nmadd result: {other:?}"),
    }
}

#[test]
fn relaxed_nmadd_executes_unfused_lane_semantics() {
    assert_eq!(
        relaxed_nmadd_lane_bits(
            [2.0, -3.0, 0.5, 4.0],
            [3.0, 2.0, 8.0, -0.5],
            [1.0, 1.0, -1.0, 5.0],
            0,
        ),
        (-5.0f32).to_bits()
    );
    assert_eq!(
        relaxed_nmadd_lane_bits(
            [2.0, -3.0, 0.5, 4.0],
            [3.0, 2.0, 8.0, -0.5],
            [1.0, 1.0, -1.0, 5.0],
            3,
        ),
        7.0f32.to_bits()
    );
}

#[test]
fn relaxed_nmadd_validator_rejects_missing_third_v128_operand() {
    let mut instructions = Vec::new();
    push_f32x4_const(&mut instructions, [1.0; 4]);
    push_f32x4_const(&mut instructions, [2.0; 4]);
    push_simd(&mut instructions, 262);
    let parsed = parse_module(&module(&instructions)).expect("fixture parses");
    assert!(matches!(
        Instance::new(parsed),
        Err(RuntimeError::Validation(
            ValidationError::OperandStackUnderflow { .. }
        ))
    ));
}

'''
text = text.replace(anchor, insert + anchor, 1)
binary.write_text(text)

differential = ROOT / "differential/tests/simd_relaxed_nmadd_f32x4.rs"
if differential.exists():
    raise SystemExit("differential nmadd fixture already exists")
differential.write_text(r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"
(module
  (func (export "a") (result i32)
    v128.const f32x4 2 -3 0.5 4
    v128.const f32x4 3 2 8 -0.5
    v128.const f32x4 1 1 -1 5
    f32x4.relaxed_nmadd
    i32x4.extract_lane 0)
  (func (export "b") (result i32)
    v128.const f32x4 2 -3 0.5 4
    v128.const f32x4 3 2 8 -0.5
    v128.const f32x4 1 1 -1 5
    f32x4.relaxed_nmadd
    i32x4.extract_lane 3))
"#;
const EXPORTS: [&str; 2] = ["a", "b"];

fn mini_trace(bytes: &[u8]) -> Vec<i32> {
    let module = parse_module(bytes).expect("mini runtime must parse relaxed nmadd fixture");
    let mut instance =
        MiniInstance::new(module).expect("mini runtime must instantiate relaxed nmadd fixture");
    EXPORTS
        .into_iter()
        .map(|export| {
            match instance
                .invoke_export_values(export, &[])
                .expect("mini relaxed nmadd execution must succeed")
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
        ReferenceModule::new(&engine, bytes).expect("Wasmtime must compile relaxed nmadd fixture");
    let mut store = Store::new(&engine, ());
    let instance = ReferenceInstance::new(&mut store, &module, &[])
        .expect("Wasmtime must instantiate relaxed nmadd fixture");
    EXPORTS
        .into_iter()
        .map(|export| {
            instance
                .get_typed_func::<(), i32>(&mut store, export)
                .expect("relaxed nmadd export must be [] -> [i32]")
                .call(&mut store, ())
                .expect("Wasmtime relaxed nmadd execution must succeed")
        })
        .collect()
}

#[test]
fn relaxed_nmadd_matches_wasmtime_on_exact_finite_lanes() {
    let bytes = wat::parse_str(FIXTURE).expect("relaxed nmadd WAT fixture must parse");
    let expected = vec![(-5.0f32).to_bits() as i32, 7.0f32.to_bits() as i32];
    let mini = mini_trace(&bytes);
    let reference = reference_trace(&bytes);
    assert_eq!(mini, expected);
    assert_eq!(reference, expected);
    assert_eq!(mini, reference);
}
''')

print(f"advanced {changed} relaxed-SIMD frontier assertions")
