use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, Value};

const I32: u8 = 0x7f;
const UPSTREAM_SPEC_COMMIT: &str = "fc209c5ed8afc4dfeb9252024d217da3376c7a6f";
const IMPORTED_GLOBAL: u8 = 0;
const X_GLOBAL: u8 = 1;

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

fn push_export(payload: &mut Vec<u8>, name: &str, function_index: u32) {
    push_name(payload, name);
    payload.push(0x00);
    push_u32(payload, function_index);
}

fn push_body(payload: &mut Vec<u8>, instructions: &[u8]) {
    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    push_u32(payload, body.len() as u32);
    payload.extend_from_slice(&body);
}

fn global_context_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    push_section(
        &mut module,
        1,
        &[
            0x03, // three types
            0x60, 0x00, 0x00, // [] -> []
            0x60, 0x00, 0x01, I32, // [] -> i32
            0x60, 0x01, I32, 0x01, I32, // [i32] -> i32
        ],
    );

    let mut imports = vec![0x01];
    push_name(&mut imports, "spectest");
    push_name(&mut imports, "global_i32");
    imports.extend([0x03, I32, 0x00]);
    push_section(&mut module, 2, &imports);

    let function_types = [
        0x00, // dummy
        0x02, // identity
        0x01, 0x01, 0x01, // loop first/mid/last
        0x01, 0x01, 0x01, // if condition/then/else
        0x01, 0x01, // br_if first/last
        0x01, // call value
        0x01, // return value
        0x01, // br value
        0x02, 0x02, // local.set / local.tee
        0x01, // global.set value
        0x01, // unary operand
        0x01, // binary operand
        0x01, // compare operand
    ];
    let mut functions = Vec::new();
    push_u32(&mut functions, function_types.len() as u32);
    functions.extend(function_types);
    push_section(&mut module, 3, &functions);

    // The upstream sequence sets $x from -12 to 6 before these context
    // assertions. This isolated translated fixture starts at that observed
    // state while retaining the imported global at index 0.
    push_section(
        &mut module,
        6,
        &[
            0x01, // one defined global
            I32, 0x01, // mutable i32
            0x41, 0x06, 0x0b, // i32.const 6; end
        ],
    );

    let exports = [
        ("as-loop-first", 2),
        ("as-loop-mid", 3),
        ("as-loop-last", 4),
        ("as-if-condition", 5),
        ("as-if-then", 6),
        ("as-if-else", 7),
        ("as-br_if-first", 8),
        ("as-br_if-last", 9),
        ("as-call-value", 10),
        ("as-return-value", 11),
        ("as-br-value", 12),
        ("as-local.set-value", 13),
        ("as-local.tee-value", 14),
        ("as-global.set-value", 15),
        ("as-unary-operand", 16),
        ("as-binary-operand", 17),
        ("as-compare-operand", 18),
    ];
    let mut export_section = Vec::new();
    push_u32(&mut export_section, exports.len() as u32);
    for (name, index) in exports {
        push_export(&mut export_section, name, index);
    }
    push_section(&mut module, 7, &export_section);

    let mut code = Vec::new();
    push_u32(&mut code, function_types.len() as u32);

    push_body(&mut code, &[]); // dummy
    push_body(&mut code, &[0x20, 0x00]); // identity

    push_body(
        &mut code,
        &[
            0x03, I32, // loop (result i32)
            0x23, X_GLOBAL, // global.get $x
            0x10, 0x00, // call $dummy
            0x10, 0x00, // call $dummy
            0x0b, // end loop
        ],
    );
    push_body(
        &mut code,
        &[
            0x03, I32, // loop (result i32)
            0x10, 0x00, // call $dummy
            0x23, X_GLOBAL, // global.get $x
            0x10, 0x00, // call $dummy
            0x0b, // end loop
        ],
    );
    push_body(
        &mut code,
        &[
            0x03, I32, // loop (result i32)
            0x10, 0x00, // call $dummy
            0x10, 0x00, // call $dummy
            0x23, X_GLOBAL, // global.get $x
            0x0b,     // end loop
        ],
    );

    push_body(
        &mut code,
        &[
            0x23, X_GLOBAL, // global.get $x
            0x04, I32, // if (result i32)
            0x10, 0x00, // call $dummy
            0x41, 0x02, // i32.const 2
            0x05, // else
            0x10, 0x00, // call $dummy
            0x41, 0x03, // i32.const 3
            0x0b, // end if
        ],
    );
    push_body(
        &mut code,
        &[
            0x41, 0x01, // i32.const 1
            0x04, I32, // if (result i32)
            0x23, X_GLOBAL, // global.get $x
            0x05,     // else
            0x41, 0x02, // i32.const 2
            0x0b, // end if
        ],
    );
    push_body(
        &mut code,
        &[
            0x41, 0x00, // i32.const 0
            0x04, I32, // if (result i32)
            0x41, 0x02, // i32.const 2
            0x05, // else
            0x23, X_GLOBAL, // global.get $x
            0x0b,     // end if
        ],
    );

    push_body(
        &mut code,
        &[
            0x02, I32, // block (result i32)
            0x23, X_GLOBAL, // branch value
            0x41, 0x02, // true condition
            0x0d, 0x00, // br_if 0
            0x41, 0x03, // i32.const 3
            0x0f, // return
            0x0b, // end block
        ],
    );
    push_body(
        &mut code,
        &[
            0x02, I32, // block (result i32)
            0x41, 0x02, // branch value
            0x23, X_GLOBAL, // true condition
            0x0d, 0x00, // br_if 0
            0x41, 0x03, // i32.const 3
            0x0f, // return
            0x0b, // end block
        ],
    );

    push_body(
        &mut code,
        &[
            0x23, X_GLOBAL, // global.get $x
            0x10, 0x01, // call $identity
        ],
    );
    push_body(
        &mut code,
        &[
            0x23, X_GLOBAL, // global.get $x
            0x0f,     // return
        ],
    );
    push_body(
        &mut code,
        &[
            0x02, I32, // block (result i32)
            0x23, X_GLOBAL, // global.get $x
            0x0c, 0x00, // br 0
            0x0b, // end block
        ],
    );

    push_body(
        &mut code,
        &[
            0x23, X_GLOBAL, // global.get $x
            0x21, 0x00, // local.set 0
            0x20, 0x00, // local.get 0
        ],
    );
    push_body(
        &mut code,
        &[
            0x23, X_GLOBAL, // global.get $x
            0x22, 0x00, // local.tee 0
        ],
    );
    push_body(
        &mut code,
        &[
            0x23, X_GLOBAL, // global.get $x
            0x24, X_GLOBAL, // global.set $x
            0x23, X_GLOBAL, // global.get $x
        ],
    );

    push_body(
        &mut code,
        &[
            0x23, X_GLOBAL, // global.get $x
            0x45,     // i32.eqz
        ],
    );
    push_body(
        &mut code,
        &[
            0x23, X_GLOBAL, // global.get $x
            0x23, X_GLOBAL, // global.get $x
            0x6c,     // i32.mul
        ],
    );
    push_body(
        &mut code,
        &[
            0x23,
            IMPORTED_GLOBAL, // global.get 0 (spectest.global_i32 = 666)
            0x41,
            0x01, // i32.const 1
            0x4b, // i32.gt_u
        ],
    );

    push_section(&mut module, 10, &code);
    module
}

