//! Guarded continuation at a source literal's original constructor boundary.
use super::*;
use crate::execution::{DecodedAction, ExecutionError, ResumableEntryCapability, StacklessTarget};
use crate::runtime::current_thread_token;
use birnam_codegen::closure_elision::{NumericPlan, Plan, Value, numeric_body::NumericBody};
use core::cell::Cell;
pub(super) mod pending;
mod region;

pub(super) struct Prepared {
    pub(super) descriptor: Vec<u8>,
    plan: Plan<Body>,
}

enum Body {
    Leaf(small_callee::Plan),
    Numeric(Box<NumericBody>),
}
impl Body {
    fn allocation_count(&self) -> usize {
        match self {
            Self::Leaf(plan) => plan.allocation_count(),
            // The caller charges the physical callee's initial PC token.
            Self::Numeric(body) => body.allocations.len() - 1,
        }
    }
    #[cfg(feature = "ffi-test-hooks")]
    fn metadata_payload_bytes(&self) -> usize {
        use core::mem::{size_of, size_of_val};
        match self {
            Self::Leaf(p) => {
                size_of_val(p.recovery_sources())
                    + size_of_val(p.boxed_indices())
                    + p.additional_allocation_metadata_bytes()
            }
            Self::Numeric(p) => {
                size_of::<NumericBody>()
                    + size_of_val(p.allocations.as_slice())
                    + size_of_val(p.publications.as_slice())
                    + size_of_val(p.template.inputs())
                    + size_of_val(p.template.sends())
                    + p.template
                        .sends()
                        .iter()
                        .map(|s| s.selector.len())
                        .sum::<usize>()
                    + p.template.step_count() * size_of::<usize>()
                    + p.recovery.storage_bytes()
                    - size_of::<numeric_chain::RecoveryMaps>()
            }
        }
    }
}
impl Prepared {
    pub(super) fn new(bytes: &[u8], row: selection::Row<'_>) -> Option<Self> {
        #[cfg(test)]
        DERIVATIONS.set(DERIVATIONS.get() + 1);
        #[cfg(feature = "ffi-test-hooks")]
        let _preparation = PreparationTime::start();
        let program = resumable::lower_portable_block(bytes).ok()?;
        // Avoid deriving/coloring another plan for ordinary literal-free code.
        if !program.blocks.first()?.operations.iter().any(|op| {
            matches!(
                op.kind,
                resumable::OperationKind::PortableBlockLiteral { .. }
            )
        }) {
            return None;
        }
        let layout = resumable::color_slots(&program).ok()?;
        let plan = if let Ok(plan) = Plan::derive(&program, &layout) {
            if plan.callee.has_literal_allocations() && !literal_callees_enabled() {
                return None;
            }
            plan.map_callee(Body::Leaf)
        } else {
            NumericPlan::derive_numeric(&program, &layout)
                .ok()?
                .map_callee(|body| Body::Numeric(Box::new(body)))
        };
        let descriptor =
            resumable_descriptor::compile_portable_descriptor_v2(&program, &layout, row, 1).ok()?;
        Some(Self { descriptor, plan })
    }
}
pub(super) struct Record {
    pub(super) descriptor: Weak<FrameDescriptor>,
    preparation: Preparation,
    visits: Cell<usize>,
    constructor: Option<Rc<crate::loader::BlockConstructionSummary>>,
}
enum Preparation {
    Pending {
        input: Rc<pending::Pending>,
        expected: pending::Expected,
    },
    Ready(Rc<Prepared>),
    Refused,
}
enum Visit {
    Cold,
    Ready(Rc<Prepared>),
    Refused,
}
impl Record {
    #[cfg(all(feature = "ffi-test-hooks", feature = "allocation-metrics"))]
    pub(super) fn watch_owners(&self, watch: &mut crate::runtime::consumer_lifetime_probe::Owners) {
        watch.arc("closure-descriptor", self.descriptor.clone());
        match &self.preparation {
            Preparation::Pending { input, .. } => watch.rc("closure-input", input),
            Preparation::Ready(prepared) => watch.rc("closure-plan", prepared),
            Preparation::Refused => (),
        }
        if let Some(constructor) = &self.constructor {
            watch.rc("closure-constructor", constructor);
        }
    }

