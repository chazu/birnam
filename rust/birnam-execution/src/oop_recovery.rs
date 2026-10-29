//! Pending-call recovery through Oop callbacks and current representation owners.
use super::*;
use crate::execution::recovery::RecoveryAuthority;
use crate::recovery_graph::{self as graph, Graph, Node, Plan, Value};

#[derive(Clone, Copy)]
pub(super) struct Inline {
    pub method: Oop,
    pub owner: Oop,
    return_slot: usize,
}
pub(super) const CONTROLS: &[&[u8]] =
    &[b"oopAt:", b"oopAt:put:", b"returnAt:", b"currentFrameView"];
#[derive(Default)]
pub(super) struct Runtime {
    requests: Vec<Request>,
}
struct Request {
    authority: RecoveryAuthority,
    consumed: bool,
}
struct RequestGuard<'a> {
    s: &'a State,
    returned_to_driver: bool,
}
impl Drop for RequestGuard<'_> {
    fn drop(&mut self) {
        let request = self
            .s
            .resume
            .borrow_mut()
            .recovery
            .requests
            .pop()
            .expect("scoped recovery request");
        if !request.consumed && !self.returned_to_driver {
            self.s
                .engine
                .borrow_mut()
                .release_acquired_entry(request.authority.entry)
                .expect("request owns acquisition");
        }
    }
}
fn authority(s: &State, cap: MachineCapability, pc: usize) -> Result<RecoveryAuthority, Oop> {
    let r = s.resume.borrow();
    let request = r
        .recovery
        .requests
        .last()
        .filter(|r| !r.consumed && r.authority.machine == cap)
        .ok_or_else(invalid)?;
    if pc >= request.authority.descriptor.program_counter_count() {
        return Err(invalid());
    }
    checked(
        s.engine
            .borrow()
            .validate_recovery_authority(OWNER, &request.authority),
    )?;
    Ok(request.authority.clone())
}
fn publish(s: &State, cap: MachineCapability, pc: usize, values: &[Oop]) -> Result<(), Oop> {
    let a = authority(s, cap, pc)?;
    if values.len() != a.descriptor.slot_count() {
        return Err(invalid());
    }
    let slots: Vec<_> = (0..values.len()).map(|i| (i, i)).collect();
    checked(
        s.engine
            .borrow_mut()
            .recover_acquired_call(OWNER, &a, pc, &slots, values, s.nil),
    )?;
    s.resume
        .borrow_mut()
        .recovery
        .requests
        .last_mut()
        .unwrap()
        .consumed = true;
    Ok(())
}

pub(in crate::synchronous) unsafe extern "C" fn with_inline(
    _: Oop,
    c: Oop,
    receiver: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [method, slot, owner] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        if owner.is_null() {
            return Err(invalid());
        }
        let q = s.scope(&[receiver, *method, *slot, *owner]);
        let return_slot = index(s, *slot)?;
        // Captured aliases share the declared entry/context. Only Id interprets
        // Method representation; these records belong solely to Execution.
        let records: Vec<_> = s.resume.borrow().records.values().cloned().collect();
        for record in &records {
            q.keep(record.method);
        }
        let mut selected = None;
        for record in records {
            if abi::as_size(s.send(
                s.id.root,
                b"sameNativeMethod:as:",
                &[receiver, record.method],
            )?) == Some(1)
            {
                selected = Some(record);
                break;
            }
        }
        let selected = selected.ok_or_else(invalid)?;
        if selected.descriptor.slot_kind(return_slot) != Some(SlotKind::Oop)
            || abi::as_size(s.send(s.id.root, b"sameNativeMethod:as:", &[*method, *method])?)
                != Some(1)
        {
            return Err(invalid());
        }
        let behavior = q.keep(s.send(s.id.root, b"behaviorOf:", &[receiver])?);
        if behavior != s.send(s.id.root, b"methodBehavior", &[])? {
            // Optional optimization must not mutate a fresh Method to copy
            // custom behavior, or silently discard that behavior.
            return Ok(receiver);
        }
        let context_vt = s.resume.borrow().context_vt;
        let context = q.keep(allocate(s, context_vt, 0)?);
        // Preserve the selected entry, including DNU argument handling.
        let copy = q.keep(s.send(receiver, b"withContext:", &[context])?);
        // This Method annotates the existing baseline; it does not create a
        // second registration or a frame owner containing the candidate recipe.
        let mut annotated = selected;
        annotated.method = copy;
        annotated.candidate = None;
        annotated.inline = Some(Inline {
            method: *method,
            owner: *owner,
            return_slot,
        });
        s.resume.borrow_mut().records.insert(context.addr(), annotated);
        let admitted = s.admitted.borrow().get(&receiver.addr()).cloned();
        if let Some(admitted) = admitted {
            s.admitted.borrow_mut().insert(copy.addr(), admitted);
        }
        Ok(copy)
    })()
    .unwrap_or_else(abi::fault_reference)
}

