//! Metacircular binding, synchronous send, and DNU redispatch.
#![allow(dead_code)] // R2 harness precedes public id_init wiring.

use crate::abi::Oop;
use crate::boundary;
use crate::execution::{ExecutionError, StacklessTarget};
use crate::heap::{Heap, Permanence};
use crate::runtime::{CanonicalObjects, DispatchGlobals, Runtime, with_active};
use crate::id_registries::Symbols;
use crate::values::allocate_empty;
use crate::behaviors::Behaviors;

const MAX_DNU_ARGS: usize = 16;

impl Runtime {
    /// Resolve one method through the metacircular lookup protocol while this
    /// runtime is already active on its owner thread.
    ///
    /// # Safety
    ///
    /// A surrounding `Runtime::enter` must be active, and both arguments must
    /// be live floor objects.
    pub(crate) unsafe fn bind_active(&self, receiver: Oop, selector: Oop) -> Result<Option<crate::abi::id_protocol::NativeEntry>, crate::abi::id_protocol::Outcome> {
        // SAFETY: forwarded active-runtime and live-receiver preconditions.
        let receiver_vtable = unsafe { self.vtable_for_valid_receiver_active(receiver) };
        if receiver_vtable.is_null() {
            return Ok(None);
        }
        // SAFETY: the surrounding enter serializes RuntimeState access.
        let globals = unsafe { self.globals_active() };
        if receiver_vtable == globals.vtable_vt && selector == globals.lookup {
            // SAFETY: the surrounding enter serializes RuntimeState access.
            return Ok(unsafe { self.lookup_native_method_active(globals.vtable_vt, globals.lookup) });
        }

        if self.guarded_default_lookup_eligible(receiver_vtable) {
            // Canonical lookup may reuse an epoch-guarded dictionary answer.
            // No callback/allocation separates this guard from the cache read;
            // ordinary target classification and acquisition still follow.
            return Ok(self.cached_default_lookup(receiver_vtable, selector));
        }

        unsafe { self.bind_through_id(receiver, selector) }
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
        let arguments = if count == 0 {
            &[]
        } else {
            // SAFETY: send_abi requires count live Oops when count is nonzero.
            unsafe { core::slice::from_raw_parts(args, count) }
        };

        // The original operands and outer managed turn are already live.
        // An ineligible access continues through the unmodified metacircular
        // resolver, including native/resumable overrides and their effects.
        if let Some(answer) = self.try_guarded_machine_send(receiver, selector, arguments) {
            return answer;
        }

        let target = self
            .resolve_stackless_target(receiver, selector)
            .unwrap_or_else(|error| boundary::fatal_invariant(error.failure_kind().symbol()));
        if !matches!(target, StacklessTarget::Unbound) {
            // SAFETY: the resolved target and caller-owned argument buffer stay
            // protected by the caller's managed turn.
            return unsafe {
                self.invoke_resolved_send_target(target, receiver, args, count, arguments)
            };
        }

        unsafe { self.send_unbound_abi(receiver, selector, args, count) }
    }

    /// Continue an already resolved missing send. In particular, Machine must
    /// not repeat the original, potentially effectful lookup on its DNU path.
    /// The caller supplies the same live buffer and active-turn contract as send.
    pub(crate) unsafe fn send_unbound_abi(
        &self, receiver: Oop, selector: Oop, args: *const Oop, count: usize,
    ) -> Oop {
        // SAFETY: the caller holds an active owner-thread runtime and turn.
        let globals = unsafe { self.globals_active() };
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
        let handler = self
            .resolve_stackless_target(receiver, globals.does_not_understand)
            .unwrap_or_else(|error| boundary::fatal_invariant(error.failure_kind().symbol()));
        if matches!(handler, StacklessTarget::Unbound) {
            boundary::fatal_invariant("doesNotUnderstand: is unbound");
        }
        let forwarded_count = 1 + forwarded_count;
        // SAFETY: the stack buffer remains live for the complete invocation.
        unsafe {
            self.invoke_resolved_send_target(
                handler,
                receiver,
                forwarded.as_ptr(),
                forwarded_count,
                &forwarded[..forwarded_count],
            )
        }
    }

    unsafe fn invoke_resolved_send_target(
        &self,
        target: StacklessTarget,
        receiver: Oop,
        args: *const Oop,
        count: usize,
        arguments: &[Oop],
    ) -> Oop {
        match target {
            StacklessTarget::Registered { entry } => self
                .enter_acquired_resumable(entry, receiver, arguments)
                .unwrap_or_else(|error| boundary::fatal_invariant(error.failure_kind().symbol())),
            StacklessTarget::Synchronous(method) => {
                // No Rust state reference or collection guard survives this
                // call. The surrounding managed turn protects the
                // dictionary-proven Method for the invocation extent.
                unsafe { self.invoke_native_abi_from_objects(method, receiver, args, count) }
            }
            StacklessTarget::NativeFailure(outcome) => self.native_language_outcome(outcome),
            StacklessTarget::Unbound => {
                boundary::fatal_invariant("resolved send target is unbound")
            }
            StacklessTarget::Resumable { .. } => boundary::fatal_invariant(
                ExecutionError::MissingImplementation
                    .failure_kind()
                    .symbol(),
            ),
        }
    }

    pub(crate) fn send_slice(&self, receiver: Oop, selector: Oop, args: &[Oop]) -> Oop {
        // SAFETY: slice supplies a valid pointer/count pair for this call.
        self.enter_application(|| unsafe {
            self.send_abi(receiver, selector, args.as_ptr(), args.len())
        })
    }
}

