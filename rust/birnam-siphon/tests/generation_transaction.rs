//! Authored candidate preflight, partial application and explicit conditional undo.
mod support;
use birnam_abi::{
    Oop,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use core::{cell::Cell, ffi::c_void};
use std::os::unix::ffi::OsStrExt;
use support::*;
struct Vm {
    id: Protocol,
    ns: Oop,
}
impl Vm {
    unsafe fn named(&self, n: &[u8]) -> Oop {
        unsafe {
            object(send(
                self.id,
                self.ns,
                b"at:",
                &[Operand::object(self.sym(n))],
            ))
        }
    }
    unsafe fn keep(&self, o: Oop) -> Oop {
        unsafe {
            retain(self.id, self.named(b"Roots"), o);
            o
        }
    }
    unsafe fn sym(&self, n: &[u8]) -> Oop {
        unsafe { self.id.intern_name(n).unwrap() }
    }
    unsafe fn call(&self, r: Oop, n: &[u8], a: &[Oop]) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                r,
                n,
                &a.iter().copied().map(Operand::object).collect::<Vec<_>>(),
            )))
        }
    }
    unsafe fn text(&self, b: &[u8]) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                self.named(b"String"),
                b"fromBytes:",
                &[Operand::bytes(b)],
            )))
        }
    }
    unsafe fn integer(&self, n: usize) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                self.named(b"Integer"),
                b"fromWord:",
                &[Operand::word(n)],
            )))
        }
    }
    unsafe fn number(&self, o: Oop) -> usize {
        unsafe {
            send(
                self.id,
                self.named(b"Integer"),
                b"wordOf:",
                &[Operand::object(o)],
            )
            .as_word()
            .unwrap()
        }
    }
    unsafe fn vector(&self, values: &[Oop]) -> Oop {
        unsafe {
            let mut v = self.call(self.named(b"Vector"), b"new", &[]);
            for &x in values {
                v = self.call(v, b"add:", &[x]);
            }
            v
        }
    }
    unsafe fn prototype(&self, name: &[u8]) -> Oop {
        unsafe { self.call(self.named(b"Globals"), b"at:", &[self.sym(name)]) }
    }
    unsafe fn record(&self, name: &[u8]) -> Oop {
        unsafe { self.call(self.prototype(name), b"clone", &[]) }
    }
    unsafe fn set(&self, e: Oop, a: &[u8], v: Oop) {
        unsafe {
            if a == b"activeGeneration" { self.call(self.prototype(b"DispatchGeneration"), b"current:", &[v]); }
            else if a == b"version" || a == b"survives" { self.call(self.named(b"db"), b"assert:attr:value:", &[e, self.sym(a), v]); }
            else { assert_eq!(self.call(self.named(b"Reflect"), b"slot:in:put:", &[self.sym(a), e, v]), e); }
        }
    }
    unsafe fn plan(&self, action: &[u8], unit: Oop) -> Oop {
        unsafe { self.vector(&[self.sym(action), unit]) }
    }
    unsafe fn direct(&self, selector: &[u8]) -> Operand {
        unsafe {
            send(
                self.id,
                self.id.receiver,
                b"bindingOf:in:",
                &[
                    Operand::object(self.sym(selector)),
                    Operand::object(self.named(b"IntegerVt")),
                ],
            )
        }
    }
}
struct Fault { id:Protocol, ns:Oop, original:NativeEntry, key:Oop, error:Oop, armed:Cell<bool>, calls:Cell<usize>, after:bool, extra:bool }
unsafe extern "C" fn fault(c:*mut c_void,r:Oop,a:*const Operand,n:usize)->Outcome {
    unsafe {
        let f=&*c.cast::<Fault>();
        let args=birnam_abi::borrowed_slice(a,n).unwrap();
        let selected=args.last().and_then(|v|v.as_object())==Some(f.key) || f.key.is_null();
        if !selected || !f.armed.replace(false){return f.original.method.unwrap()(f.original.context,r,a,n);}
        f.calls.set(f.calls.get()+1);
        if f.after {
            let result=f.original.method.unwrap()(f.original.context,r,a,n);
            if result.status!=birnam_abi::id_protocol::LOADED{return result;}
            if f.extra {
                let alias=f.id.intern_name(b"txCallback").unwrap();
                f.original.method.unwrap()(f.original.context,r,[args[0],Operand::object(alias)].as_ptr(),2);
            }
        }
        let v=Vm{id:f.id,ns:f.ns};collect(f.id,v.named(b"GC"));
        Outcome::loaded(Operand::object(f.error))
    }
}
unsafe fn entry(v:&Vm,owner:Oop,name:&[u8])->NativeEntry {
    unsafe{send(v.id,v.id.receiver,b"bindingOf:in:",&[Operand::object(v.sym(name)),Operand::object(owner)]).as_entry().unwrap()}
}
unsafe fn replace(v:&Vm,owner:Oop,name:&[u8],e:NativeEntry) {
    unsafe{send(v.id,owner,b"addMethod",&[Operand::object(v.sym(name)),Operand::entry(e)]);}
}
#[test]
#[ignore = "requires production native modules"]
fn candidate_partial_edits_and_explicit_undo_preserve_current_owners() {
    unsafe {
        let id=id();let ns=load(id,"bootstrap",&[Operand::bytes(directory().as_os_str().as_bytes())]);
        optional_modules(id, ns, &["store"]);lifecycle_records(id,ns);let v=Vm{id,ns};
        let cg=v.named(b"Codegen");let generation=v.named(b"Generation");let globals=v.named(b"Globals");let nil=v.named(b"nil");let yes=v.named(b"true");
        let eval=|text:&[u8]|v.call(cg,b"evalPersisting:",&[v.text(text)]);
        let before=v.text(b"[Module named: #txFixture requires: #[]]; let txHelper := [| 10]; [Reflect defineMethod: #txReplace on: Integer as: [:self | [txHelper value]]]; [Reflect defineMethod: #txRemove on: Integer as: [:self | 30]]");
        eval(b"let txHelper := [| 10]; [Reflect defineMethod: #txReplace on: Integer as: [:self | [txHelper value]]]; [Reflect defineMethod: #txRemove on: Integer as: [:self | 30]]");
        v.call(cg,b"recordModuleSource:named:",&[before,v.sym(b"txFixture")]);
        let helper=v.call(globals,b"at:",&[v.sym(b"txHelper")]);let old_replace=v.direct(b"txReplace").as_entry().unwrap();let old_remove=v.direct(b"txRemove").as_entry().unwrap();
        let after=v.text(b"[Module named: #txFixture requires: #[]]; let txHelper := [| 20]; let txNewHelper := [| 7]; [Reflect defineMethod: #txReplace on: Integer as: [:self | [txHelper value]]]; [Reflect defineMethod: #txAdded on: Integer as: [:self | 40]]; [Reflect removeMethod: #txRemove on: Integer]");
        let dir=std::env::temp_dir().join(format!("birnam-candidate-partial-{}",std::process::id()));std::fs::create_dir_all(&dir).unwrap();
        for synchronous in [false,true] {
            let path=v.text(dir.join(if synchronous{"sync.o"}else{"resumable.o"}).as_os_str().as_bytes());
            let compiled=if synchronous{v.call(generation,b"compileCandidate:named:output:selection:",&[after,v.sym(b"txFixture"),path,v.vector(&[])])}else{v.call(generation,b"compileCandidate:named:output:",&[after,v.sym(b"txFixture"),path])};assert_eq!(compiled,path);
            let unit=v.record(b"CompiledUnit");assert_eq!(v.call(generation,b"inspectCandidate:into:",&[path,unit]),unit);v.call(generation,b"mapCandidate:",&[unit]);
            let prepare=||v.call(generation,b"prepareActivation:plan:",&[unit,v.plan(b"activate",unit)]);
            let token=prepare();v.call(globals,b"at:put:",&[v.sym(b"txNewHelper"),nil]);
            assert_eq!(v.call(v.call(generation,b"commitPrepared:",&[token]),b"kind",&[]),v.sym(b"staleGeneration"));
            assert_eq!(v.direct(b"txReplace").as_entry(),Some(old_replace));v.call(globals,b"remove:",&[v.sym(b"txNewHelper")]);
            let token=prepare();assert_eq!(v.call(generation,b"discardPrepared:",&[token]),unit);assert_eq!(v.call(v.call(generation,b"commitPrepared:",&[token]),b"kind",&[]),v.sym(b"invalidPreparedToken"));
            let setter=entry(&v,id.receiver,b"addMethod:as:");
            let error=v.call(v.named(b"Error"),b"kind:",&[v.sym(b"setterReportedFailure")]);
            let mut f=Fault{id,ns,original:setter,key:v.sym(b"txReplace"),error,armed:Cell::new(true),calls:Cell::new(0),after:true,extra:true};
            replace(&v,id.receiver,b"addMethod:as:",NativeEntry::new(fault,core::ptr::from_mut(&mut f).cast()));
            let token=prepare();v.set(unit,b"state",v.sym(b"externalDescription"));
            v.call(v.named(b"db"),b"assert:attr:value:",&[unit,v.sym(b"state"),v.sym(b"explicitUserFact")]);
            let failed=v.call(generation,b"commitPrepared:",&[token]);replace(&v,id.receiver,b"addMethod:as:",setter);
            assert_eq!(failed,error);assert_eq!(f.calls.get(),1);
            assert_eq!(v.number(v.call(v.integer(2),b"txReplace",&[])),20,"failure ran an automatic inverse");
            assert_eq!(v.direct(b"txRemove").as_entry(),Some(old_remove));assert_eq!(v.direct(b"txAdded").kind(),birnam_abi::id_protocol::UNIT);
            assert_eq!(v.call(unit,b"rollbackAvailable",&[]),yes);
            let installed=v.call(v.named(b"Execution"),b"directMethodFor:in:",&[v.sym(b"txReplace"),v.named(b"IntegerVt")]);
            let outside=v.direct(b"txCallback").as_entry();
            eval(b"[Reflect defineMethod: #txReplace on: Integer as: [:self | 999]]");
            assert_eq!(v.call(v.call(unit,b"rollback",&[]),b"kind",&[]),v.sym(b"undoConflict"));
            assert_eq!(v.number(v.call(v.integer(2),b"txReplace",&[])),999);
            assert_eq!(v.number(v.call(v.call(globals,b"at:",&[v.sym(b"txHelper")]),b"value",&[])),20);
            v.call(v.named(b"IntegerVt"),b"addMethod:as:",&[installed,v.sym(b"txReplace")]);
            assert_eq!(v.call(unit,b"rollback",&[]),unit);
            assert_eq!(v.direct(b"txReplace").as_entry(),Some(old_replace));assert_eq!(v.direct(b"txRemove").as_entry(),Some(old_remove));
            assert_eq!(v.direct(b"txCallback").as_entry(),outside,"undo erased the setter's unrelated effect");
            assert_eq!(v.call(globals,b"at:",&[v.sym(b"txHelper")]),helper);assert_eq!(v.number(v.call(v.call(globals,b"bindingAt:",&[v.sym(b"txNewHelper")]),b"size",&[])),0);
            assert_eq!(v.call(v.named(b"db"),b"currentValue:attr:",&[unit,v.sym(b"state")]),v.sym(b"explicitUserFact"));
            // The retained construction can be activated again; no reservation
            // or global-basis service participates in this explicit policy.
            assert_eq!(send(id,id.receiver,b"bind:to:",&[Operand::object(v.sym(b"reserveBindingCompensationFor:count:")),Operand::object(globals)]).kind(),birnam_abi::id_protocol::UNIT);
            assert_eq!(v.call(unit,b"activate",&[]),unit);
            assert_eq!(v.number(v.call(v.integer(2),b"txAdded",&[])),40);assert_eq!(v.direct(b"txRemove").kind(),birnam_abi::id_protocol::UNIT);
            let removal=entry(&v,id.receiver,b"removeMethod:");
            let mut f=Fault{id,ns,original:removal,key:v.sym(b"txAdded"),error,armed:Cell::new(true),calls:Cell::new(0),after:true,extra:false};
            replace(&v,id.receiver,b"removeMethod:",NativeEntry::new(fault,core::ptr::from_mut(&mut f).cast()));
            assert_eq!(v.call(unit,b"rollback",&[]),error);replace(&v,id.receiver,b"removeMethod:",removal);
            assert_eq!(v.direct(b"txAdded").kind(),birnam_abi::id_protocol::UNIT,"failed undo was automatically redone");
            assert_eq!(v.direct(b"txRemove").as_entry(),Some(old_remove));assert_eq!(v.number(v.call(v.integer(2),b"txReplace",&[])),20);
            assert_eq!(v.call(unit,b"rollback",&[]),unit);assert_eq!(v.direct(b"txReplace").as_entry(),Some(old_replace));assert_eq!(v.call(globals,b"at:",&[v.sym(b"txHelper")]),helper);
            assert_eq!(v.call(unit,b"discard",&[]),unit);assert_eq!(v.call(generation,b"candidatePolicyCellFor:",&[unit]),nil);
            // An initializer failure is never silently replayed.
            let second=v.record(b"CompiledUnit");v.call(generation,b"inspectCandidate:into:",&[path,second]);v.call(generation,b"mapCandidate:",&[second]);
            let info=v.call(generation,b"candidateDataFor:",&[second]);let adapters=v.call(info,b"at:",&[v.integer(7)]);let adapter=v.call(adapters,b"at:",&[v.integer(0)]);
            let avt=object(send(id,id.receiver,b"behaviorOf:",&[Operand::object(adapter)]));let init=entry(&v,avt,b"initializeWith:");
            let mut f=Fault{id,ns,original:init,key:core::ptr::null_mut(),error,armed:Cell::new(true),calls:Cell::new(0),after:false,extra:false};
            replace(&v,avt,b"initializeWith:",NativeEntry::new(fault,core::ptr::from_mut(&mut f).cast()));
            assert_eq!(v.call(second,b"activate",&[]),error);
            assert_eq!(v.call(v.call(second,b"activate",&[]),b"kind",&[]),v.sym(b"candidateConstructionFailed"));assert_eq!(f.calls.get(),1);
            replace(&v,avt,b"initializeWith:",init);v.call(second,b"discard",&[]);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