    pub(super) fn new(
        input: Rc<pending::Pending>,
        descriptor: &Arc<FrameDescriptor>,
        bytes: &[u8],
    ) -> Option<Self> {
        let expected = input.expected(bytes)?;
        Some(Self {
            descriptor: Arc::downgrade(descriptor),
            preparation: Preparation::Pending { input, expected },
            visits: Cell::new(0),
            constructor: None,
        })
    }

    fn ready(&self) -> Option<&Rc<Prepared>> {
        match &self.preparation {
            Preparation::Ready(prepared) => Some(prepared),
            _ => None,
        }
    }

    pub(super) fn expired_pending(&self) -> bool {
        matches!(self.preparation, Preparation::Pending { .. })
            && self.descriptor.strong_count() == 0
    }

    fn visit(&mut self) -> Visit {
        let visits = self.visits.get().saturating_add(1);
        self.visits.set(visits);
        if visits < HOT_ENTRIES {
            return Visit::Cold;
        }
        self.prepare().map_or(Visit::Refused, Visit::Ready)
    }

    // Pure host compilation only: no Id allocation, callback or code loading.
    fn prepare(&mut self) -> Option<Rc<Prepared>> {
        if let Some(prepared) = self.ready() {
            return Some(prepared.clone());
        }
        let Preparation::Pending { input, expected } =
            core::mem::replace(&mut self.preparation, Preparation::Refused)
        else {
            return None;
        };
        let prepared = input.prepare()?;
        if prepared.descriptor.as_slice() != expected.bytes.as_ref() {
            return None;
        }
        let prepared = Rc::new(prepared);
        self.preparation = Preparation::Ready(prepared.clone());
        Some(prepared)
    }
}
struct EntryLease<'a>(&'a Runtime, Option<ResumableEntryCapability>);
pub(super) struct Resume {
    machine: Oop,
    frame: crate::execution::FrameHandle,
    pc: usize,
    action: Oop,
}
impl Drop for EntryLease<'_> {
    fn drop(&mut self) {
        if let Some(entry) = self.1 {
            unsafe {
                (*self.0.state.get())
                    .execution
                    .release_acquired_entry(entry)
            }
            .expect("owned closure callee lease");
        }
    }
}

#[cfg(test)]
thread_local! {
    pub(super) static ENABLED: Cell<bool> = const { Cell::new(false) };
    static DERIVATIONS: Cell<usize> = const { Cell::new(0) };
}
#[cfg(any(test, feature = "ffi-test-hooks"))]
thread_local! {
    pub(super) static COMPLETED_CALLS: Cell<usize> = const { Cell::new(0) };
    pub(super) static HITS: Cell<usize> = const { Cell::new(0) };
    pub(super) static MATERIALIZED: Cell<usize> = const { Cell::new(0) };
}

#[cfg(feature = "ffi-test-hooks")]
thread_local! {
    static ATTEMPTS: Cell<usize> = const { Cell::new(0) };
    static CONSTRUCTOR_QUERIES: Cell<usize> = const { Cell::new(0) };
    static CONSTRUCTOR_DISCOVERIES: Cell<usize> = const { Cell::new(0) };
    static PREPARATIONS: Cell<usize> = const { Cell::new(0) };
    static PREPARATION_NS: Cell<u128> = const { Cell::new(0) };
}
#[cfg(feature = "ffi-test-hooks")]
struct PreparationTime(std::time::Instant);
#[cfg(feature = "ffi-test-hooks")]
impl PreparationTime {
    fn start() -> Self {
        PREPARATIONS.set(PREPARATIONS.get() + 1);
        Self(std::time::Instant::now())
    }
}
#[cfg(feature = "ffi-test-hooks")]
impl Drop for PreparationTime {
    fn drop(&mut self) {
        PREPARATION_NS.set(PREPARATION_NS.get() + self.0.elapsed().as_nanos());
    }
}
#[cfg(feature = "ffi-test-hooks")]
pub(in crate::runtime) fn counters() -> (usize, usize, usize, usize, u128) {
    (
        ATTEMPTS.get(),
        HITS.get(),
        MATERIALIZED.get(),
        PREPARATIONS.get(),
        PREPARATION_NS.get(),
    )
}

#[cfg(feature = "ffi-test-hooks")]
pub(in crate::runtime) fn constructor_discoveries() -> usize {
    CONSTRUCTOR_DISCOVERIES.get()
}

#[cfg(feature = "ffi-test-hooks")]
pub(in crate::runtime) fn completed_calls() -> usize {
    COMPLETED_CALLS.get()
}

