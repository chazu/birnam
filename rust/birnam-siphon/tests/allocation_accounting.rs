//! Independent production witnesses for GC charges, geometry and collection thresholds.
mod support;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
};
use core::{cell::Cell, ffi::c_void, ptr};
use std::os::unix::{ffi::OsStrExt, process::ExitStatusExt};
use support::*;

unsafe fn bytes(id: Protocol, gc: Oop) -> usize {
    unsafe { send(id, gc, b"allocationBytes", &[]).as_word().unwrap() }
}
struct Observer {
    id: Protocol,
    gc: Oop,
    fail: Cell<bool>,
    visits: Cell<usize>,
    charge: Cell<usize>,
}
unsafe extern "C" fn references(c: *mut c_void, _: Oop, _: *const Operand, n: usize) -> Outcome {
    unsafe {
        assert_eq!(n, 2);
        let o = &*c.cast::<Observer>();
        o.visits.set(o.visits.get() + 1);
        o.charge.set(bytes(o.id, o.gc));
        if o.fail.get() {
            Outcome::failed(native::INVALID_REQUEST, 83)
        } else {
            Outcome::loaded(Operand::unit())
        }
    }
}
struct Geometry {
    id: Protocol,
    gc: Oop,
    behavior: Oop,
    original: NativeEntry,
    calls: Cell<usize>,
    collecting: Cell<bool>,
    nesting: Cell<bool>,
    huge: bool,
}
unsafe extern "C" fn geometry(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let g = &*c.cast::<Geometry>();
        assert_eq!(borrowed_slice(a, n).unwrap().len(), 1);
        g.calls.set(g.calls.get() + 1);
        if g.collecting.replace(false) {
            collect(g.id, g.gc);
        }
        if g.nesting.replace(false) {
            allocate(g.id, g.behavior, 0);
        }
        if g.huge {
            Outcome::loaded(Operand::word(usize::MAX))
        } else {
            g.original.method.unwrap()(g.original.context, r, a, n)
        }
    }
}

