use wasm_parser::parse_module;
use wasm_runtime::Instance;

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

fn build_module_with_locals(instructions: &[u8], locals: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x00]);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 5, &[0x01, 0x00, 0x01]);

    let mut body = locals.to_vec();
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);
    module
}

fn build_module(instructions: &[u8]) -> Vec<u8> {
    build_module_with_locals(instructions, &[0x00])
}

fn build_module_with_global(instructions: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x00]);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(
        &mut module,
        6,
        &[
            0x01, // one global
            0x7f, 0x01, // mutable i32
            0x41, 0x00, 0x0b, // i32.const 0; end
        ],
    );

    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);
    module
}

fn build_module_with_table(instructions: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x00]);
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 4, &[0x01, 0x70, 0x00, 0x01]);

    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);
    module
}

fn assert_admitted_module(module: Vec<u8>, expectation: &str) {
    let module = parse_module(&module).expect("fixture must remain structurally parseable");
    Instance::new(module).expect(expectation);
}

fn assert_admitted(instructions: &[u8], expectation: &str) {
    assert_admitted_module(build_module(instructions), expectation);
}

#[test]
fn noncanonical_branch_depth_is_accepted() {
    assert_admitted(
        &[0x0c, 0x80, 0x00],
        "br depth zero encoded as a width-valid non-minimal u32 LEB must be accepted",
    );
    assert_admitted(
        &[0x41, 0x00, 0x0d, 0x80, 0x00],
        "br_if depth zero encoded as a width-valid non-minimal u32 LEB must be accepted",
    );
}

#[test]
fn noncanonical_call_target_is_accepted() {
    assert_admitted(
        &[0x10, 0x80, 0x00],
        "call target zero encoded as a width-valid non-minimal u32 LEB must be accepted",
    );
}

#[test]
fn noncanonical_call_indirect_indices_are_accepted() {
    assert_admitted_module(
        build_module_with_table(&[
            0x41, 0x00, // table element selector
            0x11, 0x80, 0x00, // type index zero, non-minimal
            0x80, 0x00, // table index zero, non-minimal
        ]),
        "call_indirect type/table indices encoded as width-valid non-minimal u32 LEBs must be accepted",
    );
}

#[test]
fn noncanonical_local_indices_are_accepted() {
    let locals = [
        0x01, // one local group
        0x01, 0x7f, // one i32 local
    ];
    assert_admitted_module(
        build_module_with_locals(&[0x20, 0x80, 0x00, 0x1a], &locals),
        "local.get index zero encoded as a width-valid non-minimal u32 LEB must be accepted",
    );
    assert_admitted_module(
        build_module_with_locals(&[0x41, 0x00, 0x21, 0x80, 0x00], &locals),
        "local.set index zero encoded as a width-valid non-minimal u32 LEB must be accepted",
    );
    assert_admitted_module(
        build_module_with_locals(&[0x41, 0x00, 0x22, 0x80, 0x00, 0x1a], &locals),
        "local.tee index zero encoded as a width-valid non-minimal u32 LEB must be accepted",
    );
}

#[test]
fn noncanonical_global_indices_are_accepted() {
    assert_admitted_module(
        build_module_with_global(&[0x23, 0x80, 0x00, 0x1a]),
        "global.get index zero encoded as a width-valid non-minimal u32 LEB must be accepted",
    );
    assert_admitted_module(
        build_module_with_global(&[0x41, 0x00, 0x24, 0x80, 0x00]),
        "global.set index zero encoded as a width-valid non-minimal u32 LEB must be accepted",
    );
}

#[test]
fn noncanonical_memory_indices_are_accepted() {
    assert_admitted(
        &[0x3f, 0x80, 0x00, 0x1a],
        "memory.size index zero encoded as a width-valid non-minimal u32 LEB must be accepted",
    );
    assert_admitted(
        &[0x41, 0x00, 0x40, 0x80, 0x00, 0x1a],
        "memory.grow index zero encoded as a width-valid non-minimal u32 LEB must be accepted",
    );
}
