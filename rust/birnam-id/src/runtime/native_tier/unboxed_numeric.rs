//! Experimental raw numeric results with original allocation and V2 recovery.
use super::*;
use crate::execution::{DecodedAction, ExecutionError, FrameHandle, MachineCapability};
use crate::heap::DeferredAllocation;
use crate::runtime::current_thread_token;
#[cfg(any(test, feature = "ffi-test-hooks"))]
use core::cell::Cell;

pub(in crate::runtime) enum Outcome {
    Action(Oop),
    Return(Oop),
}

#[cfg(test)]
thread_local! {
    pub(in crate::runtime) static ENABLED: Cell<bool> = const { Cell::new(false) };
    pub(in crate::runtime) static FORCE_AT: Cell<Option<usize>> = const { Cell::new(None) };
}
#[cfg(any(test, feature = "ffi-test-hooks"))]
thread_local! {
    pub(in crate::runtime) static COMPLETED: Cell<usize> = const { Cell::new(0) };
    pub(in crate::runtime) static MATERIALIZED: Cell<usize> = const { Cell::new(0) };
}

pub(super) fn enabled() -> bool {
    #[cfg(test)]
    {
        ENABLED.get()
    }
    #[cfg(not(test))]
    {
        std::env::var_os("BIRNAM_DEV_UNBOXED_NUMERIC").is_some_and(|v| v == "1")
    }
}

struct Values {
    identities: Vec<DeferredAllocation>,
    // Untraced host storage. Materialization cannot collect and publishes all
    // necessary references before handing control back to ordinary execution.
    materialized: Vec<Option<Oop>>,
}
impl Values {
    fn realize(&mut self, r: &Runtime, index: usize) -> Result<Oop, ExecutionError> {
        let id = *self
            .identities
            .get(index)
            .ok_or(ExecutionError::InvalidAction)?;
        if let Some(value) = self.materialized[index] {
            return Ok(value);
        }
        let value = unsafe { (*r.state.get()).heap.materialize_recorded_clone(id) }
            .ok_or(ExecutionError::InvalidAction)?;
        self.materialized[index] = Some(value);
        #[cfg(any(test, feature = "ffi-test-hooks"))]
        MATERIALIZED.set(MATERIALIZED.get() + 1);
        Ok(value)
    }
    fn restore(
        &mut self,
        r: &Runtime,
        owner: u64,
        cap: MachineCapability,
        frame: FrameHandle,
        aliases: &[Option<u32>],
    ) -> Result<(), ExecutionError> {
        for (slot, result) in aliases.iter().enumerate() {
            if let Some(result) = result {
                let value = self.realize(r, *result as usize)?;
                unsafe { (*r.state.get()).execution.machine_mut(owner, cap)? }
                    .arena_mut()
                    .write_oop(frame, slot, value)?;
            }
        }
        Ok(())
    }
}

