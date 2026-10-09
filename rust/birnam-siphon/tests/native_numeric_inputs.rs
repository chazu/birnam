//! Native numeric inputs borrow the Id call lifetime; fresh outputs use owner roots.
mod support;
use birnam_abi::{Oop, borrowed_slice, id_protocol::{NativeEntry, Operand, Outcome, Protocol}};
use std::{cell::Cell, ffi::c_void, os::unix::ffi::OsStrExt};
use support::*;
struct Hook { id: Protocol, gc: Oop, query: NativeEntry, retain: NativeEntry,
    inputs: Vec<Oop>, input_retains: Cell<usize>, collections: Cell<usize>, active: Cell<bool>, collecting: Cell<bool>,
    nested_left: Cell<usize>, nested: Cell<Oop>, fail_after: Cell<usize>, failed_value: Cell<Oop>, all_retains: Cell<usize> }
unsafe extern "C" fn retain_hook(c:*mut c_void,r:Oop,a:*const Operand,n:usize)->Outcome {
    let h=unsafe{&*c.cast::<Hook>()};
    let value=object((unsafe{borrowed_slice(a,n)}).unwrap()[0]);
    h.all_retains.set(h.all_retains.get()+1);
    if h.inputs.contains(&value){h.input_retains.set(h.input_retains.get()+1);}
    unsafe{(h.retain.method.unwrap())(h.retain.context,r,a,n)}
}
unsafe extern "C" fn collecting_query(c:*mut c_void,r:Oop,a:*const Operand,n:usize)->Outcome {
    let h=unsafe{&*c.cast::<Hook>()};
    let top = !h.active.replace(true);
    let value=object((unsafe{borrowed_slice(a,n)}).unwrap()[0]);
    if !h.collecting.replace(true) {
        unsafe{collect(h.id,h.gc);}
        for &input in &h.inputs {assert!(unsafe{present(h.id,input)},"caller protection lost during native callback");}
        assert!(unsafe{present(h.id,value)},"fresh value must be owner-rooted before validation callbacks");
        h.collections.set(h.collections.get()+1);h.collecting.set(false);
    }
    if top {
        if h.nested_left.get()>0 && !h.inputs.contains(&value) {
            h.nested_left.set(h.nested_left.get()-1);
            let child=unsafe{object(send(h.id,h.inputs[0],b"fromBits:",&[Operand::word(777)]))};
            h.nested.set(child);
            h.collecting.set(true);unsafe{collect(h.id,h.gc)};h.collecting.set(false);
            assert!(unsafe{present(h.id,value)},"nested scope exit must preserve parent roots");
            assert!(!unsafe{present(h.id,child)},"discarded child scope must not leak its value");
        }
        h.active.set(false);
        if h.fail_after.get()>0 {
            h.fail_after.set(h.fail_after.get()-1);
            if h.fail_after.get()==0 {h.failed_value.set(value);return Outcome::failed(812,19);}
        }
    }
    unsafe{(h.query.method.unwrap())(h.query.context,r,a,n)}
}
#[test]
#[ignore="production numeric owners required"]
fn native_inputs_borrow_call_coverage_but_outputs_survive_collecting_validation() {
    unsafe {
        let id=id();let ns=load(id,"bootstrap",&[Operand::bytes(directory().as_os_str().as_bytes())]);
        let named=|n:&[u8]|object(send(id,ns,b"at:",&[Operand::object(id.intern_name(n).unwrap())]));
        let roots=named(b"Roots");let gc=named(b"GC");let _floor=CompiledFloor::pin(id,ns,roots);
        let word=named(b"Word");let integer=named(b"Integer");
        let a=object(send(id,word,b"fromBits:",&[Operand::word(40)]));let at=retain(id,roots,a);
        let b=object(send(id,word,b"fromBits:",&[Operand::word(2)]));let bt=retain(id,roots,b);
        let x=object(send(id,integer,b"fromWord:",&[Operand::word(17)]));let xt=retain(id,roots,x);
        let bind=|r:Oop,n:&[u8]|send(id,id.receiver,b"bind:to:",&[Operand::object(id.intern_name(n).unwrap()),Operand::object(r)]).as_entry().unwrap();
        let query=bind(id.receiver,b"behaviorOf:");let retain_entry=bind(roots,b"retain:");
        let mut h=Hook{id,gc,query,retain:retain_entry,inputs:vec![word,integer,a,b,x],input_retains:Cell::new(0),collections:Cell::new(0),active:Cell::new(false),collecting:Cell::new(false),nested_left:Cell::new(2),nested:Cell::new(std::ptr::null_mut()),fail_after:Cell::new(0),failed_value:Cell::new(std::ptr::null_mut()),all_retains:Cell::new(0)};
        let context=(&raw mut h).cast();let rb=object(send(id,id.receiver,b"behaviorOf:",&[Operand::object(roots)]));
        install(id,rb,b"retain:",retain_hook,context);
        install(id,id.receiver,b"behaviorOf:",collecting_query,context);
        let result=object(send(id,a,b"wrappingAdd:",&[Operand::object(b)]));
        assert_eq!(h.all_retains.get(),0,"numeric construction protects fresh values through owner protocols without Root registrations");
        assert!(!h.nested.get().is_null());let rt=retain(id,roots,result);
        assert_eq!(send(id,result,b"bits",&[]).as_word(),Some(42));
        assert_eq!(send(id,integer,b"wordOf:",&[Operand::object(x)]).as_word(),Some(17));
        let boxed=object(send(id,integer,b"fromWord:",&[Operand::word(91)]));let boxed_t=retain(id,roots,boxed);
        assert_eq!(send(id,boxed,b"word",&[]).as_word(),Some(91));
        assert_eq!(h.input_retains.get(),0,"receiver/inputs must not be re-registered by numeric wrappers");
        assert!(h.collections.get()>=6);
        // Fail the Number owner's validation after ObjectModel has returned.
        for factory in [word,integer] {
            h.fail_after.set(2);
            let failure=id.send_named(factory,if factory==word {b"fromBits:"} else {b"fromWord:"},&[Operand::word(1234)]).unwrap_err();
            assert_eq!((failure.status,failure.detail),(812,19));
            let failed=h.failed_value.get();collect(id,gc);
            assert!(!present(id,failed),"failure must drop private owner roots");
        }
        release(id,roots,rt);collect(id,gc);assert!(!present(id,result),"successful owner scopes must drain at return");
        install(id,id.receiver,b"behaviorOf:",query.method.unwrap(),query.context);
        install(id,rb,b"retain:",retain_entry.method.unwrap(),retain_entry.context);
        for t in [boxed_t,xt,bt,at]{release(id,roots,t);}
    }
}
