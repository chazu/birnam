//! Raw numeric reuse only between consecutive observed caller tail iterations.
use super::*;
use crate::execution::loop_iteration::Stamp;

pub(super) type Kernel = unsafe extern "C" fn(*const i64, *mut i64, u64) -> usize;
#[cfg(test)]
thread_local! {
    pub(in crate::runtime) static ENABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
#[cfg(any(test, feature = "ffi-test-hooks"))]
thread_local! {
    pub(in crate::runtime) static PREDICTED_OPERATIONS_REUSED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(super) fn enabled() -> bool {
    #[cfg(test)]
    {
        ENABLED.get()
    }
    #[cfg(not(test))]
    {
        std::env::var("BIRNAM_DEV_LOOP_INVARIANTS").is_ok_and(|v| v == "1")
    }
}

struct Previous {
    stamp: Stamp,
    inputs: Vec<i64>,
    outputs: Vec<i64>,
}

pub(super) struct Reuse {
    kernel: Kernel,
    #[cfg(any(test, feature = "ffi-test-hooks"))]
    masks: Vec<u64>,
    previous: RefCell<Option<Previous>>,
}

impl Reuse {
    pub(super) fn new(kernel: Kernel, _plan: &birnam_codegen::numeric_chain::Plan) -> Self {
        Self {
            kernel,
            #[cfg(any(test, feature = "ffi-test-hooks"))]
            masks: _plan.input_dependencies(),
            previous: RefCell::new(None),
        }
    }

    /// Owned reuse storage only: excludes shared descriptors, allocator overhead
    /// and executable pages. Include spare vector capacity, including test masks.
    #[cfg(feature = "ffi-test-hooks")]
    pub(super) fn resources(&self) -> (usize, usize) {
        let previous = self.previous.borrow();
        let payload = previous.as_ref().map_or(0, |p| {
            (p.inputs.capacity() + p.outputs.capacity()) * core::mem::size_of::<i64>()
        });
        (
            usize::from(previous.is_some()),
            core::mem::size_of::<Self>()
                + payload
                + self.masks.capacity() * core::mem::size_of::<u64>(),
        )
    }

    pub(super) fn predict(
        &self,
        ordinary: unsafe extern "C" fn(*const i64, *mut i64) -> usize,
        stamp: Option<Stamp>,
        values: &mut Buffers,
    ) -> bool {
        let mut previous = self.previous.borrow_mut();
        let reusable = stamp
            .as_ref()
            .zip(previous.as_ref())
            .filter(|(stamp, old)| stamp.follows(&old.stamp));
        let failed = if let Some((_, old)) = reusable {
            let changed = old
                .inputs
                .iter()
                .zip(&values.inputs)
                .enumerate()
                .fold(0u64, |mask, (index, (a, b))| {
                    mask | (u64::from(a != b) << index)
                });
            values.outputs.copy_from_slice(&old.outputs);
            let failed = unsafe {
                (self.kernel)(values.inputs.as_ptr(), values.outputs.as_mut_ptr(), changed)
            };
            #[cfg(any(test, feature = "ffi-test-hooks"))]
            if failed == 0 {
                PREDICTED_OPERATIONS_REUSED.set(
                    PREDICTED_OPERATIONS_REUSED.get()
                        + self
                            .masks
                            .iter()
                            .filter(|&&mask| mask & changed == 0)
                            .count(),
                );
            }
            failed
        } else {
            unsafe { ordinary(values.inputs.as_ptr(), values.outputs.as_mut_ptr()) }
        };
        if failed != 0 {
            *previous = None;
            return false;
        }
        if let Some(stamp) = stamp {
            if let Some(old) = previous.as_mut() {
                old.stamp = stamp;
                old.inputs.clone_from(&values.inputs);
                old.outputs.clone_from(&values.outputs);
            } else {
                *previous = Some(Previous {
                    stamp,
                    inputs: values.inputs.clone(),
                    outputs: values.outputs.clone(),
                });
            }
        } else {
            *previous = None;
        }
        true
    }
}

#[cfg(test)]
mod tests;
