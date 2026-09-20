//! Opaque epoch storage; mutation authority and saturation remain in Vtables.
use birnam_abi::mutation_epoch::FactoryV1;
use core::{ffi::c_void, ptr::NonNull};

pub(crate) struct Service {
    context: NonNull<c_void>,
    factory: FactoryV1,
}
impl Service {
    pub(crate) fn new() -> Self {
        #[cfg(test)]
        let factory = unsafe { *birnam_object_model::birnam_mutation_epoch_factory_v1() };
        #[cfg(not(test))]
        let factory = unsafe { birnam_abi::mutation_epoch::operations() };
        let context = NonNull::new(unsafe { factory.create.unwrap()() }).expect("mutation epoch creation failed");
        Self { context, factory }
    }
    pub(crate) fn get(&self) -> u64 {
        unsafe { self.factory.read.unwrap()(self.context.as_ptr()) }
    }
    pub(crate) fn set(&mut self, epoch: u64) {
        unsafe { self.factory.write.unwrap()(self.context.as_ptr(), epoch) };
    }
}
impl Drop for Service {
    fn drop(&mut self) { unsafe { self.factory.destroy.unwrap()(self.context.as_ptr()) }; }
}
