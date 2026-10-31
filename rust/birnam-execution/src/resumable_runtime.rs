//! Oop message connection to the retained registration and Machine algorithms.
use super::*;
#[path = "oop_call_preparation.rs"]
pub(super) mod call_preparation;
#[path = "control.rs"]
pub(super) mod control;
#[path = "frame_view.rs"]
pub(super) mod frame_view;
#[path = "oop_recovery.rs"]
pub(super) mod recovery;
#[path = "oop_native_tier.rs"]
pub(super) mod native_tier;
use crate::{entry::EntryKey, execution::*};
use std::{
    collections::HashSet,
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
    inline: Option<recovery::Inline>,
    candidate: Option<std::rc::Rc<native_tier::Candidate>>,
    // The lexical environment is instance state; the code registration is shared.
    environment: Option<Oop>,
    code_object: Option<Oop>,
}
#[derive(Default)]
pub(super) struct Runtime {
    pub roots: Vec<Oop>,
    runs: WordIndex<Run>,
    returns: WordIndex<ReturnSite>,
    pub(super) return_methods: WordIndex<usize>,
    advance_template: Oop,
    return_template: Oop,
    sink_template: Oop,
    bridge_template: Oop,
    records: WordIndex<Registered>,
    // Derived weak Method -> context association, including known captures.
    by_method: WordIndex<usize>,
    context_vt: Oop,
    template: Oop,
    dnu_template: Oop,
    run_descriptor: Option<Arc<FrameDescriptor>>,
    active: Vec<MachineCapability>,
    pending: HashSet<u64>,
    recovery: recovery::Runtime,
    controls: Vec<(&'static [u8], Oop)>,
}
impl Runtime {
    fn insert_record(&mut self, context: usize, record: Registered) {
        self.by_method.insert(record.method.addr(), context);
        self.records.insert(context, record);
    }
}
pub(super) fn knows(s: &State, method: Oop) -> bool {
    s.resume.borrow().by_method.contains_key(&method.addr())
}
pub(super) fn note_capture(s: &State, original: Oop, captured: Oop) {
    let mut runtime = s.resume.borrow_mut();
    if let Some(&context) = runtime.by_method.get(&original.addr()) {
        runtime.by_method.insert(captured.addr(), context);
    }
}
pub(super) const METHOD_NAMES: &[&[u8]] = &[
    b"methodFromBlock:",
    b"fromStep:descriptor:selection:owner:",
    b"fromStep:descriptor:selection:owner:callPlans:",
    b"fromStep:descriptor:selection:owner:callPlans:codeObject:",
    b"prepareCall:selector:",
    b"callPlan:field:",
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
    b"suspendAt:continueAt:",
    b"captureAt:continueAt:",
    b"resumeWith:",
    b"resumeWith:budget:",
    b"restoreContinuation:with:",
    b"exitRunnableBoundary:",
    b"boundaryCallSite:",
    b"exitBoundaryAt:",
    b"boundaryDeliveryPending",
    b"acknowledgeBoundaryDelivery",
    b"currentFrameView",
    b"recoverAt:with:",
    b"recoverAt:withGraph:",
    b"withInline:at:owner:",
    b"optimizeWith:",
];
fn checked<T, E>(value: Result<T, E>) -> Result<T, Oop> {
    value.map_err(|_| invalid())
}
unsafe fn controls_current(s: &State, machine: Oop, names: &[&[u8]]) -> Result<bool, Oop> {
    unsafe {
        let mut bindings = Vec::with_capacity(names.len() * 3);
        {
            let runtime = s.resume.borrow();
            for &name in names {
                let method = runtime.controls.iter().find(|(n, _)| *n == name)
                    .map(|(_, method)| *method).ok_or_else(invalid)?;
                bindings.extend([machine, s.names[name], method]);
            }
        }
        Ok(s.send(s.id.root, b"nativeBindingsMatch:", &bindings)? == abi::size(1).unwrap())
    }
}

pub(super) unsafe fn number(s: &State, value: usize) -> Result<Oop, Oop> {
    unsafe {
        s.send(
            s.named(b"Integer")?,
            b"fromWord:",
            &[abi::size(value).ok_or_else(invalid)?],
        )
    }
}
pub(super) unsafe fn index(s: &State, value: Oop) -> Result<usize, Oop> {
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
pub(super) unsafe fn bytes(s: &State, value: Oop, limit: usize) -> Result<Vec<u8>, Oop> {
    unsafe {
        let q = s.scope(&[value]);
        let buffer = q.keep(s.send(value, b"bytes", &[])?);
        let count = index(s, s.send(buffer, b"sizeWord", &[])?)?;
        if count > limit { return Err(invalid()); }
        s.byte_reader.read_count(buffer, count)
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
    unsafe { register_step(c, a, n, false, false) }
}
pub(super) unsafe extern "C" fn from_step_plans(
    _: Oop,
    c: Oop,
    _: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    unsafe { register_step(c, a, n, true, false) }
}
pub(super) unsafe extern "C" fn from_step_code(
    _: Oop,
    c: Oop,
    _: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    unsafe { register_step(c, a, n, true, true) }
}
unsafe fn register_step(c: Oop, a: *const Oop, n: usize, plans: bool, coded: bool) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let args = borrowed_slice(a, n).ok_or_else(invalid)?;
        if args.len() != 4 + usize::from(plans) + usize::from(coded) {
            return Err(invalid());
        }
        let [step, description, selection, owner] = &args[..4] else {
            unreachable!()
        };
        let _q = s.scope(args);
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
        let call_plans = if plans {
            Some(bytes(s, args[4], 16 << 20)?)
        } else {
            None
        };
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
        let mut descriptor = checked(FrameDescriptor::admit_v2(
            &description,
            authority,
            generation,
            key(s, *step)?,
        ))?;
        if let Some(plans) = call_plans {
            Arc::get_mut(&mut descriptor)
                .ok_or_else(invalid)?
                .admit_call_plans(&plans, &description, authority)
                .map_err(|_| invalid())?;
        }
        let code = if coded {
            let code = args[5];
            let factory = s.named(b"CodeObject")?;
            let arity = index(s, s.send(factory, b"parameterCountOf:", &[code])?)?;
            let captures = index(s, s.send(factory, b"captureCountOf:", &[code])?)?;
            if arity
                != descriptor
                    .v2_entry_binding()
                    .ok_or_else(invalid)?
                    .argument_count
                || captures != descriptor.entry_capture_slots().len()
            {
                return Err(invalid());
            }
            Some(code)
        } else {
            None
        };
        register_descriptor(s, descriptor, *step, *owner, code)
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe fn register_descriptor(
    s: &State,
    descriptor: Arc<FrameDescriptor>,
    step: Oop,
    owner: Oop,
    code_object: Option<Oop>,
) -> Result<Oop, Oop> {
    unsafe {
        let q = s.scope(&[step, owner]);
        if let Some(code) = code_object {
            q.keep(code);
        }
        let binding = descriptor.v2_entry_binding().ok_or_else(invalid)?;
        let (context_vt, template) = {
            let r = s.resume.borrow();
            (r.context_vt, r.template)
        };
        let context = q.keep(allocate(s, context_vt, 0)?);
        let method = q.keep(s.send(template, b"withContext:", &[context])?);
        let method_key = key(s, method)?;
        // The existing registration context retains Method, step and owner and
        // gives the driver direct access to this owner's optional candidate.
        descriptor.executable_owner.set(context);
        let mut semantic = SemanticRegistrationV1::new(
            method_key,
            descriptor.clone(),
            binding.argument_count,
            binding.receiver_slot,
            binding.argument_start,
        );
        if binding.implementation_slot.is_some() {
            // The existing constructor uses its owner as the adapter's initial
            // implementation. Execution alone interprets the descriptor slot.
            semantic = semantic.with_implementation_anchor(owner);
        } else {
            semantic.receiver_capture_offset_words = 1;
        }
        let mut engine = s.engine.borrow_mut();
        let capability = checked(engine.register_resumable(OWNER, semantic))?;
        let lease = checked(engine.publish_owned(capability))?;
        drop(engine);
        if code_object.is_some() {
            s.admitted
                .borrow_mut()
                .insert(method.addr(), CodeShape { parameters: binding.argument_count, captures: descriptor.entry_capture_slots().len() });
        }
        let mut runtime = s.resume.borrow_mut();
        runtime.insert_record(
            context.addr(),
            Registered {
                method,
                step,
                owner,
                descriptor,
                key: method_key,
                capability,
                lease,
                inline: None,
                candidate: None,
                environment: None,
                code_object,
            },
        );
        Ok(method)
    }
}
// Executable identity is supplied by Id; these are this owner's existing code
// descriptions, not a second metadata database for arbitrary Methods.
unsafe fn owned_record(s: &State, method: Oop, q: &Scope<'_>) -> Result<Option<Registered>, Oop> {
    unsafe {
        let known = {
            let runtime = s.resume.borrow();
            runtime.by_method.get(&method.addr()).and_then(|key| runtime.records.get(key)).cloned()
        };
        if let Some(record) = known { return Ok(Some(record)); }
        let records: Vec<_> = s.resume.borrow().records.values().cloned().collect();
        for record in &records {
            q.keep(record.method);
        }
        for record in records {
            if s.send(s.id.root, b"sameNativeMethod:as:", &[method, record.method])?
                == abi::size(1).unwrap()
            {
                return Ok(Some(record));
            }
        }
        Ok(None)
    }
}
pub(super) unsafe fn body(s: &State, method: Oop, q: &Scope<'_>) -> Result<Option<Oop>, Oop> {
    unsafe { Ok(owned_record(s, method, q)?.and_then(|r| r.environment)) }
}
pub(super) unsafe fn source_code(
    s: &State,
    method: Oop,
    q: &Scope<'_>,
) -> Result<Option<Oop>, Oop> {
    unsafe { Ok(owned_record(s, method, q)?.map(|r| r.code_object.unwrap_or(s.nil))) }
}
// Materialization may bind an equal portable description to a fresh executable
// instance. It must never rewrite an existing Method's source association.
pub(super) unsafe fn with_source(
    s: &State,
    method: Oop,
    code: Oop,
    q: &Scope<'_>,
) -> Result<Option<Oop>, Oop> {
    unsafe {
        let Some(mut record) = owned_record(s, method, q)? else {
            return Ok(None);
        };
        if record.environment.is_some() {
            return Err(invalid());
        }
        let original = record.code_object.ok_or_else(invalid)?;
        if original == code {
            return Ok(Some(record.method));
        }
        let factory = q.keep(s.named(b"CodeObject")?);
        let before = q.keep(s.send(factory, b"canonicalBytesOf:", &[original])?);
        let after = q.keep(s.send(factory, b"canonicalBytesOf:", &[code])?);
        if bytes(s, before, 64 << 20)? != bytes(s, after, 64 << 20)? {
            return Err(invalid());
        }
        let (vt, template) = {
            let runtime = s.resume.borrow();
            (runtime.context_vt, runtime.template)
        };
        let context = q.keep(allocate(s, vt, 0)?);
        let copy = q.keep(s.send(template, b"withContext:", &[context])?);
        record.method = copy;
        record.code_object = Some(code);
        record.candidate = None;
        let arity = record
            .descriptor
            .v2_entry_binding()
            .ok_or_else(invalid)?
            .argument_count;
        let captures = record.descriptor.entry_capture_slots().len();
        s.resume.borrow_mut().insert_record(context.addr(), record);
        s.admitted.borrow_mut().insert(copy.addr(), CodeShape { parameters: arity, captures });
        Ok(Some(copy))
    }
}
pub(super) unsafe extern "C" fn method_from_block(
    _: Oop,
    c: Oop,
    _: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [body] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[*body]);
        if s.storage.borrow().image_shell(*body) {
            return Err(invalid());
        }
        let count = {
            let storage = s.storage.borrow();
            if *body == s.block || storage.family_of(*body) != Some(s.block_vt) {
                return Err(invalid());
            }
            storage
                .payload_size(*body)
                .and_then(|bytes| (bytes / size_of::<Oop>()).checked_sub(1))
                .ok_or_else(invalid)?
        };
        let code = q.keep(s.field(*body, 0)?);
        if !knows(s, code) {
            if let Some(method) = native_code::method_from_block(s, *body, &q)? { return Ok(method); }
            if let Some(method) = native_adapters::method_from_block(s, *body, &q)? { return Ok(method); }
        }
        let selected = owned_record(s, code, &q)?;
        let mut record = selected.ok_or_else(invalid)?;
        let binding = record.descriptor.v2_entry_binding().ok_or_else(invalid)?;
        if record.environment.is_some()
            || binding.argument_count == 0
            || binding.implementation_slot.is_some()
            || count != record.descriptor.entry_capture_slots().len()
        {
            return Err(invalid());
        }
        for i in 0..count {
            if s.field(*body, i + 1)?.is_null() {
                return Err(invalid());
            }
        }
        let (context_vt, template) = {
            let runtime = s.resume.borrow();
            (runtime.context_vt, runtime.template)
        };
        let context = q.keep(allocate(s, context_vt, 0)?);
        let method = q.keep(s.send(template, b"withContext:", &[context])?);
        record.method = method;
        record.environment = Some(*body);
        record.candidate = None;
        // Retain the original descriptor/code owner and registration. No
        // per-instance dictionary, code admission or second registration.
        s.resume.borrow_mut().insert_record(context.addr(), record);
        Ok(method)
    })()
    .unwrap_or_else(abi::fault_reference)
}

