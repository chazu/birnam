//! Production counterpart of retained scheduling/lifetime mechanics probes.
#[path = "execution_machines/native_tier.rs"]
mod native_tier;
#[path = "execution_machines/recovery.rs"]
mod recovery;
mod support;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use birnam_artifact_codec::stackless_selection::{
    self as selection, Disposition, ImplementationKind, RootKind, SelectionSpec,
};
use core::{ffi::c_void, ptr};
use std::{cell::Cell, os::unix::ffi::OsStrExt};
use support::*;
struct H {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    gc: Oop,
    tokens: Vec<usize>,
}
impl H {
    unsafe fn new() -> Self {
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
            Self {
                id,
                ns,
                roots: named(b"Roots"),
                gc: named(b"GC"),
                tokens: vec![],
            }
        }
    }
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
    unsafe fn keep(&mut self, v: Oop) -> Oop {
        self.tokens.push(unsafe { retain(self.id, self.roots, v) });
        v
    }
    unsafe fn integer(&self, i: usize) -> Operand {
        unsafe {
            send(
                self.id,
                self.named(b"Integer"),
                b"fromWord:",
                &[Operand::word(i)],
            )
        }
    }
    unsafe fn value(&self, v: Operand) -> usize {
        unsafe {
            send(self.id, self.named(b"Integer"), b"wordOf:", &[v])
                .as_word()
                .unwrap()
        }
    }
    unsafe fn vector(&mut self, values: &[Oop]) -> Oop {
        unsafe {
            let mut v = object(send(self.id, self.named(b"Vector"), b"new", &[]));
            self.keep(v);
            for &value in values {
                v = object(send(self.id, v, b"add:", &[Operand::object(value)]));
                self.keep(v);
            }
            v
        }
    }
    unsafe fn method(&mut self, fixture: &str, mode: usize, owner: Oop) -> Oop {
        unsafe {
            let bytes = std::fs::read(format!(
                "{}/../../tests/fixtures/stackless-s2/frame-descriptor-{fixture}.bin",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap();
            let directory = 64 + 8 * 32;
            let offset =
                u64::from_le_bytes(bytes[directory + 16..directory + 24].try_into().unwrap())
                    as usize;
            let revision = bytes[offset..offset + 32].try_into().unwrap();
            let spec = SelectionSpec {
                source_revision: revision,
                root_kind: RootKind::MethodOperation,
                root_index: 0,
                path: if fixture == "source-map-block" {
                    &[2, 1]
                } else {
                    &[]
                },
                implementation_kind: if fixture == "source-map-block" {
                    ImplementationKind::BlockBody
                } else {
                    ImplementationKind::NamedAdapter
                },
                disposition: Disposition::ResumableRequired,
            };
            let selection = selection::encode_artifact_selection(&[revision], &[spec]).unwrap();
            let method = load(
                self.id,
                "execution_machine_fixture",
                &[
                    Operand::object(self.ns),
                    Operand::bytes(&bytes),
                    Operand::bytes(&selection),
                    Operand::object(owner),
                    Operand::word(mode),
                ],
            );
            self.keep(method)
        }
    }
    unsafe fn machine(&mut self) -> Oop {
        unsafe {
            let m = object(send(self.id, self.named(b"Machine"), b"new", &[]));
            self.keep(m)
        }
    }
    unsafe fn block(&mut self, method: Oop, captures: &[Oop]) -> Oop {
        unsafe {
            let values = self.vector(captures);
            let v = object(send(
                self.id,
                self.named(b"Block"),
                b"fromMethod:captures:",
                &[Operand::object(method), Operand::object(values)],
            ));
            self.keep(v)
        }
    }
    unsafe fn invoke(&self, method: Oop, receiver: Oop) -> Oop {
        unsafe {
            object(send(
                self.id,
                method,
                b"invokeOn:",
                &[Operand::object(receiver)],
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
    unsafe fn release_all(&mut self) {
        for token in self.tokens.drain(..) {
            unsafe { release(self.id, self.roots, token) }
        }
    }
}
struct Callback {
    id: Protocol,
    gc: Oop,
    calls: Cell<usize>,
}
unsafe extern "C" fn callback(c: *mut c_void, _: Oop, args: *const Operand, n: usize) -> Outcome {
    unsafe {
        let c = &*c.cast::<Callback>();
        let args = borrowed_slice(args, n).unwrap();
        collect(c.id, c.gc);
        c.calls.set(c.calls.get() + 1);
        Outcome::loaded(args[0])
    }
}
struct NativeBoundary {
    id: Protocol,
    gc: Oop,
    park: Cell<bool>,
    rejected: Cell<bool>,
}
unsafe extern "C" fn native_boundary(
    c: *mut c_void,
    r: Oop,
    _: *const Operand,
    _: usize,
) -> Outcome {
    unsafe {
        let c = &*c.cast::<NativeBoundary>();
        collect(c.id, c.gc);
        if c.park.get() {
            let result = c.id.send_named(r, b"park", &[]);
            c.rejected.set(result.is_err());
            result.map_or_else(|e| e, Outcome::loaded)
        } else {
            Outcome::loaded(send(c.id, r, b"controlEligible", &[]))
        }
    }
}
struct DuringTrace {
    id: Protocol,
    machine: Oop,
    rejected: Cell<bool>,
}
unsafe extern "C" fn during_trace(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    unsafe {
        let c = &*c.cast::<DuringTrace>();
        c.rejected
            .set(c.id.send_named(c.machine, b"new", &[]).is_err());
        Outcome::loaded(Operand::unit())
    }
}
unsafe extern "C" fn control_replacement(
    _: *mut c_void,
    receiver: Oop,
    _: *const Operand,
    _: usize,
) -> Outcome {
    Outcome::loaded(Operand::object(receiver))
}

// Preserve the former Runtime task-control acceptance: repeated preparation
// and new Machines must never overwrite an intentionally replaced adapter.
unsafe fn control_replacements(h: &mut H) {
    unsafe {
        let machine = h.machine();
        let behavior = object(send(
            h.id,
            h.id.receiver,
            b"behaviorOf:",
            &[Operand::object(machine)],
        ));
        for name in [b"park".as_slice(), b"protectedBlock:"] {
            let selector = h.id.intern_name(name).unwrap();
            h.keep(selector);
            let prior = send(
                h.id,
                h.id.receiver,
                b"bindingOf:in:",
                &[Operand::object(selector), Operand::object(behavior)],
            );
            // Capture the Method, retaining its registered implementation while
            // its original dictionary binding is absent.
            let method = object(send(
                h.id,
                machine,
                b"methodFor:",
                &[Operand::object(selector)],
            ));
            h.keep(method);
            install(h.id, behavior, name, control_replacement, ptr::null_mut());
            for _ in 0..2 {
                send(h.id, machine, b"prepareBlockControl", &[]);
                let other = h.machine();
                send(h.id, other, b"prepareBlockControl", &[]);
                collect(h.id, h.gc);
                let current = send(
                    h.id,
                    h.id.receiver,
                    b"bindingOf:in:",
                    &[Operand::object(selector), Operand::object(behavior)],
                )
                .as_entry()
                .unwrap();
                assert_eq!(
                    current.method.unwrap() as usize,
                    control_replacement as *const () as usize
                );
                assert_eq!(h.value(send(h.id, other, b"liveFrameCount", &[])), 0);
            }
            send(
                h.id,
                behavior,
                b"addMethod",
                &[Operand::object(selector), prior],
            );
        }
    }
}
#[test]
#[ignore = "scripts/native-message-test.sh builds production dylibs and executable fixture"]
fn production_machine_calls_blocks_budgets_waits_and_lifetimes() {
    unsafe {
        let mut h = H::new();
        if std::env::var_os("BIRNAM_EXECUTION_NATIVE_SEED").is_some() {
            native_tier::numeric(&mut h);
            return;
        }
        let nil = h.named(b"nil");
        recovery::check(&mut h);
        control_replacements(&mut h);
        native_tier::check(&mut h);
        let owner = allocate(h.id, behavior(h.id), 0);
        h.keep(owner);
        let simple = h.method("minimal-return", 1, owner);
        assert_eq!(h.invoke(simple, nil), owner);
        let capture = allocate(h.id, behavior(h.id), 0);
        h.keep(capture);
        let bm = h.method("source-map-block", 6, owner);
        let block = h.block(bm, &[capture]);
        assert_eq!(object(send(h.id, block, b"value", &[])), capture);
        let machine = h.machine();
        assert_eq!(
            object(send(h.id, machine, b"runBlock:", &[Operand::object(block)])),
            capture
        );
        assert_eq!(h.value(send(h.id, machine, b"liveFrameCount", &[])), 0);

        // A native callback forces precise collection after call operands have been
        // copied from the suspended caller. A subsequent send sees replacement.
        let behavior = behavior(h.id);
        h.keep(behavior);
        let receiver = allocate(h.id, behavior, 0);
        h.keep(receiver);
        let selector = h.id.intern_name(b"machineAnswer:").unwrap();
        h.keep(selector);
        let argument = allocate(h.id, behavior, 0);
        h.keep(argument);
        let c = Box::leak(Box::new(Callback {
            id: h.id,
            gc: h.gc,
            calls: Cell::new(0),
        }));
        let method = object(send(
            h.id,
            h.named(b"Method"),
            b"fromNative:owner:",
            &[
                Operand::entry(NativeEntry::new(callback, ptr::from_mut(c).cast())),
                Operand::object(owner),
            ],
        ));
        h.keep(method);
        h.bind(behavior, selector, method);
        let operands = h.vector(&[receiver, selector, argument]);
        let caller = h.method("call", 4, operands);
        assert_eq!(h.invoke(caller, nil), argument);
        assert_eq!(c.calls.get(), 1);
        h.bind(behavior, selector, simple);
        assert_eq!(h.invoke(caller, nil), owner);
        let tail = h.method("tail-call", 7, operands);
        assert_eq!(h.invoke(tail, nil), owner);

        // Two independent Machines stop only at committed edges, preserve frame
        // views while paused, and retain their own exact code owner through GC.
        let branch = h.method("branch-join", 5, owner);
        let branch_block = h.block(branch, &[]);
        let a = h.machine();
        let b = h.machine();
        for m in [a, b] {
            assert_eq!(
                object(send(
                    h.id,
                    m,
                    b"runBlock:budget:",
                    &[Operand::object(branch_block), h.integer(2)]
                )),
                m
            );
        }
        let view = object(send(h.id, a, b"currentFrameView", &[]));
        h.keep(view);
        assert_eq!(h.value(send(h.id, view, b"programCounter", &[])), 1);
        collect(h.id, h.gc);
        assert_eq!(
            object(send(h.id, a, b"continueWithBudget:", &[h.integer(1)])),
            a
        );
        assert_eq!(h.value(send(h.id, view, b"programCounter", &[])), 3);
        assert_eq!(
            object(send(h.id, b, b"continueWithBudget:", &[h.integer(20)])),
            owner
        );
        assert_eq!(
            object(send(h.id, a, b"continueWithBudget:", &[h.integer(20)])),
            owner
        );
        let stale = object(send(h.id, view, b"programCounter", &[]));
        h.keep(stale);
        assert_eq!(
            object(send(h.id, stale, b"kind", &[])),
            h.id.intern_name(b"staleFrameView").unwrap()
        );
        assert_eq!(object(send(h.id, a, b"currentFrameView", &[])), nil);

        let wait = h.method("call", 2, owner);
        let wait_block = h.block(wait, &[]);
        let waiting = h.machine();
        assert_eq!(
            object(send(
                h.id,
                waiting,
                b"runBlock:",
                &[Operand::object(wait_block)]
            )),
            waiting
        );
        assert!(
            h.id.send_named(waiting, b"continueWithBudget:", &[h.integer(1)])
                .is_err()
        );
        collect(h.id, h.gc);
        assert_eq!(
            object(send(
                h.id,
                waiting,
                b"resumeWith:",
                &[Operand::object(argument)]
            )),
            argument
        );
        assert!(
            h.id.send_named(waiting, b"resumeWith:", &[Operand::object(argument)])
                .is_err()
        );

        // A protected call leaves a native-independent boundary after its step
        // returns. Exiting a runnable boundary discards the child and delivers once.
        let protected_machine = h.machine();
        let protected_selector = h.id.intern_name(b"protectedBlock:").unwrap();
        h.keep(protected_selector);
        let protected_owner = h.vector(&[protected_machine, protected_selector, branch_block]);
        let protected_method = h.method("call", 4, protected_owner);
        let protected_block = h.block(protected_method, &[]);
        assert_eq!(
            object(send(
                h.id,
                protected_machine,
                b"runBlock:budget:",
                &[Operand::object(protected_block), h.integer(3)]
            )),
            protected_machine
        );
        send(
            h.id,
            protected_machine,
            b"exitRunnableBoundary:",
            &[Operand::object(argument)],
        );
        assert_eq!(
            object(send(
                h.id,
                protected_machine,
                b"boundaryDeliveryPending",
                &[]
            )),
            h.named(b"true")
        );
        assert_eq!(
            object(send(
                h.id,
                protected_machine,
                b"continueWithBudget:",
                &[h.integer(20)]
            )),
            argument
        );

        // Native callbacks create a control barrier, including a nested registered
        // call back into Execution. GC during the callback must preserve both frames.
        let native_machine = h.machine();
        let machine_behavior = object(send(
            h.id,
            h.id.receiver,
            b"behaviorOf:",
            &[Operand::object(native_machine)],
        ));
        let boundary = Box::leak(Box::new(NativeBoundary {
            id: h.id,
            gc: h.gc,
            park: Cell::new(false),
            rejected: Cell::new(false),
        }));
        install(
            h.id,
            machine_behavior,
            b"value",
            native_boundary,
            ptr::from_mut(boundary).cast(),
        );
        assert_eq!(
            object(send(
                h.id,
                native_machine,
                b"runBlock:",
                &[Operand::object(native_machine)]
            )),
            h.named(b"false")
        );
        boundary.park.set(true);
        let blocked = h.machine();
        assert!(
            h.id.send_named(blocked, b"runBlock:", &[Operand::object(blocked)])
                .is_err()
        );
        assert!(boundary.rejected.get());
        send(h.id, blocked, b"destroy", &[]);
        send(
            h.id,
            machine_behavior,
            b"removeMethod:",
            &[Operand::object(h.id.intern_name(b"value").unwrap())],
        );

        let tracer_behavior = support::behavior(h.id);
        h.keep(tracer_behavior);
        let tracer = allocate(h.id, tracer_behavior, 0);
        h.keep(tracer);
        let during = Box::leak(Box::new(DuringTrace {
            id: h.id,
            machine: h.named(b"Machine"),
            rejected: Cell::new(false),
        }));
        install(
            h.id,
            tracer_behavior,
            b"rootsDo:",
            during_trace,
            ptr::from_mut(during).cast(),
        );
        send(h.id, h.roots, b"register:", &[Operand::object(tracer)]);
        collect(h.id, h.gc);
        assert!(during.rejected.get());
        send(h.id, h.roots, b"unregister:", &[Operand::object(tracer)]);

        // Capturing transfers frames and their code pins into a one-shot
        // Continuation. Wrong-machine admission must leave the segment untouched.
        let capturer = h.method("call", 3, owner);
        let capturing_block = h.block(capturer, &[]);
        let source = h.machine();
        let continuation = object(send(
            h.id,
            source,
            b"runBlock:",
            &[Operand::object(capturing_block)],
        ));
        h.keep(continuation);
        collect(h.id, h.gc);
        let destination = h.machine();
        assert!(
            h.id.send_named(
                destination,
                b"restoreContinuation:with:",
                &[Operand::object(continuation), Operand::object(argument)]
            )
            .is_err()
        );
        assert_eq!(
            object(send(
                h.id,
                source,
                b"restoreContinuation:with:",
                &[Operand::object(continuation), Operand::object(argument)]
            )),
            argument
        );
        assert!(
            h.id.send_named(
                source,
                b"restoreContinuation:with:",
                &[Operand::object(continuation), Operand::object(argument)]
            )
            .is_err()
        );
        // Shallow Block clone preserves its captures and exact installed entry.
        let clone = object(send(h.id, block, b"clone", &[]));
        h.keep(clone);
        assert_ne!(clone, block);
        assert_eq!(object(send(h.id, clone, b"value", &[])), capture);

        // Only the paused Machine retains this installed implementation after
        // replacement. The ordinary receiver has no hidden Method/capture fields.
        let retained_owner = allocate(h.id, support::behavior(h.id), 0);
        h.keep(retained_owner);
        let retained = h.method("branch-join", 5, retained_owner);
        let retained_behavior = support::behavior(h.id);
        h.keep(retained_behavior);
        let ordinary = allocate(h.id, retained_behavior, 0);
        h.keep(ordinary);
        let value_selector = h.id.intern_name(b"value").unwrap();
        h.keep(value_selector);
        h.bind(retained_behavior, value_selector, retained);
        let only = h.machine();
        let paused_root = retain(h.id, h.roots, only);
        send(
            h.id,
            only,
            b"runBlock:budget:",
            &[Operand::object(ordinary), h.integer(2)],
        );
        h.bind(retained_behavior, value_selector, simple);
        h.release_all();
        collect(h.id, h.gc);
        assert!(present(h.id, retained_owner));
        assert!(present(h.id, retained));
        assert_eq!(
            object(send(h.id, only, b"continueWithBudget:", &[h.integer(10)])),
            retained_owner
        );
        collect(h.id, h.gc);
        assert!(!present(h.id, retained_owner));
        assert!(!present(h.id, retained));
        send(h.id, h.named(b"Execution"), b"checkpoint", &[]);
        release(h.id, h.roots, paused_root);
        collect(h.id, h.gc);

        // A detached continuation alone roots its originating Machine, executable
        // and code owner. Consuming it removes those frame edges.
        let captured_owner = allocate(h.id, support::behavior(h.id), 0);
        h.keep(captured_owner);
        let capturing = h.method("call", 3, captured_owner);
        let capturing_block = h.block(capturing, &[]);
        let source = h.machine();
        let continuation = object(send(
            h.id,
            source,
            b"runBlock:",
            &[Operand::object(capturing_block)],
        ));
        let continuation_root = retain(h.id, h.roots, continuation);
        h.release_all();
        collect(h.id, h.gc);
        assert!(present(h.id, captured_owner));
        assert!(present(h.id, source));
        let supplied = h.named(b"true");
        assert_eq!(
            object(send(
                h.id,
                source,
                b"restoreContinuation:with:",
                &[Operand::object(continuation), Operand::object(supplied)]
            )),
            supplied
        );
        collect(h.id, h.gc);
        assert!(!present(h.id, captured_owner));
        release(h.id, h.roots, continuation_root);
        collect(h.id, h.gc);
        send(h.id, h.named(b"Execution"), b"checkpoint", &[]);
    }
}
