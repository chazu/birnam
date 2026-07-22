//! Metacircular binding, synchronous send, and DNU redispatch.
#![allow(dead_code)] // R2 harness precedes public id_init wiring.

use crate::abi::Oop;
use crate::boundary;
use crate::heap::{Heap, Permanence};
use crate::raw::MethodAddress;
use crate::runtime::{CanonicalObjects, DispatchGlobals, Runtime, with_active};
use crate::symbol::Symbols;
use crate::values::Values;
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

unsafe extern "C" fn vtable_add_method(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    if count < 2 || args.is_null() {
        boundary::fatal_invariant("vtable addMethod requires selector and method");
    }
    // SAFETY: the Method ABI promises at least count entries and count >= 2.
    let selector = unsafe { args.read() };
    // SAFETY: the second entry is within the same live argument buffer.
    let encoded_method = unsafe { args.add(1).read() };
    // SAFETY: bootstrap and later IdApi.method callers encode a Method address.
    let method = unsafe { MethodAddress::from_oop(encoded_method) }
        .unwrap_or_else(|| boundary::fatal_invariant("vtable addMethod received null method"))
        .to_method();
    with_active(|runtime| {
        let _ = runtime.add_method(receiver, selector, method);
    });
    encoded_method
}

unsafe extern "C" fn vtable_remove_method(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    if count == 0 || args.is_null() {
        boundary::fatal_invariant("vtable removeMethod: requires one selector");
    }
    // SAFETY: the Method ABI promises at least count entries and count > 0.
    let selector = unsafe { args.read() };
    with_active(|runtime| {
        runtime
            .remove_method(receiver, selector)
            .map(MethodAddress::as_oop)
            .unwrap_or(core::ptr::null_mut())
    })
}

unsafe extern "C" fn vtable_allocate(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    if count == 0 || args.is_null() {
        boundary::fatal_invariant("vtable allocate requires pointer-encoded size");
    }
    // SAFETY: the Method ABI promises at least count entries and count > 0.
    let payload_size = unsafe { args.read() }.addr();
    with_active(|runtime| runtime.allocate_instance(receiver, payload_size))
}

unsafe extern "C" fn vtable_delegated(receiver: Oop, _: *const Oop, _: usize) -> Oop {
    with_active(|runtime| runtime.create_vtable(receiver))
}

unsafe extern "C" fn object_does_not_understand(_: Oop, _: *const Oop, _: usize) -> Oop {
    boundary::fatal_invariant("doesNotUnderstand:")
}

#[derive(Clone, Copy)]
pub(crate) struct BootstrapSelectors {
    pub(crate) lookup: Oop,
    pub(crate) add_method: Oop,
    pub(crate) remove_method: Oop,
    pub(crate) allocate: Oop,
    pub(crate) delegated: Oop,
    pub(crate) does_not_understand: Oop,
}

pub(crate) struct Bootstrap {
    pub(crate) runtime: Runtime,
    pub(crate) selectors: BootstrapSelectors,
}

fn static_selector(symbols: &mut Symbols, heap: &mut Heap, symbol_vtable: Oop, name: &[u8]) -> Oop {
    symbols
        .intern(heap, symbol_vtable, name)
        .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"))
}

