//! Characterize the existing publication protocol before moving its owner.
mod support;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol, UNIT},
};
use std::{
    cell::{Cell, RefCell},
    ffi::c_void,
    os::unix::ffi::OsStrExt,
};
use support::*;
struct H {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    tokens: RefCell<Vec<usize>>,
}
impl H {
    unsafe fn named(&self, n: &[u8]) -> Oop {
        unsafe {
            object(send(
                self.id,
                self.ns,
                b"at:",
                &[Operand::object(self.id.intern_name(n).unwrap())],
            ))
        }
    }
    unsafe fn keep(&self, o: Oop) -> Oop {
        self.tokens
            .borrow_mut()
            .push(unsafe { retain(self.id, self.roots, o) });
        o
    }
    unsafe fn key(&self, n: &[u8]) -> Oop {
        unsafe { self.keep(self.id.intern_name(n).unwrap()) }
    }
    unsafe fn call(&self, r: Oop, n: &[u8], args: &[Oop]) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                r,
                n,
                &args
                    .iter()
                    .copied()
                    .map(Operand::object)
                    .collect::<Vec<_>>(),
            )))
        }
    }
    unsafe fn entry(&self, r: Oop, n: &[u8]) -> NativeEntry {
        unsafe {
            send(
                self.id,
                self.id.receiver,
                b"bind:to:",
                &[Operand::object(self.key(n)), Operand::object(r)],
            )
            .as_entry()
            .unwrap()
        }
    }
}
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Normal,
    Deny,
    AuthFault,
    RawFault,
    ManagedFault,
    ErrorValue,
}
struct Probe<'a> {
    h: &'a H,
    store: Oop,
    target: Oop,
    selector: Oop,
    foreign: Oop,
    error: Oop,
    raw: NativeEntry,
    mode: Cell<Mode>,
    record: Cell<bool>,
    events: RefCell<Vec<&'static str>>,
}
unsafe extern "C" fn answer(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::object(c.cast()))
}
unsafe extern "C" fn authorize(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Probe<'_>>();
        if p.record.get() {
            p.events.borrow_mut().push("authorize");
        }
        match if p.record.get() {
            p.mode.get()
        } else {
            Mode::Normal
        } {
            Mode::Deny => Outcome::loaded(Operand::object(p.h.named(b"false"))),
            Mode::AuthFault => Outcome::failed(881, 1),
            _ => Outcome::loaded(Operand::object(p.h.named(b"true"))),
        }
    }
}
unsafe extern "C" fn raw_changed(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Probe<'_>>();
        let result = p.raw.method.unwrap()(p.raw.context, r, a, n);
        let args = borrowed_slice(a, n).unwrap();
        if p.record.get()
            && args[0].as_object() == Some(p.selector)
            && args[1].as_object() == Some(p.target)
        {
            p.events.borrow_mut().push("raw");
            // Id has already changed the dictionary; invalidation leaves no stale fact.
            assert_eq!(
                send(
                    p.h.id,
                    p.store,
                    b"rawCurrentValue:attr:",
                    &[args[1], args[0]]
                )
                .kind(),
                UNIT
            );
            if p.mode.get() == Mode::RawFault {
                return Outcome::failed(881, 2);
            }
        }
        result
    }
}
unsafe extern "C" fn managed(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Probe<'_>>();
        let args = borrowed_slice(a, n).unwrap();
        if !p.record.get() {
            return Outcome::loaded(Operand::unit());
        }
        p.events.borrow_mut().push("managed");
        collect(p.h.id, p.h.named(b"GC"));
        let direct = send(
            p.h.id,
            p.h.id.receiver,
            b"bindingOf:in:",
            &[args[0], args[1]],
        );
        let fact = send(
            p.h.id,
            p.store,
            b"rawCurrentValue:attr:",
            &[args[1], args[0]],
        );
        assert_eq!(
            direct.kind() == UNIT,
            fact.kind() == UNIT,
            "managed notification follows mirror publication"
        );
        if p.mode.get() == Mode::ManagedFault {
            send(
                p.h.id,
                p.store,
                b"assert:attr:value:",
                &[
                    Operand::object(p.foreign),
                    Operand::object(p.selector),
                    Operand::object(p.h.named(b"true")),
                ],
            );
            return Outcome::failed(881, 3);
        }
        Outcome::loaded(if p.mode.get() == Mode::ErrorValue {
            Operand::object(p.error)
        } else {
            Operand::unit()
        })
    }
}
#[test]
#[ignore = "production native modules"]
fn authorization_commit_notifications_and_failure_are_distinct() {
    unsafe {
        let id = id();
        let ns = load(
            id,
            "bootstrap",
            &[Operand::bytes(directory().as_os_str().as_bytes())],
        );
        let roots = object(send(
            id,
            ns,
            b"at:",
            &[Operand::object(id.intern_name(b"Roots").unwrap())],
        ));
        let h = H {
            id,
            ns,
            roots,
            tokens: RefCell::new(vec![]),
        };
        let store = h.named(b"Store");
        assert_eq!(send(id, id.receiver, b"methodChangesTo:", &[Operand::object(store)]).kind(), UNIT);
        let target = h.call(h.named(b"Object"), b"clone", &[]);
        let vt = h.call(id.receiver, b"ensurePrivateVtableFor:", &[target]);
        let selector = h.key(b"publicationAudit");
        let old_value = h.key(b"oldPublication");
        let new_value = h.key(b"newPublication");
        let make = |value| {
            h.keep(object(send(
                id,
                h.named(b"Method"),
                b"fromNative:owner:",
                &[
                    Operand::entry(NativeEntry::new(answer, value)),
                    Operand::object(value),
                ],
            )))
        };
        let old = make(old_value);
        let new = make(new_value);
        let mut p = Probe {
            h: &h,
            store,
            target: vt,
            selector,
            foreign: h.key(b"publicationForeign"),
            error: h.call(h.named(b"Error"), b"kind:", &[h.key(b"observerErrorValue")]),
            raw: h.entry(store, b"methodChanged:on:"),
            mode: Cell::new(Mode::Normal),
            record: Cell::new(false),
            events: RefCell::new(vec![]),
        };
        let c = (&raw mut p).cast();
        let ab = h.keep(behavior(id));
        let ob = h.keep(behavior(id));
        install(id, ab, b"value", authorize, c);
        install(id, ob, b"managedMethodChanged:on:", managed, c);
        let a = h.keep(allocate(id, ab, 0));
        let o = h.keep(allocate(id, ob, 0));
        h.call(store, b"setMethodMutationAuthorizer:", &[a]);
        h.call(store, b"setMethodMutationObserver:", &[o]);
        let sb = h.call(id.receiver, b"behaviorOf:", &[store]);
        install(id, sb, b"methodChanged:on:", raw_changed, c);
        for mode in [
            Mode::Normal,
            Mode::Deny,
            Mode::AuthFault,
            Mode::RawFault,
            Mode::ManagedFault,
            Mode::ErrorValue,
        ] {
            p.record.set(false);
            assert_eq!(
                h.call(store, b"addMethod:on:with:", &[selector, vt, old]),
                store
            );
            p.events.borrow_mut().clear();
            p.mode.set(mode);
            p.record.set(true);
            let result = id.send_named(
                store,
                b"addMethod:on:with:",
                &[
                    Operand::object(selector),
                    Operand::object(vt),
                    Operand::object(new),
                ],
            );
            let committed = !matches!(mode, Mode::Deny | Mode::AuthFault);
            assert_eq!(
                h.call(target, b"publicationAudit", &[]),
                if committed { new_value } else { old_value }
            );
            let expected_events = match mode {
                Mode::Deny | Mode::AuthFault => vec!["authorize"],
                Mode::RawFault => vec!["authorize", "raw"],
                _ => vec!["authorize", "raw", "managed"],
            };
            assert_eq!(*p.events.borrow(), expected_events);
            match mode {
                Mode::AuthFault => assert_eq!(result.unwrap_err().detail, 1),
                Mode::RawFault => assert_eq!(result.unwrap_err().detail, 2),
                Mode::ManagedFault => {
                    assert_eq!(result.unwrap_err().detail, 3);
                    assert_eq!(
                        h.call(store, b"currentValue:attr:", &[p.foreign, selector]),
                        h.named(b"true")
                    );
                }
                Mode::Deny => assert_eq!(object(result.unwrap()), h.named(b"false")),
                _ => assert_eq!(object(result.unwrap()), store), // observer Error values are void results, not vetoes
            }
            let fact = send(
                id,
                store,
                b"rawCurrentValue:attr:",
                &[Operand::object(vt), Operand::object(selector)],
            );
            if mode == Mode::RawFault {
                assert_eq!(fact.kind(), UNIT)
            } else {
                assert_eq!(fact.as_object(), Some(if committed { new } else { old }));
            }
        }
        // A failed post-remove notification does not resurrect the removed method.
        p.mode.set(Mode::ManagedFault);
        p.events.borrow_mut().clear();
        let failure = id
            .send_named(
                store,
                b"removeMethod:on:",
                &[Operand::object(selector), Operand::object(vt)],
            )
            .unwrap_err();
        assert_eq!(failure.detail, 3);
        assert_eq!(
            send(
                id,
                id.receiver,
                b"bindingOf:in:",
                &[Operand::object(selector), Operand::object(vt)]
            )
            .kind(),
            UNIT
        );
        p.mode.set(Mode::Normal);
        p.events.borrow_mut().clear();
        let missing = h.call(store, b"removeMethod:on:", &[selector, vt]);
        assert_eq!(h.call(missing, b"kind", &[]), h.key(b"missingDirectMethod"));
        assert_eq!(*p.events.borrow(), vec!["authorize"]);
        p.record.set(false);
        install(
            id,
            sb,
            b"methodChanged:on:",
            p.raw.method.unwrap(),
            p.raw.context,
        );
    }
}
