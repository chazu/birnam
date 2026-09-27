//! Real dylib admission and language wrappers through the production Bootstrap.
mod support;
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
};
use core::{
    cell::{Cell, RefCell},
    ffi::c_void,
    ptr,
};
use std::os::unix::ffi::OsStrExt;
use support::*;
struct H {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    gc: Oop,
    tokens: RefCell<Vec<usize>>,
}
impl H {
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
    unsafe fn keep(&self, o: Oop) -> Oop {
        self.tokens
            .borrow_mut()
            .push(unsafe { retain(self.id, self.roots, o) });
        o
    }
    unsafe fn string(&self, b: &[u8]) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                self.named(b"String"),
                b"fromBytes:",
                &[Operand::bytes(b)],
            )))
        }
    }
    unsafe fn call(&self, o: Oop, n: &[u8], a: &[Oop]) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                o,
                n,
                &a.iter().copied().map(Operand::object).collect::<Vec<_>>(),
            )))
        }
    }
    unsafe fn error(&self, o: Oop, kind: &[u8], message: &[u8]) {
        unsafe {
            assert_eq!(
                self.call(o, b"kind", &[]),
                self.id.intern_name(kind).unwrap()
            );
            let m = self.call(o, b"message", &[]);
            let b = send(self.id, m, b"bytes", &[]).as_bytes().unwrap();
            assert_eq!(borrowed_bytes(b.pointer, b.length).unwrap(), message);
        }
    }
}
unsafe fn binding(id: Protocol, vt: Oop, n: &[u8]) -> NativeEntry {
    unsafe {
        send(
            id,
            id.receiver,
            b"bindingOf:in:",
            &[
                Operand::object(id.intern_name(n).unwrap()),
                Operand::object(vt),
            ],
        )
        .as_entry()
        .unwrap()
    }
}
unsafe fn set(id: Protocol, vt: Oop, n: &[u8], entry: NativeEntry) {
    unsafe {
        send(
            id,
            vt,
            b"addMethod",
            &[
                Operand::object(id.intern_name(n).unwrap()),
                Operand::entry(entry),
            ],
        );
    }
}
struct Sequence {
    id: Protocol,
    gc: Oop,
    integer: Oop,
    vt: Oop,
    length: Cell<i64>,
    wrong: Cell<bool>,
    fail: Cell<bool>,
    seen: RefCell<Vec<usize>>,
    made: RefCell<Vec<Oop>>,
    path: RefCell<Vec<u8>>,
    mutate: Cell<bool>,
}
unsafe extern "C" fn size(c: *mut c_void, _: Oop, _: *const Operand, n: usize) -> Outcome {
    unsafe {
        assert_eq!(n, 0);
        let s = &*c.cast::<Sequence>();
        collect(s.id, s.gc);
        if s.mutate.get() {
            s.path.borrow_mut().fill(b'x');
        }
        if s.wrong.get() {
            return Outcome::loaded(Operand::object(s.gc));
        }
        s.id.send_named(
            s.integer,
            b"fromWord:",
            &[Operand::word(s.length.get() as usize)],
        )
        .map_or_else(|e| e, Outcome::loaded)
    }
}
unsafe extern "C" fn at(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let s = &*c.cast::<Sequence>();
        collect(s.id, s.gc);
        if s.fail.get() {
            return Outcome::failed(native::INVALID_REQUEST, 973);
        }
        let [index] = borrowed_slice(a, n).unwrap() else {
            panic!()
        };
        let index = send(s.id, s.integer, b"wordOf:", &[*index])
            .as_word()
            .unwrap();
        s.seen.borrow_mut().push(index);
        let o = if index == 0 && s.length.get() == 2 {
            s.gc
        } else {
            allocate(s.id, s.vt, 0)
        };
        if o != s.gc {
            s.made.borrow_mut().push(o);
        }
        Outcome::loaded(Operand::object(o))
    }
}
struct Forward {
    id: Protocol,
    gc: Oop,
    alias: Oop,
    calls: Cell<usize>,
    failure: Cell<bool>,
    path: Oop,
    sequence: *const Sequence,
    ns: Oop,
    publish: Cell<Oop>,
}
unsafe extern "C" fn forward(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let s = &*c.cast::<Forward>();
        collect(s.id, s.gc);
        s.calls.set(s.calls.get() + 1);
        if s.failure.get() {
            return Outcome::failed(native::INVALID_REQUEST, 974);
        }
        if !s.publish.get().is_null() {
            send(
                s.id,
                s.ns,
                b"at:put:",
                &[
                    Operand::object(s.id.intern_name(b"Error").unwrap()),
                    Operand::object(s.publish.get()),
                ],
            );
        }
        if r == s.path {
            let bytes = (&*s.sequence).path.borrow();
            return Outcome::loaded(Operand::bytes(&bytes));
        }
        s.id.send(r, s.alias, borrowed_slice(a, n).unwrap())
            .map_or_else(|e| e, Outcome::loaded)
    }
}
unsafe extern "C" fn answer(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::object(c))
}

