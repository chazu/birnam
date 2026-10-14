//! Owned slot geometry, current layouts, private behaviors, reentry and GC.
mod support;
use birnam_abi::{Oop, borrowed_slice, id_protocol::{NativeEntry, Operand, Outcome, Protocol}};
use std::{cell::Cell, ffi::c_void, os::unix::ffi::OsStrExt};
use support::*;
struct Hook {
    id: Protocol, model: Oop, gc: Oop, target: Oop, behavior: Oop,
    original: NativeEntry, calls: Cell<usize>, fail: Cell<bool>,
}
unsafe extern "C" fn collecting(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    let h = unsafe { &*c.cast::<Hook>() };
    h.calls.set(h.calls.get() + 1);
    if h.calls.get() == 1 {
        unsafe {
            send(h.id, h.model, b"setLayoutOf:to:rest:",
                &[Operand::object(h.behavior), Operand::bytes(&[0, 0, 0]), Operand::word(0)]);
            object(send(h.id, h.model, b"referenceSlotsOf:", &[Operand::object(h.target)]));
        }
    }
    unsafe { send(h.id, h.gc, b"collectPrecisely", &[]) };
    assert!(unsafe { present(h.id, h.target) }, "outer query target survives reentrant collection");
    if h.fail.get() && h.calls.get() == 2 { return Outcome::failed(927, 3); }
    unsafe { h.original.method.unwrap()(h.original.context, r, a, n) }
}
struct LayerHook {
    id: Protocol, gc: Oop, roots: Oop, target: Oop, private: Oop, replacement: Oop,
    original: NativeEntry, entered: Cell<bool>, saved: Cell<usize>,
}
unsafe extern "C" fn changing_parent(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    let h = unsafe { &*c.cast::<LayerHook>() };
    let args = unsafe { borrowed_slice(a, n) }.unwrap();
    let parent = unsafe { h.original.method.unwrap()(h.original.context, r, a, n) };
    if args[0].as_object() == Some(h.private) && !h.entered.replace(true) {
        unsafe {
            send(h.id, h.id.receiver, b"reassociateBehaviorOf:with:",
                &[Operand::object(h.target), Operand::object(h.replacement)]);
            send(h.id, h.gc, b"collectPrecisely", &[]);
            assert!(present(h.id, h.private), "inventory lost detached behavior during metadata callback");
            // Only after checking query-owned protection, retain it for test cleanup.
            h.saved.set(retain(h.id, h.roots, h.private));
        }
    }
    parent
}
unsafe extern "C" fn unreadable(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::failed(928, 4)
}
unsafe fn indices(id: Protocol, integer: Oop, vector: Oop) -> Vec<usize> {
    let size = unsafe { send(id, vector, b"size", &[]) };
    let size = unsafe { send(id, integer, b"wordOf:", &[size]) }.as_word().unwrap();
    (0..size).map(|i| unsafe {
        let index = send(id, integer, b"fromWord:", &[Operand::word(i)]);
        let value = send(id, vector, b"at:", &[index]);
        send(id, integer, b"wordOf:", &[value]).as_word().unwrap()
    }).collect()
}

