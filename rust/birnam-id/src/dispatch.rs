//! Metacircular binding, synchronous send, and DNU redispatch.
#![allow(dead_code)] // R2 harness precedes public id_init wiring.

use crate::abi::Oop;
use crate::boundary;
use crate::heap::Heap;
use crate::raw::MethodAddress;
use crate::runtime::{DispatchGlobals, Runtime, with_active};
use crate::symbol::Symbols;
use crate::vtable::Vtables;

const MAX_DNU_ARGS: usize = 16;

impl Runtime {
    pub(crate) fn bind(&self, receiver: Oop, selector: Oop) -> Option<MethodAddress> {
        let receiver_vtable = self.vtable_for_valid_receiver(receiver);
        if receiver_vtable.is_null() {
            return None;
        }
        let globals = self.globals();
        if receiver_vtable == globals.vtable_vt && selector == globals.lookup {
            return self.lookup_method(globals.vtable_vt, globals.lookup);
        }

        let lookup_args = [selector];
        // SAFETY: the one-element buffer is live for the synchronous send.
        let encoded_method = unsafe {
            self.send_abi(
                receiver_vtable,
                globals.lookup,
                lookup_args.as_ptr(),
                lookup_args.len(),
            )
        };
        // SAFETY: the lookup primitive returns only null or an address copied
        // from the runtime method dictionary.
        unsafe { MethodAddress::from_oop(encoded_method) }
    }

    /// Send while preserving the raw C argument-buffer contract.
    ///
    /// # Safety
    ///
    /// `receiver` and `selector` must be live floor objects. `args` must point
    /// to `count` Oops, except that null is permitted exactly when count is zero.
    pub(crate) unsafe fn send_abi(
        &self,
        receiver: Oop,
        selector: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        if args.is_null() && count != 0 {
            boundary::fatal_invariant("IdApi.send args is null with nonzero count");
        }

        if let Some(method) = self.bind(receiver, selector) {
            let callable = method.to_method();
            // No Rust state reference or collection guard survives this call.
            // SAFETY: forwarded ABI contract and dictionary-proven Method.
            return unsafe { callable(receiver, args, count) };
        }

        let globals = self.globals();
        let handler = self
            .bind(receiver, globals.does_not_understand)
            .unwrap_or_else(|| boundary::fatal_invariant("doesNotUnderstand: is unbound"));
        let mut forwarded = [core::ptr::null_mut(); 1 + MAX_DNU_ARGS];
        forwarded[0] = selector;
        let forwarded_count = count.min(MAX_DNU_ARGS);
        if forwarded_count != 0 {
            // args cannot be null here because the entry check accepted only a
            // null zero-count buffer.
            // SAFETY: caller provides count live Oops; copy is capped by count.
            unsafe {
                core::ptr::copy_nonoverlapping(args, forwarded[1..].as_mut_ptr(), forwarded_count)
            };
        }
        let callable = handler.to_method();
        // No Rust state reference or collection guard survives this call.
        // SAFETY: stack buffer is live for the synchronous Method invocation.
        unsafe { callable(receiver, forwarded.as_ptr(), 1 + forwarded_count) }
    }

    pub(crate) fn send_slice(&self, receiver: Oop, selector: Oop, args: &[Oop]) -> Oop {
        // SAFETY: slice supplies a valid pointer/count pair for this call.
        unsafe { self.send_abi(receiver, selector, args.as_ptr(), args.len()) }
    }
}

unsafe extern "C" fn vtable_lookup_method(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    if count == 0 || args.is_null() {
        boundary::fatal_invariant("vtable lookup requires one selector");
    }
    // SAFETY: the Method ABI promises at least count entries and count > 0.
    let selector = unsafe { args.read() };
    with_active(|runtime| {
        runtime
            .lookup_method(receiver, selector)
            .map(MethodAddress::as_oop)
            .unwrap_or(core::ptr::null_mut())
    })
}

pub(crate) struct DispatchHarness {
    pub(crate) runtime: Runtime,
    pub(crate) meta: Oop,
    pub(crate) object_vtable: Oop,
    pub(crate) lookup: Oop,
    pub(crate) does_not_understand: Oop,
}

