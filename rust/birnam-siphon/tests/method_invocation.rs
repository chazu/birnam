//! Default authored construction policy, live override seams and transient templates.
mod support;
use birnam_abi::{
    Oop,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
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
    unsafe fn keep(&self, v: Oop) -> Oop {
        self.tokens
            .borrow_mut()
            .push(unsafe { retain(self.id, self.roots, v) });
        v
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
    unsafe fn number(&self, n: Oop) -> usize {
        unsafe {
            send(
                self.id,
                self.named(b"Integer"),
                b"wordOf:",
                &[Operand::object(n)],
            )
            .as_word()
            .unwrap()
        }
    }
    unsafe fn text(&self, b: &[u8]) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                self.named(b"String"),
                b"fromBytes:",
                &[Operand::bytes(b)],
            )))
        }
    }
    unsafe fn eval(&self, b: &[u8]) -> Oop {
        unsafe {
            let result = self.call(self.named(b"Codegen"), b"eval:", &[self.text(b)]);
            assert_eq!(self.call(result, b"isError", &[]), self.named(b"false"));
            result
        }
    }
    unsafe fn clear(&self, mark: usize) {
        for t in self.tokens.borrow_mut().drain(mark..).rev() {
            unsafe { release(self.id, self.roots, t) }
        }
    }
}

unsafe fn units(h: &H) -> usize {
    unsafe {
        send(h.id, h.named(b"TCC"), b"liveNativeUnits", &[])
            .as_word()
            .unwrap()
    }
}
unsafe fn collect_units(h: &H) {
    unsafe {
        collect(h.id, h.named(b"GC"));
        send(h.id, h.named(b"TCC"), b"checkpoint", &[]);
    }
}
unsafe fn lookup(h: &H, receiver: Oop, selector: &[u8]) -> NativeEntry {
    unsafe {
        // Keep source-backed entries alive while their dictionary binding is
        // temporarily replaced and the callback collects.
        h.call(
            receiver,
            b"methodFor:",
            &[h.keep(h.id.intern_name(selector).unwrap())],
        );
        send(
            h.id,
            h.id.receiver,
            b"bind:to:",
            &[
                Operand::object(h.id.intern_name(selector).unwrap()),
                Operand::object(receiver),
            ],
        )
        .as_entry()
        .unwrap()
    }
}
unsafe fn replace(h: &H, receiver: Oop, selector: &[u8], entry: NativeEntry) {
    unsafe {
        let vt = h.call(h.id.receiver, b"ensurePrivateVtableFor:", &[receiver]);
        install(h.id, vt, selector, entry.method.unwrap(), entry.context);
    }
}
struct Probe {
    id: Protocol,
    gc: Oop,
    answer: Oop,
    calls: Cell<usize>,
    failure: bool,
}
unsafe extern "C" fn probe(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Probe>();
        p.calls.set(p.calls.get() + 1);
        collect(p.id, p.gc);
        if p.failure {
            Outcome::failed(881, 17)
        } else {
            Outcome::loaded(Operand::object(p.answer))
        }
    }
}

struct Reentry {
    id: Protocol,
    original: NativeEntry,
    reflect: Oop,
    target: Oop,
    selector: Oop,
    body: Oop,
    gc: Oop,
    calls: Cell<usize>,
}
unsafe extern "C" fn reenter(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Reentry>();
        let call = p.calls.get();
        p.calls.set(call + 1);
        if call == 0 {
            // Only the outer compile deliberately reenters; the nested compile
            // forwards normally. This is test instrumentation, not a runtime guard.
            let nested = p.id.send_named(
                p.reflect,
                b"defineMethod:on:as:",
                &[
                    Operand::object(p.selector),
                    Operand::object(p.target),
                    Operand::object(p.body),
                ],
            );
            if let Err(error) = nested {
                return error;
            }
            collect(p.id, p.gc);
        }
        p.original.method.unwrap()(p.original.context, r, a, n)
    }
}

