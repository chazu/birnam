//! Generic bootstrap allocation, without value-family representation.
use crate::abi::Oop;
use crate::heap::{Heap, Permanence};
pub(crate) fn allocate_empty(heap: &mut Heap, vtable: Oop, permanence: Permanence) -> Oop {
    heap.allocate_with_behavior(0, permanence, vtable)
}
