from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    if text.count(old) != 1:
        raise SystemExit(f"expected exactly one match in {path}: {old[:80]!r}, got {text.count(old)}")
    p.write_text(text.replace(old, new, 1))

runtime = "crates/wasm-runtime/src/lib.rs"
validator = "crates/wasm-validator/src/typed.rs"

replace_once(runtime, '''        17 => {
            let scalar = numeric::i32_from_stack(stack)?;
            let lane = scalar.to_le_bytes();
            let mut bytes = [0u8; 16];
            for chunk in bytes.chunks_exact_mut(4) {
                chunk.copy_from_slice(&lane);
            }
            stack.push(Value::V128(Rc::new(bytes)));
        }
''', '''        17 => {
            let scalar = numeric::i32_from_stack(stack)?;
            let lane = scalar.to_le_bytes();
            let mut bytes = [0u8; 16];
            for chunk in bytes.chunks_exact_mut(4) {
                chunk.copy_from_slice(&lane);
            }
            stack.push(Value::V128(Rc::new(bytes)));
        }
        18 => {
            let scalar = numeric::i64_from_stack(stack)?;
            let lane = scalar.to_le_bytes();
            let mut bytes = [0u8; 16];
            for chunk in bytes.chunks_exact_mut(8) {
                chunk.copy_from_slice(&lane);
            }
            stack.push(Value::V128(Rc::new(bytes)));
        }
        19 => {
            let scalar = match numeric::pop_typed(stack, ValueType::F32)? {
                Value::F32(value) => value,
                _ => unreachable!("pop_typed established f32"),
            };
            let lane = scalar.to_bits().to_le_bytes();
            let mut bytes = [0u8; 16];
            for chunk in bytes.chunks_exact_mut(4) {
                chunk.copy_from_slice(&lane);
            }
            stack.push(Value::V128(Rc::new(bytes)));
        }
        20 => {
            let scalar = match numeric::pop_typed(stack, ValueType::F64)? {
                Value::F64(value) => value,
                _ => unreachable!("pop_typed established f64"),
            };
            let lane = scalar.to_bits().to_le_bytes();
            let mut bytes = [0u8; 16];
            for chunk in bytes.chunks_exact_mut(8) {
                chunk.copy_from_slice(&lane);
            }
            stack.push(Value::V128(Rc::new(bytes)));
        }
''')

