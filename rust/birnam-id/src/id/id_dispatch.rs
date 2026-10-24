//! Id's metacircular binding and fixed native invocation. No upper runtime policy.
use crate::abi::{
    Oop,
    id_protocol::{self as native, NativeEntry, Operand, Outcome},
};
use crate::id::object_memory::ObjectMemory;
use crate::id::vtable::Vtables;
use core::ffi::c_void;

/// Copied Id facts for one dispatch. Each nested send carries its own context;
/// saved methods carry their registry separately in NativeEntry::context.
#[derive(Clone, Copy)]
pub(crate) struct Context {
    pub(crate) dictionaries: *mut Vtables,
    pub(crate) metavtable: Oop,
    pub(crate) lookup: Oop,
    pub(crate) does_not_understand: Oop,
    // Experimental size-immediate receiver support, confined to unit probes.
    // No production ABI, object representation or collector is changed.
    #[cfg(test)]
    pub(crate) size_probe_behavior: Oop,
}
#[cfg(test)]
pub(crate) const fn entry(method: native::Method) -> NativeEntry {
    NativeEntry::new(method, core::ptr::null_mut())
}
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
fn answer(method: Option<NativeEntry>) -> Outcome {
    Outcome::loaded(method.map(Operand::entry).unwrap_or_else(Operand::unit))
}

/// # Safety
/// A live owner-thread context and receiver/selector are required. Native methods
/// and their contexts must remain live through the caller's invocation extent.
pub(crate) unsafe fn bind(
    context: Context,
    receiver: Oop,
    selector: Oop,
) -> Result<Option<NativeEntry>, Outcome> {
    #[cfg(test)]
    let behavior = if !context.size_probe_behavior.is_null() && receiver.addr() & 1 != 0 {
        context.size_probe_behavior
    } else {
        unsafe { ObjectMemory::vtable_of_valid_object(receiver) }
    };
    #[cfg(not(test))]
    let behavior = unsafe { ObjectMemory::vtable_of_valid_object(receiver) };
    if behavior.is_null() {
        return Ok(None);
    }
    if behavior == context.metavtable && selector == context.lookup {
        return Ok(unsafe { (&*context.dictionaries).lookup(context.metavtable, context.lookup) });
    }
    // Specialize only the actual built-in lookup implementation. Id invalidates
    // its positive lookup fact on every existing-graph mutation. Check the
    // receiver's metabehavior on every send; custom lookup takes the ordinary
    // metacircular path below. Requested methods are always read live.
    if unsafe { ObjectMemory::vtable_of_valid_object(behavior) } == context.metavtable
        && unsafe { (&*context.dictionaries).has_default_lookup(context.metavtable, context.lookup) }
    {
        return Ok(unsafe { (&*context.dictionaries).lookup(behavior, selector) });
    }
    let result = unsafe {
        send(
            context,
            behavior,
            context.lookup,
            &[Operand::object(selector)],
        )
    }
    .into_result()?;
    if result.kind() == native::UNIT {
        return Ok(None);
    }
    result
        .as_entry()
        .filter(|entry| entry.method.is_some())
        .map(Some)
        .ok_or_else(invalid)
}

/// # Safety
/// The owner-thread context and native Method contracts apply. Upper callers retain
/// reclaimable code for the complete call; Id owns no execution turns.
pub(crate) unsafe fn send(
    context: Context,
    receiver: Oop,
    selector: Oop,
    arguments: &[Operand],
) -> Outcome {
    let method = match unsafe { bind(context, receiver, selector) } {
        Ok(method) => method,
        Err(failure) => return failure,
    };
    if let Some(method) = method {
        return unsafe {
            method.method.unwrap()(
                method.context,
                receiver,
                arguments.as_ptr(),
                arguments.len(),
            )
        };
    }
    if selector == context.does_not_understand
        || (selector == context.lookup
            && unsafe { ObjectMemory::vtable_of_valid_object(receiver) } == context.metavtable)
    {
        return Outcome::failed(native::UNBOUND, 0);
    }
    let handler = match unsafe { bind(context, receiver, context.does_not_understand) } {
        Ok(Some(handler)) => handler,
        Ok(None) => return Outcome::failed(native::UNBOUND, 0),
        Err(failure) => return failure,
    };
    let mut forwarded = [Operand::unit(); 17];
    forwarded[0] = Operand::object(selector);
    let count = arguments.len().min(16);
    forwarded[1..1 + count].copy_from_slice(&arguments[..count]);
    unsafe { handler.method.unwrap()(handler.context, receiver, forwarded.as_ptr(), count + 1) }
}

