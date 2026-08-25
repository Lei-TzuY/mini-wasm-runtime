use wasm_parser::parse_module;
use wasm_validator::{validate, ValidationError};

// Curated from WebAssembly/spec test/core/global.wast at
// fc209c5ed8afc4dfeb9252024d217da3376c7a6f.
const I32: u8 = 0x7f;
const F32: u8 = 0x7d;

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

fn push_name(bytes: &mut Vec<u8>, name: &str) {
    push_u32(bytes, name.len() as u32);
    bytes.extend_from_slice(name.as_bytes());
}

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    push_u32(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn immutable_i32_import() -> Vec<u8> {
    let mut payload = vec![0x01];
    push_name(&mut payload, "spectest");
    push_name(&mut payload, "global_i32");
    payload.extend([0x03, I32, 0x00]);
    payload
}

fn immutable_i32_global() -> Vec<u8> {
    vec![
        0x01, // one global
        I32, 0x00, // immutable i32
        0x41, 0x00, 0x0b, // i32.const 0; end
    ]
}

fn immutable_f32_global() -> Vec<u8> {
    vec![
        0x01, // one global
        F32, 0x00, // immutable f32
        0x43, 0x00, 0x00, 0x00, 0x00, 0x0b, // f32.const 0; end
    ]
}

fn module_with_function(
    result_type: Option<u8>,
    import_section: Option<&[u8]>,
    global_section: Option<&[u8]>,
    instructions: &[u8],
) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    let mut type_section = vec![0x01, 0x60, 0x00];
    match result_type {
        Some(value_type) => type_section.extend([0x01, value_type]),
        None => type_section.push(0x00),
    }
    push_section(&mut module, 1, &type_section);

    if let Some(imports) = import_section {
        push_section(&mut module, 2, imports);
    }

    push_section(&mut module, 3, &[0x01, 0x00]);

    if let Some(globals) = global_section {
        push_section(&mut module, 6, globals);
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
    let parsed = parse_module(module).expect("negative global vector must remain structurally valid");
    validate(&parsed).expect_err(expectation)
}

fn assert_global_index_oob(module: &[u8], expected_index: u32) {
    assert!(matches!(
        validation_error(module, "missing global index must fail closed"),
        ValidationError::GlobalIndexOutOfBounds {
            function: 0,
            global_index,
            ..
        } if global_index == expected_index
    ));
}

#[test]
fn upstream_global_get_rejects_missing_indices_across_global_index_space() {
    let defined = immutable_i32_global();
    let imported = immutable_i32_import();

    let cases = [
        (
            module_with_function(Some(I32), None, None, &[0x23, 0x00]),
            0,
        ),
        (
            module_with_function(Some(I32), None, Some(&defined), &[0x23, 0x01]),
            1,
        ),
        (
            module_with_function(Some(I32), Some(&imported), None, &[0x23, 0x01]),
            1,
        ),
        (
            module_with_function(
                Some(I32),
                Some(&imported),
                Some(&defined),
                &[0x23, 0x02],
            ),
            2,
        ),
    ];

    for (module, expected_index) in cases {
        assert_global_index_oob(&module, expected_index);
    }
}

#[test]
fn upstream_global_set_rejects_missing_indices_across_global_index_space() {
    let defined = immutable_i32_global();
    let imported = immutable_i32_import();

    let cases = [
        (
            module_with_function(None, None, None, &[0x41, 0x00, 0x24, 0x00]),
            0,
        ),
        (
            module_with_function(
                None,
                None,
                Some(&defined),
                &[0x41, 0x00, 0x24, 0x01],
            ),
            1,
        ),
        (
            module_with_function(
                None,
                Some(&imported),
                None,
                &[0x41, 0x00, 0x24, 0x01],
            ),
            1,
        ),
        (
            module_with_function(
                None,
                Some(&imported),
                Some(&defined),
                &[0x41, 0x00, 0x24, 0x02],
            ),
            2,
        ),
    ];

    for (module, expected_index) in cases {
        assert_global_index_oob(&module, expected_index);
    }
}

#[test]
fn upstream_global_set_rejects_defined_immutable_global() {
    let global = immutable_f32_global();
    let module = module_with_function(
        None,
        None,
        Some(&global),
        &[
            0x43, 0x00, 0x00, 0x80, 0x3f, // f32.const 1
            0x24, 0x00, // global.set 0
        ],
    );

    assert!(matches!(
        validation_error(&module, "defined immutable globals must reject global.set"),
        ValidationError::ImmutableGlobalSet {
            function: 0,
            global_index: 0,
            ..
        }
    ));
}

#[test]
fn upstream_global_set_rejects_imported_immutable_global() {
    let imported = immutable_i32_import();
    let module = module_with_function(
        None,
        Some(&imported),
        None,
        &[
            0x41, 0x01, // i32.const 1
            0x24, 0x00, // global.set 0
        ],
    );

    assert!(matches!(
        validation_error(&module, "imported immutable globals must reject global.set"),
        ValidationError::ImmutableGlobalSet {
            function: 0,
            global_index: 0,
            ..
        }
    ));
}
