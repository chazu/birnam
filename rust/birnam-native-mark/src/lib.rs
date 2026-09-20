//! Owned native mark scopes; traversal and heap ownership stay outside.
use birnam_abi::Oop;
use birnam_abi::native_mark::ApiV1;
use birnam_abi::trace_live::SinkV1;
use birnam_abi::storage::OK;
use core::ffi::{c_int, c_void};
use core::ptr;

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
static API: ApiV1 = ApiV1 {
    version: 1, byte_size: size_of::<ApiV1>(), create: Some(create), destroy: Some(destroy),
    begin: Some(begin), end: Some(end), mark: Some(mark),
};
/// # Safety
/// The host pins this descriptor and its code for every retaining floor/thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_native_mark_provider_v1() -> *const ApiV1 { &API }

/// # Safety
/// All descriptors and code have the native initialization lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(
    api: *mut birnam_abi::IdApi, _: *mut c_void,
    _: *const birnam_abi::namespace_presence::ApiV1,
    _: *const birnam_abi::namespace_import::ApiV1,
    _: *const birnam_abi::namespace_export::ApiV1,
    _: *const birnam_abi::vtable_enumeration::ApiV2,
    _: *const birnam_abi::method_observation::ApiV1,
    _: *const birnam_abi::root_registry::ApiV1,
    _: *const birnam_abi::gc_settings::ApiV1,
    _: *const birnam_abi::payload_layout::ApiV1,
    _: *const birnam_abi::trace_hooks::ApiV1,
    _: *const birnam_abi::finalizers::ApiV1,
) -> *mut c_void {
    assert!(!api.is_null(), "Native Mark received a null IdApi");
    ptr::null_mut()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> c_int { 1 }


#[cfg(test)]
mod tests;
