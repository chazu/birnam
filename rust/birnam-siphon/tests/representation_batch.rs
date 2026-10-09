//! Id batches its own metadata, never the Word owner's payload.
mod support;
use birnam_abi::{Oop,id_protocol::{NativeEntry,Operand,Outcome}};
use std::{cell::Cell,ffi::c_void};
use support::*;
struct Hook { original:NativeEntry,calls:Cell<usize> }
unsafe extern "C" fn forward(c:*mut c_void,r:Oop,a:*const Operand,n:usize)->Outcome {
 let h=unsafe{&*c.cast::<Hook>()};h.calls.set(h.calls.get()+1);
 unsafe{(h.original.method.unwrap())(h.original.context,r,a,n)}
}
#[test]
#[ignore="requires current native Id"]
fn batch_facts_are_current_bounded_and_decline_replaced_dependencies(){unsafe{
 let id=id();let b=behavior(id);let a=allocate(id,b,8);let z=allocate(id,b,8);
 let batch=id.intern_name(b"tryRepresentationsOf:behavior:bytes:").unwrap();
 let values=[a.addr(),z.addr()];
 let query=|values:&[usize],behavior,size|{
  let bytes=core::slice::from_raw_parts(values.as_ptr().cast::<u8>(),size_of_val(values));
  id.send(id.receiver,batch,&[Operand::bytes(bytes),Operand::object(behavior),Operand::word(size)])
 };
 assert_eq!(query(&values,b,8).unwrap().as_word(),Some(1));
 assert_eq!(query(&values,b,9).unwrap().as_word(),Some(0));
 let aliases=[a.addr(),a.addr()];assert_eq!(query(&aliases,b,8).unwrap().as_word(),Some(1));
 let other=behavior(id);assert_eq!(query(&values,other,8).unwrap().as_word(),Some(0));
 let invalid=[a.addr(),7];assert_eq!(query(&invalid,b,8).unwrap().as_word(),Some(0));
 for count in [0,1,33]{assert_eq!(query(&vec![a.addr();count],b,8).unwrap_err().status,1);}
 assert_eq!(id.send(id.receiver,batch,&[Operand::bytes(&[0]),Operand::object(b),Operand::word(8)]).unwrap_err().status,1);
 let maximum=vec![a.addr();32];assert_eq!(query(&maximum,b,8).unwrap().as_word(),Some(1));
 let bind=|name:&[u8]|send(id,id.receiver,b"bind:to:",&[Operand::object(id.intern_name(name).unwrap()),Operand::object(id.receiver)]).as_entry().unwrap();
 for name in [b"tryRepresentationIdentityOf:behavior:bytes:".as_slice(),b"behaviorOf:",b"allocationSizeOf:",b"identityOf:",b"tryRepresentationsOf:behavior:bytes:"]{
  let old=bind(name);let mut h=Hook{original:old,calls:Cell::new(0)};
  install(id,id.receiver,name,forward,(&raw mut h).cast());
  assert_eq!(query(&values,b,8).unwrap().kind(),birnam_abi::id_protocol::UNIT);
  assert_eq!(h.calls.get(),usize::from(name==b"tryRepresentationsOf:behavior:bytes:"),"owner declines omitted getters without invoking them");
  install(id,id.receiver,name,old.method.unwrap(),old.context);
 }
 assert_eq!(query(&values,b,8).unwrap().as_word(),Some(1));
 send(id,id.receiver,b"reassociateBehaviorOf:with:",&[Operand::object(z),Operand::object(other)]);
 assert_eq!(query(&values,b,8).unwrap().as_word(),Some(0),"no shape verdict survives reassociation");
}}
