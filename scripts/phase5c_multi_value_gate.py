from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected exactly one anchor in {path}, found {count}: {old[:120]!r}")
    p.write_text(text.replace(old, new, 1))


# Validator: defined functions, direct/indirect calls, and structured control may
# carry arbitrary numeric result vectors. Imported host functions remain <=1 result.
replace_once(
    "crates/wasm-validator/src/lib.rs",
    "//! Function imports and defined code may use i32/i64/f32/f64 with at most one result.\n",
    "//! Defined code may use i32/i64/f32/f64 multi-value results; the host import ABI remains at most one result.\n",
)
replace_once(
    "crates/wasm-validator/src/lib.rs",
    """        if function_type.results.len() > 1 {\n            return Err(ValidationError::UnsupportedResultArity {\n                function,\n                results: function_type.results.len(),\n            });\n        }\n""",
    "",
)

replace_once(
    "crates/wasm-validator/src/typed.rs",
    """    end_type: Option<ValueType>,\n    label_types: Vec<ValueType>,\n""",
    """    end_types: Vec<ValueType>,\n    label_types: Vec<ValueType>,\n""",
)
replace_once(
    "crates/wasm-validator/src/typed.rs",
    """struct BlockSignature {\n    params: Vec<ValueType>,\n    result: Option<ValueType>,\n}\n""",
    """struct BlockSignature {\n    params: Vec<ValueType>,\n    results: Vec<ValueType>,\n}\n""",
)
replace_once(
    "crates/wasm-validator/src/typed.rs",
    """    let function_result = function_results.first().copied();\n    let mut stack = Vec::<ValueType>::new();\n""",
    """    let mut stack = Vec::<ValueType>::new();\n""",
)
replace_once(
    "crates/wasm-validator/src/typed.rs",
    """        end_type: function_result,\n        label_types: function_results.to_vec(),\n""",
    """        end_types: function_results.to_vec(),\n        label_types: function_results.to_vec(),\n""",
)
replace_once(
    "crates/wasm-validator/src/typed.rs",
    """                let label_types = if kind == ControlKind::Loop {\n                    signature.params.clone()\n                } else {\n                    signature.result.into_iter().collect()\n                };\n                controls.push(ControlFrame {\n                    kind,\n                    height,\n                    param_types: signature.params,\n                    end_type: signature.result,\n                    label_types,\n""",
    """                let label_types = if kind == ControlKind::Loop {\n                    signature.params.clone()\n                } else {\n                    signature.results.clone()\n                };\n                controls.push(ControlFrame {\n                    kind,\n                    height,\n                    param_types: signature.params,\n                    end_types: signature.results,\n                    label_types,\n""",
)
replace_once(
    "crates/wasm-validator/src/typed.rs",
    """                let label_types = signature.result.into_iter().collect();\n                controls.push(ControlFrame {\n                    kind: ControlKind::If,\n                    height,\n                    param_types: signature.params,\n                    end_type: signature.result,\n                    label_types,\n""",
    """                let label_types = signature.results.clone();\n                controls.push(ControlFrame {\n                    kind: ControlKind::If,\n                    height,\n                    param_types: signature.params,\n                    end_types: signature.results,\n                    label_types,\n""",
)
replace_once(
    "crates/wasm-validator/src/typed.rs",
    """                if frame.kind == ControlKind::If && frame.end_type.is_some() && !frame.seen_else {\n""",
    """                if frame.kind == ControlKind::If && !frame.end_types.is_empty() && !frame.seen_else {\n""",
)
replace_once(
    "crates/wasm-validator/src/typed.rs",
    """                    stack.truncate(frame.height);\n                    if let Some(ty) = frame.end_type {\n                        stack.push(ty);\n                    }\n""",
    """                    stack.truncate(frame.height);\n                    stack.extend(frame.end_types.iter().copied());\n""",
)
replace_once(
    "crates/wasm-validator/src/typed.rs",
    """                if ty.results.len() > 1 {\n                    return Err(ValidationError::UnsupportedIndirectResultArity {\n                        function,\n                        offset,\n                        results: ty.results.len(),\n                    });\n                }\n                pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;\n""",
    """                pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;\n""",
)
replace_once(
    "crates/wasm-validator/src/typed.rs",
    """    if let Some(&result) = ty.results.first() {\n        stack.push(result);\n    }\n""",
    """    stack.extend(ty.results.iter().copied());\n""",
)
replace_once(
    "crates/wasm-validator/src/typed.rs",
    """    let expected = frame.height + usize::from(frame.end_type.is_some());\n    if stack.len() != expected {\n        return Err(ValidationError::StackHeightMismatch {\n            function,\n            offset,\n            expected,\n            actual: stack.len(),\n        });\n    }\n    if let Some(expected_type) = frame.end_type {\n        let actual = stack[frame.height];\n        if actual != expected_type {\n            return Err(ValidationError::TypeMismatch {\n                function,\n                offset,\n                expected: expected_type,\n                actual,\n            });\n        }\n    }\n""",
    """    let expected = frame.height + frame.end_types.len();\n    if stack.len() != expected {\n        return Err(ValidationError::StackHeightMismatch {\n            function,\n            offset,\n            expected,\n            actual: stack.len(),\n        });\n    }\n    for (&actual, &expected_type) in stack[frame.height..].iter().zip(&frame.end_types) {\n        if actual != expected_type {\n            return Err(ValidationError::TypeMismatch {\n                function,\n                offset,\n                expected: expected_type,\n                actual,\n            });\n        }\n    }\n""",
)
replace_once(
    "crates/wasm-validator/src/typed.rs",
    """            return Ok(BlockSignature {\n                params: Vec::new(),\n                result: None,\n            });\n""",
    """            return Ok(BlockSignature {\n                params: Vec::new(),\n                results: Vec::new(),\n            });\n""",
)
replace_once(
    "crates/wasm-validator/src/typed.rs",
    """        return Ok(BlockSignature {\n            params: Vec::new(),\n            result: Some(result),\n        });\n""",
    """        return Ok(BlockSignature {\n            params: Vec::new(),\n            results: vec![result],\n        });\n""",
)
replace_once(
    "crates/wasm-validator/src/typed.rs",
    """    if ty.results.len() > 1 {\n        return Err(ValidationError::UnsupportedBlockResultArity {\n            function,\n            offset,\n            type_index,\n            results: ty.results.len(),\n        });\n    }\n    Ok(BlockSignature {\n        params: ty.params.clone(),\n        result: ty.results.first().copied(),\n    })\n""",
    """    Ok(BlockSignature {\n        params: ty.params.clone(),\n        results: ty.results.clone(),\n    })\n""",
)

