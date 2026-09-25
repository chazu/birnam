//! Real source producer, actual heap inventory and effectful fallback witnesses.
use super::*;
use std::time::Instant;

pub(super) unsafe fn lookup(h: &H, r: Oop, name: &[u8]) -> NativeEntry {
    unsafe {
        send(
            h.id,
            h.id.receiver,
            b"bind:to:",
            &[
                Operand::object(h.id.intern_name(name).unwrap()),
                Operand::object(r),
            ],
        )
        .as_entry()
        .unwrap()
    }
}
pub(super) unsafe fn restore(h: &H, vt: Oop, name: &[u8], entry: NativeEntry) {
    unsafe { install(h.id, vt, name, entry.method.unwrap(), entry.context) }
}
pub(super) unsafe fn vt(h: &H, value: Oop) -> Oop {
    unsafe {
        object(send(
            h.id,
            h.id.receiver,
            b"behaviorOf:",
            &[Operand::object(value)],
        ))
    }
}
unsafe extern "C" fn refuse(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::word(0))
}
unsafe extern "C" fn unannotated(_: *mut c_void, r: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::object(r))
}
struct Count {
    id: Protocol,
    behavior: Oop,
    count: usize,
}
unsafe extern "C" fn count(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &mut *c.cast::<Count>();
        let [value, _, _] = borrowed_slice(a, n).unwrap() else {
            panic!("inventory")
        };
        if send(p.id, p.id.receiver, b"behaviorOf:", &[*value]).as_object() == Some(p.behavior) {
            p.count += 1;
        }
        Outcome::loaded(Operand::unit())
    }
}
pub(super) unsafe fn instances(h: &H, behavior: Oop) -> usize {
    unsafe {
        let mut c = Count {
            id: h.id,
            behavior,
            count: 0,
        };
        support::inventory(h.id, count, ptr::from_mut(&mut c).cast());
        c.count
    }
}
pub(super) unsafe fn run(h: &H, block: Oop, bounded: bool) -> Oop {
    unsafe {
        let machine = object(send(h.id, h.named(b"Machine"), b"new", &[]));
        let pin = retain(h.id, h.roots, machine);
        let result = if bounded {
            let budget = send(
                h.id,
                h.named(b"Integer"),
                b"fromWord:",
                &[Operand::word(100)],
            );
            object(send(
                h.id,
                machine,
                b"runBlock:budget:",
                &[Operand::object(block), budget],
            ))
        } else {
            object(send(h.id, machine, b"runBlock:", &[Operand::object(block)]))
        };
        release(h.id, h.roots, pin);
        result
    }
}
// Stop at actual committed edges and inspect the discarded value while it is
// still a baseline Frame slot. Collection must keep that visible clone alive.
unsafe fn inspect_paused(h: &mut H, block: Oop, source: Oop, behavior: Oop, answer: Oop) {
    unsafe {
        let machine = object(send(h.id, h.named(b"Machine"), b"new", &[]));
        h.keep(machine);
        let one = send(h.id, h.named(b"Integer"), b"fromWord:", &[Operand::word(1)]);
        h.keep(object(one));
        let mut result = object(send(
            h.id,
            machine,
            b"runBlock:budget:",
            &[Operand::object(block), one],
        ));
        let mut observed = false;
        for _ in 0..24 {
            if result != machine {
                break;
            }
            let view = object(send(h.id, machine, b"currentFrameView", &[]));
            if view != h.named(b"nil") {
                h.keep(view);
                let slots = h.integer(object(send(h.id, view, b"slotCount", &[])));
                for index in 0..slots {
                    let index = send(
                        h.id,
                        h.named(b"Integer"),
                        b"fromWord:",
                        &[Operand::word(index as usize)],
                    );
                    let value = object(send(h.id, view, b"oopAt:", &[index]));
                    if value != source && vt(h, value) == behavior {
                        h.collect();
                        assert!(present(h.id, value), "paused Frame roots the visible clone");
                        observed = true;
                    }
                }
            }
            result = object(send(h.id, machine, b"continueWithBudget:", &[one]));
        }
        assert_eq!(result, answer);
        assert!(
            observed,
            "bounded execution exposes the actual discarded clone"
        );
    }
}
struct Forward {
    id: Protocol,
    gc: Oop,
    original: NativeEntry,
    calls: Cell<usize>,
    collect: bool,
    mutate: Option<(Oop, NativeEntry)>,
}
unsafe extern "C" fn forward(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Forward>();
        p.calls.set(p.calls.get() + 1);
        if p.collect {
            collect(p.id, p.gc);
        }
        // Change clone only after the real guard accepted it. The caller must
        // notice the epoch change before skipping the now-effectful send.
        let result = p.original.method.unwrap()(p.original.context, r, a, n);
        if let Some((behavior, entry)) = p.mutate {
            install(
                p.id,
                behavior,
                b"clone",
                entry.method.unwrap(),
                entry.context,
            );
        }
        result
    }
}

