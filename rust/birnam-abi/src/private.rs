//! Private process-local native carriers, independent of the frozen IdApi.
//! Sharing these declarations adds no runtime provider or dynamic dependency.

/// Versioned Store reservation callbacks. Never persisted in source or images.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct StoreCompensationApiV1 {
    pub abi_version: u32,
    pub byte_size: usize,
    pub reserve: unsafe extern "C" fn(usize, usize) -> core::ffi::c_int,
    pub consume: unsafe extern "C" fn(usize, usize) -> core::ffi::c_int,
    pub release: unsafe extern "C" fn(usize) -> usize,
    pub reserved: unsafe extern "C" fn(usize) -> usize,
}

/// Opaque lease on an overwritten resumable implementation's generation.
/// Consumers must return tokens unchanged to the floor that issued them.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct StacklessGenerationLeaseV1 {
    pub token_index: u64,
    pub token_generation: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{align_of, offset_of, size_of};

    #[test]
    fn private_carriers_match_frozen_arm64_c_layout() {
        // Numeric expectations are independent of the shared declarations.
        // Birnam currently supports only 64-bit arm64 macOS.
        assert_eq!(size_of::<StoreCompensationApiV1>(), 48);
        assert_eq!(align_of::<StoreCompensationApiV1>(), 8);
        assert_eq!(offset_of!(StoreCompensationApiV1, abi_version), 0);
        assert_eq!(offset_of!(StoreCompensationApiV1, byte_size), 8);
        assert_eq!(offset_of!(StoreCompensationApiV1, reserve), 16);
        assert_eq!(offset_of!(StoreCompensationApiV1, consume), 24);
        assert_eq!(offset_of!(StoreCompensationApiV1, release), 32);
        assert_eq!(offset_of!(StoreCompensationApiV1, reserved), 40);
        assert_eq!(size_of::<StacklessGenerationLeaseV1>(), 16);
        assert_eq!(align_of::<StacklessGenerationLeaseV1>(), 8);
        assert_eq!(offset_of!(StacklessGenerationLeaseV1, token_index), 0);
        assert_eq!(offset_of!(StacklessGenerationLeaseV1, token_generation), 8);
        assert_eq!(StacklessGenerationLeaseV1::default().token_index, 0);
        assert_eq!(StacklessGenerationLeaseV1::default().token_generation, 0);
    }
}
