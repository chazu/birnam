//! Own the module's budget for this Heap; never call Id while heap state is borrowed.
use birnam_abi::allocation_budget::FactoryV1;
use core::ffi::c_void;
use core::ptr::NonNull;

pub(crate) struct Service {
    context: NonNull<c_void>,
    factory: FactoryV1,
}
impl Service {
    pub(crate) fn new() -> Self {
        #[cfg(test)]
        let factory = unsafe { *birnam_allocation_budget::birnam_allocation_budget_factory_v1() };
        #[cfg(not(test))]
        let factory = unsafe { birnam_abi::allocation_budget::operations() };
        let context = NonNull::new(unsafe { factory.create.unwrap()() })
            .expect("allocation budget creation failed");
        Self { context, factory }
    }
    pub(crate) fn read(&self) -> usize {
        unsafe { self.factory.read.unwrap()(self.context.as_ptr()) }
    }
    pub(crate) fn replace(&mut self, bytes: usize) {
        unsafe { self.factory.replace.unwrap()(self.context.as_ptr(), bytes) };
    }
    pub(crate) fn charge(&mut self, bytes: usize) -> bool {
        unsafe { self.factory.charge.unwrap()(self.context.as_ptr(), bytes) == 1 }
    }
    pub(crate) fn preview(&self, bytes: usize) -> Option<usize> {
        let mut total = 0;
        (unsafe { self.factory.preview.unwrap()(self.context.as_ptr(), bytes, &raw mut total) }
            == 1)
            .then_some(total)
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        unsafe { self.factory.destroy.unwrap()(self.context.as_ptr()) };
    }
}
