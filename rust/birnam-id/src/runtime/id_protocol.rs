//! Completed Runtime policy installed as ordinary extensions of Id methods.
use super::Runtime;
use crate::abi::{
    Oop,
    id_protocol::{self as native, NativeEntry, Operand, Outcome},
};
use core::ffi::c_void;

impl Runtime {
    /// The existing language send boundary supplies only Object operands.
    /// Legacy entries keep their old calling convention during migration;
    /// native modules receive the exact selected function/context pair.
    pub(crate) fn invoke_native_from_objects(
        &self,
        entry: NativeEntry,
        receiver: Oop,
        arguments: &[Oop],
    ) -> Oop {
        if let Some(legacy) = crate::legacy_method::try_decode(entry) {
            let args = if arguments.is_empty() {
                core::ptr::null()
            } else {
                arguments.as_ptr()
            };
            return unsafe { legacy.to_method()(receiver, args, arguments.len()) };
        }
        let arguments: Vec<_> = arguments.iter().copied().map(Operand::object).collect();
        let outcome = unsafe {
            entry.method.expect("selected native entry has a method")(
                entry.context,
                receiver,
                arguments.as_ptr(),
                arguments.len(),
            )
        };
        self.native_language_outcome(outcome)
    }

    pub(crate) fn native_language_outcome(&self, outcome: Outcome) -> Oop {
        if outcome.status == native::LOADED {
            if let Some(object) = outcome.value.as_object() {
                return object;
            }
            if outcome.value.kind() == native::UNIT {
                return self.canonical().nil;
            }
        }
        // Native scalars/entries/borrowed bytes are not disguised as language
        // objects. A module can expose them through an appropriate value wrapper.
        let kind = if outcome.status == native::LOADED {
            b"nativeResultRequiresWrapper".as_slice()
        } else if outcome.status == native::INVALID_REQUEST {
            b"nativeInvalidRequest".as_slice()
        } else {
            b"nativeFailure".as_slice()
        };
        let error = self
            .import(b"Error")
            .ok()
            .filter(|value| !value.is_null() && *value != self.canonical().nil)
            .unwrap_or_else(|| {
                crate::boundary::fatal_invariant("native language outcome requires Error")
            });
        let kind = self.intern(kind).unwrap();
        let _kind_root = super::ScopedOopRootGuard::new(self, kind);
        let selector = self.intern(b"kind:").unwrap();
        self.send_slice(error, selector, &[kind])
    }

    pub(crate) fn install_id_runtime_extensions(&self) {
        self.assert_owner();
        let roots = unsafe { (&*self.state.get()).root_registry.native_owner() };
        if let Some(roots) = roots { crate::root_registry::install_admission(roots, self); }
        let lifetime = unsafe { (&*self.state.get()).vtables.lifetime_owner() };
        if let Some(owner) = lifetime { crate::lifetime::install_access(owner, self); }
        let state = unsafe { &mut *self.state.get() };
        let Some(id) = state.id_protocol.as_ref() else {
            return;
        };
        let protocol = id.protocol;
        let context = core::ptr::from_ref(self).cast_mut().cast();
        let dictionaries =
            unsafe { &mut *crate::behaviors::Behaviors::dictionaries_ptr(&mut state.vtables) };
        dictionaries.add_method(
            protocol.receiver,
            protocol.intern,
            NativeEntry::new(intern, context),
        );
        dictionaries.add_method(
            protocol.receiver,
            protocol.load,
            NativeEntry::new(load, context),
        );
        let add = state.symbols.existing(b"addMethod").unwrap();
        let remove = state.symbols.existing(b"removeMethod:").unwrap();
        dictionaries.add_method(
            protocol.receiver,
            add,
            NativeEntry::new(add_method, context),
        );
        dictionaries.add_method(
            protocol.receiver,
            remove,
            NativeEntry::new(remove_method, context),
        );
        for (name, method) in [
            (b"allocate".as_slice(), allocate as native::Method),
            (b"delegated".as_slice(), delegated as native::Method),
            (b"prepareBehaviorMutationOf:".as_slice(), prepare_behavior as native::Method),
            (b"reassociateBehaviorOf:with:".as_slice(), reassociate_behavior as native::Method),
            (b"ownerOf:".as_slice(), owner_of as native::Method),
            (b"ownerOf:put:".as_slice(), owner_put as native::Method),
            (b"parentOf:".as_slice(), parent_of as native::Method),
            (b"parentOf:put:".as_slice(), parent_put as native::Method),
        ] {
            let selector = state.symbols.existing(name).unwrap();
            dictionaries.add_method(
                protocol.receiver,
                selector,
                NativeEntry::new(method, context),
            );
        }
    }
}

