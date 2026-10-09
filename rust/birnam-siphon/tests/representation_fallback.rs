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
#[test]
#[ignore="production numeric owners required"]
fn identity_replacement_preserves_numeric_read_and_allocator_failure_order() {
    unsafe {
        let id=id();let ns=load(id,"bootstrap",&[Operand::bytes(directory().as_os_str().as_bytes())]);
        let named=|n:&[u8]|object(send(id,ns,b"at:",&[Operand::object(id.intern_name(n).unwrap())]));
        let roots=named(b"Roots");let _floor=CompiledFloor::pin(id,ns,roots);let word=named(b"Word");
        let value=object(send(id,word,b"fromBits:",&[Operand::word(91)]));let token=retain(id,roots,value);
        let original=send(id,id.receiver,b"bind:to:",&[Operand::object(id.intern_name(b"identityOf:").unwrap()),Operand::object(id.receiver)]).as_entry().unwrap();
        let mut h=Hook{original,calls:Cell::new(0)};install(id,id.receiver,b"identityOf:",fail,(&raw mut h).cast());
        assert_eq!(send(id,value,b"bits",&[]).as_word(),Some(91));assert_eq!(h.calls.get(),0,"old numeric reads do not query identity");
        let failed=id.send_named(word,b"fromBits:",&[Operand::word(92)]).unwrap_err();
        assert_eq!((failed.status,failed.detail),(812,19));assert_eq!(h.calls.get(),1);
        install(id,id.receiver,b"identityOf:",original.method.unwrap(),original.context);release(id,roots,token);
    }
}
