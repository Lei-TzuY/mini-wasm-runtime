//! Stack interpreter for the Phase-5B typed numeric WebAssembly subset.

use std::{
    cell::RefCell,
    collections::HashMap,
    fmt,
    rc::{Rc, Weak},
};
use wasm_parser::{
    decode_i32, decode_i64, decode_s33, decode_u32, Constant, DataMode, ElementMode, ExportKind,
    FuncType, ImportDesc, ImportKind, Module, ParseError, ValueType,
};

mod numeric;
pub use numeric::Value;
use wasm_validator::{validate, ValidationError, MAX_MEMORY_PAGES};

pub const MAX_CALL_DEPTH: usize = 32;
const DEFAULT_MAX_CALL_DEPTH: usize = MAX_CALL_DEPTH;
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

#[derive(Clone)]
pub struct FunctionRef {
    owner: Weak<()>,
    function_index: u32,
}

impl fmt::Debug for FunctionRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FunctionRef(..)")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableHandleError {
    InvalidLimits { minimum: u32, maximum: u32 },
    AllocationFailed { elements: u32 },
    OutOfBounds { index: u32, length: u32 },
    ForeignFunctionReference { index: u32 },
    AlreadyBound,
}

impl fmt::Display for TableHandleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits { minimum, maximum } => write!(
                f,
                "table minimum {minimum} exceeds declared maximum {maximum}"
            ),
            Self::AllocationFailed { elements } => {
                write!(f, "failed to allocate table with {elements} elements")
            }
            Self::OutOfBounds { index, length } => {
                write!(
                    f,
                    "table element index {index} is out of bounds for length {length}"
                )
            }
            Self::ForeignFunctionReference { index } => write!(
                f,
                "table element {index} contains a function reference from another instance"
            ),
            Self::AlreadyBound => write!(f, "table is already bound to a live instance"),
        }
    }
}

impl std::error::Error for TableHandleError {}

#[derive(Clone)]
pub struct TableHandle {
    slots: Rc<RefCell<Vec<Option<FunctionRef>>>>,
    maximum: Option<u32>,
    owner: Rc<RefCell<Option<Weak<()>>>>,
}

impl fmt::Debug for TableHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TableHandle")
            .field("length", &self.len())
            .field("maximum", &self.maximum)
            .finish()
    }
}

impl TableHandle {
    pub fn new(minimum: u32, maximum: Option<u32>) -> Result<Self, TableHandleError> {
        if let Some(maximum) = maximum {
            if minimum > maximum {
                return Err(TableHandleError::InvalidLimits { minimum, maximum });
            }
        }
        let length = usize::try_from(minimum)
            .map_err(|_| TableHandleError::AllocationFailed { elements: minimum })?;
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(length)
            .map_err(|_| TableHandleError::AllocationFailed { elements: minimum })?;
        slots.resize(length, None);
        Ok(Self {
            slots: Rc::new(RefCell::new(slots)),
            maximum,
            owner: Rc::new(RefCell::new(None)),
        })
    }

    pub fn len(&self) -> u32 {
        u32::try_from(self.slots.borrow().len())
            .expect("table length originates from a u32 minimum")
    }

    pub fn is_empty(&self) -> bool {
        self.slots.borrow().is_empty()
    }

    pub fn maximum(&self) -> Option<u32> {
        self.maximum
    }

    pub fn get(&self, index: u32) -> Result<Option<FunctionRef>, TableHandleError> {
        self.slots
            .borrow()
            .get(index as usize)
            .cloned()
            .ok_or_else(|| TableHandleError::OutOfBounds {
                index,
                length: self.len(),
            })
    }

    pub fn set(&self, index: u32, function: Option<FunctionRef>) -> Result<(), TableHandleError> {
        let length = self.len();
        let mut slots = self.slots.borrow_mut();
        let slot = slots
            .get_mut(index as usize)
            .ok_or(TableHandleError::OutOfBounds { index, length })?;
        *slot = function;
        Ok(())
    }

    fn bind(&self, owner: &Rc<()>) -> Result<(), TableHandleError> {
        let mut binding = self.owner.borrow_mut();
        if let Some(existing) = binding.as_ref().and_then(Weak::upgrade) {
            if !Rc::ptr_eq(&existing, owner) {
                return Err(TableHandleError::AlreadyBound);
            }
            return Ok(());
        }
        *binding = Some(Rc::downgrade(owner));
        Ok(())
    }

    fn set_for_instance(
        &self,
        index: u32,
        function_index: u32,
        owner: &Rc<()>,
    ) -> Result<(), TableHandleError> {
        self.bind(owner)?;
        self.set(
            index,
            Some(FunctionRef {
                owner: Rc::downgrade(owner),
                function_index,
            }),
        )
    }

    fn function_index_for_instance(
        &self,
        index: u32,
        owner: &Rc<()>,
    ) -> Result<Option<u32>, TableHandleError> {
        let Some(function) = self.get(index)? else {
            return Ok(None);
        };
        let Some(actual_owner) = function.owner.upgrade() else {
            return Err(TableHandleError::ForeignFunctionReference { index });
        };
        if !Rc::ptr_eq(&actual_owner, owner) {
            return Err(TableHandleError::ForeignFunctionReference { index });
        }
        Ok(Some(function.function_index))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryHandleError {
    InvalidLimits { minimum: u32, maximum: u32 },
    LimitExceeded { pages: u32, limit: u32 },
    AllocationFailed { pages: u32 },
    OutOfBounds { address: u64, width: usize },
}

impl fmt::Display for MemoryHandleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits { minimum, maximum } => write!(
                f,
                "memory minimum {minimum} exceeds declared maximum {maximum}"
            ),
            Self::LimitExceeded { pages, limit } => {
                write!(
                    f,
                    "memory limit {pages} pages exceeds WebAssembly limit {limit}"
                )
            }
            Self::AllocationFailed { pages } => {
                write!(f, "failed to allocate linear memory with {pages} pages")
            }
            Self::OutOfBounds { address, width } => write!(
                f,
                "memory access at byte {address} with width {width} is out of bounds"
            ),
        }
    }
}

impl std::error::Error for MemoryHandleError {}

#[derive(Clone)]
pub struct MemoryHandle {
    memory: Rc<RefCell<LinearMemory>>,
    minimum: u32,
    maximum: Option<u32>,
}