replace_once(runtime, '''        27 => {
            let lane = *code.get(*pc).ok_or(RuntimeError::ControlInvariant(
                "validated i32x4.extract_lane immediate is missing",
            ))?;
            *pc += 1;
            if lane >= 4 {
                return Err(RuntimeError::ControlInvariant(
                    "validated i32x4.extract_lane lane is out of bounds",
                ));
            }
            let vector = numeric::v128_from_stack(stack)?;
            let start = usize::from(lane) * 4;
            let value = i32::from_le_bytes(
                vector[start..start + 4]
                    .try_into()
                    .expect("i32x4 lane width"),
            );
            stack.push(Value::I32(value));
        }
''', '''        27 => {
            let lane = *code.get(*pc).ok_or(RuntimeError::ControlInvariant(
                "validated i32x4.extract_lane immediate is missing",
            ))?;
            *pc += 1;
            if lane >= 4 {
                return Err(RuntimeError::ControlInvariant(
                    "validated i32x4.extract_lane lane is out of bounds",
                ));
            }
            let vector = numeric::v128_from_stack(stack)?;
            let start = usize::from(lane) * 4;
            let value = i32::from_le_bytes(
                vector[start..start + 4]
                    .try_into()
                    .expect("i32x4 lane width"),
            );
            stack.push(Value::I32(value));
        }
        28 => {
            let lane = *code.get(*pc).ok_or(RuntimeError::ControlInvariant(
                "validated i32x4.replace_lane immediate is missing",
            ))?;
            *pc += 1;
            if lane >= 4 {
                return Err(RuntimeError::ControlInvariant(
                    "validated i32x4.replace_lane lane is out of bounds",
                ));
            }
            let scalar = numeric::i32_from_stack(stack)?;
            let mut vector = numeric::v128_from_stack(stack)?;
            let start = usize::from(lane) * 4;
            vector[start..start + 4].copy_from_slice(&scalar.to_le_bytes());
            stack.push(Value::V128(Rc::new(vector)));
        }
        29 => {
            let lane = *code.get(*pc).ok_or(RuntimeError::ControlInvariant(
                "validated i64x2.extract_lane immediate is missing",
            ))?;
            *pc += 1;
            if lane >= 2 {
                return Err(RuntimeError::ControlInvariant(
                    "validated i64x2.extract_lane lane is out of bounds",
                ));
            }
            let vector = numeric::v128_from_stack(stack)?;
            let start = usize::from(lane) * 8;
            let value = i64::from_le_bytes(vector[start..start + 8].try_into().expect("i64x2 lane width"));
            stack.push(Value::I64(value));
        }
        30 => {
            let lane = *code.get(*pc).ok_or(RuntimeError::ControlInvariant(
                "validated i64x2.replace_lane immediate is missing",
            ))?;
            *pc += 1;
            if lane >= 2 {
                return Err(RuntimeError::ControlInvariant(
                    "validated i64x2.replace_lane lane is out of bounds",
                ));
            }
            let scalar = numeric::i64_from_stack(stack)?;
            let mut vector = numeric::v128_from_stack(stack)?;
            let start = usize::from(lane) * 8;
            vector[start..start + 8].copy_from_slice(&scalar.to_le_bytes());
            stack.push(Value::V128(Rc::new(vector)));
        }
        31 => {
            let lane = *code.get(*pc).ok_or(RuntimeError::ControlInvariant(
                "validated f32x4.extract_lane immediate is missing",
            ))?;
            *pc += 1;
            if lane >= 4 {
                return Err(RuntimeError::ControlInvariant(
                    "validated f32x4.extract_lane lane is out of bounds",
                ));
            }
            let vector = numeric::v128_from_stack(stack)?;
            let start = usize::from(lane) * 4;
            let bits = u32::from_le_bytes(vector[start..start + 4].try_into().expect("f32x4 lane width"));
            stack.push(Value::F32(f32::from_bits(bits)));
        }
        32 => {
            let lane = *code.get(*pc).ok_or(RuntimeError::ControlInvariant(
                "validated f32x4.replace_lane immediate is missing",
            ))?;
            *pc += 1;
            if lane >= 4 {
                return Err(RuntimeError::ControlInvariant(
                    "validated f32x4.replace_lane lane is out of bounds",
                ));
            }
            let scalar = match numeric::pop_typed(stack, ValueType::F32)? {
                Value::F32(value) => value,
                _ => unreachable!("pop_typed established f32"),
            };
            let mut vector = numeric::v128_from_stack(stack)?;
            let start = usize::from(lane) * 4;
            vector[start..start + 4].copy_from_slice(&scalar.to_bits().to_le_bytes());
            stack.push(Value::V128(Rc::new(vector)));
        }
        33 => {
            let lane = *code.get(*pc).ok_or(RuntimeError::ControlInvariant(
                "validated f64x2.extract_lane immediate is missing",
            ))?;
            *pc += 1;
            if lane >= 2 {
                return Err(RuntimeError::ControlInvariant(
                    "validated f64x2.extract_lane lane is out of bounds",
                ));
            }
            let vector = numeric::v128_from_stack(stack)?;
            let start = usize::from(lane) * 8;
            let bits = u64::from_le_bytes(vector[start..start + 8].try_into().expect("f64x2 lane width"));
            stack.push(Value::F64(f64::from_bits(bits)));
        }
        34 => {
            let lane = *code.get(*pc).ok_or(RuntimeError::ControlInvariant(
                "validated f64x2.replace_lane immediate is missing",
            ))?;
            *pc += 1;
            if lane >= 2 {
                return Err(RuntimeError::ControlInvariant(
                    "validated f64x2.replace_lane lane is out of bounds",
                ));
            }
            let scalar = match numeric::pop_typed(stack, ValueType::F64)? {
                Value::F64(value) => value,
                _ => unreachable!("pop_typed established f64"),
            };
            let mut vector = numeric::v128_from_stack(stack)?;
            let start = usize::from(lane) * 8;
            vector[start..start + 8].copy_from_slice(&scalar.to_bits().to_le_bytes());
            stack.push(Value::V128(Rc::new(vector)));
        }
''')

