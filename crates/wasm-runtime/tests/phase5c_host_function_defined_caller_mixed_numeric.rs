use wasm_parser::{parse_module, ValueType};
use wasm_runtime::{HostCapabilities, HostRegistry, Instance, Value};

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    assert!(
        payload.len() < 128,
        "fixture helper only needs one-byte section lengths"
    );
    module.push(id);
    module.push(payload.len() as u8);
    module.extend_from_slice(payload);
}

fn defined_caller_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    // type 0: () -> f64
    push_section(&mut module, 1, &[0x01, 0x60, 0x00, 0x01, 0x7c]);

    // import env.host : type 0
    let mut import = vec![0x01, 0x03];
    import.extend_from_slice(b"env");
    import.push(0x04);
    import.extend_from_slice(b"host");
    import.extend_from_slice(&[0x00, 0x00]);
    push_section(&mut module, 2, &import);

    // one defined function, also type 0
    push_section(&mut module, 3, &[0x01, 0x00]);

    // export defined function index 1 as "run"
    let mut export = vec![0x01, 0x03];
    export.extend_from_slice(b"run");
    export.extend_from_slice(&[0x00, 0x01]);
    push_section(&mut module, 7, &export);

    // body: call host; f64.const 1.5; f64.add; end
    let mut body = vec![0x00, 0x10, 0x00, 0x44];
    body.extend_from_slice(&1.5_f64.to_le_bytes());
    body.extend_from_slice(&[0xa0, 0x0b]);

    let mut code = vec![0x01, body.len() as u8];
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);
    module
}

fn defined_caller_with_mixed_params_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    // type 0: (i64, f32, f64) -> f64
    push_section(
        &mut module,
        1,
        &[0x01, 0x60, 0x03, 0x7e, 0x7d, 0x7c, 0x01, 0x7c],
    );

    // import env.host : type 0
    let mut import = vec![0x01, 0x03];
    import.extend_from_slice(b"env");
    import.push(0x04);
    import.extend_from_slice(b"host");
    import.extend_from_slice(&[0x00, 0x00]);
    push_section(&mut module, 2, &import);

    // one defined function, also type 0
    push_section(&mut module, 3, &[0x01, 0x00]);

    // export defined function index 1 as "run"
    let mut export = vec![0x01, 0x03];
    export.extend_from_slice(b"run");
    export.extend_from_slice(&[0x00, 0x01]);
    push_section(&mut module, 7, &export);

    // body: forward all parameters to host; add 1.5 to the f64 host result; end
    let mut body = vec![
        0x00, // local declaration count
        0x20, 0x00, // local.get 0 (i64)
        0x20, 0x01, // local.get 1 (f32)
        0x20, 0x02, // local.get 2 (f64)
        0x10, 0x00, // call imported host
        0x44, // f64.const
    ];
    body.extend_from_slice(&1.5_f64.to_le_bytes());
    body.extend_from_slice(&[0xa0, 0x0b]); // f64.add; end

    let mut code = vec![0x01, body.len() as u8];
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);
    module
}

#[test]
fn defined_wasm_caller_consumes_f64_host_result_in_typed_execution() {
    let module = parse_module(&defined_caller_module())
        .expect("mixed-numeric defined-caller fixture must remain parseable");
    let mut hosts = HostRegistry::new();
    hosts
        .register(
            "env",
            "host",
            vec![],
            vec![ValueType::F64],
            HostCapabilities::NONE,
            |_context, _args| Ok(Some(Value::F64(40.5))),
        )
        .expect("single-result f64 host signature should be admitted");

    let mut instance = Instance::with_hosts(module, hosts).expect("host binding must instantiate");
    let result = instance
        .invoke_export("run", &[])
        .expect("defined Wasm caller should consume the typed host result");

    let Some(Value::F64(value)) = result else {
        panic!("defined caller should return f64");
    };
    assert_eq!(value, 42.0);
}

#[test]
fn defined_wasm_caller_forwards_mixed_numeric_args_to_host_bit_exactly() {
    const F32_BITS: u32 = 0x7fc1_2345;
    const F64_BITS: u64 = 0x8000_0000_0000_0000;

    let module = parse_module(&defined_caller_with_mixed_params_module())
        .expect("mixed-parameter defined-caller fixture must remain parseable");
    let mut hosts = HostRegistry::new();
    hosts
        .register(
            "env",
            "host",
            vec![ValueType::I64, ValueType::F32, ValueType::F64],
            vec![ValueType::F64],
            HostCapabilities::NONE,
            |_context, args| {
                assert!(matches!(args[0], Value::I64(-9)));
                let Value::F32(f32_value) = args[1] else {
                    panic!("second host argument must remain f32");
                };
                let Value::F64(f64_value) = args[2] else {
                    panic!("third host argument must remain f64");
                };
                assert_eq!(f32_value.to_bits(), F32_BITS);
                assert_eq!(f64_value.to_bits(), F64_BITS);
                Ok(Some(Value::F64(40.5)))
            },
        )
        .expect("mixed-numeric host signature should be admitted");

    let mut instance = Instance::with_hosts(module, hosts).expect("host binding must instantiate");
    let result = instance
        .invoke_export(
            "run",
            &[
                Value::I64(-9),
                Value::F32(f32::from_bits(F32_BITS)),
                Value::F64(f64::from_bits(F64_BITS)),
            ],
        )
        .expect("defined Wasm caller should forward mixed numeric arguments");

    let Some(Value::F64(value)) = result else {
        panic!("defined caller should return f64");
    };
    assert_eq!(value, 42.0);
}
