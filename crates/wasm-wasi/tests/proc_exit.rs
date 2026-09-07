use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, RuntimeError, Value};
use wasm_wasi::{WasiInvocationOutcome, WasiPreview1};

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

fn i32leb(out: &mut Vec<u8>, mut value: i32) {
    loop {
        let byte = (value as u8) & 0x7f;
        value >>= 7;
        let done = (value == 0 && byte & 0x40 == 0) || (value == -1 && byte & 0x40 != 0);
        out.push(if done { byte } else { byte | 0x80 });
        if done {
            break;
        }
    }
}

fn name(out: &mut Vec<u8>, value: &str) {
    u32leb(out, value.len() as u32);
    out.extend_from_slice(value.as_bytes());
}

fn section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id);
    u32leb(module, payload.len() as u32);
    module.extend_from_slice(payload);
}

fn body(code: Vec<u8>) -> Vec<u8> {
    let mut body = vec![0];
    body.extend(code);
    let mut encoded = Vec::new();
    u32leb(&mut encoded, body.len() as u32);
    encoded.extend(body);
    encoded
}

fn proc_exit_module(exit_code: i32, nested: bool) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    let types = [2, 0x60, 1, 0x7f, 0, 0x60, 0, 0];
    section(&mut module, 1, &types);

    let mut imports = vec![1];
    name(&mut imports, "wasi_snapshot_preview1");
    name(&mut imports, "proc_exit");
    imports.extend([0, 0]);
    section(&mut module, 2, &imports);

    let functions = if nested {
        vec![2, 1, 1]
    } else {
        vec![1, 1]
    };
    section(&mut module, 3, &functions);
    section(&mut module, 6, &[1, 0x7f, 1, 0x41, 0, 0x0b]);

    let mut exports = vec![1];
    name(&mut exports, "run");
    exports.extend([0, if nested { 2 } else { 1 }]);
    section(&mut module, 7, &exports);

    let mut code = Vec::new();
    if nested {
        code.push(2);
        let mut helper = vec![0x41];
        i32leb(&mut helper, exit_code);
        helper.extend([0x10, 0, 0x0b]);
        code.extend(body(helper));
        code.extend(body(vec![0x10, 1, 0x41, 99, 0x24, 0, 0x0b]));
    } else {
        code.push(1);
        let mut run = vec![0x41];
        i32leb(&mut run, exit_code);
        run.extend([0x10, 0, 0x41, 99, 0x24, 0, 0x0b]);
        code.extend(body(run));
    }
    section(&mut module, 10, &code);
    module
}

fn returning_module(value: i32) -> Vec<u8> {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    section(&mut module, 1, &[1, 0x60, 0, 1, 0x7f]);
    section(&mut module, 3, &[1, 0]);

    let mut exports = vec![1];
    name(&mut exports, "run");
    exports.extend([0, 0]);
    section(&mut module, 7, &exports);

    let mut run = vec![0x41];
    i32leb(&mut run, value);
    run.push(0x0b);
    let mut code = vec![1];
    code.extend(body(run));
    section(&mut module, 10, &code);
    module
}

fn instantiate(bytes: &[u8], wasi: &WasiPreview1) -> Instance {
    let mut hosts = HostRegistry::new();
    wasi.register(&mut hosts).unwrap();
    Instance::with_hosts(parse_module(bytes).unwrap(), hosts).unwrap()
}

#[test]
fn proc_exit_is_a_typed_non_error_outcome_and_stops_following_guest_code() {
    let wasi = WasiPreview1::new();
    let mut vm = instantiate(&proc_exit_module(37, false), &wasi);

    assert_eq!(
        wasi.invoke_export_values(&mut vm, "run", &[]).unwrap(),
        WasiInvocationOutcome::Exited(37)
    );
    assert_eq!(vm.global(0), Some(Value::I32(0)));
}

#[test]
fn proc_exit_propagates_through_nested_guest_calls() {
    let wasi = WasiPreview1::new();
    let mut vm = instantiate(&proc_exit_module(23, true), &wasi);

    assert_eq!(
        wasi.invoke_export_values(&mut vm, "run", &[]).unwrap(),
        WasiInvocationOutcome::Exited(23)
    );
    assert_eq!(vm.global(0), Some(Value::I32(0)));
}

#[test]
fn normal_guest_return_remains_distinct_from_process_exit() {
    let wasi = WasiPreview1::new();
    let mut vm = instantiate(&returning_module(42), &wasi);

    assert_eq!(
        wasi.invoke_export_values(&mut vm, "run", &[]).unwrap(),
        WasiInvocationOutcome::Returned(vec![Value::I32(42)])
    );
}

#[test]
fn raw_runtime_path_preserves_legacy_host_failure_contract() {
    let wasi = WasiPreview1::new();
    let mut vm = instantiate(&proc_exit_module(9, false), &wasi);

    assert!(matches!(
        vm.invoke_export_values("run", &[]),
        Err(RuntimeError::HostCallFailed { module, name, .. })
            if module == "wasi_snapshot_preview1" && name == "proc_exit"
    ));
    assert_eq!(vm.global(0), Some(Value::I32(0)));

    assert_eq!(
        wasi.invoke_export_values(&mut vm, "run", &[]).unwrap(),
        WasiInvocationOutcome::Exited(9)
    );
}
