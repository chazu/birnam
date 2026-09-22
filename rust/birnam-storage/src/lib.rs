//! Validation of owner-supplied storage descriptions above Id.
//! Native observation scopes are independent of description-provider scopes.
//! No heap, registry, mark algorithm or GC policy.
mod observation;
mod deferred_identity;
mod deferred_index;
use birnam_abi::storage::*;
use birnam_abi::{Oop, borrowed_slice};
use core::cell::Cell;
use core::ffi::c_void;
use std::collections::{BTreeMap, BTreeSet};

struct Builder {
    token: usize,
    runtime: u64,
    allocations: BTreeMap<usize, AllocationV1>,
    claimed: BTreeSet<u64>,
    roots: Vec<Oop>,
    descriptions: Vec<(DescriptionV1, Vec<usize>)>,
    edges: Vec<EdgeV1>,
    error: Status,
}
thread_local! {
    static ACTIVE: Cell<*mut Builder> = const { Cell::new(core::ptr::null_mut()) };
    static NEXT_TOKEN: Cell<usize> = const { Cell::new(0) };
}
struct Scope;
impl Drop for Scope {
    fn drop(&mut self) {
        ACTIVE.set(core::ptr::null_mut());
    }
}

fn with_sink(context: *mut c_void, action: impl FnOnce(&mut Builder) -> Status) -> Status {
    let pointer = ACTIVE.get();
    if pointer.is_null() {
        return BUSY;
    }
    // Context is a non-dereferenced, never-reused token, not a borrowed address.
    // No foreign callback runs while this exclusive native-state borrow exists.
    let builder = unsafe { &mut *pointer };
    if context.addr() != builder.token {
        return BUSY;
    }
    if builder.error != OK {
        return builder.error;
    }
    let status = action(builder);
    if status != OK {
        builder.error = status;
    }
    status
}
unsafe extern "C" fn root(context: *mut c_void, object: Oop) -> Status {
    with_sink(context, |builder| {
        if object.is_null() {
            return OK;
        }
        if !builder.allocations.contains_key(&object.addr()) {
            return INVALID;
        }
        builder.roots.push(object);
        OK
    })
}
unsafe extern "C" fn payload(context: *mut c_void, description: *const DescriptionV1) -> Status {
    with_sink(context, |builder| {
        if description.is_null()
            || !description
                .addr()
                .is_multiple_of(align_of::<DescriptionV1>())
        {
            return INVALID;
        }
        let description = unsafe { *description };
        let Some(allocation) = builder.allocations.get(&description.object.addr()) else {
            return INVALID;
        };
        if description.runtime_identity != builder.runtime
            || description.allocation_identity != allocation.identity
            || description.extent != allocation.requested_size
            || allocation.flags & CORE_PAYLOAD != 0
            || builder.claimed.contains(&allocation.identity)
        {
            return INVALID;
        }
        let Some(offsets) = (unsafe {
            borrowed_slice(description.strong_offsets, description.strong_offset_count)
        }) else {
            return INVALID;
        };
        let mut unique = BTreeSet::new();
        let mut edges = Vec::new();
        for &offset in offsets {
            if !offset.is_multiple_of(align_of::<Oop>())
                || offset
                    .checked_add(size_of::<Oop>())
                    .is_none_or(|end| end > description.extent)
                || !unique.insert(offset)
            {
                return INVALID;
            }
            // Exact allocation membership, extent and alignment have been checked.
            // The enclosing protected view keeps these payload bytes stable.
            let to = unsafe {
                description
                    .object
                    .cast::<u8>()
                    .add(offset)
                    .cast::<Oop>()
                    .read()
            };
            if !to.is_null() {
                if !builder.allocations.contains_key(&to.addr()) {
                    return INVALID;
                }
                edges.push(EdgeV1 {
                    from: description.object,
                    to,
                });
            }
        }
        builder.claimed.insert(description.allocation_identity);
        builder.descriptions.push((description, offsets.to_vec()));
        builder.edges.extend(edges);
        OK
    })
}

