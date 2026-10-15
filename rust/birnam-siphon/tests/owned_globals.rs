//! Owned binding roots, conditional slot publication and transactional images.
mod support;
use birnam_abi::{Oop, borrowed_slice, id_protocol::{NativeEntry, Operand, Outcome, Protocol}};
use std::{cell::{Cell, RefCell}, ffi::c_void, os::unix::ffi::OsStrExt};
use support::*;
struct H { id: Protocol, ns: Oop, roots: Oop, tokens: RefCell<Vec<usize>> }
impl H {
    unsafe fn named(&self, name: &[u8]) -> Oop { unsafe { object(send(self.id,self.ns,b"at:",&[Operand::object(self.key(name))])) } }
    unsafe fn key(&self, name: &[u8]) -> Oop { unsafe { self.id.intern_name(name).unwrap() } }
    unsafe fn keep(&self, value: Oop) -> Oop { self.tokens.borrow_mut().push(unsafe { retain(self.id,self.roots,value) }); value }
    unsafe fn call(&self, r: Oop, name: &[u8], args: &[Oop]) -> Oop { unsafe { self.keep(object(send(self.id,r,name,&args.iter().copied().map(Operand::object).collect::<Vec<_>>()))) } }
    unsafe fn int(&self, n: usize) -> Oop { unsafe { self.keep(object(send(self.id,self.named(b"Integer"),b"fromWord:",&[Operand::word(n)]))) } }
    unsafe fn number(&self, n: Oop) -> usize { unsafe { send(self.id,self.named(b"Integer"),b"wordOf:",&[Operand::object(n)]).as_word().unwrap() } }
    unsafe fn text(&self, bytes: &[u8]) -> Oop { unsafe { self.keep(object(send(self.id,self.named(b"String"),b"fromBytes:",&[Operand::bytes(bytes)]))) } }
    unsafe fn eval(&self, source: &[u8]) -> Oop { unsafe { let result=self.call(self.named(b"Codegen"),b"eval:",&[self.text(source)]); assert_eq!(self.call(result,b"isError",&[]),self.named(b"false")); result } }
    unsafe fn vector(&self, values: &[Oop]) -> Oop { unsafe { let mut result=self.call(self.named(b"Vector"),b"new",&[]); for &v in values {result=self.call(result,b"add:",&[v]);} result } }
    unsafe fn binding(&self, name: &[u8]) -> Oop { unsafe { self.call(self.named(b"Globals"),b"at:",&[self.key(name)]) } }
    unsafe fn put(&self, name: &[u8], value: Oop) { unsafe { assert_eq!(self.call(self.named(b"Globals"),b"at:put:",&[self.key(name),value]),value); } }
    unsafe fn entry(&self, receiver: Oop, selector: &[u8]) -> NativeEntry { unsafe { send(self.id,self.id.receiver,b"bind:to:",&[Operand::object(self.key(selector)),Operand::object(receiver)]).as_entry().unwrap() } }
    unsafe fn restore(&self, receiver: Oop, selector: &[u8], entry: NativeEntry) { unsafe { let vt=object(send(self.id,self.id.receiver,b"behaviorOf:",&[Operand::object(receiver)])); send(self.id,vt,b"addMethod",&[Operand::object(self.key(selector)),Operand::entry(entry)]); } }
}
struct CasProbe { id: Protocol, gc: Oop, model: Oop, target: Oop, foreign: Oop, original: NativeEntry, armed: Cell<bool> }
unsafe extern "C" fn reenter_cas(c:*mut c_void,r:Oop,a:*const Operand,n:usize)->Outcome { unsafe {
    let p=&*c.cast::<CasProbe>();
    if p.armed.replace(false) {
        send(p.id,p.model,b"slotAt:in:put:",&[Operand::word(0),Operand::object(p.target),Operand::object(p.foreign)]);
        collect(p.id,p.gc);
    }
    p.original.method.unwrap()(p.original.context,r,a,n)
} }
struct ImageProbe { id:Protocol, globals:Oop, key:Oop, value:Oop, selector:Oop, original:NativeEntry, fired:Cell<bool> }
unsafe extern "C" fn fail_removal(c:*mut c_void,r:Oop,a:*const Operand,n:usize)->Outcome { unsafe {
    let p=&*c.cast::<ImageProbe>(); let args=borrowed_slice(a,n).unwrap();
    let result=p.original.method.unwrap()(p.original.context,r,a,n);
    if args[0].as_object()==Some(p.selector) && !p.fired.replace(true) {
        send(p.id,p.globals,b"at:put:",&[Operand::object(p.key),Operand::object(p.value)]);
        return Outcome::failed(882,73);
    }
    result
} }
#[test]
#[ignore="production owned Globals prefix required"]
fn conditional_slots_and_image_failure_preserve_binding_owners() { unsafe {
    let id=id(); let ns=load(id,"bootstrap",&[Operand::bytes(directory().as_os_str().as_bytes())]);
    let roots=object(send(id,ns,b"at:",&[Operand::object(id.intern_name(b"Roots").unwrap())]));
    let h=H{id,ns,roots,tokens:RefCell::new(Vec::new())};
    let model=h.named(b"ObjectModel"); let gc=h.named(b"GC"); let globals=h.named(b"Globals");
    let old=h.call(h.named(b"Object"),b"clone",&[]); let replacement=h.call(old,b"clone",&[]); let foreign=h.call(old,b"clone",&[]);
    let target=h.call(model,b"newSlots:parent:",&[h.vector(&[old]),h.named(b"ObjectVt")]);
    assert_eq!(h.call(model,b"replaceSlot:in:expected:with:",&[h.int(0),target,old,replacement]),h.named(b"true"));
    assert_eq!(h.call(model,b"replaceSlot:in:expected:with:",&[h.int(0),target,old,foreign]),h.named(b"false"));
    let index=h.int(0);
    let original=h.entry(gc,b"prepareAllocation");
    let mut p=CasProbe{id,gc,model,target,foreign,original,armed:Cell::new(true)};
    let vt=h.call(id.receiver,b"behaviorOf:",&[gc]);
    install(id,vt,b"prepareAllocation",reenter_cas,core::ptr::from_mut(&mut p).cast());
    assert_eq!(h.call(model,b"replaceSlot:in:expected:with:",&[index,target,replacement,old]),h.named(b"false"));
    h.restore(gc,b"prepareAllocation",original);
    assert_eq!(h.call(model,b"slot:in:",&[h.int(0),target]),foreign);
    // These interfaces touch only owned ordinary payloads, never foreign bytes.
    assert!(id.send_named(model,b"replaceSlot:in:expected:with:",&[Operand::object(h.int(0)),Operand::object(h.text(b"foreign")),Operand::object(old),Operand::object(replacement)]).is_err());
    assert!(id.send_named(model,b"replaceSlot:in:expected:with:",&[Operand::object(h.int(2)),Operand::object(target),Operand::object(old),Operand::object(replacement)]).is_err());

    let name_a=h.text(b"same name"); let name_b=h.text(b"same name"); assert_ne!(name_a,name_b);
    h.call(globals,b"at:put:",&[name_a,h.key(b"left")]); h.call(globals,b"at:put:",&[name_b,h.key(b"right")]);
    h.put(b"ImageNameA",name_a); h.put(b"ImageNameB",name_b);
    h.put(b"ImageOwnedMarker",h.key(b"saved")); h.put(b"ImageNil",h.named(b"nil"));
    h.put(b"ImageAliasA",old); h.put(b"ImageAliasB",old);
    h.put(b"ImageStorageAlias",h.call(globals,b"bindingOwner",&[]));
    let bank=h.call(old,b"clone",&[]); h.put(b"ImageBank",bank);
    h.call(globals,b"reserveBindingCompensationFor:count:",&[bank,h.int(5)]);
    let store=h.named(b"Store");
    h.call(store,b"assert:attr:value:",&[h.key(b"__globals__"),h.key(b"ImageOwnedMarker"),h.key(b"described")]);
    let path=std::env::temp_dir().join(format!("birnam-owned-globals-{}.image",std::process::id()));
    let filename=h.text(path.as_os_str().as_bytes()); let system=h.named(b"System");
    assert_eq!(h.call(system,b"saveImage:",&[filename]),h.named(b"true"));
    h.put(b"ImageOwnedMarker",h.key(b"resident"));
    h.eval(b"[Reflect defineMethod: #ownedImageTransient on: System as: [:self | #transient]]");
    let before=h.call(globals,b"bindingState",&[]);
    let original=h.entry(store,b"removeMethod:on:");
    let mut failure=ImageProbe{id,globals,key:h.key(b"ImageOwnedMarker"),value:h.key(b"callback"),selector:h.key(b"ownedImageTransient"),original,fired:Cell::new(false)};
    let vt=h.call(id.receiver,b"behaviorOf:",&[store]);
    install(id,vt,b"removeMethod:on:",fail_removal,core::ptr::from_mut(&mut failure).cast());
    let error=id.send_named(system,b"loadImage:",&[Operand::object(filename)]).unwrap_err();
    assert_eq!((error.status,error.detail),(882,73)); assert!(failure.fired.get());
    h.restore(store,b"removeMethod:on:",original);
    assert_eq!(h.binding(b"ImageOwnedMarker"),h.key(b"resident"));
    assert_eq!(h.call(globals,b"bindingState",&[]),before,"failed load must restore the exact resident root");
    assert_eq!(h.call(system,b"ownedImageTransient",&[]),h.key(b"transient"));
    assert_eq!(h.number(h.call(globals,b"reservedBindingCompensationFor:",&[bank])),5);
    for rebuild in [false,true] {
        h.call(system,b"loadImage:rebuild:",&[filename,h.named(if rebuild {b"true"} else {b"false"})]);
        assert_eq!(h.binding(b"ImageOwnedMarker"),h.key(b"saved"));
        assert_eq!(h.binding(b"ImageAliasA"),h.binding(b"ImageAliasB"));
        assert_eq!(h.binding(b"ImageStorageAlias"),h.call(globals,b"bindingOwner",&[]),"projection preserves storage aliases");
        assert_eq!(h.call(globals,b"includes:",&[h.key(b"ImageNil")]),h.named(b"true"));
        assert_eq!(h.call(globals,b"at:",&[h.binding(b"ImageNameA")]),h.key(b"left"));
        assert_eq!(h.call(globals,b"at:",&[h.binding(b"ImageNameB")]),h.key(b"right"));
        assert_eq!(h.number(h.call(globals,b"reservedBindingCompensationFor:",&[h.binding(b"ImageBank")])),0);
        assert_eq!(h.call(store,b"currentValue:attr:",&[h.key(b"__globals__"),h.key(b"ImageOwnedMarker")]),h.key(b"described"));
    }
    let mark=h.tokens.borrow().len();
    let key=h.call(h.named(b"Object"),b"clone",&[]);
    let value=h.call(h.named(b"Object"),b"clone",&[]);
    h.call(globals,b"at:put:",&[key,value]);
    for token in h.tokens.borrow_mut().drain(mark..).rev() { release(id,roots,token); }
    collect(id,gc);assert!(present(id,key)&&present(id,value));
    h.call(globals,b"remove:",&[key]);
    for token in h.tokens.borrow_mut().drain(mark..).rev() { release(id,roots,token); }
    collect(id,gc);assert!(!present(id,key)&&!present(id,value),"owned bindings must release removed identities");
    std::fs::remove_file(path).unwrap();
} }
