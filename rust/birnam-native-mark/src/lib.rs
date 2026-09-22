//! Owned native mark scopes; traversal and heap ownership stay outside.
use birnam_abi::Oop;
use birnam_abi::trace_live::SinkV1;
use birnam_abi::storage::OK;
use core::ffi::{c_int, c_void};

struct Scope { sink: Option<SinkV1> }
#[cold]
fn fatal(message: &str) -> ! {
    // Preserve the historical external diagnostic, including its prefix.
    eprintln!("birnam-id invariant failed: {message}");
    std::process::abort()
}
unsafe extern "C" fn create() -> *mut c_void {
    Box::into_raw(Box::new(Scope { sink: None })).cast()
}
unsafe extern "C" fn destroy(context: *mut c_void) {
    unsafe { drop(Box::from_raw(context.cast::<Scope>())) };
}
unsafe extern "C" fn mark(context: *mut c_void, object: Oop) {
    let sink = unsafe { (*context.cast::<Scope>()).sink }
        .unwrap_or_else(|| fatal("GC mark callback escaped its invocation"));
    if !object.is_null() {
        // Copy selection before emission; retain no scope borrow across reentry.
        let status = unsafe { (sink.emit)(sink.context, object.addr()) };
        if status != OK { fatal("GC tracer rejected a candidate"); }
    }
}
unsafe extern "C" fn begin(context: *mut c_void, sink: *const SinkV1) {
    let scope = context.cast::<Scope>();
    if unsafe { (*scope).sink.is_some() } { fatal("GC mark callback reentered"); }
    unsafe { (*scope).sink = Some(*sink) };
}
unsafe extern "C" fn end(context: *mut c_void) { unsafe { (*context.cast::<Scope>()).sink = None }; }

#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> c_int { 1 }

#[cfg(test)]
mod tests;
