from pathlib import Path


def replace_once(path: Path, old: str, new: str, label: str) -> None:
    text = path.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected one anchor, found {count}")
    path.write_text(text.replace(old, new, 1))


parser = Path("crates/wasm-parser/src/lib.rs")
replace_once(
    parser,
    '''    if cursor.read_u8()? != 0x0b {
        return Err(ParseError::ConstExprMissingEnd);
    }
    Ok(value)
''',
    '''    let terminator = match cursor.read_u8() {
        Ok(byte) => byte,
        Err(ParseError::UnexpectedEof) => return Err(ParseError::ConstExprMissingEnd),
        Err(error) => return Err(error),
    };
    if terminator != 0x0b {
        return Err(ParseError::ConstExprMissingEnd);
    }
    Ok(value)
''',
    "constant-expression terminator diagnostic",
)

tests = Path("crates/wasm-parser/tests/malformed_binary_corpus.rs")
replace_once(
    tests,
    '''fn const_expr_missing_end() -> Vec<u8> {
    let mut bytes = header();
    push_section(&mut bytes, 6, &[0x01, 0x7f, 0x00, 0x41, 0x00]);
    bytes
}

fn invalid_const_expr_opcode() -> Vec<u8> {
''',
    '''fn const_expr_missing_end() -> Vec<u8> {
    let mut bytes = header();
    push_section(&mut bytes, 6, &[0x01, 0x7f, 0x00, 0x41, 0x00]);
    bytes
}

fn truncated_const_expr_immediate() -> Vec<u8> {
    let mut bytes = header();
    push_section(&mut bytes, 6, &[0x01, 0x7f, 0x00, 0x41, 0x80]);
    bytes
}

fn invalid_const_expr_opcode() -> Vec<u8> {
''',
    "truncated const-expression immediate fixture",
)
replace_once(
    tests,
    '''        Case {
            name: "constant expression missing end",
            bytes: const_expr_missing_end(),
            expected: ParseError::ConstExprMissingEnd,
        },
        Case {
            name: "invalid constant expression opcode",
''',
    '''        Case {
            name: "constant expression missing end",
            bytes: const_expr_missing_end(),
            expected: ParseError::ConstExprMissingEnd,
        },
        Case {
            name: "constant expression immediate is truncated",
            bytes: truncated_const_expr_immediate(),
            expected: ParseError::UnexpectedEof,
        },
        Case {
            name: "invalid constant expression opcode",
''',
    "truncated immediate corpus case",
)
