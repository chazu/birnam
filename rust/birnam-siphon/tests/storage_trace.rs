//! Real Storage/Trace modules, scoped inventory roots and live owner selection.
mod support;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{self as native, Operand, Outcome, Protocol},
};
use core::{
    cell::{Cell, RefCell},
    ffi::c_void,
    ptr,
};
use std::{collections::HashMap, os::unix::ffi::OsStrExt};
use support::*;
struct Probe {
    id: Protocol,
    roots: Oop,
    gc: Oop,
    storage: Oop,
    trace: Oop,
    report: Cell<Oop>,
    edges: RefCell<HashMap<usize, Vec<Oop>>>,
    rows: RefCell<HashMap<usize, (usize, bool)>>,
    completed: Cell<usize>,
    observations: Cell<usize>,
    gaps: Cell<usize>,
    fail: Cell<bool>,
    bad_owner: Cell<bool>,
    collect: Cell<bool>,
    in_collection: Cell<bool>,
    saved: Cell<Oop>,
    token: Cell<usize>,
}
fn failure() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 991)
}
unsafe extern "C" fn observed(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Probe>();
        p.observations.set(p.observations.get() + 1);
        p.id.send_named(r, b"originalObserve:", borrowed_slice(a, n).unwrap())
            .map_or_else(|e| e, Outcome::loaded)
    }
}
unsafe extern "C" fn no_edges(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::unit())
}
unsafe extern "C" fn references(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Probe>();
        let [value, visitor] = borrowed_slice(a, n).unwrap() else {
            panic!("owner args")
        };
        let value = object(*value);
        let visitor = object(*visitor);
        if !p.in_collection.get() {
            let nested =
                p.id.send_named(p.storage, b"observe:", &[Operand::object(p.report.get())])
                    .unwrap_err();
            assert_eq!(nested.detail, 2);
            if p.bad_owner.get() {
                if p.saved.get().is_null() {
                    p.saved.set(visitor);
                    p.token.set(retain(p.id, p.roots, visitor));
                }
                assert!(
                    p.id.send_named(
                        visitor,
                        b"visit:",
                        &[Operand::object(ptr::without_provenance_mut(8))]
                    )
                    .is_err()
                );
                return Outcome::loaded(Operand::unit()); // failure must latch
            }
            if p.collect.get() {
                p.in_collection.set(true);
                collect(p.id, p.gc);
                p.in_collection.set(false);
            }
        }
        let edges = p
            .edges
            .borrow()
            .get(&value.addr())
            .cloned()
            .unwrap_or_default();
        for edge in edges {
            send(p.id, visitor, b"visit:", &[Operand::object(edge)]);
        }
        Outcome::loaded(Operand::unit())
    }
}
unsafe fn row(c: *mut c_void, a: *const Operand, n: usize, reached: bool) -> Outcome {
    unsafe {
        let p = &*c.cast::<Probe>();
        let [value, identity] = borrowed_slice(a, n).unwrap() else {
            panic!("report args")
        };
        let value = object(*value);
        let identity = identity.as_word().unwrap();
        assert!(
            p.rows
                .borrow_mut()
                .insert(value.addr(), (identity, reached))
                .is_none()
        );
        if p.fail.get() {
            return failure();
        }
        Outcome::loaded(Operand::unit())
    }
}
unsafe extern "C" fn reached(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe { row(c, a, n, true) }
}
unsafe extern "C" fn not_reached(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe { row(c, a, n, false) }
}
unsafe extern "C" fn complete(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Probe>();
        let [gaps] = borrowed_slice(a, n).unwrap() else {
            panic!("complete args")
        };
        p.gaps.set(gaps.as_word().unwrap());
        p.completed.set(p.completed.get() + 1);
        let nested =
            p.id.send_named(
                p.trace,
                b"report:acceptingGaps:",
                &[Operand::object(p.report.get()), Operand::word(1)],
            )
            .unwrap_err();
        assert_eq!(nested.detail, 2);
        if p.collect.get() {
            p.in_collection.set(true);
            collect(p.id, p.gc);
            p.in_collection.set(false);
            for &address in p.rows.borrow().keys() {
                assert!(present(p.id, ptr::with_exposed_provenance_mut(address)));
            }
        }
        Outcome::loaded(Operand::word(73))
    }
}
unsafe fn report(p: &Probe, accept: usize) -> Result<Operand, Outcome> {
    p.rows.borrow_mut().clear();
    p.completed.set(0);
    unsafe {
        p.id.send_named(
            p.trace,
            b"report:acceptingGaps:",
            &[Operand::object(p.report.get()), Operand::word(accept)],
        )
    }
}
#[test]
#[ignore = "scripts/native-message-test.sh builds production modules"]
fn live_diagnostics_preserve_graph_coverage_lifetimes_and_failure_cleanup() {
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
        let roots = named(b"Roots");
        let gc = named(b"GC");
        send(id, gc, b"automaticCollection:", &[Operand::word(0)]);
        send(id, gc, b"precisePayloads:", &[Operand::word(1)]);
        let p = Box::new(Probe {
            id,
            roots,
            gc,
            storage: named(b"Storage"),
            trace: named(b"Trace"),
            report: Cell::new(ptr::null_mut()),
            edges: RefCell::new(HashMap::new()),
            rows: RefCell::new(HashMap::new()),
            completed: Cell::new(0),
            observations: Cell::new(0),
            gaps: Cell::new(0),
            fail: Cell::new(false),
            bad_owner: Cell::new(false),
            collect: Cell::new(false),
            in_collection: Cell::new(false),
            saved: Cell::new(ptr::null_mut()),
            token: Cell::new(0),
        });
        let c = (&*p as *const Probe).cast_mut().cast();
        let storage_behavior = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(p.storage)],
        ));
        let original = send(
            id,
            id.receiver,
            b"bindingOf:in:",
            &[
                Operand::object(id.intern_name(b"observe:").unwrap()),
                Operand::object(storage_behavior),
            ],
        );
        send(
            id,
            storage_behavior,
            b"addMethod",
            &[
                Operand::object(id.intern_name(b"originalObserve:").unwrap()),
                original,
            ],
        );
        install(id, storage_behavior, b"observe:", observed, c);

        let report_behavior = behavior(id);
        for (name, method) in [
            (b"reached:identity:".as_slice(), reached as native::Method),
            (b"notReached:identity:", not_reached),
            (b"complete:", complete),
        ] {
            install(id, report_behavior, name, method, c);
        }
        p.report.set(allocate(id, report_behavior, 0));
        retain(id, roots, p.report.get());
        let owner_behavior = behavior(id);
        install(id, owner_behavior, b"referencesOf:do:", references, c);
        let owner = allocate(id, owner_behavior, 0);
        retain(id, roots, owner);
        let pair = behavior(id);
        retain(id, roots, pair);
        send(
            id,
            gc,
            b"trace:with:",
            &[Operand::object(pair), Operand::object(owner)],
        );
        let x = allocate(id, pair, 0);
        let y = allocate(id, pair, 0);
        let u = allocate(id, pair, 0);
        let v = allocate(id, pair, 0);
        for (a, b) in [(x, y), (y, x), (u, v), (v, u)] {
            p.edges.borrow_mut().insert(a.addr(), vec![b]);
        }
        let x_root = retain(id, roots, x);
        let raw = object(send(
            id,
            named(b"Integer"),
            b"fromWord:",
            &[Operand::word(u.addr())],
        ));
        retain(id, roots, raw);
        // Strict coverage never silently accepts unobserved native stack roots.
        assert_eq!(report(&p, 0).unwrap_err().detail, 3);
        assert_eq!(p.completed.get(), 0);
        assert!(p.rows.borrow().is_empty());
        assert_eq!(report(&p, 2).unwrap_err().detail, 3);
        p.collect.set(true);
        assert_eq!(report(&p, 1).unwrap().as_word(), Some(73));
        assert_eq!(p.completed.get(), 1);
        assert_eq!(p.gaps.get(), 1);
        for (o, reached) in [(x, true), (y, true), (u, false), (v, false), (raw, true)] {
            assert_eq!(p.rows.borrow()[&o.addr()].1, reached);
        }
        let identity = p.rows.borrow()[&u.addr()].0;
        // A retained expired owner callback cannot write into any later observation.
        p.bad_owner.set(true);
        assert!(report(&p, 1).is_err());
        p.bad_owner.set(false);
        assert!(
            id.send_named(p.saved.get(), b"visit:", &[Operand::object(x)])
                .is_err()
        );
        p.fail.set(true);
        assert_eq!(report(&p, 1).unwrap_err().detail, 991);
        p.fail.set(false);
        assert_eq!(report(&p, 1).unwrap().as_word(), Some(73));
        assert_eq!(p.rows.borrow()[&u.addr()].0, identity);

        // Reference-owner selection is live, including replacement between reports.
        let empty_behavior = behavior(id);
        install(
            id,
            empty_behavior,
            b"referencesOf:do:",
            no_edges,
            ptr::null_mut(),
        );
        let empty_owner = allocate(id, empty_behavior, 0);
        retain(id, roots, empty_owner);
        send(
            id,
            gc,
            b"trace:with:",
            &[Operand::object(pair), Operand::object(empty_owner)],
        );
        report(&p, 1).unwrap();
        assert!(p.rows.borrow()[&x.addr()].1);
        assert!(!p.rows.borrow()[&y.addr()].1);
        send(
            id,
            gc,
            b"trace:with:",
            &[Operand::object(pair), Operand::object(owner)],
        );
        report(&p, 1).unwrap();
        assert!(p.rows.borrow()[&y.addr()].1);
        assert!(p.observations.get() >= 8);
        // Root changes affect declared reachability despite observation protection.
        release(id, roots, x_root);
        report(&p, 1).unwrap();
        assert!(!p.rows.borrow()[&x.addr()].1);
        assert!(!p.rows.borrow()[&y.addr()].1);
        // Enumeration and report callbacks never reclaim unrooted objects.
        assert!(present(id, u));
        assert!(present(id, v));
        p.collect.set(false);
        release(id, roots, p.token.get());
        // Ordinary collection resumes after observation roots have been released.
        p.in_collection.set(true);
        collect(id, gc);
        p.in_collection.set(false);
        for value in [x, y, u, v] {
            assert!(!present(id, value));
        }
        // Missing nonempty payload owners are an explicit additional coverage gap.
        let unknown_behavior = behavior(id);
        retain(id, roots, unknown_behavior);
        send(
            id,
            id.receiver,
            b"parentOf:put:",
            &[
                Operand::object(unknown_behavior),
                Operand::object(ptr::null_mut()),
            ],
        );
        let unknown = allocate(id, unknown_behavior, 8);
        retain(id, roots, unknown);
        assert_eq!(report(&p, 1).unwrap_err().detail, 3);
        report(&p, 3).unwrap();
        assert_eq!(p.gaps.get(), 3);
        assert!(p.rows.borrow()[&unknown.addr()].1);
        // No later callback can retain this stack context: native test process exits.
        Box::leak(p);
    }
}
