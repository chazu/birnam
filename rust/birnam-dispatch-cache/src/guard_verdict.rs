//! Independent guard facts; epoch validity and eligibility remain with callers.
use birnam_abi::guard_verdict::{FactoryV1, StateV1};
use core::{cell::Cell, ffi::{c_int, c_void}};

unsafe extern "C" fn create() -> *mut c_void {
    Box::into_raw(Box::new(Cell::new(StateV1 { epoch: u64::MAX, verified: 0 }))).cast()
}
unsafe extern "C" fn destroy(context: *mut c_void) {
    unsafe { drop(Box::from_raw(context.cast::<Cell<StateV1>>())) };
}
unsafe extern "C" fn read(context: *const c_void, out: *mut StateV1) {
    unsafe { out.write((&*context.cast::<Cell<StateV1>>()).get()) };
}
unsafe extern "C" fn write(context: *mut c_void, epoch: u64, verified: c_int) {
    unsafe { (&*context.cast::<Cell<StateV1>>()).set(StateV1 { epoch, verified: u32::from(verified != 0) }) };
}
static FACTORY: FactoryV1 = FactoryV1 {
    version: 1, byte_size: size_of::<FactoryV1>(), create: Some(create), destroy: Some(destroy), read: Some(read), write: Some(write),
};
/// # Safety
/// The host pins this descriptor and its code for the lifetime of every owner.
pub unsafe fn guard_verdict_factory() -> *const FactoryV1 { &FACTORY }

#[cfg(test)]
mod tests {
    use super::*;
    unsafe fn state(context: *const c_void) -> StateV1 {
        let mut out = StateV1 { epoch: 0, verified: 99 };
        unsafe { read(context, &raw mut out) };
        out
    }
    #[test]
    fn initial_state_all_epochs_and_boolean_assignments() {
        unsafe {
            let context = create();assert!(!context.is_null());
            assert_eq!(state(context), StateV1 { epoch: u64::MAX, verified: 0 });
            for epoch in [0, 1, u64::MAX, 19, u64::MAX - 1, 0] {
                for flag in [0, 1, -1, 17, i32::MIN, i32::MAX, 0] {
                    for _ in 0..2 {
                        write(context, epoch, flag);
                        assert_eq!(state(context), StateV1 { epoch, verified: u32::from(flag != 0) });
                    }
                }
            }
            destroy(context);
        }
    }
    #[test]
    fn distinct_moved_owners_and_table_clearing_are_independent() {
        unsafe {
            let mut owners = vec![create(), create()];assert!(owners[0] != owners[1]);
            write(owners[0], 19, 1);write(owners[1], 19, 0);
            let moved = owners.remove(0);
            let table = crate::create();crate::clear(table);crate::destroy(table);
            assert_eq!(state(moved), StateV1 { epoch: 19, verified: 1 });
            assert_eq!(state(owners[0]), StateV1 { epoch: 19, verified: 0 });
            destroy(moved);
            write(owners[0], u64::MAX, -1);
            assert_eq!(state(owners[0]), StateV1 { epoch: u64::MAX, verified: 1 });
            destroy(owners[0]);
        }
    }
}
