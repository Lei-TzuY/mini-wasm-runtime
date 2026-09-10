from pathlib import Path

runtime = Path('crates/wasm-runtime/src/lib.rs')
text = runtime.read_text()
old = '''        228..=231 => {
            let rhs = numeric::v128_from_stack(stack)?;
            let lhs = numeric::v128_from_stack(stack)?;
            let mut result = [0u8; 16];
            for lane in 0..4 {
                let start = lane * 4;
                let lhs_lane = f32::from_bits(u32::from_le_bytes(
                    lhs[start..start + 4].try_into().expect("f32x4 lane width"),
                ));
                let rhs_lane = f32::from_bits(u32::from_le_bytes(
                    rhs[start..start + 4].try_into().expect("f32x4 lane width"),
                ));
                let output = match subopcode {
                    228 => lhs_lane + rhs_lane,
                    229 => lhs_lane - rhs_lane,
                    230 => lhs_lane * rhs_lane,
                    231 => lhs_lane / rhs_lane,
                    _ => unreachable!("matched f32x4 binary arithmetic opcode"),
                };
                result[start..start + 4].copy_from_slice(&output.to_bits().to_le_bytes());
            }
            stack.push(Value::V128(Rc::new(result)));
        }
'''
new = '''        228..=233 => {
            let rhs = numeric::v128_from_stack(stack)?;
            let lhs = numeric::v128_from_stack(stack)?;
            let mut result = [0u8; 16];
            for lane in 0..4 {
                let start = lane * 4;
                let lhs_lane = f32::from_bits(u32::from_le_bytes(
                    lhs[start..start + 4].try_into().expect("f32x4 lane width"),
                ));
                let rhs_lane = f32::from_bits(u32::from_le_bytes(
                    rhs[start..start + 4].try_into().expect("f32x4 lane width"),
                ));
                let output = match subopcode {
                    228 => lhs_lane + rhs_lane,
                    229 => lhs_lane - rhs_lane,
                    230 => lhs_lane * rhs_lane,
                    231 => lhs_lane / rhs_lane,
                    232 => {
                        if lhs_lane.is_nan() || rhs_lane.is_nan() {
                            f32::NAN
                        } else if lhs_lane == 0.0 && rhs_lane == 0.0 {
                            f32::from_bits(lhs_lane.to_bits() | rhs_lane.to_bits())
                        } else if lhs_lane < rhs_lane {
                            lhs_lane
                        } else {
                            rhs_lane
                        }
                    }
                    233 => {
                        if lhs_lane.is_nan() || rhs_lane.is_nan() {
                            f32::NAN
                        } else if lhs_lane == 0.0 && rhs_lane == 0.0 {
                            f32::from_bits(lhs_lane.to_bits() & rhs_lane.to_bits())
                        } else if lhs_lane > rhs_lane {
                            lhs_lane
                        } else {
                            rhs_lane
                        }
                    }
                    _ => unreachable!("matched f32x4 binary numeric opcode"),
                };
                result[start..start + 4].copy_from_slice(&output.to_bits().to_le_bytes());
            }
            stack.push(Value::V128(Rc::new(result)));
        }
'''
assert old in text, 'runtime arithmetic block drifted'
text = text.replace(old, new, 1)
text = text.replace('| 228..=231\n', '| 228..=233\n')
runtime.write_text(text)

validator = Path('crates/wasm-validator/src/typed.rs')
vtext = validator.read_text()
assert '| 228..=231 =>' in vtext, 'validator SIMD group drifted'
vtext = vtext.replace('| 228..=231 =>', '| 228..=233 =>')
validator.write_text(vtext)

for path in Path('crates/wasm-runtime/tests').glob('*.rs'):
    t = path.read_text()
    if 'subopcode: 232' in t:
        t = t.replace('push_simd(&mut instructions, 232);', 'push_simd(&mut instructions, 234);')
        t = t.replace('subopcode: 232', 'subopcode: 234')
        t = t.replace('f32x4_min_frontier', 'f32x4_pmin_frontier')
        path.write_text(t)