#[test]
#[ignore = "requires native prefix"]
fn production_allocation_accounting() {
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
        let behavior = behavior(id);
        let pin = retain(id, roots, behavior);
        let owner_behavior = support::behavior(id);
        let owner = allocate(id, owner_behavior, 0);
        let owner_pin = retain(id, roots, owner);
        let mut observer = Box::new(Observer {
            id,
            gc,
            fail: Cell::new(false),
            visits: Cell::new(0),
            charge: Cell::new(0),
        });
        install(
            id,
            owner_behavior,
            b"referencesOf:do:",
            references,
            ptr::from_mut(&mut *observer).cast(),
        );
        send(
            id,
            gc,
            b"trace:with:",
            &[Operand::object(behavior), Operand::object(owner)],
        );
        // Intern every test selector before starting the charge witness.
        for name in [
            b"allocationBytes".as_slice(),
            b"allocationChargeFor:",
            b"detachAllocationAt:",
        ] {
            id.intern_name(name).unwrap();
        }
        send(id, gc, b"budget:", &[Operand::word(usize::MAX)]);
        collect(id, gc);
        assert_eq!(bytes(id, gc), 0);

        let selector = id.intern_name(b"allocationChargeFor:").unwrap();
        let original = send(
            id,
            id.receiver,
            b"bind:to:",
            &[Operand::object(selector), Operand::object(id.receiver)],
        )
        .as_entry()
        .unwrap();
        let mut replacement = Box::new(Geometry {
            id,
            gc,
            behavior,
            original,
            calls: Cell::new(0),
            collecting: Cell::new(false),
            nesting: Cell::new(false),
            huge: false,
        });
        if std::env::var_os("BIRNAM_ACCOUNTING_OVERFLOW_CHILD").is_some() {
            replacement.huge = true;
            replacement.nesting.set(true);
            install(
                id,
                id.receiver,
                b"allocationChargeFor:",
                geometry,
                ptr::from_mut(&mut *replacement).cast(),
            );
            // Nested construction suppresses automatic collection. The first
            // charge fills the scalar; the second must abort, never saturate.
            allocate(id, behavior, 0);
            panic!("committed charge overflow returned");
        }

        // Literal expected sizes are independent of Id's implementation helper.
        let mut expected = 0;
        let mut objects = Vec::new();
        for (payload, charge) in [
            (0, 8),
            (1, 16),
            (7, 16),
            (8, 16),
            (9, 24),
            (15, 24),
            (16, 24),
        ] {
            assert_eq!(
                send(
                    id,
                    id.receiver,
                    b"allocationChargeFor:",
                    &[Operand::word(payload)]
                )
                .as_word(),
                Some(charge)
            );
            let value = allocate(id, behavior, payload);
            let token = retain(id, roots, value);
            objects.push((value, token));
            expected += charge;
            assert_eq!(bytes(id, gc), expected);
        }
        assert_eq!(expected, 128);
        assert_eq!(
            send(
                id,
                id.receiver,
                b"allocationChargeFor:",
                &[Operand::word(usize::MAX)]
            )
            .kind(),
            native::UNIT
        );
        assert_eq!(
            send(
                id,
                id.receiver,
                b"allocationChargeFor:",
                &[Operand::word(isize::MAX as usize)]
            )
            .kind(),
            native::UNIT
        );
        assert!(
            id.send_named(id.receiver, b"allocationChargeFor:", &[Operand::unit()])
                .is_err()
        );
        assert!(
            id.send_named(gc, b"allocationBytes", &[Operand::word(0)])
                .is_err()
        );
        assert_eq!(bytes(id, gc), expected);

        // Freeing physical storage does not refund accumulated allocation work.
        let (value, token) = objects.pop().unwrap();
        release(id, roots, token);
        let release_entry = send(
            id,
            id.receiver,
            b"detachAllocationAt:",
            &[Operand::word(value.addr())],
        )
        .as_entry()
        .unwrap();
        release_entry.method.unwrap()(release_entry.context, value, ptr::null(), 0)
            .into_result()
            .unwrap();
        assert_eq!(bytes(id, gc), expected);

        observer.fail.set(true);
        let count = send(id, id.receiver, b"allocationCount", &[])
            .as_word()
            .unwrap();
        assert_eq!(
            id.send_named(gc, b"collectPrecisely", &[])
                .unwrap_err()
                .detail,
            83
        );
        assert_eq!(
            bytes(id, gc),
            expected,
            "failed tracing does not reset accounting"
        );
        assert_eq!(
            send(id, id.receiver, b"allocationCount", &[]).as_word(),
            Some(count)
        );
        observer.fail.set(false);
        collect(id, gc);
        assert_eq!(
            observer.charge.get(),
            expected,
            "tracing sees the pre-reset charge"
        );
        assert_eq!(bytes(id, gc), 0);

        // Setting a threshold does not collect. The next existing preparation
        // point does, and resets the charge only after successful collection.
        allocate(id, behavior, 0);
        send(id, gc, b"budget:", &[Operand::word(8)]);
        assert_eq!(bytes(id, gc), 8);
        send(id, gc, b"prepareAllocation", &[]);
        assert_eq!(bytes(id, gc), 0);
        send(id, gc, b"budget:", &[Operand::word(usize::MAX)]);

        install(
            id,
            id.receiver,
            b"allocationChargeFor:",
            geometry,
            ptr::from_mut(&mut *replacement).cast(),
        );
        replacement.collecting.set(true);
        let value = allocate(id, behavior, 0);
        assert!(
            present(id, value),
            "construction roots survive a collecting geometry query"
        );
        assert_eq!(bytes(id, gc), 8);
        assert_eq!(replacement.calls.get(), 1);
        replacement.nesting.set(true);
        allocate(id, behavior, 0);
        assert_eq!(
            bytes(id, gc),
            24,
            "outer charge adds to the post-callback total"
        );
        assert_eq!(replacement.calls.get(), 3);
        install(
            id,
            id.receiver,
            b"allocationChargeFor:",
            original.method.unwrap(),
            original.context,
        );
        allocate(id, behavior, 0);
        assert_eq!(
            replacement.calls.get(),
            3,
            "replacement is not cached by GC"
        );
        assert_eq!(bytes(id, gc), 32);

        for (_, token) in objects {
            release(id, roots, token);
        }
        // Payloads have no edges, but their tracing owner must remain installed
        // until collection has retired the final instances.
        collect(id, gc);
        send(
            id,
            gc,
            b"trace:with:",
            &[Operand::object(behavior), Operand::unit()],
        );
        release(id, roots, owner_pin);
        release(id, roots, pin);
        send(id, gc, b"budget:", &[Operand::word(1 << 20)]);

        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "production_allocation_accounting",
                "--include-ignored",
                "--nocapture",
            ])
            .env("BIRNAM_ACCOUNTING_OVERFLOW_CHILD", "1")
            .output()
            .unwrap();
        assert_eq!(output.status.signal(), Some(6));
        assert_eq!(
            String::from_utf8(output.stderr).unwrap(),
            "birnam-id invariant failed: GC allocation byte counter overflow\n"
        );
    }
}
