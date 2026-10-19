//! Run actual Error/Globals/Cell initializers on a floor with no Store.
//! Ancillary record/registry/harness programs remain unchanged exported bodies.
mod support;
use birnam_abi::{Oop, borrowed_slice, id_protocol::{NativeEntry, Operand, Outcome, Protocol}};
use std::{cell::{Cell, RefCell}, ffi::c_void, path::PathBuf};
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
        unsafe { object(send(self.id,self.ns,b"resolveGlobal:",&[Operand::object(self.id.intern_name(name).unwrap())])) }
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

struct ReadProbe { id: Protocol, gc: Oop, target: Oop, result: Oop, prior: NativeEntry, calls: Cell<usize> }
unsafe extern "C" fn current_read(c:*mut c_void,r:Oop,a:*const Operand,n:usize)->Outcome { unsafe {
    let p=&*c.cast::<ReadProbe>();
    let args=borrowed_slice(a,n).unwrap();
    if args.get(1).and_then(|v|v.as_object())==Some(p.target) {
        p.calls.set(p.calls.get()+1); collect(p.id,p.gc);
        Outcome::loaded(Operand::object(p.result))
    } else { p.prior.method.unwrap()(p.prior.context,r,a,n) }
} }
struct ConstructionProbe { target: Oop, value: Cell<Oop>, clones: Cell<usize>, sets: Cell<usize> }
unsafe extern "C" fn current_clone(c:*mut c_void,_:Oop,_:*const Operand,n:usize)->Outcome { unsafe {
    assert_eq!(n,0); let p=&*c.cast::<ConstructionProbe>(); p.clones.set(p.clones.get()+1);
    Outcome::loaded(Operand::object(p.target))
} }
unsafe extern "C" fn current_set(c:*mut c_void,r:Oop,a:*const Operand,n:usize)->Outcome { unsafe {
    let p=&*c.cast::<ConstructionProbe>(); let args=borrowed_slice(a,n).unwrap(); assert_eq!(args.len(),1);
    p.value.set(object(args[0])); p.sets.set(p.sets.get()+1); Outcome::loaded(Operand::object(r))
} }
unsafe fn current_accessor_dispatch(h:&H,cell:Oop,model:Oop,gc:Oop) { unsafe {
    let nil=h.named(b"nil"); let watched=h.call(cell,b"with:",&[nil]); let result=h.integer(93);
    let vt=h.call(h.id.receiver,b"behaviorOf:",&[model]); let selector=h.id.intern_name(b"slot:in:").unwrap();
    h.call(model,b"methodFor:",&[selector]);
    let prior=send(h.id,h.id.receiver,b"bindingOf:in:",&[Operand::object(selector),Operand::object(vt)]).as_entry().unwrap();
    let probe=ReadProbe{id:h.id,gc,target:watched,result,prior,calls:Cell::new(0)};
    // Warm the ordinary getter, then change the receiver's live method binding.
    for _ in 0..128 {assert_eq!(h.call(watched,b"value",&[]),nil);}
    install(h.id,vt,b"slot:in:",current_read,(&probe as *const ReadProbe).cast_mut().cast());
    assert_eq!(h.call(watched,b"value",&[]),result); assert_eq!(probe.calls.get(),1);
    install(h.id,vt,b"slot:in:",prior.method.unwrap(),prior.context);
    assert_eq!(h.call(watched,b"value",&[]),nil);

    let factory=h.call(cell,b"clone",&[]); let target=h.call(cell,b"clone",&[]);
    let factory_vt=h.call(h.id.receiver,b"ensurePrivateVtableFor:",&[factory]);
    let target_vt=h.call(h.id.receiver,b"ensurePrivateVtableFor:",&[target]);
    let clone_sel=h.id.intern_name(b"clone").unwrap(); let set_sel=h.id.intern_name(b"set:").unwrap();
    h.call(factory,b"methodFor:",&[clone_sel]); h.call(target,b"methodFor:",&[set_sel]);
    let probe=ConstructionProbe{target,value:Cell::new(nil),clones:Cell::new(0),sets:Cell::new(0)};
    let context=(&probe as *const ConstructionProbe).cast_mut().cast();
    install(h.id,factory_vt,b"clone",current_clone,context); install(h.id,target_vt,b"set:",current_set,context);
    assert_eq!(h.call(factory,b"with:",&[result]),target);
    assert_eq!(probe.value.get(),result); assert_eq!(probe.clones.get(),1); assert_eq!(probe.sets.get(),1);
    send(h.id,factory_vt,b"removeMethod:",&[Operand::object(clone_sel)]);
    send(h.id,target_vt,b"removeMethod:",&[Operand::object(set_sel)]);
} }

