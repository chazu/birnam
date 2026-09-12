//! 74O: checked, test-only expansion of one pending call into a recovered Frame.
//! Policy and spill selection belong to the compiler-side experiment. No Id
//! sends, Birnam allocations or callbacks occur during this state transfer.

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
    pub(crate) fn recovery_experiment_logical_frames(&self) -> Vec<String> {
        self.stack
            .iter()
            .map(|index| {
                let slot = &self.slots[*index];
                format!(
                    "{:?}",
                    (
                        slot.descriptor.as_ref().map(Arc::as_ptr),
                        slot.program_counter,
                        slot.root_state_program_counter,
                        slot.pending_edge,
                        slot.return_destination,
                        slot.resumable_entry,
                        &slot.words,
                        &slot.transfer_scratch,
                    )
                )
            })
            .collect()
    }

    /// Complete logical arena state for atomic-rejection assertions, including
    /// pending edges, roots, ownership and scratch contents; excludes capacity.
    pub(crate) fn recovery_experiment_snapshot(&self) -> String {
        let mut state = format!(
            "{:?}",
            (
                self.identity,
                &self.stack,
                self.high_water_live_frames,
                self.destroyed
            )
        );
        for slot in &self.slots {
            state.push_str(&format!(
                "{:?}",
                (
                    slot.generation,
                    slot.live,
                    slot.descriptor.as_ref().map(Arc::as_ptr),
                    slot.program_counter,
                    slot.root_state_program_counter,
                    slot.pending_edge,
                    slot.return_destination,
                    slot.resumable_entry,
                    &slot.words,
                    &slot.transfer_scratch,
                )
            ));
        }
        state
    }

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
