//! Graph traversal and live representation access above Id.
//! Diagnostic tracing is stateless; the native live graph owns completed lower
//! inputs and payload reads. Neither owns heap marks, GC policy or reclamation.
use birnam_abi::storage::{self, DescribedViewV1, INCOMPLETE, INVALID, Status};
use birnam_abi::trace::{AllocationRefV1, ReportV1, RequestV1, VERSION};
use birnam_abi::borrowed_slice;
use std::collections::BTreeMap;

pub mod live;
mod graph;

fn aligned<T>(pointer: *const T) -> bool {
    !pointer.is_null() && pointer.addr().is_multiple_of(align_of::<T>())
}

/// Trace the union of core and described payload edges from declared roots.
///
/// # Safety
/// `described` is the exact result of the selected storage validator, called
/// during its visitor. Input graph arrays stay readable and unchanged;
/// callbacks and their data remain live throughout this synchronous call.
/// The enclosing Id observation remains active, and the visitor obeys its
/// read-only native contract. No
/// unwinding is permitted across this ABI. Shape checks do not validate expired
/// pointers, arbitrary native writes or a provider's representation authority.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_trace_described_v1(
    described: *const DescribedViewV1,
    request: *const RequestV1,
) -> Status {
    if !aligned(described) || !aligned(request) {
        return INVALID;
    }
    let (described, request) = unsafe { (&*described, &*request) };
    if !aligned(described.storage) {
        return INVALID;
    }
    let storage = unsafe { &*described.storage };
    if request.version != VERSION
        || request.byte_size != size_of::<RequestV1>()
        || storage.version != storage::VERSION
        || storage.byte_size != size_of::<storage::ViewV1>()
        || storage.runtime_identity == 0
        || storage.observation_identity == 0
    {
        return INVALID;
    }
    let Some(visitor) = request.visitor else {
        return INVALID;
    };
    let Some(allocations) =
        (unsafe { borrowed_slice(storage.allocations, storage.allocation_count) })
    else {
        return INVALID;
    };
    let Some(roots) = (unsafe { borrowed_slice(described.roots, described.root_count) }) else {
        return INVALID;
    };
    let Some(core_edges) = (unsafe { borrowed_slice(storage.edges, storage.edge_count) }) else {
        return INVALID;
    };
    let Some(payload_edges) =
        (unsafe { borrowed_slice(described.payload_edges, described.payload_edge_count) })
    else {
        return INVALID;
    };
    let mut objects: Vec<_> = allocations
        .iter()
        .map(|allocation| AllocationRefV1 {
            object: allocation.object,
            allocation_identity: allocation.identity,
        })
        .collect();
    objects.sort_unstable_by_key(|object| object.allocation_identity);
    let mut by_address = BTreeMap::new();
    let mut previous_identity = 0;
    for (index, object) in objects.iter().enumerate() {
        if object.object.is_null()
            || !object
                .object
                .addr()
                .is_multiple_of(align_of::<birnam_abi::Oop>())
            || object.allocation_identity == previous_identity
            || by_address.insert(object.object.addr(), index).is_some()
        {
            return INVALID;
        }
        previous_identity = object.allocation_identity;
    }
    let mut root_indices = Vec::new();
    for root in roots {
        if root.is_null() {
            continue;
        }
        let Some(&index) = by_address.get(&root.addr()) else {
            return INVALID;
        };
        root_indices.push(index);
    }
    let mut edges = vec![Vec::new(); objects.len()];
    for edge in core_edges.iter().chain(payload_edges) {
        let (Some(&from), Some(&to)) = (
            by_address.get(&edge.from.addr()),
            by_address.get(&edge.to.addr()),
        ) else {
            // Validate even edges from objects that will not be reached.
            return INVALID;
        };
        edges[from].push(to);
    }
    // Preserve all upstream gaps, including future bits. Descriptions cannot
    // erase a floor omission. Deferred records never enter the physical graph.
    let gaps = described.coverage_gaps
        | storage.coverage_gaps
        | if described.missing_description_count == 0 {
            0
        } else {
            storage::PAYLOAD_DESCRIPTIONS_MISSING
        };
    if gaps & !request.accept_gaps != 0 {
        return INCOMPLETE;
    }
    let mut reached = vec![false; objects.len()];
    let mut pending = Vec::new();
    for index in root_indices {
        if !core::mem::replace(&mut reached[index], true) {
            pending.push(index);
        }
    }
    while let Some(index) = pending.pop() {
        for &target in &edges[index] {
            if !core::mem::replace(&mut reached[target], true) {
                pending.push(target);
            }
        }
    }
    let mut reached_objects = Vec::new();
    let mut not_reached = Vec::new();
    for (object, reached) in objects.into_iter().zip(reached) {
        if reached {
            reached_objects.push(object);
        } else {
            not_reached.push(object);
        }
    }
    let report = ReportV1 {
        version: VERSION,
        byte_size: size_of::<ReportV1>(),
        runtime_identity: storage.runtime_identity,
        observation_identity: storage.observation_identity,
        described,
        reached: reached_objects.as_ptr(),
        reached_count: reached_objects.len(),
        not_reached: not_reached.as_ptr(),
        not_reached_count: not_reached.len(),
        coverage_gaps: gaps,
    };
    unsafe { visitor(request.context, &report) }
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}

#[cfg(test)]
mod tests;
