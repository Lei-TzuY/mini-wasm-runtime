//! Minimal, fail-closed WebAssembly binary parser.

use std::fmt;

const MAGIC: [u8; 4] = [0x00, 0x61, 0x73, 0x6d];
const VERSION_1: [u8; 4] = [0x01, 0x00, 0x00, 0x00];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    UnexpectedEof,
    InvalidMagic([u8; 4]),
    UnsupportedVersion([u8; 4]),
    InvalidLeb128,
    Leb128Overflow,
    UnsupportedSection(u8),
    SectionOutOfOrder {
        previous: u8,
        current: u8,
    },
    DuplicateSection(u8),
    SectionLengthMismatch(u8),
    InvalidFunctionType(u8),
    UnsupportedValueType(u8),
    InvalidUtf8,
    InvalidImportKind(u8),
    InvalidExportKind(u8),
    InvalidLimitsFlags(u8),
    InvalidReferenceType(u8),
    InvalidMutability(u8),
    UnsupportedElementSegmentMode(u32),
    UnsupportedDataSegmentMode(u32),
    InvalidConstExprOpcode(u8),
    ConstExprMissingEnd,
    ConstExprTypeMismatch {
        expected: ValueType,
        actual: ValueType,
    },
    FunctionBodyMissingEnd,
    TrailingBytes,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof => write!(f, "unexpected end of input"),
            Self::InvalidMagic(got) => write!(f, "invalid WebAssembly magic: {got:02x?}"),
            Self::UnsupportedVersion(got) => {
                write!(f, "unsupported WebAssembly version: {got:02x?}")
            }
            Self::InvalidLeb128 => write!(f, "invalid LEB128 encoding"),
            Self::Leb128Overflow => write!(f, "LEB128 value exceeds 32 bits"),
            Self::UnsupportedSection(id) => write!(f, "unsupported section id {id}"),
            Self::SectionOutOfOrder { previous, current } => {
                write!(f, "section {current} appears after section {previous}")
            }
            Self::DuplicateSection(id) => write!(f, "duplicate section id {id}"),
            Self::SectionLengthMismatch(id) => {
                write!(f, "section {id} did not consume its declared payload")
            }
            Self::InvalidFunctionType(tag) => write!(f, "invalid function type tag 0x{tag:02x}"),
            Self::UnsupportedValueType(tag) => write!(f, "unsupported value type 0x{tag:02x}"),
            Self::InvalidUtf8 => write!(f, "name is not valid UTF-8"),
            Self::InvalidImportKind(kind) => write!(
                f,
                "unsupported import kind {kind}; this milestone supports function imports only"
            ),
            Self::InvalidExportKind(kind) => write!(f, "invalid export kind {kind}"),
            Self::InvalidLimitsFlags(flags) => {
                write!(f, "invalid limits flags 0x{flags:02x}")
            }
            Self::InvalidReferenceType(tag) => {
                write!(f, "unsupported table reference type 0x{tag:02x}")
            }
            Self::InvalidMutability(value) => write!(f, "invalid global mutability byte {value}"),
            Self::UnsupportedElementSegmentMode(mode) => {
                write!(f, "unsupported element segment mode {mode}")
            }
            Self::UnsupportedDataSegmentMode(mode) => {
                write!(f, "unsupported data segment mode {mode}")
            }
            Self::InvalidConstExprOpcode(opcode) => {
                write!(f, "unsupported constant-expression opcode 0x{opcode:02x}")
            }
            Self::ConstExprMissingEnd => write!(f, "constant expression is missing end opcode"),
            Self::ConstExprTypeMismatch { expected, actual } => write!(
                f,
                "constant expression has type {actual:?}, expected {expected:?}"
            ),
            Self::FunctionBodyMissingEnd => write!(f, "function body is missing final end opcode"),
            Self::TrailingBytes => write!(f, "trailing bytes after parsed value"),
        }
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
    I32,
    I64,
    F32,
    F64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Constant {
    I32(i32),
    I64(i64),
    F32(u32),
    F64(u64),
}

