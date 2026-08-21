//! Stack interpreter for the Phase-5B typed numeric WebAssembly subset.

use std::{cell::RefCell, collections::HashMap, fmt, rc::Rc};
use wasm_parser::{
    decode_i32, decode_i64, decode_s33, decode_u32, Constant, ExportKind, FuncType, ImportDesc,
    ImportKind, Module, ParseError, ValueType,
};

mod numeric;
pub use numeric::Value;
use wasm_validator::{validate, ValidationError, MAX_MEMORY_PAGES};

const DEFAULT_MAX_CALL_DEPTH: usize = 1024;
pub const WASM_PAGE_SIZE: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostCapabilities {
    memory_read: bool,
    memory_write: bool,
}

impl HostCapabilities {
    pub const NONE: Self = Self {
        memory_read: false,
        memory_write: false,
    };
    pub const MEMORY_READ: Self = Self {
        memory_read: true,
        memory_write: false,
    };
    pub const MEMORY_READ_WRITE: Self = Self {
        memory_read: true,
        memory_write: true,
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostError {
    CapabilityDenied(&'static str),
    MemoryUnavailable,
    MemoryOutOfBounds { address: u64, width: usize },
    Message(String),
}

impl HostError {
    pub fn message(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CapabilityDenied(capability) => {
                write!(f, "host capability {capability} was not granted")
            }
            Self::MemoryUnavailable => write!(f, "module has no linear memory"),
            Self::MemoryOutOfBounds { address, width } => write!(
                f,
                "host memory access at byte {address} with width {width} is out of bounds"
            ),
            Self::Message(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for HostError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalHandleError {
    Immutable,
    TypeMismatch {
        expected: ValueType,
        actual: ValueType,
    },
}

impl fmt::Display for GlobalHandleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Immutable => write!(f, "global is immutable"),
            Self::TypeMismatch { expected, actual } => {
                write!(f, "global expects {expected:?}, got {actual:?}")
            }
        }
    }
}

impl std::error::Error for GlobalHandleError {}

#[derive(Debug, Clone)]
pub struct GlobalHandle {
    value: Rc<RefCell<Value>>,
    value_type: ValueType,
    mutable: bool,
}

impl GlobalHandle {
    pub fn immutable(value: Value) -> Self {
        Self::new(value, false)
    }

    pub fn mutable(value: Value) -> Self {
        Self::new(value, true)
    }

    fn new(value: Value, mutable: bool) -> Self {
        Self {
            value: Rc::new(RefCell::new(value)),
            value_type: value.value_type(),
            mutable,
        }
    }

    pub fn value_type(&self) -> ValueType {
        self.value_type
    }

    pub fn is_mutable(&self) -> bool {
        self.mutable
    }

    pub fn get(&self) -> Value {
        *self.value.borrow()
    }

    pub fn set(&self, value: Value) -> Result<(), GlobalHandleError> {
        if !self.mutable {
            return Err(GlobalHandleError::Immutable);
        }
        let actual = value.value_type();
        if actual != self.value_type {
            return Err(GlobalHandleError::TypeMismatch {
                expected: self.value_type,
                actual,
            });
        }
        *self.value.borrow_mut() = value;
        Ok(())
    }
}

pub struct HostContext<'a> {
    memory: Option<&'a mut LinearMemory>,
    capabilities: HostCapabilities,
}

impl HostContext<'_> {
    pub fn memory_size_pages(&self) -> Result<u32, HostError> {
        if !self.capabilities.memory_read {
            return Err(HostError::CapabilityDenied("memory.read"));
        }
        self.memory
            .as_deref()
            .map(LinearMemory::size_pages)
            .ok_or(HostError::MemoryUnavailable)
    }

    pub fn read_memory(&self, address: u32, length: usize) -> Result<Vec<u8>, HostError> {
        if !self.capabilities.memory_read {
            return Err(HostError::CapabilityDenied("memory.read"));
        }
        let memory = self.memory.as_deref().ok_or(HostError::MemoryUnavailable)?;
        let range = memory.checked_host_range(address, length)?;
        Ok(memory.bytes[range].to_vec())
    }

    pub fn write_memory(&mut self, address: u32, bytes: &[u8]) -> Result<(), HostError> {
        if !self.capabilities.memory_write {
            return Err(HostError::CapabilityDenied("memory.write"));
        }
        let memory = self
            .memory
            .as_deref_mut()
            .ok_or(HostError::MemoryUnavailable)?;
        let range = memory.checked_host_range(address, bytes.len())?;
        memory.bytes[range].copy_from_slice(bytes);
        Ok(())
    }
}

type HostCallback = Box<
    dyn for<'a> FnMut(&mut HostContext<'a>, &[Value]) -> Result<Option<Value>, HostError> + 'static,
>;

struct HostFunction {
    params: Vec<ValueType>,
    results: Vec<ValueType>,
    capabilities: HostCapabilities,
    callback: HostCallback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostRegistryError {
    DuplicateFunction { module: String, name: String },
    DuplicateGlobal { module: String, name: String },
    UnsupportedSignature,
}

impl fmt::Display for HostRegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateFunction { module, name } => {
                write!(f, "host function {module}.{name} is already registered")
            }
            Self::DuplicateGlobal { module, name } => {
                write!(
                    f,
                    "host immutable global {module}.{name} is already registered"
                )
            }
            Self::UnsupportedSignature => write!(
                f,
                "host function signatures are currently i32-only with at most one result"
            ),
        }
    }
}

impl std::error::Error for HostRegistryError {}

#[derive(Default)]
pub struct HostRegistry {
    functions: HashMap<(String, String), HostFunction>,
    globals: HashMap<(String, String), GlobalHandle>,
}

impl fmt::Debug for HostRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostRegistry")
            .field("function_count", &self.functions.len())
            .field("global_count", &self.globals.len())
            .finish()
    }
}

impl HostRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<F>(
        &mut self,
        module: impl Into<String>,
        name: impl Into<String>,
        params: Vec<ValueType>,
        results: Vec<ValueType>,
        capabilities: HostCapabilities,
        callback: F,
    ) -> Result<(), HostRegistryError>
    where
        F: for<'a> FnMut(&mut HostContext<'a>, &[Value]) -> Result<Option<Value>, HostError>
            + 'static,
    {
        if results.len() > 1
            || params
                .iter()
                .chain(results.iter())
                .any(|ty| *ty != ValueType::I32)
        {
            return Err(HostRegistryError::UnsupportedSignature);
        }
        let module = module.into();
        let name = name.into();
        let key = (module.clone(), name.clone());
        if self.functions.contains_key(&key) {
            return Err(HostRegistryError::DuplicateFunction { module, name });
        }
        self.functions.insert(
            key,
            HostFunction {
                params,
                results,
                capabilities,
                callback: Box::new(callback),
            },
        );
        Ok(())
    }

    pub fn register_immutable_global(
        &mut self,
        module: impl Into<String>,
        name: impl Into<String>,
        value: Value,
    ) -> Result<(), HostRegistryError> {
        self.register_global(module, name, GlobalHandle::immutable(value))
    }

    pub fn register_global(
        &mut self,
        module: impl Into<String>,
        name: impl Into<String>,
        global: GlobalHandle,
    ) -> Result<(), HostRegistryError> {
        let module = module.into();
        let name = name.into();
        let key = (module.clone(), name.clone());
        if self.globals.contains_key(&key) {
            return Err(HostRegistryError::DuplicateGlobal { module, name });
        }
        self.globals.insert(key, global);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeLimits {
    pub max_call_depth: usize,
    pub max_memory_pages: u32,
    /// Optional instruction budget reset for each exported invocation.
    pub fuel: Option<u64>,
    /// Optional number of host calls permitted per exported invocation.
    pub max_host_calls: Option<u64>,
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            max_call_depth: DEFAULT_MAX_CALL_DEPTH,
            max_memory_pages: MAX_MEMORY_PAGES,
            fuel: None,
            max_host_calls: None,
        }
    }
}