pub(super) unsafe fn for_dnu(s: &State, method: Oop, q: &Scope<'_>) -> Result<Option<Oop>, Oop> {
    unsafe {
        let Some(mut record) = owned_record(s, method, q)? else { return Ok(None); };
        if record.environment.is_none() { return Ok(Some(method)); }
        let (vt, template) = { let runtime = s.resume.borrow(); (runtime.context_vt, runtime.dnu_template) };
        let context = q.keep(allocate(s, vt, 0)?);
        let copy = q.keep(s.send(template, b"withContext:", &[context])?);
        record.method = copy;
        record.candidate = None;
        s.resume.borrow_mut().insert_record(context.addr(), record);
        Ok(Some(copy))
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
    let entry = acquire_entry(s, r)?;
    unsafe { materialize_entry(s, entry, r, receiver, args).map(|v| (entry, v)) }
}
fn acquire_entry(s: &State, r: &Registered) -> Result<ResumableEntryCapability, Oop> {
    let Some(entry) =
        checked(s.engine.borrow_mut().classify_resumable(r.key))?
    else {
        return Err(invalid());
    };
    Ok(entry)
}
unsafe fn materialize_entry(
    s: &State,
    entry: ResumableEntryCapability,
    r: &Registered,
    receiver: Oop,
    args: &[Oop],
) -> Result<MaterializedEntry, Oop> {
    unsafe {
        let q = s.scope(&[r.method, receiver]);
        for &arg in args {
            q.keep(arg);
        }
        let result = (|| {
            let (_, count) = checked(s.engine.borrow().acquired_entry_capture_layout(entry))?;
            if count != 0 && r.environment.is_none() {
                let storage = s.storage.borrow();
                if storage.family_of(receiver) != Some(s.block_vt)
                    || storage
                        .payload_size(receiver)
                        .is_none_or(|bytes| bytes / size_of::<Oop>() <= count)
                {
                    return Err(invalid());
                }
            }
            let mut captures = Vec::new();
            for i in 0..count {
                let value = if let Some(environment) = r.environment {
                    s.field(environment, i + 1)?
                } else {
                    s.send(receiver, b"captureWordAt:", &[abi::size(i).unwrap()])?
                };
                captures.push(q.keep(value));
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
        result
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
fn finish(s: &State, cap: MachineCapability, value: Oop) -> Result<Option<(Oop, Oop)>, Oop> {
    let mut e = s.engine.borrow_mut();
    let machine = checked(e.machine_object(OWNER, cap))?;
    let m = checked(e.machine_mut(OWNER, cap))?;
    let frame = m.arena().current_handle().ok_or_else(invalid)?;
    let destination = checked(m.arena().return_destination(frame))?;
    let frame_answer = checked(m.arena().frame_answer(frame))?;
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
        let answer = if let Some(answer) = frame_answer {
            if answer.completes_drive {
                checked(m.complete_barrier_drive(OWNER))?;
            }
            answer.continuation
        } else {
            checked(m.complete_initial_drive(OWNER))?;
            s.resume
                .borrow()
                .runs
                .get(&machine.addr())
                .ok_or_else(invalid)?
                .answer
        };
        Some((answer, value))
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
    result_slot: usize,
    tail: bool,
}
struct Run {
    advance: Oop,
    sink: Oop,
    answer: Oop,
    entry_handoff: bool,
    budget: Option<usize>,
    pending: Oop,
    outcome: Option<Oop>,
    parked_depth: Option<usize>,
    driving: bool,
}
unsafe fn method_context(s: &State, template: Oop, context: Oop) -> Result<Oop, Oop> {
    unsafe { s.send(template, b"withContext:", &[context]) }
}
unsafe fn ensure_run(s: &State, machine: Oop) -> Result<(), Oop> {
    unsafe {
        if s.resume.borrow().runs.contains_key(&machine.addr()) {
            return Ok(());
        }
        let q = s.scope(&[machine]);
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
                answer: sink,
                entry_handoff: false,
                budget: None,
                pending: advance,
                outcome: None,
                parked_depth: None,
                driving: false,
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
        run.answer = ptr::null_mut();
        run.entry_handoff = false;
        run.pending = ptr::null_mut();
        run.outcome = None;
    }
    r.returns.retain(|_, site| site.machine != machine);
}
unsafe fn failed_work(s: &State, machine: Oop, error: Oop) -> Result<Oop, Oop> {
    unsafe {
        let capability = cap(s, machine)?;
        let k = checked(s.engine.borrow().machine(OWNER, capability))?
            .arena()
            .failure_answer()
            .or_else(|| {
                s.resume
                    .borrow()
                    .runs
                    .get(&machine.addr())
                    .map(|run| run.answer)
            });
        fail_run(s, machine);
        match k {
            Some(k) if !k.is_null() => {
                s.transfer(b"answerTo:with:", &[k, abi::fault_reference(error)])
            }
            _ => Err(error),
        }
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
        let mut e = s.engine.borrow_mut();
        let m = checked(e.machine_mut(OWNER, capability))?;
        let frame = m.arena().current_handle().ok_or_else(invalid)?;
        if m.lifecycle() != MachineLifecycle::Running
            || checked(m.arena().pending_return(frame))?.addr() != context
        {
            return Err(invalid());
        }
        checked(m.arena_mut().set_pending_return(frame, ptr::null_mut()))?;
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
                result_slot,
                tail,
            },
        );
        r.return_methods.insert(method.addr(), context.addr());
        checked(
            checked(s.engine.borrow_mut().machine_mut(OWNER, capability))?
                .arena_mut()
                .set_pending_return(frame, context),
        )?;
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
                if let Some((answer, value)) = finish(s, cap, value)? {
                    return s.transfer(b"answerTo:with:", &[answer, value]);
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
        run.answer = *k;
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
                        run.answer
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
            let (step, pc, count, immediate, origin) = {
                let e = s.engine.borrow();
                let m = checked(e.machine(OWNER, capability))?;
                let f = m.arena().current_handle().ok_or_else(invalid)?;
                (
                    checked(m.arena().descriptor(f))?.step_method().to_method(),
                    checked(m.arena().program_counter(f))?,
                    checked(m.arena().descriptor(f))?.driver_argument_count(),
                    checked(m.arena().descriptor(f))?.immediate_program_counter,
                    checked(m.arena().descriptor(f))?.executable_owner.get(),
                )
            };
            q.keep(step);
            let args = match count {
                0 => vec![],
                1 => vec![if immediate {
                    abi::size(pc).ok_or_else(invalid)?
                } else {
                    q.keep(number(s, pc)?)
                }],
                _ => return Err(invalid()),
            };
            checked(s.engine.borrow_mut().begin_managed_turn())?;
            s.depth.set(s.depth.get() + 1);
            let turn = Invocation(s);
            s.resume.borrow_mut().active.push(capability);
            let active = Active(s, capability);
            let step = native_tier::select(s, machine, origin, pc, &q)?.unwrap_or(step);
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
                    if let Some((answer, value)) = finish(s, capability, value)? {
                        s.transfer(b"answerTo:with:", &[answer, value])
                    } else {
                        next_step(s, machine)
                    }
                }
                DecodedAction::Suspend {
                    result_slot,
                    delimiter,
                } => {
                    require_control_driver(s, machine)?;
                    let changed = s.engine.borrow_mut().apply_suspend(
                        OWNER,
                        capability,
                        result_slot,
                        delimiter,
                    );
                    control_checked(s, changed)?;
                    yield_control(s, machine, machine)
                }
                DecodedAction::Capture {
                    result_slot,
                    delimiter,
                } => {
                    require_control_driver(s, machine)?;
                    let vt = s
                        .engine
                        .borrow()
                        .objects()
                        .ok_or_else(invalid)?
                        .continuation_vtable;
                    let continuation = q.keep(allocate(s, vt, 3)?);
                    let captured = s.engine.borrow_mut().capture_continuation(
                        &mut s.storage.borrow_mut(),
                        OWNER,
                        capability,
                        delimiter,
                        result_slot,
                        continuation,
                    );
                    control_checked(s, captured)?;
                    yield_control(s, machine, continuation)
                }
                DecodedAction::Call {
                    receiver_slot,
                    selector_slot,
                    argument_start,
                    argument_count,
                    ..
                }
                | DecodedAction::BoundaryCall {
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
                    let slot = match decoded {
                        DecodedAction::Call { result_slot, .. }
                        | DecodedAction::BoundaryCall { result_slot, .. } => result_slot,
                        _ => 0,
                    };
                    if matches!(decoded, DecodedAction::BoundaryCall { .. }) {
                        let result = s
                            .engine
                            .borrow_mut()
                            .establish_boundary(OWNER, capability, slot);
                        control_checked(s, result)?;
                    }
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
                DecodedAction::ExitBoundary { value_slot } => {
                    let value = q.keep(checked(
                        s.engine
                            .borrow()
                            .read_current_oop(OWNER, capability, value_slot),
                    )?);
                    let result = s
                        .engine
                        .borrow_mut()
                        .exit_boundary(OWNER, capability, value, s.nil);
                    control_checked(s, result)?;
                    next_step(s, machine)
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
            run.driving = true;
            run.budget = budget;
            run.outcome = None;
            run.answer = run.sink;
            (run.pending, run.sink)
        };
        q.keep(work);
        q.keep(sink);
        let result = (|| {
            let mut work = work;
            let result = loop {
                let result = s.transfer(
                    b"drive:budget:",
                    &[work, abi::size(usize::MAX >> 1).unwrap()],
                );
                let mut runtime = s.resume.borrow_mut();
                let run = runtime.runs.get_mut(&machine.addr()).ok_or_else(invalid)?;
                if !std::mem::take(&mut run.entry_handoff) {
                    break result;
                }
                work = result?;
                if work.is_null() {
                    return Err(invalid());
                }
                run.pending = work;
            };
            match result {
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
            }
        })();
        if let Some(run) = s.resume.borrow_mut().runs.get_mut(&machine.addr()) {
            run.driving = false;
        }
        if result.is_err() {
            fail_run(s, machine);
        } else if let Some(run) = s.resume.borrow_mut().runs.get_mut(&machine.addr()) {
            run.answer = run.sink;
        }
        result
    }
}
pub(super) unsafe extern "C" fn entry(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { enter(c, r, a, n, false) }
}
pub(super) unsafe extern "C" fn dnu_entry(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { enter(c, r, a, n, true) }
}
unsafe fn enter(c: Oop, r: Oop, a: *const Oop, n: usize, prefix: bool) -> Oop {
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
        // A bound method's first formal is its actual receiver. Captures
        // come from its environment, independently of that receiver's storage.
        let body_receiver = record.environment.unwrap_or(r);
        let bound_arguments;
        let args = if record.environment.is_some() {
            let expected = record.descriptor.v2_entry_binding().ok_or_else(invalid)?
                .argument_count.checked_sub(1).ok_or_else(invalid)?;
            if args.len() < expected || (!prefix && args.len() != expected) {
                return s.transfer(b"answerTo:with:", &[*k, s.nil]);
            }
            bound_arguments = std::iter::once(r)
                .chain(args[..expected].iter().copied())
                .collect::<Vec<_>>();
            bound_arguments.as_slice()
        } else {
            args
        };
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
            let exact_arity = args.len()
                == record
                    .descriptor
                    .v2_entry_binding()
                    .ok_or_else(invalid)?
                    .argument_count;
            let acquired = acquire_entry(s, &record)?;
            // Ordinary Method entry uses the same argument binding as direct
            // invocation. Only the optional candidate needs the exact shape.
            if !site.tail && exact_arity {
                if let Some(inline) = record.inline {
                    if recovery::try_inline(
                        s,
                        &site,
                        capability,
                        acquired,
                        inline,
                        body_receiver,
                        args,
                    )? {
                        // Recovery may have installed the child frame already.
                        // Its authority validated the original pending call.
                        s.resume.borrow_mut().returns.remove(&context);
                        return next_step(s, site.machine);
                    }
                }
            }
            let value = materialize_entry(s, acquired, &record, body_receiver, args)?;
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
        let (acquired, value) = acquire(s, &record, body_receiver, args)?;
        // Establish a real transfer boundary instead of inferring one from
        // the answer Method's identity. An accepted pause hands control back to
        // our drive; a synchronous native caller makes Id reject that transfer.
        let handoff = if inherited.is_some() {
            match s.transfer(b"pause:", &[next_step(s, machine)?]) {
                Ok(_) => true,
                Err(error) => {
                    let kind = s.send(error, b"faultKind", &[]);
                    if kind != Ok(s.names[b"continuationBarrier".as_slice()]) {
                        checked(s.engine.borrow_mut().release_acquired_entry(acquired))?;
                        return Err(kind.err().unwrap_or(error));
                    }
                    false
                }
            }
        } else {
            false
        };
        let begun = if handoff {
            Ok(())
        } else {
            let mut e = s.engine.borrow_mut();
            let m = checked(e.machine_mut(OWNER, capability))?;
            checked(if inherited.is_some() {
                m.begin_nested_drive(OWNER)
            } else {
                m.begin_initial_drive(OWNER)
            })
            .map(|_| ())
        };
        if let Err(error) = begun {
            checked(s.engine.borrow_mut().release_acquired_entry(acquired))?;
            return Err(error);
        }
        if let Err(error) = ensure_run(s, machine) {
            checked(s.engine.borrow_mut().release_acquired_entry(acquired))?;
            fail_run(s, machine);
            return Err(error);
        }
        push(s, capability, acquired, value, None, false)?;
        {
            let mut e = s.engine.borrow_mut();
            let m = checked(e.machine_mut(OWNER, capability))?;
            let frame = m.arena().current_handle().ok_or_else(invalid)?;
            checked(m.arena_mut().set_frame_answer(
                frame,
                FrameAnswer {
                    continuation: *k,
                    completes_drive: !handoff,
                },
            ))?;
        }
        if handoff {
            s.resume
                .borrow_mut()
                .runs
                .get_mut(&machine.addr())
                .ok_or_else(invalid)?
                .entry_handoff = true;
            Ok(ptr::null_mut())
        } else {
            next_step(s, machine)
        }
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
            0 => s.send(machine, b"callSite:", &[abi::size(0).unwrap()]),
            1 => s.send(machine, b"returnAt:", &[abi::size(3).unwrap()]),
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
        ensure_run(s, machine)?;
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
// Native errors retain the engine's existing failure kinds as ordinary Fault
// objects. Release engine/storage borrows before invoking the Fault owner.
unsafe fn control_checked<T, E: Into<ExecutionError>>(
    s: &State,
    result: Result<T, E>,
) -> Result<T, Oop> {
    unsafe {
        result.map_err(|error| {
            let kind = error.into().failure_kind().symbol().as_bytes();
            s.send(
                s.id.root,
                b"fault:detail:",
                &[s.names[kind], ptr::null_mut()],
            )
            .err()
            .unwrap_or_else(invalid)
        })
    }
}
unsafe fn require_control_driver(s: &State, machine: Oop) -> Result<(), Oop> {
    unsafe {
        if s.resume
            .borrow()
            .runs
            .get(&machine.addr())
            .is_some_and(|run| run.driving)
        {
            Ok(())
        } else {
            control_checked(s, Err::<(), _>(ExecutionError::ContinuationBarrier))
        }
    }
}
unsafe fn yield_control(s: &State, machine: Oop, value: Oop) -> Result<Oop, Oop> {
    unsafe {
        let k = {
            let mut runtime = s.resume.borrow_mut();
            let run = runtime.runs.get_mut(&machine.addr()).ok_or_else(invalid)?;
            run.pending = run.advance;
            run.parked_depth = None;
            run.answer
        };
        s.transfer(b"answerTo:with:", &[k, value])
    }
}
unsafe fn publish_control(c: Oop, machine: Oop, a: *const Oop, n: usize, capture: bool) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [slot, pc] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let _q = s.scope(&[machine, *slot, *pc]);
        let slot = index(s, *slot)?;
        let pc = index(s, *pc)?;
        require_control_driver(s, machine)?;
        let capability = cap(s, machine)?;
        let delimiter = {
            let result = s.engine.borrow().current_delimiter(OWNER, capability);
            control_checked(s, result)?
        };
        let result = {
            let mut e = s.engine.borrow_mut();
            let mut storage = s.storage.borrow_mut();
            if capture {
                e.publish_capture(&mut storage, OWNER, capability, delimiter, slot, pc)
            } else {
                e.publish_suspend(&mut storage, OWNER, capability, slot, delimiter, pc)
            }
        };
        control_checked(s, result)
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn suspend_at(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { publish_control(c, r, a, n, false) }
}
pub(super) unsafe extern "C" fn capture_at(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { publish_control(c, r, a, n, true) }
}
unsafe fn resume_control(c: Oop, machine: Oop, a: *const Oop, n: usize, bounded: bool) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let args = borrowed_slice(a, n).ok_or_else(invalid)?;
        if n != 1 + usize::from(bounded) {
            return Err(invalid());
        }
        let q = s.scope(args);
        q.keep(machine);
        let budget = if bounded {
            Some(index(s, args[1])?)
        } else {
            None
        };
        s.send(s.gc, b"prepareAllocation", &[])?;
        let capability = cap(s, machine)?;
        let result = s
            .engine
            .borrow_mut()
            .resume_suspended_machine(OWNER, capability, args[0], s.nil);
        control_checked(s, result)?;
        drive(s, machine, capability, budget)
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn resume(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { resume_control(c, r, a, n, false) }
}
pub(super) unsafe extern "C" fn resume_bounded(
    _: Oop,
    c: Oop,
    r: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    unsafe { resume_control(c, r, a, n, true) }
}
pub(super) unsafe extern "C" fn restore(
    _: Oop,
    c: Oop,
    machine: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [continuation, value] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let _q = s.scope(&[machine, *continuation, *value]);
        s.send(s.gc, b"prepareAllocation", &[])?;
        let capability = cap(s, machine)?;
        refresh(s, *continuation)?;
        let captured = {
            let result = s
                .engine
                .borrow()
                .continuation_capability_for_object(&s.storage.borrow(), *continuation);
            control_checked(s, result)?
        };
        let result = s
            .engine
            .borrow_mut()
            .restore_continuation(OWNER, capability, captured, *value, s.nil);
        control_checked(s, result)?;
        drive(s, machine, capability, None)
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
        let result = match kind {
            0 => e.publish_continue(&mut storage, OWNER, cap, i),
            1 => e.publish_call_site(&mut storage, OWNER, cap, i, CallSiteKind::Call),
            2 => e.publish_call_site(&mut storage, OWNER, cap, i, CallSiteKind::TailCall),
            3 => e.publish_return(&mut storage, OWNER, cap, i),
            4 => e.publish_boundary_call(&mut storage, OWNER, cap, i),
            _ => e.publish_boundary_exit(&mut storage, OWNER, cap, i),
        };
        drop(storage);
        drop(e);
        if kind >= 4 {
            control_checked(s, result)
        } else {
            checked(result)
        }
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
pub(super) unsafe extern "C" fn boundary_call(
    _: Oop,
    c: Oop,
    r: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    unsafe { action(c, r, a, n, 4) }
}
pub(super) unsafe extern "C" fn boundary_exit(
    _: Oop,
    c: Oop,
    r: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    unsafe { action(c, r, a, n, 5) }
}
pub(super) unsafe extern "C" fn exit_runnable(
    _: Oop,
    c: Oop,
    r: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [value] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let _q = s.scope(&[r, *value]);
        let capability = cap(s, r)?;
        let result = s
            .engine
            .borrow_mut()
            .exit_runnable_boundary(OWNER, capability, *value, s.nil);
        control_checked(s, result)?;
        Ok(r)
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn delivery_pending(
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
        let result = s
            .engine
            .borrow()
            .boundary_delivery_pending(OWNER, cap(s, r)?);
        let pending = control_checked(s, result)?;
        s.named(if pending { b"true" } else { b"false" })
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn acknowledge(
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
        let capability = cap(s, r)?;
        let result = s
            .engine
            .borrow_mut()
            .acknowledge_boundary_delivery(OWNER, capability);
        control_checked(s, result)?;
        Ok(r)
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) fn references(s: &State, object: Oop) -> Option<Vec<Oop>> {
    if let Some(site) = s.resume.borrow().returns.get(&object.addr()) {
        return Some(vec![site.machine]);
    }
    if let Some(record) = s.resume.borrow().records.get(&object.addr()) {
        let mut values = vec![record.method, record.step, record.owner,
            record.descriptor.executable_owner.get()];
        values.extend(record.environment);
        values.extend(record.code_object);
        if let Some(inline) = record.inline {
            values.extend([inline.method, inline.owner]);
        }
        if let Some(candidate) = &record.candidate {
            values.extend(&candidate.steps);
        }
        return Some(values);
    }
    let family = s.storage.borrow().family_of(object);
    let objects = s.engine.borrow().objects();
    if family == Some(s.block_vt) {
        return None;
    }
    if family.is_none() {
        return None;
    }
    let mut values = Vec::new();
    if let Some(o) = objects {
        if family == Some(o.machine_vtable) {
            if let Some(run) = s.resume.borrow().runs.get(&object.addr()) {
                values.extend([run.advance, run.sink, run.pending]);
                values.push(run.answer);
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
    if let Some(o) = objects {
        if family == Some(o.frame_view_vtable) {
            if let Some(word) = s.storage.borrow().read_payload_word(object, 0) {
                values.push(ptr::with_exposed_provenance_mut(word));
            }
        }
        if family == Some(o.continuation_vtable) {
            s.engine
                .borrow()
                .trace_continuation_roots(&s.storage.borrow(), object, |v| values.push(v));
        }
    }
    Some(values)
}
pub(super) fn retire(s: &State, values: &[Oop]) -> Result<(), Oop> {
    let Some(o) = s.engine.borrow().objects() else {
        return Ok(());
    };
    {
        let storage = s.storage.borrow();
        for &value in values {
            let family = storage.family_of(value);
            if family == Some(o.continuation_vtable)
                && storage.payload_size(value) == Some(3 * size_of::<Oop>())
            {
                unsafe { s.engine.borrow_mut().finalize_detached_continuation(value) }
            }
            if family == Some(o.machine_vtable)
                && storage.payload_size(value) == Some(3 * size_of::<Oop>())
            {
                unsafe { s.engine.borrow_mut().finalize_detached_machine(value) }
            }
        }
    }
    let dead: HashSet<_> = values.iter().map(|value| value.addr()).collect();
    let mut runtime = s.resume.borrow_mut();
    runtime.return_methods
        .retain(|method, context| !dead.contains(method) && !dead.contains(context));
    for value in values {
        runtime.by_method.remove(&value.addr());
        runtime.runs.remove(&value.addr());
        runtime.returns.remove(&value.addr());
        let record = runtime.records.remove(&value.addr());
        if let Some(record) = record {
            // Inline annotations retain the original context. Only that context
            // owns the registration lease, even if aliases retire in this batch.
            if record.descriptor.executable_owner.get() != *value { continue; }
            checked(
                s.engine
                    .borrow_mut()
                    .retire_owned(record.capability, record.lease),
            )?;
            runtime.pending.insert(record.descriptor.identity().generation);
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
        let dnu_template = q.keep(s.send(image, b"methodAt:", &[s.names[b"resumableDnuEntry".as_slice()]])?);
        let step = q.keep(s.send(image, b"methodAt:", &[s.names[b"runBlockStep".as_slice()]])?);
        let step = q.keep(s.send(step, b"withContext:", &[s.ns])?);
        let descriptor = control::descriptor(s, step, 1, true)?;
        {
            let mut runtime = s.resume.borrow_mut();
            runtime.context_vt = context_vt;
            runtime.template = template;
            runtime.dnu_template = dnu_template;
            runtime.run_descriptor = Some(descriptor);
            runtime.roots.extend([template, dnu_template, step]);
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
            let target = if name.starts_with(b"fromStep:descriptor:selection:owner:")
                || name == b"methodFromBlock:"
            {
                execution_type
            } else if matches!(name, b"withInline:at:owner:" | b"optimizeWith:") {
                q.keep(s.send(s.id.root, b"methodBehavior", &[])?)
            } else {
                machine_vtable
            };
            let method = q.keep(s.send(image, b"methodAt:", &[s.names[name]])?);
            let method = q.keep(s.send(method, b"withContext:", &[s.ns])?);
            if native_tier::CONTROLS.contains(&name) {
                let mut r = s.resume.borrow_mut();
                r.controls.push((name, method));
                r.roots.push(method);
            }
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
        control::install(s, image, machine_vtable)?;
        frame_view::install(s, image, frame_view_vtable)?;
        for (name, value) in [
            (b"Machine".as_slice(), machine_prototype),
            (b"MachineVt", machine_vtable),
            (b"FrameView", frame_view_prototype),
            (b"Continuation", continuation_prototype),
            (b"ContinuationVt", continuation_vtable),
        ] {
            s.send(s.ns, b"at:put:", &[s.names[name], value])?;
        }
        Ok(())
    }
}
