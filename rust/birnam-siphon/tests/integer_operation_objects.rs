//! Shared operation receivers remain ordinary replaceable Id dispatch.
mod support;
use birnam_abi::{Oop,id_protocol::{NativeEntry,Operand,Outcome,Protocol}};
use std::{cell::Cell,ffi::c_void,os::unix::ffi::OsStrExt};
use support::*;
struct H {id:Protocol,gc:Oop,answer:Oop,calls:Cell<usize>}
unsafe extern "C" fn replacement(c:*mut c_void,_:Oop,_:*const Operand,n:usize)->Outcome {
    assert_eq!(n,2);let h=unsafe{&*c.cast::<H>()};unsafe{collect(h.id,h.gc)};h.calls.set(h.calls.get()+1);Outcome::loaded(Operand::object(h.answer))
}
#[test]
#[ignore="fresh authored Integer operation objects required"]
fn operations_have_distinct_receivers_and_current_collecting_handler_dispatch() {
    unsafe{
        let id=id();let ns=load(id,"bootstrap",&[Operand::bytes(directory().as_os_str().as_bytes())]);
        let named=|n:&[u8]|object(send(id,ns,b"at:",&[Operand::object(id.intern_name(n).unwrap())]));
        let roots=named(b"Roots");let _floor=CompiledFloor::pin(id,ns,roots);let integer=named(b"Integer");let globals=named(b"Globals");
        let ops:Vec<_>=[b"intAddOperation".as_slice(),b"intSubtractOperation",b"intMultiplyOperation",b"intDivideOperation",b"intRemainderOperation"].iter().map(|name|object(send(id,globals,b"at:",&[Operand::object(id.intern_name(name).unwrap())]))).collect();
        for (i,&op) in ops.iter().enumerate(){for &other in &ops[i+1..]{assert_ne!(op,other);}}
        let mut tokens=Vec::new();let number=|bits:usize|object(send(id,integer,b"fromWord:",&[Operand::word(bits)]));
        let a=number(11);tokens.push(retain(id,roots,a));let b=number(3);tokens.push(retain(id,roots,b));let answer=number(777);tokens.push(retain(id,roots,answer));
        let selector=id.intern_name(b"applyInteger:with:").unwrap();let entry=send(id,id.receiver,b"bind:to:",&[Operand::object(selector),Operand::object(ops[0])]).as_entry().unwrap();
        // A saved entry alone does not keep its context/code owner alive.
        let saved=object(send(id,named(b"Execution"),b"methodOfEntry:",&[Operand::entry(entry)]));tokens.push(retain(id,roots,saved));
        let mut h=H{id,gc:named(b"GC"),answer,calls:Cell::new(0)};
        let behavior=object(send(id,id.receiver,b"behaviorOf:",&[Operand::object(ops[0])]));
        install(id,behavior,b"applyInteger:with:",replacement,(&raw mut h).cast());
        assert_eq!(object(send(id,a,b"+",&[Operand::object(b)])),answer);assert_eq!(h.calls.get(),1);
        assert_eq!(send(id,object(send(id,a,b"-",&[Operand::object(b)])),b"word",&[]).as_word(),Some(8));
        send(id,behavior,b"addMethod",&[Operand::object(selector),Operand::entry(NativeEntry::new(entry.method.unwrap(),entry.context))]);
        assert_eq!(send(id,object(send(id,a,b"+",&[Operand::object(b)])),b"word",&[]).as_word(),Some(14));
        for t in tokens{release(id,roots,t);}
    }
}
