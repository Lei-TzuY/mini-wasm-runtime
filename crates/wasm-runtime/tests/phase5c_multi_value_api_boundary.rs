use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};

const I32: u8 = 0x7f;
const I64: u8 = 0x7e;

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

fn side_effecting_multi_result_module() -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    push_section(
        &mut module,
        1,
        &[0x02, 0x60, 0x00, 0x02, I32, I64, 0x60, 0x00, 0x01, I32],
    );
    push_section(&mut module, 3, &[0x02, 0x00, 0x01]);
    push_section(&mut module, 6, &[0x01, I32, 0x01, 0x41, 0x00, 0x0b]);
    push_section(
        &mut module,
        7,
        &[
            0x02, 0x03, b'r', b'u', b'n', 0x00, 0x00, 0x03, b'g', b'e', b't', 0x00, 0x01,
        ],
    );

    let run_body = [0x00, 0x41, 0x01, 0x24, 0x00, 0x41, 0x07, 0x42, 0x09, 0x0b];
    let get_body = [0x00, 0x23, 0x00, 0x0b];
    let mut code = vec![0x02];
    push_u32(&mut code, run_body.len() as u32);
    code.extend_from_slice(&run_body);
    push_u32(&mut code, get_body.len() as u32);
    code.extend_from_slice(&get_body);
    push_section(&mut module, 10, &code);

    module
}

#[test]
fn legacy_multi_value_api_rejects_before_function_side_effects() {
    let module = parse_module(&side_effecting_multi_result_module()).expect("fixture must parse");
    let mut instance = Instance::new(module).expect("fixture must validate and instantiate");

    assert!(matches!(
        instance.invoke_export("run", &[]),
        Err(RuntimeError::MultiValueResultRequiresValuesApi { results: 2 })
    ));
    assert_eq!(
        instance.invoke_export("get", &[]).unwrap(),
        Some(Value::I32(0)),
        "legacy API rejection must occur before global.set executes"
    );

    assert_eq!(
        instance.invoke_export_values("run", &[]).unwrap(),
        vec![Value::I32(7), Value::I64(9)]
    );
    assert_eq!(
        instance.invoke_export("get", &[]).unwrap(),
        Some(Value::I32(1)),
        "fixture must actually perform the side effect when executed through the values API"
    );
}
