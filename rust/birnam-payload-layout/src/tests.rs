use super::*;
use core::cell::{Cell, UnsafeCell};

struct Floor {
    owned: UnsafeCell<OwnedV1>,
    calls: Cell<usize>,
    reenter: Cell<bool>,
    api: Cell<*mut ApiV1>,
    input: Cell<*mut u8>,
}
impl Floor {
    fn new() -> Self {
        Self { owned: UnsafeCell::new(OwnedV1::UNSET), calls: Cell::new(0),
            reenter: Cell::new(false), api: Cell::new(ptr::null_mut()), input: Cell::new(ptr::null_mut()) }
    }
    fn descriptor(&self) -> FloorV1 {
        FloorV1 { version: 1, byte_size: size_of::<FloorV1>(),
            context: ptr::from_ref(self).cast_mut().cast(),
            admit: Some(admit), replace: Some(replace), view: Some(view) }
    }
}
impl Drop for Floor {
    fn drop(&mut self) {
        unsafe { release(core::mem::replace(self.owned.get_mut(), OwnedV1::UNSET)) };
    }
}
unsafe extern "C" fn admit(context: *mut c_void) {
    let floor = unsafe { &*context.cast::<Floor>() };
    floor.calls.set(floor.calls.get() + 1);
    if floor.reenter.replace(false) {
        unsafe { birnam_abi::payload_layout::set(floor.api.get(), ptr::null_mut(), ptr::null(), 0, 9) };
        // The outer input must be copied after admission returns.
        if !floor.input.get().is_null() { unsafe { *floor.input.get() = SLOT_OOP }; }
    }
}
unsafe extern "C" fn replace(context: *mut c_void, _: Oop, owned: OwnedV1) {
    let floor = unsafe { &*context.cast::<Floor>() };
    let previous = unsafe { core::mem::replace(&mut *floor.owned.get(), owned) };
    unsafe { release(previous) };
}
unsafe extern "C" fn view(context: *mut c_void, _: Oop) -> ViewV1 {
    unsafe { (*(*context.cast::<Floor>()).owned.get()).view }
}

#[test]
fn unset_empty_arbitrary_kinds_and_extent_boundaries() {
    unsafe {
        assert_eq!(has_layout(ViewV1::UNSET), 0);
        assert_eq!(slot_kind(ViewV1::UNSET, 0), SLOT_UNKNOWN);
        assert_eq!(all_oop(ViewV1::UNSET, 0), 0);
        for rest in [0, 1, 2, 9, 255] {
            // An unreadable pointer with zero extent must never be touched.
            let empty = copy(1usize as *const u8, 0, rest);
            assert!(empty.view.kinds.is_null());
            assert_eq!(has_layout(empty.view), 1);
            assert_eq!(all_oop(empty.view, 0), 1);
            assert_eq!(slot_kind(empty.view, usize::MAX), rest);
            assert_eq!(all_oop(empty.view, usize::MAX), c_int::from(rest == SLOT_OOP));
            release(empty);
        }
        let input = [1, 1, 9, 0, 255, 2];
        for rest in [0, 1, 9, 255] {
            let owned = copy(input.as_ptr(), input.len(), rest);
            for (index, kind) in input.iter().copied().enumerate() {
                assert_eq!(slot_kind(owned.view, index), kind);
            }
            assert_eq!(slot_kind(owned.view, input.len()), rest);
            assert_eq!(slot_kind(owned.view, usize::MAX), rest);
            for words in [0, 1, 2] { assert_eq!(all_oop(owned.view, words), 1); }
            for words in [3, 6, 7, usize::MAX] { assert_eq!(all_oop(owned.view, words), 0); }
            release(owned);
        }
        let prefix = [1, 1];
        let owned = copy(prefix.as_ptr(), 2, 0);
        assert_eq!(all_oop(owned.view, 2), 1);
        assert_eq!(all_oop(owned.view, 3), 0);
        release(owned);
    }
}

