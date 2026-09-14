from pathlib import Path


def replace(path, old, new):
    p = Path(path)
    text = p.read_text()
    if old not in text:
        raise SystemExit(f'missing anchor in {path}: {old[:80]!r}')
    p.write_text(text.replace(old, new, 1))

lib='crates/wasm-runtime/src/lib.rs'
typed='crates/wasm-validator/src/typed.rs'
sentinel='crates/wasm-runtime/tests/simd_i32x4_core.rs'

replace(lib, '''        11 => {
            let (_, memory_index, displacement) = read_memarg(code, pc)?;''', '''        84..=87 => {
            let (_, memory_index, displacement) = read_memarg(code, pc)?;
            ensure_runtime_memory_index(instance, memory_index)?;
            let lane = *code.get(*pc).ok_or(RuntimeError::ControlInvariant(
                "validated SIMD lane-load immediate is missing",
            ))?;
            *pc += 1;
            let (width, lane_limit) = match subopcode {
                84 => (1usize, 16u8),
                85 => (2usize, 8u8),
                86 => (4usize, 4u8),
                87 => (8usize, 2u8),
                _ => unreachable!("matched SIMD lane-load opcode"),
            };
            if lane >= lane_limit {
                return Err(RuntimeError::ControlInvariant(
                    "validated SIMD lane-load lane is out of bounds",
                ));
            }
            let mut vector = numeric::v128_from_stack(stack)?;
            let address = pop_runtime_memory_address(instance, stack, memory_index)?;
            let start = usize::from(lane) * width;
            match subopcode {
                84 => {
                    let value = instance.with_memory_index(memory_index, |memory| {
                        memory.load_i8_u(address, displacement)
                    })? as u8;
                    vector[start] = value;
                }
                85 => {
                    let value = instance.with_memory_index(memory_index, |memory| {
                        memory.load_i16_u(address, displacement)
                    })? as u16;
                    vector[start..start + 2].copy_from_slice(&value.to_le_bytes());
                }
                86 => {
                    let value = instance.with_memory_index(memory_index, |memory| {
                        memory.load_i32(address, displacement)
                    })?;
                    vector[start..start + 4].copy_from_slice(&value.to_le_bytes());
                }
                87 => {
                    let value = instance.with_memory_index(memory_index, |memory| {
                        memory.load_i64(address, displacement)
                    })?;
                    vector[start..start + 8].copy_from_slice(&value.to_le_bytes());
                }
                _ => unreachable!("matched SIMD lane-load opcode"),
            }
            stack.push(Value::V128(Rc::new(vector)));
        }
        11 => {
            let (_, memory_index, displacement) = read_memarg(code, pc)?;''')

replace(lib, '''                    0 | 11 => {
                        let _ = read_memarg(code, &mut pc)?;
                    }
                    12 => {''', '''                    0 | 11 => {
                        let _ = read_memarg(code, &mut pc)?;
                    }
                    84..=87 => {
                        let _ = read_memarg(code, &mut pc)?;
                        let lane = *code.get(pc).ok_or(RuntimeError::ControlInvariant(
                            "validated SIMD lane-load immediate is missing while scanning control",
                        ))?;
                        pc += 1;
                        let limit = match subopcode {
                            84 => 16,
                            85 => 8,
                            86 => 4,
                            87 => 2,
                            _ => unreachable!("matched SIMD lane-load opcode"),
                        };
                        if lane >= limit {
                            return Err(RuntimeError::ControlInvariant(
                                "validated SIMD lane-load lane is out of bounds while scanning control",
                            ));
                        }
                    }
                    12 => {''')

