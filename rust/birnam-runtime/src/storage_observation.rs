//! Opaque per-Runtime scope owner; callers retain their original admission order.
use birnam_abi::storage_observation::{FactoryV1, StateV1};
use core::{ffi::c_void, ptr::NonNull};

pub(crate) struct Service {
    context: NonNull<c_void>,
    factory: FactoryV1,
}
impl Service {
    pub(crate) fn with_factory(factory: FactoryV1) -> Self {
        let context = NonNull::new(unsafe { factory.create.unwrap()() })
            .expect("storage observation creation failed");
        Self { context, factory }
    }
    fn read(&self) -> StateV1 {
        let mut state = StateV1::default();
        unsafe { self.factory.read.unwrap()(self.context.as_ptr(), &raw mut state) };
        state
    }
    pub(crate) fn active(&self) -> bool {
        self.read().active != 0
    }
    pub(crate) fn permit(&self) -> bool {
        self.read().permit != 0
    }
    pub(crate) fn set_active(&self, enabled: bool) {
        unsafe { self.factory.set_active.unwrap()(self.context.as_ptr(), i32::from(enabled)) };
    }
    pub(crate) fn set_permit(&self, enabled: bool) {
        unsafe { self.factory.set_permit.unwrap()(self.context.as_ptr(), i32::from(enabled)) };
    }
    pub(crate) fn advance_epoch(&self) -> Option<u64> {
        let mut epoch = 0;
        (unsafe { self.factory.advance_epoch.unwrap()(self.context.as_ptr(), &raw mut epoch) } == 1)
            .then_some(epoch)
    }
    #[cfg(test)]
    pub(crate) fn refuse_advancement_for_test(&mut self) {
        unsafe extern "C" fn refuse(context: *mut c_void, _: *mut u64) -> i32 {
            let factory = unsafe { *birnam_storage::birnam_storage_observation_factory_v1() };
            let mut state = StateV1::default();
            unsafe { factory.read.unwrap()(context, &raw mut state) };
            assert_eq!(
                (state.active, state.permit),
                (0, 0),
                "permission must be consumed before advancement"
            );
            0
        }
        self.factory.advance_epoch = Some(refuse);
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        unsafe { self.factory.destroy.unwrap()(self.context.as_ptr()) };
    }
}
