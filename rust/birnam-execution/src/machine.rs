//! Message integration for the retained frame, control and registration engine.
use super::*;
use crate::entry::EntryKey;
use crate::execution::*;
use crate::objects::Objects;
use std::{collections::HashSet, sync::Arc};
pub(super) const OWNER: u64 = 1; // Instance-local engine token; public entry checks the thread.

#[derive(Clone)]
pub(super) struct Registered {
    pub origin: Oop,
    pub descriptor: Arc<FrameDescriptor>,
    key: EntryKey,
    capability: ResumableEntryCapability,
    lease: GenerationLeaseCapability,
}
pub(super) struct State {
    pub(super) engine: ExecutionState,
    pub(super) storage: Objects,
    roots: Vec<Oop>,
    contexts: Vec<Box<Operation>>,
    active: Vec<MachineCapability>,
    pending: HashSet<u64>,
    block_behavior: Oop,
    block_codes: HashMap<usize, Oop>,
    image_shells: HashSet<usize>,
    pub(super) booleans: [Oop; 2],
}
impl State {
    pub fn new() -> Self {
        Self {
            engine: ExecutionState::new(OWNER),
            storage: Objects::default(),
            roots: Vec::new(),
            contexts: Vec::new(),
            active: Vec::new(),
            pending: HashSet::new(),
            booleans: [ptr::null_mut(); 2],
            block_behavior: ptr::null_mut(),
            block_codes: HashMap::new(),
            image_shells: HashSet::new(),
        }
    }
    pub fn image_pending(&self, value: Oop) -> bool {
        self.image_shells.contains(&value.addr())
    }
    pub fn roots(&self) -> Vec<Oop> {
        self.roots.clone()
    }
    pub fn references(&self, target: Oop) -> Option<Vec<Oop>> {
        let family = self.storage.family_of(target)?;
        let mut roots = Vec::new();
        if let Some(o) = self.engine.objects() {
            if family == o.machine_vtable {
                self.engine
                    .trace_object_roots(&self.storage, target, family, |v| roots.push(v));
            } else if family == o.continuation_vtable {
                self.engine
                    .trace_continuation_roots(&self.storage, target, |v| roots.push(v));
            } else if family == o.frame_view_vtable {
                if let Some(word) = self.storage.read_payload_word(target, 0) {
                    roots.push(ptr::with_exposed_provenance_mut(word));
                }
            } else if family == self.block_behavior {
                if let Some(code) = self.block_codes.get(&target.addr()) {
                    roots.push(*code);
                }
                for i in 0..self.storage.payload_size(target).unwrap() / size_of::<usize>() {
                    roots.push(ptr::with_exposed_provenance_mut(
                        self.storage.read_payload_word(target, i).unwrap(),
                    ));
                }
            }
        }
        Some(roots)
    }
    pub unsafe fn retire_objects(&mut self, dead: &[usize]) {
        if let Some(o) = self.engine.objects() {
            // Release frame/continuation pins before dead Methods release leases.
            for &address in dead {
                let object = ptr::with_exposed_provenance_mut(address);
                if self.storage.payload_size(object).unwrap_or(0) >= 3 * size_of::<usize>() {
                    if self.storage.family_of(object) == Some(o.machine_vtable) {
                        unsafe {
                            self.engine.finalize_detached_machine(object);
                        }
                    } else if self.storage.family_of(object) == Some(o.continuation_vtable) {
                        unsafe {
                            self.engine.finalize_detached_continuation(object);
                        }
                    }
                }
                self.block_codes.remove(&address);
                self.image_shells.remove(&address);
                self.storage.remove(object);
            }
        }
    }
    pub fn retire_method(&mut self, value: &Registered) -> Result<(), Outcome> {
        self.engine
            .retire_owned(value.capability, value.lease)
            .map_err(engine_failure)?;
        if value.descriptor.identity().generation != 0 {
            self.pending.insert(value.descriptor.identity().generation);
        }
        Ok(())
    }
    pub fn generation_pending(&self, generation: u64) -> bool {
        self.pending.contains(&generation)
    }
    pub fn checkpoint(&mut self) -> Result<(), Outcome> {
        for generation in self.pending.clone() {
            if self
                .engine
                .unregister_code_generation(generation)
                .map_err(engine_failure)?
            {
                self.pending.remove(&generation);
            }
        }
        Ok(())
    }
}
pub(super) fn engine_failure<E: Into<ExecutionError>>(error: E) -> Outcome {
    Outcome::failed(
        native::INVALID_REQUEST,
        100 + error.into().failure_kind() as usize,
    )
}
pub(super) fn checked<T, E: Into<ExecutionError>>(result: Result<T, E>) -> Result<T, Outcome> {
    result.map_err(engine_failure)
}
pub(super) fn cap(s: &Execution, object: Oop) -> Result<MachineCapability, Outcome> {
    let state = s.machines.borrow();
    checked(state.engine.capability_for_object(&state.storage, object))
}
pub(super) unsafe fn index(s: &Execution, value: Operand) -> Result<usize, Outcome> {
    let value = object(value)?;
    let n = unsafe { s.send(s.integer, b"wordOf:", &[Operand::object(value)]) }?
        .as_word()
        .ok_or_else(invalid)? as i64;
    usize::try_from(n).map_err(|_| invalid())
}
pub(super) unsafe fn integer(s: &Execution, value: usize) -> Result<Operand, Outcome> {
    unsafe { s.send(s.integer, b"fromWord:", &[Operand::word(value)]) }
}
unsafe fn allocate(s: &Execution, behavior: Oop, words: usize) -> Result<Oop, Outcome> {
    let _roots = s.scope(&[behavior]);
    let value = object(unsafe {
        s.send(
            behavior,
            b"allocate",
            &[Operand::word(
                words.checked_mul(size_of::<usize>()).ok_or_else(invalid)?,
            )],
        )
    }?)?;
    s.machines
        .borrow_mut()
        .storage
        .insert(value, behavior, words);
    Ok(value)
}
unsafe fn refresh(s: &Execution, target: Oop) -> Result<(), Outcome> {
    let _roots = s.scope(&[target]);
    let expected = s
        .machines
        .borrow()
        .storage
        .payload_size(target)
        .ok_or_else(invalid)?;
    let behavior =
        object(unsafe { s.send(s.id.receiver, b"behaviorOf:", &[Operand::object(target)]) }?)?;
    let bytes = unsafe {
        s.send(
            s.id.receiver,
            b"allocationSizeOf:",
            &[Operand::object(target)],
        )
    }?
    .as_word()
    .ok_or_else(invalid)?;
    if bytes != expected {
        return Err(invalid());
    }
    s.machines
        .borrow_mut()
        .storage
        .refresh_behavior(target, behavior);
    Ok(())
}
pub(super) unsafe fn sequence(
    s: &Execution,
    source: Oop,
    roots: &Scope<'_>,
) -> Result<Vec<Oop>, Outcome> {
    unsafe { sequence_bounded(s, source, roots, usize::MAX) }
}
pub(super) unsafe fn sequence_bounded(
    s: &Execution,
    source: Oop,
    roots: &Scope<'_>,
    limit: usize,
) -> Result<Vec<Oop>, Outcome> {
    roots.keep(source);
    let size = unsafe { s.send(source, b"size", &[]) }?;
    roots.keep(object(size)?);
    let size = unsafe { index(s, size) }?;
    if size > limit {
        return Err(invalid());
    }
    let mut values = Vec::new();
    for i in 0..size {
        let ix = unsafe { integer(s, i) }?;
        let local = s.scope(&[object(ix)?]);
        let v = object(unsafe { s.send(source, b"at:", &[ix]) }?)?;
        drop(local);
        roots.keep(v);
        values.push(v);
    }
    Ok(values)
}

unsafe fn new_machine(s: &Execution) -> Result<Oop, Outcome> {
    let roots = s.scope(&[]);
    let o = s.machines.borrow().engine.objects().ok_or_else(invalid)?;
    let mut values = Vec::new();
    for (behavior, words) in [
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
        let v = unsafe { allocate(s, behavior, words) }?;
        roots.keep(v);
        values.push(v);
    }
    let mut state = s.machines.borrow_mut();
    let State {
        engine, storage, ..
    } = &mut *state;
    let cap = checked(engine.create_machine_with_objects(
        OWNER, values[0], values[1], values[2], values[3], values[4], values[5], values[6],
        values[7], values[8],
    ))?;
    let m = checked(engine.machine_mut(OWNER, cap))?;
    m.boundary_call_action = values[9];
    m.exit_boundary_action = values[10];
    if !engine.initialize_machine_payload(storage, values[0], cap) {
        return Err(invalid());
    }
    Ok(values[0])
}

