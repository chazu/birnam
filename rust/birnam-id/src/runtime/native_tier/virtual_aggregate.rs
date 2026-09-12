//! Private clone fields with physical recovery before the next observation.
use super::*;
use crate::execution::{DecodedAction, ExecutionError};
use crate::heap::{DeadAllocation, DeferredAllocation};
use crate::loader::FieldSummary;
use crate::raw::MethodAddress;
use crate::runtime::current_thread_token;
use birnam_codegen::virtual_aggregate::{Exit, Plan, Value};
use core::cell::{Cell, RefCell};

pub(super) const MIN_PROJECTIONS: usize = 3;
pub(super) const RETRY_INTERVAL: usize = 64;

pub(super) fn prepare(
    program: &resumable::LogicalProgram<'_>,
    layout: &resumable::PhysicalLayout,
) -> Option<Plan> {
    // One clone and the required projections each need their own Call PC,
    // followed by a continuation. Reject shorter programs before allocating
    // a symbolic plan or physical-slot maps.
    if program.blocks.len() <= MIN_PROJECTIONS + 1 {
        return None;
    }
    Plan::derive(program, layout).filter(worthwhile)
}

fn worthwhile(plan: &Plan) -> bool {
    plan.steps
        .iter()
        .take_while(
            |step| matches!(&step.exit, Exit::Call { arguments, .. } if arguments.is_empty()),
        )
        .count()
        > MIN_PROJECTIONS
}

#[cfg(test)]
thread_local! {
    pub(super) static ENABLED: core::cell::Cell<bool> = const { core::cell::Cell::new(true) };
    pub(super) static FORCE_EXIT: core::cell::Cell<Option<usize>> = const { core::cell::Cell::new(None) };
    pub(super) static SCRATCH_MISSES: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    pub(super) static ATTEMPTS: Cell<usize> = const { Cell::new(0) };
    pub(super) static RETRY_SKIPS: Cell<usize> = const { Cell::new(0) };
    pub(super) static HITS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    pub(super) static MATERIALIZED: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

pub(super) struct Compiled {
    plan: Rc<Plan>,
    fields: RefCell<Vec<(usize, Rc<FieldSummary>)>>,
    scratch: RefCell<Option<Box<Scratch>>>,
    retry: Cell<(u64, usize)>,
}
impl Compiled {
    pub(super) fn new(plan: Rc<Plan>) -> Self {
        Self {
            plan,
            fields: RefCell::new(Vec::new()),
            scratch: RefCell::new(None),
            retry: Cell::new((0, 0)),
        }
    }
    /// Retained field-plan/cache storage, excluding allocator and loader costs.
    #[cfg(test)]
    pub(super) fn storage_bytes(&self) -> usize {
        use core::mem::size_of;
        let fields = self.fields.borrow();
        size_of::<Self>()
            + self
                .scratch
                .borrow()
                .as_ref()
                .map_or(0, |scratch| size_of::<Scratch>() + scratch.storage_bytes())
            + 2 * size_of::<usize>()
            + size_of::<Plan>()
            + self.plan.steps.capacity() * size_of::<birnam_codegen::virtual_aggregate::Step>()
            + self
                .plan
                .steps
                .iter()
                .map(|step| {
                    step.before.capacity() * size_of::<Option<usize>>()
                        + match &step.exit {
                            Exit::Call {
                                selector,
                                arguments,
                                ..
                            } => selector.capacity() + arguments.capacity() * size_of::<Value>(),
                            _ => 0,
                        }
                })
                .sum::<usize>()
            + fields.capacity() * size_of::<(usize, Rc<FieldSummary>)>()
            + fields.len() * (2 * size_of::<usize>() + size_of::<FieldSummary>())
    }
    fn acquire_scratch(&self) -> ScratchLease<'_> {
        let scratch = self.scratch.borrow_mut().take().unwrap_or_else(|| {
            #[cfg(test)]
            SCRATCH_MISSES.set(SCRATCH_MISSES.get() + 1);
            Box::default()
        });
        ScratchLease {
            scratch: Some(scratch),
            pool: &self.scratch,
        }
    }
    #[cfg(test)]
    pub(super) fn with_held_scratch(&self, value: Oop, body: impl FnOnce() -> Oop) -> Oop {
        let mut held = self.acquire_scratch();
        held.scratch.as_mut().unwrap().inputs.push(value);
        let answer = body();
        assert_eq!(
            held.scratch.as_ref().unwrap().inputs,
            [value],
            "live checkout survives collecting reentry"
        );
        answer
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
            // Do not permanently cache an absent declaration: a later retry
            // must rediscover declarations that become available without an
            // Id binding mutation.
            let value = FieldSummary::discover(method, nil).map(Rc::new)?;
            let mut fields = self.fields.borrow_mut();
            if fields.len() == 32 {
                fields.clear();
            }
            fields.push((method as usize, value.clone()));
            value
        };
        let field = summary.field(nil)?;
        Some((summary, field))
    }
}