# Runtime: internal result vectors, structured-control result vectors, and an
# additive public multi-result invocation API. The legacy API rejects >1 result
# before executing the function.
replace_once(
    "crates/wasm-runtime/src/lib.rs",
    """    ResultArityMismatch {\n        expected: usize,\n        actual: usize,\n    },\n    MemoryUnavailable,\n""",
    """    ResultArityMismatch {\n        expected: usize,\n        actual: usize,\n    },\n    MultiValueResultRequiresValuesApi {\n        results: usize,\n    },\n    MemoryUnavailable,\n""",
)
replace_once(
    "crates/wasm-runtime/src/lib.rs",
    """            Self::ResultArityMismatch { expected, actual } => write!(\n                f,\n                \"expected {expected} result values, stack contains {actual}\"\n            ),\n            Self::MemoryUnavailable => write!(f, \"module has no linear memory\"),\n""",
    """            Self::ResultArityMismatch { expected, actual } => write!(\n                f,\n                \"expected {expected} result values, stack contains {actual}\"\n            ),\n            Self::MultiValueResultRequiresValuesApi { results } => write!(\n                f,\n                \"export returns {results} values; use invoke_export_values for multi-value results\"\n            ),\n            Self::MemoryUnavailable => write!(f, \"module has no linear memory\"),\n""",
)
replace_once(
    "crates/wasm-runtime/src/lib.rs",
    """struct BlockSignature {\n    params: Vec<ValueType>,\n    result: Option<ValueType>,\n}\n""",
    """struct BlockSignature {\n    params: Vec<ValueType>,\n    results: Vec<ValueType>,\n}\n""",
)
replace_once(
    "crates/wasm-runtime/src/lib.rs",
    """    param_types: Vec<ValueType>,\n    result_type: Option<ValueType>,\n}\n\nimpl ExecControlFrame {\n    fn label_types(&self) -> Vec<ValueType> {\n        if self.kind == ControlKind::Loop {\n            self.param_types.clone()\n        } else {\n            self.result_type.into_iter().collect()\n        }\n    }\n}\n""",
    """    param_types: Vec<ValueType>,\n    result_types: Vec<ValueType>,\n}\n\nimpl ExecControlFrame {\n    fn label_types(&self) -> Vec<ValueType> {\n        if self.kind == ControlKind::Loop {\n            self.param_types.clone()\n        } else {\n            self.result_types.clone()\n        }\n    }\n}\n""",
)
replace_once(
    "crates/wasm-runtime/src/lib.rs",
    """            let result = instance.invoke_function(start, &[], 0, &mut budget)?;\n            if result.is_some() {\n                return Err(RuntimeError::ControlInvariant(\n                    \"validated start function returned a value\",\n                ));\n            }\n""",
    """            let results = instance.invoke_function(start, &[], 0, &mut budget)?;\n            if !results.is_empty() {\n                return Err(RuntimeError::ControlInvariant(\n                    \"validated start function returned values\",\n                ));\n            }\n""",
)
replace_once(
    "crates/wasm-runtime/src/lib.rs",
    """    pub fn invoke_export(\n        &mut self,\n        name: &str,\n        args: &[Value],\n    ) -> Result<Option<Value>, RuntimeError> {\n        let function_index = {\n            let export = self\n                .module\n                .exports\n                .iter()\n                .find(|export| export.name == name)\n                .ok_or_else(|| RuntimeError::ExportNotFound(name.to_owned()))?;\n            if export.kind != ExportKind::Function {\n                return Err(RuntimeError::ExportNotFunction(name.to_owned()));\n            }\n            export.index\n        };\n        let mut budget = ExecutionBudget::new(self.limits);\n        self.invoke_function(function_index, args, 0, &mut budget)\n    }\n\n    pub fn memory(&self) -> Option<&LinearMemory> {\n""",
    """    pub fn invoke_export(\n        &mut self,\n        name: &str,\n        args: &[Value],\n    ) -> Result<Option<Value>, RuntimeError> {\n        let function_index = self.exported_function_index(name)?;\n        let result_count = self.function_type(function_index)?.results.len();\n        if result_count > 1 {\n            return Err(RuntimeError::MultiValueResultRequiresValuesApi {\n                results: result_count,\n            });\n        }\n        let mut budget = ExecutionBudget::new(self.limits);\n        let mut results = self.invoke_function(function_index, args, 0, &mut budget)?;\n        Ok(results.pop())\n    }\n\n    pub fn invoke_export_values(\n        &mut self,\n        name: &str,\n        args: &[Value],\n    ) -> Result<Vec<Value>, RuntimeError> {\n        let function_index = self.exported_function_index(name)?;\n        let mut budget = ExecutionBudget::new(self.limits);\n        self.invoke_function(function_index, args, 0, &mut budget)\n    }\n\n    fn exported_function_index(&self, name: &str) -> Result<u32, RuntimeError> {\n        let export = self\n            .module\n            .exports\n            .iter()\n            .find(|export| export.name == name)\n            .ok_or_else(|| RuntimeError::ExportNotFound(name.to_owned()))?;\n        if export.kind != ExportKind::Function {\n            return Err(RuntimeError::ExportNotFunction(name.to_owned()));\n        }\n        Ok(export.index)\n    }\n\n    pub fn memory(&self) -> Option<&LinearMemory> {\n""",
)
replace_once(
    "crates/wasm-runtime/src/lib.rs",
    """    ) -> Result<Option<Value>, RuntimeError> {\n        budget.consume_host_call(self.limits.max_host_calls)?;\n""",
    """    ) -> Result<Vec<Value>, RuntimeError> {\n        budget.consume_host_call(self.limits.max_host_calls)?;\n""",
)
replace_once(
    "crates/wasm-runtime/src/lib.rs",
    """        Ok(result)\n    }\n\n    fn invoke_function(\n""",
    """        Ok(result.into_iter().collect())\n    }\n\n    fn invoke_function(\n""",
)
replace_once(
    "crates/wasm-runtime/src/lib.rs",
    """    ) -> Result<Option<Value>, RuntimeError> {\n        let function = function_index as usize;\n""",
    """    ) -> Result<Vec<Value>, RuntimeError> {\n        let function = function_index as usize;\n""",
)
replace_once(
    "crates/wasm-runtime/src/lib.rs",
    """        let result_type = ty.results.first().copied();\n        let function_end = code\n""",
    """        let result_types = ty.results.clone();\n        let function_end = code\n""",
)
replace_once(
    "crates/wasm-runtime/src/lib.rs",
    """            param_types: Vec::new(),\n            result_type,\n        }];\n""",
    """            param_types: Vec::new(),\n            result_types: result_types.clone(),\n        }];\n""",
)
# Three structured-control frame constructors.
for _ in range(2):
    replace_once(
        "crates/wasm-runtime/src/lib.rs",
        """                        param_types: signature.params,\n                        result_type: signature.result,\n""",
        """                        param_types: signature.params,\n                        result_types: signature.results,\n""",
    )
