//! Retained whole-region numeric prediction for batch 2 compiler integration.
//! Per-call hotness and checked evaluation now live in birnam-execution; Integer
//! owns exact executable/context admission, input reads and result construction.
use super::*;
use crate::loader::NumericSummary;
use std::rc::Rc;
#[derive(Default)]
pub(super) struct Registry {
    // Retained producer hint consumed by the deferred whole-region planner.
    records: std::collections::BTreeMap<(usize, usize), Record>,
    buffers: Option<chain::Pool>,
}
struct Record {
    summary: Option<Rc<NumericSummary>>,
}
fn entry_key(entry: NativeEntry) -> (usize, usize) {
    (
        entry.method.map_or(0, |f| f as usize),
        entry.context as usize,
    )
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
