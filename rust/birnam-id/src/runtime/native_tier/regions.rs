//! Whole-PC native regions. Frames and original index allocation charges remain.
use super::*;
use birnam_codegen::frame_region;

// Process-lifetime implementation helper. The compiler bounds the range and
// runtime admission validates its backing Frame; no callback or allocation.
pub(super) unsafe extern "C" fn fill(words: *mut usize, count: usize, value: usize) {
    unsafe { core::slice::from_raw_parts_mut(words, count) }.fill(value);
}

pub(super) fn emit(plan: &frame_region::Plan) -> (Vec<u8>, usize) {
    let propagate = true;
    #[cfg(test)]
    let propagate = propagate && PROPAGATE.get();
    if propagate && plan.benefits_from_propagation() {
        return plan.emit_propagated();
    }
    let reuse = false;
    #[cfg(test)]
    let reuse = reuse || REUSE.get();
    plan.emit(reuse)
}

pub(super) struct Compiled {
    plan: Rc<frame_region::Plan>,
    kernel: unsafe extern "C" fn(*mut usize, usize),
    #[cfg(test)]
    loads: usize,
}

#[cfg(test)]
thread_local! {
    pub(super) static ENABLED: core::cell::Cell<bool> = const { core::cell::Cell::new(true) };
    pub(super) static COUNTED_INDICES: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
    pub(super) static COUNTED_BOXES: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    pub(super) static REUSE: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
    pub(super) static PROPAGATE: core::cell::Cell<bool> = const { core::cell::Cell::new(true) };
    pub(super) static COMPILE_NS: core::cell::Cell<u128> = const { core::cell::Cell::new(0) };
    pub(super) static HITS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    pub(super) static LOADS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    pub(super) static MISSES: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

fn counted_indices_enabled() -> bool {
    #[cfg(test)]
    {
        COUNTED_INDICES.get()
    }
    #[cfg(not(test))]
    {
        std::env::var("BIRNAM_DEV_COUNTED_FRAME_INDICES").is_ok_and(|value| value == "1")
    }
}

/// Called only after whole-region no-collection and canonical slot admission.
/// The original operation is Runtime::make_integer, a direct floor constructor,
/// so neither an Integer method lookup nor an exported IntegerVt name grants
/// authority here. Use the same retained canonical vtable and raw payload shape.
fn record_dead_indices(runtime: &Runtime, indices: &[u32]) -> bool {
    if !counted_indices_enabled() {
        return false;
    }
    runtime.assert_owner();
    runtime.require_allocation_allowed();
    let state = unsafe { &mut *runtime.state.get() };
    let Some(canonical) = state.canonical else {
        return false;
    };
    state
        .heap
        .record_dead_frame_indices(canonical.smallint_vt, indices)
}

impl Compiled {
    pub(super) fn new(
        plan: Rc<frame_region::Plan>,
        kernel: unsafe extern "C" fn(*mut usize, usize),
        _loads: usize,
    ) -> Self {
        Self {
            plan,
            kernel,
            #[cfg(test)]
            loads: _loads,
        }
    }

    fn words(
        &self,
        runtime: &Runtime,
        machine: Oop,
        descriptor_key: usize,
        body: impl FnOnce(&mut [usize]),
    ) -> Option<()> {
        let capability = runtime.stackless_machine_capability(machine).ok()?;
        let state = unsafe { &mut *runtime.state.get() };
        let arena = state
            .execution
            .machine_mut(super::super::current_thread_token(), capability)
            .ok()?
            .arena_mut();
        let frame = arena.current_handle()?;
        arena
            .with_native_words(
                frame,
                descriptor_key,
                self.plan.pc as usize,
                self.plan.slot_count as usize,
                body,
            )
            .ok()
    }

    pub(super) unsafe fn invoke(
        &self,
        runtime: &Runtime,
        machine: Oop,
        descriptor_key: usize,
        baseline: Method,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        let symbol = {
            let state = unsafe { &*runtime.state.get() };
            match &self.plan.selector_store {
                Some((_, _, name)) => state.symbols.existing(name),
                None => Some(core::ptr::null_mut()),
            }
        };
        let room = {
            let state = unsafe { &*runtime.state.get() };
            let bytes = crate::heap::Heap::allocation_total_size(core::mem::size_of::<i64>())
                .unwrap()
                .saturating_mul(self.plan.boxed_indices.len());
            !state
                .metadata
                .automatic_collection_due(state.heap.bytes_since_collection().saturating_add(bytes))
        };
        // Admission precedes the first body allocation or write. The existing
        // PC token has already been allocated by the driver.
        if symbol.is_none()
            || !room
            || !runtime.guarded_slot_access_eligible(machine)
            || self
                .words(runtime, machine, descriptor_key, |_| {})
                .is_none()
        {
            #[cfg(test)]
            MISSES.set(MISSES.get() + 1);
            return unsafe { baseline(machine, args, count) };
        }
        // These body operands never expose an Oop: the original region already
        // discards each make_integer result before executing its raw copy kernel.
        // Admit the entire batch atomically, otherwise retain physical allocation.
        let counted = record_dead_indices(runtime, &self.plan.boxed_indices);
        #[cfg(test)]
        if counted {
            COUNTED_BOXES.set(COUNTED_BOXES.get() + self.plan.boxed_indices.len());
        }
        for &index in &self.plan.boxed_indices {
            #[cfg(test)]
            super::leaf_tests::record_box(index as i64);
            if !counted {
                runtime.make_integer(index as i64);
            }
        }
        // No callback/collection can occur between admission and this kernel.
        // All source operations are canonical, non-observing local copies.
        self.words(runtime, machine, descriptor_key, |words| unsafe {
            (self.kernel)(words.as_mut_ptr(), symbol.expect("admitted symbol").addr())
        })
        .expect("admitted Frame cannot change inside a no-observation region");
        #[cfg(test)]
        {
            HITS.set(HITS.get() + 1);
            LOADS.set(LOADS.get() + self.loads);
        }
        // Publication remains an ordinary Id send; native observers see the
        // completed baseline Frame, including every intermediate alias slot.
        let (selector, operand) = {
            let state = unsafe { &*runtime.state.get() };
            let objects = state.execution.objects().expect("admitted Machine");
            match self.plan.exit {
                frame_region::Exit::Return(slot) => (objects.return_at, slot),
                frame_region::Exit::Continue(pc) => (objects.continue_at, pc),
                frame_region::Exit::Call(site) => (objects.call_site, site),
                frame_region::Exit::TailCall(site) => (objects.tail_call_site, site),
            }
        };
        #[cfg(test)]
        super::leaf_tests::record_box(operand as i64);
        let index = runtime.make_integer(operand as i64);
        runtime.send_slice(machine, selector, &[index])
    }
}
