/// Copied coordinates for scanning one live object's pointer-sized payload
/// words without allocating a per-object snapshot.
///
/// This deliberately carries no Rust borrow of the allocation registry: the collector must
/// mutate mark metadata while reading the separately allocated object payload.
#[derive(Clone, Copy)]
pub(crate) struct PayloadWords {
    pointer: *const usize,
    length: usize,
}

impl PayloadWords {
    /// # Safety
    /// The caller retains this exact aligned live payload and its complete extent.
    pub(crate) unsafe fn from_raw_parts(pointer: *const usize, length: usize) -> Self {
        Self { pointer, length }
    }
    pub(crate) fn len(self) -> usize {
        self.length
    }

    /// Read one word from the described live payload.
    ///
    /// # Safety
    ///
    /// The originating block must remain registered and its payload must not
    /// be mutated for the duration of the scan. `index` must be below `len`.
    pub(crate) unsafe fn read(self, index: usize) -> usize {
        debug_assert!(index < self.length);
        // SAFETY: forwarded caller contract keeps the live aligned allocation
        // stable and bounds index to this payload range.
        unsafe { self.pointer.add(index).read() }
    }
}
