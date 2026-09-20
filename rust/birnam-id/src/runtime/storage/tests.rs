use super::*;
use crate::dispatch::build_bootstrap;

#[derive(Default)]
struct Snapshot {
    runtime: u64,
    epoch: u64,
    allocations: Vec<AllocationV1>,
    roots: Vec<Oop>,
    edges: Vec<EdgeV1>,
    deferred_count: usize,
    deferred_bytes: usize,
    gaps: u32,
    providers: Vec<crate::abi::RootProvider>,
}
unsafe extern "C" fn capture(context: *mut core::ffi::c_void, view: *const ViewV1) -> Status {
    let (output, view) = unsafe { (&mut *context.cast::<Snapshot>(), &*view) };
    output.runtime = view.runtime_identity;
    output.epoch = view.observation_identity;
    output.allocations =
        unsafe { crate::abi::borrowed_slice(view.allocations, view.allocation_count) }
            .unwrap()
            .to_vec();
    output.roots = unsafe { crate::abi::borrowed_slice(view.roots, view.root_count) }
        .unwrap()
        .to_vec();
    output.edges = unsafe { crate::abi::borrowed_slice(view.edges, view.edge_count) }
        .unwrap()
        .to_vec();
    output.deferred_count = view.deferred_count;
    output.deferred_bytes = view.deferred_bytes;
    output.gaps = view.coverage_gaps;
    output.providers = unsafe { crate::abi::borrowed_slice(view.legacy_root_providers, view.legacy_root_provider_count) }.unwrap().to_vec();
    OK
}
fn snapshot(runtime: &Runtime) -> Snapshot {
    let mut output = Snapshot::default();
    let request = RequestV1 {
        version: VERSION,
        byte_size: size_of::<RequestV1>(),
        context: (&mut output as *mut Snapshot).cast(),
        visitor: Some(capture),
    };
    assert_eq!(unsafe { runtime.observe_storage(&request) }, OK);
    output
}
#[test]
fn actual_sizes_clone_headers_and_marks_are_observed_without_changes() {
    let bootstrap = build_bootstrap();
    let runtime = &bootstrap.runtime;
    let vt = runtime.globals().object_vt;
    let objects: Vec<_> = [0, 1, 7, 8, 9]
        .map(|size| runtime.allocate_instance(vt, size))
        .into();
    let clone = runtime.clone_object(objects[4]);
    let reassociated = runtime.create_vtable(vt);
    runtime.reassociate_behavior(objects[4], reassociated);
    runtime.export(b"observed-root", objects[4]).unwrap();
    runtime.gc_mark_floor_roots_for_test();
    let marks: Vec<_> = objects
        .iter()
        .map(|&object| runtime.is_marked_for_test(object))
        .collect();
    let counts = (runtime.gc_live_count(), runtime.gc_live_bytes(), unsafe {
        (*runtime.state.get()).heap.bytes_since_collection()
    });
    let observed = snapshot(runtime);
    for (&object, size) in objects.iter().zip([0usize, 1, 7, 8, 9]) {
        let entry = observed
            .allocations
            .iter()
            .find(|entry| entry.object == object)
            .unwrap();
        assert_eq!(entry.requested_size, size);
        assert_eq!(entry.padded_size, size.next_multiple_of(size_of::<Oop>()));
        assert_eq!(entry.allocation_bytes, entry.padded_size + size_of::<Oop>());
    }
    assert_eq!(
        observed
            .allocations
            .iter()
            .find(|entry| entry.object == clone)
            .unwrap()
            .requested_size,
        16
    );
    assert!(observed.edges.contains(&EdgeV1 {
        from: objects[4],
        to: reassociated
    }));
    assert!(observed.roots.contains(&objects[4]));
    assert_eq!(
        marks,
        objects
            .iter()
            .map(|&object| runtime.is_marked_for_test(object))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        counts,
        (runtime.gc_live_count(), runtime.gc_live_bytes(), unsafe {
            (*runtime.state.get()).heap.bytes_since_collection()
        })
    );
    let next = snapshot(runtime);
    assert_eq!(observed.runtime, next.runtime);
    assert!(next.epoch > observed.epoch);
    assert_eq!(
        observed
            .allocations
            .iter()
            .map(|entry| entry.identity)
            .collect::<Vec<_>>(),
        next.allocations
            .iter()
            .map(|entry| entry.identity)
            .collect::<Vec<_>>()
    );
}