#[derive(Debug)]
pub enum RuntimeError {
    Validation(ValidationError),
    Decode(ParseError),
    ExportNotFound(String),
    ExportNotFunction(String),
    FunctionOutOfBounds(u32),
    UnsupportedType(ValueType),
    WrongArgumentCount {
        expected: usize,
        actual: usize,
    },
    LocalOutOfBounds(u32),
    StackUnderflow,
    ValueTypeMismatch {
        expected: ValueType,
        actual: ValueType,
    },
    UnsupportedOpcode(u8),
    UnsupportedBlockType(u8),
    BlockTypeIndexOutOfBounds(u32),
    UnsupportedBlockResultArity {
        type_index: u32,
        results: usize,
    },
    BranchDepthOutOfBounds(u32),
    ControlStackMismatch {
        expected: usize,
        actual: usize,
    },
    ControlInvariant(&'static str),
    ResultArityMismatch {
        expected: usize,
        actual: usize,
    },
    MemoryUnavailable,
    MemoryIndexOutOfBounds(u32),
    MemoryOutOfBounds {
        address: u64,
        width: usize,
    },
    MemoryAllocationFailed {
        pages: u32,
    },
    MemoryLimitExceeded {
        minimum: u32,
        limit: u32,
    },
    DataSegmentOutOfBounds {
        segment: usize,
        offset: u64,
        length: usize,
    },
    TableAllocationFailed {
        elements: u32,
    },
    ElementSegmentOutOfBounds {
        segment: usize,
        offset: u64,
        length: usize,
    },
    GlobalOutOfBounds(u32),
    ImmutableGlobalSet(u32),
    TableIndexOutOfBounds(u32),
    TableElementOutOfBounds(u32),
    UninitializedTableElement(u32),
    IndirectCallTypeMismatch {
        expected_type: u32,
        function_index: u32,
    },
    UnresolvedImport {
        module: String,
        name: String,
    },
    UnresolvedGlobalImport {
        module: String,
        name: String,
    },
    HostGlobalTypeMismatch {
        module: String,
        name: String,
        expected: ValueType,
        actual: ValueType,
    },
    HostGlobalMutabilityMismatch {
        module: String,
        name: String,
        expected: bool,
        actual: bool,
    },
    UnsupportedObjectImport {
        module: String,
        name: String,
        kind: ImportKind,
    },
    HostSignatureMismatch {
        module: String,
        name: String,
    },
    HostCallFailed {
        module: String,
        name: String,
        error: HostError,
    },
    HostResultArityMismatch {
        module: String,
        name: String,
        expected: usize,
        actual: usize,
    },
    FuelExhausted,
    HostCallLimitExceeded {
        limit: u64,
    },
    CallDepthExceeded {
        limit: usize,
    },
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(error) => write!(f, "validation failed: {error}"),
            Self::Decode(error) => write!(f, "instruction decode failed: {error}"),
            Self::ExportNotFound(name) => write!(f, "export {name:?} not found"),
            Self::ExportNotFunction(name) => write!(f, "export {name:?} is not a function"),
            Self::FunctionOutOfBounds(index) => write!(f, "function index {index} is out of bounds"),
            Self::UnsupportedType(ty) => write!(f, "runtime does not yet execute type {ty:?}"),
            Self::WrongArgumentCount { expected, actual } => {
                write!(f, "expected {expected} arguments, got {actual}")
            }
            Self::LocalOutOfBounds(index) => write!(f, "local index {index} is out of bounds"),
            Self::StackUnderflow => write!(f, "operand stack underflow"),
            Self::ValueTypeMismatch { expected, actual } => {
                write!(f, "runtime expected {expected:?}, got {actual:?}")
            }
            Self::UnsupportedOpcode(opcode) => write!(f, "unsupported opcode 0x{opcode:02x}"),
            Self::UnsupportedBlockType(block_type) => {
                write!(f, "unsupported block type 0x{block_type:02x}")
            }
            Self::BlockTypeIndexOutOfBounds(type_index) => {
                write!(f, "block signature refers to missing type {type_index}")
            }
            Self::UnsupportedBlockResultArity { type_index, results } => write!(
                f,
                "block signature type {type_index} has {results} results; at most one is supported"
            ),
            Self::BranchDepthOutOfBounds(depth) => {
                write!(f, "branch label depth {depth} is out of bounds")
            }
            Self::ControlStackMismatch { expected, actual } => write!(
                f,
                "control frame expects stack height {expected}, got {actual}"
            ),
            Self::ControlInvariant(message) => {
                write!(f, "validated control invariant failed: {message}")
            }
            Self::ResultArityMismatch { expected, actual } => write!(
                f,
                "expected {expected} result values, stack contains {actual}"
            ),
            Self::MemoryUnavailable => write!(f, "module has no linear memory"),
            Self::MemoryIndexOutOfBounds(index) => write!(f, "memory index {index} is out of bounds"),
            Self::MemoryOutOfBounds { address, width } => write!(
                f,
                "linear-memory access at byte {address} with width {width} is out of bounds"
            ),
            Self::MemoryAllocationFailed { pages } => {
                write!(f, "failed to allocate linear memory with {pages} pages")
            }
            Self::MemoryLimitExceeded { minimum, limit } => write!(
                f,
                "module requires {minimum} initial memory pages but runtime limit is {limit}"
            ),
            Self::DataSegmentOutOfBounds {
                segment,
                offset,
                length,
            } => write!(
                f,
                "data segment {segment} at byte {offset} with length {length} does not fit initial memory"
            ),
            Self::TableAllocationFailed { elements } => {
                write!(f, "failed to allocate table with {elements} elements")
            }
            Self::ElementSegmentOutOfBounds { segment, offset, length } => write!(
                f,
                "element segment {segment} at table offset {offset} with length {length} does not fit initial table"
            ),
            Self::GlobalOutOfBounds(index) => write!(f, "global index {index} is out of bounds"),
            Self::ImmutableGlobalSet(index) => {
                write!(f, "global index {index} is immutable")
            }
            Self::TableIndexOutOfBounds(index) => {
                write!(f, "table index {index} is out of bounds")
            }
            Self::TableElementOutOfBounds(index) => {
                write!(f, "table element index {index} is out of bounds")
            }
            Self::UninitializedTableElement(index) => {
                write!(f, "table element index {index} is uninitialized")
            }
            Self::IndirectCallTypeMismatch { expected_type, function_index } => write!(
                f,
                "call_indirect expected type {expected_type}, but table function {function_index} has a different type"
            ),
            Self::UnresolvedImport { module, name } => {
                write!(f, "unresolved host function import {module}.{name}")
            }
            Self::UnresolvedGlobalImport { module, name } => {
                write!(f, "unresolved host immutable global import {module}.{name}")
            }
            Self::HostGlobalTypeMismatch {
                module,
                name,
                expected,
                actual,
            } => write!(
                f,
                "registered host global {module}.{name} has type {actual:?}, expected {expected:?}"
            ),
            Self::HostGlobalMutabilityMismatch {
                module,
                name,
                expected,
                actual,
            } => write!(
                f,
                "registered host global {module}.{name} has mutable={actual}, expected mutable={expected}"
            ),
            Self::UnsupportedObjectImport { module, name, kind } => write!(
                f,
                "import {module}.{name} has unsupported runtime object kind {kind:?}; shared object imports are not instantiated yet"
            ),
            Self::HostSignatureMismatch { module, name } => write!(
                f,
                "registered host function {module}.{name} does not match the module import signature"
            ),
            Self::HostCallFailed {
                module,
                name,
                error,
            } => write!(f, "host function {module}.{name} failed: {error}"),
            Self::HostResultArityMismatch {
                module,
                name,
                expected,
                actual,
            } => write!(
                f,
                "host function {module}.{name} returned {actual} values, expected {expected}"
            ),
            Self::FuelExhausted => write!(f, "execution fuel exhausted"),
            Self::HostCallLimitExceeded { limit } => {
                write!(f, "host call limit of {limit} was exceeded")
            }
            Self::CallDepthExceeded { limit } => {
                write!(f, "maximum call depth of {limit} was exceeded")
            }
        }
    }
}

impl std::error::Error for RuntimeError {}

impl From<ValidationError> for RuntimeError {
    fn from(value: ValidationError) -> Self {
        Self::Validation(value)
    }
}

impl From<ParseError> for RuntimeError {
    fn from(value: ParseError) -> Self {
        Self::Decode(value)
    }
}

#[derive(Debug, Clone)]
pub struct LinearMemory {
    bytes: Vec<u8>,
    max_pages: u32,
}

impl LinearMemory {
    fn new(
        min_pages: u32,
        declared_max: Option<u32>,
        runtime_max: u32,
    ) -> Result<Self, RuntimeError> {
        let runtime_max = runtime_max.min(MAX_MEMORY_PAGES);
        if min_pages > runtime_max {
            return Err(RuntimeError::MemoryLimitExceeded {
                minimum: min_pages,
                limit: runtime_max,
            });
        }
        let max_pages = declared_max.unwrap_or(MAX_MEMORY_PAGES).min(runtime_max);
        let byte_len = pages_to_bytes(min_pages)
            .ok_or(RuntimeError::MemoryAllocationFailed { pages: min_pages })?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(byte_len)
            .map_err(|_| RuntimeError::MemoryAllocationFailed { pages: min_pages })?;
        bytes.resize(byte_len, 0);
        Ok(Self { bytes, max_pages })
    }

