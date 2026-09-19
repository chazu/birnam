//! Test-only relocated allocations of baseline Unit objects in this module.
use super::*;
use birnam_native_compiler::relocated_allocation_bytes;

static BYTES: AtomicUsize = AtomicUsize::new(0);

fn change(before: usize, after: usize) {
    BYTES
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
            old.checked_sub(before)?.checked_add(after)
        })
        .expect("baseline TCC allocation accounting overflow or underflow");
}

pub(super) unsafe fn relocate(state: *mut TccState) -> c_int {
    // The Unit caller owns the live state across this synchronous operation.
    let before = unsafe { relocated_allocation_bytes(state) };
    let status = unsafe { tcc_relocate(state) };
    let after = unsafe { relocated_allocation_bytes(state) };
    // Failed relocation may still leave storage owned until payload release.
    change(before, after);
    status
}

pub(super) unsafe fn releasing(state: *mut TccState) {
    change(unsafe { relocated_allocation_bytes(state) }, 0);
}

pub(super) unsafe extern "C" fn allocation_bytes(_: Oop, _: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref(RUNTIME.get().expect("TCC runtime was not initialized")) };
    if count != 0 {
        return api.nilObj;
    }
    let Ok(value) = i64::try_from(BYTES.load(Ordering::Relaxed)) else {
        return api.nilObj;
    };
    unsafe { (birnam_abi::integer::make)(value) }
}

#[cfg(feature = "allocation-metrics")]
pub(super) unsafe extern "C" fn heap_allocation_bytes(_: Oop, _: *const Oop, count: usize) -> Oop {
    unsafe { heap_total(count, false) }
}

#[cfg(feature = "allocation-metrics")]
pub(super) unsafe extern "C" fn heap_allocation_count(_: Oop, _: *const Oop, count: usize) -> Oop {
    unsafe { heap_total(count, true) }
}

#[cfg(feature = "allocation-metrics")]
unsafe fn heap_total(count: usize, blocks: bool) -> Oop {
    let api = unsafe { api_ref(RUNTIME.get().expect("TCC runtime was not initialized")) };
    if count != 0 {
        return api.nilObj;
    }
    // Baseline module entry occurs at a quiescent compiler boundary. Snapshot
    // first: boxing the diagnostic result may itself invoke allocation/GC.
    let totals = birnam_native_compiler::heap_allocation_totals();
    let value = if blocks {
        totals.blocks
    } else {
        totals.requested_bytes
    };
    let Ok(value) = i64::try_from(value) else {
        return api.nilObj;
    };
    unsafe { (birnam_abi::integer::make)(value) }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Owned(Box<UnitPayload>);
    impl Owned {
        fn new(source: &CStr) -> Self {
            let state = unsafe { tcc_new() };
            assert!(!state.is_null());
            unsafe {
                tcc_set_error_func(state, ptr::null_mut(), Some(error_silent));
                assert_eq!(tcc_set_output_type(state, TCC_OUTPUT_MEMORY), 0);
            }
            let owner = Self(Box::new(UnitPayload {
                state,
                phase: UnitPhase::Created,
                code_generation_identity: 0,
            }));
            LIVE_UNIT_STATES.fetch_add(1, Ordering::AcqRel);
            assert_eq!(unsafe { tcc_compile_string(state, source.as_ptr()) }, 0);
            owner
        }
        fn state(&self) -> *mut TccState {
            self.0.state
        }
    }
    impl Drop for Owned {
        fn drop(&mut self) {
            unsafe { release_payload(&mut *self.0) };
        }
    }

    #[test]
    fn baseline_unit_bytes_follow_real_relocation_failure_and_release() {
        let _lock = VALIDATION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        #[cfg(feature = "allocation-metrics")]
        let heap_before = birnam_native_compiler::heap_allocation_totals();
        let baseline = BYTES.load(Ordering::Relaxed);
        let units = LIVE_UNIT_STATES.load(Ordering::Acquire);
        let mut small = Owned::new(c"int answer(void) { return 41; }");
        #[cfg(feature = "allocation-metrics")]
        assert!(
            birnam_native_compiler::heap_allocation_totals().requested_bytes
                > heap_before.requested_bytes
        );
        assert_eq!(BYTES.load(Ordering::Relaxed), baseline);
        assert_eq!(unsafe { relocate(small.state()) }, 0);
        let small_bytes = unsafe { relocated_allocation_bytes(small.state()) };
        assert!(small_bytes > 0);
        assert_eq!(BYTES.load(Ordering::Relaxed), baseline + small_bytes);
        let symbol = unsafe { tcc_get_symbol(small.state(), c"answer".as_ptr()) };
        assert!(!symbol.is_null());
        let answer: unsafe extern "C" fn() -> c_int = unsafe { core::mem::transmute(symbol) };
        assert_eq!(unsafe { answer() }, 41);
        let invalid = Owned::new(c"");
        assert!(unsafe { tcc_compile_string(invalid.state(), c"this is not C !!!".as_ptr()) } < 0);
        assert_eq!(BYTES.load(Ordering::Relaxed), baseline + small_bytes);
        drop(invalid);
        let large = Owned::new(c"volatile unsigned char data[65536]; int answer(void) { data[65535] = 7; return data[65535]; }");
        assert_eq!(unsafe { relocate(large.state()) }, 0);
        let large_bytes = unsafe { relocated_allocation_bytes(large.state()) };
        assert!(large_bytes > small_bytes);
        let symbol = unsafe { tcc_get_symbol(large.state(), c"answer".as_ptr()) };
        assert!(!symbol.is_null());
        let answer: unsafe extern "C" fn() -> c_int = unsafe { core::mem::transmute(symbol) };
        assert_eq!(unsafe { answer() }, 7);
        assert_eq!(
            BYTES.load(Ordering::Relaxed),
            baseline + small_bytes + large_bytes
        );
        let failed =
            Owned::new(c"extern int missing(void); int answer(void) { return missing(); }");
        assert!(unsafe { relocate(failed.state()) } < 0);
        let failed_bytes = unsafe { relocated_allocation_bytes(failed.state()) };
        assert_eq!(
            BYTES.load(Ordering::Relaxed),
            baseline + small_bytes + large_bytes + failed_bytes
        );
        drop(failed);
        drop(large);
        assert_eq!(BYTES.load(Ordering::Relaxed), baseline + small_bytes);
        unsafe { release_payload(&mut *small.0) };
        assert_eq!(BYTES.load(Ordering::Relaxed), baseline);
        drop(small); // The central release is idempotent.
        assert_eq!(BYTES.load(Ordering::Relaxed), baseline);
        assert_eq!(LIVE_UNIT_STATES.load(Ordering::Acquire), units);
        #[cfg(feature = "allocation-metrics")]
        assert_eq!(
            birnam_native_compiler::heap_allocation_totals(),
            heap_before
        );
    }
}
