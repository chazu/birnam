//! Additive delimited invocation. Frames own the metadata, so capture moves it
//! with execution rather than preserving a retired FrameHandle. No language
//! outcome, cleanup, cancellation, or scheduler policy lives here.
use super::*;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct FrameControl {
    pub(super) boundary_result: Option<usize>,
    pub(super) deliveries: usize,
}

pub(crate) struct BoundaryExit {
    target: FrameHandle,
    depth: usize,
    result_slot: usize,
    inherited_deliveries: usize,
    entries: Vec<ResumableEntryCapability>,
    // Keep code ownership alive until the entire transfer and pin release finish.
    _leases: Vec<RegistrationGenerationLease>,
}

impl FrameArena {
    pub(super) fn completed_boundary_deliveries(
        &self,
        frame: FrameHandle,
        result_slot: usize,
    ) -> Result<usize, FrameError> {
        let control = self.require_live(frame)?.control;
        if control
            .boundary_result
            .is_some_and(|slot| slot != result_slot)
        {
            return Err(FrameError::InvalidSlot);
        }
        control
            .deliveries
            .checked_add(usize::from(control.boundary_result.is_some()))
            .ok_or(FrameError::GenerationExhausted)
    }

    pub(super) fn validate_boundary_call(
        &self,
        frame: FrameHandle,
        receiver: usize,
        selector: usize,
        arguments: usize,
        count: usize,
        result: usize,
    ) -> Result<(), FrameError> {
        self.validate_pending_call(frame, result)?;
        let slot = self.require_live(frame)?;
        let descriptor = self.descriptor(frame)?;
        let edge = &descriptor.edges[slot.pending_edge.unwrap()];
        let site = edge
            .call_site_index
            .and_then(|index| descriptor.call_site(index))
            .ok_or(FrameError::InvalidSlot)?;
        if slot.control.boundary_result.is_some()
            || site.kind != CallSiteKind::Call
            || (
                site.receiver_slot,
                site.selector_slot,
                site.argument_start,
                site.argument_count,
            ) != (receiver, selector, arguments, count)
        {
            return Err(FrameError::InvalidSlot);
        }
        slot.control
            .deliveries
            .checked_add(1)
            .ok_or(FrameError::GenerationExhausted)?;
        Ok(())
    }

    /// Preflight and inherit the returning frame's delivery guard before pop.
    /// The caller subsequently commits its own Call, possibly adding a guard.
    pub(crate) fn inherit_return_delivery(&mut self, frame: FrameHandle) -> Result<(), FrameError> {
        let slot = self.require_live(frame)?;
        let deliveries = slot.control.deliveries;
        if self.current_handle() != Some(frame) || slot.control.boundary_result.is_some() {
            return Err(FrameError::InvalidProgramCounter);
        }
        slot.generation
            .checked_add(1)
            .ok_or(FrameError::GenerationExhausted)?;
        // A host return has no Birnam handler left to acknowledge the handoff.
        if slot.return_destination.is_none() {
            return Ok(());
        }
        let caller_index = *self
            .stack
            .get(
                self.stack
                    .len()
                    .checked_sub(2)
                    .ok_or(FrameError::StaleHandle)?,
            )
            .ok_or(FrameError::StaleHandle)?;
        let caller = FrameHandle {
            arena: self.identity,
            index: caller_index,
            generation: self.slots[caller_index].generation,
        };
        self.validate_pending_call(caller, slot.return_destination.unwrap().result_slot)?;
        let control = &mut self.slots[caller_index].control;
        let total = control
            .deliveries
            .checked_add(deliveries)
            .ok_or(FrameError::GenerationExhausted)?;
        total
            .checked_add(usize::from(control.boundary_result.is_some()))
            .ok_or(FrameError::GenerationExhausted)?;
        control.deliveries = total;
        Ok(())
    }
}