    pub fn size_pages(&self) -> u32 {
        u32::try_from(self.bytes.len() / WASM_PAGE_SIZE)
            .expect("validated memory length always fits the WebAssembly page limit")
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    fn grow(&mut self, delta_pages: u32) -> i32 {
        let old_pages = self.size_pages();
        let Some(new_pages) = old_pages.checked_add(delta_pages) else {
            return -1;
        };
        if new_pages > self.max_pages {
            return -1;
        }
        let Some(new_len) = pages_to_bytes(new_pages) else {
            return -1;
        };
        if new_len > self.bytes.len() {
            let additional = new_len - self.bytes.len();
            if self.bytes.try_reserve_exact(additional).is_err() {
                return -1;
            }
            self.bytes.resize(new_len, 0);
        }
        old_pages as i32
    }

    fn checked_range(
        &self,
        address: i32,
        displacement: u32,
        width: usize,
    ) -> Result<std::ops::Range<usize>, RuntimeError> {
        let effective = u64::from(address as u32) + u64::from(displacement);
        let end = effective
            .checked_add(width as u64)
            .ok_or(RuntimeError::MemoryOutOfBounds {
                address: effective,
                width,
            })?;
        if end > self.bytes.len() as u64 {
            return Err(RuntimeError::MemoryOutOfBounds {
                address: effective,
                width,
            });
        }
        Ok(effective as usize..end as usize)
    }

    fn checked_host_range(
        &self,
        address: u32,
        width: usize,
    ) -> Result<std::ops::Range<usize>, HostError> {
        let start = u64::from(address);
        let end = start
            .checked_add(width as u64)
            .ok_or(HostError::MemoryOutOfBounds {
                address: start,
                width,
            })?;
        if end > self.bytes.len() as u64 {
            return Err(HostError::MemoryOutOfBounds {
                address: start,
                width,
            });
        }
        Ok(start as usize..end as usize)
    }

    fn load_i32(&self, address: i32, displacement: u32) -> Result<i32, RuntimeError> {
        let range = self.checked_range(address, displacement, 4)?;
        let bytes: [u8; 4] = self.bytes[range]
            .try_into()
            .expect("checked four-byte range");
        Ok(i32::from_le_bytes(bytes))
    }

    fn load_i8_s(&self, address: i32, displacement: u32) -> Result<i32, RuntimeError> {
        let range = self.checked_range(address, displacement, 1)?;
        Ok(i32::from(self.bytes[range.start] as i8))
    }

    fn load_i8_u(&self, address: i32, displacement: u32) -> Result<i32, RuntimeError> {
        let range = self.checked_range(address, displacement, 1)?;
        Ok(i32::from(self.bytes[range.start]))
    }

    fn load_i16_s(&self, address: i32, displacement: u32) -> Result<i32, RuntimeError> {
        let range = self.checked_range(address, displacement, 2)?;
        let bytes: [u8; 2] = self.bytes[range]
            .try_into()
            .expect("checked two-byte range");
        Ok(i32::from(i16::from_le_bytes(bytes)))
    }

    fn load_i16_u(&self, address: i32, displacement: u32) -> Result<i32, RuntimeError> {
        let range = self.checked_range(address, displacement, 2)?;
        let bytes: [u8; 2] = self.bytes[range]
            .try_into()
            .expect("checked two-byte range");
        Ok(i32::from(u16::from_le_bytes(bytes)))
    }

    fn store_i32(
        &mut self,
        address: i32,
        displacement: u32,
        value: i32,
    ) -> Result<(), RuntimeError> {
        let range = self.checked_range(address, displacement, 4)?;
        self.bytes[range].copy_from_slice(&value.to_le_bytes());
        Ok(())
    }

    fn store_i8(
        &mut self,
        address: i32,
        displacement: u32,
        value: i32,
    ) -> Result<(), RuntimeError> {
        let range = self.checked_range(address, displacement, 1)?;
        self.bytes[range.start] = value as u8;
        Ok(())
    }

    fn store_i16(
        &mut self,
        address: i32,
        displacement: u32,
        value: i32,
    ) -> Result<(), RuntimeError> {
        let range = self.checked_range(address, displacement, 2)?;
        self.bytes[range].copy_from_slice(&(value as u16).to_le_bytes());
        Ok(())
    }
}

fn allocate_table(elements: u32) -> Result<Vec<Option<u32>>, RuntimeError> {
    let len =
        usize::try_from(elements).map_err(|_| RuntimeError::TableAllocationFailed { elements })?;
    let mut table = Vec::new();
    table
        .try_reserve_exact(len)
        .map_err(|_| RuntimeError::TableAllocationFailed { elements })?;
    table.resize(len, None);
    Ok(table)
}

fn pages_to_bytes(pages: u32) -> Option<usize> {
    let bytes = u64::from(pages).checked_mul(WASM_PAGE_SIZE as u64)?;
    usize::try_from(bytes).ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ControlKind {
    Function,
    Block,
    Loop,
    If,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BlockSignature {
    params: Vec<ValueType>,
    result: Option<ValueType>,
}

#[derive(Debug, Clone)]
struct ControlInfo {
    kind: ControlKind,
    body_pc: usize,
    else_pc: Option<usize>,
    end_pc: usize,
    signature: BlockSignature,
}

#[derive(Debug, Clone)]
struct ControlMap {
    openers: Vec<Option<ControlInfo>>,
}

impl ControlMap {
    fn info(&self, opener: usize) -> Result<ControlInfo, RuntimeError> {
        self.openers
            .get(opener)
            .and_then(Clone::clone)
            .ok_or(RuntimeError::ControlInvariant(
                "structured-control opener has no boundary metadata",
            ))
    }
}

#[derive(Debug, Clone)]
struct PendingControl {
    opener: usize,
    kind: ControlKind,
    body_pc: usize,
    else_pc: Option<usize>,
    signature: BlockSignature,
}

#[derive(Debug, Clone)]
struct ExecControlFrame {
    kind: ControlKind,
    body_pc: usize,
    end_pc: usize,
    stack_height: usize,
    param_types: Vec<ValueType>,
    result_type: Option<ValueType>,
}

impl ExecControlFrame {
    fn label_types(&self) -> Vec<ValueType> {
        if self.kind == ControlKind::Loop {
            self.param_types.clone()
        } else {
            self.result_type.into_iter().collect()
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ExecutionBudget {
    fuel_remaining: Option<u64>,
    host_calls: u64,
}

impl ExecutionBudget {
    fn new(limits: RuntimeLimits) -> Self {
        Self {
            fuel_remaining: limits.fuel,
            host_calls: 0,
        }
    }

    fn consume_instruction(&mut self) -> Result<(), RuntimeError> {
        if let Some(remaining) = &mut self.fuel_remaining {
            if *remaining == 0 {
                return Err(RuntimeError::FuelExhausted);
            }
            *remaining -= 1;
        }
        Ok(())
    }

    fn consume_host_call(&mut self, limit: Option<u64>) -> Result<(), RuntimeError> {
        if limit.is_some_and(|limit| self.host_calls >= limit) {
            return Err(RuntimeError::HostCallLimitExceeded {
                limit: limit.expect("checked Some above"),
            });
        }
        self.host_calls = self.host_calls.saturating_add(1);
        Ok(())
    }
}

#[derive(Debug)]
pub struct Instance {
    module: Module,
    control_maps: Vec<ControlMap>,
    memory: Option<LinearMemory>,
    table: Option<Vec<Option<u32>>>,
    globals: Vec<GlobalHandle>,
    hosts: HostRegistry,
    limits: RuntimeLimits,
}

impl Instance {
    pub fn new(module: Module) -> Result<Self, RuntimeError> {
        Self::with_config(module, HostRegistry::new(), RuntimeLimits::default())
    }

    pub fn with_hosts(module: Module, hosts: HostRegistry) -> Result<Self, RuntimeError> {
        Self::with_config(module, hosts, RuntimeLimits::default())
    }

    pub fn with_config(
        module: Module,
        hosts: HostRegistry,
        limits: RuntimeLimits,
    ) -> Result<Self, RuntimeError> {
        validate(&module)?;
        reject_unsupported_object_imports(&module)?;
        validate_host_bindings(&module, &hosts)?;
        let control_maps = module
            .code
            .iter()
            .map(|body| build_control_map(&module, &body.code))
            .collect::<Result<Vec<_>, _>>()?;
        let memory = module
            .memories
            .first()
            .map(|memory_type| {
                LinearMemory::new(
                    memory_type.limits.min,
                    memory_type.limits.max,
                    limits.max_memory_pages,
                )
            })
            .transpose()?;
        let table = module
            .tables
            .first()
            .map(|table_type| allocate_table(table_type.limits.min))
            .transpose()?;
        let globals = instantiate_globals(&module, &hosts)?;

        let mut instance = Self {
            module,
            control_maps,
            memory,
            table,
            globals,
            hosts,
            limits,
        };
        instance.initialize_data_segments()?;
        instance.initialize_element_segments()?;
        if let Some(start) = instance.module.start {
            let mut budget = ExecutionBudget::new(instance.limits);
            let result = instance.invoke_function(start, &[], 0, &mut budget)?;
            if result.is_some() {
                return Err(RuntimeError::ControlInvariant(
                    "validated start function returned a value",
                ));
            }
        }
        Ok(instance)
    }

    pub fn invoke_export(
        &mut self,
        name: &str,
        args: &[Value],
    ) -> Result<Option<Value>, RuntimeError> {
        let function_index = {
            let export = self
                .module
                .exports
                .iter()
                .find(|export| export.name == name)
                .ok_or_else(|| RuntimeError::ExportNotFound(name.to_owned()))?;
            if export.kind != ExportKind::Function {
                return Err(RuntimeError::ExportNotFunction(name.to_owned()));
            }
            export.index
        };
        let mut budget = ExecutionBudget::new(self.limits);
        self.invoke_function(function_index, args, 0, &mut budget)
    }

    pub fn memory(&self) -> Option<&LinearMemory> {
        self.memory.as_ref()
    }

    pub fn global(&self, index: u32) -> Option<Value> {
        self.globals.get(index as usize).map(GlobalHandle::get)
    }

    fn initialize_data_segments(&mut self) -> Result<(), RuntimeError> {
        for (segment_index, segment) in self.module.data.iter().enumerate() {
            let offset = u64::from(segment.offset as u32);
            let end = offset.checked_add(segment.bytes.len() as u64).ok_or(
                RuntimeError::DataSegmentOutOfBounds {
                    segment: segment_index,
                    offset,
                    length: segment.bytes.len(),
                },
            )?;
            let memory = self
                .memory
                .as_mut()
                .ok_or(RuntimeError::MemoryUnavailable)?;
            if end > memory.bytes.len() as u64 {
                return Err(RuntimeError::DataSegmentOutOfBounds {
                    segment: segment_index,
                    offset,
                    length: segment.bytes.len(),
                });
            }
            memory.bytes[offset as usize..end as usize].copy_from_slice(&segment.bytes);
        }
        Ok(())
    }

    fn initialize_element_segments(&mut self) -> Result<(), RuntimeError> {
        let elements = self.module.elements.clone();
        for (segment_index, segment) in elements.iter().enumerate() {
            if segment.table_index != 0 {
                return Err(RuntimeError::TableIndexOutOfBounds(segment.table_index));
            }
            let offset = u64::from(segment.offset as u32);
            let end = offset
                .checked_add(segment.function_indices.len() as u64)
                .ok_or(RuntimeError::ElementSegmentOutOfBounds {
                    segment: segment_index,
                    offset,
                    length: segment.function_indices.len(),
                })?;
            let table = self
                .table
                .as_mut()
                .ok_or(RuntimeError::TableIndexOutOfBounds(0))?;
            if end > table.len() as u64 {
                return Err(RuntimeError::ElementSegmentOutOfBounds {
                    segment: segment_index,
                    offset,
                    length: segment.function_indices.len(),
                });
            }
            for (slot, &function_index) in segment.function_indices.iter().enumerate() {
                table[offset as usize + slot] = Some(function_index);
            }
        }
        Ok(())
    }

    fn memory_ref(&self) -> Result<&LinearMemory, RuntimeError> {
        self.memory.as_ref().ok_or(RuntimeError::MemoryUnavailable)
    }

    fn memory_mut(&mut self) -> Result<&mut LinearMemory, RuntimeError> {
        self.memory.as_mut().ok_or(RuntimeError::MemoryUnavailable)
    }

    fn function_type(&self, function_index: u32) -> Result<FuncType, RuntimeError> {
        let function = function_index as usize;
        let imported = self.module.function_import_count();
        let type_index = if function < imported {
            self.module
                .function_import_type_index(function)
                .ok_or(RuntimeError::FunctionOutOfBounds(function_index))?
        } else {
            let defined = function
                .checked_sub(imported)
                .ok_or(RuntimeError::FunctionOutOfBounds(function_index))?;
            *self
                .module
                .function_type_indices
                .get(defined)
                .ok_or(RuntimeError::FunctionOutOfBounds(function_index))?
        } as usize;
        self.module
            .types
            .get(type_index)
            .cloned()
            .ok_or(RuntimeError::FunctionOutOfBounds(function_index))
    }

    fn invoke_host(
        &mut self,
        import_index: usize,
        args: &[Value],
        budget: &mut ExecutionBudget,
    ) -> Result<Option<Value>, RuntimeError> {
        budget.consume_host_call(self.limits.max_host_calls)?;
        let import = self
            .module
            .function_import(import_index)
            .ok_or(RuntimeError::FunctionOutOfBounds(import_index as u32))?
            .clone();
        let type_index = import
            .function_type_index()
            .ok_or(RuntimeError::FunctionOutOfBounds(import_index as u32))?;
        let ty = self.module.types[type_index as usize].clone();
        validate_values(&ty.params, args)?;

        let key = (import.module.clone(), import.name.clone());
        let (hosts, memory) = (&mut self.hosts, &mut self.memory);
        let host = hosts
            .functions
            .get_mut(&key)
            .ok_or_else(|| RuntimeError::UnresolvedImport {
                module: import.module.clone(),
                name: import.name.clone(),
            })?;
        let mut context = HostContext {
            memory: memory.as_mut(),
            capabilities: host.capabilities,
        };
        let result =
            (host.callback)(&mut context, args).map_err(|error| RuntimeError::HostCallFailed {
                module: import.module.clone(),
                name: import.name.clone(),
                error,
            })?;
        let actual = usize::from(result.is_some());
        if actual != ty.results.len() {
            return Err(RuntimeError::HostResultArityMismatch {
                module: import.module,
                name: import.name,
                expected: ty.results.len(),
                actual,
            });
        }
        Ok(result)
    }

    fn invoke_function(
        &mut self,
        function_index: u32,
        args: &[Value],
        depth: usize,
        budget: &mut ExecutionBudget,
    ) -> Result<Option<Value>, RuntimeError> {
        let function = function_index as usize;
        let imported = self.module.function_import_count();
        if function < imported {
            return self.invoke_host(function, args, budget);
        }
        if depth >= self.limits.max_call_depth {
            return Err(RuntimeError::CallDepthExceeded {
                limit: self.limits.max_call_depth,
            });
        }

        let defined = function
            .checked_sub(imported)
            .ok_or(RuntimeError::FunctionOutOfBounds(function_index))?;
        let type_index = *self
            .module
            .function_type_indices
            .get(defined)
            .ok_or(RuntimeError::FunctionOutOfBounds(function_index))?
            as usize;
        let ty = self.module.types[type_index].clone();
        validate_values(&ty.params, args)?;

        let body = self.module.code[defined].clone();
        let control_map = self.control_maps[defined].clone();
        let mut locals = args.to_vec();
        let mut local_types = ty.params.clone();
        for &(count, local_type) in &body.locals {
            let count = count as usize;
            locals.extend(std::iter::repeat(numeric::zero(local_type)).take(count));
            local_types.extend(std::iter::repeat(local_type).take(count));
        }

        let mut stack = Vec::<Value>::new();
        let mut pc = 0usize;
        let code = &body.code;
        let result_type = ty.results.first().copied();
        let function_end = code
            .len()
            .checked_sub(1)
            .ok_or(RuntimeError::ControlInvariant("function body is empty"))?;
        let mut controls = vec![ExecControlFrame {
            kind: ControlKind::Function,
            body_pc: 0,
            end_pc: function_end,
            stack_height: 0,
            param_types: Vec::new(),
            result_type,
        }];

        while pc < code.len() {
            budget.consume_instruction()?;
            let offset = pc;
            let opcode = code[pc];
            pc += 1;

            match opcode {
                0x02 | 0x03 => {
                    let signature = read_block_signature(&self.module, code, &mut pc)?;
                    let info = control_map.info(offset)?;
                    let kind = if opcode == 0x02 {
                        ControlKind::Block
                    } else {
                        ControlKind::Loop
                    };
                    ensure_control_info(&info, kind, &signature)?;
                    let stack_height = control_entry_height(&stack, &signature.params)?;
                    controls.push(ExecControlFrame {
                        kind,
                        body_pc: info.body_pc,
                        end_pc: info.end_pc,
                        stack_height,
                        param_types: signature.params,
                        result_type: signature.result,
                    });
                }
                0x04 => {
                    let signature = read_block_signature(&self.module, code, &mut pc)?;
                    let condition = numeric::i32_from_stack(&mut stack)?;
                    let info = control_map.info(offset)?;
                    ensure_control_info(&info, ControlKind::If, &signature)?;
                    let stack_height = control_entry_height(&stack, &signature.params)?;
                    let frame = ExecControlFrame {
                        kind: ControlKind::If,
                        body_pc: info.body_pc,
                        end_pc: info.end_pc,
                        stack_height,
                        param_types: signature.params,
                        result_type: signature.result,
                    };
                    if condition != 0 {
                        controls.push(frame);
                    } else if let Some(else_pc) = info.else_pc {
                        controls.push(frame);
                        pc = else_pc + 1;
                    } else {
                        stack.truncate(frame.stack_height);
                        pc = info.end_pc + 1;
                    }
                }
                0x05 => {
                    let frame = controls
                        .last()
                        .cloned()
                        .ok_or(RuntimeError::ControlInvariant(
                            "else encountered without active control frame",
                        ))?;
                    if frame.kind != ControlKind::If {
                        return Err(RuntimeError::ControlInvariant(
                            "else encountered outside active if",
                        ));
                    }
                    exit_control_frame(&mut controls, &stack)?;
                    pc = frame.end_pc + 1;
                }
                0x0b => {
                    let frame = controls
                        .last()
                        .cloned()
                        .ok_or(RuntimeError::ControlInvariant(
                            "end encountered without active control frame",
                        ))?;
                    if frame.end_pc != offset {
                        return Err(RuntimeError::ControlInvariant(
                            "end offset does not match active control frame",
                        ));
                    }
                    exit_control_frame(&mut controls, &stack)?;
                    if frame.kind == ControlKind::Function {
                        break;
                    }
                }
                0x0c => {
                    let branch_depth = read_u32_immediate(code, &mut pc)?;
                    branch_to(&mut controls, &mut stack, branch_depth, &mut pc, code.len())?;
                }
                0x0d => {
                    let branch_depth = read_u32_immediate(code, &mut pc)?;
                    let condition = numeric::i32_from_stack(&mut stack)?;
                    if condition != 0 {
                        branch_to(&mut controls, &mut stack, branch_depth, &mut pc, code.len())?;
                    }
                }
                0x0f => {
                    let branch_depth =
                        controls
                            .len()
                            .checked_sub(1)
                            .ok_or(RuntimeError::ControlInvariant(
                                "return executed without function frame",
                            ))? as u32;
                    branch_to(&mut controls, &mut stack, branch_depth, &mut pc, code.len())?;
                }
                0x10 => {
                    let callee = read_u32_immediate(code, &mut pc)?;
                    let callee_type = self.function_type(callee)?;
                    let param_count = callee_type.params.len();
                    if stack.len() < param_count {
                        return Err(RuntimeError::StackUnderflow);
                    }
                    let call_args = stack.split_off(stack.len() - param_count);
                    if let Some(result) =
                        self.invoke_function(callee, &call_args, depth + 1, budget)?
                    {
                        stack.push(result);
                    }
                }
                0x11 => {
                    let expected_type_index = read_u32_immediate(code, &mut pc)?;
                    let table_index = read_u32_immediate(code, &mut pc)?;
                    if table_index != 0 || self.table.is_none() {
                        return Err(RuntimeError::TableIndexOutOfBounds(table_index));
                    }
                    let element_index = numeric::i32_from_stack(&mut stack)? as u32;
                    let callee = self
                        .table
                        .as_ref()
                        .and_then(|table| table.get(element_index as usize))
                        .ok_or(RuntimeError::TableElementOutOfBounds(element_index))?
                        .ok_or(RuntimeError::UninitializedTableElement(element_index))?;
                    let expected_type = self
                        .module
                        .types
                        .get(expected_type_index as usize)
                        .cloned()
                        .ok_or(RuntimeError::ControlInvariant(
                            "validated call_indirect type is missing",
                        ))?;
                    let actual_type = self.function_type(callee)?;
                    if actual_type != expected_type {
                        return Err(RuntimeError::IndirectCallTypeMismatch {
                            expected_type: expected_type_index,
                            function_index: callee,
                        });
                    }
                    let param_count = expected_type.params.len();
                    if stack.len() < param_count {
                        return Err(RuntimeError::StackUnderflow);
                    }
                    let call_args = stack.split_off(stack.len() - param_count);
                    if let Some(result) =
                        self.invoke_function(callee, &call_args, depth + 1, budget)?
                    {
                        stack.push(result);
                    }
                }
                0x20 => {
                    let index = read_u32_immediate(code, &mut pc)?;
                    let value = *locals
                        .get(index as usize)
                        .ok_or(RuntimeError::LocalOutOfBounds(index))?;
                    stack.push(value);
                }
                0x21 => {
                    let index = read_u32_immediate(code, &mut pc)?;
                    let expected = *local_types
                        .get(index as usize)
                        .ok_or(RuntimeError::LocalOutOfBounds(index))?;
                    let value = numeric::pop_typed(&mut stack, expected)?;
                    let local = locals
                        .get_mut(index as usize)
                        .ok_or(RuntimeError::LocalOutOfBounds(index))?;
                    *local = value;
                }
                0x22 => {
                    let index = read_u32_immediate(code, &mut pc)?;
                    let expected = *local_types
                        .get(index as usize)
                        .ok_or(RuntimeError::LocalOutOfBounds(index))?;
                    let value = *stack.last().ok_or(RuntimeError::StackUnderflow)?;
                    numeric::expect_type(value, expected)?;
                    let local = locals
                        .get_mut(index as usize)
                        .ok_or(RuntimeError::LocalOutOfBounds(index))?;
                    *local = value;
                }
                0x23 => {
                    let index = read_u32_immediate(code, &mut pc)?;
                    let value = self
                        .globals
                        .get(index as usize)
                        .ok_or(RuntimeError::GlobalOutOfBounds(index))?
                        .get();
                    stack.push(value);
                }
                0x24 => {
                    let index = read_u32_immediate(code, &mut pc)?;
                    let global_type = self
                        .module
                        .global_type(index)
                        .ok_or(RuntimeError::GlobalOutOfBounds(index))?;
                    if !global_type.mutable {
                        return Err(RuntimeError::ImmutableGlobalSet(index));
                    }
                    let value = numeric::pop_typed(&mut stack, global_type.value_type)?;
                    self.globals
                        .get(index as usize)
                        .ok_or(RuntimeError::GlobalOutOfBounds(index))?
                        .set(value)
                        .map_err(|error| match error {
                            GlobalHandleError::Immutable => RuntimeError::ImmutableGlobalSet(index),
                            GlobalHandleError::TypeMismatch { expected, actual } => {
                                RuntimeError::ValueTypeMismatch { expected, actual }
                            }
                        })?;
                }
                0x28 | 0x2c..=0x2f => {
                    let (_, displacement) = read_memarg(code, &mut pc)?;
                    let address = numeric::i32_from_stack(&mut stack)?;
                    let value = match opcode {
                        0x28 => self.memory_ref()?.load_i32(address, displacement)?,
                        0x2c => self.memory_ref()?.load_i8_s(address, displacement)?,
                        0x2d => self.memory_ref()?.load_i8_u(address, displacement)?,
                        0x2e => self.memory_ref()?.load_i16_s(address, displacement)?,
                        0x2f => self.memory_ref()?.load_i16_u(address, displacement)?,
                        _ => unreachable!(),
                    };
                    stack.push(Value::I32(value));
                }
                0x36 | 0x3a | 0x3b => {
                    let (_, displacement) = read_memarg(code, &mut pc)?;
                    let value = numeric::i32_from_stack(&mut stack)?;
                    let address = numeric::i32_from_stack(&mut stack)?;
                    match opcode {
                        0x36 => self.memory_mut()?.store_i32(address, displacement, value)?,
                        0x3a => self.memory_mut()?.store_i8(address, displacement, value)?,
                        0x3b => self.memory_mut()?.store_i16(address, displacement, value)?,
                        _ => unreachable!(),
                    }
                }
                0x3f => {
                    let memory_index = read_u32_immediate(code, &mut pc)?;
                    ensure_runtime_memory_index(self, memory_index)?;
                    stack.push(Value::I32(self.memory_ref()?.size_pages() as i32));
                }
                0x40 => {
                    let memory_index = read_u32_immediate(code, &mut pc)?;
                    ensure_runtime_memory_index(self, memory_index)?;
                    let delta = numeric::i32_from_stack(&mut stack)? as u32;
                    let previous = self.memory_mut()?.grow(delta);
                    stack.push(Value::I32(previous));
                }
                0x41 => {
                    let (value, used) = decode_i32(&code[pc..])?;
                    pc += used;
                    stack.push(Value::I32(value));
                }
                0x42 => {
                    let (value, used) = decode_i64(&code[pc..])?;
                    pc += used;
                    stack.push(Value::I64(value));
                }
                0x43 => {
                    let bits = read_fixed_u32(code, &mut pc)?;
                    stack.push(Value::F32(f32::from_bits(bits)));
                }
                0x44 => {
                    let bits = read_fixed_u64(code, &mut pc)?;
                    stack.push(Value::F64(f64::from_bits(bits)));
                }
                0x45..=0x4f => numeric::compare_i32(&mut stack, opcode)?,
                0x50..=0x5a => numeric::compare_i64(&mut stack, opcode)?,
                0x5b..=0x60 => numeric::compare_f32(&mut stack, opcode)?,
                0x61..=0x66 => numeric::compare_f64(&mut stack, opcode)?,
                0x6a => numeric::binary_i32(&mut stack, i32::wrapping_add)?,
                0x6b => numeric::binary_i32(&mut stack, i32::wrapping_sub)?,
                0x6c => numeric::binary_i32(&mut stack, i32::wrapping_mul)?,
                0x7c => numeric::binary_i64(&mut stack, i64::wrapping_add)?,
                0x7d => numeric::binary_i64(&mut stack, i64::wrapping_sub)?,
                0x7e => numeric::binary_i64(&mut stack, i64::wrapping_mul)?,
                0x92 => numeric::binary_f32(&mut stack, |a, b| a + b)?,
                0x93 => numeric::binary_f32(&mut stack, |a, b| a - b)?,
                0x94 => numeric::binary_f32(&mut stack, |a, b| a * b)?,
                0x95 => numeric::binary_f32(&mut stack, |a, b| a / b)?,
                0xa0 => numeric::binary_f64(&mut stack, |a, b| a + b)?,
                0xa1 => numeric::binary_f64(&mut stack, |a, b| a - b)?,
                0xa2 => numeric::binary_f64(&mut stack, |a, b| a * b)?,
                0xa3 => numeric::binary_f64(&mut stack, |a, b| a / b)?,
                0xa7 | 0xac | 0xad | 0xb6 | 0xbb => numeric::convert(&mut stack, opcode)?,
                other => return Err(RuntimeError::UnsupportedOpcode(other)),
            }
        }

        let result_arity = usize::from(result_type.is_some());
        if stack.len() != result_arity {
            return Err(RuntimeError::ResultArityMismatch {
                expected: result_arity,
                actual: stack.len(),
            });
        }
        if let (Some(expected), Some(value)) = (result_type, stack.last().copied()) {
            numeric::expect_type(value, expected)?;
        }
        Ok(stack.pop())
    }
}

fn reject_unsupported_object_imports(module: &Module) -> Result<(), RuntimeError> {
    for import in &module.imports {
        match import.desc {
            ImportDesc::Function(_) | ImportDesc::Global(_) => {}
            _ => {
                return Err(RuntimeError::UnsupportedObjectImport {
                    module: import.module.clone(),
                    name: import.name.clone(),
                    kind: import.kind(),
                });
            }
        }
    }
    Ok(())
}

fn validate_host_bindings(module: &Module, hosts: &HostRegistry) -> Result<(), RuntimeError> {
    for import in module.function_imports() {
        let key = (import.module.clone(), import.name.clone());
        let host = hosts
            .functions
            .get(&key)
            .ok_or_else(|| RuntimeError::UnresolvedImport {
                module: import.module.clone(),
                name: import.name.clone(),
            })?;
        let type_index = import
            .function_type_index()
            .expect("function_imports yields only function descriptors");
        let declared = &module.types[type_index as usize];
        if host.params != declared.params || host.results != declared.results {
            return Err(RuntimeError::HostSignatureMismatch {
                module: import.module.clone(),
                name: import.name.clone(),
            });
        }
    }

    for import in &module.imports {
        let ImportDesc::Global(global_type) = import.desc else {
            continue;
        };
        let key = (import.module.clone(), import.name.clone());
        let global =
            hosts
                .globals
                .get(&key)
                .ok_or_else(|| RuntimeError::UnresolvedGlobalImport {
                    module: import.module.clone(),
                    name: import.name.clone(),
                })?;
        let actual = global.value_type();
        if actual != global_type.value_type {
            return Err(RuntimeError::HostGlobalTypeMismatch {
                module: import.module.clone(),
                name: import.name.clone(),
                expected: global_type.value_type,
                actual,
            });
        }
        if global.is_mutable() != global_type.mutable {
            return Err(RuntimeError::HostGlobalMutabilityMismatch {
                module: import.module.clone(),
                name: import.name.clone(),
                expected: global_type.mutable,
                actual: global.is_mutable(),
            });
        }
    }
    Ok(())
}

fn instantiate_globals(
    module: &Module,
    hosts: &HostRegistry,
) -> Result<Vec<GlobalHandle>, RuntimeError> {
    let mut globals = Vec::with_capacity(module.global_count());
    for import in &module.imports {
        let ImportDesc::Global(global_type) = import.desc else {
            continue;
        };
        let key = (import.module.clone(), import.name.clone());
        let global = hosts.globals.get(&key).cloned().ok_or_else(|| {
            RuntimeError::UnresolvedGlobalImport {
                module: import.module.clone(),
                name: import.name.clone(),
            }
        })?;
        if global.value_type() != global_type.value_type {
            return Err(RuntimeError::HostGlobalTypeMismatch {
                module: import.module.clone(),
                name: import.name.clone(),
                expected: global_type.value_type,
                actual: global.value_type(),
            });
        }
        if global.is_mutable() != global_type.mutable {
            return Err(RuntimeError::HostGlobalMutabilityMismatch {
                module: import.module.clone(),
                name: import.name.clone(),
                expected: global_type.mutable,
                actual: global.is_mutable(),
            });
        }
        globals.push(global);
    }
    globals.extend(
        module
            .globals
            .iter()
            .map(|global| GlobalHandle::new(value_from_constant(global.init), global.ty.mutable)),
    );
    Ok(globals)
}

fn ensure_runtime_memory_index(instance: &Instance, index: u32) -> Result<(), RuntimeError> {
    if index != 0 || instance.memory.is_none() {
        Err(RuntimeError::MemoryIndexOutOfBounds(index))
    } else {
        Ok(())
    }
}

fn validate_values(types: &[ValueType], values: &[Value]) -> Result<(), RuntimeError> {
    if values.len() != types.len() {
        return Err(RuntimeError::WrongArgumentCount {
            expected: types.len(),
            actual: values.len(),
        });
    }
    for (&expected, &value) in types.iter().zip(values) {
        numeric::expect_type(value, expected)?;
    }
    Ok(())
}

fn value_from_constant(value: Constant) -> Value {
    match value {
        Constant::I32(value) => Value::I32(value),
        Constant::I64(value) => Value::I64(value),
        Constant::F32(bits) => Value::F32(f32::from_bits(bits)),
        Constant::F64(bits) => Value::F64(f64::from_bits(bits)),
    }
}

fn control_entry_height(stack: &[Value], params: &[ValueType]) -> Result<usize, RuntimeError> {
    if stack.len() < params.len() {
        return Err(RuntimeError::StackUnderflow);
    }
    let height = stack.len() - params.len();
    validate_values(params, &stack[height..])?;
    Ok(height)
}

fn ensure_control_info(
    info: &ControlInfo,
    kind: ControlKind,
    signature: &BlockSignature,
) -> Result<(), RuntimeError> {
    if info.kind != kind || &info.signature != signature {
        return Err(RuntimeError::ControlInvariant(
            "control metadata disagrees with instruction stream",
        ));
    }
    Ok(())
}

fn exit_control_frame(
    controls: &mut Vec<ExecControlFrame>,
    stack: &[Value],
) -> Result<(), RuntimeError> {
    let frame = controls.pop().ok_or(RuntimeError::ControlInvariant(
        "attempted to leave missing control frame",
    ))?;
    let expected = frame.stack_height + usize::from(frame.result_type.is_some());
    if stack.len() != expected {
        return Err(RuntimeError::ControlStackMismatch {
            expected,
            actual: stack.len(),
        });
    }
    if let Some(expected_type) = frame.result_type {
        let value = *stack.last().ok_or(RuntimeError::StackUnderflow)?;
        numeric::expect_type(value, expected_type)?;
    }
    Ok(())
}

fn branch_to(
    controls: &mut Vec<ExecControlFrame>,
    stack: &mut Vec<Value>,
    depth: u32,
    pc: &mut usize,
    code_len: usize,
) -> Result<(), RuntimeError> {
    let depth_usize = depth as usize;
    let target_index = controls
        .len()
        .checked_sub(depth_usize + 1)
        .ok_or(RuntimeError::BranchDepthOutOfBounds(depth))?;
    let target = controls[target_index].clone();
    let label_types = target.label_types();
    let label_arity = label_types.len();
    let current_height =
        controls
            .last()
            .map(|frame| frame.stack_height)
            .ok_or(RuntimeError::ControlInvariant(
                "branch executed without active control frame",
            ))?;
    if stack.len().saturating_sub(current_height) < label_arity {
        return Err(RuntimeError::StackUnderflow);
    }

    let label_values = stack[stack.len() - label_arity..].to_vec();
    validate_values(&label_types, &label_values)?;
    stack.truncate(target.stack_height);
    stack.extend(label_values);

    match target.kind {
        ControlKind::Loop => {
            controls.truncate(target_index + 1);
            *pc = target.body_pc;
        }
        ControlKind::Block | ControlKind::If => {
            controls.truncate(target_index);
            *pc = target.end_pc + 1;
        }
        ControlKind::Function => {
            controls.clear();
            *pc = code_len;
        }
    }
    Ok(())
}

fn build_control_map(module: &Module, code: &[u8]) -> Result<ControlMap, RuntimeError> {
    let mut openers = vec![None; code.len()];
    let mut pending = Vec::<PendingControl>::new();
    let mut pc = 0usize;

    while pc < code.len() {
        let offset = pc;
        let opcode = code[pc];
        pc += 1;
        match opcode {
            0x02..=0x04 => {
                let signature = read_block_signature(module, code, &mut pc)?;
                let kind = match opcode {
                    0x02 => ControlKind::Block,
                    0x03 => ControlKind::Loop,
                    0x04 => ControlKind::If,
                    _ => unreachable!(),
                };
                pending.push(PendingControl {
                    opener: offset,
                    kind,
                    body_pc: pc,
                    else_pc: None,
                    signature,
                });
            }
            0x05 => {
                let frame = pending.last_mut().ok_or(RuntimeError::ControlInvariant(
                    "else has no pending structured-control opener",
                ))?;
                if frame.kind != ControlKind::If || frame.else_pc.is_some() {
                    return Err(RuntimeError::ControlInvariant(
                        "else does not match exactly one if",
                    ));
                }
                frame.else_pc = Some(offset);
            }
            0x0b => {
                if let Some(frame) = pending.pop() {
                    openers[frame.opener] = Some(ControlInfo {
                        kind: frame.kind,
                        body_pc: frame.body_pc,
                        else_pc: frame.else_pc,
                        end_pc: offset,
                        signature: frame.signature,
                    });
                } else if pc != code.len() {
                    return Err(RuntimeError::ControlInvariant(
                        "function end occurs before final byte",
                    ));
                }
            }
            0x0c | 0x0d | 0x10 | 0x20..=0x24 | 0x3f | 0x40 => {
                let _ = read_u32_immediate(code, &mut pc)?;
            }
            0x11 => {
                let _ = read_u32_immediate(code, &mut pc)?;
                let _ = read_u32_immediate(code, &mut pc)?;
            }
            0x28 | 0x2c..=0x2f | 0x36 | 0x3a | 0x3b => {
                let _ = read_memarg(code, &mut pc)?;
            }
            0x41 => {
                let (_, used) = decode_i32(&code[pc..])?;
                pc += used;
            }
            0x42 => {
                let (_, used) = decode_i64(&code[pc..])?;
                pc += used;
            }
            0x43 => {
                let _ = read_fixed_u32(code, &mut pc)?;
            }
            0x44 => {
                let _ = read_fixed_u64(code, &mut pc)?;
            }
            0x0f
            | 0x45..=0x66
            | 0x6a..=0x6c
            | 0x7c..=0x7e
            | 0x92..=0x95
            | 0xa0..=0xa3
            | 0xa7
            | 0xac
            | 0xad
            | 0xb6
            | 0xbb => {}
            other => return Err(RuntimeError::UnsupportedOpcode(other)),
        }
    }

    if !pending.is_empty() {
        return Err(RuntimeError::ControlInvariant(
            "structured control is not fully closed",
        ));
    }
    Ok(ControlMap { openers })
}

fn read_block_signature(
    module: &Module,
    code: &[u8],
    pc: &mut usize,
) -> Result<BlockSignature, RuntimeError> {
    let first = *code
        .get(*pc)
        .ok_or(RuntimeError::ControlInvariant("missing block type"))?;
    let immediate = match first {
        0x40 => {
            *pc += 1;
            return Ok(BlockSignature {
                params: Vec::new(),
                result: None,
            });
        }
        0x7f => Some(ValueType::I32),
        0x7e => Some(ValueType::I64),
        0x7d => Some(ValueType::F32),
        0x7c => Some(ValueType::F64),
        _ => None,
    };
    if let Some(result) = immediate {
        *pc += 1;
        return Ok(BlockSignature {
            params: Vec::new(),
            result: Some(result),
        });
    }

    let (raw, used) = decode_s33(&code[*pc..])?;
    *pc += used;
    if raw < 0 {
        return Err(RuntimeError::UnsupportedBlockType(first));
    }
    let type_index = u32::try_from(raw)
        .map_err(|_| RuntimeError::ControlInvariant("block type index exceeds u32"))?;
    let ty = module
        .types
        .get(type_index as usize)
        .ok_or(RuntimeError::BlockTypeIndexOutOfBounds(type_index))?;
    if ty.results.len() > 1 {
        return Err(RuntimeError::UnsupportedBlockResultArity {
            type_index,
            results: ty.results.len(),
        });
    }
    Ok(BlockSignature {
        params: ty.params.clone(),
        result: ty.results.first().copied(),
    })
}

fn read_memarg(code: &[u8], pc: &mut usize) -> Result<(u32, u32), RuntimeError> {
    let alignment = read_u32_immediate(code, pc)?;
    let displacement = read_u32_immediate(code, pc)?;
    Ok((alignment, displacement))
}

fn read_u32_immediate(code: &[u8], pc: &mut usize) -> Result<u32, RuntimeError> {
    let (value, used) = decode_u32(&code[*pc..])?;
    *pc += used;
    Ok(value)
}

fn read_fixed_u32(code: &[u8], pc: &mut usize) -> Result<u32, RuntimeError> {
    let end = (*pc)
        .checked_add(4)
        .filter(|end| *end <= code.len())
        .ok_or(RuntimeError::ControlInvariant("truncated f32 immediate"))?;
    let bytes: [u8; 4] = code[*pc..end]
        .try_into()
        .expect("checked four-byte immediate");
    *pc = end;
    Ok(u32::from_le_bytes(bytes))
}

fn read_fixed_u64(code: &[u8], pc: &mut usize) -> Result<u64, RuntimeError> {
    let end = (*pc)
        .checked_add(8)
        .filter(|end| *end <= code.len())
        .ok_or(RuntimeError::ControlInvariant("truncated f64 immediate"))?;
    let bytes: [u8; 8] = code[*pc..end]
        .try_into()
        .expect("checked eight-byte immediate");
    *pc = end;
    Ok(u64::from_le_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_parser::{parse_module, Export, Import, Limits, MemoryType};

    fn module_with_body(params: u8, results: u8, body: &[u8]) -> Vec<u8> {
        build_module(params, results, body, None, None)
    }

    fn module_with_memory(
        params: u8,
        results: u8,
        body: &[u8],
        max_pages: Option<u8>,
        data: Option<(u8, &[u8])>,
    ) -> Vec<u8> {
        build_module(params, results, body, Some((1, max_pages)), data)
    }

    fn build_module(
        params: u8,
        results: u8,
        body: &[u8],
        memory: Option<(u8, Option<u8>)>,
        data: Option<(u8, &[u8])>,
    ) -> Vec<u8> {
        let mut type_section = vec![0x01, 0x60, params];
        type_section.extend(std::iter::repeat(0x7f).take(params as usize));
        type_section.push(results);
        type_section.extend(std::iter::repeat(0x7f).take(results as usize));

        let mut code_payload = vec![0x01, (body.len() + 1) as u8, 0x00];
        code_payload.extend(body);

        let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        push_section(&mut bytes, 1, &type_section);
        push_section(&mut bytes, 3, &[0x01, 0x00]);
        if let Some((min, max)) = memory {
            let payload = match max {
                Some(max) => vec![0x01, 0x01, min, max],
                None => vec![0x01, 0x00, min],
            };
            push_section(&mut bytes, 5, &payload);
        }
        push_section(&mut bytes, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00]);
        push_section(&mut bytes, 10, &code_payload);
        if let Some((offset, payload)) = data {
            let mut data_section = vec![0x01, 0x00, 0x41, offset, 0x0b, payload.len() as u8];
            data_section.extend(payload);
            push_section(&mut bytes, 11, &data_section);
        }
        bytes
    }

    fn imported_double_module() -> Vec<u8> {
        let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        push_section(&mut bytes, 1, &[0x01, 0x60, 0x01, 0x7f, 0x01, 0x7f]);
        push_section(
            &mut bytes,
            2,
            &[
                0x01, 0x03, b'e', b'n', b'v', 0x06, b'd', b'o', b'u', b'b', b'l', b'e', 0x00, 0x00,
            ],
        );
        push_section(&mut bytes, 3, &[0x01, 0x00]);
        push_section(&mut bytes, 7, &[0x01, 0x03, b'r', b'u', b'n', 0x00, 0x01]);
        push_section(
            &mut bytes,
            10,
            &[0x01, 0x06, 0x00, 0x20, 0x00, 0x10, 0x00, 0x0b],
        );
        bytes
    }

    fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
        assert!(
            payload.len() < 128,
            "test helper only encodes one-byte lengths"
        );
        module.push(id);
        module.push(payload.len() as u8);
        module.extend(payload);
    }

    fn instance(bytes: &[u8]) -> Instance {
        Instance::new(parse_module(bytes).expect("parse test module"))
            .expect("validate test module")
    }

    fn double_hosts() -> HostRegistry {
        let mut hosts = HostRegistry::new();
        hosts
            .register(
                "env",
                "double",
                vec![ValueType::I32],
                vec![ValueType::I32],
                HostCapabilities::NONE,
                |_ctx, args| Ok(Some(Value::I32(args[0].as_i32().wrapping_mul(2)))),
            )
            .unwrap();
        hosts
    }

    #[test]
    fn executes_imported_host_function() {
        let module = parse_module(&imported_double_module()).unwrap();
        let mut vm = Instance::with_hosts(module, double_hosts()).unwrap();
        assert_eq!(
            vm.invoke_export("run", &[Value::I32(21)]).unwrap(),
            Some(Value::I32(42))
        );
    }

    #[test]
    fn unresolved_import_fails_instantiation() {
        let module = parse_module(&imported_double_module()).unwrap();
        let error = Instance::new(module).expect_err("missing host binding must fail");
        assert!(matches!(error, RuntimeError::UnresolvedImport { .. }));
    }

    #[test]
    fn host_signature_mismatch_fails_instantiation() {
        let module = parse_module(&imported_double_module()).unwrap();
        let mut hosts = HostRegistry::new();
        hosts
            .register(
                "env",
                "double",
                vec![],
                vec![ValueType::I32],
                HostCapabilities::NONE,
                |_ctx, _args| Ok(Some(Value::I32(1))),
            )
            .unwrap();
        let error = Instance::with_hosts(module, hosts).expect_err("signature mismatch must fail");
        assert!(matches!(error, RuntimeError::HostSignatureMismatch { .. }));
    }

    #[test]
    fn host_memory_write_requires_capability() {
        let module = Module {
            types: vec![FuncType {
                params: vec![ValueType::I32],
                results: vec![],
            }],
            imports: vec![Import {
                module: "env".into(),
                name: "poke".into(),
                desc: ImportDesc::Function(0),
            }],
            memories: vec![MemoryType {
                limits: Limits {
                    min: 1,
                    max: Some(1),
                },
            }],
            exports: vec![Export {
                name: "poke".into(),
                kind: ExportKind::Function,
                index: 0,
            }],
            ..Module::default()
        };

        let mut denied = HostRegistry::new();
        denied
            .register(
                "env",
                "poke",
                vec![ValueType::I32],
                vec![],
                HostCapabilities::NONE,
                |ctx, args| {
                    ctx.write_memory(0, &[args[0].as_i32() as u8])?;
                    Ok(None)
                },
            )
            .unwrap();
        let mut vm = Instance::with_hosts(module.clone(), denied).unwrap();
        assert!(matches!(
            vm.invoke_export("poke", &[Value::I32(0x2a)]),
            Err(RuntimeError::HostCallFailed {
                error: HostError::CapabilityDenied("memory.write"),
                ..
            })
        ));

        let mut allowed = HostRegistry::new();
        allowed
            .register(
                "env",
                "poke",
                vec![ValueType::I32],
                vec![],
                HostCapabilities::MEMORY_READ_WRITE,
                |ctx, args| {
                    ctx.write_memory(0, &[args[0].as_i32() as u8])?;
                    Ok(None)
                },
            )
            .unwrap();
        let mut vm = Instance::with_hosts(module, allowed).unwrap();
        vm.invoke_export("poke", &[Value::I32(0x2a)]).unwrap();
        assert_eq!(vm.memory().unwrap().bytes()[0], 0x2a);
    }

    #[test]
    fn fuel_limit_stops_execution() {
        let module = parse_module(&module_with_body(
            1,
            1,
            &[0x20, 0x00, 0x41, 0x01, 0x6a, 0x0b],
        ))
        .unwrap();
        let limits = RuntimeLimits {
            fuel: Some(2),
            ..RuntimeLimits::default()
        };
        let mut vm = Instance::with_config(module, HostRegistry::new(), limits).unwrap();
        assert!(matches!(
            vm.invoke_export("run", &[Value::I32(1)]),
            Err(RuntimeError::FuelExhausted)
        ));
    }

    #[test]
    fn runtime_memory_cap_is_enforced() {
        let module = parse_module(&module_with_memory(0, 0, &[0x0b], Some(2), None)).unwrap();
        let limits = RuntimeLimits {
            max_memory_pages: 0,
            ..RuntimeLimits::default()
        };
        let error = Instance::with_config(module, HostRegistry::new(), limits)
            .expect_err("initial memory must respect runtime cap");
        assert!(matches!(error, RuntimeError::MemoryLimitExceeded { .. }));
    }

    #[test]
    fn host_call_limit_is_enforced() {
        let module = parse_module(&imported_double_module()).unwrap();
        let limits = RuntimeLimits {
            max_host_calls: Some(0),
            ..RuntimeLimits::default()
        };
        let mut vm = Instance::with_config(module, double_hosts(), limits).unwrap();
        assert!(matches!(
            vm.invoke_export("run", &[Value::I32(2)]),
            Err(RuntimeError::HostCallLimitExceeded { limit: 0 })
        ));
    }

    #[test]
    fn executes_i32_add() {
        let bytes = module_with_body(2, 1, &[0x20, 0x00, 0x20, 0x01, 0x6a, 0x0b]);
        let mut vm = instance(&bytes);
        let result = vm
            .invoke_export("run", &[Value::I32(20), Value::I32(22)])
            .expect("execution succeeds");
        assert_eq!(result, Some(Value::I32(42)));
    }

    #[test]
    fn integer_arithmetic_wraps_like_wasm() {
        let bytes = module_with_body(1, 1, &[0x20, 0x00, 0x41, 0x01, 0x6a, 0x0b]);
        let mut vm = instance(&bytes);
        let result = vm
            .invoke_export("run", &[Value::I32(i32::MAX)])
            .expect("execution succeeds");
        assert_eq!(result, Some(Value::I32(i32::MIN)));
    }

    #[test]
    fn stores_and_loads_i32_little_endian() {
        let bytes = module_with_memory(
            2,
            1,
            &[
                0x20, 0x00, 0x20, 0x01, 0x36, 0x02, 0x00, 0x20, 0x00, 0x28, 0x02, 0x00, 0x0b,
            ],
            Some(2),
            None,
        );
        let mut vm = instance(&bytes);
        assert_eq!(
            vm.invoke_export("run", &[Value::I32(8), Value::I32(0x1234_5678)])
                .unwrap(),
            Some(Value::I32(0x1234_5678))
        );
        assert_eq!(
            &vm.memory().unwrap().bytes()[8..12],
            &[0x78, 0x56, 0x34, 0x12]
        );
    }

    #[test]
    fn narrow_loads_sign_and_zero_extend() {
        let signed = module_with_memory(
            2,
            1,
            &[
                0x20, 0x00, 0x20, 0x01, 0x3a, 0x00, 0x00, 0x20, 0x00, 0x2c, 0x00, 0x00, 0x0b,
            ],
            None,
            None,
        );
        let mut vm = instance(&signed);
        assert_eq!(
            vm.invoke_export("run", &[Value::I32(0), Value::I32(0xff)])
                .unwrap(),
            Some(Value::I32(-1))
        );

        let unsigned = module_with_memory(
            2,
            1,
            &[
                0x20, 0x00, 0x20, 0x01, 0x3a, 0x00, 0x00, 0x20, 0x00, 0x2d, 0x00, 0x00, 0x0b,
            ],
            None,
            None,
        );
        let mut vm = instance(&unsigned);
        assert_eq!(
            vm.invoke_export("run", &[Value::I32(0), Value::I32(0xff)])
                .unwrap(),
            Some(Value::I32(255))
        );
    }

    #[test]
    fn data_segment_initializes_memory() {
        let bytes = module_with_memory(0, 0, &[0x0b], Some(2), Some((4, b"wasm")));
        let vm = instance(&bytes);
        assert_eq!(&vm.memory().unwrap().bytes()[4..8], b"wasm");
    }

    #[test]
    fn memory_grow_returns_previous_size_and_minus_one_on_limit() {
        let bytes = module_with_memory(1, 1, &[0x20, 0x00, 0x40, 0x00, 0x0b], Some(2), None);
        let mut vm = instance(&bytes);
        assert_eq!(
            vm.invoke_export("run", &[Value::I32(1)]).unwrap(),
            Some(Value::I32(1))
        );
        assert_eq!(vm.memory().unwrap().size_pages(), 2);
        assert_eq!(
            vm.invoke_export("run", &[Value::I32(1)]).unwrap(),
            Some(Value::I32(-1))
        );
        assert_eq!(vm.memory().unwrap().size_pages(), 2);
    }

    #[test]
    fn memory_access_out_of_bounds_traps() {
        let bytes = module_with_memory(1, 1, &[0x20, 0x00, 0x28, 0x02, 0x00, 0x0b], None, None);
        let mut vm = instance(&bytes);
        let error = vm
            .invoke_export("run", &[Value::I32((WASM_PAGE_SIZE - 1) as i32)])
            .expect_err("four-byte load must cross memory boundary");
        assert!(matches!(error, RuntimeError::MemoryOutOfBounds { .. }));
    }

    #[test]
    fn data_segment_out_of_bounds_fails_instantiation() {
        let bytes = module_with_memory(0, 0, &[0x0b], None, Some((4, b"wasm")));
        let mut module = parse_module(&bytes).expect("parse test module");
        module.data[0].offset = (WASM_PAGE_SIZE - 2) as i32;
        let error = Instance::new(module).expect_err("segment must fit initial memory");
        assert!(matches!(error, RuntimeError::DataSegmentOutOfBounds { .. }));
    }

    #[test]
    fn executes_if_else_on_both_paths() {
        let bytes = module_with_body(
            1,
            1,
            &[
                0x20, 0x00, 0x04, 0x7f, 0x41, 0x0b, 0x05, 0x41, 0x16, 0x0b, 0x0b,
            ],
        );
        let mut vm = instance(&bytes);
        assert_eq!(
            vm.invoke_export("run", &[Value::I32(1)]).unwrap(),
            Some(Value::I32(11))
        );
        assert_eq!(
            vm.invoke_export("run", &[Value::I32(0)]).unwrap(),
            Some(Value::I32(22))
        );
    }

    #[test]
    fn branch_exits_block_with_result_value() {
        let bytes = module_with_body(
            0,
            1,
            &[0x02, 0x7f, 0x41, 0x2a, 0x0c, 0x00, 0x41, 0x01, 0x0b, 0x0b],
        );
        let mut vm = instance(&bytes);
        assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(Value::I32(42)));
    }

    #[test]
    fn branch_depth_can_exit_an_outer_block() {
        let bytes = module_with_body(
            0,
            1,
            &[
                0x02, 0x7f, 0x02, 0x40, 0x41, 0x2a, 0x0c, 0x01, 0x0b, 0x41, 0x07, 0x0b, 0x0b,
            ],
        );
        let mut vm = instance(&bytes);
        assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(Value::I32(42)));
    }

    #[test]
    fn loop_branch_restarts_loop_header() {
        let bytes = module_with_body(
            1,
            1,
            &[
                0x03, 0x40, 0x20, 0x00, 0x41, 0x01, 0x6b, 0x22, 0x00, 0x0d, 0x00, 0x0b, 0x20, 0x00,
                0x0b,
            ],
        );
        let mut vm = instance(&bytes);
        assert_eq!(
            vm.invoke_export("run", &[Value::I32(3)]).unwrap(),
            Some(Value::I32(0))
        );
    }

    #[test]
    fn return_exits_nested_control_immediately() {
        let bytes = module_with_body(
            0,
            1,
            &[0x02, 0x40, 0x41, 0x2a, 0x0f, 0x0b, 0x41, 0x07, 0x0b],
        );
        let mut vm = instance(&bytes);
        assert_eq!(vm.invoke_export("run", &[]).unwrap(), Some(Value::I32(42)));
    }

    #[test]
    fn unsupported_opcode_is_rejected_before_execution() {
        let bytes = module_with_body(0, 1, &[0x01, 0x0b]);
        let module = parse_module(&bytes).expect("parse test module");
        let error = Instance::new(module).expect_err("unsupported opcode must fail validation");
        assert!(matches!(
            error,
            RuntimeError::Validation(ValidationError::UnsupportedOpcode { opcode: 0x01, .. })
        ));
    }
}
