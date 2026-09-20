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

#[cfg(test)]
mod tests {
    use super::*;
    use birnam_abi::{Oop, namespace_import};

    struct Probe {
        events: Vec<&'static str>,
        name: Vec<u8>,
        value: Oop,
        replace: *mut ApiV1,
        nested: *const ApiV1,
        nested_value: Oop,
    }
    unsafe extern "C" fn access(context: *mut c_void) -> *mut c_void {
        unsafe { (*context.cast::<Probe>()).events.push("access") };
        context
    }
    unsafe extern "C" fn intern(context: *mut c_void, bytes: *const u8, length: usize) -> Oop {
        let (replace, nested, nested_value) = {
            let probe = unsafe { &mut *context.cast::<Probe>() };
            probe.events.push("intern");
            probe.name = unsafe { core::slice::from_raw_parts(bytes, length) }.to_vec();
            (probe.replace, probe.nested, probe.nested_value)
        };
        if !replace.is_null() {
            unsafe {
                (*replace).context = 7usize as Oop;
                (*replace).import = Some(replaced);
            }
        }
        if !nested.is_null() {
            assert_eq!(unsafe { namespace_import::import(nested, c"inner".as_ptr()) }, nested_value);
        }
        ptr::dangling_mut()
    }
    unsafe extern "C" fn find(context: *const c_void, symbol: Oop) -> Oop {
        assert_eq!(symbol, ptr::dangling_mut());
        let probe = unsafe { &mut *context.cast_mut().cast::<Probe>() };
        probe.events.push("find");
        probe.value
    }
    unsafe extern "C" fn replaced(context: *mut c_void, _: *const c_char) -> Oop {
        context
    }
    fn probe(value: Oop) -> Probe {
        Probe {
            events: Vec::new(),
            name: Vec::new(),
            value,
            replace: ptr::null_mut(),
            nested: ptr::null(),
            nested_value: ptr::null_mut(),
        }
    }
    fn floor(probe: &mut Probe) -> FloorV1 {
        FloorV1 {
            version: 1,
            byte_size: size_of::<FloorV1>(),
            context: ptr::from_mut(probe).cast(),
            access: Some(access),
            intern: Some(intern),
        }
    }
    unsafe fn service(probe: &mut Probe) -> *mut ApiV1 {
        let floor = floor(probe);
        unsafe { create(floor.context, Some(find), Some(crate::lookup::lookup), &floor) }
    }

    #[test]
    fn c_names_preserve_bytes_order_and_raw_binding_identity() {
        let mut first = probe(ptr::null_mut());
        let mut second = probe(9usize as Oop);
        let first_api = unsafe { service(&mut first) };
        let second_api = unsafe { service(&mut second) };
        // Empty/non-UTF8 names are legal, and the first NUL ends the name.
        for name in [c"".as_ptr(), b"raw\xff\0unscanned".as_ptr().cast()] {
            assert_eq!(unsafe { namespace_import::import(first_api, name) }, ptr::null_mut());
            assert_eq!(unsafe { namespace_import::import(second_api, name) }, second.value);
        }
        assert_eq!(first.events, ["access", "intern", "find", "access", "intern", "find"]);
        assert_eq!(second.events, first.events);
        assert_eq!(first.name, b"raw\xff");
        first.value = 3usize as Oop;
        assert_eq!(unsafe { namespace_import::import(first_api, c"changed".as_ptr()) }, first.value);
        assert_eq!(second.name, b"raw\xff");
        unsafe { destroy(first_api); destroy(second_api); }
    }

    #[test]
    fn reentry_and_live_replacement_preserve_the_active_call_selection() {
        let mut outer = probe(2usize as Oop);
        let mut inner = probe(5usize as Oop);
        let outer_api = unsafe { service(&mut outer) };
        let inner_api = unsafe { service(&mut inner) };
        outer.replace = outer_api;
        outer.nested = inner_api;
        outer.nested_value = inner.value;
        assert_eq!(unsafe { namespace_import::import(outer_api, c"outer".as_ptr()) }, outer.value);
        assert_eq!(outer.events, ["access", "intern", "find"]);
        assert_eq!(inner.events, ["access", "intern", "find"]);
        assert_eq!(outer.name, b"outer");
        assert_eq!(inner.name, b"inner");
        assert_eq!(unsafe { namespace_import::import(outer_api, c"next".as_ptr()) }, 7usize as Oop);
        // Replacement changes invocation, not ownership of the allocation.
        unsafe { destroy(outer_api); destroy(inner_api); }
    }

    #[test]
    fn copied_floor_and_caller_descriptors_have_independent_lifetimes() {
        let mut probe = probe(11usize as Oop);
        let mut floor = floor(&mut probe);
        let api = unsafe { create(floor.context, Some(find), Some(crate::lookup::lookup), &floor) };
        assert!(!api.is_null());
        floor.context = ptr::null_mut();
        floor.access = None;
        floor.intern = None;
        assert!(floor.context.is_null() && floor.access.is_none() && floor.intern.is_none());
        let canonical = unsafe { *api };
        let mut copied = canonical;
        copied.context = 13usize as Oop;
        copied.import = Some(replaced);
        assert_eq!(unsafe { namespace_import::import(&copied, c"copy".as_ptr()) }, 13usize as Oop);
        assert!(probe.events.is_empty());
        assert_eq!(unsafe { namespace_import::import(api, c"owned".as_ptr()) }, probe.value);
        unsafe { (*api).context = ptr::null_mut(); (*api).import = None; }
        assert_eq!(unsafe { namespace_import::import(&canonical, c"canonical".as_ptr()) }, probe.value);
        unsafe { destroy(api) };
    }

    #[test]
    fn invalid_creation_is_inert() {
        let mut probe = probe(ptr::null_mut());
        let mut floor = floor(&mut probe);
        let context = floor.context;
        let attempt = |storage, find, lookup, floor| unsafe { create(storage, find, lookup, floor) };
        assert!(attempt(ptr::null(), Some(find), Some(crate::lookup::lookup), &floor).is_null());
        assert!(attempt(context, None, Some(crate::lookup::lookup), &floor).is_null());
        assert!(attempt(context, Some(find), None, &floor).is_null());
        assert!(attempt(context, Some(find), Some(crate::lookup::lookup), ptr::null()).is_null());
        floor.version = 2;
        assert!(attempt(context, Some(find), Some(crate::lookup::lookup), &floor).is_null());
        floor.version = 1;
        floor.byte_size -= 1;
        assert!(attempt(context, Some(find), Some(crate::lookup::lookup), &floor).is_null());
        floor.byte_size += 1;
        floor.intern = None;
        assert!(attempt(context, Some(find), Some(crate::lookup::lookup), &floor).is_null());
        floor.intern = Some(intern);
        floor.access = None;
        assert!(attempt(context, Some(find), Some(crate::lookup::lookup), &floor).is_null());
        assert!(probe.events.is_empty());
    }
}