replace_once(
    "crates/wasm-runtime/src/lib.rs",
    """                        param_types: signature.params,\n                        result_type: signature.result,\n""",
    """                        param_types: signature.params,\n                        result_types: signature.results,\n""",
)
replace_once(
    "crates/wasm-runtime/src/lib.rs",
    """                    if let Some(result) =\n                        self.invoke_function(callee, &call_args, depth + 1, budget)?\n                    {\n                        stack.push(result);\n                    }\n""",
    """                    let results = self.invoke_function(callee, &call_args, depth + 1, budget)?;\n                    stack.extend(results);\n""",
)
replace_once(
    "crates/wasm-runtime/src/lib.rs",
    """                    if let Some(result) =\n                        self.invoke_function(callee, &call_args, depth + 1, budget)?\n                    {\n                        stack.push(result);\n                    }\n""",
    """                    let results = self.invoke_function(callee, &call_args, depth + 1, budget)?;\n                    stack.extend(results);\n""",
)
replace_once(
    "crates/wasm-runtime/src/lib.rs",
    """        let result_arity = usize::from(result_type.is_some());\n        if stack.len() != result_arity {\n            return Err(RuntimeError::ResultArityMismatch {\n                expected: result_arity,\n                actual: stack.len(),\n            });\n        }\n        if let (Some(expected), Some(value)) = (result_type, stack.last().copied()) {\n            numeric::expect_type(value, expected)?;\n        }\n        Ok(stack.pop())\n""",
    """        let result_arity = result_types.len();\n        if stack.len() != result_arity {\n            return Err(RuntimeError::ResultArityMismatch {\n                expected: result_arity,\n                actual: stack.len(),\n            });\n        }\n        validate_values(&result_types, &stack)?;\n        Ok(stack)\n""",
)
replace_once(
    "crates/wasm-runtime/src/lib.rs",
    """    let expected = frame.stack_height + usize::from(frame.result_type.is_some());\n    if stack.len() != expected {\n        return Err(RuntimeError::ControlStackMismatch {\n            expected,\n            actual: stack.len(),\n        });\n    }\n    if let Some(expected_type) = frame.result_type {\n        let value = *stack.last().ok_or(RuntimeError::StackUnderflow)?;\n        numeric::expect_type(value, expected_type)?;\n    }\n""",
    """    let expected = frame.stack_height + frame.result_types.len();\n    if stack.len() != expected {\n        return Err(RuntimeError::ControlStackMismatch {\n            expected,\n            actual: stack.len(),\n        });\n    }\n    validate_values(&frame.result_types, &stack[frame.stack_height..])?;\n""",
)
replace_once(
    "crates/wasm-runtime/src/lib.rs",
    """            return Ok(BlockSignature {\n                params: Vec::new(),\n                result: None,\n            });\n""",
    """            return Ok(BlockSignature {\n                params: Vec::new(),\n                results: Vec::new(),\n            });\n""",
)
replace_once(
    "crates/wasm-runtime/src/lib.rs",
    """        return Ok(BlockSignature {\n            params: Vec::new(),\n            result: Some(result),\n        });\n""",
    """        return Ok(BlockSignature {\n            params: Vec::new(),\n            results: vec![result],\n        });\n""",
)
replace_once(
    "crates/wasm-runtime/src/lib.rs",
    """    if ty.results.len() > 1 {\n        return Err(RuntimeError::UnsupportedBlockResultArity {\n            type_index,\n            results: ty.results.len(),\n        });\n    }\n    Ok(BlockSignature {\n        params: ty.params.clone(),\n        result: ty.results.first().copied(),\n    })\n""",
    """    Ok(BlockSignature {\n        params: ty.params.clone(),\n        results: ty.results.clone(),\n    })\n""",
)

