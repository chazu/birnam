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

pub(super) fn calls(plan: &Plan) -> impl Iterator<Item = Call<'_>> {
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
    pub callee: crate::execution::MaterializedEntry,
    _lease: EntryLease<'a>,
}

pub(super) fn admit<'a>(
    runtime: &'a Runtime,
    call: Call<'a>,
    plan: &Plan,
    vtable: Oop,
    inputs: &[Oop],
    captures: &[Oop],
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
    let lease = EntryLease(runtime, entry);
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
    let nil = runtime.canonical().nil;
    let callee = unsafe {
        (*runtime.state.get())
            .execution
            .materialize_acquired_entry(entry, nil, &arguments, captures, nil)
            .ok()?
    };
    if callee.descriptor.v2_entry_binding()?.argument_count != arguments.len() {
        return None;
    }
    {
        let registry = runtime.native_tier.borrow();
        let record = registry
            .records
            .get(&(Arc::as_ptr(&callee.descriptor) as usize))?;
        if record.disabled || record.leaf.as_deref() != Some(&plan.callee) {
            return None;
        }
    }
    Some(Admitted {
        call,
        selector,
        arguments,
        callee,
        _lease: lease,
    })
}