/// Validate prepared module descriptions inside one live storage observation.
/// # Safety
/// All input arrays, payloads, callbacks and callback data remain live throughout
/// this call. Providers obey the observation-only contract and may use a sink
/// only during their own invocation. The view comes from the selected live Id.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_storage_describe_v1(
    view: *const ViewV1,
    request: *const DescribeRequestV1,
) -> Status {
    if !ACTIVE.get().is_null() {
        return BUSY;
    }
    if view.is_null()
        || request.is_null()
        || !view.addr().is_multiple_of(align_of::<ViewV1>())
        || !request
            .addr()
            .is_multiple_of(align_of::<DescribeRequestV1>())
    {
        return INVALID;
    }
    let (view, request) = unsafe { (&*view, &*request) };
    if view.version != VERSION
        || view.byte_size != size_of::<ViewV1>()
        || view.runtime_identity == 0
        || view.observation_identity == 0
        || request.version != VERSION
        || request.byte_size != size_of::<DescribeRequestV1>()
    {
        return INVALID;
    }
    let Some(visitor) = request.visitor else {
        return INVALID;
    };
    let Some(allocations) = (unsafe { borrowed_slice(view.allocations, view.allocation_count) })
    else {
        return INVALID;
    };
    let Some(roots) = (unsafe { borrowed_slice(view.roots, view.root_count) }) else {
        return INVALID;
    };
    let Some(providers) = (unsafe { borrowed_slice(request.providers, request.provider_count) })
    else {
        return INVALID;
    };
    let mut by_address = BTreeMap::new();
    let mut identities = BTreeSet::new();
    for &allocation in allocations {
        if allocation.object.is_null()
            || allocation.identity == 0
            || allocation.requested_size > allocation.padded_size
            || !allocation.object.addr().is_multiple_of(align_of::<Oop>())
            || !identities.insert(allocation.identity)
            || by_address
                .insert(allocation.object.addr(), allocation)
                .is_some()
        {
            return INVALID;
        }
    }
    if roots
        .iter()
        .any(|root| !root.is_null() && !by_address.contains_key(&root.addr()))
    {
        return INVALID;
    }
    let mut builder = Builder {
        token: 0,
        runtime: view.runtime_identity,
        allocations: by_address,
        claimed: BTreeSet::new(),
        roots: roots.to_vec(),
        descriptions: Vec::new(),
        edges: Vec::new(),
        error: OK,
    };
    for provider in providers {
        let Some(provide) = provider.provide else {
            return INVALID;
        };
        let Some(token) = NEXT_TOKEN.get().checked_add(1) else {
            return INVALID;
        };
        NEXT_TOKEN.set(token);
        builder.token = token;
        let sink = ProviderSinkV1 {
            context: token as *mut c_void,
            root,
            describe: payload,
        };
        ACTIVE.set(&mut builder);
        let scope = Scope;
        let status = unsafe { provide(provider.context, view, &sink) };
        drop(scope);
        if builder.error != OK {
            return builder.error;
        }
        if status != OK {
            return status;
        }
    }
    let missing = allocations
        .iter()
        .filter(|entry| {
            entry.flags & CORE_PAYLOAD == 0
                && entry.requested_size != 0
                && !builder.claimed.contains(&entry.identity)
        })
        .count();
    let gaps = view.coverage_gaps
        | if missing == 0 {
            0
        } else {
            PAYLOAD_DESCRIPTIONS_MISSING
        };
    if gaps & request.reject_gaps != 0 {
        return INCOMPLETE;
    }
    let descriptions: Vec<_> = builder
        .descriptions
        .iter()
        .map(|(entry, offsets)| DescriptionV1 {
            strong_offsets: offsets.as_ptr(),
            ..*entry
        })
        .collect();
    let described = DescribedViewV1 {
        storage: view,
        roots: builder.roots.as_ptr(),
        root_count: builder.roots.len(),
        descriptions: descriptions.as_ptr(),
        description_count: descriptions.len(),
        payload_edges: builder.edges.as_ptr(),
        payload_edge_count: builder.edges.len(),
        missing_description_count: missing,
        coverage_gaps: gaps,
    };
    unsafe { visitor(request.context, &described) }
}

#[cfg(test)]
mod tests;
