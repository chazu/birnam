//! Canonical floor value payload shapes used by bootstrap.
#![allow(dead_code)] // R2.5 creates identities; R3.2 exposes value behavior.

use core::mem::offset_of;

use crate::abi::Oop;
use crate::boundary;
use crate::heap::{Heap, Permanence};
use crate::raw::AddressMap;

#[repr(C)]
struct RawInteger {
    value: i64,
}

#[repr(C)]
struct RawFloat {
    value: f64,
}

#[repr(C)]
struct RawWord {
    value: u64,
}

#[repr(C)]
struct RawString {
    pointer: *const u8,
    length: usize,
}

const _: () = {
    assert!(size_of::<RawInteger>() == 8);
    assert!(align_of::<RawInteger>() == 8);
    assert!(size_of::<RawFloat>() == 8);
    assert!(align_of::<RawFloat>() == 8);
    assert!(size_of::<RawWord>() == 8);
    assert!(align_of::<RawWord>() == 8);
    assert!(size_of::<RawString>() == 16);
    assert!(align_of::<RawString>() == 8);
    assert!(offset_of!(RawString, pointer) == 0);
    assert!(offset_of!(RawString, length) == 8);
};

/// Owns non-id allocations referenced by value payloads.
pub(crate) struct Values {
    string_bytes: AddressMap<Box<[u8]>>,
}

impl Values {
    pub(crate) fn new() -> Self {
        Self {
            string_bytes: AddressMap::default(),
        }
    }

    pub(crate) fn allocate_empty(
        &mut self,
        heap: &mut Heap,
        vtable: Oop,
        permanence: Permanence,
    ) -> Oop {
        allocate_payload(heap, vtable, 0, permanence)
    }

    pub(crate) fn allocate_integer(&mut self, heap: &mut Heap, vtable: Oop, value: i64) -> Oop {
        let object = allocate_payload(
            heap,
            vtable,
            size_of::<RawInteger>(),
            Permanence::Collectable,
        );
        // SAFETY: object owns an aligned payload of RawInteger's exact size.
        unsafe { object.cast::<RawInteger>().write(RawInteger { value }) };
        object
    }

    pub(crate) fn allocate_float(&mut self, heap: &mut Heap, vtable: Oop, value: f64) -> Oop {
        let object = allocate_payload(heap, vtable, size_of::<RawFloat>(), Permanence::Collectable);
        // SAFETY: object owns an aligned payload of RawFloat's exact size.
        unsafe { object.cast::<RawFloat>().write(RawFloat { value }) };
        object
    }

    pub(crate) fn allocate_word(&mut self, heap: &mut Heap, vtable: Oop, value: u64) -> Oop {
        let object = allocate_payload(heap, vtable, size_of::<RawWord>(), Permanence::Collectable);
        // SAFETY: object owns an aligned payload of RawWord's exact size.
        unsafe { object.cast::<RawWord>().write(RawWord { value }) };
        object
    }

    pub(crate) fn allocate_string(&mut self, heap: &mut Heap, vtable: Oop, bytes: &[u8]) -> Oop {
        let mut owned_bytes = bytes.to_vec().into_boxed_slice();
        let pointer = owned_bytes.as_mut_ptr().cast_const();
        let object = allocate_payload(
            heap,
            vtable,
            size_of::<RawString>(),
            Permanence::Collectable,
        );
        // SAFETY: object owns an aligned payload of RawString's exact size, and
        // the address-keyed owner below retains the byte allocation.
        unsafe {
            object.cast::<RawString>().write(RawString {
                pointer,
                length: bytes.len(),
            })
        };
        let previous = self.string_bytes.insert(object.addr(), owned_bytes);
        boundary::require_invariant(previous.is_none(), "duplicate String object address");
        object
    }

    pub(crate) fn integer_value(&self, object: Oop) -> i64 {
        // SAFETY: internal caller supplies an Integer object with this payload.
        unsafe { integer_value_unchecked(object) }
    }

    pub(crate) fn float_value(&self, object: Oop) -> f64 {
        // SAFETY: internal caller supplies a Float object with this payload.
        unsafe { float_value_unchecked(object) }
    }

    pub(crate) fn word_value(&self, object: Oop) -> u64 {
        // SAFETY: internal caller supplies a Word object with this payload.
        unsafe { word_value_unchecked(object) }
    }

    pub(crate) fn string_bytes(&self, object: Oop) -> Option<&[u8]> {
        self.string_bytes.get(&object.addr()).map(Box::as_ref)
    }

    pub(crate) fn release_string(&mut self, object: Oop) -> bool {
        self.string_bytes.remove(&object.addr()).is_some()
    }
}