pub(crate) unsafe extern "C" fn allocate(
    context: *mut c_void,
    behavior: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([size]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let Some(size) = size
        .as_word()
        .filter(|size| crate::native_id::objects::valid_size(*size))
    else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let runtime = unsafe { &*context.cast::<Runtime>() };
    runtime.assert_owner();
    if !unsafe { (&*runtime.state.get()).vtables.is_registered(behavior) } {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    }
    let object = runtime.enter_application(|| {
        let _behavior = super::ScopedOopRootGuard::new(runtime, behavior);
        runtime.allocate_instance(behavior, size)
    });
    Outcome::loaded(Operand::object(object))
}

pub(crate) unsafe extern "C" fn delegated(
    context: *mut c_void,
    parent: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    if count != 0 {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    }
    let runtime = unsafe { &*context.cast::<Runtime>() };
    runtime.assert_owner();
    if !unsafe { (&*runtime.state.get()).vtables.is_registered(parent) } {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    }
    let behavior = runtime.enter_application(|| {
        let _parent = super::ScopedOopRootGuard::new(runtime, parent);
        runtime.create_vtable(parent)
    });
    Outcome::loaded(Operand::object(behavior))
}

// Complete upper policy carries its Runtime explicitly. Raw Id callers need no
// thread-local active Runtime merely to invoke these installed native entries.
unsafe extern "C" fn prepare_behavior(context: *mut c_void, _: Oop, args: *const Operand, count: usize) -> Outcome {
    let Some([object]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else { return Outcome::failed(native::INVALID_REQUEST, 0); };
    let Some(object) = object.as_object() else { return Outcome::failed(native::INVALID_REQUEST, 0); };
    let runtime = unsafe { &*context.cast::<Runtime>() };
    if runtime.checked_vtable_of(object).is_null() { return Outcome::failed(native::INVALID_REQUEST, 0); }
    let behavior = runtime.enter_application(|| {
        let _object = super::ScopedOopRootGuard::new(runtime, object);
        runtime.prepare_behavior_mutation(object)
    });
    Outcome::loaded(Operand::object(behavior))
}
unsafe extern "C" fn reassociate_behavior(context: *mut c_void, _: Oop, args: *const Operand, count: usize) -> Outcome {
    let Some([object, behavior]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else { return Outcome::failed(native::INVALID_REQUEST, 0); };
    let (Some(object), Some(behavior)) = (object.as_object(), behavior.as_object()) else { return Outcome::failed(native::INVALID_REQUEST, 0); };
    let runtime = unsafe { &*context.cast::<Runtime>() };
    if runtime.payload_size(object).is_none() || runtime.payload_size(behavior).is_none() { return Outcome::failed(native::INVALID_REQUEST, 0); }
    runtime.reassociate_behavior(object, behavior);
    Outcome::loaded(Operand::object(object))
}
unsafe fn relation(context: *mut c_void, args: *const Operand, count: usize, owner: bool) -> Outcome {
    let Some([behavior]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else { return Outcome::failed(native::INVALID_REQUEST, 0); };
    let Some(behavior) = behavior.as_object() else { return Outcome::failed(native::INVALID_REQUEST, 0); };
    let runtime = unsafe { &*context.cast::<Runtime>() };
    // Preserve the legacy all-Oop query's null sentinel until its consumers
    // migrate. Canonical owner records still distinguish null from actual Nil.
    Outcome::loaded(Operand::object(if owner { runtime.vtable_owner(behavior) } else { runtime.vtable_parent(behavior) }))
}
unsafe extern "C" fn owner_of(context: *mut c_void, _: Oop, args: *const Operand, count: usize) -> Outcome { unsafe { relation(context, args, count, true) } }
unsafe extern "C" fn parent_of(context: *mut c_void, _: Oop, args: *const Operand, count: usize) -> Outcome { unsafe { relation(context, args, count, false) } }
unsafe fn put_relation(context: *mut c_void, args: *const Operand, count: usize, owner: bool) -> Outcome {
    let Some([behavior, value]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else { return Outcome::failed(native::INVALID_REQUEST, 0); };
    let Some(behavior) = behavior.as_object() else { return Outcome::failed(native::INVALID_REQUEST, 0); };
    let value = if value.kind() == native::UNIT { core::ptr::null_mut() }
        else { let Some(value) = value.as_object() else { return Outcome::failed(native::INVALID_REQUEST, 0); }; value };
    let runtime = unsafe { &*context.cast::<Runtime>() };
    runtime.assert_owner();
    let valid = unsafe { (*runtime.state.get()).vtables.is_registered(behavior)
        && (owner || value.is_null() || (*runtime.state.get()).vtables.is_registered(value)) };
    if !valid { return Outcome::failed(native::INVALID_REQUEST, 0); }
    if owner { runtime.set_vtable_owner(behavior, value); } else { runtime.set_vtable_parent(behavior, value); }
    Outcome::loaded(Operand::object(behavior))
}
unsafe extern "C" fn owner_put(context: *mut c_void, _: Oop, args: *const Operand, count: usize) -> Outcome { unsafe { put_relation(context, args, count, true) } }
unsafe extern "C" fn parent_put(context: *mut c_void, _: Oop, args: *const Operand, count: usize) -> Outcome { unsafe { put_relation(context, args, count, false) } }

unsafe extern "C" fn intern(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([argument]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let Some(bytes) = argument.as_bytes() else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let Some(bytes) = (unsafe { crate::abi::borrowed_bytes(bytes.pointer, bytes.length) })
        .filter(|bytes| !bytes.contains(&0))
    else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let runtime = unsafe { &*context.cast::<Runtime>() };
    let symbol =
        runtime.enter_application(|| runtime.intern(bytes).expect("validated Symbol name"));
    Outcome::loaded(Operand::object(symbol))
}
unsafe extern "C" fn load(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some(arguments) = (unsafe { crate::abi::borrowed_slice(args, count) }) else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let Some((path, arguments)) = arguments.split_first() else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let Some(path) = path.as_bytes() else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let Some(path) = (unsafe { crate::abi::borrowed_bytes(path.pointer, path.length) }) else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let runtime = unsafe { &*context.cast::<Runtime>() };
    let protocol = unsafe {
        (&*runtime.state.get())
            .id_protocol
            .as_ref()
            .unwrap()
            .protocol
    };
    let mut result = Outcome::failed(native::INITIALIZER_FAILED, 0);
    runtime.enter_application(|| {
        result = unsafe { crate::abi::native_module::load(&protocol, path, arguments) };
        // enter_application retains an outward Object through its checkpoint.
        result.value.as_object().unwrap_or(runtime.canonical().nil)
    });
    result
}

pub(crate) unsafe extern "C" fn add_method(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([selector, entry]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let (Some(selector), Some(entry)) = (
        selector.as_object(),
        entry.as_entry().filter(|entry| entry.method.is_some()),
    ) else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let runtime = unsafe { &*context.cast::<Runtime>() };
    runtime.enter_application(|| {
        runtime.add_native_method(receiver, selector, entry);
        receiver
    });
    Outcome::loaded(Operand::entry(entry))
}
pub(crate) unsafe extern "C" fn remove_method(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([selector]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let Some(selector) = selector.as_object() else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let runtime = unsafe { &*context.cast::<Runtime>() };
    let mut removed = None;
    runtime.enter_application(|| {
        removed = runtime.remove_native_method(receiver, selector);
        receiver
    });
    Outcome::loaded(removed.map(Operand::entry).unwrap_or_else(Operand::unit))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_construction_survives_adoption_and_uses_gc_accounting_afterward() {
        let initial = crate::initial_objects::InitialObjects::new();
        let id = initial.protocol.protocol;
        let call = |receiver, name: &[u8], arguments: &[Operand]| unsafe {
            let symbol =
                id.message.unwrap()(id.context, id.receiver, id.intern, &Operand::bytes(name), 1);
            assert_eq!(symbol.status, native::LOADED);
            id.message.unwrap()(
                id.context,
                receiver,
                symbol.value.as_object().unwrap(),
                arguments.as_ptr(),
                arguments.len(),
            )
        };
        let object = |outcome: Outcome| {
            assert_eq!(outcome.status, native::LOADED);
            outcome.value.as_object().unwrap()
        };
        let prototype = object(call(id.receiver, b"objectPrototype", &[]));
        let object_behavior = object(call(
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(prototype)],
        ));
        let behavior = object(call(object_behavior, b"delegated", &[]));
        let early = object(call(behavior, b"allocate", &[Operand::word(16)]));
        let child = object(call(object_behavior, b"allocate", &[Operand::word(0)]));
        unsafe {
            early.cast::<Oop>().write(child);
            early.cast::<usize>().add(1).write(83);
        }
        let identity = call(id.receiver, b"identityOf:", &[Operand::object(early)])
            .value
            .as_word()
            .unwrap();
        assert_eq!(
            call(id.receiver, b"allocationSizeOf:", &[Operand::object(early)])
                .value
                .as_word(),
            Some(16)
        );
        assert_eq!(
            object(call(id.receiver, b"behaviorOf:", &[Operand::object(early)])),
            behavior
        );
        for size in [usize::MAX, isize::MAX as usize] {
            assert_eq!(
                call(behavior, b"allocate", &[Operand::word(size)]).status,
                native::INVALID_REQUEST
            );
        }
        assert_eq!(
            call(behavior, b"allocate", &[Operand::object(prototype)]).status,
            native::INVALID_REQUEST
        );
        assert_eq!(
            call(prototype, b"allocationSizeOf:", &[]).status,
            native::UNBOUND
        );
        assert_eq!(
            call(
                id.receiver,
                b"allocationSizeOf:",
                &[Operand::object(core::ptr::null_mut())]
            )
            .value
            .kind(),
            native::UNIT
        );

        let bootstrap = Box::new(crate::dispatch::build_bootstrap_from(initial));
        let runtime = &bootstrap.runtime;
        runtime.install_id_runtime_extensions();
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        runtime.enable_symbol_compaction_for_test();
        // The upper owner supplies its payload description before collection.
        let layout = [crate::abi::SLOT_OOP, crate::abi::SLOT_RAW];
        runtime.set_layout(
            behavior,
            layout.as_ptr(),
            layout.len(),
            crate::abi::SLOT_RAW,
        );
        {
            let _early = super::super::ScopedOopRootGuard::new(runtime, early);
            runtime.enter(|| runtime.gc_collect_from_native_range(0, 0));
            assert_eq!(
                call(id.receiver, b"identityOf:", &[Operand::object(early)])
                    .value
                    .as_word(),
                Some(identity)
            );
            assert_eq!(
                call(id.receiver, b"allocationSizeOf:", &[Operand::object(child)])
                    .value
                    .as_word(),
                Some(0)
            );
            assert_eq!(unsafe { early.cast::<Oop>().read() }, child);
            assert_eq!(unsafe { early.cast::<usize>().add(1).read() }, 83);
            let later_behavior = object(call(object_behavior, b"delegated", &[]));
            let _behavior = super::super::ScopedOopRootGuard::new(runtime, later_behavior);
            let later = object(call(later_behavior, b"allocate", &[Operand::word(7)]));
            assert_eq!(
                runtime.permanence_of(later_behavior),
                Some(crate::heap::Permanence::Collectable)
            );
            assert_eq!(
                runtime.permanence_of(later),
                Some(crate::heap::Permanence::Collectable)
            );
            assert_eq!(
                call(id.receiver, b"allocationSizeOf:", &[Operand::object(later)])
                    .value
                    .as_word(),
                Some(7)
            );
            assert!(
                call(id.receiver, b"identityOf:", &[Operand::object(later)])
                    .value
                    .as_word()
                    .unwrap()
                    > identity
            );
            let _later = super::super::ScopedOopRootGuard::new(runtime, later);
            runtime.enter(|| runtime.gc_collect_from_native_range(0, 0));
            assert_eq!(
                runtime.permanence_of(later),
                Some(crate::heap::Permanence::Collectable)
            );
        }
        runtime.enter(|| runtime.gc_collect_from_native_range(0, 0));
        for dead in [early, child, behavior] {
            assert_eq!(
                call(id.receiver, b"identityOf:", &[Operand::object(dead)])
                    .value
                    .kind(),
                native::UNIT
            );
            assert_eq!(
                call(id.receiver, b"behaviorOf:", &[Operand::object(dead)])
                    .value
                    .kind(),
                native::UNIT
            );
        }
    }

    #[test]
    fn the_same_carrier_survives_adoption_and_gc_accounts_for_later_symbols() {
        let initial = crate::initial_objects::InitialObjects::new();
        let protocol = initial.protocol.protocol;
        let intern = |bytes: &[u8]| unsafe {
            protocol.message.unwrap()(
                protocol.context,
                protocol.receiver,
                protocol.intern,
                &Operand::bytes(bytes),
                1,
            )
        };
        let before = intern(b"native-before-adoption").value.as_object().unwrap();
        let bootstrap = Box::new(crate::dispatch::build_bootstrap_from(initial));
        let runtime = &bootstrap.runtime;
        runtime.install_id_runtime_extensions();
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        runtime.enable_symbol_compaction_for_test();
        assert_eq!(
            intern(b"native-before-adoption").value.as_object(),
            Some(before)
        );
        let _before_root = crate::runtime::ScopedOopRootGuard::new(runtime, before);
        let after = intern(b"native-after-adoption").value.as_object().unwrap();
        assert_eq!(
            runtime.permanence_of(after),
            Some(crate::heap::Permanence::Collectable)
        );
        {
            let _after_root = crate::runtime::ScopedOopRootGuard::new(runtime, after);
            runtime.enter(|| runtime.gc_collect_from_native_range(0, 0));
            assert_eq!(
                runtime.symbol_bytes(before),
                Some(&b"native-before-adoption"[..])
            );
            assert_eq!(
                runtime.symbol_bytes(after),
                Some(&b"native-after-adoption"[..])
            );
        }
        runtime.enter(|| runtime.gc_collect_from_native_range(0, 0));
        assert_eq!(runtime.symbol_bytes(after), None);
        assert_eq!(intern(b"bad\0name").status, native::INVALID_REQUEST);
        let retained = runtime.intern(b"intern:").unwrap();
        assert_eq!(retained, protocol.intern);
    }
}
