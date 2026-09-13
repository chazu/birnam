//! Guarded continuation at a source literal's original constructor boundary.
use super::*;
use crate::execution::{DecodedAction, ExecutionError, ResumableEntryCapability, StacklessTarget};
use crate::runtime::current_thread_token;
use birnam_codegen::closure_elision::{Plan, Value};
use core::cell::Cell;

pub(super) struct Prepared {
    pub(super) descriptor: Vec<u8>,
    plan: Plan,
}
impl Prepared {
    pub(super) fn new(bytes: &[u8], row: selection::Row<'_>) -> Option<Self> {
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
        let plan = Plan::derive(&program, &layout).ok()?;
        let descriptor =
            resumable_descriptor::compile_portable_descriptor_v2(&program, &layout, row, 1).ok()?;
        Some(Self { descriptor, plan })
    }
}
pub(super) struct Record {
    pub(super) descriptor: Weak<FrameDescriptor>,
    prepared: Rc<Prepared>,
    visits: Cell<usize>,
}
impl Record {
    pub(super) fn new(prepared: Rc<Prepared>, descriptor: &Arc<FrameDescriptor>) -> Self {
        Self {
            descriptor: Arc::downgrade(descriptor),
            prepared,
            visits: Cell::new(0),
        }
    }
}
struct EntryLease<'a>(&'a Runtime, ResumableEntryCapability);
pub(super) struct Resume {
    machine: Oop,
    frame: crate::execution::FrameHandle,
    pc: usize,
    action: Oop,
}
impl Drop for EntryLease<'_> {
    fn drop(&mut self) {
        unsafe {
            (*self.0.state.get())
                .execution
                .release_acquired_entry(self.1)
        }
        .expect("owned closure callee lease");
    }
}

#[cfg(test)]
thread_local! {
    pub(super) static ENABLED: Cell<bool> = const { Cell::new(false) };
    pub(super) static HITS: Cell<usize> = const { Cell::new(0) };
    pub(super) static MATERIALIZED: Cell<usize> = const { Cell::new(0) };
}

