//! Real compiled Alien recipe owner and native wrapping, with Store/Relational absent.
mod support;
use birnam_abi::{Oop, id_protocol::{Operand, Protocol}};
use std::cell::RefCell;
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
    unsafe fn publish(&self, name: &[u8], value: Oop) {
        unsafe { send(self.id,self.ns,b"at:put:",&[Operand::object(self.id.intern_name(name).unwrap()),Operand::object(value)]); }
    }
    unsafe fn clear_from(&self, floor: usize) {
        for token in self.tokens.borrow_mut().drain(floor..).rev() { unsafe { release(self.id,self.roots,token); } }
    }
}

#[test]
#[ignore = "scripts/alien-without-store-test.py supplies an actual Store-absent artifact floor"]
fn alien_wraps_without_store_or_relational() { unsafe {
    for name in ["store", "relational", "bootstrap", "image", "isolate"] {
        assert!(!directory().join(format!("libbirnam_{name}.dylib")).exists());
    }
    let id=id(); let roots=load(id,"root_registry",&[]);
    let ns=load(id,"namespace",&[Operand::object(roots)]);
    let h=H{id,ns,roots,tokens:RefCell::new(Vec::new())};
    let prototype=object(send(id,id.receiver,b"objectPrototype",&[]));
    let vt=object(send(id,id.receiver,b"behaviorOf:",&[Operand::object(prototype)]));
    h.publish(b"Object",prototype); h.publish(b"ObjectVt",vt); h.publish(b"Vtable",id.receiver);
    for module in ["boolean","nil"] { mount(id,module,ns); }
    let gc=load(id,"collect",&[Operand::object(roots)]); h.publish(b"GC",gc); send(id,gc,b"activate",&[]);
    for module in ["object_model","word","bytes","execution","portable_code","string","integer","float"] { mount(id,module,ns); }
    let string=h.named(b"String");
    for name in [b"true".as_slice(), b"nil"] { send(id,h.named(name), b"installPrinting:", &[Operand::object(string)]); }
    let symbol=id.intern_name(b"Symbol").unwrap();
    h.publish(b"SymbolVt",object(send(id,id.receiver,b"behaviorOf:",&[Operand::object(symbol)])));
    for module in ["prelude","system_output","system_input","environment","file_io","storage"] { mount(id,module,ns); }
    send(id,h.named(b"Integer"), b"installFloat:", &[Operand::object(h.named(b"Float"))]);
    send(id,string, b"installNumbers:", &[Operand::object(h.named(b"Integer")), Operand::object(h.named(b"Float"))]);
    let collection=h.keep(object(send(id,vt,b"delegated",&[])));
    h.publish(b"Collection",collection); h.publish(b"CollectionVt",collection);
    send(id,id.receiver,b"parentOf:put:", &[Operand::object(h.named(b"StringVt")), Operand::object(collection)]);
    for module in ["vector","map"] { mount(id,module,ns); }
    send(id,string, b"installCollections:", &[Operand::object(h.named(b"Vector"))]);
    for module in ["artifact","peg","tcc","native_backend","codegen_runtime","generation"] { mount(id,module,ns); }
    let text=|bytes: &[u8]| h.keep(object(send(id,string,b"fromBytes:",&[Operand::bytes(bytes)])));
    let artifact=|name: &str| {
        eprintln!("loading actual initializer: {name}");
        let path=directory().join(format!("libbirnam_{name}.dylib"));
        let result=h.call(h.named(b"Codegen"), b"load:", &[text(path.as_os_str().as_bytes())]);
        assert_ne!(result,h.named(b"nil"), "nil initializer: {name}");
        assert_eq!(h.call(result,b"isError",&[]),h.named(b"false"),"error initializer: {name}");
    };
    for name in ["error", "globalsOwner", "sourceRegistry", "cell", "modules", "lang", "nativeValues", "booleanValues", "integerValues", "blockLoopControl", "blockLoop", "range", "systemGc", "printing", "sourceLoader", "nativeLoader", "alienCompiler", "alienRecipes", "imageFormat", "trace", "globals", "methodInvocation", "stdlib"] { artifact(name); }
    mount(id,"alien",ns);
    let eval=|source: &[u8]| h.call(h.named(b"Codegen"), b"evalPersisting:", &[text(source)]);
    for name in [b"Store".as_slice(),b"db",b"Relational"] {
        assert!(send(id,ns,b"at:",&[Operand::object(id.intern_name(name).unwrap())]).as_object().is_none());
    }
    assert_eq!(eval(br#"[[|
let before := [AlienRecipes objectFor: "labs" signature: "l l"];
let wrapped := [Alien wrap: "labs" signature: "l l" as: #floorAbsolute on: Integer];
let code := [AlienRecipes objectFor: "labs" signature: "l l"];
[System gc];
[[[before isNil] & [wrapped == Integer]] & [[[-7 floorAbsolute] = 7] & [code notNil]]]
] value]"#),h.named(b"true"));
    assert_eq!(eval(br#"[[|
let code := [AlienRecipes objectFor: "labs" signature: "l l"];
[Alien wrap: "labs" signature: "l l" as: #floorAbsolute on: Integer];
let reused := [code == [AlienRecipes objectFor: "labs" signature: "l l"]];
let active := [[AlienRecipes snapshotRegistrations] size];
[Reflect defineMethod: #floorAbsolute on: Integer as: [:self | 99]];
[System gc];
[reused & [[active = 1] & [[[-7 floorAbsolute] = 99] & [[[AlienRecipes snapshotRegistrations] size] = 0]]]]
] value]"#),h.named(b"true"));
    h.clear_from(0); collect(id,gc);
}}
