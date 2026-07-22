//! Single-thread-owned process state and short-borrow accessors.
#![allow(dead_code)] // R2 harness precedes public id_init wiring.

use core::cell::{Cell, UnsafeCell};

use crate::abi::{Method, Oop};
use crate::boundary;
use crate::heap::{Heap, Permanence};
use crate::raw::MethodAddress;
use crate::symbol::{EmbeddedNul, Symbols};
use crate::values::Values;
use crate::vtable::{AddMethod, Vtables};

#[derive(Clone, Copy)]
pub(crate) struct DispatchGlobals {
    pub(crate) vtable_vt: Oop,
    pub(crate) object_vt: Oop,
    pub(crate) symbol_vt: Oop,
    pub(crate) lookup: Oop,
    pub(crate) does_not_understand: Oop,
}

/// Identities fixed by the one-way bootstrap transition.
#[derive(Clone, Copy)]
pub(crate) struct CanonicalObjects {
    pub(crate) symbol_list: Oop,
    pub(crate) environment: Oop,
    pub(crate) smallint_vt: Oop,
    pub(crate) string_vt: Oop,
    pub(crate) float_vt: Oop,
    pub(crate) word_vt: Oop,
    pub(crate) nil_vt: Oop,
    pub(crate) bool_vt: Oop,
    pub(crate) nil: Oop,
    pub(crate) true_object: Oop,
    pub(crate) false_object: Oop,
    pub(crate) object_prototype: Oop,
    pub(crate) integer_prototype: Oop,
    pub(crate) float_prototype: Oop,
    pub(crate) word_prototype: Oop,
    pub(crate) string_prototype: Oop,
    pub(crate) symbol_prototype: Oop,
}

struct RuntimeState {
    // Ordered authorities are dropped before the raw heap owner at harness
    // teardown. No destructor dereferences a heap payload.
    symbols: Symbols,
    values: Values,
    vtables: Vtables,
    heap: Heap,
    globals: DispatchGlobals,
    canonical: Option<CanonicalObjects>,
}

/// One runtime owned by the thread that constructed it.
pub(crate) struct Runtime {
    owner: std::thread::ThreadId,
    state: UnsafeCell<RuntimeState>,
}

impl Runtime {
    pub(crate) fn new(
        heap: Heap,
        vtables: Vtables,
        symbols: Symbols,
        globals: DispatchGlobals,
    ) -> Self {
        Self::new_with_values(heap, vtables, symbols, Values::new(), globals)
    }

    pub(crate) fn new_with_values(
        heap: Heap,
        vtables: Vtables,
        symbols: Symbols,
        values: Values,
        globals: DispatchGlobals,
    ) -> Self {
        Self {
            owner: std::thread::current().id(),
            state: UnsafeCell::new(RuntimeState {
                symbols,
                values,
                vtables,
                heap,
                globals,
                canonical: None,
            }),
        }
    }

    fn assert_owner(&self) {
        boundary::require_invariant(
            std::thread::current().id() == self.owner,
            "Rust floor entered from a non-owner thread",
        );
    }

    pub(crate) fn globals(&self) -> DispatchGlobals {
        self.assert_owner();
        // SAFETY: owner-thread serialization; copy ends the shared access here.
        unsafe { (*self.state.get()).globals }
    }

    pub(crate) fn lookup_method(&self, vtable: Oop, selector: Oop) -> Option<MethodAddress> {
        self.assert_owner();
        // SAFETY: owner-thread serialization; MethodAddress is copied out and
        // the collection borrow ends before any caller can invoke it.
        unsafe { (*self.state.get()).vtables.lookup(vtable, selector) }
    }

    pub(crate) fn add_method(&self, vtable: Oop, selector: Oop, method: Method) -> AddMethod {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        unsafe {
            (&mut *self.state.get())
                .vtables
                .add_method(vtable, selector, method)
        }
    }

    pub(crate) fn remove_method(&self, vtable: Oop, selector: Oop) -> Option<MethodAddress> {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        unsafe {
            (&mut *self.state.get())
                .vtables
                .remove_method(vtable, selector)
        }
    }

