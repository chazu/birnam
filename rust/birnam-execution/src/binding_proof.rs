//! Admit capture borrowing through current Id, using already-owned selectors.
//! Expected entries are equality witnesses, never saved executable calls.
use super::*;

pub(super) unsafe fn matches_all(
    s: &Execution,
    expected: &[(Oop, &'static [u8], NativeEntry)],
) -> Result<bool, Outcome> {
    // Current capture/helper callers have at most three target bindings.
    if expected.len() > 3 { return Err(invalid()); }
    let empty = (ptr::null_mut(), ptr::null_mut(), NativeEntry { method: None, context: ptr::null_mut() });
    let mut rows = [empty; 3];
    for (row, &(receiver, name, entry)) in rows.iter_mut().zip(expected) {
        *row = (receiver, s.names[name], entry);
    }
    unsafe { s.binding_guard.admits(s.id, &rows[..expected.len()]) }
}