#[derive(Clone, Copy)]
enum Computed {
    Real(Oop),
    Virtual(usize),
}
#[derive(Clone, Copy)]
enum Operation {
    Clone(usize),
    Projection(Oop),
}

/// Only vector headers live on the native stack. Payload Oops are untraced.
#[derive(Default)]
struct Scratch {
    inputs: Vec<Oop>,
    results: Vec<Computed>,
    operations: Vec<Operation>,
    nodes: Vec<Oop>,
    summaries: Vec<Rc<FieldSummary>>,
    snapshots: Vec<Option<Rc<DeadAllocation>>>,
    identities: Vec<Option<DeferredAllocation>>,
    physical: Vec<Option<Oop>>,
}
impl Scratch {
    fn clear(&mut self) {
        self.inputs.clear();
        self.results.clear();
        self.operations.clear();
        self.nodes.clear();
        self.summaries.clear();
        self.snapshots.clear();
        self.identities.clear();
        self.physical.clear();
    }
    #[cfg(test)]
    fn storage_bytes(&self) -> usize {
        use core::mem::size_of;
        self.inputs.capacity() * size_of::<Oop>()
            + self.results.capacity() * size_of::<Computed>()
            + self.operations.capacity() * size_of::<Operation>()
            + self.nodes.capacity() * size_of::<Oop>()
            + self.summaries.capacity() * size_of::<Rc<FieldSummary>>()
            + self.snapshots.capacity() * size_of::<Option<Rc<DeadAllocation>>>()
            + self.identities.capacity() * size_of::<Option<DeferredAllocation>>()
            + self.physical.capacity() * size_of::<Option<Oop>>()
    }
}
/// An invocation owns its buffers through the observing boundary. Reentry can
/// obtain another set without sharing live values or holding a RefCell borrow.
/// At most one cleared set remains cached per compiled candidate.
struct ScratchLease<'a> {
    scratch: Option<Box<Scratch>>,
    pool: &'a RefCell<Option<Box<Scratch>>>,
}
impl Drop for ScratchLease<'_> {
    fn drop(&mut self) {
        // Release pins/snapshots before borrowing the pool, including on refusal
        // or execution error. No stale values are part of a later invocation.
        self.scratch.as_mut().unwrap().clear();
        let mut pool = self.pool.borrow_mut();
        if pool.is_none() {
            *pool = self.scratch.take();
        }
    }
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
    let enabled = true;
    #[cfg(test)]
    let enabled = ENABLED.get();
    if !enabled {
        return None;
    }
    let epoch = unsafe { (*runtime.state.get()).vtables.mutation_epoch() };
    let (failed_epoch, remaining) = compiled.retry.get();
    if epoch != u64::MAX && epoch == failed_epoch && remaining != 0 {
        compiled.retry.set((epoch, remaining - 1));
        #[cfg(test)]
        RETRY_SKIPS.set(RETRY_SKIPS.get() + 1);
        return None;
    }
    #[cfg(test)]
    ATTEMPTS.set(ATTEMPTS.get() + 1);
    let result = unsafe { execute_now(runtime, machine, key, candidate, compiled, entry_args) };
    // Only a refusal skips future attempts. Successful execution and execution
    // errors retain normal driver handling. A binding epoch change retries at
    // once; pure declaration changes without an epoch are reconsidered within
    // RETRY_INTERVAL invocations. Every skipped attempt runs ordinary Id code.
    compiled.retry.set((
        epoch,
        if result.is_none() {
            RETRY_INTERVAL - 1
        } else {
            0
        },
    ));
    result
}

