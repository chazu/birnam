//! Owner-side budget stops. A runnable stop has no pending result slot and
//! retains its exact committed frame state, roots, delimiters, and entry pins.
use super::*;

impl MachineState {
    pub(crate) fn stop_runnable(&mut self, owner: u64) -> Result<(), ExecutionError> {
        self.require_driving(owner)?;
        if !self.control_eligible() {
            return Err(ExecutionError::ContinuationBarrier);
        }
        let frame = self.arena.current_handle().ok_or(FrameError::StaleHandle)?;
        if self.arena.require_live(frame)?.pending_edge.is_some() {
            return Err(ExecutionError::InvalidAction);
        }
        // Preflight the only fallible epoch change before mutating diagnostics.
        self.drive_epoch
            .checked_add(1)
            .ok_or(MachineError::DriveEpochExhausted)?;
        self.finish_drive_epoch()?;
        self.drive_depth = 0;
        self.lifecycle = MachineLifecycle::Runnable;
        Ok(())
    }

    pub(crate) fn require_runnable(&self, owner: u64) -> Result<(), ExecutionError> {
        self.require_owner(owner)?;
        if self.lifecycle != MachineLifecycle::Runnable || self.drive_depth != 0 {
            return Err(MachineError::InvalidLifecycle.into());
        }
        let frame = self.arena.current_handle().ok_or(FrameError::StaleHandle)?;
        if self.arena.require_live(frame)?.pending_edge.is_some() {
            return Err(ExecutionError::InvalidAction);
        }
        Ok(())
    }

    pub(crate) fn resume_runnable(&mut self, owner: u64) -> Result<(), ExecutionError> {
        self.require_runnable(owner)?;
        self.advance_drive_epoch()?;
        self.drive_boundaries.push(BoundaryDiagnostics::default());
        self.drive_depth = 1;
        self.lifecycle = MachineLifecycle::Running;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduling_stop_rejects_barriers_pending_edges_and_epoch_overflow() {
        for fault in 0..5 {
            let mut state = ExecutionState::new(1);
            let cap = state.create_machine(1).unwrap();
            let machine = state.machine_mut(1, cap).unwrap();
            machine.begin_initial_drive(1).unwrap();
            let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
                edges: vec![probe_continue(0, 0, 1)],
                ..probe_spec(1, 1)
            })
            .unwrap();
            let frame = machine
                .arena
                .push(descriptor, 0, core::ptr::null_mut(), &[])
                .unwrap();
            match fault {
                0 => machine.enter_synchronous_boundary(1).unwrap(),
                1 => {
                    machine.begin_nested_drive(1).unwrap();
                }
                2 => machine.arena.begin_continue_transition(frame, 0).unwrap(),
                3 => machine.drive_epoch = u64::MAX,
                _ => (),
            }
            let before = machine.arena.recovery_experiment_snapshot();
            let epoch = machine.drive_epoch;
            let depth = machine.drive_depth;
            let boundaries = machine.drive_boundaries.clone();
            assert!(
                machine
                    .stop_runnable(if fault == 4 { 2 } else { 1 })
                    .is_err()
            );
            assert_eq!(machine.arena.recovery_experiment_snapshot(), before);
            assert_eq!(machine.drive_epoch, epoch);
            assert_eq!(machine.drive_depth, depth);
            assert_eq!(machine.drive_boundaries, boundaries);
            assert_eq!(machine.lifecycle, MachineLifecycle::Running);
        }
    }
}
