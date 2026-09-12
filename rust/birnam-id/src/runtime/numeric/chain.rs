//! Speculative raw arithmetic, validated against actual operands at each call.
//!
//! A ticket holds raw integers and owned compiler/declaration metadata, never
//! borrowed Frame words or unrooted Oops.
//! Every original send and allocation still runs at its ordinary boundary.
//! Even an intervening collector or observing lookup may change execution:
//! the next call must independently match its current native declaration and
//! both current operands before a predicted result can replace arithmetic.
use super::*;
use birnam_codegen::numeric_chain::{Input, Operator, Template, Value};
use birnam_native_compiler::Unit;
use std::cell::RefCell;
use std::ffi::CString;

// A small per-runtime pool avoids allocating temporary host arrays on each
// invocation. Leases own the pool so cache clearing and nested drives cannot
// invalidate active storage. Free buffers contain no traced Birnam references.
const MAX_FREE_BUFFERS: usize = 4;
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
}

/// A prediction's native declaration is an owned lookup hint. Its current
/// context is queried again when the actual resolved Method matches the hint.
pub(in crate::runtime) struct Binding {
    pub(in crate::runtime) method: Method,
    pub(in crate::runtime) summary: Rc<NumericSummary>,
    pub(in crate::runtime) operator: Operator,
}

impl CompiledChain {
    pub(in crate::runtime) fn new(template: Rc<Template>, bindings: Vec<Binding>) -> Option<Self> {
        let operators = bindings
            .iter()
            .map(|binding| binding.operator)
            .collect::<Vec<_>>();
        let plan = template.instantiate(&operators)?;
        let source = CString::new(plan.emit()).ok()?;
        // The validated emitter makes bounded accesses and has no callbacks.
        let unit = unsafe { Unit::compile(&source, &[]) }?;
        let kernel = unsafe { core::mem::transmute(unit.symbol(c"numeric_chain")?.as_ptr()) };
        Some(Self {
            template,
            bindings,
            kernel,
            _unit: unit,
        })
    }

    fn predict(self: &Rc<Self>, key: usize, mut buffers: BufferLease) -> Option<Ticket> {
        if buffers.values.inputs.len() != self.template.inputs().len()
            || buffers.values.outputs.len() != self.bindings.len()
        {
            return None;
        }
        // Both disjoint arrays have the exact extents validated by Plan.
        // This owner pins the unit until the call has returned.
        if unsafe {
            (self.kernel)(
                buffers.values.inputs.as_ptr(),
                buffers.values.outputs.as_mut_ptr(),
            )
        } != 0
        {
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
    method: Method,
    summary: Rc<NumericSummary>,
}
impl Prediction {
    pub(super) fn summary(&self, method: Method) -> Option<&NumericSummary> {
        (self.method as usize == method as usize).then_some(self.summary.as_ref())
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
            let vt = runtime.canonical().smallint_vt;
            if Vtables::checked_vtable_of(&state.heap, vt) != state.globals.vtable_vt {
                return false;
            }
            for (send, binding) in self.chain.template.sends().iter().zip(&self.chain.bindings) {
                let Some(symbol) = state.symbols.existing(&send.selector) else {
                    return false;
                };
                let method = MethodAddress::from_method(binding.method);
                if state.vtables.lookup(vt, symbol) != Some(method)
                    || state.execution.has_resumable_thunk(method)
                {
                    return false;
                }
            }
        }
        // Pure native queries must not run under a RuntimeState borrow.
        self.chain.bindings.iter().all(|binding| {
            binding.summary.operator(runtime.canonical().smallint_vt) == Some(binding.operator)
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
            let canonical = state.canonical?;
            let error = state.symbols.existing(b"ErrorVt")?;
            if state.environment.import(error) == canonical.smallint_vt {
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
                        if Vtables::checked_vtable_of(&state.heap, oop) != canonical.smallint_vt
                            || !state.heap.payload_size(oop).is_some_and(|size| size >= 8)
                        {
                            return None;
                        }
                        state.values.integer_value(oop)
                    }
                };
            }
        }
        if let Some(compiled) = compiled {
            return compiled.predict(key, buffers);
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
                    let method = state.vtables.lookup(self.canonical().smallint_vt, symbol)?;
                    if state.execution.has_resumable_thunk(method) {
                        return None;
                    }
                    Some(method.to_method())
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
                    .get(&(method as usize))?
                    .summary
                    .clone()?;
                let operator = summary.operator(self.canonical().smallint_vt)?;
                Some(Binding {
                    method,
                    summary,
                    operator,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let compiled = self.native_tier.borrow_mut().numeric_chain(key, bindings)?;
        compiled.predict(key, buffers)
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
