//! Optional batched admission retains ordered custom guards and exact contexts.
mod support;
use birnam_abi::{
    binding_guard::{BindingGuard, SERVICES},
    id_protocol::{NativeEntry, Operand, Outcome},
    Oop,
};
use std::{cell::Cell, ffi::c_void, os::unix::ffi::OsStrExt};
use support::*;
struct Hook {
    entry: NativeEntry,
    calls: Cell<usize>,
}
unsafe extern "C" fn forward(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    let h = unsafe { &*c.cast::<Hook>() };
    h.calls.set(h.calls.get() + 1);
    unsafe { (h.entry.method.unwrap())(h.entry.context, r, a, n) }
}
unsafe extern "C" fn fail(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::failed(1901, 73)
}
#[test]
#[ignore = "production native guard modules required"]
fn batch_declines_changed_services_and_preserves_guard_errors() {
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
        let word = named(b"Word");
        let model = named(b"ObjectModel");
        let bind = |receiver, selector| {
            send(
                id,
                id.receiver,
                b"bind:to:",
                &[Operand::object(selector), Operand::object(receiver)],
            )
            .as_entry()
            .unwrap()
        };
        let alloc = id.intern_name(b"allocate:bytes:").unwrap();
        let entry = bind(model, alloc);
        let services = SERVICES.map(|name| id.intern_name(name).unwrap());
        let declaration = id.intern_name(b"nativeBindingQuerySemantics:for:").unwrap();
        let proof = BindingGuard::discover(id, services, declaration);
        assert!(proof.admits(id, &[(model, alloc, entry)]).unwrap());
        assert!(
            !proof
                .admits(
                    id,
                    &[(
                        model,
                        alloc,
                        NativeEntry::new(entry.method.unwrap(), std::ptr::null_mut())
                    )]
                )
                .unwrap(),
            "context participates in admission"
        );
        let check = || {
            id.send_named(
                model,
                b"canOmitAllocationFor:",
                &[Operand::object(named(b"WordVt"))],
            )
        };
        assert_eq!(check().unwrap().as_word(), Some(1));
        let lookup = bind(id.receiver, services[0]);
        let mut h = Hook {
            entry: lookup,
            calls: Cell::new(0),
        };
        install(
            id,
            id.receiver,
            b"usesDefaultLookup:",
            forward,
            (&raw mut h).cast(),
        );
        assert!(!proof.admits(id, &[(model, alloc, entry)]).unwrap());
        assert_eq!(check().unwrap().as_word(), Some(0));
        assert_eq!(
            h.calls.get(),
            1,
            "ObjectModel's original lookup callback executes; GC then declines changed allocation services"
        );
        install(
            id,
            id.receiver,
            b"usesDefaultLookup:",
            lookup.method.unwrap(),
            lookup.context,
        );
        let original_bind = bind(id.receiver, services[1]);
        install(id, id.receiver, b"bind:to:", fail, std::ptr::null_mut());
        let error = check().unwrap_err();
        assert_eq!((error.status, error.detail), (1901, 73));
        install(
            id,
            id.receiver,
            b"bind:to:",
            original_bind.method.unwrap(),
            original_bind.context,
        );
        let matcher = bind(id.receiver, services[2]);
        h.entry = matcher;
        h.calls.set(0);
        install(
            id,
            id.receiver,
            b"nativeBindingsMatch:",
            forward,
            (&raw mut h).cast(),
        );
        assert!(
            !proof.admits(id, &[(model, alloc, entry)]).unwrap(),
            "forwarded matcher checks its own binding"
        );
        assert_eq!(check().unwrap().as_word(), Some(1));
        assert!(h.calls.get() >= 3);
        install(
            id,
            id.receiver,
            b"nativeBindingsMatch:",
            matcher.method.unwrap(),
            matcher.context,
        );
        // A guard constructed under a noncanonical query stays conservatively disabled.
        h.entry = lookup;
        install(
            id,
            id.receiver,
            b"usesDefaultLookup:",
            forward,
            (&raw mut h).cast(),
        );
        let disabled = BindingGuard::discover(id, services, declaration);
        install(
            id,
            id.receiver,
            b"usesDefaultLookup:",
            lookup.method.unwrap(),
            lookup.context,
        );
        assert!(!disabled.admits(id, &[(model, alloc, entry)]).unwrap());
        assert!(proof.admits(id, &[(model, alloc, entry)]).unwrap());
        // Changing only GC's second batched target must also decline.
        let gc = named(b"GC");
        let gc_vt = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(gc)],
        ));
        let prepare = bind(gc, id.intern_name(b"prepareAllocation").unwrap());
        h.entry = prepare;
        h.calls.set(0);
        install(
            id,
            gc_vt,
            b"prepareAllocation",
            forward,
            (&raw mut h).cast(),
        );
        assert_eq!(check().unwrap().as_word(), Some(0));
        assert_eq!(
            h.calls.get(),
            0,
            "admission must never perform allocation preparation"
        );
        install(
            id,
            gc_vt,
            b"prepareAllocation",
            prepare.method.unwrap(),
            prepare.context,
        );
        assert_eq!(check().unwrap().as_word(), Some(1));
        // Ordinary Word input still works after all replacements are restored.
        let input = send(id, word, b"fromBits:", &[Operand::word(7)]);
        assert_eq!(send(id, object(input), b"bits", &[]).as_word(), Some(7));
    }
}
