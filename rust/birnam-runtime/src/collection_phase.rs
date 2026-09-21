//! Borrow Collect's phase in production; preserve legacy assignment diagnostics.
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
    native: Option<crate::collection_state::Shared>,
}
impl Service {
    pub(crate) fn from_collection(owner: crate::collection_state::Shared) -> Self {
        Self { context: NonNull::new(owner.0.phase_context).unwrap(), factory: unsafe { *owner.0.phase_factory }, native: Some(owner) }
    }
    pub(crate) fn new() -> Self {
        #[cfg(test)]
        let factory = unsafe { *birnam_collect::collection_phase_factory() };
        #[cfg(not(test))]
        let factory = unsafe { birnam_abi::collection_phase::operations() };
        let context = NonNull::new(unsafe { factory.create.unwrap()() })
            .expect("collection phase creation failed");
        Self { context, factory, native: None }
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
        if self.native.is_none() { unsafe { self.factory.destroy.unwrap()(self.context.as_ptr()) }; }
    }
}