pub(crate) use crate::legacy_method::lookup_method as vtable_lookup_method;

pub(crate) unsafe extern "C" fn vtable_allocate(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    if count == 0 || args.is_null() {
        boundary::fatal_invariant("vtable allocate requires pointer-encoded size");
    }
    // SAFETY: the Method ABI promises at least count entries and count > 0.
    let payload_size = unsafe { args.read() }.addr();
    with_active(|runtime| {
        // SAFETY: with_active supplies the runtime installed by Runtime::enter.
        unsafe { runtime.allocate_instance_active(receiver, payload_size) }
    })
}

pub(crate) unsafe extern "C" fn vtable_delegated(receiver: Oop, _: *const Oop, _: usize) -> Oop {
    with_active(|runtime| runtime.create_vtable(receiver))
}

// These are protocols on Vtable, taking an explicit candidate. Queries retain
// the old exact-registry behavior even for null or non-vtable candidates.
pub(crate) unsafe extern "C" fn vtable_owner_of(_: Oop, args: *const Oop, count: usize) -> Oop {
    let Some([vtable]) = (unsafe { crate::abi::borrowed_oop_args(args, count) }) else {
        boundary::fatal_invariant("ownerOf: requires one candidate");
    };
    with_active(|runtime| runtime.vtable_owner(*vtable))
}

unsafe extern "C" fn vtable_owner_put(_: Oop, args: *const Oop, count: usize) -> Oop {
    let Some([vtable, owner]) = (unsafe { crate::abi::borrowed_oop_args(args, count) }) else {
        boundary::fatal_invariant("ownerOf:put: requires a vtable and owner");
    };
    with_active(|runtime| runtime.set_vtable_owner(*vtable, *owner));
    *vtable
}

pub(crate) unsafe extern "C" fn vtable_parent_of(_: Oop, args: *const Oop, count: usize) -> Oop {
    let Some([vtable]) = (unsafe { crate::abi::borrowed_oop_args(args, count) }) else {
        boundary::fatal_invariant("parentOf: requires one candidate");
    };
    with_active(|runtime| runtime.vtable_parent(*vtable))
}

unsafe extern "C" fn vtable_parent_put(_: Oop, args: *const Oop, count: usize) -> Oop {
    let Some([vtable, parent]) = (unsafe { crate::abi::borrowed_oop_args(args, count) }) else {
        boundary::fatal_invariant("parentOf:put: requires a vtable and parent");
    };
    with_active(|runtime| runtime.set_vtable_parent(*vtable, *parent));
    *vtable
}

unsafe extern "C" fn prepare_behavior_mutation(_: Oop, args: *const Oop, count: usize) -> Oop {
    let Some([object]) = (unsafe { crate::abi::borrowed_oop_args(args, count) }) else {
        boundary::fatal_invariant("prepareBehaviorMutationOf: requires one object");
    };
    with_active(|runtime| runtime.prepare_behavior_mutation(*object))
}
unsafe extern "C" fn reassociate_behavior(_: Oop, args: *const Oop, count: usize) -> Oop {
    let Some([object, behavior]) = (unsafe { crate::abi::borrowed_oop_args(args, count) }) else {
        boundary::fatal_invariant("reassociateBehaviorOf:with: requires object and behavior");
    };
    with_active(|runtime| runtime.reassociate_behavior(*object, *behavior));
    *object
}

unsafe extern "C" fn object_does_not_understand(_: Oop, _: *const Oop, _: usize) -> Oop {
    boundary::fatal_invariant("doesNotUnderstand:")
}

pub(crate) unsafe extern "C" fn object_clone(receiver: Oop, _: *const Oop, _: usize) -> Oop {
    with_active(|runtime| runtime.clone_object(receiver))
}

unsafe extern "C" fn object_identical(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    if count == 0 || args.is_null() {
        boundary::fatal_invariant("object == requires one argument");
    }
    // SAFETY: the Method ABI promises at least count entries and count > 0.
    let other = unsafe { args.read() };
    with_active(|runtime| {
        let canonical = runtime.canonical();
        if receiver == other {
            canonical.true_object
        } else {
            canonical.false_object
        }
    })
}

#[derive(Clone, Copy)]
pub(crate) struct BootstrapSelectors {
    pub(crate) lookup: Oop,
    pub(crate) add_method: Oop,
    pub(crate) remove_method: Oop,
    pub(crate) allocate: Oop,
    pub(crate) delegated: Oop,
    pub(crate) does_not_understand: Oop,
    pub(crate) clone: Oop,
    pub(crate) new: Oop,
    pub(crate) identical: Oop,
}

pub(crate) struct Bootstrap {
    pub(crate) runtime: Runtime,
    pub(crate) selectors: BootstrapSelectors,
}



