//! Speculative raw arithmetic, validated against actual operands at each call.
//!
//! A ticket holds raw integers and owned compiler/declaration metadata, never
//! borrowed Frame words or unrooted Oops.
//! The per-call experiment retains each original send and allocation boundary;
//! its next prediction must match current operands and native declarations after
//! any observing lookup or collection. Production driver fusion instead admits
//! a whole bounded region before its first effect, retaining all allocations
//! while proving that no observation can intervene between numeric operations.
use super::*;
use birnam_codegen::numeric_chain::{Input, Operator, Template, Value};
use birnam_native_compiler::Unit;
use std::cell::RefCell;
use std::ffi::CString;
pub(in crate::runtime) mod loop_reuse;

// A small per-runtime pool avoids allocating temporary host arrays on each
// invocation. Leases own the pool so cache clearing and nested drives cannot
// invalidate active storage. Free buffers contain no traced Birnam references.
const MAX_FREE_BUFFERS: usize = 4;
#[cfg(any(test, feature = "ffi-test-hooks"))]
thread_local! {
    pub(in crate::runtime) static ENTRY_PREDICTION_KERNELS: Cell<usize> = const { Cell::new(0) };
}
#[derive(Default)]
struct Buffers {
    inputs: Vec<i64>,
    outputs: Vec<i64>,
}
#[derive(Default)]
pub(super) struct Pool {
    free: Rc<RefCell<Vec<Buffers>>>,
}
struct BufferLease {
    values: Buffers,
    pool: Rc<RefCell<Vec<Buffers>>>,
}
impl Pool {
    fn take(&self, inputs: usize, outputs: usize) -> BufferLease {
        let mut values = self.free.borrow_mut().pop().unwrap_or_default();
        values.inputs.resize(inputs, 0);
        values.outputs.resize(outputs, 0);
        BufferLease {
            values,
            pool: self.free.clone(),
        }
    }
}
impl Drop for BufferLease {
    fn drop(&mut self) {
        let mut pool = self.pool.borrow_mut();
        if pool.len() < MAX_FREE_BUFFERS {
            pool.push(std::mem::take(&mut self.values));
        }
    }
}

pub(in crate::runtime) struct CompiledChain {
    template: Rc<Template>,
    bindings: Vec<Binding>,
    kernel: unsafe extern "C" fn(*const i64, *mut i64) -> usize,
    _unit: Unit,
    loop_reuse: Option<loop_reuse::Reuse>,
}

/// A prediction's native declaration is an owned lookup hint. Its current
/// context is queried again when the actual resolved Method matches the hint.
pub(in crate::runtime) struct Binding {
    pub(in crate::runtime) method: NativeEntry,
    pub(in crate::runtime) summary: Rc<NumericSummary>,
    pub(in crate::runtime) operator: Operator,
}

impl CompiledChain {
    /// Predict from explicit entry Oops before a physical callee Frame exists.
    /// The caller validates the current descriptor record. This path uses an
    /// already compiled owner and never changes hotness or loop reuse history.
    pub(in crate::runtime) fn predict_entry(
        self: &Rc<Self>,
        runtime: &Runtime,
        key: usize,
        initial: &[(usize, Oop)],
    ) -> Option<Ticket> {
        self.prepare_entry(runtime, key, initial)?.evaluate_entry()
    }

