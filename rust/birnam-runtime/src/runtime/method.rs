//! Language Method values retain an exact entry, never a function address Oop.
//!
//! `Object methodFor:` and `Vtable methodFor:` answer an owned Method or nil.
//! `Method invokeOn:` supplies no arguments; `invokeOn:arguments:` reads an
//! argument sequence through `size` and `at:`. Invocation uses the captured
//! implementation even after its binding is replaced or removed. `clone`
//! preserves Method identity. `Vtable addMethod:as:` installs the captured entry.
//!
//! Methods retain the owning TCC Unit, registered generated code, or the native
//! image containing the implementation. Native contexts and their object roots remain the native
//! author's responsibility. Methods are process-local; Image rejects durable
//! instances with `methodNotImageable`. Create fresh Methods after image boot.
use super::*;
use crate::abi::id_protocol::{self as native, NativeEntry, Operand, Outcome};
use core::ffi::c_void;

#[derive(Default)]
pub(super) struct Methods {
    behavior: Oop,
    values: crate::raw::AddressMap<Owned>,
    owners: crate::raw::AddressMap<Oop>,
    turn_owners: crate::raw::AddressMap<Oop>,
}
struct Roots<'a>(Vec<ScopedOopRootGuard<'a>>);
impl Drop for Roots<'_> {
    fn drop(&mut self) {
        while self.0.pop().is_some() {}
    }
}
struct Owned {
    entry: NativeEntry,
    lease: Option<GenerationLeaseCapability>,
    image: Option<crate::loader::MethodImage>,
}
impl Runtime {
    pub(crate) fn register_method_owner(&self, function: *const c_void, owner: Oop) -> bool {
        self.assert_owner();
        if function.is_null()
            || self.payload_size(owner).is_none()
            || unsafe { (&*self.state.get()).vtables.lifetime_owner() }.is_none()
        {
            return false;
        }
        let mut methods = self.methods.borrow_mut();
        if methods
            .owners
            .get(&function.addr())
            .is_some_and(|previous| *previous != owner)
        {
            return false;
        }
        methods.owners.insert(function.addr(), owner);
        true
    }
    pub(crate) fn forget_method_owner(&self, owner: Oop) {
        self.assert_owner();
        self.methods
            .borrow_mut()
            .owners
            .retain(|_, value| *value != owner);
    }
    fn method_owner(&self, entry: NativeEntry) -> Oop {
        crate::legacy_method::direct_method(entry)
            .and_then(|method| {
                self.methods
                    .borrow()
                    .owners
                    .get(&method.as_oop().addr())
                    .copied()
            })
            .unwrap_or(core::ptr::null_mut())
    }
    pub(crate) fn update_method_owner_edge(
        &self,
        behavior: Oop,
        selector: Oop,
        previous: Option<NativeEntry>,
        current: Option<NativeEntry>,
    ) {
        let old = previous.map_or(core::ptr::null_mut(), |entry| self.method_owner(entry));
        let new = current.map_or(core::ptr::null_mut(), |entry| self.method_owner(entry));
        if old == new {
            return;
        }
        if !old.is_null() && unsafe { (&*self.state.get()).execution.has_managed_turn() } {
            self.methods
                .borrow_mut()
                .turn_owners
                .insert(old.addr(), old);
        }
        let owner = unsafe { (&*self.state.get()).vtables.lifetime_owner() }
            .expect("generated native methods require Lifetime");
        unsafe { (owner.set_edge)(owner.context, behavior, selector.addr(), new) };
    }
    pub(crate) fn trace_method_turn_owners(&self, mark: MarkFn) {
        let owners: Vec<_> = self
            .methods
            .borrow()
            .turn_owners
            .values()
            .copied()
            .collect();
        for owner in owners {
            unsafe { mark(owner) };
        }
    }
    pub(crate) fn release_method_turn_owners(&self) {
        self.methods.borrow_mut().turn_owners.clear();
    }
    pub(crate) fn install_method_objects(&self) {
        if !self.methods.borrow().behavior.is_null() {
            return;
        }
        let behavior = self.create_vtable(self.globals().object_vt);
        let _behavior = ScopedOopRootGuard::new(self, behavior);
        self.set_layout(behavior, core::ptr::null(), 0, crate::abi::SLOT_OOP);
        let prototype = self.allocate_empty(behavior, Permanence::Permanent);
        self.set_vtable_owner(behavior, prototype);
        for (name, implementation) in [
            (b"invokeOn:".as_slice(), invoke as Method),
            (
                b"invokeOn:arguments:".as_slice(),
                invoke_arguments as Method,
            ),
            (b"clone".as_slice(), identity as Method),
        ] {
            self.add_method(
                behavior,
                self.intern(name).expect("static selector"),
                implementation,
            );
        }
        self.methods.borrow_mut().behavior = behavior;
        let method_for = self.intern(b"methodFor:").expect("static selector");
        self.add_method(self.globals().object_vt, method_for, object_method_for);
        self.add_method(self.globals().vtable_vt, method_for, behavior_method_for);
        let install = self.intern(b"addMethod:as:").expect("static selector");
        self.add_method(self.globals().vtable_vt, install, install_method);
        self.export(b"MethodVt", behavior).expect("static export");
        self.export(b"Method", prototype).expect("static export");
    }
    pub(crate) fn owned_method_entry(&self, object: Oop) -> Option<NativeEntry> {
        self.assert_owner();
        self.methods
            .borrow()
            .values
            .get(&object.addr())
            .map(|value| value.entry)
    }
    pub(crate) fn retain_method(&self, entry: NativeEntry) -> Oop {
        self.assert_owner();
        let Some(function) = entry.method else {
            return self.canonical().nil;
        };
        let legacy = crate::legacy_method::direct_method(entry);
        let owner = self.method_owner(entry);
        let _owner = ScopedOopRootGuard::new(self, owner);
        let lease = match legacy
            .map(|method| self.try_acquire_stackless_generation_lease(method))
            .transpose()
        {
            Ok(lease) => lease.flatten(),
            Err(_) => {
                return self.native_language_outcome(Outcome::failed(native::INVALID_REQUEST, 0));
            }
        };
        let address = legacy.map_or(function as *const c_void, |method| {
            method.as_oop().cast_const()
        });
        // JIT implementations need registration ownership. A plain process
        // image can be retained directly; its native author owns its context.
        let image = if lease.is_none() && owner.is_null() {
            crate::loader::MethodImage::retain(address)
        } else {
            None
        };
        if lease.is_none() && image.is_none() && owner.is_null() {
            return self.native_language_outcome(Outcome::failed(native::INVALID_REQUEST, 0));
        }
        let behavior = self.methods.borrow().behavior;
        boundary::require_invariant(
            !behavior.is_null(),
            "Method representation is not constructed",
        );
        let object = self.allocate_instance(behavior, size_of::<Oop>());
        unsafe {
            object.cast::<Oop>().write(owner);
        }
        self.methods.borrow_mut().values.insert(
            object.addr(),
            Owned {
                entry,
                lease,
                image,
            },
        );
        self.set_finalizer(object, finalize);
        object
    }
    pub(super) fn invoke_owned_method(&self, object: Oop, receiver: Oop, args: &[Oop]) -> Oop {
        let _object = ScopedOopRootGuard::new(self, object);
        let _receiver = ScopedOopRootGuard::new(self, receiver);
        let _args = Roots(
            args.iter()
                .map(|arg| ScopedOopRootGuard::new(self, *arg))
                .collect(),
        );
        let Some(entry) = self.owned_method_entry(object) else {
            return self.canonical().nil;
        };
        if let Some(method) = crate::legacy_method::direct_method(entry) {
            let classified = unsafe {
                (&mut *self.state.get())
                    .execution
                    .classify_resumable(method)
            };
            match classified {
                Ok(Some(StacklessTarget::Registered { entry })) => {
                    return self
                        .enter_acquired_resumable(entry, receiver, args)
                        .unwrap_or_else(|_| self.canonical().nil);
                }
                Err(_) => return self.canonical().nil,
                _ => {}
            }
        }
        // This is a language interface: no pointer-encoded native Word arguments.
        let args: Vec<_> = args.iter().copied().map(Operand::object).collect();
        let outcome =
            unsafe { entry.method.unwrap()(entry.context, receiver, args.as_ptr(), args.len()) };
        self.native_language_outcome(outcome)
    }
}
unsafe extern "C" fn object_method_for(object: Oop, args: *const Oop, count: usize) -> Oop {
    with_active(|runtime| unsafe {
        behavior_method_for(runtime.checked_vtable_of(object), args, count)
    })
}
unsafe extern "C" fn behavior_method_for(behavior: Oop, args: *const Oop, count: usize) -> Oop {
    with_active(|runtime| {
        let Some([selector]) = (unsafe { crate::abi::borrowed_oop_args(args, count) }) else {
            return runtime.canonical().nil;
        };
        runtime.send_slice(behavior, runtime.globals().lookup, &[*selector])
    })
}
unsafe extern "C" fn install_method(behavior: Oop, args: *const Oop, count: usize) -> Oop {
    with_active(|runtime| {
        let Some([method, selector]) = (unsafe { crate::abi::borrowed_oop_args(args, count) })
        else {
            return runtime.canonical().nil;
        };
        if runtime.owned_method_entry(*method).is_none() {
            return runtime.canonical().nil;
        }
        let add = runtime.intern(b"addMethod").unwrap();
        runtime.send_slice(behavior, add, &[*selector, *method])
    })
}
unsafe extern "C" fn identity(object: Oop, _: *const Oop, _: usize) -> Oop {
    object
}
unsafe extern "C" fn invoke(object: Oop, args: *const Oop, count: usize) -> Oop {
    with_active(|runtime| {
        let Some([receiver]) = (unsafe { crate::abi::borrowed_oop_args(args, count) }) else {
            return runtime.canonical().nil;
        };
        runtime.invoke_owned_method(object, *receiver, &[])
    })
}
unsafe extern "C" fn invoke_arguments(object: Oop, args: *const Oop, count: usize) -> Oop {
    with_active(|runtime| {
        let Some([receiver, source]) = (unsafe { crate::abi::borrowed_oop_args(args, count) })
        else {
            return runtime.canonical().nil;
        };
        let _object = ScopedOopRootGuard::new(runtime, object);
        let _receiver = ScopedOopRootGuard::new(runtime, *receiver);
        let _source = ScopedOopRootGuard::new(runtime, *source);
        let size = runtime.send_slice(*source, runtime.intern(b"size").unwrap(), &[]);
        if !runtime.is_integer(size) {
            return runtime.canonical().nil;
        }
        let Ok(count) = usize::try_from(runtime.integer_value(size)) else {
            return runtime.canonical().nil;
        };
        let selector = runtime.intern(b"at:").unwrap();
        let _selector = ScopedOopRootGuard::new(runtime, selector);
        let mut args = Vec::new();
        let mut roots = Roots(Vec::new());
        for index in 0..count {
            let index = runtime.make_integer(index as i64);
            let value = runtime.send_slice(*source, selector, &[index]);
            roots.0.push(ScopedOopRootGuard::new(runtime, value));
            args.push(value);
        }
        runtime.invoke_owned_method(object, *receiver, &args)
    })
}
unsafe extern "C" fn release_image(context: *mut c_void) -> u32 {
    drop(unsafe { Box::from_raw(context.cast::<crate::loader::MethodImage>()) });
    1
}
unsafe extern "C" fn finalize(object: Oop) {
    with_active(|runtime| {
        let owned = runtime.methods.borrow_mut().values.remove(&object.addr());
        if let Some(owned) = owned {
            if let Some(lease) = owned.lease {
                let _ = runtime.release_stackless_generation_lease(lease);
            }
            if let Some(image) = owned.image {
                runtime
                    .queue_native_retirement(Box::into_raw(Box::new(image)).cast(), release_image);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Context {
        calls: Cell<usize>,
    }
    unsafe extern "C" fn missing_lookup(_: Oop, _: *const Oop, _: usize) -> Oop {
        with_active(|runtime| runtime.canonical().nil)
    }
    unsafe extern "C" fn invalid_lookup(_: Oop, _: *const Oop, _: usize) -> Oop {
        with_active(|runtime| runtime.make_integer(42))
    }
    #[test]
    fn language_lookup_nil_is_unbound_and_other_objects_are_not_code_addresses() {
        let bootstrap = crate::dispatch::build_bootstrap();
        let runtime = &bootstrap.runtime;
        let meta = runtime.create_vtable(runtime.globals().vtable_vt);
        let behavior = runtime.create_vtable(runtime.globals().object_vt);
        runtime.reassociate_behavior(behavior, meta);
        let receiver = runtime.allocate_instance(behavior, 0);
        let selector = runtime.intern(b"absent").unwrap();
        runtime.add_method(meta, runtime.globals().lookup, missing_lookup);
        assert!(matches!(
            runtime.enter(|| unsafe { runtime.bind_active(receiver, selector) }),
            Ok(None)
        ));
        runtime.add_method(meta, runtime.globals().lookup, invalid_lookup);
        let failure = runtime
            .enter(|| unsafe { runtime.bind_active(receiver, selector) })
            .unwrap_err();
        assert_eq!(failure.status, native::INVALID_REQUEST);
    }
    unsafe extern "C" fn answer(
        context: *mut c_void,
        receiver: Oop,
        _: *const Operand,
        count: usize,
    ) -> Outcome {
        assert_eq!(count, 0);
        let context = unsafe { &*context.cast::<Context>() };
        context.calls.set(context.calls.get() + 1);
        with_active(|runtime| {
            runtime.gc_collect_from_native_range(0, 0);
        });
        Outcome::loaded(Operand::object(receiver))
    }
    #[test]
    fn native_method_retains_exact_context_through_removal_reentry_and_gc() {
        let bootstrap = crate::dispatch::build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.install_execution_objects();
        runtime.install_method_objects();
        runtime.gc_set_precise(true);
        let context = Context {
            calls: Cell::new(0),
        };
        let entry = NativeEntry::new(answer, core::ptr::from_ref(&context).cast_mut().cast());
        let behavior = runtime.create_vtable(runtime.globals().object_vt);
        let _behavior = ScopedOopRootGuard::new(runtime, behavior);
        let selector = runtime.intern(b"ownedNativeMethod").unwrap();
        runtime.add_native_method(behavior, selector, entry);
        let method = runtime.send_slice(behavior, runtime.globals().lookup, &[selector]);
        let _method = ScopedOopRootGuard::new(runtime, method);
        assert_eq!(runtime.owned_method_entry(method), Some(entry));
        runtime.remove_native_method(behavior, selector);
        let receiver = runtime.allocate_instance(behavior, 0);
        let call = runtime.intern(b"invokeOn:").unwrap();
        assert_eq!(runtime.send_slice(method, call, &[receiver]), receiver);
        assert_eq!(context.calls.get(), 1);
        assert!(runtime.payload_size(receiver).is_some());
        drop(_method);
        runtime.enter(|| runtime.gc_collect_from_native_range(0, 0));
        assert!(runtime.owned_method_entry(method).is_none());
    }
}
