//! Object Model and GC exercised only through production Siphon/Id/Bootstrap.
mod support;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
};
use core::{cell::Cell, ffi::c_void, ptr};
use std::os::unix::ffi::OsStrExt;
use support::*;
unsafe extern "C" fn one(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::word(1))
}
unsafe extern "C" fn two(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::word(2))
}
unsafe fn private(id: Protocol, value: Oop) -> Oop {
    object(unsafe {
        send(
            id,
            id.receiver,
            b"ensurePrivateVtableFor:",
            &[Operand::object(value)],
        )
    })
}
unsafe fn allocated(id: Protocol, model: Oop, behavior: Oop, bytes: usize) -> Oop {
    object(unsafe {
        send(
            id,
            model,
            b"allocate:bytes:",
            &[Operand::object(behavior), Operand::word(bytes)],
        )
    })
}
unsafe fn get(id: Protocol, model: Oop, value: Oop, index: usize) -> Operand {
    unsafe {
        send(
            id,
            model,
            b"slotAt:in:",
            &[Operand::word(index), Operand::object(value)],
        )
    }
}
unsafe fn put(id: Protocol, model: Oop, value: Oop, index: usize, edge: Oop) {
    unsafe {
        send(
            id,
            model,
            b"slotAt:in:put:",
            &[
                Operand::word(index),
                Operand::object(value),
                if edge.is_null() {
                    Operand::unit()
                } else {
                    Operand::object(edge)
                },
            ],
        );
    }
}
unsafe fn layout(id: Protocol, model: Oop, behavior: Oop, kinds: &[u8], rest: u8) {
    unsafe {
        send(
            id,
            model,
            b"setLayoutOf:to:rest:",
            &[
                Operand::object(behavior),
                Operand::bytes(kinds),
                Operand::word(rest as usize),
            ],
        );
    }
}
struct Finalizer {
    count: Cell<usize>,
    id: Protocol,
    model: Oop,
    watched_behavior: Cell<Oop>,
    gc: Oop,
    watched: Cell<Oop>,
}
unsafe extern "C" fn finalize(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let f = unsafe { &*context.cast::<Finalizer>() };
    let answer = (|| unsafe {
        let Some([value]) = borrowed_slice(args, count) else {
            return Err(Outcome::failed(native::INVALID_REQUEST, 0));
        };
        if value.as_object() != Some(f.watched.get()) {
            return Err(Outcome::failed(native::INVALID_REQUEST, 0));
        }
        // The metadata must already be retired when an object's finalizer runs.
        if f.id
            .send_named(
                f.id.receiver,
                b"ownerOf:",
                &[Operand::object(f.watched_behavior.get())],
            )?
            .kind()
            != native::UNIT
        {
            return Err(Outcome::failed(native::INVALID_REQUEST, 0));
        }
        // A phase check must still reject private-behavior preparation even when
        // the receiver needs no fresh behavior (Id is its own behavior).
        if f.id
            .send_named(
                f.id.receiver,
                b"ensurePrivateVtableFor:",
                &[Operand::object(f.id.receiver)],
            )
            .is_ok()
        {
            return Err(Outcome::failed(native::INVALID_REQUEST, 0));
        }
        if f.id
            .send_named(
                f.model,
                b"allocate:bytes:",
                &[Operand::object(f.id.receiver), Operand::word(0)],
            )
            .is_ok()
        {
            return Err(Outcome::failed(native::INVALID_REQUEST, 0));
        }
        if f.id.send_named(f.gc, b"collect", &[])?.as_word() != Some(0) {
            return Err(Outcome::failed(native::INVALID_REQUEST, 0));
        }
        f.count.set(f.count.get() + 1);
        Ok(Operand::unit())
    })();
    answer.map_or_else(|e| e, Outcome::loaded)
}
struct Reentry {
    id: Protocol,
    gc: Oop,
    source: Oop,
    original: NativeEntry,
    entered: Cell<bool>,
}
unsafe extern "C" fn allocate_with_reentry(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let state = unsafe { &*context.cast::<Reentry>() };
    if !state.entered.replace(true) {
        // The independent native oracle knows this test object's one-Oop extent.
        // Change the source after clone took its snapshot, then force exact GC.
        unsafe {
            state.source.cast::<Oop>().write(ptr::null_mut());
        }
        if let Err(e) = unsafe { state.id.send_named(state.gc, b"collectPrecisely", &[]) } {
            return e;
        }
    }
    unsafe { state.original.method.unwrap()(state.original.context, receiver, args, count) }
}
struct Counted {
    original: NativeEntry,
    count: Cell<usize>,
}
unsafe extern "C" fn counted(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let c = unsafe { &*context.cast::<Counted>() };
    c.count.set(c.count.get() + 1);
    unsafe { c.original.method.unwrap()(c.original.context, receiver, args, count) }
}
#[test]
#[ignore = "scripts/native-message-test.sh builds production dylibs"]
fn ordinary_objects_private_behaviors_and_retirement_use_live_messages() {
    unsafe {
        let id = id();
        let ns = load(
            id,
            "bootstrap",
            &[Operand::bytes(directory().as_os_str().as_bytes())],
        );
        let named = |name: &[u8]| {
            object(send(
                id,
                ns,
                b"at:",
                &[Operand::object(id.intern_name(name).unwrap())],
            ))
        };
        let gc = named(b"GC");
        let roots = named(b"Roots");
        let model = named(b"ObjectModel");
        let prototype = named(b"Object");
        let base = named(b"ObjectVt");
        assert_eq!(load(id, "object_model", &[Operand::object(ns)]), model);
        assert_eq!(
            send(
                id,
                id.receiver,
                b"ownerOf:",
                &[Operand::object(ptr::null_mut())]
            )
            .kind(),
            native::UNIT
        );
        let model_behavior = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(model)],
        ));
        let retire_selector = id.intern_name(b"retire:").unwrap();
        let retire = send(
            id,
            model_behavior,
            b"lookup",
            &[Operand::object(retire_selector)],
        )
        .as_entry()
        .unwrap();
        let retirement = Box::leak(Box::new(Counted {
            original: retire,
            count: Cell::new(0),
        }));
        install(
            id,
            model_behavior,
            b"retire:",
            counted,
            ptr::from_mut(retirement).cast(),
        );
        collect(id, gc);
        assert_eq!(retirement.count.get(), 1);
        install(
            id,
            model_behavior,
            b"retire:",
            retire.method.unwrap(),
            retire.context,
        );

        assert_eq!(private(id, prototype), base);
        assert_eq!(private(id, base), base);
        for value in [named(b"true"), named(b"nil"), ns] {
            let original = object(send(
                id,
                id.receiver,
                b"behaviorOf:",
                &[Operand::object(value)],
            ));
            assert_eq!(private(id, value), original);
        }
        let original = object(send(id, prototype, b"clone", &[]));
        let sibling = object(send(id, prototype, b"new", &[]));
        let original_root = retain(id, roots, original);
        let sibling_root = retain(id, roots, sibling);
        assert_ne!(original, sibling);
        let personal = private(id, original);
        assert_eq!(private(id, original), personal);
        assert_eq!(
            object(send(
                id,
                id.receiver,
                b"parentOf:",
                &[Operand::object(personal)]
            )),
            base
        );
        assert_eq!(
            object(send(
                id,
                id.receiver,
                b"ownerOf:",
                &[Operand::object(personal)]
            )),
            original
        );
        assert_eq!(
            object(send(
                id,
                id.receiver,
                b"behaviorOf:",
                &[Operand::object(sibling)]
            )),
            base
        );
        install(id, base, b"answer", one, ptr::null_mut());
        install(id, personal, b"answer", two, ptr::null_mut());
        assert_eq!(send(id, original, b"answer", &[]).as_word(), Some(2));
        assert_eq!(send(id, sibling, b"answer", &[]).as_word(), Some(1));
        let copy = object(send(id, original, b"clone", &[]));
        let copy_root = retain(id, roots, copy);
        assert_eq!(
            object(send(
                id,
                id.receiver,
                b"behaviorOf:",
                &[Operand::object(copy)]
            )),
            personal
        );
        let copy_behavior = private(id, copy);
        assert_ne!(copy_behavior, personal);
        assert_eq!(
            object(send(
                id,
                id.receiver,
                b"parentOf:",
                &[Operand::object(copy_behavior)]
            )),
            personal
        );
        // Existing Id reflection, with no Object Model enumeration adapter.
        assert!(
            send(id, id.receiver, b"behaviorCount", &[])
                .as_word()
                .unwrap()
                > 0
        );
        let selector = object(send(
            id,
            id.receiver,
            b"selectorAt:in:",
            &[Operand::word(0), Operand::object(personal)],
        ));
        assert_eq!(selector, id.intern_name(b"answer").unwrap());
        // Live dependency replacement remains visible after construction.
        let lookup = |name: &[u8]| {
            send(
                id,
                id.receiver,
                b"lookup",
                &[Operand::object(id.intern_name(name).unwrap())],
            )
            .as_entry()
            .unwrap()
        };
        let prior = lookup(b"ownerOf:");
        let counted_owner = Box::leak(Box::new(Counted {
            original: prior,
            count: Cell::new(0),
        }));
        install(
            id,
            id.receiver,
            b"ownerOf:",
            counted,
            ptr::from_mut(counted_owner).cast(),
        );
        assert_eq!(private(id, original), personal);
        assert!(counted_owner.count.get() > 0);
        install(
            id,
            id.receiver,
            b"ownerOf:",
            prior.method.unwrap(),
            prior.context,
        );
        // A behavior root retains its owner, but the map itself roots neither.
        let behavior_root = retain(id, roots, personal);
        release(id, roots, original_root);
        release(id, roots, copy_root);
        collect(id, gc);
        assert!(present(id, original));
        assert!(!present(id, copy));
        release(id, roots, behavior_root);
        collect(id, gc);
        assert!(!present(id, original) && !present(id, personal) && !present(id, copy_behavior));
        assert_eq!(
            send(id, id.receiver, b"ownerOf:", &[Operand::object(personal)]).kind(),
            native::UNIT
        );
        release(id, roots, sibling_root);
        // Exact metadata is copied, accepts arbitrary native codes, and does not
        // inherit. Ordinary slots retain children and shallow clones share them.
        let b = object(send(id, base, b"delegated", &[]));
        let b_root = retain(id, roots, b);
        let mut kinds = [1, 9];
        layout(id, model, b, &kinds, 1);
        kinds.fill(0);
        assert_eq!(
            send(
                id,
                model,
                b"slotKindOf:at:",
                &[Operand::object(b), Operand::word(1)]
            )
            .as_word(),
            Some(9)
        );
        assert_eq!(
            send(
                id,
                model,
                b"slotKindOf:at:",
                &[Operand::object(b), Operand::word(usize::MAX)]
            )
            .as_word(),
            Some(1)
        );
        assert_eq!(
            send(
                id,
                model,
                b"allOopLayoutOf:words:",
                &[Operand::object(b), Operand::word(2)]
            )
            .as_word(),
            Some(0)
        );
        let child_behavior = object(send(id, b, b"delegated", &[]));
        let cb_root = retain(id, roots, child_behavior);
        assert_eq!(
            send(
                id,
                model,
                b"slotKindOf:at:",
                &[Operand::object(child_behavior), Operand::word(0)]
            )
            .as_word(),
            Some(0)
        );
        layout(id, model, b, &[], 1);
        let source = allocated(id, model, b, 8);
        let source_root = retain(id, roots, source);
        let child = object(send(id, prototype, b"clone", &[]));
        put(id, model, source, 0, child);
        let clone = object(send(id, source, b"clone", &[]));
        let clone_root = retain(id, roots, clone);
        assert_ne!(source, clone);
        assert_eq!(object(get(id, model, clone, 0)), child);
        put(id, model, source, 0, ptr::null_mut());
        collect(id, gc);
        assert!(present(id, child));
        release(id, roots, clone_root);
        collect(id, gc);
        assert!(!present(id, child));
        assert!(
            id.send_named(
                model,
                b"slotAt:in:",
                &[Operand::word(usize::MAX), Operand::object(source)]
            )
            .is_err()
        );
        // Raw padded bytes are copied exactly; finalizer ownership is not copied.
        layout(id, model, b, &[], 9);
        let raw = allocated(id, model, b, 9);
        let raw_root = retain(id, roots, raw);
        for i in 0..16 {
            raw.cast::<u8>().add(i).write(i as u8 + 7);
        }
        let raw_copy = object(send(id, raw, b"clone", &[]));
        let raw_copy_root = retain(id, roots, raw_copy);
        assert_eq!(
            send(
                id,
                id.receiver,
                b"allocationSizeOf:",
                &[Operand::object(raw_copy)]
            )
            .as_word(),
            Some(16)
        );
        assert_eq!(
            std::slice::from_raw_parts(raw.cast::<u8>(), 16),
            std::slice::from_raw_parts(raw_copy.cast::<u8>(), 16)
        );
        // A native family with unowned payload cannot fall through to raw copying.
        let foreign = allocate(id, b, 8);
        assert!(id.send_named(foreign, b"clone", &[]).is_err());
        release(id, roots, raw_root);
        release(id, roots, raw_copy_root);
        // Clone's copied references survive allocating reentry even when the
        // source is changed before collection. No shadow collector is involved.
        layout(id, model, b, &[], 1);
        let snapshot_child = object(send(id, prototype, b"clone", &[]));
        put(id, model, source, 0, snapshot_child);
        let original_allocate = lookup(b"allocate");
        let reentry = Box::leak(Box::new(Reentry {
            id,
            gc,
            source,
            original: original_allocate,
            entered: Cell::new(false),
        }));
        install(
            id,
            id.receiver,
            b"allocate",
            allocate_with_reentry,
            ptr::from_mut(reentry).cast(),
        );
        let saved = object(send(id, source, b"clone", &[]));
        let saved_root = retain(id, roots, saved);
        install(
            id,
            id.receiver,
            b"allocate",
            original_allocate.method.unwrap(),
            original_allocate.context,
        );
        assert!(reentry.entered.get());
        assert!(present(id, snapshot_child));
        assert_eq!(object(get(id, model, saved, 0)), snapshot_child);
        assert_eq!(get(id, model, source, 0).kind(), native::UNIT);
        release(id, roots, saved_root);
        release(id, roots, source_root);
        release(id, roots, b_root);
        release(id, roots, cb_root);
        collect(id, gc);
        assert!(!present(id, snapshot_child));
        // Repeatedly create/retire private cycles. Metadata must be gone before
        // finalization and never apply to a newly allocated identity.
        let f = Box::leak(Box::new(Finalizer {
            count: Cell::new(0),
            id,
            model,
            watched_behavior: Cell::new(ptr::null_mut()),
            gc,
            watched: Cell::new(ptr::null_mut()),
        }));
        let fb = object(send(id, base, b"delegated", &[]));
        install(id, fb, b"finalize:", finalize, ptr::from_mut(f).cast());
        let finalizer = allocate(id, fb, 0);
        retain(id, roots, finalizer);
        for _ in 0..24 {
            let value = object(send(id, prototype, b"clone", &[]));
            let behavior = private(id, value);
            f.watched.set(value);
            f.watched_behavior.set(behavior);
            send(
                id,
                gc,
                b"finalize:with:",
                &[Operand::object(value), Operand::object(finalizer)],
            );
            let duplicate = object(send(id, value, b"clone", &[]));
            collect(id, gc);
            assert!(!present(id, value) && !present(id, duplicate) && !present(id, behavior));
            assert_eq!(
                send(id, id.receiver, b"ownerOf:", &[Operand::object(behavior)]).kind(),
                native::UNIT
            );
        }
        assert_eq!(f.count.get(), 24);
        // All construction remains usable with collection before every allocation.
        send(id, gc, b"budget:", &[Operand::word(1)]);
        let fresh = object(send(id, prototype, b"clone", &[]));
        let fresh_root = retain(id, roots, fresh);
        let behavior = private(id, fresh);
        assert_eq!(private(id, fresh), behavior);
        assert!(present(id, fresh) && present(id, behavior));
        release(id, roots, fresh_root);
        send(id, gc, b"budget:", &[Operand::word(1 << 20)]);
        collect(id, gc);
    }
}