replace_once(runtime, '''                    | 15
                    | 16
                    | 17
''', '''                    | 15
                    | 16
                    | 17
                    | 18
                    | 19
                    | 20
''')
replace_once(runtime, '''                    27 => {
                        let lane = *code.get(pc).ok_or(RuntimeError::ControlInvariant(
                            "validated i32x4.extract_lane immediate is missing while scanning control",
                        ))?;
                        pc += 1;
                        if lane >= 4 {
                            return Err(RuntimeError::ControlInvariant(
                                "validated i32x4.extract_lane lane is out of bounds while scanning control",
                            ));
                        }
                    }
''', '''                    27 | 28 | 31 | 32 => {
                        let lane = *code.get(pc).ok_or(RuntimeError::ControlInvariant(
                            "validated four-lane SIMD immediate is missing while scanning control",
                        ))?;
                        pc += 1;
                        if lane >= 4 {
                            return Err(RuntimeError::ControlInvariant(
                                "validated four-lane SIMD lane is out of bounds while scanning control",
                            ));
                        }
                    }
                    29 | 30 | 33 | 34 => {
                        let lane = *code.get(pc).ok_or(RuntimeError::ControlInvariant(
                            "validated two-lane SIMD immediate is missing while scanning control",
                        ))?;
                        pc += 1;
                        if lane >= 2 {
                            return Err(RuntimeError::ControlInvariant(
                                "validated two-lane SIMD lane is out of bounds while scanning control",
                            ));
                        }
                    }
''')

replace_once(validator, '''                    15..=17 => {
                        pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;
                        stack.push(ValueType::V128);
                    }
''', '''                    15..=17 => {
                        pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;
                        stack.push(ValueType::V128);
                    }
                    18 => {
                        pop_expect(&mut stack, &controls, ValueType::I64, function, offset)?;
                        stack.push(ValueType::V128);
                    }
                    19 => {
                        pop_expect(&mut stack, &controls, ValueType::F32, function, offset)?;
                        stack.push(ValueType::V128);
                    }
                    20 => {
                        pop_expect(&mut stack, &controls, ValueType::F64, function, offset)?;
                        stack.push(ValueType::V128);
                    }
''')
replace_once(validator, '''                    27 => {
                        let lane = *code
                            .get(pc)
                            .ok_or(ValidationError::MalformedImmediate { function, offset })?;
                        pc += 1;
                        if lane >= 4 {
                            return Err(ValidationError::MalformedImmediate { function, offset });
                        }
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        stack.push(ValueType::I32);
                    }
''', '''                    27 => {
                        let lane = *code
                            .get(pc)
                            .ok_or(ValidationError::MalformedImmediate { function, offset })?;
                        pc += 1;
                        if lane >= 4 {
                            return Err(ValidationError::MalformedImmediate { function, offset });
                        }
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        stack.push(ValueType::I32);
                    }
                    28 => {
                        let lane = *code.get(pc).ok_or(ValidationError::MalformedImmediate { function, offset })?;
                        pc += 1;
                        if lane >= 4 { return Err(ValidationError::MalformedImmediate { function, offset }); }
                        pop_expect(&mut stack, &controls, ValueType::I32, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        stack.push(ValueType::V128);
                    }
                    29 | 31 | 33 => {
                        let lane = *code.get(pc).ok_or(ValidationError::MalformedImmediate { function, offset })?;
                        pc += 1;
                        let (limit, result) = match subopcode {
                            29 => (2, ValueType::I64),
                            31 => (4, ValueType::F32),
                            33 => (2, ValueType::F64),
                            _ => unreachable!(),
                        };
                        if lane >= limit { return Err(ValidationError::MalformedImmediate { function, offset }); }
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        stack.push(result);
                    }
                    30 | 32 | 34 => {
                        let lane = *code.get(pc).ok_or(ValidationError::MalformedImmediate { function, offset })?;
                        pc += 1;
                        let (limit, scalar) = match subopcode {
                            30 => (2, ValueType::I64),
                            32 => (4, ValueType::F32),
                            34 => (2, ValueType::F64),
                            _ => unreachable!(),
                        };
                        if lane >= limit { return Err(ValidationError::MalformedImmediate { function, offset }); }
                        pop_expect(&mut stack, &controls, scalar, function, offset)?;
                        pop_expect(&mut stack, &controls, ValueType::V128, function, offset)?;
                        stack.push(ValueType::V128);
                    }
''')

