# Phase 5B numeric type model

Phase 5B turns the validator and interpreter from an i32-specialized execution model into a typed MVP numeric core.

## Scope

Supported defined-function/local/global value types:

- `i32`
- `i64`
- `f32`
- `f64`

Function imports remain intentionally i32-only in this slice so the existing host ABI does not silently change while the internal runtime value model is generalized.

The execution slice covers:

- typed locals, parameters, zero-or-one results, globals, and block results;
- `i32.const`, `i64.const`, `f32.const`, `f64.const`;
- integer and floating comparisons;
- existing i32 add/sub/mul plus i64 add/sub/mul;
- f32/f64 add/sub/mul/div;
- `i32.wrap_i64`, `i64.extend_i32_s`, `i64.extend_i32_u`, `f32.demote_f64`, and `f64.promote_f32`;
- typed direct and indirect call stack effects;
- exact type convergence at structured-control boundaries.

Memory instructions remain the Phase-3 i32 load/store family. Trapping float-to-integer conversions, reinterpret instructions, broader numeric operators, and multi-value results remain later work.

## Validator model

The operand stack stores a type for every reachable value. Control frames retain their entry stack height plus an optional result type. Unreachable code uses an explicit polymorphic state: pops at the current frame height succeed without manufacturing a concrete type, while any concrete value that is present must still match the opcode's required type.

Every instruction is specified as a typed stack transform rather than an arity-only transform. Calls consume the exact declared parameter types in order and push the declared result type. Branches preserve the exact label result type. `if` conditions and memory addresses remain `i32`.

## Runtime model

`Value` becomes a four-variant numeric enum. Locals are zero-initialized by declared type. Runtime stack operations check the expected variant as defense in depth even though validation has already proven the static type.

Floating values use native IEEE-754 `f32` / `f64` storage. Binary constants are decoded from their raw little-endian bit patterns so NaN payloads are not normalized during parsing or instruction decode.

## Compatibility boundary

Phase 5B must preserve all Phase-1 through Phase-5A behavior, host capability checks, resource metering, memory traps, table traps, start-function behavior, and Rust 1.81 MSRV support.