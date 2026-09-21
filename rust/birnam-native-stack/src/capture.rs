//! Keep the register-save frame alive above all tracing and collection frames.
use birnam_abi::native_stack::CapturedV1;
use core::ffi::{c_int, c_void};

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[repr(align(16))]
struct RegisterSave([usize; 24]);

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const _: () = {
    // Darwin arm64 jmp_buf is 48 ints (192 bytes).
    assert!(size_of::<RegisterSave>() == 192);
    assert!(align_of::<RegisterSave>() == 16);
};

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
unsafe extern "C" {
    fn _setjmp(buffer: *mut usize) -> c_int;
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[inline(never)]
pub(super) unsafe extern "C" fn capture(
    bottom: usize,
    context: *mut c_void,
    operation: Option<CapturedV1>,
) -> c_int {
    let Some(operation) = operation else { return 0; };
    if bottom == 0 { return 0; }
    let mut registers = RegisterSave([0; 24]);
    unsafe { _setjmp(registers.0.as_mut_ptr()) };
    let start = core::ptr::addr_of!(registers).addr();
    let Some(distance) = bottom.checked_sub(start) else { return 0; };
    if distance > 128 << 20 { return 0; }
    unsafe { operation(context, start, bottom) };
    // Prevent a tail call or reuse of the save area during the callback.
    core::hint::black_box(&registers);
    1
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
pub(super) unsafe extern "C" fn capture(
    _: usize, _: *mut c_void, _: Option<CapturedV1>,
) -> c_int { 0 }
