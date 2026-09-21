use super::*;
use crate::dispatch::build_bootstrap;
use core::cell::RefCell;

thread_local! {
    static OBJECTS: Cell<[Oop; 6]> = const { Cell::new([core::ptr::null_mut(); 6]) };
    static EVENTS: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
    static NEXT_PRECISE: Cell<bool> = const { Cell::new(false) };
}

unsafe extern "C" fn change_settings_hook(_: Oop, _: MarkFn) {
    event("change settings");
    with_active(|runtime| unsafe {
        let settings = runtime.gc_settings();
        birnam_abi::gc_settings::set_precise(settings, i32::from(NEXT_PRECISE.get()));
        birnam_abi::gc_settings::set_auto(settings, 0);
        assert_eq!(runtime.gc_collect(), 0);
    });
}

#[test]
fn precise_mode_is_read_per_object_after_earlier_trace_hook_changes_it() {
    for next in [true, false] {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.enter(|| {
            EVENTS.with_borrow_mut(Vec::clear);
            NEXT_PRECISE.set(next);
            let base = runtime.globals().object_vt;
            let a_vt = runtime.create_vtable(base);
            let b_vt = runtime.create_vtable(base);
            let child_vt = runtime.create_vtable(base);
            let a = runtime.allocate_instance(a_vt, size_of::<Oop>());
            let b = runtime.allocate_instance(b_vt, 0);
            let child = runtime.allocate_instance(child_vt, 0);
            unsafe { a.cast::<Oop>().write(child) };
            OBJECTS.set([a, b, core::ptr::null_mut(), child, core::ptr::null_mut(), a_vt]);
            runtime.set_layout(a_vt, &crate::abi::SLOT_RAW, 1, crate::abi::SLOT_RAW);
            runtime.set_trace_hook(b_vt, change_settings_hook);
            runtime.set_trace_hook(child_vt, child_hook);
            runtime.gc_set_precise(!next);
            runtime.gc_register_root_provider(first_roots);
            runtime.gc_collect_from_native_range(0, 0);
            let metadata = unsafe { &(*runtime.state.get()).metadata };
            assert_eq!(metadata.precise(), next);
            assert!(!metadata.auto_requested());
            assert_eq!(runtime.checked_vtable_of(child).is_null(), next);
            let expected: &[&str] = if next { &["roots1", "change settings"] }
                else { &["roots1", "change settings", "child"] };
            assert_eq!(EVENTS.with_borrow(Clone::clone), expected);
            OBJECTS.set([core::ptr::null_mut(); 6]);
        });
    }
}
fn event(value: &'static str) {
    EVENTS.with_borrow_mut(|events| events.push(value));
}
unsafe extern "C" fn first_roots(mark: MarkFn) {
    event("roots1");
    let objects = OBJECTS.get();
    unsafe {
        mark(objects[0]);
        mark(objects[1]);
    }
}
unsafe extern "C" fn second_roots(mark: MarkFn) {
    event("roots2");
    let objects = OBJECTS.get();
    unsafe {
        mark(objects[1]);
        mark((objects[4].addr() + 1) as Oop);
    }
    with_active(|runtime| assert_eq!(runtime.gc_collect(), 0));
}
unsafe extern "C" fn obsolete_hook(_: Oop, _: MarkFn) {
    panic!("stale hook used");
}
unsafe extern "C" fn new_hook(_: Oop, _: MarkFn) {
    event("newA");
}
unsafe extern "C" fn child_hook(_: Oop, _: MarkFn) {
    event("child");
}
unsafe extern "C" fn c_hook(_: Oop, _: MarkFn) {
    event("C");
}
unsafe extern "C" fn b_hook(_: Oop, mark: MarkFn) {
    event("B");
    let objects = OBJECTS.get();
    with_active(|runtime| {
        // This runs after root enumeration but before A is popped. Its current
        // payload policy and hook must be read at A's turn, not snapshotted early.
        let kind = SLOT_OOP;
        unsafe { birnam_abi::payload_layout::set(runtime.payload_layout(), objects[5], &kind, 1, crate::abi::SLOT_RAW) };
        runtime.set_trace_hook(objects[5], new_hook);
    });
    unsafe {
        mark(objects[2]);
    }
}
unsafe extern "C" fn dead_hook(_: Oop, _: MarkFn) {
    panic!("unreached hook executed");
}

#[test]
fn live_walk_preserves_provider_order_and_delays_layout_and_hook_lookup() {
    let bootstrap = build_bootstrap();
    let runtime = &bootstrap.runtime;
    runtime.enter(|| {
        EVENTS.with_borrow_mut(Vec::clear);
        let base = runtime.globals().object_vt;
        let a_vt = runtime.create_vtable(base);
        let b_vt = runtime.create_vtable(base);
        let c_vt = runtime.create_vtable(base);
        let child_vt = runtime.create_vtable(base);
        let dead_vt = runtime.create_vtable(base);
        let a = runtime.allocate_instance(a_vt, size_of::<Oop>());
        let b = runtime.allocate_instance(b_vt, 0);
        let c = runtime.allocate_instance(c_vt, 0);
        let child = runtime.allocate_instance(child_vt, 0);
        let dead = runtime.allocate_instance(dead_vt, 0);
        unsafe {
            a.cast::<Oop>().write(child);
        }
        OBJECTS.set([a, b, c, child, dead, a_vt]);
        runtime.set_trace_hook(a_vt, obsolete_hook);
        runtime.set_trace_hook(b_vt, b_hook);
        runtime.set_trace_hook(c_vt, c_hook);
        runtime.set_trace_hook(child_vt, child_hook);
        runtime.set_trace_hook(dead_vt, dead_hook);
        let raw = crate::abi::SLOT_RAW;
        runtime.set_layout(a_vt, &raw, 1, raw);
        runtime.gc_set_precise(true);
        runtime.gc_register_root_provider(first_roots);
        runtime.gc_register_root_provider(second_roots);
        let reclaimed = runtime.gc_collect_from_native_range(0, 0);
        assert!(reclaimed >= 1);
        assert_eq!(
            EVENTS.with_borrow(Clone::clone),
            ["roots1", "roots2", "B", "C", "newA", "child"]
        );
        assert_eq!(runtime.checked_vtable_of(child), child_vt);
        assert_eq!(runtime.checked_vtable_of(dead), core::ptr::null_mut());
        OBJECTS.set([core::ptr::null_mut(); 6]);
    });
}
