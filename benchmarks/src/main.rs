#![forbid(unsafe_code)]

use std::{
    collections::{HashMap, HashSet},
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process,
    time::Instant,
};

use wasm_parser::parse_module;
use wasm_runtime::{Instance, Value};

const DEFAULT_ITERATIONS: usize = 1_000;
const DEFAULT_WARMUP: usize = 64;
const DEFAULT_SAMPLES: usize = 1;
const MIN_CONTROLLED_SAMPLES: usize = 7;
const RELATIVE_REGRESSION_MARGIN: f64 = 0.10;
const NOISE_MULTIPLIER: f64 = 3.0;
const MAX_RELATIVE_MAD: f64 = 0.10;
const BASELINE_FORMAT: &str = "mini-wasm-benchmark-baseline-v1";
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

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

    fn tag(self) -> u8 {
        match self {
            Self::I32(_) => 0x32,
            Self::I64(_) => 0x64,
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

#[derive(Debug)]
enum BaselineAction {
    Write(PathBuf),
    Check(PathBuf),
}

#[derive(Debug)]
struct Options {
    iterations: usize,
    warmup: usize,
    samples: usize,
    host_id: Option<String>,
    baseline_action: Option<BaselineAction>,
}

#[derive(Debug, Clone)]
struct Measurement {
    name: &'static str,
    fingerprint: u64,
    median_ns_per_iter: f64,
    mad_ns_per_iter: f64,
    min_ns_per_iter: f64,
    max_ns_per_iter: f64,
    checksum: u64,
}

#[derive(Debug, Clone)]
struct BaselineEntry {
    fingerprint: u64,
    median_ns_per_iter: f64,
    mad_ns_per_iter: f64,
}

#[derive(Debug)]
struct Baseline {
    host_id: String,
    iterations: usize,
    warmup: usize,
    samples: usize,
    entries: HashMap<String, BaselineEntry>,
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

fn parse_path(flag: &str, value: Option<String>) -> PathBuf {
    let value = value.unwrap_or_else(|| {
        eprintln!("missing value for {flag}");
        process::exit(2);
    });
    if value.is_empty() {
        eprintln!("{flag} path must not be empty");
        process::exit(2);
    }
    PathBuf::from(value)
}

fn set_baseline_action(current: &mut Option<BaselineAction>, action: BaselineAction) {
    if current.is_some() {
        eprintln!("--write-baseline and --check-baseline are mutually exclusive");
        process::exit(2);
    }
    *current = Some(action);
}

fn options() -> Options {
    let mut options = Options {
        iterations: DEFAULT_ITERATIONS,
        warmup: DEFAULT_WARMUP,
        samples: DEFAULT_SAMPLES,
        host_id: None,
        baseline_action: None,
    };
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--iterations" => options.iterations = parse_count("--iterations", args.next()),
            "--warmup" => options.warmup = parse_count("--warmup", args.next()),
            "--samples" => options.samples = parse_count("--samples", args.next()),
            "--host-id" => {
                let host_id = args.next().unwrap_or_else(|| {
                    eprintln!("missing value for --host-id");
                    process::exit(2);
                });
                if host_id.trim().is_empty()
                    || host_id.contains('\t')
                    || host_id.contains('\n')
                    || host_id.contains('\r')
                {
                    eprintln!("--host-id must be non-empty and contain no tabs or newlines");
                    process::exit(2);
                }
                options.host_id = Some(host_id);
            }
            "--write-baseline" => {
                let path = parse_path("--write-baseline", args.next());
                set_baseline_action(&mut options.baseline_action, BaselineAction::Write(path));
            }
            "--check-baseline" => {
                let path = parse_path("--check-baseline", args.next());
                set_baseline_action(&mut options.baseline_action, BaselineAction::Check(path));
            }
            "--help" | "-h" => {
                println!(
                    "usage: mini-wasm-benchmarks [--iterations N] [--warmup N] [--samples N] \\
                     [--host-id ID (--write-baseline PATH | --check-baseline PATH)]\n\
                     defaults: --iterations {DEFAULT_ITERATIONS} --warmup {DEFAULT_WARMUP} \\
                     --samples {DEFAULT_SAMPLES}\n\
                     controlled baseline modes require at least {MIN_CONTROLLED_SAMPLES} samples"
                );
                process::exit(0);
            }
            other => {
                eprintln!("unknown argument: {other}");
                process::exit(2);
            }
        }
    }

    if options.baseline_action.is_some() {
        if options.host_id.is_none() {
            eprintln!("controlled baseline modes require --host-id");
            process::exit(2);
        }
        if options.samples < MIN_CONTROLLED_SAMPLES {
            eprintln!(
                "controlled baseline modes require at least {MIN_CONTROLLED_SAMPLES} samples"
            );
            process::exit(2);
        }
    } else if options.host_id.is_some() {
        eprintln!("--host-id is only valid with --write-baseline or --check-baseline");
        process::exit(2);
    }

    options
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

fn fnv1a(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

fn workload_fingerprint(workload: &Workload) -> u64 {
    let hash = fnv1a(FNV_OFFSET, workload.name.as_bytes());
    let hash = fnv1a(hash, &[0]);
    let hash = fnv1a(hash, workload.wat.as_bytes());
    let hash = fnv1a(hash, &[workload.expected.tag()]);
    fnv1a(hash, &workload.expected.bits().to_le_bytes())
}

fn median(values: &[f64]) -> f64 {
    assert!(!values.is_empty());
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let middle = sorted.len() / 2;
    if (sorted.len() & 1) == 0 {
        (sorted[middle - 1] + sorted[middle]) / 2.0
    } else {
        sorted[middle]
    }
}

fn median_absolute_deviation(values: &[f64], center: f64) -> f64 {
    let deviations: Vec<f64> = values.iter().map(|value| (value - center).abs()).collect();
    median(&deviations)
}

fn measure_workload(
    workload: &Workload,
    iterations: usize,
    warmup: usize,
    samples: usize,
) -> Measurement {
    let bytes = wat::parse_str(workload.wat)
        .unwrap_or_else(|error| panic!("failed to compile {} WAT: {error}", workload.name));
    let module = parse_module(&bytes)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", workload.name));
    let mut instance = Instance::new(module)
        .unwrap_or_else(|error| panic!("failed to instantiate {}: {error}", workload.name));

    for _ in 0..warmup {
        invoke_checked(&mut instance, workload);
    }

    let mut sample_ns = Vec::with_capacity(samples);
    let mut expected_checksum = None;
    for _ in 0..samples {
        let start = Instant::now();
        let mut checksum = 0_u64;
        for iteration in 0..iterations {
            let bits = invoke_checked(&mut instance, workload);
            checksum = fold_checksum(checksum, bits, iteration);
        }
        let ns_per_iter = start.elapsed().as_nanos() as f64 / iterations as f64;
        assert!(
            ns_per_iter.is_finite() && ns_per_iter > 0.0,
            "{} produced an invalid timing sample {ns_per_iter}",
            workload.name
        );
        if let Some(expected) = expected_checksum {
            assert_eq!(
                checksum, expected,
                "{} checksum changed between timing samples",
                workload.name
            );
        } else {
            expected_checksum = Some(checksum);
        }
        sample_ns.push(ns_per_iter);
    }

    let median_ns_per_iter = median(&sample_ns);
    let mad_ns_per_iter = median_absolute_deviation(&sample_ns, median_ns_per_iter);
    let min_ns_per_iter = sample_ns.iter().copied().fold(f64::INFINITY, f64::min);
    let max_ns_per_iter = sample_ns.iter().copied().fold(f64::NEG_INFINITY, f64::max);

    Measurement {
        name: workload.name,
        fingerprint: workload_fingerprint(workload),
        median_ns_per_iter,
        mad_ns_per_iter,
        min_ns_per_iter,
        max_ns_per_iter,
        checksum: expected_checksum.expect("at least one sample is required"),
    }
}

fn relative_mad(median_ns_per_iter: f64, mad_ns_per_iter: f64) -> f64 {
    mad_ns_per_iter / median_ns_per_iter
}

fn ensure_stable_measurements(measurements: &[Measurement], label: &str) -> Result<(), String> {
    let unstable: Vec<String> = measurements
        .iter()
        .filter_map(|measurement| {
            let relative =
                relative_mad(measurement.median_ns_per_iter, measurement.mad_ns_per_iter);
            (relative > MAX_RELATIVE_MAD).then(|| {
                format!(
                    "{} relative MAD {:.2}% exceeds {:.2}%",
                    measurement.name,
                    relative * 100.0,
                    MAX_RELATIVE_MAD * 100.0
                )
            })
        })
        .collect();
    if unstable.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{label} measurement is too noisy for a trusted comparison: {}",
            unstable.join("; ")
        ))
    }
}

