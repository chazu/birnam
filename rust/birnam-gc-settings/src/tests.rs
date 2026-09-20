use super::*;
use core::cell::Cell;

#[derive(Default)]
struct Admission {
    calls: Cell<usize>,
    reenter: Cell<bool>,
    api: Cell<*mut ApiV1>,
}
unsafe extern "C" fn admit(context: *mut c_void) {
    let state = unsafe { &*context.cast::<Admission>() };
    state.calls.set(state.calls.get() + 1);
    if state.reenter.replace(false) {
        unsafe {
            birnam_abi::gc_settings::set_auto(state.api.get(), 0);
            birnam_abi::gc_settings::set_precise(state.api.get(), -7);
        }
    }
}
fn floor(state: &Admission) -> FloorV1 {
    FloorV1 { version:1, byte_size:size_of::<FloorV1>(),
        context:ptr::from_ref(state).cast_mut().cast(), admit:Some(admit) }
}

#[test]
fn factory_creation_is_inert_and_copies_admission() {
    let state=Admission::default();
    unsafe {
        assert!(create(ptr::null()).is_null());
        let mut selected=floor(&state);
        selected.version=2; assert!(create(&selected).is_null());
        selected=floor(&state); selected.byte_size-=1; assert!(create(&selected).is_null());
        selected=floor(&state); selected.admit=None; assert!(create(&selected).is_null());
        selected=floor(&state);
        let api=create(&selected); assert!(!api.is_null());
        assert_eq!(state.calls.get(),0);
        selected.context=ptr::null_mut(); selected.admit=None;
        assert!(selected.admit.is_none());
        birnam_abi::gc_settings::set_auto(api,0);
        birnam_abi::gc_settings::set_precise(api,-1);
        assert_eq!(state.calls.get(),2);
        assert_eq!(automatic_due(api,1,usize::MAX),0);
        assert_eq!(precise(api),1);
        destroy(api);
        assert_eq!(state.calls.get(),2);
    }
}

#[test]
fn defaults_threshold_stack_readiness_and_nonzero_conversion_are_exact() {
    let state=Admission::default();
    unsafe {
        let api=create(&floor(&state));
        assert_eq!(precise(api),0);
        for ready in [0,1,-1,c_int::MIN,c_int::MAX] {
            for bytes in [0,AUTO_COLLECTION_THRESHOLD-1,AUTO_COLLECTION_THRESHOLD,AUTO_COLLECTION_THRESHOLD+1,usize::MAX] {
                assert_eq!(automatic_due(api,ready,bytes),c_int::from(ready!=0 && bytes>=AUTO_COLLECTION_THRESHOLD));
            }
        }
        for enabled in [0,1,-1,c_int::MIN,c_int::MAX,0] {
            set_auto(api,enabled); set_precise(api,enabled);
            assert_eq!(automatic_due(api,1,AUTO_COLLECTION_THRESHOLD),c_int::from(enabled!=0));
            assert_eq!(precise(api),c_int::from(enabled!=0));
            assert_eq!(automatic_due(api,0,usize::MAX),0);
        }
        // Re-enabling uses the same caller-supplied accumulated allocation count.
        set_auto(api,1);
        assert_eq!(automatic_due(api,1,AUTO_COLLECTION_THRESHOLD),1);
        assert_eq!(state.calls.get(),0);
        destroy(api);
    }
}

#[test]
fn admission_reentry_finishes_before_outer_assignment() {
    let state=Admission::default();
    unsafe {
        let api=create(&floor(&state)); state.api.set(api);
        state.reenter.set(true);
        birnam_abi::gc_settings::set_auto(api,1);
        assert_eq!(state.calls.get(),3);
        assert_eq!(automatic_due(api,1,AUTO_COLLECTION_THRESHOLD),1);
        assert_eq!(precise(api),1);
        state.reenter.set(true);
        birnam_abi::gc_settings::set_precise(api,0);
        assert_eq!(state.calls.get(),6);
        assert_eq!(automatic_due(api,1,AUTO_COLLECTION_THRESHOLD),0);
        assert_eq!(precise(api),0);
        destroy(api);
    }
}

#[test]
fn canonical_queries_setters_and_destruction_ignore_public_instrumentation() {
    let first=Admission::default();
    let second=Admission::default();
    unsafe {
        let a=create(&floor(&first));
        let b=create(&floor(&second));
        (*a).context=ptr::null_mut(); (*a).set_auto=None; (*a).set_precise=None;
        set_auto(a,0); set_precise(a,1);
        assert_eq!(automatic_due(a,1,usize::MAX),0);
        assert_eq!(automatic_due(b,1,usize::MAX),1);
        assert_eq!(precise(a),1); assert_eq!(precise(b),0);
        destroy(a);
        birnam_abi::gc_settings::set_precise(b,1);
        assert_eq!(precise(b),1);
        assert_eq!(first.calls.get(),0); assert_eq!(second.calls.get(),1);
        destroy(b);
    }
}
