//! Run the production authored Globals method bodies on a floor with no Store.
//! Programs are exported from the installed Cell artifact, never rewritten here.
mod support;
use birnam_abi::{Oop, id_protocol::{Operand, Protocol}};
use std::{cell::RefCell, path::PathBuf};
use std::os::unix::ffi::OsStrExt;
use support::*;

struct H { id: Protocol, ns: Oop, roots: Oop, tokens: RefCell<Vec<usize>> }
unsafe fn mount(id: Protocol, module: &str, ns: Oop) {
    let path=directory().join(format!("libbirnam_{module}.dylib"));
    unsafe { id.send(id.receiver,id.load,&[Operand::bytes(path.as_os_str().as_bytes()),Operand::object(ns)]) }
        .unwrap_or_else(|e| panic!("load {module}: {e:?}"));
}
impl H {
    unsafe fn keep(&self, o: Oop) -> Oop {
        self.tokens.borrow_mut().push(unsafe { retain(self.id,self.roots,o) }); o
    }
    unsafe fn named(&self, name: &[u8]) -> Oop {
        unsafe { object(send(self.id,self.ns,b"at:",&[Operand::object(self.id.intern_name(name).unwrap())])) }
    }
    unsafe fn call(&self, r: Oop, name: &[u8], args: &[Oop]) -> Oop {
        unsafe { self.keep(object(send(self.id,r,name,&args.iter().copied().map(Operand::object).collect::<Vec<_>>()))) }
    }
    unsafe fn integer(&self, value: usize) -> Oop {
        unsafe { self.keep(object(send(self.id,self.named(b"Integer"),b"fromWord:",&[Operand::word(value)]))) }
    }
    unsafe fn number(&self, value: Oop) -> usize {
        unsafe { send(self.id,self.named(b"Integer"),b"wordOf:",&[Operand::object(value)]).as_word().unwrap() }
    }
    unsafe fn publish(&self, name: &[u8], value: Oop) {
        unsafe { send(self.id,self.ns,b"at:put:",&[Operand::object(self.id.intern_name(name).unwrap()),Operand::object(value)]); }
    }
    unsafe fn clear_from(&self, floor: usize) {
        for token in self.tokens.borrow_mut().drain(floor..).rev() { unsafe { release(self.id,self.roots,token); } }
    }
}

