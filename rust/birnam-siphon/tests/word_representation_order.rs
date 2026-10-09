//! Batch decline retains public Word input and malformed-call error precedence.
mod support;
use birnam_abi::{Oop,borrowed_slice,id_protocol::{NativeEntry,Operand,Outcome}};
use std::{cell::{Cell,RefCell},ffi::c_void,os::unix::ffi::OsStrExt};
use support::*;
struct Hook { original:NativeEntry, calls:RefCell<Vec<Oop>>, fail:Cell<bool> }
unsafe extern "C" fn inspect(c:*mut c_void,r:Oop,a:*const Operand,n:usize)->Outcome{
 let h=unsafe{&*c.cast::<Hook>()};let args=unsafe{borrowed_slice(a,n)}.unwrap();h.calls.borrow_mut().push(args[0].as_object().unwrap());
 if h.fail.get(){return Outcome::failed(812,19);}
 unsafe{(h.original.method.unwrap())(h.original.context,r,a,n)}
}
#[test]
#[ignore="requires production Word owner"]
fn ordered_fallback_and_left_failure_precede_bad_right_carriers(){unsafe{
 let id=id();let ns=load(id,"bootstrap",&[Operand::bytes(directory().as_os_str().as_bytes())]);
 let named=|name:&[u8]|object(send(id,ns,b"at:",&[Operand::object(id.intern_name(name).unwrap())]));
 let word=named(b"Word");let roots=named(b"Roots");
 let a=object(send(id,word,b"fromBits:",&[Operand::word(7)]));let at=retain(id,roots,a);
 let b=object(send(id,word,b"fromBits:",&[Operand::word(9)]));let bt=retain(id,roots,b);
 let query=id.intern_name(b"tryRepresentationIdentityOf:behavior:bytes:").unwrap();
 let original=send(id,id.receiver,b"bind:to:",&[Operand::object(query),Operand::object(id.receiver)]).as_entry().unwrap();
 let mut h=Hook{original,calls:RefCell::new(Vec::new()),fail:Cell::new(false)};
 install(id,id.receiver,b"tryRepresentationIdentityOf:behavior:bytes:",inspect,(&raw mut h).cast());
 let answer=object(send(id,a,b"wrappingAdd:",&[Operand::object(b)]));let pin=retain(id,roots,answer);
 assert_eq!(&h.calls.borrow()[..2],&[a,b]);assert_eq!(send(id,answer,b"bits",&[]).as_word(),Some(16));
 h.calls.borrow_mut().clear();h.fail.set(true);
 for args in [vec![Operand::object(b)],vec![Operand::word(9)],vec![]]{
  let error=id.send_named(a,b"wrappingAdd:",&args).unwrap_err();assert_eq!((error.status,error.detail),(812,19));
  assert_eq!(&*h.calls.borrow(),&[a]);h.calls.borrow_mut().clear();
 }
 install(id,id.receiver,b"tryRepresentationIdentityOf:behavior:bytes:",original.method.unwrap(),original.context);
 for t in [pin,bt,at]{release(id,roots,t);}
}}
