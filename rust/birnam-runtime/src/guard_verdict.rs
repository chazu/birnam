//! Opaque guard-verdict owner with the original tuple get/set behavior.
use birnam_abi::guard_verdict::{FactoryV1, StateV1};
use core::{ffi::c_void, ptr::NonNull};

pub(crate) struct Service {
    context: NonNull<c_void>,
    factory: FactoryV1,
}
impl Service {
    pub(crate) fn with_factory(factory: FactoryV1) -> Self {
        let context = NonNull::new(unsafe { factory.create.unwrap()() }).expect("guard verdict creation failed");
        Self { context, factory }
    }
    pub(crate) fn get(&self) -> (u64, bool) {
        let mut state = StateV1 { epoch: 0, verified: 0 };
        unsafe { self.factory.read.unwrap()(self.context.as_ptr(), &raw mut state) };
        (state.epoch, state.verified != 0)
    }
    pub(crate) fn set(&self, state: (u64, bool)) {
        unsafe { self.factory.write.unwrap()(self.context.as_ptr(), state.0, i32::from(state.1)) };
    }
}
impl Drop for Service {
    fn drop(&mut self) { unsafe { self.factory.destroy.unwrap()(self.context.as_ptr()) }; }
}
