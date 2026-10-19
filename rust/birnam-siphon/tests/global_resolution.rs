//! Source resolution remains late-bound and scopes fetched values through reentry.
mod support;
use birnam_abi::{Oop, borrowed_slice, id_protocol::{NativeEntry, Operand, Outcome, Protocol}};
use std::{cell::{Cell,RefCell}, ffi::c_void, os::unix::ffi::OsStrExt};
use support::*;
struct H {id:Protocol,ns:Oop,gc:Oop,original:NativeEntry,key:Oop,nil:Cell<Oop>,globals:Cell<Oop>,
    replacement_nil:Oop,replacement_globals:Oop,events:RefCell<Vec<Oop>>,mutate:Cell<bool>,fail:Cell<bool>,active:Cell<bool>,namespace_reply:Cell<u8>,globals_reply:Cell<u8>}
unsafe extern "C" fn namespace_get(c:*mut c_void,r:Oop,a:*const Operand,n:usize)->Outcome {
    let h=unsafe{&*c.cast::<H>()};let key=object((unsafe{borrowed_slice(a,n)}).unwrap()[0]);
    h.events.borrow_mut().push(key);
    if key==unsafe{h.id.intern_name(b"Globals").unwrap()} {
        match h.namespace_reply.get() {
            1 => return Outcome::loaded(Operand::unit()),
            2 => return Outcome::loaded(Operand::word(7)),
            3 => return Outcome::failed(813,20),
            _ => {}
        }
    }
    if h.fail.get()&&key==h.key{return Outcome::failed(812,19);}
    unsafe{(h.original.method.unwrap())(h.original.context,r,a,n)}
}
unsafe extern "C" fn globals_get(c:*mut c_void,r:Oop,a:*const Operand,n:usize)->Outcome {
    let h=unsafe{&*c.cast::<H>()};let [key]=(unsafe{borrowed_slice(a,n)}).unwrap() else{panic!("key")};
    assert_eq!(object(*key),h.key);h.events.borrow_mut().push(r);
    if h.mutate.get()&&!h.active.replace(true) {
        unsafe{
            for (name,value) in [(b"nil".as_slice(),h.replacement_nil),(b"Globals",h.replacement_globals)] {
                send(h.id,h.ns,b"at:put:",&[Operand::object(h.id.intern_name(name).unwrap()),Operand::object(value)]);
            }
            collect(h.id,h.gc);
            assert!(present(h.id,h.nil.get()),"fetched nil needs scope protection after rebinding");
            assert!(present(h.id,h.globals.get()),"fetched Globals needs protection after rebinding");
        }
        h.active.set(false);
    }
    match h.globals_reply.get() {
        1 => Outcome::failed(814,21),
        2 => Outcome::loaded(Operand::word(9)),
        _ => Outcome::loaded(Operand::object(h.nil.get()))
    }
}
#[test]
#[ignore="production Namespace resolution required"]
fn global_resolution_preserves_precedence_getter_order_rebinding_gc_and_failures() {
    unsafe{
        let id=id();let ns=load(id,"bootstrap",&[Operand::bytes(directory().as_os_str().as_bytes())]);
        let named=|n:&[u8]|object(send(id,ns,b"at:",&[Operand::object(id.intern_name(n).unwrap())]));
        let roots=named(b"Roots");let gc=named(b"GC");let _floor=CompiledFloor::pin(id,ns,roots);
        let nil=named(b"nil");let globals=named(b"Globals");let key=id.intern_name(b"ResolutionProbe").unwrap();
        let missing=id.intern_name(b"ResolutionMissing").unwrap();
        let a=allocate(id,named(b"ObjectVt"),0);let at=retain(id,roots,a);let b=allocate(id,named(b"ObjectVt"),0);let bt=retain(id,roots,b);
        send(id,ns,b"at:put:",&[Operand::object(key),Operand::object(a)]);
        assert_eq!(object(send(id,ns,b"resolveGlobal:",&[Operand::object(key)])),a);
        send(id,globals,b"at:put:",&[Operand::object(key),Operand::object(b)]);
        assert_eq!(object(send(id,ns,b"resolveGlobal:",&[Operand::object(key)])),b);
        send(id,globals,b"at:put:",&[Operand::object(key),Operand::object(nil)]);
        assert_eq!(object(send(id,ns,b"resolveGlobal:",&[Operand::object(key)])),a);
        assert_eq!(object(send(id,ns,b"resolveGlobal:",&[Operand::object(missing)])),nil);
        let bind=|r:Oop,n:&[u8]|send(id,id.receiver,b"bind:to:",&[Operand::object(id.intern_name(n).unwrap()),Operand::object(r)]).as_entry().unwrap();
        let original=bind(ns,b"at:");let nb=object(send(id,id.receiver,b"behaviorOf:",&[Operand::object(ns)]));
        let gb=behavior(id);let gbt=retain(id,roots,gb);let temporary_globals=allocate(id,gb,0);let gt=retain(id,roots,temporary_globals);
        let temporary_nil=allocate(id,named(b"ObjectVt"),0);let nt=retain(id,roots,temporary_nil);
        let mut h=H{id,ns,gc,original,key,nil:Cell::new(temporary_nil),globals:Cell::new(temporary_globals),replacement_nil:nil,replacement_globals:globals,
            events:RefCell::new(Vec::new()),mutate:Cell::new(true),fail:Cell::new(false),active:Cell::new(false),namespace_reply:Cell::new(0),globals_reply:Cell::new(0)};
        let c=(&raw mut h).cast();install(id,gb,b"at:",globals_get,c);install(id,nb,b"at:",namespace_get,c);
        send(id,ns,b"at:put:",&[Operand::object(id.intern_name(b"nil").unwrap()),Operand::object(temporary_nil)]);
        send(id,ns,b"at:put:",&[Operand::object(id.intern_name(b"Globals").unwrap()),Operand::object(temporary_globals)]);
        release(id,roots,gt);release(id,roots,nt);
        assert_eq!(object(send(id,ns,b"resolveGlobal:",&[Operand::object(key)])),a);
        assert_eq!(*h.events.borrow(),[id.intern_name(b"nil").unwrap(),id.intern_name(b"Globals").unwrap(),temporary_globals,key]);
        collect(id,gc);assert!(!present(id,temporary_nil)&&!present(id,temporary_globals),"resolver scopes must drain on return");
        h.mutate.set(false);h.nil.set(nil);h.globals.set(globals);h.fail.set(true);h.events.borrow_mut().clear();
        let error=id.send_named(ns,b"resolveGlobal:",&[Operand::object(key)]).unwrap_err();assert_eq!((error.status,error.detail),(812,19));
        h.fail.set(false);h.events.borrow_mut().clear();h.namespace_reply.set(1);
        assert_eq!(object(send(id,ns,b"resolveGlobal:",&[Operand::object(key)])),a);
        assert_eq!(*h.events.borrow(),[id.intern_name(b"nil").unwrap(),id.intern_name(b"Globals").unwrap(),key]);
        h.namespace_reply.set(2);
        assert_eq!(id.send_named(ns,b"resolveGlobal:",&[Operand::object(key)]).unwrap_err().status,birnam_abi::id_protocol::INVALID_REQUEST);
        h.namespace_reply.set(3);
        let error=id.send_named(ns,b"resolveGlobal:",&[Operand::object(key)]).unwrap_err();assert_eq!((error.status,error.detail),(813,20));
        h.namespace_reply.set(0);
        let failed_owner=allocate(id,gb,0);let ft=retain(id,roots,failed_owner);
        send(id,ns,b"at:put:",&[Operand::object(id.intern_name(b"Globals").unwrap()),Operand::object(failed_owner)]);
        h.globals_reply.set(1);
        let error=id.send_named(ns,b"resolveGlobal:",&[Operand::object(key)]).unwrap_err();assert_eq!((error.status,error.detail),(814,21));
        h.globals_reply.set(2);
        assert_eq!(id.send_named(ns,b"resolveGlobal:",&[Operand::object(key)]).unwrap_err().status,birnam_abi::id_protocol::INVALID_REQUEST);
        send(id,ns,b"at:put:",&[Operand::object(id.intern_name(b"Globals").unwrap()),Operand::object(globals)]);
        release(id,roots,ft);
        install(id,nb,b"at:",original.method.unwrap(),original.context);
        assert!(id.send_named(ns,b"resolveGlobal:",&[Operand::word(1)]).is_err());
        for token in [gbt,bt,at]{release(id,roots,token);}
    }
}
