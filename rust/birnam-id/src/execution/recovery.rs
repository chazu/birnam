//! Checked recovery transfer. No Id allocation or callbacks during admission.
use super::*;
use std::collections::BTreeSet;

pub(crate) struct InlineRecovery<'a> {
    pub(crate) caller: FrameHandle,
    pub(crate) caller_descriptor: &'a Arc<FrameDescriptor>,
    pub(crate) caller_pc: usize,
    pub(crate) callee_descriptor: Arc<FrameDescriptor>,
    pub(crate) callee_pc: usize,
    pub(crate) result_slot: usize,
    pub(crate) slots: &'a [(usize, usize)],
    pub(crate) spills: &'a [Oop],
    pub(crate) entry: Option<ResumableEntryCapability>,
}

impl FrameArena {
    pub(crate) fn recover_inline_child(
        &mut self,
        plan: InlineRecovery<'_>,
        nil: Oop,
    ) -> Result<FrameHandle, FrameError> {
        if self.current_handle() != Some(plan.caller)
            || !core::ptr::eq(
                self.descriptor(plan.caller)?,
                Arc::as_ptr(plan.caller_descriptor),
            )
            || self.program_counter(plan.caller)? != plan.caller_pc
        {
            return Err(FrameError::StaleHandle);
        }
        let caller = self.require_live(plan.caller)?;
        let edge = caller
            .pending_edge
            .and_then(|index| caller.descriptor.as_ref()?.edges.get(index))
            .ok_or(FrameError::InvalidProgramCounter)?;
        if edge.kind != EdgeTransitionKind::Call
            || edge.result_slot != Some(plan.result_slot)
            || edge.source_pc != caller.root_state_program_counter
            || edge.target_pc != caller.program_counter
        {
            return Err(FrameError::InvalidProgramCounter);
        }
        let mut destinations = BTreeSet::new();
        let mut initial = Vec::with_capacity(plan.slots.len());
        for &(slot, spill) in plan.slots {
            if !destinations.insert(slot) {
                return Err(FrameError::InvalidSlot);
            }
            let value = *plan.spills.get(spill).ok_or(FrameError::InvalidSlot)?;
            initial.push((slot, value));
        }
        self.validate_push(&plan.callee_descriptor, plan.callee_pc, &initial)?;
        // Even a nil-valued live location must be explicit in the recipe.
        for slot in 0..plan.callee_descriptor.slot_count() {
            if plan.callee_descriptor.traces_slot(plan.callee_pc, slot)
                && !destinations.contains(&slot)
            {
                return Err(FrameError::InvalidSlot);
            }
        }
        // All fallible admission checks precede mutation. Vec allocation here
        // follows the arena's existing process-fatal OOM policy, never Birnam GC.
        // The caller's handle, pending call/root state and original code remain.
        self.push_validated(
            plan.callee_descriptor,
            plan.callee_pc,
            nil,
            &initial,
            Some(ReturnDestination {
                result_slot: plan.result_slot,
            }),
            plan.entry,
        )
    }
}

/// Process-local authority for exactly one still-pending registered Call.
#[derive(Clone)]
pub(crate) struct RecoveryAuthority {
    pub(crate) machine: MachineCapability,
    pub(crate) caller: FrameHandle,
    pub(crate) caller_descriptor: Arc<FrameDescriptor>,
    pub(crate) caller_pc: usize,
    pub(crate) epoch: u64,
    pub(crate) entry: ResumableEntryCapability,
    pub(crate) descriptor: Arc<FrameDescriptor>,
    pub(crate) result_slot: usize,
}

