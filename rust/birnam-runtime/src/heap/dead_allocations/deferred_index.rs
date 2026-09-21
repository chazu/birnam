//! Copied native rows. Lookup, ordering and displaced-index repair remain in Id.
use super::DeferredAllocation;
use birnam_abi::deferred_index::{FactoryV1, RowV1};
use core::{ffi::c_void, ptr};

pub(in crate::heap) struct Index {
    context: *mut c_void,
    factory: FactoryV1,
}
impl Index {
    pub(in crate::heap) fn new() -> Self {
        #[cfg(test)]
        let factory = unsafe { *birnam_storage::birnam_deferred_index_factory_v1() };
        #[cfg(not(test))]
        let factory = unsafe { birnam_abi::deferred_index::operations() };
        Self {
            context: ptr::null_mut(),
            factory,
        }
    }
    pub(in crate::heap) fn len(&self) -> usize {
        unsafe { self.factory.len.unwrap()(self.context) }
    }
    pub(in crate::heap) fn capacity(&self) -> usize {
        unsafe { self.factory.capacity.unwrap()(self.context) }
    }
    pub(in crate::heap) fn push(&mut self, (identity, index): (DeferredAllocation, usize)) {
        unsafe {
            self.factory.push.unwrap()(
                &mut self.context,
                RowV1 {
                    identity: identity.0,
                    index,
                },
            )
        };
    }
    pub(in crate::heap) fn get(&self, position: usize) -> (DeferredAllocation, usize) {
        let mut row = RowV1::default();
        assert_eq!(
            unsafe { self.factory.read.unwrap()(self.context, position, &mut row) },
            1
        );
        (DeferredAllocation(row.identity), row.index)
    }
    pub(in crate::heap) fn set(
        &mut self,
        position: usize,
        (identity, index): (DeferredAllocation, usize),
    ) {
        assert_eq!(
            unsafe {
                self.factory.write.unwrap()(
                    self.context,
                    position,
                    RowV1 {
                        identity: identity.0,
                        index,
                    },
                )
            },
            1
        );
    }
    pub(in crate::heap) fn swap_remove(&mut self, position: usize) -> (DeferredAllocation, usize) {
        let mut row = RowV1::default();
        assert_eq!(
            unsafe { self.factory.swap_remove.unwrap()(self.context, position, &mut row) },
            1
        );
        (DeferredAllocation(row.identity), row.index)
    }
    pub(in crate::heap) fn clear(&mut self) {
        unsafe { self.factory.clear.unwrap()(self.context) };
    }
    pub(in crate::heap) fn iter(&self) -> impl Iterator<Item = (DeferredAllocation, usize)> + '_ {
        (0..self.len()).map(|position| self.get(position))
    }
}
impl Drop for Index {
    fn drop(&mut self) {
        unsafe { self.factory.destroy.unwrap()(self.context) };
    }
}