impl Runtime {
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
    ) -> Option<Result<Oop, ExecutionError>> {
        #[cfg(not(test))]
        let enabled = std::env::var_os("BIRNAM_DEV_CLOSURE_ELISION").is_some_and(|v| v == "1");
        #[cfg(test)]
        let enabled = ENABLED.get();
        if !enabled
            || !crate::public_api::closure_codegen_context_matches(codegen_api)
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
        let prepared = {
            let registry = self.native_tier.borrow();
            let record = registry
                .closures
                .get(&(Arc::as_ptr(&descriptor) as usize))?;
            if !record
                .descriptor
                .upgrade()
                .is_some_and(|d| Arc::ptr_eq(&d, &descriptor))
            {
                return None;
            }
            let visits = record.visits.get().saturating_add(1);
            record.visits.set(visits);
            if visits < HOT_ENTRIES {
                return None;
            }
            record.prepared.clone()
        };
        let plan = &prepared.plan;
        let edge = plan.continuation.as_ref()?;
        if plan.node_index != node_index || captures.len() != plan.capture_sources.len() {
            return None;
        }
        let summary = crate::loader::BlockConstructionSummary::discover(constructor)?;
        if !unsafe {
            summary.warm(
                nil,
                self.globals().symbol_vt,
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
        let (selector, method, snapshot) = {
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
            let selector = state.symbols.existing(&plan.selector)?;
            let method = state.vtables.lookup(block_vtable, selector)?;
            let mut payload = Vec::with_capacity(captures.len() + 1);
            payload.push(code_object);
            payload.extend_from_slice(captures);
            let snapshot = state.heap.prepare_virtual_payload(block_vtable, &payload)?;
            (selector, method, snapshot)
        };
        let _read = self.enter_dispatch_read();
        let StacklessTarget::Registered { entry } = (unsafe {
            (*self.state.get())
                .execution
                .classify_resumable(method)
                .ok()?
        })?
        else {
            return None;
        };
        let lease = EntryLease(self, entry);
        let arguments: Vec<_> = plan
            .argument_sources
            .iter()
            .map(|&s| inputs[s as usize])
            .collect();
        let callee = unsafe {
            (*self.state.get())
                .execution
                .materialize_acquired_entry(entry, nil, &arguments, captures, nil)
                .ok()?
        };
        if callee.descriptor.v2_entry_binding()?.argument_count != arguments.len() {
            return None;
        }
        {
            let registry = self.native_tier.borrow();
            let record = registry
                .records
                .get(&(Arc::as_ptr(&callee.descriptor) as usize))?;
            if record.disabled || record.leaf.as_deref() != Some(&plan.callee) {
                return None;
            }
        }
        let boxes = plan.after_constructor_boxes.len()
            + plan.callee.boxed_indices().len()
            + callee.descriptor.driver_argument_count();
        let budget = crate::heap::Heap::allocation_total_size(8)?
            .checked_mul(boxes)?
            .checked_add(snapshot.total_size())?;
        {
            let state = unsafe { &*self.state.get() };
            if !state.heap.admits_virtual_allocations(1)
                || state.metadata.automatic_collection_due(
                    state.heap.bytes_since_collection().checked_add(budget)?,
                )
            {
                return None;
            }
        }
        if !fused_numeric::controls(self, machine) {
            return None;
        }
        Some((|| {
            let identity = unsafe { (*self.state.get()).heap.record_virtual_clone(snapshot) }
                .ok_or(ExecutionError::InvalidAction)?;
            let mut publication = nil;
            for &index in &plan.after_constructor_boxes {
                publication = self.make_integer(index as i64);
            }
            {
                let arena =
                    unsafe { (*self.state.get()).execution.machine_mut(owner, cap)? }.arena_mut();
                for &(slot, value) in &plan.constructor_stores {
                    arena.write_oop(
                        frame,
                        slot as usize,
                        match value {
                            Value::Input(s) => inputs[s as usize],
                            Value::Closure => nil,
                        },
                    )?;
                }
                let window = plan.call_window_start as usize;
                arena.write_oop(frame, window, nil)?;
                arena.write_oop(frame, window + 1, selector)?;
                for (i, &value) in arguments.iter().enumerate() {
                    arena.write_oop(frame, window + 2 + i, value)?;
                }
            }
            let action = unsafe {
                crate::execution::machine_call_site(machine, core::ptr::from_ref(&publication), 1)
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
            if std::env::var_os("BIRNAM_DEV_CLOSURE_FORCE_RECOVERY").is_some_and(|v| v == "1") {
                let block = unsafe {
                    (*self.state.get())
                        .heap
                        .materialize_recorded_clone(identity)
                }
                .ok_or(ExecutionError::InvalidAction)?;
                let arena =
                    unsafe { (*self.state.get()).execution.machine_mut(owner, cap)? }.arena_mut();
                for (slot, &live) in plan.invocation_aliases.iter().enumerate() {
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
            if callee.descriptor.driver_argument_count() == 1 {
                self.make_integer(0);
            }
            for &index in plan.callee.boxed_indices() {
                self.make_integer(index as i64);
            }
            unsafe {
                let state = &mut *self.state.get();
                state.execution.commit_inlined_return(
                    &mut state.heap,
                    owner,
                    cap,
                    result_slot,
                    plan.callee.return_slot(),
                    nil,
                    inputs[plan.return_source as usize],
                )?;
            }
            let aliases = plan
                .continuation_aliases
                .as_ref()
                .ok_or(ExecutionError::InvalidAction)?;
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
                #[cfg(test)]
                MATERIALIZED.set(MATERIALIZED.get() + 1);
            }
            drop(lease);
            drop(_read);
            #[cfg(test)]
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
            }
            Ok(action)
        })())
    }
}
