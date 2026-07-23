//! Rust TCC dynamic-rung shell.
//!
//! N4.1 freezes the load boundary and exported object names before the unsafe
//! libtcc owner is ported. No compile or runtime-symbol authority moves here:
//! the Zig TCC dylib remains canonical until a later N4.2/N4.3 cutover.

use birnam_abi::{IdApi, Oop};
use core::ffi::{c_char, c_void};
use core::ptr;
use std::sync::OnceLock;

const RUST_TCC_IDENTITY: &[u8] = b"rust-tcc-n4.1-shell\0";
const TCC_NAME: &[u8] = b"Tcc\0";
const TCC_VT_NAME: &[u8] = b"TccVt\0";
const UNIT_VT_NAME: &[u8] = b"TccUnitVt\0";

#[derive(Clone, Copy)]
struct Runtime {
    api: usize,
    tcc_vt: usize,
    tcc: usize,
    unit_vt: usize,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn abort(message: &str) -> ! {
    eprintln!("birnam Rust TCC: {message}");
    std::process::abort();
}

unsafe fn api_ref(runtime: &Runtime) -> &'static IdApi {
    // SAFETY: module_init records the process-owned IdApi table and the host
    // retains its floor dylib for the process lifetime.
    unsafe { &*(runtime.api as *const IdApi) }
}

unsafe fn import(api: &IdApi, name: &[u8]) -> Oop {
    // SAFETY: all shell names are static NUL-terminated strings.
    unsafe { (api.import)(name.as_ptr().cast::<c_char>()) }
}

unsafe fn bring_up(api_pointer: *mut IdApi) {
    if api_pointer.is_null() {
        abort("module_init received a null IdApi");
    }
    let api = unsafe { &*api_pointer };
    let runtime = *RUNTIME.get_or_init(|| {
        let reloading = unsafe { (api.isExported)(TCC_NAME.as_ptr().cast()) != 0 };
        let (tcc_vt, unit_vt, tcc) = if reloading {
            (
                unsafe { import(api, TCC_VT_NAME) },
                unsafe { import(api, UNIT_VT_NAME) },
                unsafe { import(api, TCC_NAME) },
            )
        } else {
            let tcc_vt = unsafe { (api.proto)(api.object_vt) };
            let unit_vt = unsafe { (api.proto)(api.object_vt) };
            let tcc = unsafe { (api.alloc)(tcc_vt, 0) };
            (tcc_vt, unit_vt, tcc)
        };
        if [tcc_vt, unit_vt, tcc].iter().any(|value| value.is_null()) {
            abort("TCC shell exports were incomplete");
        }
        Runtime {
            api: api_pointer as usize,
            tcc_vt: tcc_vt as usize,
            unit_vt: unit_vt as usize,
            tcc: tcc as usize,
        }
    });

    let api = unsafe { api_ref(&runtime) };
    if unsafe { (api.isExported)(TCC_NAME.as_ptr().cast()) } == 0 {
        unsafe {
            let _ = (api.exportObj)(TCC_VT_NAME.as_ptr().cast(), runtime.tcc_vt as Oop);
            let _ = (api.exportObj)(UNIT_VT_NAME.as_ptr().cast(), runtime.unit_vt as Oop);
            let _ = (api.exportObj)(TCC_NAME.as_ptr().cast(), runtime.tcc as Oop);
        }
    }
}

/// Identifies the explicit N4.1 shell; it does not claim TCC behavior.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_tcc_rust_identity() -> *const c_char {
    RUST_TCC_IDENTITY.as_ptr().cast()
}

/// Initialize the TCC shell against the process-owned IdApi.
///
/// # Safety
///
/// api must be the live host IdApi table or null for the documented fatal
/// boundary.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _state: *mut c_void) -> *mut c_void {
    unsafe { bring_up(api) };
    ptr::null_mut()
}

#[cfg(test)]
mod tests {
    use super::{RUST_TCC_IDENTITY, birnam_tcc_rust_identity};
    use std::ffi::CStr;

    #[test]
    fn shell_identity_is_terminated() {
        // SAFETY: the export points to immutable crate storage for process life.
        let identity = unsafe { CStr::from_ptr(birnam_tcc_rust_identity()) };
        assert_eq!(identity.to_bytes_with_nul(), RUST_TCC_IDENTITY);
    }
}
