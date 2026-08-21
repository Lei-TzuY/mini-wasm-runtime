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
    SectionOutOfOrder { previous: u8, current: u8 },
    DuplicateSection(u8),
    SectionLengthMismatch(u8),
    InvalidFunctionType(u8),
    UnsupportedValueType(u8),
    InvalidUtf8,
    InvalidExportKind(u8),
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
            Self::InvalidExportKind(kind) => write!(f, "invalid export kind {kind}"),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuncType {
    pub params: Vec<ValueType>,
    pub results: Vec<ValueType>,
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
pub struct FunctionBody {
    /// Local declarations encoded as (count, type) groups.
    pub locals: Vec<(u32, ValueType)>,
    /// Raw instruction bytes, including the final `end` (0x0b).
    pub code: Vec<u8>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Module {
    pub types: Vec<FuncType>,
    pub function_type_indices: Vec<u32>,
    pub exports: Vec<Export>,
    pub code: Vec<FunctionBody>,
}

/// Decode a canonical-or-noncanonical unsigned LEB128 u32 value.
///
/// The parser accepts encodings allowed by the WebAssembly binary format but
/// rejects any representation that needs more than five bytes or overflows u32.
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
    let mut seen = [false; 11];

    while !cursor.is_eof() {
        let section_id = cursor.read_u8()?;
        let section_len = cursor.read_u32()? as usize;
        let payload = cursor.read_exact(section_len)?;

        if section_id == 0 {
            continue;
        }
        if !matches!(section_id, 1 | 3 | 7 | 10) {
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
            3 => parse_function_section(&mut section, &mut module)?,
            7 => parse_export_section(&mut section, &mut module)?,
            10 => parse_code_section(&mut section, &mut module)?,
            _ => unreachable!("unsupported sections are rejected above"),
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

fn parse_function_section(
    cursor: &mut Cursor<'_>,
    module: &mut Module,
) -> Result<(), ParseError> {
    let count = cursor.read_u32()?;
    module.function_type_indices.reserve(count as usize);
    for _ in 0..count {
        module.function_type_indices.push(cursor.read_u32()?);
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
        let byte = *self.bytes.get(self.offset).ok_or(ParseError::UnexpectedEof)?;
        self.offset += 1;
        Ok(byte)
    }

    fn read_u32(&mut self) -> Result<u32, ParseError> {
        let (value, used) = decode_u32(self.remaining())?;
        self.offset += used;
        Ok(value)
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

    fn add_module() -> Vec<u8> {
        vec![
            0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // header
            0x01, 0x07, 0x01, 0x60, 0x02, 0x7f, 0x7f, 0x01, 0x7f, // type
            0x03, 0x02, 0x01, 0x00, // function
            0x07, 0x07, 0x01, 0x03, b'a', b'd', b'd', 0x00, 0x00, // export
            0x0a, 0x09, 0x01, 0x07, 0x00, 0x20, 0x00, 0x20, 0x01, 0x6a, 0x0b, // code
        ]
    }

    #[test]
    fn parses_minimal_add_module() {
        let module = parse_module(&add_module()).expect("valid test module");
        assert_eq!(module.types.len(), 1);
        assert_eq!(module.types[0].params, vec![ValueType::I32, ValueType::I32]);
        assert_eq!(module.function_type_indices, vec![0]);
        assert_eq!(module.exports[0].name, "add");
        assert_eq!(module.code.len(), 1);
        assert_eq!(module.code[0].code.last(), Some(&0x0b));
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
}
