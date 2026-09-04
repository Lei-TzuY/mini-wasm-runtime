from pathlib import Path

validator = Path("crates/wasm-validator/src/typed.rs")
text = validator.read_text()
old = '''            0xfc => {
                let subopcode = read_u32(code, &mut pc, function, offset)?;
                let (input, output) = match subopcode {
                    0 | 1 => (ValueType::F32, ValueType::I32),
                    2 | 3 => (ValueType::F64, ValueType::I32),
                    4 | 5 => (ValueType::F32, ValueType::I64),
                    6 | 7 => (ValueType::F64, ValueType::I64),
                    _ => {
                        return Err(ValidationError::UnsupportedPrefixedOpcode {
                            function,
                            offset,
                            prefix: 0xfc,
                            subopcode,
                        })
                    }
                };
                unary(&mut stack, &controls, input, output, function, offset)?;
            }
'''
new = '''            0xfc => {
                let subopcode = read_u32(code, &mut pc, function, offset)?;
                match subopcode {
                    0 | 1 => unary(&mut stack, &controls, ValueType::F32, ValueType::I32, function, offset)?,
                    2 | 3 => unary(&mut stack, &controls, ValueType::F64, ValueType::I32, function, offset)?,
                    4 | 5 => unary(&mut stack, &controls, ValueType::F32, ValueType::I64, function, offset)?,
                    6 | 7 => unary(&mut stack, &controls, ValueType::F64, ValueType::I64, function, offset)?,
                    10 => {
                        super::read_memory_index(code, &mut pc, module, function, offset)?;
                        super::read_memory_index(code, &mut pc, module, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;
                    }
                    _ => {
                        return Err(ValidationError::UnsupportedPrefixedOpcode {
                            function,
                            offset,
                            prefix: 0xfc,
                            subopcode,
                        })
                    }
                }
            }
'''
if old not in text:
    raise SystemExit("validator anchor not found")
validator.write_text(text.replace(old, new, 1))

runtime = Path("crates/wasm-runtime/src/lib.rs")
text = runtime.read_text()
anchor = '''    fn checked_range(
        &self,
        address: i32,
        displacement: u32,
        width: usize,
    ) -> Result<std::ops::Range<usize>, RuntimeError> {
'''
if anchor not in text:
    raise SystemExit("LinearMemory checked_range anchor not found")
copy_method = '''    fn copy(&mut self, destination: i32, source: i32, length: i32) -> Result<(), RuntimeError> {
        let width = length as u32 as usize;
        let source_range = self.checked_range(source, 0, width)?;
        let destination_range = self.checked_range(destination, 0, width)?;
        self.bytes.copy_within(source_range, destination_range.start);
        Ok(())
    }

'''
text = text.replace(anchor, copy_method + anchor, 1)

control_old = '''            0xfc => {
                let subopcode = read_u32_immediate(code, &mut pc)?;
                if subopcode > 7 {
                    return Err(RuntimeError::UnsupportedPrefixedOpcode {
                        prefix: 0xfc,
                        subopcode,
                    });
                }
            }
'''
control_new = '''            0xfc => {
                let subopcode = read_u32_immediate(code, &mut pc)?;
                match subopcode {
                    0..=7 => {}
                    10 => {
                        let _ = read_u32_immediate(code, &mut pc)?;
                        let _ = read_u32_immediate(code, &mut pc)?;
                    }
                    _ => {
                        return Err(RuntimeError::UnsupportedPrefixedOpcode {
                            prefix: 0xfc,
                            subopcode,
                        })
                    }
                }
            }
'''
if control_old not in text:
    raise SystemExit("control predecoder 0xfc anchor not found")
text = text.replace(control_old, control_new, 1)

exec_old = '''                0xfc => {
                    let subopcode = read_u32_immediate(code, &mut pc)?;
                    numeric::trunc_sat(&mut stack, subopcode)?;
                }
'''
exec_new = '''                0xfc => {
                    let subopcode = read_u32_immediate(code, &mut pc)?;
                    match subopcode {
                        0..=7 => numeric::trunc_sat(&mut stack, subopcode)?,
                        10 => {
                            let destination_memory = read_u32_immediate(code, &mut pc)?;
                            let source_memory = read_u32_immediate(code, &mut pc)?;
                            ensure_runtime_memory_index(self, destination_memory)?;
                            ensure_runtime_memory_index(self, source_memory)?;
                            let length = numeric::i32_from_stack(&mut stack)?;
                            let source = numeric::i32_from_stack(&mut stack)?;
                            let destination = numeric::i32_from_stack(&mut stack)?;
                            self.with_memory_mut(|memory| memory.copy(destination, source, length))?;
                        }
                        _ => return Err(RuntimeError::UnsupportedPrefixedOpcode { prefix: 0xfc, subopcode }),
                    }
                }
'''
if exec_old not in text:
    raise SystemExit("runtime 0xfc anchor not found")