pub(crate) unsafe extern "C" fn vtable_lookup_method(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    if context.is_null() || count != 1 || args.is_null() {
        return invalid();
    }
    let Some(selector) = (unsafe { args.read() }).as_object() else {
        return invalid();
    };
    answer(unsafe { (&*context.cast::<Vtables>()).lookup(receiver, selector) })
}
pub(crate) unsafe extern "C" fn vtable_add_method(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    if context.is_null() || count != 2 || args.is_null() {
        return invalid();
    }
    let Some(selector) = (unsafe { args.read() }).as_object() else {
        return invalid();
    };
    let Some(method) = (unsafe { args.add(1).read() })
        .as_entry()
        .filter(|entry| entry.method.is_some())
    else {
        return invalid();
    };
    unsafe { (&mut *context.cast::<Vtables>()).add_method(receiver, selector, method) };
    Outcome::loaded(Operand::entry(method))
}
pub(crate) unsafe extern "C" fn vtable_remove_method(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    if context.is_null() || count != 1 || args.is_null() {
        return invalid();
    }
    let Some(selector) = (unsafe { args.read() }).as_object() else {
        return invalid();
    };
    answer(unsafe { (&mut *context.cast::<Vtables>()).remove_method(receiver, selector) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::initial_objects::InitialObjects;
    fn intern(id: &InitialObjects, bytes: &[u8]) -> Oop {
        let protocol = id.protocol.protocol;
        let answer = unsafe {
            protocol.message.unwrap()(
                protocol.context,
                protocol.receiver,
                protocol.intern,
                &Operand::bytes(bytes),
                1,
            )
        };
        assert_eq!(answer.status, native::LOADED);
        answer.value.as_object().unwrap()
    }

    unsafe extern "C" fn identity(
        _: *mut c_void,
        receiver: Oop,
        _: *const Operand,
        _: usize,
    ) -> Outcome {
        Outcome::loaded(Operand::object(receiver))
    }
    unsafe extern "C" fn dnu(
        _: *mut c_void,
        _: Oop,
        args: *const Operand,
        count: usize,
    ) -> Outcome {
        assert_eq!(count, 2);
        Outcome::loaded(unsafe { args.add(1).read() })
    }
    #[test]
    fn saved_primitives_work_without_an_active_message_and_keep_their_registry() {
        let first = InitialObjects::new();
        let second = InitialObjects::new();
        let select = |id: &InitialObjects, name: &[u8]| unsafe {
            let selector = intern(id, name);
            super::send(
                id.protocol.dispatch_context(),
                id.vtable_vt,
                id.lookup,
                &[Operand::object(selector)],
            )
            .value
            .as_entry()
            .unwrap()
        };
        let first_lookup = select(&first, b"lookup");
        let second_lookup = select(&second, b"lookup");
        assert_eq!(
            first_lookup.method.map(|f| f as usize),
            second_lookup.method.map(|f| f as usize)
        );
        assert_ne!(first_lookup.context, second_lookup.context);
        let add = select(&first, b"addMethod");
        let remove = select(&first, b"removeMethod:");
        let selector = intern(&first, b"savedPrimitiveProbe");
        let selected = entry(identity);
        let invoke = |method: NativeEntry, receiver, args: &[Operand]| unsafe {
            method.method.unwrap()(method.context, receiver, args.as_ptr(), args.len())
        };
        assert_eq!(
            invoke(
                add,
                first.object_vt,
                &[Operand::object(selector), Operand::entry(selected)]
            )
            .value
            .as_entry(),
            Some(selected)
        );
        // No enclosing message or ambient context exists for these calls.
        assert_eq!(
            invoke(first_lookup, first.object_vt, &[Operand::object(selector)])
                .value
                .as_entry(),
            Some(selected)
        );
        assert_eq!(
            invoke(
                second_lookup,
                second.object_vt,
                &[Operand::object(selector)]
            )
            .value
            .kind(),
            native::UNIT
        );
        assert_eq!(
            invoke(remove, first.object_vt, &[Operand::object(selector)])
                .value
                .as_entry(),
            Some(selected)
        );
        assert_eq!(
            invoke(first_lookup, first.object_vt, &[Operand::object(selector)])
                .value
                .kind(),
            native::UNIT
        );
    }

    struct LookupProbe {
        original: NativeEntry,
        calls: core::cell::Cell<usize>,
    }
    unsafe extern "C" fn counted_lookup(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
        let probe = unsafe { &*c.cast::<LookupProbe>() };
        probe.calls.set(probe.calls.get() + 1);
        unsafe { probe.original.method.unwrap()(probe.original.context, r, a, n) }
    }
    unsafe extern "C" fn answer_word(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
        Outcome::loaded(Operand::word(19))
    }
    #[test]
    fn specialized_lookup_falls_back_for_replacement_and_custom_metabehavior() {
        let id = InitialObjects::new();
        let add = intern(&id, b"addMethod");
        let selector = intern(&id, b"identity");
        let original = unsafe { (&*id.protocol.dispatch_context().dictionaries).lookup(id.vtable_vt, id.lookup) }.unwrap();
        let mut probe = LookupProbe { original, calls: core::cell::Cell::new(0) };
        let custom = NativeEntry::new(counted_lookup, core::ptr::from_mut(&mut probe).cast());
        unsafe {
            let send = |r, sel, a: &[Operand]| super::send(id.protocol.dispatch_context(), r, sel, a);
            let install = |r, sel, method| send(r, add, &[Operand::object(sel), Operand::entry(method)]);
            assert_eq!(install(id.object_vt, selector, entry(identity)).status, native::LOADED);
            assert_eq!(send(id.object_prototype, selector, &[]).value.as_object(), Some(id.object_prototype));
            assert_eq!(install(id.vtable_vt, id.lookup, custom).status, native::LOADED);
            probe.calls.set(0);
            assert_eq!(send(id.object_prototype, selector, &[]).value.as_object(), Some(id.object_prototype));
            assert!(probe.calls.get() > 0, "replaced lookup must execute");
            assert_eq!(install(id.object_vt, selector, entry(answer_word)).status, native::LOADED);
            assert_eq!(send(id.object_prototype, selector, &[]).value.as_word(), Some(19));
            assert_eq!(install(id.vtable_vt, id.lookup, original).status, native::LOADED);
            assert_eq!(send(id.object_prototype, selector, &[]).value.as_word(), Some(19));
            let meta = send(id.vtable_vt, intern(&id, b"delegated"), &[]).value.as_object().unwrap();
            assert_eq!(install(meta, id.lookup, custom).status, native::LOADED);
            assert_eq!(send(id.vtable_vt, intern(&id, b"reassociateBehaviorOf:with:"), &[Operand::object(id.object_vt), Operand::object(meta)]).status, native::LOADED);
            probe.calls.set(0);
            assert_eq!(send(id.object_prototype, selector, &[]).value.as_word(), Some(19));
            assert!(probe.calls.get() > 0, "custom metabehavior lookup must execute");
        }
    }
    #[test]
    fn actual_initial_graph_installs_invokes_and_forwards_without_runtime() {
        let id = InitialObjects::new();
        let add = intern(&id, b"addMethod");
        let remove = intern(&id, b"removeMethod:");
        let selector = intern(&id, b"identity");
        let missing = intern(&id, b"missing:");
        unsafe {
            let send = |receiver, selector, args: &[Operand]| {
                super::send(id.protocol.dispatch_context(), receiver, selector, args)
            };
            assert_eq!(
                send(
                    id.object_vt,
                    add,
                    &[Operand::object(selector), Operand::entry(entry(identity))]
                )
                .status,
                native::LOADED
            );
            assert_eq!(
                send(id.object_prototype, selector, &[]).value.as_object(),
                Some(id.object_prototype)
            );
            assert_eq!(
                send(id.object_prototype, missing, &[]).status,
                native::UNBOUND
            );
            assert_eq!(
                send(
                    id.object_vt,
                    add,
                    &[
                        Operand::object(id.does_not_understand),
                        Operand::entry(entry(dnu))
                    ]
                )
                .status,
                native::LOADED
            );
            assert_eq!(
                send(id.object_prototype, missing, &[Operand::word(73)])
                    .value
                    .as_word(),
                Some(73)
            );
            assert_eq!(
                send(id.vtable_vt, remove, &[Operand::object(id.lookup)]).status,
                native::LOADED
            );
            assert_eq!(
                send(id.object_prototype, selector, &[]).status,
                native::UNBOUND
            );
        };
    }
    #[test]
    fn c_native_entry_round_trips_typed_operands_through_actual_id() {
        let directory =
            std::env::temp_dir().join(format!("birnam-id-protocol-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let library = directory.join("entry.dylib");
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../rust/birnam-abi/tests/fixtures/id-protocol-v1/entry.c");
        let output = std::process::Command::new("cc")
            .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-dynamiclib"])
            .arg(source)
            .arg("-o")
            .arg(&library)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let path = std::ffi::CString::new(library.as_os_str().as_encoded_bytes()).unwrap();
        // The foreign entry's image stays pinned for the process. This is an
        // ABI fixture, not an alternate Id: it is called by the actual core.
        unsafe extern "C" {
            fn dlopen(path: *const core::ffi::c_char, flags: core::ffi::c_int) -> *mut c_void;
            fn dlsym(image: *mut c_void, name: *const core::ffi::c_char) -> *mut c_void;
        }
        let image = unsafe { dlopen(path.as_ptr(), 2 | 4) };
        assert!(!image.is_null());
        let address = unsafe { dlsym(image, c"birnam_id_protocol_echo".as_ptr()) };
        assert!(!address.is_null());
        let method: native::Method = unsafe { core::mem::transmute(address) };
        let id = InitialObjects::new();
        let add = intern(&id, b"addMethod");
        let echo = intern(&id, b"echo:");
        let foreign = NativeEntry::new(method, id.object_prototype);
        let protocol = native::Protocol {
            version: native::VERSION,
            byte_size: size_of::<native::Protocol>() as u32,
            message: None,
            context: core::ptr::null_mut(),
            receiver: id.object_prototype,
            intern: id.lookup,
            load: echo,
        };
        let operands = [
            Operand::unit(),
            Operand::object(id.symbol_prototype),
            Operand::word(usize::MAX),
            Operand::bytes(b"binary\0\xff"),
            Operand::entry(foreign),
            Operand::protocol(&protocol),
        ];
        unsafe {
            let send = |receiver, selector, args: &[Operand]| {
                super::send(id.protocol.dispatch_context(), receiver, selector, args)
            };
            assert_eq!(
                send(
                    id.object_vt,
                    add,
                    &[Operand::object(echo), Operand::entry(foreign)]
                )
                .status,
                native::LOADED
            );
            for operand in operands {
                let answer = send(id.object_prototype, echo, &[operand]);
                assert_eq!(answer.status, native::LOADED);
                assert_eq!(answer.value, operand);
            }
            let error = send(
                id.object_prototype,
                echo,
                &[Operand::unit(), Operand::unit()],
            );
            assert_eq!((error.status, error.detail), (native::INVALID_REQUEST, 2));
        };
        // A C root owner calls the current visitor method through the real Id
        // carrier, including replacement and exact native failure propagation.
        unsafe {
            let address = dlsym(image, c"birnam_gc_enumerate".as_ptr());
            assert!(!address.is_null());
            let enumerate: native::Method = core::mem::transmute(address);
            let actual = id.protocol.protocol;
            let roots = intern(&id, b"rootsDo:");
            let visit = intern(&id, b"visit:");
            let send = |receiver, selector, args: &[Operand]| {
                actual.send(receiver, selector, args)
            };
            send(id.object_vt, add, &[
                Operand::object(roots),
                Operand::entry(NativeEntry::new(enumerate, (&raw const actual).cast_mut().cast())),
            ]).unwrap();
            send(id.object_vt, add, &[Operand::object(visit), Operand::entry(foreign)]).unwrap();
            let result = send(id.object_prototype, roots, &[Operand::object(id.object_prototype)]).unwrap();
            assert_eq!(result.as_object(), Some(actual.receiver));
            unsafe extern "C" fn reject(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
                Outcome::failed(native::INVALID_REQUEST, 917)
            }
            send(id.object_vt, add, &[Operand::object(visit), Operand::entry(NativeEntry::new(reject, core::ptr::null_mut()))]).unwrap();
            assert_eq!(send(id.object_prototype, roots, &[Operand::object(id.object_prototype)]).unwrap_err().detail, 917);
            assert!(send(id.object_prototype, roots, &[Operand::entry(foreign)]).is_err());
        }
        std::fs::remove_dir_all(directory).unwrap();
    }
}