    /// Reuse only after ordinary entry validation, and only within the supplied
    /// bounded admission scope. Each actual call still owns its Oops and Frame.
    pub(in crate::runtime) fn predict_entry_reusing<'p>(
        self: &Rc<Self>,
        runtime: &Runtime,
        key: usize,
        initial: &[(usize, Oop)],
        previous: impl Iterator<Item = &'p Rc<Ticket>>,
    ) -> Option<Rc<Ticket>> {
        let ticket = self.prepare_entry(runtime, key, initial)?;
        for prior in previous {
            if prior.key == ticket.key
                && Rc::ptr_eq(&prior.chain, &ticket.chain)
                && prior.buffers.values.inputs == ticket.buffers.values.inputs
            {
                return Some(prior.clone());
            }
        }
        Some(Rc::new(ticket.evaluate_entry()?))
    }

    fn prepare_entry(
        self: &Rc<Self>,
        runtime: &Runtime,
        key: usize,
        initial: &[(usize, Oop)],
    ) -> Option<Ticket> {
        let mut buffers = runtime
            .numeric
            .borrow_mut()
            .buffers
            .get_or_insert_with(Pool::default)
            .take(self.template.inputs().len(), self.bindings.len());
        {
            let state = unsafe { &*runtime.state.get() };
            let _canonical = state.canonical?;
            let error = state.symbols.existing(b"ErrorVt")?;
            if state.environment.import(error) == runtime.integer_behavior() {
                return None;
            }
            for (destination, input) in buffers.values.inputs.iter_mut().zip(self.template.inputs())
            {
                *destination = match input {
                    Input::Literal { value, .. } => *value,
                    Input::Frame(slot) => {
                        let value = initial.iter().find_map(|&(index, value)| {
                            (index == *slot as usize).then_some(value)
                        })?;
                        if Behaviors::checked_vtable_of(&state.heap, value) != runtime.integer_behavior()
                            || !state.heap.payload_size(value).is_some_and(|size| size >= runtime.integer_storage().payload_size)
                        {
                            return None;
                        }
                        runtime.integer_value(value)
                    }
                };
            }
        }
        let ticket = Ticket {
            key,
            chain: self.clone(),
            buffers,
            next: 0,
        };
        // Queries are outside RuntimeState/registry borrows. The current native
        // declaration supplies arithmetic authority, not selector spelling.
        if !ticket.fusion_bindings_current(runtime) {
            return None;
        }
        Some(ticket)
    }

    #[cfg(feature = "ffi-test-hooks")]
    pub(in crate::runtime) fn loop_resources(&self) -> (usize, usize, usize) {
        self.loop_reuse.as_ref().map_or((0, 0, 0), |reuse| {
            let (cached, bytes) = reuse.resources();
            (1, cached, bytes)
        })
    }

    pub(in crate::runtime) fn new(template: Rc<Template>, bindings: Vec<Binding>) -> Option<Self> {
        let operators = bindings
            .iter()
            .map(|binding| binding.operator)
            .collect::<Vec<_>>();
        let plan = template.instantiate(&operators)?;
        let reuse_enabled = loop_reuse::enabled();
        let source = CString::new(if reuse_enabled {
            plan.emit_loop_reuse()
        } else {
            plan.emit()
        })
        .ok()?;
        // The validated emitter makes bounded accesses and has no callbacks.
        let unit = unsafe { Unit::compile(&source, &[]) }?;
        let kernel = unsafe { core::mem::transmute(unit.symbol(c"numeric_chain")?.as_ptr()) };
        let loop_reuse = if reuse_enabled {
            let reuse_kernel = unsafe {
                core::mem::transmute::<*mut core::ffi::c_void, loop_reuse::Kernel>(
                    unit.symbol(c"numeric_chain_reuse")?.as_ptr(),
                )
            };
            Some(loop_reuse::Reuse::new(reuse_kernel, &plan))
        } else {
            None
        };
        Some(Self {
            template,
            bindings,
            kernel,
            _unit: unit,
            loop_reuse,
        })
    }

    fn predict(
        self: &Rc<Self>,
        key: usize,
        mut buffers: BufferLease,
        stamp: Option<crate::execution::loop_iteration::Stamp>,
    ) -> Option<Ticket> {
        if buffers.values.inputs.len() != self.template.inputs().len()
            || buffers.values.outputs.len() != self.bindings.len()
        {
            return None;
        }
        // Both disjoint arrays have the exact extents validated by Plan.
        // This owner pins the unit until the call has returned.
        let success = if let Some(reuse) = &self.loop_reuse {
            reuse.predict(self.kernel, stamp, &mut buffers.values)
        } else {
            unsafe {
                (self.kernel)(
                    buffers.values.inputs.as_ptr(),
                    buffers.values.outputs.as_mut_ptr(),
                ) == 0
            }
        };
        if !success {
            return None;
        }
        Some(Ticket {
            key,
            chain: self.clone(),
            buffers,
            next: 0,
        })
    }
}

