//! The common native Id message and module-loading contract.
//!
//! Objects are opaque references. Modules communicate through Id messages;
//! this crate contains no module-specific APIs, storage layouts or registries.

use core::ffi::c_void;
use core::mem::align_of;

pub mod id_protocol;
pub mod native_module;

pub type Oop = *mut c_void;

/// Borrow a foreign read-only buffer while enforcing the ABI's null/count and
/// alignment rules.
///
/// A zero count always returns an empty slice without dereferencing `pointer`;
/// null with a nonzero count and a misaligned nonempty pointer return `None`.
///
/// # Safety
///
/// For a nonzero count, `pointer` must address `count` initialized `T` values
/// that remain readable for `'a`. This helper validates carrier shape, not
/// foreign allocation provenance or lifetime.
pub unsafe fn borrowed_slice<'a, T>(pointer: *const T, count: usize) -> Option<&'a [T]> {
    if count == 0 {
        return Some(&[]);
    }
    if pointer.is_null() || !(pointer as usize).is_multiple_of(align_of::<T>()) {
        return None;
    }
    // SAFETY: the caller supplies the readable extent/lifetime; the checks
    // above establish the non-null and alignment preconditions.
    Some(unsafe { core::slice::from_raw_parts(pointer, count) })
}

/// Borrow opaque object arguments; a zero count permits a null pointer.
///
/// # Safety
///
/// For a nonzero count, `pointer` must address `count` initialized `Oop`
/// carriers that remain readable for `'a`.
pub unsafe fn borrowed_oop_args<'a>(pointer: *const Oop, count: usize) -> Option<&'a [Oop]> {
    // SAFETY: forwarded caller contract.
    unsafe { borrowed_slice(pointer, count) }
}

/// Borrow bytes supplied through a length-delimited C ABI field.
///
/// # Safety
///
/// For a nonzero count, `pointer` must address `count` readable bytes for `'a`.
pub unsafe fn borrowed_bytes<'a>(pointer: *const u8, count: usize) -> Option<&'a [u8]> {
    // SAFETY: forwarded caller contract.
    unsafe { borrowed_slice(pointer, count) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borrowed_buffers_enforce_null_count_and_alignment_shape() {
        // SAFETY: zero-count buffers are never dereferenced.
        assert_eq!(
            unsafe { borrowed_oop_args(core::ptr::null(), 0) },
            Some(&[][..])
        );
        // SAFETY: this deliberately violates only the checked null/count shape.
        assert_eq!(unsafe { borrowed_oop_args(core::ptr::null(), 1) }, None);

        let values = [core::ptr::null_mut(), 42usize as Oop];
        // SAFETY: values supplies two initialized Oop carriers for this scope.
        assert_eq!(
            unsafe { borrowed_oop_args(values.as_ptr(), values.len()) },
            Some(&values[..])
        );

        let bytes = [1u8, 2, 3];
        // SAFETY: bytes supplies three initialized bytes for this scope.
        assert_eq!(
            unsafe { borrowed_bytes(bytes.as_ptr(), bytes.len()) },
            Some(&bytes[..])
        );
    }
}
