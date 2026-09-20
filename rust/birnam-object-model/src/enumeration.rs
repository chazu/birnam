//! Count and checked selection over a transient Id-owned registry view.
use birnam_abi::vtable_enumeration::{ApiV1, FactoryV1, FloorV1};
use birnam_abi::Oop;
use core::ffi::c_void;
use core::ptr;

#[repr(C)]
struct Enumeration {
    api: ApiV1,
    floor: FloorV1,
}
unsafe extern "C" fn count(context: *mut c_void) -> usize {
    let floor = unsafe { (*context.cast::<Enumeration>()).floor };
    unsafe { floor.view.unwrap()(floor.context) }.count
}
unsafe extern "C" fn at(context: *mut c_void, index: usize) -> Oop {
    let floor = unsafe { (*context.cast::<Enumeration>()).floor };
    let view = unsafe { floor.view.unwrap()(floor.context) };
    if index >= view.count { return ptr::null_mut(); }
    // No callback or mutation occurs between the borrow and this identity copy.
    unsafe { *view.entries.add(index) }
}
unsafe extern "C" fn create(floor: *const FloorV1) -> *mut ApiV1 {
    if floor.is_null() { return ptr::null_mut(); }
    let floor = unsafe { *floor };
    if floor.version != 1 || floor.byte_size != size_of::<FloorV1>() || floor.view.is_none() {
        return ptr::null_mut();
    }
    let mut service = Box::new(Enumeration {
        api: ApiV1 { version: 1, byte_size: size_of::<ApiV1>(), context: ptr::null_mut(),
                     count: Some(count), at: Some(at) },
        floor,
    });
    service.api.context = ptr::from_mut(service.as_mut()).cast();
    Box::into_raw(service).cast()
}
unsafe extern "C" fn destroy(api: *mut ApiV1) {
    unsafe { drop(Box::from_raw(api.cast::<Enumeration>())) };
}
static FACTORY: FactoryV1 = FactoryV1 {
    version: 1, byte_size: size_of::<FactoryV1>(), create: Some(create), destroy: Some(destroy),
};
/// # Safety
/// Factory operations require their declared owner-thread and borrow contracts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_vtable_enumeration_factory_v1() -> *const FactoryV1 {
    &FACTORY
}

#[cfg(test)]
mod tests {
    use super::*;
    use birnam_abi::vtable_enumeration::{self as api, ViewV1};

    struct Probe {
        entries: Vec<Oop>,
        reads: usize,
        replace: *mut ApiV1,
    }
    unsafe extern "C" fn view(context: *mut c_void) -> ViewV1 {
        let probe = unsafe { &mut *context.cast::<Probe>() };
        probe.reads += 1;
        if !probe.replace.is_null() {
            unsafe { (*probe.replace).context = 17usize as Oop; (*probe.replace).count = Some(replaced); }
        }
        ViewV1 { entries: probe.entries.as_ptr(), count: probe.entries.len() }
    }
    unsafe extern "C" fn replaced(context: *mut c_void) -> usize { context as usize }
    fn floor(probe: &mut Probe) -> FloorV1 {
        FloorV1 { version: 1, byte_size: size_of::<FloorV1>(), context: ptr::from_mut(probe).cast(), view: Some(view) }
    }
    #[test]
    fn views_remain_live_unrooted_ordered_and_independent() {
        let mut left = Probe { entries: vec![1usize as Oop, 2usize as Oop, 3usize as Oop], reads: 0, replace: ptr::null_mut() };
        let mut right = Probe { entries: Vec::new(), reads: 0, replace: ptr::null_mut() };
        let a = unsafe { create(&floor(&mut left)) };
        let b = unsafe { create(&floor(&mut right)) };
        assert_eq!(left.reads + right.reads, 0);
        assert_eq!(unsafe { api::count(a) }, 3);
        left.entries.remove(1);
        left.entries.push(ptr::null_mut());
        assert_eq!(unsafe { api::at(a, 1) }, 3usize as Oop);
        assert!(unsafe { api::at(a, 2) }.is_null());
        assert!(unsafe { api::at(a, 3) }.is_null());
        assert!(unsafe { api::at(a, usize::MAX) }.is_null());
        assert_eq!(unsafe { api::count(b) }, 0);
        assert!(unsafe { api::at(b, 0) }.is_null());
        assert_eq!(left.reads, 5);
        assert_eq!(right.reads, 2);
        unsafe { destroy(a) };
        right.entries.push(7usize as Oop);
        assert_eq!(unsafe { api::at(b, 0) }, 7usize as Oop);
        unsafe { destroy(b) };
    }
    #[test]
    fn floor_and_active_selection_are_copied_but_public_selection_is_live() {
        let mut probe = Probe { entries: vec![9usize as Oop], reads: 0, replace: ptr::null_mut() };
        let mut access = floor(&mut probe);
        let owned = unsafe { create(&access) };
        access.view = None;
        access.context = ptr::null_mut();
        assert!(access.view.is_none() && access.context.is_null());
        let saved = unsafe { *owned };
        probe.replace = owned;
        assert_eq!(unsafe { api::count(owned) }, 1);
        assert_eq!(unsafe { api::count(owned) }, 17);
        assert_eq!(unsafe { api::count(&saved) }, 1);
        assert_eq!(probe.reads, 2);
        unsafe { (*owned).context = ptr::null_mut(); destroy(owned); }
    }
    #[test]
    fn incomplete_floor_is_rejected_without_observation() {
        let mut probe = Probe { entries: Vec::new(), reads: 0, replace: ptr::null_mut() };
        let good = floor(&mut probe);
        assert!(unsafe { create(ptr::null()) }.is_null());
        for bad in [FloorV1 { version: 2, ..good }, FloorV1 { byte_size: good.byte_size - 1, ..good }, FloorV1 { view: None, ..good }] {
            assert!(unsafe { create(&bad) }.is_null());
        }
        assert_eq!(probe.reads, 0);
    }
}
