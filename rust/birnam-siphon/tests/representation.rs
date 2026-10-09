//! Id-owned atomic facts preserve changed-getter fallback and self admission.
mod support;
use birnam_abi::{Oop, id_protocol::{NativeEntry, Operand, Outcome, Protocol}};
use std::{cell::Cell, ffi::c_void, os::unix::ffi::OsStrExt};
use support::*;
struct Hook { original: NativeEntry, calls: Cell<usize> }
unsafe extern "C" fn fail(c:*mut c_void,_:Oop,_:*const Operand,_:usize)->Outcome {
    unsafe{&*c.cast::<Hook>()}.calls.set(1); Outcome::failed(812,19)
}
unsafe extern "C" fn forward(c:*mut c_void,r:Oop,a:*const Operand,n:usize)->Outcome {
    let h=unsafe{&*c.cast::<Hook>()};h.calls.set(h.calls.get()+1);
    unsafe{(h.original.method.unwrap())(h.original.context,r,a,n)}
}
fn atomic_facts_match_heap_and_decline_changed_getters_queries_and_lookup() -> Protocol {
    unsafe {
        let id=id();let b=behavior(id);let other=behavior(id);let v=allocate(id,b,8);
        let selector=id.intern_name(b"tryRepresentationIdentityOf:behavior:bytes:").unwrap();
        let args=[Operand::object(v),Operand::object(b),Operand::word(8)];
        let query=|args:&[Operand]| id.send(id.receiver,selector,args).unwrap();
        let identity=send(id,id.receiver,b"identityOf:",&[Operand::object(v)]).as_word().unwrap();
        assert_eq!(query(&args).as_word(),Some(identity));assert_ne!(identity,0);
        for wrong in [[Operand::object(v),Operand::object(other),Operand::word(8)],
            [Operand::object(v),Operand::object(b),Operand::word(7)],
            [Operand::object(std::ptr::with_exposed_provenance_mut(7)),Operand::object(b),Operand::word(8)]] {
            assert_eq!(query(&wrong).as_word(),Some(0));
        }
        assert!(id.send(id.receiver,selector,&args[..2]).is_err());
        assert!(id.send(id.receiver,selector,&[args[0],args[1],Operand::object(v)]).is_err());
        let bind=|name:&[u8]|send(id,id.receiver,b"bind:to:",&[Operand::object(id.intern_name(name).unwrap()),Operand::object(id.receiver)]).as_entry().unwrap();
        for name in [b"behaviorOf:".as_slice(),b"allocationSizeOf:",b"identityOf:"] {
            let entry=bind(name);let mut h=Hook{original:entry,calls:Cell::new(0)};
            install(id,id.receiver,name,fail,(&raw mut h).cast());
            assert_eq!(query(&args).kind(),0);assert_eq!(h.calls.get(),0,"atomic admission must not execute changed getters");
            install(id,id.receiver,name,entry.method.unwrap(),entry.context);
        }
        let own=bind(b"tryRepresentationIdentityOf:behavior:bytes:");let mut h=Hook{original:own,calls:Cell::new(0)};
        install(id,id.receiver,b"tryRepresentationIdentityOf:behavior:bytes:",forward,(&raw mut h).cast());
        assert_eq!(query(&args).kind(),0,"a forwarding wrapper cannot claim the canonical answer");
        install(id,id.receiver,b"tryRepresentationIdentityOf:behavior:bytes:",own.method.unwrap(),own.context);
        let lookup=bind(b"lookup");h.original=lookup;
        install(id,id.receiver,b"lookup",forward,(&raw mut h).cast());h.calls.set(0);
        assert_eq!(query(&args).kind(),0);assert_eq!(h.calls.get(),1,"only the ordinary dispatch lookup occurs");
        install(id,id.receiver,b"lookup",lookup.method.unwrap(),lookup.context);
        send(id,id.receiver,b"reassociateBehaviorOf:with:",&[Operand::object(v),Operand::object(other)]);
        assert_eq!(query(&args).as_word(),Some(0),"no shape grant persists across reassociation");
        id
    }
}
#[test]
#[ignore="current native Id required"]
fn representation_facts_and_fallbacks() {
    atomic_facts_match_heap_and_decline_changed_getters_queries_and_lookup();
}