#[test]
#[ignore = "requires native prefix and C loader fixtures"]
fn native_loader_uses_messages() {
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
        let h = H {
            id,
            ns,
            roots: named(b"Roots"),
            gc: named(b"GC"),
            tokens: RefCell::new(Vec::new()),
        };
        send(id, h.gc, b"precisePayloads:", &[Operand::word(1)]);
        let system = h.named(b"System");
        let nil = h.named(b"nil");
        let system_vt = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(system)],
        ));
        let target = directory().join("libbirnam_loader_target_fixture.dylib");
        let path = h.string(target.as_os_str().as_bytes());
        let missing = h.string(
            directory()
                .join("no-such-module.dylib")
                .as_os_str()
                .as_bytes(),
        );
        let no_entry = h.string(
            directory()
                .join("libbirnam_loader_missing_fixture.dylib")
                .as_os_str()
                .as_bytes(),
        );
        let incompatible = h.string(
            directory()
                .join("libbirnam_loader_incompatible_fixture.dylib")
                .as_os_str()
                .as_bytes(),
        );
        assert_eq!(h.call(system, b"loadModule:", &[path]), system);
        assert_eq!(h.call(system, b"loadModule:", &[]), nil);
        assert_eq!(h.call(system, b"loadModule:", &[path, path]), nil);
        assert_eq!(h.call(system, b"loadModule:arguments:", &[path]), nil);
        assert_eq!(h.call(system, b"loadModule:", &[nil]), nil);
        assert_eq!(
            id.send_named(system, b"loadModule:", &[Operand::word(0)])
                .unwrap_err()
                .status,
            native::INVALID_REQUEST
        );
        h.error(
            h.call(system, b"loadModule:", &[missing]),
            b"ioError",
            b"could not load module dylib",
        );
        for p in [no_entry, incompatible] {
            h.error(
                h.call(system, b"loadModule:", &[p]),
                b"notAModule",
                b"missing or incompatible birnam_native_module_v1",
            );
        }
        // Binary paths retain Id's existing invalid-request -> moduleError mapping.
        let nul = h.string(b"bad\0path");
        h.error(
            h.call(system, b"loadModule:", &[nul]),
            b"moduleError",
            b"native module initialization failed",
        );
        let vt = h.keep(behavior(id));
        let mut sequence = Sequence {
            id,
            gc: h.gc,
            integer: h.named(b"Integer"),
            vt,
            length: Cell::new(0),
            wrong: Cell::new(false),
            fail: Cell::new(false),
            seen: RefCell::new(Vec::new()),
            made: RefCell::new(Vec::new()),
            path: RefCell::new(target.as_os_str().as_bytes().to_vec()),
            mutate: Cell::new(false),
        };
        let seq_vt = h.keep(behavior(id));
        install(
            id,
            seq_vt,
            b"size",
            size,
            (&mut sequence as *mut Sequence).cast(),
        );
        install(
            id,
            seq_vt,
            b"at:",
            at,
            (&mut sequence as *mut Sequence).cast(),
        );
        let seq = h.keep(allocate(id, seq_vt, 0));
        assert_eq!(
            h.call(system, b"loadModule:arguments:", &[path, seq]),
            system
        );
        sequence.wrong.set(true);
        h.error(
            h.call(system, b"loadModule:arguments:", &[path, seq]),
            b"typeError",
            b"module arguments need an Integer size",
        );
        sequence.wrong.set(false);
        sequence.length.set(-1);
        h.error(
            h.call(system, b"loadModule:arguments:", &[path, seq]),
            b"typeError",
            b"module argument count must be nonnegative",
        );
        // Collect at every migrated dependency boundary; forward exclusively by sends.
        let factory_behavior = |name: &[u8]| {
            object(send(
                id,
                id.receiver,
                b"behaviorOf:",
                &[Operand::object(h.named(name))],
            ))
        };
        let mut guards = Vec::new();
        for (owner, name, alias) in [
            (
                factory_behavior(b"Integer"),
                b"fromWord:".as_slice(),
                b"loaderOriginalFromWord:".as_slice(),
            ),
            (
                factory_behavior(b"Integer"),
                b"wordOf:",
                b"loaderOriginalWordOf:",
            ),
            (h.named(b"StringVt"), b"bytes", b"loaderOriginalBytes"),
            (
                factory_behavior(b"String"),
                b"fromBytes:",
                b"loaderOriginalFromBytes:",
            ),
            (
                factory_behavior(b"Error"),
                b"kind:message:",
                b"loaderOriginalError:message:",
            ),
            (id.receiver, b"load", b"loaderOriginalLoad"),
        ] {
            // The load selector is supplied by Id; do not assume its textual spelling.
            let selector = if owner == id.receiver {
                id.load
            } else {
                id.intern_name(name).unwrap()
            };
            let original = send(
                id,
                id.receiver,
                b"bindingOf:in:",
                &[Operand::object(selector), Operand::object(owner)],
            )
            .as_entry()
            .unwrap();
            set(id, owner, alias, original);
            let mut context = Box::new(Forward {
                id,
                gc: h.gc,
                alias: id.intern_name(alias).unwrap(),
                calls: Cell::new(0),
                failure: Cell::new(false),
                path: if name == b"bytes" {
                    path
                } else {
                    ptr::null_mut()
                },
                sequence: &sequence,
                ns,
                publish: Cell::new(ptr::null_mut()),
            });
            send(
                id,
                owner,
                b"addMethod",
                &[
                    Operand::object(selector),
                    Operand::entry(NativeEntry::new(
                        forward,
                        (&mut *context as *mut Forward).cast(),
                    )),
                ],
            );
            guards.push((owner, selector, alias, original, context));
        }
        sequence.length.set(2);
        sequence.mutate.set(true);
        let returned = h.call(system, b"loadModule:arguments:", &[path, seq]);
        assert_eq!(sequence.seen.borrow().as_slice(), [0, 1]);
        assert_eq!(returned, *sequence.made.borrow().last().unwrap());
        assert!(present(id, returned)); // fixture collected with all arguments in use
        sequence.mutate.set(false);
        *sequence.path.borrow_mut() = target.as_os_str().as_bytes().to_vec();
        // Unit returns the actual receiver, whose only protection is the wrapper's roots.
        let alias_vt = h.keep(behavior(id));
        set(
            id,
            alias_vt,
            b"loadModule:",
            binding(id, system_vt, b"loadModule:"),
        );
        let receiver = allocate(id, alias_vt, 0);
        assert_eq!(h.call(receiver, b"loadModule:", &[path]), receiver);
        assert!(present(id, receiver));
        sequence.length.set(1);
        let returned = h.call(system, b"loadModule:arguments:", &[path, seq]);
        assert_eq!(returned, *sequence.made.borrow().last().unwrap());
        sequence.length.set(3);
        h.error(
            h.call(system, b"loadModule:arguments:", &[path, seq]),
            b"typeError",
            b"native module returned a non-language value",
        );
        sequence.length.set(4);
        h.error(
            h.call(system, b"loadModule:arguments:", &[path, seq]),
            b"moduleError",
            b"native module initialization failed",
        );
        collect(id, h.gc);
        for &o in sequence.made.borrow().iter().rev().take(4) {
            assert!(!present(id, o));
        }
        sequence.fail.set(true);
        sequence.length.set(1);
        assert_eq!(
            id.send_named(
                system,
                b"loadModule:arguments:",
                &[Operand::object(path), Operand::object(seq)]
            )
            .unwrap_err()
            .detail,
            973
        );
        sequence.fail.set(false);
        // Failures from conversion sends propagate; failures from Id load retain language mapping.
        for index in [0, 1, 2, 3, 4, 5] {
            guards[index].4.failure.set(true);
            let args = if index == 3 || index == 4 {
                vec![Operand::object(missing)]
            } else {
                vec![Operand::object(path), Operand::object(seq)]
            };
            let result = id.send_named(
                system,
                if args.len() == 1 {
                    b"loadModule:"
                } else {
                    b"loadModule:arguments:"
                },
                &args,
            );
            if index == 5 {
                h.error(
                    h.keep(object(result.unwrap())),
                    b"moduleError",
                    b"native module initialization failed",
                );
            } else {
                assert_eq!(result.unwrap_err().detail, 974);
            }
            guards[index].4.failure.set(false);
        }
        // Lazy Error lookup sees publication during String construction.
        let error = h.named(b"Error");
        let factory_vt = h.keep(behavior(id));
        let sentinel = h.keep(allocate(id, vt, 0));
        install(id, factory_vt, b"kind:message:", answer, sentinel);
        let factory = h.keep(allocate(id, factory_vt, 0));
        guards[3].4.publish.set(factory);
        assert_eq!(h.call(system, b"loadModule:", &[missing]), sentinel);
        guards[3].4.publish.set(ptr::null_mut());
        send(
            id,
            ns,
            b"at:put:",
            &[
                Operand::object(id.intern_name(b"Error").unwrap()),
                Operand::object(error),
            ],
        );
        for (owner, selector, alias, entry, context) in guards {
            assert!(context.calls.get() > 0);
            send(
                id,
                owner,
                b"addMethod",
                &[Operand::object(selector), Operand::entry(entry)],
            );
            send(
                id,
                owner,
                b"removeMethod:",
                &[Operand::object(id.intern_name(alias).unwrap())],
            );
        }
        // The ordinary compiled-source path reaches the same production wrapper.
        let source =
            h.string(format!("[System loadModule: {:?}]", target.to_str().unwrap()).as_bytes());
        assert_eq!(h.call(h.named(b"Codegen"), b"eval:", &[source]), system);
        let original = binding(id, system_vt, b"loadModule:");
        install(id, system_vt, b"loadModule:", answer, sentinel);
        assert_eq!(load(id, "native_loader", &[Operand::object(ns)]), system);
        assert_eq!(h.call(system, b"loadModule:", &[path]), sentinel);
        set(id, system_vt, b"loadModule:", original);
        for name in [b"size".as_slice(), b"at:"] {
            send(
                id,
                seq_vt,
                b"removeMethod:",
                &[Operand::object(id.intern_name(name).unwrap())],
            );
        }
        for token in h.tokens.into_inner() {
            release(id, h.roots, token);
        }
    }
}
