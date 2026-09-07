use wasm_parser::{parse_module, ValueType};

fn section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    assert!(payload.len() < 128);
    module.push(id);
    module.push(payload.len() as u8);
    module.extend_from_slice(payload);
}

#[test]
fn parses_funcref_function_signatures_and_locals() {
    let mut bytes = b"\0asm\x01\0\0\0".to_vec();
    section(&mut bytes, 1, &[1, 0x60, 1, 0x70, 1, 0x70]);
    section(&mut bytes, 3, &[1, 0]);
    section(&mut bytes, 10, &[1, 6, 1, 1, 0x70, 0x20, 0, 0x0b]);

    let module = parse_module(&bytes).expect("funcref function signature and local parse");
    assert_eq!(module.types.len(), 1);
    assert_eq!(module.types[0].params, vec![ValueType::FuncRef]);
    assert_eq!(module.types[0].results, vec![ValueType::FuncRef]);
    assert_eq!(module.code.len(), 1);
    assert_eq!(module.code[0].locals, vec![(1, ValueType::FuncRef)]);
}
