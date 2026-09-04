use wasm_parser::{parse_module, ParseError};
use wasm_runtime::{Instance, RuntimeError};

fn section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    module.push(payload.len() as u8);
    module.extend_from_slice(payload);
}

fn module(body: &[u8], data: &[u8], declared_count: u8) -> Vec<u8> {
    let mut wasm = b"\0asm\x01\0\0\0".to_vec();
    section(&mut wasm, 1, &[1, 0x60, 0, 0]);
    section(&mut wasm, 3, &[1, 0]);
    section(&mut wasm, 5, &[1, 0, 1]);
    section(&mut wasm, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);
    section(&mut wasm, 12, &[declared_count]);
    let mut code = vec![1, (body.len() + 1) as u8, 0];
    code.extend_from_slice(body);
    section(&mut wasm, 10, &code);
    let mut data_section = vec![1, 1, data.len() as u8];
    data_section.extend_from_slice(data);
    section(&mut wasm, 11, &data_section);
    wasm
}

#[test]
fn memory_init_copies_passive_data() {
    let body = [0x41, 4, 0x41, 1, 0x41, 3, 0xfc, 8, 0, 0, 0x0b];
    let parsed = parse_module(&module(&body, b"hello", 1)).unwrap();
    let mut vm = Instance::new(parsed).unwrap();
    vm.invoke_export("run", &[]).unwrap();
    assert_eq!(&vm.memory().unwrap().bytes()[4..7], b"ell");
}

#[test]
fn data_drop_empties_segment_and_traps_followup_init() {
    let body = [0xfc, 9, 0, 0x41, 0, 0x41, 0, 0x41, 1, 0xfc, 8, 0, 0, 0x0b];
    let parsed = parse_module(&module(&body, b"x", 1)).unwrap();
    let mut vm = Instance::new(parsed).unwrap();
    let error = vm.invoke_export("run", &[]).unwrap_err();
    assert!(matches!(
        error,
        RuntimeError::DataSegmentSourceOutOfBounds { .. }
    ));
    assert_eq!(vm.memory().unwrap().bytes()[0], 0);
}

#[test]
fn source_oob_is_atomic() {
    let body = [0x41, 0, 0x41, 4, 0x41, 2, 0xfc, 8, 0, 0, 0x0b];
    let parsed = parse_module(&module(&body, b"hello", 1)).unwrap();
    let mut vm = Instance::new(parsed).unwrap();
    assert!(matches!(
        vm.invoke_export("run", &[]),
        Err(RuntimeError::DataSegmentSourceOutOfBounds { .. })
    ));
    assert_eq!(&vm.memory().unwrap().bytes()[0..2], &[0, 0]);
}

#[test]
fn parser_rejects_datacount_mismatch() {
    let body = [0x0b];
    assert!(matches!(
        parse_module(&module(&body, b"x", 2)),
        Err(ParseError::DataCountMismatch {
            declared: 2,
            actual: 1
        })
    ));
}
