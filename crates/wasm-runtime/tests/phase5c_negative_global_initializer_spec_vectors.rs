use wasm_parser::{parse_module, ParseError, ValueType};

const I32: u8 = 0x7f;
const F32: u8 = 0x7d;
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

fn module_with_global(value_type: u8, initializer: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let mut globals = vec![
        0x01, // one global
        value_type, 0x00, // immutable
    ];
    globals.extend_from_slice(initializer);
    push_section(&mut module, 6, &globals);
    module
}

#[test]
fn upstream_global_initializer_rejects_non_constant_local_get() {
    // WebAssembly/spec test/core/global.wast @ the pinned revision:
    // (global f32 (local.get 0)) is invalid because local.get is not allowed
    // in this supported constant-expression subset.
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);

    let module = module_with_global(
        F32,
        &[
            0x20, 0x00, // local.get 0
            0x0b, // end
        ],
    );

    assert_eq!(
        parse_module(&module),
        Err(ParseError::InvalidConstExprOpcode(0x20))
    );
}

#[test]
fn upstream_global_initializer_rejects_operations_after_constant_literal() {
    // These source-faithful invalid forms all begin with a valid literal but
    // continue with an instruction that is not part of the supported MVP
    // constant-expression subset. The parser must not silently stop after the
    // literal and accept the remaining operator.
    let cases = [
        (
            F32,
            vec![
                0x43, 0x00, 0x00, 0x00, 0x00, // f32.const 0
                0x8c, // f32.neg
                0x0b, // end
            ],
        ),
        (
            I32,
            vec![
                0x41, 0x00, // i32.const 0
                0x68, // i32.ctz
                0x0b, // end
            ],
        ),
        (
            I32,
            vec![
                0x41, 0x00, // i32.const 0
                0x01, // nop
                0x0b, // end
            ],
        ),
    ];

    for (value_type, initializer) in cases {
        let module = module_with_global(value_type, &initializer);
        assert_eq!(parse_module(&module), Err(ParseError::ConstExprMissingEnd));
    }
}

#[test]
fn upstream_global_initializer_rejects_declared_type_mismatch() {
    // (global i32 (f32.const 0))
    let module = module_with_global(
        I32,
        &[
            0x43, 0x00, 0x00, 0x00, 0x00, // f32.const 0
            0x0b, // end
        ],
    );

    assert_eq!(
        parse_module(&module),
        Err(ParseError::ConstExprTypeMismatch {
            expected: ValueType::I32,
            actual: ValueType::F32,
        })
    );
}