#[cfg(feature = "ffi-test-hooks")]
pub(in crate::runtime) fn constructor_queries() -> usize {
    CONSTRUCTOR_QUERIES.get()
}

#[cfg(feature = "ffi-test-hooks")]
impl Record {
    // Serialized/element payload plus inline plan/record storage. This excludes
    // spare Vec capacity, BTree nodes, reference counts and allocator metadata.
    fn metadata_payload_bytes(&self) -> usize {
        use core::mem::{size_of, size_of_val};
        let prepared = match &self.preparation {
            Preparation::Ready(prepared) => prepared,
            Preparation::Pending { input, expected } => {
                return size_of::<Self>() + input.payload_bytes() + expected.bytes.len();
            }
            Preparation::Refused => return size_of::<Self>(),
        };
        let p = &prepared.plan;
        size_of::<Self>()
            + size_of::<Prepared>()
            + self
                .constructor
                .as_ref()
                .map_or(0, |_| size_of::<crate::loader::BlockConstructionSummary>())
            + prepared.descriptor.len()
            + p.program.len()
            + size_of_val(p.following.as_slice())
            + p.following
                .iter()
                .map(|step| {
                    step.selector.len()
                        + size_of_val(step.argument_sources.as_slice())
                        + size_of_val(step.stores.as_slice())
                        + size_of_val(step.boxed_indices.as_slice())
                        + size_of_val(step.invocation_aliases.as_slice())
                        + size_of_val(step.continuation.copies.as_slice())
                        + size_of_val(step.continuation.clear_bitmap_words.as_slice())
                        + size_of_val(step.continuation_aliases.as_slice())
                })
                .sum::<usize>()
            + size_of_val(p.capture_sources.as_slice())
            + size_of_val(p.argument_sources.as_slice())
            + p.selector.len()
            + p.callee.metadata_payload_bytes()
            + size_of_val(p.invocation_aliases.as_slice())
            + size_of_val(p.constructor_stores.as_slice())
            + p.continuation.as_ref().map_or(0, |e| {
                size_of_val(e.copies.as_slice()) + size_of_val(e.clear_bitmap_words.as_slice())
            })
            + p.continuation_aliases
                .as_ref()
                .map_or(0, |a| size_of_val(a.as_slice()))
            + size_of_val(p.before_constructor_boxes.as_slice())
            + size_of_val(p.after_constructor_boxes.as_slice())
    }
}

pub(super) fn enabled() -> bool {
    #[cfg(not(test))]
    {
        std::env::var_os("BIRNAM_DEV_CLOSURE_ELISION").is_some_and(|v| v == "1")
    }
    #[cfg(test)]
    {
        ENABLED.get()
    }
}

impl Runtime {
    #[cfg(feature = "ffi-test-hooks")]
    pub(in crate::runtime) fn closure_pending_resources(&self) -> (usize, usize) {
        let registry = self.native_tier.borrow();
        (
            registry
                .closures
                .values()
                .filter(|record| matches!(record.preparation, Preparation::Pending { .. }))
                .count(),
            registry
                .closure_pending_budget
                .as_ref()
                .map_or(0, pending::Budget::bytes),
        )
    }

