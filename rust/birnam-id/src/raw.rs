//! Audited raw object, method, and pointer conversions.

use core::ffi::c_void;
use core::hash::{BuildHasherDefault, Hasher};
use core::ptr::NonNull;
use std::collections::HashMap;

use crate::abi::{Method, Oop};

/// Exact address hasher shared by heap and vtable membership registries.
#[derive(Default)]
pub(crate) struct AddressHasher(u64);

impl Hasher for AddressHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        // Fallback for completeness; usize keys use write_usize below.
        let mut hash = 0xcbf29ce484222325u64;
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        self.0 = hash;
    }

    fn write_usize(&mut self, address: usize) {
        self.0 = ((address >> 3) as u64).wrapping_mul(0x9e3779b97f4a7c15);
    }
}

pub(crate) type AddressMap<Value> = HashMap<usize, Value, BuildHasherDefault<AddressHasher>>;

/// A native Method address stored as inert dictionary data.
///
/// No Rust reference is created for foreign code. Conversion back to a callable
/// function pointer is isolated here and is valid on the frozen arm64 Mach-O
/// target where data and function pointers have the same representation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub(crate) struct MethodAddress(NonNull<c_void>);

impl MethodAddress {
    pub(crate) fn from_method(method: Method) -> Self {
        let pointer = method as *const () as *mut c_void;
        // Function pointers in the IdApi ABI are non-null.
        Self(NonNull::new(pointer).expect("Method must be non-null"))
    }

    #[allow(dead_code)] // Reflection/predicate fixtures use this before R2.4 dispatch wiring.
    pub(crate) fn as_oop(self) -> Oop {
        self.0.as_ptr()
    }

    /// Recover the callable Method value originally supplied to `from_method`.
    #[allow(dead_code)] // Becomes the sole invocation conversion in R2.4.
    pub(crate) fn to_method(self) -> Method {
        // SAFETY: MethodAddress can be constructed only from a Method, and the
        // frozen target represents both pointer forms in one 64-bit word.
        unsafe { core::mem::transmute::<*mut c_void, Method>(self.0.as_ptr()) }
    }
}

const _: () = {
    assert!(size_of::<MethodAddress>() == size_of::<Method>());
    assert!(align_of::<MethodAddress>() == align_of::<Method>());
    assert!(size_of::<MethodAddress>() == size_of::<Oop>());
    assert!(align_of::<MethodAddress>() == align_of::<Oop>());
};

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" fn answer(_: Oop, _: *const Oop, _: usize) -> Oop {
        42usize as Oop
    }

    #[test]
    fn method_address_round_trips_without_a_reference() {
        let address = MethodAddress::from_method(answer);
        assert!(!address.as_oop().is_null());
        let recovered = address.to_method();
        // SAFETY: the test supplies the zero-count/null-buffer contract.
        assert_eq!(
            unsafe { recovered(core::ptr::null_mut(), core::ptr::null(), 0) }.addr(),
            42
        );
    }
}
