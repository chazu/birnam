//! Ordinary-slot adapters retain live dispatch and nested, exact owner roots.
mod support;
use birnam_abi::{Oop, borrowed_slice, id_protocol::{Operand, Outcome, Protocol}};
use std::{cell::{Cell, RefCell}, ffi::c_void, os::unix::ffi::OsStrExt};
use support::*;

struct Probe {
    id: Protocol, model: Oop, gc: Oop, visitor: Oop,
    target: Oop, edge: Oop, index: Oop,
    seen: RefCell<Vec<Oop>>, nested: Cell<bool>, fail: Cell<bool>, calls: Cell<usize>,
}
unsafe extern "C" fn visit(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    let p = unsafe { &*c.cast::<Probe>() };
    let [value] = (unsafe { borrowed_slice(a, n) }).unwrap() else { panic!("visitor arity") };
    p.seen.borrow_mut().push(object(*value));
    Outcome::loaded(Operand::unit())
}
unsafe fn snapshot(p: &Probe) -> Vec<Oop> {
    p.seen.borrow_mut().clear();
    unsafe { send(p.id, p.model, b"rootsDo:", &[Operand::object(p.visitor)]) };
    p.seen.borrow().clone()
}
unsafe extern "C" fn convert(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    let p = unsafe { &*c.cast::<Probe>() };
    assert_eq!((unsafe { borrowed_slice(a, n) }).unwrap()[0].as_object(), Some(p.index));
    p.calls.set(p.calls.get() + 1);
    let before = unsafe { snapshot(p) };
    for v in [p.target, p.index] { assert!(before.contains(&v), "inputs reported before callback"); }
    unsafe { send(p.id, p.gc, b"collectPrecisely", &[]) };
    for v in [p.target, p.edge, p.index] { assert!(unsafe { present(p.id, v) }); }
    if !p.nested.replace(true) {
        // Reenter the boxed adapter, then verify its exit did not pop the parent.
        assert_eq!(unsafe { send(p.id, p.model, b"slot:in:",
            &[Operand::object(p.index), Operand::object(p.target)]) }.as_object(), Some(p.edge));
        p.nested.set(false);
        assert_eq!(unsafe { snapshot(p) }, before, "nested scope restores exact parent");
        unsafe { send(p.id, p.gc, b"collectPrecisely", &[]) };
        if p.fail.get() { return Outcome::failed(912, 7); }
    }
    Outcome::loaded(Operand::word(0))
}
unsafe extern "C" fn replaced_read(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::failed(913, 8)
}

#[test]
#[ignore = "production dylibs required"]
fn boxed_slots_use_current_conversion_and_raw_access_with_scoped_roots() {
    unsafe {
        let id = id();
        let ns = load(id, "bootstrap", &[Operand::bytes(directory().as_os_str().as_bytes())]);
        let named = |name: &[u8]| object(send(id, ns, b"at:", &[Operand::object(id.intern_name(name).unwrap())]));
        let model = named(b"ObjectModel"); let gc = named(b"GC"); let roots = named(b"Roots");
        let original_integer = named(b"Integer");
        let original_token = retain(id, roots, original_integer);
        let prototype = named(b"Object");
        let target_behavior = object(send(id, named(b"ObjectVt"), b"delegated", &[]));
        let behavior_token = retain(id, roots, target_behavior);
        send(id, model, b"setLayoutOf:to:rest:", &[Operand::object(target_behavior), Operand::bytes(&[1]), Operand::word(0)]);
        let target = object(send(id, model, b"allocate:bytes:", &[Operand::object(target_behavior), Operand::word(size_of::<Oop>())]));
        let target_token = retain(id, roots, target);
        let edge = object(send(id, prototype, b"clone", &[])); let edge_token = retain(id, roots, edge);
        let index = object(send(id, prototype, b"clone", &[])); let index_token = retain(id, roots, index);
        send(id, model, b"slotAt:in:put:", &[Operand::word(0), Operand::object(target), Operand::object(edge)]);
        let integer = object(send(id, prototype, b"clone", &[])); let integer_token = retain(id, roots, integer);
        let visitor = object(send(id, prototype, b"clone", &[])); let visitor_token = retain(id, roots, visitor);
        let p = Box::leak(Box::new(Probe { id, model, gc, visitor, target, edge, index,
            seen: RefCell::new(Vec::new()), nested: Cell::new(false), fail: Cell::new(false), calls: Cell::new(0) }));
        let context = (&raw mut *p).cast();
        let visitor_vt = object(send(id, id.receiver, b"ensurePrivateVtableFor:", &[Operand::object(visitor)]));
        install(id, visitor_vt, b"visit:", visit, context);
        let integer_vt = object(send(id, id.receiver, b"ensurePrivateVtableFor:", &[Operand::object(integer)]));
        install(id, integer_vt, b"wordOf:", convert, context);
        let integer_name = id.intern_name(b"Integer").unwrap();
        send(id, ns, b"at:put:", &[Operand::object(integer_name), Operand::object(integer)]);
        let getter = id.intern_name(b"slot:in:").unwrap();
        let setter = id.intern_name(b"slot:in:put:").unwrap();
        let read = [Operand::object(index), Operand::object(target)];
        let write = [read[0], read[1], Operand::object(edge)];
        // Drop external protection immediately before entry. Precise collection
        // ignores Rust locals and Probe's opaque native pointers.
        for token in [target_token, edge_token, index_token, behavior_token] { release(id, roots, token); }
        let before = snapshot(p);
        assert_eq!(id.send(model, setter, &write).unwrap().as_object(), Some(target));
        assert_eq!(snapshot(p), before, "successful write leaves no private scope");
        assert_eq!(id.send(model, getter, &read).unwrap().as_object(), Some(edge));
        assert_eq!(snapshot(p), before, "successful read leaves no private scope");
        assert_eq!(p.calls.get(), 4, "replacement conversion is current on both calls and reentry");
        p.fail.set(true);
        let error = id.send(model, setter, &write).unwrap_err();
        assert_eq!((error.status, error.detail), (912, 7));
        assert_eq!(snapshot(p), before, "failure leaves no private scope");
        p.fail.set(false);
        // Restore the numeric owner, then prove the raw access remains a send.
        send(id, ns, b"at:put:", &[Operand::object(integer_name), Operand::object(original_integer)]);
        let model_vt = object(send(id, id.receiver, b"behaviorOf:", &[Operand::object(model)]));
        let raw_selector = id.intern_name(b"slotAt:in:").unwrap();
        let original_raw = send(id, model_vt, b"lookup", &[Operand::object(raw_selector)]).as_entry().unwrap();
        install(id, model_vt, b"slotAt:in:", replaced_read, std::ptr::null_mut());
        let zero = send(id, original_integer, b"fromWord:", &[Operand::word(0)]);
        let error = id.send(model, getter, &[zero, Operand::object(target)]).unwrap_err();
        assert_eq!((error.status, error.detail), (913, 8));
        assert_eq!(snapshot(p), before);
        install(id, model_vt, b"slotAt:in:", original_raw.method.unwrap(), original_raw.context);
        send(id, gc, b"collectPrecisely", &[]);
        for v in [target, edge, index] { assert!(!present(id, v), "exited scopes must not retain objects"); }
        for token in [original_token, integer_token, visitor_token] { release(id, roots, token); }
        assert!(id.send(model, getter, &[]).is_err());
        assert!(id.send(model, setter, &[Operand::word(0), Operand::unit(), Operand::unit()]).is_err());
    }
}
