//! Qualify the owner-facing binding constructor against native and authored
//! shared step programs, including live invocation, control and code lifetime.
mod support;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use birnam_artifact_codec::{frame_descriptor as frame, stackless_selection as selection};
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
    unsafe fn bytes(&self, v: Oop) -> Vec<u8> {
        unsafe {
            let b = send(self.id, v, b"bytes", &[]).as_bytes().unwrap();
            birnam_abi::borrowed_bytes(b.pointer, b.length)
                .unwrap()
                .to_vec()
        }
    }
    unsafe fn eval(&self, b: &[u8]) -> Oop {
        unsafe {
            let result = self.call(self.named(b"Codegen"), b"eval:", &[self.text(b)]);
            assert_eq!(self.call(result, b"isError", &[]), self.named(b"false"));
            result
        }
    }
    unsafe fn vector(&self, x: &[Oop]) -> Oop {
        unsafe {
            let mut v = self.call(self.named(b"Vector"), b"new", &[]);
            for &x in x {
                v = self.call(v, b"add:", &[x]);
            }
            v
        }
    }
    unsafe fn clear(&self, mark: usize) {
        for t in self.tokens.borrow_mut().drain(mark..).rev() {
            unsafe { release(self.id, self.roots, t) }
        }
    }
}
fn array(source: &str, name: &str) -> Vec<u8> {
    let prefix = format!("static const unsigned char {name}[]={{");
    source
        .split(&prefix)
        .nth(1)
        .unwrap()
        .split("};")
        .next()
        .unwrap()
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().parse().unwrap())
        .collect()
}
struct Shape {
    descriptor: Vec<u8>,
    selection: Vec<u8>,
    meta: frame::Metadata,
    receiver_slot: usize,
    selector_slot: usize,
    argument_start: usize,
    selector: Vec<u8>,
}
impl Shape {
    unsafe fn new(h: &H, arity: usize) -> Self {
        unsafe {
            let source = h.call(
                h.named(b"NativeBackend"),
                b"named:synchronous:",
                &[h.integer(arity), h.named(b"false")],
            );
            let source = String::from_utf8(h.bytes(source)).unwrap();
            let descriptor = array(&source, "descriptor_0");
            let selected = array(&source, "selection_0");
            let inventory = selection::inspect_artifact_selection(&selected).unwrap();
            let row = inventory.rows().next().unwrap().unwrap();
            let path: Vec<_> = row.path.iter().collect();
            let authority = selection::SelectionSpec {
                source_revision: row.source_revision,
                root_kind: row.root_kind,
                root_index: row.root_index,
                path: &path,
                implementation_kind: row.implementation_kind,
                disposition: row.disposition,
            };
            let d = frame::inspect(&descriptor, authority).unwrap();
            let call = d.call_site(0).unwrap();
            assert_eq!(d.call_site_count(), 1);
            assert_eq!(call.kind, frame::CallKind::TailCall);
            Self {
                meta: d.metadata(),
                receiver_slot: call.receiver_slot as usize,
                selector_slot: call.selector_slot as usize,
                argument_start: call.argument_start as usize,
                selector: call.selector.to_vec(),
                descriptor,
                selection: selected,
            }
        }
    }
}
struct Shared {
    id: Protocol,
    ns: Oop,
    shape: Shape,
    calls: Cell<usize>,
    entry: Option<NativeEntry>,
    counter: Option<Oop>,
}
// Explicit temporary roots for this native test caller; no foreign payload reads.
unsafe extern "C" fn shared_step(
    c: *mut c_void,
    machine: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    unsafe {
        let p = &*c.cast::<Shared>();
        let roots = object(send(
            p.id,
            p.ns,
            b"at:",
            &[Operand::object(p.id.intern_name(b"Roots").unwrap())],
        ));
        let h = H {
            id: p.id,
            ns: p.ns,
            roots,
            tokens: RefCell::new(vec![]),
        };
        let result = (|| {
            let args = borrowed_slice(a, n).unwrap();
            assert_eq!(args.len(), 1);
            assert_eq!(h.number(object(args[0])), 0);
            p.calls.set(p.calls.get() + 1);
            // Read before writing: canonical slot coloring may overlap source/destination.
            let implementation = h.call(
                machine,
                b"oopAt:",
                &[h.integer(p.shape.meta.implementation_slot.unwrap() as usize)],
            );
            let receiver = h.call(
                machine,
                b"oopAt:",
                &[h.integer(p.shape.meta.entry_receiver_slot as usize)],
            );
            let mut args = vec![receiver];
            for i in 0..p.shape.meta.entry_argument_count as usize {
                args.push(h.call(
                    machine,
                    b"oopAt:",
                    &[h.integer(p.shape.meta.entry_argument_start as usize + i)],
                ));
            }
            for (i, v) in args.iter().enumerate() {
                h.call(
                    machine,
                    b"oopAt:put:",
                    &[h.integer(p.shape.argument_start + i), *v],
                );
            }
            h.call(
                machine,
                b"oopAt:put:",
                &[h.integer(p.shape.receiver_slot), implementation],
            );
            let selector = h.keep(p.id.intern_name(&p.shape.selector).unwrap());
            h.call(
                machine,
                b"oopAt:put:",
                &[h.integer(p.shape.selector_slot), selector],
            );
            p.id.send_named(machine, b"tailCallSite:", &[Operand::object(h.integer(0))])
        })();
        // Machine owns the returned action; its caller still owns Machine throughout.
        h.clear(0);
        result.map_or_else(|e| e, Outcome::loaded)
    }
}
unsafe fn call_count(h: &H, s: &Shared) -> usize {
    unsafe {
        match s.counter {
            Some(c) => h.number(h.call(c, b"value", &[])),
            None => s.calls.get(),
        }
    }
}
unsafe fn shared_method(h: &H, shared: &Shared, owner: Oop, implementation: Oop) -> Oop {
    unsafe {
        // Separate logical binding/generation ownership from physical shared code.
        let binding_owner = h.call(
            h.named(b"ObjectModel"),
            b"newSlots:parent:",
            &[h.vector(&[owner]), h.named(b"ObjectVt")],
        );
        let template = h.keep(object(send(
            h.id,
            h.named(b"Method"),
            b"fromStep:descriptor:selection:owner:implementation:",
            &[
                Operand::entry(shared.entry.unwrap_or_else(|| {
                    NativeEntry::new(shared_step, (shared as *const Shared).cast_mut().cast())
                })),
                Operand::bytes(&shared.shape.descriptor),
                Operand::bytes(&shared.shape.selection),
                Operand::object(binding_owner),
                Operand::object(h.named(b"nil")),
            ],
        )));
        h.call(template, b"withImplementation:", &[implementation])
    }
}
unsafe fn bind(h: &H, target: Oop, selector: Oop, method: Oop) {
    unsafe {
        let vt = h.call(h.id.receiver, b"ensurePrivateVtableFor:", &[target]);
        h.call(vt, b"addMethod:as:", &[method, selector]);
    }
}
struct Callback {
    id: Protocol,
    gc: Oop,
    expected: Vec<Oop>,
    answer: Oop,
    calls: Cell<usize>,
}
unsafe extern "C" fn check_call(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Callback>();
        let args = borrowed_slice(a, n).unwrap();
        assert_eq!(
            args.iter().map(|x| object(*x)).collect::<Vec<_>>(),
            p.expected
        );
        collect(p.id, p.gc);
        p.calls.set(p.calls.get() + 1);
        Outcome::loaded(Operand::object(p.answer))
    }
}
struct Lookup {
    id: Protocol,
    gc: Oop,
    selector: Oop,
    selected: NativeEntry,
    fallback: NativeEntry,
    calls: Cell<usize>,
}
unsafe extern "C" fn custom_lookup(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Lookup>();
        let args = borrowed_slice(a, n).unwrap();
        if args[0].as_object() == Some(p.selector) {
            p.calls.set(p.calls.get() + 1);
            collect(p.id, p.gc);
            Outcome::loaded(Operand::entry(p.selected))
        } else {
            (p.fallback.method.unwrap())(p.fallback.context, r, a, n)
        }
    }
}
unsafe extern "C" fn fail_call(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::failed(899, 71)
}
unsafe fn original_entry(h: &H, r: Oop, s: &[u8]) -> NativeEntry {
    unsafe {
        send(
            h.id,
            h.id.receiver,
            b"bind:to:",
            &[
                Operand::object(h.id.intern_name(s).unwrap()),
                Operand::object(r),
            ],
        )
        .as_entry()
        .unwrap()
    }
}
unsafe fn generated_method(h: &H, arity: usize, implementation: Oop) -> (Oop, Oop) {
    unsafe {
        let source = h.call(
            h.named(b"NativeBackend"),
            b"named:synchronous:",
            &[h.integer(arity), h.named(b"false")],
        );
        let unit = h.call(h.named(b"TCC"), b"compile:", &[source]);
        let method = h.call(unit, b"initializeWith:", &[implementation]);
        (method, unit)
    }
}
struct Recover {
    id: Protocol,
    ns: Oop,
    recipe: Oop,
    calls: Cell<usize>,
}
unsafe extern "C" fn recover_entry(
    c: *mut c_void,
    machine: Oop,
    _: *const Operand,
    _: usize,
) -> Outcome {
    unsafe {
        let p = &*c.cast::<Recover>();
        p.calls.set(p.calls.get() + 1);
        let roots = object(send(
            p.id,
            p.ns,
            b"at:",
            &[Operand::object(p.id.intern_name(b"Roots").unwrap())],
        ));
        let h = H {
            id: p.id,
            ns: p.ns,
            roots,
            tokens: RefCell::new(vec![]),
        };
        collect(p.id, h.named(b"GC"));
        let result = p.id.send_named(
            machine,
            b"recoverAt:with:",
            &[Operand::object(h.integer(0)), Operand::object(p.recipe)],
        );
        h.clear(0);
        result.map_or_else(|e| e, |_| Outcome::loaded(Operand::unit()))
    }
}
struct Direct {
    id: Protocol,
    implementation: Oop,
    selector: Vec<u8>,
}
unsafe extern "C" fn direct_entry(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Direct>();
        let mut arguments = vec![Operand::object(r)];
        arguments.extend_from_slice(borrowed_slice(a, n).unwrap());
        p.id.send_named(p.implementation, &p.selector, &arguments)
            .map_or_else(|e| e, Outcome::loaded)
    }
}
unsafe fn authored_shared_step(h: &H, shape: &Shape) -> (NativeEntry, Oop, Oop) {
    unsafe {
        assert_eq!(
            shape.meta.entry_argument_count, 1,
            "authored fixture is unary"
        );
        let source = format!(
            "let receiverSlot := {}; let implementationSlot := {}; let argumentSlot := {}; let outgoingArgumentStart := {}; let outgoingReceiverSlot := {}; let outgoingSelectorSlot := {}; let outgoingSecondArgument := {}; let callSite := 0;\n{}",
            shape.meta.entry_receiver_slot,
            shape.meta.implementation_slot.unwrap(),
            shape.meta.entry_argument_start,
            shape.argument_start,
            shape.receiver_slot,
            shape.selector_slot,
            shape.argument_start + 1,
            include_str!("../../../tests/fixtures/shared-invocation-step.birnam")
        );
        let path =
            std::env::temp_dir().join(format!("birnam-shared-step-{}.dylib", std::process::id()));
        let output = h.text(path.as_os_str().as_bytes());
        let compiled = h.call(
            h.named(b"Codegen"),
            b"compile:output:from:synchronousBodies:",
            &[h.text(source.as_bytes()), output, output, h.named(b"true")],
        );
        assert_ne!(compiled, h.named(b"nil"));
        assert_eq!(h.call(compiled, b"isError", &[]), h.named(b"false"));
        h.call(h.named(b"Codegen"), b"load:", &[output]);
        let key = h.keep(h.id.intern_name(b"SharedInvocationStep").unwrap());
        let step = h.call(h.named(b"Globals"), b"at:", &[key]);
        let counter_key = h.keep(h.id.intern_name(b"SharedInvocationCounter").unwrap());
        let counter = h.call(h.named(b"Globals"), b"at:", &[counter_key]);
        let method = h.call(
            step,
            b"methodFor:",
            &[h.keep(h.id.intern_name(b"advance:").unwrap())],
        );
        let entry = original_entry(h, step, b"advance:");
        let vt = h.call(h.id.receiver, b"behaviorOf:", &[step]);
        h.call(
            h.named(b"Store"),
            b"removeMethod:on:",
            &[h.keep(h.id.intern_name(b"advance:").unwrap()), vt],
        );
        h.call(h.named(b"Globals"), b"remove:", &[key]);
        h.call(h.named(b"Globals"), b"remove:", &[counter_key]);
        std::fs::remove_file(path).unwrap();
        (entry, method, counter)
    }
}
unsafe fn pause_at_implementation(
    h: &H,
    machine: Oop,
    block: Oop,
    shape: &Shape,
    implementation: Oop,
) -> Oop {
    unsafe {
        assert_eq!(
            h.call(machine, b"runBlock:budget:", &[block, h.integer(0)]),
            machine
        );
        for _ in 0..2000 {
            let view = h.call(machine, b"currentFrameView", &[]);
            let identity = h.call(view, b"implementationIdentity", &[]);
            let identity = send(h.id, identity, b"bits", &[]).as_word().unwrap();
            if identity as u64 == shape.meta.portable_implementation_identity
                && h.call(
                    view,
                    b"oopAt:",
                    &[h.integer(shape.meta.implementation_slot.unwrap() as usize)],
                ) == implementation
            {
                return view;
            }
            assert_eq!(
                h.call(machine, b"continueWithBudget:", &[h.integer(1)]),
                machine,
                "completed without exposing the adapter frame"
            );
        }
        panic!("adapter frame was not reached")
    }
}
#[test]
#[ignore = "production modules"]
fn shared_forwarding_preserves_live_invocation_and_method_ownership() {
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
        let gc = h.named(b"GC");
        let selector = h.keep(id.intern_name(b"probe:").unwrap());
        let target = h.call(h.named(b"Object"), b"clone", &[]);
        let argument = h.call(h.named(b"Object"), b"clone", &[]);
        let first = h.eval(b"[:self :arg | [System gc]; #[self arg #first]]");
        let second = h.eval(b"[:self :arg | [System gc]; #[self arg #second]]");
        let shape = Shape::new(&h, 1);
        let (entry, owner, counter) = if std::env::var_os("BIRNAM_AUTHORED_SHARED_STEP").is_some() {
            let (entry, owner, counter) = authored_shared_step(&h, &shape);
            (Some(entry), owner, Some(counter))
        } else {
            (None, h.call(h.named(b"Object"), b"clone", &[]), None)
        };
        let shared = Box::leak(Box::new(Shared {
            id,
            ns,
            shape,
            calls: Cell::new(0),
            entry,
            counter,
        }));
        let a = shared_method(&h, shared, owner, first);
        let b = shared_method(&h, shared, owner, second);
        assert_ne!(a, b);
        bind(&h, target, selector, a);
        let answer = h.call(target, b"probe:", &[argument]);
        assert_eq!(h.call(answer, b"at:", &[h.integer(0)]), target);
        assert_eq!(h.call(answer, b"at:", &[h.integer(1)]), argument);
        assert_eq!(
            h.call(answer, b"at:", &[h.integer(2)]),
            id.intern_name(b"first").unwrap()
        );
        bind(&h, target, selector, b);
        let answer = h.call(target, b"probe:", &[argument]);
        assert_eq!(
            h.call(answer, b"at:", &[h.integer(2)]),
            id.intern_name(b"second").unwrap()
        );
        let answer = h.call(a, b"invokeOn:arguments:", &[target, h.vector(&[argument])]);
        assert_eq!(
            h.call(answer, b"at:", &[h.integer(2)]),
            id.intern_name(b"first").unwrap()
        );

        // The code is shareable; the loaded TCC Unit currently is not an instance factory.
        let (baseline, unit) = generated_method(&h, 1, first);
        assert!(
            id.send_named(unit, b"initializeWith:", &[Operand::object(second)])
                .is_err()
        );
        // Replacement of the implementation object's invocation remains observable.
        let cb = Box::leak(Box::new(Callback {
            id,
            gc,
            expected: vec![target, argument],
            answer: argument,
            calls: Cell::new(0),
        }));
        let first_vt = h.call(id.receiver, b"ensurePrivateVtableFor:", &[first]);
        install(
            id,
            first_vt,
            b"value:value:",
            check_call,
            (cb as *mut Callback).cast(),
        );
        for method in [a, baseline] {
            bind(&h, target, selector, method);
            assert_eq!(h.call(target, b"probe:", &[argument]), argument);
        }
        assert_eq!(cb.calls.get(), 2);
        // Custom lookup on an arbitrary implementation object is not bypassed.
        let callable_vt = h.keep(behavior(id));
        let callable = h.keep(allocate(id, callable_vt, 0));
        let invocation = h.keep(id.intern_name(b"value:value:").unwrap());
        let lookup = Box::leak(Box::new(Lookup {
            id,
            gc,
            selector: invocation,
            selected: NativeEntry::new(check_call, (cb as *mut Callback).cast()),
            fallback: original_entry(&h, callable_vt, b"lookup"),
            calls: Cell::new(0),
        }));
        let meta = h.keep(behavior(id));
        install(
            id,
            meta,
            b"lookup",
            custom_lookup,
            (lookup as *mut Lookup).cast(),
        );
        h.call(
            id.receiver,
            b"reassociateBehaviorOf:with:",
            &[callable_vt, meta],
        );
        let custom = shared_method(&h, shared, owner, callable);
        let (custom_baseline, _) = generated_method(&h, 1, callable);
        for method in [custom, custom_baseline] {
            bind(&h, target, selector, method);
            assert_eq!(h.call(target, b"probe:", &[argument]), argument);
        }
        assert_eq!(lookup.calls.get(), 2);
        // Unknown invocation goes through the live DNU protocol, with original operands.
        let dnu_vt = h.keep(behavior(id));
        let dnu = h.keep(allocate(id, dnu_vt, 0));
        let dnu_probe = Box::leak(Box::new(Callback {
            id,
            gc,
            expected: vec![invocation, target, argument],
            answer: argument,
            calls: Cell::new(0),
        }));
        install(
            id,
            dnu_vt,
            b"doesNotUnderstand:",
            check_call,
            (dnu_probe as *mut Callback).cast(),
        );
        let dnu_method = shared_method(&h, shared, owner, dnu);
        bind(&h, target, selector, dnu_method);
        assert_eq!(h.call(target, b"probe:", &[argument]), argument);
        assert_eq!(dnu_probe.calls.get(), 1);
        // The proposed entry has no new error policy: compare it with today's ordinary
        // generated resumable adapter for both native failures and Error-valued returns.
        install(id, dnu_vt, b"value:value:", fail_call, std::ptr::null_mut());
        let (failure_baseline, _) = generated_method(&h, 1, dnu);
        for method in [dnu_method, failure_baseline] {
            bind(&h, target, selector, method);
            let err = h.call(target, b"probe:", &[argument]);
            assert_eq!(
                h.call(err, b"kind", &[]),
                id.intern_name(b"nativeFailure").unwrap()
            );
        }
        // Receiver + argument order at the zero, multi-argument and maximum named arities.
        for arity in [0, 2, 16] {
            let shape = Shape::new(&h, arity);
            let shared = Box::leak(Box::new(Shared {
                id,
                ns,
                shape,
                calls: Cell::new(0),
                entry: None,
                counter: None,
            }));
            let args: Vec<_> = (0..arity)
                .map(|_| h.call(h.named(b"Object"), b"clone", &[]))
                .collect();
            let mut expected = vec![target];
            expected.extend_from_slice(&args);
            let cb = Box::leak(Box::new(Callback {
                id,
                gc,
                expected,
                answer: argument,
                calls: Cell::new(0),
            }));
            let vt = h.keep(behavior(id));
            let implementation = h.keep(allocate(id, vt, 0));
            install(
                id,
                vt,
                &shared.shape.selector,
                check_call,
                (cb as *mut Callback).cast(),
            );
            let method = shared_method(&h, shared, owner, implementation);
            let name = if arity == 0 {
                b"probe".to_vec()
            } else {
                b"part:".repeat(arity)
            };
            let key = h.keep(id.intern_name(&name).unwrap());
            bind(&h, target, key, method);
            assert_eq!(h.call(target, &name, &args), argument);
            assert_eq!(call_count(&h, shared), 1);
            assert_eq!(cb.calls.get(), 1);
        }

        // Error objects are ordinary results and retain their original identity.
        let errors =
            h.eval(b"let e := [Error kind: #sharedInvocationError]; #[e [:self :arg | e]]");
        let error = h.call(errors, b"at:", &[h.integer(0)]);
        let error_impl = h.call(errors, b"at:", &[h.integer(1)]);
        let error_method = shared_method(&h, shared, owner, error_impl);
        let (error_baseline, _) = generated_method(&h, 1, error_impl);
        for method in [error_method, error_baseline] {
            bind(&h, target, selector, method);
            assert_eq!(h.call(target, b"probe:", &[argument]), error);
        }
        // Actual carrier conversion remains at Execution's explicit native boundary.
        for signature in ["ww", "bb"] {
            let body = format!(
                "[Execution block: [:self :value | [System gc]; value] withNativeSignature: \"{signature}\"]"
            );
            let implementation = h.eval(body.as_bytes());
            let method = shared_method(&h, shared, owner, implementation);
            let (baseline, _) = generated_method(&h, 1, implementation);
            for method in [method, baseline] {
                bind(&h, target, selector, method);
                if signature == "ww" {
                    assert_eq!(
                        send(id, target, b"probe:", &[Operand::word(73)]).as_word(),
                        Some(73)
                    );
                } else {
                    let bytes = send(id, target, b"probe:", &[Operand::bytes(b"shared bytes")])
                        .as_bytes()
                        .unwrap();
                    assert_eq!(
                        birnam_abi::borrowed_bytes(bytes.pointer, bytes.length).unwrap(),
                        b"shared bytes"
                    );
                }
            }
        }
        // Captures retain the same shared Cell; nested sends select each method's own implementation.
        let pair=h.eval(b"let cell := [Cell with: nil]; #[cell [:self :arg | [cell set: arg]; [System gc]; [cell value]]]");
        let cell = h.call(pair, b"at:", &[h.integer(0)]);
        let captured_impl = h.call(pair, b"at:", &[h.integer(1)]);
        let captured = shared_method(&h, shared, owner, captured_impl);
        bind(&h, target, selector, captured);
        assert_eq!(h.call(target, b"probe:", &[argument]), argument);
        assert_eq!(h.call(cell, b"value", &[]), argument);
        let other = h.keep(id.intern_name(b"other:").unwrap());
        bind(&h, target, other, captured);
        let nested_impl = h.eval(b"[:self :arg | [self other: arg]]");
        let nested = shared_method(&h, shared, owner, nested_impl);
        bind(&h, target, selector, nested);
        let calls = call_count(&h, shared);
        assert_eq!(h.call(target, b"probe:", &[argument]), argument);
        assert_eq!(call_count(&h, shared), calls + 2);
        let globals = h.named(b"Globals");
        let key_target = h.keep(id.intern_name(b"SharedProbeTarget").unwrap());
        let key_arg = h.keep(id.intern_name(b"SharedProbeArgument").unwrap());
        h.call(globals, b"at:put:", &[key_target, target]);
        h.call(globals, b"at:put:", &[key_arg, argument]);
        let caller = h.eval(b"[| [SharedProbeTarget probe: SharedProbeArgument]]");
        // A pending named frame retains the selected Method despite dictionary replacement.
        // The later implementation invocation is still looked up in the changed world.
        bind(&h, target, selector, a);
        let m = h.call(h.named(b"Machine"), b"new", &[]);
        let count = call_count(&h, shared);
        let view = pause_at_implementation(&h, m, caller, &shared.shape, first);
        assert_eq!(call_count(&h, shared), count);
        let generation_a = h.call(view, b"generationIdentity", &[]);
        let generation_a = send(id, generation_a, b"bits", &[]).as_word().unwrap();
        assert_eq!(
            h.call(
                view,
                b"oopAt:",
                &[h.integer(shared.shape.meta.entry_receiver_slot as usize)]
            ),
            target
        );
        assert_eq!(
            h.call(
                view,
                b"oopAt:",
                &[h.integer(shared.shape.meta.entry_argument_start as usize)]
            ),
            argument
        );
        bind(&h, target, selector, b);
        let changed = Box::leak(Box::new(Callback {
            id,
            gc,
            expected: vec![target, argument],
            answer: target,
            calls: Cell::new(0),
        }));
        install(
            id,
            first_vt,
            b"value:value:",
            check_call,
            (changed as *mut Callback).cast(),
        );
        collect(id, gc);
        assert_eq!(
            h.call(m, b"continueWithBudget:", &[h.integer(2000)]),
            target
        );
        assert_eq!(changed.calls.get(), 1);
        assert_eq!(call_count(&h, shared), count + 1);
        let stale = h.call(view, b"programCounter", &[]);
        assert_eq!(
            h.call(stale, b"kind", &[]),
            id.intern_name(b"staleFrameView").unwrap()
        );
        let result = h.call(target, b"probe:", &[argument]);
        assert_eq!(
            h.call(result, b"at:", &[h.integer(2)]),
            id.intern_name(b"second").unwrap()
        );
        let comparison = h.call(h.named(b"Machine"), b"new", &[]);
        let view_b = pause_at_implementation(&h, comparison, caller, &shared.shape, second);
        let generation_b = h.call(view_b, b"generationIdentity", &[]);
        assert_ne!(
            generation_a,
            send(id, generation_b, b"bits", &[]).as_word().unwrap(),
            "code sharing merged binding generations"
        );
        h.call(comparison, b"continueWithBudget:", &[h.integer(2000)]);
        // Sharing an entry must not introduce a synchronous barrier around a parkable body.
        let waiting = h.call(h.named(b"Machine"), b"new", &[]);
        let key_machine = h.keep(id.intern_name(b"SharedProbeMachine").unwrap());
        h.call(globals, b"at:put:", &[key_machine, waiting]);
        let park_impl = h.eval(b"[:self :arg | [SharedProbeMachine park]; [System gc]; arg]");
        let park_method = shared_method(&h, shared, owner, park_impl);
        bind(&h, target, selector, park_method);
        assert_eq!(h.call(waiting, b"runBlock:", &[caller]), waiting);
        assert!(
            id.send_named(
                waiting,
                b"continueWithBudget:",
                &[Operand::object(h.integer(1))]
            )
            .is_err()
        );
        collect(id, gc);
        assert_eq!(h.call(waiting, b"resumeWith:", &[argument]), argument);
        assert!(
            id.send_named(waiting, b"resumeWith:", &[Operand::object(argument)])
                .is_err()
        );
        // Negative control: a plain C-ABI forwarder still uses live lookup, but
        // inserts a synchronous boundary and cannot replace a parkable named frame.
        let direct_context = Box::leak(Box::new(Direct {
            id,
            implementation: park_impl,
            selector: shared.shape.selector.clone(),
        }));
        let direct = h.keep(object(send(
            id,
            h.named(b"Method"),
            b"fromNative:owner:implementation:",
            &[
                Operand::entry(NativeEntry::new(
                    direct_entry,
                    (direct_context as *mut Direct).cast(),
                )),
                Operand::object(owner),
                Operand::object(park_impl),
            ],
        )));
        let direct_machine = h.call(h.named(b"Machine"), b"new", &[]);
        h.call(globals, b"at:put:", &[key_machine, direct_machine]);
        bind(&h, target, selector, direct);
        let direct_result = id.send_named(direct_machine, b"runBlock:", &[Operand::object(caller)]);
        assert_ne!(
            direct_result.as_ref().ok().and_then(|v| v.as_object()),
            Some(direct_machine),
            "negative control unexpectedly suspended through native boundary"
        );
        assert!(
            id.send_named(direct_machine, b"resumeWith:", &[Operand::object(argument)])
                .is_err()
        );
        eprintln!(
            "plain native trampoline: did not preserve suspension (expected negative control)"
        );
        h.call(globals, b"remove:", &[key_machine]);
        // Enter optimized code, perform one effect, then recover to the shared baseline.
        // The baseline forwarding and body must not replay that entered effect.
        let recover_impl = h.eval(b"[:self :arg | [System gc]; arg]");
        let recover_baseline = shared_method(&h, shared, owner, recover_impl);
        let mut slots = vec![h.named(b"nil"); shared.shape.meta.slot_count as usize];
        slots[shared.shape.meta.entry_receiver_slot as usize] = target;
        slots[shared.shape.meta.entry_argument_start as usize] = argument;
        slots[shared.shape.meta.implementation_slot.unwrap() as usize] = recover_impl;
        let recipe = h.vector(&slots);
        let recovery = Box::leak(Box::new(Recover {
            id,
            ns,
            recipe,
            calls: Cell::new(0),
        }));
        let recovery_owner = h.vector(&[recipe]);
        let optimized = h.keep(object(send(
            id,
            recover_baseline,
            b"withInline:at:owner:",
            &[
                Operand::entry(NativeEntry::new(
                    recover_entry,
                    (recovery as *mut Recover).cast(),
                )),
                Operand::object(h.integer(0)),
                Operand::object(recovery_owner),
            ],
        )));
        bind(&h, target, selector, optimized);
        let recover_machine = h.call(h.named(b"Machine"), b"new", &[]);
        let non_tail=h.eval(b"[| let result := [SharedProbeTarget probe: SharedProbeArgument]; [System gc]; result]");
        let entered = call_count(&h, shared);
        assert_eq!(h.call(recover_machine, b"runBlock:", &[non_tail]), argument);
        assert_eq!(recovery.calls.get(), 1);
        assert_eq!(call_count(&h, shared), entered + 1);
        eprintln!("shared adapter: optimized entry recovered once to ordinary forwarding");

        // A new implementation must not inherit an optimization of the old one.
        let rebound = h.call(optimized, b"withImplementation:", &[second]);
        bind(&h, target, selector, rebound);
        let rebound_machine = h.call(h.named(b"Machine"), b"new", &[]);
        let result = h.call(rebound_machine, b"runBlock:", &[non_tail]);
        assert_eq!(
            h.call(result, b"at:", &[h.integer(2)]),
            id.intern_name(b"second").unwrap()
        );
        assert_eq!(
            recovery.calls.get(),
            1,
            "rebound Method inherited stale optimization"
        );

        // Only the entered frame retains this method/implementation/owner after removal.
        let mark = h.tokens.borrow().len();
        let private_owner = h.call(
            h.named(b"ObjectModel"),
            b"newSlots:parent:",
            &[h.vector(&[owner]), h.named(b"ObjectVt")],
        );
        let private_impl = h.eval(b"[:self :arg | [System gc]; arg]");
        let private = shared_method(&h, shared, private_owner, private_impl);
        bind(&h, target, selector, private);
        let active = h.call(h.named(b"Machine"), b"new", &[]);
        let active_token = retain(id, roots, active);
        let active_view = pause_at_implementation(&h, active, caller, &shared.shape, private_impl);
        let view_token = retain(id, roots, active_view);
        bind(&h, target, selector, b);
        h.clear(mark);
        collect(id, gc);
        assert!(present(id, private) && present(id, private_impl) && present(id, private_owner));
        assert_eq!(
            object(send(
                id,
                active,
                b"continueWithBudget:",
                &[Operand::object(h.integer(2000))]
            )),
            argument
        );
        collect(id, gc);
        assert!(
            !present(id, private) && !present(id, private_impl) && !present(id, private_owner),
            "finished invocation retained private state"
        );
        release(id, roots, view_token);
        release(id, roots, active_token);
        // Rebinding does not retain a chain of previous Methods or bodies.
        // Retiring the original registration must leave its sibling callable.
        let mark = h.tokens.borrow().len();
        let old_body = h.eval(b"[:self :arg | #[self arg]]");
        let original = shared_method(&h, shared, owner, old_body);
        let sibling = h.call(original, b"withImplementation:", &[second]);
        let sibling_root = retain(id, roots, sibling);
        h.clear(mark);
        collect(id, gc);
        assert!(
            !present(id, original) && !present(id, old_body),
            "rebind retained old binding state"
        );
        let answer = h.call(
            sibling,
            b"invokeOn:arguments:",
            &[target, h.vector(&[argument])],
        );
        assert_eq!(
            h.call(answer, b"at:", &[h.integer(2)]),
            id.intern_name(b"second").unwrap()
        );
        release(id, roots, sibling_root);
        collect(id, gc);
        assert!(
            !present(id, sibling),
            "code owner retained a retired binding"
        );

        h.call(globals, b"remove:", &[key_target]);
        h.call(globals, b"remove:", &[key_arg]);

        // The same step can be authored in Birnam and compiled once. Its captured Method
        // owns the code; bound user Methods own only their separate implementation data.
        let authored_shape = Shape::new(&h, 1);
        let (authored_entry, authored_owner, authored_counter) =
            authored_shared_step(&h, &authored_shape);
        let authored_shared = Box::leak(Box::new(Shared {
            id,
            ns,
            shape: authored_shape,
            calls: Cell::new(0),
            entry: Some(authored_entry),
            counter: Some(authored_counter),
        }));
        let authored_a = shared_method(&h, authored_shared, authored_owner, recover_impl);
        let authored_b = shared_method(&h, authored_shared, authored_owner, second);
        bind(&h, target, selector, authored_a);
        assert_eq!(h.call(target, b"probe:", &[argument]), argument);
        bind(&h, target, selector, authored_b);
        let result = h.call(target, b"probe:", &[argument]);
        assert_eq!(
            h.call(result, b"at:", &[h.integer(2)]),
            id.intern_name(b"second").unwrap()
        );
        let captured_result = h.call(
            authored_a,
            b"invokeOn:arguments:",
            &[target, h.vector(&[argument])],
        );
        assert_eq!(captured_result, argument);
        collect(id, gc);
        eprintln!(
            "authored shared step: one compiled Birnam step serves distinct retained implementations"
        );
        // Optional native resource metrics compare code ownership, not CPU throughput.
        collect(id, gc);
        send(id, h.named(b"TCC"), b"checkpoint", &[]);
        if let Ok(value) = id.send_named(h.named(b"TCC"), b"liveNativeUnits", &[]) {
            if let Some(before) = value.as_word() {
                let mark = h.tokens.borrow().len();
                for _ in 0..16 {
                    let _ = generated_method(&h, 1, recover_impl);
                }
                let after = send(id, h.named(b"TCC"), b"liveNativeUnits", &[])
                    .as_word()
                    .unwrap();
                assert_eq!(after - before, 16);
                h.clear(mark);
                collect(id, gc);
                send(id, h.named(b"TCC"), b"checkpoint", &[]);
                assert_eq!(
                    send(id, h.named(b"TCC"), b"liveNativeUnits", &[])
                        .as_word()
                        .unwrap(),
                    before,
                    "ordinary temporary adapters did not retire"
                );
                let binding_template =
                    shared_method(&h, authored_shared, authored_owner, h.named(b"nil"));
                let before = send(id, h.named(b"TCC"), b"liveNativeUnits", &[])
                    .as_word()
                    .unwrap();
                let mark = h.tokens.borrow().len();
                let mut methods = Vec::new();
                for _ in 0..16 {
                    methods.push(h.call(binding_template, b"withImplementation:", &[recover_impl]));
                }
                let after = send(id, h.named(b"TCC"), b"liveNativeUnits", &[])
                    .as_word()
                    .unwrap();
                assert_eq!(after, before);
                h.clear(mark);
                collect(id, gc);
                assert!(
                    methods.into_iter().all(|m| !present(id, m)),
                    "shared code retained dead Methods"
                );
                eprintln!(
                    "registration code units: 16 ordinary definitions +16; 16 shared-step bindings +0 (one existing shared step)"
                );
            }
        }
        eprintln!(
            "shared adapter: distinct Methods and binding owners, one shared step entry; calls={}",
            call_count(&h, shared)
        );
        collect(id, gc);
    }
}