    pub(crate) fn create_vtable(&self, parent: Oop) -> Oop {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        state
            .vtables
            .create(&mut state.heap, parent, state.globals.vtable_vt)
    }

    pub(crate) fn set_vtable_owner(&self, vtable: Oop, owner: Oop) {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        unsafe { (&mut *self.state.get()).vtables.set_owner(vtable, owner) };
    }

    pub(crate) fn allocate_instance(&self, vtable: Oop, payload_size: usize) -> Oop {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        let object = state.heap.allocate(payload_size, Permanence::Collectable);
        boundary::require_invariant(
            state.heap.set_vtable(object, vtable),
            "new instance block was not registered",
        );
        object
    }

    pub(crate) fn allocate_empty(&self, vtable: Oop, permanence: Permanence) -> Oop {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        state
            .values
            .allocate_empty(&mut state.heap, vtable, permanence)
    }

    pub(crate) fn allocate_integer(&self, vtable: Oop, value: i64) -> Oop {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        state
            .values
            .allocate_integer(&mut state.heap, vtable, value)
    }

    pub(crate) fn allocate_float(&self, vtable: Oop, value: f64) -> Oop {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        state.values.allocate_float(&mut state.heap, vtable, value)
    }

    pub(crate) fn allocate_word(&self, vtable: Oop, value: u64) -> Oop {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        state.values.allocate_word(&mut state.heap, vtable, value)
    }

    pub(crate) fn allocate_string(&self, vtable: Oop, bytes: &[u8]) -> Oop {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        state.values.allocate_string(&mut state.heap, vtable, bytes)
    }

    pub(crate) fn install_canonical(&self, canonical: CanonicalObjects) {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let slot = unsafe { &mut (*self.state.get()).canonical };
        boundary::require_invariant(slot.is_none(), "canonical objects installed twice");
        *slot = Some(canonical);
    }

    pub(crate) fn canonical(&self) -> CanonicalObjects {
        self.assert_owner();
        // SAFETY: owner-thread serialization; the identity set is copied out.
        unsafe { (*self.state.get()).canonical }
            .unwrap_or_else(|| boundary::fatal_invariant("canonical objects not installed"))
    }

    pub(crate) fn intern(&self, bytes: &[u8]) -> Result<Oop, EmbeddedNul> {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        let symbol_vtable = state.globals.symbol_vt;
        state.symbols.intern(&mut state.heap, symbol_vtable, bytes)
    }

    pub(crate) fn vtable_for_valid_receiver(&self, receiver: Oop) -> Oop {
        self.assert_owner();
        // SAFETY: dispatch callers guarantee a live floor receiver. No runtime
        // collection borrow is needed for the header fast path.
        unsafe { Vtables::dispatch_vtable_of(receiver) }
    }

    pub(crate) fn enter<R>(&self, operation: impl FnOnce() -> R) -> R {
        self.assert_owner();
        ACTIVE_RUNTIME.with(|active| {
            let current = active.replace(self as *const Runtime);
            boundary::require_invariant(
                current.is_null() || core::ptr::eq(current, self),
                "two Rust floor runtimes entered on one thread",
            );
            let guard = ActiveRuntimeGuard {
                active,
                previous: current,
            };
            let result = operation();
            drop(guard);
            result
        })
    }

    #[cfg(test)]
    pub(crate) fn symbol_count(&self) -> usize {
        // SAFETY: test calls remain owner-thread and copy only the count.
        unsafe { (*self.state.get()).symbols.count() }
    }

    #[cfg(test)]
    pub(crate) fn symbol_at(&self, index: usize) -> Oop {
        // SAFETY: test calls remain owner-thread and copy only an identity.
        unsafe { (*self.state.get()).symbols.at(index) }
    }

    #[cfg(test)]
    pub(crate) fn symbol_bytes(&self, symbol: Oop) -> Option<&[u8]> {
        // SAFETY: test calls remain owner-thread; returned bytes are inspected
        // synchronously without mutation or foreign calls.
        unsafe { (*self.state.get()).symbols.bytes_of(symbol) }
    }

