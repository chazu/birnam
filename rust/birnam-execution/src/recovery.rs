//! Pending-call recovery. Recipe reads and allocations use current owner messages;
//! the retained frame engine alone admits and atomically publishes the transfer.
use super::*;
use crate::{
    execution::{recovery::RecoveryAuthority, *},
    machine::{self, OWNER, checked},
    recovery_graph::{self as graph, Graph, Node, Plan, Value},
};
#[derive(Clone, Copy)]
pub(super) enum Body {
    Native(NativeEntry),
    DiscardedClone(crate::discarded_clone::Plan),
    NumericPredicate(crate::numeric_predicate::Plan),
}
#[derive(Clone, Copy)]
pub(super) struct Inline {
    pub body: Body,
    pub owner: Oop,
    pub return_slot: usize,
}
pub(super) struct Request {
    authority: RecoveryAuthority,
    consumed: bool,
    predicate_target: Option<(Oop, usize)>,
}
struct RequestGuard<'a> {
    execution: &'a Execution,
    returned_to_driver: bool,
}
impl Drop for RequestGuard<'_> {
    fn drop(&mut self) {
        let request = self
            .execution
            .requests
            .borrow_mut()
            .pop()
            .expect("scoped recovery request");
        if !request.consumed && !self.returned_to_driver {
            self.execution
                .machines
                .borrow_mut()
                .engine
                .release_acquired_entry(request.authority.entry)
                .expect("request owns acquisition");
        }
    }
}
fn authority(
    s: &Execution,
    cap: MachineCapability,
    pc: usize,
) -> Result<RecoveryAuthority, Outcome> {
    let requests = s.requests.borrow();
    let request = requests
        .last()
        .filter(|r| !r.consumed && r.authority.machine == cap)
        .ok_or_else(invalid)?;
    if pc >= request.authority.descriptor.program_counter_count() {
        return Err(invalid());
    }
    checked(
        s.machines
            .borrow()
            .engine
            .validate_recovery_authority(OWNER, &request.authority),
    )?;
    Ok(request.authority.clone())
}
fn publish(
    s: &Execution,
    cap: MachineCapability,
    pc: usize,
    values: &[Oop],
) -> Result<(), Outcome> {
    let a = authority(s, cap, pc)?;
    if values.len() != a.descriptor.slot_count() {
        return Err(invalid());
    }
    let slots: Vec<_> = (0..values.len()).map(|i| (i, i)).collect();
    checked(
        s.machines
            .borrow_mut()
            .engine
            .recover_acquired_call(OWNER, &a, pc, &slots, values, s.nil),
    )?;
    s.requests.borrow_mut().last_mut().unwrap().consumed = true;
    Ok(())
}

pub(super) unsafe fn with_inline(
    s: &Execution,
    receiver: Oop,
    args: &[Operand],
) -> Result<Operand, Outcome> {
    unsafe {
        let [entry, slot, owner] = args else {
            return Err(invalid());
        };
        let inline = Inline {
            body: Body::Native(native_entry(*entry)?),
            owner: object(*owner)?,
            return_slot: machine::index(s, *slot)?,
        };
        let roots = s.scope(&[receiver, inline.owner]);
        {
            let methods = s.methods.borrow();
            let registered = methods
                .get(&receiver.addr())
                .and_then(|m| m.resumable.as_ref())
                .ok_or_else(invalid)?;
            if registered.descriptor.slot_kind(inline.return_slot) != Some(SlotKind::Oop) {
                return Err(invalid());
            }
        }
        let value = s.capture(s.entry(receiver, false)?)?;
        roots.keep(object(value)?);
        s.methods
            .borrow_mut()
            .get_mut(&object(value)?.addr())
            .unwrap()
            .inline = Some(inline);
        Ok(value)
    }
}

