//! The existing single native pre-overwrite observer, independently owned.
use birnam_abi::method_observation::{ApiV1, FactoryV1, FloorV1, HookV1};
use core::ffi::{c_int, c_void};
use core::ptr;

#[repr(C)]
struct Observation {
    api: ApiV1,
    floor: FloorV1,
    hook: Option<HookV1>,
}
unsafe extern "C" fn replace(context: *mut c_void, hook: Option<HookV1>) {
    let owned = context.cast::<Observation>();
    let floor = unsafe { (*owned).floor };
    // End the borrow before admission; replacement still happens after it.
    unsafe { floor.admit.unwrap()(floor.context) };
    unsafe { (*owned).hook = hook };
}
unsafe extern "C" fn notify(api: *mut ApiV1, direct: c_int, vtable: birnam_abi::Oop, selector: birnam_abi::Oop) {
    let hook = unsafe { (*api.cast::<Observation>()).hook };
    if direct != 0 {
        if let Some(hook) = hook {
            // No borrow of the owned service survives this callback.
            unsafe { hook(vtable, selector) };
        }
    }
}
unsafe extern "C" fn create(floor: *const FloorV1) -> *mut ApiV1 {
    if floor.is_null() { return ptr::null_mut(); }
    let floor = unsafe { *floor };
    if floor.version != 1 || floor.byte_size != size_of::<FloorV1>() || floor.admit.is_none() {
        return ptr::null_mut();
    }
    let mut owned = Box::new(Observation {
        api: ApiV1 { version: 1, byte_size: size_of::<ApiV1>(), context: ptr::null_mut(), replace: Some(replace) },
        floor, hook: None,
    });
    owned.api.context = ptr::from_mut(owned.as_mut()).cast();
    Box::into_raw(owned).cast()
}
unsafe extern "C" fn destroy(api: *mut ApiV1) {
    unsafe { drop(Box::from_raw(api.cast::<Observation>())) };
}
static FACTORY: FactoryV1 = FactoryV1 {
    version: 1, byte_size: size_of::<FactoryV1>(), create: Some(create), destroy: Some(destroy), notify: Some(notify),
};
/// # Safety
/// Factory operations require owner-thread access and their declared lifetimes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_method_observation_factory_v1() -> *const FactoryV1 { &FACTORY }

