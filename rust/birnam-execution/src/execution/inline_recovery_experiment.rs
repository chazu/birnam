//! Test-only full-state observations for recovery admission.
use super::*;

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
}