impl ExecutionState {
    pub(crate) fn recovery_authority(
        &self,
        owner: u64,
        capability: MachineCapability,
        entry: ResumableEntryCapability,
        result_slot: usize,
    ) -> Result<RecoveryAuthority, ExecutionError> {
        self.require_owner(owner)?;
        let machine = self.machine(owner, capability)?;
        machine.require_driving(owner)?;
        let caller = machine
            .arena
            .current_handle()
            .ok_or(FrameError::StaleHandle)?;
        machine.arena.validate_pending_call(caller, result_slot)?;
        let registered = self
            .resumable_entries
            .get(entry.index)
            .filter(|slot| slot.generation == entry.generation)
            .and_then(|slot| slot.entry.as_ref())
            .filter(|entry| entry.acquired_entries != 0)
            .ok_or(RegistrationError::StaleCapability)?;
        Ok(RecoveryAuthority {
            machine: capability,
            caller,
            caller_descriptor: machine.arena.descriptor_arc(caller)?,
            caller_pc: machine.arena.program_counter(caller)?,
            epoch: machine.drive_epoch,
            entry,
            descriptor: registered.descriptor.clone(),
            result_slot,
        })
    }

    pub(crate) fn recover_acquired_call(
        &mut self,
        owner: u64,
        authority: &RecoveryAuthority,
        pc: usize,
        slots: &[(usize, usize)],
        spills: &[Oop],
        nil: Oop,
    ) -> Result<FrameHandle, ExecutionError> {
        self.require_owner(owner)?;
        let current = self.recovery_authority(
            owner,
            authority.machine,
            authority.entry,
            authority.result_slot,
        )?;
        if current.caller != authority.caller
            || current.epoch != authority.epoch
            || current.caller_pc != authority.caller_pc
            || !Arc::ptr_eq(&current.caller_descriptor, &authority.caller_descriptor)
            || !Arc::ptr_eq(&current.descriptor, &authority.descriptor)
        {
            return Err(FrameError::StaleHandle.into());
        }
        // Preflight the pin increment before touching either the arena or counts.
        let entry = self.resumable_entries[authority.entry.index]
            .entry
            .as_ref()
            .unwrap();
        entry
            .active_frame_pins
            .checked_add(1)
            .ok_or(RegistrationError::BindingReferenceOverflow)?;
        let frame = self
            .machine_mut(owner, authority.machine)?
            .arena
            .recover_inline_child(
                InlineRecovery {
                    caller: authority.caller,
                    caller_descriptor: &authority.caller_descriptor,
                    caller_pc: authority.caller_pc,
                    callee_descriptor: authority.descriptor.clone(),
                    callee_pc: pc,
                    result_slot: authority.result_slot,
                    slots,
                    spills,
                    entry: Some(authority.entry),
                },
                nil,
            )?;
        // The immutable preflight above makes this transfer infallible here.
        self.transfer_acquired_entry_to_frame(authority.entry)
            .expect("preflighted recovery pin transfer");
        Ok(frame)
    }
}

impl ExecutionState {
    pub(crate) fn inline_return_storage_valid(
        &self,
        heap: &Heap,
        owner: u64,
        capability: MachineCapability,
    ) -> bool {
        let Some(objects) = self.objects else {
            return false;
        };
        let Ok(machine) = self.machine(owner, capability) else {
            return false;
        };
        validate_action_storage(heap, machine.return_action, objects.return_action_vtable, 5)
            .is_ok()
    }

    /// Preserve the reflective scratch ReturnAction as well as the Call result.
    pub(crate) fn commit_inlined_return(
        &mut self,
        heap: &mut Heap,
        owner: u64,
        capability: MachineCapability,
        result_slot: usize,
        callee_slot: usize,
        nil: Oop,
        value: Oop,
    ) -> Result<(), ExecutionError> {
        let objects = self.objects.ok_or(ExecutionError::InvalidAction)?;
        let machine = self.machine_mut(owner, capability)?;
        machine.require_driving(owner)?;
        let frame = machine
            .arena
            .current_handle()
            .ok_or(FrameError::StaleHandle)?;
        machine.arena.validate_pending_call(frame, result_slot)?;
        initialize_action(
            heap,
            machine.return_action,
            objects.return_action_vtable,
            &[
                1,
                capability.index,
                capability.generation as usize,
                machine.drive_epoch as usize,
                callee_slot,
            ],
        )?;
        machine
            .arena
            .commit_call_transition(frame, result_slot, nil, value)?;
        Ok(())
    }
}
