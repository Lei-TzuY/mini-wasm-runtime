use wasm_parser::{parse_module, ValueType};
use wasm_runtime::{
    HostCapabilities, HostRegistry, Instance, RuntimeError, RuntimeLimits, Value,
};
use wasm_validator::ValidationError;

fn u32leb(out: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    u32leb(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn one_body_section(body: &[u8]) -> Vec<u8> {
    let mut code = vec![1];
    u32leb(&mut code, body.len() as u32);
    code.extend_from_slice(body);
    code
}

fn recursive_module(call_opcode: u8) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(&mut module, 1, &[1, 0x60, 1, 0x7f, 1, 0x7f]);
    section(&mut module, 3, &[1, 0]);
    section(&mut module, 7, &[1, 3, b'r', b'u', b'n', 0, 0]);

    let body = [
        0, // local declaration count
        0x20, 0, // local.get 0
        0x45, // i32.eqz
        0x04, 0x7f, // if (result i32)
        0x41, 0, // i32.const 0
        0x05, // else
        0x20, 0, // local.get 0
        0x41, 1, // i32.const 1
        0x6b, // i32.sub
        call_opcode, 0, // call/return_call function 0
        0x0b, // end if
        0x0b, // end function
    ];
    section(&mut module, 10, &one_body_section(&body));
    module
}

fn indirect_second_table_module() -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(&mut module, 1, &[1, 0x60, 1, 0x7f, 1, 0x7f]);
    section(&mut module, 3, &[2, 0, 0]);
    section(
        &mut module,
        4,
        &[2, 0x70, 1, 1, 1, 0x70, 1, 1, 1],
    );
    section(&mut module, 7, &[1, 3, b'r', b'u', b'n', 0, 1]);
    section(
        &mut module,
        9,
        &[1, 2, 1, 0x41, 0, 0x0b, 0, 1, 0],
    );

    let target = [
        0, // local declaration count
        0x20, 0, // local.get 0
        0x41, 1, // i32.const 1
        0x6a, // i32.add
        0x0b,
    ];
    let caller = [
        0, // local declaration count
        0x20, 0, // tail-call parameter
        0x41, 0, // table element index
        0x13, 0, 1, // return_call_indirect type 0 table 1
        0x0b,
    ];

    let mut code = vec![2];
    for body in [&target[..], &caller[..]] {
        u32leb(&mut code, body.len() as u32);
        code.extend_from_slice(body);
    }
    section(&mut module, 10, &code);
    module
}

fn mismatched_return_module() -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(
        &mut module,
        1,
        &[
            2,
            0x60, 1, 0x7f, 1, 0x7e, // type 0: (i32) -> i64
            0x60, 1, 0x7f, 1, 0x7f, // type 1: (i32) -> i32
        ],
    );
    section(&mut module, 3, &[2, 0, 1]);
    section(&mut module, 7, &[1, 3, b'r', b'u', b'n', 0, 1]);

    let callee = [0, 0x42, 7, 0x0b]; // i64.const 7
    let caller = [0, 0x20, 0, 0x12, 0, 0x0b];
    let mut code = vec![2];
    for body in [&callee[..], &caller[..]] {
        u32leb(&mut code, body.len() as u32);
        code.extend_from_slice(body);
    }
    section(&mut module, 10, &code);
    module
}

fn shallow_stack_limits() -> RuntimeLimits {
    RuntimeLimits {
        max_call_depth: 1,
        fuel: Some(100_000),
        ..RuntimeLimits::default()
    }
}

#[test]
fn direct_tail_recursion_reuses_the_current_runtime_frame() {
    let module = parse_module(&recursive_module(0x12)).expect("tail-call module must parse");
    let mut instance =
        Instance::with_config(module, Default::default(), shallow_stack_limits())
            .expect("tail-call module must validate");

    assert_eq!(
        instance
            .invoke_export("run", &[Value::I32(5_000)])
            .expect("tail recursion must not consume call depth"),
        Some(Value::I32(0))
    );
}

