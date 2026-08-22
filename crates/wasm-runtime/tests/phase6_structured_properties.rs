use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};

const I32: u8 = 0x7f;
const SEED: u64 = 0x243f_6a88_85a3_08d3;

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Add,
    Sub,
    Mul,
    And,
    Or,
    Xor,
}

impl Op {
    fn generate(rng: &mut XorShift64) -> Self {
        match rng.next_u64() % 6 {
            0 => Self::Add,
            1 => Self::Sub,
            2 => Self::Mul,
            3 => Self::And,
            4 => Self::Or,
            _ => Self::Xor,
        }
    }

    fn opcode(self) -> u8 {
        match self {
            Self::Add => 0x6a,
            Self::Sub => 0x6b,
            Self::Mul => 0x6c,
            Self::And => 0x71,
            Self::Or => 0x72,
            Self::Xor => 0x73,
        }
    }

    fn apply(self, lhs: i32, rhs: i32) -> i32 {
        match self {
            Self::Add => lhs.wrapping_add(rhs),
            Self::Sub => lhs.wrapping_sub(rhs),
            Self::Mul => lhs.wrapping_mul(rhs),
            Self::And => lhs & rhs,
            Self::Or => lhs | rhs,
            Self::Xor => lhs ^ rhs,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Expr {
    Leaf(u8),
    Binary(Op, Box<Expr>, Box<Expr>),
}

impl Expr {
    fn generate(rng: &mut XorShift64, depth: usize) -> Self {
        if depth == 0 || rng.next_u64() % 5 == 0 {
            return Self::Leaf((rng.next_u64() % 4) as u8);
        }
        Self::Binary(
            Op::generate(rng),
            Box::new(Self::generate(rng, depth - 1)),
            Box::new(Self::generate(rng, depth - 1)),
        )
    }

    fn evaluate(&self, args: &[i32; 4]) -> i32 {
        match self {
            Self::Leaf(index) => args[*index as usize],
            Self::Binary(op, lhs, rhs) => op.apply(lhs.evaluate(args), rhs.evaluate(args)),
        }
    }

    fn compile(&self, instructions: &mut Vec<u8>) {
        match self {
            Self::Leaf(index) => instructions.extend_from_slice(&[0x20, *index]),
            Self::Binary(op, lhs, rhs) => {
                lhs.compile(instructions);
                rhs.compile(instructions);
                instructions.push(op.opcode());
            }
        }
    }

    fn complexity(&self) -> (usize, u32) {
        match self {
            Self::Leaf(index) => (1, u32::from(*index)),
            Self::Binary(_, lhs, rhs) => {
                let (lhs_nodes, lhs_weight) = lhs.complexity();
                let (rhs_nodes, rhs_weight) = rhs.complexity();
                (1 + lhs_nodes + rhs_nodes, lhs_weight + rhs_weight)
            }
        }
    }

    fn contains_op(&self, wanted: Op) -> bool {
        match self {
            Self::Leaf(_) => false,
            Self::Binary(op, lhs, rhs) => {
                *op == wanted || lhs.contains_op(wanted) || rhs.contains_op(wanted)
            }
        }
    }
}

fn shrink_candidates(expr: &Expr) -> Vec<Expr> {
    match expr {
        Expr::Leaf(index) => {
            if *index == 0 {
                Vec::new()
            } else {
                vec![Expr::Leaf(0)]
            }
        }
        Expr::Binary(op, lhs, rhs) => {
            let mut candidates = vec![(**lhs).clone(), (**rhs).clone()];
            for smaller_lhs in shrink_candidates(lhs) {
                candidates.push(Expr::Binary(
                    *op,
                    Box::new(smaller_lhs),
                    Box::new((**rhs).clone()),
                ));
            }
            for smaller_rhs in shrink_candidates(rhs) {
                candidates.push(Expr::Binary(
                    *op,
                    Box::new((**lhs).clone()),
                    Box::new(smaller_rhs),
                ));
            }
            candidates
        }
    }
}

fn minimize_failure(mut current: Expr, mut fails: impl FnMut(&Expr) -> bool) -> Expr {
    loop {
        let current_complexity = current.complexity();
        let next = shrink_candidates(&current)
            .into_iter()
            .find(|candidate| candidate.complexity() < current_complexity && fails(candidate));
        match next {
            Some(next) => current = next,
            None => return current,
        }
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

fn function_module(params: &[u8], instructions: &[u8], memory_pages: Option<u32>) -> Vec<u8> {
    let mut module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    let mut ty = vec![0x01, 0x60];
    push_u32(&mut ty, params.len() as u32);
    ty.extend_from_slice(params);
    ty.extend_from_slice(&[0x01, I32]);
    push_section(&mut module, 1, &ty);
    push_section(&mut module, 3, &[0x01, 0x00]);

    if let Some(pages) = memory_pages {
        let mut memory = vec![0x01, 0x00];
        push_u32(&mut memory, pages);
        push_section(&mut module, 5, &memory);
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

fn execute_i32(bytes: &[u8], args: &[Value]) -> Result<i32, String> {
    let module = parse_module(bytes).map_err(|error| format!("parse: {error:?}"))?;
    let mut instance = Instance::new(module).map_err(|error| format!("instantiate: {error:?}"))?;
    match instance.invoke_export("run", args) {
        Ok(Some(Value::I32(value))) => Ok(value),
        Ok(other) => Err(format!("unexpected result shape: {other:?}")),
        Err(error) => Err(format!("execute: {error:?}")),
    }
}

fn expr_module(expr: &Expr) -> Vec<u8> {
    let mut instructions = Vec::new();
    expr.compile(&mut instructions);
    function_module(&[I32, I32, I32, I32], &instructions, None)
}

fn execute_expr(expr: &Expr, args: &[i32; 4]) -> Result<i32, String> {
    let values = args.map(Value::I32);
    execute_i32(&expr_module(expr), &values)
}

fn assert_expr_case(expr: &Expr, args: &[i32; 4], case: usize) {
    let expected = expr.evaluate(args);
    match execute_expr(expr, args) {
        Ok(actual) if actual == expected => {}
        observed => {
            let minimized = minimize_failure(expr.clone(), |candidate| {
                let candidate_expected = candidate.evaluate(args);
                !matches!(
                    execute_expr(candidate, args),
                    Ok(actual) if actual == candidate_expected
                )
            });
            panic!(
                "structured expression mismatch at seed={SEED:#018x} case={case}: \
                 args={args:?}, expected={expected}, observed={observed:?}, \
                 original={expr:?}, minimized={minimized:?}"
            );
        }
    }
}

#[test]
fn deterministic_shrinker_reduces_a_structured_counterexample() {
    let original = Expr::Binary(
        Op::Add,
        Box::new(Expr::Binary(
            Op::Mul,
            Box::new(Expr::Leaf(3)),
            Box::new(Expr::Leaf(2)),
        )),
        Box::new(Expr::Binary(
            Op::Xor,
            Box::new(Expr::Leaf(1)),
            Box::new(Expr::Leaf(0)),
        )),
    );

    let minimized = minimize_failure(original, |expr| expr.contains_op(Op::Mul));
    assert_eq!(
        minimized,
        Expr::Binary(Op::Mul, Box::new(Expr::Leaf(0)), Box::new(Expr::Leaf(0)))
    );
}

#[test]
fn generated_expression_trees_match_wrapping_reference_semantics() {
    let mut rng = XorShift64::new(SEED);
    for case in 0..128 {
        let expr = Expr::generate(&mut rng, 4);
        let args = [
            rng.next_i32(),
            rng.next_i32(),
            rng.next_i32(),
            rng.next_i32(),
        ];
        assert_expr_case(&expr, &args, case);
    }
}

#[test]
fn generated_if_trees_match_selected_reference_branch() {
    let mut rng = XorShift64::new(SEED ^ 0x1319_8a2e_0370_7344);

    for case in 0..96 {
        let then_expr = Expr::generate(&mut rng, 3);
        let else_expr = Expr::generate(&mut rng, 3);
        let args = [
            rng.next_i32(),
            rng.next_i32(),
            rng.next_i32(),
            rng.next_i32(),
        ];
        let condition = rng.next_i32();

        let mut instructions = vec![0x20, 0x04, 0x04, I32];
        then_expr.compile(&mut instructions);
        instructions.push(0x05);
        else_expr.compile(&mut instructions);
        instructions.push(0x0b);

        let bytes = function_module(&[I32, I32, I32, I32, I32], &instructions, None);
        let values = [
            Value::I32(args[0]),
            Value::I32(args[1]),
            Value::I32(args[2]),
            Value::I32(args[3]),
            Value::I32(condition),
        ];
        let expected = if condition != 0 {
            then_expr.evaluate(&args)
        } else {
            else_expr.evaluate(&args)
        };
        let observed = execute_i32(&bytes, &values);

        assert_eq!(
            observed,
            Ok(expected),
            "generated if mismatch at seed={SEED:#018x} case={case}: condition={condition}, \
             args={args:?}, then={then_expr:?}, else={else_expr:?}"
        );
    }
}

fn memory_round_trip_module(offset: u32) -> Vec<u8> {
    let mut instructions = vec![0x20, 0x00, 0x20, 0x01, 0x36, 0x02];
    push_u32(&mut instructions, offset);
    instructions.extend_from_slice(&[0x20, 0x00, 0x28, 0x02]);
    push_u32(&mut instructions, offset);
    function_module(&[I32, I32], &instructions, Some(1))
}

#[test]
fn generated_memory_boundaries_distinguish_round_trips_from_oob_traps() {
    const PAGE_BYTES: u64 = 65_536;
    let offsets = [0_u32, 1, 7, 64, 1_024];
    let fixed_addresses = [0_u32, 1, 65_531, 65_532, 65_533, 65_535, u32::MAX];
    let mut address_rng = XorShift64::new(SEED ^ 0xa409_3822_299f_31d0);
    let mut value_rng = XorShift64::new(SEED ^ 0x082e_fa98_ec4e_6c89);

    for offset in offsets {
        let bytes = memory_round_trip_module(offset);

        for (case, address) in fixed_addresses
            .into_iter()
            .chain((0..48).map(|index| {
                if index % 2 == 0 {
                    (address_rng.next_u64() % 65_533) as u32
                } else {
                    65_520 + (address_rng.next_u64() % 32) as u32
                }
            }))
            .enumerate()
        {
            let value = value_rng.next_i32();
            let module = parse_module(&bytes).expect("generated memory fixture must parse");
            let mut instance =
                Instance::new(module).expect("generated memory fixture must instantiate");
            let observed =
                instance.invoke_export("run", &[Value::I32(address as i32), Value::I32(value)]);
            let effective = u64::from(address) + u64::from(offset);
            let in_bounds = effective <= PAGE_BYTES - 4;

            if in_bounds {
                assert_eq!(
                    observed.unwrap(),
                    Some(Value::I32(value)),
                    "memory round-trip mismatch at offset={offset} case={case} address={address}"
                );
            } else {
                assert!(
                    matches!(
                        observed,
                        Err(RuntimeError::MemoryOutOfBounds { width: 4, .. })
                    ),
                    "expected memory OOB at offset={offset} case={case} address={address}, \
                     observed={observed:?}"
                );
            }
        }
    }
}