impl fmt::Debug for MemoryHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryHandle")
            .field("minimum", &self.minimum)
            .field("maximum", &self.maximum)
            .field("size_pages", &self.size_pages())
            .finish()
    }
}

impl MemoryHandle {
    pub fn new(minimum: u32, maximum: Option<u32>) -> Result<Self, MemoryHandleError> {
        if let Some(maximum) = maximum {
            if minimum > maximum {
                return Err(MemoryHandleError::InvalidLimits { minimum, maximum });
            }
            if maximum > MAX_MEMORY_PAGES {
                return Err(MemoryHandleError::LimitExceeded {
                    pages: maximum,
                    limit: MAX_MEMORY_PAGES,
                });
            }
        }
        if minimum > MAX_MEMORY_PAGES {
            return Err(MemoryHandleError::LimitExceeded {
                pages: minimum,
                limit: MAX_MEMORY_PAGES,
            });
        }
        let byte_len = pages_to_bytes(minimum)
            .ok_or(MemoryHandleError::AllocationFailed { pages: minimum })?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(byte_len)
            .map_err(|_| MemoryHandleError::AllocationFailed { pages: minimum })?;
        bytes.resize(byte_len, 0);
        let max_pages = maximum.unwrap_or(MAX_MEMORY_PAGES);
        Ok(Self {
            memory: Rc::new(RefCell::new(LinearMemory { bytes, max_pages })),
            minimum,
            maximum,
        })
    }

    pub fn minimum(&self) -> u32 {
        self.minimum
    }

    pub fn maximum(&self) -> Option<u32> {
        self.maximum
    }

    pub fn size_pages(&self) -> u32 {
        self.memory.borrow().size_pages()
    }

    pub fn read(&self, address: u32, length: usize) -> Result<Vec<u8>, MemoryHandleError> {
        let memory = self.memory.borrow();
        let range = memory
            .checked_host_range(address, length)
            .map_err(|error| match error {
                HostError::MemoryOutOfBounds { address, width } => {
                    MemoryHandleError::OutOfBounds { address, width }
                }
                _ => unreachable!("checked_host_range only reports bounds errors"),
            })?;
        Ok(memory.bytes[range].to_vec())
    }

    pub fn write(&self, address: u32, bytes: &[u8]) -> Result<(), MemoryHandleError> {
        let mut memory = self.memory.borrow_mut();
        let range =
            memory
                .checked_host_range(address, bytes.len())
                .map_err(|error| match error {
                    HostError::MemoryOutOfBounds { address, width } => {
                        MemoryHandleError::OutOfBounds { address, width }
                    }
                    _ => unreachable!("checked_host_range only reports bounds errors"),
                })?;
        memory.bytes[range].copy_from_slice(bytes);
        Ok(())
    }

    pub fn grow(&self, delta_pages: u32) -> i32 {
        self.memory.borrow_mut().grow(delta_pages)
    }
}

enum HostMemory<'a> {
    Owned(&'a mut LinearMemory),
    Shared(MemoryHandle),
}

pub struct HostContext<'a> {
    memory: Option<HostMemory<'a>>,
    capabilities: HostCapabilities,
}

impl HostContext<'_> {
    pub fn memory_size_pages(&self) -> Result<u32, HostError> {
        if !self.capabilities.memory_read {
            return Err(HostError::CapabilityDenied("memory.read"));
        }
        match self.memory.as_ref().ok_or(HostError::MemoryUnavailable)? {
            HostMemory::Owned(memory) => Ok(memory.size_pages()),
            HostMemory::Shared(memory) => Ok(memory.size_pages()),
        }
    }

    pub fn read_memory(&self, address: u32, length: usize) -> Result<Vec<u8>, HostError> {
        if !self.capabilities.memory_read {
            return Err(HostError::CapabilityDenied("memory.read"));
        }
        match self.memory.as_ref().ok_or(HostError::MemoryUnavailable)? {
            HostMemory::Owned(memory) => {
                let range = memory.checked_host_range(address, length)?;
                Ok(memory.bytes[range].to_vec())
            }
            HostMemory::Shared(memory) => {
                memory.read(address, length).map_err(|error| match error {
                    MemoryHandleError::OutOfBounds { address, width } => {
                        HostError::MemoryOutOfBounds { address, width }
                    }
                    _ => HostError::MemoryUnavailable,
                })
            }
        }
    }

    pub fn write_memory(&mut self, address: u32, bytes: &[u8]) -> Result<(), HostError> {
        if !self.capabilities.memory_write {
            return Err(HostError::CapabilityDenied("memory.write"));
        }
        match self.memory.as_mut().ok_or(HostError::MemoryUnavailable)? {
            HostMemory::Owned(memory) => {
                let range = memory.checked_host_range(address, bytes.len())?;
                memory.bytes[range].copy_from_slice(bytes);
                Ok(())
            }
            HostMemory::Shared(memory) => {
                memory.write(address, bytes).map_err(|error| match error {
                    MemoryHandleError::OutOfBounds { address, width } => {
                        HostError::MemoryOutOfBounds { address, width }
                    }
                    _ => HostError::MemoryUnavailable,
                })
            }
        }
    }
}

type HostCallback = Box<
    dyn for<'a> FnMut(&mut HostContext<'a>, &[Value]) -> Result<Vec<Value>, HostError> + 'static,
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
    DuplicateTable { module: String, name: String },
    DuplicateMemory { module: String, name: String },
    UnsupportedSignature,
}

impl fmt::Display for HostRegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateFunction { module, name } => {
                write!(f, "host function {module}.{name} is already registered")
            }
            Self::DuplicateGlobal { module, name } => {
                write!(f, "host global {module}.{name} is already registered")
            }
            Self::DuplicateTable { module, name } => {
                write!(f, "host table {module}.{name} is already registered")
            }
            Self::DuplicateMemory { module, name } => {
                write!(f, "host memory {module}.{name} is already registered")
            }
            Self::UnsupportedSignature => write!(
                f,
                "HostRegistry::register supports numeric value types with at most one result; use register_values for multi-result callbacks"
            ),
        }
    }
}

impl std::error::Error for HostRegistryError {}

