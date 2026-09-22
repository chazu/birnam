//! Per-Runtime observation scopes; no Id or description-provider callbacks.
use birnam_abi::storage_observation::{StateV1};
use core::{
    cell::Cell,
    ffi::{c_int, c_void},
};

unsafe extern "C" fn create() -> *mut c_void {
    Box::into_raw(Box::new(Cell::new(StateV1::default()))).cast()
}
unsafe extern "C" fn destroy(context: *mut c_void) {
    unsafe { drop(Box::from_raw(context.cast::<Cell<StateV1>>())) };
}
unsafe fn owner<'a>(context: *const c_void) -> &'a Cell<StateV1> {
    unsafe { &*context.cast::<Cell<StateV1>>() }
}
unsafe extern "C" fn read(context: *const c_void, out: *mut StateV1) {
    unsafe { out.write(owner(context).get()) };
}
unsafe extern "C" fn set_active(context: *mut c_void, enabled: c_int) {
    let owner = unsafe { owner(context) };
    let mut state = owner.get();
    state.active = u32::from(enabled != 0);
    owner.set(state);
}
unsafe extern "C" fn set_permit(context: *mut c_void, enabled: c_int) {
    let owner = unsafe { owner(context) };
    let mut state = owner.get();
    state.permit = u32::from(enabled != 0);
    owner.set(state);
}
unsafe extern "C" fn advance_epoch(context: *mut c_void, out: *mut u64) -> c_int {
    let owner = unsafe { owner(context) };
    let mut state = owner.get();
    let Some(epoch) = state.epoch.checked_add(1) else {
        return 0;
    };
    state.epoch = epoch;
    owner.set(state);
    unsafe { out.write(epoch) };
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_owners_and_flag_assignments_preserve_other_fields() {
        unsafe {
            let a = create();
            let b = create();
            assert!(a != b && !a.is_null() && !b.is_null());
            let mut state = StateV1::default();
            read(a, &raw mut state);
            assert_eq!(state, StateV1::default());
            let mut epoch = 99;
            assert_eq!(advance_epoch(a, &raw mut epoch), 1);
            assert_eq!(epoch, 1);
            for active in [0, 1, -1, 17, 0] {
                for permit in [0, 1, -1, 17, 0] {
                    set_active(a, active);
                    set_permit(a, permit);
                    read(a, &raw mut state);
                    assert_eq!(
                        state,
                        StateV1 {
                            epoch: 1,
                            active: u32::from(active != 0),
                            permit: u32::from(permit != 0)
                        }
                    );
                    read(b, &raw mut state);
                    assert_eq!(state, StateV1::default());
                }
            }
            destroy(a);
            assert_eq!(advance_epoch(b, &raw mut epoch), 1);
            assert_eq!(epoch, 1);
            destroy(b);
        }
    }
    #[test]
    fn epoch_overflow_changes_neither_state_nor_output() {
        unsafe {
            let context = create();
            for active in [0, 1] {
                for permit in [0, 1] {
                    owner(context).set(StateV1 {
                        epoch: u64::MAX - 1,
                        active,
                        permit,
                    });
                    let mut out = 19;
                    assert_eq!(advance_epoch(context, &raw mut out), 1);
                    assert_eq!(out, u64::MAX);
                    let expected = StateV1 {
                        epoch: u64::MAX,
                        active,
                        permit,
                    };
                    assert_eq!(owner(context).get(), expected);
                    out = 23;
                    assert_eq!(advance_epoch(context, &raw mut out), 0);
                    assert_eq!(out, 23);
                    assert_eq!(owner(context).get(), expected);
                }
            }
            destroy(context);
        }
    }
}
