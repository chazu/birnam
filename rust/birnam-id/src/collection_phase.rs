//! Per-Runtime owner handle and Id's existing phase-assignment diagnostic.
use birnam_abi::collect::{FINALIZING, IDLE, MARKING};
use birnam_abi::collection_phase::FactoryV1;
use core::{ffi::c_void, ptr::NonNull};

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum Phase {
    Idle,
    Marking,
    Finalizing,
}
pub(crate) struct Service {
    context: NonNull<c_void>,
    factory: FactoryV1,
}
impl Service {
    pub(crate) fn new() -> Self {
        #[cfg(test)]
        let factory = unsafe { *birnam_collect::birnam_collection_phase_factory_v1() };
        #[cfg(not(test))]
        let factory = unsafe { birnam_abi::collection_phase::operations() };
        let context = NonNull::new(unsafe { factory.create.unwrap()() })
            .expect("collection phase creation failed");
        Self { context, factory }
    }
    pub(crate) fn get(&self) -> Phase {
        match unsafe { self.factory.read.unwrap()(self.context.as_ptr()) } {
            IDLE => Phase::Idle,
            MARKING => Phase::Marking,
            FINALIZING => Phase::Finalizing,
            _ => crate::boundary::fatal_invariant("invalid collection phase"),
        }
    }
    pub(crate) fn set(&self, phase: Phase) {
        self.set_raw(match phase {
            Phase::Idle => IDLE,
            Phase::Marking => MARKING,
            Phase::Finalizing => FINALIZING,
        });
    }
    pub(crate) fn set_raw(&self, phase: u32) {
        crate::boundary::require_invariant(
            unsafe { self.factory.set.unwrap()(self.context.as_ptr(), phase) } == 1,
            "invalid collection phase",
        );
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        unsafe { self.factory.destroy.unwrap()(self.context.as_ptr()) };
    }
}
