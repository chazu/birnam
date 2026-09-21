//! Id's metacircular binding and fixed native invocation. No upper runtime policy.
use crate::abi::{
    Oop,
    id_protocol::{self as native, NativeEntry, Operand, Outcome},
};
use crate::boundary;
use crate::object_memory::ObjectMemory;
use crate::vtable::Vtables;
use core::cell::Cell;
use core::ffi::c_void;

#[derive(Clone, Copy)]
struct Context {
    dictionaries: *mut Vtables,
    metavtable: Oop,
    lookup: Oop,
    does_not_understand: Oop,
}
thread_local! {
    static ACTIVE: Cell<Option<Context>> = const { Cell::new(None) };
}
struct Scope(Option<Context>);
impl Drop for Scope {
    fn drop(&mut self) {
        ACTIVE.set(self.0);
    }
}

/// # Safety
/// The caller serializes this live registry on its owning thread. No reference
/// into it may span native invocation. The registry remains live in this scope.
pub(crate) unsafe fn enter<R>(
    dictionaries: *mut Vtables,
    metavtable: Oop,
    lookup: Oop,
    does_not_understand: Oop,
    operation: impl FnOnce() -> R,
) -> R {
    let previous = ACTIVE.replace(Some(Context {
        dictionaries,
        metavtable,
        lookup,
        does_not_understand,
    }));
    let _scope = Scope(previous);
    operation()
}
fn context() -> Context {
    ACTIVE
        .get()
        .unwrap_or_else(|| boundary::fatal_invariant("Id dispatch used outside its entry scope"))
}
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
/// An Id entry scope and live receiver/selector are required. Native methods
/// and their contexts must remain live through the caller's invocation extent.
pub(crate) unsafe fn bind(receiver: Oop, selector: Oop) -> Result<Option<NativeEntry>, Outcome> {
    let context = context();
    let behavior = unsafe { ObjectMemory::vtable_of_valid_object(receiver) };
    if behavior.is_null() {
        return Ok(None);
    }
    if behavior == context.metavtable && selector == context.lookup {
        return Ok(unsafe { (&*context.dictionaries).lookup(context.metavtable, context.lookup) });
    }
    let result =
        unsafe { send(behavior, context.lookup, &[Operand::object(selector)]) }.into_result()?;
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
/// The entry scope and native Method contracts apply. Upper callers retain
/// reclaimable code for the complete call; Id owns no execution turns.
pub(crate) unsafe fn send(receiver: Oop, selector: Oop, arguments: &[Operand]) -> Outcome {
    let method = match unsafe { bind(receiver, selector) } {
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
    let context = context();
    if selector == context.does_not_understand
        || (selector == context.lookup
            && unsafe { ObjectMemory::vtable_of_valid_object(receiver) } == context.metavtable)
    {
        return Outcome::failed(native::UNBOUND, 0);
    }
    let handler = match unsafe { bind(receiver, context.does_not_understand) } {
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
    _: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    if count != 1 || args.is_null() {
        return invalid();
    }
    let Some(selector) = (unsafe { args.read() }).as_object() else {
        return invalid();
    };
    answer(unsafe { (&*context().dictionaries).lookup(receiver, selector) })
}
pub(crate) unsafe extern "C" fn vtable_add_method(
    _: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    if count != 2 || args.is_null() {
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
    unsafe { (&mut *context().dictionaries).add_method(receiver, selector, method) };
    Outcome::loaded(Operand::entry(method))
}
pub(crate) unsafe extern "C" fn vtable_remove_method(
    _: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    if count != 1 || args.is_null() {
        return invalid();
    }
    let Some(selector) = (unsafe { args.read() }).as_object() else {
        return invalid();
    };
    answer(unsafe { (&mut *context().dictionaries).remove_method(receiver, selector) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::initial_objects::InitialObjects;
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
    fn actual_initial_graph_installs_invokes_and_forwards_without_runtime() {
        let mut id = InitialObjects::new();
        let add = id.symbols.existing(b"addMethod").unwrap();
        let remove = id.symbols.existing(b"removeMethod:").unwrap();
        let selector = id
            .symbols
            .intern(&mut id.memory, id.symbol_vt, b"identity")
            .unwrap();
        let missing = id
            .symbols
            .intern(&mut id.memory, id.symbol_vt, b"missing:")
            .unwrap();
        unsafe {
            enter(
                &mut *id.dictionaries,
                id.vtable_vt,
                id.lookup,
                id.does_not_understand,
                || {
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
                },
            )
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
        let image = unsafe { crate::loader::open(path.as_ptr(), 2 | 4) };
        assert!(!image.is_null());
        let address = unsafe { crate::loader::symbol(image, c"birnam_id_protocol_echo".as_ptr()) };
        assert!(!address.is_null());
        let method: native::Method = unsafe { core::mem::transmute(address) };
        let mut id = InitialObjects::new();
        let add = id.symbols.existing(b"addMethod").unwrap();
        let echo = id
            .symbols
            .intern(&mut id.memory, id.symbol_vt, b"echo:")
            .unwrap();
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
            enter(
                &mut *id.dictionaries,
                id.vtable_vt,
                id.lookup,
                id.does_not_understand,
                || {
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
                },
            )
        };
        std::fs::remove_dir_all(directory).unwrap();
    }
}
