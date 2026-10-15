//! Method observation lives in Id messages; Store consumes the notification.
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
use std::os::unix::ffi::OsStrExt;
use support::*;
struct Probe {
    id: Protocol,
    gc: Oop,
    target: Oop,
    key: Oop,
    nested: Oop,
    replacement: Cell<Oop>,
    events: RefCell<Vec<(usize, usize, bool)>>,
    fail: Cell<bool>,
    collect: Cell<bool>,
    clear_self: Cell<bool>,
    self_survived: Cell<bool>,
}
unsafe extern "C" fn answer(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::word(42))
}
unsafe extern "C" fn changed(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Probe>();
        let [key, target] = borrowed_slice(a, n).unwrap() else {
            panic!("observation args")
        };
        let key = object(*key);
        let target = object(*target);
        let present = send(
            p.id,
            p.id.receiver,
            b"bindingOf:in:",
            &[Operand::object(key), Operand::object(target)],
        )
        .as_entry()
        .is_some();
        p.events.borrow_mut().push((r.addr(), key.addr(), present));
        let next = p.replacement.replace(ptr::null_mut());
        if !next.is_null() {
            send(
                p.id,
                p.id.receiver,
                b"methodChangesTo:",
                &[Operand::object(next)],
            );
            install(p.id, p.target, b"nestedObserved", answer, ptr::null_mut());
        }
        if p.clear_self.replace(false) {
            send(p.id, p.id.receiver, b"methodChangesTo:", &[Operand::unit()]);
            collect(p.id, p.gc);
            p.self_survived.set(support::present(p.id, r));
        }
        if p.collect.get() {
            collect(p.id, p.gc);
            assert!(support::present(p.id, r));
        }
        if p.fail.get() {
            return Outcome::failed(native::INVALID_REQUEST, 992);
        }
        Outcome::loaded(Operand::unit())
    }
}
#[test]
#[ignore = "scripts/native-message-test.sh builds production modules"]
fn observation_uses_live_messages_and_survives_replacement_reentry_and_gc() {
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
        let target = behavior(id);
        retain(id, roots, target);
        let key = id.intern_name(b"observed").unwrap();
        retain(id, roots, key);
        let nested = id.intern_name(b"nestedObserved").unwrap();
        retain(id, roots, nested);
        let p = Box::new(Probe {
            id,
            gc,
            target,
            key,
            nested,
            replacement: Cell::new(ptr::null_mut()),
            events: RefCell::new(vec![]),
            fail: Cell::new(false),
            collect: Cell::new(false),
            clear_self: Cell::new(false),
            self_survived: Cell::new(false),
        });
        let c = (&*p as *const Probe).cast_mut().cast();
        let observer_behavior = behavior(id);
        retain(id, roots, observer_behavior);
        install(id, observer_behavior, b"methodChanged:on:", changed, c);
        let first = allocate(id, observer_behavior, 0);
        let first_root = retain(id, roots, first);
        let second = allocate(id, observer_behavior, 0);
        let second_root = retain(id, roots, second);
        let prior = send(
            id,
            id.receiver,
            b"methodChangesTo:",
            &[Operand::object(first)],
        );
        assert_eq!(prior.kind(), native::UNIT, "bootstrap must not install a mirror");
        p.collect.set(true);
        // Notifications see the committed dictionary, including additions.
        install(id, target, b"observed", answer, ptr::null_mut());
        install(id, target, b"observed", answer, ptr::null_mut());
        send(id, target, b"removeMethod:", &[Operand::object(key)]);
        send(id, target, b"removeMethod:", &[Operand::object(key)]);
        assert_eq!(
            &*p.events.borrow(),
            &[
                (first.addr(), key.addr(), true),
                (first.addr(), key.addr(), true),
                (first.addr(), key.addr(), false)
            ]
        );
        p.events.borrow_mut().clear();
        p.replacement.set(second);
        install(id, target, b"observed", answer, ptr::null_mut());
        assert_eq!(
            &*p.events.borrow(),
            &[
                (first.addr(), key.addr(), true),
                (second.addr(), nested.addr(), true)
            ]
        );
        // Future notifications follow the replacement; an entered observer can return.
        release(id, roots, first_root);
        release(id, roots, second_root);
        collect(id, gc);
        assert!(!present(id, first));
        assert!(present(id, second));
        p.events.borrow_mut().clear();
        p.fail.set(true);
        let error = id
            .send_named(target, b"removeMethod:", &[Operand::object(key)])
            .unwrap_err();
        assert_eq!(error.detail, 992);
        assert_eq!(
            send(
                id,
                id.receiver,
                b"bindingOf:in:",
                &[Operand::object(key), Operand::object(target)]
            )
            .kind(),
            native::UNIT
        );
        p.fail.set(false);
        install(id, target, b"observed", answer, ptr::null_mut());
        // The notification slot may be the receiver's last external root.
        // Clearing it during an entered native callback must not retire that
        // callback's receiver before it returns.
        let second_hold = retain(id, roots, second);
        let entered = allocate(id, observer_behavior, 0);
        let entered_root = retain(id, roots, entered);
        send(
            id,
            id.receiver,
            b"methodChangesTo:",
            &[Operand::object(entered)],
        );
        release(id, roots, entered_root);
        p.collect.set(false);
        p.clear_self.set(true);
        install(id, target, b"observed", answer, ptr::null_mut());
        assert!(
            p.self_survived.get(),
            "entered observer was collected before return"
        );
        collect(id, gc);
        assert!(!present(id, entered));
        let failed_observer = allocate(id, observer_behavior, 0);
        let failed_root = retain(id, roots, failed_observer);
        send(
            id,
            id.receiver,
            b"methodChangesTo:",
            &[Operand::object(failed_observer)],
        );
        release(id, roots, failed_root);
        p.clear_self.set(true);
        p.self_survived.set(false);
        p.fail.set(true);
        let failure = id
            .send_named(target, b"removeMethod:", &[Operand::object(key)])
            .unwrap_err();
        assert_eq!(failure.detail, 992);
        assert!(
            p.self_survived.get(),
            "failing entered observer was collected"
        );
        p.fail.set(false);
        collect(id, gc);
        assert!(
            !present(id, failed_observer),
            "failed notification leaked a temporary root"
        );
        send(
            id,
            id.receiver,
            b"methodChangesTo:",
            &[Operand::object(second)],
        );
        release(id, roots, second_hold);
        // Clearing stops notifications; restore the original empty slot.
        assert_eq!(
            send(id, id.receiver, b"methodChangesTo:", &[Operand::unit()]).as_object(),
            Some(second)
        );
        let count = p.events.borrow().len();
        install(id, target, b"observed", answer, ptr::null_mut());
        assert_eq!(p.events.borrow().len(), count);
        send(id, id.receiver, b"methodChangesTo:", &[prior]);
        collect(id, gc);
        assert!(!present(id, second));
        assert_eq!(p.key, key);
        assert_eq!(p.nested, nested);
        Box::leak(p);
    }
}
