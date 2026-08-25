use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError};
use wasm_validator::ValidationError;

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

fn header() -> Vec<u8> {
    vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
}

fn module_with_function(
    instructions: &[u8],
    table: Option<&[u8]>,
    memory: Option<&[u8]>,
    global: Option<&[u8]>,
) -> Vec<u8> {
    let mut module = header();
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x00]);
    push_section(&mut module, 3, &[0x01, 0x00]);
    if let Some(table) = table {
        push_section(&mut module, 4, table);
    }
    if let Some(memory) = memory {
        push_section(&mut module, 5, memory);
    }
    if let Some(global) = global {
        push_section(&mut module, 6, global);
    }

    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);
    module
}

fn validation_error(module: &[u8], expectation: &str) -> ValidationError {
    let module = parse_module(module).expect("negative fixture must remain structurally parseable");
    match Instance::new(module).expect_err(expectation) {
        RuntimeError::Validation(error) => error,
        other => panic!("expected validator rejection, got {other:?}"),
    }
}

#[test]
fn global_get_index_must_exist() {
    let module = module_with_function(&[0x23, 0x00], None, None, None);
    assert!(matches!(
        validation_error(
            &module,
            "global.get must stay inside the global index space"
        ),
        ValidationError::GlobalIndexOutOfBounds {
            function: 0,
            global_index: 0,
            ..
        }
    ));
}

#[test]
fn global_set_rejects_immutable_global() {
    let immutable_i32_global = [
        0x01, // one global
        0x7f, 0x00, // immutable i32
        0x41, 0x00, 0x0b, // i32.const 0; end
    ];
    let module = module_with_function(
        &[
            0x41, 0x00, // i32.const 0
            0x24, 0x00, // global.set 0
        ],
        None,
        None,
        Some(&immutable_i32_global),
    );
    assert!(matches!(
        validation_error(&module, "global.set must reject immutable globals"),
        ValidationError::ImmutableGlobalSet {
            function: 0,
            global_index: 0,
            ..
        }
    ));
}

#[test]
fn call_indirect_table_index_must_exist() {
    let module = module_with_function(
        &[
            0x41, 0x00, // selector
            0x11, 0x00, 0x00, // call_indirect type 0, missing table 0
        ],
        None,
        None,
        None,
    );
    assert!(matches!(
        validation_error(&module, "call_indirect must not target a missing table"),
        ValidationError::TableIndexOutOfBounds {
            function: 0,
            table_index: 0,
            ..
        }
    ));
}

#[test]
fn call_indirect_type_index_must_exist() {
    let table = [
        0x01, // one table
        0x70, // funcref
        0x00, 0x01, // min=1, no maximum
    ];
    let module = module_with_function(
        &[
            0x41, 0x00, // selector
            0x11, 0x01, 0x00, // missing type 1, table 0
        ],
        Some(&table),
        None,
        None,
    );
    assert!(matches!(
        validation_error(&module, "call_indirect must not refer to a missing type"),
        ValidationError::IndirectTypeIndexOutOfBounds {
            function: 0,
            type_index: 1,
            ..
        }
    ));
}

#[test]
fn memory_size_index_must_exist() {
    let memory = [
        0x01, // one memory
        0x00, 0x01, // min=1, no maximum
    ];
    let module = module_with_function(
        &[
            0x3f, 0x01, // memory.size 1, but only memory 0 exists
        ],
        None,
        Some(&memory),
        None,
    );
    assert!(matches!(
        validation_error(&module, "memory.size must use an existing memory index"),
        ValidationError::MemoryIndexOutOfBounds {
            function: 0,
            memory_index: 1,
            ..
        }
    ));
}

#[test]
fn memory_access_alignment_cannot_exceed_natural_alignment() {
    let memory = [
        0x01, // one memory
        0x00, 0x01, // min=1, no maximum
    ];
    let module = module_with_function(
        &[
            0x41, 0x00, // address
            0x28, 0x03, 0x00, // i32.load align=3, offset=0; natural align is 2
        ],
        None,
        Some(&memory),
        None,
    );
    assert!(matches!(
        validation_error(&module, "memory access must reject over-aligned memargs"),
        ValidationError::InvalidMemoryAlignment {
            function: 0,
            alignment: 3,
            maximum: 2,
            ..
        }
    ));
}

#[test]
fn malformed_memory_immediate_fails_closed() {
    let memory = [
        0x01, // one memory
        0x00, 0x01, // min=1, no maximum
    ];
    let module = module_with_function(
        &[
            0x41, 0x00, // address
            0x28, // i32.load
            0x80, 0x80, 0x80, 0x80, 0x80, // unterminated u32 LEB128 alignment
        ],
        None,
        Some(&memory),
        None,
    );
    assert!(matches!(
        validation_error(&module, "malformed memarg must fail closed"),
        ValidationError::MalformedImmediate { function: 0, .. }
    ));
}
