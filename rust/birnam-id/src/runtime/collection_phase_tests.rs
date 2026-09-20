//! Also run unchanged against the preserved Runtime with its original Cell.
use super::*;
use crate::dispatch::build_bootstrap;
use birnam_abi::storage::{BUSY, OK, RequestV1, ViewV1};

fn code(phase: CollectionPhase) -> u32 {
    match phase {
        CollectionPhase::Idle => 0,
        CollectionPhase::Marking => 1,
        CollectionPhase::Finalizing => 2,
    }
}
thread_local! {
    static STORAGE_PHASE: Cell<u32> = const { Cell::new(0) };
    static VISITS: Cell<usize> = const { Cell::new(0) };
}
unsafe extern "C" fn visitor(_: *mut core::ffi::c_void, _: *const ViewV1) -> i32 {
    VISITS.set(VISITS.get() + 1);
    OK
}
unsafe extern "C" fn change_phase_then_observe(
    receiver: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    with_active(|runtime| collect::assign_phase_for_test(runtime, STORAGE_PHASE.get()));
    unsafe { storage::with_storage_view(receiver, args, count) }
}
unsafe extern "C" fn identity(receiver: Oop, _: *const Oop, _: usize) -> Oop {
    receiver
}

#[test]
fn runtime_phase_transcript() {
    let mut owners = vec![build_bootstrap(), build_bootstrap()];
    let first = owners.remove(0);
    let runtime = &first.runtime;
    assert_eq!(code(runtime.collection_phase.get()), 0);
    assert_eq!(code(owners[0].runtime.collection_phase.get()), 0);
    let mode = std::env::var("BIRNAM_PHASE_CASE").unwrap_or_default();
    if !mode.is_empty() {
        let phase = if mode == "allocate-marking" { 1 } else { 2 };
        collect::assign_phase_for_test(runtime, phase);
        if mode == "invalid-source-phase" {
            collect::assign_phase_for_test(runtime, u32::MAX);
        } else {
            assert!(mode == "allocate-marking" || mode == "allocate-finalizing");
            runtime.allocate_instance(core::ptr::without_provenance_mut(1), 0);
        }
        panic!("expected phase rejection");
    }
    for before in 0..3 {
        for after in 0..3 {
            collect::assign_phase_for_test(runtime, before);
            collect::assign_phase_for_test(runtime, after);
            assert_eq!(code(runtime.collection_phase.get()), after);
            assert_eq!(code(owners[0].runtime.collection_phase.get()), 0);
        }
    }
    println!("\nphase independent moved Runtime owners and all valid assignments");
    for phase in [1, 2] {
        collect::assign_phase_for_test(runtime, phase);
        // Aligned but unreadable pointers prove refusal precedes descriptor
        // and request access. Nested GC must return before touching its anchor.
        assert!(!unsafe { runtime.attach_trace(core::ptr::without_provenance(8)) });
        assert!(!unsafe { runtime.attach_collect(core::ptr::without_provenance(8)) });
        assert_eq!(
            unsafe { runtime.observe_storage(core::ptr::without_provenance(8)) },
            BUSY
        );
        assert_eq!(runtime.gc_collect(), 0);
        assert_eq!(code(runtime.collection_phase.get()), phase);
        println!("\nphase {phase}: nested GC and early trace/collect/storage refusals");
    }
    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = CollectionGuard {
            phase: &runtime.collection_phase,
        };
        collect::assign_phase_for_test(runtime, 1);
        std::panic::resume_unwind(Box::new(()));
    }));
    assert!(unwind.is_err());
    assert_eq!(code(runtime.collection_phase.get()), 0);
    runtime.require_allocation_allowed();
    println!("\nphase unwind resets to Idle and allocation is admitted");

    let parent = runtime.create_vtable(runtime.globals().object_vt);
    let child = runtime.create_vtable(parent);
    let selector = runtime.intern(b"phaseAnswer").unwrap();
    runtime.add_method(parent, selector, identity);
    let expected = runtime.lookup_method(parent, selector);
    assert!(expected.is_some());
    for phase in 0..3 {
        runtime.dispatch_cache.clear();
        dispatch_cache::HITS.set(0);
        dispatch_cache::MISSES.set(0);
        let _guard = runtime.enter_managed_turn();
        collect::assign_phase_for_test(runtime, phase);
        for _ in 0..2 {
            assert_eq!(runtime.cached_default_lookup(parent, selector), expected);
        }
        assert_eq!(
            (dispatch_cache::HITS.get(), dispatch_cache::MISSES.get()),
            (0, 0)
        );
        for _ in 0..2 {
            assert_eq!(runtime.cached_default_lookup(child, selector), expected);
        }
        let counts = (dispatch_cache::HITS.get(), dispatch_cache::MISSES.get());
        assert_eq!(counts, if phase == 0 { (1, 1) } else { (0, 0) });
        println!("\nphase {phase}: direct lookup bypass; inherited cache {counts:?}");
        collect::assign_phase_for_test(runtime, 0);
    }
    // Permission is created by ordinary observation and the replacement method
    // changes phase before calling the actual default storage method.
    let storage_selector = runtime.intern(b"withStorageView:").unwrap();
    runtime.add_method(
        runtime.globals().vtable_vt,
        storage_selector,
        change_phase_then_observe,
    );
    let request = RequestV1 {
        version: 1,
        byte_size: size_of::<RequestV1>(),
        context: core::ptr::null_mut(),
        visitor: Some(visitor),
    };
    for phase in 0..3 {
        STORAGE_PHASE.set(phase);
        VISITS.set(0);
        let result = unsafe { runtime.observe_storage(&request) };
        assert_eq!(result, if phase == 0 { OK } else { BUSY });
        assert_eq!(VISITS.get(), usize::from(phase == 0));
        println!(
            "\nphase {phase}: live default storage admission {result}, visits {}",
            VISITS.get()
        );
        collect::assign_phase_for_test(runtime, 0);
    }
    runtime.add_method(
        runtime.globals().vtable_vt,
        storage_selector,
        storage::with_storage_view,
    );
    drop(first);
    let second = &owners[0].runtime;
    second.allocate_instance(second.globals().object_vt, 0);
    assert_eq!(code(second.collection_phase.get()), 0);
    println!("\nphase remaining Runtime survives owner destruction");
}

#[test]
fn phase_failures_preserve_diagnostics() {
    use std::os::unix::process::ExitStatusExt;
    for mode in [
        "allocate-marking",
        "allocate-finalizing",
        "invalid-source-phase",
    ] {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "runtime::collection_phase_tests::runtime_phase_transcript",
                "--nocapture",
            ])
            .env("BIRNAM_PHASE_CASE", mode)
            .output()
            .unwrap();
        assert_eq!(result.status.signal(), Some(6), "{mode}");
        let message = if mode == "invalid-source-phase" {
            "invalid collection phase"
        } else {
            "id allocation attempted during collection"
        };
        assert_eq!(
            String::from_utf8(result.stderr).unwrap(),
            format!("birnam-id invariant failed: {message}\n")
        );
    }
}
