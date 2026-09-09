//! Design 74 E1: development-only guarded Machine slot access.
//!
//! These exports exist only under the `dev-guarded-slots` feature. Generated
//! code selected for the experiment boxes the slot index exactly as the
//! baseline emission does, calls one of these, and on a miss performs the
//! ordinary `oopAt:` / `oopAt:put:` send with the same operands exactly once.
//! On a hit the answer is what the canonical Method would have answered
//! (Design 74A §1.3, §4). Nothing here is part of the frozen IdApi or of any
//! admitted artifact's runtime ABI hash.

use crate::abi::Oop;
use crate::runtime::with_active_if_present;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static ATTEMPTS: AtomicU64 = AtomicU64::new(0);
static HITS: AtomicU64 = AtomicU64::new(0);
static COUNTING: AtomicBool = AtomicBool::new(false);
static COUNTING_DECIDED: AtomicBool = AtomicBool::new(false);

unsafe extern "C" {
    fn atexit(callback: extern "C" fn()) -> core::ffi::c_int;
}

extern "C" fn report_counters() {
    let attempts = ATTEMPTS.load(Ordering::Relaxed);
    let hits = HITS.load(Ordering::Relaxed);
    eprintln!(
        "birnam-dev-guarded-slots attempts={attempts} hits={hits} misses={}",
        attempts.saturating_sub(hits)
    );
}

/// Counting is an instrumented mode selected by `BIRNAM_DEV_GUARDED_SLOTS_COUNT=1`
/// and reported once at process exit; timing runs leave it off (Design 74 §7.3).
fn counting() -> bool {
    if !COUNTING_DECIDED.load(Ordering::Relaxed) {
        let enabled = std::env::var_os("BIRNAM_DEV_GUARDED_SLOTS_COUNT")
            .is_some_and(|value| value == "1");
        if enabled && !COUNTING.swap(true, Ordering::Relaxed) {
            // SAFETY: registering a plain extern "C" callback with libc.
            unsafe { atexit(report_counters) };
        }
        COUNTING_DECIDED.store(true, Ordering::Relaxed);
    }
    COUNTING.load(Ordering::Relaxed)
}

fn record(hit: bool) {
    if counting() {
        ATTEMPTS.fetch_add(1, Ordering::Relaxed);
        if hit {
            HITS.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Answer 1 and write the slot value to `out` on a guard hit; answer 0 and
/// leave `out` untouched when the receiver is ineligible.
///
/// # Safety
///
/// `out` must be a valid pointer for one Oop write. `machine` and `operand`
/// are live values copied from generated code's rooted locals.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_dev_frame_oop_at_v1(
    machine: Oop,
    operand: Oop,
    out: *mut Oop,
) -> u32 {
    if out.is_null() {
        return 0;
    }
    let answer = with_active_if_present(|runtime| runtime.guarded_frame_oop_at(machine, operand))
        .flatten();
    match answer {
        Some(value) => {
            record(true);
            // SAFETY: the caller supplies a writable Oop slot.
            unsafe { out.write(value) };
            1
        }
        None => {
            record(false);
            0
        }
    }
}

/// Write form of `birnam_dev_frame_oop_at_v1`; on a hit `out` receives the
/// stored value, as `oopAt:put:` answers it.
///
/// # Safety
///
/// Same contract as `birnam_dev_frame_oop_at_v1`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_dev_frame_oop_at_put_v1(
    machine: Oop,
    operand: Oop,
    value: Oop,
    out: *mut Oop,
) -> u32 {
    if out.is_null() {
        return 0;
    }
    let answer = with_active_if_present(|runtime| {
        runtime.guarded_frame_oop_at_put(machine, operand, value)
    })
    .flatten();
    match answer {
        Some(stored) => {
            record(true);
            // SAFETY: the caller supplies a writable Oop slot.
            unsafe { out.write(stored) };
            1
        }
        None => {
            record(false);
            0
        }
    }
}
