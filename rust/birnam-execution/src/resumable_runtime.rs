//! Oop message connection to the retained registration and Machine algorithms.
use super::*;
use crate::{entry::EntryKey, execution::*};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
const OWNER: u64 = 1;
#[derive(Clone)]
struct Registered {
    method: Oop,
    step: Oop,
    owner: Oop,
    descriptor: Arc<FrameDescriptor>,
    key: EntryKey,
    capability: ResumableEntryCapability,
    lease: GenerationLeaseCapability,
}
#[derive(Default)]
pub(super) struct Runtime {
    pub roots: Vec<Oop>,
    pub block_values: Vec<Oop>,
    default_queries: Vec<Oop>,
    records: HashMap<usize, Registered>,
    methods: HashMap<usize, usize>,
    context_vt: Oop,
    template: Oop,
    run_descriptor: Option<Arc<FrameDescriptor>>,
    active: Vec<MachineCapability>,
    pending: HashSet<u64>,
}
pub(super) const METHOD_NAMES: &[&[u8]] = &[
    b"fromStep:descriptor:selection:owner:",
    b"resumableEntry",
    b"runBlockStep",
    b"new",
    b"runBlock:",
    b"runBlock:budget:",
    b"continueWithBudget:",
    b"destroy",
    b"executionState",
    b"isRunnable",
    b"liveFrameCount",
    b"oopAt:",
    b"oopAt:put:",
    b"continueAt:",
    b"callSite:",
    b"tailCallSite:",
    b"returnAt:",
];
fn checked<T, E>(value: Result<T, E>) -> Result<T, Oop> {
    value.map_err(|_| invalid())
}