runtime.write_text(text.replace(exec_old, exec_new, 1))

Path("crates/wasm-runtime/tests/bulk_memory_copy.rs").write_text(r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};

fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    assert!(payload.len() < 128);
    module.push(id);
    module.push(payload.len() as u8);
    module.extend(payload);
}

fn module_with_bodies(bodies: &[Vec<u8>], exports: &[(&str, u8)]) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    push_section(&mut bytes, 1, &[0x01, 0x60, 0x00, 0x01, 0x7f]);
    let mut functions = vec![bodies.len() as u8];
    functions.extend(std::iter::repeat(0x00).take(bodies.len()));
    push_section(&mut bytes, 3, &functions);
    push_section(&mut bytes, 5, &[0x01, 0x00, 0x01]);
    let mut export_payload = vec![exports.len() as u8];
    for (name, index) in exports {
        export_payload.push(name.len() as u8);
        export_payload.extend(name.as_bytes());
        export_payload.push(0x00);
        export_payload.push(*index);
    }
    push_section(&mut bytes, 7, &export_payload);
    let mut code = vec![bodies.len() as u8];
    for body in bodies {
        code.push((body.len() + 1) as u8);
        code.push(0x00);
        code.extend(body);
    }
    push_section(&mut bytes, 10, &code);
    bytes
}

fn store8(address: u8, value: u8, body: &mut Vec<u8>) {
    body.extend([0x41, address, 0x41, value, 0x3a, 0x00, 0x00]);
}

#[test]
fn memory_copy_uses_memmove_semantics_for_overlap() {
    let mut body = Vec::new();
    for (address, value) in [(0, 10), (1, 20), (2, 30), (3, 40), (4, 50), (5, 60)] {
        store8(address, value, &mut body);
    }
    body.extend([0x41, 0x02, 0x41, 0x00, 0x41, 0x06, 0xfc, 0x0a, 0x00, 0x00, 0x41, 0x07, 0x2d, 0x00, 0x00, 0x0b]);
    let bytes = module_with_bodies(&[body], &[("run", 0)]);
    let mut instance = Instance::new(parse_module(&bytes).unwrap()).unwrap();
    assert_eq!(instance.invoke_export("run", &[]).unwrap(), Some(Value::I32(60)));
}

#[test]
fn memory_copy_preflights_destination_before_mutation() {
    let trap = vec![0x41, 0xff, 0xff, 0x03, 0x41, 0x00, 0x41, 0x02, 0xfc, 0x0a, 0x00, 0x00, 0x41, 0x00, 0x0b];
    let tail = vec![0x41, 0xff, 0xff, 0x03, 0x2d, 0x00, 0x00, 0x0b];
    let bytes = module_with_bodies(&[trap, tail], &[("trap", 0), ("tail", 1)]);
    let mut instance = Instance::new(parse_module(&bytes).unwrap()).unwrap();
    assert!(matches!(instance.invoke_export("trap", &[]), Err(RuntimeError::MemoryOutOfBounds { .. })));
    assert_eq!(instance.invoke_export("tail", &[]).unwrap(), Some(Value::I32(0)));
}

#[test]
fn memory_copy_rejects_nonzero_memory_indices() {
    let body = vec![0x41, 0x00, 0x41, 0x00, 0x41, 0x00, 0xfc, 0x0a, 0x01, 0x00, 0x41, 0x00, 0x0b];
    let bytes = module_with_bodies(&[body], &[("run", 0)]);
    assert!(Instance::new(parse_module(&bytes).unwrap()).is_err());
}
''')

Path("docs/bulk-memory-copy.md").write_text('''# Bulk memory: `memory.copy`\n\nThis vertical slice adds executable bulk-memory `memory.copy` (`0xfc 10`) for the runtime's existing single 32-bit memory surface. The validator consumes and validates both memory indices, requires the three i32 operands `(destination, source, length)`, and continues to reject non-zero memory indices until multi-memory is explicitly implemented.\n\nExecution treats i32 operands as unsigned memory32 addresses/lengths, preflights both source and destination ranges before mutation, and uses memmove-equivalent overlap semantics. The operation is routed through the existing `with_memory_mut` abstraction, so defined memory and imported `MemoryHandle` backing share the same implementation.\n\nThe bounded regression suite covers overlapping copies, fail-closed out-of-bounds destination handling with no partial write, and non-zero memory-index rejection. Other bulk-memory instructions (`memory.init`, `data.drop`, table forms) remain out of scope for this slice.\n''')
