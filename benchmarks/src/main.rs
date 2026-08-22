#![forbid(unsafe_code)]

use std::{env, process, time::Instant};

use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value};

const DEFAULT_ITERATIONS: usize = 1_000;
const DEFAULT_WARMUP: usize = 64;

#[derive(Clone, Copy)]
enum Expected {
    I32(i32),
    I64(i64),
}

impl Expected {
    fn bits(self) -> u64 {
        match self {
            Self::I32(value) => u64::from(value as u32),
            Self::I64(value) => value as u64,
        }
    }

    fn matches(self, values: &[Value]) -> bool {
        match (self, values) {
            (Self::I32(expected), [Value::I32(actual)]) => expected == *actual,
            (Self::I64(expected), [Value::I64(actual)]) => expected == *actual,
            _ => false,
        }
    }
}

struct Workload {
    name: &'static str,
    wat: &'static str,
    expected: Expected,
}

fn workloads() -> [Workload; 4] {
    [
        Workload {
            name: "integer_mix_loop",
            wat: r#"(module
                (func (export "run") (result i32)
                    (local $i i32)
                    (local $x i32)
                    i32.const 305419896
                    local.set $x
                    block $done
                        loop $loop
                            local.get $x
                            i32.const 1664525
                            i32.mul
                            i32.const 1013904223
                            i32.add
                            local.set $x
                            local.get $i
                            i32.const 1
                            i32.add
                            local.tee $i
                            i32.const 256
                            i32.lt_u
                            br_if $loop
                        end
                    end
                    local.get $x))"#,
            expected: Expected::I32(0x99c2db78_u32 as i32),
        },
        Workload {
            name: "control_br_table",
            wat: r#"(module
                (func (export "run") (result i32)
                    (local $i i32)
                    (local $acc i32)
                    loop $loop
                        block $after
                            block $case2
                                block $case1
                                    block $case0
                                        local.get $i
                                        i32.const 3
                                        i32.and
                                        br_table $case0 $case1 $case2 $after
                                    end
                                    local.get $acc
                                    i32.const 1
                                    i32.add
                                    local.set $acc
                                    br $after
                                end
                                local.get $acc
                                i32.const 2
                                i32.add
                                local.set $acc
                                br $after
                            end
                            local.get $acc
                            i32.const 4
                            i32.add
                            local.set $acc
                        end
                        local.get $i
                        i32.const 1
                        i32.add
                        local.tee $i
                        i32.const 128
                        i32.lt_u
                        br_if $loop
                    end
                    local.get $acc))"#,
            expected: Expected::I32(224),
        },
        Workload {
            name: "memory_i64_roundtrip",
            wat: r#"(module
                (memory 1)
                (func (export "run") (result i64)
                    (local $i i32)
                    (local $acc i64)
                    loop $loop
                        i32.const 64
                        i64.const 1234605616436508552
                        i64.store
                        local.get $acc
                        i32.const 64
                        i64.load
                        i64.add
                        local.set $acc
                        local.get $i
                        i32.const 1
                        i32.add
                        local.tee $i
                        i32.const 64
                        i32.lt_u
                        br_if $loop
                    end
                    local.get $acc))"#,
            expected: Expected::I64(5_227_783_157_098_340_864),
        },
        Workload {
            name: "float_sign_bits",
            wat: r#"(module
                (func (export "run") (result i64)
                    (local $i i32)
                    (local $x f64)
                    f64.const 1.5
                    local.set $x
                    loop $loop
                        local.get $x
                        f64.neg
                        f64.abs
                        local.set $x
                        local.get $i
                        i32.const 1
                        i32.add
                        local.tee $i
                        i32.const 256
                        i32.lt_u
                        br_if $loop
                    end
                    local.get $x
                    i64.reinterpret_f64))"#,
            expected: Expected::I64(0x3ff8_0000_0000_0000),
        },
    ]
}

fn parse_count(flag: &str, value: Option<String>) -> usize {
    let value = value.unwrap_or_else(|| {
        eprintln!("missing value for {flag}");
        process::exit(2);
    });
    match value.parse::<usize>() {
        Ok(0) | Err(_) => {
            eprintln!("{flag} must be a positive integer");
            process::exit(2);
        }
        Ok(value) => value,
    }
}

fn options() -> (usize, usize) {
    let mut iterations = DEFAULT_ITERATIONS;
    let mut warmup = DEFAULT_WARMUP;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--iterations" => iterations = parse_count("--iterations", args.next()),
            "--warmup" => warmup = parse_count("--warmup", args.next()),
            "--help" | "-h" => {
                println!(
                    "usage: mini-wasm-benchmarks [--iterations N] [--warmup N]\n\
                     defaults: --iterations {DEFAULT_ITERATIONS} --warmup {DEFAULT_WARMUP}"
                );
                process::exit(0);
            }
            other => {
                eprintln!("unknown argument: {other}");
                process::exit(2);
            }
        }
    }
    (iterations, warmup)
}

fn invoke_checked(instance: &mut Instance, workload: &Workload) -> u64 {
    let values = instance
        .invoke_export_values("run", &[])
        .unwrap_or_else(|error| panic!("{} trapped unexpectedly: {error}", workload.name));
    assert!(
        workload.expected.matches(&values),
        "{} returned {values:?}, expected deterministic result bits 0x{:016x}",
        workload.name,
        workload.expected.bits()
    );
    workload.expected.bits()
}

fn fold_checksum(checksum: u64, bits: u64, iteration: usize) -> u64 {
    checksum.rotate_left(9)
        ^ bits.wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (iteration as u64).wrapping_mul(0xd6e8_feb8_6659_fd93)
}

fn main() {
    let (iterations, warmup) = options();

    for workload in workloads() {
        let bytes = wat::parse_str(workload.wat)
            .unwrap_or_else(|error| panic!("failed to compile {} WAT: {error}", workload.name));
        let module = parse_module(&bytes)
            .unwrap_or_else(|error| panic!("failed to parse {}: {error}", workload.name));
        let mut instance = Instance::new(module)
            .unwrap_or_else(|error| panic!("failed to instantiate {}: {error}", workload.name));

        for _ in 0..warmup {
            invoke_checked(&mut instance, &workload);
        }

        let start = Instant::now();
        let mut checksum = 0_u64;
        for iteration in 0..iterations {
            let bits = invoke_checked(&mut instance, &workload);
            checksum = fold_checksum(checksum, bits, iteration);
        }
        let elapsed = start.elapsed();
        let elapsed_ns = elapsed.as_nanos();
        let ns_per_iter = elapsed_ns as f64 / iterations as f64;

        println!(
            "benchmark={} iterations={} warmup={} elapsed_ns={} ns_per_iter={:.2} checksum={:016x}",
            workload.name, iterations, warmup, elapsed_ns, ns_per_iter, checksum
        );
    }
}