/// False leaves the acquisition with the ordinary driver. True or failure
/// consumes it, either by release or by transfer to the recovered frame.
pub(super) unsafe fn try_inline(
    s: &State,
    site: &ReturnSite,
    capability: MachineCapability,
    entry: ResumableEntryCapability,
    inline: Inline,
    receiver: Oop,
    args: &[Oop],
) -> Result<bool, Oop> {
    unsafe {
        let admitted = (|| {
            let eligible = {
                let e = s.engine.borrow();
                checked(e.machine(OWNER, capability))?.control_eligible()
                    && e.inline_return_storage_valid(&s.storage.borrow(), OWNER, capability)
            };
            if !eligible {
                return Ok(None);
            }
            if !controls_current(s, site.machine, CONTROLS)? {
                return Ok(None);
            }
            checked(s.engine.borrow().recovery_authority(
                OWNER,
                capability,
                entry,
                site.result_slot,
            ))
            .map(Some)
        })();
        let a = match admitted {
            Ok(Some(a)) => a,
            Ok(None) => return Ok(false),
            Err(e) => {
                checked(s.engine.borrow_mut().release_acquired_entry(entry))?;
                return Err(e);
            }
        };
        s.resume.borrow_mut().recovery.requests.push(Request {
            authority: a,
            consumed: false,
        });
        let mut guard = RequestGuard {
            s,
            returned_to_driver: false,
        };
        let q = s.scope(&[site.machine, inline.method, inline.owner, receiver]);
        for &arg in args {
            q.keep(arg);
        }
        let mut arguments = Vec::with_capacity(args.len() + 1);
        arguments.push(receiver);
        arguments.extend_from_slice(args);
        s.resume.borrow_mut().active.push(capability);
        let active = Active(s, capability);
        let answer = q.keep(s.invoke(inline.method, site.machine, &arguments)?);
        drop(active);
        if s.resume.borrow().recovery.requests.last().unwrap().consumed {
            return if answer.is_null() {
                Ok(true)
            } else {
                Err(invalid())
            };
        }
        authority(s, capability, 0)?;
        if answer.is_null() {
            guard.returned_to_driver = true;
            return Ok(false);
        }
        checked(s.engine.borrow_mut().commit_inlined_return(
            &mut s.storage.borrow_mut(),
            OWNER,
            capability,
            site.result_slot,
            inline.return_slot,
            s.nil,
            answer,
        ))?;
        Ok(true)
    }
}