fn render_baseline(host_id: &str, options: &Options, measurements: &[Measurement]) -> String {
    let mut output = format!(
        "# {BASELINE_FORMAT}\nhost_id\t{host_id}\niterations\t{}\nwarmup\t{}\nsamples\t{}\nrelative_margin\t{RELATIVE_REGRESSION_MARGIN:.6}\nnoise_multiplier\t{NOISE_MULTIPLIER:.6}\nmax_relative_mad\t{MAX_RELATIVE_MAD:.6}\n",
        options.iterations, options.warmup, options.samples
    );
    for measurement in measurements {
        output.push_str(&format!(
            "benchmark\t{}\t{:016x}\t{:.6}\t{:.6}\n",
            measurement.name,
            measurement.fingerprint,
            measurement.median_ns_per_iter,
            measurement.mad_ns_per_iter
        ));
    }
    output
}

fn parse_positive_usize(value: &str, field: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("baseline {field} must be a positive integer"))
}

fn parse_finite_f64(value: &str, field: &str, allow_zero: bool) -> Result<f64, String> {
    let value = value
        .parse::<f64>()
        .map_err(|error| format!("baseline {field} is invalid: {error}"))?;
    let valid_sign = if allow_zero {
        value >= 0.0
    } else {
        value > 0.0
    };
    if !value.is_finite() || !valid_sign {
        let requirement = if allow_zero {
            "non-negative"
        } else {
            "positive"
        };
        return Err(format!("baseline {field} must be finite and {requirement}"));
    }
    Ok(value)
}

