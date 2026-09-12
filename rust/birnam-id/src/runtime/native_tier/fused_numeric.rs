//! Bounded numeric execution using unchanged PC entries and V2 transitions.
use super::*;
use crate::execution::{DecodedAction, ExecutionError};
use crate::raw::MethodAddress;
use crate::runtime::current_thread_token;

#[cfg(test)]
thread_local! {
    pub(in crate::runtime) static ENABLED: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
    pub(in crate::runtime) static HITS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

fn controls(runtime: &Runtime, machine: Oop) -> bool {
    if !runtime.guarded_slot_access_eligible(machine) {
        return false;
    }
    let state = unsafe { &*runtime.state.get() };
    let Some(objects) = state.execution.objects() else {
        return false;
    };
    if state.vtables.mutation_epoch() == u64::MAX || !state.pending_stackless_retirements.is_empty()
    {
        return false;
    }
    [
        (
            objects.call_site,
            crate::execution::machine_call_site as Method,
        ),
        (
            objects.tail_call_site,
            crate::execution::machine_tail_call_site as Method,
        ),
        (
            objects.return_at,
            crate::execution::machine_return_at as Method,
        ),
        (
            objects.continue_at,
            crate::execution::machine_continue_at as Method,
        ),
    ]
    .iter()
    .all(|&(selector, method)| {
        let address = MethodAddress::from_method(method);
        state
            .vtables
            .direct_lookup(objects.machine_vtable, selector)
            == Some(address)
            && !state.execution.has_resumable_thunk(address)
    })
}

pub(super) unsafe fn execute(
    runtime: &Runtime,
    machine: Oop,
    key: usize,
    candidate: &Rc<Candidate>,
    entry_args: *const Oop,
) -> Option<Result<Oop, ExecutionError>> {
    #[cfg(not(test))]
    let enabled = false;
    #[cfg(test)]
    let enabled = ENABLED.get() && super::super::numeric::ENABLED.get();
    if !enabled || !controls(runtime, machine) {
        return None;
    }
    let owner = current_thread_token();
    let capability = runtime.stackless_machine_capability(machine).ok()?;
    let frame = {
        let state = unsafe { &*runtime.state.get() };
        let arena = state.execution.machine(owner, capability).ok()?.arena();
        let frame = arena.current_handle()?;
        if arena.program_counter(frame).ok()? != 0
            || arena.descriptor(frame).ok()? as *const FrameDescriptor as usize != key
        {
            return None;
        }
        frame
    };
    let ticket = runtime.prepare_numeric_chain_candidate(capability, owner, key)?;
    if ticket.fusion_template().step_count() != candidate.entries.len()
        || !ticket.fusion_bindings_current(runtime)
    {
        return None;
    }
    let room = {
        let state = unsafe { &*runtime.state.get() };
        let bytes = crate::heap::Heap::allocation_total_size(8)?
            .checked_mul(ticket.fusion_template().body_boxes())?;
        !state
            .metadata
            .automatic_collection_due(state.heap.bytes_since_collection().checked_add(bytes)?)
    };
    if !room {
        return None;
    }
    // From here, the canonical body has no observing method, collection or
    // retirement boundary. Every original allocation is still performed.
    Some((|| {
        let nil = runtime.canonical().nil;
        let mut operation = 0;
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
                DecodedAction::Continue => unsafe {
                    (*runtime.state.get())
                        .execution
                        .commit_current_continue(owner, capability, nil)?;
                },
                DecodedAction::Call { result_slot, .. } => {
                    let raw = ticket
                        .fusion_result(operation, pc)
                        .ok_or(ExecutionError::InvalidAction)?;
                    let value = {
                        let _boundary = runtime.enter_stackless_boundary(owner, capability)?;
                        runtime.make_integer(raw)
                    };
                    unsafe {
                        (*runtime.state.get())
                            .execution
                            .commit_current_call_result(
                                owner,
                                capability,
                                result_slot,
                                nil,
                                value,
                            )?;
                    }
                    operation += 1;
                }
                DecodedAction::TailCall { .. } => {
                    let raw = ticket
                        .fusion_result(operation, pc)
                        .ok_or(ExecutionError::InvalidAction)?;
                    #[cfg(test)]
                    HITS.set(HITS.get() + operation + 1);
                    let _boundary = runtime.enter_stackless_boundary(owner, capability)?;
                    return Ok(runtime.make_integer(raw));
                }
                DecodedAction::Return { value_slot } => {
                    #[cfg(test)]
                    HITS.set(HITS.get() + operation);
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
