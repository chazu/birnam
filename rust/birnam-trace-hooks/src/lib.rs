//! Native trace-hook attachments and bounded inherited selection.
use birnam_abi::trace_hooks::{EntryV1, SlotV1, SourceV1};
use birnam_abi::{Oop, TraceHook};
use core::ffi::{c_int, c_void};
use core::ptr;

unsafe extern "C" fn slot(hook: TraceHook) -> SlotV1 { SlotV1 { hook: Some(hook) } }

unsafe extern "C" fn resolve(source: *const SourceV1, vtable: Oop) -> Option<TraceHook> {
    let source = unsafe { *source };
    let mut current = vtable;
    let mut remaining = source.budget;
    while !current.is_null() && remaining != 0 {
        let mut entry = EntryV1 { parent: ptr::null_mut(), slot: SlotV1::EMPTY };
        if unsafe { (source.read)(source.context, current, &mut entry) } == 0 {
            return None;
        }
        if let Some(hook) = entry.slot.hook { return Some(hook); }
        current = entry.parent;
        remaining -= 1;
    }
    None
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> c_int { 1 }

#[cfg(test)]
mod tests;
