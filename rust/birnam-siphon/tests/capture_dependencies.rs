//! Capture admission depends on fixed selectors, not unrelated name services.
mod support;
use birnam_abi::{Oop, borrowed_bytes, id_protocol::{NativeEntry, Operand, Outcome}};
use std::{cell::Cell, ffi::c_void, os::unix::ffi::OsStrExt};
use support::*;
struct Hook { entry: NativeEntry, calls: Cell<usize> }
unsafe extern "C" fn forward(c:*mut c_void,r:Oop,a:*const Operand,n:usize)->Outcome {
    let h=unsafe{&*c.cast::<Hook>()};h.calls.set(h.calls.get()+1);
    unsafe{(h.entry.method.unwrap())(h.entry.context,r,a,n)}
}
#[test]
#[ignore="requires production native modules"]
fn unrelated_symbol_lookup_is_not_a_capture_dependency() {
 unsafe {
    let id=id();let ns=load(id,"bootstrap",&[Operand::bytes(directory().as_os_str().as_bytes())]);
    let named=|name:&[u8]|object(send(id,ns,b"at:",&[Operand::object(id.intern_name(name).unwrap())]));
    let text=send(id,named(b"String"),b"fromBytes:",&[Operand::bytes(b"let marker := [Object clone]; [| marker]")]);
    let roots=named(b"Roots");let text_pin=retain(id,roots,object(text));
    let block=object(send(id,named(b"Codegen"),b"eval:",&[text]));let pin=retain(id,roots,block);release(id,roots,text_pin);
    let query=id.intern_name(b"tryBorrowCaptureOperands:").unwrap();
    let symbol=id.intern_name(b"symbolNamed:").unwrap();
    let old=send(id,id.receiver,b"bind:to:",&[Operand::object(symbol),Operand::object(id.receiver)]).as_entry().unwrap();
    let mut h=Hook{entry:old,calls:Cell::new(0)};
    install(id,id.receiver,b"symbolNamed:",forward,(&raw mut h).cast());
    let result=id.send(block,query,&[Operand::word(1)]).unwrap();let bytes=result.as_bytes().expect("unrelated name binding must not decline capture borrow");
    assert_eq!(bytes.length,size_of::<Oop>());
    assert!(borrowed_bytes(bytes.pointer,bytes.length).unwrap().iter().any(|&b|b!=0));
    assert_eq!(h.calls.get(),0,"pre-resolved capture request does not invoke symbol lookup");
    install(id,id.receiver,b"symbolNamed:",old.method.unwrap(),old.context);
    release(id,roots,pin);
 }
}
