use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value};

const I32: u8 = 0x7f;
const I64: u8 = 0x7e;
const F32: u8 = 0x7d;
const F64: u8 = 0x7c;
const SEED: u64 = 0x9e37_79b9_7f4a_7c15;

struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        assert_ne!(seed, 0, "xorshift seed must be non-zero");
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        let mut value = self.state;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.state = value;
        value
    }

    fn next_i32(&mut self) -> i32 {
        self.next_u64() as u32 as i32
    }

    fn next_i64(&mut self) -> i64 {
        self.next_u64() as i64
    }
}

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

fn function_module(
    params: &[u8],
    result: u8,
    instructions: &[u8],
    memory: Option<(u32, Option<u32>)>,
) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    let mut ty = vec![0x01, 0x60];
    push_u32(&mut ty, params.len() as u32);
    ty.extend_from_slice(params);
    ty.extend_from_slice(&[0x01, result]);
    push_section(&mut module, 1, &ty);
    push_section(&mut module, 3, &[0x01, 0x00]);

    if let Some((minimum, maximum)) = memory {
        let mut payload = vec![0x01];
        match maximum {
            Some(maximum) => {
                payload.push(0x01);
                push_u32(&mut payload, minimum);
                push_u32(&mut payload, maximum);
            }
            None => {
                payload.push(0x00);
                push_u32(&mut payload, minimum);
            }
        }
        push_section(&mut module, 5, &payload);
    }

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

fn instantiate(bytes: Vec<u8>) -> Instance {
    Instance::new(parse_module(&bytes).expect("generated property fixture must parse"))
        .expect("generated property fixture must validate and instantiate")
}

fn invoke_i32(instance: &mut Instance, args: &[Value]) -> i32 {
    match instance
        .invoke_export("run", args)
        .expect("generated i32 property invocation must not trap")
    {
        Some(Value::I32(value)) => value,
        other => panic!("generated property returned non-i32 result: {other:?}"),
    }
}

fn invoke_i64(instance: &mut Instance, args: &[Value]) -> i64 {
    match instance
        .invoke_export("run", args)
        .expect("generated i64 property invocation must not trap")
    {
        Some(Value::I64(value)) => value,
        other => panic!("generated property returned non-i64 result: {other:?}"),
    }
}

#[test]
fn generated_i32_wrapping_arithmetic_matches_reference_semantics() {
    let mut add = instantiate(function_module(
        &[I32, I32],
        I32,
        &[0x20, 0x00, 0x20, 0x01, 0x6a],
        None,
    ));
    let mut sub = instantiate(function_module(
        &[I32, I32],
        I32,
        &[0x20, 0x00, 0x20, 0x01, 0x6b],
        None,
    ));
    let mut mul = instantiate(function_module(
        &[I32, I32],
        I32,
        &[0x20, 0x00, 0x20, 0x01, 0x6c],
        None,
    ));

    let edge_pairs = [
        (0, 0),
        (i32::MAX, 1),
        (i32::MIN, -1),
        (-1, -1),
        (0x4000_0000, 4),
        (i32::MIN, i32::MIN),
    ];

    for (case, (a, b)) in edge_pairs
        .into_iter()
        .chain((0..512).scan(XorShift64::new(SEED), |rng, _| {
            Some((rng.next_i32(), rng.next_i32()))
        }))
        .enumerate()
    {
        let args = [Value::I32(a), Value::I32(b)];
        assert_eq!(
            invoke_i32(&mut add, &args),
            a.wrapping_add(b),
            "add property failed at case {case}: a={a}, b={b}"
        );
        assert_eq!(
            invoke_i32(&mut sub, &args),
            a.wrapping_sub(b),
            "sub property failed at case {case}: a={a}, b={b}"
        );
        assert_eq!(
            invoke_i32(&mut mul, &args),
            a.wrapping_mul(b),
            "mul property failed at case {case}: a={a}, b={b}"
        );
    }
}

#[test]
fn generated_signed_division_and_remainder_obey_quotient_remainder_identity() {
    let mut div = instantiate(function_module(
        &[I32, I32],
        I32,
        &[0x20, 0x00, 0x20, 0x01, 0x6d],
        None,
    ));
    let mut rem = instantiate(function_module(
        &[I32, I32],
        I32,
        &[0x20, 0x00, 0x20, 0x01, 0x6f],
        None,
    ));
    let mut rng = XorShift64::new(SEED ^ 0xd1b5_4a32_d192_ed03);
    let mut checked = 0usize;

    for case in 0..768 {
        let a = rng.next_i32();
        let b = rng.next_i32();
        if b == 0 || (a == i32::MIN && b == -1) {
            continue;
        }

        let args = [Value::I32(a), Value::I32(b)];
        let quotient = invoke_i32(&mut div, &args);
        let remainder = invoke_i32(&mut rem, &args);

        assert_eq!(
            quotient,
            a / b,
            "div_s property failed at case {case}: a={a}, b={b}"
        );
        assert_eq!(
            remainder,
            a % b,
            "rem_s property failed at case {case}: a={a}, b={b}"
        );
        assert_eq!(
            quotient.wrapping_mul(b).wrapping_add(remainder),
            a,
            "q*b+r identity failed at case {case}: a={a}, b={b}, q={quotient}, r={remainder}"
        );
        assert!(
            remainder == 0 || remainder.signum() == a.signum(),
            "remainder sign property failed at case {case}: a={a}, b={b}, r={remainder}"
        );
        assert!(
            remainder.unsigned_abs() < b.unsigned_abs(),
            "remainder magnitude property failed at case {case}: a={a}, b={b}, r={remainder}"
        );
        checked += 1;
    }

    assert!(
        checked >= 760,
        "too many generated division cases were skipped"
    );
}

