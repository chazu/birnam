use super::*;
use core::cell::{Cell, RefCell};

struct Floor {
    api: Cell<*mut ApiV1>,
    reenter: Cell<bool>,
    seen: RefCell<Vec<usize>>,
}
impl Floor {
    fn new() -> Self { Self { api: Cell::new(ptr::null_mut()), reenter: Cell::new(false), seen: RefCell::new(Vec::new()) } }
    fn descriptor(&self) -> FloorV1 {
        FloorV1 { version: 1, byte_size: size_of::<FloorV1>(),
            context: ptr::from_ref(self).cast_mut().cast(), admit: Some(admit) }
    }
}
unsafe extern "C" fn admit(context: *mut c_void) {
    let floor = unsafe { &*context.cast::<Floor>() };
    floor.seen.borrow_mut().push(unsafe { get(floor.api.get()) });
    if floor.reenter.replace(false) {
        unsafe { birnam_abi::native_stack::set(floor.api.get(), 17) };
        assert_eq!(unsafe { get(floor.api.get()) }, 17);
    }
}
#[test]
fn zero_default_and_all_address_bits_round_trip_after_admission() {
    let floor = Floor::new();
    let api = unsafe { create(&floor.descriptor()) };floor.api.set(api);
    assert_eq!(unsafe { get(api) }, 0);assert!(floor.seen.borrow().is_empty());
    let addresses = [0, 1, 3, usize::MAX, usize::MAX - 1, 0];
    let mut previous = 0;
    for address in addresses {
        unsafe { birnam_abi::native_stack::set(api, address) };
        assert_eq!(floor.seen.borrow().last(), Some(&previous));
        assert_eq!(unsafe { get(api) }, address);previous = address;
    }
    unsafe { destroy(api) };assert_eq!(floor.seen.borrow().len(), addresses.len());
}
#[test]
fn copied_floor_and_reentry_complete_before_the_outer_assignment() {
    let a = Floor::new();let b = Floor::new();
    let mut descriptor = a.descriptor();
    let api_a = unsafe { create(&descriptor) };a.api.set(api_a);
    descriptor.context = b.descriptor().context;
    let api_b = unsafe { create(&descriptor) };b.api.set(api_b);
    a.reenter.set(true);
    unsafe { birnam_abi::native_stack::set(api_a, 23) };
    assert_eq!(*a.seen.borrow(), [0, 0]);assert!(b.seen.borrow().is_empty());
    assert_eq!(unsafe { get(api_a) }, 23);assert_eq!(unsafe { get(api_b) }, 0);
    unsafe { destroy(api_a);birnam_abi::native_stack::set(api_b, 91);destroy(api_b); }
    assert_eq!(*b.seen.borrow(), [0]);
}
#[test]
fn live_public_selection_does_not_redirect_canonical_state_or_destruction() {
    let floor = Floor::new();
    let api = unsafe { create(&floor.descriptor()) };floor.api.set(api);
    unsafe extern "C" fn selected(context: *mut c_void, address: usize) {
        assert_eq!(context.addr(), 7);assert_eq!(address, 19);
    }
    unsafe {
        (*api).context = ptr::without_provenance_mut(7);(*api).set = Some(selected);
        birnam_abi::native_stack::set(api, 19);
        assert_eq!(get(api), 0);set(api, usize::MAX);assert_eq!(get(api), usize::MAX);
        (*api).context = ptr::null_mut();(*api).set = None;
        set(api, 0);assert_eq!(get(api), 0);destroy(api);
    }
    assert!(floor.seen.borrow().is_empty());
}
#[test]
fn descriptor_validation_and_teardown_are_inert() {
    let floor = Floor::new();
    unsafe {
        assert!(create(ptr::null()).is_null());
        let mut bad = floor.descriptor();bad.version += 1;assert!(create(&bad).is_null());
        bad = floor.descriptor();bad.byte_size -= 1;assert!(create(&bad).is_null());
        bad = floor.descriptor();bad.admit = None;assert!(create(&bad).is_null());
        let api = create(&floor.descriptor());
        assert!(birnam_abi::native_stack::valid(api));assert!(!birnam_abi::native_stack::valid(ptr::null()));
        let mut bad = *api;bad.version += 1;assert!(!birnam_abi::native_stack::valid(&bad));
        bad = *api;bad.byte_size -= 1;assert!(!birnam_abi::native_stack::valid(&bad));
        bad = *api;bad.set = None;assert!(!birnam_abi::native_stack::valid(&bad));destroy(api);
    }
    assert!(floor.seen.borrow().is_empty());
}