/// Caller transfers the acquisition only if this returns true (or an error).
/// A false guard verdict leaves it with the ordinary driver.
pub(super) unsafe fn try_inline(
    s: &Execution,
    machine: Oop,
    cap: MachineCapability,
    entry: ResumableEntryCapability,
    result_slot: usize,
    inline: Inline,
    receiver: Oop,
    args: &[Oop],
) -> Result<bool, Outcome> {
    unsafe {
        let eligible = {
            let state = s.machines.borrow();
            state.engine.machine(OWNER, cap).map(|m| {
                m.control_eligible()
                    && state
                        .engine
                        .inline_return_storage_valid(&state.storage, OWNER, cap)
            })
        };
        let eligible = match eligible {
            Ok(value) => value,
            Err(e) => {
                checked(s.machines.borrow_mut().engine.release_acquired_entry(entry))?;
                return Err(machine::engine_failure(e));
            }
        };
        if !eligible {
            return Ok(false);
        }
        let roots = s.scope(&[machine, inline.owner, receiver]);
        for &arg in args {
            roots.keep(arg);
        }
        // Guard lookup can reenter. Expose recovery authority only after guards
        // have passed, and re-admit the pending call after their callbacks.
        match machine::inline_controls_current(s, machine) {
            Ok(true) => (),
            Ok(false) => return Ok(false),
            Err(error) => {
                checked(s.machines.borrow_mut().engine.release_acquired_entry(entry))?;
                return Err(error);
            }
        }
        let admitted = {
            s.machines
                .borrow()
                .engine
                .recovery_authority(OWNER, cap, entry, result_slot)
        };
        let a = match admitted {
            Ok(a) => a,
            Err(e) => {
                checked(s.machines.borrow_mut().engine.release_acquired_entry(entry))?;
                return Err(machine::engine_failure(e));
            }
        };
        s.requests.borrow_mut().push(Request {
            authority: a,
            consumed: false,
            predicate_target: matches!(inline.body, Body::NumericPredicate(_))
                .then_some((machine, inline.return_slot)),
        });
        let mut request = RequestGuard {
            execution: s,
            returned_to_driver: false,
        };
        authority(s, cap, 0)?;
        let mut operands = Vec::with_capacity(args.len() + 1);
        operands.push(Operand::object(receiver));
        operands.extend(args.iter().copied().map(Operand::object));
        let answer = match inline.body {
            Body::Native(entry) => {
                entry.method.unwrap()(entry.context, machine, operands.as_ptr(), operands.len())
                    .into_result()?
            }
            Body::DiscardedClone(plan) => {
                crate::discarded_clone::try_result(s, machine, plan, args)?
            }
            Body::NumericPredicate(plan) => {
                crate::numeric_predicate::try_result(s, machine, plan, args)?
            }
        };
        if s.requests.borrow().last().unwrap().consumed {
            // Recovery published a baseline Frame, or the predicate visitor committed
            // its result. Only acknowledgement may follow either publication.
            return if answer.kind() == native::UNIT {
                Ok(true)
            } else {
                Err(invalid())
            };
        }
        authority(s, cap, 0)?;
        if answer.kind() == native::UNIT {
            request.returned_to_driver = true;
            return Ok(false);
        }
        let value = object(answer)?;
        // The discarded-clone result is already a rooted argument. Do not send
        // another retain after its final guard: that callback could invalidate
        // the verdict. Arbitrary native results still need their own root.
        if matches!(inline.body, Body::Native(_)) {
            roots.keep(value);
        }
        let mut state = s.machines.borrow_mut();
        let machine::State {
            engine, storage, ..
        } = &mut *state;
        checked(engine.commit_inlined_return(
            storage,
            OWNER,
            cap,
            result_slot,
            inline.return_slot,
            s.nil,
            value,
        ))?;
        Ok(true) // guard releases the acquisition after committing the direct result
    }
}