pub(super) unsafe fn execute(
    r: &Runtime,
    machine: Oop,
    key: usize,
    candidate: &Rc<Candidate>,
    entry_args: *const Oop,
) -> Option<Result<Outcome, ExecutionError>> {
    let maps = candidate.unboxed_numeric.as_ref()?;
    if !enabled() || !fused_numeric::controls(r, machine) {
        return None;
    }
    let owner = current_thread_token();
    let cap = r.stackless_machine_capability(machine).ok()?;
    let frame = {
        let state = unsafe { &*r.state.get() };
        let arena = state.execution.machine(owner, cap).ok()?.arena();
        let frame = arena.current_handle()?;
        if arena.program_counter(frame).ok()? != 0
            || arena.descriptor(frame).ok()? as *const FrameDescriptor as usize != key
        {
            return None;
        }
        let vt = r.canonical().smallint_vt;
        // The ordinary Integer vtable has no declared layout and therefore
        // uses conservative payload tracing. The admitted native constructor
        // supplies exact raw storage authority. A live value is realized
        // before tracing; dead private values retain their exact journal bits.
        if state.vtables.trace_hook(vt).is_some()
            || state.vtables.has_layout(vt)
                && state.vtables.slot_kind(vt, 0) != crate::abi::SLOT_RAW
        {
            return None;
        }
        frame
    };
    let ticket = r.prepare_numeric_chain_candidate(cap, owner, key)?;
    let template = ticket.fusion_template();
    if maps.steps().len() != candidate.entries.len()
        || template.step_count() != maps.steps().len()
        || template.sends().len() != maps.result_count()
        || !ticket.fusion_bindings_current(r)
    {
        return None;
    }
    let snapshots = {
        let state = unsafe { &*r.state.get() };
        let bytes =
            crate::heap::Heap::allocation_total_size(8)?.checked_mul(template.body_boxes())?;
        if !state.heap.admits_virtual_allocations(maps.result_count())
            || state
                .metadata
                .automatic_collection_due(state.heap.bytes_since_collection().checked_add(bytes)?)
        {
            return None;
        }
        template
            .sends()
            .iter()
            .enumerate()
            .map(|(index, send)| {
                let raw = ticket.fusion_result(index, send.pc as usize)?;
                state
                    .heap
                    .prepare_virtual_bytes(r.canonical().smallint_vt, &raw.to_ne_bytes())
            })
            .collect::<Option<Vec<_>>>()?
    };
    #[cfg(test)]
    let force_at = FORCE_AT.get();
    #[cfg(not(test))]
    let force_at = std::env::var("BIRNAM_DEV_UNBOXED_NUMERIC_FORCE_AT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok());
    // From here no observer, collection or retirement can run. Original PC
    // entries only move/stage the supported values; no virtual Oop escapes.
    Some((|| {
        let nil = r.canonical().nil;
        let mut values = Values {
            identities: Vec::with_capacity(snapshots.len()),
            materialized: vec![None; snapshots.len()],
        };
        let mut operation = 0;
        for (pc, method) in candidate.entries.iter().enumerate() {
            let token;
            let args = if pc == 0 {
                entry_args
            } else {
                token = r.make_integer(pc as i64);
                core::ptr::from_ref(&token)
            };
            let regions_enabled = true;
            #[cfg(test)]
            let regions_enabled = regions_enabled && regions::ENABLED.get();
            let action = if regions_enabled && let Some(Some(region)) = candidate.regions.get(pc) {
                unsafe { region.invoke(r, machine, key, *method, args, 1) }
            } else {
                unsafe { method(machine, args, 1) }
            };
            let decoded = unsafe {
                (*r.state.get()).execution.decode_action(
                    &(*r.state.get()).heap,
                    owner,
                    cap,
                    action,
                )?
            };
            let aliases = &maps.steps()[pc].at_action;
            if force_at == Some(pc) {
                values.restore(r, owner, cap, frame, aliases)?;
                return Ok(Outcome::Action(action));
            }
            match decoded {
                DecodedAction::Continue => unsafe {
                    (*r.state.get())
                        .execution
                        .commit_current_continue(owner, cap, nil)?;
                },
                DecodedAction::Call { result_slot, .. } => {
                    let _boundary = r.enter_stackless_boundary(owner, cap)?;
                    ticket
                        .fusion_result(operation, pc)
                        .ok_or(ExecutionError::InvalidAction)?;
                    let id = unsafe {
                        (*r.state.get())
                            .heap
                            .record_virtual_clone(snapshots[operation].clone())
                    }
                    .ok_or(ExecutionError::InvalidAction)?;
                    values.identities.push(id);
                    unsafe {
                        (*r.state.get()).execution.commit_current_call_result(
                            owner,
                            cap,
                            result_slot,
                            nil,
                            nil,
                        )?;
                    }
                    operation += 1;
                    #[cfg(any(test, feature = "ffi-test-hooks"))]
                    COMPLETED.set(COMPLETED.get() + 1);
                }
                DecodedAction::TailCall { .. } => {
                    let raw = ticket
                        .fusion_result(operation, pc)
                        .ok_or(ExecutionError::InvalidAction)?;
                    let _boundary = r.enter_stackless_boundary(owner, cap)?;
                    let result = r.make_integer(raw);
                    #[cfg(any(test, feature = "ffi-test-hooks"))]
                    COMPLETED.set(COMPLETED.get() + 1);
                    // The ordinary driver's optimized-return path pops this
                    // Frame without allocating or calling foreign code. All
                    // its other private identities die before observation.
                    return Ok(Outcome::Return(result));
                }
                DecodedAction::Return { value_slot } => {
                    let result = if let Some(index) = aliases[value_slot] {
                        values.realize(r, index as usize)?
                    } else {
                        unsafe { (*r.state.get()).execution.machine(owner, cap)? }
                            .arena()
                            .read_oop(frame, value_slot)?
                    };
                    return Ok(Outcome::Return(result));
                }
                _ => return Err(ExecutionError::InvalidAction),
            }
        }
        Err(ExecutionError::InvalidAction)
    })())
}