#[test]
fn generated_i64_shift_and_rotate_counts_follow_modulo_64_semantics() {
    let mut shl = instantiate(function_module(
        &[I64, I64],
        I64,
        &[0x20, 0x00, 0x20, 0x01, 0x86],
        None,
    ));
    let mut shr_s = instantiate(function_module(
        &[I64, I64],
        I64,
        &[0x20, 0x00, 0x20, 0x01, 0x87],
        None,
    ));
    let mut shr_u = instantiate(function_module(
        &[I64, I64],
        I64,
        &[0x20, 0x00, 0x20, 0x01, 0x88],
        None,
    ));
    let mut rotl = instantiate(function_module(
        &[I64, I64],
        I64,
        &[0x20, 0x00, 0x20, 0x01, 0x89],
        None,
    ));
    let mut rotr = instantiate(function_module(
        &[I64, I64],
        I64,
        &[0x20, 0x00, 0x20, 0x01, 0x8a],
        None,
    ));
    let mut rng = XorShift64::new(SEED ^ 0xa076_1d64_78bd_642f);

    for case in 0..512 {
        let value = rng.next_i64();
        let count = rng.next_i64();
        let host_count = count as u32;
        let args = [Value::I64(value), Value::I64(count)];

        assert_eq!(
            invoke_i64(&mut shl, &args),
            value.wrapping_shl(host_count),
            "i64.shl property failed at case {case}: value={value}, count={count}"
        );
        assert_eq!(
            invoke_i64(&mut shr_s, &args),
            value.wrapping_shr(host_count),
            "i64.shr_s property failed at case {case}: value={value}, count={count}"
        );
        assert_eq!(
            invoke_i64(&mut shr_u, &args),
            (value as u64).wrapping_shr(host_count) as i64,
            "i64.shr_u property failed at case {case}: value={value}, count={count}"
        );
        assert_eq!(
            invoke_i64(&mut rotl, &args),
            (value as u64).rotate_left(host_count) as i64,
            "i64.rotl property failed at case {case}: value={value}, count={count}"
        );
        assert_eq!(
            invoke_i64(&mut rotr, &args),
            (value as u64).rotate_right(host_count) as i64,
            "i64.rotr property failed at case {case}: value={value}, count={count}"
        );
    }
}

#[test]
fn generated_reinterpret_round_trips_preserve_every_source_bit() {
    let mut i32_round_trip = instantiate(function_module(
        &[I32],
        I32,
        &[0x20, 0x00, 0xbe, 0xbc],
        None,
    ));
    let mut i64_round_trip = instantiate(function_module(
        &[I64],
        I64,
        &[0x20, 0x00, 0xbf, 0xbd],
        None,
    ));
    let mut rng = XorShift64::new(SEED ^ 0xe703_7ed1_a0b4_28db);

    for case in 0..512 {
        let i32_bits = rng.next_i32();
        let i64_bits = rng.next_i64();

        assert_eq!(
            invoke_i32(&mut i32_round_trip, &[Value::I32(i32_bits)]),
            i32_bits,
            "i32 -> f32 -> i32 reinterpret property failed at case {case}: bits={:#010x}",
            i32_bits as u32
        );
        assert_eq!(
            invoke_i64(&mut i64_round_trip, &[Value::I64(i64_bits)]),
            i64_bits,
            "i64 -> f64 -> i64 reinterpret property failed at case {case}: bits={:#018x}",
            i64_bits as u64
        );
    }
}

#[test]
fn generated_numeric_memory_round_trips_preserve_values_and_float_bits() {
    let mut i64_memory = instantiate(function_module(
        &[I64],
        I64,
        &[
            0x41, 0x00, 0x20, 0x00, 0x37, 0x03, 0x00, 0x41, 0x00, 0x29, 0x03, 0x00,
        ],
        Some((1, Some(1))),
    ));
    let mut f32_memory = instantiate(function_module(
        &[F32],
        I32,
        &[
            0x41, 0x00, 0x20, 0x00, 0x38, 0x02, 0x00, 0x41, 0x00, 0x2a, 0x02, 0x00, 0xbc,
        ],
        Some((1, Some(1))),
    ));
    let mut f64_memory = instantiate(function_module(
        &[F64],
        I64,
        &[
            0x41, 0x00, 0x20, 0x00, 0x39, 0x03, 0x00, 0x41, 0x00, 0x2b, 0x03, 0x00, 0xbd,
        ],
        Some((1, Some(1))),
    ));
    let mut rng = XorShift64::new(SEED ^ 0x8ebc_6af0_9c88_c6e3);

    for case in 0..384 {
        let i64_value = rng.next_i64();
        let f32_bits = rng.next_u64() as u32;
        let f64_bits = rng.next_u64();

        assert_eq!(
            invoke_i64(&mut i64_memory, &[Value::I64(i64_value)]),
            i64_value,
            "i64 memory round-trip failed at case {case}: value={i64_value}"
        );
        assert_eq!(
            invoke_i32(&mut f32_memory, &[Value::F32(f32::from_bits(f32_bits))]) as u32,
            f32_bits,
            "f32 memory bit round-trip failed at case {case}: bits={f32_bits:#010x}"
        );
        assert_eq!(
            invoke_i64(&mut f64_memory, &[Value::F64(f64::from_bits(f64_bits))]) as u64,
            f64_bits,
            "f64 memory bit round-trip failed at case {case}: bits={f64_bits:#018x}"
        );
    }
}
