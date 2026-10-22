//! Nullable adaptation, exact carriers, collection and reentrant invocation.
mod support;
use birnam_abi::{Oop,borrowed_slice,id_protocol::{NativeEntry,Operand,Outcome,Protocol}};
use std::{cell::Cell,ffi::c_void,os::unix::ffi::OsStrExt,ptr};
use support::*;
struct Probe {id:Protocol,gc:Oop,receiver:Oop,selector:Oop,nested:Cell<bool>,calls:Cell<usize>}
unsafe extern "C" fn inspect(c:*mut c_void,_:Oop,a:*const Operand,n:usize)->Outcome { unsafe {
    let p=&*c.cast::<Probe>();let args=borrowed_slice(a,n).unwrap();let before=args.to_vec();
    p.calls.set(p.calls.get()+1);
    collect(p.id,p.gc);
    if !p.nested.replace(true) {
        let result=p.id.send(p.receiver,p.selector,&[Operand::object(p.receiver)]).unwrap();
        assert_eq!(result.as_object(),Some(p.receiver));p.nested.set(false);
    }
    assert_eq!(args,before.as_slice(),"nested entry must not overwrite outer arguments");
    Outcome::loaded(args[0])
}}
#[test]
#[ignore="requires native prefix"]
fn invocation_preserves_nullable_policy_and_reentrant_packets() {unsafe {
    let id=id();let ns=load(id,"bootstrap",&[Operand::bytes(directory().as_os_str().as_bytes())]);
    let named=|name:&[u8]|object(send(id,ns,b"at:",&[Operand::object(id.intern_name(name).unwrap())]));
    let roots=named(b"Roots");let nil=named(b"nil");
    let behavior=behavior(id);retain(id,roots,behavior);let receiver=allocate(id,behavior,0);retain(id,roots,receiver);
    let selector=id.intern_name(b"packet").unwrap();retain(id,roots,selector);
    let mut p=Probe {id,gc:named(b"GC"),receiver,selector,nested:Cell::new(false),calls:Cell::new(0)};
    for authored in [false,true] {
        let mut args=vec![Operand::entry(NativeEntry::new(inspect,ptr::from_mut(&mut p).cast())),Operand::object(named(b"Execution"))];
        if authored {args.push(Operand::object(nil));}
        let method=object(send(id,named(b"Method"),if authored {b"fromNative:owner:implementation:"} else {b"fromNative:owner:"},&args));retain(id,roots,method);
        send(id,behavior,b"addMethod:as:",&[Operand::object(method),Operand::object(selector)]);
        for first in [Operand::object(receiver),Operand::object(ptr::null_mut()),Operand::word(13),Operand::unit()] {
            let packet=[first,Operand::object(receiver),Operand::bytes(b"unchanged"),Operand::word(0)];
            let result=id.send(receiver,selector,&packet).unwrap();
            let expected=if authored && first.as_object()==Some(ptr::null_mut()) {Operand::object(nil)} else {first};
            assert_eq!(result,expected);
            assert_eq!(packet[0],first);
        }
    }
    assert_eq!(p.calls.get(),16);
    send(id,behavior,b"removeMethod:",&[Operand::object(selector)]);
}}