fn registration(s: &State, method: Oop) -> Option<Registered> {
    let runtime = s.resume.borrow();
    runtime
        .records
        .get(runtime.methods.get(&method.addr())?)
        .cloned()
}
// These retained Methods are equality witnesses, never cached calls. The
// existing Id guard checks the matcher itself and all current query bindings.
const DEFAULT_QUERIES: &[&[u8]] = &[b"usesDefaultLookup:", b"bind:to:", b"nativeBindingsMatch:"];
unsafe fn pure_lookup(s: &State, receiver: Oop) -> Result<bool, Oop> {
    unsafe {
        let witnesses = s.resume.borrow().default_queries.clone();
        if witnesses.len() != DEFAULT_QUERIES.len() {
            return Ok(false);
        }
        let mut rows = Vec::with_capacity(witnesses.len() * 3);
        for (&name, method) in DEFAULT_QUERIES.iter().zip(witnesses) {
            rows.extend([s.id.root, s.names[name], method]);
        }
        if abi::as_size(s.send(s.id.root, b"nativeBindingsMatch:", &rows)?) != Some(1) {
            return Ok(false);
        }
        Ok(abi::as_size(s.send(s.id.root, b"usesDefaultLookup:", &[receiver])?) == Some(1))
    }
}
unsafe fn number(s: &State, value: usize) -> Result<Oop, Oop> {
    unsafe {
        s.send(
            s.named(b"Integer")?,
            b"fromWord:",
            &[abi::size(value).ok_or_else(invalid)?],
        )
    }
}
unsafe fn index(s: &State, value: Oop) -> Result<usize, Oop> {
    unsafe {
        if let Some(v) = abi::as_size(value) {
            return Ok(v);
        }
        let q = s.scope(&[value]);
        let word = q.keep(s.send(s.named(b"Integer")?, b"wordOf:", &[value])?);
        if let Some(v) = abi::as_size(word) {
            return Ok(v);
        }
        let lo = abi::as_size(s.send(word, b"low32", &[])?).ok_or_else(invalid)?;
        let hi = abi::as_size(s.send(word, b"high32", &[])?).ok_or_else(invalid)?;
        if lo > u32::MAX as usize || hi > 0x7fffffff {
            return Err(invalid());
        }
        Ok(lo | (hi << 32))
    }
}
unsafe fn bytes(s: &State, value: Oop, limit: usize) -> Result<Vec<u8>, Oop> {
    unsafe {
        let q = s.scope(&[value]);
        let buffer = q.keep(s.send(value, b"bytes", &[])?);
        let count = index(s, s.send(buffer, b"sizeWord", &[])?)?;
        if count > limit {
            return Err(invalid());
        }
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(count).map_err(|_| invalid())?;
        for i in 0..count {
            let value = s.send(buffer, b"atWord:", &[abi::size(i).unwrap()])?;
            bytes.push(
                abi::as_size(value)
                    .and_then(|v| u8::try_from(v).ok())
                    .ok_or_else(invalid)?,
            );
        }
        Ok(bytes)
    }
}
unsafe fn key(s: &State, method: Oop) -> Result<EntryKey, Oop> {
    unsafe {
        let identity = abi::as_size(s.send(s.id.root, b"identityOf:", &[method])?)
            .filter(|n| *n != 0)
            .ok_or_else(invalid)?;
        Ok(EntryKey::new(method, identity))
    }
}
unsafe fn allocate(s: &State, behavior: Oop, words: usize) -> Result<Oop, Oop> {
    unsafe {
        let q = s.scope(&[behavior]);
        let value = q.keep(
            s.send(
                behavior,
                b"allocate",
                &[
                    abi::size(words.checked_mul(size_of::<Oop>()).ok_or_else(invalid)?)
                        .ok_or_else(invalid)?,
                ],
            )?,
        );
        s.storage.borrow_mut().insert(value, behavior, words);
        Ok(value)
    }
}
fn cap(s: &State, machine: Oop) -> Result<MachineCapability, Oop> {
    checked(
        s.engine
            .borrow()
            .capability_for_object(&s.storage.borrow(), machine),
    )
}
unsafe fn refresh(s: &State, value: Oop) -> Result<(), Oop> {
    unsafe {
        let expected = s.storage.borrow().payload_size(value).ok_or_else(invalid)?;
        let behavior = s.send(s.id.root, b"behaviorOf:", &[value])?;
        if abi::as_size(s.send(s.id.root, b"allocationSizeOf:", &[value])?) != Some(expected) {
            return Err(invalid());
        }
        s.storage.borrow_mut().refresh_behavior(value, behavior);
        Ok(())
    }
}
pub(super) unsafe extern "C" fn from_step(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [step, description, selection, owner] = borrowed_slice(a, n).ok_or_else(invalid)?
        else {
            return Err(invalid());
        };
        let q = s.scope(&[*step, *owner, *description, *selection]);
        let description = bytes(
            s,
            *description,
            birnam_artifact_codec::frame_descriptor::MAX_DESCRIPTOR_BYTES,
        )?;
        let selection = bytes(
            s,
            *selection,
            birnam_artifact_codec::stackless_selection::MAX_CANONICAL_VALUE_BYTES,
        )?;
        let selected = checked(
            birnam_artifact_codec::stackless_selection::inspect_artifact_selection(&selection),
        )?;
        if selected.row_count() != 1 {
            return Err(invalid());
        }
        let row = checked(selected.rows().next().ok_or_else(invalid)?)?;
        let path: Vec<_> = row.path.iter().collect();
        let authority = birnam_artifact_codec::stackless_selection::SelectionSpec {
            source_revision: row.source_revision,
            root_kind: row.root_kind,
            root_index: row.root_index,
            path: &path,
            implementation_kind: row.implementation_kind,
            disposition: row.disposition,
        };
        let generation = abi::as_size(s.send(s.id.root, b"identityOf:", &[*owner])?)
            .filter(|n| *n != 0)
            .ok_or_else(invalid)? as u64;
        let descriptor = checked(FrameDescriptor::admit_v2(
            &description,
            authority,
            generation,
            key(s, *step)?,
        ))?;
        let binding = descriptor.v2_entry_binding().ok_or_else(invalid)?;
        // This milestone admits plain Block bodies. Implementation-slot adapters are
        // a separate existing constructor contract, never silently ignored here.
        if binding.implementation_slot.is_some() {
            return Err(invalid());
        }
        let (context_vt, template) = {
            let r = s.resume.borrow();
            (r.context_vt, r.template)
        };
        let context = q.keep(allocate(s, context_vt, 0)?);
        let method = q.keep(s.send(template, b"withContext:", &[context])?);
        let method_key = key(s, method)?;
        descriptor.executable_owner.set(method);
        let mut semantic = SemanticRegistrationV1::new(
            method_key,
            descriptor.clone(),
            binding.argument_count,
            binding.receiver_slot,
            binding.argument_start,
        );
        semantic.receiver_capture_offset_words = 1;
        let mut engine = s.engine.borrow_mut();
        let capability = checked(engine.register_resumable(OWNER, semantic))?;
        let lease = checked(engine.publish_owned(capability))?;
        drop(engine);
        let mut runtime = s.resume.borrow_mut();
        runtime.methods.insert(method.addr(), context.addr());
        runtime.records.insert(
            context.addr(),
            Registered {
                method,
                step: *step,
                owner: *owner,
                descriptor,
                key: method_key,
                capability,
                lease,
            },
        );
        Ok(method)
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe fn capture_alias(s: &State, original: Oop, captured: Oop) -> Result<(), Oop> {
    unsafe {
        let context = s.resume.borrow().methods.get(&original.addr()).copied();
        if let Some(context) = context {
            if abi::as_size(s.send(s.id.root, b"sameNativeMethod:as:", &[original, captured])?)
                == Some(1)
            {
                s.resume
                    .borrow_mut()
                    .methods
                    .insert(captured.addr(), context);
            }
        }
        Ok(())
    }
}
unsafe fn new_machine(s: &State) -> Result<Oop, Oop> {
    unsafe {
        let o = s.engine.borrow().objects().ok_or_else(invalid)?;
        let q = s.scope(&[]);
        let mut values = Vec::new();
        for (vt, size) in [
            (o.machine_vtable, 3),
            (o.continue_action_vtable, 4),
            (o.call_action_vtable, 9),
            (o.tail_call_action_vtable, 8),
            (o.return_action_vtable, 5),
            (o.suspend_action_vtable, 7),
            (o.capture_action_vtable, 7),
            (o.restore_action_vtable, 6),
            (o.abort_action_vtable, 7),
            (o.boundary_call_action_vtable, 9),
            (o.exit_boundary_action_vtable, 5),
        ] {
            values.push(q.keep(allocate(s, vt, size)?));
        }
        let mut e = s.engine.borrow_mut();
        let cap = checked(e.create_machine_with_objects(
            OWNER, values[0], values[1], values[2], values[3], values[4], values[5], values[6],
            values[7], values[8],
        ))?;
        let m = checked(e.machine_mut(OWNER, cap))?;
        m.boundary_call_action = values[9];
        m.exit_boundary_action = values[10];
        if !e.initialize_machine_payload(&mut s.storage.borrow_mut(), values[0], cap) {
            return Err(invalid());
        }
        Ok(values[0])
    }
}
unsafe fn acquire(
    s: &State,
    r: &Registered,
    receiver: Oop,
    args: &[Oop],
) -> Result<(ResumableEntryCapability, MaterializedEntry), Oop> {
    unsafe {
        let q = s.scope(&[r.method, receiver]);
        for &arg in args {
            q.keep(arg);
        }
        let Some(StacklessTarget::Registered { entry }) =
            checked(s.engine.borrow_mut().classify_resumable(r.key))?
        else {
            return Err(invalid());
        };
        let result = (|| {
            let (_, count) = checked(s.engine.borrow().acquired_entry_capture_layout(entry))?;
            let mut captures = Vec::new();
            for i in 0..count {
                captures.push(q.keep(s.send(
                    receiver,
                    b"captureWordAt:",
                    &[abi::size(i).unwrap()],
                )?));
            }
            checked(
                s.engine
                    .borrow()
                    .materialize_acquired_entry(entry, receiver, args, &captures, s.nil),
            )
        })();
        if result.is_err() {
            checked(s.engine.borrow_mut().release_acquired_entry(entry))?;
        }
        result.map(|v| (entry, v))
    }
}
fn push(
    s: &State,
    cap: MachineCapability,
    entry: ResumableEntryCapability,
    value: MaterializedEntry,
    destination: Option<ReturnDestination>,
    tail: bool,
) -> Result<(), Oop> {
    let mut e = s.engine.borrow_mut();
    checked(e.transfer_acquired_entry_to_frame(entry))?;
    let result = (|| {
        let m = checked(e.machine_mut(OWNER, cap))?;
        if tail {
            let old = checked(
                m.arena()
                    .resumable_entry(m.arena().current_handle().ok_or_else(invalid)?),
            )?;
            checked(m.arena_mut().replace_current_with_entry(
                value.descriptor,
                0,
                s.nil,
                &value.initial_oops,
                Some(entry),
            ))?;
            if let Some(old) = old {
                checked(e.release_entry_frame_pin(old))?;
            }
        } else {
            checked(m.arena_mut().push_with_return_and_entry(
                value.descriptor,
                0,
                s.nil,
                &value.initial_oops,
                destination,
                Some(entry),
            ))?;
        }
        Ok(())
    })();
    if result.is_err() {
        checked(e.release_entry_frame_pin(entry))?;
    }
    result
}
fn finish(s: &State, cap: MachineCapability, value: Oop) -> Result<Option<Oop>, Oop> {
    let mut e = s.engine.borrow_mut();
    let m = checked(e.machine_mut(OWNER, cap))?;
    let frame = m.arena().current_handle().ok_or_else(invalid)?;
    let destination = checked(m.arena().return_destination(frame))?;
    let entry = checked(m.arena().resumable_entry(frame))?;
    checked(m.arena_mut().inherit_return_delivery(frame))?;
    checked(m.arena_mut().pop())?;
    let answer = if let Some(destination) = destination {
        let caller = m.arena().current_handle().ok_or_else(invalid)?;
        checked(m.arena_mut().commit_call_transition(
            caller,
            destination.result_slot,
            s.nil,
            value,
        ))?;
        None
    } else {
        checked(m.complete_barrier_drive(OWNER))?;
        Some(value)
    };
    if let Some(entry) = entry {
        checked(e.release_entry_frame_pin(entry))?;
    }
    Ok(answer)
}
struct Active<'a>(&'a State, MachineCapability);
impl Drop for Active<'_> {
    fn drop(&mut self) {
        assert_eq!(self.0.resume.borrow_mut().active.pop(), Some(self.1));
    }
}
unsafe fn drive(
    s: &State,
    machine: Oop,
    cap: MachineCapability,
    mut budget: Option<usize>,
) -> Result<Oop, Oop> {
    unsafe {
        let _roots = s.scope(&[machine]);
        checked(s.engine.borrow_mut().begin_managed_turn())?;
        s.depth.set(s.depth.get() + 1);
        let _turn = Invocation(s);
        s.resume.borrow_mut().active.push(cap);
        let _active = Active(s, cap);
        let result = (|| {
            loop {
                if let Some(left) = budget.as_mut() {
                    if *left == 0 {
                        let mut e = s.engine.borrow_mut();
                        let m = checked(e.machine_mut(OWNER, cap))?;
                        if m.control_eligible() {
                            checked(m.stop_runnable(OWNER))?;
                            return Ok(machine);
                        }
                    } else {
                        *left -= 1;
                    }
                }
                let (step, pc, argument_count) = {
                    let e = s.engine.borrow();
                    let m = checked(e.machine(OWNER, cap))?;
                    let frame = m.arena().current_handle().ok_or_else(invalid)?;
                    (
                        checked(m.arena().descriptor(frame))?
                            .step_method()
                            .to_method(),
                        checked(m.arena().program_counter(frame))?,
                        checked(m.arena().descriptor(frame))?.driver_argument_count(),
                    )
                };
                let q = s.scope(&[step]);
                let arguments = match argument_count {
                    0 => Vec::new(),
                    1 => vec![q.keep(number(s, pc)?)],
                    _ => return Err(invalid()),
                };
                let action = q.keep(s.invoke(step, machine, &arguments)?);
                refresh(s, action)?;
                let decoded = checked(s.engine.borrow().decode_action(
                    &s.storage.borrow(),
                    OWNER,
                    cap,
                    action,
                ))?;
                match decoded {
                    DecodedAction::Continue => {
                        checked(
                            s.engine
                                .borrow_mut()
                                .commit_current_continue(OWNER, cap, s.nil),
                        )?;
                    }
                    DecodedAction::Return { value_slot } => {
                        let value =
                            checked(s.engine.borrow().read_current_oop(OWNER, cap, value_slot))?;
                        if let Some(v) = finish(s, cap, value)? {
                            return Ok(v);
                        }
                    }
                    DecodedAction::Call {
                        receiver_slot,
                        selector_slot,
                        argument_start,
                        argument_count,
                        result_slot: _,
                    }
                    | DecodedAction::TailCall {
                        receiver_slot,
                        selector_slot,
                        argument_start,
                        argument_count,
                    } => {
                        let tail = matches!(decoded, DecodedAction::TailCall { .. });
                        let result_slot = if let DecodedAction::Call { result_slot, .. } = decoded {
                            result_slot
                        } else {
                            0
                        };
                        let operands = checked(s.engine.borrow().copy_send_operands(
                            OWNER,
                            cap,
                            receiver_slot,
                            selector_slot,
                            argument_start,
                            argument_count,
                        ))?;
                        q.keep(operands.receiver);
                        q.keep(operands.selector);
                        for &arg in &operands.arguments {
                            q.keep(arg);
                        }
                        // A preliminary lookup is allowed only when the current
                        // lookup is callback-free. Unknown/custom lookup is sent
                        // once through ordinary Id and its native boundary.
                        let selected = if pure_lookup(s, operands.receiver)? {
                            q.keep(s.send(
                                s.id.root,
                                b"bind:to:",
                                &[operands.selector, operands.receiver],
                            )?)
                        } else {
                            ptr::null_mut()
                        };
                        let args = operands.arguments;
                        let registered = registration(s, selected).or_else(|| {
                            if s.resume.borrow().block_values.contains(&selected) {
                                let method = s.field(operands.receiver, 0).ok()?;
                                registration(s, method)
                            } else {
                                None
                            }
                        });
                        if let Some(r) = registered {
                            let (entry, value) = acquire(s, &r, operands.receiver, &args)?;
                            let destination = if tail {
                                None
                            } else {
                                Some(ReturnDestination { result_slot })
                            };
                            push(s, cap, entry, value, destination, tail)?;
                        } else {
                            checked(
                                checked(s.engine.borrow_mut().machine_mut(OWNER, cap))?
                                    .enter_synchronous_boundary(OWNER),
                            )?;
                            // Never reflect on the selected Method here. Ordinary
                            // Id dispatch enters it directly and handles DNU.
                            let value = s.id.send(operands.receiver, operands.selector, &args);
                            checked(
                                checked(s.engine.borrow_mut().machine_mut(OWNER, cap))?
                                    .leave_synchronous_boundary(OWNER),
                            )?;
                            let value = q.keep(s.language_result(value)?);
                            if tail {
                                if let Some(v) = finish(s, cap, value)? {
                                    return Ok(v);
                                }
                            } else {
                                checked(s.engine.borrow_mut().commit_current_call_result(
                                    OWNER,
                                    cap,
                                    result_slot,
                                    s.nil,
                                    value,
                                ))?;
                            }
                        }
                    }
                    _ => return Err(invalid()),
                }
            }
        })();
        if result.is_err() {
            if let Ok(m) = s.engine.borrow_mut().machine_mut(OWNER, cap) {
                let _ = m.fail(OWNER);
            }
        }
        result
    }
}
unsafe fn start_registered(
    s: &State,
    r: &Registered,
    receiver: Oop,
    args: &[Oop],
) -> Result<Oop, Oop> {
    unsafe {
        let inherited = s.resume.borrow().active.last().copied();
        let machine = if let Some(cap) = inherited {
            checked(s.engine.borrow().machine_object(OWNER, cap))?
        } else {
            new_machine(s)?
        };
        let _q = s.scope(&[machine, r.method, receiver]);
        let cap = cap(s, machine)?;
        let (entry, value) = acquire(s, r, receiver, args)?;
        let begun = {
            let mut e = s.engine.borrow_mut();
            let m = checked(e.machine_mut(OWNER, cap))?;
            checked(if inherited.is_some() {
                m.begin_nested_drive(OWNER)
            } else {
                m.begin_initial_drive(OWNER)
            })
        };
        if let Err(error) = begun {
            checked(s.engine.borrow_mut().release_acquired_entry(entry))?;
            return Err(error);
        }
        push(s, cap, entry, value, None, false)?;
        drive(s, machine, cap, None)
    }
}
pub(super) unsafe extern "C" fn entry(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let p = INSTANCE.load(Ordering::Acquire);
        if p.is_null() {
            return Err(invalid());
        }
        let s = state((*p).ns)?;
        let record = s
            .resume
            .borrow()
            .records
            .get(&c.addr())
            .cloned()
            .ok_or_else(invalid)?;
        let args = borrowed_slice(a, n).ok_or_else(invalid)?;
        start_registered(s, &record, r, args)
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn run_step(
    _: Oop,
    c: Oop,
    machine: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [pc] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        match index(s, *pc)? {
            0 => s.send(machine, b"callSite:", &[number(s, 0)?]),
            1 => s.send(machine, b"returnAt:", &[number(s, 3)?]),
            _ => Err(invalid()),
        }
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn new(_: Oop, c: Oop, _: Oop, _: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        if n != 0 {
            return Err(invalid());
        }
        new_machine(s)
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe fn run_block(c: Oop, machine: Oop, a: *const Oop, n: usize, bounded: bool) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let args = borrowed_slice(a, n).ok_or_else(invalid)?;
        if n != 1 + usize::from(bounded) {
            return Err(invalid());
        }
        let q = s.scope(args);
        q.keep(machine);
        s.send(s.gc, b"prepareAllocation", &[])?;
        let budget = if bounded {
            Some(index(s, args[1])?)
        } else {
            None
        };
        let cap = cap(s, machine)?;
        let descriptor = s
            .resume
            .borrow()
            .run_descriptor
            .clone()
            .ok_or_else(invalid)?;
        {
            let mut e = s.engine.borrow_mut();
            let m = checked(e.machine_mut(OWNER, cap))?;
            checked(m.begin_initial_drive(OWNER))?;
            checked(m.arena_mut().push(
                descriptor,
                0,
                s.nil,
                &[(0, s.nil), (1, args[0]), (2, s.names[b"value".as_slice()])],
            ))?;
        }
        drive(s, machine, cap, budget)
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn run(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { run_block(c, r, a, n, false) }
}
pub(super) unsafe extern "C" fn run_bounded(
    _: Oop,
    c: Oop,
    r: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    unsafe { run_block(c, r, a, n, true) }
}
pub(super) unsafe extern "C" fn continue_drive(
    _: Oop,
    c: Oop,
    r: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [budget] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        s.send(s.gc, b"prepareAllocation", &[])?;
        let budget = index(s, *budget)?;
        let cap = cap(s, r)?;
        checked(checked(s.engine.borrow_mut().machine_mut(OWNER, cap))?.resume_runnable(OWNER))?;
        drive(s, r, cap, Some(budget))
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn destroy(_: Oop, c: Oop, r: Oop, _: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        if n != 0 {
            return Err(invalid());
        }
        s.send(s.gc, b"prepareAllocation", &[])?;
        let cap = cap(s, r)?;
        checked(s.engine.borrow_mut().destroy_machine_explicit(OWNER, cap))?;
        Ok(r)
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn lifecycle(_: Oop, c: Oop, r: Oop, _: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        if n != 0 {
            return Err(invalid());
        }
        let life = s
            .engine
            .borrow()
            .machine(OWNER, cap(s, r)?)
            .map_err(|_| invalid())?
            .lifecycle();
        let name: &[u8] = match life {
            MachineLifecycle::New => b"new",
            MachineLifecycle::Running => b"running",
            MachineLifecycle::Runnable => b"runnable",
            MachineLifecycle::Completed => b"completed",
            MachineLifecycle::Suspended => b"suspended",
            MachineLifecycle::Failed => b"failed",
            MachineLifecycle::Destroyed => b"destroyed",
        };
        Ok(s.names[name])
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn is_runnable(
    _: Oop,
    c: Oop,
    r: Oop,
    _: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        if n != 0 {
            return Err(invalid());
        }
        let runnable = checked(s.engine.borrow().machine(OWNER, cap(s, r)?))?.lifecycle()
            == MachineLifecycle::Runnable;
        s.named(if runnable { b"true" } else { b"false" })
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn frame_count(
    _: Oop,
    c: Oop,
    r: Oop,
    _: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        if n != 0 {
            return Err(invalid());
        }
        let count = checked(s.engine.borrow().machine_diagnostics(OWNER, cap(s, r)?))?.live_frames;
        number(s, count)
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn read(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [i] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let i = index(s, *i)?;
        checked(s.engine.borrow().read_current_oop(OWNER, cap(s, r)?, i))
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn write(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [i, value] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let i = index(s, *i)?;
        let cap = cap(s, r)?;
        checked(
            s.engine
                .borrow_mut()
                .write_current_oop(OWNER, cap, i, *value),
        )?;
        Ok(*value)
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe fn action(c: Oop, r: Oop, a: *const Oop, n: usize, kind: u8) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [i] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let i = index(s, *i)?;
        let cap = cap(s, r)?;
        let mut e = s.engine.borrow_mut();
        let mut storage = s.storage.borrow_mut();
        checked(match kind {
            0 => e.publish_continue(&mut storage, OWNER, cap, i),
            1 => e.publish_call_site(&mut storage, OWNER, cap, i, CallSiteKind::Call),
            2 => e.publish_call_site(&mut storage, OWNER, cap, i, CallSiteKind::TailCall),
            _ => e.publish_return(&mut storage, OWNER, cap, i),
        })
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn continue_at(
    _: Oop,
    c: Oop,
    r: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    unsafe { action(c, r, a, n, 0) }
}
pub(super) unsafe extern "C" fn call_site(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { action(c, r, a, n, 1) }
}
pub(super) unsafe extern "C" fn tail_call_site(
    _: Oop,
    c: Oop,
    r: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    unsafe { action(c, r, a, n, 2) }
}
pub(super) unsafe extern "C" fn return_at(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { action(c, r, a, n, 3) }
}
pub(super) fn references(s: &State, object: Oop) -> Result<Option<Vec<Oop>>, Oop> {
    if let Some(record) = s.resume.borrow().records.get(&object.addr()) {
        return Ok(Some(vec![record.method, record.step, record.owner]));
    }
    let family = s.storage.borrow().family_of(object);
    let objects = s.engine.borrow().objects();
    if family == Some(s.block_vt) {
        return Ok(None);
    }
    if family.is_none() {
        return Ok(None);
    }
    let mut values = Vec::new();
    if let Some(o) = objects {
        if family == Some(o.machine_vtable) {
            s.engine.borrow().trace_object_roots(
                &s.storage.borrow(),
                object,
                o.machine_vtable,
                |v| values.push(v),
            );
        }
    }
    Ok(Some(values))
}
pub(super) fn retire(s: &State, values: &[Oop]) -> Result<(), Oop> {
    let Some(o) = s.engine.borrow().objects() else {
        return Ok(());
    };
    for &value in values {
        if s.storage.borrow().family_of(value) == Some(o.machine_vtable)
            && s.storage.borrow().payload_size(value) == Some(3 * size_of::<Oop>())
        {
            unsafe { s.engine.borrow_mut().finalize_detached_machine(value) }
        }
    }
    for value in values {
        let record = s.resume.borrow_mut().records.remove(&value.addr());
        if let Some(record) = record {
            checked(
                s.engine
                    .borrow_mut()
                    .retire_owned(record.capability, record.lease),
            )?;
            s.resume
                .borrow_mut()
                .pending
                .insert(record.descriptor.identity().generation);
        }
        s.resume.borrow_mut().methods.remove(&value.addr());
    }
    Ok(())
}
pub(super) fn checkpoint(s: &State) -> Result<(), Oop> {
    let pending = s.resume.borrow().pending.clone();
    for generation in pending {
        if checked(s.engine.borrow_mut().unregister_code_generation(generation))? {
            s.resume.borrow_mut().pending.remove(&generation);
        }
    }
    Ok(())
}

pub(super) unsafe fn install(s: &State, image: Oop, base: Oop) -> Result<(), Oop> {
    unsafe {
        let q = s.scope(&[image, base]);
        let fresh = || -> Result<(Oop, Oop), Oop> {
            let vt = q.keep(s.send(base, b"delegated", &[])?);
            let prototype = q.keep(allocate(s, vt, 0)?);
            s.resume.borrow_mut().roots.extend([vt, prototype]);
            s.send(s.gc, b"trace:with:", &[vt, s.receiver])?;
            Ok((vt, prototype))
        };
        let (machine_vtable, machine_prototype) = fresh()?;
        let (frame_view_vtable, frame_view_prototype) = fresh()?;
        let (continuation_vtable, continuation_prototype) = fresh()?;
        let (continue_action_vtable, continue_action_prototype) = fresh()?;
        let (call_action_vtable, call_action_prototype) = fresh()?;
        let (tail_call_action_vtable, tail_call_action_prototype) = fresh()?;
        let (return_action_vtable, return_action_prototype) = fresh()?;
        let (suspend_action_vtable, suspend_action_prototype) = fresh()?;
        let (capture_action_vtable, capture_action_prototype) = fresh()?;
        let (restore_action_vtable, restore_action_prototype) = fresh()?;
        let (boundary_call_action_vtable, boundary_call_action_prototype) = fresh()?;
        let (exit_boundary_action_vtable, exit_boundary_action_prototype) = fresh()?;
        let (abort_action_vtable, abort_action_prototype) = fresh()?;
        let (context_vt, _) = fresh()?;
        let objects = ExecutionObjects {
            machine_vtable,
            machine_prototype,
            frame_view_vtable,
            frame_view_prototype,
            continuation_vtable,
            continuation_prototype,
            continue_action_vtable,
            continue_action_prototype,
            call_action_vtable,
            call_action_prototype,
            tail_call_action_vtable,
            tail_call_action_prototype,
            return_action_vtable,
            return_action_prototype,
            suspend_action_vtable,
            suspend_action_prototype,
            capture_action_vtable,
            capture_action_prototype,
            restore_action_vtable,
            restore_action_prototype,
            boundary_call_action_vtable,
            boundary_call_action_prototype,
            exit_boundary_action_vtable,
            exit_boundary_action_prototype,
            boundary_call_kind: s.names[b"boundaryCall".as_slice()],
            exit_boundary_kind: s.names[b"exitBoundary".as_slice()],
            abort_action_vtable,
            abort_action_prototype,
            continue_kind: s.names[b"continue".as_slice()],
            call_kind: s.names[b"call".as_slice()],
            tail_call_kind: s.names[b"tailCall".as_slice()],
            return_kind: s.names[b"return".as_slice()],
            suspend_kind: s.names[b"suspend".as_slice()],
            capture_kind: s.names[b"capture".as_slice()],
            restore_kind: s.names[b"restore".as_slice()],
            abort_kind: s.names[b"abort".as_slice()],
            oop_slot_kind: s.names[b"oop".as_slice()],
            stale_frame_view_kind: s.names[b"staleFrameView".as_slice()],
            invalid_frame_slot_kind: s.names[b"invalidFrameSlot".as_slice()],
            oop_at: s.names[b"oopAt:".as_slice()],
            oop_at_put: s.names[b"oopAt:put:".as_slice()],
            continue_at: s.names[b"continueAt:".as_slice()],
            call_site: s.names[b"callSite:".as_slice()],
            tail_call_site: s.names[b"tailCallSite:".as_slice()],
            return_at: s.names[b"returnAt:".as_slice()],
        };
        s.resume.borrow_mut().roots.extend(objects.roots());
        s.engine.borrow_mut().install_objects(objects);
        let template = q.keep(s.send(
            image,
            b"methodAt:",
            &[s.names[b"resumableEntry".as_slice()]],
        )?);
        let step = q.keep(s.send(image, b"methodAt:", &[s.names[b"runBlockStep".as_slice()]])?);
        let step = q.keep(s.send(step, b"withContext:", &[s.ns])?);
        let descriptor = checked(FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 0,
                generation: 0,
                source_map: 0,
            },
            program_counter_count: 2,
            driver_argument_count: 1,
            slot_count: 4,
            entry_receiver_slot: 0,
            entry_argument_start: 1,
            entry_argument_count: 1,
            entry_capture_slots: vec![],
            implementation_slot: None,
            root_bitmap_words: vec![15; 2],
            call_sites: vec![CallSiteDescriptor {
                kind: CallSiteKind::Call,
                receiver_slot: 1,
                selector_slot: 2,
                argument_start: 0,
                argument_count: 0,
                result_slot: Some(3),
                continuation_pc: Some(1),
            }],
            edges: vec![FrameEdgeSpec {
                kind: EdgeTransitionKind::Call,
                source_pc: 0,
                target_pc: 1,
                call_site_index: Some(0),
                result_slot: Some(3),
                copies: vec![],
                clear_bitmap_words: vec![0],
            }],
            step_method: key(s, step)?,
        }))?;
        descriptor.executable_owner.set(s.receiver);
        {
            let mut runtime = s.resume.borrow_mut();
            runtime.context_vt = context_vt;
            runtime.template = template;
            runtime.run_descriptor = Some(descriptor);
            runtime.roots.extend([template, step]);
            let witnesses = runtime.block_values.clone();
            runtime.roots.extend(witnesses);
        }
        let execution_type = q.keep(s.send(s.id.root, b"behaviorOf:", &[s.receiver])?);
        for &name in METHOD_NAMES {
            if matches!(name, b"resumableEntry" | b"runBlockStep") {
                continue;
            }
            let target = if name == b"fromStep:descriptor:selection:owner:" {
                execution_type
            } else {
                machine_vtable
            };
            let method = q.keep(s.send(image, b"methodAt:", &[s.names[name]])?);
            let method = q.keep(s.send(method, b"withContext:", &[s.ns])?);
            s.send(target, b"addMethod", &[s.names[name], method])?;
        }
        let mut witnesses = Vec::new();
        for &name in DEFAULT_QUERIES {
            let method = q.keep(s.send(s.id.root, b"bind:to:", &[s.names[name], s.id.root])?);
            if abi::as_size(s.send(
                s.id.root,
                b"nativeBindingQuerySemantics:for:",
                &[method, s.names[name]],
            )?) != Some(1)
            {
                witnesses.clear();
                break;
            }
            witnesses.push(method);
        }
        {
            let mut runtime = s.resume.borrow_mut();
            runtime.roots.extend_from_slice(&witnesses);
            runtime.default_queries = witnesses;
        }
        for (name, value) in [
            (b"Machine".as_slice(), machine_prototype),
            (b"MachineVt", machine_vtable),
        ] {
            s.send(s.ns, b"at:put:", &[s.names[name], value])?;
        }
        Ok(())
    }
}
