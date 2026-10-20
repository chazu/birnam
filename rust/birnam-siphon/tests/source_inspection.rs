//! Static source observations must not publish compiler metadata or retain inputs.
mod support;
use birnam_abi::{Oop, borrowed_bytes, id_protocol::{self as abi, Operand, Outcome, NativeEntry, Protocol}};
use std::{cell::{Cell,RefCell}, ffi::c_void, os::unix::ffi::OsStrExt};
use support::*;
struct H { id:Protocol, ns:Oop, roots:Oop, tokens:RefCell<Vec<usize>> }
impl H {
    unsafe fn named(&self,n:&[u8])->Oop {unsafe {object(send(self.id,self.ns,b"at:",&[Operand::object(self.id.intern_name(n).unwrap())]))}}
    unsafe fn keep(&self,o:Oop)->Oop {self.tokens.borrow_mut().push(unsafe{retain(self.id,self.roots,o)});o}
    unsafe fn call(&self,r:Oop,n:&[u8],a:&[Oop])->Oop {unsafe{self.keep(object(send(self.id,r,n,&a.iter().copied().map(Operand::object).collect::<Vec<_>>())))} }
    unsafe fn text(&self,b:&[u8])->Oop {unsafe{self.keep(object(send(self.id,self.named(b"String"),b"fromBytes:",&[Operand::bytes(b)])))}}
    unsafe fn num(&self,v:usize)->Oop {unsafe{self.keep(object(send(self.id,self.named(b"Integer"),b"fromWord:",&[Operand::word(v)])))}}
    unsafe fn word(&self,o:Oop)->usize {unsafe{send(self.id,self.named(b"Integer"),b"wordOf:",&[Operand::object(o)]).as_word().unwrap()}}
    unsafe fn at(&self,o:Oop,i:usize)->Oop {unsafe{self.call(o,b"at:",&[self.num(i)])}}
    unsafe fn bytes(&self,o:Oop)->Vec<u8> {unsafe{let v=send(self.id,o,b"bytes",&[]).as_bytes().unwrap();borrowed_bytes(v.pointer,v.length).unwrap().to_vec()}}
    unsafe fn clear(&self,n:usize) {for token in self.tokens.borrow_mut().drain(n..).rev(){unsafe{release(self.id,self.roots,token)}}}
    unsafe fn entry(&self,vt:Oop,selector:&[u8])->NativeEntry {unsafe{
        let entry=send(self.id,self.id.receiver,b"bindingOf:in:",&[Operand::object(self.id.intern_name(selector).unwrap()),Operand::object(vt)]).as_entry().unwrap();
        if let Some(method)=send(self.id,self.named(b"Execution"),b"methodOfEntry:",&[Operand::entry(entry)]).as_object(){self.keep(method);}
        entry
    }}
}
struct Hook {id:Protocol,original:NativeEntry,gc:Oop,calls:Cell<usize>,deny:Cell<bool>}
unsafe extern "C" fn observed(c:*mut c_void,r:Oop,a:*const Operand,n:usize)->Outcome {unsafe{
    let h=&*c.cast::<Hook>();h.calls.set(h.calls.get()+1);
    if h.deny.get(){return Outcome::failed(abi::INVALID_REQUEST,99301)}
    collect(h.id,h.gc);
    h.original.method.unwrap()(h.original.context,r,a,n)
}}
const MODULE:&[u8]=b"[Module named: #inspectionProbe requires: #[#dependency]]";
const ASSETS:&[u8]=b"[System embedFile: \"a.bin\"]; [| [System embedFile: \"b.bin\"]]; [System embedFile: \"a.bin\"]";
#[test]
#[ignore="requires production modules"]
fn temporary_source_inspection_does_not_publish_or_retain() {unsafe{
    let id=id();let ns=load(id,"bootstrap",&[Operand::bytes(directory().as_os_str().as_bytes())]);
        optional_modules(id, ns, &["store"]);
    let roots=object(send(id,ns,b"at:",&[Operand::object(id.intern_name(b"Roots").unwrap())]));
    let h=H{id,ns,roots,tokens:RefCell::new(vec![])};
    let cg=h.named(b"Codegen");let store=h.named(b"Store");let gc=h.named(b"GC");let nil=h.named(b"nil");
    send(id,gc,b"precisePayloads:",&[Operand::word(1)]);
    let store_vt=h.named(b"StoreVt");let lang_vt=h.named(b"LangVt");
    let mut writes=Box::new(Hook{id,original:h.entry(store_vt,b"assert:attr:value:"),gc,calls:Cell::new(0),deny:Cell::new(false)});
    let mut parses=Box::new(Hook{id,original:h.entry(lang_vt,b"parse:"),gc,calls:Cell::new(0),deny:Cell::new(false)});
    install(id,store_vt,b"assert:attr:value:",observed,(&mut *writes as *mut Hook).cast());
    install(id,lang_vt,b"parse:",observed,(&mut *parses as *mut Hook).cast());
    let mark=h.tokens.borrow().len();
    for calls in [2,0,1,10,30,30] {
        let facts_before=h.word(h.call(store,b"retainedFactCount",&[]));h.clear(mark);collect(id,gc);
        let before=send(id,id.receiver,b"allocationCount",&[]).as_word().unwrap();
        let parses_before=parses.calls.get();
        for _ in 0..calls {
            let source=h.text(MODULE);let same=h.text(MODULE);assert_ne!(source,same);
            for source in [source,same] {
                let m=h.call(cg,b"moduleMetadataOf:",&[source]);assert_eq!(h.word(h.call(m,b"size",&[])),5);
                assert_eq!(h.at(m,0),id.intern_name(b"moduleMetadata").unwrap());
                assert_eq!(h.at(m,1),id.intern_name(b"inspectionProbe").unwrap());
                assert_eq!(h.at(h.at(m,2),0),id.intern_name(b"dependency").unwrap());
                assert_eq!(h.bytes(h.at(m,3)).len(),64);
            }
            let paths=h.call(cg,b"embeddedFilesOf:",&[h.text(ASSETS)]);
            assert_eq!(h.word(h.call(paths,b"size",&[])),2);
            assert_eq!(h.bytes(h.at(paths,0)),b"a.bin");assert_eq!(h.bytes(h.at(paths,1)),b"b.bin");
            h.clear(mark);
        }
        let facts_after=h.word(h.call(store,b"retainedFactCount",&[]));h.clear(mark);collect(id,gc);
        let after=send(id,id.receiver,b"allocationCount",&[]).as_word().unwrap();
        eprintln!("source inspection {calls}: objects={before}->{after} facts={facts_before}->{facts_after} writes={}",writes.calls.get());
        assert_eq!(facts_after,facts_before,"inspection published facts");assert_eq!(writes.calls.get(),0);
        assert_eq!(parses.calls.get()-parses_before,calls*3,"inspection must use current parser");
        if calls!=2 {assert_eq!(after,before,"temporary inspection retained language objects");}
    }
    // Nil, valid non-module and malformed source have no publication side effect.
    assert_eq!(h.call(cg,b"moduleMetadataOf:",&[nil]),nil);
    assert_eq!(h.call(cg,b"moduleMetadataOf:",&[h.text(b"42")]),nil);
    h.call(cg,b"clearCompilationError",&[]);
    assert_eq!(h.call(cg,b"moduleMetadataOf:",&[h.text(b"[broken")]),nil);
    assert_eq!(h.bytes(h.call(cg,b"lastError",&[])),b"compiled source metadata is missing; rebuild the artifact or image");
    assert_eq!(h.word(h.call(h.call(cg,b"embeddedFilesOf:",&[nil]),b"size",&[])),0);
    assert_eq!(writes.calls.get(),0);
    // Preparing returns an ordinary owned value; retaining just the String is
    // not a hidden metadata root. Neither preparation nor registration needs db.
    let store_root=h.keep(store);let db=h.keep(h.named(b"db"));
    send(id,ns,b"at:put:",&[Operand::object(id.intern_name(b"Store").unwrap()),Operand::object(nil)]);
    send(id,ns,b"at:put:",&[Operand::object(id.intern_name(b"db").unwrap()),Operand::object(nil)]);
    let mark=h.tokens.borrow().len();
    for count in [2,0,1,30,30] {
        collect(id,gc);let before=send(id,id.receiver,b"allocationCount",&[]).as_word().unwrap();
        for _ in 0..count {
            let source=h.text(MODULE);let record=h.call(cg,b"analyzeSourceMetadata:",&[source]);
            assert_eq!(h.call(cg,b"sourceTextOf:",&[record]),source);
            assert_ne!(h.call(cg,b"compiledSourceMetadataFor:",&[record]),nil);
            assert_eq!(h.call(cg,b"compiledSourceMetadataFor:",&[source]),nil);
            h.clear(mark);
        }
        collect(id,gc);let after=send(id,id.receiver,b"allocationCount",&[]).as_word().unwrap();
        eprintln!("owned compilations {count}: objects={before}->{after}");
        if count!=2 {assert_eq!(after,before,"unregistered compilation retained objects")}
    }
    let source=h.text(b"[Module named: #retainedProbe requires: #[]]");
    let record=h.call(cg,b"analyzeSourceMetadata:",&[source]);
    assert_eq!(h.at(record,0),id.intern_name(b"compiledSourceV1").unwrap());
    assert_eq!(h.at(record,1),source);
    assert_eq!(writes.calls.get(),0,"preparation must not publish facts");
    let cg_vt=h.named(b"CodegenVt");
    let mut getter=Box::new(Hook{id,original:h.entry(cg_vt,b"retainedSourceMetadataOf:"),gc,calls:Cell::new(0),deny:Cell::new(false)});
    install(id,cg_vt,b"retainedSourceMetadataOf:",observed,(&mut *getter as *mut Hook).cast());
    assert_ne!(h.call(cg,b"compiledSourceMetadataFor:",&[record]),nil);
    assert_eq!(getter.calls.get(),1,"compiler bypassed the current metadata accessor");
    install(id,cg_vt,b"retainedSourceMetadataOf:",getter.original.method.unwrap(),getter.original.context);
    let parsed=parses.calls.get();parses.deny.set(true);
    assert_ne!(h.call(cg,b"moduleMetadataOf:",&[record]),nil);
    assert_eq!(h.word(h.call(h.call(cg,b"embeddedFilesOf:",&[record]),b"size",&[])),0);
    assert_eq!(h.call(cg,b"analyzeSourceMetadata:",&[record]),record);
    let name=h.keep(id.intern_name(b"retainedProbe").unwrap());
    assert_eq!(h.call(cg,b"recordModuleSource:named:",&[record,name]),source);
    assert_eq!(h.call(cg,b"moduleSourceFor:",&[name]),source);
    assert_eq!(h.call(cg,b"compiledSourceFor:",&[source]),record);
    assert_eq!(h.call(cg,b"sourceTextOf:",&[h.at(h.call(cg,b"sourceRevisionOf:",&[record]),7)]),source);
    assert_ne!(h.call(cg,b"compiledSourceMetadataFor:",&[source]),nil);
    assert_eq!(parses.calls.get(),parsed,"retained records must not reparse");
    let fresh=h.text(b"[Module named: #retainedProbe requires: #[]]");
    assert_eq!(h.call(cg,b"compiledSourceMetadataFor:",&[fresh]),nil,"equal text must not merge source identity");
    let missing=h.call(cg,b"compiledSource:metadata:",&[source,nil]);
    assert_eq!(h.call(cg,b"compiledSourceMetadataFor:",&[missing]),nil);
    let malformed=h.call(h.named(b"Vector"),b"new",&[]);
    let bad=h.call(cg,b"compiledSource:metadata:",&[source,malformed]);
    assert!(id.send_named(cg,b"compiledSourceMetadataFor:",&[Operand::object(bad)]).is_err());
    assert_eq!(parses.calls.get(),parsed,"missing/malformed retained metadata must not parse");
    // Registry replacement selects a new immutable value; earlier revisions keep
    // their original record and source identity. Use a copied descriptor.
    let snapshot=h.call(cg,b"sourceRevisionOf:",&[record]);
    let mut replacement=h.call(h.named(b"Vector"),b"new",&[]);
    for i in 0..3 {replacement=h.call(replacement,b"add:",&[h.at(record,i)]);}
    assert_ne!(replacement,record);
    assert_eq!(h.call(cg,b"replaceCompiledSource:named:",&[replacement,name]),source);
    assert_eq!(h.call(cg,b"compiledSourceFor:",&[source]),replacement);
    assert_eq!(h.at(snapshot,7),record);
    assert_eq!(writes.calls.get(),0);
    send(id,ns,b"at:put:",&[Operand::object(id.intern_name(b"Store").unwrap()),Operand::object(store_root)]);
    send(id,ns,b"at:put:",&[Operand::object(id.intern_name(b"db").unwrap()),Operand::object(db)]);
    // The former fact key is now independent user data, never consulted/removed.
    let key=h.keep(id.intern_name(b"compiledSourceMetadata").unwrap());
    h.call(store,b"assert:attr:value:",&[source,key,malformed]);
    assert_ne!(h.call(cg,b"compiledSourceMetadataFor:",&[source]),nil);
    assert_eq!(h.call(store,b"currentValue:attr:",&[source,key]),malformed);
    assert_eq!(writes.calls.get(),1);
    let before=parses.calls.get();
    assert!(id.send_named(cg,b"analyzeSourceMetadata:",&[Operand::object(source)]).is_err(),"explicit text analysis must use the current parser even for registered text");
    assert_eq!(parses.calls.get(),before+1);
    install(id,store_vt,b"assert:attr:value:",writes.original.method.unwrap(),writes.original.context);
    install(id,lang_vt,b"parse:",parses.original.method.unwrap(),parses.original.context);
    h.clear(0);
}}