Path("crates/wasm-runtime/tests/simd_remaining_lanes.rs").write_text(r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};
use wasm_validator::ValidationError;

fn push_u32(bytes: &mut Vec<u8>, mut value: u32) {
    loop { let mut b = (value & 0x7f) as u8; value >>= 7; if value != 0 { b |= 0x80; } bytes.push(b); if value == 0 { break; } }
}
fn section(m: &mut Vec<u8>, id: u8, p: &[u8]) { m.push(id); push_u32(m, p.len() as u32); m.extend_from_slice(p); }
fn module(result: u8, body: &[u8]) -> Vec<u8> {
    let mut m = vec![0,0x61,0x73,0x6d,1,0,0,0];
    section(&mut m,1,&[1,0x60,0,1,result]); section(&mut m,3,&[1,0]); section(&mut m,7,&[1,3,b'r',b'u',b'n',0,0]);
    let mut b=vec![0]; b.extend_from_slice(body); b.push(0x0b); let mut c=vec![1]; push_u32(&mut c,b.len() as u32); c.extend(b); section(&mut m,10,&c); m
}
fn simd(v:&mut Vec<u8>, op:u8){v.extend_from_slice(&[0xfd,op]);}
fn invoke(result:u8, body:&[u8])->Value { let mut i=Instance::new(parse_module(&module(result,body)).unwrap()).unwrap(); i.invoke_export_values("run",&[]).unwrap().remove(0) }

