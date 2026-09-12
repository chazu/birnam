//! Private clone fields with physical recovery before the next observation.
use super::*;
use crate::execution::{DecodedAction, ExecutionError};
use crate::heap::{DeadAllocation, DeferredAllocation};
use crate::loader::FieldSummary;
use crate::raw::MethodAddress;
use crate::runtime::current_thread_token;
use birnam_codegen::virtual_aggregate::{Exit, Plan, Value};
use core::cell::RefCell;

#[cfg(test)]
thread_local! {
    pub(super) static ENABLED: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
    pub(super) static FORCE_EXIT: core::cell::Cell<Option<usize>> = const { core::cell::Cell::new(None) };
    pub(super) static HITS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    pub(super) static MATERIALIZED: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

pub(super) struct Compiled {
    plan: Rc<Plan>,
    fields: RefCell<Vec<(usize, Option<Rc<FieldSummary>>)>>,
}
impl Compiled {
    pub(super) fn new(plan: Rc<Plan>) -> Self {
        Self {
            plan,
            fields: RefCell::new(Vec::new()),
        }
    }
    fn field(&self, method: Method, nil: Oop) -> Option<(Rc<FieldSummary>, usize)> {
        let cached = self
            .fields
            .borrow()
            .iter()
            .find(|(address, _)| *address == method as usize)
            .map(|(_, value)| value.clone());
        let summary = if let Some(value) = cached {
            value
        } else {
            // No cache or RuntimeState borrow crosses the pure native query.
            let value = FieldSummary::discover(method, nil).map(Rc::new);
            let mut fields = self.fields.borrow_mut();
            if fields.len() == 32 {
                fields.clear();
            }
            fields.push((method as usize, value.clone()));
            value
        }?;
        let field = summary.field(nil)?;
        Some((summary, field))
    }
}

#[derive(Clone, Copy)]
enum Computed {
    Real(Oop),
    Virtual(usize),
}
enum Operation {
    Clone(usize),
    Projection(Oop),
}
struct Node {
    prototype: Oop,
    snapshot: Rc<DeadAllocation>,
}

unsafe fn invoke(
    candidate: &Candidate,
    runtime: &Runtime,
    machine: Oop,
    key: usize,
    pc: usize,
    args: *const Oop,
) -> Oop {
    let regions_enabled = true;
    #[cfg(test)]
    let regions_enabled = regions_enabled && super::regions::ENABLED.get();
    let method = candidate.entries[pc];
    if regions_enabled && let Some(Some(region)) = candidate.regions.get(pc) {
        unsafe { region.invoke(runtime, machine, key, method, args, 1) }
    } else {
        unsafe { method(machine, args, 1) }
    }
}

pub(super) unsafe fn execute(
    runtime: &Runtime,
    machine: Oop,
    key: usize,
    candidate: &Rc<Candidate>,
    compiled: &Compiled,
    entry_args: *const Oop,
) -> Option<Result<Oop, ExecutionError>> {
    #[cfg(not(test))]
    let enabled = false;
    #[cfg(test)]
    let enabled = ENABLED.get();
    if !enabled || !fused_numeric::controls(runtime, machine) {
        return None;
    }
    let owner = current_thread_token();
    let capability = runtime.stackless_machine_capability(machine).ok()?;
    let plan = &compiled.plan;
    let (frame, inputs) = {
        let state = unsafe { &*runtime.state.get() };
        let arena = state.execution.machine(owner, capability).ok()?.arena();
        let frame = arena.current_handle()?;
        if arena.program_counter(frame).ok()? != 0
            || frame.words().ok()?[2] == usize::MAX
            || arena.descriptor(frame).ok()? as *const FrameDescriptor as usize != key
        {
            return None;
        }
        let inputs = (0..plan.slot_count)
            .map(|slot| arena.read_oop(frame, slot))
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        (frame, inputs)
    };
    let nil = runtime.canonical().nil;
    let mut results = Vec::new();
    let mut operations = Vec::new();
    let mut nodes: Vec<Node> = Vec::new();
    let mut summaries = Vec::new();
    let mut projections = 0;
    for step in &plan.steps {
        let Exit::Call {
            receiver,
            selector,
            arguments,
            ..
        } = &step.exit
        else {
            break;
        };
        if !arguments.is_empty() {
            break;
        }
        let receiver = match *receiver {
            Value::Input(slot) => Computed::Real(*inputs.get(slot)?),
            Value::Result(index) => *results.get(index)?,
        };
        let prototype = match receiver {
            Computed::Real(object) => object,
            Computed::Virtual(index) => nodes[index].prototype,
        };
        let method = {
            let state = unsafe { &*runtime.state.get() };
            let vt = state.heap.vtable_of(prototype)?;
            if state.heap.vtable_of(vt) != Some(state.globals.vtable_vt) {
                break;
            }
            let symbol = state.symbols.existing(selector)?;
            let method = state.vtables.lookup(vt, symbol)?;
            if state.execution.has_resumable_thunk(method) {
                break;
            }
            method
        };
        if method == MethodAddress::from_method(crate::dispatch::object_clone) {
            let snapshot = if let Computed::Virtual(index) = receiver {
                nodes[index].snapshot.clone()
            } else {
                let state = unsafe { &*runtime.state.get() };
                let vt = state.heap.vtable_of(prototype)?;
                if state.heap.has_finalizer(prototype) != Some(false)
                    || !state.vtables.has_layout(vt)
                    || state.vtables.trace_hook(vt).is_some()
                    || state.vtables.slot_kind(vt, 0) != crate::abi::SLOT_OOP
                {
                    break;
                }
                let snapshot = state.heap.prepare_dead_clone(prototype)?;
                let payload = state.heap.payload_size(prototype)?;
                if !payload.is_multiple_of(core::mem::size_of::<Oop>())
                    || (0..payload / core::mem::size_of::<Oop>())
                        .any(|i| state.vtables.slot_kind(vt, i) != crate::abi::SLOT_OOP)
                {
                    break;
                }
                snapshot
            };
            results.push(Computed::Virtual(nodes.len()));
            operations.push(Operation::Clone(nodes.len()));
            nodes.push(Node {
                prototype,
                snapshot,
            });
        } else {
            let Computed::Virtual(index) = receiver else {
                break;
            };
            let Some((summary, field)) = compiled.field(method.to_method(), nil) else {
                break;
            };
            let value = unsafe {
                (*runtime.state.get())
                    .heap
                    .read_payload_word(nodes[index].prototype, field)
            }
            .map(core::ptr::with_exposed_provenance_mut)?;
            summaries.push(summary);
            projections += 1;
            operations.push(Operation::Projection(value));
            results.push(Computed::Real(value));
        }
    }
    if nodes.is_empty() || projections == 0 {
        return None;
    }
    // Test exits use the same admitted region, but recover before an earlier
    // Call. In particular, an exit after the first clone must really recover
    // that identity, rather than simply refusing for lack of a projection.
    #[cfg(test)]
    if let Some(stop) = FORCE_EXIT.get() {
        assert!(stop > 0);
        operations.truncate(stop);
    }
    let stop = operations.len();
    let (clone_count, clone_bytes) = operations.iter().try_fold(
        (0usize, 0usize),
        |(count, bytes), operation| match operation {
            Operation::Clone(index) => Some((
                count + 1,
                bytes.checked_add(nodes[*index].snapshot.total_size())?,
            )),
            Operation::Projection(_) => Some((count, bytes)),
        },
    )?;
    let mut boxes = plan.steps[..stop]
        .iter()
        .map(|step| step.integer_boxes)
        .sum::<usize>();
    let boundary = plan.steps.get(stop)?;
    // A canonical Return can finish without materializing dead virtual values,
    // but then its original token/bind/publication boxes must also exclude GC.
    if matches!(boundary.exit, Exit::Return(_)) {
        boxes += boundary.integer_boxes;
    }
    let budget = crate::heap::Heap::allocation_total_size(8)?
        .checked_mul(boxes)?
        .checked_add(clone_bytes)?;
    if !fused_numeric::controls(runtime, machine) {
        return None;
    }
    {
        let state = unsafe { &*runtime.state.get() };
        if !state.heap.admits_virtual_allocations(clone_count)
            || state
                .metadata
                .automatic_collection_due(state.heap.bytes_since_collection().checked_add(budget)?)
        {
            return None;
        }
    }
    // All declarations are pure, all current binding reads precede this body,
    // and the complete budget excludes observation until physical recovery.
    Some((|| {
        let mut identities: Vec<Option<DeferredAllocation>> = vec![None; nodes.len()];
        for (pc, operation) in operations.iter().enumerate() {
            let token;
            let args = if pc == 0 {
                entry_args
            } else {
                token = runtime.make_integer(pc as i64);
                core::ptr::from_ref(&token)
            };
            let action = unsafe { invoke(candidate, runtime, machine, key, pc, args) };
            let decoded = unsafe {
                let state = &*runtime.state.get();
                state
                    .execution
                    .decode_action(&state.heap, owner, capability, action)?
            };
            let DecodedAction::Call { result_slot, .. } = decoded else {
                return Err(ExecutionError::InvalidAction);
            };
            let result = {
                let _boundary = runtime.enter_stackless_boundary(owner, capability)?;
                match operation {
                    Operation::Clone(index) => {
                        identities[*index] = Some(
                            unsafe {
                                (*runtime.state.get())
                                    .heap
                                    .record_virtual_clone(nodes[*index].snapshot.clone())
                            }
                            .ok_or(ExecutionError::InvalidAction)?,
                        );
                        nil
                    }
                    Operation::Projection(value) => *value,
                }
            };
            unsafe {
                (*runtime.state.get())
                    .execution
                    .commit_current_call_result(owner, capability, result_slot, nil, result)?;
            }
        }
        let materialize = match boundary.exit {
            Exit::Return(Value::Input(_)) => false,
            Exit::Return(Value::Result(index)) => matches!(results[index], Computed::Virtual(_)),
            _ => true,
        };
        #[cfg(test)]
        let materialize = materialize || FORCE_EXIT.get().is_some();
        if materialize {
            let mut physical = vec![None; nodes.len()];
            for (slot, result) in boundary.before.iter().enumerate() {
                let Some(result) = result else {
                    continue;
                };
                let Computed::Virtual(index) = results[*result] else {
                    continue;
                };
                let value = if let Some(value) = physical[index] {
                    value
                } else {
                    let value = unsafe {
                        (*runtime.state.get()).heap.materialize_recorded_clone(
                            identities[index].ok_or(ExecutionError::InvalidAction)?,
                        )
                    }
                    .ok_or(ExecutionError::InvalidAction)?;
                    physical[index] = Some(value);
                    #[cfg(test)]
                    MATERIALIZED.set(MATERIALIZED.get() + 1);
                    value
                };
                unsafe {
                    (*runtime.state.get())
                        .execution
                        .machine_mut(owner, capability)?
                }
                .arena_mut()
                .write_oop(frame, slot, value)?;
            }
        }
        #[cfg(test)]
        HITS.set(HITS.get() + clone_count);
        // These original token/PC operations can now observe or collect: every
        // virtual identity they could expose is an ordinary rooted Frame value.
        let token = runtime.make_integer(stop as i64);
        let action = unsafe {
            invoke(
                candidate,
                runtime,
                machine,
                key,
                stop,
                core::ptr::from_ref(&token),
            )
        };
        drop(summaries);
        Ok(action)
    })())
}