Path('crates/wasm-runtime/tests/simd_f32x4_minmax.rs').write_text(r'''use wasm_parser::parse_module;
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
fn module(instructions: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x00,0x61,0x73,0x6d,0x01,0,0,0];
    push_section(&mut bytes,1,&[0x01,0x60,0x00,0x01,0x7f]);
    push_section(&mut bytes,3,&[0x01,0x00]);
    push_section(&mut bytes,5,&[0x01,0x00,0x01]);
    push_section(&mut bytes,7,&[0x01,0x03,b'r',b'u',b'n',0x00,0x00]);
    let mut body=vec![0x00]; body.extend_from_slice(instructions); body.push(0x0b);
    let mut code=vec![0x01]; push_u32(&mut code,body.len() as u32); code.extend(body);
    push_section(&mut bytes,10,&code); bytes
}
fn push_simd(i:&mut Vec<u8>, subopcode:u32){i.push(0xfd);push_u32(i,subopcode);}
fn push_f32x4_const(i:&mut Vec<u8>, lanes:[f32;4]){push_simd(i,12);for lane in lanes{i.extend_from_slice(&lane.to_bits().to_le_bytes());}}
fn push_i32_const(i:&mut Vec<u8>, value:i32){i.push(0x41);let mut value=value;loop{let byte=(value as u8)&0x7f;value>>=7;let sign=byte&0x40!=0;let done=(value==0&&!sign)||(value==-1&&sign);i.push(if done{byte}else{byte|0x80});if done{break;}}}
fn lane_bits(lhs:[f32;4],rhs:[f32;4],subopcode:u32,lane:u32)->u32{
    let mut i=Vec::new();push_i32_const(&mut i,0);push_f32x4_const(&mut i,lhs);push_f32x4_const(&mut i,rhs);push_simd(&mut i,subopcode);push_simd(&mut i,11);i.extend_from_slice(&[4,0]);push_i32_const(&mut i,0);i.push(0x28);i.push(2);push_u32(&mut i,lane*4);
    let parsed=parse_module(&module(&i)).expect("fixture parses");let mut instance=Instance::new(parsed).expect("fixture validates");match instance.invoke_export_values("run",&[]).expect("executes").as_slice(){[Value::I32(v)]=>*v as u32,other=>panic!("unexpected result: {other:?}")}
}
#[test]
fn f32x4_min_max_cover_order_and_signed_zero(){
    assert_eq!(lane_bits([3.0,-2.0,8.0,1.0],[4.0,-5.0,7.0,2.0],232,0),3.0f32.to_bits());
    assert_eq!(lane_bits([3.0,-2.0,8.0,1.0],[4.0,-5.0,7.0,2.0],233,2),8.0f32.to_bits());
    assert_eq!(lane_bits([0.0;4],[-0.0;4],232,0),(-0.0f32).to_bits());
    assert_eq!(lane_bits([-0.0;4],[0.0;4],233,0),0.0f32.to_bits());
}
#[test]
fn f32x4_min_max_propagate_nan(){
    let min=f32::from_bits(lane_bits([f32::NAN;4],[1.0;4],232,0));
    let max=f32::from_bits(lane_bits([1.0;4],[f32::NAN;4],233,0));
    assert!(min.is_nan()); assert!(max.is_nan());
}
#[test]
fn validator_rejects_f32x4_min_type_confusion(){
    let mut i=Vec::new();push_f32x4_const(&mut i,[1.0;4]);push_i32_const(&mut i,1);push_simd(&mut i,232);
    let parsed=parse_module(&module(&i)).expect("fixture parses");
    assert!(matches!(Instance::new(parsed),Err(RuntimeError::Validation(ValidationError::TypeMismatch{..}))));
}
#[test]
fn adjacent_f32x4_pmin_frontier_remains_fail_closed(){
    let mut i=Vec::new();push_f32x4_const(&mut i,[1.0;4]);push_f32x4_const(&mut i,[2.0;4]);push_simd(&mut i,234);
    let parsed=parse_module(&module(&i)).expect("fixture parses");
    assert!(matches!(Instance::new(parsed),Err(RuntimeError::Validation(ValidationError::UnsupportedPrefixedOpcode{prefix:0xfd,subopcode:234,..}))));
}
''')

Path('differential/tests/simd_f32x4_minmax.rs').write_text(r'''use wasm_runtime::{Instance, Value};
use wasmtime::{Engine, Instance as WasmtimeInstance, Module as WasmtimeModule, Store};

const FIXTURE:&str=r#"(module
 (memory 1)
 (func (export "min") (result i32) i32.const 0 v128.const f32x4 3 -2 8 1 v128.const f32x4 4 -5 7 2 f32x4.min v128.store i32.const 0 i32.load)
 (func (export "max") (result i32) i32.const 0 v128.const f32x4 3 -2 8 1 v128.const f32x4 4 -5 7 2 f32x4.max v128.store i32.const 0 i32.load offset=8))"#;
#[test]
fn f32x4_minmax_matches_wasmtime_reference(){
 let bytes=wat::parse_str(FIXTURE).expect("wat");
 let parsed=wasm_parser::parse_module(&bytes).expect("parse"); let mut mini=Instance::new(parsed).expect("mini");
 let mini_min=match mini.invoke_export_values("min",&[]).unwrap().as_slice(){[Value::I32(v)]=>*v,_=>panic!()};
 let mini_max=match mini.invoke_export_values("max",&[]).unwrap().as_slice(){[Value::I32(v)]=>*v,_=>panic!()};
 let engine=Engine::default(); let module=WasmtimeModule::new(&engine,&bytes).unwrap(); let mut store=Store::new(&engine,()); let instance=WasmtimeInstance::new(&mut store,&module,&[]).unwrap();
 let ref_min=instance.get_typed_func::<(),i32>(&mut store,"min").unwrap().call(&mut store,()).unwrap();
 let ref_max=instance.get_typed_func::<(),i32>(&mut store,"max").unwrap().call(&mut store,()).unwrap();
 assert_eq!((mini_min,mini_max),(ref_min,ref_max));
}
''')

roadmap=Path('docs/roadmap.md')
r=roadmap.read_text()
anchor='- SIMD `f32x4` binary arithmetic is executable for `add`, `sub`, `mul`, and `div`, with typed validation, structured-control scanning, regressions, and Wasmtime differential coverage.\n'
if anchor in r:
    r=r.replace(anchor,anchor+'- SIMD `f32x4` ordered `min`/`max` semantics are executable with NaN propagation, signed-zero handling, typed validation, regressions, and Wasmtime differential coverage; `pmin` remains fail-closed.\n',1)
else:
    r += '\n- SIMD `f32x4` ordered `min`/`max` semantics are executable with NaN propagation, signed-zero handling, typed validation, regressions, and Wasmtime differential coverage; `pmin` remains fail-closed.\n'
roadmap.write_text(r)
