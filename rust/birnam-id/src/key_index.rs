//! Derived positions only; ordered keys, cutoff and lookup decisions remain in Id.
use birnam_abi::key_index::FactoryV1;
use core::{ffi::c_void, ptr};

pub(crate) struct Index {
    context: *mut c_void,
    factory: FactoryV1,
}
impl Index {
    pub(crate) fn new() -> Self {
        #[cfg(test)]
        let factory = unsafe { *birnam_object_model::birnam_key_index_factory_v1() };
        #[cfg(not(test))]
        let factory = unsafe { birnam_abi::key_index::operations() };
        Self {
            context: ptr::null_mut(),
            factory,
        }
    }
    pub(crate) fn is_empty(&self) -> bool {
        unsafe { self.factory.is_empty.unwrap()(self.context) != 0 }
    }
    pub(crate) fn get(&self, key: usize) -> Option<usize> {
        let mut position = 0;
        (unsafe { self.factory.get.unwrap()(self.context, key, &mut position) } != 0)
            .then_some(position)
    }
    pub(crate) fn insert(&mut self, key: usize, position: usize) {
        // The module borrows this slot only until insert returns. Records may move.
        unsafe { self.factory.insert.unwrap()(&mut self.context, key, position) };
    }
    pub(crate) fn clear(&mut self) {
        unsafe { self.factory.clear.unwrap()(self.context) };
    }
}
impl Drop for Index {
    fn drop(&mut self) {
        unsafe { self.factory.destroy.unwrap()(self.context) };
    }
}
