//! Real compiled owners and source-free Image recovery with Store/Relational absent.
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
#[ignore = "scripts/image-without-store-test.py supplies an actual Store-absent artifact floor"]
fn image_roundtrips_without_store_or_relational() { unsafe {
    let mode=std::env::var("BIRNAM_IMAGE_FLOOR_MODE").unwrap();
    for name in ["relational", "bootstrap", "isolate"] {
        assert!(!directory().join(format!("libbirnam_{name}.dylib")).exists());
    }
    if !mode.starts_with("optional-") { assert!(!directory().join("libbirnam_store.dylib").exists()); }
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
    for name in ["error", "globalsOwner", "sourceRegistry", "cell", "modules", "lang", "nativeValues", "booleanValues", "integerValues", "blockLoopControl", "blockLoop", "range", "systemGc", "printing", "sourceLoader", "nativeLoader", "alienCompiler", "alienRecipes", "imageFormat", "trace", "globals", "methodInvocation", "stdlib", "cli"] { artifact(name); }
    mount(id,"alien",ns);
    let eval=|source: &[u8]| h.call(h.named(b"Codegen"), b"evalPersisting:", &[text(source)]);
    let module=h.call(h.named(b"Globals"),b"at:",&[id.intern_name(b"Module").unwrap()]);
    h.call(module,b"libDir:",&[text(directory().as_os_str().as_bytes())]);
    mount(id,"image",ns);
    for name in [b"Store".as_slice(),b"db",b"Relational"] {
        assert!(send(id,ns,b"at:",&[Operand::object(id.intern_name(name).unwrap())]).as_object().is_none());
    }
    assert_eq!(h.call(h.named(b"System"),b"imageStore",&[]),h.named(b"nil"));
    let reflect=h.named(b"Reflect");
    let reflect_behavior=object(send(id,id.receiver,b"behaviorOf:",&[Operand::object(reflect)]));
    assert!(send(id,id.receiver,b"bindingOf:in:",&[Operand::object(id.intern_name(b"factsOf:").unwrap()),Operand::object(reflect_behavior)]).as_entry().is_none());
    assert_eq!(eval(include_bytes!("../../../tests/fixtures/documentation-hooks.birnam")),h.named(b"true"));
    assert_eq!(eval(include_bytes!("../../../tests/fixtures/cli-lsp-boundary.birnam")),h.named(b"true"));
    collect(id,gc);

    let path=std::env::var("BIRNAM_IMAGE_FLOOR_PATH").unwrap();
    let path=text(path.as_bytes());
    // The only added library in the positive controls is Store.
    if mode.starts_with("optional-") {
        mount(id,"store",ns);
        assert_eq!(eval(br#"[Module kernel: #store]; [Module field: #imageProvider of: #store put: true]; true"#),h.named(b"true"));
    }
    if mode=="optional-save" {
        assert_eq!(eval(br#"let optionalImageStore := Store;
[Reflect defineMethod: #imageStore on: System as: [:self | optionalImageStore]];
[Store assert: #optionalFloor attr: #value value: 7]; true"#),h.named(b"true"));
        assert_eq!(h.call(h.named(b"System"),b"saveImage:",&[path]),h.named(b"true"));
    } else if mode=="save" {
        assert_eq!(eval(br#"let coreImageCell := [Cell with: #before];
let coreImageAlias := coreImageCell;
[Reflect defineMethod: #coreImageAnswer on: Integer as: [:self | 42]];
[Alien wrap: "labs" signature: "l l" as: #coreImageAbs on: Integer]; true"#),h.named(b"true"));
        assert_eq!(h.call(h.named(b"System"),b"saveImage:",&[path]),h.named(b"true"));
    } else {
        let rebuild=mode.ends_with("rebuild");
        let answer=h.call(h.named(b"System"),b"loadImage:rebuild:",&[path,h.named(if rebuild {b"true"} else {b"false"})]);
        if mode=="missing" {
            assert!(answer == h.named(b"nil") || h.call(answer,b"isError",&[]) == h.named(b"true"));
            let report=h.call(h.named(b"System"),b"imageSaveReport",&[]);
            assert_eq!(h.call(report,b"status",&[]),id.intern_name(b"rejected").unwrap());
        } else if mode.starts_with("optional-") {
            assert_eq!(answer,h.named(if rebuild {b"false"} else {b"true"}));
            assert_eq!(eval(br#"[[[Store currentValue: #optionalFloor attr: #value] = 7] & [[System imageStore] == Store]]"#),h.named(b"true"));
        } else {
            assert_eq!(answer,h.named(if rebuild {b"false"} else {b"true"}));
            assert_eq!(eval(br#"[[|
[System gc];
let alias := [coreImageCell == coreImageAlias];
let before := [[coreImageCell value] == #before];
[coreImageAlias set: #changed]; [System gc];
let changed := [[coreImageCell value] == #changed];
[coreImageCell set: #before];
[[[alias & before] & changed] & [[[1 coreImageAnswer] = 42] & [[-9 coreImageAbs] = 9]]]
] value]"#),h.named(b"true"));
            if mode=="resave" {
                let output=std::env::var("BIRNAM_IMAGE_FLOOR_OUTPUT").unwrap();
                assert_eq!(h.call(h.named(b"System"),b"saveImage:",&[text(output.as_bytes())]),h.named(b"true"));
            }
        }
    }
    h.clear_from(0); collect(id,gc);
}}
