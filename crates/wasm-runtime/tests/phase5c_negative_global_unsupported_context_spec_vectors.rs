use wasm_parser::parse_module;
use wasm_validator::{validate, ValidationError};

const I32: u8 = 0x7f;
const UPSTREAM_SPEC_COMMIT: &str = "fc209c5ed8afc4dfeb9252024d217da3376c7a6f";

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

fn module_with_function(instructions: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    push_section(
        &mut module,
        1,
        &[
            0x01, // one type
            0x60, 0x00, 0x01, I32, // [] -> i32
        ],
    );
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(
        &mut module,
        6,
        &[
            0x01, // one global
            I32, 0x01, // mutable i32
            0x41, 0x06, 0x0b, // i32.const 6; end
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

fn assert_unsupported(module: &[u8], opcode: u8) {
    let parsed = parse_module(module).expect("unsupported context vector must remain well-formed");
    assert!(matches!(
        validate(&parsed),
        Err(ValidationError::UnsupportedOpcode {
            function: 0,
            opcode: actual,
            ..
        }) if actual == opcode
    ));
}

#[test]
fn upstream_global_get_br_table_contexts_remain_fail_closed() {
    // The pinned upstream vectors place global.get first and last among the
    // br_table operands. br_table itself is not yet supported, so both forms
    // must be rejected before execution rather than partially interpreted.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let cases = [
        module_with_function(&[
            0x02, I32, // block (result i32)
            0x23, 0x00, // global.get $x (branch value)
            0x41, 0x02, // i32.const 2 (selector)
            0x0e, 0x01, 0x00, 0x00, // br_table 0 0
            0x0b, // end block
        ]),
        module_with_function(&[
            0x02, I32, // block (result i32)
            0x41, 0x02, // i32.const 2 (branch value)
            0x23, 0x00, // global.get $x (selector)
            0x0e, 0x01, 0x00, 0x00, // br_table 0 0
            0x0b, // end block
        ]),
    ];

    for module in cases {
        assert_unsupported(&module, 0x0e);
    }
}
