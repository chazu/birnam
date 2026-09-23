//! Owned callable behavior over the actual Id/Bootstrap/GC production owners.
//! These tests intentionally do not claim Machine or compiled-unit acceptance.
mod support;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
};
use core::{
    cell::{Cell, RefCell},
    ffi::c_void,
    ptr,
};
use std::os::unix::ffi::OsStrExt;
use support::*;

struct Harness {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    gc: Oop,
}
impl Harness {
    unsafe fn named(&self, name: &[u8]) -> Oop {
        unsafe {
            object(send(
                self.id,
                self.ns,
                b"at:",
                &[Operand::object(self.id.intern_name(name).unwrap())],
            ))
        }
    }
    unsafe fn owned(&self, entry: NativeEntry, owner: Oop) -> Oop {
        unsafe {
            object(send(
                self.id,
                self.named(b"Method"),
                b"fromNative:owner:",
                &[Operand::entry(entry), Operand::object(owner)],
            ))
        }
    }
    unsafe fn bind(&self, behavior: Oop, selector: Oop, method: Oop) {
        unsafe {
            send(
                self.id,
                behavior,
                b"addMethod:as:",
                &[Operand::object(method), Operand::object(selector)],
            );
        }
    }
    unsafe fn remove(&self, behavior: Oop, selector: Oop) {
        unsafe {
            send(
                self.id,
                behavior,
                b"removeMethod:",
                &[Operand::object(selector)],
            );
        }
    }
    unsafe fn keep(&self, value: Oop) -> usize {
        unsafe { retain(self.id, self.roots, value) }
    }
    unsafe fn release(&self, token: usize) {
        unsafe {
            release(self.id, self.roots, token);
        }
    }
    unsafe fn gc(&self) {
        unsafe {
            collect(self.id, self.gc);
        }
    }
}
struct Context {
    h: *const Harness,
    owner: Oop,
    behavior: Oop,
    selector: Oop,
    replace: Cell<bool>,
    calls: Cell<usize>,
    finalized: Cell<usize>,
}
unsafe extern "C" fn finalize(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let c = &*c.cast::<Context>();
        let [value] = borrowed_slice(a, n).unwrap() else {
            panic!("finalizer shape");
        };
        assert_eq!(value.as_object(), Some(c.owner));
        c.finalized.set(c.finalized.get() + 1);
        Outcome::loaded(Operand::unit())
    }
}
unsafe extern "C" fn receiver(_: *mut c_void, r: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::object(r))
}
unsafe extern "C" fn native_result(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    match unsafe { &*c.cast::<Cell<usize>>() }.get() {
        0 => Outcome::loaded(Operand::unit()),
        1 => Outcome::loaded(Operand::word(123)),
        2 => Outcome::failed(native::INVALID_REQUEST, 17),
        _ => Outcome::failed(native::MAPPING_FAILED, 18),
    }
}
unsafe extern "C" fn answer(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let c = &*c.cast::<Context>();
        let h = &*c.h;
        if c.replace.replace(false) {
            // Native replacement bypasses addMethod:as:. The active invocation
            // must retain the old callable independently of its former binding.
            send(
                h.id,
                c.behavior,
                b"addMethod",
                &[
                    Operand::object(c.selector),
                    Operand::entry(NativeEntry::new(receiver, ptr::null_mut())),
                ],
            );
        }
        h.gc();
        assert!(present(h.id, c.owner));
        assert_eq!(c.finalized.get(), 0);
        c.calls.set(c.calls.get() + 1);
        let args = borrowed_slice(a, n).unwrap();
        for arg in args {
            assert!(present(h.id, object(*arg)));
        }
        Outcome::loaded(args.last().copied().unwrap_or(Operand::object(c.owner)))
    }
}
struct Sequence {
    h: *const Harness,
    returned: RefCell<Vec<Oop>>,
    fail: Cell<bool>,
}
unsafe extern "C" fn size(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    unsafe {
        let s = &*c.cast::<Sequence>();
        let h = &*s.h;
        Outcome::loaded(send(
            h.id,
            h.named(b"Integer"),
            b"fromWord:",
            &[Operand::word(3)],
        ))
    }
}
unsafe extern "C" fn at(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let s = &*c.cast::<Sequence>();
        let h = &*s.h;
        h.gc();
        for &v in s.returned.borrow().iter() {
            assert!(present(h.id, v), "prior argument lost across at:");
        }
        let [index] = borrowed_slice(a, n).unwrap() else {
            panic!("at shape");
        };
        let index = send(h.id, object(*index), b"word", &[]).as_word().unwrap();
        if s.fail.get() && index == 2 {
            return Outcome::failed(native::INVALID_REQUEST, 91);
        }
        let value = object(send(
            h.id,
            h.named(b"Integer"),
            b"fromWord:",
            &[Operand::word(100 + index)],
        ));
        s.returned.borrow_mut().push(value);
        Outcome::loaded(Operand::object(value))
    }
}
struct Lookup {
    h: *const Harness,
    selected: Oop,
    nested: Oop,
    selector: Oop,
    mode: Cell<usize>,
}
unsafe extern "C" fn choose(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    unsafe {
        let c = &*c.cast::<Lookup>();
        let h = &*c.h;
        if c.mode.get() == 1 {
            return Outcome::loaded(Operand::object(h.named(b"nil")));
        }
        if c.mode.get() == 2 {
            return Outcome::loaded(Operand::object(h.named(b"true")));
        }
        // The author explicitly owns and reports context references. The fixture
        // keeps selected/nested rooted while its lookup is installed.
        let selected = c.selected;
        let token = h.keep(selected);
        let nested = send(
            h.id,
            c.nested,
            b"methodFor:",
            &[Operand::object(c.selector)],
        );
        let nested_token = h.keep(object(nested));
        h.gc();
        h.release(nested_token);
        h.release(token);
        Outcome::loaded(Operand::object(selected))
    }
}

