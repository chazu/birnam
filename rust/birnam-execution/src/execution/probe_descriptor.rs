//! Test construction helpers for genuine V2 descriptors. Probes declare edges
//! explicitly and exercise the same validator and transition engine as compiled
//! code. Conservative all-Oop root maps are convenient for hand-written steps.
use super::*;

const UNUSED_STEP: Method = Method::new(unused_step_entry, core::ptr::null_mut());
unsafe extern "C" fn unused_step_entry(
    _: *mut core::ffi::c_void,
    _: Oop,
    _: *const birnam_abi::id_protocol::Operand,
    _: usize,
) -> birnam_abi::id_protocol::Outcome {
    birnam_abi::id_protocol::Outcome::loaded(birnam_abi::id_protocol::Operand::unit())
}

pub(crate) fn probe_spec(slot_count: usize, pc_count: usize) -> FrameDescriptorV2Spec {
    let mut roots = vec![u64::MAX; slot_count.div_ceil(64)];
    if !slot_count.is_multiple_of(64) {
        *roots.last_mut().unwrap() = (1u64 << (slot_count % 64)) - 1;
    }
    FrameDescriptorV2Spec {
        identity: DescriptorIdentity {
            implementation: 0,
            generation: 0,
            source_map: 0,
        },
        program_counter_count: pc_count,
        driver_argument_count: usize::from(pc_count > 1),
        slot_count,
        entry_receiver_slot: 0,
        entry_argument_start: 1,
        entry_argument_count: 0,
        entry_capture_slots: vec![],
        implementation_slot: None,
        root_bitmap_words: roots.repeat(pc_count),
        call_sites: vec![],
        edges: vec![],
        step_method: UNUSED_STEP,
    }
}

pub(crate) fn probe_continue(source: usize, target: usize, slots: usize) -> FrameEdgeSpec {
    FrameEdgeSpec {
        kind: EdgeTransitionKind::Continue,
        source_pc: source,
        target_pc: target,
        call_site_index: None,
        result_slot: None,
        copies: vec![],
        clear_bitmap_words: vec![0; slots.div_ceil(64)],
    }
}

pub(crate) fn probe_call(
    source: usize,
    target: usize,
    site: usize,
    result: usize,
    slots: usize,
) -> FrameEdgeSpec {
    FrameEdgeSpec {
        kind: EdgeTransitionKind::Call,
        call_site_index: Some(site),
        result_slot: Some(result),
        ..probe_continue(source, target, slots)
    }
}

/// A hand-written control operation has the same result-bearing edge as a
/// compiled Call. It yields that invocation instead of executing its receiver.
pub(crate) fn probe_control(result: usize, target: usize) -> CallSiteDescriptor {
    CallSiteDescriptor {
        kind: CallSiteKind::Call,
        receiver_slot: result,
        selector_slot: result,
        argument_start: 0,
        argument_count: 0,
        result_slot: Some(result),
        continuation_pc: Some(target),
    }
}
