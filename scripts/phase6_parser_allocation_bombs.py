from pathlib import Path

parser = Path("crates/wasm-parser/src/lib.rs")
text = parser.read_text()

replacements = {
    "    module.imports.reserve(count as usize);\n": "",
    "    module.function_type_indices.reserve(count as usize);\n": "",
    "    module.tables.reserve(count as usize);\n": "",
    "    module.memories.reserve(count as usize);\n": "",
    "    module.globals.reserve(count as usize);\n": "",
    "    module.exports.reserve(count as usize);\n": "",
    "    module.elements.reserve(count as usize);\n": "",
    "    module.code.reserve(count as usize);\n": "",
    "    module.data.reserve(count as usize);\n": "",
    "        let mut function_indices = Vec::with_capacity(function_count as usize);\n": "        let mut function_indices = Vec::new();\n",
    "        let mut locals = Vec::with_capacity(local_group_count as usize);\n": "        let mut locals = Vec::new();\n",
    "    let mut values = Vec::with_capacity(count as usize);\n": "    let mut values = Vec::new();\n",
}

for old, new in replacements.items():
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"allocation hardening anchor {old!r}: expected once, found {count}")
    text = text.replace(old, new, 1)

for forbidden in (
    ".reserve(count as usize)",
    "Vec::with_capacity(function_count as usize)",
    "Vec::with_capacity(local_group_count as usize)",
    "Vec::with_capacity(count as usize)",
):
    if forbidden in text:
        raise SystemExit(f"untrusted-count preallocation remains: {forbidden}")

parser.write_text(text)

Path("crates/wasm-parser/tests/allocation_bomb_corpus.rs").write_text(r'''use wasm_parser::{parse_module, ParseError};

const U32_MAX_LEB: [u8; 5] = [0xff, 0xff, 0xff, 0xff, 0x0f];

fn header() -> Vec<u8> {
    vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
}

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

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    push_u32(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn type_parameter_count_bomb() -> Vec<u8> {
    let mut module = header();
    let mut payload = vec![0x01, 0x60];
    payload.extend_from_slice(&U32_MAX_LEB);
    push_section(&mut module, 1, &payload);
    module
}

fn import_entry_count_bomb() -> Vec<u8> {
    let mut module = header();
    push_section(&mut module, 2, &U32_MAX_LEB);
    module
}

fn element_function_count_bomb() -> Vec<u8> {
    let mut module = header();
    let mut payload = vec![0x01, 0x01, 0x00];
    payload.extend_from_slice(&U32_MAX_LEB);
    push_section(&mut module, 9, &payload);
    module
}

fn local_group_count_bomb() -> Vec<u8> {
    let mut module = header();
    let mut payload = vec![0x01, 0x05];
    payload.extend_from_slice(&U32_MAX_LEB);
    push_section(&mut module, 10, &payload);
    module
}

fn data_segment_count_bomb() -> Vec<u8> {
    let mut module = header();
    push_section(&mut module, 11, &U32_MAX_LEB);
    module
}

#[test]
fn untrusted_vector_counts_do_not_trigger_upfront_giant_allocations() {
    for (name, bytes) in [
        ("type parameter count", type_parameter_count_bomb()),
        ("import entry count", import_entry_count_bomb()),
        ("element function count", element_function_count_bomb()),
        ("local group count", local_group_count_bomb()),
        ("data segment count", data_segment_count_bomb()),
    ] {
        assert_eq!(
            parse_module(&bytes),
            Err(ParseError::UnexpectedEof),
            "{name} bomb should fail on missing encoded entries rather than preallocating from the declared count"
        );
    }
}
''')
