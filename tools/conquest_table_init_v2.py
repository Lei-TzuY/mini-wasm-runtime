from pathlib import Path


def rep(path, old, new, label):
    p = Path(path)
    text = p.read_text()
    if text.count(old) != 1:
        raise SystemExit(f"{label}: expected one anchor, got {text.count(old)}")
    p.write_text(text.replace(old, new, 1))

R = "crates/wasm-runtime/src/lib.rs"
V = "crates/wasm-validator/src/lib.rs"
T = "crates/wasm-validator/src/typed.rs"

old = """    DataSegmentSourceOutOfBounds {
        segment: u32,
        offset: u64,
        length: usize,
    },
"""
rep(R, old, old + """    ElementIndexOutOfBounds(u32),
    ElementSegmentSourceOutOfBounds {
        segment: u32,
        offset: u64,
        length: usize,
    },
""", "runtime errors")

old = """            Self::DataSegmentSourceOutOfBounds { segment, offset, length } => write!(
                f,
                "data segment {segment} source byte {offset} with length {length} is out of bounds"
            ),
"""
rep(R, old, old + """            Self::ElementIndexOutOfBounds(index) => {
                write!(f, "element segment index {index} is out of bounds")
            }
            Self::ElementSegmentSourceOutOfBounds { segment, offset, length } => write!(
                f,
                "element segment {segment} source slot {offset} with length {length} is out of bounds"
            ),
""", "runtime display")

rep(R, """    data_segments: Vec<Vec<u8>>,
    table: Option<TableHandle>,
""", """    data_segments: Vec<Vec<u8>>,
    element_segments: Vec<Vec<u32>>,
    table: Option<TableHandle>,
""", "instance field")

rep(R, """        let identity = Rc::new(());
        let table = instantiate_table(&module, &hosts, &identity)?;
""", """        let element_segments = module
            .elements
            .iter()
            .map(|segment| match segment.mode {
                ElementMode::Passive => segment.function_indices.clone(),
                ElementMode::Active { .. } | ElementMode::Declarative => Vec::new(),
            })
            .collect();
        let identity = Rc::new(());
        let table = instantiate_table(&module, &hosts, &identity)?;
""", "element storage")

rep(R, """            data_segments,
            table,
""", """            data_segments,
            element_segments,
            table,
""", "instance init")

anchor = """    fn initialize_element_segments(&mut self) -> Result<(), RuntimeError> {
"""
methods = """    fn table_init(
        &mut self,
        element_index: u32,
        table_index: u32,
        destination: i32,
        source: i32,
        length: i32,
    ) -> Result<(), RuntimeError> {
        if table_index != 0 {
            return Err(RuntimeError::TableElementOutOfBounds(table_index));
        }
        let width = length as u32 as usize;
        let source_start = u64::from(source as u32);
        let source_end = source_start.checked_add(width as u64).ok_or(
            RuntimeError::ElementSegmentSourceOutOfBounds {
                segment: element_index,
                offset: source_start,
                length: width,
            },
        )?;
        let segment = self.element_segments.get(element_index as usize)
            .ok_or(RuntimeError::ElementIndexOutOfBounds(element_index))?;
        if source_end > segment.len() as u64 {
            return Err(RuntimeError::ElementSegmentSourceOutOfBounds {
                segment: element_index,
                offset: source_start,
                length: width,
            });
        }
        let start = source_start as usize;
        let functions = segment[start..start + width].to_vec();
        let table = self.table.as_ref()
            .ok_or(RuntimeError::TableElementOutOfBounds(destination as u32))?;
        let destination_start = u64::from(destination as u32);
        let destination_end = destination_start.checked_add(width as u64)
            .ok_or(RuntimeError::TableElementOutOfBounds(destination as u32))?;
        if destination_end > table.slots.borrow().len() as u64 {
            return Err(RuntimeError::TableElementOutOfBounds(destination as u32));
        }
        let mut slots = table.slots.borrow_mut();
        let destination_start = destination_start as usize;
        for (offset, function_index) in functions.into_iter().enumerate() {
            slots[destination_start + offset] = Some(FunctionRef {
                owner: Rc::downgrade(&self.identity),
                function_index,
            });
        }
        Ok(())
    }

    fn elem_drop(&mut self, element_index: u32) -> Result<(), RuntimeError> {
        self.element_segments.get_mut(element_index as usize)
            .ok_or(RuntimeError::ElementIndexOutOfBounds(element_index))?
            .clear();
        Ok(())
    }

"""
rep(R, anchor, methods + anchor, "runtime methods")