struct Reader<'a> {
    s: &'a State,
    roots: Scope<'a>,
}
impl Reader<'_> {
    unsafe fn bits(&self, value: Oop) -> Result<u64, Oop> {
        unsafe {
            self.roots.keep(value);
            if let Some(v) = abi::as_size(value) {
                return Ok(v as u64);
            }
            let lo = abi::as_size(self.s.send(value, b"low32", &[])?).ok_or_else(invalid)?;
            let hi = abi::as_size(self.s.send(value, b"high32", &[])?).ok_or_else(invalid)?;
            if lo > u32::MAX as usize || hi > u32::MAX as usize {
                return Err(invalid());
            }
            Ok(lo as u64 | ((hi as u64) << 32))
        }
    }
    unsafe fn integer(&self, value: Oop) -> Result<i64, Oop> {
        unsafe {
            self.roots.keep(value);
            self.bits(
                self.s
                    .send(self.s.named(b"Integer")?, b"wordOf:", &[value])?,
            )
            .map(|v| v as i64)
        }
    }
    unsafe fn reference(&self, value: Oop) -> Result<Value, Oop> {
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
    unsafe fn vector(&self, value: Oop, limit: usize) -> Result<Vec<Oop>, Oop> {
        unsafe {
            self.roots.keep(value);
            let count = self.roots.keep(self.s.send(value, b"size", &[])?);
            let count = usize::try_from(self.integer(count)?).map_err(|_| invalid())?;
            if count > limit {
                return Err(invalid());
            }
            let mut values = Vec::with_capacity(count);
            for i in 0..count {
                let i = self.roots.keep(number(self.s, i)?);
                values.push(self.roots.keep(self.s.send(value, b"at:", &[i])?));
            }
            Ok(values)
        }
    }
}
pub(in crate::synchronous) unsafe extern "C" fn recover<const GRAPH: bool>(
    _: Oop,
    c: Oop,
    machine: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [pc, recipe] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let reader = Reader {
            s,
            roots: s.scope(&[machine, *pc, *recipe]),
        };
        let pc = index(s, *pc)?;
        let capability = cap(s, machine)?;
        let a = authority(s, capability, pc)?;
        if !GRAPH {
            let values = reader.vector(*recipe, a.descriptor.slot_count())?;
            publish(s, capability, pc, &values)?;
        } else {
            let parts = reader.vector(*recipe, 3)?;
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
                    (1, [_, value]) => Node::FloatBits(reader.bits(s.send(
                        s.named(b"Float")?,
                        b"bitsOf:",
                        &[*value],
                    )?)?),
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
            materialize(s, capability, pc, &plan, &inputs)?;
        }
        Ok(machine)
    })()
    .unwrap_or_else(abi::fault_reference)
}
fn resolve(value: Value, inputs: &[Oop], nodes: &[Oop]) -> Oop {
    match value {
        Value::Input(i) => inputs[i as usize],
        Value::Node(i) => nodes[i as usize],
    }
}
unsafe fn validate_storage(s: &State, plan: &Plan, inputs: &[Oop]) -> Result<(), Oop> {
    unsafe {
        if inputs.len() != plan.input_count() {
            return Err(invalid());
        }
        let q = s.scope(inputs);
        let model = q.keep(s.named(b"ObjectModel")?);
        for node in plan.nodes() {
            if let Node::Aggregate {
                vtable_input,
                fields,
            } = node
            {
                let behavior = inputs[*vtable_input as usize];
                if abi::as_size(s.send(
                    model,
                    b"allOopLayoutOf:words:",
                    &[behavior, abi::size(fields.len().max(1)).unwrap()],
                )?) != Some(1)
                    || s.send(s.gc, b"referenceOwnerOf:", &[behavior])? != model
                {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
}
unsafe fn word(s: &State, bits: u64) -> Result<Oop, Oop> {
    unsafe {
        s.send(
            s.named(b"Word")?,
            b"fromUnsignedLow:high:",
            &[
                abi::size(bits as u32 as usize).unwrap(),
                abi::size((bits >> 32) as usize).unwrap(),
            ],
        )
    }
}
unsafe fn materialize(
    s: &State,
    cap: MachineCapability,
    pc: usize,
    plan: &Plan,
    inputs: &[Oop],
) -> Result<(), Oop> {
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
                    let bits = roots.keep(word(s, *value as u64)?);
                    validate_storage(s, plan, inputs)?;
                    authority(s, cap, pc)?;
                    s.send(s.named(b"Integer")?, b"fromWord:", &[bits])?
                }
                Node::FloatBits(value) => {
                    let bits = roots.keep(word(s, *value)?);
                    validate_storage(s, plan, inputs)?;
                    authority(s, cap, pc)?;
                    s.send(s.named(b"Float")?, b"fromBits:", &[bits])?
                }
                Node::Aggregate {
                    vtable_input,
                    fields,
                } => {
                    let model = roots.keep(s.named(b"ObjectModel")?);
                    let value = roots.keep(s.send(
                        model,
                        b"allocate:bytes:",
                        &[
                            inputs[*vtable_input as usize],
                            abi::size(fields.len() * size_of::<Oop>()).unwrap(),
                        ],
                    )?);
                    validate_storage(s, plan, inputs)?;
                    authority(s, cap, pc)?;
                    for (i, field) in fields.iter().enumerate() {
                        s.send(
                            model,
                            b"slotAt:in:put:",
                            &[
                                abi::size(i).unwrap(),
                                value,
                                resolve(*field, inputs, &nodes),
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
