//! Borrowed sequence views and owner-scoped admission of current callees.
use super::*;
use birnam_codegen::resumable::ColoredEdge;

pub(super) struct Call<'a> {
    pub pc: u32,
    pub selector: &'a [u8],
    pub arguments: &'a [u32],
    pub returned: Option<u32>,
    pub stores: &'a [(u32, Value)],
    pub boxes: &'a [u32],
    pub aliases: &'a [bool],
    pub edge: &'a ColoredEdge,
    pub after: &'a [bool],
}

pub(super) fn calls(plan: &Plan<Body>) -> impl Iterator<Item = Call<'_>> {
    core::iter::once(Call {
        pc: 0,
        selector: &plan.selector,
        arguments: &plan.argument_sources,
        returned: plan.return_source,
        stores: &plan.constructor_stores,
        boxes: &plan.after_constructor_boxes,
        aliases: &plan.invocation_aliases,
        edge: plan.continuation.as_ref().unwrap(),
        after: plan.continuation_aliases.as_ref().unwrap(),
    })
    .chain(plan.following.iter().map(|step| Call {
        pc: step.pc,
        selector: &step.selector,
        arguments: &step.argument_sources,
        returned: step.return_source,
        stores: &step.stores,
        boxes: &step.boxed_indices,
        aliases: &step.invocation_aliases,
        edge: &step.continuation,
        after: &step.continuation_aliases,
    }))
}

pub(super) struct Admitted<'a> {
    pub call: Call<'a>,
    pub selector: Oop,
    pub arguments: Vec<Oop>,
    pub descriptor: Arc<FrameDescriptor>,
    lease: EntryLease<'a>,
    numeric: Option<NumericExecution>,
}

pub(super) fn admit<'a>(
    runtime: &'a Runtime,
    machine: Oop,
    call: Call<'a>,
    plan: &Plan<Body>,
    vtable: Oop,
    inputs: &[Oop],
    captures: &[Oop],
    previous: &[Admitted<'_>],
) -> Option<Admitted<'a>> {
    let (selector, method) = {
        let state = unsafe { &*runtime.state.get() };
        let selector = state.symbols.existing(call.selector)?;
        (selector, state.vtables.lookup(vtable, selector)?)
    };
    let StacklessTarget::Registered { entry } = (unsafe {
        (*runtime.state.get())
            .execution
            .classify_resumable(method)
            .ok()?
    })?
    else {
        return None;
    };
    let lease = EntryLease(runtime, Some(entry));
    let capture_layout = unsafe {
        (*runtime.state.get())
            .execution
            .acquired_entry_capture_layout(entry)
            .ok()?
    };
    // Match ordinary receiver decoding: captures follow the CodeObject word.
    // A zero-capture entry with zero offset reads no receiver payload at all.
    if capture_layout != (1, captures.len()) && !(captures.is_empty() && capture_layout == (0, 0)) {
        return None;
    }
    let arguments: Vec<_> = call.arguments.iter().map(|s| inputs[*s as usize]).collect();
    let descriptor = unsafe {
        (*runtime.state.get())
            .execution
            .acquired_entry_descriptor(entry)
            .ok()?
    };
    if descriptor.v2_entry_binding()?.argument_count != arguments.len() {
        return None;
    }
    let candidate = {
        let registry = runtime.compiler().native_tier.borrow();
        let record = registry.records.get(&(Arc::as_ptr(&descriptor) as usize))?;
        if record.disabled {
            return None;
        }
        match &plan.callee {
            Body::Leaf(leaf) => {
                if record.leaf.as_deref() != Some(leaf) {
                    return None;
                }
                None
            }
            Body::Numeric(_) => {
                let candidate = record.candidate.as_ref()?;
                // The actual acquired registration must originate in these exact
                // canonical Portable bytes, including capture order and dead
                // literal occurrences. Matching descriptor shape is insufficient.
                if candidate.numeric_closure_program.as_deref() != Some(plan.program.as_slice()) {
                    return None;
                }
                Some(candidate.clone())
            }
        }
    };
    let numeric = if let Some(candidate) = candidate {
        let initial = unsafe {
            (*runtime.state.get())
                .execution
                .materialize_acquired_entry(
                    entry,
                    runtime.canonical().nil,
                    &arguments,
                    captures,
                    runtime.canonical().nil,
                )
                .ok()?
        };
        let prediction = runtime.prepare_numeric_entry_prediction_reusing(
            machine,
            &descriptor,
            &initial.initial_oops,
            previous
                .iter()
                .rev()
                .filter_map(|entry| entry.numeric.as_ref().map(|n| &n.prediction)),
        )?;
        Some(NumericExecution {
            candidate,
            prediction,
            initial: initial.initial_oops,
        })
    } else {
        None
    };
    Some(Admitted {
        call,
        selector,
        arguments,
        descriptor,
        lease,
        numeric,
    })
}

struct NumericExecution {
    candidate: Rc<Candidate>,
    prediction: Rc<super::super::super::numeric::Ticket>,
    initial: Vec<(usize, Oop)>,
}
impl Admitted<'_> {
    pub(super) unsafe fn execute_numeric(
        &mut self,
        runtime: &Runtime,
        machine: Oop,
        capability: crate::execution::MachineCapability,
        result_slot: usize,
        token: &Oop,
    ) -> Result<Oop, ExecutionError> {
        let prepared = self.numeric.take().ok_or(ExecutionError::InvalidAction)?;
        let entry = self.lease.1.ok_or(ExecutionError::InvalidAction)?;
        let owner = current_thread_token();
        let nil = runtime.canonical().nil;
        unsafe {
            let state = &mut *runtime.state.get();
            // Establish machine validity before transferring the acquisition.
            state.execution.machine(owner, capability)?;
            state.execution.transfer_acquired_entry_to_frame(entry)?;
            self.lease.1 = None;
            let pushed = state
                .execution
                .machine_mut(owner, capability)?
                .arena_mut()
                .push_with_return_and_entry(
                    self.descriptor.clone(),
                    0,
                    nil,
                    &prepared.initial,
                    Some(crate::execution::ReturnDestination { result_slot }),
                    Some(entry),
                );
            if pushed.is_err() {
                state.execution.release_entry_frame_pin(entry)?;
            }
            pushed?;
        }
        let result = unsafe {
            fused_numeric::execute_prepared(
                runtime,
                machine,
                Arc::as_ptr(&self.descriptor) as usize,
                &prepared.candidate,
                &prepared.prediction,
                core::ptr::from_ref(token),
            )
        }?;
        // The ordinary return path applies original V2 copies/clears, pops the
        // real Frame and releases its pin. Machine action scratch is preserved.
        if runtime
            .finish_stackless_return(owner, capability, result)?
            .is_some()
        {
            return Err(ExecutionError::InvalidAction);
        }
        Ok(result)
    }
}