    #[cfg(feature = "ffi-test-hooks")]
    pub(in crate::runtime) fn closure_metadata_payload_bytes(&self) -> usize {
        self.native_tier
            .borrow()
            .closures
            .values()
            .map(Record::metadata_payload_bytes)
            .sum()
    }
    pub(in crate::runtime) fn finish_closure_inline(
        &self,
        machine: Oop,
        cap: crate::execution::MachineCapability,
        action: Oop,
    ) -> Result<bool, ExecutionError> {
        let Some(resume) = self.native_tier.borrow_mut().closure_resume.take() else {
            return Ok(false);
        };
        let state = unsafe { &*self.state.get() };
        let arena = state
            .execution
            .machine(current_thread_token(), cap)?
            .arena();
        if resume.machine != machine
            || resume.action != action
            || arena.current_handle() != Some(resume.frame)
            || arena.program_counter(resume.frame)? != resume.pc
        {
            return Err(ExecutionError::InvalidAction);
        }
        Ok(true)
    }
    /// Null/None declines before any constructor charge or Frame write. After
    /// admission, an execution error belongs to this path and must never retry
    /// the original constructor or replay the caller prefix.
    pub(crate) unsafe fn try_inline_closure(
        &self,
        machine: Oop,
        source_method: *const c_void,
        node_index: u32,
        constructor: *const c_void,
        block_vtable: Oop,
        code_object: Oop,
        captures: &[Oop],
        materialization_owner: Oop,
        materialized: bool,
        codegen_api: &birnam_abi::IdApi,
        integer: &birnam_abi::integer::IntegerApiV1,
    ) -> Option<Result<Oop, ExecutionError>> {
        #[cfg(feature = "ffi-test-hooks")]
        ATTEMPTS.set(ATTEMPTS.get() + 1);
        if !enabled()
            || !crate::public_api::closure_codegen_context_matches(
                codegen_api,
                self.canonical().nil, integer, &self.integer_operations(),
            )
            || !fused_numeric::controls(self, machine)
            || self.native_tier.borrow().closure_resume.is_some()
        {
            return None;
        }
        let owner = current_thread_token();
        let cap = self.stackless_machine_capability(machine).ok()?;
        let nil = self.canonical().nil;
        let (frame, descriptor) = {
            let state = unsafe { &*self.state.get() };
            let arena = state.execution.machine(owner, cap).ok()?.arena();
            let frame = arena.current_handle()?;
            let descriptor = arena.descriptor_arc(frame).ok()?;
            if arena.program_counter(frame).ok()? != 0
                || descriptor.step_method() as *const c_void != source_method
            {
                return None;
            }
            (frame, descriptor)
        };
        let (prepared, constructor_summary) = {
            let mut registry = self.native_tier.borrow_mut();
            let record = registry
                .closures
                .get_mut(&(Arc::as_ptr(&descriptor) as usize))?;
            if !record
                .descriptor
                .upgrade()
                .is_some_and(|d| Arc::ptr_eq(&d, &descriptor))
            {
                return None;
            }
            let prepared = match record.visit() {
                Visit::Cold => return None,
                Visit::Ready(prepared) => prepared,
                Visit::Refused => {
                    let retired = registry
                        .closures
                        .remove(&(Arc::as_ptr(&descriptor) as usize));
                    drop(registry);
                    drop(retired);
                    return None;
                }
            };
            (prepared, record.constructor.clone())
        };
        let plan = &prepared.plan;
        plan.continuation.as_ref()?;
        if plan.node_index != node_index || captures.len() != plan.capture_sources.len() {
            return None;
        }
        // An ordinary native callee cannot use this consumer. This read-only
        // rejection precedes constructor inspection and host payload work. It
        // neither acquires an entry nor caches a refusal: registration and
        // lookup changes are visible on the next call. Successful admission
        // still performs its original lookup and generation acquisition below.
        {
            let state = unsafe { &*self.state.get() };
            if state.heap.vtable_of(block_vtable) != Some(state.globals.vtable_vt) {
                return None;
            }
            let selector = state.symbols.existing(&plan.selector)?;
            let method = state.vtables.lookup(block_vtable, selector)?;
            if !state.execution.has_resumable_thunk(method) {
                return None;
            }
        }
        // Cache only the resident declaration and its native-image pin, never
        // its answer. The bounded, weak-descriptor record owns one entry; the
        // selected Rc keeps the image live if that record is replaced/pruned.
        let summary = if let Some(summary) = constructor_summary.filter(|s| s.matches(constructor))
        {
            summary
        } else {
            #[cfg(feature = "ffi-test-hooks")]
            CONSTRUCTOR_DISCOVERIES.set(CONSTRUCTOR_DISCOVERIES.get() + 1);
            let summary = Rc::new(crate::loader::BlockConstructionSummary::discover(
                constructor,
            )?);
            let previous = {
                let mut registry = self.native_tier.borrow_mut();
                let record = registry
                    .closures
                    .get_mut(&(Arc::as_ptr(&descriptor) as usize))?;
                if !record
                    .ready()
                    .is_some_and(|current| Rc::ptr_eq(current, &prepared))
                {
                    return None;
                }
                record.constructor.replace(summary.clone())
            };
            // Release an old native pin outside the registry borrow.
            drop(previous);
            summary
        };
        #[cfg(feature = "ffi-test-hooks")]
        CONSTRUCTOR_QUERIES.set(CONSTRUCTOR_QUERIES.get() + 1);
        if !unsafe {
            summary.warm(
                self,
                block_vtable,
                code_object,
                captures.len(),
                materialization_owner,
                &plan.program,
                materialized,
            )
        } {
            return None;
        }

        // Oop scratch is deliberately a host Vec, not a conservatively scanned
        // native array. All values remain rooted by the caller/original literal.
        let mut inputs = vec![nil; plan.invocation_aliases.len()];
        let snapshot = {
            let state = unsafe { &*self.state.get() };
            let arena = state.execution.machine(owner, cap).ok()?.arena();
            for (slot, value) in inputs.iter_mut().enumerate() {
                *value = arena.read_oop(frame, slot).ok()?;
            }
            for (&slot, &value) in plan.capture_sources.iter().zip(captures) {
                if inputs[slot as usize] != value {
                    return None;
                }
            }
            if state.heap.vtable_of(block_vtable) != Some(state.globals.vtable_vt)
                || state.vtables.trace_hook(block_vtable).is_some()
                || !state
                    .vtables
                    .all_oop_layout(block_vtable, captures.len() + 1)
                || !state
                    .execution
                    .inline_return_storage_valid(&state.heap, owner, cap)
            {
                return None;
            }
            let mut payload = Vec::with_capacity(captures.len() + 1);
            payload.push(code_object);
            payload.extend_from_slice(captures);
            state.heap.prepare_virtual_payload(block_vtable, &payload)?
        };
        let _read = self.enter_dispatch_read();
        let mut admitted = Vec::new();
        let mut budget = snapshot.total_size();
        let integer_bytes = crate::heap::Heap::allocation_total_size(self.integer_storage().payload_size)?;
        for call in region::calls(plan) {
            let Some(selected) = region::admit(
                self,
                machine,
                call,
                plan,
                block_vtable,
                &inputs,
                captures,
                &admitted,
            ) else {
                break;
            };
            let boxes = selected.call.boxes.len()
                + plan.callee.allocation_count()
                + selected.descriptor.driver_argument_count()
                + if admitted.is_empty() {
                    0
                } else {
                    descriptor.driver_argument_count()
                };
            let next_budget = budget.checked_add(integer_bytes.checked_mul(boxes)?)?;
            let state = unsafe { &*self.state.get() };
            if !state.heap.admits_virtual_allocations(1)
                || state.metadata.automatic_collection_due(
                    state
                        .heap
                        .bytes_since_collection()
                        .checked_add(next_budget)?,
                )
            {
                break;
            }
            budget = next_budget;
            admitted.push(selected);
        }
        // A later refusal retains the already admitted shorter region. No
        // original charge or Frame store has happened at this point.
        let last = admitted.last()?;
        let edge = last.call.edge;
        let aliases = last.call.after;
        if !fused_numeric::controls(self, machine) {
            return None;
        }
        // No observing callback or collection can run inside this admitted
        // region. Read developer recovery controls once here; the next region
        // still observes fresh controls, including after an ordinary fallback.
        let force_recovery =
            std::env::var_os("BIRNAM_DEV_CLOSURE_FORCE_RECOVERY").is_some_and(|v| v == "1");
        let force_at = std::env::var("BIRNAM_DEV_CLOSURE_FORCE_AT")
            .ok()
            .and_then(|v| v.parse::<usize>().ok());
        Some((|| {
            let identity = unsafe { (*self.state.get()).heap.record_virtual_clone(snapshot) }
                .ok_or(ExecutionError::InvalidAction)?;
            // Untraced scratch; the admitted interval cannot collect or observe.
            let mut produced = Vec::new();
            let mut action = nil;
            for (call_index, selected) in admitted.iter_mut().enumerate() {
                let call = &selected.call;
                if call_index != 0 && descriptor.driver_argument_count() == 1 {
                    self.make_integer(call.pc as i64);
                }
                let mut publication = nil;
                for &index in call.boxes {
                    publication = self.make_integer(index as i64);
                }
                {
                    let arena = unsafe { (*self.state.get()).execution.machine_mut(owner, cap)? }
                        .arena_mut();
                    for &(slot, value) in call.stores {
                        arena.write_oop(
                            frame,
                            slot as usize,
                            match value {
                                Value::Input(s) => inputs[s as usize],
                                Value::Closure => nil,
                                Value::Result(i) => produced[i as usize],
                            },
                        )?;
                    }
                    let window = plan.call_window_start as usize;
                    arena.write_oop(frame, window, nil)?;
                    arena.write_oop(frame, window + 1, selected.selector)?;
                    for (i, &value) in selected.arguments.iter().enumerate() {
                        arena.write_oop(frame, window + 2 + i, value)?;
                    }
                }
                action = unsafe {
                    crate::execution::machine_call_site(
                        machine,
                        core::ptr::from_ref(&publication),
                        1,
                    )
                };
                let decoded = unsafe {
                    (*self.state.get()).execution.decode_action(
                        &(*self.state.get()).heap,
                        owner,
                        cap,
                        action,
                    )?
                };
                let DecodedAction::Call { result_slot, .. } = decoded else {
                    return Err(ExecutionError::InvalidAction);
                };
                if force_recovery || force_at == Some(call_index) {
                    let block = unsafe {
                        (*self.state.get())
                            .heap
                            .materialize_recorded_clone(identity)
                    }
                    .ok_or(ExecutionError::InvalidAction)?;
                    let arena = unsafe { (*self.state.get()).execution.machine_mut(owner, cap)? }
                        .arena_mut();
                    for (slot, &live) in call.aliases.iter().enumerate() {
                        if live {
                            arena.write_oop(frame, slot, block)?;
                        }
                    }
                    if std::env::var_os("BIRNAM_DEV_CLOSURE_REPORT").is_some_and(|v| v == "1") {
                        eprintln!("closure-elision: forced constructor recovery");
                    }
                    // Normal dispatch consumes the already published Call. The
                    // original constructor and caller prefix are not repeated.
                    return Ok(action);
                }
                let entry_token = if selected.descriptor.driver_argument_count() == 1 {
                    self.make_integer(0)
                } else {
                    nil
                };
                match &plan.callee {
                    Body::Leaf(leaf) => {
                        let mut returned = call.returned.map(|s| inputs[s as usize]).unwrap_or(nil);
                        for (index, word) in leaf.allocation_values().enumerate() {
                            let allocated = self.make_integer(word);
                            if leaf.return_allocation() == Some(index) {
                                returned = allocated;
                            }
                        }
                        if leaf.return_allocation().is_some() {
                            produced.push(returned);
                        }
                        unsafe {
                            let state = &mut *self.state.get();
                            state.execution.commit_inlined_return(
                                &mut state.heap,
                                owner,
                                cap,
                                result_slot,
                                leaf.return_slot(),
                                nil,
                                returned,
                            )?;
                        }
                    }
                    Body::Numeric(_) => {
                        let returned = unsafe {
                            selected.execute_numeric(
                                self,
                                machine,
                                cap,
                                result_slot,
                                &entry_token,
                            )?
                        };
                        if plan.return_source.is_none() {
                            produced.push(returned);
                        }
                    }
                }
                #[cfg(any(test, feature = "ffi-test-hooks"))]
                COMPLETED_CALLS.set(COMPLETED_CALLS.get() + 1);
            }
            if aliases.iter().any(|&v| v) {
                let block = unsafe {
                    (*self.state.get())
                        .heap
                        .materialize_recorded_clone(identity)
                }
                .ok_or(ExecutionError::InvalidAction)?;
                let arena =
                    unsafe { (*self.state.get()).execution.machine_mut(owner, cap)? }.arena_mut();
                for (slot, &live) in aliases.iter().enumerate() {
                    if live {
                        arena.write_oop(frame, slot, block)?;
                    }
                }
                #[cfg(any(test, feature = "ffi-test-hooks"))]
                MATERIALIZED.set(MATERIALIZED.get() + 1);
            }
            #[cfg(not(test))]
            let completed_count = admitted.len();
            drop(admitted);
            drop(_read);
            #[cfg(any(test, feature = "ffi-test-hooks"))]
            HITS.set(HITS.get() + 1);
            // Unwind the original C literal's capture locals before the driver
            // allocates the successor token or enters an observing PC. Calling
            // it here would extend conservative roots past their baseline life.
            self.native_tier.borrow_mut().closure_resume = Some(Resume {
                machine,
                frame,
                pc: edge.target.0 as usize,
                action,
            });
            #[cfg(not(test))]
            if std::env::var_os("BIRNAM_DEV_CLOSURE_REPORT").is_some_and(|v| v == "1") {
                eprintln!(
                    "closure-elision: hit materialized={}",
                    aliases.iter().any(|&v| v)
                );
                if completed_count > 1 {
                    eprintln!("closure-elision: completed calls={completed_count}");
                }
            }
            Ok(action)
        })())
    }
}
