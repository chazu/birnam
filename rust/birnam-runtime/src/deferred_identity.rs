//! Opaque identity storage; issuance and exhaustion remain in Heap.
use birnam_abi::deferred_identity::FactoryV1;
use core::{ffi::c_void, ptr::NonNull};

pub(crate) struct Service {
    context: NonNull<c_void>,
    factory: FactoryV1,
}
impl Service {
    pub(crate) fn with_factory(factory: FactoryV1) -> Self {
        let context = NonNull::new(unsafe { factory.create.unwrap()() })
            .expect("deferred identity creation failed");
        Self { context, factory }
    }
    pub(crate) fn get(&self) -> u64 {
        unsafe { self.factory.read.unwrap()(self.context.as_ptr()) }
    }
    pub(crate) fn set(&mut self, value: u64) {
        unsafe { self.factory.write.unwrap()(self.context.as_ptr(), value) };
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        unsafe { self.factory.destroy.unwrap()(self.context.as_ptr()) };
    }
}