unsafe fn execute_now(
    runtime: &Runtime,
    machine: Oop,
    key: usize,
    candidate: &Rc<Candidate>,
    compiled: &Compiled,
    entry_args: *const Oop,
) -> Option<Result<Oop, ExecutionError>> {
    let owner = current_thread_token();
    let capability = runtime.stackless_machine_capability(machine).ok()?;
    let plan = &compiled.plan;
    let nil = runtime.canonical().nil;
    let mut lease = compiled.acquire_scratch();
    let Scratch {
        inputs,
        results,
        operations,
        nodes,
        summaries,
        snapshots,
        identities,
        physical,
    } = lease.scratch.as_deref_mut().unwrap();
    let frame = {
        let state = unsafe { &*runtime.state.get() };
        let arena = state.execution.machine(owner, capability).ok()?.arena();
        let frame = arena.current_handle()?;
        if arena.program_counter(frame).ok()? != 0
            || frame.words().ok()?[2] == usize::MAX
            || arena.descriptor(frame).ok()? as *const FrameDescriptor as usize != key
        {
            return None;
        }
        inputs.resize(plan.slot_count, nil);
        for (slot, value) in inputs.iter_mut().enumerate() {
            *value = arena.read_oop(frame, slot).ok()?;
        }
        frame
    };
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
            Computed::Virtual(index) => nodes[index],
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
            // Defer the storage proof and snapshot until at least one field
            // declaration admits the region. Rejected declarations need neither.
            results.push(Computed::Virtual(nodes.len()));
            operations.push(Operation::Clone(nodes.len()));
            nodes.push(prototype);
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
                    .read_payload_word(nodes[index], field)
            }
            .map(core::ptr::with_exposed_provenance_mut)?;
            summaries.push(summary);
            operations.push(Operation::Projection(value));
            results.push(Computed::Real(value));
        }
    }
    if nodes.is_empty() || summaries.len() < MIN_PROJECTIONS {
        return None;
    }
    // Pure preflight reads may have inspected payload bits, but no result is
    // executed or published until every cloned payload has passed this proof.
    snapshots.resize(nodes.len(), None);
    {
        let state = unsafe { &*runtime.state.get() };
        for index in 0..nodes.len() {
            let prototype = nodes[index];
            if let Some(previous) = nodes[..index].iter().position(|&value| value == prototype) {
                snapshots[index] = snapshots[previous].clone();
                continue;
            }
            let vt = state.heap.vtable_of(prototype)?;
            let payload = state.heap.payload_size(prototype)?;
            if payload > crate::heap::MAX_DEAD_PAYLOAD_BYTES
                || !payload.is_multiple_of(core::mem::size_of::<Oop>())
                || state.heap.has_finalizer(prototype) != Some(false)
                || state.vtables.trace_hook(vt).is_some()
                || !state
                    .vtables
                    .all_oop_layout(vt, (payload / core::mem::size_of::<Oop>()).max(1))
            {
                return None;
            }
            snapshots[index] = Some(state.heap.prepare_dead_clone(prototype)?);
        }
    }
    // Test exits use the same admitted region, but recover before an earlier
    // Call. In particular, an exit after the first clone must really recover
    // that identity, rather than simply refusing for lack of a projection.
    #[cfg(test)]
    if let Some(forced) = FORCE_EXIT.get() {
        assert!(forced > 0);
        operations.truncate(forced);
    }
    let stop = operations.len();
    let (clone_count, clone_bytes) = operations.iter().try_fold(
        (0usize, 0usize),
        |(count, bytes), operation| match operation {
            Operation::Clone(index) => Some((
                count + 1,
                bytes.checked_add(snapshots[*index].as_ref()?.total_size())?,
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
    // Preflight has only read private state and called pure declarations. This
    // is the first point that needs canonical Id controls: before any original
    // PC executes or any logical clone allocation is charged.
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
        identities.resize(nodes.len(), None);
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
                                (*runtime.state.get()).heap.record_virtual_clone(
                                    snapshots[*index]
                                        .as_ref()
                                        .ok_or(ExecutionError::InvalidAction)?
                                        .clone(),
                                )
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
            physical.resize(nodes.len(), None);
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
        Ok(action)
    })())
}