pub(crate) fn build_harness() -> DispatchHarness {
    let mut heap = Heap::new();
    let mut vtables = Vtables::new();
    let meta = vtables.create(&mut heap, core::ptr::null_mut(), core::ptr::null_mut());
    boundary::require_invariant(heap.set_vtable(meta, meta), "metavtable was not registered");
    let object_vtable = vtables.create(&mut heap, core::ptr::null_mut(), meta);
    let symbol_vtable = vtables.create(&mut heap, object_vtable, meta);
    let mut symbols = Symbols::new();
    let lookup = symbols
        .intern(&mut heap, symbol_vtable, b"lookup")
        .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
    let does_not_understand = symbols
        .intern(&mut heap, symbol_vtable, b"doesNotUnderstand:")
        .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));

    let runtime = Runtime::new(
        heap,
        vtables,
        symbols,
        DispatchGlobals {
            vtable_vt: meta,
            object_vt: object_vtable,
            symbol_vt: symbol_vtable,
            lookup,
            does_not_understand,
        },
    );
    assert_eq!(
        runtime.add_method(meta, lookup, vtable_lookup_method),
        crate::vtable::AddMethod::Added
    );
    DispatchHarness {
        runtime,
        meta,
        object_vtable,
        lookup,
        does_not_understand,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" fn answer_42(_: Oop, _: *const Oop, _: usize) -> Oop {
        42usize as Oop
    }

    unsafe extern "C" fn report_buffer_shape(_: Oop, args: *const Oop, count: usize) -> Oop {
        if args.is_null() && count == 0 {
            1usize as Oop
        } else if !args.is_null() && count == 0 {
            2usize as Oop
        } else {
            core::ptr::null_mut()
        }
    }

    unsafe extern "C" fn dnu_count(_: Oop, _: *const Oop, count: usize) -> Oop {
        count as Oop
    }

    unsafe extern "C" fn reentrant_send(receiver: Oop, args: *const Oop, count: usize) -> Oop {
        if count != 1 || args.is_null() {
            return core::ptr::null_mut();
        }
        // SAFETY: this Method requires one selector argument.
        let selector = unsafe { args.read() };
        with_active(|runtime| {
            // SAFETY: receiver/selector are harness objects; null/zero is valid.
            unsafe { runtime.send_abi(receiver, selector, core::ptr::null(), 0) }
        })
    }

    fn selector(runtime: &Runtime, name: &[u8]) -> Oop {
        runtime.intern(name).expect("test selector contains no NUL")
    }

    #[test]
    fn bind_bottoms_out_lookup_and_ordinary_send_is_metacircular() {
        let harness = build_harness();
        let answer = selector(&harness.runtime, b"answer");
        let receiver = harness.runtime.allocate_instance(harness.object_vtable, 0);
        assert_eq!(
            harness
                .runtime
                .add_method(harness.object_vtable, answer, answer_42),
            crate::vtable::AddMethod::Added
        );

        let result = harness.runtime.enter(|| {
            assert!(harness.runtime.bind(harness.meta, harness.lookup).is_some());
            harness.runtime.send_slice(receiver, answer, &[])
        });
        assert_eq!(result.addr(), 42);
    }

    #[test]
    fn null_zero_buffer_is_preserved_without_constructing_a_slice() {
        let harness = build_harness();
        let shape = selector(&harness.runtime, b"bufferShape");
        let receiver = harness.runtime.allocate_instance(harness.object_vtable, 0);
        let _ = harness
            .runtime
            .add_method(harness.object_vtable, shape, report_buffer_shape);

        harness.runtime.enter(|| {
            // SAFETY: null is valid exactly for this zero-count send.
            let null_result = unsafe {
                harness
                    .runtime
                    .send_abi(receiver, shape, core::ptr::null(), 0)
            };
            assert_eq!(null_result.addr(), 1);
            assert_eq!(harness.runtime.send_slice(receiver, shape, &[]).addr(), 2);
        });
    }

    #[test]
    fn dnu_forwards_selector_and_caps_original_arguments_at_sixteen() {
        let harness = build_harness();
        let missing = selector(&harness.runtime, b"missingSelector");
        let receiver = harness.runtime.allocate_instance(harness.object_vtable, 0);
        let _ = harness.runtime.add_method(
            harness.object_vtable,
            harness.does_not_understand,
            dnu_count,
        );
        let arguments = [core::ptr::null_mut(); 20];
        let result = harness
            .runtime
            .enter(|| harness.runtime.send_slice(receiver, missing, &arguments));
        assert_eq!(result.addr(), 17);
    }

    #[test]
    fn rust_method_reenters_send_without_a_mutex_or_live_borrow() {
        let harness = build_harness();
        let inner = selector(&harness.runtime, b"inner");
        let outer = selector(&harness.runtime, b"outer:");
        let receiver = harness.runtime.allocate_instance(harness.object_vtable, 0);
        let _ = harness
            .runtime
            .add_method(harness.object_vtable, inner, answer_42);
        let _ = harness
            .runtime
            .add_method(harness.object_vtable, outer, reentrant_send);

        let result = harness
            .runtime
            .enter(|| harness.runtime.send_slice(receiver, outer, &[inner]));
        assert_eq!(result.addr(), 42);
    }

    #[test]
    fn harness_retains_byte_oriented_selector_authority() {
        let harness = build_harness();
        assert_eq!(harness.runtime.symbol_count(), 2);
        let globals = harness.runtime.globals();
        assert_eq!(globals.object_vt, harness.object_vtable);
        assert!(!globals.symbol_vt.is_null());
    }
}