# CLI uses the vector-return API; single-value output remains byte-for-byte equivalent.
replace_once(
    "crates/wasm-cli/src/main.rs",
    """    match instance.invoke_export(export, args)? {\n        Some(Value::I32(value)) => println!(\"{value}\"),\n        Some(Value::I64(value)) => println!(\"{value}\"),\n        Some(Value::F32(value)) => println!(\"{value}\"),\n        Some(Value::F64(value)) => println!(\"{value}\"),\n        None => println!(\"()\"),\n    }\n    Ok(())\n}\n\nfn usage() {\n""",
    """    let results = instance.invoke_export_values(export, args)?;\n    match results.as_slice() {\n        [] => println!(\"()\"),\n        [value] => println!(\"{}\", format_value(*value)),\n        values => println!(\n            \"({})\",\n            values\n                .iter()\n                .copied()\n                .map(format_value)\n                .collect::<Vec<_>>()\n                .join(\", \")\n        ),\n    }\n    Ok(())\n}\n\nfn format_value(value: Value) -> String {\n    match value {\n        Value::I32(value) => value.to_string(),\n        Value::I64(value) => value.to_string(),\n        Value::F32(value) => value.to_string(),\n        Value::F64(value) => value.to_string(),\n    }\n}\n\nfn usage() {\n""",
)

