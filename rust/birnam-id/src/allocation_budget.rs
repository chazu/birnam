//! Borrow Collect's budget in production; legacy Heaps construct their own.
use birnam_abi::allocation_budget::FactoryV1;
use core::ffi::c_void;
use core::ptr::NonNull;

pub(crate) struct Service {
    context: NonNull<c_void>,
    factory: FactoryV1,
    native: Option<crate::collection_state::Shared>,
}
impl Service {
    pub(crate) fn from_collection(owner: crate::collection_state::Shared) -> Self {
        Self { context: NonNull::new(owner.0.budget_context).unwrap(), factory: unsafe { *owner.0.budget_factory }, native: Some(owner) }
    }
    pub(crate) fn new() -> Self {
        #[cfg(test)]
        let factory = unsafe { *birnam_allocation_budget::birnam_allocation_budget_factory_v1() };
        #[cfg(not(test))]
        let factory = unsafe { birnam_abi::allocation_budget::operations() };
        let context = NonNull::new(unsafe { factory.create.unwrap()() })
            .expect("allocation budget creation failed");
        Self { context, factory, native: None }
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
        if self.native.is_none() { unsafe { self.factory.destroy.unwrap()(self.context.as_ptr()) }; }
    }
}
