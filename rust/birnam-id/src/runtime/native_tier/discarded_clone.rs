//! Eliminate physical clones whose identities die before the next observation.
use super::*;
use crate::execution::{DecodedAction, ExecutionError};
use crate::raw::MethodAddress;
use crate::runtime::current_thread_token;

#[cfg(test)]
thread_local! {
    pub(super) static ENABLED: core::cell::Cell<bool> = const { core::cell::Cell::new(true) };
    pub(super) static HITS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

pub(super) unsafe fn execute(
    runtime: &Runtime,
    machine: Oop,
    key: usize,
    candidate: &Rc<Candidate>,
    plan: &birnam_codegen::discarded_clone::Plan,
    entry_args: *const Oop,
) -> Option<Result<Oop, ExecutionError>> {
    #[cfg(not(test))]
    let enabled = true;
    #[cfg(test)]
    let enabled = ENABLED.get();
    if !enabled
        // Qualified measurements show that a single clone does not amortize
        // admission and journal preparation. Require repeated reuse instead.
        || plan.sites().len() < 3
        || !fused_numeric::controls(runtime, machine)
        || candidate.entries.len() != plan.sites().len() + 1
    {
        return None;
    }
    let owner = current_thread_token();
    let capability = runtime.stackless_machine_capability(machine).ok()?;
    let (frame, snapshots) = {
        let state = unsafe { &*runtime.state.get() };
        if !state.heap.admits_dead_allocations(plan.sites().len()) {
            return None;
        }
        let arena = state.execution.machine(owner, capability).ok()?.arena();
        let frame = arena.current_handle()?;
        if arena.program_counter(frame).ok()? != 0
            || arena.descriptor(frame).ok()? as *const FrameDescriptor as usize != key
            || frame.words().ok()?[2] == usize::MAX
        {
            return None;
        }
        let mut snapshots = Vec::with_capacity(plan.sites().len());
        let mut cache: Vec<(usize, Rc<crate::heap::DeadAllocation>)> = Vec::new();
        let mut bytes =
            crate::heap::Heap::allocation_total_size(8)?.checked_mul(plan.integer_boxes())?;
        for site in plan.sites() {
            let prototype = arena.read_oop(frame, site.source_slot).ok()?;
            if cache
                .first()
                .is_some_and(|(address, _)| *address != prototype.addr())
            {
                return None;
            }
            let vt = state.heap.vtable_of(prototype)?;
            let selector = state.symbols.existing(&site.selector)?;
            let method = MethodAddress::from_method(crate::dispatch::object_clone);
            if state.heap.vtable_of(vt) != Some(state.globals.vtable_vt)
                || state.vtables.lookup(vt, selector) != Some(method)
                || state.execution.has_resumable_thunk(method)
            {
                return None;
            }
            let snapshot = if let Some((_, snapshot)) = cache
                .iter()
                .find(|(address, _)| *address == prototype.addr())
            {
                // No Id callback or collection runs during admission. The
                // snapshot's storage proof remains valid here, while each
                // site's possibly different selector still needs its guard.
                snapshot.clone()
            } else {
                if state.heap.has_finalizer(prototype) != Some(false)
                    || !state.vtables.has_layout(vt)
                    || state.vtables.trace_hook(vt).is_some()
                    || state.vtables.slot_kind(vt, 0) != crate::abi::SLOT_OOP
                {
                    return None;
                }
                // Preparation checks the payload bound before copying or
                // walking its slots, keeping admission work bounded too.
                let snapshot = state.heap.prepare_dead_clone(prototype)?;
                let payload = state.heap.payload_size(prototype)?;
                if !payload.is_multiple_of(core::mem::size_of::<Oop>())
                    || (0..payload / core::mem::size_of::<Oop>())
                        .any(|slot| state.vtables.slot_kind(vt, slot) != crate::abi::SLOT_OOP)
                {
                    return None;
                }
                cache.push((prototype.addr(), snapshot.clone()));
                snapshot
            };
            bytes = bytes.checked_add(snapshot.total_size())?;
            snapshots.push(snapshot);
        }
        if state
            .metadata
            .automatic_collection_due(state.heap.bytes_since_collection().checked_add(bytes)?)
        {
            return None;
        }
        (frame, snapshots)
    };
    // Current bindings and the whole allocation budget exclude observers until
    // this Frame returns. Nil stands in only for values proven dead at Return.
    Some((|| {
        let nil = runtime.canonical().nil;
        for (pc, method) in candidate.entries.iter().enumerate() {
            let token;
            let args = if pc == 0 {
                entry_args
            } else {
                token = runtime.make_integer(pc as i64);
                core::ptr::from_ref(&token)
            };
            let regions_enabled = true;
            #[cfg(test)]
            let regions_enabled = regions_enabled && super::regions::ENABLED.get();
            let action = if regions_enabled && let Some(Some(region)) = candidate.regions.get(pc) {
                unsafe { region.invoke(runtime, machine, key, *method, args, 1) }
            } else {
                unsafe { method(machine, args, 1) }
            };
            let decoded = unsafe {
                let state = &*runtime.state.get();
                state
                    .execution
                    .decode_action(&state.heap, owner, capability, action)?
            };
            match decoded {
                DecodedAction::Call { result_slot, .. } if pc < snapshots.len() => {
                    {
                        let _boundary = runtime.enter_stackless_boundary(owner, capability)?;
                        if !unsafe {
                            (*runtime.state.get())
                                .heap
                                .record_dead_allocation(snapshots[pc].clone())
                        } {
                            return Err(ExecutionError::InvalidAction);
                        }
                    }
                    unsafe {
                        (*runtime.state.get())
                            .execution
                            .commit_current_call_result(owner, capability, result_slot, nil, nil)?;
                    }
                }
                DecodedAction::Return { value_slot } if pc == snapshots.len() => {
                    #[cfg(test)]
                    HITS.set(HITS.get() + snapshots.len());
                    return unsafe {
                        (*runtime.state.get())
                            .execution
                            .machine(owner, capability)?
                    }
                    .arena()
                    .read_oop(frame, value_slot)
                    .map_err(Into::into);
                }
                _ => return Err(ExecutionError::InvalidAction),
            }
        }
        Err(ExecutionError::InvalidAction)
    })())
}