# Integration corpus.
test = r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};
use wasm_validator::{validate, ValidationError};

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

fn header() -> Vec<u8> {
    vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
}

fn two_result_type_section() -> Vec<u8> {
    vec![0x01, 0x60, 0x00, 0x02, I32, I64]
}

fn two_result_export_module() -> Vec<u8> {
    let mut module = header();
    push_section(&mut module, 1, &two_result_type_section());
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);
    push_section(
        &mut module,
        10,
        &[0x01, 0x06, 0x00, 0x41, 0x07, 0x42, 0x09, 0x0b],
    );
    module
}

fn direct_call_module() -> Vec<u8> {
    let mut module = header();
    push_section(&mut module, 1, &two_result_type_section());
    push_section(&mut module, 3, &[0x02, 0x00, 0x00]);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x01]);

    let mut code = vec![0x02];
    let callee = [0x00, 0x41, 0x0b, 0x42, 0x16, 0x0b];
    push_u32(&mut code, callee.len() as u32);
    code.extend_from_slice(&callee);
    let caller = [0x00, 0x10, 0x00, 0x0b];
    push_u32(&mut code, caller.len() as u32);
    code.extend_from_slice(&caller);
    push_section(&mut module, 10, &code);
    module
}

fn indirect_call_module() -> Vec<u8> {
    let mut module = header();
    push_section(&mut module, 1, &two_result_type_section());
    push_section(&mut module, 3, &[0x02, 0x00, 0x00]);
    push_section(&mut module, 4, &[0x01, 0x70, 0x00, 0x01]);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x01]);
    push_section(
        &mut module,
        9,
        &[0x01, 0x00, 0x41, 0x00, 0x0b, 0x01, 0x00],
    );

    let mut code = vec![0x02];
    let callee = [0x00, 0x41, 0x21, 0x42, 0x2c, 0x0b];
    push_u32(&mut code, callee.len() as u32);
    code.extend_from_slice(&callee);
    let caller = [0x00, 0x41, 0x00, 0x11, 0x00, 0x00, 0x0b];
    push_u32(&mut code, caller.len() as u32);
    code.extend_from_slice(&caller);
    push_section(&mut module, 10, &code);
    module
}