// Registration consumes existing canonical descriptor and selection bytes. Both
// are copied before sending to any dependency; no Rust carrier crosses a message.
unsafe fn from_step(s: &Execution, args: &[Operand]) -> Result<Operand, Outcome> {
    let [step, descriptor, selection, owner, ..] = args else {
        return Err(invalid());
    };
    if args.len() != 4 && args.len() != 5 {
        return Err(invalid());
    }
    let implementation = args.get(4).copied().map(object).transpose()?;
    let step = native_entry(*step)?;
    let owner = object(*owner)?;
    let copy = |value: Operand, limit: usize| -> Result<Vec<u8>, Outcome> {
        let b = value.as_bytes().ok_or_else(invalid)?;
        if b.length > limit {
            return Err(invalid());
        }
        Ok(unsafe { borrowed_bytes(b.pointer, b.length) }
            .ok_or_else(invalid)?
            .to_vec())
    };
    let descriptor = copy(
        *descriptor,
        birnam_artifact_codec::frame_descriptor::MAX_DESCRIPTOR_BYTES,
    )?;
    let selection = copy(
        *selection,
        birnam_artifact_codec::stackless_selection::MAX_CANONICAL_VALUE_BYTES,
    )?;
    let _roots = s.scope(&[owner]);
    if let Some(v) = implementation {
        _roots.keep(v);
    }
    let selected =
        birnam_artifact_codec::stackless_selection::inspect_artifact_selection(&selection)
            .map_err(|_| invalid())?;
    if selected.row_count() != 1 {
        return Err(invalid());
    }
    let row = selected
        .rows()
        .next()
        .ok_or_else(invalid)?
        .map_err(|_| invalid())?;
    let path: Vec<_> = row.path.iter().collect();
    let authority = birnam_artifact_codec::stackless_selection::SelectionSpec {
        source_revision: row.source_revision,
        root_kind: row.root_kind,
        root_index: row.root_index,
        path: &path,
        implementation_kind: row.implementation_kind,
        disposition: row.disposition,
    };
    let generation = unsafe { s.send(s.id.receiver, b"identityOf:", &[Operand::object(owner)]) }?
        .as_word()
        .filter(|v| *v != 0)
        .ok_or_else(invalid)? as u64;
    let descriptor = FrameDescriptor::admit_v2(&descriptor, authority, generation, step)
        .map_err(|_| invalid())?;
    if implementation.is_some()
        && descriptor
            .v2_entry_binding()
            .and_then(|b| b.implementation_slot)
            .is_none()
    {
        return Err(invalid());
    }
    let block = row.implementation_kind
        == birnam_artifact_codec::stackless_selection::ImplementationKind::BlockBody;
    unsafe { register_descriptor_with_captures(s, descriptor, owner, block, implementation) }
        .map(Operand::object)
}
pub(super) unsafe fn register_descriptor(
    s: &Execution,
    descriptor: Arc<FrameDescriptor>,
    owner: Oop,
) -> Result<Oop, Outcome> {
    unsafe { register_descriptor_with_captures(s, descriptor, owner, false, None) }
}
unsafe fn register_descriptor_with_captures(
    s: &Execution,
    descriptor: Arc<FrameDescriptor>,
    owner: Oop,
    block: bool,
    implementation: Option<Oop>,
) -> Result<Oop, Outcome> {
    unsafe {
        let roots = s.scope(&[owner]);
        let value = object(s.make(descriptor.step_method(), owner, None)?)?;
        roots.keep(value);
        s.methods
            .borrow_mut()
            .get_mut(&value.addr())
            .unwrap()
            .implementation = implementation;
        let key = EntryKey::from_method(s.entry(value, false)?);
        let binding = descriptor.v2_entry_binding().ok_or_else(invalid)?;
        descriptor.executable_owner.set(value);
        let mut carrier = SemanticRegistrationV1::new(
            key.to_method(),
            descriptor.clone(),
            binding.argument_count,
            binding.receiver_slot,
            binding.argument_start,
        );
        if binding.implementation_slot.is_some() {
            carrier = carrier.with_implementation_anchor(implementation.unwrap_or(owner));
        } else if block {
            carrier.receiver_capture_offset_words = 1;
        }
        let mut state = s.machines.borrow_mut();
        let capability = checked(state.engine.register_resumable(OWNER, carrier))?;
        let lease = checked(state.engine.publish_owned(capability))?;
        s.methods
            .borrow_mut()
            .get_mut(&value.addr())
            .unwrap()
            .resumable = Some(Registered {
            origin: value,
            descriptor,
            key,
            capability,
            lease,
        });
        Ok(value)
    }
}

unsafe fn new_block(s: &Execution, args: &[Operand], portable: bool) -> Result<Operand, Outcome> {
    let (method, code, captures) = match (portable, args) {
        (false, [method, captures]) => (*method, None, *captures),
        (true, [method, code, captures]) => (*method, Some(object(*code)?), *captures),
        _ => return Err(invalid()),
    };
    let method = object(method)?;
    let roots = s.scope(&[method, object(captures)?]);
    if let Some(code) = code {
        roots.keep(code);
    }
    let (mut arity, mut expected) = {
        let methods = s.methods.borrow();
        let m = methods.get(&method.addr()).ok_or_else(invalid)?;
        match &m.resumable {
            Some(r) => (
                r.descriptor.v2_entry_binding().unwrap().argument_count,
                Some(r.descriptor.entry_capture_slots().len()),
            ),
            None => (0, None),
        }
    };
    if let Some(code) = code {
        // Canonical meaning comes from its owner through a message, never through
        // replaceable inspection fields or foreign payload words.
        let factory = object(unsafe {
            s.id.send_named(
                s.namespace,
                b"at:",
                &[Operand::object(s.id.intern_name(b"CodeObject")?)],
            )
        }?)?;
        roots.keep(factory);
        let bytes = object(unsafe {
            s.id.send_named(factory, b"canonicalBytesOf:", &[Operand::object(code)])
        }?)?;
        roots.keep(bytes);
        let raw = unsafe { s.id.send_named(bytes, b"bytes", &[]) }?
            .as_bytes()
            .ok_or_else(invalid)?;
        let owned = unsafe { borrowed_bytes(raw.pointer, raw.length) }
            .ok_or_else(invalid)?
            .to_vec();
        let program =
            birnam_artifact_codec::portable_program::inspect(&owned).map_err(|_| invalid())?;
        if expected.is_some_and(|n| {
            n != program.capture_count as usize || arity != program.parameter_count as usize
        }) {
            return Err(invalid());
        }
        arity = program.parameter_count as usize;
        expected = Some(program.capture_count as usize);
    }
    let captures = unsafe { sequence(s, object(captures)?, &roots) }?;
    if expected.is_some_and(|n| n != captures.len()) {
        return Err(invalid());
    }
    let base = s.machines.borrow().block_behavior;
    let b = object(unsafe { s.send(base, b"delegated", &[]) }?)?;
    roots.keep(b);
    let selector = if arity == 0 {
        b"value".to_vec()
    } else {
        b"value:".repeat(arity)
    };
    let selector = unsafe { s.id.intern_name(&selector) }?;
    roots.keep(selector);
    unsafe {
        s.send(
            b,
            b"addMethod",
            &[
                Operand::object(selector),
                Operand::entry(s.entry(method, false)?),
            ],
        )
    }?;
    let block = unsafe { allocate(s, b, captures.len() + 1) }?;
    roots.keep(block);
    let mut state = s.machines.borrow_mut();
    state.storage.set_family(block, base);
    if let Some(code) = code {
        state.block_codes.insert(block.addr(), code);
    }
    state.storage.write_payload_word(block, 0, method.addr());
    for (i, capture) in captures.iter().enumerate() {
        state
            .storage
            .write_payload_word(block, i + 1, capture.addr());
    }
    Ok(Operand::object(block))
}