fn instance() -> Instance {
    let module = parse_module(&global_context_module()).expect("global context vector must parse");
    let mut hosts = HostRegistry::new();
    hosts
        .register_immutable_global("spectest", "global_i32", Value::I32(666))
        .expect("register spectest global_i32");
    Instance::with_hosts(module, hosts).expect("global context vector must instantiate")
}

#[test]
fn upstream_global_get_control_context_vectors_execute() {
    assert_eq!(UPSTREAM_SPEC_COMMIT.len(), 40);
    let mut vm = instance();

    for (name, expected) in [
        ("as-loop-first", 6),
        ("as-loop-mid", 6),
        ("as-loop-last", 6),
        ("as-if-condition", 2),
        ("as-if-then", 6),
        ("as-if-else", 6),
        ("as-br_if-first", 6),
        ("as-br_if-last", 2),
        ("as-call-value", 6),
        ("as-return-value", 6),
        ("as-br-value", 6),
    ] {
        assert_eq!(
            vm.invoke_export(name, &[])
                .expect("supported global context must execute"),
            Some(Value::I32(expected)),
            "unexpected result for {name}"
        );
    }
}

#[test]
fn upstream_global_get_operand_context_vectors_execute() {
    let mut vm = instance();

    for (name, args, expected) in [
        ("as-local.set-value", vec![Value::I32(1)], 6),
        ("as-local.tee-value", vec![Value::I32(1)], 6),
        ("as-global.set-value", vec![], 6),
        ("as-unary-operand", vec![], 0),
        ("as-binary-operand", vec![], 36),
        ("as-compare-operand", vec![], 1),
    ] {
        assert_eq!(
            vm.invoke_export(name, &args)
                .expect("supported global operand context must execute"),
            Some(Value::I32(expected)),
            "unexpected result for {name}"
        );
    }
}
