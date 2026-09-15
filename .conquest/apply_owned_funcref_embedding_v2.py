from pathlib import Path

path = Path("crates/wasm-runtime/src/lib.rs")
text = path.read_text()


def replace_once(old: str, new: str, label: str) -> None:
    global text
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one anchor, found {count}")
    text = text.replace(old, new, 1)

replace_once(
    '''#[derive(Clone)]
pub struct FunctionRef {
    owner: Weak<()>,
    function_index: u32,
}

impl fmt::Debug for FunctionRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FunctionRef(..)")
    }
}
''',
    '''#[derive(Clone)]
pub struct FunctionRef {
    owner: Weak<()>,
    function_index: u32,
}

impl fmt::Debug for FunctionRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FunctionRef(..)")
    }
}

impl PartialEq for FunctionRef {
    fn eq(&self, other: &Self) -> bool {
        self.function_index == other.function_index && self.owner.ptr_eq(&other.owner)
    }
}

impl Eq for FunctionRef {}

#[derive(Debug, Clone, PartialEq)]
pub enum ExternValue {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    V128(Rc<[u8; 16]>),
    FuncRef(Option<FunctionRef>),
}

impl ExternValue {
    pub fn value_type(&self) -> ValueType {
        match self {
            Self::I32(_) => ValueType::I32,
            Self::I64(_) => ValueType::I64,
            Self::F32(_) => ValueType::F32,
            Self::F64(_) => ValueType::F64,
            Self::V128(_) => ValueType::V128,
            Self::FuncRef(_) => ValueType::FuncRef,
        }
    }
}
''',
    "FunctionRef/ExternValue",
)

replace_once(
    '''    UnownedFunctionReferenceArgument,
    UnsupportedOpcode(u8),
''',
    '''    UnownedFunctionReferenceArgument,
    ForeignFunctionReferenceArgument,
    ExpiredFunctionReferenceArgument,
    UnsupportedOpcode(u8),
''',
    "RuntimeError variants",
)

replace_once(
    '''            Self::UnownedFunctionReferenceArgument => write!(
                f,
                "non-null function references cannot cross the embedding boundary until instance ownership is represented"
            ),
''',
    '''            Self::UnownedFunctionReferenceArgument => write!(
                f,
                "raw non-null function references cannot cross the embedding boundary without instance ownership"
            ),
            Self::ForeignFunctionReferenceArgument => write!(
                f,
                "function reference argument belongs to a different live instance"
            ),
            Self::ExpiredFunctionReferenceArgument => write!(
                f,
                "function reference argument belongs to an instance that no longer exists"
            ),
''',
    "RuntimeError display",
)

invoke = '''    pub fn invoke_export_values(
        &mut self,
        name: &str,
        args: &[Value],
    ) -> Result<Vec<Value>, RuntimeError> {
        let function_index = self.exported_function_index(name)?;
        let function_type = self.function_type(function_index)?;
        validate_embedding_arguments(&function_type.params, args)?;
        let mut budget = ExecutionBudget::new(self.limits);
        self.invoke_function(function_index, args, 0, &mut budget)
    }
'''

replace_once(
    invoke,
    invoke
    + '''
    pub fn invoke_export_extern_values(
        &mut self,
        name: &str,
        args: &[ExternValue],
    ) -> Result<Vec<ExternValue>, RuntimeError> {
        let function_index = self.exported_function_index(name)?;
        let function_type = self.function_type(function_index)?;
        validate_extern_value_types(&function_type.params, args)?;
        let internal_args = args
            .iter()
            .map(|value| self.extern_value_to_internal(value))
            .collect::<Result<Vec<_>, _>>()?;
        let mut budget = ExecutionBudget::new(self.limits);
        let results = self.invoke_function(function_index, &internal_args, 0, &mut budget)?;
        Ok(results
            .into_iter()
            .map(|value| self.internal_value_to_extern(value))
            .collect())
    }

    fn extern_value_to_internal(&self, value: &ExternValue) -> Result<Value, RuntimeError> {
        Ok(match value {
            ExternValue::I32(value) => Value::I32(*value),
            ExternValue::I64(value) => Value::I64(*value),
            ExternValue::F32(value) => Value::F32(*value),
            ExternValue::F64(value) => Value::F64(*value),
            ExternValue::V128(value) => Value::V128(value.clone()),
            ExternValue::FuncRef(None) => Value::FuncRef(None),
            ExternValue::FuncRef(Some(reference)) => {
                let Some(owner) = reference.owner.upgrade() else {
                    return Err(RuntimeError::ExpiredFunctionReferenceArgument);
                };
                if !Rc::ptr_eq(&owner, &self.identity) {
                    return Err(RuntimeError::ForeignFunctionReferenceArgument);
                }
                Value::FuncRef(Some(reference.function_index))
            }
        })
    }

    fn internal_value_to_extern(&self, value: Value) -> ExternValue {
        match value {
            Value::I32(value) => ExternValue::I32(value),
            Value::I64(value) => ExternValue::I64(value),
            Value::F32(value) => ExternValue::F32(value),
            Value::F64(value) => ExternValue::F64(value),
            Value::V128(value) => ExternValue::V128(value),
            Value::FuncRef(None) => ExternValue::FuncRef(None),
            Value::FuncRef(Some(function_index)) => ExternValue::FuncRef(Some(FunctionRef {
                owner: Rc::downgrade(&self.identity),
                function_index,
            })),
        }
    }
''',
    "external invoke API",
)

anchor = '''fn validate_embedding_arguments(types: &[ValueType], values: &[Value]) -> Result<(), RuntimeError> {
'''
if text.count(anchor) != 1:
    raise SystemExit(f"extern validation anchor: expected one, found {text.count(anchor)}")
text = text.replace(
    anchor,
    '''fn validate_extern_value_types(
    types: &[ValueType],
    values: &[ExternValue],
) -> Result<(), RuntimeError> {
    if types.len() != values.len() {
        return Err(RuntimeError::WrongArgumentCount {
            expected: types.len(),
            actual: values.len(),
        });
    }
    for (&expected, value) in types.iter().zip(values) {
        let actual = value.value_type();
        if actual != expected {
            return Err(RuntimeError::ValueTypeMismatch { expected, actual });
        }
    }
    Ok(())
}

'''
    + anchor,
    1,
)

path.write_text(text)
