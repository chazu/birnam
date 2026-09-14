//! Source-region admission around real automatic-collection boundaries.
use super::native_tier::closure_elision::{completed_calls, counters};
use super::*;
use core::cell::RefCell;

#[derive(Default)]
struct Observation {
    machine: Oop,
    input: Oop,
    other: Oop,
    collections: usize,
    bytes: usize,
    pc: usize,
    pcs: usize,
    slots: usize,
}
thread_local! {
    // Untraced. The registered source invocation and explicit driver roots own
    // these objects; the provider records effects only during the measured run.
    static OBSERVATION: RefCell<Option<Observation>> = const { RefCell::new(None) };
}

unsafe extern "C" fn observe(mark: crate::abi::MarkFn) {
    OBSERVATION.with_borrow_mut(|observation| {
        let Some(o) = observation else { return };
        with_active(|r| {
            let cap = r.stackless_machine_capability(o.machine).unwrap();
            {
                let state = unsafe { &mut *r.state.get() };
                let arena = state
                    .execution
                    .machine(current_thread_token(), cap)
                    .unwrap()
                    .arena();
                let frame = arena.current_handle().unwrap();
                o.collections += 1;
                o.bytes = state.heap.bytes_since_collection();
                o.pc = arena.program_counter(frame).unwrap();
                o.pcs = arena.descriptor(frame).unwrap().program_counter_count();
                o.slots = arena.descriptor(frame).unwrap().slot_count();
                // This is a private all-Oop fixture object, not a store Cell.
                // A foreign root callback may change its field without sends
                // or allocation. Mark the installed value even if input was
                // already visited earlier in this collection.
                assert!(state.heap.write_payload_word(o.input, 0, o.other.addr()));
            }
            unsafe {
                mark(o.input);
                mark(o.other);
            }
        });
    });
}

pub(super) unsafe extern "C" fn check(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 3);
    let a = unsafe { core::slice::from_raw_parts(args, count) };
    with_source_probe_native_roots(|r| {
        let factory = a[0];
        let gap = r.integer_value(a[1]) as usize;
        let escaping = r.integer_value(a[2]) != 0;
        // Numeric callees retain their original boxes, so a 32-call region
        // needs a larger initial gap than the alias-only fixture. This bounds
        // test input only; the production collection threshold is unchanged.
        assert!((64..=65536).contains(&gap) && gap.is_multiple_of(8));
        assert!(OBSERVATION.with_borrow(Option::is_none));
        assert!(unsafe {
            (*r.state.get())
                .metadata
                .automatic_collection_due(crate::gc::AUTO_COLLECTION_THRESHOLD)
        });
        let vt = r.create_vtable(r.globals().object_vt);
        r.set_layout(vt, core::ptr::null(), 0, crate::abi::SLOT_OOP);
        let input = r.allocate_instance(vt, core::mem::size_of::<Oop>());
        let _input = ScopedOopRootGuard::new(r, input);
        let other = r.make_integer(99);
        let _other = ScopedOopRootGuard::new(r, other);
        let nil = r.canonical().nil;
        assert!(unsafe {
            (*r.state.get())
                .heap
                .write_payload_word(input, 0, nil.addr())
        });
        let selector = r.intern(b"value:").unwrap();
        for _ in 0..128 {
            r.send_slice(factory, selector, &[input]);
        }
        let machine = r.create_stackless_machine().unwrap();
        let _machine = ScopedOopRootGuard::new(r, machine);
        let StacklessTarget::Registered { entry } =
            r.resolve_stackless_target(factory, selector).unwrap()
        else {
            panic!("budget fixture must be an actual registered source factory");
        };
        let target = r
            .materialize_registered_target(entry, factory, &[input])
            .unwrap();
        r.gc_collect_for_source_probe();
        let start = crate::gc::AUTO_COLLECTION_THRESHOLD - gap;
        let current = unsafe { (*r.state.get()).heap.bytes_since_collection() };
        r.allocate_instance(
            r.globals().object_vt,
            start - current - Heap::allocation_total_size(0).unwrap(),
        );
        assert_eq!(
            unsafe { (*r.state.get()).heap.bytes_since_collection() },
            start
        );
        r.gc_register_root_provider(observe);
        OBSERVATION.replace(Some(Observation {
            machine,
            input,
            other,
            ..Observation::default()
        }));
        let before = counters();
        let completed_before = completed_calls();
        let result = r
            .drive_stackless_with_resolver(
                machine,
                target.descriptor,
                &target.initial_oops,
                Some(entry),
                |runtime, receiver, selector, _| {
                    runtime.resolve_stackless_target(receiver, selector)
                },
            )
            .unwrap();
        let observation = OBSERVATION.take().unwrap();
        assert!(observation.collections <= 1);
        let heap = unsafe { &(*r.state.get()).heap };
        if escaping {
            assert_eq!(heap.read_payload_word(result, 1), Some(input.addr()));
        } else {
            assert_eq!(result, input);
        }
        assert_eq!(
            heap.read_payload_word(input, 0),
            Some(if observation.collections == 0 {
                nil.addr()
            } else {
                other.addr()
            })
        );
        let after = counters();
        println!(
            "closure-budget gap={gap} threshold={} escaping={} collections={} trace_bytes={} trace_pc={} trace_pcs={} trace_slots={} remaining_bytes={} physical_saved={} attempts={} hits={} materialized={} completed_calls={}",
            crate::gc::AUTO_COLLECTION_THRESHOLD,
            usize::from(escaping),
            observation.collections,
            observation.bytes,
            observation.pc,
            observation.pcs,
            observation.slots,
            heap.bytes_since_collection(),
            heap.allocation_count() - heap.physical_allocation_count(),
            after.0 - before.0,
            after.1 - before.1,
            after.2 - before.2,
            completed_calls() - completed_before
        );
        r.destroy_stackless_machine(machine).unwrap();
        nil
    })
}
