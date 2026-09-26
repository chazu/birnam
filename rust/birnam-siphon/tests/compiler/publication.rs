//! Reflect publication uses Store's current message protocol and existing hooks.
use super::*;
use birnam_abi::id_protocol as native;

struct Probe {
    id: Protocol,
    gc: Oop,
    store: Oop,
    target: Oop,
    integer: Oop,
    alias: Oop,
    decision: Cell<Oop>,
    authorizations: Cell<usize>,
    notifications: Cell<usize>,
    publications: Cell<usize>,
    expected: Cell<usize>,
    submitted: Cell<Oop>,
    authorization_failure: Cell<bool>,
    notification_failure: Cell<bool>,
}
unsafe extern "C" fn publish(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Probe>();
        assert_eq!(n, 3);
        p.submitted.set(object(borrowed_slice(a, n).unwrap()[2]));
        collect(p.id, p.gc);
        p.publications.set(p.publications.get() + 1);
        p.id.send(r, p.alias, borrowed_slice(a, n).unwrap())
            .map_or_else(|e| e, Outcome::loaded)
    }
}
unsafe extern "C" fn authorize(c: *mut c_void, _: Oop, _: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Probe>();
        assert_eq!(n, 0);
        collect(p.id, p.gc);
        p.authorizations.set(p.authorizations.get() + 1);
        if p.authorization_failure.get() {
            Outcome::failed(native::INVALID_REQUEST, 931)
        } else {
            Outcome::loaded(Operand::object(p.decision.get()))
        }
    }
}
unsafe extern "C" fn observe(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Probe>();
        let a = borrowed_slice(a, n).unwrap();
        assert_eq!(n, 2);
        collect(p.id, p.gc);
        // Notification sees both the committed binding and its owned Method mirror.
        let mirror = object(send(p.id, p.store, b"rawCurrentValue:attr:", &[a[1], a[0]]));
        assert_eq!(mirror, p.submitted.get());
        let value = p.id.send(p.target, object(a[0]), &[]).unwrap();
        assert_eq!(
            send(p.id, p.integer, b"wordOf:", &[value]).as_word(),
            Some(p.expected.get())
        );
        p.notifications.set(p.notifications.get() + 1);
        if p.notification_failure.get() {
            Outcome::failed(native::INVALID_REQUEST, 932)
        } else {
            // Store ignores the observer's language result.
            Outcome::loaded(Operand::object(p.decision.get()))
        }
    }
}
unsafe extern "C" fn allow(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::object(c))
}
unsafe extern "C" fn ignore(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::unit())
}
unsafe fn hook(
    h: &mut H,
    selector: &[u8],
    method: native::Method,
    context: *mut c_void,
) -> (Oop, Oop) {
    unsafe {
        let receiver = object(send(h.id, h.named(b"Object"), b"clone", &[]));
        h.keep(receiver);
        let vt = object(send(
            h.id,
            h.id.receiver,
            b"ensurePrivateVtableFor:",
            &[Operand::object(receiver)],
        ));
        h.keep(vt);
        install(h.id, vt, selector, method, context);
        (receiver, vt)
    }
}
pub(super) unsafe fn check(h: &mut H) {
    unsafe {
        h.clear();
        h.collect();
        assert_eq!(h.units(), 0);
        let id = h.id;
        let store = h.named(b"Store");
        let reflect = h.named(b"Reflect");
        let target = object(send(id, h.named(b"Object"), b"clone", &[]));
        h.keep(target);
        h.bind(b"PublicationTarget", target);
        let selector = id.intern_name(b"publicationAnswer").unwrap();
        h.keep(selector);
        let store_vt = h.named(b"StoreVt");
        let alias = id.intern_name(b"savedCompilerAddMethod:on:with:").unwrap();
        h.keep(alias);
        let add = id.intern_name(b"addMethod:on:with:").unwrap();
        let original = send(
            id,
            id.receiver,
            b"bindingOf:in:",
            &[Operand::object(add), Operand::object(store_vt)],
        )
        .as_entry()
        .unwrap();
        send(
            id,
            store_vt,
            b"addMethod",
            &[Operand::object(alias), Operand::entry(original)],
        );
        let mut p = Probe {
            id,
            gc: h.named(b"GC"),
            store,
            target,
            integer: h.named(b"Integer"),
            alias,
            decision: Cell::new(h.named(b"true")),
            authorizations: Cell::new(0),
            notifications: Cell::new(0),
            publications: Cell::new(0),
            expected: Cell::new(41),
            submitted: Cell::new(ptr::null_mut()),
            authorization_failure: Cell::new(false),
            notification_failure: Cell::new(false),
        };
        let context = ptr::from_mut(&mut p).cast();
        let (authorizer, authorizer_vt) = hook(h, b"value", authorize, context);
        let (observer, observer_vt) = hook(h, b"managedMethodChanged:on:", observe, context);
        send(
            id,
            store,
            b"setMethodMutationAuthorizer:",
            &[Operand::object(authorizer)],
        );
        send(
            id,
            store,
            b"setMethodMutationObserver:",
            &[Operand::object(observer)],
        );
        install(id, store_vt, b"addMethod:on:with:", publish, context);
        assert_eq!(h.eval(b"[Reflect defineMethod: #publicationAnswer on: PublicationTarget as: [:self | 41]]"), target);
        assert_eq!(p.publications.get(), 1);
        assert_eq!(p.authorizations.get(), 1);
        assert_eq!(p.notifications.get(), 1);
        let vt = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(target)],
        ));
        h.keep(vt);
        let old = object(send(
            id,
            target,
            b"methodFor:",
            &[Operand::object(selector)],
        ));
        h.keep(old);
        let old_mirror = object(send(
            id,
            store,
            b"rawCurrentValue:attr:",
            &[Operand::object(vt), Operand::object(selector)],
        ));
        h.keep(old_mirror);
        let old_binding = send(
            id,
            id.receiver,
            b"bindingOf:in:",
            &[Operand::object(selector), Operand::object(vt)],
        )
        .as_entry()
        .unwrap();
        let implementation = h.eval(b"[:self | 42]");
        let args = [
            Operand::object(selector),
            Operand::object(target),
            Operand::object(implementation),
        ];
        h.collect();
        let before = h.units();
        let text = h.string(b"publication refused");
        let error = object(send(
            id,
            h.named(b"Error"),
            b"kind:message:",
            &[
                Operand::object(id.intern_name(b"publicationRefused").unwrap()),
                Operand::object(text),
            ],
        ));
        h.keep(error);
        for denied in [h.named(b"false"), h.named(b"nil"), error] {
            p.decision.set(denied);
            assert_eq!(
                object(send(id, reflect, b"defineMethod:on:as:", &args)),
                denied
            );
            assert_eq!(
                send(
                    id,
                    id.receiver,
                    b"bindingOf:in:",
                    &[Operand::object(selector), Operand::object(vt)]
                )
                .as_entry()
                .unwrap(),
                old_binding
            );
            assert_eq!(
                object(send(
                    id,
                    store,
                    b"rawCurrentValue:attr:",
                    &[Operand::object(vt), Operand::object(selector)]
                )),
                old_mirror
            );
            assert_eq!(p.notifications.get(), 1);
            h.collect();
            assert_eq!(h.units(), before, "refused named adapter is reclaimed");
        }
        p.authorization_failure.set(true);
        let failure = id
            .send_named(reflect, b"defineMethod:on:as:", &args)
            .unwrap_err();
        assert_eq!(
            (failure.status, failure.detail),
            (native::INVALID_REQUEST, 931)
        );
        assert_eq!(
            send(
                id,
                id.receiver,
                b"bindingOf:in:",
                &[Operand::object(selector), Operand::object(vt)]
            )
            .as_entry()
            .unwrap(),
            old_binding
        );
        assert_eq!(p.notifications.get(), 1);
        h.collect();
        assert_eq!(h.units(), before);
        p.authorization_failure.set(false);
        p.decision.set(h.named(b"true"));
        p.expected.set(42);
        p.notification_failure.set(true);
        let failure = id
            .send_named(reflect, b"defineMethod:on:as:", &args)
            .unwrap_err();
        assert_eq!(
            (failure.status, failure.detail),
            (native::INVALID_REQUEST, 932)
        );
        assert_eq!(p.notifications.get(), 2);
        assert_eq!(
            h.integer(object(send(id, target, b"publicationAnswer", &[]))),
            42
        );
        assert_ne!(
            object(send(
                id,
                store,
                b"rawCurrentValue:attr:",
                &[Operand::object(vt), Operand::object(selector)]
            )),
            old_mirror
        );
        assert_eq!(
            h.integer(object(send(
                id,
                old,
                b"invokeOn:",
                &[Operand::object(target)]
            ))),
            41
        );
        // Replacing hook methods changes the next send; no cached native callbacks.
        install(id, authorizer_vt, b"value", allow, h.named(b"true"));
        install(
            id,
            observer_vt,
            b"managedMethodChanged:on:",
            ignore,
            ptr::null_mut(),
        );
        assert_eq!(
            object(send(id, reflect, b"defineMethod:on:as:", &args)),
            target
        );
        assert_eq!(p.publications.get(), 7);
        assert_eq!(p.authorizations.get(), 6);
        assert_eq!(p.notifications.get(), 2);
        send(
            id,
            store_vt,
            b"addMethod",
            &[Operand::object(add), Operand::entry(original)],
        );
        send(id, store_vt, b"removeMethod:", &[Operand::object(alias)]);
        // Existing Store removal retires the mirror and installed adapter together.
        send(
            id,
            store,
            b"removeMethod:on:",
            &[Operand::object(selector), Operand::object(vt)],
        );
        assert_eq!(
            send(
                id,
                store,
                b"rawCurrentValue:attr:",
                &[Operand::object(vt), Operand::object(selector)]
            )
            .kind(),
            native::UNIT
        );
        h.bind(b"PublicationTarget", h.named(b"nil"));
        h.clear();
        h.collect();
        assert_eq!(h.units(), 0);
        assert_eq!(h.native_bytes(), 0);
    }
}