#[test]
#[ignore = "scripts/globals-without-store-test.py exports actual production programs"]
fn authored_bindings_work_without_store_or_relational() { unsafe {
    let programs=PathBuf::from(std::env::var_os("BIRNAM_GLOBAL_PROGRAMS").expect("exported programs"));
    for name in ["store","relational","bootstrap","generation","lang"] { assert!(!directory().join(format!("libbirnam_{name}.dylib")).exists()); }
    let id=id(); let roots=load(id,"root_registry",&[]);
    let ns=load(id,"namespace",&[Operand::object(roots)]);
    let h=H{id,ns,roots,tokens:RefCell::new(Vec::new())};
    let prototype=object(send(id,id.receiver,b"objectPrototype",&[]));
    let vt=object(send(id,id.receiver,b"behaviorOf:",&[Operand::object(prototype)]));
    h.publish(b"Object",prototype); h.publish(b"ObjectVt",vt);
    for module in ["boolean","nil"] { mount(id,module,ns); }
    let gc=load(id,"collect",&[Operand::object(roots)]); h.publish(b"GC",gc); send(id,gc,b"activate",&[]);
    for module in ["object_model","word","bytes","execution","portable_code","string","integer"] {
        mount(id,module,ns);
    }
    let collection=h.keep(object(send(id,vt,b"delegated",&[])));
    h.publish(b"Collection",collection); h.publish(b"CollectionVt",collection);
    let symbol=id.intern_name(b"Symbol").unwrap();
    h.publish(b"SymbolVt",object(send(id,id.receiver,b"behaviorOf:",&[Operand::object(symbol)])));
    for module in ["vector","map","tcc","native_backend","codegen_runtime"] {
        mount(id,module,ns);
    }
    // Namespace is a real binding owner, not a fake database. Only the compiler's
    // existing Globals-first read convention needs this alias in the small floor.
    h.publish(b"Globals",ns);

    let model=h.named(b"ObjectModel");let nil=h.named(b"nil");
    let vector=|values:&[Oop]| { let mut v=h.call(h.named(b"Vector"),b"new",&[]);for &value in values {v=h.call(v,b"add:",&[value]);}v };
    let empty=vector(&[]);let map=h.call(h.named(b"Map"),b"new",&[]);
    let state=vector(&[map,empty,h.integer(0),map,map]);
    let storage=h.call(model,b"newSlots:parent:",&[vector(&[state]),vt]);
    let globals=h.call(model,b"newSlots:parent:",&[empty,vt]);
    let key_prototype=h.call(model,b"newSlots:parent:",&[vector(&[nil]),vt]);
    let error=h.call(model,b"newSlots:parent:",&[empty,vt]);
    let error_prototype=h.call(model,b"newSlots:parent:",&[vector(&[nil,nil]),vt]);
    let zero=h.keep(object(send(id,h.named(b"Word"),b"fromBits:",&[Operand::word(0)])));
    let owners=std::collections::HashMap::from([
        ("globals",globals),("storage",storage),("keyPrototype",key_prototype),("model",model),
        ("object",prototype),("symbolBehavior",h.named(b"SymbolVt")),("nil",nil),("map",h.named(b"Map")),
        ("reflect",h.named(b"Reflect")),("error",error),("errorPrototype",error_prototype),("word0",zero),("index0",h.integer(0)),("index1",h.integer(1)),("index2",h.integer(2)),("index3",h.integer(3)),("index4",h.integer(4))]);
    let plan:serde_json::Value=serde_json::from_slice(&std::fs::read(programs.join("plan.json")).unwrap()).unwrap();
    for (index,row) in plan.as_array().unwrap().iter().enumerate() {
        let target=owners[row["target"].as_str().unwrap()];
        let behavior=if row["target"]=="symbolBehavior" {target} else {object(send(id,id.receiver,b"behaviorOf:",&[Operand::object(target)]))};
        let captures=vector(&row["captures"].as_array().unwrap().iter().map(|name|owners[name.as_str().unwrap()]).collect::<Vec<_>>());
        let bytes=std::fs::read(programs.join(format!("{index}.bin"))).unwrap();
        let bytes=h.keep(object(send(id,h.named(b"Bytes"),b"fromBytes:",&[Operand::bytes(&bytes)])));
        let mut code=h.call(h.named(b"CodeObject"),b"fromCanonicalBytes:",&[bytes]);
        if let Some(signature)=row["signature"].as_str().filter(|s|!s.is_empty()) {
            let signature=h.keep(object(send(id,h.named(b"Bytes"),b"fromBytes:",&[Operand::bytes(signature.as_bytes())])));
            code=h.call(code,b"withNativeSignature:",&[signature]);
        }
        let block=h.call(h.named(b"Codegen"),b"materializeResumable:captures:",&[code,captures]);
        let source=h.call(h.named(b"NativeBackend"),b"named:synchronous:",&[h.integer(row["arity"].as_u64().unwrap() as usize),h.named(b"true")]);
        let unit=h.call(h.named(b"TCC"),b"compile:",&[source]);let method=h.call(unit,b"initializeWith:",&[block]);
        h.call(behavior,b"addMethod:as:",&[method,id.intern_name(row["selector"].as_str().unwrap().as_bytes()).unwrap()]);
    }
    h.publish(b"Globals",globals);
    h.publish(b"Error",error);h.publish(b"ErrorPrototype",error_prototype);
    for name in [b"Store".as_slice(),b"db",b"Relational",b"Lang",b"Generation"] {
        assert!(send(id,ns,b"at:",&[Operand::object(id.intern_name(name).unwrap())]).as_object().is_none());
    }
    let name=id.intern_name(b"owned").unwrap();
    assert_eq!(h.call(globals,b"includes:",&[name]),h.named(b"false"));
    h.call(globals,b"at:put:",&[name,nil]);
    assert_eq!(h.call(globals,b"includes:",&[name]),h.named(b"true"));
    let before=h.call(globals,b"bindingAt:",&[name]);assert_eq!(h.number(h.call(before,b"size",&[])),1);
    let value=h.call(prototype,b"clone",&[]);let token=h.call(prototype,b"clone",&[]);
    h.call(globals,b"reserveBindingCompensationFor:count:",&[token,h.integer(3)]);
    assert_eq!(h.call(globals,b"restoreBinding:expected:previous:reservation:",&[name,before,vector(&[value]),token]),globals);
    collect(id,gc);assert_eq!(h.call(globals,b"at:",&[name]),value);
    let foreign=id.intern_name(b"foreign").unwrap();h.call(globals,b"at:put:",&[foreign,value]);
    h.call(globals,b"restoreBinding:expected:previous:reservation:",&[name,vector(&[value]),empty,token]);
    assert_eq!(h.call(globals,b"includes:",&[name]),h.named(b"false"));
    assert_eq!(h.call(globals,b"at:",&[foreign]),value);
    h.call(globals,b"releaseBindingCompensationFor:",&[token]);
    let failure=h.call(globals,b"restoreBinding:expected:previous:reservation:",&[name,empty,before,token]);
    assert_eq!(h.call(failure,b"kind",&[]),id.intern_name(b"compensationConflict").unwrap());
    let a=h.keep(object(send(id,h.named(b"String"),b"fromBytes:",&[Operand::bytes(b"same")])));
    let b=h.keep(object(send(id,h.named(b"String"),b"fromBytes:",&[Operand::bytes(b"same")])));
    assert_ne!(a,b);h.call(globals,b"at:put:",&[a,name]);h.call(globals,b"at:put:",&[b,foreign]);
    assert_eq!(h.call(globals,b"at:",&[a]),name);assert_eq!(h.call(globals,b"at:",&[b]),foreign);
    h.call(globals,b"remove:",&[a]);assert_eq!(h.call(globals,b"includes:",&[a]),h.named(b"false"));
    assert_eq!(h.call(globals,b"at:",&[b]),foreign);
    let checkpoint=h.call(globals,b"imageBindingCheckpoint",&[]);
    h.call(globals,b"remove:",&[b]);h.call(globals,b"restoreImageBindings:",&[checkpoint]);
    assert_eq!(h.call(globals,b"at:",&[b]),foreign);
    h.call(globals,b"reserveBindingCompensationFor:count:",&[token,h.integer(2)]);
    h.call(globals,b"releaseImageReservations",&[]);
    assert_eq!(h.number(h.call(globals,b"reservedBindingCompensationFor:",&[token])),0);
    let floor=h.tokens.borrow().len();
    let key=h.call(prototype,b"clone",&[]);let transient=h.call(prototype,b"clone",&[]);
    h.call(globals,b"at:put:",&[key,transient]);h.clear_from(floor);collect(id,gc);
    assert!(present(id,key)&&present(id,transient));
    h.call(globals,b"remove:",&[key]);h.clear_from(floor);collect(id,gc);
    assert!(!present(id,key)&&!present(id,transient));
    h.clear_from(0);
} }
