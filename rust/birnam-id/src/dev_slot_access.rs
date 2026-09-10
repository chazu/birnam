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
    use std::io::Write;
    let attempts = ATTEMPTS.load(Ordering::Relaxed);
    let hits = HITS.load(Ordering::Relaxed);
    let line = format!(
        "birnam-dev-guarded-slots attempts={attempts} hits={hits} misses={}\n",
        attempts.saturating_sub(hits)
    );
    // "1" reports on stderr; any other value names a file the line is appended
    // to, so multi-process harnesses that own stderr can still be counted.
    match std::env::var_os("BIRNAM_DEV_GUARDED_SLOTS_COUNT") {
        Some(target) if target != "1" => {
            if let Ok(mut file) = std::fs::OpenOptions::new().append(true).create(true).open(target) {
                let _ = file.write_all(line.as_bytes());
            }
        }
        _ => eprint!("{line}"),
    }
}

/// Counting is an instrumented mode selected by `BIRNAM_DEV_GUARDED_SLOTS_COUNT`
/// (`1` for stderr, otherwise a file path to append to) and reported once at
/// process exit; timing runs leave it off (Design 74 §7.3).
fn counting() -> bool {
    if !COUNTING_DECIDED.load(Ordering::Relaxed) {
        let enabled = std::env::var_os("BIRNAM_DEV_GUARDED_SLOTS_COUNT")
            .is_some_and(|value| !value.is_empty());
        if enabled && !COUNTING.swap(true, Ordering::Relaxed) {
            // SAFETY: registering plain extern "C" callbacks with libc.
            unsafe {
                atexit(report_counters);
                atexit(report_box_counter);
            }
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

// ---------------------------------------------------------------------------
// Design 74D (E4): unboxed slot index. These entry points always answer: the
// direct result on a guard hit, and on a miss the ordinary send's result, with
// the index boxed inside the floor immediately before that send (74D §2.2,
// §2.4). Generated code selected for E4 therefore never allocates for a slot
// index on the hit path and never allocates after producing a store's value.

static UNBOXED: AtomicU64 = AtomicU64::new(0);
static BOXES: AtomicU64 = AtomicU64::new(0);

/// One counting check per access, as the E1 entry points pay: attempts and
/// hits share the E1 counters, `unboxed` proves the unboxed entry points ran,
/// and `boxes` counts miss-path boxes created inside the floor.
fn record_unboxed(hit: bool) {
    if counting() {
        ATTEMPTS.fetch_add(1, Ordering::Relaxed);
        UNBOXED.fetch_add(1, Ordering::Relaxed);
        if hit {
            HITS.fetch_add(1, Ordering::Relaxed);
        } else {
            BOXES.fetch_add(1, Ordering::Relaxed);
        }
    }
}

extern "C" fn report_box_counter() {
    use std::io::Write;
    let unboxed = UNBOXED.load(Ordering::Relaxed);
    let boxes = BOXES.load(Ordering::Relaxed);
    let line = format!("birnam-dev-elided-slots unboxed={unboxed} boxes={boxes}\n");
    match std::env::var_os("BIRNAM_DEV_GUARDED_SLOTS_COUNT") {
        Some(target) if target != "1" => {
            if let Ok(mut file) = std::fs::OpenOptions::new().append(true).create(true).open(target) {
                let _ = file.write_all(line.as_bytes());
            }
        }
        _ => eprint!("{line}"),
    }
}

/// Read the current Frame slot `index` of `machine`: the guarded direct value
/// on a hit, otherwise the ordinary `oopAt:` send's answer.
///
/// # Safety
///
/// `machine` is a live value copied from generated code's rooted locals and
/// the call is made inside an active owner-thread runtime, as every
/// StepMethod call is.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_dev_frame_slot_get_v1(machine: Oop, index: core::ffi::c_long) -> Oop {
    crate::runtime::with_active(|runtime| match runtime.guarded_frame_slot_at(machine, index as i64) {
        Some(value) => {
            record_unboxed(true);
            value
        }
        None => {
            record_unboxed(false);
            runtime.fallback_frame_oop_at(machine, index as i64)
        }
    })
}

/// Write form of `birnam_dev_frame_slot_get_v1`; answers the stored value on a
/// hit and the ordinary `oopAt:put:` send's answer on a miss.
///
/// # Safety
///
/// Same contract as `birnam_dev_frame_slot_get_v1`; `value` is the store's
/// freshly produced value, which the miss path roots before allocating.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_dev_frame_slot_put_v1(
    machine: Oop,
    index: core::ffi::c_long,
    value: Oop,
) -> Oop {
    crate::runtime::with_active(|runtime| {
        match runtime.guarded_frame_slot_at_put(machine, index as i64, value) {
            Some(stored) => {
                record_unboxed(true);
                stored
            }
            None => {
                record_unboxed(false);
                runtime.fallback_frame_oop_at_put(machine, index as i64, value)
            }
        }
    })
}
