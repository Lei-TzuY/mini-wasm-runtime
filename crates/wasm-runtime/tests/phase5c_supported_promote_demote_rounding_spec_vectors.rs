use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value};

const F32: u8 = 0x7d;
const F64: u8 = 0x7c;
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

fn single_result_module(result_type: u8, instructions: &[u8]) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(
        &mut module,
        1,
        &[0x01, 0x60, 0x00, 0x01, result_type],
    );
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);

    let mut body = vec![0x00];
    body.extend_from_slice(instructions);
    body.push(0x0b);
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);
    module
}

fn invoke(module: &[u8]) -> Value {
    let module = parse_module(module).expect("pinned conversion vector must parse");
    let mut instance = Instance::new(module).expect("pinned conversion vector must validate");
    instance
        .invoke_export("run", &[])
        .expect("pinned conversion vector must execute")
        .expect("pinned conversion vector must return one value")
}

fn promote_module(bits: u32) -> Vec<u8> {
    let mut instructions = vec![0x43]; // f32.const
    instructions.extend_from_slice(&bits.to_le_bytes());
    instructions.push(0xbb); // f64.promote_f32
    single_result_module(F64, &instructions)
}

fn demote_module(bits: u64) -> Vec<u8> {
    let mut instructions = vec![0x44]; // f64.const
    instructions.extend_from_slice(&bits.to_le_bytes());
    instructions.push(0xb6); // f32.demote_f64
    single_result_module(F32, &instructions)
}

#[test]
fn pinned_upstream_promote_f32_preserves_finite_extrema_subnormals_and_infinities() {
    // WebAssembly/spec test/core/conversions.wast @ the pinned revision.
    let cases = [
        (0x0000_0001u32, 0x36a0_0000_0000_0000u64), // +min subnormal
        (0x8000_0001, 0xb6a0_0000_0000_0000),       // -min subnormal
        (0x7f7f_ffff, 0x47ef_ffff_e000_0000),       // +max finite
        (0xff7f_ffff, 0xc7ef_ffff_e000_0000),       // -max finite
        (0x0400_0000, 0x3880_0000_0000_0000),       // 0x1p-119
        (0x7e47_c33f, 0x47c8_f867_e000_0000),       // 0x1.8f867ep+125
        (0x7f80_0000, 0x7ff0_0000_0000_0000),       // +inf
        (0xff80_0000, 0xfff0_0000_0000_0000),       // -inf
    ];

    for (source_bits, expected_bits) in cases {
        match invoke(&promote_module(source_bits)) {
            Value::F64(value) => assert_eq!(
                value.to_bits(),
                expected_bits,
                "WebAssembly/spec@{UPSTREAM_SPEC_COMMIT}: f64.promote_f32 source=0x{source_bits:08x}"
            ),
            other => panic!("expected f64 result, got {other:?}"),
        }
    }
}

#[test]
fn pinned_upstream_demote_f64_rounding_boundaries_match_spec() {
    // Source-faithful bit encodings for the corresponding hexadecimal-float assertions in
    // WebAssembly/spec test/core/conversions.wast @ the pinned revision.
    let cases = [
        (0x0000_0000_0000_0001u64, 0x0000_0000u32), // min f64 subnormal -> +0
        (0x8000_0000_0000_0001, 0x8000_0000),       // -min f64 subnormal -> -0
        (0x380f_fffe_0000_0000, 0x0080_0000),       // normal/subnormal boundary rounds up
        (0x380f_fffd_ffff_ffff, 0x007f_ffff),       // just below boundary stays subnormal
        (0x36a0_0000_0000_0000, 0x0000_0001),       // exact +min f32 subnormal
        (0xb6a0_0000_0000_0000, 0x8000_0001),       // exact -min f32 subnormal
        (0x47ef_ffff_d000_0000, 0x7f7f_fffe),       // below max-finite midpoint
        (0x47ef_ffff_d000_0001, 0x7f7f_ffff),       // just above midpoint
        (0x47ef_ffff_f000_0000, 0x7f80_0000),       // overflow midpoint -> +inf
        (0xc7ef_ffff_f000_0000, 0xff80_0000),       // overflow midpoint -> -inf
        (0x3ff0_0000_1000_0000, 0x3f80_0000),       // halfway, ties to even 1.0
        (0x3ff0_0000_1000_0001, 0x3f80_0001),       // one f64 ulp above halfway
        (0x3ff0_0000_3000_0000, 0x3f80_0002),       // next halfway, ties to even
        (0x3690_0000_0000_0000, 0x0000_0000),       // half min-subnormal -> +0
        (0xb690_0000_0000_0000, 0x8000_0000),       // negative half -> -0
        (0x3690_0000_0000_0001, 0x0000_0001),       // just above half -> min subnormal
        (0xb690_0000_0000_0001, 0x8000_0001),       // negative counterpart
    ];

    for (source_bits, expected_bits) in cases {
        match invoke(&demote_module(source_bits)) {
            Value::F32(value) => assert_eq!(
                value.to_bits(),
                expected_bits,
                "WebAssembly/spec@{UPSTREAM_SPEC_COMMIT}: f32.demote_f64 source=0x{source_bits:016x}"
            ),
            other => panic!("expected f32 result, got {other:?}"),
        }
    }
}
