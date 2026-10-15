//! Native forwarding/hash/map inputs borrow the caller; detached nodes stay rooted.
mod support;
use birnam_abi::{Oop, borrowed_slice, id_protocol::{NativeEntry, Operand, Outcome, Protocol}};
use birnam_artifact_codec::portable_program::{NodeSpec, ProgramSpec, StatementSpec, encode};
use std::{cell::{Cell,RefCell}, ffi::c_void, os::unix::ffi::OsStrExt};
use support::*;
struct H {
    id:Protocol, roots:Oop, gc:Oop, model:Oop, map:Oop, key:Oop,
    query:NativeEntry, retainer:NativeEntry, vector_entry:NativeEntry, only_vector:Cell<bool>, inside_vector:Cell<bool>, inputs:RefCell<Vec<Oop>>,
    input_retains:Cell<usize>, map_token:Cell<usize>, key_token:Cell<usize>,
    vector_nodes:Cell<Option<[Oop;4]>>, old_node:Cell<Oop>, mutate:Cell<bool>, active:Cell<bool>, failed:Cell<bool>, calls:Cell<usize>,
}
unsafe extern "C" fn retain_hook(c:*mut c_void,r:Oop,a:*const Operand,n:usize)->Outcome {
    let h=unsafe{&*c.cast::<H>()};let value=object((unsafe{borrowed_slice(a,n)}).unwrap()[0]);
    if (!h.only_vector.get()||h.inside_vector.get())&&h.inputs.borrow().contains(&value){h.input_retains.set(h.input_retains.get()+1);}
    unsafe{(h.retainer.method.unwrap())(h.retainer.context,r,a,n)}
}
unsafe extern "C" fn vector_entry(c:*mut c_void,r:Oop,a:*const Operand,n:usize)->Outcome {
    let h=unsafe{&*c.cast::<H>()};let previous=h.inside_vector.replace(r==h.map);
    let result=unsafe{h.vector_entry.method.unwrap()(h.vector_entry.context,r,a,n)};
    h.inside_vector.set(previous);result
}
unsafe extern "C" fn hash(c:*mut c_void,_:Oop,_:*const Operand,n:usize)->Outcome {
    assert_eq!(n,0);let h=unsafe{&*c.cast::<H>()};
    if !h.active.replace(true) {
        unsafe {
            for token in [&h.map_token,&h.key_token]{let t=token.replace(0);if t!=0{release(h.id,h.roots,t);}}
            if h.mutate.get(){send(h.id,h.map,b"rehashImage:",&[Operand::object(h.map)]);}
            collect(h.id,h.gc);
            assert!(present(h.id,h.map)&&present(h.id,h.key));
            if h.mutate.get(){assert!(present(h.id,h.old_node.get()),"old node needs its own snapshot protection after rehash");}
        }
        h.calls.set(h.calls.get()+1);h.active.set(false);
        if h.failed.get(){return Outcome::failed(812,19);}
    }
    Outcome::loaded(Operand::word(42))
}
unsafe extern "C" fn collecting_query(c:*mut c_void,r:Oop,a:*const Operand,n:usize)->Outcome {
    let h=unsafe{&*c.cast::<H>()};
    if !h.active.replace(true) {
        if let Some([old_root, old_tail, new_root, new_tail]) = h.vector_nodes.get().filter(|_| unsafe {borrowed_slice(a,n)}.and_then(|args|args.first()).and_then(|arg|arg.as_object())==Some(h.key)) {
            unsafe {
                // Enter through generated code, then discard the native caller's
                // external leases. Its call frame alone must cover both inputs.
                for token in [&h.map_token, &h.key_token] {
                    let t=token.replace(0); if t!=0 {release(h.id,h.roots,t);}
                }
                send(h.id,h.model,b"slotAt:in:put:",&[Operand::word(2),Operand::object(h.map),Operand::object(new_root)]);
                send(h.id,h.model,b"slotAt:in:put:",&[Operand::word(3),Operand::object(h.map),Operand::object(new_tail)]);
                collect(h.id,h.gc);
                assert!(present(h.id,old_root)&&present(h.id,old_tail),"detached Vector snapshot must stay live");
            }
        }
        unsafe{collect(h.id,h.gc);}
        for &v in h.inputs.borrow().iter(){assert!(unsafe{present(h.id,v)},"caller input lost during getter callback");}
        h.calls.set(h.calls.get()+1);h.active.set(false);
        if h.failed.get(){return Outcome::failed(813,20);}
    }
    unsafe{(h.query.method.unwrap())(h.query.context,r,a,n)}
}
#[test]
#[ignore="production owners and synchronous compiler required"]
fn inputs_and_map_snapshots_survive_collecting_reentrant_and_failing_reads() {
    unsafe {
        let id=id();let ns=load(id,"bootstrap",&[Operand::bytes(directory().as_os_str().as_bytes())]);
        let named=|n:&[u8]|object(send(id,ns,b"at:",&[Operand::object(id.intern_name(n).unwrap())]));
        let roots=named(b"Roots");let gc=named(b"GC");let _floor=CompiledFloor::pin(id,ns,roots);
        let bind=|r:Oop,n:&[u8]|send(id,id.receiver,b"bind:to:",&[Operand::object(id.intern_name(n).unwrap()),Operand::object(r)]).as_entry().unwrap();
        let b=behavior(id);let bt=retain(id,roots,b);let key=allocate(id,b,0);let kt=retain(id,roots,key);
        let value=allocate(id,named(b"ObjectVt"),0);let vt=retain(id,roots,value);
        let mut h=H{id,roots,gc,model:named(b"ObjectModel"),map:std::ptr::null_mut(),key,
            vector_entry:bind(named(b"Vector"),b"at:"),only_vector:Cell::new(false),inside_vector:Cell::new(false),query:bind(id.receiver,b"behaviorOf:"),retainer:bind(roots,b"retain:"),inputs:RefCell::new(Vec::new()),input_retains:Cell::new(0),
            map_token:Cell::new(0),key_token:Cell::new(0),vector_nodes:Cell::new(None),old_node:Cell::new(std::ptr::null_mut()),mutate:Cell::new(false),active:Cell::new(true),failed:Cell::new(false),calls:Cell::new(0)};
        let c=(&raw mut h).cast();install(id,b,b"collectionHash",hash,c);
        let empty=object(send(id,named(b"Map"),b"new",&[]));let et=retain(id,roots,empty);
        h.map=object(send(id,empty,b"at:put:",&[Operand::object(key),Operand::object(value)]));h.map_token.set(retain(id,roots,h.map));h.key_token.set(kt);
        h.old_node.set(object(send(id,h.model,b"slotAt:in:",&[Operand::word(0),Operand::object(h.map)])));
        let portable=encode(ProgramSpec{parameters:&[b"map",b"key"],captures:&[],lets:&[],
            statements:&[StatementSpec::Expression{root_node:2}],nodes:&[NodeSpec::LexicalRead(0),NodeSpec::LexicalRead(1),NodeSpec::Send{selector:b"at:",operands:&[0,1]}]}).unwrap();
        let bytes=object(send(id,named(b"Bytes"),b"fromBytes:",&[Operand::bytes(&portable)]));let bytet=retain(id,roots,bytes);
        let code=object(send(id,named(b"CodeObject"),b"fromCanonicalBytes:",&[Operand::object(bytes)]));let ct=retain(id,roots,code);
        let captures=object(send(id,named(b"Vector"),b"new",&[]));let cap=retain(id,roots,captures);
        let block=object(send(id,named(b"Codegen"),b"materializeSynchronous:captures:",&[Operand::object(code),Operand::object(captures)]));let blt=retain(id,roots,block);
        let resumable=object(send(id,named(b"Codegen"),b"materializeResumable:captures:",&[Operand::object(code),Operand::object(captures)]));let rst=retain(id,roots,resumable);
        let rb=object(send(id,id.receiver,b"behaviorOf:",&[Operand::object(roots)]));install(id,rb,b"retain:",retain_hook,c);
        h.inputs.replace(vec![h.map,key]);h.active.set(false);
        assert_eq!(object(send(id,block,b"value:value:",&[Operand::object(h.map),Operand::object(key)])),value);
        assert_eq!(h.input_retains.get(),0,"native Map lookup must not re-register caller inputs");
        // Retain the native caller's next invocation inputs after the outer one ends.
        h.map_token.set(retain(id,roots,h.map));h.key_token.set(retain(id,roots,key));h.input_retains.set(0);h.mutate.set(true);
        assert_eq!(object(send(id,block,b"value:value:",&[Operand::object(h.map),Operand::object(key)])),value);
        h.map_token.set(retain(id,roots,h.map));h.key_token.set(retain(id,roots,key));
        collect(id,gc);assert!(!present(id,h.old_node.get()),"completed lookup must release detached old node");
        h.mutate.set(false);h.failed.set(true);
        let error=id.send_named(block,b"value:value:",&[Operand::object(h.map),Operand::object(key)]).unwrap_err();assert_eq!((error.status,error.detail),(812,19));h.failed.set(false);
        // Fixed Reflect forwarding and Symbol hashing likewise need no input token.
        let reflect=named(b"Reflect");let symbol=id.intern_name(b"borrowed-symbol").unwrap();let st=retain(id,roots,symbol);
        h.inputs.replace(vec![reflect,value,symbol]);h.input_retains.set(0);h.query=bind(id.receiver,b"behaviorOf:");
        install(id,id.receiver,b"behaviorOf:",collecting_query,c);
        assert_eq!(object(send(id,reflect,b"vtableOf:",&[Operand::object(value)])),named(b"ObjectVt"));
        assert_eq!(h.input_retains.get(),0);h.failed.set(true);
        let error=id.send_named(reflect,b"vtableOf:",&[Operand::object(value)]).unwrap_err();assert_eq!((error.status,error.detail),(813,20));h.failed.set(false);
        install(id,id.receiver,b"behaviorOf:",h.query.method.unwrap(),h.query.context);
        h.query=bind(id.receiver,b"symbolBytesOf:");install(id,id.receiver,b"symbolBytesOf:",collecting_query,c);
        assert!(send(id,symbol,b"collectionHash",&[]).as_word().is_some());assert_eq!(h.input_retains.get(),0);
        install(id,id.receiver,b"symbolBytesOf:",h.query.method.unwrap(),h.query.context);
        let wb=named(b"WordVt");h.inputs.replace(vec![wb]);h.input_retains.set(0);
        h.query=bind(gc,b"canOmitAllocationFor:");let gb=object(send(id,id.receiver,b"behaviorOf:",&[Operand::object(gc)]));
        install(id,gb,b"canOmitAllocationFor:",collecting_query,c);
        assert!(send(id,h.model,b"canOmitAllocationFor:",&[Operand::object(wb)]).as_word().is_some());
        assert_eq!(h.input_retains.get(),0,"permission query must borrow its behavior input");
        h.failed.set(true);let error=id.send_named(h.model,b"canOmitAllocationFor:",&[Operand::object(wb)]).unwrap_err();
        assert_eq!((error.status,error.detail),(813,20));h.failed.set(false);
        install(id,gb,b"canOmitAllocationFor:",h.query.method.unwrap(),h.query.context);
        // Vector language operations borrow the same ABI input coverage. Keep
        // both its tree and tail snapshot when a conversion callback detaches
        // them, including during a failed read.
        let vector_behavior=named(b"VectorVt");
        h.only_vector.set(true);install(id,vector_behavior,b"at:",vector_entry,c);
        for block in [block,resumable] {
        h.inputs.borrow_mut().clear();
        let mut vector=object(send(id,named(b"Vector"),b"new",&[]));
        let mut vector_token=retain(id,roots,vector);
        let mut replacement=object(send(id,named(b"Vector"),b"new",&[]));
        let mut replacement_token=retain(id,roots,replacement);
        for _ in 0..40 {
            for (v,t,element) in [(&mut vector,&mut vector_token,value),(&mut replacement,&mut replacement_token,symbol)] {
                let next=object(send(id,*v,b"add:",&[Operand::object(element)]));
                let token=retain(id,roots,next);release(id,roots,*t);*v=next;*t=token;
            }
        }
        let nodes=|v| [2,3].map(|i|object(send(id,h.model,b"slotAt:in:",&[Operand::word(i),Operand::object(v)])));
        let [old_root,old_tail]=nodes(vector);let [new_root,new_tail]=nodes(replacement);
        let integer=named(b"Integer");let ib=object(send(id,id.receiver,b"behaviorOf:",&[Operand::object(integer)]));
        let index=object(send(id,integer,b"fromWord:",&[Operand::word(39)]));
        h.map=vector;h.key=index;h.map_token.set(vector_token);h.key_token.set(retain(id,roots,index));
        h.inputs.replace(vec![vector,index]);h.input_retains.set(0);
        h.vector_nodes.set(Some([old_root,old_tail,new_root,new_tail]));
        h.query=bind(integer,b"wordOf:");install(id,ib,b"wordOf:",collecting_query,c);
        assert_eq!(object(send(id,block,b"value:value:",&[Operand::object(vector),Operand::object(index)])),value);
        assert_eq!(h.input_retains.get(),0,"Vector must not re-register caller inputs");
        h.vector_nodes.set(None);h.map_token.set(retain(id,roots,vector));h.key_token.set(retain(id,roots,index));
        collect(id,gc);assert!(!present(id,old_root)&&!present(id,old_tail),"completed read releases detached snapshot");
        h.input_retains.set(0);h.failed.set(true);
        let error=id.send_named(block,b"value:value:",&[Operand::object(vector),Operand::object(index)]).unwrap_err();
        assert_eq!((error.status,error.detail),(813,20));assert_eq!(h.input_retains.get(),0);h.failed.set(false);
        install(id,ib,b"wordOf:",h.query.method.unwrap(),h.query.context);
        release(id,roots,replacement_token);
        for token in [&h.map_token,&h.key_token] {let t=token.replace(0);if t!=0 {release(id,roots,t);}}
        }
        install(id,vector_behavior,b"at:",h.vector_entry.method.unwrap(),h.vector_entry.context);
        install(id,rb,b"retain:",h.retainer.method.unwrap(),h.retainer.context);
        for token in [rst,st,blt,cap,ct,bytet,et,vt,bt]{release(id,roots,token);}
    }
}
