//! Bootstrap loop semantics and replacement by the existing authored controls.
mod support;
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use core::{
    cell::{Cell, RefCell},
    ffi::c_void,
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
    unsafe fn call(&self, r: Oop, name: &[u8], args: &[Oop]) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                r,
                name,
                &args
                    .iter()
                    .copied()
                    .map(Operand::object)
                    .collect::<Vec<_>>(),
            )))
        }
    }
    unsafe fn eval(&self, b: &[u8]) -> Oop {
        unsafe { self.call(self.named(b"Codegen"), b"eval:", &[self.string(b)]) }
    }
    unsafe fn number(&self, o: Oop) -> usize {
        unsafe {
            send(
                self.id,
                self.named(b"Integer"),
                b"wordOf:",
                &[Operand::object(o)],
            )
            .as_word()
            .unwrap()
        }
    }
    unsafe fn bind(&self, name: &[u8], o: Oop) {
        unsafe {
            send(
                self.id,
                self.ns,
                b"at:put:",
                &[
                    Operand::object(self.id.intern_name(name).unwrap()),
                    Operand::object(o),
                ],
            );
        }
    }
    unsafe fn bytes(&self, o: Oop) -> Vec<u8> {
        unsafe {
            let b = send(self.id, o, b"bytes", &[]).as_bytes().unwrap();
            borrowed_bytes(b.pointer, b.length).unwrap().to_vec()
        }
    }
    unsafe fn error(&self, kind: &[u8]) -> Oop {
        unsafe {
            self.call(
                self.named(b"Error"),
                b"kind:",
                &[self.keep(self.id.intern_name(kind).unwrap())],
            )
        }
    }
}
unsafe fn binding(h: &H, vt: Oop, name: &[u8]) -> NativeEntry {
    unsafe {
        send(
            h.id,
            h.id.receiver,
            b"bindingOf:in:",
            &[
                Operand::object(h.id.intern_name(name).unwrap()),
                Operand::object(vt),
            ],
        )
        .as_entry()
        .unwrap()
    }
}
unsafe fn set(h: &H, vt: Oop, name: &[u8], entry: NativeEntry) {
    unsafe {
        send(
            h.id,
            vt,
            b"addMethod",
            &[
                Operand::object(h.id.intern_name(name).unwrap()),
                Operand::entry(entry),
            ],
        );
    }
}
unsafe fn behavior_of(h: &H, value: Oop) -> Oop {
    unsafe {
        object(send(
            h.id,
            h.id.receiver,
            b"behaviorOf:",
            &[Operand::object(value)],
        ))
    }
}
struct Counter {
    id: Protocol,
    gc: Oop,
    truth: Oop,
    falsity: Oop,
    nil: Oop,
    calls: Cell<usize>,
    conditions: Cell<usize>,
    limit: Cell<usize>,
    inverted: Cell<bool>,
    condition_result: Cell<Option<Oop>>,
    body_result: Cell<Option<Oop>>,
    failure: Cell<usize>,
    collecting: Cell<bool>,
}
unsafe extern "C" fn condition(c: *mut c_void, _: Oop, _: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let c = &*c.cast::<Counter>();
        if n != 0 {
            return Err(Outcome::failed(1, 0));
        }
        c.conditions.set(c.conditions.get() + 1);
        if c.collecting.get() {
            c.id.send_named(c.gc, b"collectPrecisely", &[])?;
        }
        if c.failure.get() == 1 {
            return Err(Outcome::failed(881, 17));
        }
        Ok(Operand::object(c.condition_result.get().unwrap_or(
            if (c.calls.get() < c.limit.get()) != c.inverted.get() {
                c.truth
            } else {
                c.falsity
            },
        )))
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
unsafe extern "C" fn body(c: *mut c_void, _: Oop, _: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let c = &*c.cast::<Counter>();
        if n != 0 {
            return Err(Outcome::failed(1, 0));
        }
        c.calls.set(c.calls.get() + 1);
        if c.collecting.get() {
            c.id.send_named(c.gc, b"collectPrecisely", &[])?;
        }
        if c.failure.get() == 2 {
            return Err(Outcome::failed(882, 19));
        }
        Ok(Operand::object(c.body_result.get().unwrap_or(c.nil)))
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
#[test]
#[ignore = "requires production native modules"]
fn bootstrap_loops_use_live_sends_and_preserve_control_boundaries() {
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
        let block_vt = behavior_of(&h, h.named(b"Block"));
        assert_eq!(
            object(send(
                id,
                id.receiver,
                b"ownerOf:",
                &[Operand::object(block_vt)]
            )),
            h.named(b"Block")
        );
        let while_true = binding(&h, block_vt, b"whileTrue:");
        let while_false = binding(&h, block_vt, b"whileFalse:");
        let c = Counter {
            id,
            gc: h.gc,
            truth: h.named(b"true"),
            falsity: h.named(b"false"),
            nil: h.named(b"nil"),
            calls: Cell::new(0),
            conditions: Cell::new(0),
            limit: Cell::new(10_000),
            inverted: Cell::new(false),
            condition_result: Cell::new(None),
            body_result: Cell::new(None),
            failure: Cell::new(0),
            collecting: Cell::new(false),
        };
        // Aliases on non-Blocks preserve the receiver/body duck typing contract.
        let cvt = h.keep(behavior(id));
        let bvt = h.keep(behavior(id));
        install(
            id,
            cvt,
            b"value",
            condition,
            (&c as *const Counter).cast_mut().cast(),
        );
        install(
            id,
            bvt,
            b"value",
            body,
            (&c as *const Counter).cast_mut().cast(),
        );
        set(&h, cvt, b"loop:", while_true);
        let cond = h.keep(allocate(id, cvt, 0));
        let action = h.keep(allocate(id, bvt, 0));
        assert_eq!(h.call(cond, b"loop:", &[action]), c.nil);
        assert_eq!((c.conditions.get(), c.calls.get()), (10_001, 10_000));
        assert_eq!(h.call(cond, b"loop:", &[]), c.nil);
        assert_eq!(h.call(cond, b"loop:", &[action, action]), c.nil);
        assert_eq!(
            h.call(cond, b"loop:", &[c.nil]),
            c.nil,
            "false-first does not inspect body"
        );
        c.inverted.set(true);
        c.calls.set(0);
        c.conditions.set(0);
        c.limit.set(3);
        set(&h, cvt, b"loop:", while_false);
        assert_eq!(h.call(cond, b"loop:", &[action]), c.nil);
        assert_eq!((c.conditions.get(), c.calls.get()), (4, 3));
        c.inverted.set(false);
        set(&h, cvt, b"loop:", while_true);
        c.collecting.set(true);
        let err = h.error(b"testLoop");
        c.condition_result.set(Some(err));
        c.calls.set(0);
        assert_eq!(h.call(cond, b"loop:", &[action]), err);
        assert_eq!(c.calls.get(), 0);
        c.condition_result.set(Some(c.truth));
        c.body_result.set(Some(err));
        assert_eq!(h.call(cond, b"loop:", &[action]), err);
        assert_eq!(c.calls.get(), 1);
        c.condition_result.set(Some(c.nil));
        let error = h.call(cond, b"loop:", &[action]);
        assert_eq!(
            h.call(error, b"kind", &[]),
            id.intern_name(b"mustBeBoolean").unwrap()
        );
        assert_eq!(
            h.bytes(h.call(error, b"message", &[])),
            b"while loop condition must answer true or false"
        );
        c.condition_result.set(Some(c.truth));
        c.body_result.set(None);
        for (which, status, detail) in [(1, 881, 17), (2, 882, 19)] {
            c.failure.set(which);
            let error = id
                .send_named(cond, b"loop:", &[Operand::object(action)])
                .unwrap_err();
            assert_eq!((error.status, error.detail), (status, detail));
        }
        // Failure passthrough is scoped to the bootstrap loop. Ordinary source
        // evaluation must retain Execution's normal language-error conversion.
        h.bind(b"LoopFailureCondition", cond);
        c.failure.set(1);
        let ordinary = h.eval(b"[LoopFailureCondition value]");
        assert_eq!(h.call(ordinary, b"kind", &[]), id.intern_name(b"nativeFailure").unwrap());
        h.bind(b"LoopFailureCondition", c.nil);
        c.failure.set(0);
        c.condition_result.set(None);
        c.calls.set(0);
        c.limit.set(3);
        assert_eq!(h.call(cond, b"loop:", &[action]), c.nil);
        // A direct native caller need not pin the input Objects itself.
        let transient_cond = allocate(id, cvt, 0);
        let token = retain(id, h.roots, transient_cond);
        let transient_body = allocate(id, bvt, 0);
        release(id, h.roots, token);
        c.calls.set(0);
        assert_eq!(
            object(send(
                id,
                transient_cond,
                b"loop:",
                &[Operand::object(transient_body)]
            )),
            c.nil
        );
        collect(id, h.gc);
        assert!(!present(id, transient_cond));
        assert!(!present(id, transient_body));
        transient_results(&h, cond, action, bvt, &c);
        compiled(&h);
        replacements(&h, cond, action, cvt, bvt, &c);
        control(&h);
    }
}
unsafe fn compiled(h: &H) {
    unsafe {
        let value = h.eval(b"let n := [Cell with: 0]; [n set: 0]; [[| [[n value] < 100]] whileTrue: [| [n set: [[n value] + 1]]]]; [n value]");
        assert_eq!(h.number(value), 100);
        let value = h.eval(b"let m := [Cell with: 0]; [m set: 0]; [[| [[m value] >= 7]] whileFalse: [| [System gc]; [m set: [[m value] + 1]]]]; [m value]");
        assert_eq!(h.number(value), 7);
        let error = h.eval(b"[[| 42] whileTrue: [| nil]]");
        assert_eq!(
            h.call(error, b"kind", &[]),
            h.id.intern_name(b"mustBeBoolean").unwrap()
        );
        let error = h.eval(b"[[| true] whileTrue: [| [Error kind: #bodyError]]]");
        assert_eq!(
            h.call(error, b"kind", &[]),
            h.id.intern_name(b"bodyError").unwrap()
        );
    }
}
struct Reply {
    id: Protocol,
    gc: Oop,
    value: Operand,
    calls: Cell<usize>,
}
unsafe extern "C" fn reply(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    (|| unsafe {
        let c = &*c.cast::<Reply>();
        c.calls.set(c.calls.get() + 1);
        c.id.send_named(c.gc, b"collectPrecisely", &[])?;
        Ok(c.value)
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
struct Mutate {
    id: Protocol,
    gc: Oop,
    target: Oop,
    entry: NativeEntry,
    result: Oop,
    calls: Cell<usize>,
}
unsafe extern "C" fn mutate(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    (|| unsafe {
        let m = &*c.cast::<Mutate>();
        m.calls.set(m.calls.get() + 1);
        m.id.send_named(
            m.target,
            b"addMethod",
            &[
                Operand::object(m.id.intern_name(b"value")?),
                Operand::entry(m.entry),
            ],
        )?;
        m.id.send_named(m.gc, b"collectPrecisely", &[])?;
        Ok(Operand::object(m.result))
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
unsafe fn replacements(h: &H, cond: Oop, action: Oop, cvt: Oop, bvt: Oop, c: &Counter) {
    unsafe {
        let id = h.id;
        let original_condition = binding(h, cvt, b"value");
        let original_body = binding(h, bvt, b"value");
        let stop = Reply {
            id,
            gc: h.gc,
            value: Operand::object(c.falsity),
            calls: Cell::new(0),
        };
        let stop_entry = NativeEntry::new(reply, (&stop as *const Reply).cast_mut().cast());
        let m = Mutate {
            id,
            gc: h.gc,
            target: cvt,
            entry: stop_entry,
            result: c.nil,
            calls: Cell::new(0),
        };
        install(
            id,
            bvt,
            b"value",
            mutate,
            (&m as *const Mutate).cast_mut().cast(),
        );
        c.calls.set(0);
        c.limit.set(1);
        assert_eq!(h.call(cond, b"loop:", &[action]), c.nil);
        assert_eq!(
            (m.calls.get(), stop.calls.get()),
            (1, 1),
            "condition reselected after body mutation"
        );
        set(h, cvt, b"value", original_condition);
        set(h, bvt, b"value", original_body);
        let returned_error = h.error(b"replacedBody");
        let answer = Reply {
            id,
            gc: h.gc,
            value: Operand::object(returned_error),
            calls: Cell::new(0),
        };
        let m = Mutate {
            id,
            gc: h.gc,
            target: bvt,
            entry: NativeEntry::new(reply, (&answer as *const Reply).cast_mut().cast()),
            result: c.truth,
            calls: Cell::new(0),
        };
        install(
            id,
            cvt,
            b"value",
            mutate,
            (&m as *const Mutate).cast_mut().cast(),
        );
        assert_eq!(h.call(cond, b"loop:", &[action]), returned_error);
        assert_eq!(
            (m.calls.get(), answer.calls.get()),
            (1, 1),
            "body selected after condition mutation"
        );
        set(h, cvt, b"value", original_condition);
        set(h, bvt, b"value", original_body);
        // Keep exact-family error recognition; spoofing isError changes neither
        // invalid-condition handling nor the treatment of an ordinary body value.
        let spoof_vt = h.keep(behavior(id));
        let truth = Reply {
            id,
            gc: h.gc,
            value: Operand::object(c.truth),
            calls: Cell::new(0),
        };
        install(
            id,
            spoof_vt,
            b"isError",
            reply,
            (&truth as *const Reply).cast_mut().cast(),
        );
        let spoof = h.keep(allocate(id, spoof_vt, 0));
        c.condition_result.set(Some(spoof));
        assert_eq!(
            h.call(h.call(cond, b"loop:", &[action]), b"kind", &[]),
            id.intern_name(b"mustBeBoolean").unwrap()
        );
        c.condition_result.set(None);
        c.body_result.set(Some(spoof));
        c.calls.set(0);
        assert_eq!(h.call(cond, b"loop:", &[action]), c.nil);
        assert_eq!(truth.calls.get(), 0);
        c.body_result.set(None);
        c.condition_result.set(Some(c.nil));
        let error_factory = h.named(b"Error");
        h.bind(b"Error", c.nil);
        assert_eq!(
            h.call(cond, b"loop:", &[action]),
            c.nil,
            "lazy missing Error factory"
        );
        h.bind(b"Error", error_factory);
        let error_vt = behavior_of(h, error_factory);
        let original_error = binding(h, error_vt, b"kind:message:");
        // A source callable's context is collectible after replacement. Retain
        // its Method while keeping the raw entry for later restoration.
        let original_method=send(id,h.named(b"Execution"),b"methodOfEntry:",&[Operand::entry(original_error)]);
        h.keep(object(original_method));
        let answer = Reply {
            id,
            gc: h.gc,
            value: Operand::object(returned_error),
            calls: Cell::new(0),
        };
        install(
            id,
            error_vt,
            b"kind:message:",
            reply,
            (&answer as *const Reply).cast_mut().cast(),
        );
        assert_eq!(h.call(cond, b"loop:", &[action]), returned_error);
        let answer = Reply {
            value: Operand::unit(),
            ..answer
        };
        install(
            id,
            error_vt,
            b"kind:message:",
            reply,
            (&answer as *const Reply).cast_mut().cast(),
        );
        // Native Unit from a replaced factory is language nil, as in ordinary
        // compiled sends. The authored method returns that language value.
        assert_eq!(h.call(cond, b"loop:", &[action]), c.nil);
        set(h, error_vt, b"kind:message:", original_error);
        c.condition_result.set(None);
    }
}
unsafe fn control(h: &H) {
    unsafe {
        let id = h.id;
        let machine = h.call(h.named(b"Machine"), b"new", &[]);
        h.bind(b"LoopMachine", machine);
        // Synchronous bootstrap loops retain the existing native barrier.
        let blocked = h.eval(b"[| [[| true] whileTrue: [| [LoopMachine park]]]]");
        let failure = id
            .send_named(machine, b"runBlock:", &[Operand::object(blocked)])
            .unwrap_err();
        assert_eq!(failure.status, birnam_abi::id_protocol::INVALID_REQUEST);
        assert_eq!(
            h.call(machine, b"executionState", &[]),
            id.intern_name(b"failed").unwrap()
        );
        h.call(machine, b"destroy", &[]);
        let old_condition = h.eval(b"[| [LoopMachine park]; false]");
        h.bind(b"OldLoopCondition", old_condition);
        // Install the existing authored Boolean/while methods, without importing
        // the unrelated scheduler/collection policy later in that source unit.
        let authored = include_str!("../../../modules/stackless-control.birnam");
        let start = authored.find("// Boolean is").unwrap();
        let end = authored.find("// Integer iteration").unwrap();
        let result = h.eval(authored[start..end].as_bytes());
        assert_ne!(h.call(result, b"isError", &[]), h.named(b"true"));
        let vt = behavior_of(h, h.named(b"Block"));
        let installed = binding(h, vt, b"whileTrue:");
        h.eval(b"[Module load: #blockLoop]");
        let reloaded = binding(h, vt, b"whileTrue:");
        assert_eq!(installed.context, reloaded.context);
        assert_eq!(
            installed.method.map(|f| f as usize),
            reloaded.method.map(|f| f as usize)
        );
        let machine = h.call(h.named(b"Machine"), b"new", &[]);
        h.bind(b"LoopMachine", machine);
        let caller = h.eval(b"[| [OldLoopCondition whileTrue: [| nil]]]");
        assert_eq!(h.call(machine, b"runBlock:", &[caller]), machine);
        collect(id, h.gc);
        assert_eq!(
            h.call(machine, b"resumeWith:", &[h.named(b"nil")]),
            h.named(b"nil")
        );
        let machine = h.call(h.named(b"Machine"), b"new", &[]);
        h.bind(b"LoopMachine", machine);
        let block = h.eval(b"[| let state := [Cell with: 0]; [state set: 0]; [[| [[state value] < 2]] whileTrue: [| [LoopMachine park]; [state set: [[state value] + 1]]]]; [state value]]");
        assert_eq!(h.call(machine, b"runBlock:", &[block]), machine);
        collect(id, h.gc);
        assert_eq!(h.call(machine, b"resumeWith:", &[h.named(b"nil")]), machine);
        collect(id, h.gc);
        assert_eq!(
            h.number(h.call(machine, b"resumeWith:", &[h.named(b"nil")])),
            2
        );
    }
}

struct Fresh {
    counter: *const Counter,
    prototype: Oop,
    previous: Cell<Oop>,
}
unsafe extern "C" fn fresh(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    (|| unsafe {
        let f = &*c.cast::<Fresh>();
        let c = &*f.counter;
        c.id.send_named(c.gc, b"collectPrecisely", &[])?;
        let previous = f.previous.get();
        if !previous.is_null()
            && c.id
                .send_named(
                    c.id.receiver,
                    b"allocationAt:",
                    &[Operand::word(previous.addr())],
                )?
                .as_object()
                .is_some()
        {
            return Err(Outcome::failed(984, c.calls.get()));
        }
        c.calls.set(c.calls.get() + 1);
        let value = c.id.send_named(f.prototype, b"clone", &[])?;
        f.previous.set(object(value));
        Ok(value)
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
struct NamespaceHook {
    id: Protocol,
    gc: Oop,
    alias: Oop,
    key: Oop,
}
unsafe extern "C" fn namespace_at(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let f = &*c.cast::<NamespaceHook>();
        let args = borrowed_slice(a, n).unwrap();
        if args.first().and_then(|v| v.as_object()) == Some(f.key) {
            f.id.send_named(f.gc, b"collectPrecisely", &[])?;
        }
        f.id.send(r, f.alias, args)
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
unsafe fn transient_results(h: &H, cond: Oop, action: Oop, bvt: Oop, c: &Counter) {
    unsafe {
        let fresh_context = Fresh {
            counter: c,
            prototype: h.named(b"Object"),
            previous: Cell::new(core::ptr::null_mut()),
        };
        let original = binding(h, bvt, b"value");
        install(
            h.id,
            bvt,
            b"value",
            fresh,
            (&fresh_context as *const Fresh).cast_mut().cast(),
        );
        let ns_vt = behavior_of(h, h.ns);
        let ns_original = binding(h, ns_vt, b"at:");
        let alias = h.keep(h.id.intern_name(b"loopOriginalAt:").unwrap());
        set(h, ns_vt, b"loopOriginalAt:", ns_original);
        let hook = NamespaceHook {
            id: h.id,
            gc: h.gc,
            alias,
            key: h.keep(h.id.intern_name(b"ErrorVt").unwrap()),
        };
        install(
            h.id,
            ns_vt,
            b"at:",
            namespace_at,
            (&hook as *const NamespaceHook).cast_mut().cast(),
        );
        c.calls.set(0);
        c.limit.set(3);
        assert_eq!(h.call(cond, b"loop:", &[action]), c.nil);
        assert_eq!(c.calls.get(), 3);
        collect(h.id, h.gc);
        assert!(!present(h.id, fresh_context.previous.get()));
        set(h, ns_vt, b"at:", ns_original);
        set(h, bvt, b"value", original);
    }
}