replace(typed, '''                    12 => {
                        skip_fixed(code, &mut pc, 16, function, offset)?;''', '''                    84..=87 => {
                        super::ensure_memory(module, function, offset)?;
                        let max_alignment = subopcode - 84;
                        let (_, memory_index, _) = super::read_memarg(
                            code,
                            &mut pc,
                            module,
                            function,
                            offset,
                            max_alignment,
                        )?;
                        let lane = *code
                            .get(pc)
                            .ok_or(ValidationError::MalformedImmediate { function, offset })?;
                        pc += 1;
                        let lane_limit = match subopcode {
                            84 => 16,
                            85 => 8,
                            86 => 4,
                            87 => 2,
                            _ => unreachable!("matched SIMD lane-load opcode"),
                        };
                        if lane >= lane_limit {
                            return Err(ValidationError::MalformedImmediate { function, offset });
                        }
                        let address_type = if module
                            .memory_type(memory_index)
                            .expect("validated memory index")
                            .limits
                            .memory64
                        {
                            ValueType::I64
                        } else {
                            ValueType::I32
                        };
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        pop_expect(&mut stack, &controls, address_type, function, offset)?;
                        stack.push(ValueType::V128);
                    }
                    12 => {
                        skip_fixed(code, &mut pc, 16, function, offset)?;''')

replace(sentinel, '''fn next_simd_lane_memory_subopcode_remains_fail_closed() {
    let bytes = module(&[
        0xfd, 0x54, // v128.load8_lane is the next unsupported lane-memory capability
    ]);''', '''fn next_simd_lane_memory_subopcode_remains_fail_closed() {
    let bytes = module(&[
        0xfd, 0x58, // v128.store8_lane is the next unsupported lane-memory capability
    ]);''')
replace(sentinel, '''                subopcode: 84,''', '''                subopcode: 88,''')

Path('crates/wasm-runtime/tests/simd_lane_memory_loads.rs').write_text(r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};
use wasm_validator::ValidationError;

