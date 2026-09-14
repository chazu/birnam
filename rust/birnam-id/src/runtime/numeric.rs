//! Guarded successful numeric operations inside the ordinary native boundary.
//! Baseline Frames, dispatch effects and result allocation remain in place.
use super::*;
use crate::loader::NumericSummary;
use std::collections::BTreeMap;
use std::rc::Rc;

const HOT_CALLS: usize = 64;
const MAX_RECORDS: usize = 256;

#[derive(Default)]
pub(super) struct Registry {
    records: BTreeMap<usize, Record>,
    buffers: Option<chain::Pool>,
}

#[derive(Default)]
struct Record {
    visits: usize,
    queried: bool,
    summary: Option<Rc<NumericSummary>>,
}

impl Registry {
    fn observe(&mut self, method: Method) -> (bool, Option<Rc<NumericSummary>>) {
        let key = method as usize;
        if !self.records.contains_key(&key) && self.records.len() == MAX_RECORDS {
            self.records.pop_first();
        }
        let record = self.records.entry(key).or_default();
        record.visits = record.visits.saturating_add(1);
        let query = !record.queried && record.visits >= HOT_CALLS;
        if query {
            record.queried = true;
        }
        (query, record.summary.clone())
    }
}

impl Runtime {
    fn numeric_summary(&self, method: Method) -> Option<Rc<NumericSummary>> {
        let (query, mut summary) = self.numeric.borrow_mut().observe(method);
        if query {
            // Discovery enters a native declaration. No RuntimeState or
            // registry borrow survives that call. Native code owns its pure
            // declaration contract; the returned image pin is retained here.
            summary = NumericSummary::discover(method, self.canonical().smallint_vt).map(Rc::new);
            if let Some(record) = self
                .numeric
                .borrow_mut()
                .records
                .get_mut(&(method as usize))
            {
                record.summary = summary.clone();
            }
        }
        summary
    }

    #[cfg(test)]
    pub(super) fn try_numeric_call(
        &self,
        method: Method,
        receiver: Oop,
        arguments: &[Oop],
    ) -> Option<Oop> {
        self.try_numeric_call_with_prediction(method, receiver, arguments, None)
    }

    pub(super) fn try_numeric_call_with_prediction(
        &self,
        method: Method,
        receiver: Oop,
        arguments: &[Oop],
        prediction: Option<chain::Prediction>,
    ) -> Option<Oop> {
        #[cfg(test)]
        if !ENABLED.get() {
            return None;
        }
        let &[argument] = arguments else {
            return None;
        };
        // Do not intern anything on behalf of an optimization. Prelude's first
        // ErrorVt import, if needed, still executes in the original method.
        let (left, right, error_argument) = {
            let state = unsafe { &*self.state.get() };
            let canonical = state.canonical?;
            for value in [receiver, argument] {
                if Vtables::checked_vtable_of(&state.heap, value) != canonical.smallint_vt
                    || !state.heap.payload_size(value).is_some_and(|size| size >= 8)
                {
                    return None;
                }
            }
            let error_symbol = state.symbols.existing(b"ErrorVt")?;
            let error_argument = state.environment.import(error_symbol) == canonical.smallint_vt;
            (
                state.values.integer_value(receiver),
                state.values.integer_value(argument),
                error_argument,
            )
        };
        let predicted_summary = prediction.as_ref().and_then(|p| p.summary(method));
        let fallback_summary = if predicted_summary.is_none() {
            self.numeric_summary(method)
        } else {
            None
        };
        let summary = predicted_summary.or(fallback_summary.as_deref())?;
        let operator = summary.operator(self.canonical().smallint_vt)?;
        if error_argument {
            #[cfg(test)]
            HITS.set(HITS.get() + 1);
            return Some(argument);
        }
        let value = match prediction
            .as_ref()
            .filter(|p| p.matches(operator, left, right))
        {
            Some(prediction) => {
                #[cfg(test)]
                CHAIN_HITS.set(CHAIN_HITS.get() + 1);
                prediction.result()
            }
            None => operator.evaluate(left, right)?,
        };
        #[cfg(test)]
        HITS.set(HITS.get() + 1);
        // Stay inside the exact ordinary synchronous boundary. The allocation
        // can collect, observe the Frame or retire bindings. The summary's Rc
        // still pins its native image through this potentially observing call.
        let result = self.make_integer(value);
        drop(prediction);
        drop(fallback_summary);
        Some(result)
    }
}

#[cfg(test)]
thread_local! {
    pub(super) static ENABLED: Cell<bool> = const { Cell::new(true) };
    pub(super) static CHAIN_ENABLED: Cell<bool> = const { Cell::new(false) };
    pub(super) static CHAIN_HITS: Cell<usize> = const { Cell::new(0) };
    pub(super) static HITS: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
pub(super) mod tests;

mod chain;
#[cfg(any(test, feature = "ffi-test-hooks"))]
pub(super) use chain::ENTRY_PREDICTION_KERNELS;
#[cfg(any(test, feature = "ffi-test-hooks"))]
pub(in crate::runtime) use chain::loop_reuse;
pub(super) use chain::{Binding, CompiledChain, Ticket};
