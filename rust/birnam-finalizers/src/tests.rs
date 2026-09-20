use super::*;
use core::cell::{Cell, RefCell};

unsafe extern "C" fn first(_: Oop) { panic!("registration invoked first finalizer"); }
unsafe extern "C" fn second(_: Oop) { panic!("registration invoked second finalizer"); }
fn oop(index: usize) -> Oop { ptr::without_provenance_mut(index) }
fn address(callback: Option<Finalizer>) -> Option<usize> { callback.map(|callback| callback as usize) }

#[test]
fn canonical_slots_keep_one_callback_word_and_never_invoke_it() {
    assert_eq!(size_of::<SlotV1>(), size_of::<Finalizer>());
    for callback in [first as Finalizer, second] {
        assert_eq!(address(unsafe { FACTORY.slot.unwrap()(0, callback) }.callback), Some(callback as usize));
        assert!(unsafe { FACTORY.slot.unwrap()(1, callback) }.callback.is_none());
    }
    assert!(SlotV1::EMPTY.callback.is_none());
}

#[derive(Debug, PartialEq)]
enum Event { Admit, Read(usize), Write(usize, usize) }
struct Floor {
    api: Cell<*mut ApiV1>,
    reenter: Cell<bool>,
    events: RefCell<Vec<Event>>,
    attached: Cell<Option<Finalizer>>,
}
impl Floor {
    fn new() -> Self { Self { api: Cell::new(ptr::null_mut()), reenter: Cell::new(false),
        events: RefCell::new(Vec::new()), attached: Cell::new(None) } }
    fn descriptor(&self) -> FloorV1 {
        FloorV1 { version: 1, byte_size: size_of::<FloorV1>(), context: ptr::from_ref(self).cast_mut().cast(),
            admit: Some(admit), read_permanent: Some(read), replace: Some(replace) }
    }
}
unsafe extern "C" fn admit(context: *mut c_void) {
    let floor = unsafe { &*context.cast::<Floor>() };
    floor.events.borrow_mut().push(Event::Admit);
    if floor.reenter.replace(false) {
        unsafe { birnam_abi::finalizers::set(floor.api.get(), oop(16), second) };
    }
}
unsafe extern "C" fn read(context: *mut c_void, object: Oop, output: *mut c_int) -> c_int {
    let floor = unsafe { &*context.cast::<Floor>() };
    floor.events.borrow_mut().push(Event::Read(object.addr()));
    match object.addr() {
        16 => { unsafe { output.write(0) }; 1 }
        32 => { unsafe { output.write(1) }; 1 }
        _ => 0,
    }
}
unsafe extern "C" fn replace(context: *mut c_void, object: Oop, slot: SlotV1) {
    assert_eq!(object, oop(16));
    let floor = unsafe { &*context.cast::<Floor>() };
    floor.events.borrow_mut().push(Event::Write(object.addr(), address(slot.callback).unwrap()));
    floor.attached.set(slot.callback);
}

#[test]
fn exact_membership_and_permanence_follow_admission_and_replacement_is_silent() {
    let floor = Floor::new();
    let api = unsafe { create(&floor.descriptor()) };
    assert!(unsafe { birnam_abi::finalizers::valid(api) });
    // Exhaustively test a small address space, including null, exact starts,
    // interiors, and foreign addresses. Expected eligibility is a fixed set.
    for object in 0..64 {
        floor.events.borrow_mut().clear();
        unsafe { birnam_abi::finalizers::set(api, oop(object), first) };
        let mut expected = vec![Event::Admit, Event::Read(object)];
        if object == 16 { expected.push(Event::Write(16, first as *const () as usize)); }
        assert_eq!(*floor.events.borrow(), expected);
    }
    unsafe { birnam_abi::finalizers::set(api, oop(16), second) };
    assert_eq!(address(floor.attached.get()), Some(second as *const () as usize));
    unsafe { birnam_abi::finalizers::set(api, oop(32), first); destroy(api); }
    assert_eq!(address(floor.attached.get()), Some(second as *const () as usize));
}

#[test]
fn copied_floor_reentry_live_public_selection_and_independent_canonical_operations() {
    let a = Floor::new();
    let b = Floor::new();
    let mut descriptor = a.descriptor();
    let api_a = unsafe { create(&descriptor) };
    descriptor.context = b.descriptor().context;
    let api_b = unsafe { create(&descriptor) };
    assert_ne!(api_a, api_b);
    assert!(a.events.borrow().is_empty() && b.events.borrow().is_empty());
    a.api.set(api_a); a.reenter.set(true);
    unsafe { birnam_abi::finalizers::set(api_a, oop(16), first) };
    assert_eq!(*a.events.borrow(), [Event::Admit, Event::Admit, Event::Read(16),
        Event::Write(16, second as *const () as usize), Event::Read(16), Event::Write(16, first as *const () as usize)]);
    assert!(b.events.borrow().is_empty());
    unsafe extern "C" fn alternate(context: *mut c_void, object: Oop, callback: Finalizer) {
        assert_eq!(context.addr(), 45); assert_eq!(object.addr(), 19);
        assert_eq!(callback as usize, second as *const () as usize);
    }
    unsafe {
        (*api_a).context = oop(45); (*api_a).set = Some(alternate);
        birnam_abi::finalizers::set(api_a, oop(19), second);
        assert_eq!(address(FACTORY.slot.unwrap()(0, first).callback), Some(first as *const () as usize));
        destroy(api_a);
        birnam_abi::finalizers::set(api_b, oop(16), second);
        destroy(api_b);
    }
    assert_eq!(*b.events.borrow(), [Event::Admit, Event::Read(16), Event::Write(16, second as *const () as usize)]);
}

#[test]
fn factory_and_capability_validation_are_inert() {
    let floor = Floor::new();
    unsafe {
        assert!(create(ptr::null()).is_null());
        let mut bad = floor.descriptor(); bad.version += 1; assert!(create(&bad).is_null());
        bad = floor.descriptor(); bad.byte_size -= 1; assert!(create(&bad).is_null());
        bad = floor.descriptor(); bad.admit = None; assert!(create(&bad).is_null());
        bad = floor.descriptor(); bad.read_permanent = None; assert!(create(&bad).is_null());
        bad = floor.descriptor(); bad.replace = None; assert!(create(&bad).is_null());
        let api = create(&floor.descriptor());
        assert!(birnam_abi::finalizers::valid(api));
        assert!(!birnam_abi::finalizers::valid(ptr::null()));
        let mut bad = *api; bad.version += 1; assert!(!birnam_abi::finalizers::valid(&bad));
        bad = *api; bad.byte_size -= 1; assert!(!birnam_abi::finalizers::valid(&bad));
        bad = *api; bad.set = None; assert!(!birnam_abi::finalizers::valid(&bad));
        destroy(api);
    }
    assert!(floor.events.borrow().is_empty());
}
