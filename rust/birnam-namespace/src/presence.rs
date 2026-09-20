//! Canonical C-name presence; Symbol interning remains an Id primitive.
use birnam_abi::namespace::{FindV1, LookupV1};
use birnam_abi::namespace_presence::{ApiV1, FloorV1};
use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;

#[repr(C)]
struct Presence {
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
unsafe extern "C" fn contains(context: *mut c_void, name: *const c_char) -> c_int {
    if name.is_null() {
        fatal("IdApi.import name is null");
    }
    let bytes = unsafe { CStr::from_ptr(name) }.to_bytes();
    let (lookup, namespace, find, floor) = {
        let service = unsafe { &*context.cast::<Presence>() };
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
    c_int::from(!value.is_null())
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
    let mut service = Box::new(Presence {
        api: ApiV1 {
            version: 1,
            byte_size: size_of::<ApiV1>(),
            context: ptr::null_mut(),
            contains: Some(contains),
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
    unsafe { drop(Box::from_raw(api.cast::<Presence>())) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use birnam_abi::{Oop, namespace_presence};

    struct Probe {
        events: Vec<&'static str>,
        name: Vec<u8>,
        value: Oop,
        replace: *mut ApiV1,
    }
    unsafe extern "C" fn access(context: *mut c_void) -> *mut c_void {
        unsafe { (*context.cast::<Probe>()).events.push("access") };
        context
    }
    unsafe extern "C" fn intern(context: *mut c_void, bytes: *const u8, length: usize) -> Oop {
        let probe = unsafe { &mut *context.cast::<Probe>() };
        probe.events.push("intern");
        probe.name = unsafe { core::slice::from_raw_parts(bytes, length) }.to_vec();
        if !probe.replace.is_null() {
            unsafe {
                (*probe.replace).context = ptr::null_mut();
                (*probe.replace).contains = Some(replaced);
            }
        }
        ptr::dangling_mut()
    }
    unsafe extern "C" fn find(context: *const c_void, symbol: Oop) -> Oop {
        assert_eq!(symbol, ptr::dangling_mut());
        let probe = unsafe { &mut *context.cast_mut().cast::<Probe>() };
        probe.events.push("find");
        probe.value
    }
    unsafe extern "C" fn replaced(context: *mut c_void, _: *const c_char) -> c_int {
        assert!(context.is_null());
        -7
    }
    fn probe(value: Oop) -> Probe {
        Probe {
            events: Vec::new(),
            name: Vec::new(),
            value,
            replace: ptr::null_mut(),
        }
    }
    unsafe fn service(probe: &mut Probe) -> *mut ApiV1 {
        let context = ptr::from_mut(probe).cast();
        let floor = FloorV1 {
            version: 1,
            byte_size: size_of::<FloorV1>(),
            context,
            access: Some(access),
            intern: Some(intern),
        };
        unsafe { create(context, Some(find), Some(crate::lookup::lookup), &floor) }
    }
    #[test]
    fn c_names_intern_before_find_and_only_raw_null_is_absent() {
        let mut first = probe(ptr::null_mut());
        let mut second = probe(9usize as Oop);
        let first_api = unsafe { service(&mut first) };
        let second_api = unsafe { service(&mut second) };
        // Empty and non-UTF8 names are legal; a C name stops at its first NUL.
        for name in [c"".as_ptr(), b"raw\xff\0unscanned".as_ptr().cast()] {
            assert_eq!(unsafe { namespace_presence::contains(first_api, name) }, 0);
            assert_eq!(unsafe { namespace_presence::contains(second_api, name) }, 1);
        }
        assert_eq!(
            first.events,
            ["access", "intern", "find", "access", "intern", "find"]
        );
        assert_eq!(second.events, first.events);
        assert_eq!(first.name, b"raw\xff");
        // State remains independent when one binding changes.
        first.value = 3usize as Oop;
        assert_eq!(
            unsafe { namespace_presence::contains(first_api, c"new".as_ptr()) },
            1
        );
        assert_eq!(second.name, b"raw\xff");
        unsafe {
            destroy(first_api);
            destroy(second_api);
        }
    }
    #[test]
    fn active_call_keeps_selection_and_destroy_ignores_replaced_context() {
        let mut probe = probe(2usize as Oop);
        let api = unsafe { service(&mut probe) };
        probe.replace = api;
        assert_eq!(
            unsafe { namespace_presence::contains(api, c"outer".as_ptr()) },
            1
        );
        assert_eq!(probe.events, ["access", "intern", "find"]);
        assert_eq!(
            unsafe { namespace_presence::contains(api, c"next".as_ptr()) },
            -7
        );
        // The public context no longer points at the owned service.
        unsafe { destroy(api) };
    }
    #[test]
    fn invalid_creation_is_inert() {
        let mut probe = probe(ptr::null_mut());
        let context = ptr::from_mut(&mut probe).cast();
        let mut floor = FloorV1 {
            version: 1,
            byte_size: size_of::<FloorV1>(),
            context,
            access: Some(access),
            intern: Some(intern),
        };
        let attempt =
            |storage, find, lookup, floor| unsafe { create(storage, find, lookup, floor) };
        assert!(attempt(ptr::null(), Some(find), Some(crate::lookup::lookup), &floor).is_null());
        assert!(attempt(context, None, Some(crate::lookup::lookup), &floor).is_null());
        assert!(attempt(context, Some(find), None, &floor).is_null());
        assert!(
            attempt(
                context,
                Some(find),
                Some(crate::lookup::lookup),
                ptr::null()
            )
            .is_null()
        );
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
