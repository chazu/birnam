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
    runs: HashMap<usize, Run>,
    returns: HashMap<usize, ReturnSite>,
    pub(super) return_methods: HashMap<usize, usize>,
    advance_template: Oop,
    return_template: Oop,
    sink_template: Oop,
    bridge_template: Oop,
    records: HashMap<usize, Registered>,
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
    b"advanceExecution",
    b"receiveCall",
    b"receiveRun",
    b"driveBridge",
];
fn checked<T, E>(value: Result<T, E>) -> Result<T, Oop> {
    value.map_err(|_| invalid())
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
unsafe fn runtime_state<'a>() -> Result<&'a State, Oop> {
    let p = INSTANCE.load(Ordering::Acquire);
    if p.is_null() {
        return Err(invalid());
    }
    unsafe { state((*p).ns) }
}
struct Active<'a>(&'a State, MachineCapability);
impl Drop for Active<'_> {
    fn drop(&mut self) {
        assert_eq!(self.0.resume.borrow_mut().active.pop(), Some(self.1));
    }
}
#[derive(Clone)]
struct ReturnSite {
    machine: Oop,
    frame: FrameHandle,
    result_slot: usize,
    tail: bool,
}
struct Run {
    advance: Oop,
    sink: Oop,
    answers: Vec<Oop>,
    budget: Option<usize>,
    pending: Oop,
    outcome: Option<Oop>,
    parked_depth: Option<usize>,
}
unsafe fn method_context(s: &State, template: Oop, context: Oop) -> Result<Oop, Oop> {
    unsafe { s.send(template, b"withContext:", &[context]) }
}
unsafe fn ensure_run(s: &State, machine: Oop, answer: Oop) -> Result<(), Oop> {
    unsafe {
        if s.resume.borrow().runs.contains_key(&machine.addr()) {
            s.resume
                .borrow_mut()
                .runs
                .get_mut(&machine.addr())
                .unwrap()
                .answers
                .push(answer);
            return Ok(());
        }
        let q = s.scope(&[machine, answer]);
        let (advance, sink) = {
            let r = s.resume.borrow();
            (r.advance_template, r.sink_template)
        };
        let advance = q.keep(method_context(s, advance, machine)?);
        let sink = q.keep(method_context(s, sink, machine)?);
        s.resume.borrow_mut().runs.insert(
            machine.addr(),
            Run {
                advance,
                sink,
                answers: vec![answer],
                budget: None,
                pending: advance,
                outcome: None,
                parked_depth: None,
            },
        );
        Ok(())
    }
}
fn fail_run(s: &State, machine: Oop) {
    if let Ok(cap) = cap(s, machine) {
        if let Ok(m) = s.engine.borrow_mut().machine_mut(OWNER, cap) {
            let _ = m.fail(OWNER);
        }
    }
    let mut r = s.resume.borrow_mut();
    if let Some(run) = r.runs.get_mut(&machine.addr()) {
        run.answers.clear();
        run.pending = ptr::null_mut();
        run.outcome = None;
    }
    r.returns.retain(|_, site| site.machine != machine);
}
unsafe fn failed_work(s: &State, machine: Oop, error: Oop) -> Result<Oop, Oop> {
    unsafe {
        let k = s
            .resume
            .borrow()
            .runs
            .get(&machine.addr())
            .and_then(|r| r.answers.last())
            .copied();
        fail_run(s, machine);
        match k {
            Some(k) if !k.is_null() => {
                s.transfer(b"answerTo:with:", &[k, abi::fault_reference(error)])
            }
            _ => Err(error),
        }
    }
}
unsafe fn answer_segment(s: &State, machine: Oop, value: Oop) -> Result<Oop, Oop> {
    unsafe {
        let k = s
            .resume
            .borrow_mut()
            .runs
            .get_mut(&machine.addr())
            .ok_or_else(invalid)?
            .answers
            .pop()
            .ok_or_else(invalid)?;
        s.transfer(b"answerTo:with:", &[k, value])
    }
}
unsafe fn next_step(s: &State, machine: Oop) -> Result<Oop, Oop> {
    s.resume
        .borrow()
        .runs
        .get(&machine.addr())
        .map(|r| r.advance)
        .ok_or_else(invalid)
}
fn take_return(s: &State, context: usize) -> Result<ReturnSite, Oop> {
    let site = s
        .resume
        .borrow()
        .returns
        .get(&context)
        .cloned()
        .ok_or_else(invalid)?;
    let capability = cap(s, site.machine)?;
    {
        let e = s.engine.borrow();
        let m = checked(e.machine(OWNER, capability))?;
        if m.lifecycle() != MachineLifecycle::Running
            || m.arena().current_handle() != Some(site.frame)
        {
            return Err(invalid());
        }
    }
    s.resume.borrow_mut().returns.remove(&context);
    Ok(site)
}
unsafe fn return_method(
    s: &State,
    machine: Oop,
    capability: MachineCapability,
    result_slot: usize,
    tail: bool,
) -> Result<Oop, Oop> {
    unsafe {
        let q = s.scope(&[machine]);
        let frame = checked(s.engine.borrow().machine(OWNER, capability))?
            .arena()
            .current_handle()
            .ok_or_else(invalid)?;
        let (vt, template) = {
            let r = s.resume.borrow();
            (r.context_vt, r.return_template)
        };
        let context = q.keep(allocate(s, vt, 0)?);
        let method = q.keep(method_context(s, template, context)?);
        let mut r = s.resume.borrow_mut();
        r.returns.insert(
            context.addr(),
            ReturnSite {
                machine,
                frame,
                result_slot,
                tail,
            },
        );
        r.return_methods.insert(method.addr(), context.addr());
        Ok(method)
    }
}
pub(super) unsafe extern "C" fn receive_call(
    _: Oop,
    c: Oop,
    _: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = runtime_state()?;
        let [_, value] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let site = take_return(s, c.addr())?;
        let q = s.scope(&[site.machine, *value]);
        let result = (|| {
            let value = q.keep(s.language_result(if abi::is_fault(*value) {
                Err(abi::object_reference(*value))
            } else {
                Ok(*value)
            })?);
            let cap = cap(s, site.machine)?;
            if site.tail {
                if let Some(value) = finish(s, cap, value)? {
                    return answer_segment(s, site.machine, value);
                }
            } else {
                checked(s.engine.borrow_mut().commit_current_call_result(
                    OWNER,
                    cap,
                    site.result_slot,
                    s.nil,
                    value,
                ))?;
            }
            next_step(s, site.machine)
        })();
        match result {
            Err(e) => failed_work(s, site.machine, e),
            ok => ok,
        }
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn receive_run(
    _: Oop,
    machine: Oop,
    _: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = runtime_state()?;
        let [_, value] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let mut runtime = s.resume.borrow_mut();
        let run = runtime.runs.get_mut(&machine.addr()).ok_or_else(invalid)?;
        if run.outcome.is_some() {
            return Err(invalid());
        }
        run.outcome = Some(*value);
        Ok(ptr::null_mut())
    })()
    .unwrap_or_else(abi::fault_reference)
}
// Used only when an explicit Machine drive is itself called from native code
// inside an existing Id drive. Id supplies the synchronous return boundary.
pub(super) unsafe extern "C" fn drive_bridge(
    _: Oop,
    machine: Oop,
    _: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = runtime_state()?;
        let [k] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let mut runtime = s.resume.borrow_mut();
        let run = runtime.runs.get_mut(&machine.addr()).ok_or_else(invalid)?;
        *run.answers.first_mut().ok_or_else(invalid)? = *k;
        Ok(run.pending)
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn advance(
    _: Oop,
    machine: Oop,
    _: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = runtime_state()?;
        if borrowed_slice(a, n).ok_or_else(invalid)?.len() != 1 {
            return Err(invalid());
        }
        let q = s.scope(&[machine]);
        let capability = cap(s, machine)?;
        let result = (|| {
            // This is a committed execution safe point. All pending Id work and
            // language values are reachable before policy may collect.
            s.send(s.gc, b"prepareAllocation", &[])?;
            let exhausted = s
                .resume
                .borrow()
                .runs
                .get(&machine.addr())
                .ok_or_else(invalid)?
                .budget
                == Some(0);
            if exhausted {
                let next = next_step(s, machine)?;
                if checked(s.engine.borrow().machine(OWNER, capability))?.control_eligible() {
                    checked(
                        checked(s.engine.borrow_mut().machine_mut(OWNER, capability))?
                            .stop_runnable(OWNER),
                    )?;
                    let k = {
                        let mut r = s.resume.borrow_mut();
                        let run = r.runs.get_mut(&machine.addr()).unwrap();
                        run.pending = next;
                        *run.answers.first().ok_or_else(invalid)?
                    };
                    return s.transfer(b"answerTo:with:", &[k, machine]);
                }
                // A lookup helper may yield, but an unaware native caller may
                // not. Admission belongs to Id; no callee/lookup guessing.
                match s.transfer(b"pause:", &[next]) {
                    Ok(_) => return Ok(ptr::null_mut()),
                    Err(e) => {
                        if s.send(e, b"faultKind", &[])?
                            != s.names[b"continuationBarrier".as_slice()]
                        {
                            return Err(e);
                        }
                    }
                }
            }
            if let Some(left) = s
                .resume
                .borrow_mut()
                .runs
                .get_mut(&machine.addr())
                .ok_or_else(invalid)?
                .budget
                .as_mut()
            {
                *left = left.saturating_sub(1);
            }
            let (step, pc, count) = {
                let e = s.engine.borrow();
                let m = checked(e.machine(OWNER, capability))?;
                let f = m.arena().current_handle().ok_or_else(invalid)?;
                (
                    checked(m.arena().descriptor(f))?.step_method().to_method(),
                    checked(m.arena().program_counter(f))?,
                    checked(m.arena().descriptor(f))?.driver_argument_count(),
                )
            };
            q.keep(step);
            let args = match count {
                0 => vec![],
                1 => vec![q.keep(number(s, pc)?)],
                _ => return Err(invalid()),
            };
            checked(s.engine.borrow_mut().begin_managed_turn())?;
            s.depth.set(s.depth.get() + 1);
            let turn = Invocation(s);
            s.resume.borrow_mut().active.push(capability);
            let active = Active(s, capability);
            let action = q.keep(s.invoke(step, machine, &args)?);
            drop(active);
            drop(turn);
            refresh(s, action)?;
            let decoded = checked(s.engine.borrow().decode_action(
                &s.storage.borrow(),
                OWNER,
                capability,
                action,
            ))?;
            match decoded {
                DecodedAction::Continue => {
                    checked(
                        s.engine
                            .borrow_mut()
                            .commit_current_continue(OWNER, capability, s.nil),
                    )?;
                    next_step(s, machine)
                }
                DecodedAction::Return { value_slot } => {
                    let value = q.keep(checked(
                        s.engine
                            .borrow()
                            .read_current_oop(OWNER, capability, value_slot),
                    )?);
                    if let Some(value) = finish(s, capability, value)? {
                        answer_segment(s, machine, value)
                    } else {
                        next_step(s, machine)
                    }
                }
                DecodedAction::Call {
                    receiver_slot,
                    selector_slot,
                    argument_start,
                    argument_count,
                    ..
                }
                | DecodedAction::TailCall {
                    receiver_slot,
                    selector_slot,
                    argument_start,
                    argument_count,
                } => {
                    let tail = matches!(decoded, DecodedAction::TailCall { .. });
                    let slot = if let DecodedAction::Call { result_slot, .. } = decoded {
                        result_slot
                    } else {
                        0
                    };
                    let operands = checked(s.engine.borrow().copy_send_operands(
                        OWNER,
                        capability,
                        receiver_slot,
                        selector_slot,
                        argument_start,
                        argument_count,
                    ))?;
                    for &v in &operands.arguments {
                        q.keep(v);
                    }
                    q.keep(operands.receiver);
                    q.keep(operands.selector);
                    let k = q.keep(return_method(s, machine, capability, slot, tail)?);
                    let mut values = vec![operands.receiver, operands.selector, k];
                    values.extend_from_slice(&operands.arguments);
                    s.transfer(b"prepareSend", &values)
                }
                _ => Err(invalid()),
            }
        })();
        match result {
            Err(e) => failed_work(s, machine, e),
            ok => ok,
        }
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe fn drive(
    s: &State,
    machine: Oop,
    capability: MachineCapability,
    budget: Option<usize>,
) -> Result<Oop, Oop> {
    unsafe {
        let q = s.scope(&[machine]);
        checked(s.engine.borrow_mut().begin_managed_turn())?;
        s.depth.set(s.depth.get() + 1);
        let _turn = Invocation(s);
        s.resume.borrow_mut().active.push(capability);
        let _active = Active(s, capability);
        let (work, sink) = {
            let mut r = s.resume.borrow_mut();
            let run = r.runs.get_mut(&machine.addr()).ok_or_else(invalid)?;
            run.budget = budget;
            run.outcome = None;
            *run.answers.first_mut().ok_or_else(invalid)? = run.sink;
            (run.pending, run.sink)
        };
        q.keep(work);
        q.keep(sink);
        let result = (|| match s.transfer(
            b"drive:budget:",
            &[work, abi::size(usize::MAX >> 1).unwrap()],
        ) {
            Ok(pending) if !pending.is_null() => {
                let depth = checked(
                    checked(s.engine.borrow_mut().machine_mut(OWNER, capability))?
                        .park_id_work(OWNER),
                )?;
                let mut runtime = s.resume.borrow_mut();
                let run = runtime.runs.get_mut(&machine.addr()).unwrap();
                run.pending = pending;
                run.parked_depth = Some(depth);
                Ok(machine)
            }
            Ok(_) => {
                let value = s
                    .resume
                    .borrow_mut()
                    .runs
                    .get_mut(&machine.addr())
                    .ok_or_else(invalid)?
                    .outcome
                    .take()
                    .ok_or_else(invalid)?;
                if abi::is_fault(value) {
                    Err(abi::object_reference(value))
                } else {
                    Ok(value)
                }
            }
            Err(error) => {
                let kind = s.send(error, b"faultKind", &[])?;
                if kind != s.names[b"driveActive".as_slice()] {
                    return Err(error);
                }
                let template = s.resume.borrow().bridge_template;
                let bridge = q.keep(method_context(s, template, machine)?);
                s.invoke(bridge, machine, &[])
            }
        })();
        if result.is_err() {
            fail_run(s, machine);
        } else if let Some(run) = s.resume.borrow_mut().runs.get_mut(&machine.addr()) {
            if let Some(k) = run.answers.first_mut() {
                *k = run.sink;
            }
        }
        result
    }
}
pub(super) unsafe extern "C" fn entry(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = runtime_state()?;
        let [k, args @ ..] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let record = s
            .resume
            .borrow()
            .records
            .get(&c.addr())
            .cloned()
            .ok_or_else(invalid)?;
        let q = s.scope(&[c, r, *k]);
        for &arg in args {
            q.keep(arg);
        }
        let known = s.resume.borrow().return_methods.get(&k.addr()).copied();
        if let Some(context) = known {
            let site = s
                .resume
                .borrow()
                .returns
                .get(&context)
                .cloned()
                .ok_or_else(invalid)?;
            let capability = cap(s, site.machine)?;
            q.keep(site.machine);
            let (acquired, value) = acquire(s, &record, r, args)?;
            let site = match take_return(s, context) {
                Ok(site) => site,
                Err(e) => {
                    checked(s.engine.borrow_mut().release_acquired_entry(acquired))?;
                    return Err(e);
                }
            };
            push(
                s,
                capability,
                acquired,
                value,
                if site.tail {
                    None
                } else {
                    Some(ReturnDestination {
                        result_slot: site.result_slot,
                    })
                },
                site.tail,
            )?;
            return next_step(s, site.machine);
        }
        let inherited = s.resume.borrow().active.last().copied();
        let machine = q.keep(if let Some(cap) = inherited {
            checked(s.engine.borrow().machine_object(OWNER, cap))?
        } else {
            new_machine(s)?
        });
        let capability = cap(s, machine)?;
        let (acquired, value) = acquire(s, &record, r, args)?;
        let begun = {
            let mut e = s.engine.borrow_mut();
            let m = checked(e.machine_mut(OWNER, capability))?;
            checked(if inherited.is_some() {
                m.begin_nested_drive(OWNER)
            } else {
                m.begin_initial_drive(OWNER)
            })
        };
        if let Err(error) = begun {
            checked(s.engine.borrow_mut().release_acquired_entry(acquired))?;
            return Err(error);
        }
        if let Err(error) = ensure_run(s, machine, *k) {
            checked(s.engine.borrow_mut().release_acquired_entry(acquired))?;
            fail_run(s, machine);
            return Err(error);
        }
        push(s, capability, acquired, value, None, false)?;
        next_step(s, machine)
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
        ensure_run(s, machine, ptr::null_mut())?;
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
        let parked = s
            .resume
            .borrow_mut()
            .runs
            .get_mut(&r.addr())
            .ok_or_else(invalid)?
            .parked_depth
            .take();
        if let Some(depth) = parked {
            checked(
                checked(s.engine.borrow_mut().machine_mut(OWNER, cap))?
                    .resume_id_work(OWNER, depth),
            )?;
        } else {
            checked(
                checked(s.engine.borrow_mut().machine_mut(OWNER, cap))?.resume_runnable(OWNER),
            )?;
        }
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
        s.resume.borrow_mut().runs.remove(&r.addr());
        s.resume
            .borrow_mut()
            .returns
            .retain(|_, site| site.machine != r);
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
    if let Some(site) = s.resume.borrow().returns.get(&object.addr()) {
        return Ok(Some(vec![site.machine]));
    }
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
            if let Some(run) = s.resume.borrow().runs.get(&object.addr()) {
                values.extend([run.advance, run.sink, run.pending]);
                values.extend(&run.answers);
                values.extend(run.outcome);
            }
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
        {
            let mut r = s.resume.borrow_mut();
            r.runs.remove(&value.addr());
            r.returns.remove(&value.addr());
            r.return_methods
                .retain(|method, context| *method != value.addr() && *context != value.addr());
        }
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
        }
        let execution_type = q.keep(s.send(s.id.root, b"behaviorOf:", &[s.receiver])?);
        for &name in METHOD_NAMES {
            if matches!(
                name,
                b"resumableEntry"
                    | b"runBlockStep"
                    | b"advanceExecution"
                    | b"receiveCall"
                    | b"receiveRun"
                    | b"driveBridge"
            ) {
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
        for (name, slot) in [
            (b"advanceExecution".as_slice(), 0),
            (b"receiveCall", 1),
            (b"receiveRun", 2),
            (b"driveBridge", 3),
        ] {
            let method = q.keep(s.send(image, b"methodAt:", &[s.names[name]])?);
            let mut r = s.resume.borrow_mut();
            r.roots.push(method);
            match slot {
                0 => r.advance_template = method,
                1 => r.return_template = method,
                2 => r.sink_template = method,
                _ => r.bridge_template = method,
            };
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