struct Active<'a>(&'a Execution);
impl Drop for Active<'_> {
    fn drop(&mut self) {
        self.0.machines.borrow_mut().active.pop();
    }
}
struct Turn<'a>(&'a Execution);
impl Drop for Turn<'_> {
    fn drop(&mut self) {
        self.0
            .machines
            .borrow_mut()
            .engine
            .end_managed_turn()
            .expect("balanced managed turn");
        self.0.depth.set(self.0.depth.get() - 1);
    }
}
fn active(s: &Execution, cap: MachineCapability) -> Active<'_> {
    s.machines.borrow_mut().active.push(cap);
    Active(s)
}
fn acquire(
    s: &Execution,
    r: &Registered,
    receiver: Oop,
    args: &[Oop],
) -> Result<(ResumableEntryCapability, MaterializedEntry), Outcome> {
    let mut state = s.machines.borrow_mut();
    if state.image_pending(receiver) {
        return Err(invalid());
    }
    let Some(StacklessTarget::Registered { entry }) =
        checked(state.engine.classify_resumable(r.key))?
    else {
        return Err(invalid());
    };
    let result = (|| {
        let (offset, count) = checked(state.engine.acquired_entry_capture_layout(entry))?;
        let mut captures = Vec::new();
        if offset + count > 0 {
            if state.storage.family_of(receiver) != Some(state.block_behavior)
                || state.storage.payload_size(receiver)
                    != Some((offset + count) * size_of::<usize>())
            {
                return Err(invalid());
            }
            for i in 0..count {
                captures.push(ptr::with_exposed_provenance_mut(
                    state
                        .storage
                        .read_payload_word(receiver, offset + i)
                        .ok_or_else(invalid)?,
                ));
            }
        }
        checked(
            state
                .engine
                .materialize_acquired_entry(entry, receiver, args, &captures, s.nil),
        )
    })();
    if result.is_err() {
        checked(state.engine.release_acquired_entry(entry))?;
    }
    result.map(|v| (entry, v))
}
fn push(
    s: &Execution,
    cap: MachineCapability,
    entry: ResumableEntryCapability,
    value: MaterializedEntry,
    destination: Option<ReturnDestination>,
    tail: bool,
) -> Result<(), Outcome> {
    let mut state = s.machines.borrow_mut();
    checked(state.engine.transfer_acquired_entry_to_frame(entry))?;
    let result = (|| {
        let m = checked(state.engine.machine_mut(OWNER, cap))?;
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
                checked(state.engine.release_entry_frame_pin(old))?;
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
        checked(state.engine.release_entry_frame_pin(entry))?;
    }
    result
}
fn finish(s: &Execution, cap: MachineCapability, value: Oop) -> Result<Option<Oop>, Outcome> {
    let mut state = s.machines.borrow_mut();
    let m = checked(state.engine.machine_mut(OWNER, cap))?;
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
        checked(state.engine.release_entry_frame_pin(entry))?;
    }
    Ok(answer)
}
pub(super) unsafe fn lookup_target(
    s: &Execution,
    receiver: Oop,
    selector: Oop,
) -> Result<Option<NativeEntry>, Outcome> {
    // Id owns fresh lookup, including lookup overrides. The returned executable
    // is consumed exactly once; dispatch never caches a behavior or binding.
    let value = unsafe {
        s.send(
            s.id.receiver,
            b"bind:to:",
            &[Operand::object(selector), Operand::object(receiver)],
        )
    }?;
    if value.kind() == native::UNIT {
        Ok(None)
    } else {
        native_entry(value).map(Some)
    }
}
pub(super) unsafe fn loop_drive(
    s: &Execution,
    machine: Oop,
    cap: MachineCapability,
    mut budget: Option<usize>,
) -> Result<Operand, Outcome> {
    let _roots = s.scope(&[machine]);
    checked(s.machines.borrow_mut().engine.begin_managed_turn())?;
    s.depth.set(s.depth.get() + 1);
    let _turn = Turn(s);
    let result = (|| unsafe {
        loop {
            if let Some(left) = budget.as_mut() {
                if *left == 0 {
                    let mut state = s.machines.borrow_mut();
                    let m = checked(state.engine.machine_mut(OWNER, cap))?;
                    if m.control_eligible() {
                        checked(m.stop_runnable(OWNER))?;
                        return Ok(Operand::object(machine));
                    }
                } else {
                    *left -= 1;
                }
            }
            let (step, pc, argc, origin) = {
                let state = s.machines.borrow();
                let m = checked(state.engine.machine(OWNER, cap))?;
                let frame = m.arena().current_handle().ok_or_else(invalid)?;
                let d = checked(m.arena().descriptor(frame))?;
                (
                    d.step_method(),
                    checked(m.arena().program_counter(frame))?,
                    d.driver_argument_count(),
                    d.executable_owner.get(),
                )
            };
            let turn_roots = s.scope(&[]);
            let arguments = if argc == 1 {
                let pc = integer(s, pc)?;
                turn_roots.keep(object(pc)?);
                vec![pc]
            } else {
                vec![]
            };
            let action = {
                let _active = active(s, cap);
                let step = crate::native_tier::select(s, machine, origin, pc, &turn_roots)?
                    .unwrap_or(step);
                object(
                    step.method.ok_or_else(invalid)?(
                        step.context,
                        machine,
                        arguments.as_ptr(),
                        arguments.len(),
                    )
                    .into_result()?,
                )?
            };
            turn_roots.keep(action);
            refresh(s, action)?;
            let decoded = {
                let state = s.machines.borrow();
                checked(
                    state
                        .engine
                        .decode_action(&state.storage, OWNER, cap, action),
                )?
            };
            match decoded {
                DecodedAction::Continue => checked(
                    s.machines
                        .borrow_mut()
                        .engine
                        .commit_current_continue(OWNER, cap, s.nil),
                )?,
                DecodedAction::Call {
                    receiver_slot,
                    selector_slot,
                    argument_start,
                    argument_count,
                    result_slot: _,
                }
                | DecodedAction::BoundaryCall {
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
                    let result_slot = match decoded {
                        DecodedAction::Call { result_slot, .. }
                        | DecodedAction::BoundaryCall { result_slot, .. } => result_slot,
                        _ => 0,
                    };
                    if matches!(decoded, DecodedAction::BoundaryCall { .. }) {
                        checked(s.machines.borrow_mut().engine.establish_boundary(
                            OWNER,
                            cap,
                            result_slot,
                        ))?;
                    }
                    let operands = checked(s.machines.borrow().engine.copy_send_operands(
                        OWNER,
                        cap,
                        receiver_slot,
                        selector_slot,
                        argument_start,
                        argument_count,
                    ))?;
                    let _active = active(s, cap);
                    let selected = lookup_target(s, operands.receiver, operands.selector);
                    let registered = selected
                        .as_ref()
                        .ok()
                        .and_then(|entry| entry.and_then(|entry| s.selected(entry)))
                        .and_then(|method| {
                            turn_roots.keep(method);
                            s.methods
                                .borrow()
                                .get(&method.addr())
                                .and_then(|m| m.resumable.clone().map(|r| (r, m.inline)))
                        });
                    if let Some((registered, inline)) = registered {
                        let (entry, value) =
                            acquire(s, &registered, operands.receiver, &operands.arguments)?;
                        if !tail && !matches!(decoded, DecodedAction::BoundaryCall { .. }) {
                            if let Some(inline) = inline.filter(|i| {
                                budget.is_none()
                                    || matches!(i.body, crate::recovery::Body::Native(_))
                            }) {
                                // The candidate may collect or mutate its receiver before
                                // declining. Keep the already materialized baseline inputs.
                                for &(_, value) in &value.initial_oops {
                                    turn_roots.keep(value);
                                }
                                if crate::recovery::try_inline(
                                    s,
                                    machine,
                                    cap,
                                    entry,
                                    result_slot,
                                    inline,
                                    operands.receiver,
                                    &operands.arguments,
                                )? {
                                    continue;
                                }
                            }
                        }
                        if tail {
                            let mut state = s.machines.borrow_mut();
                            let State {
                                engine, storage, ..
                            } = &mut *state;
                            match engine.try_restart_registered_tail(
                                storage,
                                OWNER,
                                cap,
                                entry,
                                operands.receiver,
                                &operands.arguments,
                                s.nil,
                            ) {
                                Ok(true) => continue,
                                Ok(false) => (),
                                Err(error) => {
                                    checked(engine.release_acquired_entry(entry))?;
                                    return Err(engine_failure(error));
                                }
                            }
                        }
                        push(
                            s,
                            cap,
                            entry,
                            value,
                            (!tail).then_some(ReturnDestination { result_slot }),
                            tail,
                        )?;
                    } else {
                        checked(
                            checked(s.machines.borrow_mut().engine.machine_mut(OWNER, cap))?
                                .enter_synchronous_boundary(OWNER),
                        )?;
                        let args: Vec<_> = operands
                            .arguments
                            .iter()
                            .map(|v| Operand::object(*v))
                            .collect();
                        let value = match selected {
                            Ok(Some(entry)) => {
                                crate::numeric::invoke(s, entry, operands.receiver, &args)
                            }
                            Ok(None) => {
                                let dnu = s.names[b"doesNotUnderstand:".as_slice()];
                                if operands.selector == dnu {
                                    Err(Outcome::failed(native::UNBOUND, 0))
                                } else {
                                    match lookup_target(s, operands.receiver, dnu) {
                                        Ok(Some(entry)) => {
                                            let mut forwarded =
                                                vec![Operand::object(operands.selector)];
                                            forwarded.extend(args.iter().take(16).copied());
                                            entry.method.unwrap()(
                                                entry.context,
                                                operands.receiver,
                                                forwarded.as_ptr(),
                                                forwarded.len(),
                                            )
                                            .into_result()
                                        }
                                        Ok(None) => Err(Outcome::failed(native::UNBOUND, 0)),
                                        Err(error) => Err(error),
                                    }
                                }
                            }
                            Err(error) => Err(error),
                        };
                        let value = s.language_result(value);
                        checked(
                            checked(s.machines.borrow_mut().engine.machine_mut(OWNER, cap))?
                                .leave_synchronous_boundary(OWNER),
                        )?;
                        let value = object(value?)?;
                        turn_roots.keep(value);
                        if tail {
                            if let Some(value) = finish(s, cap, value)? {
                                return Ok(Operand::object(value));
                            }
                        } else {
                            checked(s.machines.borrow_mut().engine.commit_current_call_result(
                                OWNER,
                                cap,
                                result_slot,
                                s.nil,
                                value,
                            ))?;
                        }
                    }
                }
                DecodedAction::Return { value_slot } => {
                    let value = checked(
                        s.machines
                            .borrow()
                            .engine
                            .read_current_oop(OWNER, cap, value_slot),
                    )?;
                    if let Some(value) = finish(s, cap, value)? {
                        return Ok(Operand::object(value));
                    }
                }
                DecodedAction::Suspend {
                    result_slot,
                    delimiter,
                } => {
                    checked(s.machines.borrow_mut().engine.apply_suspend(
                        OWNER,
                        cap,
                        result_slot,
                        delimiter,
                    ))?;
                    return Ok(Operand::object(machine));
                }
                DecodedAction::Capture {
                    result_slot,
                    delimiter,
                } => {
                    let vt = s
                        .machines
                        .borrow()
                        .engine
                        .objects()
                        .unwrap()
                        .continuation_vtable;
                    let continuation = allocate(s, vt, 3)?;
                    turn_roots.keep(continuation);
                    let mut state = s.machines.borrow_mut();
                    let State {
                        engine, storage, ..
                    } = &mut *state;
                    checked(engine.capture_continuation(
                        storage,
                        OWNER,
                        cap,
                        delimiter,
                        result_slot,
                        continuation,
                    ))?;
                    return Ok(Operand::object(continuation));
                }
                DecodedAction::Restore {
                    continuation_slot,
                    supplied_value_slot,
                } => {
                    let (continuation, supplied) = {
                        let state = s.machines.borrow();
                        (
                            checked(state.engine.read_current_oop(OWNER, cap, continuation_slot))?,
                            checked(state.engine.read_current_oop(
                                OWNER,
                                cap,
                                supplied_value_slot,
                            ))?,
                        )
                    };
                    refresh(s, continuation)?;
                    let mut state = s.machines.borrow_mut();
                    let continuation = checked(
                        state
                            .engine
                            .continuation_capability_for_object(&state.storage, continuation),
                    )?;
                    checked(state.engine.restore_continuation(
                        OWNER,
                        cap,
                        continuation,
                        supplied,
                        s.nil,
                    ))?;
                }
                DecodedAction::ExitBoundary { value_slot } => {
                    let mut state = s.machines.borrow_mut();
                    let value = checked(state.engine.read_current_oop(OWNER, cap, value_slot))?;
                    checked(state.engine.exit_boundary(OWNER, cap, value, s.nil))?;
                }
                DecodedAction::Abort { reason_slot, .. } => {
                    let mut state = s.machines.borrow_mut();
                    let value = checked(state.engine.read_current_oop(OWNER, cap, reason_slot))?;
                    checked(checked(state.engine.machine_mut(OWNER, cap))?.fail(OWNER))?;
                    return Ok(Operand::object(value));
                }
            }
        }
    })();
    if result.is_err() {
        if let Ok(m) = s.machines.borrow_mut().engine.machine_mut(OWNER, cap) {
            let _ = m.fail(OWNER);
        }
    }
    result
}
unsafe fn start(
    s: &Execution,
    machine: Oop,
    r: &Registered,
    receiver: Oop,
    args: &[Oop],
    nested: bool,
    budget: Option<usize>,
) -> Result<Operand, Outcome> {
    let cap = cap(s, machine)?;
    let (entry, value) = acquire(s, r, receiver, args)?;
    let begun = {
        let mut state = s.machines.borrow_mut();
        let m = checked(state.engine.machine_mut(OWNER, cap))?;
        checked(if nested {
            m.begin_nested_drive(OWNER)
        } else {
            m.begin_initial_drive(OWNER)
        })
    };
    if let Err(error) = begun {
        checked(s.machines.borrow_mut().engine.release_acquired_entry(entry))?;
        return Err(error);
    }
    push(s, cap, entry, value, None, false)?;
    unsafe { loop_drive(s, machine, cap, budget) }
}
pub(super) unsafe fn invoke_registered(
    s: &Execution,
    r: &Registered,
    receiver: Oop,
    args: &[Operand],
) -> Result<Operand, Outcome> {
    unsafe { s.send(s.gc, b"prepareAllocation", &[]) }?;
    let args: Vec<_> = args.iter().copied().map(object).collect::<Result<_, _>>()?;
    let inherited = s.machines.borrow().active.last().copied();
    let machine = if let Some(cap) = inherited {
        checked(s.machines.borrow().engine.machine_object(OWNER, cap))?
    } else {
        unsafe { new_machine(s) }?
    };
    let roots = s.scope(&[machine, r.origin, receiver]);
    for v in &args {
        roots.keep(*v);
    }
    unsafe { start(s, machine, r, receiver, &args, inherited.is_some(), None) }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Op {
    PreserveFailures,
    FromStep,
    WithInline,
    WithDiscardedClone,
    WithNumericPredicate,
    Optimize,
    Recover(bool),
    Block(bool),
    BlockCode,
    BlockSynchronous,
    RestoreCaptures,
    BlockShell,
    BlockCaptureCount,
    BlockCapture(bool),
    CloneBlock,
    Prepare,
    New,
    Disabled,
    Run(bool),
    Resume(bool),
    ContinueDrive,
    RestoreContinuation,
    Lifecycle,
    Runnable,
    Destroy,
    Read,
    Write,
    Continue,
    Call(bool),
    Return,
    Suspend(bool),
    Restore,
    Abort,
    BoundaryCall,
    ExitBoundary,
    ExitRunnableBoundary,
    Delivery,
    Acknowledge,
    View,
    ViewWord(usize),
    ViewRead,
    ViewKind,
    Diagnostic(usize),
    Kind,
    Word(usize),
}
struct Operation {
    execution: *const Execution,
    kind: Op,
}
unsafe fn boolean(s: &Execution, value: bool) -> Result<Operand, Outcome> {
    // Boolean identities are held in the module root set after construction.
    Ok(Operand::object(
        s.machines.borrow().booleans[usize::from(value)],
    ))
}
unsafe extern "C" fn operation(
    context: *mut c_void,
    receiver: Oop,
    arguments: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let op = &*context.cast::<Operation>();
        let s = state(op.execution.cast_mut().cast())?;
        let args = borrowed_slice(arguments, count).ok_or_else(invalid)?;
        let roots = s.scope(&[receiver]);
        for a in args {
            if let Some(v) = a.as_object() {
                roots.keep(v);
            }
        }
        // GC owns mutation-phase admission. No engine borrow crosses this send.
        s.send(s.gc, b"prepareAllocation", &[])?;
        match op.kind {
            Op::PreserveFailures => {
                let [block] = args else { return Err(invalid()); };
                struct Restore<'a>(&'a Cell<bool>, bool);
                impl Drop for Restore<'_> { fn drop(&mut self) { self.0.set(self.1); } }
                let _restore = Restore(&s.preserve_native_failures, s.preserve_native_failures.replace(true));
                return s.id.send_named(object(*block)?, b"value", &[]);
            }
            Op::FromStep => return from_step(s, args),
            Op::WithInline => return crate::recovery::with_inline(s, receiver, args),
            Op::WithDiscardedClone => return crate::discarded_clone::attach(s, receiver, args),
            Op::WithNumericPredicate => return crate::numeric_predicate::attach(s, receiver, args),
            Op::Optimize => return crate::native_tier::install(s, receiver, args),
            Op::Block(portable) => return new_block(s, args, portable),
            Op::BlockSynchronous if args.is_empty() => {
                let method = {
                    let state = s.machines.borrow();
                    if !state.block_codes.contains_key(&receiver.addr()) {
                        return Ok(Operand::object(s.nil));
                    }
                    state
                        .storage
                        .read_payload_word(receiver, 0)
                        .ok_or_else(invalid)?
                };
                let sync = s
                    .methods
                    .borrow()
                    .get(&method)
                    .ok_or_else(invalid)?
                    .resumable
                    .is_none();
                return boolean(s, sync);
            }
            Op::BlockShell => {
                let [method, code] = args else {
                    return Err(invalid());
                };
                let factory = object(s.id.send_named(
                    s.namespace,
                    b"at:",
                    &[Operand::object(s.id.intern_name(b"CodeObject")?)],
                )?)?;
                roots.keep(factory);
                let encoded = object(s.id.send_named(factory, b"canonicalBytesOf:", &[*code])?)?;
                roots.keep(encoded);
                let raw =
                    s.id.send_named(encoded, b"bytes", &[])?
                        .as_bytes()
                        .ok_or_else(invalid)?;
                let bytes = borrowed_bytes(raw.pointer, raw.length)
                    .ok_or_else(invalid)?
                    .to_vec();
                let count = birnam_artifact_codec::portable_program::inspect(&bytes)
                    .map_err(|_| invalid())?
                    .capture_count;
                let vector = object(s.id.send_named(
                    s.namespace,
                    b"at:",
                    &[Operand::object(s.id.intern_name(b"Vector")?)],
                )?)?;
                roots.keep(vector);
                let mut captures = object(s.id.send_named(vector, b"new", &[])?)?;
                roots.keep(captures);
                for _ in 0..count {
                    captures =
                        object(s.id.send_named(captures, b"add:", &[Operand::object(s.nil)])?)?;
                    roots.keep(captures);
                }
                let value = new_block(s, &[*method, *code, Operand::object(captures)], true)?;
                s.machines
                    .borrow_mut()
                    .image_shells
                    .insert(object(value)?.addr());
                return Ok(value);
            }
            Op::RestoreCaptures => {
                let [captures, target] = args else {
                    return Err(invalid());
                };
                let target = object(*target)?;
                let values = sequence(s, object(*captures)?, &roots)?;
                let mut state = s.machines.borrow_mut();
                if !state.image_shells.contains(&target.addr())
                    || state.storage.payload_size(target)
                        != Some((values.len() + 1) * size_of::<usize>())
                {
                    return Err(invalid());
                }
                for (i, value) in values.iter().enumerate() {
                    state
                        .storage
                        .write_payload_word(target, i + 1, value.addr());
                }
                state.image_shells.remove(&target.addr());
                return Ok(Operand::object(target));
            }
            Op::BlockCode | Op::BlockCaptureCount | Op::BlockCapture(_) => {
                let (target, index) = match (op.kind, args) {
                    (Op::BlockCode | Op::BlockCaptureCount, []) => (receiver, None),
                    (Op::BlockCapture(false), [i]) => (receiver, Some(*i)),
                    (Op::BlockCapture(true), [i, target]) => (object(*target)?, Some(*i)),
                    _ => return Ok(Operand::object(s.nil)),
                };
                let index = if let Some(i) = index {
                    let n = s.send(s.integer, b"wordOf:", &[i])?;
                    let Some(n) = n.as_word().filter(|n| *n <= isize::MAX as usize) else {
                        return Ok(Operand::object(s.nil));
                    };
                    Some(n)
                } else {
                    None
                };
                let value = {
                    let state = s.machines.borrow();
                    if state.storage.family_of(target) != Some(state.block_behavior)
                        || !state.block_codes.contains_key(&target.addr())
                    {
                        // These are PortableCode inspection messages. Ordinary
                        // native Blocks without a CodeObject remain uninspectable.
                        return Ok(Operand::object(s.nil));
                    }
                    let words = state.storage.payload_size(target).unwrap() / size_of::<usize>();
                    if words == 0 {
                        return Ok(Operand::object(s.nil));
                    }
                    match op.kind {
                        Op::BlockCode => {
                            return Ok(Operand::object(
                                state
                                    .block_codes
                                    .get(&target.addr())
                                    .copied()
                                    .unwrap_or(s.nil),
                            ));
                        }
                        Op::BlockCaptureCount => words - 1,
                        _ => {
                            let i = index.unwrap();
                            return Ok(Operand::object(if i < words - 1 {
                                ptr::with_exposed_provenance_mut(
                                    state.storage.read_payload_word(target, i + 1).unwrap(),
                                )
                            } else {
                                s.nil
                            }));
                        }
                    }
                };
                return integer(s, value);
            }
            Op::New if args.is_empty() => return new_machine(s).map(Operand::object),
            Op::Prepare if args.is_empty() => return Ok(Operand::object(receiver)),
            Op::Disabled if args.is_empty() => return Ok(Operand::object(s.nil)),
            _ => (),
        }
        refresh(s, receiver)?;
        match op.kind {
            Op::CloneBlock if args.is_empty() => {
                let (behavior, words) = {
                    let state = s.machines.borrow();
                    if state.image_pending(receiver)
                        || state.storage.family_of(receiver) != Some(state.block_behavior)
                    {
                        return Err(invalid());
                    }
                    let words = (0..state.storage.payload_size(receiver).unwrap()
                        / size_of::<usize>())
                        .map(|i| state.storage.read_payload_word(receiver, i).unwrap())
                        .collect::<Vec<_>>();
                    (state.storage.vtable_of(receiver).unwrap(), words)
                };
                let value = allocate(s, behavior, words.len())?;
                let mut state = s.machines.borrow_mut();
                let family = state.block_behavior;
                state.storage.set_family(value, family);
                if let Some(code) = state.block_codes.get(&receiver.addr()).copied() {
                    state.block_codes.insert(value.addr(), code);
                }
                for (i, word) in words.iter().enumerate() {
                    state.storage.write_payload_word(value, i, *word);
                }
                return Ok(Operand::object(value));
            }
            Op::Kind if args.is_empty() => {
                let state = s.machines.borrow();
                return state
                    .engine
                    .action_kind(&state.storage, receiver)
                    .map(Operand::object)
                    .ok_or_else(invalid);
            }
            Op::Word(i) if args.is_empty() => {
                let value = {
                    let state = s.machines.borrow();
                    state
                        .engine
                        .action_word(&state.storage, receiver, i)
                        .ok_or_else(invalid)?
                };
                return integer(s, value);
            }
            Op::ViewWord(field) if args.is_empty() => {
                let snapshot = {
                    let state = s.machines.borrow();
                    state
                        .engine
                        .frame_view_snapshot(&state.storage, OWNER, receiver)
                };
                let snapshot = match snapshot {
                    Ok(v) => v,
                    Err(e) => return view_error(s, e),
                };
                let value = match field {
                    0 => snapshot.identity.implementation as usize,
                    1 => snapshot.identity.generation as usize,
                    2 => snapshot.identity.source_map as usize,
                    3 => snapshot.program_counter,
                    _ => snapshot.slot_count,
                };
                return if field < 3 {
                    s.send(s.word, b"fromBits:", &[Operand::word(value)])
                } else {
                    integer(s, value)
                };
            }
            Op::ViewRead | Op::ViewKind if args.len() == 1 => {
                let i = match index(s, args[0]) {
                    Ok(v) => v,
                    Err(_) => return view_error(s, FrameViewError::InvalidFrameSlot),
                };
                let value = {
                    let state = s.machines.borrow();
                    if matches!(op.kind, Op::ViewRead) {
                        state
                            .engine
                            .frame_view_oop(&state.storage, OWNER, receiver, i)
                    } else {
                        state
                            .engine
                            .frame_view_slot_kind(&state.storage, OWNER, receiver, i)
                            .map(|_| state.engine.objects().unwrap().oop_slot_kind)
                    }
                };
                return match value {
                    Ok(v) => Ok(Operand::object(v)),
                    Err(e) => view_error(s, e),
                };
            }
            _ => (),
        }
        let cap = cap(s, receiver)?;
        if let Op::Recover(graph) = op.kind {
            return crate::recovery::recover(s, receiver, args, graph);
        }
        match op.kind {
            Op::Run(bounded) if args.len() == 1 + usize::from(bounded) => {
                let block = object(args[0])?;
                roots.keep(block);
                let budget = if bounded {
                    Some(index(s, args[1])?)
                } else {
                    None
                };
                crate::control::run(s, receiver, block, budget)
            }
            Op::RestoreContinuation if args.len() == 2 => {
                let continuation = object(args[0])?;
                let value = object(args[1])?;
                refresh(s, continuation)?;
                {
                    let mut state = s.machines.borrow_mut();
                    let continuation = checked(
                        state
                            .engine
                            .continuation_capability_for_object(&state.storage, continuation),
                    )?;
                    checked(state.engine.restore_continuation(
                        OWNER,
                        cap,
                        continuation,
                        value,
                        s.nil,
                    ))?;
                }
                loop_drive(s, receiver, cap, None)
            }
            Op::Resume(bounded) if args.len() == 1 + usize::from(bounded) => {
                let value = object(args[0])?;
                let budget = if bounded {
                    Some(index(s, args[1])?)
                } else {
                    None
                };
                checked(
                    s.machines
                        .borrow_mut()
                        .engine
                        .resume_suspended_machine(OWNER, cap, value, s.nil),
                )?;
                loop_drive(s, receiver, cap, budget)
            }
            Op::ContinueDrive if args.len() == 1 => {
                let budget = index(s, args[0])?;
                {
                    let mut state = s.machines.borrow_mut();
                    checked(checked(state.engine.machine_mut(OWNER, cap))?.resume_runnable(OWNER))?;
                }
                loop_drive(s, receiver, cap, Some(budget))
            }
            Op::Destroy if args.is_empty() => {
                checked(
                    s.machines
                        .borrow_mut()
                        .engine
                        .destroy_machine_explicit(OWNER, cap),
                )?;
                Ok(Operand::object(receiver))
            }
            Op::Lifecycle | Op::Runnable if args.is_empty() => {
                let life = {
                    let state = s.machines.borrow();
                    let m = checked(state.engine.machine(OWNER, cap))?;
                    m.lifecycle()
                };
                if matches!(op.kind, Op::Runnable) {
                    return boolean(s, life == MachineLifecycle::Runnable);
                }
                let name: &[u8] = match life {
                    MachineLifecycle::New => b"new",
                    MachineLifecycle::Running => b"running",
                    MachineLifecycle::Runnable => b"runnable",
                    MachineLifecycle::Completed => b"completed",
                    MachineLifecycle::Suspended => b"suspended",
                    MachineLifecycle::Failed => b"failed",
                    MachineLifecycle::Destroyed => b"destroyed",
                };
                Ok(Operand::object(s.names[name]))
            }
            Op::Read if args.len() == 1 => {
                let i = index(s, args[0])?;
                checked(s.machines.borrow().engine.read_current_oop(OWNER, cap, i))
                    .map(Operand::object)
            }
            Op::Write if args.len() == 2 => {
                let i = index(s, args[0])?;
                let v = object(args[1])?;
                checked(
                    s.machines
                        .borrow_mut()
                        .engine
                        .write_current_oop(OWNER, cap, i, v),
                )?;
                Ok(Operand::object(v))
            }
            Op::Continue
            | Op::Call(_)
            | Op::Return
            | Op::BoundaryCall
            | Op::ExitBoundary
            | Op::Abort
                if args.len() == 1 =>
            {
                let i = index(s, args[0])?;
                let mut state = s.machines.borrow_mut();
                let State {
                    engine, storage, ..
                } = &mut *state;
                checked(match op.kind {
                    Op::Continue => engine.publish_continue(storage, OWNER, cap, i),
                    Op::Call(tail) => engine.publish_call_site(
                        storage,
                        OWNER,
                        cap,
                        i,
                        if tail {
                            CallSiteKind::TailCall
                        } else {
                            CallSiteKind::Call
                        },
                    ),
                    Op::Return => engine.publish_return(storage, OWNER, cap, i),
                    Op::BoundaryCall => engine.publish_boundary_call(storage, OWNER, cap, i),
                    Op::ExitBoundary => engine.publish_boundary_exit(storage, OWNER, cap, i),
                    _ => engine.publish_abort(storage, OWNER, cap, None, i),
                })
                .map(Operand::object)
            }
            Op::Suspend(capture) if args.len() == 2 => {
                let result = index(s, args[0])?;
                let pc = index(s, args[1])?;
                let mut state = s.machines.borrow_mut();
                let State {
                    engine, storage, ..
                } = &mut *state;
                let delimiter = checked(engine.current_delimiter(OWNER, cap))?;
                checked(if capture {
                    engine.publish_capture(storage, OWNER, cap, delimiter, result, pc)
                } else {
                    engine.publish_suspend(storage, OWNER, cap, result, delimiter, pc)
                })
                .map(Operand::object)
            }
            Op::Restore if args.len() == 2 => {
                let continuation = index(s, args[0])?;
                let value = index(s, args[1])?;
                let mut state = s.machines.borrow_mut();
                let State {
                    engine, storage, ..
                } = &mut *state;
                checked(engine.publish_restore(storage, OWNER, cap, continuation, value))
                    .map(Operand::object)
            }
            Op::ExitRunnableBoundary if args.len() == 1 => {
                checked(s.machines.borrow_mut().engine.exit_runnable_boundary(
                    OWNER,
                    cap,
                    object(args[0])?,
                    s.nil,
                ))?;
                Ok(Operand::object(receiver))
            }
            Op::Delivery if args.is_empty() => {
                let value = checked(
                    s.machines
                        .borrow()
                        .engine
                        .boundary_delivery_pending(OWNER, cap),
                )?;
                boolean(s, value)
            }
            Op::Acknowledge if args.is_empty() => {
                checked(
                    s.machines
                        .borrow_mut()
                        .engine
                        .acknowledge_boundary_delivery(OWNER, cap),
                )?;
                Ok(Operand::object(receiver))
            }
            Op::View if args.is_empty() => {
                let (handle, vt) = {
                    let state = s.machines.borrow();
                    (
                        state
                            .engine
                            .current_frame_view_handle(OWNER, cap)
                            .map_err(|_| invalid())?,
                        state.engine.objects().unwrap().frame_view_vtable,
                    )
                };
                let Some(handle) = handle else {
                    return Ok(Operand::object(s.nil));
                };
                let view = allocate(s, vt, 4)?;
                roots.keep(view);
                let mut state = s.machines.borrow_mut();
                let State {
                    engine, storage, ..
                } = &mut *state;
                engine
                    .initialize_frame_view(storage, view, receiver, handle)
                    .map_err(|_| invalid())?;
                Ok(Operand::object(view))
            }
            Op::Diagnostic(i) if args.is_empty() => {
                let d = checked(s.machines.borrow().engine.machine_diagnostics(OWNER, cap))?;
                integer(
                    s,
                    [
                        d.live_frames,
                        d.high_water_live_frames,
                        d.reserved_bytes,
                        d.current_synchronous_boundary_depth,
                        d.high_water_synchronous_boundary_depth,
                    ][i],
                )
            }
            _ => Err(invalid()),
        }
    })())
}
unsafe fn view_error(s: &Execution, error: FrameViewError) -> Result<Operand, Outcome> {
    let kind: &[u8] = match error {
        FrameViewError::InvalidFrameSlot => b"invalidFrameSlot",
        _ => b"staleFrameView",
    };
    unsafe { s.send(s.error, b"kind:", &[Operand::object(s.names[kind])]) }
}
unsafe fn install_op(
    s: &Execution,
    behavior: Oop,
    name: &'static [u8],
    kind: Op,
) -> Result<(), Outcome> {
    let op = Box::new(Operation { execution: s, kind });
    let entry = NativeEntry::new(operation, ptr::from_ref(&*op).cast_mut().cast());
    s.machines.borrow_mut().contexts.push(op);
    unsafe {
        s.send(
            behavior,
            b"addMethod",
            &[Operand::object(s.names[name]), Operand::entry(entry)],
        )
    }?;
    Ok(())
}