pub(in crate::runtime) struct Prediction {
    operator: Operator,
    left: i64,
    right: i64,
    result: i64,
    method: NativeEntry,
    summary: Rc<NumericSummary>,
}
impl Prediction {
    pub(super) fn summary(&self, method: NativeEntry) -> Option<&NumericSummary> {
        (self.method == method).then_some(self.summary.as_ref())
    }
    pub(super) fn matches(&self, operator: Operator, left: i64, right: i64) -> bool {
        self.operator == operator && self.left == left && self.right == right
    }
    pub(super) fn result(&self) -> i64 {
        self.result
    }
}

pub(in crate::runtime) struct Ticket {
    key: usize,
    chain: Rc<CompiledChain>,
    // Keep speculative future bits in untraced host buffers, not a native
    // stack array where conservative collection could mistake them for Oops.
    buffers: BufferLease,
    next: usize,
}
impl Ticket {
    fn evaluate_entry(mut self) -> Option<Self> {
        // This is a callee preflight, not an observed caller tail iteration.
        // Do not consume or replace loop-reuse history.
        #[cfg(any(test, feature = "ffi-test-hooks"))]
        ENTRY_PREDICTION_KERNELS.set(ENTRY_PREDICTION_KERNELS.get() + 1);
        let success = unsafe {
            (self.chain.kernel)(
                self.buffers.values.inputs.as_ptr(),
                self.buffers.values.outputs.as_mut_ptr(),
            ) == 0
        };
        success.then_some(self)
    }

    pub(in crate::runtime) fn fusion_template(&self) -> &Template {
        &self.chain.template
    }

    pub(in crate::runtime) fn fusion_result(&self, index: usize, pc: usize) -> Option<i64> {
        (self.chain.template.sends().get(index)?.pc as usize == pc)
            .then(|| self.buffers.values.outputs[index])
    }

    pub(in crate::runtime) fn fusion_bindings_current(&self, runtime: &Runtime) -> bool {
        {
            let state = unsafe { &*runtime.state.get() };
            let vt = runtime.integer_behavior();
            if Behaviors::checked_vtable_of(&state.heap, vt) != state.globals.vtable_vt {
                return false;
            }
            for (send, binding) in self.chain.template.sends().iter().zip(&self.chain.bindings) {
                let Some(symbol) = state.symbols.existing(&send.selector) else {
                    return false;
                };
                let method = binding.method;
                if state.vtables.native_lookup(vt, symbol) != Some(method)
                    || crate::legacy_method::try_decode(method).is_some_and(|method| state.execution.has_resumable_thunk(method))
                {
                    return false;
                }
            }
        }
        // Pure native queries must not run under a RuntimeState borrow.
        self.chain.bindings.iter().all(|binding| {
            binding.summary.operator(runtime) == Some(binding.operator)
        })
    }

    pub(in crate::runtime) fn take(&mut self, key: usize, pc: usize) -> Option<Prediction> {
        let send = self.chain.template.sends().get(self.next)?;
        if self.key != key || send.pc as usize != pc {
            self.next = self.buffers.values.outputs.len();
            return None;
        }
        let value = |v| match v {
            Value::Input(i) => self.buffers.values.inputs[i as usize],
            Value::Result(i) => self.buffers.values.outputs[i as usize],
        };
        let binding = &self.chain.bindings[self.next];
        let prediction = Prediction {
            operator: binding.operator,
            left: value(send.left),
            right: value(send.right),
            result: self.buffers.values.outputs[self.next],
            method: binding.method,
            summary: binding.summary.clone(),
        };
        self.next += 1;
        Some(prediction)
    }
}

impl Runtime {
    pub(in crate::runtime) fn prepare_numeric_chain(
        &self,
        capability: MachineCapability,
        owner: u64,
        key: usize,
    ) -> Option<Ticket> {
        // The real driver integration is exercised by test builds. Per-call validation currently costs
        // more than the arithmetic it replaces; retain the established boxed
        // consumer in ordinary builds until admission becomes profitable.
        #[cfg(not(test))]
        let enabled = false;
        #[cfg(test)]
        let enabled = ENABLED.get() && CHAIN_ENABLED.get();
        if !enabled {
            return None;
        }
        self.prepare_numeric_chain_candidate(capability, owner, key)
    }