#[test]
#[ignore = "production modules"]
fn default_construction_shares_templates_and_observes_current_policy() {
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
        let baseline = std::env::var_os("BIRNAM_METHOD_INVOCATION_BASELINE").is_some();
        let factory = if baseline {
            h.named(b"nil")
        } else {
            h.named(b"MethodInvocation")
        };
        if !baseline {
            h.call(factory, b"install", &[]);
        }
        let target = h.call(h.named(b"Object"), b"clone", &[]);
        let body = h.eval(b"[:self :argument | argument]");
        let selector = h.keep(id.intern_name(b"sharedProbe:").unwrap());
        if !baseline {
            h.call(factory, b"clearTemplates", &[]);
        }
        collect_units(&h);
        let binding_basis = h.number(h.call(h.named(b"Globals"), b"bindingBasis", &[]));
        let before = units(&h);
        let before_bytes = send(id, h.named(b"TCC"), b"liveNativeBytes", &[]).as_word();
        let mark = h.tokens.borrow().len();
        let started = std::time::Instant::now();
        for _ in 0..16 {
            assert_eq!(
                h.call(
                    h.named(b"Reflect"),
                    b"defineMethod:on:as:",
                    &[selector, target, body]
                ),
                target
            );
            h.call(target, b"methodFor:", &[selector]); // retain all sixteen logical bindings
        }
        let construction_ns = started.elapsed().as_nanos();
        collect_units(&h);
        let added = units(&h) - before;
        let after_bytes = send(id, h.named(b"TCC"), b"liveNativeBytes", &[]).as_word();
        if let (Some(before_bytes), Some(after_bytes)) = (before_bytes, after_bytes) {
            eprintln!(
                "default code footprint: baseline={baseline}, resident_units={before}, resident_bytes={before_bytes}, added_bytes={}",
                after_bytes - before_bytes
            );
        }

        assert_eq!(added, if baseline { 16 } else { 1 });
        assert_eq!(
            h.call(target, b"sharedProbe:", &[h.named(b"true")]),
            h.named(b"true")
        );
        eprintln!(
            "default construction: baseline={baseline}, definitions=16, added_tcc_units={added}, elapsed_ns={construction_ns}"
        );
        let argument = h.named(b"true");
        for _ in 0..100 {
            send(id, target, b"sharedProbe:", &[Operand::object(argument)]);
        }
        let started = std::time::Instant::now();
        for _ in 0..10000 {
            assert_eq!(
                send(id, target, b"sharedProbe:", &[Operand::object(argument)]).as_object(),
                Some(argument)
            );
        }
        eprintln!(
            "default invocation: baseline={baseline}, calls=10000, elapsed_ns={}",
            started.elapsed().as_nanos()
        );

        h.call(
            h.named(b"Reflect"),
            b"removeMethod:on:",
            &[selector, target],
        );
        h.clear(mark);
        if !baseline {
            h.call(factory, b"clearTemplates", &[]);
        }
        collect_units(&h);
        assert_eq!(
            units(&h),
            before,
            "removed bindings or cleared templates retained code"
        );
        assert_eq!(
            h.number(h.call(h.named(b"Globals"), b"bindingBasis", &[])),
            binding_basis,
            "derived method templates changed the binding revision"
        );
        if baseline {
            return;
        }

        let error = h.call(
            h.named(b"Error"),
            b"kind:",
            &[h.keep(id.intern_name(b"templateRejected").unwrap())],
        );
        let mut p = Box::new(Probe {
            id,
            gc: h.named(b"GC"),
            answer: error,
            calls: Cell::new(0),
            failure: false,
        });
        let entry = NativeEntry::new(probe, (&mut *p as *mut Probe).cast());
        let backend = h.named(b"NativeBackend");
        let original = lookup(&h, backend, b"named:synchronous:");
        replace(&h, backend, b"named:synchronous:", entry);
        assert_eq!(
            h.call(
                h.named(b"Reflect"),
                b"defineMethod:on:as:",
                &[selector, target, body]
            ),
            error
        );
        assert_eq!(p.calls.get(), 1);
        assert_eq!(h.call(target, b"methodFor:", &[selector]), h.named(b"nil"));
        assert_eq!(
            h.number(h.call(h.call(factory, b"templates", &[]), b"size", &[])),
            0
        );
        p.failure = true;
        let failed = h.call(
            h.named(b"Reflect"),
            b"defineMethod:on:as:",
            &[selector, target, body],
        );
        assert_eq!(
            h.call(failed, b"kind", &[]),
            id.intern_name(b"nativeFailure").unwrap()
        );
        assert_eq!(p.calls.get(), 2);
        replace(&h, backend, b"named:synchronous:", original);

        // TCC can return an ordinary Error too. initializeWith: then follows
        // that Error's ordinary protocol, preserving identity without a guard.
        let tcc = h.named(b"TCC");
        let compile = lookup(&h, tcc, b"compile:");
        p.failure = false;
        replace(&h, tcc, b"compile:", entry);
        assert_eq!(
            h.call(
                h.named(b"Reflect"),
                b"defineMethod:on:as:",
                &[selector, target, body]
            ),
            error
        );
        assert_eq!(p.calls.get(), 3);
        replace(&h, tcc, b"compile:", compile);

        // The public factory send is live, including replacement and GC.
        let original_factory = lookup(&h, factory, b"methodForArity:implementation:");
        p.failure = false;
        replace(&h, factory, b"methodForArity:implementation:", entry);
        assert_eq!(
            h.call(
                h.named(b"Reflect"),
                b"defineMethod:on:as:",
                &[selector, target, body]
            ),
            error
        );
        assert_eq!(p.calls.get(), 4);
        replace(
            &h,
            factory,
            b"methodForArity:implementation:",
            original_factory,
        );

        // Reusing selected code does not freeze the template's constructor send.
        let template = h.call(factory, b"templateForArity:", &[h.integer(1)]);
        let constructor = lookup(&h, template, b"withImplementation:");
        replace(&h, template, b"withImplementation:", entry);
        assert_eq!(
            h.call(
                h.named(b"Reflect"),
                b"defineMethod:on:as:",
                &[selector, target, body]
            ),
            error
        );
        assert_eq!(p.calls.get(), 5);
        replace(&h, template, b"withImplementation:", constructor);
        h.call(factory, b"clearTemplates", &[]);
        eprintln!(
            "factory policy: Error identity, native failure conversion, no publication/replay, live owner and template overrides passed"
        );
        let nested_target = h.call(h.named(b"Object"), b"clone", &[]);
        let nested_selector = h.keep(id.intern_name(b"nestedProbe").unwrap());
        let nested_body = h.eval(b"[:self | self]");
        let mut nested = Box::new(Reentry {
            id,
            original,
            reflect: h.named(b"Reflect"),
            target: nested_target,
            selector: nested_selector,
            body: nested_body,
            gc: h.named(b"GC"),
            calls: Cell::new(0),
        });
        replace(
            &h,
            backend,
            b"named:synchronous:",
            NativeEntry::new(reenter, (&mut *nested as *mut Reentry).cast()),
        );
        assert_eq!(
            h.call(
                h.named(b"Reflect"),
                b"defineMethod:on:as:",
                &[selector, target, body]
            ),
            target
        );
        assert_eq!(nested.calls.get(), 2, "compiler callback was replayed");
        assert_eq!(
            h.number(h.call(h.call(factory, b"templates", &[]), b"size", &[])),
            2,
            "outer publication lost the nested template"
        );
        assert_eq!(h.call(nested_target, b"nestedProbe", &[]), nested_target);
        replace(&h, backend, b"named:synchronous:", original);
        h.call(factory, b"clearTemplates", &[]);
        assert_eq!(
            h.call(target, b"sharedProbe:", &[h.named(b"true")]),
            h.named(b"true")
        );
        assert_eq!(h.call(nested_target, b"nestedProbe", &[]), nested_target);
        eprintln!(
            "factory reentry: distinct arities survive, no replay; clearing cache preserves installed bindings"
        );
        // A nested update can also occur after the cache snapshot was read.
        // Decline only the cache write; do not lose the nested entry or replay.
        let model = h.named(b"ObjectModel");
        let compare = lookup(&h, model, b"replaceSlot:in:expected:with:");
        let mut late = Box::new(Reentry {
            id,
            original: compare,
            reflect: h.named(b"Reflect"),
            target: nested_target,
            selector: nested_selector,
            body: nested_body,
            gc: h.named(b"GC"),
            calls: Cell::new(0),
        });
        replace(
            &h,
            model,
            b"replaceSlot:in:expected:with:",
            NativeEntry::new(reenter, (&mut *late as *mut Reentry).cast()),
        );
        assert_eq!(
            h.call(
                h.named(b"Reflect"),
                b"defineMethod:on:as:",
                &[selector, target, body]
            ),
            target
        );
        replace(&h, model, b"replaceSlot:in:expected:with:", compare);
        assert_eq!(late.calls.get(), 2, "cache publication was retried");
        let cache = h.call(factory, b"templates", &[]);
        assert_eq!(h.number(h.call(cache, b"size", &[])), 1);
        assert_eq!(h.call(cache, b"at:", &[h.integer(1)]), h.named(b"nil"));
        assert_ne!(h.call(cache, b"at:", &[h.integer(0)]), h.named(b"nil"));
        assert_eq!(
            h.call(target, b"sharedProbe:", &[h.named(b"true")]),
            h.named(b"true")
        );
        h.call(factory, b"clearTemplates", &[]);

        // Constructing a value must not invalidate a prepared generation merely
        // by warming its private cache. These are the actual owner revisions.
        let globals = h.number(h.call(h.named(b"Globals"), b"bindingBasis", &[]));
        let facts = h.number(h.call(h.named(b"db"), b"basis", &[]));
        let epoch = send(id, id.receiver, b"bindingEpoch", &[])
            .as_word()
            .unwrap();
        let method = h.call(
            h.named(b"Reflect"),
            b"methodForArity:implementation:",
            &[h.integer(1), body],
        );
        assert_eq!(h.call(method, b"isError", &[]), h.named(b"false"));
        h.call(factory, b"clearTemplates", &[]);
        assert_eq!(
            h.number(h.call(h.named(b"Globals"), b"bindingBasis", &[])),
            globals
        );
        assert_eq!(h.number(h.call(h.named(b"db"), b"basis", &[])), facts);
        assert_eq!(
            send(id, id.receiver, b"bindingEpoch", &[]).as_word(),
            Some(epoch)
        );
        eprintln!(
            "owned cache: late update preserved without retry; construction/clear leave Globals, Store and Id binding revisions unchanged"
        );
        // Reflect is a facade protocol, not a singleton-only dispatch target.
        // A delegated facade's constructor override must receive its own send.
        let facade = h.call(h.named(b"Reflect"), b"clone", &[]);
        let facade_target = h.call(h.named(b"Object"), b"clone", &[]);
        let facade_selector = h.keep(id.intern_name(b"facadeProbe:").unwrap());
        let facade_constructor = lookup(&h, facade, b"methodForArity:implementation:");
        replace(&h, facade, b"methodForArity:implementation:", entry);
        assert_eq!(
            h.call(
                facade,
                b"defineMethod:on:as:",
                &[facade_selector, facade_target, body]
            ),
            error
        );
        assert_eq!(
            h.call(facade_target, b"methodFor:", &[facade_selector]),
            h.named(b"nil")
        );
        assert_eq!(
            h.call(
                h.named(b"Reflect"),
                b"defineMethod:on:as:",
                &[facade_selector, facade_target, body]
            ),
            facade_target
        );
        replace(
            &h,
            facade,
            b"methodForArity:implementation:",
            facade_constructor,
        );
        eprintln!(
            "Reflect facade: delegated constructor override is live and isolated from the canonical facade"
        );
    }
}
