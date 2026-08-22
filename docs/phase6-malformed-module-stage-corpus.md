# Phase 6 malformed-module stage corpus

The runtime test suite now includes a focused adversarial corpus that separates malformed modules rejected by validation from modules that are statically valid but fail only during execution.

The validation tranche starts from parser-accepted modules and then exercises exact rejection classes for function/code count mismatches, missing type targets, duplicate and out-of-range exports, invalid local/call indices, memory instructions without memory, excessive memory alignment, invalid branch depth, malformed `else` structure, immutable global mutation, invalid start signatures, operand-stack underflow, typed operand mismatches, and invalid memory limits.

The runtime tranche enforces stage ordering explicitly: each fixture must parse, validate, and instantiate successfully before its dynamic failure is accepted. Current runtime cases cover null and out-of-range `call_indirect` table elements, an out-of-bounds i32 store, and NaN-to-integer trapping conversion.

This tranche does not relax validation or add product dependencies. Its purpose is to make negative behavior fail closed at the correct layer and to prevent future refactors from accidentally moving malformed input deeper into execution.

Future expansion should add more import/object mismatch cases, element/data segment boundary failures, generated validator mutations, and minimized regression fixtures for any failures discovered by fuzzing or differential execution.
