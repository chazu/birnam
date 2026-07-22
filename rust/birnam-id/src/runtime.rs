//! Single-thread-owned process state and short-borrow accessors.
#![allow(dead_code)] // R2 harness precedes public id_init wiring.

use core::cell::{Cell, UnsafeCell};

use crate::abi::{Method, Oop};
use crate::boundary;
use crate::heap::{Heap, Permanence};
use crate::raw::MethodAddress;
use crate::symbol::{EmbeddedNul, Symbols};
use crate::vtable::{AddMethod, Vtables};

#[derive(Clone, Copy)]
pub(crate) struct DispatchGlobals {
    pub(crate) vtable_vt: Oop,
    pub(crate) object_vt: Oop,
    pub(crate) symbol_vt: Oop,
    pub(crate) lookup: Oop,
    pub(crate) does_not_understand: Oop,
}

struct RuntimeState {
    // Ordered authorities are dropped before the raw heap owner at harness
    // teardown. No destructor dereferences a heap payload.
    symbols: Symbols,
    vtables: Vtables,
    heap: Heap,
    globals: DispatchGlobals,
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
        Self {
            owner: std::thread::current().id(),
            state: UnsafeCell::new(RuntimeState {
                symbols,
                vtables,
                heap,
                globals,
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
