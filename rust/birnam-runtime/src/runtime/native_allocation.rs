//! Generic construction of blocks with native-owned payloads.
use super::*;
use birnam_abi::native_allocation::{NativeAllocationPlanV1, NativeAllocationV1};
use core::ffi::c_void;

impl Runtime {
    pub(crate) fn native_allocation(&self) -> NativeAllocationV1 {
        self.assert_owner();
        NativeAllocationV1 {
            version: 1,
            byte_size: size_of::<NativeAllocationV1>(),
            context: core::ptr::from_ref(self).cast_mut().cast(),
            allocate,
        }
    }
}

unsafe extern "C" fn allocate(context: *mut c_void, plan: *const NativeAllocationPlanV1) -> Oop {
    boundary::require_invariant(
        !context.is_null() && !plan.is_null(),
        "invalid native allocation",
    );
    let runtime = unsafe { &*context.cast::<Runtime>() };
    let plan = unsafe { plan.read() };
    runtime.assert_owner();
    runtime.require_allocation_allowed();
    runtime.maybe_collect_before_allocation();
    // These admitted callbacks only touch private native state. In particular
    // no callback may reenter Id while the heap authority below is borrowed.
    unsafe { (plan.prepare)(plan.context) };
    let state = unsafe { &mut *runtime.state.get() };
    let object = state
        .heap
        .allocate_with_behavior(plan.payload_size, Permanence::Collectable, plan.behavior);
    unsafe { (plan.initialize)(plan.context, object) };
    if let Some(finalizer) = plan.finalizer {
        boundary::require_invariant(
            state.heap.set_finalizer(object, finalizer),
            "new native block lost finalizer",
        );
    }
    object
}