impl ExecutionState {
    pub(crate) fn publish_boundary_call(
        &mut self,
        heap: &mut Heap,
        owner: u64,
        capability: MachineCapability,
        index: usize,
    ) -> Result<Oop, ExecutionError> {
        let objects = self.objects.ok_or(ExecutionError::InvalidAction)?;
        let machine = self.machine_mut(owner, capability)?;
        machine.require_driving(owner)?;
        let frame = machine
            .arena
            .current_handle()
            .ok_or(FrameError::StaleHandle)?;
        let site = *machine
            .arena
            .descriptor(frame)?
            .call_site(index)
            .ok_or(FrameError::InvalidSlot)?;
        if site.kind != CallSiteKind::Call
            || machine
                .arena
                .require_live(frame)?
                .control
                .boundary_result
                .is_some()
        {
            return Err(FrameError::InvalidSlot.into());
        }
        machine
            .arena
            .require_live(frame)?
            .control
            .deliveries
            .checked_add(1)
            .ok_or(FrameError::GenerationExhausted)?;
        let generation = usize::try_from(capability.generation)
            .map_err(|_| MachineError::RegistryGenerationExhausted)?;
        let epoch =
            usize::try_from(machine.drive_epoch).map_err(|_| MachineError::DriveEpochExhausted)?;
        let result = site.result_slot.ok_or(FrameError::InvalidSlot)?;
        let next = site
            .continuation_pc
            .ok_or(FrameError::InvalidProgramCounter)?;
        validate_action_storage(
            heap,
            machine.boundary_call_action,
            objects.boundary_call_action_vtable,
            9,
        )?;
        machine.arena.begin_call_transition(frame, next, index)?;
        initialize_action(
            heap,
            machine.boundary_call_action,
            objects.boundary_call_action_vtable,
            &[
                1,
                capability.index,
                generation,
                epoch,
                site.receiver_slot,
                site.selector_slot,
                site.argument_start,
                site.argument_count,
                result,
            ],
        )?;
        Ok(machine.boundary_call_action)
    }

    pub(crate) fn establish_boundary(
        &mut self,
        owner: u64,
        capability: MachineCapability,
        result: usize,
    ) -> Result<(), ExecutionError> {
        let machine = self.machine_mut(owner, capability)?;
        machine.require_driving(owner)?;
        let frame = machine
            .arena
            .current_handle()
            .ok_or(FrameError::StaleHandle)?;
        machine.arena.validate_pending_call(frame, result)?;
        let slot = machine.arena.require_live_mut(frame)?;
        if slot.control.boundary_result.is_some() {
            return Err(ExecutionError::InvalidAction);
        }
        slot.control
            .deliveries
            .checked_add(1)
            .ok_or(FrameError::GenerationExhausted)?;
        slot.control.boundary_result = Some(result);
        Ok(())
    }

    pub(crate) fn boundary_delivery_pending(
        &self,
        owner: u64,
        capability: MachineCapability,
    ) -> Result<bool, ExecutionError> {
        let machine = self.machine(owner, capability)?;
        Ok(machine
            .arena
            .stack
            .iter()
            .any(|index| machine.arena.slots[*index].control.deliveries != 0))
    }

    pub(crate) fn acknowledge_boundary_delivery(
        &mut self,
        owner: u64,
        capability: MachineCapability,
    ) -> Result<(), ExecutionError> {
        let machine = self.machine_mut(owner, capability)?;
        machine.require_driving(owner)?;
        let index = machine
            .arena
            .stack
            .iter()
            .rev()
            .copied()
            .find(|index| machine.arena.slots[*index].control.deliveries != 0)
            .ok_or(ExecutionError::InvalidAction)?;
        machine.arena.slots[index].control.deliveries -= 1;
        Ok(())
    }

    pub(crate) fn preflight_boundary_exit(
        &self,
        owner: u64,
        capability: MachineCapability,
    ) -> Result<BoundaryExit, ExecutionError> {
        let machine = self.machine(owner, capability)?;
        machine.require_driving(owner)?;
        self.preflight_boundary_frames(owner, capability)
    }

