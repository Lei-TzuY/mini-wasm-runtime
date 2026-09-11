from pathlib import Path

p=Path('crates/wasm-runtime/src/lib.rs')
s=p.read_text()
old='''        240..=243 => {\n            let rhs = numeric::v128_from_stack(stack)?;'''
new='''        244..=245 => {\n            let rhs = numeric::v128_from_stack(stack)?;\n            let lhs = numeric::v128_from_stack(stack)?;\n            let mut result = [0u8; 16];\n            for lane in 0..2 {\n                let start = lane * 8;\n                let lhs_lane = f64::from_bits(u64::from_le_bytes(\n                    lhs[start..start + 8].try_into().expect("f64x2 lane width"),\n                ));\n                let rhs_lane = f64::from_bits(u64::from_le_bytes(\n                    rhs[start..start + 8].try_into().expect("f64x2 lane width"),\n                ));\n                let output = match subopcode {\n                    244 => {\n                        if lhs_lane.is_nan() || rhs_lane.is_nan() {\n                            f64::NAN\n                        } else if lhs_lane == 0.0 && rhs_lane == 0.0 {\n                            f64::from_bits(lhs_lane.to_bits() | rhs_lane.to_bits())\n                        } else if lhs_lane < rhs_lane { lhs_lane } else { rhs_lane }\n                    }\n                    245 => {\n                        if lhs_lane.is_nan() || rhs_lane.is_nan() {\n                            f64::NAN\n                        } else if lhs_lane == 0.0 && rhs_lane == 0.0 {\n                            f64::from_bits(lhs_lane.to_bits() & rhs_lane.to_bits())\n                        } else if lhs_lane > rhs_lane { lhs_lane } else { rhs_lane }\n                    }\n                    _ => unreachable!("matched f64x2 min max opcode"),\n                };\n                result[start..start + 8].copy_from_slice(&output.to_bits().to_le_bytes());\n            }\n            stack.push(Value::V128(Rc::new(result)));\n        }\n        240..=243 => {\n            let rhs = numeric::v128_from_stack(stack)?;'''
assert old in s
p.write_text(s.replace(old,new,1))

p=Path('crates/wasm-validator/src/typed.rs'); s=p.read_text(); old='''                    | 228..=235\n                    | 240..=243 => {'''; new='''                    | 228..=235\n                    | 240..=245 => {'''; assert old in s; p.write_text(s.replace(old,new,1))

p=Path('crates/wasm-runtime/src/lib.rs'); s=p.read_text(); old='''                    | 239\n                    | 240..=243\n                    | 142'''; new='''                    | 239\n                    | 240..=245\n                    | 142'''; assert old in s; p.write_text(s.replace(old,new,1))

# Move all explicit adjacent fail-closed frontier assertions from 244 to 246 in SIMD tests.
for p in Path('crates/wasm-runtime/tests').glob('simd_*.rs'):
    s=p.read_text()
    if '244' in s:
        p.write_text(s.replace('244', '246'))

Path('crates/wasm-runtime/tests/simd_f64x2_minmax.rs').write_text(r'''use std::rc::Rc;
use wasm_parser::{FunctionBody, FunctionType, Module, ValueType};
use wasm_runtime::{Instance, RuntimeError, Value};

fn uleb(mut n: u32) -> Vec<u8> { let mut out=Vec::new(); loop { let mut b=(n&0x7f) as u8; n >>= 7; if n!=0 { b|=0x80; } out.push(b); if n==0 { return out; } } }
fn module(code: Vec<u8>) -> Module { Module { types: vec![FunctionType{params:vec![],results:vec![ValueType::V128]}], function_type_indices:vec![0], codes:vec![FunctionBody{locals:vec![],code}], ..Module::default() } }
fn v128(bits:[u64;2])->Vec<u8>{ let mut x=vec![0xfd,12]; for b in bits { x.extend_from_slice(&b.to_le_bytes()); } x }
fn run(op:u32,a:[u64;2],b:[u64;2])->[u64;2]{ let mut c=v128(a); c.extend(v128(b)); c.push(0xfd); c.extend(uleb(op)); c.push(0x0b); let i=Instance::instantiate(Rc::new(module(c))).unwrap(); let r=i.invoke(0,&[]).unwrap(); let Value::V128(v)=&r[0] else { panic!() }; [u64::from_le_bytes(v[0..8].try_into().unwrap()),u64::from_le_bytes(v[8..16].try_into().unwrap())] }
#[test] fn ordered_and_signed_zero(){ assert_eq!(run(244,[3f64.to_bits(),0.0f64.to_bits()],[2f64.to_bits(),(-0.0f64).to_bits()]),[2f64.to_bits(),(-0.0f64).to_bits()]); assert_eq!(run(245,[3f64.to_bits(),0.0f64.to_bits()],[2f64.to_bits(),(-0.0f64).to_bits()]),[3f64.to_bits(),0.0f64.to_bits()]); }
#[test] fn nan_propagates(){ for op in [244,245] { assert!(f64::from_bits(run(op,[f64::NAN.to_bits(),1f64.to_bits()],[2f64.to_bits(),f64::NAN.to_bits()])[0]).is_nan()); assert!(f64::from_bits(run(op,[f64::NAN.to_bits(),1f64.to_bits()],[2f64.to_bits(),f64::NAN.to_bits()])[1]).is_nan()); } }
#[test] fn adjacent_pmin_stays_fail_closed(){ let mut c=v128([1f64.to_bits();2]); c.extend(v128([2f64.to_bits();2])); c.push(0xfd); c.extend(uleb(246)); c.push(0x0b); let e=Instance::instantiate(Rc::new(module(c))).unwrap_err(); assert!(matches!(e,RuntimeError::Validation(_))); }
''')

p=Path('docs/roadmap.md'); s=p.read_text(); old='''`f64x2.add`, `f64x2.sub`, `f64x2.mul`, and `f64x2.div` are executable with typed validation, structured-control handling, IEEE-754 edge regressions, and Wasmtime differential coverage; the adjacent `f64x2.min` opcode remains fail-closed.'''; new='''`f64x2.add`, `f64x2.sub`, `f64x2.mul`, and `f64x2.div` are executable with typed validation, structured-control handling, IEEE-754 edge regressions, and Wasmtime differential coverage; `f64x2.min` and `f64x2.max` are executable with ordered NaN propagation, signed-zero handling, typed validation, structured-control handling, and focused regressions; the adjacent `f64x2.pmin` opcode remains fail-closed.'''; assert old in s; p.write_text(s.replace(old,new,1))
