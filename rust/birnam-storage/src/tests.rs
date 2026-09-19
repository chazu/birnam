use super::*;

struct Probe {
    description: DescriptionV1,
    repeat: bool,
    sink_context: *mut c_void,
    saved_root: Option<unsafe extern "C" fn(*mut c_void, Oop) -> Status>,
    visits: usize,
}
unsafe extern "C" fn provide(
    context: *mut c_void,
    _: *const ViewV1,
    sink: *const ProviderSinkV1,
) -> Status {
    let (probe, sink) = unsafe { (&mut *context.cast::<Probe>(), &*sink) };
    probe.sink_context = sink.context;
    probe.saved_root = Some(sink.root);
    unsafe { (sink.describe)(sink.context, &probe.description) };
    if probe.repeat {
        unsafe { (sink.describe)(sink.context, &probe.description) };
    }
    OK // deliberately ignores rejection; the validator must latch it
}
unsafe extern "C" fn visit(context: *mut c_void, _: *const DescribedViewV1) -> Status {
    unsafe {
        (*context.cast::<Probe>()).visits += 1;
    }
    OK
}
#[test]
fn claims_use_allocation_identity_and_sink_errors_cannot_be_ignored() {
    let mut words = [core::ptr::null_mut::<c_void>(); 2];
    let object = words.as_mut_ptr().cast();
    let allocation = AllocationV1 {
        object,
        identity: 10,
        requested_size: 16,
        padded_size: 16,
        allocation_bytes: 24,
        behavior: core::ptr::null_mut(),
        flags: 0,
        trace_hook: None,
    };
    let view = ViewV1 {
        version: VERSION,
        byte_size: size_of::<ViewV1>(),
        runtime_identity: 2,
        observation_identity: 1,
        allocations: &allocation,
        allocation_count: 1,
        roots: core::ptr::null(),
        root_count: 0,
        edges: core::ptr::null(),
        edge_count: 0,
        legacy_root_providers: core::ptr::null(),
        legacy_root_provider_count: 0,
        deferred_count: 0,
        deferred_bytes: 0,
        coverage_gaps: NATIVE_ROOTS_UNOBSERVED,
        gc_precise: 0,
    };
    let mut probe = Probe {
        description: DescriptionV1 {
            runtime_identity: 2,
            allocation_identity: 10,
            object,
            extent: 16,
            strong_offsets: core::ptr::null(),
            strong_offset_count: 0,
        },
        repeat: false,
        sink_context: core::ptr::null_mut(),
        saved_root: None,
        visits: 0,
    };
    let context = (&mut probe as *mut Probe).cast();
    let provider = ProviderV1 {
        context,
        provide: Some(provide),
    };
    let mut request = DescribeRequestV1 {
        version: VERSION,
        byte_size: size_of::<DescribeRequestV1>(),
        providers: &provider,
        provider_count: 1,
        reject_gaps: PAYLOAD_DESCRIPTIONS_MISSING,
        context,
        visitor: Some(visit),
    };
    assert_eq!(unsafe { birnam_storage_describe_v1(&view, &request) }, OK);
    assert_eq!(probe.visits, 1);
    assert_eq!(
        unsafe { probe.saved_root.unwrap()(probe.sink_context, object) },
        BUSY
    );
    probe.description.allocation_identity = 9; // same address, retired identity
    assert_eq!(
        unsafe { birnam_storage_describe_v1(&view, &request) },
        INVALID
    );
    probe.description.allocation_identity = 10;
    probe.description.runtime_identity = 3;
    assert_eq!(
        unsafe { birnam_storage_describe_v1(&view, &request) },
        INVALID
    );
    probe.description.runtime_identity = 2;
    probe.repeat = true;
    assert_eq!(
        unsafe { birnam_storage_describe_v1(&view, &request) },
        INVALID
    );
    assert_eq!(probe.visits, 1);
    request.provider_count = 0;
    assert_eq!(
        unsafe { birnam_storage_describe_v1(&view, &request) },
        INCOMPLETE
    );
    request.reject_gaps = 0;
    assert_eq!(unsafe { birnam_storage_describe_v1(&view, &request) }, OK);
    request.reject_gaps = NATIVE_ROOTS_UNOBSERVED;
    assert_eq!(
        unsafe { birnam_storage_describe_v1(&view, &request) },
        INCOMPLETE
    );
}