    fn preflight_boundary_frames(
        &self,
        owner: u64,
        capability: MachineCapability,
    ) -> Result<BoundaryExit, ExecutionError> {
        let machine = self.machine(owner, capability)?;
        let arena = &machine.arena;
        let depth = arena
            .stack
            .iter()
            .rposition(|index| arena.slots[*index].control.boundary_result.is_some())
            .ok_or(ExecutionError::InvalidAction)?;
        // The retained handler is below the body. Never exit its own pending
        // invocation before a body frame exists, or cross a nested native drive.
        if depth + 1 >= arena.stack.len()
            || machine
                .delimiter_stack
                .iter()
                .any(|index| machine.delimiters[*index].frame_boundary > depth)
            || machine
                .drive_boundaries
                .last()
                .is_some_and(|record| record.current != 0)
        {
            return Err(ExecutionError::ContinuationBarrier);
        }
        let index = arena.stack[depth];
        let slot = &arena.slots[index];
        let target = FrameHandle {
            arena: arena.identity,
            index,
            generation: slot.generation,
        };
        let result_slot = slot.control.boundary_result.unwrap();
        arena.validate_pending_call(target, result_slot)?;
        let mut deliveries = arena.completed_boundary_deliveries(target, result_slot)?;
        let mut entries = Vec::new();
        let mut leases = Vec::new();
        for index in &arena.stack[depth + 1..] {
            let slot = &arena.slots[*index];
            slot.generation
                .checked_add(1)
                .ok_or(FrameError::GenerationExhausted)?;
            deliveries = deliveries
                .checked_add(slot.control.deliveries)
                .ok_or(FrameError::GenerationExhausted)?;
            if let Some(capability) = slot.resumable_entry {
                let entry = self
                    .resumable_entries
                    .get(capability.index)
                    .filter(|slot| slot.generation == capability.generation)
                    .and_then(|slot| slot.entry.as_ref())
                    .ok_or(RegistrationError::StaleCapability)?;
                let count = entries
                    .iter()
                    .filter(|previous| **previous == capability)
                    .count()
                    + 1;
                if entry.active_frame_pins < count {
                    return Err(RegistrationError::StaleCapability.into());
                }
                entries.push(capability);
                if let Some(lease) = &entry.registration_generation_lease {
                    leases.push(lease.clone());
                }
            }
        }
        Ok(BoundaryExit {
            target,
            depth,
            result_slot,
            inherited_deliveries: deliveries - 1,
            entries,
            _leases: leases,
        })
    }

    pub(crate) fn exit_boundary(
        &mut self,
        owner: u64,
        capability: MachineCapability,
        value: Oop,
        nil: Oop,
    ) -> Result<(), ExecutionError> {
        let plan = self.preflight_boundary_exit(owner, capability)?;
        self.commit_boundary_exit(owner, capability, value, nil, plan);
        Ok(())
    }

    /// Owner control ingress for a budget-stopped Machine. No body step or
    /// synthetic send runs before the transfer. Delivery already in progress
    /// must reach its handler before another external exit can be selected.
    pub(crate) fn exit_runnable_boundary(
        &mut self,
        owner: u64,
        capability: MachineCapability,
        value: Oop,
        nil: Oop,
    ) -> Result<(), ExecutionError> {
        self.machine(owner, capability)?.require_runnable(owner)?;
        if self.boundary_delivery_pending(owner, capability)? {
            return Err(ExecutionError::ContinuationBarrier);
        }
        let plan = self.preflight_boundary_frames(owner, capability)?;
        self.commit_boundary_exit(owner, capability, value, nil, plan);
        Ok(())
    }

    fn commit_boundary_exit(
        &mut self,
        owner: u64,
        capability: MachineCapability,
        value: Oop,
        nil: Oop,
        plan: BoundaryExit,
    ) {
        // From here to publication: no allocation, user call, GC, or fallible
        // discovery. Value remains in the source frame until the commit starts.
        let machine = self
            .machine_mut(owner, capability)
            .expect("preflight owner");
        while machine.arena.live_depth() > plan.depth + 1 {
            machine.arena.pop().expect("preflight frame generations");
        }
        machine.arena.slots[plan.target.index].control.deliveries = plan.inherited_deliveries;
        machine
            .arena
            .commit_call_transition(plan.target, plan.result_slot, nil, value)
            .expect("preflight pending edge and delivery count");
        for entry in &plan.entries {
            self.release_entry_frame_pin(*entry)
                .expect("preflight pin counts");
        }
    }

    pub(crate) fn publish_boundary_exit(
        &mut self,
        heap: &mut Heap,
        owner: u64,
        capability: MachineCapability,
        value_slot: usize,
    ) -> Result<Oop, ExecutionError> {
        self.preflight_boundary_exit(owner, capability)?;
        let objects = self.objects.ok_or(ExecutionError::InvalidAction)?;
        let machine = self.machine(owner, capability)?;
        let frame = machine
            .arena
            .current_handle()
            .ok_or(FrameError::StaleHandle)?;
        machine.arena.read_oop(frame, value_slot)?;
        let generation = usize::try_from(capability.generation)
            .map_err(|_| MachineError::RegistryGenerationExhausted)?;
        let epoch =
            usize::try_from(machine.drive_epoch).map_err(|_| MachineError::DriveEpochExhausted)?;
        initialize_action(
            heap,
            machine.exit_boundary_action,
            objects.exit_boundary_action_vtable,
            &[1, capability.index, generation, epoch, value_slot],
        )?;
        Ok(machine.exit_boundary_action)
    }
}