/// Build the complete R2 identity graph without publishing an `IdApi` table.
pub(crate) fn build_bootstrap() -> Bootstrap {
    let mut heap = Heap::new();
    let mut vtables = Vtables::new();

    let vtable_vt = vtables.create(&mut heap, core::ptr::null_mut(), core::ptr::null_mut());
    boundary::require_invariant(
        heap.set_vtable(vtable_vt, vtable_vt),
        "metavtable was not registered",
    );
    let object_vt = vtables.create(&mut heap, core::ptr::null_mut(), vtable_vt);
    vtables.set_parent(vtable_vt, object_vt);
    let symbol_vt = vtables.create(&mut heap, object_vt, vtable_vt);
    let symbol_list = vtables.create(&mut heap, core::ptr::null_mut(), vtable_vt);
    let environment = vtables.create(&mut heap, core::ptr::null_mut(), vtable_vt);
    let smallint_vt = vtables.create(&mut heap, object_vt, vtable_vt);
    let string_vt = vtables.create(&mut heap, object_vt, vtable_vt);
    let float_vt = vtables.create(&mut heap, object_vt, vtable_vt);
    let word_vt = vtables.create(&mut heap, object_vt, vtable_vt);
    let nil_vt = vtables.create(&mut heap, object_vt, vtable_vt);
    let bool_vt = vtables.create(&mut heap, object_vt, vtable_vt);

    let mut values = Values::new();
    let nil = values.allocate_empty(&mut heap, nil_vt, Permanence::Permanent);
    let true_object = values.allocate_empty(&mut heap, bool_vt, Permanence::Permanent);
    let false_object = values.allocate_empty(&mut heap, bool_vt, Permanence::Permanent);

    let mut symbols = Symbols::new();
    let selectors = BootstrapSelectors {
        lookup: static_selector(&mut symbols, &mut heap, symbol_vt, b"lookup"),
        add_method: static_selector(&mut symbols, &mut heap, symbol_vt, b"addMethod"),
        remove_method: static_selector(&mut symbols, &mut heap, symbol_vt, b"removeMethod:"),
        allocate: static_selector(&mut symbols, &mut heap, symbol_vt, b"allocate"),
        delegated: static_selector(&mut symbols, &mut heap, symbol_vt, b"delegated"),
        does_not_understand: static_selector(
            &mut symbols,
            &mut heap,
            symbol_vt,
            b"doesNotUnderstand:",
        ),
    };

    let runtime = Runtime::new_with_values(
        heap,
        vtables,
        symbols,
        values,
        DispatchGlobals {
            vtable_vt,
            object_vt,
            symbol_vt,
            lookup: selectors.lookup,
            does_not_understand: selectors.does_not_understand,
        },
    );

    boundary::require_invariant(
        runtime.add_method(vtable_vt, selectors.lookup, vtable_lookup_method)
            == crate::vtable::AddMethod::Added,
        "bootstrap lookup binding was not new",
    );
    boundary::require_invariant(
        runtime.add_method(vtable_vt, selectors.add_method, vtable_add_method)
            == crate::vtable::AddMethod::Added,
        "bootstrap addMethod binding was not new",
    );
    boundary::require_invariant(
        runtime.add_method(vtable_vt, selectors.remove_method, vtable_remove_method)
            == crate::vtable::AddMethod::Added,
        "bootstrap removeMethod: binding was not new",
    );

    runtime.enter(|| {
        for (vtable, selector, method) in [
            (
                vtable_vt,
                selectors.allocate,
                vtable_allocate as crate::abi::Method,
            ),
            (
                vtable_vt,
                selectors.delegated,
                vtable_delegated as crate::abi::Method,
            ),
            (
                object_vt,
                selectors.does_not_understand,
                object_does_not_understand as crate::abi::Method,
            ),
        ] {
            let arguments = [selector, MethodAddress::from_method(method).as_oop()];
            let installed = runtime.send_slice(vtable, selectors.add_method, &arguments);
            boundary::require_invariant(
                installed == arguments[1],
                "metacircular addMethod changed method identity",
            );
        }
    });

    let object_prototype = runtime.allocate_empty(object_vt, Permanence::Permanent);
    let integer_prototype = runtime.allocate_integer(smallint_vt, 0);
    let float_prototype = runtime.allocate_float(float_vt, 0.0);
    let word_prototype = runtime.allocate_word(word_vt, 0);
    let string_prototype = runtime.allocate_string(string_vt, b"");
    let symbol_prototype = runtime
        .intern(b"Symbol")
        .unwrap_or_else(|_| boundary::fatal_invariant("static prototype name contains NUL"));

    for (vtable, owner) in [
        (object_vt, object_prototype),
        (smallint_vt, integer_prototype),
        (float_vt, float_prototype),
        (word_vt, word_prototype),
        (string_vt, string_prototype),
        (symbol_vt, symbol_prototype),
        (nil_vt, nil),
        (bool_vt, true_object),
    ] {
        runtime.set_vtable_owner(vtable, owner);
    }

    runtime.install_canonical(CanonicalObjects {
        symbol_list,
        environment,
        smallint_vt,
        string_vt,
        float_vt,
        word_vt,
        nil_vt,
        bool_vt,
        nil,
        true_object,
        false_object,
        object_prototype,
        integer_prototype,
        float_prototype,
        word_prototype,
        string_prototype,
        symbol_prototype,
    });

    Bootstrap { runtime, selectors }
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

#[cfg(feature = "ffi-test-hooks")]
unsafe extern "C" fn external_null_answer(_: Oop, args: *const Oop, count: usize) -> Oop {
    if args.is_null() && count == 0 {
        4242usize as Oop
    } else {
        core::ptr::null_mut()
    }
}

#[cfg(feature = "ffi-test-hooks")]
pub(crate) fn run_external_reentry(method: crate::abi::Method) -> usize {
    let harness = build_harness();
    let inner = harness
        .runtime
        .intern(b"externalInner")
        .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
    let outer = harness
        .runtime
        .intern(b"externalOuter:")
        .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
    let receiver = harness.runtime.allocate_instance(harness.object_vtable, 0);
    let _ = harness
        .runtime
        .add_method(harness.object_vtable, inner, external_null_answer);
    let _ = harness
        .runtime
        .add_method(harness.object_vtable, outer, method);
    harness
        .runtime
        .enter(|| harness.runtime.send_slice(receiver, outer, &[inner]))
        .addr()
}

/// Reenter the dynamically active R2 dispatch harness from a foreign Method.
///
/// # Safety
///
/// The ordinary send ABI contract applies to all arguments.
#[cfg(feature = "ffi-test-hooks")]
pub(crate) unsafe fn active_send(
    receiver: Oop,
    selector: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    with_active(|runtime| {
        // SAFETY: forwarded caller contract; active runtime remains live for
        // the complete synchronous callback extent.
        unsafe { runtime.send_abi(receiver, selector, args, count) }
    })
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

    unsafe extern "C" fn answer_77(_: Oop, _: *const Oop, _: usize) -> Oop {
        77usize as Oop
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

    #[test]
    fn bootstrap_freezes_vtable_symbol_and_method_order() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let globals = runtime.globals();
        let canonical = runtime.canonical();
        let expected_vtables = [
            globals.vtable_vt,
            globals.object_vt,
            globals.symbol_vt,
            canonical.symbol_list,
            canonical.environment,
            canonical.smallint_vt,
            canonical.string_vt,
            canonical.float_vt,
            canonical.word_vt,
            canonical.nil_vt,
            canonical.bool_vt,
        ];
        let expected_parents = [
            globals.object_vt,
            core::ptr::null_mut(),
            globals.object_vt,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            globals.object_vt,
            globals.object_vt,
            globals.object_vt,
            globals.object_vt,
            globals.object_vt,
            globals.object_vt,
        ];

        assert_eq!(runtime.vtable_count(), expected_vtables.len());
        for (index, expected) in expected_vtables.into_iter().enumerate() {
            assert_eq!(runtime.vtable_at(index), expected);
            assert_eq!(runtime.vtable_parent(expected), expected_parents[index]);
            assert_eq!(runtime.checked_vtable_of(expected), globals.vtable_vt);
            assert_eq!(runtime.permanence_of(expected), Some(Permanence::Permanent));
            assert_eq!(runtime.payload_size(expected), Some(80));
        }

        let selector_names: [&[u8]; 7] = [
            b"lookup",
            b"addMethod",
            b"removeMethod:",
            b"allocate",
            b"delegated",
            b"doesNotUnderstand:",
            b"Symbol",
        ];
        assert_eq!(runtime.symbol_count(), selector_names.len());
        for (index, name) in selector_names.into_iter().enumerate() {
            let symbol = runtime.symbol_at(index);
            assert_eq!(runtime.symbol_bytes(symbol), Some(name));
            assert_eq!(runtime.checked_vtable_of(symbol), globals.symbol_vt);
            assert_eq!(runtime.permanence_of(symbol), Some(Permanence::Permanent));
        }

        let meta_selectors = [
            bootstrap.selectors.lookup,
            bootstrap.selectors.add_method,
            bootstrap.selectors.remove_method,
            bootstrap.selectors.allocate,
            bootstrap.selectors.delegated,
        ];
        for (index, selector) in meta_selectors.into_iter().enumerate() {
            assert_eq!(
                runtime.vtable_selector_at(globals.vtable_vt, index),
                selector
            );
        }
        assert!(
            runtime
                .vtable_selector_at(globals.vtable_vt, meta_selectors.len())
                .is_null()
        );
        assert_eq!(
            runtime.vtable_selector_at(globals.object_vt, 0),
            bootstrap.selectors.does_not_understand
        );
        assert!(runtime.vtable_selector_at(globals.object_vt, 1).is_null());
    }

    #[test]
    fn bootstrap_canonical_identities_have_final_shapes_lifetimes_and_owners() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let globals = runtime.globals();
        let canonical = runtime.canonical();

        let all_identities = [
            globals.vtable_vt,
            globals.object_vt,
            globals.symbol_vt,
            canonical.symbol_list,
            canonical.environment,
            canonical.smallint_vt,
            canonical.string_vt,
            canonical.float_vt,
            canonical.word_vt,
            canonical.nil_vt,
            canonical.bool_vt,
            canonical.nil,
            canonical.true_object,
            canonical.false_object,
            canonical.object_prototype,
            canonical.integer_prototype,
            canonical.float_prototype,
            canonical.word_prototype,
            canonical.string_prototype,
            canonical.symbol_prototype,
        ];
        let unique_addresses: std::collections::HashSet<_> =
            all_identities.into_iter().map(Oop::addr).collect();
        assert_eq!(unique_addresses.len(), all_identities.len());

        for (object, vtable) in [
            (canonical.nil, canonical.nil_vt),
            (canonical.true_object, canonical.bool_vt),
            (canonical.false_object, canonical.bool_vt),
            (canonical.object_prototype, globals.object_vt),
            (canonical.integer_prototype, canonical.smallint_vt),
            (canonical.float_prototype, canonical.float_vt),
            (canonical.word_prototype, canonical.word_vt),
            (canonical.string_prototype, canonical.string_vt),
            (canonical.symbol_prototype, globals.symbol_vt),
        ] {
            assert_eq!(runtime.checked_vtable_of(object), vtable);
        }
        for (vtable, owner) in [
            (canonical.nil_vt, canonical.nil),
            (canonical.bool_vt, canonical.true_object),
            (globals.object_vt, canonical.object_prototype),
            (canonical.smallint_vt, canonical.integer_prototype),
            (canonical.float_vt, canonical.float_prototype),
            (canonical.word_vt, canonical.word_prototype),
            (canonical.string_vt, canonical.string_prototype),
            (globals.symbol_vt, canonical.symbol_prototype),
        ] {
            assert_eq!(runtime.vtable_owner(vtable), owner);
        }
        for object in [
            canonical.nil,
            canonical.true_object,
            canonical.false_object,
            canonical.object_prototype,
            canonical.symbol_prototype,
        ] {
            assert_eq!(runtime.permanence_of(object), Some(Permanence::Permanent));
        }
        for object in [
            canonical.integer_prototype,
            canonical.float_prototype,
            canonical.word_prototype,
            canonical.string_prototype,
        ] {
            assert_eq!(runtime.permanence_of(object), Some(Permanence::Collectable));
        }
        for object in [
            canonical.nil,
            canonical.true_object,
            canonical.false_object,
            canonical.object_prototype,
        ] {
            assert_eq!(runtime.payload_size(object), Some(0));
        }
        assert_eq!(runtime.payload_size(canonical.integer_prototype), Some(8));
        assert_eq!(runtime.payload_size(canonical.float_prototype), Some(8));
        assert_eq!(runtime.payload_size(canonical.word_prototype), Some(8));
        assert_eq!(runtime.payload_size(canonical.string_prototype), Some(16));
        assert_eq!(runtime.payload_size(canonical.symbol_prototype), Some(8));
        assert_eq!(runtime.integer_value(canonical.integer_prototype), 0);
        assert_eq!(runtime.float_value(canonical.float_prototype), 0.0);
        assert_eq!(runtime.word_value(canonical.word_prototype), 0);
        assert_eq!(
            runtime.string_bytes(canonical.string_prototype),
            Some(&b""[..])
        );
        assert_eq!(
            runtime.symbol_bytes(canonical.symbol_prototype),
            Some(&b"Symbol"[..])
        );
    }

    #[test]
    fn bootstrap_primitives_support_metacircular_dispatch_removal_and_dnu() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let globals = runtime.globals();
        let answer = selector(runtime, b"bootstrapAnswer");
        let missing = selector(runtime, b"bootstrapMissing:");

        runtime.enter(|| {
            let child = runtime.send_slice(globals.object_vt, bootstrap.selectors.delegated, &[]);
            let install = [answer, MethodAddress::from_method(answer_77).as_oop()];
            assert_eq!(
                runtime.send_slice(child, bootstrap.selectors.add_method, &install),
                install[1]
            );

            let receiver = runtime.send_slice(
                child,
                bootstrap.selectors.allocate,
                &[core::ptr::null_mut()],
            );
            assert_eq!(runtime.send_slice(receiver, answer, &[]).addr(), 77);

            let dnu_install = [
                bootstrap.selectors.does_not_understand,
                MethodAddress::from_method(dnu_count).as_oop(),
            ];
            let _ = runtime.send_slice(child, bootstrap.selectors.add_method, &dnu_install);
            assert_eq!(
                runtime
                    .send_slice(receiver, missing, &[core::ptr::null_mut(); 20])
                    .addr(),
                17
            );

            assert_eq!(
                runtime.send_slice(child, bootstrap.selectors.remove_method, &[answer]),
                install[1]
            );
            assert_eq!(runtime.send_slice(receiver, answer, &[]).addr(), 1);
        });
    }

    #[test]
    fn canonical_exports_preserve_order_aliases_and_same_name_rebinding() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let globals = runtime.globals();
        let canonical = runtime.canonical();
        let expected: [(&[u8], Oop); 14] = [
            (b"Object", canonical.object_prototype),
            (b"Integer", canonical.integer_prototype),
            (b"Float", canonical.float_prototype),
            (b"Word", canonical.word_prototype),
            (b"String", canonical.string_prototype),
            (b"Boolean", canonical.true_object),
            (b"Vtable", globals.vtable_vt),
            (b"Symbol", canonical.symbol_prototype),
            (b"SymbolVt", globals.symbol_vt),
            (b"ObjectVt", globals.object_vt),
            (b"IntegerVt", canonical.smallint_vt),
            (b"FloatVt", canonical.float_vt),
            (b"WordVt", canonical.word_vt),
            (b"StringVt", canonical.string_vt),
        ];

        runtime.publish_canonical_exports();
        assert_eq!(runtime.export_count(), expected.len());
        for (index, (name, value)) in expected.into_iter().enumerate() {
            let symbol = runtime.export_name_at(index);
            assert_eq!(runtime.symbol_bytes(symbol), Some(name));
            assert_eq!(runtime.import(name), Ok(value));
            assert_eq!(runtime.exported_name(value), runtime.symbol_name(symbol));
        }

        assert_eq!(
            runtime.export(b"ObjectAlias", canonical.object_prototype),
            Ok(canonical.object_prototype)
        );
        assert_eq!(
            // The first insertion remains the reverse-name authority.
            unsafe { std::ffi::CStr::from_ptr(runtime.exported_name(canonical.object_prototype)) }
                .to_bytes(),
            b"Object"
        );
        assert_eq!(
            runtime.export(b"Object", canonical.integer_prototype),
            Ok(canonical.integer_prototype)
        );
        assert_eq!(runtime.export_count(), expected.len() + 1);
        assert_eq!(runtime.import(b"Object"), Ok(canonical.integer_prototype));
        assert_eq!(
            unsafe { std::ffi::CStr::from_ptr(runtime.exported_name(canonical.object_prototype)) }
                .to_bytes(),
            b"ObjectAlias"
        );
    }
}
