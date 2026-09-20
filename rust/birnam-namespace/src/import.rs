//! Canonical C-name import; Symbol interning remains an Id primitive.
use birnam_abi::namespace::{FindV1, LookupV1};
use birnam_abi::namespace_import::{ApiV1, FloorV1};
use core::ffi::{CStr, c_char, c_void};
use core::ptr;

#[repr(C)]
struct Import {
    api: ApiV1,
    namespace: *const c_void,
    find: FindV1,
    lookup: LookupV1,
    floor: FloorV1,
}
fn fatal(message: &str) -> ! {
    eprintln!("birnam-id invariant failed: {message}");
    std::process::abort()
}
unsafe extern "C" fn imported(context: *mut c_void, name: *const c_char) -> birnam_abi::Oop {
    if name.is_null() {
        fatal("IdApi.import name is null");
    }
    let bytes = unsafe { CStr::from_ptr(name) }.to_bytes();
    let (lookup, namespace, find, floor) = {
        let service = unsafe { &*context.cast::<Import>() };
        (
            service.lookup,
            service.namespace,
            service.find,
            service.floor,
        )
    };
    // Preserve floor owner/phase admission before the selected lookup. The
    // transient interner context is never retained by this service.
    let interner = unsafe { floor.access.unwrap()(floor.context) };
    let mut value = ptr::null_mut();
    let status = unsafe {
        (lookup)(
            namespace,
            find,
            interner,
            floor.intern.unwrap(),
            bytes.as_ptr(),
            bytes.len(),
            &mut value,
        )
    };
    if status == 0 {
        fatal("IdApi.import name contains embedded NUL");
    }
    if status != 1 {
        fatal("Namespace lookup returned invalid status");
    }
    value
}
pub(crate) unsafe extern "C" fn create(
    namespace: *const c_void,
    find: Option<FindV1>,
    lookup: Option<LookupV1>,
    floor: *const FloorV1,
) -> *mut ApiV1 {
    if namespace.is_null() || find.is_none() || lookup.is_none() || floor.is_null() {
        return ptr::null_mut();
    }
    let floor = unsafe { *floor };
    if floor.version != 1
        || floor.byte_size != size_of::<FloorV1>()
        || floor.access.is_none()
        || floor.intern.is_none()
    {
        return ptr::null_mut();
    }
    let mut service = Box::new(Import {
        api: ApiV1 {
            version: 1,
            byte_size: size_of::<ApiV1>(),
            context: ptr::null_mut(),
            import: Some(imported),
        },
        namespace,
        find: find.unwrap(),
        lookup: lookup.unwrap(),
        floor,
    });
    service.api.context = ptr::from_mut(service.as_mut()).cast();
    Box::into_raw(service).cast()
}
pub(crate) unsafe extern "C" fn destroy(api: *mut ApiV1) {
    // Live operation/context replacement does not transfer service ownership.
    unsafe { drop(Box::from_raw(api.cast::<Import>())) };
}