#[test]
fn ordinary_recursion_still_obeys_the_call_depth_limit() {
    let module = parse_module(&recursive_module(0x10)).expect("ordinary-call module must parse");
    let mut instance =
        Instance::with_config(module, Default::default(), shallow_stack_limits())
            .expect("ordinary-call module must validate");

    assert!(matches!(
        instance.invoke_export("run", &[Value::I32(2)]),
        Err(RuntimeError::CallDepthExceeded { limit: 1 })
    ));
}

#[test]
fn return_call_indirect_dispatches_through_a_nonzero_table() {
    let module =
        parse_module(&indirect_second_table_module()).expect("indirect tail-call module must parse");
    let mut instance = Instance::new(module).expect("indirect tail-call module must validate");

    assert_eq!(
        instance.invoke_export("run", &[Value::I32(41)]).unwrap(),
        Some(Value::I32(42))
    );
}

#[test]
fn tail_call_result_type_must_match_the_current_function_result() {
    let module = parse_module(&mismatched_return_module()).expect("mismatch module must parse");
    assert!(matches!(
        Instance::new(module),
        Err(RuntimeError::Validation(
            ValidationError::TailCallResultTypeMismatch {
                expected,
                actual,
                ..
            }
        )) if expected == vec![ValueType::I32] && actual == vec![ValueType::I64]
    ));
}


fn name(out: &mut Vec<u8>, value: &str) {
    u32leb(out, value.len() as u32);
    out.extend_from_slice(value.as_bytes());
}

fn multi_value_tail_module() -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(
        &mut module,
        1,
        &[1, 0x60, 1, 0x7f, 2, 0x7f, 0x7e],
    );
    section(&mut module, 3, &[2, 0, 0]);
    section(&mut module, 7, &[1, 3, b'r', b'u', b'n', 0, 1]);

    let target = [
        0, // local declaration count
        0x20, 0, // local.get 0
        0x42, 9, // i64.const 9
        0x0b,
    ];
    let caller = [
        0, // local declaration count
        0x20, 0, // local.get 0
        0x12, 0, // return_call function 0
        0x0b,
    ];
    let mut code = vec![2];
    for body in [&target[..], &caller[..]] {
        u32leb(&mut code, body.len() as u32);
        code.extend_from_slice(body);
    }
    section(&mut module, 10, &code);
    module
}

fn imported_tail_module() -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(&mut module, 1, &[1, 0x60, 1, 0x7f, 1, 0x7f]);

    let mut imports = vec![1];
    name(&mut imports, "env");
    name(&mut imports, "host");
    imports.extend([0, 0]);
    section(&mut module, 2, &imports);

    section(&mut module, 3, &[1, 0]);
    section(&mut module, 7, &[1, 3, b'r', b'u', b'n', 0, 1]);
    let body = [
        0, // local declaration count
        0x20, 0, // local.get 0
        0x12, 0, // return_call imported function 0
        0x0b,
    ];
    section(&mut module, 10, &one_body_section(&body));
    module
}

#[test]
fn direct_tail_call_forwards_ordered_multi_value_results() {
    let module = parse_module(&multi_value_tail_module()).expect("multi-value tail module parses");
    let mut instance = Instance::new(module).expect("multi-value tail module validates");

    assert_eq!(
        instance
            .invoke_export_values("run", &[Value::I32(7)])
            .expect("multi-value tail call executes"),
        vec![Value::I32(7), Value::I64(9)]
    );
}

#[test]
fn direct_tail_call_can_finish_in_an_imported_host_function() {
    let module = parse_module(&imported_tail_module()).expect("imported tail module parses");
    let mut hosts = HostRegistry::new();
    hosts
        .register_values(
            "env",
            "host",
            vec![ValueType::I32],
            vec![ValueType::I32],
            HostCapabilities::NONE,
            |_context, args| Ok(vec![Value::I32(args[0].as_i32().wrapping_add(5))]),
        )
        .unwrap();

    let mut instance =
        Instance::with_config(module, hosts, shallow_stack_limits()).expect("tail host binding");
    assert_eq!(
        instance
            .invoke_export("run", &[Value::I32(37)])
            .expect("tail host call executes"),
        Some(Value::I32(42))
    );
}