#[test]
#[ignore = "scripts/native-message-test.sh builds production dylibs"]
fn methods_retain_bindings_active_calls_arguments_and_exact_lookup_contexts() {
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
        let h = Box::leak(Box::new(Harness {
            id,
            ns,
            roots: named(b"Roots"),
            gc: named(b"GC"),
        }));
        assert_eq!(
            load(id, "execution", &[Operand::object(ns)]),
            h.named(b"Execution")
        );
        let behavior = behavior(id);
        let behavior_root = h.keep(behavior);
        let instance = allocate(id, behavior, 0);
        let instance_root = h.keep(instance);
        let selector = id.intern_name(b"ownedProbe").unwrap();
        let selector_root = h.keep(selector);
        let owner = allocate(id, behavior, 0);
        let owner_root = h.keep(owner);
        let c = Box::leak(Box::new(Context {
            h,
            owner,
            behavior,
            selector,
            replace: Cell::new(false),
            calls: Cell::new(0),
            finalized: Cell::new(0),
        }));
        let entry = NativeEntry::new(answer, ptr::from_mut(c).cast());
        let observer_behavior = support::behavior(id);
        let observer_root = h.keep(observer_behavior);
        install(
            id,
            observer_behavior,
            b"finalize:",
            finalize,
            ptr::from_mut(c).cast(),
        );
        let observer = allocate(id, observer_behavior, 0);
        send(
            id,
            h.gc,
            b"finalize:with:",
            &[Operand::object(owner), Operand::object(observer)],
        );
        h.release(observer_root);
        let method = h.owned(entry, owner);
        let method_root = h.keep(method);
        h.bind(behavior, selector, method);
        h.release(method_root);
        h.release(owner_root);
        h.gc();
        assert!(present(id, method), "binding must retain Method");
        assert!(present(id, owner), "binding alone must retain native owner");
        assert_eq!(
            id.send(instance, selector, &[]).unwrap().as_object(),
            Some(owner)
        );
        // No captured Method or temporary owner root exists here. Remove the
        // sole binding inside the call and collect before returning from it.
        c.replace.set(true);
        assert_eq!(
            id.send(instance, selector, &[]).unwrap().as_object(),
            Some(owner)
        );
        // The caller now owns the returned live extent and reinstalls it before
        // testing a separately held Method's exact-entry behavior.
        let reinstall_root = h.keep(method);
        h.bind(behavior, selector, method);
        h.release(reinstall_root);
        let captured = object(send(
            id,
            instance,
            b"methodFor:",
            &[Operand::object(selector)],
        ));
        let captured_root = h.keep(captured);
        assert_eq!(
            send(id, captured, b"clone", &[]).as_object(),
            Some(captured)
        );
        c.replace.set(true);
        assert_eq!(
            id.send(instance, selector, &[]).unwrap().as_object(),
            Some(owner)
        );
        assert_eq!(
            id.send(instance, selector, &[]).unwrap().as_object(),
            Some(instance)
        );
        assert_eq!(
            send(id, captured, b"invokeOn:", &[Operand::object(instance)]).as_object(),
            Some(owner)
        );

        let seq_behavior = support::behavior(id);
        let seq_behavior_root = h.keep(seq_behavior);
        let seq_state = Box::leak(Box::new(Sequence {
            h,
            returned: RefCell::new(Vec::new()),
            fail: Cell::new(false),
        }));
        install(
            id,
            seq_behavior,
            b"size",
            size,
            ptr::from_mut(seq_state).cast(),
        );
        install(
            id,
            seq_behavior,
            b"at:",
            at,
            ptr::from_mut(seq_state).cast(),
        );
        let seq = allocate(id, seq_behavior, 0);
        let seq_root = h.keep(seq);
        let result = object(send(
            id,
            captured,
            b"invokeOn:arguments:",
            &[Operand::object(instance), Operand::object(seq)],
        ));
        let result_root = h.keep(result);
        assert_eq!(send(id, result, b"word", &[]).as_word(), Some(102));
        h.gc();
        for &v in &seq_state.returned.borrow()[..2] {
            assert!(!present(id, v), "completed argument leaked");
        }
        h.release(result_root);
        h.gc();
        assert!(!present(id, result));
        seq_state.returned.borrow_mut().clear();
        seq_state.fail.set(true);
        let failure = id
            .send_named(
                captured,
                b"invokeOn:arguments:",
                &[Operand::object(instance), Operand::object(seq)],
            )
            .unwrap_err();
        assert_eq!(failure.detail, 91);
        h.gc();
        for &v in seq_state.returned.borrow().iter() {
            assert!(!present(id, v), "failed argument read leaked");
        }

        // Same implementation, different native contexts. A nested lookup must
        // not replace the selected context, and invalid language lookup is rejected.
        let other_owner = allocate(id, behavior, 0);
        let other_owner_root = h.keep(other_owner);
        let c2 = Box::leak(Box::new(Context {
            h,
            owner: other_owner,
            behavior,
            selector,
            replace: Cell::new(false),
            calls: Cell::new(0),
            finalized: Cell::new(0),
        }));
        let other = h.owned(
            NativeEntry::new(answer, ptr::from_mut(c2).cast()),
            other_owner,
        );
        let other_root = h.keep(other);
        let nested = support::behavior(id);
        let nested_root = h.keep(nested);
        h.bind(nested, selector, other);
        let meta = support::behavior(id);
        let meta_root = h.keep(meta);
        let custom = support::behavior(id);
        let custom_root = h.keep(custom);
        let custom_instance = allocate(id, custom, 0);
        let custom_instance_root = h.keep(custom_instance);
        let lookup = id.intern_name(b"lookup").unwrap();
        let lc = Box::leak(Box::new(Lookup {
            h,
            selected: captured,
            nested,
            selector,
            mode: Cell::new(0),
        }));
        let lookup_method = h.owned(NativeEntry::new(choose, ptr::from_mut(lc).cast()), owner);
        h.bind(meta, lookup, lookup_method);
        send(
            id,
            id.receiver,
            b"reassociateBehaviorOf:with:",
            &[Operand::object(custom), Operand::object(meta)],
        );
        assert_eq!(
            id.send(custom_instance, selector, &[]).unwrap().as_object(),
            Some(owner)
        );
        assert_eq!(c2.calls.get(), 0, "nested lookup changed selected context");
        lc.mode.set(1);
        assert_eq!(
            send(id, custom, b"lookup", &[Operand::object(selector)]).kind(),
            native::UNIT
        );
        lc.mode.set(2);
        assert_eq!(
            id.send_named(custom, b"lookup", &[Operand::object(selector)])
                .unwrap_err()
                .status,
            native::INVALID_REQUEST
        );
        // Reassign before collection so the intentionally invalid lookup cannot
        // interfere with GC's own reference protocol.
        send(
            id,
            id.receiver,
            b"reassociateBehaviorOf:with:",
            &[Operand::object(custom), Operand::object(id.receiver)],
        );
        h.remove(meta, lookup);
        h.remove(nested, selector);
        for token in [
            other_root,
            other_owner_root,
            nested_root,
            meta_root,
            custom_root,
            custom_instance_root,
            seq_root,
            seq_behavior_root,
            captured_root,
        ] {
            h.release(token);
        }
        h.remove(behavior, selector);
        h.gc();
        send(id, h.named(b"Execution"), b"checkpoint", &[]);
        assert!(!present(id, owner), "last reference did not release owner");
        assert!(!present(id, method));
        assert!(!present(id, captured));
        assert_eq!(c.finalized.get(), 1);

        // Exercise an executable produced outside Rust and loaded through the
        // real retaining Bootstrap loader, then remove the loader's result root.
        let fixture_owner = allocate(id, behavior, 0);
        let fixture_owner_root = h.keep(fixture_owner);
        let fixture_path = directory().join("libbirnam_execution_method_fixture.dylib");
        let fixture = object(send(
            id,
            h.named(b"Bootstrap"),
            b"loadNative:",
            &[
                Operand::bytes(fixture_path.as_os_str().as_bytes()),
                Operand::object(h.named(b"Method")),
                Operand::object(fixture_owner),
            ],
        ));
        h.bind(behavior, selector, fixture);
        assert_eq!(
            send(
                id,
                h.named(b"Bootstrap"),
                b"release:",
                &[Operand::object(fixture)]
            )
            .as_word(),
            Some(1)
        );
        h.release(fixture_owner_root);
        h.gc();
        assert_eq!(
            id.send(instance, selector, &[]).unwrap().as_object(),
            Some(fixture_owner)
        );
        h.remove(behavior, selector);
        h.gc();
        assert!(
            !present(id, fixture_owner),
            "native fixture mapping must not hide an owner leak"
        );
        assert!(!present(id, fixture));

        let cycle_behavior = support::behavior(id);
        let cycle_root = h.keep(cycle_behavior);
        let cycle_owner = allocate(id, cycle_behavior, 0);
        let cycle_owner_root = h.keep(cycle_owner);
        let cycle_method = object(send(
            id,
            h.named(b"Bootstrap"),
            b"loadNative:",
            &[
                Operand::bytes(fixture_path.as_os_str().as_bytes()),
                Operand::object(h.named(b"Method")),
                Operand::object(cycle_owner),
            ],
        ));
        h.bind(cycle_behavior, selector, cycle_method);
        send(
            id,
            h.named(b"Bootstrap"),
            b"release:",
            &[Operand::object(cycle_method)],
        );
        h.release(cycle_owner_root);
        h.release(cycle_root);
        h.gc();
        for value in [cycle_behavior, cycle_owner, cycle_method] {
            assert!(
                !present(id, value),
                "unreachable binding/Method/owner cycle leaked"
            );
        }

        // Keep the checked language result convention at Method invocation.
        let result_mode = Box::leak(Box::new(Cell::new(0usize)));
        let result_method = h.owned(
            NativeEntry::new(native_result, ptr::from_mut(result_mode).cast()),
            h.named(b"nil"),
        );
        let result_method_root = h.keep(result_method);
        assert_eq!(
            send(
                id,
                result_method,
                b"invokeOn:",
                &[Operand::object(instance)]
            )
            .as_object(),
            Some(h.named(b"nil"))
        );
        for (mode, kind) in [
            (1, b"nativeResultRequiresWrapper".as_slice()),
            (2, b"nativeInvalidRequest"),
            (3, b"nativeFailure"),
        ] {
            result_mode.set(mode);
            let error = object(send(
                id,
                result_method,
                b"invokeOn:",
                &[Operand::object(instance)],
            ));
            let error_root = h.keep(error);
            assert_eq!(
                send(id, error, b"kind", &[]).as_object(),
                Some(id.intern_name(kind).unwrap())
            );
            h.release(error_root);
        }
        h.release(result_method_root);

        // A Method capturing native Id lookup already returns NativeEntry/Unit.
        // Installing it as lookup must preserve that typed result convention.
        let raw_lookup = object(send(
            id,
            id.receiver,
            b"methodFor:",
            &[Operand::object(lookup)],
        ));
        let raw_lookup_root = h.keep(raw_lookup);
        let native_meta = support::behavior(id);
        let native_meta_root = h.keep(native_meta);
        h.bind(native_meta, lookup, raw_lookup);
        let native_behavior = support::behavior(id);
        let native_behavior_root = h.keep(native_behavior);
        install(
            id,
            native_behavior,
            b"rawLookupProbe",
            receiver,
            ptr::null_mut(),
        );
        send(
            id,
            id.receiver,
            b"reassociateBehaviorOf:with:",
            &[
                Operand::object(native_behavior),
                Operand::object(native_meta),
            ],
        );
        let selected = send(
            id,
            native_behavior,
            b"lookup",
            &[Operand::object(id.intern_name(b"rawLookupProbe").unwrap())],
        );
        assert!(selected.as_entry().is_some());
        let reflected = object(send(
            id,
            raw_lookup,
            b"invokeOn:arguments:",
            &[
                Operand::object(native_behavior),
                Operand::object(object(send(
                    id,
                    h.named(b"Vector"),
                    b"add:",
                    &[Operand::object(id.intern_name(b"rawLookupProbe").unwrap())],
                ))),
            ],
        ));
        let reflected_root = h.keep(reflected);
        assert_eq!(
            send(id, reflected, b"invokeOn:", &[Operand::object(instance)]).as_object(),
            Some(instance)
        );
        for token in [
            reflected_root,
            raw_lookup_root,
            native_behavior_root,
            native_meta_root,
        ] {
            h.release(token);
        }
        h.gc();
        send(id, h.named(b"Execution"), b"checkpoint", &[]);
        for token in [instance_root, behavior_root, selector_root] {
            h.release(token);
        }
    }
}
