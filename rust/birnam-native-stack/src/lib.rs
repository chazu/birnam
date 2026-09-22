//! Native root-anchor state, register capture and word scanning.
mod capture;
use core::ffi::{c_int, c_void};
use core::ptr;

#[repr(C)]
struct Anchor {
    address: usize,
}
unsafe extern "C" fn set(api: *mut Anchor, address: usize) {
    unsafe { (*api.cast::<Anchor>()).address = address };
}
unsafe extern "C" fn get(api: *const Anchor) -> usize {
    unsafe { (*api.cast::<Anchor>()).address }
}

// The scanner runs below the live register-capture frame.
unsafe extern "C" fn scan(pointer: *const birnam_abi::trace_live::SinkV1, start: usize, bottom: usize) {
    let sink = unsafe { &*pointer };
    let mut address = start.next_multiple_of(size_of::<usize>());
    while address.checked_add(size_of::<usize>()).is_some_and(|end| end <= bottom) {
        // Read and emit one complete word at a time, preserving callback effects.
        let candidate = unsafe { (address as *const usize).read() };
        let status = unsafe { (sink.emit)(sink.context, candidate) };
        if status != birnam_abi::storage::OK {
            eprintln!("birnam-id invariant failed: GC tracer rejected a candidate");
            std::process::abort();
        }
        address += size_of::<usize>();
    }
}
// Birnam currently targets arm64 macOS. This boundary outlives startup frames.
unsafe extern "C" fn thread_bottom() -> usize {
    unsafe extern "C" {
        fn pthread_self() -> *mut c_void;
        fn pthread_get_stackaddr_np(thread: *mut c_void) -> *mut c_void;
    }
    unsafe { pthread_get_stackaddr_np(pthread_self()).addr() }
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> c_int { 1 }

#[cfg(test)]
mod tests;
