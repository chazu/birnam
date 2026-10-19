//! Run the production authored Globals initializer on a floor with no Store.
//! Load its real compiled initializer; no exported-body substitute or Globals seed.
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
#[ignore = "scripts/globals-without-store-test.py supplies an actual Store-absent artifact floor"]
fn authored_bindings_work_without_store_or_relational() { unsafe {
    for name in ["store","relational","bootstrap","generation","lang"] { assert!(!directory().join(format!("libbirnam_{name}.dylib")).exists()); }
    let id=id(); let roots=load(id,"root_registry",&[]);
    let ns=load(id,"namespace",&[Operand::object(roots)]);
    let h=H{id,ns,roots,tokens:RefCell::new(Vec::new())};
    let prototype=object(send(id,id.receiver,b"objectPrototype",&[]));
    let vt=object(send(id,id.receiver,b"behaviorOf:",&[Operand::object(prototype)]));
    h.publish(b"Object",prototype); h.publish(b"ObjectVt",vt);
    for module in ["boolean","nil"] { mount(id,module,ns); }
    let gc=load(id,"collect",&[Operand::object(roots)]); h.publish(b"GC",gc); send(id,gc,b"activate",&[]);
    for module in ["object_model","word","bytes","execution","portable_code","string","integer","float","prelude","file_io"] {
        mount(id,module,ns);
    }
    let collection=h.keep(object(send(id,vt,b"delegated",&[])));
    h.publish(b"Collection",collection); h.publish(b"CollectionVt",collection);
    let symbol=id.intern_name(b"Symbol").unwrap();
    h.publish(b"SymbolVt",object(send(id,id.receiver,b"behaviorOf:",&[Operand::object(symbol)])));
    for module in ["vector","map","artifact","tcc","native_backend","codegen_runtime"] {
        mount(id,module,ns);
    }
    let nil=h.named(b"nil");
    assert_eq!(send(id,ns,b"at:",&[Operand::object(id.intern_name(b"Globals").unwrap())]).kind(),birnam_abi::id_protocol::UNIT);
    assert_eq!(object(send(id,ns,b"resolveGlobal:",&[Operand::object(id.intern_name(b"Object").unwrap())])),prototype);
    assert_eq!(object(send(id,ns,b"resolveGlobal:",&[Operand::object(id.intern_name(b"missingBeforeGlobals").unwrap())])),nil);
    let load_artifact=|name:&str| {
        let path=directory().join(format!("libbirnam_{name}.dylib"));
        let path=h.keep(object(send(id,h.named(b"String"),b"fromBytes:",&[Operand::bytes(path.as_os_str().as_bytes())])));
        h.call(h.named(b"Codegen"),b"load:",&[path])
    };
    load_artifact("error");
    let globals=load_artifact("globalsOwner");
    assert_eq!(globals,h.named(b"Globals"));
    assert_ne!(globals,nil);
    for name in [b"Store".as_slice(),b"db",b"Relational",b"Lang",b"Generation"] {
        assert!(send(id,ns,b"at:",&[Operand::object(id.intern_name(name).unwrap())]).as_object().is_none());
    }
    let name=id.intern_name(b"owned").unwrap();
    assert_eq!(h.call(globals,b"includes:",&[name]),h.named(b"false"));
    h.call(globals,b"at:put:",&[name,nil]);
    assert_eq!(h.call(globals,b"includes:",&[name]),h.named(b"true"));
    let before=h.call(globals,b"bindingAt:",&[name]);assert_eq!(h.number(h.call(before,b"size",&[])),1);
    let value=h.call(prototype,b"clone",&[]);
    h.call(globals,b"at:put:",&[name,value]);
    collect(id,gc);assert_eq!(h.call(globals,b"at:",&[name]),value);
    assert_eq!(h.call(before,b"at:",&[h.integer(0)]),nil,"snapshot changed");
    let foreign=id.intern_name(b"foreign").unwrap();h.call(globals,b"at:put:",&[foreign,value]);
    h.call(globals,b"remove:",&[name]);
    assert_eq!(h.call(globals,b"includes:",&[name]),h.named(b"false"));
    assert_eq!(h.call(globals,b"at:",&[foreign]),value);
    let a=h.keep(object(send(id,h.named(b"String"),b"fromBytes:",&[Operand::bytes(b"same")])));
    let b=h.keep(object(send(id,h.named(b"String"),b"fromBytes:",&[Operand::bytes(b"same")])));
    assert_ne!(a,b);h.call(globals,b"at:put:",&[a,name]);h.call(globals,b"at:put:",&[b,foreign]);
    assert_eq!(h.call(globals,b"at:",&[a]),name);assert_eq!(h.call(globals,b"at:",&[b]),foreign);
    h.call(globals,b"remove:",&[a]);assert_eq!(h.call(globals,b"includes:",&[a]),h.named(b"false"));
    assert_eq!(h.call(globals,b"at:",&[b]),foreign);
    let snapshot=h.call(globals,b"bindingState",&[]);
    h.call(globals,b"remove:",&[b]);
    let bindings=h.call(snapshot,b"at:",&[h.integer(0)]);
    let key=h.call(b,b"bindingKey",&[]);
    assert_eq!(h.call(bindings,b"at:",&[key]),foreign);
    assert_eq!(h.number(h.call(snapshot,b"size",&[])),4);
    let floor=h.tokens.borrow().len();
    let key=h.call(prototype,b"clone",&[]);let transient=h.call(prototype,b"clone",&[]);
    h.call(globals,b"at:put:",&[key,transient]);h.clear_from(floor);collect(id,gc);
    assert!(present(id,key)&&present(id,transient));
    h.call(globals,b"remove:",&[key]);h.clear_from(floor);collect(id,gc);
    assert!(!present(id,key)&&!present(id,transient));
    h.call(globals,b"at:put:",&[a,name]);
    let owner=h.call(globals,b"bindingOwner",&[]);
    let state=h.call(globals,b"bindingState",&[]);
    assert_eq!(load_artifact("globalsOwner"),globals);
    assert_eq!(h.call(globals,b"bindingOwner",&[]),owner);
    assert_eq!(h.call(globals,b"bindingState",&[]),state);
    assert_eq!(h.call(globals,b"at:",&[a]),name);
    assert_eq!(h.call(globals,b"includes:",&[b]),h.named(b"false"));
    assert_eq!(h.call(globals,b"at:",&[foreign]),value);

    // Optional loading is deliberately a second phase, from a separate input
    // directory. It must not overwrite the binding owner or seed global facts.
    let optional=PathBuf::from(std::env::var_os("BIRNAM_OPTIONAL_MODULE_DIRECTORY").unwrap());
    for name in ["store","relational"] {
        let path=optional.join(format!("libbirnam_{name}.dylib"));
        id.send(id.receiver,id.load,&[Operand::bytes(path.as_os_str().as_bytes()),Operand::object(ns)]).unwrap();
        assert_eq!(h.named(b"Globals"),globals);
        assert_eq!(h.call(globals,b"bindingOwner",&[]),owner);
        assert_eq!(h.call(globals,b"at:",&[foreign]),value);
    }
    let store=h.named(b"Store");
    let entity=id.intern_name(b"__globals__").unwrap();
    assert_eq!(h.call(store,b"hasValue:attr:",&[entity,foreign]),h.named(b"false"));
    h.call(store,b"assert:attr:value:",&[entity,foreign,nil]);
    assert_eq!(h.call(globals,b"at:",&[foreign]),value);
    let basis=h.call(store,b"basis",&[]);
    h.call(globals,b"at:put:",&[name,value]);
    assert_eq!(h.number(h.call(store,b"basis",&[])),h.number(basis));
    // Repeated opt-in keeps the same Store and its explicit data as well.
    for module in ["store","relational"] {
        let path=optional.join(format!("libbirnam_{module}.dylib"));
        id.send(id.receiver,id.load,&[Operand::bytes(path.as_os_str().as_bytes()),Operand::object(ns)]).unwrap();
    }
    assert_eq!(h.named(b"Store"),store);
    assert_eq!(h.call(store,b"hasValue:attr:",&[entity,foreign]),h.named(b"true"));
    assert_eq!(h.call(globals,b"bindingOwner",&[]),owner);
    assert_eq!(h.call(globals,b"at:",&[foreign]),value);
    h.clear_from(0);
} }
