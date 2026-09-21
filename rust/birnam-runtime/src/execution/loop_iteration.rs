//! Private evidence of an actual caller self-tail transition, never a PC heuristic.
use super::*;

#[derive(Clone)]
pub(crate) struct Stamp {
    frame: FrameHandle,
    descriptor: Arc<FrameDescriptor>,
    self_tail_entry: bool,
}

impl Stamp {
    pub(crate) fn follows(&self, previous: &Self) -> bool {
        self.self_tail_entry
            && self.frame.arena == previous.frame.arena
            && self.frame.index == previous.frame.index
            && Some(self.frame.generation) == previous.frame.generation.checked_add(1)
            && Arc::ptr_eq(&self.descriptor, &previous.descriptor)
    }
}

impl FrameArena {
    pub(crate) fn caller_loop_stamp(&self) -> Option<Stamp> {
        let index = *self.stack.get(self.stack.len().checked_sub(2)?)?;
        let slot = self.slots.get(index)?;
        slot.live.then(|| Stamp {
            frame: FrameHandle {
                arena: self.identity,
                index,
                generation: slot.generation,
            },
            descriptor: slot.descriptor.as_ref().expect("live descriptor").clone(),
            self_tail_entry: slot.self_tail_entry,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observe(arena: &mut FrameArena, descriptor: &Arc<FrameDescriptor>) -> Stamp {
        arena
            .push(descriptor.clone(), 0, core::ptr::null_mut(), &[])
            .unwrap();
        let stamp = arena.caller_loop_stamp().unwrap();
        arena.pop().unwrap();
        stamp
    }

    #[test]
    fn stamps_require_actual_self_tail_and_do_not_survive_restore_or_slot_reuse() {
        let descriptor = FrameDescriptor::validate_v2(probe_spec(2, 1)).unwrap();
        let changed = FrameDescriptor::validate_v2(probe_spec(2, 1)).unwrap();
        let nil = core::ptr::null_mut();
        let mut arena = FrameArena::new(7419);
        assert!(arena.caller_loop_stamp().is_none());
        arena.push(descriptor.clone(), 0, nil, &[]).unwrap();
        let first = observe(&mut arena, &descriptor);
        let again = observe(&mut arena, &descriptor);
        assert!(!again.follows(&first), "same iteration is not a backedge");
        arena
            .replace_current(descriptor.clone(), 0, nil, &[])
            .unwrap();
        let second = observe(&mut arena, &descriptor);
        assert!(second.follows(&first));
        arena.pop().unwrap();
        arena.push(descriptor.clone(), 0, nil, &[]).unwrap();
        let fresh = observe(&mut arena, &descriptor);
        assert!(
            !fresh.follows(&second),
            "pop/push must not impersonate a tail"
        );
        arena.replace_current(changed.clone(), 0, nil, &[]).unwrap();
        let changed_stamp = observe(&mut arena, &descriptor);
        assert!(!changed_stamp.follows(&fresh), "changed implementation");
        arena.replace_current(changed.clone(), 0, nil, &[]).unwrap();
        let before_capture = observe(&mut arena, &descriptor);
        assert!(before_capture.follows(&changed_stamp));
        let segment = arena.detach_from(0).unwrap();
        arena.install_segment(0, segment).unwrap();
        let restored = observe(&mut arena, &descriptor);
        assert!(
            !restored.follows(&before_capture),
            "restoration clears loop evidence"
        );
        arena.replace_current(changed.clone(), 0, nil, &[]).unwrap();
        assert!(observe(&mut arena, &descriptor).follows(&restored));
        let mut foreign = FrameArena::new(7420);
        foreign.push(changed.clone(), 0, nil, &[]).unwrap();
        foreign
            .replace_current(changed.clone(), 0, nil, &[])
            .unwrap();
        assert!(!observe(&mut foreign, &descriptor).follows(&first));
        let index = *arena.stack.last().unwrap();
        arena.slots[index].generation = u64::MAX;
        assert_eq!(
            arena.replace_current(changed, 0, nil, &[]),
            Err(FrameError::GenerationExhausted)
        );
    }
}