/// Read an Integer payload without consulting runtime-owned state.
///
/// # Safety
///
/// `object` must be a live Integer object whose payload has `RawInteger`'s
/// final ABI shape. The caller must preserve the floor's owner-thread and
/// object-lifetime rules for the duration of the read.
pub(crate) unsafe fn integer_value_unchecked(object: Oop) -> i64 {
    // SAFETY: forwarded caller contract supplies a live, aligned RawInteger.
    unsafe { object.cast::<RawInteger>().read().value }
}

/// Read a Float payload without consulting runtime-owned state.
///
/// # Safety
///
/// `object` must be a live Float object whose payload has `RawFloat`'s final
/// ABI shape. The caller must preserve the floor's owner-thread and
/// object-lifetime rules for the duration of the read.
pub(crate) unsafe fn float_value_unchecked(object: Oop) -> f64 {
    // SAFETY: forwarded caller contract supplies a live, aligned RawFloat.
    unsafe { object.cast::<RawFloat>().read().value }
}

/// Read a Word payload without consulting runtime-owned state.
///
/// # Safety
///
/// `object` must be a live Word object whose payload has `RawWord`'s final ABI
/// shape. The caller must preserve the floor's owner-thread and object-lifetime
/// rules for the duration of the read.
pub(crate) unsafe fn word_value_unchecked(object: Oop) -> u64 {
    // SAFETY: forwarded caller contract supplies a live, aligned RawWord.
    unsafe { object.cast::<RawWord>().read().value }
}

fn allocate_payload(
    heap: &mut Heap,
    vtable: Oop,
    payload_size: usize,
    permanence: Permanence,
) -> Oop {
    let object = heap.allocate(payload_size, permanence);
    boundary::require_invariant(
        heap.set_vtable(object, vtable),
        "new value block was not registered",
    );
    object
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vtable::Vtables;
    use core::ptr::NonNull;

    fn value_harness() -> (Heap, Values, Oop) {
        let mut heap = Heap::new();
        let mut vtables = Vtables::new();
        let meta = vtables.create(&mut heap, core::ptr::null_mut(), core::ptr::null_mut());
        assert!(heap.set_vtable(meta, meta));
        let family = vtables.create(&mut heap, core::ptr::null_mut(), meta);
        (heap, Values::new(), family)
    }

    #[test]
    fn scalar_payloads_have_final_shapes_and_values() {
        let (mut heap, mut values, family) = value_harness();
        let integer = values.allocate_integer(&mut heap, family, -42);
        let float = values.allocate_float(&mut heap, family, 3.25);
        let word = values.allocate_word(&mut heap, family, 0xfedc_ba98_7654_3210);
        assert_eq!(values.integer_value(integer), -42);
        assert_eq!(values.float_value(float), 3.25);
        assert_eq!(values.word_value(word), 0xfedc_ba98_7654_3210);
        // SAFETY: the harness owns live objects of each exact payload type.
        unsafe {
            assert_eq!(integer_value_unchecked(integer), -42);
            assert_eq!(float_value_unchecked(float), 3.25);
            assert_eq!(word_value_unchecked(word), 0xfedc_ba98_7654_3210);
        }
        for object in [integer, float, word] {
            assert_eq!(heap.payload_size(object), Some(8));
            assert_eq!(heap.permanence_of(object), Some(Permanence::Collectable));
            assert_eq!(Vtables::checked_vtable_of(&heap, object), family);
        }
    }

    #[test]
    fn strings_own_stable_arbitrary_bytes_in_a_two_word_payload() {
        let (mut heap, mut values, family) = value_harness();
        let empty = values.allocate_string(&mut heap, family, b"");
        let binary = values.allocate_string(&mut heap, family, &[0, 0xff, b'a']);
        assert_eq!(values.string_bytes(empty), Some(&b""[..]));
        assert_eq!(values.string_bytes(binary), Some(&[0, 0xff, b'a'][..]));
        assert_eq!(heap.payload_size(empty), Some(16));
        assert_eq!(heap.payload_size(binary), Some(16));

        let raw = NonNull::new(binary.cast::<RawString>()).unwrap();
        // SAFETY: binary is a live RawString payload for this test.
        let raw = unsafe { raw.as_ref() };
        assert_eq!(raw.length, 3);
        assert_eq!(raw.pointer, values.string_bytes(binary).unwrap().as_ptr());
        assert!(values.release_string(binary));
        assert_eq!(values.string_bytes(binary), None);
        assert!(!values.release_string(binary));
    }

    #[test]
    fn zero_payload_objects_can_be_permanent_or_collectable() {
        let (mut heap, mut values, family) = value_harness();
        let permanent = values.allocate_empty(&mut heap, family, Permanence::Permanent);
        let collectable = values.allocate_empty(&mut heap, family, Permanence::Collectable);
        assert_eq!(heap.payload_size(permanent), Some(0));
        assert_eq!(heap.payload_size(collectable), Some(0));
        assert_eq!(heap.permanence_of(permanent), Some(Permanence::Permanent));
        assert_eq!(
            heap.permanence_of(collectable),
            Some(Permanence::Collectable)
        );
    }
}