    pub(in crate::runtime) fn prepare_numeric_chain_candidate(
        &self,
        capability: MachineCapability,
        owner: u64,
        key: usize,
    ) -> Option<Ticket> {
        let (template, compiled) = self.native_tier.borrow_mut().numeric_template(key)?;
        let mut buffers = self
            .numeric
            .borrow_mut()
            .buffers
            .get_or_insert_with(Pool::default)
            .take(template.inputs().len(), template.sends().len());
        // Read speculative inputs without allocating, invoking Id or retaining
        // references into the heap across a native declaration/compiler call.
        {
            let state = unsafe { &*self.state.get() };
            let _canonical = state.canonical?;
            let error = state.symbols.existing(b"ErrorVt")?;
            if state.environment.import(error) == self.integer_behavior() {
                return None;
            }
            let arena = state.execution.machine(owner, capability).ok()?.arena();
            let frame = arena.current_handle()?;
            let descriptor = arena.descriptor(frame).ok()?;
            if descriptor as *const FrameDescriptor as usize != key {
                return None;
            }
            for (destination, input) in buffers.values.inputs.iter_mut().zip(template.inputs()) {
                *destination = match input {
                    Input::Literal { value, .. } => *value,
                    Input::Frame(slot) => {
                        let oop = arena.read_oop(frame, *slot as usize).ok()?;
                        if Behaviors::checked_vtable_of(&state.heap, oop) != self.integer_behavior()
                            || !state.heap.payload_size(oop).is_some_and(|size| size >= self.integer_storage().payload_size)
                        {
                            return None;
                        }
                        self.integer_value(oop)
                    }
                };
            }
        }
        let stamp = || {
            let state = unsafe { &*self.state.get() };
            state
                .execution
                .machine(owner, capability)
                .ok()?
                .arena()
                .caller_loop_stamp()
        };
        if let Some(compiled) = compiled {
            let loop_stamp = compiled.loop_reuse.as_ref().and_then(|_| stamp());
            return compiled.predict(key, buffers, loop_stamp);
        }
        // Dictionary lookup is only needed to prepare a new kernel. A cached
        // binding is just a prediction: actual dispatch and native-context
        // validation below remain mandatory even after arbitrary mutation.
        let methods = {
            let state = unsafe { &*self.state.get() };
            template
                .sends()
                .iter()
                .map(|send| {
                    let symbol = state.symbols.existing(&send.selector)?;
                    let method = state.vtables.native_lookup(self.integer_behavior(), symbol)?;
                    if crate::legacy_method::try_decode(method).is_some_and(|method| state.execution.has_resumable_thunk(method)) {
                        return None;
                    }
                    Some(method)
                })
                .collect::<Option<Vec<_>>>()?
        };
        let bindings = methods
            .into_iter()
            .map(|method| {
                // Predictions do not count as calls for the hotness gate.
                let summary = self
                    .numeric
                    .borrow()
                    .records
                    .get(&super::entry_key(method))?
                    .summary
                    .clone()?;
                let operator = summary.operator(self)?;
                Some(Binding {
                    method,
                    summary,
                    operator,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let compiled = self.native_tier.borrow_mut().numeric_chain(key, bindings)?;
        let loop_stamp = compiled.loop_reuse.as_ref().and_then(|_| stamp());
        compiled.predict(key, buffers, loop_stamp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_buffer_leases_survive_cache_replacement_and_bound_retention() {
        let pool = Pool::default();
        let weak = Rc::downgrade(&pool.free);
        let mut first = pool.take(2, 3);
        first.values.inputs.copy_from_slice(&[11, 13]);
        first.values.outputs.copy_from_slice(&[24, 37, 61]);
        let mut nested = pool.take(2, 3);
        nested.values.inputs.fill(99);
        nested.values.outputs.fill(100);
        assert_eq!(first.values.inputs, [11, 13]);
        assert_eq!(first.values.outputs, [24, 37, 61]);
        assert_ne!(first.values.inputs.as_ptr(), nested.values.inputs.as_ptr());
        drop(pool);
        drop(nested);
        assert!(weak.upgrade().is_some());
        assert_eq!(first.values.outputs[2], 61);
        drop(first);
        assert!(
            weak.upgrade().is_none(),
            "an evicted pool dies with its final active lease"
        );

        let pool = Pool::default();
        let leases = (0..12).map(|_| pool.take(64, 32)).collect::<Vec<_>>();
        drop(leases);
        assert_eq!(pool.free.borrow().len(), MAX_FREE_BUFFERS);
    }
}
