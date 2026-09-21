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
    let behavior = unsafe { ObjectMemory::vtable_of_valid_object(receiver) };
    if behavior.is_null() {
        return Ok(None);
    }
    if behavior == context.metavtable && selector == context.lookup {
        return Ok(unsafe { (&*context.dictionaries).lookup(context.metavtable, context.lookup) });
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
            .join("../../abi/fixtures/id-protocol-v1/entry.c");
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
        std::fs::remove_dir_all(directory).unwrap();
    }
}
