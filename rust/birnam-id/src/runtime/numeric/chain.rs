//! Speculative raw arithmetic, validated against actual operands at each call.
//!
//! A ticket holds only integers, never borrowed Frame words or unrooted Oops.
//! Every original send and allocation still runs at its ordinary boundary.
//! Even an intervening collector or observing lookup may change execution:
//! the next call must independently match its current native declaration and
//! both current operands before a predicted result can replace arithmetic.
use super::*;
use birnam_codegen::numeric_chain::{Input, Operator, Template, Value};
use birnam_native_compiler::Unit;
use std::ffi::CString;

pub(in crate::runtime) struct CompiledChain {
    template: Rc<Template>,
    operators: Vec<Operator>,
    kernel: unsafe extern "C" fn(*const i64, *mut i64) -> usize,
    _unit: Unit,
}

impl CompiledChain {
    pub(in crate::runtime) fn new(template: Rc<Template>, operators: &[Operator]) -> Option<Self> {
        let plan = template.instantiate(operators)?;
        let source = CString::new(plan.emit()).ok()?;
        // The validated emitter makes bounded accesses and has no callbacks.
        let unit = unsafe { Unit::compile(&source, &[]) }?;
        let kernel = unsafe { core::mem::transmute(unit.symbol(c"numeric_chain")?.as_ptr()) };
        Some(Self {
            template,
            operators: operators.to_vec(),
            kernel,
            _unit: unit,
        })
    }

    pub(in crate::runtime) fn operators(&self) -> &[Operator] {
        &self.operators
    }

    fn predict(&self, key: usize, inputs: &[i64]) -> Option<Ticket> {
        if inputs.len() != self.template.inputs().len() {
            return None;
        }
        let mut outputs = vec![0; self.operators.len()];
        // Both disjoint arrays have the exact extents validated by Plan.
        // This owner pins the unit until the call has returned. The ticket
        // contains plain data and does not depend on its executable lifetime.
        if unsafe { (self.kernel)(inputs.as_ptr(), outputs.as_mut_ptr()) } != 0 {
            return None;
        }
        let value = |v| match v {
            Value::Input(i) => inputs[i as usize],
            Value::Result(i) => outputs[i as usize],
        };
        let predictions = self
            .template
            .sends()
            .iter()
            .enumerate()
            .map(|(i, send)| {
                (
                    send.pc as usize,
                    Prediction {
                        operator: self.operators[i],
                        left: value(send.left),
                        right: value(send.right),
                        result: outputs[i],
                    },
                )
            })
            .collect::<Vec<_>>()
            .into_iter();
        Some(Ticket { key, predictions })
    }
}

#[derive(Clone, Copy)]
pub(in crate::runtime) struct Prediction {
    operator: Operator,
    left: i64,
    right: i64,
    result: i64,
}
impl Prediction {
    pub(super) fn matches(&self, operator: Operator, left: i64, right: i64) -> bool {
        self.operator == operator && self.left == left && self.right == right
    }
    pub(super) fn result(&self) -> i64 {
        self.result
    }
}

pub(in crate::runtime) struct Ticket {
    key: usize,
    predictions: std::vec::IntoIter<(usize, Prediction)>,
}
impl Ticket {
    pub(in crate::runtime) fn take(&mut self, key: usize, pc: usize) -> Option<Prediction> {
        if self.key != key {
            self.predictions = Vec::new().into_iter();
            return None;
        }
        let (expected_pc, prediction) = self.predictions.next()?;
        (expected_pc == pc).then_some(prediction)
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
        let template = self.native_tier.borrow().numeric_template(key)?;
        // Read speculative inputs without allocating, invoking Id or retaining
        // references into the heap across a native declaration/compiler call.
        let (inputs, methods) = {
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
            let inputs = template
                .inputs()
                .iter()
                .map(|input| match input {
                    Input::Literal { value, .. } => Some(*value),
                    Input::Frame(slot) => {
                        let oop = arena.read_oop(frame, *slot as usize).ok()?;
                        if Vtables::checked_vtable_of(&state.heap, oop) != canonical.smallint_vt
                            || !state.heap.payload_size(oop).is_some_and(|size| size >= 8)
                        {
                            return None;
                        }
                        Some(state.values.integer_value(oop))
                    }
                })
                .collect::<Option<Vec<_>>>()?;
            // Dictionary lookup is merely a prediction, never a substitute for
            // the actual late-bound dispatch, including custom lookup effects.
            let methods = template
                .sends()
                .iter()
                .map(|send| {
                    let symbol = state.symbols.existing(&send.selector)?;
                    let method = state.vtables.lookup(canonical.smallint_vt, symbol)?;
                    if state.execution.has_resumable_thunk(method) {
                        return None;
                    }
                    Some(method.to_method())
                })
                .collect::<Option<Vec<_>>>()?;
            (inputs, methods)
        };
        let operators = methods
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
                summary.operator(self.canonical().smallint_vt)
            })
            .collect::<Option<Vec<_>>>()?;
        let compiled = self
            .native_tier
            .borrow_mut()
            .numeric_chain(key, &operators)?;
        compiled.predict(key, &inputs)
    }
}