#[test]
fn deferred_counts_and_weak_symbols_remain_distinct_from_physical_storage() {
    let bootstrap = build_bootstrap();
    let runtime = &bootstrap.runtime;
    let symbol = runtime.intern(b"only-weak-observation-symbol").unwrap();
    assert!(snapshot(runtime).roots.contains(&symbol));
    runtime.enable_symbol_compaction_for_test();
    assert!(!snapshot(runtime).roots.contains(&symbol));
    let vt = runtime.globals().object_vt;
    unsafe {
        let heap = &mut (*runtime.state.get()).heap;
        let recipe = heap.prepare_virtual_bytes(vt, &[0xa5; 9]).unwrap();
        assert!(heap.record_dead_allocation(recipe));
    }
    let counts = (runtime.gc_live_count(), runtime.gc_live_bytes(), unsafe {
        (*runtime.state.get()).heap.bytes_since_collection()
    });
    let observed = snapshot(runtime);
    assert_eq!(observed.deferred_count, 1);
    assert_eq!(observed.deferred_bytes, 24);
    assert_ne!(observed.gaps & DEFERRED_IDENTITIES_UNAVAILABLE, 0);
    assert_eq!(
        observed.allocations.len() + observed.deferred_count,
        counts.0
    );
    assert_eq!(
        observed
            .allocations
            .iter()
            .map(|entry| entry.allocation_bytes)
            .sum::<usize>()
            + observed.deferred_bytes,
        counts.1
    );
    assert_eq!(
        counts,
        (runtime.gc_live_count(), runtime.gc_live_bytes(), unsafe {
            (*runtime.state.get()).heap.bytes_since_collection()
        })
    );
}

unsafe extern "C" fn nested_and_fail(context: *mut core::ffi::c_void, _: *const ViewV1) -> Status {
    let runtime = unsafe { &*context.cast::<Runtime>() };
    let nested = RequestV1 {
        version: VERSION,
        byte_size: size_of::<RequestV1>(),
        context: core::ptr::null_mut(),
        visitor: Some(capture),
    };
    if unsafe { runtime.observe_storage(&nested) } != BUSY {
        return INVALID;
    }
    77
}
#[test]
fn host_permit_rejects_active_execution_and_restores_access_after_error() {
    let bootstrap = build_bootstrap();
    let runtime = &bootstrap.runtime;
    let request = RequestV1 {
        version: VERSION,
        byte_size: size_of::<RequestV1>(),
        context: (runtime as *const Runtime).cast_mut().cast(),
        visitor: Some(nested_and_fail),
    };
    let read = runtime.enter_dispatch_read();
    assert_eq!(unsafe { runtime.observe_storage(&request) }, BUSY);
    drop(read);
    assert_eq!(unsafe { runtime.observe_storage(&request) }, 77);
    assert!(
        !runtime
            .intern(b"after-observation-error")
            .unwrap()
            .is_null()
    );
    assert_eq!(
        snapshot(runtime).gaps & NATIVE_ROOTS_UNOBSERVED,
        NATIVE_ROOTS_UNOBSERVED
    );
    let other = build_bootstrap();
    assert_ne!(snapshot(runtime).runtime, snapshot(&other.runtime).runtime);
}

#[test]
fn detached_storage_loses_its_identity_before_subsequent_allocation() {
    let bootstrap = build_bootstrap();
    let runtime = &bootstrap.runtime;
    let object = runtime.allocate_instance(runtime.globals().object_vt, 9);
    let previous = snapshot(runtime)
        .allocations
        .into_iter()
        .find(|entry| entry.object == object)
        .unwrap()
        .identity;
    unsafe {
        let detached = (*runtime.state.get())
            .heap
            .detach_dead_collectable(object.addr())
            .unwrap();
        detached.reclaim();
    }
    assert!(
        snapshot(runtime)
            .allocations
            .iter()
            .all(|entry| entry.identity != previous)
    );
    let next = runtime.allocate_instance(runtime.globals().object_vt, 9);
    let current = snapshot(runtime)
        .allocations
        .into_iter()
        .find(|entry| entry.object == next)
        .unwrap();
    assert!(current.identity > previous);
    assert_eq!(current.requested_size, 9);
}

#[test]
fn root_registry_diagnostic_snapshot_preserves_duplicates_without_invocation() {
    unsafe extern "C" fn uncalled(_: crate::abi::MarkFn) { panic!("diagnostics invoked a provider"); }
    let bootstrap = build_bootstrap();
    let runtime = &bootstrap.runtime;
    let before = snapshot(runtime);
    runtime.gc_register_root_provider(uncalled);
    runtime.gc_register_root_provider(uncalled);
    let api = runtime.root_registry();
    // Restricted storage observation uses immutable owned operations, not this
    // mutable public descriptor or its ordinary-admission callback.
    unsafe { (*api).context = core::ptr::null_mut(); (*api).register = None; }
    let observed = snapshot(runtime);
    assert_eq!(observed.providers.len(), before.providers.len() + 2);
    assert!(observed.providers[before.providers.len()..].iter().all(|provider|
        core::ptr::fn_addr_eq(*provider, uncalled as crate::abi::RootProvider)));
    assert_ne!(observed.gaps & LEGACY_ROOTS_UNOBSERVED, 0);
    runtime.gc_register_root_provider(uncalled);
    assert_eq!(observed.providers.len(), before.providers.len() + 2);
    assert_eq!(snapshot(runtime).providers.len(), before.providers.len() + 3);
}