pub(super) const NAMES: &[&[u8]] = &[
    b"abort",
    b"abortAt:",
    b"acknowledgeBoundaryDelivery",
    b"addMethod",
    b"allocationSizeOf:",
    b"argumentCount",
    b"argumentStart",
    b"at:put:",
    b"bind:to:",
    b"boundaryCall",
    b"boundaryCallSite:",
    b"boundaryDeliveryPending",
    b"call",
    b"callSite:",
    b"capture",
    b"captureAt:continueAt:",
    b"clone",
    b"completed",
    b"continuationSlot",
    b"continue",
    b"continueAt:",
    b"continueWithBudget:",
    b"controlEligible",
    b"currentFrameView",
    b"currentSynchronousBoundaryDepth",
    b"delegated",
    b"delimiterGeneration",
    b"delimiterIndex",
    b"destroy",
    b"destroyed",
    b"driveEpoch",
    b"executionState",
    b"exitBoundary",
    b"exitBoundaryAt:",
    b"exitRunnableBoundary:",
    b"failed",
    b"fromMethod:captures:",
    b"fromMethod:codeObject:captures:",
    b"codeObject",
    b"isSynchronous",
    b"imageShellFromMethod:codeObject:",
    b"restoreImageCaptures:of:",
    b"captureCount",
    b"captureAt:",
    b"captureAt:of:",
    b"evaluatePreservingFailures:",
    b"fromStep:descriptor:selection:owner:",
    b"fromStep:descriptor:selection:owner:implementation:",
    b"withDiscardedClone:answer:selector:return:in:",
    b"withNumericPredicate:then:left:right:limit:return:in:",
    b"evaluatePredicate:then:left:right:limit:in:epoch:with:",
    b"Integer",
    b"at:",
    b"canDiscardClone:of:",
    b"canOmitIndexing",
    b"usesDefaultAllocation",
    b"withInline:at:owner:",
    b"optimizeWith:",
    b"evaluate:receiver:argument:with:",
    b"recoverAt:with:",
    b"recoverAt:withGraph:",
    b"allOopLayoutOf:words:",
    b"referenceOwnerOf:",
    b"slotAt:in:put:",
    b"allocate:bytes:",
    b"bitsOf:",
    b"usesDefaultLookup:",
    b"behaviorEpoch",
    b"generationIdentity",
    b"methodIdentity",
    b"sourceMapLocation",
    b"fromBits:",
    b"highWaterLiveFrameCount",
    b"highWaterSynchronousBoundaryDepth",
    b"identityOf:",
    b"implementationIdentity",
    b"invalidFrameSlot",
    b"isRunnable",
    b"kind",
    b"liveFrameCount",
    b"machineGeneration",
    b"machineIndex",
    b"new",
    b"oop",
    b"oopAt:",
    b"oopAt:put:",
    b"programCounter",
    b"reasonSlot",
    b"receiverSlot",
    b"reservedArenaBytes",
    b"restore",
    b"restore:with:",
    b"resultSlot",
    b"resumeWith:",
    b"resumeWith:budget:",
    b"return",
    b"returnAt:",
    b"runBlock:",
    b"runBlock:budget:",
    b"runnable",
    b"running",
    b"schemaVersion",
    b"selectorSlot",
    b"slotCount",
    b"slotKindAt:",
    b"sourceMapIdentity",
    b"staleFrameView",
    b"suppliedValueSlot",
    b"suspend",
    b"suspendAt:continueAt:",
    b"suspended",
    b"tailCall",
    b"tailCallSite:",
    b"trace:with:",
    b"value",
    b"protectedBlock:",
    b"leaveBoundary:",
    b"park",
    b"prepareBlockControl",
    b"doesNotUnderstand:",
    b"restoreContinuation:with:",
    b"valueSlot",
];
pub(super) unsafe fn install(s: &Execution, namespace: Oop) -> Result<(), Outcome> {
    unsafe {
        let scope = s.scope(&[namespace]);
        // Delegate from Object, not Nil: its prototype is obtained from Namespace.
        let object_prototype = object(s.id.send_named(
            namespace,
            b"at:",
            &[Operand::object(s.id.intern_name(b"Object")?)],
        )?)?;
        let object_behavior = object(s.send(
            s.id.receiver,
            b"behaviorOf:",
            &[Operand::object(object_prototype)],
        )?)?;
        scope.keep(object_behavior);
        for (i, name) in [b"false".as_slice(), b"true"].iter().enumerate() {
            let value = object(s.id.send_named(
                namespace,
                b"at:",
                &[Operand::object(s.id.intern_name(name)?)],
            )?)?;
            let mut state = s.machines.borrow_mut();
            state.booleans[i] = value;
            state.roots.push(value);
        }
        let machine_vtable = object(s.send(object_behavior, b"delegated", &[])?)?;
        scope.keep(machine_vtable);
        let machine_prototype = allocate(s, machine_vtable, 0)?;
        scope.keep(machine_prototype);
        s.machines
            .borrow_mut()
            .roots
            .extend([machine_vtable, machine_prototype]);
        s.send(
            s.gc,
            b"trace:with:",
            &[Operand::object(machine_vtable), Operand::object(s.receiver)],
        )?;
        let frame_view_vtable = object(s.send(object_behavior, b"delegated", &[])?)?;
        scope.keep(frame_view_vtable);
        let frame_view_prototype = allocate(s, frame_view_vtable, 0)?;
        scope.keep(frame_view_prototype);
        s.machines
            .borrow_mut()
            .roots
            .extend([frame_view_vtable, frame_view_prototype]);
        s.send(
            s.gc,
            b"trace:with:",
            &[
                Operand::object(frame_view_vtable),
                Operand::object(s.receiver),
            ],
        )?;
        let continuation_vtable = object(s.send(object_behavior, b"delegated", &[])?)?;
        scope.keep(continuation_vtable);
        let continuation_prototype = allocate(s, continuation_vtable, 0)?;
        scope.keep(continuation_prototype);
        s.machines
            .borrow_mut()
            .roots
            .extend([continuation_vtable, continuation_prototype]);
        s.send(
            s.gc,
            b"trace:with:",
            &[
                Operand::object(continuation_vtable),
                Operand::object(s.receiver),
            ],
        )?;
        let continue_action_vtable = object(s.send(object_behavior, b"delegated", &[])?)?;
        scope.keep(continue_action_vtable);
        let continue_action_prototype = allocate(s, continue_action_vtable, 0)?;
        scope.keep(continue_action_prototype);
        s.machines
            .borrow_mut()
            .roots
            .extend([continue_action_vtable, continue_action_prototype]);
        s.send(
            s.gc,
            b"trace:with:",
            &[
                Operand::object(continue_action_vtable),
                Operand::object(s.receiver),
            ],
        )?;
        let call_action_vtable = object(s.send(object_behavior, b"delegated", &[])?)?;
        scope.keep(call_action_vtable);
        let call_action_prototype = allocate(s, call_action_vtable, 0)?;
        scope.keep(call_action_prototype);
        s.machines
            .borrow_mut()
            .roots
            .extend([call_action_vtable, call_action_prototype]);
        s.send(
            s.gc,
            b"trace:with:",
            &[
                Operand::object(call_action_vtable),
                Operand::object(s.receiver),
            ],
        )?;
        let tail_call_action_vtable = object(s.send(object_behavior, b"delegated", &[])?)?;
        scope.keep(tail_call_action_vtable);
        let tail_call_action_prototype = allocate(s, tail_call_action_vtable, 0)?;
        scope.keep(tail_call_action_prototype);
        s.machines
            .borrow_mut()
            .roots
            .extend([tail_call_action_vtable, tail_call_action_prototype]);
        s.send(
            s.gc,
            b"trace:with:",
            &[
                Operand::object(tail_call_action_vtable),
                Operand::object(s.receiver),
            ],
        )?;
        let return_action_vtable = object(s.send(object_behavior, b"delegated", &[])?)?;
        scope.keep(return_action_vtable);
        let return_action_prototype = allocate(s, return_action_vtable, 0)?;
        scope.keep(return_action_prototype);
        s.machines
            .borrow_mut()
            .roots
            .extend([return_action_vtable, return_action_prototype]);
        s.send(
            s.gc,
            b"trace:with:",
            &[
                Operand::object(return_action_vtable),
                Operand::object(s.receiver),
            ],
        )?;
        let suspend_action_vtable = object(s.send(object_behavior, b"delegated", &[])?)?;
        scope.keep(suspend_action_vtable);
        let suspend_action_prototype = allocate(s, suspend_action_vtable, 0)?;
        scope.keep(suspend_action_prototype);
        s.machines
            .borrow_mut()
            .roots
            .extend([suspend_action_vtable, suspend_action_prototype]);
        s.send(
            s.gc,
            b"trace:with:",
            &[
                Operand::object(suspend_action_vtable),
                Operand::object(s.receiver),
            ],
        )?;
        let capture_action_vtable = object(s.send(object_behavior, b"delegated", &[])?)?;
        scope.keep(capture_action_vtable);
        let capture_action_prototype = allocate(s, capture_action_vtable, 0)?;
        scope.keep(capture_action_prototype);
        s.machines
            .borrow_mut()
            .roots
            .extend([capture_action_vtable, capture_action_prototype]);
        s.send(
            s.gc,
            b"trace:with:",
            &[
                Operand::object(capture_action_vtable),
                Operand::object(s.receiver),
            ],
        )?;
        let restore_action_vtable = object(s.send(object_behavior, b"delegated", &[])?)?;
        scope.keep(restore_action_vtable);
        let restore_action_prototype = allocate(s, restore_action_vtable, 0)?;
        scope.keep(restore_action_prototype);
        s.machines
            .borrow_mut()
            .roots
            .extend([restore_action_vtable, restore_action_prototype]);
        s.send(
            s.gc,
            b"trace:with:",
            &[
                Operand::object(restore_action_vtable),
                Operand::object(s.receiver),
            ],
        )?;
        let abort_action_vtable = object(s.send(object_behavior, b"delegated", &[])?)?;
        scope.keep(abort_action_vtable);
        let abort_action_prototype = allocate(s, abort_action_vtable, 0)?;
        scope.keep(abort_action_prototype);
        s.machines
            .borrow_mut()
            .roots
            .extend([abort_action_vtable, abort_action_prototype]);
        s.send(
            s.gc,
            b"trace:with:",
            &[
                Operand::object(abort_action_vtable),
                Operand::object(s.receiver),
            ],
        )?;
        let boundary_call_action_vtable = object(s.send(object_behavior, b"delegated", &[])?)?;
        scope.keep(boundary_call_action_vtable);
        let boundary_call_action_prototype = allocate(s, boundary_call_action_vtable, 0)?;
        scope.keep(boundary_call_action_prototype);
        s.machines
            .borrow_mut()
            .roots
            .extend([boundary_call_action_vtable, boundary_call_action_prototype]);
        s.send(
            s.gc,
            b"trace:with:",
            &[
                Operand::object(boundary_call_action_vtable),
                Operand::object(s.receiver),
            ],
        )?;
        let exit_boundary_action_vtable = object(s.send(object_behavior, b"delegated", &[])?)?;
        scope.keep(exit_boundary_action_vtable);
        let exit_boundary_action_prototype = allocate(s, exit_boundary_action_vtable, 0)?;
        scope.keep(exit_boundary_action_prototype);
        s.machines
            .borrow_mut()
            .roots
            .extend([exit_boundary_action_vtable, exit_boundary_action_prototype]);
        s.send(
            s.gc,
            b"trace:with:",
            &[
                Operand::object(exit_boundary_action_vtable),
                Operand::object(s.receiver),
            ],
        )?;
        let block_vtable = object(s.send(object_behavior, b"delegated", &[])?)?;
        scope.keep(block_vtable);
        let block_prototype = allocate(s, block_vtable, 0)?;
        scope.keep(block_prototype);
        // Reflect must update the shared Block behavior, so existing and future
        // compiled Blocks observe prototype method replacement.
        s.id.send_named(
            s.id.receiver,
            b"ownerOf:put:",
            &[
                Operand::object(block_vtable),
                Operand::object(block_prototype),
            ],
        )?;
        s.machines
            .borrow_mut()
            .roots
            .extend([block_vtable, block_prototype]);
        s.send(
            s.gc,
            b"trace:with:",
            &[Operand::object(block_vtable), Operand::object(s.receiver)],
        )?;
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
            abort_action_vtable,
            abort_action_prototype,
            boundary_call_action_vtable,
            boundary_call_action_prototype,
            exit_boundary_action_vtable,
            exit_boundary_action_prototype,
            continue_kind: s.names[b"continue".as_slice()],
            call_kind: s.names[b"call".as_slice()],
            tail_call_kind: s.names[b"tailCall".as_slice()],
            return_kind: s.names[b"return".as_slice()],
            suspend_kind: s.names[b"suspend".as_slice()],
            capture_kind: s.names[b"capture".as_slice()],
            restore_kind: s.names[b"restore".as_slice()],
            abort_kind: s.names[b"abort".as_slice()],
            boundary_call_kind: s.names[b"boundaryCall".as_slice()],
            exit_boundary_kind: s.names[b"exitBoundary".as_slice()],
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
        {
            let mut state = s.machines.borrow_mut();
            state.engine.install_objects(objects);
            state.block_behavior = block_vtable;
        }
        let execution_behavior = object(s.send(s.id.receiver, b"behaviorOf:", &[Operand::object(s.receiver)])?)?;
        install_op(s, execution_behavior, b"evaluatePreservingFailures:", Op::PreserveFailures)?;
        install_op(
            s,
            s.method_behavior,
            b"fromStep:descriptor:selection:owner:",
            Op::FromStep,
        )?;
        install_op(
            s,
            s.method_behavior,
            b"withInline:at:owner:",
            Op::WithInline,
        )?;
        install_op(
            s,
            s.method_behavior,
            b"withDiscardedClone:answer:selector:return:in:",
            Op::WithDiscardedClone,
        )?;
        install_op(
            s,
            s.method_behavior,
            b"withNumericPredicate:then:left:right:limit:return:in:",
            Op::WithNumericPredicate,
        )?;
        install_op(s, s.method_behavior, b"optimizeWith:", Op::Optimize)?;
        install_op(
            s,
            s.method_behavior,
            b"fromStep:descriptor:selection:owner:implementation:",
            Op::FromStep,
        )?;
        install_op(s, machine_vtable, b"recoverAt:with:", Op::Recover(false))?;
        install_op(
            s,
            machine_vtable,
            b"recoverAt:withGraph:",
            Op::Recover(true),
        )?;
        install_op(s, machine_vtable, b"new", Op::New)?;
        install_op(s, machine_vtable, b"clone", Op::Disabled)?;
        install_op(s, machine_vtable, b"runBlock:", Op::Run(false))?;
        install_op(s, machine_vtable, b"runBlock:budget:", Op::Run(true))?;
        install_op(
            s,
            machine_vtable,
            b"restoreContinuation:with:",
            Op::RestoreContinuation,
        )?;
        install_op(s, machine_vtable, b"resumeWith:", Op::Resume(false))?;
        install_op(s, machine_vtable, b"resumeWith:budget:", Op::Resume(true))?;
        install_op(s, machine_vtable, b"continueWithBudget:", Op::ContinueDrive)?;
        install_op(s, machine_vtable, b"executionState", Op::Lifecycle)?;
        install_op(s, machine_vtable, b"isRunnable", Op::Runnable)?;
        install_op(s, machine_vtable, b"prepareBlockControl", Op::Prepare)?;
        install_op(s, machine_vtable, b"destroy", Op::Destroy)?;
        install_op(s, machine_vtable, b"oopAt:", Op::Read)?;
        install_op(s, machine_vtable, b"oopAt:put:", Op::Write)?;
        install_op(s, machine_vtable, b"continueAt:", Op::Continue)?;
        install_op(s, machine_vtable, b"callSite:", Op::Call(false))?;
        install_op(s, machine_vtable, b"tailCallSite:", Op::Call(true))?;
        install_op(s, machine_vtable, b"returnAt:", Op::Return)?;
        install_op(
            s,
            machine_vtable,
            b"suspendAt:continueAt:",
            Op::Suspend(false),
        )?;
        install_op(
            s,
            machine_vtable,
            b"captureAt:continueAt:",
            Op::Suspend(true),
        )?;
        install_op(s, machine_vtable, b"restore:with:", Op::Restore)?;
        install_op(s, machine_vtable, b"abortAt:", Op::Abort)?;
        install_op(s, machine_vtable, b"boundaryCallSite:", Op::BoundaryCall)?;
        install_op(s, machine_vtable, b"exitBoundaryAt:", Op::ExitBoundary)?;
        install_op(
            s,
            machine_vtable,
            b"exitRunnableBoundary:",
            Op::ExitRunnableBoundary,
        )?;
        install_op(s, machine_vtable, b"boundaryDeliveryPending", Op::Delivery)?;
        install_op(
            s,
            machine_vtable,
            b"acknowledgeBoundaryDelivery",
            Op::Acknowledge,
        )?;
        install_op(s, machine_vtable, b"currentFrameView", Op::View)?;
        install_op(s, machine_vtable, b"liveFrameCount", Op::Diagnostic(0))?;
        install_op(
            s,
            machine_vtable,
            b"highWaterLiveFrameCount",
            Op::Diagnostic(1),
        )?;
        install_op(s, machine_vtable, b"reservedArenaBytes", Op::Diagnostic(2))?;
        install_op(
            s,
            machine_vtable,
            b"currentSynchronousBoundaryDepth",
            Op::Diagnostic(3),
        )?;
        install_op(
            s,
            machine_vtable,
            b"highWaterSynchronousBoundaryDepth",
            Op::Diagnostic(4),
        )?;
        install_op(
            s,
            frame_view_vtable,
            b"implementationIdentity",
            Op::ViewWord(0),
        )?;
        install_op(s, frame_view_vtable, b"methodIdentity", Op::ViewWord(0))?;
        install_op(s, frame_view_vtable, b"sourceMapLocation", Op::ViewWord(3))?;
        install_op(s, frame_view_vtable, b"generationIdentity", Op::ViewWord(1))?;
        install_op(s, frame_view_vtable, b"sourceMapIdentity", Op::ViewWord(2))?;
        install_op(s, frame_view_vtable, b"programCounter", Op::ViewWord(3))?;
        install_op(s, frame_view_vtable, b"slotCount", Op::ViewWord(4))?;
        install_op(s, frame_view_vtable, b"oopAt:", Op::ViewRead)?;
        install_op(s, frame_view_vtable, b"slotKindAt:", Op::ViewKind)?;
        install_op(s, frame_view_vtable, b"clone", Op::Disabled)?;
        install_op(s, block_vtable, b"fromMethod:captures:", Op::Block(false))?;
        install_op(
            s,
            block_vtable,
            b"fromMethod:codeObject:captures:",
            Op::Block(true),
        )?;
        s.id.send_named(
            namespace,
            b"at:put:",
            &[
                Operand::object(s.id.intern_name(b"BlockVt")?),
                Operand::object(block_vtable),
            ],
        )?;
        install_op(s, block_vtable, b"codeObject", Op::BlockCode)?;
        install_op(s, block_vtable, b"isSynchronous", Op::BlockSynchronous)?;
        install_op(
            s,
            block_vtable,
            b"imageShellFromMethod:codeObject:",
            Op::BlockShell,
        )?;
        install_op(
            s,
            block_vtable,
            b"restoreImageCaptures:of:",
            Op::RestoreCaptures,
        )?;
        install_op(s, block_vtable, b"captureCount", Op::BlockCaptureCount)?;
        install_op(s, block_vtable, b"captureAt:", Op::BlockCapture(false))?;
        install_op(s, block_vtable, b"captureAt:of:", Op::BlockCapture(true))?;
        install_op(s, block_vtable, b"clone", Op::CloneBlock)?;
        install_op(s, continuation_vtable, b"clone", Op::Disabled)?;
        install_op(s, continue_action_vtable, b"kind", Op::Kind)?;
        install_op(s, continue_action_vtable, b"schemaVersion", Op::Word(0))?;
        install_op(s, continue_action_vtable, b"machineIndex", Op::Word(1))?;
        install_op(s, continue_action_vtable, b"machineGeneration", Op::Word(2))?;
        install_op(s, continue_action_vtable, b"driveEpoch", Op::Word(3))?;
        install_op(s, continue_action_vtable, b"clone", Op::Disabled)?;
        install_op(s, call_action_vtable, b"kind", Op::Kind)?;
        install_op(s, call_action_vtable, b"schemaVersion", Op::Word(0))?;
        install_op(s, call_action_vtable, b"machineIndex", Op::Word(1))?;
        install_op(s, call_action_vtable, b"machineGeneration", Op::Word(2))?;
        install_op(s, call_action_vtable, b"driveEpoch", Op::Word(3))?;
        install_op(s, call_action_vtable, b"clone", Op::Disabled)?;
        install_op(s, call_action_vtable, b"receiverSlot", Op::Word(4))?;
        install_op(s, call_action_vtable, b"selectorSlot", Op::Word(5))?;
        install_op(s, call_action_vtable, b"argumentStart", Op::Word(6))?;
        install_op(s, call_action_vtable, b"argumentCount", Op::Word(7))?;
        install_op(s, call_action_vtable, b"resultSlot", Op::Word(8))?;
        install_op(s, tail_call_action_vtable, b"kind", Op::Kind)?;
        install_op(s, tail_call_action_vtable, b"schemaVersion", Op::Word(0))?;
        install_op(s, tail_call_action_vtable, b"machineIndex", Op::Word(1))?;
        install_op(
            s,
            tail_call_action_vtable,
            b"machineGeneration",
            Op::Word(2),
        )?;
        install_op(s, tail_call_action_vtable, b"driveEpoch", Op::Word(3))?;
        install_op(s, tail_call_action_vtable, b"clone", Op::Disabled)?;
        install_op(s, tail_call_action_vtable, b"receiverSlot", Op::Word(4))?;
        install_op(s, tail_call_action_vtable, b"selectorSlot", Op::Word(5))?;
        install_op(s, tail_call_action_vtable, b"argumentStart", Op::Word(6))?;
        install_op(s, tail_call_action_vtable, b"argumentCount", Op::Word(7))?;
        install_op(s, return_action_vtable, b"kind", Op::Kind)?;
        install_op(s, return_action_vtable, b"schemaVersion", Op::Word(0))?;
        install_op(s, return_action_vtable, b"machineIndex", Op::Word(1))?;
        install_op(s, return_action_vtable, b"machineGeneration", Op::Word(2))?;
        install_op(s, return_action_vtable, b"driveEpoch", Op::Word(3))?;
        install_op(s, return_action_vtable, b"clone", Op::Disabled)?;
        install_op(s, return_action_vtable, b"valueSlot", Op::Word(4))?;
        install_op(s, suspend_action_vtable, b"kind", Op::Kind)?;
        install_op(s, suspend_action_vtable, b"schemaVersion", Op::Word(0))?;
        install_op(s, suspend_action_vtable, b"machineIndex", Op::Word(1))?;
        install_op(s, suspend_action_vtable, b"machineGeneration", Op::Word(2))?;
        install_op(s, suspend_action_vtable, b"driveEpoch", Op::Word(3))?;
        install_op(s, suspend_action_vtable, b"clone", Op::Disabled)?;
        install_op(s, suspend_action_vtable, b"delimiterIndex", Op::Word(4))?;
        install_op(
            s,
            suspend_action_vtable,
            b"delimiterGeneration",
            Op::Word(5),
        )?;
        install_op(s, suspend_action_vtable, b"resultSlot", Op::Word(6))?;
        install_op(s, capture_action_vtable, b"kind", Op::Kind)?;
        install_op(s, capture_action_vtable, b"schemaVersion", Op::Word(0))?;
        install_op(s, capture_action_vtable, b"machineIndex", Op::Word(1))?;
        install_op(s, capture_action_vtable, b"machineGeneration", Op::Word(2))?;
        install_op(s, capture_action_vtable, b"driveEpoch", Op::Word(3))?;
        install_op(s, capture_action_vtable, b"clone", Op::Disabled)?;
        install_op(s, capture_action_vtable, b"delimiterIndex", Op::Word(4))?;
        install_op(
            s,
            capture_action_vtable,
            b"delimiterGeneration",
            Op::Word(5),
        )?;
        install_op(s, capture_action_vtable, b"resultSlot", Op::Word(6))?;
        install_op(s, restore_action_vtable, b"kind", Op::Kind)?;
        install_op(s, restore_action_vtable, b"schemaVersion", Op::Word(0))?;
        install_op(s, restore_action_vtable, b"machineIndex", Op::Word(1))?;
        install_op(s, restore_action_vtable, b"machineGeneration", Op::Word(2))?;
        install_op(s, restore_action_vtable, b"driveEpoch", Op::Word(3))?;
        install_op(s, restore_action_vtable, b"clone", Op::Disabled)?;
        install_op(s, restore_action_vtable, b"continuationSlot", Op::Word(4))?;
        install_op(s, restore_action_vtable, b"suppliedValueSlot", Op::Word(5))?;
        install_op(s, abort_action_vtable, b"kind", Op::Kind)?;
        install_op(s, abort_action_vtable, b"schemaVersion", Op::Word(0))?;
        install_op(s, abort_action_vtable, b"machineIndex", Op::Word(1))?;
        install_op(s, abort_action_vtable, b"machineGeneration", Op::Word(2))?;
        install_op(s, abort_action_vtable, b"driveEpoch", Op::Word(3))?;
        install_op(s, abort_action_vtable, b"clone", Op::Disabled)?;
        install_op(s, abort_action_vtable, b"delimiterIndex", Op::Word(4))?;
        install_op(s, abort_action_vtable, b"delimiterGeneration", Op::Word(5))?;
        install_op(s, abort_action_vtable, b"reasonSlot", Op::Word(6))?;
        install_op(s, boundary_call_action_vtable, b"kind", Op::Kind)?;
        install_op(
            s,
            boundary_call_action_vtable,
            b"schemaVersion",
            Op::Word(0),
        )?;
        install_op(s, boundary_call_action_vtable, b"machineIndex", Op::Word(1))?;
        install_op(
            s,
            boundary_call_action_vtable,
            b"machineGeneration",
            Op::Word(2),
        )?;
        install_op(s, boundary_call_action_vtable, b"driveEpoch", Op::Word(3))?;
        install_op(s, boundary_call_action_vtable, b"clone", Op::Disabled)?;
        install_op(s, boundary_call_action_vtable, b"receiverSlot", Op::Word(4))?;
        install_op(s, boundary_call_action_vtable, b"selectorSlot", Op::Word(5))?;
        install_op(
            s,
            boundary_call_action_vtable,
            b"argumentStart",
            Op::Word(6),
        )?;
        install_op(
            s,
            boundary_call_action_vtable,
            b"argumentCount",
            Op::Word(7),
        )?;
        install_op(s, boundary_call_action_vtable, b"resultSlot", Op::Word(8))?;
        install_op(s, exit_boundary_action_vtable, b"kind", Op::Kind)?;
        install_op(
            s,
            exit_boundary_action_vtable,
            b"schemaVersion",
            Op::Word(0),
        )?;
        install_op(s, exit_boundary_action_vtable, b"machineIndex", Op::Word(1))?;
        install_op(
            s,
            exit_boundary_action_vtable,
            b"machineGeneration",
            Op::Word(2),
        )?;
        install_op(s, exit_boundary_action_vtable, b"driveEpoch", Op::Word(3))?;
        install_op(s, exit_boundary_action_vtable, b"clone", Op::Disabled)?;
        install_op(s, exit_boundary_action_vtable, b"valueSlot", Op::Word(4))?;
        crate::control::install(s, machine_vtable)?;
        s.send(
            namespace,
            b"at:put:",
            &[
                Operand::object(s.id.intern_name(b"Machine")?),
                Operand::object(machine_prototype),
            ],
        )?;
        s.send(
            namespace,
            b"at:put:",
            &[
                Operand::object(s.id.intern_name(b"FrameView")?),
                Operand::object(frame_view_prototype),
            ],
        )?;
        s.send(
            namespace,
            b"at:put:",
            &[
                Operand::object(s.id.intern_name(b"Continuation")?),
                Operand::object(continuation_prototype),
            ],
        )?;
        s.send(
            namespace,
            b"at:put:",
            &[
                Operand::object(s.id.intern_name(b"ContinueAction")?),
                Operand::object(continue_action_prototype),
            ],
        )?;
        s.send(
            namespace,
            b"at:put:",
            &[
                Operand::object(s.id.intern_name(b"CallAction")?),
                Operand::object(call_action_prototype),
            ],
        )?;
        s.send(
            namespace,
            b"at:put:",
            &[
                Operand::object(s.id.intern_name(b"TailCallAction")?),
                Operand::object(tail_call_action_prototype),
            ],
        )?;
        s.send(
            namespace,
            b"at:put:",
            &[
                Operand::object(s.id.intern_name(b"ReturnAction")?),
                Operand::object(return_action_prototype),
            ],
        )?;
        s.send(
            namespace,
            b"at:put:",
            &[
                Operand::object(s.id.intern_name(b"SuspendAction")?),
                Operand::object(suspend_action_prototype),
            ],
        )?;
        s.send(
            namespace,
            b"at:put:",
            &[
                Operand::object(s.id.intern_name(b"CaptureAction")?),
                Operand::object(capture_action_prototype),
            ],
        )?;
        s.send(
            namespace,
            b"at:put:",
            &[
                Operand::object(s.id.intern_name(b"RestoreAction")?),
                Operand::object(restore_action_prototype),
            ],
        )?;
        s.send(
            namespace,
            b"at:put:",
            &[
                Operand::object(s.id.intern_name(b"AbortAction")?),
                Operand::object(abort_action_prototype),
            ],
        )?;
        s.send(
            namespace,
            b"at:put:",
            &[
                Operand::object(s.id.intern_name(b"BoundaryCallAction")?),
                Operand::object(boundary_call_action_prototype),
            ],
        )?;
        s.send(
            namespace,
            b"at:put:",
            &[
                Operand::object(s.id.intern_name(b"ExitBoundaryAction")?),
                Operand::object(exit_boundary_action_prototype),
            ],
        )?;
        s.send(
            namespace,
            b"at:put:",
            &[
                Operand::object(s.id.intern_name(b"Block")?),
                Operand::object(block_prototype),
            ],
        )?;
        Ok(())
    }
}

