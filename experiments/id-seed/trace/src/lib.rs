//! Reachability only, through a borrowed Id view and upper-owned descriptions.
use birnam_observation_abi::*;
use birnam_seed_abi::*;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::c_void;

struct Graph {
    allocations: BTreeMap<Oop, Allocation>,
    roots: BTreeSet<Oop>,
    edges: BTreeMap<Oop, Vec<Oop>>,
    described: BTreeSet<Oop>,
    error: Status,
}
impl Graph {
    fn fail(&mut self, error: Status) -> Status {
        self.error = error;
        error
    }
    fn root(&mut self, object: Oop) -> Status {
        if !self.allocations.contains_key(&object) {
            return self.fail(INVALID);
        }
        self.roots.insert(object);
        OK
    }
    fn edge(&mut self, from: Oop, to: Oop) -> Status {
        if !self.allocations.contains_key(&from) || !self.allocations.contains_key(&to) {
            return self.fail(INVALID);
        }
        self.edges.entry(from).or_default().push(to);
        OK
    }
}
unsafe extern "C" fn root(data: *mut c_void, object: Oop) -> Status {
    unsafe { &mut *data.cast::<Graph>() }.root(object)
}
unsafe extern "C" fn payload(
    data: *mut c_void,
    object: Oop,
    bytes: usize,
    offsets: *const usize,
    count: usize,
) -> Status {
    let graph = unsafe { &mut *data.cast::<Graph>() };
    let Some(allocation) = graph.allocations.get(&object) else {
        return graph.fail(INVALID);
    };
    if allocation.bytes != bytes
        || allocation.payload_owner != OWNER_PAYLOAD
        || !graph.described.insert(object)
        || (count > 0 && offsets.is_null())
        || count > bytes / size_of::<Oop>()
    {
        return graph.fail(INVALID);
    }
    let mut seen = BTreeSet::new();
    for index in 0..count {
        let offset = unsafe { *offsets.add(index) };
        if !offset.is_multiple_of(align_of::<Oop>())
            || !seen.insert(offset)
            || offset
                .checked_add(size_of::<Oop>())
                .is_none_or(|end| end > bytes)
        {
            return graph.fail(INVALID);
        }
        // Declared payload slots only; never reinterpret a behavior/header.
        let target = unsafe { ((object + offset) as *const Oop).read() };
        if target != 0 && graph.edge(object, target) != OK {
            return graph.error;
        }
    }
    OK
}

unsafe extern "C" fn observe(
    api: *const SeedApi,
    data: *mut c_void,
    _: Oop,
    args: *const Value,
    count: usize,
    _: *mut Value,
) -> Status {
    let request = unsafe { &*data.cast::<TraceRequest>() };
    let view = match unsafe { decode::<StorageView>(args, count) } {
        Ok(v) => v,
        Err(e) => return e,
    };
    unsafe { trace_view(api, request, view) }
}

