//! The native caller's registers and stack outlive traversal and collection.
use super::*;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[repr(align(16))]
struct RegisterSave([usize; 24]);
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const _: () = {
    assert!(size_of::<RegisterSave>() == 192);
    assert!(align_of::<RegisterSave>() == 16);
};
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[inline(never)]
pub(super) unsafe fn with_roots<T>(
    operation: impl FnOnce(&[usize]) -> Result<T, Outcome>,
) -> Result<T, Outcome> {
    unsafe extern "C" {
        fn _setjmp(buffer: *mut usize) -> i32;
        fn pthread_self() -> *mut c_void;
        fn pthread_get_stackaddr_np(thread: *mut c_void) -> *mut c_void;
    }
    let mut registers = RegisterSave([0; 24]);
    unsafe { _setjmp(registers.0.as_mut_ptr()) };
    let start = ptr::addr_of!(registers).addr();
    let bottom = unsafe { pthread_get_stackaddr_np(pthread_self()) }.addr();
    let distance = bottom.checked_sub(start).ok_or_else(invalid)?;
    if distance > 128 << 20 {
        return Err(invalid());
    }
    let mut roots = Vec::new();
    let mut address = start.next_multiple_of(size_of::<usize>());
    while address
        .checked_add(size_of::<usize>())
        .is_some_and(|end| end <= bottom)
    {
        roots.push(unsafe { (address as *const usize).read() });
        address += size_of::<usize>();
    }
    let result = operation(&roots);
    core::hint::black_box(&registers);
    result
}
#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
pub(super) unsafe fn with_roots<T>(
    _: impl FnOnce(&[usize]) -> Result<T, Outcome>,
) -> Result<T, Outcome> {
    Err(invalid())
}