    #[cfg(test)]
    pub(crate) fn vtable_count(&self) -> usize {
        // SAFETY: test calls remain owner-thread and copy only the count.
        unsafe { (*self.state.get()).vtables.count() }
    }

    #[cfg(test)]
    pub(crate) fn vtable_at(&self, index: usize) -> Oop {
        // SAFETY: test calls remain owner-thread and copy only an identity.
        unsafe { (*self.state.get()).vtables.at(index) }
    }

    #[cfg(test)]
    pub(crate) fn vtable_parent(&self, vtable: Oop) -> Oop {
        // SAFETY: test calls remain owner-thread and copy only an identity.
        unsafe { (*self.state.get()).vtables.parent_of(vtable) }
    }

    #[cfg(test)]
    pub(crate) fn vtable_owner(&self, vtable: Oop) -> Oop {
        // SAFETY: test calls remain owner-thread and copy only an identity.
        unsafe { (*self.state.get()).vtables.owner_of(vtable) }
    }

    #[cfg(test)]
    pub(crate) fn vtable_selector_at(&self, vtable: Oop, index: usize) -> Oop {
        // SAFETY: test calls remain owner-thread and copy only an identity.
        unsafe { (*self.state.get()).vtables.selector_at(vtable, index) }
    }

    #[cfg(test)]
    pub(crate) fn checked_vtable_of(&self, object: Oop) -> Oop {
        // SAFETY: test calls remain owner-thread; checked lookup copies header.
        unsafe { Vtables::checked_vtable_of(&(*self.state.get()).heap, object) }
    }

    #[cfg(test)]
    pub(crate) fn payload_size(&self, object: Oop) -> Option<usize> {
        // SAFETY: test calls remain owner-thread and copy only the size.
        unsafe { (*self.state.get()).heap.payload_size(object) }
    }

    #[cfg(test)]
    pub(crate) fn permanence_of(&self, object: Oop) -> Option<Permanence> {
        // SAFETY: test calls remain owner-thread and copy only the class.
        unsafe { (*self.state.get()).heap.permanence_of(object) }
    }

    #[cfg(test)]
    pub(crate) fn integer_value(&self, object: Oop) -> i64 {
        // SAFETY: test supplies the canonical Integer prototype.
        unsafe { (*self.state.get()).values.integer_value(object) }
    }

    #[cfg(test)]
    pub(crate) fn float_value(&self, object: Oop) -> f64 {
        // SAFETY: test supplies the canonical Float prototype.
        unsafe { (*self.state.get()).values.float_value(object) }
    }

    #[cfg(test)]
    pub(crate) fn word_value(&self, object: Oop) -> u64 {
        // SAFETY: test supplies the canonical Word prototype.
        unsafe { (*self.state.get()).values.word_value(object) }
    }

    #[cfg(test)]
    pub(crate) fn string_bytes(&self, object: Oop) -> Option<&[u8]> {
        // SAFETY: test supplies the canonical String prototype and performs no
        // mutation or foreign call while inspecting the owned bytes.
        unsafe { (*self.state.get()).values.string_bytes(object) }
    }
}

thread_local! {
    static ACTIVE_RUNTIME: Cell<*const Runtime> = const { Cell::new(core::ptr::null()) };
}

struct ActiveRuntimeGuard<'a> {
    active: &'a Cell<*const Runtime>,
    previous: *const Runtime,
}

impl Drop for ActiveRuntimeGuard<'_> {
    fn drop(&mut self) {
        self.active.set(self.previous);
    }
}

pub(crate) fn with_active<R>(operation: impl FnOnce(&Runtime) -> R) -> R {
    ACTIVE_RUNTIME.with(|active| {
        let runtime = active.get();
        boundary::require_invariant(!runtime.is_null(), "no active Rust floor runtime");
        // SAFETY: enter installs a pointer to a live owner-thread Runtime for
        // this dynamic extent, including synchronous reentrant callbacks.
        operation(unsafe { &*runtime })
    })
}