old = """                        11 => {
                            let memory_index = read_u32_immediate(code, &mut pc)?;
                            ensure_runtime_memory_index(self, memory_index)?;
                            let length = numeric::i32_from_stack(&mut stack)?;
                            let value = numeric::i32_from_stack(&mut stack)?;
                            let destination = numeric::i32_from_stack(&mut stack)?;
                            self.with_memory_mut(|memory| memory.fill(destination, value, length))?;
                        }
"""
rep(R, old, old + """                        12 => {
                            let element_index = read_u32_immediate(code, &mut pc)?;
                            let table_index = read_u32_immediate(code, &mut pc)?;
                            let length = numeric::i32_from_stack(&mut stack)?;
                            let source = numeric::i32_from_stack(&mut stack)?;
                            let destination = numeric::i32_from_stack(&mut stack)?;
                            self.table_init(element_index, table_index, destination, source, length)?;
                        }
                        13 => {
                            let element_index = read_u32_immediate(code, &mut pc)?;
                            self.elem_drop(element_index)?;
                        }
""", "runtime opcodes")

old = """                    11 => {
                        let _ = read_u32_immediate(code, &mut pc)?;
                    }
"""
rep(R, old, old + """                    12 => {
                        let _ = read_u32_immediate(code, &mut pc)?;
                        let _ = read_u32_immediate(code, &mut pc)?;
                    }
                    13 => {
                        let _ = read_u32_immediate(code, &mut pc)?;
                    }
""", "predecode opcodes")

old = """    DataIndexOutOfBounds {
        function: usize,
        offset: usize,
        data_index: u32,
    },
"""
rep(V, old, old + """    ElementIndexOutOfBounds {
        function: usize,
        offset: usize,
        element_index: u32,
    },
""", "validator error")

old = """            Self::DataIndexOutOfBounds { function, offset, data_index } => write!(
                f,
                "function {function} bulk-memory instruction at byte {offset} refers to missing data segment {data_index}"
            ),
"""
rep(V, old, old + """            Self::ElementIndexOutOfBounds {
                function,
                offset,
                element_index,
            } => write!(
                f,
                "function {function} bulk-memory instruction at byte {offset} refers to missing element segment {element_index}"
            ),
""", "validator display")

old = """                    11 => {
                        super::read_memory_index(code, &mut pc, module, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;
                    }
"""
rep(T, old, old + """                    12 => {
                        let element_index = read_u32(code, &mut pc, function, offset)?;
                        if element_index as usize >= module.elements.len() {
                            return Err(ValidationError::ElementIndexOutOfBounds { function, offset, element_index });
                        }
                        let table_index = read_u32(code, &mut pc, function, offset)?;
                        if table_index as usize >= module.table_count() {
                            return Err(ValidationError::TableIndexOutOfBounds { function, offset, table_index });
                        }
                        pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;
                    }
                    13 => {
                        let element_index = read_u32(code, &mut pc, function, offset)?;
                        if element_index as usize >= module.elements.len() {
                            return Err(ValidationError::ElementIndexOutOfBounds { function, offset, element_index });
                        }
                    }
""", "typed opcodes")