fn parse_fingerprint(value: &str, line_number: usize) -> Result<u64, String> {
    if value.len() != 16 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "baseline line {line_number} fingerprint must be exactly 16 hexadecimal digits"
        ));
    }
    u64::from_str_radix(value, 16)
        .map_err(|error| format!("baseline line {line_number} fingerprint is invalid: {error}"))
}

fn parse_baseline(text: &str) -> Result<Baseline, String> {
    let expected_header = format!("# {BASELINE_FORMAT}");
    if text.lines().next() != Some(expected_header.as_str()) {
        return Err(format!("baseline must begin with {expected_header:?}"));
    }

    let mut metadata = HashMap::<String, String>::new();
    let mut entries = HashMap::<String, BaselineEntry>::new();
    for (line_index, raw_line) in text.lines().enumerate().skip(1) {
        let line_number = line_index + 1;
        if raw_line.is_empty() || raw_line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = raw_line.split('\t').collect();
        if fields.first() == Some(&"benchmark") {
            if fields.len() != 5 {
                return Err(format!(
                    "baseline line {line_number} benchmark row must have exactly five tab-separated fields"
                ));
            }
            let name = fields[1];
            if name.is_empty() {
                return Err(format!(
                    "baseline line {line_number} has an empty benchmark name"
                ));
            }
            let fingerprint = parse_fingerprint(fields[2], line_number)?;
            let median_ns_per_iter = parse_finite_f64(fields[3], "benchmark median", false)?;
            let mad_ns_per_iter = parse_finite_f64(fields[4], "benchmark MAD", true)?;
            if entries
                .insert(
                    name.to_owned(),
                    BaselineEntry {
                        fingerprint,
                        median_ns_per_iter,
                        mad_ns_per_iter,
                    },
                )
                .is_some()
            {
                return Err(format!("baseline contains duplicate benchmark {name:?}"));
            }
        } else {
            if fields.len() != 2 {
                return Err(format!(
                    "baseline line {line_number} metadata row must have exactly two tab-separated fields"
                ));
            }
            if metadata
                .insert(fields[0].to_owned(), fields[1].to_owned())
                .is_some()
            {
                return Err(format!(
                    "baseline contains duplicate metadata field {:?}",
                    fields[0]
                ));
            }
        }
    }

    let take = |metadata: &mut HashMap<String, String>, field: &str| {
        metadata
            .remove(field)
            .ok_or_else(|| format!("baseline is missing required field {field:?}"))
    };
    let host_id = take(&mut metadata, "host_id")?;
    if host_id.trim().is_empty() {
        return Err("baseline host_id must not be empty".to_owned());
    }
    let iterations = parse_positive_usize(&take(&mut metadata, "iterations")?, "iterations")?;
    let warmup = parse_positive_usize(&take(&mut metadata, "warmup")?, "warmup")?;
    let samples = parse_positive_usize(&take(&mut metadata, "samples")?, "samples")?;
    let relative_margin = parse_finite_f64(
        &take(&mut metadata, "relative_margin")?,
        "relative_margin",
        true,
    )?;
    let noise_multiplier = parse_finite_f64(
        &take(&mut metadata, "noise_multiplier")?,
        "noise_multiplier",
        true,
    )?;
    let max_relative_mad = parse_finite_f64(
        &take(&mut metadata, "max_relative_mad")?,
        "max_relative_mad",
        true,
    )?;
    if !metadata.is_empty() {
        let mut unknown: Vec<String> = metadata.into_keys().collect();
        unknown.sort();
        return Err(format!(
            "baseline contains unsupported metadata fields: {}",
            unknown.join(", ")
        ));
    }
    if (relative_margin - RELATIVE_REGRESSION_MARGIN).abs() > f64::EPSILON
        || (noise_multiplier - NOISE_MULTIPLIER).abs() > f64::EPSILON
        || (max_relative_mad - MAX_RELATIVE_MAD).abs() > f64::EPSILON
    {
        return Err("baseline policy constants do not match this benchmark binary".to_owned());
    }
    if samples < MIN_CONTROLLED_SAMPLES {
        return Err(format!(
            "baseline samples must be at least {MIN_CONTROLLED_SAMPLES}"
        ));
    }
    if entries.is_empty() {
        return Err("baseline contains no benchmark entries".to_owned());
    }

    Ok(Baseline {
        host_id,
        iterations,
        warmup,
        samples,
        entries,
    })
}