pub(super) fn commit_predicate(s: &Execution, value: Oop) -> Result<(), Outcome> {
    let (machine, slot) = s.requests.borrow()
        .last()
        .filter(|r| !r.consumed)
        .and_then(|r| r.predicate_target)
        .ok_or_else(invalid)?;
    let cap = machine::cap(s, machine)?;
    let a = authority(s, cap, 0)?;
    {
        let mut state = s.machines.borrow_mut();
        let machine::State {
            engine, storage, ..
        } = &mut *state;
        checked(engine.commit_inlined_return(
            storage,
            OWNER,
            cap,
            a.result_slot,
            slot,
            s.nil,
            value,
        ))?;
        checked(engine.release_acquired_entry(a.entry))?;
    }
    s.requests.borrow_mut().last_mut().unwrap().consumed = true;
    Ok(())
}

struct Reader<'a> {
    s: &'a Execution,
    roots: Scope<'a>,
}
impl<'a> Reader<'a> {
    fn new(s: &'a Execution, recipe: Oop) -> Self {
        Self {
            s,
            roots: s.scope(&[recipe]),
        }
    }
    unsafe fn integer(&self, value: Oop) -> Result<i64, Outcome> {
        unsafe {
            self.s
                .send(self.s.integer_factory()?, b"wordOf:", &[Operand::object(value)])
        }?
        .as_word()
        .map(|n| n as i64)
        .ok_or_else(invalid)
    }
    unsafe fn reference(&self, value: Oop) -> Result<Value, Outcome> {
        let value = unsafe { self.integer(value) }?;
        if value >= 0 {
            u32::try_from(value)
                .map(Value::Input)
                .map_err(|_| invalid())
        } else {
            u32::try_from(-(value + 1))
                .map(Value::Node)
                .map_err(|_| invalid())
        }
    }
    unsafe fn vector(&self, value: Oop, limit: usize) -> Result<Vec<Oop>, Outcome> {
        unsafe {
            self.roots.keep(value);
            let size = object(self.s.send(value, b"size", &[])?)?;
            self.roots.keep(size);
            let size = usize::try_from(self.integer(size)?).map_err(|_| invalid())?;
            if size > limit {
                return Err(invalid());
            }
            let mut values = Vec::with_capacity(size);
            for i in 0..size {
                let index = machine::integer(self.s, i)?;
                self.roots.keep(object(index)?);
                let value = object(self.s.send(value, b"at:", &[index])?)?;
                self.roots.keep(value);
                values.push(value);
            }
            Ok(values)
        }
    }
}
pub(super) unsafe fn recover(
    s: &Execution,
    machine: Oop,
    args: &[Operand],
    graph: bool,
) -> Result<Operand, Outcome> {
    unsafe {
        let [pc, recipe] = args else {
            return Err(invalid());
        };
        let pc = machine::index(s, *pc)?;
        let recipe = object(*recipe)?;
        let cap = machine::cap(s, machine)?;
        let a = authority(s, cap, pc)?;
        let reader = Reader::new(s, recipe);
        if !graph {
            let values = reader.vector(recipe, a.descriptor.slot_count())?;
            publish(s, cap, pc, &values)?;
        } else {
            let parts = reader.vector(recipe, 3)?;
            let [inputs, nodes, slots] = parts.as_slice() else {
                return Err(invalid());
            };
            let inputs = reader.vector(*inputs, graph::MAX_INPUTS)?;
            let nodes = reader.vector(*nodes, graph::MAX_NODES)?;
            let slots = reader.vector(*slots, graph::MAX_SLOTS)?;
            if slots.len() != a.descriptor.slot_count() {
                return Err(invalid());
            }
            let slots = slots
                .into_iter()
                .map(|v| reader.reference(v))
                .collect::<Result<Vec<_>, _>>()?;
            let mut parsed = Vec::with_capacity(nodes.len());
            let mut remaining = graph::MAX_FIELDS;
            for node in nodes {
                let fields = reader.vector(node, 3)?;
                let Some(&kind) = fields.first() else {
                    return Err(invalid());
                };
                parsed.push(match (reader.integer(kind)?, fields.as_slice()) {
                    (0, [_, value]) => Node::Integer(reader.integer(*value)?),
                    (1, [_, value]) => Node::FloatBits(
                        s.send(s.float_factory()?, b"bitsOf:", &[Operand::object(*value)])?
                            .as_word()
                            .ok_or_else(invalid)? as u64,
                    ),
                    (2, [_, behavior, values]) => {
                        let behavior =
                            u32::try_from(reader.integer(*behavior)?).map_err(|_| invalid())?;
                        let values = reader.vector(*values, remaining)?;
                        remaining -= values.len();
                        Node::Aggregate {
                            vtable_input: behavior,
                            fields: values
                                .into_iter()
                                .map(|v| reader.reference(v))
                                .collect::<Result<_, _>>()?,
                        }
                    }
                    _ => return Err(invalid()),
                });
            }
            let plan = Graph {
                input_count: inputs.len(),
                nodes: parsed,
                slots,
            }
            .validate()
            .map_err(|_| invalid())?;
            materialize(s, cap, pc, &plan, &inputs)?;
        }
        Ok(Operand::object(machine))
    }
}
fn resolve(value: Value, inputs: &[Oop], nodes: &[Oop]) -> Oop {
    match value {
        Value::Input(i) => inputs[i as usize],
        Value::Node(i) => nodes[i as usize],
    }
}
unsafe fn validate_storage(s: &Execution, plan: &Plan, inputs: &[Oop]) -> Result<(), Outcome> {
    unsafe {
        if inputs.len() != plan.input_count() {
            return Err(invalid());
        }
        for node in plan.nodes() {
            if let Node::Aggregate {
                vtable_input,
                fields,
            } = node
            {
                let behavior = inputs[*vtable_input as usize];
                if s.send(
                    s.model,
                    b"allOopLayoutOf:words:",
                    &[
                        Operand::object(behavior),
                        Operand::word(fields.len().max(1)),
                    ],
                )?
                .as_word()
                    != Some(1)
                    || s.send(s.gc, b"referenceOwnerOf:", &[Operand::object(behavior)])?
                        .as_object()
                        != Some(s.model)
                {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
}
unsafe fn materialize(
    s: &Execution,
    cap: MachineCapability,
    pc: usize,
    plan: &Plan,
    inputs: &[Oop],
) -> Result<(), Outcome> {
    unsafe {
        let roots = s.scope(inputs);
        let mut nodes = Vec::with_capacity(plan.nodes().len());
        authority(s, cap, pc)?;
        validate_storage(s, plan, inputs)?;
        for node in plan.nodes() {
            s.send(s.gc, b"prepareAllocation", &[])?;
            validate_storage(s, plan, inputs)?;
            authority(s, cap, pc)?;
            let value = match node {
                Node::Integer(value) => {
                    object(s.send(s.integer_factory()?, b"fromWord:", &[Operand::word(*value as usize)])?)?
                }
                Node::FloatBits(value) => {
                    object(s.send(s.float_factory()?, b"fromBits:", &[Operand::word(*value as usize)])?)?
                }
                Node::Aggregate {
                    vtable_input,
                    fields,
                } => {
                    let behavior = inputs[*vtable_input as usize];
                    let value = object(s.send(
                        s.model,
                        b"allocate:bytes:",
                        &[
                            Operand::object(behavior),
                            Operand::word(fields.len() * size_of::<usize>()),
                        ],
                    )?)?;
                    roots.keep(value);
                    validate_storage(s, plan, inputs)?;
                    authority(s, cap, pc)?;
                    for (i, field) in fields.iter().enumerate() {
                        s.send(
                            s.model,
                            b"slotAt:in:put:",
                            &[
                                Operand::word(i),
                                Operand::object(value),
                                Operand::object(resolve(*field, inputs, &nodes)),
                            ],
                        )?;
                        validate_storage(s, plan, inputs)?;
                        authority(s, cap, pc)?;
                    }
                    value
                }
            };
            roots.keep(value);
            nodes.push(value);
            validate_storage(s, plan, inputs)?;
            authority(s, cap, pc)?;
        }
        let slots: Vec<_> = plan
            .slots()
            .iter()
            .map(|v| resolve(*v, inputs, &nodes))
            .collect();
        validate_storage(s, plan, inputs)?;
        publish(s, cap, pc, &slots)
    }
}
