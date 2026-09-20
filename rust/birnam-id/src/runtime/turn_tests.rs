use super::*;
use crate::dispatch::build_bootstrap;
use core::ffi::c_void;
use std::rc::Rc;

struct Release {
    calls: Rc<Cell<usize>>,
    result: Oop,
    reenter: bool,
}

unsafe extern "C" fn release(context: *mut c_void) -> u32 {
    let record = unsafe { Box::from_raw(context.cast::<Release>()) };
    with_active(|runtime| {
        assert!(!unsafe { (&*runtime.state.get()).execution.has_managed_turn() });
        record.calls.set(record.calls.get() + 1);
        if record.reenter {
            let child = Box::into_raw(Box::new(Release {
                calls: record.calls.clone(), result: core::ptr::null_mut(), reenter: false,
            }));
            runtime.queue_native_retirement(child.cast(), release);
            runtime.enter_application(|| {
                assert!(unsafe { (&*runtime.state.get()).execution.has_managed_turn() });
                // Exclude the native stack: only the pinned entry's explicit
                // result root can keep this otherwise unreachable object alive.
                runtime.gc_collect_from_native_range(0, 0);
                assert_eq!(runtime.permanence_of(record.result), Some(Permanence::Collectable));
                runtime.retirement_checkpoint();
                record.result
            });
            assert_eq!(record.calls.get(), 1, "reentry cannot recursively drain");
        }
    });
    1
}

#[test]
fn outer_checkpoint_roots_the_result_and_protects_reentrant_callbacks() {
    let bootstrap = build_bootstrap();
    let runtime = &bootstrap.runtime;
    runtime.publish_canonical_exports();
    runtime.install_execution_objects();
    let calls = Rc::new(Cell::new(0));
    let result = runtime.enter_application(|| {
        let result = runtime.allocate_instance(runtime.globals().object_vt, 0);
        let record = Box::into_raw(Box::new(Release {
            calls: calls.clone(), result, reenter: true,
        }));
        runtime.queue_native_retirement(record.cast(), release);
        runtime.enter_application(|| runtime.canonical().nil);
        assert_eq!(calls.get(), 0, "nested return is not a checkpoint");
        result
    });
    assert_eq!(calls.get(), 1);
    assert_eq!(runtime.permanence_of(result), Some(Permanence::Collectable));
    runtime.enter_application(|| runtime.canonical().nil);
    assert_eq!(calls.get(), 2, "new work drains at the next outer checkpoint");
    runtime.enter(|| runtime.gc_collect_from_native_range(0, 0));
    assert_eq!(runtime.permanence_of(result), None, "outward root is scoped");
    runtime.enter_application(|| runtime.canonical().nil);
    assert_eq!(calls.get(), 2, "native ownership is released exactly once");
}

#[test]
fn collection_inside_a_turn_queues_release_until_the_pinned_outer_return() {
    let bootstrap = build_bootstrap();
    let runtime = &bootstrap.runtime;
    let calls = Rc::new(Cell::new(0));
    runtime.enter_application(|| {
        let record = Box::into_raw(Box::new(Release {
            calls: calls.clone(), result: core::ptr::null_mut(), reenter: false,
        }));
        runtime.queue_native_retirement(record.cast(), release);
        runtime.gc_collect();
        assert_eq!(calls.get(), 0);
        runtime.enter_application(|| {
            runtime.gc_collect();
            assert_eq!(calls.get(), 0);
            runtime.canonical().nil
        })
    });
    assert_eq!(calls.get(), 1);
}

unsafe extern "C" fn detach_finalized_owner(owner: Oop) {
    let slot = owner.cast::<*mut Release>();
    let record = unsafe { slot.replace(core::ptr::null_mut()) };
    if !record.is_null() {
        with_active(|runtime| runtime.queue_native_retirement(record.cast(), release));
    }
}

#[test]
fn finalized_owner_is_freed_before_its_detached_native_record_is_released() {
    let bootstrap = build_bootstrap();
    let runtime = &bootstrap.runtime;
    let calls = Rc::new(Cell::new(0));
    runtime.enter_application(|| {
        let owner = runtime.allocate_instance(runtime.globals().object_vt, size_of::<*mut Release>());
        let record = Box::into_raw(Box::new(Release {
            calls: calls.clone(), result: core::ptr::null_mut(), reenter: false,
        }));
        unsafe { owner.cast::<*mut Release>().write(record) };
        runtime.set_finalizer(owner, detach_finalized_owner);
        runtime.gc_collect_from_native_range(0, 0);
        assert_eq!(runtime.permanence_of(owner), None, "collector freed the owner payload");
        assert_eq!(calls.get(), 0, "detached native ownership survives the payload");
        runtime.canonical().nil
    });
    assert_eq!(calls.get(), 1);
    runtime.enter_application(|| runtime.canonical().nil);
    assert_eq!(calls.get(), 1);
}
