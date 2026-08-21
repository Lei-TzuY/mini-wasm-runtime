from pathlib import Path

path = Path("crates/wasm-validator/src/typed.rs")
text = path.read_text()
old = '''fn control_at_depth<'a>(
    controls: &'a [ControlFrame],
    depth: u32,
    function: usize,
    offset: usize,
) -> Result<&'a ControlFrame, ValidationError> {'''
new = '''fn control_at_depth(
    controls: &[ControlFrame],
    depth: u32,
    function: usize,
    offset: usize,
) -> Result<&ControlFrame, ValidationError> {'''
assert old in text
path.write_text(text.replace(old, new, 1))
