//! Retained native Block/control adapters. Task policy remains in Birnam.
use super::machine::*;
use super::*;
use crate::execution::*;
use std::sync::Arc;
fn descriptor(s: &Execution, step: Method, arity: usize, returns: bool) -> Arc<FrameDescriptor> {
    let value = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
        identity: DescriptorIdentity {
            implementation: step as usize as u64,
            generation: 0,
            source_map: 0,
        },
        program_counter_count: if returns { 2 } else { 1 },
        driver_argument_count: 1,
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
        step_method: NativeEntry::new(step, ptr::from_ref(s).cast_mut().cast()),
    })
    .expect("retained native control descriptor");
    value.executable_owner.set(s.receiver);
    value
}
pub(super) unsafe fn install(s: &Execution, behavior: Oop) -> Result<(), Outcome> {
    unsafe {
        for (name, step, arity, returns) in [
            (b"controlEligible".as_slice(), eligible as Method, 0, false),
            (b"protectedBlock:", protected, 1, true),
            (b"leaveBoundary:", leave, 1, false),
            (b"park", park, 0, true),
        ] {
            let method = register_descriptor(s, descriptor(s, step, arity, returns), s.receiver)?;
            let _roots = s.scope(&[method]);
            s.send(
                behavior,
                b"addMethod",
                &[
                    Operand::object(s.names[name]),
                    Operand::entry(s.entry(method, false)?),
                ],
            )?;
        }
        Ok(())
    }
}
pub(super) unsafe fn run(
    s: &Execution,
    machine: Oop,
    block: Oop,
    budget: Option<usize>,
) -> Result<Operand, Outcome> {
    let cap = cap(s, machine)?;
    {
        let mut state = s.machines.borrow_mut();
        let m = checked(state.engine.machine_mut(OWNER, cap))?;
        checked(m.begin_initial_drive(OWNER))?;
        checked(m.arena_mut().push(
            descriptor(s, run_step, 1, true),
            0,
            s.nil,
            &[(0, machine), (1, block), (2, s.names[b"value".as_slice()])],
        ))?;
    }
    unsafe { loop_drive(s, machine, cap, budget) }
}
unsafe fn step(
    s: &Execution,
    machine: Oop,
    args: &[Operand],
    kind: usize,
) -> Result<Operand, Outcome> {
    unsafe {
        if args.len() != 1 {
            return Err(invalid());
        }
        let pc = index(s, args[0])?;
        let cap = cap(s, machine)?;
        let mut state = s.machines.borrow_mut();
        let State {
            engine,
            storage,
            booleans,
            ..
        } = &mut *state;
        if checked(engine.read_current_oop(OWNER, cap, 0))? != machine {
            return Err(invalid());
        }
        let action = match kind {
            0 | 1 if pc == 0 => {
                checked(engine.write_current_oop(OWNER, cap, 2, s.names[b"value".as_slice()]))?;
                if kind == 1 {
                    engine.publish_boundary_call(storage, OWNER, cap, 0)
                } else {
                    engine.publish_call_site(storage, OWNER, cap, 0, CallSiteKind::Call)
                }
            }
            2 => engine.publish_boundary_exit(storage, OWNER, cap, 1),
            3 if pc == 0 => {
                let delimiter = checked(engine.current_delimiter(OWNER, cap))?;
                engine.publish_suspend(storage, OWNER, cap, 3, delimiter, 1)
            }
            4 => {
                let eligible = checked(engine.machine(OWNER, cap))?.control_eligible();
                checked(engine.write_current_oop(OWNER, cap, 3, booleans[usize::from(eligible)]))?;
                engine.publish_return(storage, OWNER, cap, 3)
            }
            _ => engine.publish_return(storage, OWNER, cap, 3),
        };
        checked(action).map(Operand::object)
    }
}
macro_rules! adapter {
    ($name:ident,$kind:expr) => {
        unsafe extern "C" fn $name(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
            done((|| unsafe {
                let s = state(c)?;
                step(s, r, borrowed_slice(a, n).ok_or_else(invalid)?, $kind)
            })())
        }
    };
}
adapter!(run_step, 0);
adapter!(protected, 1);
adapter!(leave, 2);
adapter!(park, 3);
adapter!(eligible, 4);
