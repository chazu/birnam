//! Real compiled task initializer and dependencies, with Store/Relational absent.
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
#[ignore = "scripts/task-without-store-test.py supplies an actual Store-absent artifact floor"]
fn tasks_work_without_store_or_relational() { unsafe {
    for name in ["store", "relational", "bootstrap", "image", "alien", "isolate"] {
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
    for name in ["error", "globalsOwner", "sourceRegistry", "cell", "modules", "lang", "nativeValues", "booleanValues", "integerValues", "blockLoopControl", "blockLoop", "range", "systemGc", "printing", "sourceLoader", "nativeLoader", "alienCompiler", "imageFormat", "trace", "globals", "methodInvocation", "stdlib"] { artifact(name); }
    let globals=h.named(b"Globals");
    let module=h.call(globals,b"at:",&[id.intern_name(b"Module").unwrap()]);
    h.call(module,b"markLoaded:",&[id.intern_name(b"stdlib").unwrap()]);
    artifact("stacklessControl");
    for name in [b"Store".as_slice(),b"db",b"Relational"] {
        assert!(send(id,ns,b"at:",&[Operand::object(id.intern_name(name).unwrap())]).as_object().is_none());
    }
    let eval=|source: &[u8]| h.call(h.named(b"Codegen"), b"evalPersisting:", &[text(source)]);
    assert_eq!(eval(br#"[[|
let first := ["same" , " value"]; let other := ["same" , " value"];
let cell := [Cell with: first];
let refused := [cell replace: other with: nil];
let unchanged := [[cell value] == first];
let accepted := [cell replace: first with: nil]; [System gc];
[[[[first = other] & [[first == other] not]] & [refused == false]] &
 [unchanged & [accepted & [[cell value] isNil]]]]
] value]"#),h.named(b"true"));
    // Production loads this authored module from source into the startup image.
    // Run its actual initializer; its resource methods still require Alien/Image
    // when called, but owned preparation fields do not require Store.
    h.call(module,b"markLoaded:",&[id.intern_name(b"stacklessControl").unwrap()]);
    let initialized=eval(include_bytes!("../../../modules/isolate.birnam"));
    assert_eq!(h.call(initialized,b"isError",&[]),h.named(b"false"));
    assert_eq!(eval(br#"[[|
let owner := [Concurrency clone];
let left := [owner field: #workspace]; let right := [owner field: #workspace];
[left set: #owned]; [System gc];
[[[right value] == #owned] & [[owner workspace] == #owned]]
] value]"#),h.named(b"true"));

    let result=eval(br#"[[|
let task := [TaskControl start: [:self | [self wait]; 42]];
let token := [task waitToken];
[System gc];
[task resolve: token with: nil];
[[task pollOutcome] = #[#success 42]]
] value]"#);
    assert_eq!(result,h.named(b"true"));
    let result=eval(br#"[[|
let cleaned := [0 asCell];
let task := [TaskControl start: [:self | [self ensure: [| [self wait]] cleanup: [| [cleaned inc]]]]];
[task cancel: #stop];
[[[task pollOutcome] = #[#cancelled #stop]] & [[cleaned value] = 1]]
] value]"#);
    assert_eq!(result,h.named(b"true"));
    // A retained view is the only root of this owner and its payload.
    let start=h.tokens.borrow().len();
    let task_control=h.call(globals,b"at:",&[id.intern_name(b"TaskControl").unwrap()]);
    let owner=h.call(task_control, b"clone", &[]);
    let payload=h.call(prototype,b"clone",&[]);
    h.call(owner,b"cancelReason:",&[payload]);
    let view=h.call(owner,b"field:",&[id.intern_name(b"cancelReason").unwrap()]);
    let view_root=retain(id,roots,view);
    h.clear_from(start); collect(id,gc);
    assert!(present(id,owner)&&present(id,payload));
    assert_eq!(h.call(view,b"value",&[]),payload);
    h.clear_from(start); release(id,roots,view_root); collect(id,gc);
    assert!(!present(id,owner)&&!present(id,payload)&&!present(id,view));
    h.clear_from(0);
} }