Path('crates/wasm-runtime/tests/bulk_table_init_drop.rs').write_text(r'''use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, RuntimeError, TableHandle};
use wasm_validator::ValidationError;

fn u32leb(out: &mut Vec<u8>, mut v: u32) { loop { let mut b = (v & 0x7f) as u8; v >>= 7; if v != 0 { b |= 0x80; } out.push(b); if v == 0 { break; } } }
fn name(out: &mut Vec<u8>, s: &str) { u32leb(out, s.len() as u32); out.extend_from_slice(s.as_bytes()); }
fn section(m: &mut Vec<u8>, id: u8, p: &[u8]) { m.push(id); u32leb(m, p.len() as u32); m.extend_from_slice(p); }
fn module(body: &[u8]) -> Vec<u8> {
    let mut m = b"\0asm\x01\0\0\0".to_vec();
    section(&mut m, 1, &[1,0x60,0,0]);
    let mut i = vec![1]; name(&mut i,"env"); name(&mut i,"tab"); i.extend([1,0x70,0,2]); section(&mut m,2,&i);
    section(&mut m,3,&[1,0]); section(&mut m,7,&[1,3,b'r',b'u',b'n',0,0]);
    section(&mut m,9,&[1,1,0,1,0]);
    let mut c = vec![1,(body.len()+1) as u8,0]; c.extend_from_slice(body); section(&mut m,10,&c); m
}
fn hosts(table:&TableHandle)->HostRegistry { let mut h=HostRegistry::new(); h.register_table("env","tab",table.clone()).unwrap(); h }

#[test] fn table_init_populates_imported_table(){ let body=[0x41,0,0x41,0,0x41,1,0xfc,12,0,0,0x0b]; let table=TableHandle::new(2,Some(2)).unwrap(); let mut vm=Instance::with_hosts(parse_module(&module(&body)).unwrap(),hosts(&table)).unwrap(); vm.invoke_export("run",&[]).unwrap(); assert!(table.get(0).unwrap().is_some()); assert!(table.get(1).unwrap().is_none()); }
#[test] fn elem_drop_traps_followup_nonempty_init_atomically(){ let body=[0xfc,13,0,0x41,0,0x41,0,0x41,1,0xfc,12,0,0,0x0b]; let table=TableHandle::new(2,Some(2)).unwrap(); let mut vm=Instance::with_hosts(parse_module(&module(&body)).unwrap(),hosts(&table)).unwrap(); assert!(matches!(vm.invoke_export("run",&[]),Err(RuntimeError::ElementSegmentSourceOutOfBounds{..}))); assert!(table.get(0).unwrap().is_none()); }
#[test] fn source_oob_is_atomic(){ let body=[0x41,0,0x41,1,0x41,1,0xfc,12,0,0,0x0b]; let table=TableHandle::new(2,Some(2)).unwrap(); let mut vm=Instance::with_hosts(parse_module(&module(&body)).unwrap(),hosts(&table)).unwrap(); assert!(matches!(vm.invoke_export("run",&[]),Err(RuntimeError::ElementSegmentSourceOutOfBounds{..}))); assert!(table.get(0).unwrap().is_none()); }
#[test] fn destination_oob_is_atomic(){ let body=[0x41,2,0x41,0,0x41,1,0xfc,12,0,0,0x0b]; let table=TableHandle::new(2,Some(2)).unwrap(); let mut vm=Instance::with_hosts(parse_module(&module(&body)).unwrap(),hosts(&table)).unwrap(); assert!(matches!(vm.invoke_export("run",&[]),Err(RuntimeError::TableElementOutOfBounds(_)))); assert!(table.get(0).unwrap().is_none()&&table.get(1).unwrap().is_none()); }
#[test] fn validator_rejects_bad_element_index(){ let body=[0x41,0,0x41,0,0x41,0,0xfc,12,1,0,0x0b]; let table=TableHandle::new(2,Some(2)).unwrap(); assert!(matches!(Instance::with_hosts(parse_module(&module(&body)).unwrap(),hosts(&table)),Err(RuntimeError::Validation(ValidationError::ElementIndexOutOfBounds{element_index:1,..})))); }
#[test] fn validator_rejects_bad_table_index(){ let body=[0x41,0,0x41,0,0x41,0,0xfc,12,0,1,0x0b]; let table=TableHandle::new(2,Some(2)).unwrap(); assert!(matches!(Instance::with_hosts(parse_module(&module(&body)).unwrap(),hosts(&table)),Err(RuntimeError::Validation(ValidationError::TableIndexOutOfBounds{table_index:1,..})))); }
''')

Path('docs/bulk-table-init-drop.md').write_text('''# Bulk table: `table.init` and `elem.drop`\n\nThis vertical slice adds executable bulk-memory `table.init` (`0xfc 12`) and `elem.drop` (`0xfc 13`) for the existing single funcref-table surface. Passive element payloads are stored per instance; active and declarative segments begin unavailable to bulk initialization.\n\nValidation checks element/table indices and the three i32 operands. Execution uses unsigned table32 addressing, preflights both source and destination ranges before mutation, updates imported `TableHandle` backing directly, and makes dropped segments empty for subsequent initialization.\n''')