#[test]
#[ignore = "scripts/cell-without-store-test.py selects actual initializers and ancillary programs without Store"]
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
    let artifact=|name: &str| {
        let path=directory().join(format!("libbirnam_{name}.dylib"));
        let path=h.keep(object(send(id,h.named(b"String"),b"fromBytes:",&[Operand::bytes(path.as_os_str().as_bytes())])));
        let result=h.call(h.named(b"Codegen"),b"load:",&[path]);
        assert_ne!(result,h.named(b"nil"),"nil initializer: {name}");
        assert_eq!(h.call(result,b"isError",&[]),h.named(b"false"),"failed initializer: {name}");
        result
    };
    artifact("error"); let globals=artifact("globalsOwner"); let cell=artifact("cell");
    assert_eq!(h.call(globals,b"at:",&[id.intern_name(b"Cell").unwrap()]),cell);
    assert_eq!(artifact("cell"),cell,"repeat initialization preserves the Cell prototype");
    let model=h.named(b"ObjectModel"); let nil=h.named(b"nil");
    let values=h.call(h.named(b"Vector"),b"new",&[]);
    let values=h.call(values,b"add:",&[nil]);
    let captures=h.call(h.named(b"Vector"),b"new",&[]);
    let with_slots=std::env::var_os("BIRNAM_CELL_SLOTS").is_some();
    let with_registry=std::env::var_os("BIRNAM_MODULE_REGISTRY").is_some();
    let manager=if with_slots {
        let fields=h.call(values,b"add:",&[h.call(h.named(b"Map"),b"new",&[])]);
        let manager=h.call(model,b"newSlots:parent:",&[fields,vt]); h.publish(b"Module",manager);
        h.publish(b"SymbolExemplarVt",h.named(b"SymbolVt")); manager
    } else { nil };
    let mut requests=Vec::new();
    if with_slots {
        let reflect=h.named(b"Reflect");
        requests.extend([(prototype,b"slotNames".as_slice(),0),(prototype,b"isError",0),
            (reflect,b"isSymbol:",1),(reflect,b"slotsOf:",1),(reflect,b"slotIndex:in:",2),
            (reflect,b"slot:in:",2),(reflect,b"slot:in:put:",3),
            (manager,b"slotNames",0),(manager,b"libDir",0),(manager,b"libDir:",1)]);
    }
    if with_registry {
        requests.extend([(manager,b"registry".as_slice(),0),(manager,b"registry:",1),(manager,b"recordNames",0),
            (manager,b"recordOf:",1),(manager,b"registryMutation:",1),(manager,b"record:put:",2),
            (manager,b"field:of:",2),(manager,b"field:of:put:",3),(manager,b"removeRecord:",1),
            (manager,b"named:requires:",2),(manager,b"kernel:",1),(manager,b"requiresOf:",1),
            (manager,b"kindOf:",1),(manager,b"isRegistered:",1),(manager,b"isLoaded:",1),
            (manager,b"markLoaded:",1),(manager,b"loadOrder:",1),(manager,b"visit:order:path:",3),(manager,b"publishRegistry:from:",2)]);
    }
    let mut records = Vec::new();
    if std::env::var_os("BIRNAM_ORDINARY_RECORDS").is_some() {
        for (name, fields, methods) in [
            (b"Logger".as_slice(), vec![b"name".as_slice(),b"level",b"sink"], vec![(b"slotNames".as_slice(),0),(b"name",0),(b"thresholdRank",0),(b"sinkPath",0),(b"named:",1),(b"toFile:",1),(b"toStdout",0)]),
            (b"Config", vec![b"configPath".as_slice()], vec![(b"slotNames".as_slice(),0),(b"path",0)]),
            (b"HttpRequest", vec![b"method".as_slice(),b"path",b"raw"], vec![(b"slotNames".as_slice(),0),(b"method",0),(b"path",0),(b"raw",0)]),
            (b"ProcResult", vec![b"stdout".as_slice(),b"exit"], vec![(b"slotNames".as_slice(),0),(b"stdout",0),(b"exitCode",0)]),
            (b"AppConfig", vec![b"values".as_slice(),b"origins",b"configPath"], vec![(b"slotNames".as_slice(),0),(b"values",0),(b"origins",0),(b"path",0),(b"at:",1),(b"originAt:",1)]),
        ] {
            let mut initial=h.call(h.named(b"Vector"),b"new",&[]);
            for _ in &fields { initial=h.call(initial,b"add:",&[nil]); }
            let record=h.call(model,b"newSlots:parent:",&[initial,vt]);h.publish(name,record);
            let getters = methods.iter().skip(1).take(fields.len()).map(|(selector,_)| *selector).collect::<Vec<_>>();
            requests.extend(methods.into_iter().map(|(selector,arity)|(record,selector,arity)));
            records.push((record,fields,getters));
        }
    }
    // The manifest names actual exported programs; no method body is rewritten.
    let harness_manifest=if std::env::var_os("BIRNAM_HARNESS_OWNERS").is_some() {
        Some(serde_json::from_slice::<Vec<(String,Vec<String>,Vec<String>)>>(
            &std::fs::read(programs.join("harness.json")).unwrap()).unwrap())
    } else { None };
    if let Some(manifest)=&harness_manifest {
        for (name,fields,methods) in manifest {
            let mut initial=h.call(h.named(b"Vector"),b"new",&[]);
            for field in fields {
                let value=if name=="Test" { h.call(h.named(b"Vector"),b"new",&[]) }
                    else if name=="ConsoleRunner" || (name=="Bench" && field=="failures") {h.integer(0)}
                    else {nil};
                initial=h.call(initial,b"add:",&[value]);
            }
            let owner=h.call(model,b"newSlots:parent:",&[initial,vt]);h.publish(name.as_bytes(),owner);
            requests.extend(methods.iter().map(|method|(owner,method.as_bytes(),method.matches(':').count())));
            records.push((owner,fields.iter().map(|f|f.as_bytes()).collect(),
                methods.iter().skip(1).take(fields.len()).map(|m|m.as_bytes()).collect()));
        }
        requests.extend([b"anySatisfy:".as_slice(),b"count:",b"select:",b"reject:"].into_iter().map(|method|(collection,method,1)));
    }
    for (index,(target,selector,arity)) in requests.into_iter().enumerate() {
        let behavior=if target==collection { collection } else { object(send(id,id.receiver,b"behaviorOf:",&[Operand::object(target)])) };
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
    for (record,fields,getters) in &records {
        let mark=h.tokens.borrow().len();
        let reflect=h.named(b"Reflect");let value=h.call(prototype,b"clone",&[]);
        let instance=h.call(*record,b"clone",&[]);
        let rows=h.call(reflect,b"slotsOf:",&[instance]);
        assert_eq!(h.number(h.call(rows,b"size",&[])),fields.len());
        for (index,(&field,&getter)) in fields.iter().zip(getters).enumerate() {
            let field=h.keep(id.intern_name(field).unwrap());
            let field_value=if index==0 { value } else { h.call(prototype,b"clone",&[]) };
            h.call(reflect,b"slot:in:put:",&[field,instance,field_value]);
            assert_eq!(h.call(reflect,b"slot:in:",&[field,instance]),field_value);
            assert_eq!(h.call(instance,getter,&[]),field_value);
        }
        let clone=h.call(instance,b"clone",&[]);
        let field=h.keep(id.intern_name(fields[0]).unwrap());
        h.call(reflect,b"slot:in:put:",&[field,clone,nil]);collect(id,gc);
        assert_eq!(h.call(reflect,b"slot:in:",&[field,instance]),value);
        assert_eq!(h.call(reflect,b"slot:in:",&[field,clone]),nil);
        h.clear_from(mark);collect(id,gc);
        assert!(!present(id,instance));assert!(!present(id,clone));assert!(!present(id,value));
    }
    if std::env::var_os("BIRNAM_ORDINARY_RECORDS").is_some() {
        let logger=h.call(h.named(b"Logger"),b"named:",&[h.integer(42)]);
        assert_eq!(h.number(h.call(logger,b"name",&[])),42);
        assert_eq!(h.number(h.call(logger,b"thresholdRank",&[])),1);
        h.call(logger,b"toFile:",&[logger]);collect(id,gc);
        assert_eq!(h.call(logger,b"sinkPath",&[]),logger);
        h.call(logger,b"toStdout",&[]);assert_eq!(h.call(logger,b"sinkPath",&[]),nil);
        let app=h.named(b"AppConfig");let key=h.keep(id.intern_name(b"setting").unwrap());
        let map=h.call(h.named(b"Map"),b"new",&[]);let map=h.call(map,b"at:put:",&[key,logger]);
        h.call(h.named(b"Reflect"),b"slot:in:put:",&[id.intern_name(b"values").unwrap(),app,map]);
        assert_eq!(h.call(app,b"at:",&[key]),logger);
        h.call(h.named(b"Reflect"),b"slot:in:put:",&[id.intern_name(b"origins").unwrap(),app,map]);
        assert_eq!(h.call(app,b"originAt:",&[key]),logger);
    }
    if harness_manifest.is_some() {
        let start=h.tokens.borrow().len();let registry=h.named(b"Test");let suite_proto=h.named(b"TestSuite");
        let module=h.keep(id.intern_name(b"harnessModule").unwrap());let other=h.keep(id.intern_name(b"otherModule").unwrap());
        let a=h.call(suite_proto,b"named:module:",&[module,module]);
        let b=h.call(suite_proto,b"named:module:",&[other,module]);
        h.call(registry,b"register:",&[a]);
        let snapshot=h.call(registry,b"suites",&[]);assert_eq!(h.number(h.call(snapshot,b"size",&[])),2);
        let selected=h.call(registry,b"suitesForModule:",&[module]);
        assert_eq!(h.call(selected,b"at:",&[h.integer(0)]),a);assert_eq!(h.call(selected,b"at:",&[h.integer(1)]),b);
        h.call(a,b"test:do:",&[module,nil]);let cases=h.call(a,b"cases",&[]);
        let case=h.call(cases,b"at:",&[h.integer(0)]);assert_eq!(h.call(case,b"name",&[]),module);assert_eq!(h.call(case,b"body",&[]),nil);
        h.call(registry,b"unregister:",&[a]);h.call(registry,b"unregister:",&[a]);
        assert_eq!(h.number(h.call(snapshot,b"size",&[])),2);
        let remaining=h.call(registry,b"suites",&[]);assert_eq!(h.call(remaining,b"at:",&[h.integer(0)]),b);
        h.call(h.named(b"Reflect"),b"slot:in:put:",&[id.intern_name(b"testsModule").unwrap(),b,other]);
        assert_eq!(h.number(h.call(h.call(registry,b"suitesForModule:",&[module]),b"size",&[])),0);
        h.call(registry,b"unregister:",&[b]);
        let suite=h.call(h.named(b"BenchSuite"),b"named:",&[module]);
        h.call(suite,b"fixedCase:iters:do:",&[module,h.integer(7),nil]);
        let bc=h.call(h.call(suite,b"cases",&[]),b"at:",&[h.integer(0)]);
        assert_eq!(h.number(h.call(bc,b"fixedN",&[])),7);assert_eq!(h.number(h.call(bc,b"k",&[])),1);
        let context=h.call(h.named(b"TestContext"),b"for:",&[suite]);assert_eq!(h.call(context,b"runner",&[]),suite);
        h.clear_from(start);collect(id,gc);
        for value in [a,b,case,suite,bc,context] {assert!(!present(id,value),"unregistered harness objects must collect");}
    }
    if with_slots {
        let reflect=h.named(b"Reflect");
        let key=h.keep(id.intern_name(b"libDir").unwrap());
        let rows=h.call(reflect,b"slotsOf:",&[manager]);
        assert_eq!(h.number(h.call(rows,b"size",&[])),2);
        let row=h.call(rows,b"at:",&[h.integer(0)]);
        assert_eq!(h.call(row,b"at:",&[id.intern_name(b"name").unwrap()]),key);
        assert_eq!(h.call(reflect,b"slot:in:",&[key,manager]),nil);
        let path=h.keep(object(send(id,h.named(b"String"),b"fromBytes:",&[Operand::bytes(b"/owned")])));
        assert_eq!(h.call(reflect,b"slot:in:put:",&[key,manager,path]),manager);
        collect(id,gc);
        assert_eq!(h.call(manager,b"libDir",&[]),path);
        h.call(manager,b"libDir:",&[nil]);
        assert_eq!(h.call(reflect,b"slot:in:",&[h.integer(0),manager]),nil);
        let default=h.call(manager,b"libDir",&[]);
        let bytes=send(id,default,b"bytes",&[]).as_bytes().unwrap();
        assert_eq!(birnam_abi::borrowed_bytes(bytes.pointer,bytes.length).unwrap(),b"lib");
        // Foreign native storage is explicitly unsupported, never decoded.
        assert_eq!(h.call(reflect,b"slotsOf:",&[path]),nil);
    }
    if with_registry {
        let a=h.keep(id.intern_name(b"a").unwrap()); let b=h.keep(id.intern_name(b"b").unwrap());
        let empty=h.call(h.named(b"Vector"),b"new",&[]);
        let deps=h.call(empty,b"add:",&[b]);
        h.call(manager,b"markLoaded:",&[a]);
        h.call(manager,b"named:requires:",&[a,deps]); h.call(manager,b"kernel:",&[b]);
        assert_eq!(h.call(manager,b"isLoaded:",&[a]),h.named(b"true"));
        assert_eq!(h.call(manager,b"requiresOf:",&[a]),deps);
        let names=h.call(empty,b"add:",&[a]);
        let order=h.call(manager,b"loadOrder:",&[names]);
        assert_eq!(h.call(order,b"at:",&[h.integer(0)]),b);
        assert_eq!(h.call(order,b"at:",&[h.integer(1)]),a);
        let snapshot=h.call(manager,b"registry",&[]);
        let clone=h.call(manager,b"clone",&[]);
        h.call(manager,b"removeRecord:",&[a]); collect(id,gc);
        assert_eq!(h.call(manager,b"recordOf:",&[a]),nil);
        assert_ne!(h.call(snapshot,b"at:",&[a]),nil);
        assert_eq!(h.call(clone,b"isRegistered:",&[a]),h.named(b"true"));
        // Registry ownership is ordinary reachability, not a database root.
        let start=h.tokens.borrow().len();
        let private=h.call(manager,b"clone",&[]);
        let key=h.keep(id.intern_name(b"privateRecord").unwrap());
        h.call(private,b"named:requires:",&[key,empty]);
        let record=h.call(private,b"recordOf:",&[key]);
        h.clear_from(start); collect(id,gc);
        assert!(!present(id,private) && !present(id,record));
    }
    current_accessor_dispatch(&h,cell,model,gc);
    let floor=h.tokens.borrow().len();
    let value=h.call(prototype,b"clone",&[]);
    let local=h.call(cell,b"with:",&[value]);
    let local_token=retain(id,roots,local);
    h.clear_from(floor); collect(id,gc);
    assert!(present(id,local) && present(id,value));
    assert_eq!(h.call(local,b"value",&[]),value);
    assert_eq!(h.call(local,b"replace:with:",&[nil,nil]),h.named(b"false"));
    assert_eq!(h.call(local,b"value",&[]),value);
    assert_eq!(h.call(local,b"replace:with:",&[value,nil]),h.named(b"true"));
    assert_eq!(h.call(local,b"value",&[]),nil);
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