/// Only compare entries installed by this Execution owner; Id judges its own
/// default lookup. Epoch comparison fences mutations during guard observation.
pub(super) unsafe fn inline_controls_current(s: &Execution, machine: Oop) -> Result<bool, Outcome> {
    unsafe { controls_current(s, machine, false) }
}
pub(super) unsafe fn tier_controls_current(s: &Execution, machine: Oop) -> Result<bool, Outcome> {
    unsafe { controls_current(s, machine, true) }
}
unsafe fn controls_current(s: &Execution, machine: Oop, tier: bool) -> Result<bool, Outcome> {
    unsafe {
        let epoch = s
            .send(s.id.receiver, b"behaviorEpoch", &[])?
            .as_word()
            .ok_or_else(invalid)?;
        if epoch == usize::MAX
            || s.send(
                s.id.receiver,
                b"usesDefaultLookup:",
                &[Operand::object(machine)],
            )?
            .as_word()
                != Some(1)
        {
            return Ok(false);
        }
        let mut operations = vec![
            (b"oopAt:".as_slice(), Op::Read),
            (b"oopAt:put:", Op::Write),
            (b"returnAt:", Op::Return),
            (b"currentFrameView", Op::View),
        ];
        if tier {
            operations.extend([
                (b"callSite:".as_slice(), Op::Call(false)),
                (b"tailCallSite:", Op::Call(true)),
                (b"continueAt:", Op::Continue),
            ]);
        }
        for (name, kind) in operations {
            let expected = {
                let state = s.machines.borrow();
                let context = state
                    .contexts
                    .iter()
                    .find(|op| op.kind == kind)
                    .ok_or_else(invalid)?;
                NativeEntry::new(operation, ptr::from_ref(&**context).cast_mut().cast())
            };
            if lookup_target(s, machine, s.names[name])? != Some(expected) {
                return Ok(false);
            }
        }
        Ok(s.send(s.id.receiver, b"behaviorEpoch", &[])?.as_word() == Some(epoch))
    }
}
