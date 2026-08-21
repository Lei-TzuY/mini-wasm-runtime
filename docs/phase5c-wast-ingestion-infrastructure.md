# Phase 5C — WAST Ingestion Infrastructure

This tranche introduces a real WAST parser/filter/runner path for conformance testing. It deliberately stops short of claiming that the pinned upstream WebAssembly spec suite is now ingested wholesale.

## Dependency boundary

The integration harness uses the exact dev-only dependency:

`wast = "=217.0.0"`

`wast` is the Bytecode Alliance WAT/WAST parser and encoder. Version 217.0.0 is pinned so the test AST/API cannot drift underneath CI and its MSRV remains below this repository's Rust 1.81 floor.

The dependency exists only under `crates/wasm-runtime` dev-dependencies. No reference runtime such as Wasmtime or Wasmer is added, and no parser/validator/runtime product code depends on `wast`.

## Execution pipeline

The harness parses a WAST script with `wast::Wast`, then handles the supported directive subset explicitly.

For a supported core module:

1. parse the WAST AST;
2. encode `QuoteWat::Wat(Wat::Module(...))` to binary with `QuoteWat::encode`;
3. parse that binary through this repository's `wasm_parser::parse_module`;
4. validate and instantiate it through `Instance::new`;
5. execute supported assertions through the runtime's public export APIs.

The WAST crate is therefore only the text/script front end. The implementation under test remains this repository's parser, validator, and interpreter.

## Initial supported assertion subset

The first runner supports:

- current unnamed core `module` directives;
- `assert_return` wrapping an unnamed current-module `invoke`;
- i32, i64, f32, and f64 invoke arguments;
- exact i32/i64 expected results;
- exact f32/f64 expected results compared by raw bits;
- canonical and arithmetic f32/f64 NaN result patterns;
- ordered multi-value result vectors through `Instance::invoke_export_values`;
- `assert_trap` for explicitly mapped runtime trap classes.

The initial trap-message map contains:

- `integer divide by zero`;
- `integer overflow`;
- `invalid conversion to integer`;
- `out of bounds memory access`.

Unknown trap text is not guessed or substring-matched into a runtime class.

## Explicit filtering contract

Unsupported WAST constructs are never silently dropped. The harness records a typed `FilterReason` for unsupported:

- module shapes;
- directives;
- execution forms such as `get`;
- named-module invocations;
- argument types;
- expected-result types;
- trap messages.

This distinction matters for future upstream ingestion. A rising filtered count must remain observable instead of being mistaken for passing conformance coverage.

## Contract fixture

`tests/fixtures/phase5c_ingestion_contract.wast` is a committed synthetic fixture that exercises both paths.

Executed assertions cover:

- i32 parameters and result;
- ordered `[i32, i64]` multi-value results;
- f64 `nearest` behavior;
- an integer divide-by-zero trap.

The same script deliberately contains unsupported `get` and `register` directives. The integration test requires exactly those operations to appear as explicit filter results.

## Compatibility validation

The matrix validates both current stable Rust and the project's Rust 1.81 MSRV. This is important because the WAST dependency is test infrastructure that must not silently raise the repository's compiler floor.

The infrastructure has also been validated on Ubuntu, Windows, and macOS, ensuring that text parsing, binary encoding, dependency resolution, and the repository's execution path are not tied to one host platform.

## Non-goals

This tranche does not claim:

- full or broad upstream spec-suite ingestion;
- automatic fetching from the network during CI;
- automatic selection of every assertion the runtime happens to support;
- support for named module registries, `get`, components, references, v128, or proposal-specific directives;
- differential execution against a reference engine;
- a production dependency on the WAST parser.

The next conformance slice should feed pinned upstream `.wast` source snapshots or a deterministic pinned-source manifest through this runner and make the supported/filtered accounting part of the regression contract.
