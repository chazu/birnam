//! Explicit conditional method undo preserves later writes and ABA detection.
mod support;
use birnam_abi::{Oop, id_protocol::{NativeEntry, Operand, Outcome}};
use std::{ffi::c_void, os::unix::ffi::OsStrExt};
use support::*;
unsafe extern "C" fn answer(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::word(c.addr()))
}
#[test]
#[ignore = "production native modules"]
fn conditional_binding_undo_preserves_later_writes_and_aba() {
    unsafe {
        let id=id(); let ns=load(id,"bootstrap",&[Operand::bytes(directory().as_os_str().as_bytes())]);
        let named=|n:&[u8]| object(send(id,ns,b"at:",&[Operand::object(id.intern_name(n).unwrap())]));
        let keep=|o| { retain(id,named(b"Roots"),o); o };
        let key=keep(id.intern_name(b"inverseProbe").unwrap()); let next=keep(id.intern_name(b"nextInverse").unwrap());
        let vt=keep(behavior(id)); let target=keep(allocate(id,vt,0));
        let one=Operand::entry(NativeEntry::new(answer,1usize as *mut c_void));
        let two=Operand::entry(NativeEntry::new(answer,2usize as *mut c_void));
        let epoch=|| send(id,id.receiver,b"bindingEpoch",&[]).as_word().unwrap();
        let restore=|key, expected, previous, stamp| send(id,id.receiver,b"restoreBinding:of:in:expected:previous:atEpoch:",
            &[Operand::object(key),Operand::object(target),Operand::object(vt),expected,previous,Operand::word(stamp)]);
        send(id,vt,b"addMethod",&[Operand::object(key),one]);
        send(id,vt,b"addMethod",&[Operand::object(key),two]);
        assert!(restore(key,two,one,epoch()).as_word().is_some());
        assert_eq!(send(id,target,b"inverseProbe",&[]).as_word(),Some(1));
        assert!(restore(key,two,one,epoch()).as_word().is_some(),"already restored");
        send(id,vt,b"addMethod",&[Operand::object(key),two]);
        let before=epoch();
        send(id,vt,b"removeMethod:",&[Operand::object(key)]);
        send(id,vt,b"addMethod",&[Operand::object(key),two]);
        assert!(restore(key,two,one,before).as_word().is_none(),"ABA must conflict");
        assert!(restore(key,one,Operand::unit(),epoch()).as_word().is_none(),"foreign value");
        assert!(restore(key,two,Operand::unit(),epoch()).as_word().is_some(),"restore absence");
        assert!(restore(key,Operand::unit(),one,epoch()).as_word().is_some(),"restore removal");
        let other=keep(behavior(id));
        send(id,id.receiver,b"reassociateBehaviorOf:with:",&[Operand::object(target),Operand::object(other)]);
        assert!(restore(key,one,two,epoch()).as_word().is_none(),"changed target association");
        send(id,id.receiver,b"reassociateBehaviorOf:with:",&[Operand::object(target),Operand::object(vt)]);
        send(id,vt,b"addMethod",&[Operand::object(key),two]);
        send(id,vt,b"addMethod",&[Operand::object(next),two]);
        let committed=restore(key,two,one,epoch()).as_word().unwrap();
        send(id,vt,b"addMethod",&[Operand::object(next),Operand::entry(NativeEntry::new(answer,3usize as *mut c_void))]);
        collect(id,named(b"GC"));
        assert!(restore(next,two,one,committed).as_word().is_none(),"later write must invalidate next inverse");
        assert_eq!(send(id,target,b"nextInverse",&[]).as_word(),Some(3));
    }
}
