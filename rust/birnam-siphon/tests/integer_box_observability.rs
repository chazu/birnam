//! Concrete counterexamples to unguarded Integer/Word conversion elimination.
mod support;
use birnam_abi::{Oop,borrowed_slice,id_protocol::{NativeEntry,Operand,Outcome,Protocol}};
use std::{cell::{Cell,RefCell},ffi::c_void,os::unix::ffi::OsStrExt};
use support::*;
struct Hook { id:Protocol, roots:Oop, original:NativeEntry, replace:Cell<bool>, calls:Cell<usize>, seen:RefCell<Vec<(Oop,usize)>> }
unsafe extern "C" fn constructor(c:*mut c_void,r:Oop,a:*const Operand,n:usize)->Outcome {
 let h=unsafe{&*c.cast::<Hook>()};h.calls.set(h.calls.get()+1);
 let args=unsafe{borrowed_slice(a,n)}.unwrap();
 let replacement=[Operand::word(10)];let args=if h.replace.get(){&replacement[..]}else{args};
 let answer=unsafe{(h.original.method.unwrap())(h.original.context,r,args.as_ptr(),args.len())};
 if let Some(o)=answer.value.as_object(){let pin=unsafe{retain(h.id,h.roots,o)};h.seen.borrow_mut().push((o,pin));}
 answer
}
unsafe extern "C" fn getter(c:*mut c_void,r:Oop,_:*const Operand,_:usize)->Outcome {
 let h=unsafe{&*c.cast::<Hook>()};h.calls.set(h.calls.get()+1);
 let pin=unsafe{retain(h.id,h.roots,r)};h.seen.borrow_mut().push((r,pin));
 Outcome::loaded(Operand::word(99))
}
#[test]
#[ignore="requires production Integer policy"]
fn constructors_and_getters_observe_and_change_temporary_words() {
 unsafe {
 let id=id();let ns=load(id,"bootstrap",&[Operand::bytes(directory().as_os_str().as_bytes())]);
 let named=|name:&[u8]|object(send(id,ns,b"at:",&[Operand::object(id.intern_name(name).unwrap())]));
 let (integer,word,roots,gc)=(named(b"Integer"),named(b"Word"),named(b"Roots"),named(b"GC"));
 let a=object(send(id,integer,b"fromWord:",&[Operand::word(7)]));let at=retain(id,roots,a);
 let b=object(send(id,integer,b"fromWord:",&[Operand::word(9)]));let bt=retain(id,roots,b);
 let run=||object(send(id,a,b"+",&[Operand::object(b)]));
 let read=|o|send(id,o,b"word",&[]).as_word().unwrap();assert_eq!(read(run()),16);
 let entry=|o,name:&[u8]|send(id,id.receiver,b"bind:to:",&[Operand::object(id.intern_name(name).unwrap()),Operand::object(o)]).as_entry().unwrap();
 let original=entry(word,b"fromBits:");let vt=named(b"WordVt");
 let mut h=Hook{id,roots,original,replace:Cell::new(false),calls:Cell::new(0),seen:RefCell::new(Vec::new())};
 install(id,vt,b"fromBits:",constructor,(&raw mut h).cast());
 assert_eq!(read(run()),16);assert_eq!(h.calls.get(),2,"both input Word factories remain current sends");
 assert_ne!(h.seen.borrow()[0].0,h.seen.borrow()[1].0,"temporary boxes have distinct identity");
 collect(id,gc);for &(o,_) in h.seen.borrow().iter(){assert!(present(id,o),"factory can retain a temporary beyond addition");}
 h.replace.set(true);h.calls.set(0);assert_eq!(read(run()),20,"constructor replacement changes arithmetic inputs");assert_eq!(h.calls.get(),2);
 install(id,vt,b"fromBits:",original.method.unwrap(),original.context);
 let bits=entry(word,b"bits");h.calls.set(0);
 install(id,vt,b"bits",getter,(&raw mut h).cast());
 assert_eq!(read(run()),99,"result conversion uses the current Word getter");assert_eq!(h.calls.get(),1);
 install(id,vt,b"bits",bits.method.unwrap(),bits.context);
 assert_eq!(read(run()),16);
 for (_,pin) in h.seen.borrow_mut().drain(..){release(id,roots,pin);}release(id,roots,bt);release(id,roots,at);
 }
}
