//! Retained Machine control adapters. Task policy remains in Birnam.
use super::*;
pub(super) unsafe fn descriptor(
    s: &State,
    step: Oop,
    arity: usize,
    returns: bool,
) -> Result<Arc<FrameDescriptor>, Oop> {
    unsafe {
        let step = key(s, step)?;
        let value = checked(FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: step.key() as u64,
                generation: 0,
                source_map: 0,
            },
            program_counter_count: if returns { 2 } else { 1 },
            driver_argument_count: 1,
            immediate_program_counter: true,
            slot_count: 4,
            entry_receiver_slot: 0,
            entry_argument_start: 1,
            entry_argument_count: arity,
            entry_capture_slots: vec![],
            implementation_slot: None,
            root_bitmap_words: vec![15; if returns { 2 } else { 1 }],
            call_sites: if returns {
                vec![CallSiteDescriptor {
                    kind: CallSiteKind::Call,
                    receiver_slot: 1,
                    selector_slot: 2,
                    argument_start: 0,
                    argument_count: 0,
                    result_slot: Some(3),
                    continuation_pc: Some(1),
                }]
            } else {
                vec![]
            },
            edges: if returns {
                vec![FrameEdgeSpec {
                    kind: EdgeTransitionKind::Call,
                    source_pc: 0,
                    target_pc: 1,
                    call_site_index: Some(0),
                    result_slot: Some(3),
                    copies: vec![],
                    clear_bitmap_words: vec![0],
                }]
            } else {
                vec![]
            },
            step_method: step,
        }))?;
        value.executable_owner.set(s.receiver);
        Ok(value)
    }
}
pub(super) unsafe fn install(s: &State, image: Oop, behavior: Oop) -> Result<(), Oop> {
    unsafe {
        let q = s.scope(&[image, behavior]);
        for (name, arity, returns) in [
            (b"controlEligible".as_slice(), 0, false),
            (b"protectedBlock:", 1, true),
            (b"leaveBoundary:", 1, false),
            (b"park", 0, true),
        ] {
            let step = q.keep(s.send(image, b"methodAt:", &[s.names[name]])?);
            let step = q.keep(s.send(step, b"withContext:", &[s.ns])?);
            let descriptor = descriptor(s, step, arity, returns)?;
            let method = q.keep(register_descriptor(s, descriptor, step, s.receiver)?);
            s.send(behavior, b"addMethod", &[s.names[name], method])?;
        }
        Ok(())
    }
}
unsafe fn step(c: Oop, machine: Oop, a: *const Oop, n: usize, kind: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [pc] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let pc = index(s, *pc)?;
        let capability = cap(s, machine)?;
        if checked(s.engine.borrow().read_current_oop(OWNER, capability, 0))? != machine {
            return Err(invalid());
        }
        if kind == 3 && pc == 0 {
            require_control_driver(s, machine)?;
        }
        let eligible = if kind == 4 {
            let value = checked(s.engine.borrow().machine(OWNER, capability))?.control_eligible();
            s.named(if value { b"true" } else { b"false" })?
        } else {
            s.nil
        };
        let result = {
            let mut e = s.engine.borrow_mut();
            let mut storage = s.storage.borrow_mut();
            match kind {
                1 if pc == 0 => {
                    checked(e.write_current_oop(
                        OWNER,
                        capability,
                        2,
                        s.names[b"value".as_slice()],
                    ))?;
                    e.publish_boundary_call(&mut storage, OWNER, capability, 0)
                }
                2 => e.publish_boundary_exit(&mut storage, OWNER, capability, 1),
                3 if pc == 0 => {
                    let delimiter = checked(e.current_delimiter(OWNER, capability))?;
                    e.publish_suspend(&mut storage, OWNER, capability, 3, delimiter, 1)
                }
                4 => {
                    checked(e.write_current_oop(OWNER, capability, 3, eligible))?;
                    e.publish_return(&mut storage, OWNER, capability, 3)
                }
                _ => e.publish_return(&mut storage, OWNER, capability, 3),
            }
        };
        control_checked(s, result)
    })()
    .unwrap_or_else(abi::fault_reference)
}
macro_rules! adapter {
    ($name:ident,$kind:expr) => {
        pub(in super::super) unsafe extern "C" fn $name(
            _: Oop,
            c: Oop,
            r: Oop,
            a: *const Oop,
            n: usize,
        ) -> Oop {
            unsafe { step(c, r, a, n, $kind) }
        }
    };
}
adapter!(protected, 1);
adapter!(leave, 2);
adapter!(park, 3);
adapter!(eligible, 4);