fn branching_block_module() -> Vec<u8> {
    let mut module = header();
    push_section(&mut module, 1, &two_result_type_section());
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);

    let body = [
        0x00, 0x02, 0x00, 0x41, 0x03, 0x42, 0x04, 0x0c, 0x00, 0x0b, 0x0b,
    ];
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);
    module
}

fn multi_result_if_module() -> Vec<u8> {
    let mut module = header();
    let types = [
        0x02, 0x60, 0x00, 0x02, I32, I64, 0x60, 0x01, I32, 0x02, I32, I64,
    ];
    push_section(&mut module, 1, &types);
    push_section(&mut module, 3, &[0x01, 0x01]);
    push_section(&mut module, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);

    let body = [
        0x00, 0x20, 0x00, 0x04, 0x00, 0x41, 0x0b, 0x42, 0x16, 0x05, 0x41, 0x21, 0x42,
        0x2c, 0x0b, 0x0b,
    ];
    let mut code = vec![0x01];
    push_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    push_section(&mut module, 10, &code);
    module
}

fn wrong_result_order_module() -> Vec<u8> {
    let mut module = header();
    push_section(&mut module, 1, &two_result_type_section());
    push_section(&mut module, 3, &[0x01, 0x00]);
    push_section(&mut module, 10, &[0x01, 0x06, 0x00, 0x42, 0x01, 0x41, 0x02, 0x0b]);
    module
}

fn multi_result_import_module() -> Vec<u8> {
    let mut module = header();
    push_section(&mut module, 1, &two_result_type_section());
    push_section(
        &mut module,
        2,
        &[
            0x01, 0x03, b'e', b'n', b'v', 0x05, b'm', b'u', b'l', b't', b'i', 0x00, 0x00,
        ],
    );
    module
}

fn instance(bytes: Vec<u8>) -> Instance {
    Instance::new(parse_module(&bytes).expect("multi-value fixture must parse"))
        .expect("multi-value fixture must validate and instantiate")
}

#[test]
fn exported_defined_function_returns_ordered_multi_values() {
    let mut vm = instance(two_result_export_module());
    assert_eq!(
        vm.invoke_export_values("run", &[]).unwrap(),
        vec![Value::I32(7), Value::I64(9)]
    );
}

#[test]
fn legacy_invoke_api_rejects_multi_value_before_execution() {
    let mut vm = instance(two_result_export_module());
    assert!(matches!(
        vm.invoke_export("run", &[]),
        Err(RuntimeError::MultiValueResultRequiresValuesApi { results: 2 })
    ));
}

#[test]
fn direct_call_propagates_all_results_in_stack_order() {
    let mut vm = instance(direct_call_module());
    assert_eq!(
        vm.invoke_export_values("run", &[]).unwrap(),
        vec![Value::I32(11), Value::I64(22)]
    );
}

