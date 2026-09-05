use wasm_parser::parse_module;
use wasm_runtime::{HostRegistry, Instance, RuntimeError, TableHandle};
use wasm_validator::ValidationError;

fn u32leb(out: &mut Vec<u8>, mut value: u32) { loop { let mut b=(value&0x7f) as u8; value >>= 7; if value!=0 { b|=0x80; } out.push(b); if value==0 { break; } } }
fn name(out:&mut Vec<u8>, s:&str){u32leb(out,s.len() as u32);out.extend_from_slice(s.as_bytes());}
fn section(m:&mut Vec<u8>,id:u8,p:&[u8]){m.push(id);u32leb(m,p.len() as u32);m.extend_from_slice(p);}
fn module(body:&[u8])->Vec<u8>{
 let mut m=b"\0asm\x01\0\0\0".to_vec(); section(&mut m,1,&[1,0x60,0,0]);
 let mut i=vec![1]; name(&mut i,"env"); name(&mut i,"tab"); i.extend([1,0x70,0,4]); section(&mut m,2,&i);
 section(&mut m,3,&[1,0]); section(&mut m,7,&[1,3,b'r',b'u',b'n',0,0]); section(&mut m,9,&[1,1,0,1,0]);
 let mut c=vec![1,(body.len()+1) as u8,0];c.extend_from_slice(body);section(&mut m,10,&c);m
}
fn hosts(t:&TableHandle)->HostRegistry{let mut h=HostRegistry::new();h.register_table("env","tab",t.clone()).unwrap();h}
fn init(dst:u8,b:&mut Vec<u8>){b.extend([0x41,dst,0x41,0,0x41,1,0xfc,12,0,0]);}
fn fill(dst:u8,len:u8,b:&mut Vec<u8>){b.extend([0x41,dst,0xd0,0x70,0x41,len,0xfc,17,0]);}
fn present(t:&TableHandle)->Vec<bool>{(0..t.len()).map(|i|t.get(i).unwrap().is_some()).collect()}

#[test] fn clears_range_in_imported_table(){let mut b=vec![];init(0,&mut b);init(1,&mut b);init(2,&mut b);fill(1,2,&mut b);b.push(0x0b);let t=TableHandle::new(4,Some(4)).unwrap();let mut vm=Instance::with_hosts(parse_module(&module(&b)).unwrap(),hosts(&t)).unwrap();vm.invoke_export("run",&[]).unwrap();assert_eq!(present(&t),vec![true,false,false,false]);}
#[test] fn zero_length_at_end_is_valid(){let mut b=vec![];fill(4,0,&mut b);b.push(0x0b);let t=TableHandle::new(4,Some(4)).unwrap();let mut vm=Instance::with_hosts(parse_module(&module(&b)).unwrap(),hosts(&t)).unwrap();vm.invoke_export("run",&[]).unwrap();assert_eq!(present(&t),vec![false;4]);}
#[test] fn oob_fill_is_atomic(){let mut b=vec![];init(0,&mut b);fill(3,2,&mut b);b.push(0x0b);let t=TableHandle::new(4,Some(4)).unwrap();let mut vm=Instance::with_hosts(parse_module(&module(&b)).unwrap(),hosts(&t)).unwrap();assert!(matches!(vm.invoke_export("run",&[]),Err(RuntimeError::TableElementOutOfBounds(_))));assert_eq!(present(&t),vec![true,false,false,false]);}
#[test] fn rejects_nonzero_table(){let b=[0x41,0,0xd0,0x70,0x41,0,0xfc,17,1,0x0b];let t=TableHandle::new(4,Some(4)).unwrap();assert!(matches!(Instance::with_hosts(parse_module(&module(&b)).unwrap(),hosts(&t)),Err(RuntimeError::Validation(ValidationError::TableIndexOutOfBounds{table_index:1,..}))));}
#[test] fn rejects_numeric_fill_value(){let b=[0x41,0,0x41,0,0x41,1,0xfc,17,0,0x0b];let t=TableHandle::new(4,Some(4)).unwrap();assert!(matches!(Instance::with_hosts(parse_module(&module(&b)).unwrap(),hosts(&t)),Err(RuntimeError::Validation(ValidationError::TypeMismatch{..}))));}
