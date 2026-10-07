//! Guarded bulk cleanup must preserve tokens, order, reentry and live fallback.
mod support;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use std::{
    cell::{Cell, RefCell},
    ffi::c_void,
};
use support::*;
struct Visitor(RefCell<Vec<usize>>);
unsafe extern "C" fn visit(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    let [v] = unsafe { borrowed_slice(a, n) }.unwrap() else {
        panic!("visit")
    };
    unsafe { &*c.cast::<Visitor>() }
        .0
        .borrow_mut()
        .push(object(*v).addr());
    Outcome::loaded(Operand::unit())
}
struct Hook {
    id: Protocol,
    behavior: Oop,
    original: NativeEntry,
    calls: RefCell<Vec<usize>>,
    next: Cell<*mut Hook>,
    collect: Cell<Oop>,
}
unsafe extern "C" fn release_hook(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    let h = unsafe { &*c.cast::<Hook>() };
    h.calls.borrow_mut().push(
        unsafe { borrowed_slice(a, n) }.unwrap()[0]
            .as_word()
            .unwrap(),
    );
    let next = h.next.replace(std::ptr::null_mut());
    if !next.is_null() {
        unsafe { install(h.id, h.behavior, b"release:", release_hook, next.cast()) };
    }
    if !h.collect.get().is_null() {
        unsafe { send(h.id, h.collect.get(), b"collectPrecisely", &[]) };
    }
    unsafe { (h.original.method.unwrap())(h.original.context, r, a, n) }
}
struct EpochHook {
    id: Protocol,
    original: NativeEntry,
    behavior: Oop,
    replacement: *mut Hook,
    calls: Cell<usize>,
}
unsafe extern "C" fn epoch_hook(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    let h = unsafe { &*c.cast::<EpochHook>() };
    let calls = h.calls.get() + 1;
    h.calls.set(calls);
    if calls == 2 {
        unsafe {
            install(
                h.id,
                h.behavior,
                b"release:",
                release_hook,
                h.replacement.cast(),
            )
        };
    }
    unsafe { (h.original.method.unwrap())(h.original.context, r, a, n) }
}
unsafe fn entry(id: Protocol, r: Oop, name: &[u8]) -> NativeEntry {
    unsafe {
        send(
            id,
            id.receiver,
            b"bind:to:",
            &[
                Operand::object(id.intern_name(name).unwrap()),
                Operand::object(r),
            ],
        )
    }
    .as_entry()
    .unwrap()
}
unsafe fn bind(id: Protocol, b: Oop, name: &[u8], e: NativeEntry) {
    unsafe {
        send(
            id,
            b,
            b"addMethod",
            &[
                Operand::object(id.intern_name(name).unwrap()),
                Operand::entry(e),
            ],
        )
    };
}
unsafe fn bulk(id: Protocol, roots: Oop, tokens: &[usize]) -> usize {
    let bytes: Vec<_> = tokens.iter().flat_map(|t| t.to_ne_bytes()).collect();
    unsafe { send(id, roots, b"tryReleaseMany:", &[Operand::bytes(&bytes)]) }
        .as_word()
        .unwrap()
}
#[test]
#[ignore = "requires current production Roots dylib"]
fn bulk_release_is_exact_or_declines_before_any_mutation() {
    unsafe {
        let id = id();
        let roots = load(id, "root_registry", &[]);
        let b = behavior(id);
        let objects: Vec<_> = (0..4).map(|_| allocate(id, b, 0)).collect();
        let log = Box::leak(Box::new(Visitor(RefCell::new(Vec::new()))));
        let vb = behavior(id);
        install(id, vb, b"visit:", visit, (log as *mut Visitor).cast());
        let visitor = allocate(id, vb, 0);
        let snapshot = || {
            log.0.borrow_mut().clear();
            send(id, roots, b"rootsDo:", &[Operand::object(visitor)]);
            log.0.borrow().clone()
        };
        let baseline = snapshot();
        let shape = [0, 1, 0, 2, 3, 1, 2, 0];
        let tokens: Vec<_> = shape
            .iter()
            .map(|i| retain(id, roots, objects[*i]))
            .collect();
        let request = [tokens[1], tokens[4], tokens[0], tokens[1], usize::MAX];
        for token in request.iter().rev() {
            send(id, roots, b"release:", &[Operand::word(*token)]);
        }
        let expected = snapshot();
        for t in tokens {
            send(id, roots, b"release:", &[Operand::word(t)]);
        }
        assert_eq!(snapshot(), baseline);
        let tokens: Vec<_> = shape
            .iter()
            .map(|i| retain(id, roots, objects[*i]))
            .collect();
        let request = [tokens[1], tokens[4], tokens[0], tokens[1], usize::MAX];
        assert_eq!(bulk(id, roots, &request), 1);
        assert_eq!(
            snapshot(),
            expected,
            "swap removal order and duplicates must match"
        );
        assert_eq!(bulk(id, roots, &tokens), 1);
        assert_eq!(snapshot(), baseline);
        let rb = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(roots)],
        ));
        let original = entry(id, roots, b"release:");
        let mut second = Hook {
            id,
            behavior: rb,
            original,
            calls: RefCell::new(Vec::new()),
            next: Cell::new(std::ptr::null_mut()),
            collect: Cell::new(std::ptr::null_mut()),
        };
        let mut first = Hook {
            id,
            behavior: rb,
            original,
            calls: RefCell::new(Vec::new()),
            next: Cell::new(&raw mut second),
            collect: Cell::new(std::ptr::null_mut()),
        };
        install(id, rb, b"release:", release_hook, (&raw mut first).cast());
        let tokens: Vec<_> = objects.iter().map(|o| retain(id, roots, *o)).collect();
        let before = snapshot();
        assert_eq!(bulk(id, roots, &tokens), 0);
        assert_eq!(snapshot(), before);
        assert!(first.calls.borrow().is_empty());
        for t in tokens.iter().rev() {
            send(id, roots, b"release:", &[Operand::word(*t)]);
        }
        assert_eq!(*first.calls.borrow(), vec![tokens[3]]);
        assert_eq!(
            *second.calls.borrow(),
            vec![tokens[2], tokens[1], tokens[0]]
        );
        bind(id, rb, b"release:", original);
        // Mutation at the final epoch fence declines, without consuming tokens.
        let original_epoch = entry(id, id.receiver, b"behaviorEpoch");
        let mut epoch = EpochHook {
            id,
            original: original_epoch,
            behavior: rb,
            replacement: &raw mut second,
            calls: Cell::new(0),
        };
        install(
            id,
            id.receiver,
            b"behaviorEpoch",
            epoch_hook,
            (&raw mut epoch).cast(),
        );
        let token = retain(id, roots, objects[0]);
        let before = snapshot();
        assert_eq!(bulk(id, roots, &[token]), 0);
        assert_eq!(snapshot(), before);
        assert_eq!(
            epoch.calls.get(),
            0,
            "changed proof services must decline before invoking them"
        );
        bind(id, id.receiver, b"behaviorEpoch", original_epoch);
        send(id, roots, b"release:", &[Operand::word(token)]);
        bind(id, rb, b"release:", original);
        assert_eq!(snapshot(), baseline);
        // Bad ABI input rejects before touching the registry.
        let token = retain(id, roots, objects[0]);
        let before = snapshot();
        assert!(
            id.send_named(roots, b"tryReleaseMany:", &[Operand::bytes(&[1])])
                .is_err()
        );
        assert_eq!(snapshot(), before);
        release(id, roots, token);
        let fresh = retain(id, roots, objects[0]);
        assert!(fresh > token);
        release(id, roots, fresh);
    }
}