/// # Safety
/// All borrowed descriptors and code have the native initialization lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(
    api: *mut birnam_abi::IdApi, _: *mut c_void,
    _: *const birnam_abi::namespace_presence::ApiV1,
    _: *const birnam_abi::namespace_import::ApiV1,
    _: *const birnam_abi::namespace_export::ApiV1,
    _: *const birnam_abi::vtable_enumeration::ApiV2,
    _: *const ApiV1,
) -> *mut c_void {
    assert!(!api.is_null(), "Method Observation received a null IdApi");
    ptr::null_mut()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> c_int { 1 }

#[cfg(test)]
mod tests {
    use super::*;
    use birnam_abi::method_observation;
    use std::cell::{Cell, RefCell};

    thread_local! {
        static EVENTS: RefCell<Vec<(u8, usize, usize)>> = const { RefCell::new(Vec::new()) };
        static REENTER: Cell<*mut ApiV1> = const { Cell::new(ptr::null_mut()) };
    }
    unsafe extern "C" fn first(vtable: birnam_abi::Oop, selector: birnam_abi::Oop) {
        EVENTS.with_borrow_mut(|events| events.push((1, vtable.addr(), selector.addr())));
        let api = REENTER.replace(ptr::null_mut());
        if !api.is_null() {
            unsafe { method_observation::replace(api, Some(second)); notify(api, 1, vtable, selector); }
        }
    }
    unsafe extern "C" fn second(vtable: birnam_abi::Oop, selector: birnam_abi::Oop) {
        EVENTS.with_borrow_mut(|events| events.push((2, vtable.addr(), selector.addr())));
    }
    unsafe extern "C" fn admit(context: *mut c_void) {
        unsafe { *context.cast::<usize>() += 1; }
    }
    fn floor(count: &mut usize) -> FloorV1 {
        FloorV1 { version: 1, byte_size: size_of::<FloorV1>(), context: ptr::from_mut(count).cast(), admit: Some(admit) }
    }
    unsafe fn fire(api: *mut ApiV1, direct: c_int) {
        unsafe { notify(api, direct, ptr::without_provenance_mut(17), ptr::without_provenance_mut(29)); }
    }

    #[test]
    fn creation_validates_without_admission_and_copies_the_floor() {
        let mut count = 0;
        let mut descriptor = floor(&mut count);
        unsafe {
            assert!(create(ptr::null()).is_null());
            descriptor.version = 2; assert!(create(&descriptor).is_null());
            descriptor.version = 1; descriptor.byte_size -= 1; assert!(create(&descriptor).is_null());
            descriptor = floor(&mut count); descriptor.admit = None; assert!(create(&descriptor).is_null());
            descriptor = floor(&mut count);
            let api = create(&descriptor); assert!(!api.is_null()); assert_eq!(count, 0);
            descriptor.context = ptr::null_mut(); descriptor.admit = None;
            method_observation::replace(api, Some(first)); assert_eq!(count, 1);
            destroy(api); assert_eq!(count, 1);
            assert!(descriptor.admit.is_none());
        }
    }

    #[test]
    fn independent_slots_preserve_replace_clear_and_direct_binding_laws() {
        EVENTS.with_borrow_mut(Vec::clear);
        let (mut left_admissions, mut right_admissions) = (0, 0);
        unsafe {
            let left = create(&floor(&mut left_admissions));
            let right = create(&floor(&mut right_admissions));
            fire(left, 1); fire(right, 1);
            method_observation::replace(left, Some(first));
            method_observation::replace(left, Some(first));
            method_observation::replace(right, Some(second));
            fire(left, 0); fire(left, 1); fire(right, 1);
            method_observation::replace(left, Some(second)); fire(left, 1);
            method_observation::replace(left, None); fire(left, 1); fire(right, 1);
            method_observation::replace(left, None);
            destroy(left); destroy(right);
        }
        assert_eq!((left_admissions, right_admissions), (5, 1));
        EVENTS.with_borrow(|events| assert_eq!(events.as_slice(), &[(1,17,29),(2,17,29),(2,17,29),(2,17,29)]));
    }

    #[test]
    fn notification_snapshot_allows_reentry_and_subsequent_selection() {
        EVENTS.with_borrow_mut(Vec::clear);
        let mut admissions = 0;
        unsafe {
            let api = create(&floor(&mut admissions));
            method_observation::replace(api, Some(first));
            REENTER.set(api); fire(api, 1); fire(api, 1);
            destroy(api);
        }
        assert_eq!(admissions, 2);
        EVENTS.with_borrow(|events| assert_eq!(events.as_slice(), &[(1,17,29),(2,17,29),(2,17,29)]));
    }

    #[test]
    fn copied_public_selection_cannot_retarget_owned_notification_or_destruction() {
        EVENTS.with_borrow_mut(Vec::clear);
        let (mut left_admissions, mut right_admissions) = (0, 0);
        unsafe {
            let left = create(&floor(&mut left_admissions));
            let right = create(&floor(&mut right_admissions));
            let copy = *left;
            method_observation::replace(&copy, Some(first));
            (*left).context = (*right).context;
            method_observation::replace(left, Some(second));
            // Internal notification and destruction always use the original
            // owned address, independent of public operation/context changes.
            (*left).replace = None;
            fire(left, 1); fire(right, 1);
            destroy(left); fire(right, 1); destroy(right);
        }
        assert_eq!((left_admissions, right_admissions), (1, 1));
        EVENTS.with_borrow(|events| assert_eq!(events.as_slice(), &[(1,17,29),(2,17,29),(2,17,29)]));
    }

    #[test]
    fn outer_replacement_follows_reentrant_admission() {
        unsafe extern "C" fn reentrant_admit(context: *mut c_void) {
            let api = unsafe { *context.cast::<*mut ApiV1>() };
            if !REENTER.replace(ptr::null_mut()).is_null() {
                unsafe { method_observation::replace(api, Some(second)); }
            }
        }
        EVENTS.with_borrow_mut(Vec::clear);
        let mut api: *mut ApiV1 = ptr::null_mut();
        let descriptor = FloorV1 { version:1, byte_size:size_of::<FloorV1>(), context:ptr::from_mut(&mut api).cast(), admit:Some(reentrant_admit) };
        unsafe {
            api = create(&descriptor); REENTER.set(api);
            method_observation::replace(api, Some(first)); fire(api, 1); destroy(api);
        }
        EVENTS.with_borrow(|events| assert_eq!(events.as_slice(), &[(1,17,29)]));
    }
}
