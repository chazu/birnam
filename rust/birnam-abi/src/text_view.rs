//! Shared exact/printable borrowed byte dependency (81AK).
use crate::{IdApi, Oop};
use core::ffi::c_int;
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
/// A borrowed extent. Null data denotes absence; a nonnull zero-length extent
/// denotes a present empty view. No ownership or lifetime is transferred.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BytesV1 {
    pub data: *const u8,
    pub length: usize,
}
impl BytesV1 {
    pub fn from_slice(value: Option<&[u8]>) -> Self {
        match value {
            Some(bytes) => Self {
                data: bytes.as_ptr(),
                length: bytes.len(),
            },
            None => Self {
                data: ptr::null(),
                length: 0,
            },
        }
    }
}
pub type ViewV1 = unsafe extern "C" fn(*const IdApi, Oop) -> BytesV1;
/// Immutable and process-pinned. Operations use the supplied table on every call
/// and retain no objects, API pointer, selectors or byte buffers.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV1 {
    pub version: u32,
    pub byte_size: usize,
    pub exact: Option<ViewV1>,
    pub printable: Option<ViewV1>,
}
const _: () = {
    assert!(size_of::<BytesV1>() == 16);
    assert!(core::mem::offset_of!(BytesV1, length) == 8);
    assert!(size_of::<ApiV1>() == 32);
    assert!(core::mem::offset_of!(ApiV1, printable) == 24);
};
pub type ProviderV1 = unsafe extern "C" fn() -> *const ApiV1;
pub type DependencyV1 = unsafe extern "C" fn(*const ApiV1) -> c_int;
static DEPENDENCY: AtomicPtr<ApiV1> = AtomicPtr::new(ptr::null_mut());
/// # Safety
/// Mount on the owner thread; immutable descriptor/code outlive the consumer.
pub unsafe fn attach(api: *const ApiV1) -> c_int {
    if api.is_null() {
        return 0;
    }
    let value = unsafe { *api };
    if value.version != 1
        || value.byte_size != size_of::<ApiV1>()
        || value.exact.is_none()
        || value.printable.is_none()
    {
        return 0;
    }
    match DEPENDENCY.compare_exchange(
        ptr::null_mut(),
        api.cast_mut(),
        Ordering::AcqRel,
        Ordering::Acquire,
    ) {
        Ok(_) => 1,
        Err(existing) => c_int::from(ptr::eq(existing, api)),
    }
}
fn operations() -> ApiV1 {
    let api = DEPENDENCY.load(Ordering::Acquire);
    assert!(
        !api.is_null(),
        "native text view dependency was not mounted"
    );
    unsafe { *api }
}
unsafe fn decode<'a>(view: BytesV1) -> Option<&'a [u8]> {
    if view.data.is_null() {
        None
    } else if view.length == 0 {
        Some(&[])
    } else {
        Some(unsafe { core::slice::from_raw_parts(view.data, view.length) })
    }
}
/// # Safety
/// Owner-thread call using the live selected table. Preserve the underlying
/// String callback's borrow lifetime and keep its storage alive throughout use.
/// No byte copy or extra root is created, including across existing callbacks.
pub unsafe fn string_bytes(api: &IdApi, value: Oop) -> Option<&[u8]> {
    unsafe { decode(operations().exact.unwrap()(api, value)) }
}
/// # Safety
/// The exact-view borrow obligations apply. Fallback may intern, send, collect
/// and reenter; it retains neither the source nor the rendered String afterward.
pub unsafe fn printable_string(api: &IdApi, value: Oop) -> Option<&[u8]> {
    unsafe { decode(operations().printable.unwrap()(api, value)) }
}
