//! Preflight arithmetic for a callee whose physical Frame is not installed yet.
use super::*;

impl Runtime {
    /// Prediction only. A consumer must separately acquire/pin the callee entry,
    /// validate its exact body, constructor, capture layout and observation budget.
    pub(in crate::runtime) fn prepare_numeric_entry_prediction(
        &self,
        machine: Oop,
        descriptor: &Arc<FrameDescriptor>,
        initial: &[(usize, Oop)],
    ) -> Option<super::super::numeric::Ticket> {
        #[cfg(feature = "ffi-test-hooks")]
        if crate::runtime::unboxed_numeric_probe::control_active(machine) {
            return None;
        }
        if !fused_numeric::controls(self, machine)
            || initial.len() > descriptor.slot_count()
            || initial.iter().enumerate().any(|(index, &(slot, _))| {
                slot >= descriptor.slot_count()
                    || initial[..index].iter().any(|&(earlier, _)| earlier == slot)
            })
        {
            return None;
        }
        let key = Arc::as_ptr(descriptor) as usize;
        let compiled = {
            let registry = self.native_tier.borrow();
            let record = registry.records.get(&key)?;
            if record.disabled
                || !record
                    .descriptor
                    .upgrade()
                    .is_some_and(|d| Arc::ptr_eq(&d, descriptor))
                || !record
                    .candidate
                    .as_ref()
                    .is_some_and(|candidate| candidate.numeric)
            {
                return None;
            }
            // Only actual canonical execution warms/prepares a numeric callee.
            // Constructor preflight never increments its visit/start counters.
            record.numeric_candidate.clone()?
        };
        compiled.predict_entry(self, key, initial)
    }
}