#[derive(Default)]
pub struct HostRegistry {
    functions: HashMap<(String, String), HostFunction>,
    globals: HashMap<(String, String), GlobalHandle>,
    tables: HashMap<(String, String), TableHandle>,
    memories: HashMap<(String, String), MemoryHandle>,
}

impl fmt::Debug for HostRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostRegistry")
            .field("function_count", &self.functions.len())
            .field("global_count", &self.globals.len())
            .field("table_count", &self.tables.len())
            .field("memory_count", &self.memories.len())
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
        mut callback: F,
    ) -> Result<(), HostRegistryError>
    where
        F: for<'a> FnMut(&mut HostContext<'a>, &[Value]) -> Result<Option<Value>, HostError>
            + 'static,
    {
        if results.len() > 1 {
            return Err(HostRegistryError::UnsupportedSignature);
        }
        self.register_values(
            module,
            name,
            params,
            results,
            capabilities,
            move |context, args| callback(context, args).map(|result| result.into_iter().collect()),
        )
    }

    pub fn register_values<F>(
        &mut self,
        module: impl Into<String>,
        name: impl Into<String>,
        params: Vec<ValueType>,
        results: Vec<ValueType>,
        capabilities: HostCapabilities,
        callback: F,
    ) -> Result<(), HostRegistryError>
    where
        F: for<'a> FnMut(&mut HostContext<'a>, &[Value]) -> Result<Vec<Value>, HostError> + 'static,
    {
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

    pub fn register_table(
        &mut self,
        module: impl Into<String>,
        name: impl Into<String>,
        table: TableHandle,
    ) -> Result<(), HostRegistryError> {
        let module = module.into();
        let name = name.into();
        let key = (module.clone(), name.clone());
        if self.tables.contains_key(&key) {
            return Err(HostRegistryError::DuplicateTable { module, name });
        }
        self.tables.insert(key, table);
        Ok(())
    }

    pub fn register_memory(
        &mut self,
        module: impl Into<String>,
        name: impl Into<String>,
        memory: MemoryHandle,
    ) -> Result<(), HostRegistryError> {
        let module = module.into();
        let name = name.into();
        let key = (module.clone(), name.clone());
        if self.memories.contains_key(&key) {
            return Err(HostRegistryError::DuplicateMemory { module, name });
        }
        self.memories.insert(key, memory);
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
    IntegerDivisionByZero,
    IntegerOverflow,
    InvalidConversionToInteger,
    UnsupportedPrefixedOpcode {
        prefix: u8,
        subopcode: u32,
    },
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
    MultiValueResultRequiresValuesApi {
        results: usize,
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
    UnresolvedTableImport {
        module: String,
        name: String,
    },
    UnresolvedMemoryImport {
        module: String,
        name: String,
    },
    HostMemoryLimitsMismatch {
        module: String,
        name: String,
        expected_minimum: u32,
        expected_maximum: Option<u32>,
        actual_minimum: u32,
        actual_maximum: Option<u32>,
    },
    HostMemoryRuntimeLimitMismatch {
        module: String,
        name: String,
        memory_limit: u32,
        runtime_limit: u32,
    },
    HostTableLimitsMismatch {
        module: String,
        name: String,
        expected_minimum: u32,
        expected_maximum: Option<u32>,
        actual_minimum: u32,
        actual_maximum: Option<u32>,
    },
    HostTableAlreadyBound {
        module: String,
        name: String,
    },
    ForeignTableFunctionReference {
        element_index: u32,
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
    HostResultTypeMismatch {
        module: String,
        name: String,
        expected: ValueType,
        actual: ValueType,
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
            Self::IntegerDivisionByZero => write!(f, "integer division by zero"),
            Self::IntegerOverflow => write!(f, "integer overflow"),
            Self::InvalidConversionToInteger => write!(f, "invalid conversion to integer"),
            Self::UnsupportedPrefixedOpcode { prefix, subopcode } => write!(
                f,
                "unsupported prefixed opcode 0x{prefix:02x}:{subopcode}"
            ),
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
            Self::MultiValueResultRequiresValuesApi { results } => write!(
                f,
                "export returns {results} values; use invoke_export_values for multi-value results"
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
            Self::UnresolvedTableImport { module, name } => {
            write!(f, "unresolved host table import {module}.{name}")
        }
        Self::UnresolvedMemoryImport { module, name } => {
            write!(f, "unresolved host memory import {module}.{name}")
        }
        Self::HostMemoryLimitsMismatch {
            module,
            name,
            expected_minimum,
            expected_maximum,
            actual_minimum,
            actual_maximum,
        } => write!(
            f,
            "host memory {module}.{name} has limits min={actual_minimum} max={actual_maximum:?}, which do not satisfy imported min={expected_minimum} max={expected_maximum:?}"
        ),
        Self::HostMemoryRuntimeLimitMismatch {
            module,
            name,
            memory_limit,
            runtime_limit,
        } => write!(
            f,
            "host memory {module}.{name} can reach {memory_limit} pages, exceeding runtime limit {runtime_limit}"
        ),
        Self::HostTableLimitsMismatch {
                module,
                name,
                expected_minimum,
                expected_maximum,
                actual_minimum,
                actual_maximum,
            } => write!(
                f,
                "host table {module}.{name} has limits min={actual_minimum} max={actual_maximum:?}, which do not satisfy imported min={expected_minimum} max={expected_maximum:?}"
            ),
            Self::HostTableAlreadyBound { module, name } => write!(
                f,
                "host table {module}.{name} is already bound to another live runtime instance"
            ),
            Self::ForeignTableFunctionReference { element_index } => write!(
                f,
                "table element {element_index} refers to a function owned by another runtime instance"
            ),
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
            Self::HostResultTypeMismatch {
                module,
                name,
                expected,
                actual,
            } => write!(
                f,
                "host function {module}.{name} returned {actual:?}, expected {expected:?}"
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

    fn load_i64(&self, address: i32, displacement: u32) -> Result<i64, RuntimeError> {
        let range = self.checked_range(address, displacement, 8)?;
        let bytes: [u8; 8] = self.bytes[range]
            .try_into()
            .expect("checked eight-byte range");
        Ok(i64::from_le_bytes(bytes))
    }

    fn load_f32(&self, address: i32, displacement: u32) -> Result<f32, RuntimeError> {
        let range = self.checked_range(address, displacement, 4)?;
        let bytes: [u8; 4] = self.bytes[range]
            .try_into()
            .expect("checked four-byte range");
        Ok(f32::from_bits(u32::from_le_bytes(bytes)))
    }

    fn load_f64(&self, address: i32, displacement: u32) -> Result<f64, RuntimeError> {
        let range = self.checked_range(address, displacement, 8)?;
        let bytes: [u8; 8] = self.bytes[range]
            .try_into()
            .expect("checked eight-byte range");
        Ok(f64::from_bits(u64::from_le_bytes(bytes)))
    }

    fn load_i64_8_s(&self, address: i32, displacement: u32) -> Result<i64, RuntimeError> {
        let range = self.checked_range(address, displacement, 1)?;
        Ok(i64::from(self.bytes[range.start] as i8))
    }

    fn load_i64_8_u(&self, address: i32, displacement: u32) -> Result<i64, RuntimeError> {
        let range = self.checked_range(address, displacement, 1)?;
        Ok(i64::from(self.bytes[range.start]))
    }

    fn load_i64_16_s(&self, address: i32, displacement: u32) -> Result<i64, RuntimeError> {
        let range = self.checked_range(address, displacement, 2)?;
        let bytes: [u8; 2] = self.bytes[range]
            .try_into()
            .expect("checked two-byte range");
        Ok(i64::from(i16::from_le_bytes(bytes)))
    }

    fn load_i64_16_u(&self, address: i32, displacement: u32) -> Result<i64, RuntimeError> {
        let range = self.checked_range(address, displacement, 2)?;
        let bytes: [u8; 2] = self.bytes[range]
            .try_into()
            .expect("checked two-byte range");
        Ok(i64::from(u16::from_le_bytes(bytes)))
    }

    fn load_i64_32_s(&self, address: i32, displacement: u32) -> Result<i64, RuntimeError> {
        let range = self.checked_range(address, displacement, 4)?;
        let bytes: [u8; 4] = self.bytes[range]
            .try_into()
            .expect("checked four-byte range");
        Ok(i64::from(i32::from_le_bytes(bytes)))
    }

    fn load_i64_32_u(&self, address: i32, displacement: u32) -> Result<i64, RuntimeError> {
        let range = self.checked_range(address, displacement, 4)?;
        let bytes: [u8; 4] = self.bytes[range]
            .try_into()
            .expect("checked four-byte range");
        Ok(i64::from(u32::from_le_bytes(bytes)))
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
    fn store_i64(
        &mut self,
        address: i32,
        displacement: u32,
        value: i64,
    ) -> Result<(), RuntimeError> {
        let range = self.checked_range(address, displacement, 8)?;
        self.bytes[range].copy_from_slice(&value.to_le_bytes());
        Ok(())
    }

    fn store_f32(
        &mut self,
        address: i32,
        displacement: u32,
        value: f32,
    ) -> Result<(), RuntimeError> {
        let range = self.checked_range(address, displacement, 4)?;
        self.bytes[range].copy_from_slice(&value.to_bits().to_le_bytes());
        Ok(())
    }

    fn store_f64(
        &mut self,
        address: i32,
        displacement: u32,
        value: f64,
    ) -> Result<(), RuntimeError> {
        let range = self.checked_range(address, displacement, 8)?;
        self.bytes[range].copy_from_slice(&value.to_bits().to_le_bytes());
        Ok(())
    }

    fn store_i64_8(
        &mut self,
        address: i32,
        displacement: u32,
        value: i64,
    ) -> Result<(), RuntimeError> {
        let range = self.checked_range(address, displacement, 1)?;
        self.bytes[range.start] = value as u8;
        Ok(())
    }

    fn store_i64_16(
        &mut self,
        address: i32,
        displacement: u32,
        value: i64,
    ) -> Result<(), RuntimeError> {
        let range = self.checked_range(address, displacement, 2)?;
        self.bytes[range].copy_from_slice(&(value as u16).to_le_bytes());
        Ok(())
    }

    fn store_i64_32(
        &mut self,
        address: i32,
        displacement: u32,
        value: i64,
    ) -> Result<(), RuntimeError> {
        let range = self.checked_range(address, displacement, 4)?;
        self.bytes[range].copy_from_slice(&(value as u32).to_le_bytes());
        Ok(())
    }
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
    results: Vec<ValueType>,
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
    result_types: Vec<ValueType>,
}

impl ExecControlFrame {
    fn label_types(&self) -> Vec<ValueType> {
        if self.kind == ControlKind::Loop {
            self.param_types.clone()
        } else {
            self.result_types.clone()
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
    identity: Rc<()>,
    module: Module,
    control_maps: Vec<ControlMap>,
    memory: Option<LinearMemory>,
    imported_memory: Option<MemoryHandle>,
    table: Option<TableHandle>,
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
        validate_host_bindings(&module, &hosts, limits)?;
        let control_maps = module
            .code
            .iter()
            .map(|body| build_control_map(&module, &body.code))
            .collect::<Result<Vec<_>, _>>()?;
        let imported_memory = instantiate_imported_memory(&module, &hosts, limits)?;
        let memory = if imported_memory.is_none() {
            module
                .memories
                .first()
                .map(|memory_type| {
                    LinearMemory::new(
                        memory_type.limits.min,
                        memory_type.limits.max,
                        limits.max_memory_pages,
                    )
                })
                .transpose()?
        } else {
            None
        };
        let identity = Rc::new(());
        let table = instantiate_table(&module, &hosts, &identity)?;
        let globals = instantiate_globals(&module, &hosts)?;

        let mut instance = Self {
            identity,
            module,
            control_maps,
            memory,
            imported_memory,
            table,
            globals,
            hosts,
            limits,
        };
        instance.initialize_data_segments()?;
        instance.initialize_element_segments()?;
        if let Some(start) = instance.module.start {
            let mut budget = ExecutionBudget::new(instance.limits);
            let results = instance.invoke_function(start, &[], 0, &mut budget)?;
            if !results.is_empty() {
                return Err(RuntimeError::ControlInvariant(
                    "validated start function returned values",
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
        let function_index = self.exported_function_index(name)?;
        let result_count = self.function_type(function_index)?.results.len();
        if result_count > 1 {
            return Err(RuntimeError::MultiValueResultRequiresValuesApi {
                results: result_count,
            });
        }
        let mut budget = ExecutionBudget::new(self.limits);
        let mut results = self.invoke_function(function_index, args, 0, &mut budget)?;
        Ok(results.pop())
    }

    pub fn invoke_export_values(
        &mut self,
        name: &str,
        args: &[Value],
    ) -> Result<Vec<Value>, RuntimeError> {
        let function_index = self.exported_function_index(name)?;
        let mut budget = ExecutionBudget::new(self.limits);
        self.invoke_function(function_index, args, 0, &mut budget)
    }

    fn exported_function_index(&self, name: &str) -> Result<u32, RuntimeError> {
        let export = self
            .module
            .exports
            .iter()
            .find(|export| export.name == name)
            .ok_or_else(|| RuntimeError::ExportNotFound(name.to_owned()))?;
        if export.kind != ExportKind::Function {
            return Err(RuntimeError::ExportNotFunction(name.to_owned()));
        }
        Ok(export.index)
    }

    pub fn memory(&self) -> Option<&LinearMemory> {
        self.memory.as_ref()
    }

    pub fn global(&self, index: u32) -> Option<Value> {
        self.globals.get(index as usize).map(GlobalHandle::get)
    }

    fn initialize_data_segments(&mut self) -> Result<(), RuntimeError> {
        let data = self.module.data.clone();

        // Preflight every active segment before mutating a potentially host-shared memory.
        for (segment_index, segment) in data.iter().enumerate() {
            let DataMode::Active {
                memory_index,
                offset,
            } = segment.mode
            else {
                continue;
            };
            if memory_index != 0 {
                return Err(RuntimeError::MemoryIndexOutOfBounds(memory_index));
            }
            let offset = u64::from(offset as u32);
            let end = offset.checked_add(segment.bytes.len() as u64).ok_or(
                RuntimeError::DataSegmentOutOfBounds {
                    segment: segment_index,
                    offset,
                    length: segment.bytes.len(),
                },
            )?;
            let memory_len = self.with_memory(|memory| Ok(memory.bytes.len() as u64))?;
            if end > memory_len {
                return Err(RuntimeError::DataSegmentOutOfBounds {
                    segment: segment_index,
                    offset,
                    length: segment.bytes.len(),
                });
            }
        }

        for segment in &data {
            let DataMode::Active { offset, .. } = segment.mode else {
                continue;
            };
            let offset = u64::from(offset as u32);
            self.with_memory_mut(|memory| {
                let start = usize::try_from(offset).map_err(|_| {
                    RuntimeError::ControlInvariant("preflighted data offset no longer fits usize")
                })?;
                let end = start + segment.bytes.len();
                memory.bytes[start..end].copy_from_slice(&segment.bytes);
                Ok(())
            })?;
        }
        Ok(())
    }

    fn initialize_element_segments(&mut self) -> Result<(), RuntimeError> {
        let elements = self.module.elements.clone();

        // Preflight every active segment before mutating a potentially host-shared table.
        // A later OOB segment must not leave earlier segment writes externally visible.
        for (segment_index, segment) in elements.iter().enumerate() {
            let ElementMode::Active {
                table_index,
                offset,
            } = segment.mode
            else {
                continue;
            };
            if table_index != 0 {
                return Err(RuntimeError::TableIndexOutOfBounds(table_index));
            }
            let offset = u64::from(offset as u32);
            let end = offset
                .checked_add(segment.function_indices.len() as u64)
                .ok_or(RuntimeError::ElementSegmentOutOfBounds {
                    segment: segment_index,
                    offset,
                    length: segment.function_indices.len(),
                })?;
            let table = self
                .table
                .as_ref()
                .ok_or(RuntimeError::TableIndexOutOfBounds(0))?;
            if end > u64::from(table.len()) {
                return Err(RuntimeError::ElementSegmentOutOfBounds {
                    segment: segment_index,
                    offset,
                    length: segment.function_indices.len(),
                });
            }
        }

        for segment in &elements {
            let ElementMode::Active { offset, .. } = segment.mode else {
                continue;
            };
            let offset = u64::from(offset as u32);
            let table = self
                .table
                .as_ref()
                .ok_or(RuntimeError::TableIndexOutOfBounds(0))?;
            for (slot, &function_index) in segment.function_indices.iter().enumerate() {
                let index = u32::try_from(offset + slot as u64).map_err(|_| {
                    RuntimeError::ControlInvariant(
                        "preflighted element segment index no longer fits u32",
                    )
                })?;
                table
                    .set_for_instance(index, function_index, &self.identity)
                    .map_err(|error| map_table_element_error(error, index))?;
            }
        }
        Ok(())
    }

    fn with_memory<R>(
        &self,
        f: impl FnOnce(&LinearMemory) -> Result<R, RuntimeError>,
    ) -> Result<R, RuntimeError> {
        if let Some(memory) = self.memory.as_ref() {
            return f(memory);
        }
        if let Some(memory) = self.imported_memory.as_ref() {
            let memory = memory.memory.borrow();
            return f(&memory);
        }
        Err(RuntimeError::MemoryUnavailable)
    }

    fn with_memory_mut<R>(
        &mut self,
        f: impl FnOnce(&mut LinearMemory) -> Result<R, RuntimeError>,
    ) -> Result<R, RuntimeError> {
        if let Some(memory) = self.memory.as_mut() {
            return f(memory);
        }
        if let Some(memory) = self.imported_memory.as_ref() {
            let mut memory = memory.memory.borrow_mut();
            return f(&mut memory);
        }
        Err(RuntimeError::MemoryUnavailable)
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
    ) -> Result<Vec<Value>, RuntimeError> {
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
        let (hosts, memory, imported_memory) =
            (&mut self.hosts, &mut self.memory, &self.imported_memory);
        let host = hosts
            .functions
            .get_mut(&key)
            .ok_or_else(|| RuntimeError::UnresolvedImport {
                module: import.module.clone(),
                name: import.name.clone(),
            })?;
        let context_memory = if let Some(shared) = imported_memory.as_ref() {
            Some(HostMemory::Shared(shared.clone()))
        } else {
            memory.as_mut().map(HostMemory::Owned)
        };
        let mut context = HostContext {
            memory: context_memory,
            capabilities: host.capabilities,
        };
        let result =
            (host.callback)(&mut context, args).map_err(|error| RuntimeError::HostCallFailed {
                module: import.module.clone(),
                name: import.name.clone(),
                error,
            })?;
        let actual = result.len();
        if actual != ty.results.len() {
            return Err(RuntimeError::HostResultArityMismatch {
                module: import.module,
                name: import.name,
                expected: ty.results.len(),
                actual,
            });
        }
        for (&expected, &value) in ty.results.iter().zip(&result) {
            let actual = value.value_type();
            if actual != expected {
                return Err(RuntimeError::HostResultTypeMismatch {
                    module: import.module.clone(),
                    name: import.name.clone(),
                    expected,
                    actual,
                });
            }
        }
        Ok(result)
    }

    fn invoke_function(
        &mut self,
        function_index: u32,
        args: &[Value],
        depth: usize,
        budget: &mut ExecutionBudget,
    ) -> Result<Vec<Value>, RuntimeError> {
        let function = function_index as usize;
        let imported = self.module.function_import_count();
        if function < imported {
            return self.invoke_host(function, args, budget);
        }
        let call_depth_limit = self.limits.max_call_depth.min(MAX_CALL_DEPTH);
        if depth >= call_depth_limit {
            return Err(RuntimeError::CallDepthExceeded {
                limit: call_depth_limit,
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
        let result_types = ty.results.clone();
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
            result_types: result_types.clone(),
        }];

        while pc < code.len() {
            budget.consume_instruction()?;
            let offset = pc;
            let opcode = code[pc];
            pc += 1;

            match opcode {
                0x01 => {}
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
                        result_types: signature.results,
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
                        result_types: signature.results,
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
                0x0e => {
                    let target_count = read_u32_immediate(code, &mut pc)?;
                    let selector = numeric::i32_from_stack(&mut stack)? as u32;
                    let mut selected_depth = None;
                    for target_index in 0..target_count {
                        let depth = read_u32_immediate(code, &mut pc)?;
                        if target_index == selector {
                            selected_depth = Some(depth);
                        }
                    }
                    let default_depth = read_u32_immediate(code, &mut pc)?;
                    let branch_depth = selected_depth.unwrap_or(default_depth);
                    branch_to(&mut controls, &mut stack, branch_depth, &mut pc, code.len())?;
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
                    let results = self.invoke_function(callee, &call_args, depth + 1, budget)?;
                    stack.extend(results);
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
                        .ok_or(RuntimeError::TableIndexOutOfBounds(table_index))?
                        .function_index_for_instance(element_index, &self.identity)
                        .map_err(|error| map_table_element_error(error, element_index))?
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
                    let results = self.invoke_function(callee, &call_args, depth + 1, budget)?;
                    stack.extend(results);
                }
                0x1a => {
                    let _ = stack.pop().ok_or(RuntimeError::StackUnderflow)?;
                }
                0x1b => {
                    let condition = numeric::i32_from_stack(&mut stack)?;
                    let second = stack.pop().ok_or(RuntimeError::StackUnderflow)?;
                    let first = stack.pop().ok_or(RuntimeError::StackUnderflow)?;
                    let expected = first.value_type();
                    let actual = second.value_type();
                    if actual != expected {
                        return Err(RuntimeError::ValueTypeMismatch { expected, actual });
                    }
                    stack.push(if condition != 0 { first } else { second });
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
                0x28..=0x35 => {
                    let (_, displacement) = read_memarg(code, &mut pc)?;
                    let address = numeric::i32_from_stack(&mut stack)?;
                    let value = match opcode {
                        0x28 => Value::I32(
                            self.with_memory(|memory| memory.load_i32(address, displacement))?,
                        ),
                        0x29 => Value::I64(
                            self.with_memory(|memory| memory.load_i64(address, displacement))?,
                        ),
                        0x2a => Value::F32(
                            self.with_memory(|memory| memory.load_f32(address, displacement))?,
                        ),
                        0x2b => Value::F64(
                            self.with_memory(|memory| memory.load_f64(address, displacement))?,
                        ),
                        0x2c => Value::I32(
                            self.with_memory(|memory| memory.load_i8_s(address, displacement))?,
                        ),
                        0x2d => Value::I32(
                            self.with_memory(|memory| memory.load_i8_u(address, displacement))?,
                        ),
                        0x2e => Value::I32(
                            self.with_memory(|memory| memory.load_i16_s(address, displacement))?,
                        ),
                        0x2f => Value::I32(
                            self.with_memory(|memory| memory.load_i16_u(address, displacement))?,
                        ),
                        0x30 => Value::I64(
                            self.with_memory(|memory| memory.load_i64_8_s(address, displacement))?,
                        ),
                        0x31 => Value::I64(
                            self.with_memory(|memory| memory.load_i64_8_u(address, displacement))?,
                        ),
                        0x32 => {
                            Value::I64(self.with_memory(|memory| {
                                memory.load_i64_16_s(address, displacement)
                            })?)
                        }
                        0x33 => {
                            Value::I64(self.with_memory(|memory| {
                                memory.load_i64_16_u(address, displacement)
                            })?)
                        }
                        0x34 => {
                            Value::I64(self.with_memory(|memory| {
                                memory.load_i64_32_s(address, displacement)
                            })?)
                        }
                        0x35 => {
                            Value::I64(self.with_memory(|memory| {
                                memory.load_i64_32_u(address, displacement)
                            })?)
                        }
                        _ => unreachable!(),
                    };
                    stack.push(value);
                }
                0x36..=0x3e => {
                    let (_, displacement) = read_memarg(code, &mut pc)?;
                    match opcode {
                        0x36 | 0x3a | 0x3b => {
                            let value = numeric::i32_from_stack(&mut stack)?;
                            let address = numeric::i32_from_stack(&mut stack)?;
                            match opcode {
                                0x36 => self.with_memory_mut(|memory| {
                                    memory.store_i32(address, displacement, value)
                                })?,
                                0x3a => self.with_memory_mut(|memory| {
                                    memory.store_i8(address, displacement, value)
                                })?,
                                0x3b => self.with_memory_mut(|memory| {
                                    memory.store_i16(address, displacement, value)
                                })?,
                                _ => unreachable!(),
                            }
                        }
                        0x37 | 0x3c..=0x3e => {
                            let value = match numeric::pop_typed(&mut stack, ValueType::I64)? {
                                Value::I64(value) => value,
                                _ => unreachable!("pop_typed established i64"),
                            };
                            let address = numeric::i32_from_stack(&mut stack)?;
                            match opcode {
                                0x37 => self.with_memory_mut(|memory| {
                                    memory.store_i64(address, displacement, value)
                                })?,
                                0x3c => self.with_memory_mut(|memory| {
                                    memory.store_i64_8(address, displacement, value)
                                })?,
                                0x3d => self.with_memory_mut(|memory| {
                                    memory.store_i64_16(address, displacement, value)
                                })?,
                                0x3e => self.with_memory_mut(|memory| {
                                    memory.store_i64_32(address, displacement, value)
                                })?,
                                _ => unreachable!(),
                            }
                        }
                        0x38 => {
                            let value = match numeric::pop_typed(&mut stack, ValueType::F32)? {
                                Value::F32(value) => value,
                                _ => unreachable!("pop_typed established f32"),
                            };
                            let address = numeric::i32_from_stack(&mut stack)?;
                            self.with_memory_mut(|memory| {
                                memory.store_f32(address, displacement, value)
                            })?;
                        }
                        0x39 => {
                            let value = match numeric::pop_typed(&mut stack, ValueType::F64)? {
                                Value::F64(value) => value,
                                _ => unreachable!("pop_typed established f64"),
                            };
                            let address = numeric::i32_from_stack(&mut stack)?;
                            self.with_memory_mut(|memory| {
                                memory.store_f64(address, displacement, value)
                            })?;
                        }
                        _ => unreachable!(),
                    }
                }
                0x3f => {
                    let memory_index = read_u32_immediate(code, &mut pc)?;
                    ensure_runtime_memory_index(self, memory_index)?;
                    stack.push(Value::I32(
                        self.with_memory(|memory| Ok(memory.size_pages()))? as i32,
                    ));
                }
                0x40 => {
                    let memory_index = read_u32_immediate(code, &mut pc)?;
                    ensure_runtime_memory_index(self, memory_index)?;
                    let delta = numeric::i32_from_stack(&mut stack)? as u32;
                    let previous = self.with_memory_mut(|memory| Ok(memory.grow(delta)))?;
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
                0x67..=0x69 => numeric::unary_integer(&mut stack, opcode)?,
                0x6a..=0x78 => numeric::binary_integer(&mut stack, opcode)?,
                0x79..=0x7b => numeric::unary_integer(&mut stack, opcode)?,
                0x7c..=0x8a => numeric::binary_integer(&mut stack, opcode)?,
                0x8b..=0x91 | 0x99..=0x9f => numeric::unary_float(&mut stack, opcode)?,
                0x92..=0x98 | 0xa0..=0xa6 => numeric::binary_float(&mut stack, opcode)?,
                0xa7..=0xbf => numeric::convert(&mut stack, opcode)?,
                0xfc => {
                    let subopcode = read_u32_immediate(code, &mut pc)?;
                    numeric::trunc_sat(&mut stack, subopcode)?;
                }
                other => return Err(RuntimeError::UnsupportedOpcode(other)),
            }
        }

        let result_arity = result_types.len();
        if stack.len() != result_arity {
            return Err(RuntimeError::ResultArityMismatch {
                expected: result_arity,
                actual: stack.len(),
            });
        }
        validate_values(&result_types, &stack)?;
        Ok(stack)
    }
}

fn validate_host_bindings(
    module: &Module,
    hosts: &HostRegistry,
    limits: RuntimeLimits,
) -> Result<(), RuntimeError> {
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
        let ImportDesc::Table(table_type) = import.desc else {
            continue;
        };
        let key = (import.module.clone(), import.name.clone());
        let table = hosts
            .tables
            .get(&key)
            .ok_or_else(|| RuntimeError::UnresolvedTableImport {
                module: import.module.clone(),
                name: import.name.clone(),
            })?;
        validate_table_limits(import, table_type.limits.min, table_type.limits.max, table)?;
    }

    for import in &module.imports {
        let ImportDesc::Memory(memory_type) = import.desc else {
            continue;
        };
        let key = (import.module.clone(), import.name.clone());
        let memory =
            hosts
                .memories
                .get(&key)
                .ok_or_else(|| RuntimeError::UnresolvedMemoryImport {
                    module: import.module.clone(),
                    name: import.name.clone(),
                })?;
        validate_memory_limits(
            import,
            memory_type.limits.min,
            memory_type.limits.max,
            memory,
        )?;
        validate_memory_runtime_limit(import, memory, limits.max_memory_pages)?;
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

fn validate_memory_limits(
    import: &wasm_parser::Import,
    expected_minimum: u32,
    expected_maximum: Option<u32>,
    memory: &MemoryHandle,
) -> Result<(), RuntimeError> {
    let actual_minimum = memory.minimum();
    let actual_maximum = memory.maximum();
    let minimum_matches = actual_minimum >= expected_minimum;
    let maximum_matches = match expected_maximum {
        None => true,
        Some(expected) => matches!(actual_maximum, Some(actual) if actual <= expected),
    };
    if minimum_matches && maximum_matches {
        return Ok(());
    }
    Err(RuntimeError::HostMemoryLimitsMismatch {
        module: import.module.clone(),
        name: import.name.clone(),
        expected_minimum,
        expected_maximum,
        actual_minimum,
        actual_maximum,
    })
}

fn validate_memory_runtime_limit(
    import: &wasm_parser::Import,
    memory: &MemoryHandle,
    runtime_limit: u32,
) -> Result<(), RuntimeError> {
    let runtime_limit = runtime_limit.min(MAX_MEMORY_PAGES);
    let memory_limit = memory.maximum().unwrap_or(MAX_MEMORY_PAGES);
    if memory.size_pages() <= runtime_limit && memory_limit <= runtime_limit {
        return Ok(());
    }
    Err(RuntimeError::HostMemoryRuntimeLimitMismatch {
        module: import.module.clone(),
        name: import.name.clone(),
        memory_limit,
        runtime_limit,
    })
}

fn instantiate_imported_memory(
    module: &Module,
    hosts: &HostRegistry,
    limits: RuntimeLimits,
) -> Result<Option<MemoryHandle>, RuntimeError> {
    for import in &module.imports {
        let ImportDesc::Memory(memory_type) = import.desc else {
            continue;
        };
        let key = (import.module.clone(), import.name.clone());
        let memory = hosts.memories.get(&key).cloned().ok_or_else(|| {
            RuntimeError::UnresolvedMemoryImport {
                module: import.module.clone(),
                name: import.name.clone(),
            }
        })?;
        validate_memory_limits(
            import,
            memory_type.limits.min,
            memory_type.limits.max,
            &memory,
        )?;
        validate_memory_runtime_limit(import, &memory, limits.max_memory_pages)?;
        return Ok(Some(memory));
    }
    Ok(None)
}

fn validate_table_limits(
    import: &wasm_parser::Import,
    expected_minimum: u32,
    expected_maximum: Option<u32>,
    table: &TableHandle,
) -> Result<(), RuntimeError> {
    let actual_minimum = table.len();
    let actual_maximum = table.maximum();
    let minimum_matches = actual_minimum >= expected_minimum;
    let maximum_matches = match expected_maximum {
        None => true,
        Some(expected) => matches!(actual_maximum, Some(actual) if actual <= expected),
    };
    if minimum_matches && maximum_matches {
        return Ok(());
    }
    Err(RuntimeError::HostTableLimitsMismatch {
        module: import.module.clone(),
        name: import.name.clone(),
        expected_minimum,
        expected_maximum,
        actual_minimum,
        actual_maximum,
    })
}

fn instantiate_table(
    module: &Module,
    hosts: &HostRegistry,
    identity: &Rc<()>,
) -> Result<Option<TableHandle>, RuntimeError> {
    for import in &module.imports {
        let ImportDesc::Table(table_type) = import.desc else {
            continue;
        };
        let key = (import.module.clone(), import.name.clone());
        let table =
            hosts
                .tables
                .get(&key)
                .cloned()
                .ok_or_else(|| RuntimeError::UnresolvedTableImport {
                    module: import.module.clone(),
                    name: import.name.clone(),
                })?;
        validate_table_limits(import, table_type.limits.min, table_type.limits.max, &table)?;
        table.bind(identity).map_err(|error| match error {
            TableHandleError::AlreadyBound => RuntimeError::HostTableAlreadyBound {
                module: import.module.clone(),
                name: import.name.clone(),
            },
            other => map_table_element_error(other, 0),
        })?;
        return Ok(Some(table));
    }

    let Some(table_type) = module.tables.first() else {
        return Ok(None);
    };
    let table = TableHandle::new(table_type.limits.min, table_type.limits.max).map_err(
        |error| match error {
            TableHandleError::AllocationFailed { elements } => {
                RuntimeError::TableAllocationFailed { elements }
            }
            TableHandleError::InvalidLimits { .. } => {
                RuntimeError::ControlInvariant("validated defined table has inconsistent limits")
            }
            other => map_table_element_error(other, 0),
        },
    )?;
    table.bind(identity).map_err(|_| {
        RuntimeError::ControlInvariant("fresh defined table is unexpectedly already bound")
    })?;
    Ok(Some(table))
}

fn map_table_element_error(error: TableHandleError, index: u32) -> RuntimeError {
    match error {
        TableHandleError::OutOfBounds { .. } => RuntimeError::TableElementOutOfBounds(index),
        TableHandleError::ForeignFunctionReference { .. } => {
            RuntimeError::ForeignTableFunctionReference {
                element_index: index,
            }
        }
        TableHandleError::AlreadyBound => {
            RuntimeError::ControlInvariant("table binding changed while instance is live")
        }
        TableHandleError::AllocationFailed { elements } => {
            RuntimeError::TableAllocationFailed { elements }
        }
        TableHandleError::InvalidLimits { .. } => {
            RuntimeError::ControlInvariant("table handle has inconsistent limits")
        }
    }
}

fn instantiate_globals(
    module: &Module,
    hosts: &HostRegistry,
) -> Result<Vec<GlobalHandle>, RuntimeError> {
    let mut globals = Vec::with_capacity(module.global_count());
    for import in &module.imports {
        // Preserve imported globals in WebAssembly global-index order.
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
    if index != 0 || (instance.memory.is_none() && instance.imported_memory.is_none()) {
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
    let expected = frame.stack_height + frame.result_types.len();
    if stack.len() != expected {
        return Err(RuntimeError::ControlStackMismatch {
            expected,
            actual: stack.len(),
        });
    }
    validate_values(&frame.result_types, &stack[frame.stack_height..])?;
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
            0x0e => {
                let target_count = read_u32_immediate(code, &mut pc)?;
                for _ in 0..target_count {
                    let _ = read_u32_immediate(code, &mut pc)?;
                }
                let _ = read_u32_immediate(code, &mut pc)?;
            }
            0x11 => {
                let _ = read_u32_immediate(code, &mut pc)?;
                let _ = read_u32_immediate(code, &mut pc)?;
            }
            0x28..=0x3e => {
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
            0x01 | 0x0f | 0x1a | 0x1b | 0x45..=0x66 | 0x67..=0x8a | 0x8b..=0xa6 | 0xa7..=0xbf => {}
            0xfc => {
                let subopcode = read_u32_immediate(code, &mut pc)?;
                if subopcode > 7 {
                    return Err(RuntimeError::UnsupportedPrefixedOpcode {
                        prefix: 0xfc,
                        subopcode,
                    });
                }
            }
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
                results: Vec::new(),
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
            results: vec![result],
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
    Ok(BlockSignature {
        params: ty.params.clone(),
        results: ty.results.clone(),
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
        module.data[0].mode = DataMode::Active {
            memory_index: 0,
            offset: (WASM_PAGE_SIZE - 2) as i32,
        };
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
    fn unsupported_typed_select_is_rejected_before_execution() {
        let bytes = module_with_body(0, 1, &[0x1c, 0x0b]);
        let module = parse_module(&bytes).expect("parse test module");
        let error = Instance::new(module).expect_err("unsupported opcode must fail validation");
        assert!(matches!(
            error,
            RuntimeError::Validation(ValidationError::UnsupportedOpcode { opcode: 0x1c, .. })
        ));
    }
}
