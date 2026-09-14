from pathlib import Path

lib = Path('crates/wasm-runtime/src/lib.rs')
s = lib.read_text()
anchor = '        256 => {\n'
impl = '''        273 => {
            // i16x8.relaxed_q15mulr_s is implementation-defined only for
            // INT16_MIN * INT16_MIN. Choose the deterministic saturating result,
            // matching i16x8.q15mulr_sat_s for every lane.
            let rhs = numeric::v128_from_stack(stack)?;
            let lhs = numeric::v128_from_stack(stack)?;
            let mut result = [0u8; 16];
            for lane in 0..8 {
                let start = lane * 2;
                let lhs_lane = i16::from_le_bytes(
                    lhs[start..start + 2].try_into().expect("i16x8 lane width"),
                );
                let rhs_lane = i16::from_le_bytes(
                    rhs[start..start + 2].try_into().expect("i16x8 lane width"),
                );
                let product = i32::from(lhs_lane) * i32::from(rhs_lane);
                let rounded = (product + 0x4000) >> 15;
                let output = rounded.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
                result[start..start + 2].copy_from_slice(&output.to_le_bytes());
            }
            stack.push(Value::V128(Rc::new(result)));
        }
'''
assert anchor in s and '        273 => {' not in s
s = s.replace(anchor, impl + anchor, 1)
assert '240..=272' in s
s = s.replace('240..=272', '240..=273', 1)
lib.write_text(s)

val = Path('crates/wasm-validator/src/typed.rs')
s = val.read_text()
assert '269..=272 =>' in s
s = s.replace('269..=272 =>', '269..=273 =>', 1)
val.write_text(s)

for p in Path('crates/wasm-runtime/tests').glob('simd_*.rs'):
    s = p.read_text()
    if 'subopcode: 273' not in s:
        continue
    s = s.replace('subopcode: 273', 'subopcode: 274')
    s = s.replace(', 273);', ', 274);')
    p.write_text(s)

Path('crates/wasm-runtime/tests/simd_relaxed_q15mulr.rs').write_text(r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance, RuntimeError, Value};
use wasm_validator::ValidationError;

fn u32leb(out: &mut Vec<u8>, mut v: u32) { loop { let mut b=(v&0x7f) as u8; v >>= 7; if v!=0 { b|=0x80; } out.push(b); if v==0 { break; } } }
fn section(m:&mut Vec<u8>, id:u8, p:&[u8]) { m.push(id); u32leb(m,p.len() as u32); m.extend_from_slice(p); }
fn module(ins:&[u8]) -> Vec<u8> { let mut m=vec![0,97,115,109,1,0,0,0]; section(&mut m,1,&[1,0x60,0,1,0x7f]); section(&mut m,3,&[1,0]); section(&mut m,7,&[1,3,b'r',b'u',b'n',0,0]); let mut b=vec![0]; b.extend_from_slice(ins); b.push(0x0b); let mut c=vec![1]; u32leb(&mut c,b.len() as u32); c.extend(b); section(&mut m,10,&c); m }
fn simd(i:&mut Vec<u8>, op:u32) { i.push(0xfd); u32leb(i,op); }
fn splat(i:&mut Vec<u8>, x:i16) { simd(i,12); for _ in 0..8 { i.extend_from_slice(&x.to_le_bytes()); } }
fn lane(lhs:i16,rhs:i16)->i32 { let mut i=Vec::new(); splat(&mut i,lhs); splat(&mut i,rhs); simd(&mut i,273); simd(&mut i,24); i.push(0); let p=parse_module(&module(&i)).unwrap(); let mut x=Instance::new(p).unwrap(); match x.invoke_export_values("run",&[]).unwrap().as_slice(){[Value::I32(v)]=>*v,_=>panic!()} }
#[test] fn relaxed_q15mulr_executes_rounded_q15_lanes(){ assert_eq!(lane(16384,16384),8192); assert_eq!(lane(-16384,16384),-8192); }
#[test] fn relaxed_q15mulr_chooses_saturating_overflow_result(){ assert_eq!(lane(i16::MIN,i16::MIN),i16::MAX as i32); }
#[test] fn validator_rejects_relaxed_q15mulr_type_confusion(){ let mut i=Vec::new(); splat(&mut i,1); i.push(0x41); i.push(1); simd(&mut i,273); let p=parse_module(&module(&i)).unwrap(); assert!(matches!(Instance::new(p),Err(RuntimeError::Validation(ValidationError::TypeMismatch{..})))); }
#[test] fn next_relaxed_simd_frontier_remains_fail_closed(){ let mut i=Vec::new(); splat(&mut i,1); splat(&mut i,1); simd(&mut i,274); simd(&mut i,24); i.push(0); let p=parse_module(&module(&i)).unwrap(); assert!(matches!(Instance::new(p),Err(RuntimeError::Validation(ValidationError::UnsupportedPrefixedOpcode{prefix:0xfd,subopcode:274,..})))); }
''')

Path('differential/tests/simd_relaxed_q15mulr.rs').write_text(r'''use wasm_parser::parse_module;
use wasm_runtime::{Instance as MiniInstance, Value};
use wasmtime::{Config, Engine, Instance as ReferenceInstance, Module as ReferenceModule, Store};
const FIXTURE:&str=r#"(module (func (export "run") (result i32) v128.const i16x8 16384 0 0 0 0 0 0 0 v128.const i16x8 16384 0 0 0 0 0 0 0 i16x8.relaxed_q15mulr_s i16x8.extract_lane_s 0))"#;
#[test] fn relaxed_q15mulr_matches_wasmtime_for_defined_lane(){ let bytes=wat::parse_str(FIXTURE).unwrap(); let parsed=parse_module(&bytes).unwrap(); let mut mini=MiniInstance::new(parsed).unwrap(); let mv=match mini.invoke_export_values("run",&[]).unwrap().as_slice(){[Value::I32(v)]=>*v,_=>panic!()}; let mut c=Config::new(); c.wasm_simd(true); c.wasm_relaxed_simd(true); let e=Engine::new(&c).unwrap(); let m=ReferenceModule::new(&e,&bytes).unwrap(); let mut s=Store::new(&e,()); let x=ReferenceInstance::new(&mut s,&m,&[]).unwrap(); let rv=x.get_typed_func::<(),i32>(&mut s,"run").unwrap().call(&mut s,()).unwrap(); assert_eq!(mv,8192); assert_eq!(mv,rv); }
''')