#[test] fn i32x4_replace_lane_executes() {
    let mut b=vec![0x41,7]; simd(&mut b,17); b.extend_from_slice(&[0x41,42]); simd(&mut b,28); b.push(2); simd(&mut b,27); b.push(2);
    assert!(matches!(invoke(0x7f,&b), Value::I32(42)));
}
#[test] fn i64x2_splat_extract_replace_execute() {
    let mut b=vec![0x42,7]; simd(&mut b,18); b.extend_from_slice(&[0x42,0x2a]); simd(&mut b,30); b.push(1); simd(&mut b,29); b.push(1);
    assert!(matches!(invoke(0x7e,&b), Value::I64(42)));
}
#[test] fn float_lane_ops_preserve_bits() {
    let f32_bits=0x7fc0_1234u32; let mut b=vec![0x43]; b.extend_from_slice(&f32_bits.to_le_bytes()); simd(&mut b,19); simd(&mut b,31); b.push(3);
    match invoke(0x7d,&b) { Value::F32(v)=>assert_eq!(v.to_bits(),f32_bits), x=>panic!("{x:?}") }
    let f64_bits=0x7ff8_0000_0000_1234u64; let mut b=vec![0x44]; b.extend_from_slice(&f64_bits.to_le_bytes()); simd(&mut b,20); simd(&mut b,33); b.push(1);
    match invoke(0x7c,&b) { Value::F64(v)=>assert_eq!(v.to_bits(),f64_bits), x=>panic!("{x:?}") }
}
#[test] fn lane_ops_work_inside_structured_control() {
    let mut b=vec![0x02,0x7e,0x42,9]; simd(&mut b,18); simd(&mut b,29); b.push(1); b.push(0x0b);
    assert!(matches!(invoke(0x7e,&b), Value::I64(9)));
}
#[test] fn validator_rejects_invalid_lane_and_scalar_type() {
    let mut bad_lane=vec![0x42,0]; simd(&mut bad_lane,18); simd(&mut bad_lane,29); bad_lane.push(2);
    let parsed=parse_module(&module(0x7e,&bad_lane)).unwrap();
    assert!(matches!(Instance::new(parsed),Err(RuntimeError::Validation(ValidationError::MalformedImmediate{..}))));
    let mut bad_type=vec![0x41,0]; simd(&mut bad_type,18);
    let parsed=parse_module(&module(0x7b,&bad_type)).unwrap();
    assert!(matches!(Instance::new(parsed),Err(RuntimeError::Validation(ValidationError::TypeMismatch{..}))));
}
''')

Path("differential/tests/simd_remaining_lanes.rs").write_text(r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};

const WAT: &str = r#"(module
  (func (export "i32_replace") (result i32) i32.const 7 i32x4.splat i32.const 42 i32x4.replace_lane 2 i32x4.extract_lane 2)
  (func (export "i64_lane") (result i64) i64.const 7 i64x2.splat i64.const 42 i64x2.replace_lane 1 i64x2.extract_lane 1)
  (func (export "f32_lane") (result f32) f32.const 1.5 f32x4.splat f32.const -2.25 f32x4.replace_lane 3 f32x4.extract_lane 3)
  (func (export "f64_lane") (result f64) f64.const 1.5 f64x2.splat f64.const -2.25 f64x2.replace_lane 1 f64x2.extract_lane 1)
)"#;

#[test]
fn remaining_lane_primitives_match_wasmtime() {
    let bytes=wat::parse_str(WAT).unwrap();
    let module=parse_module(&bytes).unwrap(); let mut mini=MiniInstance::new(module).unwrap();
    let mut config=Config::new(); config.wasm_simd(true); let engine=Engine::new(&config).unwrap(); let rm=ReferenceModule::new(&engine,&bytes).unwrap(); let mut store=Store::new(&engine,()); let r=ReferenceInstance::new(&mut store,&rm,&[]).unwrap();
    assert_eq!(mini.invoke_export_values("i32_replace",&[]).unwrap(), vec![Value::I32(r.get_typed_func::<(),i32>(&mut store,"i32_replace").unwrap().call(&mut store,()).unwrap())]);
    assert_eq!(mini.invoke_export_values("i64_lane",&[]).unwrap(), vec![Value::I64(r.get_typed_func::<(),i64>(&mut store,"i64_lane").unwrap().call(&mut store,()).unwrap())]);
    let mf=mini.invoke_export_values("f32_lane",&[]).unwrap(); let rf=r.get_typed_func::<(),f32>(&mut store,"f32_lane").unwrap().call(&mut store,()).unwrap(); match mf.as_slice(){[Value::F32(v)]=>assert_eq!(v.to_bits(),rf.to_bits()),x=>panic!("{x:?}")}
    let mf=mini.invoke_export_values("f64_lane",&[]).unwrap(); let rf=r.get_typed_func::<(),f64>(&mut store,"f64_lane").unwrap().call(&mut store,()).unwrap(); match mf.as_slice(){[Value::F64(v)]=>assert_eq!(v.to_bits(),rf.to_bits()),x=>panic!("{x:?}")}
}
''')