fn validate_baseline_context(
    baseline: &Baseline,
    options: &Options,
    host_id: &str,
) -> Result<(), String> {
    if baseline.host_id != host_id {
        return Err(format!(
            "baseline host_id {:?} does not match requested host {:?}",
            baseline.host_id, host_id
        ));
    }
    if baseline.iterations != options.iterations
        || baseline.warmup != options.warmup
        || baseline.samples != options.samples
    {
        return Err(format!(
            "baseline measurement settings are iterations={}, warmup={}, samples={}, but candidate uses iterations={}, warmup={}, samples={}",
            baseline.iterations,
            baseline.warmup,
            baseline.samples,
            options.iterations,
            options.warmup,
            options.samples
        ));
    }
    Ok(())
}

fn baseline_is_stable(entry: &BaselineEntry) -> bool {
    relative_mad(entry.median_ns_per_iter, entry.mad_ns_per_iter) <= MAX_RELATIVE_MAD
}

fn regression_limit(baseline: &BaselineEntry, candidate: &Measurement) -> f64 {
    baseline.median_ns_per_iter * (1.0 + RELATIVE_REGRESSION_MARGIN)
        + NOISE_MULTIPLIER * baseline.mad_ns_per_iter.max(candidate.mad_ns_per_iter)
}

fn check_against_baseline(baseline: &Baseline, measurements: &[Measurement]) -> Result<(), String> {
    ensure_stable_measurements(measurements, "candidate")?;

    let current_names: HashSet<&str> = measurements.iter().map(|item| item.name).collect();
    let baseline_names: HashSet<&str> = baseline.entries.keys().map(String::as_str).collect();
    if current_names != baseline_names {
        let mut missing: Vec<&str> = current_names.difference(&baseline_names).copied().collect();
        let mut stale: Vec<&str> = baseline_names.difference(&current_names).copied().collect();
        missing.sort_unstable();
        stale.sort_unstable();
        return Err(format!(
            "baseline workload set mismatch; missing=[{}], stale=[{}]",
            missing.join(","),
            stale.join(",")
        ));
    }

    let mut failures = Vec::new();
    for candidate in measurements {
        let baseline_entry = baseline
            .entries
            .get(candidate.name)
            .expect("workload sets were checked above");
        if baseline_entry.fingerprint != candidate.fingerprint {
            failures.push(format!(
                "{} workload fingerprint changed (baseline={:016x}, candidate={:016x})",
                candidate.name, baseline_entry.fingerprint, candidate.fingerprint
            ));
            continue;
        }
        if !baseline_is_stable(baseline_entry) {
            failures.push(format!(
                "{} baseline relative MAD {:.2}% exceeds {:.2}%",
                candidate.name,
                relative_mad(
                    baseline_entry.median_ns_per_iter,
                    baseline_entry.mad_ns_per_iter
                ) * 100.0,
                MAX_RELATIVE_MAD * 100.0
            ));
            continue;
        }
        let limit = regression_limit(baseline_entry, candidate);
        let delta_percent =
            (candidate.median_ns_per_iter / baseline_entry.median_ns_per_iter - 1.0) * 100.0;
        let status = if candidate.median_ns_per_iter > limit {
            failures.push(format!(
                "{} median {:.2} ns/iter exceeds limit {:.2} ns/iter",
                candidate.name, candidate.median_ns_per_iter, limit
            ));
            "regression"
        } else {
            "pass"
        };
        println!(
            "comparison={} fingerprint={:016x} baseline_median_ns_per_iter={:.2} baseline_mad_ns_per_iter={:.2} candidate_median_ns_per_iter={:.2} candidate_mad_ns_per_iter={:.2} delta_percent={:.2} limit_ns_per_iter={:.2} status={status}",
            candidate.name,
            candidate.fingerprint,
            baseline_entry.median_ns_per_iter,
            baseline_entry.mad_ns_per_iter,
            candidate.median_ns_per_iter,
            candidate.mad_ns_per_iter,
            delta_percent,
            limit
        );
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "controlled-host performance check failed: {}",
            failures.join("; ")
        ))
    }
}