#[test]
fn indirect_call_propagates_all_results_after_dynamic_type_check() {
    let mut vm = instance(indirect_call_module());
    assert_eq!(
        vm.invoke_export_values("run", &[]).unwrap(),
        vec![Value::I32(33), Value::I64(44)]
    );
}

#[test]
fn branch_preserves_multi_value_block_label_vector() {
    let mut vm = instance(branching_block_module());
    assert_eq!(
        vm.invoke_export_values("run", &[]).unwrap(),
        vec![Value::I32(3), Value::I64(4)]
    );
}

#[test]
fn if_else_validates_and_returns_each_multi_value_arm() {
    let mut vm = instance(multi_result_if_module());
    assert_eq!(
        vm.invoke_export_values("run", &[Value::I32(1)]).unwrap(),
        vec![Value::I32(11), Value::I64(22)]
    );
    assert_eq!(
        vm.invoke_export_values("run", &[Value::I32(0)]).unwrap(),
        vec![Value::I32(33), Value::I64(44)]
    );
}

#[test]
fn validator_rejects_wrong_multi_result_order() {
    let module = parse_module(&wrong_result_order_module()).unwrap();
    assert!(matches!(
        validate(&module),
        Err(ValidationError::TypeMismatch { .. })
    ));
}

#[test]
fn multi_result_host_imports_remain_fail_closed_at_host_abi_boundary() {
    let module = parse_module(&multi_result_import_module()).unwrap();
    assert!(matches!(
        validate(&module),
        Err(ValidationError::UnsupportedImportResultArity {
            import: 0,
            results: 2
        })
    ));
}
'''
Path("crates/wasm-runtime/tests/phase5c_multi_value.rs").write_text(test)

# Focused design note; roadmap/README are finalized after the validated product lands.
doc = r'''# Phase 5C — multi-value results

This slice extends defined-function and structured-control execution from zero-or-one result to ordered vectors of numeric results.

## Execution model

The operand stack already carries arbitrary value vectors. Multi-value support therefore promotes function and control-frame result metadata from `Option<ValueType>` to `Vec<ValueType>` and preserves values in declared stack order.

Supported in this slice:

- defined function signatures with multiple numeric results;
- exported multi-result defined functions;
- direct calls returning multiple values;
- `call_indirect` returning multiple values after the existing dynamic type check;
- type-index `block` / `if` signatures with multiple results;
- branch label vectors carrying multiple block/function results;
- function return carrying the complete declared result vector.

## Public API compatibility

`Instance::invoke_export_values` is the canonical vector-return API and returns `Vec<Value>` for zero, one, or many results.

The existing `Instance::invoke_export` API remains source-compatible for zero-or-one-result exports. If an export declares multiple results, it rejects the call before execution with `MultiValueResultRequiresValuesApi`; this prevents a compatibility error from executing a side-effecting function and only then discovering that the caller chose the wrong result API.

The CLI uses the vector-return path. Zero and one result retain their prior output shape; multiple values print as an ordered tuple-like list.

## Host ABI boundary

Registered Rust host callbacks remain zero-or-one-result in this slice. Function imports declaring multiple results therefore continue to fail closed during validation with `UnsupportedImportResultArity`.

This is deliberate scope separation: defined WebAssembly multi-value execution no longer depends on redesigning the trusted host callback return type.

## Validation invariants

- function-end stack height equals the complete result-vector arity;
- every result position must match the declared type in order;
- direct and indirect calls push every result in declaration order;
- loop labels continue to carry block parameters, while block/if/function labels carry complete result vectors;
- an `if` with any results still requires an `else`;
- type-index block signatures may carry multiple results, while inline block types remain zero-or-one-result by encoding.

## Non-goals

This slice does not add:

- multi-result Rust host callback registration;
- reference types beyond the existing funcref table subset;
- exception handling;
- tail calls;
- WASI;
- threads/shared memory.
'''
Path("docs/phase5c-multi-value.md").write_text(doc)