impl Constant {
    pub fn value_type(self) -> ValueType {
        match self {
            Self::I32(_) => ValueType::I32,
            Self::I64(_) => ValueType::I64,
            Self::F32(_) => ValueType::F32,
            Self::F64(_) => ValueType::F64,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuncType {
    pub params: Vec<ValueType>,
    pub results: Vec<ValueType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    pub module: String,
    pub name: String,
    pub type_index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub min: u32,
    pub max: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableType {
    pub limits: Limits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryType {
    pub limits: Limits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlobalType {
    pub value_type: ValueType,
    pub mutable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Global {
    pub ty: GlobalType,
    pub init: Constant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportKind {
    Function,
    Table,
    Memory,
    Global,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Export {
    pub name: String,
    pub kind: ExportKind,
    pub index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementSegment {
    pub table_index: u32,
    pub offset: i32,
    pub function_indices: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionBody {
    /// Local declarations encoded as (count, type) groups.
    pub locals: Vec<(u32, ValueType)>,
    /// Raw instruction bytes, including the final `end` (0x0b).
    pub code: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataSegment {
    /// Active segments currently target memory 0 only.
    pub memory_index: u32,
    /// Constant i32 byte offset evaluated during instantiation.
    pub offset: i32,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Module {
    pub types: Vec<FuncType>,
    /// Function imports occupy the first entries in the module function index space.
    pub imports: Vec<Import>,
    /// Type indices for defined (non-imported) functions only.
    pub function_type_indices: Vec<u32>,
    pub tables: Vec<TableType>,
    pub memories: Vec<MemoryType>,
    pub globals: Vec<Global>,
    pub exports: Vec<Export>,
    pub start: Option<u32>,
    pub elements: Vec<ElementSegment>,
    pub code: Vec<FunctionBody>,
    pub data: Vec<DataSegment>,
}

/// Decode a canonical-or-noncanonical unsigned LEB128 u32 value.
pub fn decode_u32(input: &[u8]) -> Result<(u32, usize), ParseError> {
    let mut result = 0u32;
    for index in 0..5 {
        let byte = *input.get(index).ok_or(ParseError::UnexpectedEof)?;
        let payload = u32::from(byte & 0x7f);
        if index == 4 && payload > 0x0f {
            return Err(ParseError::Leb128Overflow);
        }
        result |= payload << (index * 7);
        if byte & 0x80 == 0 {
            return Ok((result, index + 1));
        }
    }
    Err(ParseError::InvalidLeb128)
}

/// Decode a signed LEB128 i32 value.
pub fn decode_i32(input: &[u8]) -> Result<(i32, usize), ParseError> {
    let mut result = 0i64;
    let mut shift = 0u32;
    for index in 0..5 {
        let byte = *input.get(index).ok_or(ParseError::UnexpectedEof)?;
        let payload = i64::from(byte & 0x7f);
        result |= payload << shift;
        shift += 7;
        if byte & 0x80 == 0 {
            if index == 4 {
                let terminal = byte & 0x70;
                if terminal != 0x00 && terminal != 0x70 {
                    return Err(ParseError::Leb128Overflow);
                }
            }
            if byte & 0x40 != 0 && shift < 64 {
                result |= (!0i64) << shift;
            }
            return i32::try_from(result)
                .map(|value| (value, index + 1))
                .map_err(|_| ParseError::Leb128Overflow);
        }
    }
    Err(ParseError::InvalidLeb128)
}

/// Decode a signed LEB128 i64 value.
pub fn decode_i64(input: &[u8]) -> Result<(i64, usize), ParseError> {
    let mut result = 0i128;
    let mut shift = 0u32;
    for index in 0..10 {
        let byte = *input.get(index).ok_or(ParseError::UnexpectedEof)?;
        let payload = i128::from(byte & 0x7f);
        result |= payload << shift;
        shift += 7;
        if byte & 0x80 == 0 {
            if index == 9 {
                let terminal = byte & 0x7e;
                if terminal != 0x00 && terminal != 0x7e {
                    return Err(ParseError::Leb128Overflow);
                }
            }
            if byte & 0x40 != 0 {
                result |= (!0i128) << shift;
            }
            return i64::try_from(result)
                .map(|value| (value, index + 1))
                .map_err(|_| ParseError::Leb128Overflow);
        }
    }
    Err(ParseError::InvalidLeb128)
}

/// Decode a signed 33-bit LEB128 value used by WebAssembly block types.
///
/// The signed-33 domain can represent every u32 type index plus the negative
/// single-byte value-type encodings reserved by the binary format.
pub fn decode_s33(input: &[u8]) -> Result<(i64, usize), ParseError> {
    let mut result = 0i64;
    let mut shift = 0u32;
    for index in 0..5 {
        let byte = *input.get(index).ok_or(ParseError::UnexpectedEof)?;
        let payload = i64::from(byte & 0x7f);
        result |= payload << shift;
        shift += 7;
        if byte & 0x80 == 0 {
            if index == 4 {
                let unused = byte & 0x60;
                if unused != 0x00 && unused != 0x60 {
                    return Err(ParseError::Leb128Overflow);
                }
            }
            if byte & 0x40 != 0 && shift < 64 {
                result |= (!0i64) << shift;
            }
            const MIN_S33: i64 = -(1i64 << 32);
            const MAX_S33: i64 = (1i64 << 32) - 1;
            if !(MIN_S33..=MAX_S33).contains(&result) {
                return Err(ParseError::Leb128Overflow);
            }
            return Ok((result, index + 1));
        }
    }
    Err(ParseError::InvalidLeb128)
}

pub fn parse_module(bytes: &[u8]) -> Result<Module, ParseError> {
    let mut cursor = Cursor::new(bytes);
    let magic = cursor.read_array4()?;
    if magic != MAGIC {
        return Err(ParseError::InvalidMagic(magic));
    }
    let version = cursor.read_array4()?;
    if version != VERSION_1 {
        return Err(ParseError::UnsupportedVersion(version));
    }

    let mut module = Module::default();
    let mut last_standard = 0u8;
    let mut seen = [false; 12];

    while !cursor.is_eof() {
        let section_id = cursor.read_u8()?;
        let section_len = cursor.read_u32()? as usize;
        let payload = cursor.read_exact(section_len)?;

        if section_id == 0 {
            continue;
        }
        if !matches!(section_id, 1..=11) {
            return Err(ParseError::UnsupportedSection(section_id));
        }
        if section_id < last_standard {
            return Err(ParseError::SectionOutOfOrder {
                previous: last_standard,
                current: section_id,
            });
        }
        if seen[usize::from(section_id)] {
            return Err(ParseError::DuplicateSection(section_id));
        }
        seen[usize::from(section_id)] = true;
        last_standard = section_id;

        let mut section = Cursor::new(payload);
        match section_id {
            1 => parse_type_section(&mut section, &mut module)?,
            2 => parse_import_section(&mut section, &mut module)?,
            3 => parse_function_section(&mut section, &mut module)?,
            4 => parse_table_section(&mut section, &mut module)?,
            5 => parse_memory_section(&mut section, &mut module)?,
            6 => parse_global_section(&mut section, &mut module)?,
            7 => parse_export_section(&mut section, &mut module)?,
            8 => parse_start_section(&mut section, &mut module)?,
            9 => parse_element_section(&mut section, &mut module)?,
            10 => parse_code_section(&mut section, &mut module)?,
            11 => parse_data_section(&mut section, &mut module)?,
            _ => unreachable!("standard section range is exhaustive"),
        }
        if !section.is_eof() {
            return Err(ParseError::SectionLengthMismatch(section_id));
        }
    }
    Ok(module)
}

fn parse_type_section(cursor: &mut Cursor<'_>, module: &mut Module) -> Result<(), ParseError> {
    let count = cursor.read_u32()?;
    for _ in 0..count {
        let tag = cursor.read_u8()?;
        if tag != 0x60 {
            return Err(ParseError::InvalidFunctionType(tag));
        }
        let params = read_value_type_vec(cursor)?;
        let results = read_value_type_vec(cursor)?;
        module.types.push(FuncType { params, results });
    }
    Ok(())
}

fn parse_import_section(cursor: &mut Cursor<'_>, module: &mut Module) -> Result<(), ParseError> {
    let count = cursor.read_u32()?;
    module.imports.reserve(count as usize);
    for _ in 0..count {
        let import_module = cursor.read_name()?;
        let name = cursor.read_name()?;
        let kind = cursor.read_u8()?;
        if kind != 0x00 {
            return Err(ParseError::InvalidImportKind(kind));
        }
        let type_index = cursor.read_u32()?;
        module.imports.push(Import {
            module: import_module,
            name,
            type_index,
        });
    }
    Ok(())
}

fn parse_function_section(cursor: &mut Cursor<'_>, module: &mut Module) -> Result<(), ParseError> {
    let count = cursor.read_u32()?;
    module.function_type_indices.reserve(count as usize);
    for _ in 0..count {
        module.function_type_indices.push(cursor.read_u32()?);
    }
    Ok(())
}

fn parse_table_section(cursor: &mut Cursor<'_>, module: &mut Module) -> Result<(), ParseError> {
    let count = cursor.read_u32()?;
    module.tables.reserve(count as usize);
    for _ in 0..count {
        let reference_type = cursor.read_u8()?;
        if reference_type != 0x70 {
            return Err(ParseError::InvalidReferenceType(reference_type));
        }
        module.tables.push(TableType {
            limits: read_limits(cursor)?,
        });
    }
    Ok(())
}

fn parse_memory_section(cursor: &mut Cursor<'_>, module: &mut Module) -> Result<(), ParseError> {
    let count = cursor.read_u32()?;
    module.memories.reserve(count as usize);
    for _ in 0..count {
        module.memories.push(MemoryType {
            limits: read_limits(cursor)?,
        });
    }
    Ok(())
}

fn parse_global_section(cursor: &mut Cursor<'_>, module: &mut Module) -> Result<(), ParseError> {
    let count = cursor.read_u32()?;
    module.globals.reserve(count as usize);
    for _ in 0..count {
        let value_type = read_value_type(cursor)?;
        let mutable = match cursor.read_u8()? {
            0 => false,
            1 => true,
            other => return Err(ParseError::InvalidMutability(other)),
        };
        let init = read_const_expr(cursor)?;
        let actual = init.value_type();
        if actual != value_type {
            return Err(ParseError::ConstExprTypeMismatch {
                expected: value_type,
                actual,
            });
        }
        module.globals.push(Global {
            ty: GlobalType {
                value_type,
                mutable,
            },
            init,
        });
    }
    Ok(())
}

fn parse_export_section(cursor: &mut Cursor<'_>, module: &mut Module) -> Result<(), ParseError> {
    let count = cursor.read_u32()?;
    module.exports.reserve(count as usize);
    for _ in 0..count {
        let name = cursor.read_name()?;
        let kind = match cursor.read_u8()? {
            0 => ExportKind::Function,
            1 => ExportKind::Table,
            2 => ExportKind::Memory,
            3 => ExportKind::Global,
            other => return Err(ParseError::InvalidExportKind(other)),
        };
        let index = cursor.read_u32()?;
        module.exports.push(Export { name, kind, index });
    }
    Ok(())
}

fn parse_start_section(cursor: &mut Cursor<'_>, module: &mut Module) -> Result<(), ParseError> {
    module.start = Some(cursor.read_u32()?);
    Ok(())
}

fn parse_element_section(cursor: &mut Cursor<'_>, module: &mut Module) -> Result<(), ParseError> {
    let count = cursor.read_u32()?;
    module.elements.reserve(count as usize);
    for _ in 0..count {
        let mode = cursor.read_u32()?;
        if mode != 0 {
            return Err(ParseError::UnsupportedElementSegmentMode(mode));
        }
        let offset = read_i32_const_expr(cursor)?;
        let function_count = cursor.read_u32()?;
        let mut function_indices = Vec::with_capacity(function_count as usize);
        for _ in 0..function_count {
            function_indices.push(cursor.read_u32()?);
        }
        module.elements.push(ElementSegment {
            table_index: 0,
            offset,
            function_indices,
        });
    }
    Ok(())
}

fn parse_code_section(cursor: &mut Cursor<'_>, module: &mut Module) -> Result<(), ParseError> {
    let count = cursor.read_u32()?;
    module.code.reserve(count as usize);
    for _ in 0..count {
        let body_len = cursor.read_u32()? as usize;
        let body_bytes = cursor.read_exact(body_len)?;
        let mut body = Cursor::new(body_bytes);
        let local_group_count = body.read_u32()?;
        let mut locals = Vec::with_capacity(local_group_count as usize);
        for _ in 0..local_group_count {
            let count = body.read_u32()?;
            let ty = read_value_type(&mut body)?;
            locals.push((count, ty));
        }
        let code = body.remaining().to_vec();
        if code.last().copied() != Some(0x0b) {
            return Err(ParseError::FunctionBodyMissingEnd);
        }
        body.consume_remaining();
        module.code.push(FunctionBody { locals, code });
    }
    Ok(())
}

fn parse_data_section(cursor: &mut Cursor<'_>, module: &mut Module) -> Result<(), ParseError> {
    let count = cursor.read_u32()?;
    module.data.reserve(count as usize);
    for _ in 0..count {
        let mode = cursor.read_u32()?;
        if mode != 0 {
            return Err(ParseError::UnsupportedDataSegmentMode(mode));
        }
        let offset = read_i32_const_expr(cursor)?;
        let len = cursor.read_u32()? as usize;
        let bytes = cursor.read_exact(len)?.to_vec();
        module.data.push(DataSegment {
            memory_index: 0,
            offset,
            bytes,
        });
    }
    Ok(())
}

fn read_limits(cursor: &mut Cursor<'_>) -> Result<Limits, ParseError> {
    let flags = cursor.read_u8()?;
    let min = cursor.read_u32()?;
    let max = match flags {
        0x00 => None,
        0x01 => Some(cursor.read_u32()?),
        _ => return Err(ParseError::InvalidLimitsFlags(flags)),
    };
    Ok(Limits { min, max })
}

fn read_const_expr(cursor: &mut Cursor<'_>) -> Result<Constant, ParseError> {
    let opcode = cursor.read_u8()?;
    let value = match opcode {
        0x41 => Constant::I32(cursor.read_i32()?),
        0x42 => Constant::I64(cursor.read_i64()?),
        0x43 => Constant::F32(cursor.read_u32_le()?),
        0x44 => Constant::F64(cursor.read_u64_le()?),
        other => return Err(ParseError::InvalidConstExprOpcode(other)),
    };
    if cursor.read_u8()? != 0x0b {
        return Err(ParseError::ConstExprMissingEnd);
    }
    Ok(value)
}

fn read_i32_const_expr(cursor: &mut Cursor<'_>) -> Result<i32, ParseError> {
    match read_const_expr(cursor)? {
        Constant::I32(value) => Ok(value),
        other => Err(ParseError::ConstExprTypeMismatch {
            expected: ValueType::I32,
            actual: other.value_type(),
        }),
    }
}

fn read_value_type_vec(cursor: &mut Cursor<'_>) -> Result<Vec<ValueType>, ParseError> {
    let count = cursor.read_u32()?;
    let mut values = Vec::with_capacity(count as usize);
    for _ in 0..count {
        values.push(read_value_type(cursor)?);
    }
    Ok(values)
}

fn read_value_type(cursor: &mut Cursor<'_>) -> Result<ValueType, ParseError> {
    match cursor.read_u8()? {
        0x7f => Ok(ValueType::I32),
        0x7e => Ok(ValueType::I64),
        0x7d => Ok(ValueType::F32),
        0x7c => Ok(ValueType::F64),
        other => Err(ParseError::UnsupportedValueType(other)),
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn is_eof(&self) -> bool {
        self.offset == self.bytes.len()
    }

    fn remaining(&self) -> &'a [u8] {
        &self.bytes[self.offset..]
    }

    fn consume_remaining(&mut self) {
        self.offset = self.bytes.len();
    }

    fn read_u8(&mut self) -> Result<u8, ParseError> {
        let byte = *self
            .bytes
            .get(self.offset)
            .ok_or(ParseError::UnexpectedEof)?;
        self.offset += 1;
        Ok(byte)
    }

    fn read_u32(&mut self) -> Result<u32, ParseError> {
        let (value, used) = decode_u32(self.remaining())?;
        self.offset += used;
        Ok(value)
    }

    fn read_i32(&mut self) -> Result<i32, ParseError> {
        let (value, used) = decode_i32(self.remaining())?;
        self.offset += used;
        Ok(value)
    }

    fn read_i64(&mut self) -> Result<i64, ParseError> {
        let (value, used) = decode_i64(self.remaining())?;
        self.offset += used;
        Ok(value)
    }

    fn read_u32_le(&mut self) -> Result<u32, ParseError> {
        let bytes: [u8; 4] = self
            .read_exact(4)?
            .try_into()
            .expect("read_exact returned four bytes");
        Ok(u32::from_le_bytes(bytes))
    }

    fn read_u64_le(&mut self) -> Result<u64, ParseError> {
        let bytes: [u8; 8] = self
            .read_exact(8)?
            .try_into()
            .expect("read_exact returned eight bytes");
        Ok(u64::from_le_bytes(bytes))
    }

    fn read_array4(&mut self) -> Result<[u8; 4], ParseError> {
        let bytes = self.read_exact(4)?;
        Ok([bytes[0], bytes[1], bytes[2], bytes[3]])
    }

    fn read_exact(&mut self, len: usize) -> Result<&'a [u8], ParseError> {
        let end = self
            .offset
            .checked_add(len)
            .filter(|end| *end <= self.bytes.len())
            .ok_or(ParseError::UnexpectedEof)?;
        let result = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(result)
    }

    fn read_name(&mut self) -> Result<String, ParseError> {
        let len = self.read_u32()? as usize;
        let bytes = self.read_exact(len)?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| ParseError::InvalidUtf8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
        assert!(payload.len() < 128);
        module.push(id);
        module.push(payload.len() as u8);
        module.extend(payload);
    }

    fn add_module() -> Vec<u8> {
        vec![
            0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x07, 0x01, 0x60, 0x02, 0x7f,
            0x7f, 0x01, 0x7f, 0x03, 0x02, 0x01, 0x00, 0x07, 0x07, 0x01, 0x03, b'a', b'd', b'd',
            0x00, 0x00, 0x0a, 0x09, 0x01, 0x07, 0x00, 0x20, 0x00, 0x20, 0x01, 0x6a, 0x0b,
        ]
    }

    fn memory_data_module() -> Vec<u8> {
        vec![
            0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x05, 0x04, 0x01, 0x01, 0x01, 0x02,
            0x0b, 0x09, 0x01, 0x00, 0x41, 0x04, 0x0b, 0x03, b'w', b'a', b's',
        ]
    }

    fn imported_function_module() -> Vec<u8> {
        let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        bytes.extend([0x01, 0x06, 0x01, 0x60, 0x01, 0x7f, 0x01, 0x7f]);
        let import = [
            0x01, 0x03, b'e', b'n', b'v', 0x06, b'd', b'o', b'u', b'b', b'l', b'e', 0x00, 0x00,
        ];
        push_section(&mut bytes, 2, &import);
        bytes
    }

    #[test]
    fn parses_minimal_add_module() {
        let module = parse_module(&add_module()).expect("valid test module");
        assert_eq!(module.types.len(), 1);
        assert_eq!(module.types[0].params, vec![ValueType::I32, ValueType::I32]);
        assert_eq!(module.function_type_indices, vec![0]);
        assert_eq!(module.exports[0].name, "add");
        assert_eq!(module.code.len(), 1);
    }

    #[test]
    fn parses_function_import() {
        let module = parse_module(&imported_function_module()).expect("valid function import");
        assert_eq!(module.imports.len(), 1);
        assert_eq!(module.imports[0].module, "env");
        assert_eq!(module.imports[0].name, "double");
        assert_eq!(module.imports[0].type_index, 0);
    }

    #[test]
    fn rejects_non_function_import() {
        let mut bytes = imported_function_module();
        let kind_offset = bytes.len() - 2;
        bytes[kind_offset] = 0x02;
        assert_eq!(
            parse_module(&bytes),
            Err(ParseError::InvalidImportKind(0x02))
        );
    }

    #[test]
    fn parses_table_global_start_and_element() {
        let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        push_section(&mut bytes, 1, &[0x01, 0x60, 0x00, 0x00]);
        push_section(&mut bytes, 3, &[0x01, 0x00]);
        push_section(&mut bytes, 4, &[0x01, 0x70, 0x01, 0x02, 0x04]);
        push_section(&mut bytes, 6, &[0x01, 0x7f, 0x01, 0x41, 0x2a, 0x0b]);
        push_section(&mut bytes, 8, &[0x00]);
        push_section(&mut bytes, 9, &[0x01, 0x00, 0x41, 0x01, 0x0b, 0x01, 0x00]);
        push_section(&mut bytes, 10, &[0x01, 0x02, 0x00, 0x0b]);
        let module = parse_module(&bytes).expect("phase 5A sections parse");
        assert_eq!(
            module.tables[0].limits,
            Limits {
                min: 2,
                max: Some(4)
            }
        );
        assert_eq!(module.globals[0].init, Constant::I32(42));
        assert!(module.globals[0].ty.mutable);
        assert_eq!(module.start, Some(0));
        assert_eq!(module.elements[0].offset, 1);
        assert_eq!(module.elements[0].function_indices, vec![0]);
    }

    #[test]
    fn parses_all_numeric_global_constants_without_normalizing_float_bits() {
        let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        let f32_bits = 0x7fc0_1234u32;
        let f64_bits = 0x7ff8_0000_0000_1234u64;
        let mut globals = vec![
            0x04, 0x7f, 0x00, 0x41, 0x7f, 0x0b, 0x7e, 0x00, 0x42, 0x7e, 0x0b, 0x7d, 0x00, 0x43,
        ];
        globals.extend(f32_bits.to_le_bytes());
        globals.push(0x0b);
        globals.extend([0x7c, 0x00, 0x44]);
        globals.extend(f64_bits.to_le_bytes());
        globals.push(0x0b);
        push_section(&mut bytes, 6, &globals);
        let module = parse_module(&bytes).expect("numeric globals parse");
        assert_eq!(module.globals[0].init, Constant::I32(-1));
        assert_eq!(module.globals[1].init, Constant::I64(-2));
        assert_eq!(module.globals[2].init, Constant::F32(f32_bits));
        assert_eq!(module.globals[3].init, Constant::F64(f64_bits));
    }

    #[test]
    fn rejects_global_initializer_type_mismatch() {
        let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        push_section(&mut bytes, 6, &[0x01, 0x7e, 0x00, 0x41, 0x00, 0x0b]);
        assert_eq!(
            parse_module(&bytes),
            Err(ParseError::ConstExprTypeMismatch {
                expected: ValueType::I64,
                actual: ValueType::I32,
            })
        );
    }

    #[test]
    fn rejects_non_funcref_table() {
        let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        push_section(&mut bytes, 4, &[0x01, 0x6f, 0x00, 0x01]);
        assert_eq!(
            parse_module(&bytes),
            Err(ParseError::InvalidReferenceType(0x6f))
        );
    }

    #[test]
    fn rejects_unsupported_element_mode() {
        let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        push_section(&mut bytes, 9, &[0x01, 0x01]);
        assert_eq!(
            parse_module(&bytes),
            Err(ParseError::UnsupportedElementSegmentMode(1))
        );
    }

    #[test]
    fn parses_memory_and_active_data_segment() {
        let module = parse_module(&memory_data_module()).expect("valid memory module");
        assert_eq!(module.memories.len(), 1);
        assert_eq!(module.memories[0].limits.min, 1);
        assert_eq!(module.memories[0].limits.max, Some(2));
        assert_eq!(module.data[0].offset, 4);
        assert_eq!(module.data[0].bytes, b"was");
    }

    #[test]
    fn rejects_bad_magic() {
        let mut bytes = add_module();
        bytes[0] = 0xff;
        assert!(matches!(
            parse_module(&bytes),
            Err(ParseError::InvalidMagic(_))
        ));
    }

    #[test]
    fn decodes_signed_and_unsigned_leb128() {
        assert_eq!(decode_u32(&[0xe5, 0x8e, 0x26]), Ok((624_485, 3)));
        assert_eq!(decode_i32(&[0x7f]), Ok((-1, 1)));
        assert_eq!(decode_i32(&[0xc0, 0xbb, 0x78]), Ok((-123_456, 3)));
    }

    #[test]
    fn rejects_u32_leb_overflow() {
        assert_eq!(
            decode_u32(&[0xff, 0xff, 0xff, 0xff, 0x10]),
            Err(ParseError::Leb128Overflow)
        );
    }

    #[test]
    fn rejects_out_of_order_sections() {
        let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        bytes.extend([0x03, 0x01, 0x00]);
        bytes.extend([0x01, 0x01, 0x00]);
        assert!(matches!(
            parse_module(&bytes),
            Err(ParseError::SectionOutOfOrder { .. })
        ));
    }

    #[test]
    fn rejects_unsupported_data_segment_mode() {
        let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        bytes.extend([0x0b, 0x03, 0x01, 0x01, 0x00]);
        assert_eq!(
            parse_module(&bytes),
            Err(ParseError::UnsupportedDataSegmentMode(1))
        );
    }
}