unsafe fn trace_view(api: *const SeedApi, request: &TraceRequest, view: &StorageView) -> Status {
    if (request.root_count > 0 && request.roots.is_null())
        || (request.provider_count > 0 && request.providers.is_null())
    {
        return INVALID;
    }
    let mut graph = Graph {
        allocations: BTreeMap::new(),
        roots: BTreeSet::new(),
        edges: BTreeMap::new(),
        described: BTreeSet::new(),
        error: OK,
    };
    for i in 0..view.allocation_count {
        let allocation = unsafe { *view.allocations.add(i) };
        if allocation.object == 0
            || !allocation.object.is_multiple_of(align_of::<Oop>())
            || !matches!(allocation.payload_owner, OWNER_PAYLOAD | SEED_PAYLOAD)
            || graph
                .allocations
                .insert(allocation.object, allocation)
                .is_some()
        {
            return INVALID;
        }
    }
    for i in 0..view.root_count {
        if graph.root(unsafe { *view.roots.add(i) }) != OK {
            return graph.error;
        }
    }
    for i in 0..view.edge_count {
        let edge = unsafe { *view.edges.add(i) };
        if graph.edge(edge.from, edge.to) != OK {
            return graph.error;
        }
    }
    for i in 0..request.root_count {
        if graph.root(unsafe { *request.roots.add(i) }) != OK {
            return graph.error;
        }
    }
    let sink = ProviderSink {
        context: (&mut graph as *mut Graph).cast(),
        root,
        payload,
    };
    let arguments = [unsafe { record(&sink) }];
    for i in 0..request.provider_count {
        let provider = unsafe { *request.providers.add(i) };
        let mut out = Value::none();
        let status =
            unsafe { (provider.entry)(api, provider.data, 0, arguments.as_ptr(), 1, &mut out) };
        if status != OK {
            return status;
        }
        // A provider cannot hide a rejected description by ignoring its result.
        if graph.error != OK {
            return graph.error;
        }
    }
    if graph.allocations.values().any(|a| {
        a.payload_owner == OWNER_PAYLOAD && a.bytes != 0 && !graph.described.contains(&a.object)
    }) {
        return INCOMPLETE;
    }
    let mut reachable = BTreeSet::new();
    let mut pending: Vec<_> = graph.roots.iter().copied().collect();
    while let Some(object) = pending.pop() {
        if reachable.insert(object)
            && let Some(edges) = graph.edges.get(&object)
        {
            pending.extend(edges);
        }
    }
    let unreachable: Vec<_> = graph
        .allocations
        .keys()
        .copied()
        .filter(|object| !reachable.contains(object))
        .collect();
    let reachable: Vec<_> = reachable.into_iter().collect();
    let report = TraceReport {
        view,
        reachable: reachable.as_ptr(),
        reachable_count: reachable.len(),
        unreachable: unreachable.as_ptr(),
        unreachable_count: unreachable.len(),
    };
    let arguments = [unsafe { record(&report) }];
    let mut out = Value::none();
    unsafe { (request.report.entry)(api, request.report.data, 0, arguments.as_ptr(), 1, &mut out) }
}

/// Run at an explicit host safepoint with provider entries already resolved.
/// # Safety
/// Seed, request arrays and all method code/data must be live on the owner
/// thread. Providers/report callbacks may allocate their own native work buffers
/// but must not mutate object payloads or seed/module state. No pointer is retained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_trace_v1(
    api: *const SeedApi,
    request: *const TraceRequest,
) -> Status {
    if api.is_null() || request.is_null() {
        return INVALID;
    }
    if unsafe { (*api).version } != VERSION
        || unsafe { (*api).size } as usize != size_of::<SeedApi>()
    {
        return INVALID;
    }
    let request = unsafe { &*request };
    if (request.root_count > 0 && request.roots.is_null())
        || (request.provider_count > 0 && request.providers.is_null())
    {
        return INVALID;
    }
    let selector = match unsafe { intern(api, OBSERVE) } {
        Ok(s) => s,
        Err(e) => return e,
    };
    let visitor = Method {
        entry: observe,
        data: (request as *const TraceRequest).cast_mut().cast(),
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

/// Calculate from a borrowed view while the caller holds the Id phase.
/// # Safety
/// Same trusted lifetime/root/payload contract as birnam_trace_v1. The caller
/// must keep storage excluded throughout this call, including its report.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_trace_view_v1(
    api: *const SeedApi,
    _: *mut c_void,
    _: Oop,
    args: *const Value,
    count: usize,
    _: *mut Value,
) -> Status {
    let invocation = match unsafe { decode::<TraceViewRequest>(args, count) } {
        Ok(r) => r,
        Err(e) => return e,
    };
    if api.is_null() || invocation.view.is_null() || invocation.request.is_null() {
        return INVALID;
    }
    unsafe { trace_view(api, &*invocation.request, &*invocation.view) }
}
