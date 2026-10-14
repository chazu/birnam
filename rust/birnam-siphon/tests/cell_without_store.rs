//! Run the production authored Cell method bodies on a floor with no Store.
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
#[ignore = "scripts/cell-without-store-test.py exports production programs and selects a Store-free prefix"]
fn authored_cells_mutate_and_collect_without_store() { unsafe {
    let programs=PathBuf::from(std::env::var_os("BIRNAM_CELL_PROGRAMS").expect("exported Cell programs"));
    assert!(!directory().join("libbirnam_store.dylib").exists());
    assert!(!directory().join("libbirnam_relational.dylib").exists());
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
    let model=h.named(b"ObjectModel"); let nil=h.named(b"nil");
    let values=h.call(h.named(b"Vector"),b"new",&[]);
    let values=h.call(values,b"add:",&[nil]);
    let cell=h.call(model,b"newSlots:parent:",&[values,vt]); h.publish(b"Cell",cell);
    let behavior=object(send(id,id.receiver,b"behaviorOf:",&[Operand::object(cell)]));
    let captures=h.call(h.named(b"Vector"),b"new",&[]);
    for (index,selector,arity) in [(0,b"with:".as_slice(),1),(1,b"value",0),(2,b"set:",1),(3,b"history",0),(4,b"inc",0),(5,b"dec",0)] {
        let bytes=std::fs::read(programs.join(format!("{index}.bin"))).unwrap();
        let bytes=h.keep(object(send(id,h.named(b"Bytes"),b"fromBytes:",&[Operand::bytes(&bytes)])));
        let code=h.call(h.named(b"CodeObject"),b"fromCanonicalBytes:",&[bytes]);
        let block=h.call(h.named(b"Codegen"),b"materializeResumable:captures:",&[code,captures]);
        let source=h.call(h.named(b"NativeBackend"),b"named:synchronous:",&[h.integer(arity),h.named(b"true")]);
        let unit=h.call(h.named(b"TCC"),b"compile:",&[source]);
        let method=h.call(unit,b"initializeWith:",&[block]);
        // The raw binding mechanism is the supported bootstrap path. No Store
        // MOP, global facts, source parser or source-metadata registry is loaded.
        let selector=h.keep(id.intern_name(selector).unwrap());
        h.call(behavior,b"addMethod:as:",&[method,selector]);
    }
    for name in [b"Store".as_slice(),b"db",b"Relational",b"Lang",b"Generation"] {
        assert!(send(id,ns,b"at:",&[Operand::object(id.intern_name(name).unwrap())]).as_object().is_none());
    }
    let floor=h.tokens.borrow().len();
    let value=h.call(prototype,b"clone",&[]);
    let local=h.call(cell,b"with:",&[value]);
    let local_token=retain(id,roots,local);
    h.clear_from(floor); collect(id,gc);
    assert!(present(id,local) && present(id,value));
    assert_eq!(h.call(local,b"value",&[]),value);
    h.call(local,b"set:",&[h.integer(41)]);
    assert_eq!(h.number(h.call(local,b"inc",&[])),42);
    assert_eq!(h.number(h.call(local,b"dec",&[])),41);
    assert_eq!(h.number(h.call(local,b"history",&[])),0);
    let clone=h.call(local,b"clone",&[]);
    assert_ne!(clone,local);
    h.call(clone,b"set:",&[nil]);
    assert_eq!(h.number(h.call(local,b"value",&[])),41);
    assert_eq!(h.call(clone,b"value",&[]),nil);
    h.clear_from(floor); release(id,roots,local_token); collect(id,gc);
    assert!(!present(id,local) && !present(id,clone) && !present(id,value));
    h.clear_from(0);
} }
