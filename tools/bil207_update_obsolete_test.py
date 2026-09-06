from pathlib import Path

path = Path("crates/wasm-parser/src/lib.rs")
text = path.read_text()
old = '''    #[test]
    fn rejects_expression_based_element_mode() {
        let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        push_section(&mut bytes, 9, &[0x01, 0x04]);
        assert_eq!(
            parse_module(&bytes),
            Err(ParseError::UnsupportedElementSegmentMode(4))
        );
    }
'''
new = '''    #[test]
    fn rejects_truncated_expression_based_element_mode() {
        let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        push_section(&mut bytes, 9, &[0x01, 0x04]);
        assert_eq!(parse_module(&bytes), Err(ParseError::UnexpectedEof));
    }
'''
if text.count(old) != 1:
    raise SystemExit(f"expected one superseded parser regression, found {text.count(old)}")
path.write_text(text.replace(old, new, 1))
