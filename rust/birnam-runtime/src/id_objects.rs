//! Upper construction queries of completed Id objects, through the native ABI.
use crate::abi::{
    Oop,
    id_protocol::{Operand, Outcome, Protocol},
};

pub(crate) struct Objects {
    pub(crate) vtable_vt: Oop,
    pub(crate) object_vt: Oop,
    pub(crate) symbol_vt: Oop,
    pub(crate) lookup: Oop,
    pub(crate) does_not_understand: Oop,
    pub(crate) object_prototype: Oop,
    pub(crate) symbol_prototype: Oop,
}
impl Objects {
    /// The caller retains Id and roots each answer before another native call.
    /// Current Id interning preserves the collecting constructor's accounting.
    pub(crate) fn read(id: Protocol, mut retain: impl FnMut(Oop)) -> Self {
        let object = |answer: Outcome| {
            answer
                .into_result()
                .unwrap_or_else(|_| panic!("Id construction query failed"))
                .as_object()
                .filter(|value| !value.is_null())
                .expect("Id query returned no object")
        };
        let name = |name: &[u8]| {
            object(unsafe {
                id.message.unwrap()(id.context, id.receiver, id.intern, &Operand::bytes(name), 1)
            })
        };
        let lookup = name(b"lookup");
        retain(lookup);
        let does_not_understand = name(b"doesNotUnderstand:");
        retain(does_not_understand);
        let prototype_selector = name(b"objectPrototype");
        retain(prototype_selector);
        let behavior_selector = name(b"behaviorOf:");
        retain(behavior_selector);
        let query = |selector, args: &[Operand]| {
            object(unsafe {
                id.message.unwrap()(id.context, id.receiver, selector, args.as_ptr(), args.len())
            })
        };
        let object_prototype = query(prototype_selector, &[]);
        retain(object_prototype);
        let object_vt = query(behavior_selector, &[Operand::object(object_prototype)]);
        retain(object_vt);
        // Intern the live Symbol prototype rather than taking an old private
        // construction pointer that may have been retired before Runtime exists.
        let symbol_prototype = name(b"Symbol");
        retain(symbol_prototype);
        let symbol_vt = query(behavior_selector, &[Operand::object(symbol_prototype)]);
        retain(symbol_vt);
        Self {
            vtable_vt: id.receiver,
            object_vt,
            symbol_vt,
            lookup,
            does_not_understand,
            object_prototype,
            symbol_prototype,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::id_protocol as native;
    use core::{cell::Cell, ffi::c_void};

    #[test]
    fn runtime_construction_uses_an_opaque_native_carrier() {
        struct Proxy {
            inner: Protocol,
            calls: Cell<usize>,
        }
        unsafe extern "C" fn message(
            context: *mut c_void,
            receiver: Oop,
            selector: Oop,
            args: *const Operand,
            count: usize,
        ) -> Outcome {
            let proxy = unsafe { &*context.cast::<Proxy>() };
            proxy.calls.set(proxy.calls.get() + 1);
            unsafe {
                proxy.inner.message.unwrap()(proxy.inner.context, receiver, selector, args, count)
            }
        }
        let initial = crate::initial_objects::InitialObjects::new();
        let proxy = Box::new(Proxy {
            inner: initial.protocol.protocol,
            calls: Cell::new(0),
        });
        let id = Protocol {
            message: Some(message),
            context: core::ptr::from_ref(proxy.as_ref()).cast_mut().cast(),
            ..proxy.inner
        };
        let mut bootstrap = crate::dispatch::build_bootstrap_with(
            id, None, None, None, None, None, None, None, None, crate::support::diagnostic(),
        );
        assert!(proxy.calls.get() > 0);
        // The constructor cannot cast this context to private Id State. Keep
        // the proxy and its provider alive even if a test moves out the Runtime.
        bootstrap.runtime.test_id_owner = Some(Box::new((proxy, initial.protocol)));
        let runtime = &bootstrap.runtime;
        let selector = runtime.intern(b"clone").unwrap();
        let object = runtime.send_slice(runtime.canonical().object_prototype, selector, &[]);
        assert_eq!(
            runtime.checked_vtable_of(object),
            runtime.globals().object_vt
        );
        assert_ne!(object, runtime.canonical().object_prototype);
    }

    #[test]
    fn retained_entries_support_an_id_with_different_primitive_implementations() {
        // A provider may use one forwarding implementation and distinct opaque
        // contexts for all three operations. Runtime must use declared entries,
        // not private Id function addresses or a function-only classification.
        unsafe extern "C" fn forward(
            context: *mut c_void,
            receiver: Oop,
            args: *const Operand,
            count: usize,
        ) -> Outcome {
            let entry = unsafe { &*context.cast::<native::NativeEntry>() };
            unsafe { entry.method.unwrap()(entry.context, receiver, args, count) }
        }
        let initial = crate::initial_objects::InitialObjects::new();
        let id = initial.protocol.protocol;
        let originals = Box::new([
            initial.primitives.lookup,
            initial.primitives.add_method,
            initial.primitives.remove_method,
        ]);
        for (name, original) in [b"lookup".as_slice(), b"addMethod", b"removeMethod:"]
            .into_iter()
            .zip(originals.iter())
        {
            let selector = unsafe {
                id.message.unwrap()(id.context, id.receiver, id.intern, &Operand::bytes(name), 1)
            }
            .value
            .as_object()
            .unwrap();
            let args = [
                Operand::object(selector),
                Operand::entry(native::NativeEntry::new(
                    forward,
                    core::ptr::from_ref(original).cast_mut().cast(),
                )),
            ];
            let add = initial.primitives.add_method;
            assert_eq!(
                unsafe { add.method.unwrap()(add.context, id.receiver, args.as_ptr(), args.len()) }
                    .status,
                native::LOADED
            );
        }
        // This is the provider's completed initial graph. Capture it before
        // constructing any upper module, as production Bootstrap does.
        let primitives = unsafe { birnam_abi::id_primitives::select(id) }
            .unwrap_or_else(|_| panic!("completed provider"));
        let mut bootstrap = crate::dispatch::build_bootstrap_with(
            id,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(primitives),
            crate::support::diagnostic(),
        );
        bootstrap.runtime.test_id_owner = Some(Box::new((originals, initial.protocol)));
        let runtime = &bootstrap.runtime;
        let _turn = runtime.enter_managed_turn();
        assert!(runtime.guarded_default_lookup_eligible(runtime.globals().object_vt));
        let selector = runtime.intern(b"clone").unwrap();
        let object = runtime.send_slice(runtime.canonical().object_prototype, selector, &[]);
        assert_eq!(
            runtime.checked_vtable_of(object),
            runtime.globals().object_vt
        );
        assert_ne!(object, runtime.canonical().object_prototype);
    }

    #[test]
    fn construction_queries_the_live_symbol_prototype_after_early_retirement() {
        let initial = crate::initial_objects::InitialObjects::new();
        let id = initial.protocol.protocol;
        let old = initial.symbol_prototype;
        let send = |name: &[u8], arguments: &[Operand]| unsafe {
            let selector =
                id.message.unwrap()(id.context, id.receiver, id.intern, &Operand::bytes(name), 1)
                    .value
                    .as_object()
                    .unwrap();
            let result = id.message.unwrap()(
                id.context,
                id.receiver,
                selector,
                arguments.as_ptr(),
                arguments.len(),
            );
            assert_eq!(result.status, native::LOADED);
            result.value
        };
        let address = old.addr().to_ne_bytes();
        send(b"retireIndexesFor:", &[Operand::bytes(&address)]);
        let release = send(b"detachAllocationAt:", &[Operand::word(old.addr())])
            .as_entry()
            .unwrap();
        // Keep the detached allocation until a new Symbol exists, preventing
        // address reuse from hiding stale use of the private construction field.
        let bootstrap = crate::dispatch::build_bootstrap_from(initial);
        let current = bootstrap.runtime.canonical().symbol_prototype;
        assert_ne!(old, current);
        assert_eq!(bootstrap.runtime.intern(b"Symbol").unwrap(), current);
        assert_eq!(
            bootstrap.runtime.checked_vtable_of(current),
            bootstrap.runtime.globals().symbol_vt
        );
        assert_eq!(
            unsafe {
                release.method.unwrap()(
                    release.context,
                    core::ptr::null_mut(),
                    core::ptr::null(),
                    0,
                )
            }
            .status,
            native::LOADED
        );
    }
}
