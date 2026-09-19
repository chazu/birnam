//! Per-runtime enumeration over the selected opaque Namespace storage.
use birnam_abi::Oop;
use birnam_abi::namespace_enumeration::{ApiV1, CountV1, FloorV1, NameAtV1};
use core::ffi::c_void;

#[repr(C)]
struct Enumeration {
    api: ApiV1,
    storage: *const c_void,
    count: unsafe extern "C" fn(*const c_void) -> usize,
    name_at: unsafe extern "C" fn(*const c_void, usize) -> Oop,
    floor: FloorV1,
}
unsafe extern "C" fn count(context: *mut c_void) -> usize {
    let service = unsafe { &*context.cast::<Enumeration>() };
    unsafe { service.floor.preflight.unwrap()(service.floor.context) };
    unsafe { (service.count)(service.storage) }
}
unsafe extern "C" fn name_at(context: *mut c_void, index: usize) -> Oop {
    let service = unsafe { &*context.cast::<Enumeration>() };
    unsafe { service.floor.preflight.unwrap()(service.floor.context) };
    unsafe { (service.name_at)(service.storage, index) }
}
pub(crate) unsafe extern "C" fn create(
    storage: *const c_void,
    selected_count: Option<unsafe extern "C" fn(*const c_void) -> usize>,
    selected_name_at: Option<unsafe extern "C" fn(*const c_void, usize) -> Oop>,
    floor: *const FloorV1,
) -> *mut ApiV1 {
    if storage.is_null()
        || selected_count.is_none()
        || selected_name_at.is_none()
        || floor.is_null()
    {
        return core::ptr::null_mut();
    }
    let floor = unsafe { *floor };
    if floor.version != 1 || floor.byte_size != size_of::<FloorV1>() || floor.preflight.is_none() {
        return core::ptr::null_mut();
    }
    let mut service = Box::new(Enumeration {
        api: ApiV1 {
            count: CountV1 {
                version: 1,
                byte_size: size_of::<CountV1>(),
                context: core::ptr::null_mut(),
                count: Some(count),
            },
            name_at: NameAtV1 {
                version: 1,
                byte_size: size_of::<NameAtV1>(),
                context: core::ptr::null_mut(),
                name_at: Some(name_at),
            },
        },
        storage,
        count: selected_count.unwrap(),
        name_at: selected_name_at.unwrap(),
        floor,
    });
    let context = core::ptr::from_mut(service.as_mut()).cast();
    service.api.count.context = context;
    service.api.name_at.context = context;
    Box::into_raw(service).cast()
}
pub(crate) unsafe extern "C" fn destroy(api: *mut ApiV1) {
    unsafe { drop(Box::from_raw(api.cast::<Enumeration>())) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ptr;
    struct Probe {
        log: Vec<(&'static str, usize)>,
        names: Vec<Oop>,
    }
    unsafe extern "C" fn preflight(context: *mut c_void) {
        unsafe { &mut *context.cast::<Probe>() }
            .log
            .push(("preflight", 0));
    }
    unsafe extern "C" fn selected_count(context: *const c_void) -> usize {
        let probe = unsafe { &mut *context.cast_mut().cast::<Probe>() };
        probe.log.push(("count", 0));
        probe.names.len()
    }
    unsafe extern "C" fn selected_name(context: *const c_void, index: usize) -> Oop {
        let probe = unsafe { &mut *context.cast_mut().cast::<Probe>() };
        probe.log.push(("name", index));
        probe.names.get(index).copied().unwrap_or(ptr::null_mut())
    }
    #[test]
    fn incomplete_access_declines_without_callbacks() {
        let mut probe = Probe {
            log: Vec::new(),
            names: Vec::new(),
        };
        let pointer = ptr::from_mut(&mut probe).cast();
        let floor = FloorV1 {
            version: 1,
            byte_size: size_of::<FloorV1>(),
            context: pointer,
            preflight: Some(preflight),
        };
        unsafe {
            assert!(
                create(
                    ptr::null(),
                    Some(selected_count),
                    Some(selected_name),
                    &floor
                )
                .is_null()
            );
            assert!(create(pointer, None, Some(selected_name), &floor).is_null());
            assert!(create(pointer, Some(selected_count), None, &floor).is_null());
            assert!(
                create(
                    pointer,
                    Some(selected_count),
                    Some(selected_name),
                    ptr::null()
                )
                .is_null()
            );
            for invalid in [
                FloorV1 {
                    version: 2,
                    ..floor
                },
                FloorV1 {
                    byte_size: floor.byte_size - 1,
                    ..floor
                },
                FloorV1 {
                    preflight: None,
                    ..floor
                },
            ] {
                assert!(
                    create(pointer, Some(selected_count), Some(selected_name), &invalid).is_null()
                );
            }
        }
        assert!(probe.log.is_empty());
    }
    #[test]
    fn selected_storage_order_bounds_nulls_and_independent_contexts() {
        let mut first = Probe {
            log: Vec::new(),
            names: vec![ptr::dangling_mut::<c_void>(), ptr::null_mut()],
        };
        let mut second = Probe {
            log: Vec::new(),
            names: vec![],
        };
        let mut apis = Vec::new();
        for probe in [&mut first, &mut second] {
            let pointer = ptr::from_mut(&mut *probe).cast();
            let mut floor = FloorV1 {
                version: 1,
                byte_size: size_of::<FloorV1>(),
                context: pointer,
                preflight: Some(preflight),
            };
            let api = unsafe { create(pointer, Some(selected_count), Some(selected_name), &floor) };
            assert!(!api.is_null() && probe.log.is_empty());
            floor.preflight = None;
            assert!(floor.preflight.is_none());
            apis.push(api);
        }
        unsafe {
            let left = &*apis[0];
            let right = &*apis[1];
            assert_ne!(left.count.context, right.count.context);
            assert_eq!(left.count.count.unwrap()(left.count.context), 2);
            for index in [0, 1, 2, usize::MAX] {
                assert_eq!(
                    left.name_at.name_at.unwrap()(left.name_at.context, index),
                    first.names.get(index).copied().unwrap_or(ptr::null_mut())
                );
            }
            assert_eq!(
                first.log,
                [
                    ("preflight", 0),
                    ("count", 0),
                    ("preflight", 0),
                    ("name", 0),
                    ("preflight", 0),
                    ("name", 1),
                    ("preflight", 0),
                    ("name", 2),
                    ("preflight", 0),
                    ("name", usize::MAX)
                ]
            );
            assert!(second.log.is_empty());
            destroy(apis[0]);
            assert_eq!(right.count.count.unwrap()(right.count.context), 0);
            destroy(apis[1]);
            assert_eq!(second.log, [("preflight", 0), ("count", 0)]);
        }
    }
}
