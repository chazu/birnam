//! Stateless platform implementation of the common native module load request.
//! Both Siphon's ad-hoc message and the real Id loading method use this path.
use crate::id_protocol::{self as native, Module, Operand, Outcome, Protocol};
use core::ffi::{c_char, c_int, c_void};
use std::ffi::CString;

#[cfg_attr(target_os = "linux", link(name = "dl"))]
unsafe extern "C" {
    fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
    fn dlsym(image: *mut c_void, name: *const c_char) -> *mut c_void;
    fn dlclose(image: *mut c_void) -> c_int;
}

/// # Safety
/// The carrier, arguments and their owners remain live for initialization.
/// Initializers are trusted native code and run on the Id owner thread.
/// Once entered, an initializer's image stays mapped even when it reports
/// failure: arbitrary published pointers cannot be rolled back by the loader.
pub unsafe fn load(protocol: &Protocol, path: &[u8], arguments: &[Operand]) -> Outcome {
    let Ok(path) = CString::new(path) else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    if path.as_bytes().is_empty() {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    }
    #[cfg(target_os = "macos")]
    const FLAGS: c_int = 2 | 4;
    #[cfg(not(target_os = "macos"))]
    const FLAGS: c_int = 2;
    let image = unsafe { dlopen(path.as_ptr(), FLAGS) };
    if image.is_null() {
        return Outcome::failed(native::MAPPING_FAILED, 0);
    }
    let descriptor = unsafe { dlsym(image, c"birnam_native_module_v1".as_ptr()) }.cast::<Module>();
    if descriptor.is_null() {
        unsafe { dlclose(image) };
        return Outcome::failed(native::MISSING_ENTRY, 0);
    }
    // Read the common header before accessing fields of the admitted layout.
    let version = unsafe { (*descriptor).version };
    let size = unsafe { (*descriptor).byte_size };
    if version != native::VERSION || size as usize != size_of::<Module>() {
        unsafe { dlclose(image) };
        return Outcome::failed(native::INCOMPATIBLE_ABI, version as usize);
    }
    let Some(initialize) = (unsafe { (*descriptor).initialize }) else {
        unsafe { dlclose(image) };
        return Outcome::failed(native::INCOMPATIBLE_ABI, 0);
    };
    // Deliberately retain the loader reference from this point, including an
    // initializer failure after partial publication. No registry or GC exists
    // below this boundary, and dlclose would invalidate escaped native entries.
    unsafe { initialize(protocol, arguments.as_ptr(), arguments.len()) }
}