#[test]
fn input_is_copied_and_self_borrowed_replacement_survives_release() {
    let floor = Floor::new();
    unsafe {
        let api = create(&floor.descriptor());
        let mut input = [1, 9, 2];
        birnam_abi::payload_layout::set(api, ptr::null_mut(), input.as_ptr(), input.len(), 0);
        input.fill(0);
        let old = (*floor.owned.get()).view;
        assert_ne!(old.kinds, input.as_ptr());
        assert_eq!(slot_kind(old, 1), 9);
        birnam_abi::payload_layout::set(api, ptr::null_mut(), old.kinds, old.count, 1);
        let new = (*floor.owned.get()).view;
        assert_ne!(new.kinds, old.kinds);
        assert_eq!(slot_kind(new, 0), 1);
        assert_eq!(slot_kind(new, 1), 9);
        assert_eq!(slot_kind(new, 2), 2);
        assert_eq!(slot_kind(new, 3), 1);
        destroy(api);
    }
}

#[test]
fn factory_validation_is_inert_and_floor_is_copied() {
    let floor = Floor::new();
    unsafe {
        assert!(create(ptr::null()).is_null());
        let mut bad = floor.descriptor(); bad.version = 2; assert!(create(&bad).is_null());
        bad = floor.descriptor(); bad.byte_size -= 1; assert!(create(&bad).is_null());
        bad = floor.descriptor(); bad.admit = None; assert!(create(&bad).is_null());
        bad = floor.descriptor(); bad.replace = None; assert!(create(&bad).is_null());
        bad = floor.descriptor(); bad.view = None; assert!(create(&bad).is_null());
        let mut selected = floor.descriptor();
        let api = create(&selected);
        assert_eq!(floor.calls.get(), 0);
        selected.context = ptr::null_mut(); selected.admit = None;
        assert!(selected.admit.is_none());
        birnam_abi::payload_layout::set(api, ptr::null_mut(), ptr::null(), 0, 9);
        assert_eq!(birnam_abi::payload_layout::slot_kind(api, ptr::null_mut(), 0), 9);
        assert_eq!(floor.calls.get(), 2);
        destroy(api);
        assert_eq!(floor.calls.get(), 2);
    }
}

#[test]
fn admission_reentry_precedes_outer_copy_and_replacement() {
    let floor = Floor::new();
    unsafe {
        let api = create(&floor.descriptor()); floor.api.set(api);
        let mut input = 0;
        floor.input.set(&mut input); floor.reenter.set(true);
        birnam_abi::payload_layout::set(api, ptr::null_mut(), &input, 1, 2);
        assert_eq!(floor.calls.get(), 2);
        assert_eq!(slot_kind((*floor.owned.get()).view, 0), SLOT_OOP);
        assert_eq!(slot_kind((*floor.owned.get()).view, 1), 2);
        floor.input.set(ptr::null_mut()); floor.reenter.set(true);
        assert_eq!(birnam_abi::payload_layout::slot_kind(api, ptr::null_mut(), 0), 9);
        assert_eq!(floor.calls.get(), 4);
        destroy(api);
    }
}

#[test]
fn canonical_operations_ignore_public_descriptor_and_keep_independent_owners() {
    let first = Floor::new(); let second = Floor::new();
    unsafe {
        let a = create(&first.descriptor()); let b = create(&second.descriptor());
        birnam_abi::payload_layout::set(a, ptr::null_mut(), [1].as_ptr(), 1, 0);
        birnam_abi::payload_layout::set(b, ptr::null_mut(), [2].as_ptr(), 1, 9);
        (*a).context = ptr::null_mut(); (*a).set = None; (*a).slot_kind = None;
        let a_view = (*first.owned.get()).view;
        assert_eq!(has_layout(a_view), 1);
        assert_eq!(all_oop(a_view, 1), 1);
        assert_eq!(slot_kind(a_view, 0), 1);
        destroy(a);
        assert_eq!(birnam_abi::payload_layout::slot_kind(b, ptr::null_mut(), 0), 2);
        assert_eq!(first.calls.get(), 1);
        assert_eq!(second.calls.get(), 2);
        destroy(b);
    }
}
