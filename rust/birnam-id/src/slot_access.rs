//! Guarded Frame slot access for generated StepMethods (Designs 74A–74E).
//!
//! Generated code reads and writes its Frame slots through these two entry
//! points with the slot index as a machine integer. On a guard hit (the
//! receiver's vtable is the canonical Machine vtable, that vtable's direct
//! `oopAt:` / `oopAt:put:` entries are the floor Methods, and no custom
//! lookup can intervene) the floor answers directly from the current Frame
//! without allocating or dispatching. On a miss the floor performs the
//! ordinary late-bound send once, with exactly the operands generated code
//! used to produce itself: the boxed index, the canonical selector, the value.
//!
//! These are runtime shims named in the runtime ABI identity
//! (`birnam-image`'s `RUNTIME_SHIM_NAMES`), not IdApi fields.

use crate::abi::Oop;

/// Read the current Frame slot `index` of `machine`: the guarded direct value
/// on a hit, otherwise the ordinary `oopAt:` send's answer.
///
/// # Safety
///
/// `machine` is a live value copied from generated code's rooted locals and
/// the call is made inside an active owner-thread runtime, as every
/// StepMethod call is.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_frame_slot_get_v1(machine: Oop, index: core::ffi::c_long) -> Oop {
    crate::runtime::with_active(|runtime| {
        runtime
            .guarded_frame_slot_at(machine, index as i64)
            .unwrap_or_else(|| runtime.fallback_frame_oop_at(machine, index as i64))
    })
}

/// Write form of `birnam_frame_slot_get_v1`; answers the stored value on a
/// hit and the ordinary `oopAt:put:` send's answer on a miss.
///
/// # Safety
///
/// Same contract as `birnam_frame_slot_get_v1`; `value` is the store's
/// freshly produced value, which the miss path roots before allocating.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_frame_slot_put_v1(
    machine: Oop,
    index: core::ffi::c_long,
    value: Oop,
) -> Oop {
    crate::runtime::with_active(|runtime| {
        runtime
            .guarded_frame_slot_at_put(machine, index as i64, value)
            .unwrap_or_else(|| runtime.fallback_frame_oop_at_put(machine, index as i64, value))
    })
}