fn push_u32(bytes: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 { byte |= 0x80; }
        bytes.push(byte);
        if value == 0 { break; }
    }
}
fn push_section(module: &mut Vec<u8>, id: u8, payload: &[u8]) {
    module.push(id); push_u32(module, payload.len() as u32); module.extend_from_slice(payload);
}
fn module(result: u8, instructions: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x00,0x61,0x73,0x6d,0x01,0x00,0x00,0x00];
    push_section(&mut bytes, 1, &[0x01,0x60,0x00,0x01,result]);
    push_section(&mut bytes, 3, &[0x01,0x00]);
    push_section(&mut bytes, 5, &[0x01,0x00,0x01]);
    push_section(&mut bytes, 7, &[0x01,0x03,b'r',b'u',b'n',0x00,0x00]);
    let mut body = vec![0x00]; body.extend_from_slice(instructions); body.push(0x0b);
    let mut code=vec![0x01]; push_u32(&mut code, body.len() as u32); code.extend(body); push_section(&mut bytes,10,&code);
    let data=[1u8,2,3,4,5,6,7,8];
    let mut ds=vec![0x01,0x00,0x41,0x00,0x0b,data.len() as u8]; ds.extend_from_slice(&data); push_section(&mut bytes,11,&ds);
    bytes
}
fn zero_v128(i: &mut Vec<u8>) { i.extend_from_slice(&[0xfd,0x0c]); i.extend_from_slice(&[0;16]); }
fn invoke(result: u8, sub: u8, align: u8, lane: u8, extract: u8) -> Value {
    let mut i=vec![0x41,0x00]; zero_v128(&mut i); i.extend_from_slice(&[0xfd,sub,align,0x00,lane,0xfd,extract,lane]);
    let parsed=parse_module(&module(result,&i)).expect("lane-load fixture parses");
    let mut instance=Instance::new(parsed).expect("lane-load fixture validates");
    instance.invoke_export_values("run",&[]).expect("lane-load executes").remove(0)
}
#[test]
fn simd_lane_load_widths_execute() {
    assert_eq!(invoke(0x7f,0x54,0,7,0x16), Value::I32(1));
    assert_eq!(invoke(0x7f,0x55,1,3,0x19), Value::I32(0x0201));
    assert_eq!(invoke(0x7f,0x56,2,2,0x1b), Value::I32(0x04030201));
    assert_eq!(invoke(0x7e,0x57,3,1,0x1d), Value::I64(0x0807060504030201));
}
#[test]
fn validator_rejects_lane_load_out_of_bounds() {
    let mut i=vec![0x41,0x00]; zero_v128(&mut i); i.extend_from_slice(&[0xfd,0x54,0x00,0x00,16]);
    let parsed=parse_module(&module(0x7b,&i)).expect("invalid lane fixture parses");
    assert!(matches!(Instance::new(parsed), Err(RuntimeError::Validation(ValidationError::MalformedImmediate{..}))));
}
#[test]
fn validator_rejects_lane_load_type_confusion() {
    let i=[0x41,0x00,0x41,0x00,0xfd,0x54,0x00,0x00,0x00];
    let parsed=parse_module(&module(0x7b,&i)).expect("type-confusion fixture parses");
    assert!(matches!(Instance::new(parsed), Err(RuntimeError::Validation(ValidationError::TypeMismatch{..}))));
}
''')

Path('differential/tests/simd_lane_memory_loads.rs').write_text(r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const FIXTURE: &str = r#"(module
  (memory 1) (data (i32.const 0) "\01\02\03\04\05\06\07\08")
  (func (export "l8") (result i32) i32.const 0 v128.const i32x4 0 0 0 0 v128.load8_lane 7 i8x16.extract_lane_u 7)
  (func (export "l16") (result i32) i32.const 0 v128.const i32x4 0 0 0 0 v128.load16_lane 3 i16x8.extract_lane_u 3)
  (func (export "l32") (result i32) i32.const 0 v128.const i32x4 0 0 0 0 v128.load32_lane 2 i32x4.extract_lane 2)
  (func (export "l64") (result i64) i32.const 0 v128.const i32x4 0 0 0 0 v128.load64_lane 1 i64x2.extract_lane 1))"#;

#[test]
fn simd_lane_loads_match_wasmtime_reference() {
    let bytes=wat::parse_str(FIXTURE).expect("WAT parses");
    let module=parse_module(&bytes).expect("mini parses");
    let mut mini=MiniInstance::new(module).expect("mini instantiates");
    let mini_i32 = |instance: &mut MiniInstance, name: &str| match instance.invoke_export_values(name,&[]).expect("mini executes").as_slice(){[Value::I32(v)]=>*v,other=>panic!("unexpected {other:?}")};
    assert_eq!(mini_i32(&mut mini,"l8"),1);
    assert_eq!(mini_i32(&mut mini,"l16"),0x0201);
    assert_eq!(mini_i32(&mut mini,"l32"),0x04030201);
    let mini64=match mini.invoke_export_values("l64",&[]).expect("mini l64").as_slice(){[Value::I64(v)]=>*v,other=>panic!("unexpected {other:?}")};
    assert_eq!(mini64,0x0807060504030201);

    let mut cfg=Config::new(); cfg.wasm_simd(true); let engine=Engine::new(&cfg).unwrap();
    let module=ReferenceModule::new(&engine,&bytes).unwrap(); let mut store=Store::new(&engine,()); let instance=ReferenceInstance::new(&mut store,&module,&[]).unwrap();
    assert_eq!(instance.get_typed_func::<(),i32>(&mut store,"l8").unwrap().call(&mut store,()).unwrap(),1);
    assert_eq!(instance.get_typed_func::<(),i32>(&mut store,"l16").unwrap().call(&mut store,()).unwrap(),0x0201);
    assert_eq!(instance.get_typed_func::<(),i32>(&mut store,"l32").unwrap().call(&mut store,()).unwrap(),0x04030201);
    assert_eq!(instance.get_typed_func::<(),i64>(&mut store,"l64").unwrap().call(&mut store,()).unwrap(),mini64);
}
''')
