//! Owner conditions used by advanced live-image compensation.
mod support;
use birnam_abi::{Oop, id_protocol::{NativeEntry, Operand, Outcome, Protocol}};
use std::{ffi::c_void, os::unix::ffi::OsStrExt, cell::Cell};
use support::*;
unsafe extern "C" fn answer(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::word(c.addr()))
}
struct Edit { id: Protocol, target: Oop, key: Oop, armed: Cell<bool>, gc: Oop }
unsafe extern "C" fn changed(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Edit>();
        if p.armed.replace(false) {
            send(p.id,p.target,b"addMethod",&[Operand::object(p.key),Operand::entry(NativeEntry::new(answer,3usize as *mut c_void))]);
            collect(p.id,p.gc);
        }
        Outcome::loaded(Operand::unit())
    }
}
#[test]
#[ignore = "production native modules"]
fn conditional_inverse_owners_preserve_reentry_aba_and_policy_changes() {
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
        let mut probe=Edit{id,target:vt,key:next,armed:Cell::new(false),gc:named(b"GC")};
        let ob=keep(behavior(id)); install(id,ob,b"methodChanged:on:",changed,(&mut probe as *mut Edit).cast());
        let observer=keep(allocate(id,ob,0));
        send(id,id.receiver,b"methodChangesTo:",&[Operand::object(observer)]);
        probe.armed.set(true);
        let committed=restore(key,two,one,epoch()).as_word().unwrap();
        assert!(restore(next,two,one,committed).as_word().is_none(),"inverse callback must invalidate next inverse");
        assert_eq!(send(id,target,b"nextInverse",&[]).as_word(),Some(3));
        send(id,id.receiver,b"methodChangesTo:",&[Operand::unit()]);
        let store=named(b"Store"); let stage=keep(object(send(id,store,b"new",&[])));
        let rev=|o| send(id,o,b"imageStateRevision",&[]).as_word().unwrap();
        let swap=|l,r| send(id,store,b"swapImageStateWith:atRevision:stagedRevision:",
            &[Operand::object(stage),Operand::word(l),Operand::word(r)]).as_word().unwrap();
        let (left,right)=(rev(store),rev(stage)); assert_eq!(swap(left,right),1);
        let (left,right)=(rev(store),rev(stage));
        // A policy-only change need not alter fact basis; owner revision catches it.
        send(id,store,b"retainCurrentOnly",&[]);
        assert_eq!(swap(left,right),0);
        let value=keep(id.intern_name(b"explicitAfterImage").unwrap());
        let (left,right)=(rev(store),rev(stage));
        send(id,store,b"assert:attr:value:",&[Operand::object(key),Operand::object(next),Operand::object(value)]);
        assert_eq!(swap(left,right),0);
        assert_eq!(send(id,store,b"currentValue:attr:",&[Operand::object(key),Operand::object(next)]).as_object(),Some(value));
        // A stale forward publication also cannot erase the resident callback write.
        assert_eq!(swap(left,right),0);
        assert_eq!(swap(rev(store),rev(stage)),1);
        assert_eq!(swap(rev(store),rev(stage)),1);
        assert_eq!(send(id,store,b"currentValue:attr:",&[Operand::object(key),Operand::object(next)]).as_object(),Some(value));
    }
}
