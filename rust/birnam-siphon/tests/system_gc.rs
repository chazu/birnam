//! System controls, conservative/precise payloads, and live tracing changes.
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
use support::*;
struct H {
    id: Protocol,
    ns: Oop,
    gc: Oop,
    roots: Oop,
    model: Oop,
    tokens: RefCell<Vec<usize>>,
}
impl H {
    unsafe fn named(&self, n: &[u8]) -> Oop {
        object(unsafe {
            send(
                self.id,
                self.ns,
                b"at:",
                &[Operand::object(self.id.intern_name(n).unwrap())],
            )
        })
    }
    unsafe fn keep(&self, o: Oop) -> Oop {
        self.tokens
            .borrow_mut()
            .push(unsafe { retain(self.id, self.roots, o) });
        o
    }
    unsafe fn integer(&self, n: usize) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                self.named(b"Integer"),
                b"fromWord:",
                &[Operand::word(n)],
            )))
        }
    }
    unsafe fn layout(&self, b: Oop, kind: u8) {
        unsafe {
            send(
                self.id,
                self.model,
                b"setLayoutOf:to:rest:",
                &[
                    Operand::object(b),
                    Operand::bytes(&[]),
                    Operand::word(kind as usize),
                ],
            );
        }
    }
    unsafe fn raw(&self, b: Oop, word: usize) -> Oop {
        let o = object(unsafe {
            send(
                self.id,
                self.model,
                b"allocate:bytes:",
                &[Operand::object(b), Operand::word(size_of::<usize>())],
            )
        });
        unsafe {
            o.cast::<usize>().write(word);
        }
        o
    }
    unsafe fn clear_since(&self, start: usize) {
        let tokens = self.tokens.borrow_mut().split_off(start);
        for t in tokens {
            unsafe {
                release(self.id, self.roots, t);
            }
        }
    }
    unsafe fn mode(&self, enabled: bool) {
        unsafe {
            send(
                self.id,
                self.gc,
                b"precisePayloads:",
                &[Operand::word(enabled as usize)],
            );
        }
    }
}
unsafe fn binding(id: Protocol, b: Oop, n: &[u8]) -> NativeEntry {
    unsafe {
        send(
            id,
            id.receiver,
            b"bindingOf:in:",
            &[
                Operand::object(id.intern_name(n).unwrap()),
                Operand::object(b),
            ],
        )
    }
    .as_entry()
    .unwrap()
}
unsafe fn bind(id: Protocol, b: Oop, n: &[u8], e: NativeEntry) {
    unsafe {
        send(
            id,
            b,
            b"addMethod",
            &[
                Operand::object(id.intern_name(n).unwrap()),
                Operand::entry(e),
            ],
        );
    }
}
unsafe fn vt(id: Protocol, o: Oop) -> Oop {
    object(unsafe { send(id, id.receiver, b"behaviorOf:", &[Operand::object(o)]) })
}
struct Spy {
    id: Protocol,
    alias: Oop,
    calls: Cell<usize>,
    failure: Cell<bool>,
}
unsafe extern "C" fn forward(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    let s = unsafe { &*c.cast::<Spy>() };
    s.calls.set(s.calls.get() + 1);
    if s.failure.get() {
        return Outcome::failed(native::INVALID_REQUEST, 971);
    }
    unsafe { s.id.send(r, s.alias, borrowed_slice(a, n).unwrap()) }
        .map_or_else(|e| e, Outcome::loaded)
}
unsafe extern "C" fn word(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::word(c.addr()))
}
struct Query {
    id: Protocol,
    integer_vt: Oop,
    answer: Oop,
    word: Cell<usize>,
}
unsafe extern "C" fn constant_answer(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::object(c))
}
unsafe extern "C" fn query(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    let q = unsafe { &*c.cast::<Query>() };
    unsafe {
        install(q.id, q.integer_vt, b"fromWord:", constant_answer, q.answer);
    }
    Outcome::loaded(Operand::word(q.word.get()))
}
struct Finalizer {
    id: Protocol,
    gc: Oop,
    calls: Cell<usize>,
}
unsafe extern "C" fn finalize(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    (|| unsafe {
        let f = &*c.cast::<Finalizer>();
        f.calls.set(f.calls.get() + 1);
        f.id.send_named(f.gc, b"automaticCollection:", &[Operand::word(0)])?;
        f.id.send_named(f.gc, b"precisePayloads:", &[Operand::word(1)])?;
        assert_eq!(f.id.send_named(f.gc, b"collect", &[])?.as_word(), Some(0));
        Ok(Operand::unit())
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
struct Walk {
    id: Protocol,
    gc: Oop,
    model: Oop,
    parent: Oop,
    carrier: Oop,
    trigger: Oop,
    behavior: Oop,
    next: bool,
    change_layout: bool,
    events: RefCell<Vec<&'static str>>,
}
unsafe extern "C" fn walk(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let w = &*c.cast::<Walk>();
        let [target, visitor] = borrowed_slice(a, n).unwrap() else {
            panic!()
        };
        if target.as_object() == Some(w.parent) {
            for o in [w.carrier, w.trigger] {
                w.id.send_named(object(*visitor), b"visit:", &[Operand::object(o)])?;
            }
        } else if target.as_object() == Some(w.trigger) {
            w.events.borrow_mut().push("trigger");
            w.id.send_named(w.gc, b"precisePayloads:", &[Operand::word(w.next as usize)])?;
            w.id.send_named(w.gc, b"automaticCollection:", &[Operand::word(0)])?;
            if w.change_layout {
                w.id.send_named(
                    w.model,
                    b"setLayoutOf:to:rest:",
                    &[
                        Operand::object(w.behavior),
                        Operand::bytes(&[]),
                        Operand::word(1),
                    ],
                )?;
            }
            assert_eq!(w.id.send_named(w.gc, b"collect", &[])?.as_word(), Some(0));
            assert!(
                w.id.send_named(w.behavior, b"allocate", &[Operand::word(0)])
                    .is_err()
            );
        } else if target.as_object() == Some(w.carrier) {
            w.events.borrow_mut().push("carrier");
            w.id.send_named(w.model, b"referencesOf:do:", &[*target, *visitor])?;
        }
        Ok(Operand::unit())
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
#[test]
#[ignore = "requires native prefix"]
fn gc_controls_and_payload_policy_use_messages() {
    unsafe {
        let id = id();
        let ns = load(
            id,
            "bootstrap",
            &[Operand::bytes(directory().as_os_str().as_encoded_bytes())],
        );
        let named = |n: &[u8]| {
            object(send(
                id,
                ns,
                b"at:",
                &[Operand::object(id.intern_name(n).unwrap())],
            ))
        };
        let h = H {
            id,
            ns,
            gc: named(b"GC"),
            roots: named(b"Roots"),
            model: named(b"ObjectModel"),
            tokens: RefCell::new(Vec::new()),
        };
        let system = named(b"System");
        let nil = named(b"nil");
        let truth = named(b"true");
        let falsity = named(b"false");
        assert_eq!(send(id, h.gc, b"precisePayloads", &[]).as_word(), Some(0));
        // No API setting changes are hidden in the shared test harness.
        for selector in [b"autoGc:".as_slice(), b"preciseGc:"] {
            for args in [
                vec![],
                vec![Operand::object(nil)],
                vec![Operand::object(truth), Operand::object(falsity)],
            ] {
                assert_eq!(send(id, system, selector, &args).as_object(), Some(nil));
            }
        }
        for selector in [b"gc".as_slice(), b"liveObjects"] {
            assert_eq!(
                send(id, system, selector, &[Operand::object(nil)]).as_object(),
                Some(nil)
            );
        }
        for selector in [b"automaticCollection:".as_slice(), b"precisePayloads:"] {
            assert!(id.send_named(h.gc, selector, &[]).is_err());
            assert!(
                id.send_named(h.gc, selector, &[Operand::object(truth)])
                    .is_err()
            );
        }
        assert_eq!(
            send(id, system, b"autoGc:", &[Operand::object(falsity)]).as_object(),
            Some(system)
        );
        // Existing queries observe current implementations and select Integer afterwards.
        let gc_vt = vt(id, h.gc);
        let int_vt = vt(id, named(b"Integer"));
        let int_original = binding(id, int_vt, b"fromWord:");
        let sentinel = h.keep(allocate(id, named(b"ObjectVt"), 0));
        let mut q = Query {
            id,
            integer_vt: int_vt,
            answer: sentinel,
            word: Cell::new(usize::MAX),
        };
        for (receiver, behavior, selector, system_selector) in [
            (h.gc, gc_vt, b"collect".as_slice(), b"gc".as_slice()),
            (id.receiver, id.receiver, b"allocationCount", b"liveObjects"),
        ] {
            let original = binding(id, behavior, selector);
            install(id, behavior, selector, query, (&raw mut q).cast());
            assert_eq!(
                send(id, system, system_selector, &[]).as_object(),
                Some(sentinel)
            );
            bind(id, behavior, selector, original);
            bind(id, int_vt, b"fromWord:", int_original);
            let _ = receiver;
        }
        let original = binding(id, gc_vt, b"collect");
        for count in [0, 7, i64::MAX as usize, (i64::MAX as usize) + 1, usize::MAX] {
            install(
                id,
                gc_vt,
                b"collect",
                word,
                ptr::with_exposed_provenance_mut(count),
            );
            let result = object(send(id, system, b"gc", &[]));
            assert_eq!(send(id, result, b"word", &[]).as_word(), Some(count));
        }
        bind(id, gc_vt, b"collect", original);
        // Copied System setters preserve their alias receiver.
        let alias_b = h.keep(behavior(id));
        let alias = h.keep(allocate(id, alias_b, 0));
        for selector in [b"autoGc:".as_slice(), b"preciseGc:"] {
            bind(
                id,
                alias_b,
                selector,
                binding(id, named(b"SystemVt"), selector),
            );
            assert_eq!(
                send(id, alias, selector, &[Operand::object(falsity)]).as_object(),
                Some(alias)
            );
        }
        // A System query keeps an otherwise unrooted alias alive when the GC
        // implementation is replaced with a declared-roots-only collection send.
        bind(id, alias_b, b"gc", binding(id, named(b"SystemVt"), b"gc"));
        let mut collector = Spy {
            id,
            alias: id.intern_name(b"collectPrecisely").unwrap(),
            calls: Cell::new(0),
            failure: Cell::new(false),
        };
        let original = binding(id, gc_vt, b"collect");
        install(id, gc_vt, b"collect", forward, (&raw mut collector).cast());
        let unrooted = allocate(id, alias_b, 0);
        let result = object(send(id, unrooted, b"gc", &[]));
        assert!(send(id, result, b"word", &[]).as_word().is_some());
        assert!(present(id, unrooted));
        assert_eq!(collector.calls.get(), 1);
        collector.failure.set(true);
        assert_eq!(id.send_named(system, b"gc", &[]).unwrap_err().detail, 971);
        bind(id, gc_vt, b"collect", original);
        // Live forwarding uses an alias send, including exact native failure propagation.
        for selector in [b"automaticCollection:".as_slice(), b"precisePayloads:"] {
            let original = binding(id, gc_vt, selector);
            bind(id, gc_vt, b"savedGcSetting:", original);
            let mut spy = Spy {
                id,
                alias: id.intern_name(b"savedGcSetting:").unwrap(),
                calls: Cell::new(0),
                failure: Cell::new(false),
            };
            install(id, gc_vt, selector, forward, (&raw mut spy).cast());
            let public = if selector == b"automaticCollection:" {
                b"autoGc:".as_slice()
            } else {
                b"preciseGc:"
            };
            assert_eq!(
                send(id, system, public, &[Operand::object(falsity)]).as_object(),
                Some(system)
            );
            assert_eq!(spy.calls.get(), 1);
            spy.failure.set(true);
            let failure = id
                .send_named(system, public, &[Operand::object(truth)])
                .unwrap_err();
            assert_eq!(failure.detail, 971);
            bind(id, gc_vt, selector, original);
            send(id, gc_vt, b"removeMethod:", &[Operand::object(spy.alias)]);
        }
        // Native count bit patterns are passed unchanged to Integer.
        let count = send(id, id.receiver, b"allocationCount", &[])
            .as_word()
            .unwrap();
        let answer = object(send(id, system, b"liveObjects", &[]));
        assert_eq!(send(id, answer, b"word", &[]).as_word(), Some(count));
        // Empty family receivers are valid factories, even though numeric value
        // operations require eight bytes. GC must not impose that value restriction.
        for family in [b"Integer".as_slice(), b"Float", b"Word", b"Range"] {
            let empty = h.keep(allocate(id, vt(id, named(family)), 0));
            h.mode(false);
            collect(id, h.gc);
            assert!(present(id, empty));
        }
        // Owner-scanned raw slots; missing layouts remain conservative even in precise mode.
        for (kind, precise, alive) in [(0, false, true), (0, true, false), (1, true, true)] {
            let start = h.tokens.borrow().len();
            h.mode(precise);
            let b = h.keep(behavior(id));
            h.layout(b, kind);
            let target = allocate(id, named(b"ObjectVt"), 0);
            let carrier = h.keep(h.raw(b, target.addr()));
            collect(id, h.gc);
            assert_eq!(present(id, target), alive);
            assert!(present(id, carrier));
            h.clear_since(start);
        }
        h.mode(true);
        let b = h.keep(behavior(id));
        let target = allocate(id, named(b"ObjectVt"), 0);
        h.keep(h.raw(b, target.addr()));
        collect(id, h.gc);
        assert!(present(id, target));
        for family in [b"Integer".as_slice(), b"Float", b"Word"] {
            for precise in [false, true] {
                let start = h.tokens.borrow().len();
                h.mode(precise);
                let target = allocate(id, named(b"ObjectVt"), 0);
                let selector = if family == b"Integer" {
                    b"fromWord:".as_slice()
                } else {
                    b"fromBits:"
                };
                let number = h.keep(object(send(
                    id,
                    named(family),
                    selector,
                    &[Operand::word(target.addr())],
                )));
                send(
                    id,
                    id.receiver,
                    b"ensurePrivateVtableFor:",
                    &[Operand::object(number)],
                );
                collect(id, h.gc);
                assert_eq!(
                    present(id, target),
                    !precise,
                    "{} precise={precise}",
                    String::from_utf8_lossy(family)
                );
                h.clear_since(start);
            }
        }
        // Range values and String/Builder payload mirrors also retain raw candidates
        // only in conservative mode. Text bytes remain outside this payload contract.
        for family in [b"Range".as_slice(), b"String", b"StringBuilder"] {
            for precise in [false, true] {
                let start = h.tokens.borrow().len();
                h.mode(precise);
                let target = allocate(id, named(b"ObjectVt"), 0);
                let carrier = if family == b"Range" {
                    let begin = h.integer(target.addr());
                    let end = h.integer(target.addr());
                    object(send(id, begin, b"to:", &[Operand::object(end)]))
                } else if family == b"String" {
                    object(send(
                        id,
                        named(b"String"),
                        b"fromBytes:",
                        &[Operand::bytes(b"abc")],
                    ))
                } else {
                    object(send(id, named(b"StringBuilder"), b"new", &[]))
                };
                h.keep(carrier);
                // Drop the Range's construction Integers so only its own payload is tested.
                if family == b"Range" {
                    let tokens = h
                        .tokens
                        .borrow_mut()
                        .drain(start..start + 2)
                        .collect::<Vec<_>>();
                    for t in tokens {
                        release(id, h.roots, t);
                    }
                } else {
                    // Independent native fixture: mirror words do not own String buffers.
                    carrier.cast::<usize>().write(target.addr());
                }
                collect(id, h.gc);
                assert_eq!(
                    present(id, target),
                    !precise,
                    "{} precise={precise}",
                    String::from_utf8_lossy(family)
                );
                h.clear_since(start);
            }
        }
        // String contents are never candidates, even when their bytes spell an address.
        h.mode(false);
        let target = allocate(id, named(b"ObjectVt"), 0);
        h.keep(object(send(
            id,
            named(b"String"),
            b"fromBytes:",
            &[Operand::bytes(&target.addr().to_ne_bytes())],
        )));
        collect(id, h.gc);
        assert!(!present(id, target));
        // A hook changes the mode/layout before the later carrier's turn (LIFO).
        for (next, change_layout) in [(true, false), (false, false), (true, true)] {
            h.mode(!next);
            let b = h.keep(behavior(id));
            h.layout(b, 0);
            let target = allocate(id, named(b"ObjectVt"), 0);
            let carrier = h.raw(b, target.addr());
            let parent = h.keep(allocate(id, b, 0));
            let trigger = allocate(id, b, 0);
            let owner_b = h.keep(behavior(id));
            let owner = h.keep(allocate(id, owner_b, 0));
            let mut w = Walk {
                id,
                gc: h.gc,
                model: h.model,
                parent,
                carrier,
                trigger,
                behavior: b,
                next,
                change_layout,
                events: RefCell::new(Vec::new()),
            };
            install(id, owner_b, b"referencesOf:do:", walk, (&raw mut w).cast());
            send(
                id,
                h.gc,
                b"trace:with:",
                &[Operand::object(b), Operand::object(owner)],
            );
            collect(id, h.gc);
            assert_eq!(*w.events.borrow(), ["trigger", "carrier"]);
            assert_eq!(present(id, target), !next || change_layout);
            // Remove this test-owned graph before its native context leaves scope.
            send(
                id,
                h.gc,
                b"trace:with:",
                &[Operand::object(b), Operand::unit()],
            );
            send(
                id,
                owner_b,
                b"removeMethod:",
                &[Operand::object(
                    id.intern_name(b"referencesOf:do:").unwrap(),
                )],
            );
        }
        // Precision is queried through a live message, with failures preventing sweep.
        h.mode(true);
        let original = binding(id, gc_vt, b"precisePayloads");
        bind(id, gc_vt, b"savedPrecision", original);
        let mut spy = Spy {
            id,
            alias: id.intern_name(b"savedPrecision").unwrap(),
            calls: Cell::new(0),
            failure: Cell::new(false),
        };
        install(
            id,
            gc_vt,
            b"precisePayloads",
            forward,
            (&raw mut spy).cast(),
        );
        collect(id, h.gc);
        assert!(spy.calls.get() > 0);
        let dead = allocate(id, named(b"ObjectVt"), 0);
        spy.failure.set(true);
        let charged = send(id, h.gc, b"allocationBytes", &[]).as_word();
        let error = id.send_named(h.gc, b"collectPrecisely", &[]).unwrap_err();
        assert_eq!(error.detail, 971);
        assert!(present(id, dead));
        assert_eq!(send(id, h.gc, b"allocationBytes", &[]).as_word(), charged);
        bind(id, gc_vt, b"precisePayloads", original);
        send(id, gc_vt, b"removeMethod:", &[Operand::object(spy.alias)]);
        collect(id, h.gc);
        assert!(!present(id, dead));
        // Native-stack roots remain protected by ordinary collect in either payload mode.
        for precise in [false, true] {
            h.mode(precise);
            let stack_only = allocate(id, named(b"ObjectVt"), 0);
            std::hint::black_box(&stack_only);
            send(id, h.gc, b"collect", &[]);
            assert!(present(id, std::hint::black_box(stack_only)));
        }
        // Disabled collection keeps accounting; enabling does not collect immediately.
        h.mode(true);
        send(id, h.gc, b"budget:", &[Operand::word(1)]);
        send(id, system, b"autoGc:", &[Operand::object(falsity)]);
        collect(id, h.gc);
        let mut previous = 0;
        for _ in 0..3 {
            h.keep(allocate(id, named(b"ObjectVt"), 0));
            let charged = send(id, h.gc, b"allocationBytes", &[]).as_word().unwrap();
            assert!(
                charged > previous,
                "disabled automatic GC must not reset charges during allocation"
            );
            previous = charged;
        }
        let charge = send(id, h.gc, b"allocationBytes", &[]).as_word().unwrap();
        assert!(charge > 0);
        send(id, h.gc, b"prepareAllocation", &[]);
        assert_eq!(
            send(id, h.gc, b"allocationBytes", &[]).as_word(),
            Some(charge)
        );
        send(id, system, b"autoGc:", &[Operand::object(truth)]);
        assert_eq!(
            send(id, h.gc, b"allocationBytes", &[]).as_word(),
            Some(charge)
        );
        send(id, h.gc, b"prepareAllocation", &[]);
        assert_eq!(send(id, h.gc, b"allocationBytes", &[]).as_word(), Some(0));
        send(id, h.gc, b"budget:", &[Operand::word(1 << 20)]);
        send(id, system, b"autoGc:", &[Operand::object(falsity)]);
        let dead = allocate(id, named(b"ObjectVt"), 0);
        collect(id, h.gc);
        assert!(!present(id, dead));
        // Finalizers may set policy too, without allowing recursive collection.
        let owner_b = h.keep(behavior(id));
        let owner = h.keep(allocate(id, owner_b, 0));
        let mut f = Finalizer {
            id,
            gc: h.gc,
            calls: Cell::new(0),
        };
        install(id, owner_b, b"finalize:", finalize, (&raw mut f).cast());
        let dead = allocate(id, named(b"ObjectVt"), 0);
        send(
            id,
            h.gc,
            b"finalize:with:",
            &[Operand::object(dead), Operand::object(owner)],
        );
        h.mode(false);
        collect(id, h.gc);
        assert_eq!(f.calls.get(), 1);
        assert!(!present(id, dead));
        assert_eq!(send(id, h.gc, b"precisePayloads", &[]).as_word(), Some(1));
        send(
            id,
            owner_b,
            b"removeMethod:",
            &[Operand::object(id.intern_name(b"finalize:").unwrap())],
        );
        // Reload preserves installed replacement methods and current settings.
        let system_vt = named(b"SystemVt");
        let original = binding(id, system_vt, b"gc");
        install(id, system_vt, b"gc", constant_answer, sentinel);
        let path = directory().join("libbirnam_system_gc.dylib");
        assert_eq!(
            id.send(
                id.receiver,
                id.load,
                &[
                    Operand::bytes(path.as_os_str().as_encoded_bytes()),
                    Operand::object(ns)
                ]
            )
            .unwrap()
            .kind(),
            native::UNIT
        );
        assert_eq!(send(id, h.gc, b"precisePayloads", &[]).as_word(), Some(1));
        assert_eq!(send(id, system, b"gc", &[]).as_object(), Some(sentinel));
        bind(id, system_vt, b"gc", original);
        let source = h.keep(object(send(
            id,
            named(b"String"),
            b"fromBytes:",
            &[Operand::bytes(b"[System preciseGc: false]")],
        )));
        assert_eq!(
            send(id, named(b"Codegen"), b"eval:", &[Operand::object(source)]).as_object(),
            Some(system)
        );
        assert_eq!(send(id, h.gc, b"precisePayloads", &[]).as_word(), Some(0));
        for token in h.tokens.take() {
            release(id, h.roots, token);
        }
    }
}
