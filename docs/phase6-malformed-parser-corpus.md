# Phase 6 malformed-binary parser corpus

This hardening slice treats malformed WebAssembly bytes as an explicit parser contract rather than incidental unit-test coverage.

## Contract

Corpus entries are raw module bytes passed through the public `parse_module` API. Each case must fail closed with the precise `ParseError` class expected for the earliest malformed construct.

The initial corpus covers:

- truncated module headers and section payloads
- u32 LEB128 overflow in section lengths
- invalid UTF-8 names
- missing function-body terminators
- missing constant-expression terminators
- truncated constant-expression immediates
- invalid constant-expression opcodes
- invalid function-type tags
- unsupported value types
- invalid global mutability bytes
- invalid table reference types
- unsupported section ids in the current parser subset
- invalid export kinds

PR #16 already covers complementary parser invariants such as duplicate/out-of-order sections, exact section-payload consumption, and unsupported limits flags. This corpus intentionally does not duplicate them.

## Constant-expression diagnostic boundary

The corpus exposed a diagnostic bug in `read_const_expr`: after a constant immediate had been decoded successfully, EOF while reading the required trailing `end` (`0x0b`) escaped as generic `UnexpectedEof`, even though the parser defines `ConstExprMissingEnd` for this condition.

The fix is deliberately narrow:

- a complete constant immediate followed by EOF is `ConstExprMissingEnd`;
- a complete constant immediate followed by a non-`0x0b` byte is also `ConstExprMissingEnd`;
- EOF while decoding the constant immediate itself remains `UnexpectedEof` (or the relevant LEB128 error);
- constant-expression opcode/type diagnostics are unchanged.

A paired regression fixture distinguishes missing terminator from truncated immediate so future refactors cannot collapse these cases into one generic EOF classification.

## Non-goals

This slice is not parser fuzzing, a full WebAssembly spec-test import, or a claim that every malformed module class is covered. Validator/runtime malformed-state corpora and generated/fuzzed inputs remain separate Phase 6 work.