pub(super) unsafe fn check(h: &mut H) {
    unsafe {
        h.clear();
        h.collect();
        let gc = h.named(b"GC");
        let model = h.named(b"ObjectModel");
        let model_vt = vt(h, model);
        let guard = lookup(h, model, b"canDiscardClone:of:");
        let behavior = behavior(h.id);
        h.keep(behavior);
        let prototype = object(send(
            h.id,
            model,
            b"allocate:bytes:",
            &[Operand::object(behavior), Operand::word(0)],
        ));
        h.keep(prototype);
        let answer = object(send(
            h.id,
            h.named(b"Integer"),
            b"fromWord:",
            &[Operand::word(42)],
        ));
        h.keep(answer);
        h.bind(b"CloneSource", prototype);
        h.bind(b"CloneAnswer", answer);
        let candidate = h.eval(b"[:p :answer | [p clone]; answer]");
        h.bind(b"DiscardClone", candidate);
        let method = h.named(b"Method");
        let method_vt = vt(h, method);
        let attach_name = b"withDiscardedClone:answer:selector:return:in:";
        let attach = lookup(h, method, attach_name);
        install(h.id, method_vt, attach_name, unannotated, ptr::null_mut());
        let baseline = h.eval(b"[:p :answer | [p clone]; answer]");
        restore(h, method_vt, attach_name, attach);
        let caller = h
            .compile(b"let result := [DiscardClone value: CloneSource value: CloneAnswer]; result");
        let block = h.block(caller);
        send(h.id, gc, b"budget:", &[Operand::word(usize::MAX)]);
        // Intern observation selectors before measuring; inventory is independent
        // of GC's charge implementation and does not use an optimizer hit counter.
        h.id.intern_name(b"allocationCount").unwrap();
        let before = instances(h, behavior);
        assert_eq!(run(h, block, false), answer);
        assert_eq!(
            instances(h, behavior),
            before,
            "admitted source omits the physical clone"
        );
        install(
            h.id,
            model_vt,
            b"canDiscardClone:of:",
            refuse,
            ptr::null_mut(),
        );
        assert_eq!(run(h, block, false), answer);
        assert_eq!(
            instances(h, behavior),
            before + 1,
            "decline executes one ordinary clone"
        );
        restore(h, model_vt, b"canDiscardClone:of:", guard);
        assert_eq!(run(h, block, true), answer);
        assert_eq!(
            instances(h, behavior),
            before + 2,
            "bounded runs preserve the baseline steps"
        );

        inspect_paused(h, block, prototype, behavior, answer);

        let clone = lookup(h, prototype, b"clone");
        let mut changed = Box::new(Forward {
            id: h.id,
            gc,
            original: clone,
            calls: Cell::new(0),
            collect: false,
            mutate: None,
        });
        install(
            h.id,
            behavior,
            b"clone",
            forward,
            ptr::from_mut(&mut *changed).cast(),
        );
        assert_eq!(run(h, block, false), answer);
        assert_eq!(changed.calls.get(), 1);
        restore(h, behavior, b"clone", clone);

        // Each lower replacement makes the optimization decline. Observe actual
        // callback effects, not an assumed equivalence of allocation totals.
        for (receiver, selector) in [
            (h.id.receiver, b"lookup".as_slice()),
            (h.id.receiver, b"allocate"),
            (h.id.receiver, b"allocate:bytes:"),
            (model, b"allocate:bytes:"),
            (gc, b"prepareAllocation"),
            (h.named(b"Integer"), b"fromWord:"),
            (h.named(b"Integer"), b"wordOf:"),
        ] {
            let original = lookup(h, receiver, selector);
            let behavior_of_receiver = vt(h, receiver);
            let mut p = Box::new(Forward {
                id: h.id,
                gc,
                original,
                calls: Cell::new(0),
                collect: false,
                mutate: None,
            });
            install(
                h.id,
                behavior_of_receiver,
                selector,
                forward,
                ptr::from_mut(&mut *p).cast(),
            );
            let before = instances(h, behavior);
            assert_eq!(run(h, block, false), answer);
            assert!(p.calls.get() > 0, "replacement {selector:?} runs");
            assert_eq!(
                instances(h, behavior),
                before + 1,
                "effectful allocation path declines"
            );
            restore(h, behavior_of_receiver, selector, original);
        }

        let mut collecting = Box::new(Forward {
            id: h.id,
            gc,
            original: guard,
            calls: Cell::new(0),
            collect: true,
            mutate: None,
        });
        install(
            h.id,
            model_vt,
            b"canDiscardClone:of:",
            forward,
            ptr::from_mut(&mut *collecting).cast(),
        );
        assert_eq!(run(h, block, false), answer);
        assert!(collecting.calls.get() > 0);
        assert!(present(h.id, prototype) && present(h.id, answer));
        collecting.collect = false;
        collecting.mutate = Some((
            behavior,
            NativeEntry::new(forward, ptr::from_mut(&mut *changed).cast()),
        ));
        changed.calls.set(0);
        assert_eq!(run(h, block, false), answer);
        assert_eq!(
            changed.calls.get(),
            1,
            "guard-time replacement executes the new clone once"
        );
        restore(h, behavior, b"clone", clone);
        restore(h, model_vt, b"canDiscardClone:of:", guard);

        // An alias of the actual implementation is eligible; the selector's
        // spelling does not grant or deny semantic authority.
        install(
            h.id,
            behavior,
            b"discardableCopy",
            clone.method.unwrap(),
            clone.context,
        );
        let alias = h.eval(b"[:p :answer | [p discardableCopy]; answer]");
        h.bind(b"DiscardClone", alias);
        let before = instances(h, behavior);
        assert_eq!(run(h, block, false), answer);
        assert_eq!(instances(h, behavior), before);
        h.bind(b"DiscardClone", candidate);

        // Nonempty ordinary payloads remain baseline-only in this first slice.
        let nonempty = object(send(
            h.id,
            model,
            b"allocate:bytes:",
            &[Operand::object(behavior), Operand::word(size_of::<usize>())],
        ));
        h.keep(nonempty);
        h.bind(b"CloneSource", nonempty);
        let before = instances(h, behavior);
        assert_eq!(run(h, block, false), answer);
        assert_eq!(instances(h, behavior), before + 1);
        h.bind(b"CloneSource", prototype);

        // A lexical binding is inspectable even when its value is never returned.
        let bound = h.eval(b"[:p :answer | let discarded := [p clone]; answer]");
        h.bind(b"DiscardClone", bound);
        let before = instances(h, behavior);
        assert_eq!(run(h, block, false), answer);
        assert_eq!(instances(h, behavior), before + 1);
        h.bind(b"DiscardClone", candidate);

        // Returning the source itself retains the input's identity without a clone.
        h.bind(b"CloneAnswer", prototype);
        let before = instances(h, behavior);
        assert_eq!(run(h, block, false), prototype);
        assert_eq!(instances(h, behavior), before);
        h.bind(b"CloneAnswer", answer);

        // Generated index construction follows the source Unit's live Namespace.
        // Even an equivalent replacement receiver is outside this admission.
        let integer = h.named(b"Integer");
        let alternate = object(send(
            h.id,
            model,
            b"allocate:bytes:",
            &[Operand::object(vt(h, integer)), Operand::word(0)],
        ));
        h.keep(alternate);
        h.bind(b"Integer", alternate);
        let before = instances(h, behavior);
        assert_eq!(run(h, block, false), answer);
        assert_eq!(instances(h, behavior), before + 1);
        h.bind(b"Integer", integer);

        // A live clone is not eligible, even though the allocation path is the same.
        let escape = h.eval(b"[:p :answer | [p clone]]");
        h.bind(b"DiscardClone", escape);
        let before = instances(h, behavior);
        let copy = run(h, block, false);
        assert_ne!(copy, prototype);
        assert_ne!(copy, answer);
        assert_eq!(instances(h, behavior), before + 1);
        h.bind(b"DiscardClone", candidate);

        // Alternating repeated execution separates a local cost measurement from
        // correctness. Baseline/optimized run the same compiled caller and inputs.
        let mut elapsed = [0u128; 2];
        let mut allocations = [0usize; 2];
        for round in 0..6 {
            for mode in [round % 2, 1 - round % 2] {
                h.bind(
                    b"DiscardClone",
                    if mode == 0 { baseline } else { candidate },
                );
                h.collect();
                let count_before = send(h.id, h.id.receiver, b"allocationCount", &[])
                    .as_word()
                    .unwrap();
                let start = Instant::now();
                for _ in 0..30 {
                    assert_eq!(run(h, block, false), answer);
                }
                elapsed[mode] += start.elapsed().as_nanos();
                allocations[mode] += send(h.id, h.id.receiver, b"allocationCount", &[])
                    .as_word()
                    .unwrap()
                    - count_before;
            }
        }
        restore(h, model_vt, b"canDiscardClone:of:", guard);
        assert!(allocations[1] < allocations[0]);
        eprintln!(
            "discarded-clone: 180 runs baseline={}ns/{} allocations optimized={}ns/{} allocations",
            elapsed[0], allocations[0], elapsed[1], allocations[1]
        );
        for name in [
            b"DiscardClone".as_slice(),
            b"CloneSource",
            b"CloneAnswer",
            b"result",
        ] {
            h.bind(name, h.named(b"nil"));
        }
        h.clear();
        h.collect();
        assert_eq!(h.units(), 0);
        assert_eq!(h.native_bytes(), 0);
        send(h.id, gc, b"budget:", &[Operand::word(1 << 20)]);
    }
}
