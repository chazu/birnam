//! Integer index admission must retain current entries, policy and fallback.
mod support;
use birnam_abi::{
    Oop,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
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
    unsafe { h.entry.method.unwrap()(h.entry.context, r, a, n) }
}
struct Policy {
    id: Protocol,
    target: Oop,
    entry: NativeEntry,
    mode: Cell<usize>,
    calls: Cell<usize>,
}
unsafe extern "C" fn policy(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    let p = unsafe { &*c.cast::<Policy>() };
    p.calls.set(p.calls.get() + 1);
    if p.mode.get() == 2 {
        unsafe {
            install(
                p.id,
                p.target,
                b"wordOf:",
                p.entry.method.unwrap(),
                p.entry.context,
            );
        }
    }
    Outcome::loaded(Operand::word(usize::from(p.mode.get() != 0)))
}
unsafe extern "C" fn fail(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::failed(1901, 73)
}
#[test]
#[ignore = "production native modules required"]
fn integer_index_admission_preserves_replacement_policy_and_epoch_fence() {
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
        let integer = named(b"Integer");
        let vt = named(b"IntegerVt");
        let model = named(b"ObjectModel");
        let bind = |r, name: &[u8]| {
            send(
                id,
                id.receiver,
                b"bind:to:",
                &[
                    Operand::object(id.intern_name(name).unwrap()),
                    Operand::object(r),
                ],
            )
            .as_entry()
            .unwrap()
        };
        let restore =
            |r, name: &[u8], e: NativeEntry| install(id, r, name, e.method.unwrap(), e.context);
        let check = || id.send_named(integer, b"canOmitIndexing", &[]);
        assert_eq!(check().unwrap().as_word(), Some(1));
        let from = bind(integer, b"fromWord:");
        let word = bind(integer, b"wordOf:");
        for (name, entry) in [
            (b"fromWord:".as_slice(), from),
            (b"wordOf:".as_slice(), word),
        ] {
            // Same method, different context must decline without invoking it.
            install(id, vt, name, entry.method.unwrap(), std::ptr::null_mut());
            assert_eq!(check().unwrap().as_word(), Some(0));
            restore(vt, name, entry);
            assert_eq!(check().unwrap().as_word(), Some(1));
        }
        let lookup = bind(id.receiver, b"usesDefaultLookup:");
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
        assert_eq!(check().unwrap().as_word(), Some(0));
        assert!(h.calls.get() > 0, "original lookup callback must run");
        restore(id.receiver, b"usesDefaultLookup:", lookup);
        let binder = bind(id.receiver, b"bind:to:");
        install(id, id.receiver, b"bind:to:", fail, std::ptr::null_mut());
        let error = check().unwrap_err();
        assert_eq!((error.status, error.detail), (1901, 73));
        restore(id.receiver, b"bind:to:", binder);
        let matcher = bind(id.receiver, b"nativeBindingsMatch:");
        h.entry = matcher;
        h.calls.set(0);
        install(
            id,
            id.receiver,
            b"nativeBindingsMatch:",
            forward,
            (&raw mut h).cast(),
        );
        assert_eq!(check().unwrap().as_word(), Some(1));
        assert!(h.calls.get() > 0);
        restore(id.receiver, b"nativeBindingsMatch:", matcher);
        // A live permission may change without a method-epoch change.
        let model_vt = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(model)],
        ));
        let original_policy = bind(model, b"canOmitAllocationFor:");
        let mut p = Policy {
            id,
            target: vt,
            entry: word,
            mode: Cell::new(1),
            calls: Cell::new(0),
        };
        install(
            id,
            model_vt,
            b"canOmitAllocationFor:",
            policy,
            (&raw mut p).cast(),
        );
        let epoch = send(id, id.receiver, b"behaviorEpoch", &[]).as_word();
        assert_eq!(check().unwrap().as_word(), Some(1));
        p.mode.set(0);
        assert_eq!(check().unwrap().as_word(), Some(0));
        p.mode.set(1);
        assert_eq!(check().unwrap().as_word(), Some(1));
        assert_eq!(
            send(id, id.receiver, b"behaviorEpoch", &[]).as_word(),
            epoch
        );
        // Mutation during permission evaluation is rejected by the final fence.
        p.entry = NativeEntry::new(word.method.unwrap(), std::ptr::null_mut());
        p.mode.set(2);
        assert_eq!(check().unwrap().as_word(), Some(0));
        restore(vt, b"wordOf:", word);
        restore(model_vt, b"canOmitAllocationFor:", original_policy);
        assert_eq!(p.calls.get(), 4);
        assert_eq!(check().unwrap().as_word(), Some(1));
        // Ordinary conversion still uses the restored live factory and getter.
        let input = send(id, integer, b"fromWord:", &[Operand::word(41)]);
        assert_eq!(send(id, integer, b"wordOf:", &[input]).as_word(), Some(41));
    }
}