fn write_baseline(
    path: &Path,
    host_id: &str,
    options: &Options,
    measurements: &[Measurement],
) -> Result<(), String> {
    ensure_stable_measurements(measurements, "baseline")?;
    let contents = render_baseline(host_id, options, measurements);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            format!(
                "refusing to overwrite baseline {path:?}; choose a new path or remove the old file first: {error}"
            )
        })?;
    file.write_all(contents.as_bytes())
        .map_err(|error| format!("failed to write baseline {path:?}: {error}"))?;
    println!("baseline_written={} host_id={host_id}", path.display());
    Ok(())
}

fn load_baseline(path: &Path) -> Result<Baseline, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read baseline {path:?}: {error}"))?;
    parse_baseline(&text)
}

fn main() {
    let options = options();
    let host_id = options.host_id.as_deref();

    let baseline = match &options.baseline_action {
        Some(BaselineAction::Check(path)) => {
            let baseline = load_baseline(path).unwrap_or_else(|error| {
                eprintln!("{error}");
                process::exit(1);
            });
            validate_baseline_context(
                &baseline,
                &options,
                host_id.expect("baseline mode requires host id"),
            )
            .unwrap_or_else(|error| {
                eprintln!("{error}");
                process::exit(1);
            });
            Some(baseline)
        }
        _ => None,
    };

    let measurements: Vec<Measurement> = workloads()
        .iter()
        .map(|workload| {
            measure_workload(
                workload,
                options.iterations,
                options.warmup,
                options.samples,
            )
        })
        .collect();

    for measurement in &measurements {
        println!(
            "benchmark={} fingerprint={:016x} iterations={} warmup={} samples={} median_ns_per_iter={:.2} mad_ns_per_iter={:.2} min_ns_per_iter={:.2} max_ns_per_iter={:.2} checksum={:016x}",
            measurement.name,
            measurement.fingerprint,
            options.iterations,
            options.warmup,
            options.samples,
            measurement.median_ns_per_iter,
            measurement.mad_ns_per_iter,
            measurement.min_ns_per_iter,
            measurement.max_ns_per_iter,
            measurement.checksum
        );
    }

    let result = match (&options.baseline_action, baseline.as_ref()) {
        (Some(BaselineAction::Write(path)), None) => write_baseline(
            path,
            host_id.expect("baseline mode requires host id"),
            &options,
            &measurements,
        ),
        (Some(BaselineAction::Check(_)), Some(baseline)) => {
            check_against_baseline(baseline, &measurements)
        }
        (None, None) => Ok(()),
        _ => unreachable!("baseline action and loaded state must agree"),
    };

    if let Err(error) = result {
        eprintln!("{error}");
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measurement(name: &'static str, fingerprint: u64, median: f64, mad: f64) -> Measurement {
        Measurement {
            name,
            fingerprint,
            median_ns_per_iter: median,
            mad_ns_per_iter: mad,
            min_ns_per_iter: median - mad,
            max_ns_per_iter: median + mad,
            checksum: 0,
        }
    }

    #[test]
    fn median_and_mad_resist_one_large_outlier() {
        let values = [10.0, 11.0, 12.0, 13.0, 100.0];
        let center = median(&values);
        assert_eq!(center, 12.0);
        assert_eq!(median_absolute_deviation(&values, center), 1.0);
    }

    #[test]
    fn workload_fingerprint_changes_with_definition() {
        let mut items = workloads();
        let original = workload_fingerprint(&items[0]);
        items[0].expected = Expected::I32(0);
        assert_ne!(original, workload_fingerprint(&items[0]));
    }

    #[test]
    fn baseline_round_trip_preserves_policy_and_measurements() {
        let options = Options {
            iterations: 2_000,
            warmup: 128,
            samples: 9,
            host_id: Some("lab-box-a".to_owned()),
            baseline_action: None,
        };
        let measurements = [
            measurement("integer_mix_loop", 0x1111, 1_000.0, 20.0),
            measurement("control_br_table", 0x2222, 2_000.0, 30.0),
        ];
        let rendered = render_baseline("lab-box-a", &options, &measurements);
        let parsed = parse_baseline(&rendered).expect("rendered baseline must parse");
        assert_eq!(parsed.host_id, "lab-box-a");
        assert_eq!(parsed.iterations, 2_000);
        assert_eq!(parsed.warmup, 128);
        assert_eq!(parsed.samples, 9);
        assert_eq!(parsed.entries.len(), 2);
        assert_eq!(parsed.entries["integer_mix_loop"].fingerprint, 0x1111);
        assert_eq!(
            parsed.entries["integer_mix_loop"].median_ns_per_iter,
            1_000.0
        );
        assert_eq!(parsed.entries["control_br_table"].mad_ns_per_iter, 30.0);
    }

    #[test]
    fn regression_limit_combines_relative_margin_and_observed_noise() {
        let baseline = BaselineEntry {
            fingerprint: 1,
            median_ns_per_iter: 1_000.0,
            mad_ns_per_iter: 10.0,
        };
        let passing = measurement("x", 1, 1_120.0, 10.0);
        let failing = measurement("x", 1, 1_140.0, 10.0);
        assert_eq!(regression_limit(&baseline, &passing), 1_130.0);
        assert!(passing.median_ns_per_iter <= regression_limit(&baseline, &passing));
        assert!(failing.median_ns_per_iter > regression_limit(&baseline, &failing));
    }

    #[test]
    fn baseline_parser_rejects_policy_drift() {
        let text = "# mini-wasm-benchmark-baseline-v1\nhost_id\tlab\niterations\t1000\nwarmup\t64\nsamples\t7\nrelative_margin\t0.500000\nnoise_multiplier\t3.000000\nmax_relative_mad\t0.100000\nbenchmark\tx\t0000000000000001\t1000.0\t10.0\n";
        let error = parse_baseline(text).expect_err("policy drift must fail closed");
        assert!(error.contains("policy constants"));
    }

    #[test]
    fn baseline_check_rejects_workload_fingerprint_drift() {
        let baseline = Baseline {
            host_id: "lab".to_owned(),
            iterations: 1_000,
            warmup: 64,
            samples: 7,
            entries: HashMap::from([(
                "x".to_owned(),
                BaselineEntry {
                    fingerprint: 1,
                    median_ns_per_iter: 1_000.0,
                    mad_ns_per_iter: 10.0,
                },
            )]),
        };
        let candidate = [measurement("x", 2, 1_000.0, 10.0)];
        let error =
            check_against_baseline(&baseline, &candidate).expect_err("fingerprint drift must fail");
        assert!(error.contains("fingerprint changed"));
    }
}