#[test]
#[ignore = "production dylibs required"]
fn reference_inventory_uses_owned_metadata_and_roots_its_snapshot() { unsafe {
    let id = id();
    let ns = load(id, "bootstrap", &[Operand::bytes(directory().as_os_str().as_bytes())]);
    let named = |name: &[u8]| object(send(id, ns, b"at:", &[Operand::object(id.intern_name(name).unwrap())]));
    let model = named(b"ObjectModel"); let roots = named(b"Roots"); let gc = named(b"GC");
    let integer = named(b"Integer"); let nil = named(b"nil");
    let behavior = object(send(id, named(b"ObjectVt"), b"delegated", &[]));
    let bt = retain(id, roots, behavior);
    send(id, model, b"setLayoutOf:to:rest:", &[Operand::object(behavior), Operand::bytes(&[1, 0, 1]), Operand::word(0)]);
    let target = object(send(id, model, b"allocate:bytes:", &[Operand::object(behavior), Operand::word(3 * size_of::<Oop>())]));
    let tt = retain(id, roots, target);
    let inventory = object(send(id, model, b"referenceSlotsOf:", &[Operand::object(target)]));
    let it = retain(id, roots, inventory);
    assert_eq!(indices(id, integer, inventory), vec![0, 2]);
    let private = object(send(id, id.receiver, b"ensurePrivateVtableFor:", &[Operand::object(target)]));
    let pt = retain(id, roots, private);
    assert_eq!(indices(id, integer, object(send(id, model, b"referenceSlotsOf:", &[Operand::object(target)]))), vec![0, 2]);
    let text = send(id, named(b"String"), b"fromBytes:", &[Operand::bytes(b"opaque")]);
    assert_eq!(send(id, model, b"referenceSlotsOf:", &[text]).as_object(), Some(nil));
    let zero = send(id, integer, b"fromWord:", &[Operand::word(0)]);
    assert!(indices(id, integer, object(send(id, model, b"referenceSlotsOf:", &[zero]))).is_empty(), "owned raw numeric bits are never reference slots");
    assert_eq!(send(id, model, b"referenceSlotsOf:", &[Operand::object(private)]).as_object(), Some(nil));
    let empty = object(send(id, named(b"Object"), b"clone", &[]));
    assert!(indices(id, integer, object(send(id, model, b"referenceSlotsOf:", &[Operand::object(empty)]))).is_empty());
    let untyped = object(send(id, model, b"allocate:bytes:", &[Operand::object(named(b"ObjectVt")), Operand::word(size_of::<Oop>())]));
    assert_eq!(send(id, model, b"referenceSlotsOf:", &[Operand::object(untyped)]).as_object(), Some(nil));
    // Geometry does not consult the readable-value path.
    let model_vt = object(send(id, id.receiver, b"behaviorOf:", &[Operand::object(model)]));
    let raw = id.intern_name(b"slotAt:in:").unwrap();
    let read = send(id, model_vt, b"lookup", &[Operand::object(raw)]).as_entry().unwrap();
    install(id, model_vt, b"slotAt:in:", unreadable, std::ptr::null_mut());
    assert_eq!(indices(id, integer, object(send(id, model, b"referenceSlotsOf:", &[Operand::object(target)]))), vec![0, 2]);
    install(id, model_vt, b"slotAt:in:", read.method.unwrap(), read.context);
    // Replacing a metadata getter can detach the behavior being inspected.
    // A precise collection must still see the query's local behavior roots.
    let parent_selector = id.intern_name(b"parentOf:").unwrap();
    let parent = send(id, id.receiver, b"lookup", &[Operand::object(parent_selector)]).as_entry().unwrap();
    let layer = Box::leak(Box::new(LayerHook { id, gc, roots, target, private,
        replacement: named(b"ObjectVt"), original: parent, entered: Cell::new(false), saved: Cell::new(0) }));
    install(id, id.receiver, b"parentOf:", changing_parent, (&raw mut *layer).cast());
    release(id, roots, pt);
    let observed = object(send(id, model, b"referenceSlotsOf:", &[Operand::object(target)]));
    install(id, id.receiver, b"parentOf:", parent.method.unwrap(), parent.context);
    assert!(layer.entered.get());
    let pt = layer.saved.get(); assert_ne!(pt, 0);
    assert_eq!(indices(id, integer, observed), vec![0, 2]);
    send(id, id.receiver, b"reassociateBehaviorOf:with:", &[Operand::object(target), Operand::object(private)]);
    let integer_vt = object(send(id, id.receiver, b"behaviorOf:", &[Operand::object(integer)]));
    let selector = id.intern_name(b"fromWord:").unwrap();
    let original = send(id, integer_vt, b"lookup", &[Operand::object(selector)]).as_entry().unwrap();
    let h = Box::leak(Box::new(Hook { id, model, gc, target, behavior: private,
        original, calls: Cell::new(0), fail: Cell::new(false) }));
    let query = id.intern_name(b"referenceSlotsOf:").unwrap();
    install(id, integer_vt, b"fromWord:", collecting, (&raw mut *h).cast());
    release(id, roots, tt);
    release(id, roots, pt);
    let snapshot = object(id.send(model, query, &[Operand::object(target)]).unwrap());
    let st = retain(id, roots, snapshot);
    let target_again = retain(id, roots, target);
    assert_eq!(h.calls.get(), 2, "snapshot still constructs both pre-callback indices");
    install(id, integer_vt, b"fromWord:", original.method.unwrap(), original.context);
    assert_eq!(indices(id, integer, snapshot), vec![0, 2]);
    assert!(indices(id, integer, object(send(id, model, b"referenceSlotsOf:", &[Operand::object(target)]))).is_empty());
    assert_eq!(indices(id, integer, inventory), vec![0, 2], "old inventory does not update itself");
    send(id, model, b"setLayoutOf:to:rest:", &[Operand::object(private), Operand::bytes(&[1, 0, 1]), Operand::word(0)]);
    h.calls.set(0); h.fail.set(true);
    install(id, integer_vt, b"fromWord:", collecting, (&raw mut *h).cast());
    release(id, roots, target_again);
    let error = id.send(model, query, &[Operand::object(target)]).unwrap_err();
    assert_eq!((error.status, error.detail), (927, 3));
    install(id, integer_vt, b"fromWord:", original.method.unwrap(), original.context);
    release(id, roots, bt);
    send(id, gc, b"collectPrecisely", &[]);
    assert!(!present(id, target), "query and failure must not leak target protection");
    assert_eq!(indices(id, integer, snapshot), vec![0, 2], "returned snapshot remains caller-owned");
    for token in [it, st] { release(id, roots, token); }
    assert!(id.send(model, query, &[]).is_err());
    assert!(id.send(model, query, &[Operand::word(0)]).is_err());
} }
