//! Single-thread-owned process state and short-borrow accessors.
#![allow(dead_code)] // R2 harness precedes public id_init wiring.

use core::cell::{Cell, UnsafeCell};

use crate::abi::{Method, MethodOverwriteHook, Oop};
use crate::boundary;
use crate::env::Environment;
use crate::gc::{Marker, MetadataShell, invoke_with_mark_callback};
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
    internal_dictionaries: Option<(Oop, Oop)>,
    metadata: MetadataShell,
    environment: Environment,
    method_overwrite_hook: Option<MethodOverwriteHook>,
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
                internal_dictionaries: None,
                metadata: MetadataShell::new(),
                environment: Environment::new(),
                method_overwrite_hook: None,
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
        // Copy all callback inputs and end the state borrow before foreign code.
        // SAFETY: owner-thread serialization; direct_lookup and hook are copied.
        let hook = unsafe {
            let state = &*self.state.get();
            state
                .vtables
                .direct_lookup(vtable, selector)
                .and(state.method_overwrite_hook)
        };
        if let Some(hook) = hook {
            // No Rust state borrow survives this callback. The ABI contract
            // permits only pure Store-log mutation and read-only floor queries.
            unsafe { hook(vtable, selector) };
        }
        // SAFETY: callback returned and owner-thread serialization is restored.
        unsafe {
            (&mut *self.state.get())
                .vtables
                .add_method(vtable, selector, method)
        }
    }

    pub(crate) fn remove_method(&self, vtable: Oop, selector: Oop) -> Option<MethodAddress> {
        self.assert_owner();
        // Copy all callback inputs and end the state borrow before foreign code.
        // SAFETY: owner-thread serialization; direct_lookup and hook are copied.
        let hook = unsafe {
            let state = &*self.state.get();
            state
                .vtables
                .direct_lookup(vtable, selector)
                .and(state.method_overwrite_hook)
        };
        if let Some(hook) = hook {
            // No Rust state borrow survives this callback. The ABI contract
            // permits only pure Store-log mutation and read-only floor queries.
            unsafe { hook(vtable, selector) };
        }
        // SAFETY: callback returned and owner-thread serialization is restored.
        unsafe {
            (&mut *self.state.get())
                .vtables
                .remove_method(vtable, selector)
        }
    }

    pub(crate) fn set_method_overwrite_hook(&self, hook: Option<MethodOverwriteHook>) {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        unsafe { (&mut *self.state.get()).method_overwrite_hook = hook };
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

    pub(crate) fn set_vtable_parent(&self, vtable: Oop, parent: Oop) {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        unsafe { (&mut *self.state.get()).vtables.set_parent(vtable, parent) };
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

    pub(crate) fn clone_object(&self, object: Oop) -> Oop {
        if object.is_null() {
            return object;
        }
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        unsafe { (&mut *self.state.get()).heap.clone_collectable(object) }
            .unwrap_or_else(|| boundary::fatal_invariant("clone receiver is not a floor object"))
    }

    pub(crate) fn ensure_private_vtable(&self, object: Oop) -> Oop {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        let current = Vtables::checked_vtable_of(&state.heap, object);
        boundary::require_invariant(
            !current.is_null(),
            "ensurePrivateVtable receiver is not a floor object",
        );
        if current == state.globals.vtable_vt {
            return object;
        }
        if state.vtables.owner_of(current) == object {
            return current;
        }
        let private = state
            .vtables
            .create(&mut state.heap, current, state.globals.vtable_vt);
        state.vtables.set_owner(private, object);
        boundary::require_invariant(
            state.heap.set_vtable(object, private),
            "private-vtable receiver block was not registered",
        );
        private
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

    pub(crate) fn clone_string(&self, object: Oop, finalizer: crate::abi::Finalizer) -> Oop {
        if object.is_null() {
            return object;
        }
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        let bytes = state
            .values
            .string_bytes(object)
            .unwrap_or_else(|| boundary::fatal_invariant("String clone lost byte ownership"))
            .to_vec();
        let string_vtable = state
            .canonical
            .unwrap_or_else(|| boundary::fatal_invariant("canonical objects not installed"))
            .string_vt;
        let clone = state
            .values
            .allocate_string(&mut state.heap, string_vtable, &bytes);
        boundary::require_invariant(
            state.heap.set_finalizer(clone, finalizer),
            "String clone did not retain finalizer metadata",
        );
        clone
    }

    pub(crate) fn install_canonical(&self, canonical: CanonicalObjects) {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let slot = unsafe { &mut (*self.state.get()).canonical };
        boundary::require_invariant(slot.is_none(), "canonical objects installed twice");
        *slot = Some(canonical);
    }

    pub(crate) fn install_internal_dictionaries(&self, symbols: Oop, environment: Oop) {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        boundary::require_invariant(
            state.internal_dictionaries.is_none(),
            "internal dictionaries installed twice",
        );
        state.internal_dictionaries = Some((symbols, environment));
        for index in 0..state.symbols.count() {
            let symbol = state.symbols.at(index);
            state
                .vtables
                .add_opaque_binding(symbols, symbol, core::ptr::null_mut());
        }
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
        intern_in_state(state, bytes)
    }

    pub(crate) fn symbol_name(&self, symbol: Oop) -> *const core::ffi::c_char {
        self.assert_owner();
        // SAFETY: owner-thread serialization; the permanent Symbol record owns
        // the returned NUL-terminated bytes for the runtime lifetime.
        unsafe { (*self.state.get()).symbols.c_name_of(symbol) }
    }

    pub(crate) fn import(&self, bytes: &[u8]) -> Result<Oop, EmbeddedNul> {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        let symbol = intern_in_state(state, bytes)?;
        Ok(state.environment.import(symbol))
    }

    pub(crate) fn export(&self, bytes: &[u8], value: Oop) -> Result<Oop, EmbeddedNul> {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        let symbol = intern_in_state(state, bytes)?;
        let exported = state.environment.export(symbol, value);
        if let Some((_, environment)) = state.internal_dictionaries {
            state.vtables.add_opaque_binding(environment, symbol, value);
        }
        Ok(exported)
    }

    pub(crate) fn exported_name(&self, object: Oop) -> *const core::ffi::c_char {
        self.assert_owner();
        // SAFETY: owner-thread serialization; the permanent Symbol record owns
        // the returned NUL-terminated bytes for the runtime lifetime.
        let state = unsafe { &*self.state.get() };
        let symbol = state.environment.exported_name_of(object);
        state.symbols.c_name_of(symbol)
    }

    pub(crate) fn publish_canonical_exports(&self) {
        let globals = self.globals();
        let canonical = self.canonical();
        for (name, value) in [
            (&b"Object"[..], canonical.object_prototype),
            (&b"Integer"[..], canonical.integer_prototype),
            (&b"Float"[..], canonical.float_prototype),
            (&b"Word"[..], canonical.word_prototype),
            (&b"String"[..], canonical.string_prototype),
            (&b"Boolean"[..], canonical.true_object),
            (&b"Vtable"[..], globals.vtable_vt),
            (&b"Symbol"[..], canonical.symbol_prototype),
            (&b"SymbolVt"[..], globals.symbol_vt),
            (&b"ObjectVt"[..], globals.object_vt),
            (&b"IntegerVt"[..], canonical.smallint_vt),
            (&b"FloatVt"[..], canonical.float_vt),
            (&b"WordVt"[..], canonical.word_vt),
            (&b"StringVt"[..], canonical.string_vt),
        ] {
            self.export(name, value)
                .unwrap_or_else(|_| boundary::fatal_invariant("static export name contains NUL"));
        }
    }

    #[cfg(test)]
    pub(crate) fn export_count(&self) -> usize {
        self.assert_owner();
        // SAFETY: owner-thread serialization and a copied count.
        unsafe { (*self.state.get()).environment.count() }
    }

    #[cfg(test)]
    pub(crate) fn export_name_at(&self, index: usize) -> Oop {
        self.assert_owner();
        // SAFETY: owner-thread serialization and a copied identity.
        unsafe { (*self.state.get()).environment.name_at(index) }
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

    pub(crate) fn vtable_count(&self) -> usize {
        self.assert_owner();
        // SAFETY: owner-thread serialization and a copied count.
        unsafe { (*self.state.get()).vtables.count() }
    }

    pub(crate) fn vtable_at(&self, index: usize) -> Oop {
        self.assert_owner();
        // SAFETY: owner-thread serialization and a copied identity.
        unsafe { (*self.state.get()).vtables.at(index) }
    }

    pub(crate) fn vtable_parent(&self, vtable: Oop) -> Oop {
        self.assert_owner();
        // SAFETY: owner-thread serialization and a copied identity.
        unsafe { (*self.state.get()).vtables.parent_of(vtable) }
    }

    pub(crate) fn vtable_owner(&self, vtable: Oop) -> Oop {
        self.assert_owner();
        // SAFETY: owner-thread serialization and a copied identity.
        unsafe { (*self.state.get()).vtables.owner_of(vtable) }
    }

    pub(crate) fn vtable_selector_at(&self, vtable: Oop, index: usize) -> Oop {
        self.assert_owner();
        // SAFETY: owner-thread serialization and a copied identity.
        unsafe { (*self.state.get()).vtables.selector_at(vtable, index) }
    }

    pub(crate) fn checked_vtable_of(&self, object: Oop) -> Oop {
        self.assert_owner();
        // SAFETY: owner-thread serialization; checked lookup copies the header.
        unsafe { Vtables::checked_vtable_of(&(*self.state.get()).heap, object) }
    }

    pub(crate) fn payload_size(&self, object: Oop) -> Option<usize> {
        self.assert_owner();
        // SAFETY: owner-thread serialization and a copied size.
        unsafe { (*self.state.get()).heap.payload_size(object) }
    }

    pub(crate) fn gc_set_stack_bottom(&self, address: usize) {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        unsafe { (&mut *self.state.get()).metadata.set_stack_bottom(address) };
    }

    pub(crate) fn gc_set_auto_requested(&self, enabled: bool) {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        unsafe {
            (&mut *self.state.get())
                .metadata
                .set_auto_requested(enabled)
        };
    }

    pub(crate) fn gc_set_precise(&self, enabled: bool) {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        unsafe { (&mut *self.state.get()).metadata.set_precise(enabled) };
    }

    pub(crate) fn gc_register_root_provider(&self, provider: crate::abi::RootProvider) {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        unsafe {
            (&mut *self.state.get())
                .metadata
                .register_root_provider(provider)
        };
    }

    pub(crate) fn gc_collect_disabled(&self) -> usize {
        self.assert_owner();
        // R4.1 activates heap marking but not foreign roots, stack scanning, or
        // sweeping. Exercise the permanent floor roots, then clear the private
        // marks so the still-disabled public operation remains observationally
        // inert and returns the R3-compatible zero result.
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        state.heap.clear_marks();
        self.run_mark_phase();
        // SAFETY: all callbacks have returned and owner-thread serialization is
        // restored before marks are cleared.
        let state = unsafe { &mut *self.state.get() };
        state.heap.clear_marks();
        state.metadata.collect_disabled()
    }

    pub(crate) fn gc_live_count(&self) -> usize {
        self.assert_owner();
        // SAFETY: owner-thread serialization and a copied count.
        unsafe { (*self.state.get()).heap.allocation_count() }
    }

    fn run_mark_phase(&self) {
        let state_pointer = self.state.get();
        let mut marker = Marker::new();
        // Copy callback authority and raw owner pointers, then end the state
        // borrow before the first foreign call.
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let (providers, heap_pointer) = unsafe {
            let state = &mut *state_pointer;
            mark_floor_roots(state, &mut marker);
            (
                state.metadata.root_providers(),
                core::ptr::addr_of_mut!(state.heap),
            )
        };
        let marker_pointer = core::ptr::addr_of_mut!(marker);

        for provider in providers {
            // SAFETY: no Rust references to state.heap or marker survive this
            // call; the provider may use the mark function only synchronously.
            unsafe {
                invoke_with_mark_callback(heap_pointer, marker_pointer, |mark| provider(mark))
            };
        }

        while let Some(address) = marker.pop() {
            // Conservative scanning and hook lookup finish before foreign code.
            // SAFETY: owner-thread serialization and no callback in this scope.
            let hook = unsafe {
                let state = &mut *state_pointer;
                marker.trace_conservative_payload(&mut state.heap, address);
                let object = address as Oop;
                let vtable = state.heap.vtable_of(object).unwrap_or_else(|| {
                    boundary::fatal_invariant("marked GC block lost its vtable")
                });
                state.vtables.trace_hook(vtable)
            };
            if let Some(hook) = hook {
                let object = address as Oop;
                // SAFETY: no Rust references to state.heap or marker survive
                // this call; the hook may mark only synchronously.
                unsafe {
                    invoke_with_mark_callback(heap_pointer, marker_pointer, |mark| {
                        hook(object, mark)
                    })
                };
            }
        }
    }

    pub(crate) fn set_trace_hook(&self, vtable: Oop, hook: crate::abi::TraceHook) {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        unsafe {
            (&mut *self.state.get())
                .vtables
                .set_trace_hook(vtable, hook)
        };
    }

    pub(crate) fn set_finalizer(&self, object: Oop, finalizer: crate::abi::Finalizer) {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let _ = unsafe {
            (&mut *self.state.get())
                .heap
                .set_finalizer(object, finalizer)
        };
    }

    pub(crate) fn set_layout(&self, vtable: Oop, kinds: *const u8, count: usize, rest: u8) {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        unsafe {
            (&mut *self.state.get())
                .vtables
                .set_layout(vtable, kinds, count, rest)
        };
    }

    pub(crate) fn slot_kind(&self, vtable: Oop, word: usize) -> u8 {
        self.assert_owner();
        // SAFETY: owner-thread serialization and a copied slot kind.
        unsafe { (*self.state.get()).vtables.slot_kind(vtable, word) }
    }

    #[cfg(test)]
    pub(crate) fn permanence_of(&self, object: Oop) -> Option<Permanence> {
        // SAFETY: test calls remain owner-thread and copy only the class.
        unsafe { (*self.state.get()).heap.permanence_of(object) }
    }

    pub(crate) fn integer_value(&self, object: Oop) -> i64 {
        self.assert_owner();
        // SAFETY: caller supplies an Integer object.
        unsafe { (*self.state.get()).values.integer_value(object) }
    }

    pub(crate) fn float_value(&self, object: Oop) -> f64 {
        self.assert_owner();
        // SAFETY: caller supplies a Float object.
        unsafe { (*self.state.get()).values.float_value(object) }
    }

    pub(crate) fn word_value(&self, object: Oop) -> u64 {
        self.assert_owner();
        // SAFETY: caller supplies a Word object.
        unsafe { (*self.state.get()).values.word_value(object) }
    }

    pub(crate) fn is_integer(&self, object: Oop) -> bool {
        self.checked_vtable_of(object) == self.canonical().smallint_vt
    }

    pub(crate) fn is_float(&self, object: Oop) -> bool {
        self.checked_vtable_of(object) == self.canonical().float_vt
    }

    pub(crate) fn is_word(&self, object: Oop) -> bool {
        self.checked_vtable_of(object) == self.canonical().word_vt
    }

    pub(crate) fn string_bytes(&self, object: Oop) -> Option<&[u8]> {
        self.assert_owner();
        // SAFETY: caller supplies a String object and performs no mutation or
        // foreign call while inspecting the owned bytes.
        unsafe { (*self.state.get()).values.string_bytes(object) }
    }

    pub(crate) fn release_string(&self, object: Oop) {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let released = unsafe { (&mut *self.state.get()).values.release_string(object) };
        boundary::require_invariant(released, "String finalizer lost byte ownership");
    }

    #[cfg(test)]
    pub(crate) fn has_finalizer(&self, object: Oop) -> Option<bool> {
        self.assert_owner();
        // SAFETY: owner-thread serialization and a copied metadata bit.
        unsafe { (*self.state.get()).heap.has_finalizer(object) }
    }

    #[cfg(test)]
    pub(crate) fn gc_mark_floor_roots_for_test(&self) {
        self.assert_owner();
        // SAFETY: test calls remain owner-thread and invoke no foreign code.
        unsafe { (&mut *self.state.get()).heap.clear_marks() };
        self.run_mark_phase();
    }

    #[cfg(test)]
    pub(crate) fn is_marked_for_test(&self, object: Oop) -> Option<bool> {
        self.assert_owner();
        // SAFETY: test calls remain owner-thread and copy one metadata bit.
        unsafe { (*self.state.get()).heap.is_marked(object) }
    }
}

fn mark_floor_roots(state: &mut RuntimeState, marker: &mut Marker) {
    let export_values = state.environment.root_values();
    let interned_symbols = (0..state.symbols.count())
        .map(|index| state.symbols.at(index))
        .collect::<Vec<_>>();
    for root in export_values.into_iter().chain(interned_symbols) {
        marker.mark_oop(&mut state.heap, root);
    }
}

fn intern_in_state(state: &mut RuntimeState, bytes: &[u8]) -> Result<Oop, EmbeddedNul> {
    let before = state.symbols.count();
    let symbol = state
        .symbols
        .intern(&mut state.heap, state.globals.symbol_vt, bytes)?;
    if state.symbols.count() != before
        && let Some((symbols, _)) = state.internal_dictionaries
    {
        state
            .vtables
            .add_opaque_binding(symbols, symbol, core::ptr::null_mut());
    }
    Ok(symbol)
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
