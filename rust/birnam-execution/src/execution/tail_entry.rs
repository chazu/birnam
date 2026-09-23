//! One bounded registered tail transition using immutable entry-layout work.
//! Dispatch, acquisition and action publication have already happened normally.
use super::*;

#[cfg(test)]
thread_local! {
    pub(crate) static ENABLED: core::cell::Cell<bool> = const { core::cell::Cell::new(true) };
    pub(crate) static HITS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

struct Admission {
    descriptor: Arc<FrameDescriptor>,
    frame: FrameHandle,
    next_generation: u64,
    old_entry: Option<ResumableEntryCapability>,
    capture_offset: usize,
    implementation: usize,
}

impl ExecutionState {
    fn admit_tail_entry(
        &self,
        heap: &Heap,
        owner: u64,
        machine: MachineCapability,
        capability: ResumableEntryCapability,
        receiver: Oop,
    ) -> Result<Option<Admission>, ExecutionError> {
        let slot = self
            .resumable_entries
            .get(capability.index)
            .filter(|slot| slot.generation == capability.generation)
            .ok_or(RegistrationError::StaleCapability)?;
        let entry = slot
            .entry
            .as_ref()
            .filter(|entry| entry.acquired_entries != 0)
            .ok_or(RegistrationError::StaleCapability)?;
        let descriptor = &entry.descriptor;
        let plan = descriptor.tail_entry_plan.get_or_init(|| {
            let binding = descriptor.v2_entry_binding?;
            crate::entry_plan::Plan::derive(
                descriptor.slot_count(),
                binding.receiver_slot,
                binding.argument_start,
                binding.argument_count,
                binding.implementation_slot,
                &descriptor.entry_capture_slots,
            )
            .map(Box::new)
        });
        if plan.is_none() {
            return Ok(None);
        }
        // Match the ordinary materializer's exact closure payload check, even
        // when an implementation header exists but there are no captures.
        let words = entry
            .receiver_capture_offset_words
            .checked_add(descriptor.entry_capture_slots.len())
            .ok_or(ExecutionError::InvalidAction)?;
        if words != 0 {
            let bytes = words
                .checked_mul(core::mem::size_of::<usize>())
                .ok_or(ExecutionError::InvalidAction)?;
            if heap.payload_size(receiver) != Some(bytes) {
                return Err(ExecutionError::InvalidAction);
            }
        }
        let arena = self.machine(owner, machine)?.arena();
        let frame = arena.current_handle().ok_or(FrameError::StaleHandle)?;
        let current = arena.require_live(frame)?;
        if current.control.boundary_result.is_some() {
            return Err(FrameError::InvalidProgramCounter.into());
        }
        let next_generation = current
            .generation
            .checked_add(1)
            .ok_or(FrameError::GenerationExhausted)?;
        Ok(Some(Admission {
            descriptor: descriptor.clone(),
            frame,
            next_generation,
            old_entry: current.resumable_entry,
            capture_offset: entry.receiver_capture_offset_words,
            implementation: entry.implementation_anchor.map_or(0, |value| value.addr()),
        }))
    }

    pub(crate) fn try_restart_registered_tail(
        &mut self,
        heap: &Heap,
        owner: u64,
        machine: MachineCapability,
        capability: ResumableEntryCapability,
        receiver: Oop,
        arguments: &[Oop],
        nil: Oop,
    ) -> Result<bool, ExecutionError> {
        #[cfg(not(test))]
        let enabled = true;
        #[cfg(test)]
        let enabled = ENABLED.get();
        if !enabled {
            return Ok(false);
        }
        let admission = match self.admit_tail_entry(heap, owner, machine, capability, receiver) {
            Ok(Some(admission)) => admission,
            Ok(None) => return Ok(false),
            Err(error) => {
                let _ = self.release_acquired_entry(capability);
                return Err(error);
            }
        };
        self.transfer_acquired_entry_to_frame(capability)?;
        // All fallible semantic checks precede publication. Host Vec growth
        // does not run Birnam collection or callbacks, just as in arena push.
        let arena = self
            .machine_mut(owner, machine)
            .expect("admitted Machine")
            .arena_mut();
        let slot = &mut arena.slots[admission.frame.index];
        let plan = admission
            .descriptor
            .tail_entry_plan
            .get()
            .and_then(Option::as_ref)
            .expect("admitted immutable entry plan");
        slot.words.resize(plan.slot_count(), 0);
        // Oop and usize have identical size/alignment; every bit pattern is a
        // valid usize. This view is read-only and cannot alias the arena words.
        let argument_words = unsafe {
            core::slice::from_raw_parts(arguments.as_ptr().cast::<usize>(), arguments.len())
        };
        plan.write(
            &mut slot.words,
            receiver.addr(),
            argument_words,
            admission.implementation,
            |index| {
                heap.read_payload_word(receiver, admission.capture_offset + index)
                    .expect("validated capture payload without intervening mutation")
            },
            nil.addr(),
        );
        slot.transfer_scratch
            .resize(admission.descriptor.max_edge_copy_count, 0);
        slot.self_tail_entry = slot
            .descriptor
            .as_ref()
            .is_some_and(|old| Arc::ptr_eq(old, &admission.descriptor));
        slot.descriptor = Some(admission.descriptor);
        slot.program_counter = 0;
        slot.root_state_program_counter = 0;
        slot.pending_edge = None;
        slot.resumable_entry = Some(capability);
        slot.generation = admission.next_generation;
        // Return destination, stack depth, high-water state and live bit stay
        // exactly as pop followed by push would leave them. Old views go stale.
        if let Some(old) = admission.old_entry {
            self.release_entry_frame_pin(old)?;
        }
        #[cfg(test)]
        HITS.set(HITS.get() + 1);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const STEP: Method = Method::new(step_entry, core::ptr::null_mut());
    unsafe extern "C" fn step_entry(
        _: *mut core::ffi::c_void,
        _: Oop,
        _: *const birnam_abi::id_protocol::Operand,
        _: usize,
    ) -> birnam_abi::id_protocol::Outcome {
        std::hint::black_box(741701usize);
        birnam_abi::id_protocol::Outcome::loaded(birnam_abi::id_protocol::Operand::unit())
    }
    const THUNK: Method = Method::new(thunk_entry, core::ptr::null_mut());
    unsafe extern "C" fn thunk_entry(
        _: *mut core::ffi::c_void,
        _: Oop,
        _: *const birnam_abi::id_protocol::Operand,
        _: usize,
    ) -> birnam_abi::id_protocol::Outcome {
        std::hint::black_box(741702usize);
        birnam_abi::id_protocol::Outcome::loaded(birnam_abi::id_protocol::Operand::unit())
    }
    fn oop(value: usize) -> Oop {
        core::ptr::with_exposed_provenance_mut(value)
    }
    fn acquire(state: &mut ExecutionState) -> ResumableEntryCapability {
        let Some(StacklessTarget::Registered { entry }) = state
            .classify_resumable(MethodAddress::from_method(THUNK))
            .unwrap()
        else {
            panic!("registered target")
        };
        entry
    }
    struct Mode(bool);
    impl Drop for Mode {
        fn drop(&mut self) {
            ENABLED.set(self.0);
        }
    }

    #[test]
    fn planned_tail_entries_match_complete_frames_and_reject_without_publication() {
        let _mode = Mode(ENABLED.replace(true));
        let owner = 7417;
        let nil = oop(42);
        let mut state = ExecutionState::new(owner);
        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            entry_receiver_slot: 9,
            entry_argument_start: 1,
            entry_argument_count: 3,
            implementation_slot: Some(12),
            entry_capture_slots: vec![7, 0],
            step_method: STEP,
            ..probe_spec(14, 1)
        })
        .unwrap();
        let carrier = SemanticRegistrationV1::new(THUNK, descriptor.clone(), 3, 9, 1)
            .with_implementation_anchor(oop(88));
        state.register_resumable(owner, carrier).unwrap();
        state
            .note_direct_binding_added(MethodAddress::from_method(THUNK))
            .unwrap();
        let baseline = state.create_machine(owner).unwrap();
        let candidate = state.create_machine(owner).unwrap();
        for machine in [baseline, candidate] {
            let arena = state.machine_mut(owner, machine).unwrap().arena_mut();
            let frame = arena.push(descriptor.clone(), 0, nil, &[]).unwrap();
            arena.slots[frame.index].return_destination =
                Some(ReturnDestination { result_slot: 6 });
        }
        let mut heap = Heap::default();
        let mut receiver_words = [0usize; 2];
        let receiver = receiver_words.as_mut_ptr().cast();
        heap.insert(receiver, oop(1), 2);
        for iteration in 0..16 {
            let captures = [oop(100 + iteration), oop(200 + iteration)];
            for (index, capture) in captures.iter().enumerate() {
                assert!(heap.write_payload_word(receiver, index, capture.addr()));
            }
            let arguments: Vec<_> = (0..iteration % 6).map(|i| oop(300 + i)).collect();
            let mut handles = Vec::new();
            for machine in [baseline, candidate] {
                let old = state
                    .machine(owner, machine)
                    .unwrap()
                    .arena()
                    .current_handle()
                    .unwrap();
                // Exercise stale temporary values and scratch words, not just
                // an already-nil source Frame.
                state.machine_mut(owner, machine).unwrap().arena_mut().slots[old.index].words[11] =
                    999;
                let entry = acquire(&mut state);
                if machine == candidate {
                    assert!(
                        state
                            .try_restart_registered_tail(
                                &heap, owner, machine, entry, receiver, &arguments, nil
                            )
                            .unwrap()
                    );
                } else {
                    let materialized = state
                        .materialize_acquired_entry(entry, receiver, &arguments, &captures, nil)
                        .unwrap();
                    let old_entry = state
                        .machine(owner, machine)
                        .unwrap()
                        .arena()
                        .resumable_entry(old)
                        .unwrap();
                    state.transfer_acquired_entry_to_frame(entry).unwrap();
                    state
                        .machine_mut(owner, machine)
                        .unwrap()
                        .arena_mut()
                        .replace_current_with_entry(
                            materialized.descriptor,
                            0,
                            nil,
                            &materialized.initial_oops,
                            Some(entry),
                        )
                        .unwrap();
                    if let Some(old) = old_entry {
                        state.release_entry_frame_pin(old).unwrap();
                    }
                }
                let arena = state.machine(owner, machine).unwrap().arena();
                assert_eq!(arena.read_oop(old, 0), Err(FrameError::StaleHandle));
                let current = arena.current_handle().unwrap();
                assert_eq!(current.generation, old.generation + 1);
                handles.push(current);
            }
            let left = state.machine(owner, baseline).unwrap().arena();
            let right = state.machine(owner, candidate).unwrap().arena();
            assert_eq!(
                left.recovery_experiment_logical_frames(),
                right.recovery_experiment_logical_frames()
            );
            assert_eq!(
                left.slots[handles[0].index].self_tail_entry,
                right.slots[handles[1].index].self_tail_entry
            );
            assert_eq!(left.high_water_live_frames, right.high_water_live_frames);
            assert_eq!(
                left.slots[handles[0].index].words.capacity(),
                right.slots[handles[1].index].words.capacity()
            );
            // Continuation detach/restore must see the same initialized root
            // state and keep all entry pins; each detach invalidates old views.
            if iteration == 8 {
                for machine in [baseline, candidate] {
                    let arena = state.machine_mut(owner, machine).unwrap().arena_mut();
                    let segment = arena.detach_from(0).unwrap();
                    arena.install_segment(0, segment).unwrap();
                }
            }
        }
        let mut malformed_words = [0usize; 1];
        let malformed = malformed_words.as_mut_ptr().cast();
        heap.insert(malformed, oop(1), 1);
        for exhausted in [false, true] {
            if exhausted {
                let arena = state.machine_mut(owner, candidate).unwrap().arena_mut();
                let frame = arena.current_handle().unwrap();
                arena.slots[frame.index].generation = u64::MAX;
            }
            let before = state
                .machine(owner, candidate)
                .unwrap()
                .arena()
                .recovery_experiment_snapshot();
            let entry = acquire(&mut state);
            let result = state.try_restart_registered_tail(
                &heap,
                owner,
                candidate,
                entry,
                if exhausted { receiver } else { malformed },
                &[],
                nil,
            );
            assert!(
                matches!(result, Err(ExecutionError::InvalidAction)) && !exhausted
                    || matches!(
                        result,
                        Err(ExecutionError::Frame(FrameError::GenerationExhausted))
                    ) && exhausted
            );
            assert_eq!(
                state
                    .machine(owner, candidate)
                    .unwrap()
                    .arena()
                    .recovery_experiment_snapshot(),
                before
            );
            assert_eq!(
                state.resumable_entries[entry.index]
                    .entry
                    .as_ref()
                    .unwrap()
                    .acquired_entries,
                0
            );
        }
        for machine in [baseline, candidate] {
            state.destroy_machine(owner, machine).unwrap();
        }
        let entry = state
            .resumable_entries
            .iter()
            .find_map(|slot| slot.entry.as_ref())
            .unwrap();
        assert_eq!(entry.active_frame_pins, 0);
    }
}
