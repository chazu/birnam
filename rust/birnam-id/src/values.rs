//! Generic bootstrap allocation, without value-family representation.
use crate::abi::Oop;
use crate::boundary;
use crate::heap::{Heap, Permanence};
pub(crate) fn allocate_empty(heap: &mut Heap, vtable: Oop, permanence: Permanence) -> Oop {
    let object = heap.allocate(0, permanence);
    boundary::require_invariant(heap.set_vtable(object, vtable), "new empty block was not registered");
    object
}
