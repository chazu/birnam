//! Canonical C-name export; Id retains Symbol and weak-mirror representation.
use birnam_abi::namespace::{BindV1, PublishV1};
use birnam_abi::namespace_export::{ApiV1, FloorV1};
use birnam_abi::Oop;
use core::ffi::{CStr, c_char, c_void};
use core::ptr;

#[repr(C)]
struct Export {
    api: ApiV1,
    namespace: *mut c_void,
    bind: BindV1,
    publish: PublishV1,
    floor: FloorV1,
}
fn fatal(message: &str) -> ! {
    eprintln!("birnam-id invariant failed: {message}");
    std::process::abort()
}
unsafe extern "C" fn exported(context: *mut c_void, name: *const c_char, value: Oop) -> Oop {
    if name.is_null() {
        fatal("IdApi.exportObj name is null");
    }
    let bytes = unsafe { CStr::from_ptr(name) }.to_bytes();
    let (publish, namespace, bind, floor) = {
        let service = unsafe { &*context.cast::<Export>() };
        (service.publish, service.namespace, service.bind, service.floor)
    };
    // Preserve floor owner/phase admission before the selected publication.
    // The transient primitive context is never retained by this service.
    let primitive = unsafe { floor.access.unwrap()(floor.context) };
    let mut result = ptr::null_mut();
    let status = unsafe {
        publish(namespace, bind, primitive, floor.intern.unwrap(), floor.mirror.unwrap(),
                bytes.as_ptr(), bytes.len(), value, &mut result)
    };
    match status {
        0 => fatal("IdApi.exportObj name contains embedded NUL"),
        1 => result,
        _ => fatal("Namespace publication returned invalid status"),
    }
}
pub(crate) unsafe extern "C" fn create(
    namespace: *mut c_void,
    bind: Option<BindV1>,
    publish: Option<PublishV1>,
    floor: *const FloorV1,
) -> *mut ApiV1 {
    if namespace.is_null() || bind.is_none() || publish.is_none() || floor.is_null() {
        return ptr::null_mut();
    }
    let floor = unsafe { *floor };
    if floor.version != 1
        || floor.byte_size != size_of::<FloorV1>()
        || floor.access.is_none()
        || floor.intern.is_none()
        || floor.mirror.is_none()
    {
        return ptr::null_mut();
    }
    let mut service = Box::new(Export {
        api: ApiV1 {
            version: 1,
            byte_size: size_of::<ApiV1>(),
            context: ptr::null_mut(),
            export: Some(exported),
        },
        namespace,
        bind: bind.unwrap(),
        publish: publish.unwrap(),
        floor,
    });
    service.api.context = ptr::from_mut(service.as_mut()).cast();
    Box::into_raw(service).cast()
}
pub(crate) unsafe extern "C" fn destroy(api: *mut ApiV1) {
    // Live operation/context replacement does not transfer service ownership.
    unsafe { drop(Box::from_raw(api.cast::<Export>())) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use birnam_abi::namespace_export;

    struct Probe {
        events: Vec<&'static str>,
        name: Vec<u8>,
        bound: Option<(Oop, Oop)>,
        mirrored: Option<(Oop, Oop)>,
        result: Oop,
        replace: *mut ApiV1,
        nested: *const ApiV1,
        nested_result: Oop,
    }
    unsafe extern "C" fn access(context: *mut c_void) -> *mut c_void {
        unsafe { (*context.cast::<Probe>()).events.push("access") };
        context
    }
    unsafe extern "C" fn intern(context: *mut c_void, bytes: *const u8, length: usize) -> Oop {
        let probe = unsafe { &mut *context.cast::<Probe>() };
        probe.events.push("intern");
        probe.name = unsafe { core::slice::from_raw_parts(bytes, length) }.to_vec();
        3usize as Oop
    }
    unsafe extern "C" fn bind(context: *mut c_void, symbol: Oop, value: Oop) -> Oop {
        let (replace, nested, nested_result, result) = {
            let probe = unsafe { &mut *context.cast::<Probe>() };
            probe.events.push("bind");
            probe.bound = Some((symbol, value));
            (probe.replace, probe.nested, probe.nested_result, probe.result)
        };
        if !replace.is_null() {
            unsafe { (*replace).context = 7usize as Oop; (*replace).export = Some(replaced); }
        }
        if !nested.is_null() {
            assert_eq!(unsafe { namespace_export::export(nested, c"inner".as_ptr(), 5usize as Oop) }, nested_result);
        }
        result
    }
    unsafe extern "C" fn mirror(context: *mut c_void, symbol: Oop, value: Oop) {
        let probe = unsafe { &mut *context.cast::<Probe>() };
        probe.events.push("mirror");
        probe.mirrored = Some((symbol, value));
    }
    unsafe extern "C" fn replaced(context: *mut c_void, _: *const c_char, _: Oop) -> Oop { context }
    fn probe(result: Oop) -> Probe {
        Probe { events: Vec::new(), name: Vec::new(), bound: None, mirrored: None,
                result, replace: ptr::null_mut(), nested: ptr::null(), nested_result: ptr::null_mut() }
    }
    fn floor(probe: &mut Probe) -> FloorV1 {
        FloorV1 { version: 1, byte_size: size_of::<FloorV1>(),
                  context: ptr::from_mut(probe).cast(), access: Some(access),
                  intern: Some(intern), mirror: Some(mirror) }
    }
    unsafe fn service(probe: &mut Probe) -> *mut ApiV1 {
        let floor = floor(probe);
        unsafe { create(floor.context, Some(bind), Some(crate::publication::publish), &floor) }
    }
    #[test]
    fn c_names_publication_order_input_identity_and_independent_result() {
        let mut first = probe(ptr::null_mut());
        let mut second = probe(9usize as Oop);
        let first_api = unsafe { service(&mut first) };
        let second_api = unsafe { service(&mut second) };
        for name in [c"".as_ptr(), b"raw\xff\0unscanned".as_ptr().cast()] {
            assert_eq!(unsafe { namespace_export::export(first_api, name, 11usize as Oop) }, ptr::null_mut());
            assert_eq!(unsafe { namespace_export::export(second_api, name, ptr::null_mut()) }, second.result);
        }
        assert_eq!(first.events, ["access", "intern", "bind", "mirror", "access", "intern", "bind", "mirror"]);
        assert_eq!(second.events, first.events);
        assert_eq!(first.name, b"raw\xff");
        assert_eq!(second.name, first.name);
        assert_eq!(first.bound, Some((3usize as Oop, 11usize as Oop)));
        assert_eq!(first.mirrored, first.bound);
        assert_eq!(second.bound, Some((3usize as Oop, ptr::null_mut())));
        assert_eq!(second.mirrored, second.bound);
        unsafe { destroy(first_api); destroy(second_api); }
    }
    #[test]
    fn reentry_and_replacement_keep_active_publication_and_mirror() {
        let mut outer = probe(2usize as Oop);
        let mut inner = probe(13usize as Oop);
        let outer_api = unsafe { service(&mut outer) };
        let inner_api = unsafe { service(&mut inner) };
        outer.replace = outer_api;
        outer.nested = inner_api;
        outer.nested_result = inner.result;
        assert_eq!(unsafe { namespace_export::export(outer_api, c"outer".as_ptr(), 17usize as Oop) }, outer.result);
        assert_eq!(outer.events, ["access", "intern", "bind", "mirror"]);
        assert_eq!(inner.events, outer.events);
        assert_eq!(outer.name, b"outer");
        assert_eq!(inner.name, b"inner");
        assert_eq!(outer.mirrored, Some((3usize as Oop, 17usize as Oop)));
        assert_eq!(inner.mirrored, Some((3usize as Oop, 5usize as Oop)));
        assert_eq!(unsafe { namespace_export::export(outer_api, c"next".as_ptr(), ptr::null_mut()) }, 7usize as Oop);
        unsafe { destroy(outer_api); destroy(inner_api); }
    }
    #[test]
    fn floor_copy_descriptor_copy_and_owned_destruction_are_independent() {
        let mut probe = probe(11usize as Oop);
        let mut floor = floor(&mut probe);
        let api = unsafe { create(floor.context, Some(bind), Some(crate::publication::publish), &floor) };
        assert!(!api.is_null());
        floor.context = ptr::null_mut(); floor.access = None; floor.intern = None; floor.mirror = None;
        assert!(floor.context.is_null() && floor.access.is_none() && floor.intern.is_none() && floor.mirror.is_none());
        let canonical = unsafe { *api };
        let mut copied = canonical;
        copied.context = 13usize as Oop; copied.export = Some(replaced);
        assert_eq!(unsafe { namespace_export::export(&copied, c"copy".as_ptr(), ptr::null_mut()) }, 13usize as Oop);
        assert!(probe.events.is_empty());
        assert_eq!(unsafe { namespace_export::export(api, c"owned".as_ptr(), ptr::null_mut()) }, probe.result);
        unsafe { (*api).context = ptr::null_mut(); (*api).export = None; }
        assert_eq!(unsafe { namespace_export::export(&canonical, c"canonical".as_ptr(), ptr::null_mut()) }, probe.result);
        unsafe { destroy(api) };
    }
    #[test]
    fn invalid_creation_is_inert() {
        let mut probe = probe(ptr::null_mut());
        let mut floor = floor(&mut probe);
        let context = floor.context;
        let attempt = |storage, bind, publish, floor| unsafe { create(storage, bind, publish, floor) };
        assert!(attempt(ptr::null_mut(), Some(bind), Some(crate::publication::publish), &floor).is_null());
        assert!(attempt(context, None, Some(crate::publication::publish), &floor).is_null());
        assert!(attempt(context, Some(bind), None, &floor).is_null());
        assert!(attempt(context, Some(bind), Some(crate::publication::publish), ptr::null()).is_null());
        floor.version = 2;
        assert!(attempt(context, Some(bind), Some(crate::publication::publish), &floor).is_null());
        floor.version = 1; floor.byte_size -= 1;
        assert!(attempt(context, Some(bind), Some(crate::publication::publish), &floor).is_null());
        floor.byte_size += 1; floor.access = None;
        assert!(attempt(context, Some(bind), Some(crate::publication::publish), &floor).is_null());
        floor.access = Some(access); floor.intern = None;
        assert!(attempt(context, Some(bind), Some(crate::publication::publish), &floor).is_null());
        floor.intern = Some(intern); floor.mirror = None;
        assert!(attempt(context, Some(bind), Some(crate::publication::publish), &floor).is_null());
        assert!(probe.events.is_empty());
    }
}