/// Install upper mutation, lifetime and dispatch behavior on an existing Runtime.
/// Object construction and canonical values belong to their owning modules.
pub(crate) fn install_runtime_methods(runtime: &Runtime, protocol: crate::abi::id_protocol::Protocol, selectors: &BootstrapSelectors) {
    let DispatchGlobals { vtable_vt, object_vt, .. } = runtime.globals();
    let has_object_dnu = unsafe { runtime.lookup_native_method_active(object_vt, selectors.does_not_understand).is_some() };
    // Id's typed mutation operations serve construction. Runtime installs its
    // completed mutation/lifetime policy after it has a stable owner address.
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
            (
                object_vt,
                selectors.clone,
                object_clone as crate::abi::Method,
            ),
            (object_vt, selectors.new, object_clone as crate::abi::Method),
        ] {
            if has_object_dnu && selector == selectors.does_not_understand { continue; }
            let entry = runtime.encode_legacy_method(method, selector == selectors.lookup);
            let arguments = [crate::abi::id_protocol::Operand::object(selector),
                crate::abi::id_protocol::Operand::entry(entry)];
            let installed = runtime.enter(|| unsafe {
                protocol.message.unwrap()(protocol.context, vtable, selectors.add_method,
                    arguments.as_ptr(), arguments.len())
            });
            boundary::require_invariant(
                installed.status == crate::abi::id_protocol::LOADED
                    && installed.value.as_entry() == Some(entry),
                "metacircular addMethod changed native entry",
            );
        }
    });

    for (name, method) in [
        (
            b"prepareBehaviorMutationOf:".as_slice(),
            prepare_behavior_mutation as crate::abi::Method,
        ),
        (
            b"reassociateBehaviorOf:with:".as_slice(),
            reassociate_behavior as crate::abi::Method,
        ),
        (
            b"ownerOf:".as_slice(),
            vtable_owner_of as crate::abi::Method,
        ),
        (
            b"ownerOf:put:".as_slice(),
            vtable_owner_put as crate::abi::Method,
        ),
        (
            b"parentOf:".as_slice(),
            vtable_parent_of as crate::abi::Method,
        ),
        (
            b"parentOf:put:".as_slice(),
            vtable_parent_put as crate::abi::Method,
        ),
    ] {
        let selector = runtime.intern(name).expect("static seed selector");
        runtime.add_method(vtable_vt, selector, method);
    }

    let storage_selector = runtime.intern(b"withStorageView:").expect("static storage selector");
    runtime.add_method(vtable_vt, storage_selector, crate::runtime::storage::with_storage_view);

}

#[cfg(test)]
pub(crate) struct DispatchHarness {
    pub(crate) runtime: Runtime,
    pub(crate) meta: Oop,
    pub(crate) object_vtable: Oop,
    pub(crate) lookup: Oop,
    pub(crate) does_not_understand: Oop,
}

