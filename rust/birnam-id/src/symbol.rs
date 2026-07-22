//! Permanent Symbol objects and byte-oriented interning.
#![allow(dead_code)] // R2.3 precedes bootstrap/export wiring.

use core::ffi::c_char;
use core::mem::offset_of;
use core::ptr::NonNull;

use crate::abi::Oop;
use crate::boundary;
use crate::heap::{Heap, Permanence};
use crate::raw::AddressMap;

#[repr(C)]
struct RawSymbol {
    string: *mut c_char,
}

const _: () = {
    assert!(size_of::<RawSymbol>() == 8);
    assert!(align_of::<RawSymbol>() == 8);
    assert!(offset_of!(RawSymbol, string) == 0);
};

struct SymbolRecord {
    object: Oop,
    nul_terminated_bytes: Box<[u8]>,
}

impl SymbolRecord {
    fn bytes(&self) -> &[u8] {
        &self.nul_terminated_bytes[..self.nul_terminated_bytes.len() - 1]
    }

    fn c_name(&self) -> *const c_char {
        self.nul_terminated_bytes.as_ptr().cast()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct EmbeddedNul;

/// Ordered authority for interned Symbol identities.
pub(crate) struct Symbols {
    ordered: Vec<SymbolRecord>,
    by_address: AddressMap<usize>,
}

impl Symbols {
    pub(crate) fn new() -> Self {
        Self {
            ordered: Vec::new(),
            by_address: AddressMap::default(),
        }
    }

    pub(crate) fn intern(
        &mut self,
        heap: &mut Heap,
        symbol_vtable: Oop,
        bytes: &[u8],
    ) -> Result<Oop, EmbeddedNul> {
        if bytes.contains(&0) {
            return Err(EmbeddedNul);
        }
        if let Some(existing) = self.ordered.iter().find(|record| record.bytes() == bytes) {
            return Ok(existing.object);
        }

        let mut owned_name = Vec::with_capacity(bytes.len() + 1);
        owned_name.extend_from_slice(bytes);
        owned_name.push(0);
        let mut owned_name = owned_name.into_boxed_slice();
        let name_pointer = owned_name.as_mut_ptr().cast::<c_char>();

        let object = heap.allocate(size_of::<RawSymbol>(), Permanence::Permanent);
        boundary::require_invariant(
            heap.set_vtable(object, symbol_vtable),
            "new Symbol block was not registered",
        );
        let raw = NonNull::new(object.cast::<RawSymbol>())
            .unwrap_or_else(|| boundary::fatal_invariant("new Symbol payload is null"));
        // SAFETY: object owns an aligned payload of exactly RawSymbol's size,
        // and name_pointer targets the boxed bytes retained by the record.
        unsafe {
            raw.as_ptr().write(RawSymbol {
                string: name_pointer,
            })
        };

        let index = self.ordered.len();
        self.ordered.push(SymbolRecord {
            object,
            nul_terminated_bytes: owned_name,
        });
        let previous = self.by_address.insert(object.addr(), index);
        boundary::require_invariant(previous.is_none(), "duplicate Symbol object address");
        Ok(object)
    }

    pub(crate) fn count(&self) -> usize {
        self.ordered.len()
    }

    pub(crate) fn at(&self, index: usize) -> Oop {
        self.ordered
            .get(index)
            .map(|record| record.object)
            .unwrap_or(core::ptr::null_mut())
    }

    pub(crate) fn bytes_of(&self, candidate: Oop) -> Option<&[u8]> {
        let index = *self.by_address.get(&candidate.addr())?;
        Some(self.ordered[index].bytes())
    }

    pub(crate) fn c_name_of(&self, candidate: Oop) -> *const c_char {
        let Some(index) = self.by_address.get(&candidate.addr()) else {
            return core::ptr::null();
        };
        self.ordered[*index].c_name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vtable::Vtables;
    use std::ffi::CStr;

    fn symbol_harness() -> (Heap, Oop) {
        let mut heap = Heap::new();
        let mut vtables = Vtables::new();
        let meta = vtables.create(&mut heap, core::ptr::null_mut(), core::ptr::null_mut());
        assert!(heap.set_vtable(meta, meta));
        let symbol_vtable = vtables.create(&mut heap, core::ptr::null_mut(), meta);
        (heap, symbol_vtable)
    }

    #[test]
    fn repeated_equal_bytes_return_one_permanent_identity() {
        let (mut heap, symbol_vtable) = symbol_harness();
        let mut symbols = Symbols::new();
        let first = symbols.intern(&mut heap, symbol_vtable, b"lookup").unwrap();
        let second = symbols.intern(&mut heap, symbol_vtable, b"lookup").unwrap();
        assert_eq!(first, second);
        assert_eq!(symbols.count(), 1);
        assert_eq!(symbols.at(0), first);
        assert!(symbols.at(1).is_null());
        assert_eq!(heap.permanence_of(first), Some(Permanence::Permanent));
        assert_eq!(Vtables::checked_vtable_of(&heap, first), symbol_vtable);
    }

    #[test]
    fn arbitrary_non_nul_bytes_never_require_utf8() {
        let (mut heap, symbol_vtable) = symbol_harness();
        let mut symbols = Symbols::new();
        let cases: [&[u8]; 6] = [
            b"",
            b"ascii",
            b"doesNotUnderstand:",
            b"at:put:",
            b"!@#$%^&*",
            &[0xff, 0xfe, b':'],
        ];
        for bytes in cases {
            let symbol = symbols.intern(&mut heap, symbol_vtable, bytes).unwrap();
            assert_eq!(symbols.bytes_of(symbol), Some(bytes));
            // CStr validates only the terminating NUL; no UTF-8 conversion is
            // attempted for the non-UTF-8 selector.
            let c_name = symbols.c_name_of(symbol);
            // SAFETY: each record retains a stable NUL-terminated boxed name.
            assert_eq!(unsafe { CStr::from_ptr(c_name) }.to_bytes(), bytes);
        }
        assert_eq!(symbols.count(), cases.len());
        assert_eq!(
            symbols.intern(&mut heap, symbol_vtable, b"bad\0name"),
            Err(EmbeddedNul)
        );
    }

    #[test]
    fn enumeration_order_and_c_name_pointers_survive_unrelated_growth() {
        let (mut heap, symbol_vtable) = symbol_harness();
        let mut symbols = Symbols::new();
        let anchor = symbols
            .intern(&mut heap, symbol_vtable, b"anchor:")
            .unwrap();
        let anchor_pointer = symbols.c_name_of(anchor);

        let mut expected = vec![anchor];
        for index in 0..4096u32 {
            let name = format!("symbol-{index}");
            let symbol = symbols
                .intern(&mut heap, symbol_vtable, name.as_bytes())
                .unwrap();
            expected.push(symbol);
            let _ = heap.allocate(index as usize & 31, Permanence::Collectable);
        }

        assert_eq!(symbols.count(), expected.len());
        for (index, expected_symbol) in expected.into_iter().enumerate() {
            assert_eq!(symbols.at(index), expected_symbol);
        }
        assert_eq!(symbols.c_name_of(anchor), anchor_pointer);
        // SAFETY: unrelated Vec/Heap growth cannot move the boxed name bytes.
        assert_eq!(
            unsafe { CStr::from_ptr(anchor_pointer) }.to_bytes(),
            b"anchor:"
        );
    }

    #[test]
    fn foreign_carriers_are_not_symbols() {
        let (mut heap, symbol_vtable) = symbol_harness();
        let mut symbols = Symbols::new();
        let _ = symbols.intern(&mut heap, symbol_vtable, b"real").unwrap();
        let mut foreign = 0usize;
        let foreign: Oop = core::ptr::from_mut(&mut foreign).cast();
        assert_eq!(symbols.bytes_of(core::ptr::null_mut()), None);
        assert_eq!(symbols.bytes_of(foreign), None);
        assert!(symbols.c_name_of(foreign).is_null());
    }
}
