//! Collection policy and upper metadata retirement, independent of Id storage.
use birnam_observation_abi::*;
use birnam_seed_abi::*;
use std::collections::BTreeSet;
use std::ffi::c_void;
use std::mem::MaybeUninit;

struct Collection<'a> {
    request: &'a CollectRequest,
    result: &'a mut CollectionResult,
}
#[derive(Default)]
struct Candidates {
    objects: Vec<Oop>,
    bytes: usize,
    reported: bool,
}
unsafe extern "C" fn candidates(
    _: *const SeedApi,
    data: *mut c_void,
    _: Oop,
    args: *const Value,
    count: usize,
    _: *mut Value,
) -> Status {
    let output = unsafe { &mut *data.cast::<Candidates>() };
    if output.reported {
        return INVALID;
    }
    let report = match unsafe { decode::<TraceReport>(args, count) } {
        Ok(r) => r,
        Err(e) => return e,
    };
    let view = unsafe { &*report.view };
    let mut dead = BTreeSet::new();
    for i in 0..report.unreachable_count {
        if !dead.insert(unsafe { *report.unreachable.add(i) }) {
            return INVALID;
        }
    }
    let mut found = 0;
    for i in 0..view.allocation_count {
        let a = unsafe { *view.allocations.add(i) };
        if dead.contains(&a.object) {
            if a.payload_owner != OWNER_PAYLOAD {
                return INVALID;
            }
            let Some(bytes) = output.bytes.checked_add(a.bytes) else {
                return INVALID;
            };
            output.bytes = bytes;
            found += 1;
        }
    }
    if found != dead.len() {
        return INVALID;
    }
    output.objects = dead.into_iter().collect();
    output.reported = true;
    OK
}
struct Retirement<'a> {
    participants: &'a [CollectionParticipant],
    objects: &'a [Oop],
}
unsafe extern "C" fn retire(
    api: *const SeedApi,
    data: *mut c_void,
    _: Oop,
    _: *const Value,
    _: usize,
    _: *mut Value,
) -> Status {
    let retirement = unsafe { &*data.cast::<Retirement<'_>>() };
    let mut prepared: Vec<RetirementPlan> = Vec::with_capacity(retirement.participants.len());
    for participant in retirement.participants {
        let mut plan = MaybeUninit::<RetirementPlan>::uninit();
        let request = RetirementRequest {
            objects: retirement.objects.as_ptr(),
            count: retirement.objects.len(),
            plan: plan.as_mut_ptr(),
        };
        let args = [unsafe { record(&request) }];
        let method = participant.prepare;
        let mut ignored = Value::none();
        let status = unsafe { (method.entry)(api, method.data, 0, args.as_ptr(), 1, &mut ignored) };
        if status != OK {
            for plan in prepared.into_iter().rev() {
                unsafe { (plan.abort)(plan.data) };
            }
            return status;
        }
        prepared.push(unsafe { plan.assume_init() });
    }
    // Every fallible participant has accepted. Commits only swap prepared
    // metadata; they cannot allocate, send, run finalizers or report failure.
    for plan in prepared {
        unsafe { (plan.commit)(plan.data) };
    }
    OK
}
unsafe extern "C" fn collect_view(
    api: *const SeedApi,
    data: *mut c_void,
    _: Oop,
    args: *const Value,
    count: usize,
    _: *mut Value,
) -> Status {
    let collection = unsafe { &mut *data.cast::<Collection<'_>>() };
    let capability = match unsafe { decode::<ReclamationView>(args, count) } {
        Ok(r) => r,
        Err(e) => return e,
    };
    let view = unsafe { &*capability.view };
    let request = collection.request;
    let participants = if request.participant_count == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(request.participants, request.participant_count) }
    };
    let providers: Vec<_> = participants.iter().map(|p| p.provider).collect();
    let mut roots: Vec<_> = (0..request.root_count)
        .map(|i| unsafe { *request.roots.add(i) })
        .collect();
    // This first policy retains all seed-owned objects as actual roots. Their
    // outgoing edges must retain any ordinary objects they reference as well.
    for i in 0..view.allocation_count {
        let allocation = unsafe { *view.allocations.add(i) };
        if allocation.payload_owner == SEED_PAYLOAD {
            roots.push(allocation.object);
        }
    }
    let mut output = Candidates::default();
    let trace_request = TraceRequest {
        roots: roots.as_ptr(),
        root_count: roots.len(),
        providers: providers.as_ptr(),
        provider_count: providers.len(),
        report: Method {
            entry: candidates,
            data: (&mut output as *mut Candidates).cast(),
        },
    };
    let invocation = TraceViewRequest {
        view,
        request: &trace_request,
    };
    let args = [unsafe { record(&invocation) }];
    let mut ignored = Value::none();
    let trace = request.trace;
    let status = unsafe { (trace.entry)(api, trace.data, 0, args.as_ptr(), 1, &mut ignored) };
    if status != OK {
        return status;
    }
    if !output.reported {
        return INVALID;
    }
    if output.objects.is_empty() {
        return OK;
    }
    let mut retirement = Retirement {
        participants,
        objects: &output.objects,
    };
    let release = ReleaseRequest {
        epoch: capability.epoch,
        objects: output.objects.as_ptr(),
        count: output.objects.len(),
        retire: Method {
            entry: retire,
            data: (&mut retirement as *mut Retirement<'_>).cast(),
        },
    };
    let args = [unsafe { record(&release) }];
    let method = capability.release;
    let status = unsafe { (method.entry)(api, method.data, 0, args.as_ptr(), 1, &mut ignored) };
    if status != OK {
        return status;
    }
    // The view's allocation pointers are invalid after successful release.
    collection.result.retired_objects = output.objects.len();
    collection.result.retired_bytes = output.bytes;
    OK
}

/// Collect ordinary storage at an explicit owner-thread host safepoint.
/// # Safety
/// All records/code/data are live and pinned. The host must supply every root
/// and metadata owner, including weak caches with no payload descriptions.
/// Prepared callbacks obey the no-fail commit contract. No native stack scan.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_collect_v1(
    api: *const SeedApi,
    request: *const CollectRequest,
    result: *mut CollectionResult,
) -> Status {
    if api.is_null() || request.is_null() || result.is_null() {
        return INVALID;
    }
    unsafe { result.write(CollectionResult::default()) };
    if unsafe { (*api).version } != VERSION
        || unsafe { (*api).size } as usize != size_of::<SeedApi>()
    {
        return INVALID;
    }
    let request = unsafe { &*request };
    if (request.root_count != 0 && request.roots.is_null())
        || (request.participant_count != 0 && request.participants.is_null())
    {
        return INVALID;
    }
    let selector = match unsafe { intern(api, RECLAIM) } {
        Ok(s) => s,
        Err(e) => return e,
    };
    let mut collection = Collection {
        request,
        result: unsafe { &mut *result },
    };
    let visitor = Method {
        entry: collect_view,
        data: (&mut collection as *mut Collection<'_>).cast(),
    };
    match unsafe {
        call(
            api,
            (*api).roots.vtable,
            selector,
            &[Value::method(visitor)],
        )
    } {
        Ok(_) => OK,
        Err(e) => e,
    }
}