#[cfg(test)]
pub(crate) fn build_harness() -> DispatchHarness {
    let mut heap = Heap::new();
    let mut vtables = Behaviors::new();
    let meta = vtables.create(&mut heap, core::ptr::null_mut(), core::ptr::null_mut());
    boundary::require_invariant(heap.set_vtable(meta, meta), "metavtable was not registered");
    let object_vtable = vtables.create(&mut heap, core::ptr::null_mut(), meta);
    let symbol_vtable = vtables.create(&mut heap, object_vtable, meta);
    let mut symbols = crate::id_registries::Symbols::Local(crate::owned::Owned::new(crate::symbol::Symbols::new()));
    let lookup = heap.intern_symbol(&mut symbols, symbol_vtable, b"lookup")
        .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
    let does_not_understand = heap.intern_symbol(&mut symbols, symbol_vtable, b"doesNotUnderstand:")
        .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));

    let dictionaries = vtables.local_pointer().unwrap();
    let runtime = Runtime::new(
        heap,
        vtables,
        symbols,
        None,
        DispatchGlobals {
            vtable_vt: meta,
            object_vt: object_vtable,
            symbol_vt: symbol_vtable,
            lookup,
            does_not_understand,
        },
    );
    // The minimal harness uses Id's actual typed lookup, without Method objects
    // or upper Execution services that this fixture deliberately omits.
    assert_eq!(
        runtime.add_native_method(meta, lookup, crate::abi::id_protocol::NativeEntry::new(
            crate::id_dispatch::vtable_lookup_method, dictionaries.cast())),
        crate::id_registries::AddMethod::Added
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
    use crate::behaviors::AddMethod;
    use core::cell::Cell;

    thread_local! {
        static HOOK_RUNTIME: Cell<*const Runtime> = const { Cell::new(core::ptr::null()) };
        static HOOK_VTABLE: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
        static HOOK_SELECTOR: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
        static HOOK_EXPECTED_RESULT: Cell<usize> = const { Cell::new(0) };
        static HOOK_CALLS: Cell<usize> = const { Cell::new(0) };
        static HOOK_SAW_PRE_MUTATION: Cell<bool> = const { Cell::new(false) };
        static HOOK_READ_REFLECTION: Cell<bool> = const { Cell::new(false) };
        static GC_PROVIDER_ROOT: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
        static GC_TRACE_CHILD: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
        static GC_PROVIDER_CALLS: Cell<usize> = const { Cell::new(0) };
        static GC_TRACE_CALLS: Cell<usize> = const { Cell::new(0) };
    }

    static GC_PRECISE_LAYOUT: [u8; 3] = [
        crate::abi::SLOT_OOP,
        crate::abi::SLOT_RAW,
        crate::abi::SLOT_CODE,
    ];

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

    unsafe extern "C" fn observe_overwrite(vtable: Oop, selector: Oop) {
        HOOK_CALLS.set(HOOK_CALLS.get() + 1);
        let runtime = HOOK_RUNTIME.get();
        if runtime.is_null() {
            return;
        }
        // SAFETY: the test installs its live runtime pointer only for the
        // synchronous mutation calls below.
        let runtime = unsafe { &*runtime };
        let expected = HOOK_EXPECTED_RESULT.get();
        HOOK_SAW_PRE_MUTATION.set(
            vtable == HOOK_VTABLE.get()
                && selector == HOOK_SELECTOR.get()
                && runtime
                    .lookup_method(vtable, selector)
                    .is_some_and(|method| {
                        // SAFETY: the test dictionary contains only the
                        // zero-argument answer methods defined in this module.
                        unsafe { method.to_method()(vtable, core::ptr::null(), 0) }.addr()
                            == expected
                    }),
        );
        HOOK_READ_REFLECTION.set(
            runtime.vtable_count() >= 3
                && runtime.vtable_at(0) == runtime.globals().vtable_vt
                && runtime.vtable_selector_at(vtable, 0) == selector
                && runtime.vtable_parent(vtable) == runtime.globals().object_vt,
        );
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

    unsafe extern "C" fn gc_test_provider(mark: crate::abi::MarkFn) {
        GC_PROVIDER_CALLS.set(GC_PROVIDER_CALLS.get() + 1);
        // SAFETY: the collector supplies a synchronous dynamically scoped mark
        // callback and this provider does not retain it.
        unsafe { mark(GC_PROVIDER_ROOT.get()) };
    }

    unsafe extern "C" fn gc_test_trace(_: Oop, mark: crate::abi::MarkFn) {
        GC_TRACE_CALLS.set(GC_TRACE_CALLS.get() + 1);
        // SAFETY: the collector supplies a synchronous dynamically scoped mark
        // callback and this hook does not retain it.
        unsafe { mark(GC_TRACE_CHILD.get()) };
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
            crate::behaviors::AddMethod::Added
        );

        let result = harness.runtime.enter(|| {
            // SAFETY: this assertion is inside the surrounding enter.
            assert!(unsafe {
                harness
                    .runtime
                    .bind_active(harness.meta, harness.lookup)
                    .unwrap_or_else(|_| panic!("lookup failed")).is_some()
            });
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
    fn overwrite_hook_runs_before_replacement_and_removal_without_state_borrows() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let globals = runtime.globals();
        let vtable = runtime.create_vtable(globals.object_vt);
        let selector = selector(runtime, b"hookOrdering");

        assert_eq!(
            runtime.add_method(vtable, selector, answer_42),
            AddMethod::Added
        );
        HOOK_RUNTIME.set(core::ptr::from_ref(runtime));
        HOOK_VTABLE.set(vtable);
        HOOK_SELECTOR.set(selector);
        HOOK_EXPECTED_RESULT.set(42);
        HOOK_CALLS.set(0);
        HOOK_SAW_PRE_MUTATION.set(false);
        HOOK_READ_REFLECTION.set(false);
        runtime.enter(|| unsafe { birnam_abi::method_observation::replace(runtime.method_observation(), Some(observe_overwrite)) });

        let AddMethod::Overwrote(previous) = runtime.add_method(vtable, selector, answer_77) else {
            panic!("method replacement did not report the previous binding");
        };
        // Function-pointer address equality is not portable (and Miri may
        // materialize distinct shims), so prove the copied binding by calling it.
        assert_eq!(
            unsafe { previous.to_method()(vtable, core::ptr::null(), 0) }.addr(),
            42
        );
        assert_eq!(HOOK_CALLS.get(), 1);
        assert!(HOOK_SAW_PRE_MUTATION.get());
        assert!(HOOK_READ_REFLECTION.get());
        let installed = runtime
            .lookup_method(vtable, selector)
            .expect("replacement binding remains installed");
        assert_eq!(
            unsafe { installed.to_method()(vtable, core::ptr::null(), 0) }.addr(),
            77
        );

        HOOK_EXPECTED_RESULT.set(77);
        HOOK_SAW_PRE_MUTATION.set(false);
        let removed = runtime
            .remove_method(vtable, selector)
            .expect("replacement binding remains removable");
        assert_eq!(
            unsafe { removed.to_method()(vtable, core::ptr::null(), 0) }.addr(),
            77
        );
        assert_eq!(HOOK_CALLS.get(), 2);
        assert!(HOOK_SAW_PRE_MUTATION.get());
        assert_eq!(runtime.lookup_method(vtable, selector), None);

        assert_eq!(runtime.remove_method(vtable, selector), None);
        assert_eq!(HOOK_CALLS.get(), 2);
        runtime.enter(|| unsafe { birnam_abi::method_observation::replace(runtime.method_observation(), None) });
        assert_eq!(
            runtime.add_method(vtable, selector, answer_42),
            AddMethod::Added
        );
        assert!(matches!(
            runtime.add_method(vtable, selector, answer_77),
            AddMethod::Overwrote(_)
        ));
        assert_eq!(HOOK_CALLS.get(), 2);
        HOOK_RUNTIME.set(core::ptr::null());
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
            canonical.nil_vt,
            canonical.bool_vt,
            runtime.string_behavior(),
            runtime.float_behavior(),
            runtime.integer_behavior(),
            runtime.word_behavior(),
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
            assert_eq!(runtime.permanence_of(expected), Some(if expected == runtime.string_behavior() || expected == runtime.integer_behavior() || expected == runtime.float_behavior() || expected == runtime.word_behavior() { Permanence::Collectable } else { Permanence::Permanent }));
            assert_eq!(runtime.payload_size(expected), Some(40));
        }

        // Initial Id names exist before the upper runtime installs methods.
        let selector_names: [&[u8]; 58] = [
            b"lookup",
            b"doesNotUnderstand:",
            b"Symbol",
            b"addMethod",
            b"removeMethod:",
            b"intern:",
            b"loadNative:",
            b"allocate",
            b"allocationAt:",
            b"allocationCount",
            b"delegated",
            b"objectPrototype",
            b"behaviorOf:",
            b"reassociateBehaviorOf:with:",
            b"behaviorCount",
            b"behaviorAt:",
            b"selectorAt:in:",
            b"parentOf:",
            b"parentOf:put:",
            b"allocationSizeOf:",
            b"identityOf:",
            b"symbolBytesOf:",
            b"rootsDo:",
            b"allocationsDo:",
            b"referencesOf:do:",
            b"retireIndexesFor:",
            b"detachAllocationAt:",
            b"symbolNamed:",
            b"symbolCount",
            b"symbolAt:",
            b"symbolNameBytes",
            b"behaviorEpoch",
            b"isBehavior:",
            b"behaviorView",
            b"selectorsIn:",
            b"bindingOf:in:",
            b"lookup:in:",
            b"bindingsIn:do:",
            b"addMethod:to:as:",
            b"removeMethod:from:",
            b"addWeakBinding:to:as:",
            b"newBehaviorWithParent:",
            b"bind:to:",
            b"clone",
            b"new",
            b"==",
            b"prepareBehaviorMutationOf:",
            b"ownerOf:",
            b"ownerOf:put:",
            b"withStorageView:",
            b"String",
            b"StringVt",
            b"Float",
            b"FloatVt",
            b"Integer",
            b"IntegerVt",
            b"Word",
            b"WordVt",
        ];
        assert_eq!(runtime.symbol_count(), selector_names.len());
        for (index, name) in selector_names.iter().copied().enumerate() {
            let symbol = runtime.symbol_at(index);
            assert_eq!(runtime.symbol_bytes(symbol), Some(name));
            assert_eq!(
                runtime.vtable_selector_at(canonical.symbol_list, index),
                symbol
            );
            assert_eq!(runtime.checked_vtable_of(symbol), globals.symbol_vt);
            assert_eq!(runtime.permanence_of(symbol), Some(Permanence::Collectable));
        }
        assert!(
            runtime
                .vtable_selector_at(canonical.symbol_list, selector_names.len())
                .is_null()
        );
        assert_eq!(runtime.export_count(), 8);
        assert_eq!(runtime.import(b"Integer").unwrap(), runtime.integer_prototype());

        let meta_selectors = [
            bootstrap.selectors.lookup,
            bootstrap.selectors.add_method,
            bootstrap.selectors.remove_method,
            runtime.intern(b"intern:").unwrap(),
            runtime.intern(b"loadNative:").unwrap(),
            bootstrap.selectors.allocate,
            runtime.intern(b"allocationAt:").unwrap(),
            runtime.intern(b"allocationCount").unwrap(),
            bootstrap.selectors.delegated,
            runtime.intern(b"objectPrototype").unwrap(),
            runtime.intern(b"behaviorOf:").unwrap(),
            runtime.intern(b"reassociateBehaviorOf:with:").unwrap(),
            runtime.intern(b"behaviorCount").unwrap(),
            runtime.intern(b"behaviorAt:").unwrap(),
            runtime.intern(b"selectorAt:in:").unwrap(),
            runtime.intern(b"parentOf:").unwrap(),
            runtime.intern(b"parentOf:put:").unwrap(),
            runtime.intern(b"allocationSizeOf:").unwrap(),
            runtime.intern(b"identityOf:").unwrap(),
            runtime.intern(b"symbolBytesOf:").unwrap(),
            runtime.intern(b"rootsDo:").unwrap(),
            runtime.intern(b"allocationsDo:").unwrap(),
            runtime.intern(b"referencesOf:do:").unwrap(),
            runtime.intern(b"retireIndexesFor:").unwrap(),
            runtime.intern(b"detachAllocationAt:").unwrap(),
            runtime.intern(b"symbolNamed:").unwrap(),
            runtime.intern(b"symbolCount").unwrap(),
            runtime.intern(b"symbolAt:").unwrap(),
            runtime.intern(b"symbolNameBytes").unwrap(),
            runtime.intern(b"behaviorEpoch").unwrap(),
            runtime.intern(b"isBehavior:").unwrap(),
            runtime.intern(b"behaviorView").unwrap(),
            runtime.intern(b"selectorsIn:").unwrap(),
            runtime.intern(b"bindingOf:in:").unwrap(),
            runtime.intern(b"lookup:in:").unwrap(),
            runtime.intern(b"bindingsIn:do:").unwrap(),
            runtime.intern(b"addMethod:to:as:").unwrap(),
            runtime.intern(b"removeMethod:from:").unwrap(),
            runtime.intern(b"addWeakBinding:to:as:").unwrap(),
            runtime.intern(b"newBehaviorWithParent:").unwrap(),
            runtime.intern(b"bind:to:").unwrap(),
            runtime.intern(b"prepareBehaviorMutationOf:").unwrap(),
            runtime.intern(b"ownerOf:").unwrap(),
            runtime.intern(b"ownerOf:put:").unwrap(),
            runtime.intern(b"withStorageView:").unwrap(),
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
        let object_selectors = [
            bootstrap.selectors.does_not_understand,
            bootstrap.selectors.clone,
            bootstrap.selectors.new,
            bootstrap.selectors.identical,
        ];
        for (index, selector) in object_selectors.into_iter().enumerate() {
            assert_eq!(
                runtime.vtable_selector_at(globals.object_vt, index),
                selector
            );
        }
        assert!(
            runtime
                .vtable_selector_at(globals.object_vt, object_selectors.len())
                .is_null()
        );
        assert_eq!(
            runtime.vtable_selector_at(runtime.string_behavior(), 0),
            bootstrap.selectors.clone
        );
        assert_eq!(
            runtime.vtable_selector_at(runtime.string_behavior(), 1),
            bootstrap.selectors.new
        );
        assert!(runtime.vtable_selector_at(runtime.string_behavior(), 2).is_null());
    }

    #[test]
    fn bootstrap_clone_identity_and_private_vtables_match_the_prototype_model() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let globals = runtime.globals();
        let canonical = runtime.canonical();

        let object = runtime.allocate_instance(globals.object_vt, 9);
        let extent = runtime.payload_size(object).unwrap();
        // SAFETY: object is a live floor block with extent writable bytes.
        let payload = unsafe { std::slice::from_raw_parts_mut(object.cast::<u8>(), extent) };
        for (index, byte) in payload.iter_mut().enumerate() {
            *byte = (index as u8).wrapping_mul(19).wrapping_add(7);
        }
        let expected_payload = payload.to_vec();

        runtime.enter(|| {
            let clone = runtime.send_slice(object, bootstrap.selectors.clone, &[]);
            let new = runtime.send_slice(object, bootstrap.selectors.new, &[]);
            for copy in [clone, new] {
                assert_ne!(copy, object);
                assert_eq!(runtime.checked_vtable_of(copy), globals.object_vt);
                // SAFETY: source and copy are live blocks with the same extent.
                let copied =
                    unsafe { std::slice::from_raw_parts(copy.cast::<u8>().cast_const(), extent) };
                assert_eq!(copied, expected_payload);
            }
            assert_eq!(
                runtime.send_slice(object, bootstrap.selectors.identical, &[object]),
                canonical.true_object
            );
            assert_eq!(
                runtime.send_slice(object, bootstrap.selectors.identical, &[clone]),
                canonical.false_object
            );

            let original = runtime.allocate_string(runtime.string_behavior(), b"owned\0\xffbytes");
            let copied = runtime.send_slice(original, bootstrap.selectors.clone, &[]);
            assert_ne!(copied, original);
            assert_eq!(runtime.checked_vtable_of(copied), runtime.string_behavior());
            assert_eq!(runtime.string_bytes(copied), runtime.string_bytes(original));
            assert_ne!(
                runtime.string_bytes(copied).unwrap().as_ptr(),
                runtime.string_bytes(original).unwrap().as_ptr()
            );
            assert_eq!(runtime.has_finalizer(copied), Some(true));
        });

        assert_eq!(
            runtime.ensure_private_vtable(runtime.integer_prototype()),
            runtime.integer_behavior()
        );
        assert_eq!(
            runtime.ensure_private_vtable(globals.object_vt),
            globals.object_vt
        );
        assert_eq!(
            runtime.ensure_private_vtable(canonical.nil),
            canonical.nil_vt
        );
        assert_eq!(
            runtime.ensure_private_vtable(canonical.true_object),
            canonical.bool_vt
        );

        let sibling = runtime.allocate_instance(globals.object_vt, 0);
        let private = runtime.ensure_private_vtable(object);
        assert_ne!(private, globals.object_vt);
        assert_eq!(runtime.ensure_private_vtable(object), private);
        assert_eq!(runtime.checked_vtable_of(object), private);
        assert_eq!(runtime.checked_vtable_of(sibling), globals.object_vt);
        assert_eq!(runtime.vtable_parent(private), globals.object_vt);
        assert_eq!(runtime.vtable_owner(private), object);

        let false_private = runtime.ensure_private_vtable(canonical.false_object);
        assert_ne!(false_private, canonical.bool_vt);
        assert_eq!(runtime.vtable_parent(false_private), canonical.bool_vt);
        assert_eq!(runtime.vtable_owner(false_private), canonical.false_object);
        assert_eq!(
            runtime.checked_vtable_of(canonical.true_object),
            canonical.bool_vt
        );
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
            runtime.integer_behavior(),
            runtime.string_behavior(),
            runtime.float_behavior(),
            canonical.nil_vt,
            canonical.bool_vt,
            canonical.nil,
            canonical.true_object,
            canonical.false_object,
            canonical.object_prototype,
            runtime.integer_prototype(),
            runtime.float_prototype(),
            runtime.word_prototype(),
            runtime.string_prototype(),
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
            (runtime.integer_prototype(), runtime.integer_behavior()),
            (runtime.float_prototype(), runtime.float_behavior()),
            (runtime.word_prototype(), runtime.word_behavior()),
            (runtime.string_prototype(), runtime.string_behavior()),
            (canonical.symbol_prototype, globals.symbol_vt),
        ] {
            assert_eq!(runtime.checked_vtable_of(object), vtable);
        }
        for (vtable, owner) in [
            (canonical.nil_vt, canonical.nil),
            (canonical.bool_vt, canonical.true_object),
            (globals.object_vt, canonical.object_prototype),
            (runtime.integer_behavior(), runtime.integer_prototype()),
            (runtime.float_behavior(), runtime.float_prototype()),
            (runtime.word_behavior(), runtime.word_prototype()),
            (runtime.string_behavior(), runtime.string_prototype()),
            (globals.symbol_vt, canonical.symbol_prototype),
        ] {
            assert_eq!(runtime.vtable_owner(vtable), owner);
        }
        for object in [
            canonical.nil,
            canonical.true_object,
            canonical.false_object,
            canonical.object_prototype,
        ] {
            assert_eq!(runtime.permanence_of(object), Some(Permanence::Permanent));
        }
        for object in [
            runtime.integer_prototype(),
            runtime.float_prototype(),
            runtime.word_prototype(),
            runtime.string_prototype(),
            canonical.symbol_prototype,
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
        assert_eq!(runtime.payload_size(runtime.integer_prototype()), Some(8));
        assert_eq!(runtime.payload_size(runtime.float_prototype()), Some(8));
        assert_eq!(runtime.payload_size(runtime.word_prototype()), Some(8));
        assert_eq!(runtime.payload_size(runtime.string_prototype()), Some(16));
        assert_eq!(runtime.payload_size(canonical.symbol_prototype), Some(8));
        assert_eq!(runtime.integer_value(runtime.integer_prototype()), 0);
        assert_eq!(runtime.float_value(runtime.float_prototype()), 0.0);
        assert_eq!(runtime.word_value(runtime.word_prototype()), 0);
        assert_eq!(
            runtime.string_bytes(runtime.string_prototype()),
            Some(&b""[..])
        );
        assert_eq!(
            runtime.symbol_bytes(canonical.symbol_prototype),
            Some(&b"Symbol"[..])
        );
    }

    #[test]
    fn floor_mark_roots_cover_exports_without_rooting_weak_dynamic_vtables() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let globals = runtime.globals();
        let rooted = runtime.allocate_instance(globals.object_vt, size_of::<Oop>());
        let child = runtime.allocate_instance(globals.object_vt, 0);
        let unrooted = runtime.allocate_instance(globals.object_vt, 0);
        // SAFETY: rooted owns one writable, pointer-aligned Oop payload slot.
        unsafe { rooted.cast::<Oop>().write(child) };
        runtime.export(b"GcRoot", rooted).unwrap();

        let private_owner = runtime.allocate_instance(globals.object_vt, 0);
        let private_vtable = runtime.ensure_private_vtable(private_owner);
        assert_eq!(runtime.vtable_owner(private_vtable), private_owner);
        assert_eq!(
            runtime.permanence_of(private_vtable),
            Some(Permanence::Collectable)
        );

        runtime.gc_mark_floor_roots_for_test();

        assert_eq!(runtime.is_marked_for_test(rooted), Some(true));
        assert_eq!(runtime.is_marked_for_test(child), Some(true));
        assert_eq!(runtime.is_marked_for_test(unrooted), Some(false));
        assert_eq!(runtime.is_marked_for_test(private_owner), Some(false));
        assert_eq!(runtime.is_marked_for_test(private_vtable), Some(false));
        assert_eq!(
            runtime.is_marked_for_test(runtime.intern(b"GcRoot").unwrap()),
            Some(true),
            "the export authority roots its name Symbol"
        );
    }

    #[test]
    fn foreign_provider_and_reachable_instance_hook_extend_the_mark_graph() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let globals = runtime.globals();
        let hooked_vtable = runtime.create_vtable(globals.object_vt);
        runtime.set_trace_hook(hooked_vtable, gc_test_trace);
        let provider_root = runtime.allocate_instance(hooked_vtable, 0);
        let trace_child = runtime.allocate_instance(globals.object_vt, 0);
        let unreachable = runtime.allocate_instance(hooked_vtable, 0);
        GC_PROVIDER_ROOT.set(provider_root);
        GC_TRACE_CHILD.set(trace_child);
        GC_PROVIDER_CALLS.set(0);
        GC_TRACE_CALLS.set(0);
        runtime.gc_register_root_provider(gc_test_provider);

        runtime.gc_mark_floor_roots_for_test();

        assert_eq!(GC_PROVIDER_CALLS.get(), 1);
        assert_eq!(GC_TRACE_CALLS.get(), 1);
        assert_eq!(runtime.is_marked_for_test(provider_root), Some(true));
        assert_eq!(runtime.is_marked_for_test(trace_child), Some(true));
        assert_eq!(runtime.is_marked_for_test(unreachable), Some(false));
        GC_PROVIDER_ROOT.set(core::ptr::null_mut());
        GC_TRACE_CHILD.set(core::ptr::null_mut());
    }

    #[test]
    fn precise_layout_traces_only_oop_prefix_and_rest_slots() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let globals = runtime.globals();
        let precise_vtable = runtime.create_vtable(globals.object_vt);
        runtime.set_layout(
            precise_vtable,
            GC_PRECISE_LAYOUT.as_ptr(),
            GC_PRECISE_LAYOUT.len(),
            crate::abi::SLOT_OOP,
        );
        let root = runtime.allocate_instance(precise_vtable, 4 * size_of::<Oop>());
        let oop_child = runtime.allocate_instance(globals.object_vt, 0);
        let raw_false_positive = runtime.allocate_instance(globals.object_vt, 0);
        let code_false_positive = runtime.allocate_instance(globals.object_vt, 0);
        let rest_child = runtime.allocate_instance(globals.object_vt, 0);
        // SAFETY: root owns four writable, pointer-aligned Oop-sized words. Raw
        // and code slots intentionally contain pointer-shaped test bits.
        unsafe {
            let words = root.cast::<Oop>();
            words.add(0).write(oop_child);
            words.add(1).write(raw_false_positive);
            words.add(2).write(code_false_positive);
            words.add(3).write(rest_child);
        }
        runtime.export(b"PreciseGcRoot", root).unwrap();
        runtime.gc_set_precise(true);

        runtime.gc_mark_floor_roots_for_test();

        assert_eq!(runtime.is_marked_for_test(root), Some(true));
        assert_eq!(runtime.is_marked_for_test(oop_child), Some(true));
        assert_eq!(runtime.is_marked_for_test(rest_child), Some(true));
        assert_eq!(runtime.is_marked_for_test(raw_false_positive), Some(false));
        assert_eq!(runtime.is_marked_for_test(code_false_positive), Some(false));

        runtime.gc_set_precise(false);
        runtime.gc_mark_floor_roots_for_test();
        assert_eq!(runtime.is_marked_for_test(raw_false_positive), Some(true));
        assert_eq!(runtime.is_marked_for_test(code_false_positive), Some(true));
    }

    #[test]
    fn precise_mode_keeps_unknown_layouts_conservative() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let globals = runtime.globals();
        let unknown_vtable = runtime.create_vtable(globals.object_vt);
        let root = runtime.allocate_instance(unknown_vtable, size_of::<Oop>());
        let child = runtime.allocate_instance(globals.object_vt, 0);
        // SAFETY: root owns one writable, pointer-aligned Oop payload slot.
        unsafe { root.cast::<Oop>().write(child) };
        runtime.export(b"UnknownLayoutGcRoot", root).unwrap();
        runtime.gc_set_precise(true);

        runtime.gc_mark_floor_roots_for_test();

        assert_eq!(runtime.is_marked_for_test(root), Some(true));
        assert_eq!(runtime.is_marked_for_test(child), Some(true));
        assert_eq!(
            runtime.slot_kind(unknown_vtable, 0),
            crate::abi::SLOT_UNKNOWN
        );
    }

    #[test]
    fn bootstrap_primitives_support_metacircular_dispatch_removal_and_dnu() {
        use crate::abi::id_protocol::{self as native, Operand};
        let initial = crate::initial_objects::InitialObjects::new();
        let protocol = initial.protocol.protocol;
        let bootstrap = build_bootstrap_from(initial);
        let runtime = &bootstrap.runtime;
        runtime.install_id_runtime_extensions();
        let globals = runtime.globals();
        let answer = selector(runtime, b"bootstrapAnswer");
        let missing = selector(runtime, b"bootstrapMissing:");
        let native_send = |receiver, selector, args: &[Operand]| {
            let outcome = runtime.enter(|| unsafe {
                protocol.message.unwrap()(protocol.context, receiver, selector, args.as_ptr(), args.len())
            });
            assert_eq!(outcome.status, native::LOADED);
            outcome.value
        };
        let child = native_send(globals.object_vt, bootstrap.selectors.delegated, &[]).as_object().unwrap();
        let method = runtime.encode_legacy_method(answer_77, false);
        let install = [Operand::object(answer), Operand::entry(method)];
        assert_eq!(native_send(child, bootstrap.selectors.add_method, &install).as_entry(), Some(method));
        let receiver = native_send(child, bootstrap.selectors.allocate, &[Operand::word(0)]).as_object().unwrap();
        assert_eq!(native_send(receiver, answer, &[]).as_object().unwrap().addr(), 77);
        let dnu_install = [Operand::object(bootstrap.selectors.does_not_understand),
            Operand::entry(runtime.encode_legacy_method(dnu_count, false))];
        native_send(child, bootstrap.selectors.add_method, &dnu_install);
        assert_eq!(native_send(receiver, missing, &[Operand::object(core::ptr::null_mut()); 20]).as_object().unwrap().addr(), 17);
        assert_eq!(native_send(child, bootstrap.selectors.remove_method, &[Operand::object(answer)]).as_entry(), Some(method));
        assert_eq!(native_send(receiver, answer, &[]).as_object().unwrap().addr(), 1);
    }

    #[test]
    fn canonical_exports_preserve_order_aliases_and_same_name_rebinding() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let globals = runtime.globals();
        let canonical = runtime.canonical();
        let expected: [(&[u8], Oop); 15] = [
            (b"String", runtime.string_prototype()),
            (b"StringVt", runtime.string_behavior()),
            (b"Float", runtime.float_prototype()),
            (b"FloatVt", runtime.float_behavior()),
            (b"Integer", runtime.integer_prototype()),
            (b"IntegerVt", runtime.integer_behavior()),
            (b"Word", runtime.word_prototype()),
            (b"WordVt", runtime.word_behavior()),
            (b"Object", canonical.object_prototype),
            (b"Boolean", canonical.true_object),
            (b"False", canonical.false_object),
            (b"Vtable", globals.vtable_vt),
            (b"Symbol", canonical.symbol_prototype),
            (b"SymbolVt", globals.symbol_vt),
            (b"ObjectVt", globals.object_vt),
        ];

        runtime.publish_canonical_exports();
        assert_eq!(runtime.export_count(), expected.len());
        for (index, (name, value)) in expected.iter().copied().enumerate() {
            let symbol = runtime.export_name_at(index);
            assert_eq!(runtime.symbol_bytes(symbol), Some(name));
            assert_eq!(
                runtime.vtable_selector_at(canonical.environment, index),
                symbol
            );
            assert_eq!(runtime.import(name), Ok(value));
            assert_eq!(runtime.exported_name(value), runtime.symbol_name(symbol));
        }
        assert!(
            runtime
                .vtable_selector_at(canonical.environment, expected.len())
                .is_null()
        );

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
            runtime.export(b"Object", runtime.integer_prototype()),
            Ok(runtime.integer_prototype())
        );
        assert_eq!(runtime.export_count(), expected.len() + 1);
        assert_eq!(runtime.import(b"Object"), Ok(runtime.integer_prototype()));
        assert_eq!(
            unsafe { std::ffi::CStr::from_ptr(runtime.exported_name(canonical.object_prototype)) }
                .to_bytes(),
            b"ObjectAlias"
        );
    }
}
