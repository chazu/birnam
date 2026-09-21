//! Error representation and propagation before numbers, Execution or Runtime.
use birnam_abi::{
    MarkFn, Oop, base_values,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
    lifetime, namespace_owner, root_registry,
    string::StringApiV1,
};
use core::{ffi::c_void, ptr};
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

struct State {
    id: Protocol,
    roots: root_registry::OwnedV1,
    values: base_values::ValuesV1,
    string: StringApiV1,
    behavior: Oop,
    factory_behavior: Oop,
    factory: Oop,
    wild_kind: Oop,
    dnu_kind: Oop,
    allocation_size: NativeEntry,
    behavior_of: NativeEntry,
    symbol_bytes: NativeEntry,
    allocate: Oop,
    from_bytes: Oop,
}
static STATE: AtomicPtr<State> = AtomicPtr::new(ptr::null_mut());
static READY: AtomicBool = AtomicBool::new(false);
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
fn send(id: Protocol, receiver: Oop, selector: Oop, args: &[Operand]) -> Result<Operand, Outcome> {
    unsafe { id.message.unwrap()(id.context, receiver, selector, args.as_ptr(), args.len()) }
        .into_result()
}
fn intern(id: Protocol, bytes: &[u8]) -> Result<Oop, Outcome> {
    send(id, id.receiver, id.intern, &[Operand::bytes(bytes)])?
        .as_object()
        .ok_or_else(invalid)
}
fn select(id: Protocol, name: &[u8]) -> Result<NativeEntry, Outcome> {
    send(
        id,
        id.receiver,
        intern(id, b"lookup")?,
        &[Operand::object(intern(id, name)?)],
    )?
    .as_entry()
    .ok_or_else(invalid)
}
fn fact(state: &State, entry: NativeEntry, object: Oop) -> Result<Operand, Outcome> {
    let args = [Operand::object(object)];
    unsafe { entry.method.unwrap()(entry.context, state.id.receiver, args.as_ptr(), args.len()) }
        .into_result()
}
struct Root {
    roots: root_registry::OwnedV1,
    token: usize,
}
impl Root {
    fn new(roots: root_registry::OwnedV1, object: Oop) -> Self {
        let token = unsafe { roots.retain.unwrap()(roots.context, object) };
        assert_ne!(token, 0);
        Self { roots, token }
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        assert_eq!(
            unsafe { self.roots.release.unwrap()(self.roots.context, self.token) },
            1
        );
    }
}
unsafe extern "C" fn mark_roots(mark: MarkFn) {
    let state = STATE.load(Ordering::Acquire);
    if state.is_null() {
        return;
    }
    let objects = unsafe {
        [
            (*state).behavior,
            (*state).factory_behavior,
            (*state).factory,
            (*state).wild_kind,
            (*state).dnu_kind,
            (*state).values.nil,
            (*state).values.true_object,
            (*state).values.false_object,
            (*state).string.prototype,
            (*state).allocate,
            (*state).from_bytes,
            (*state).id.receiver,
            (*state).id.intern,
            (*state).id.load,
        ]
    };
    for object in objects {
        if !object.is_null() {
            unsafe { mark(object) };
        }
    }
}
unsafe fn state<'a>(context: *mut c_void) -> &'a State {
    unsafe { &*context.cast::<State>() }
}
fn answer(result: Result<Oop, Outcome>) -> Outcome {
    match result {
        Ok(object) => Outcome::loaded(Operand::object(object)),
        Err(error) => error,
    }
}
fn object_argument(state: &State, operand: Operand) -> Result<Oop, Outcome> {
    let object = operand.as_object().ok_or_else(invalid)?;
    if object.is_null() {
        return Ok(state.values.nil);
    }
    if fact(state, state.allocation_size, object)?
        .as_word()
        .is_none()
    {
        return Err(invalid());
    }
    Ok(object)
}
fn make(state: &State, kind: Oop, message: Oop) -> Result<Oop, Outcome> {
    let _kind = Root::new(state.roots, kind);
    let _message = Root::new(state.roots, message);
    let object = send(
        state.id,
        state.behavior,
        state.allocate,
        &[Operand::word(size_of::<super::ErrorPayload>())],
    )?
    .as_object()
    .filter(|value| !value.is_null())
    .ok_or_else(invalid)?;
    if !fact(state, state.allocation_size, object)?
        .as_word()
        .is_some_and(|size| size >= size_of::<super::ErrorPayload>())
        || fact(state, state.behavior_of, object)?.as_object() != Some(state.behavior)
    {
        return Err(invalid());
    }
    unsafe {
        object
            .cast::<super::ErrorPayload>()
            .write(super::ErrorPayload { kind, message })
    };
    Ok(object)
}
unsafe extern "C" fn kind_message(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([kind, message]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else {
        return invalid();
    };
    let state = unsafe { state(context) };
    answer((|| {
        make(
            state,
            object_argument(state, *kind)?,
            object_argument(state, *message)?,
        )
    })())
}
unsafe extern "C" fn kind(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([kind]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else {
        return invalid();
    };
    let state = unsafe { state(context) };
    answer(object_argument(state, *kind).and_then(|kind| make(state, kind, state.values.nil)))
}
unsafe extern "C" fn message(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([message]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else {
        return invalid();
    };
    let state = unsafe { state(context) };
    answer(
        object_argument(state, *message).and_then(|message| make(state, state.wild_kind, message)),
    )
}
fn read(state: &State, receiver: Oop, message: bool) -> Result<Oop, Outcome> {
    if !fact(state, state.allocation_size, receiver)?
        .as_word()
        .is_some_and(|size| size >= size_of::<super::ErrorPayload>())
    {
        return Err(invalid());
    }
    let value = unsafe {
        if message {
            (*receiver.cast::<super::ErrorPayload>()).message
        } else {
            (*receiver.cast::<super::ErrorPayload>()).kind
        }
    };
    // A copied method or changed behavior must not expose a raw word as an Oop.
    object_argument(state, Operand::object(value))
}
unsafe extern "C" fn read_kind(
    context: *mut c_void,
    receiver: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    if count != 0 {
        return invalid();
    }
    answer(read(unsafe { state(context) }, receiver, false))
}
unsafe extern "C" fn read_message(
    context: *mut c_void,
    receiver: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    if count != 0 {
        return invalid();
    }
    answer(read(unsafe { state(context) }, receiver, true))
}
unsafe extern "C" fn yes(context: *mut c_void, _: Oop, _: *const Operand, count: usize) -> Outcome {
    if count != 0 {
        return invalid();
    }
    Outcome::loaded(Operand::object(unsafe {
        state(context).values.true_object
    }))
}
unsafe extern "C" fn no(context: *mut c_void, _: Oop, _: *const Operand, count: usize) -> Outcome {
    if count != 0 {
        return invalid();
    }
    Outcome::loaded(Operand::object(unsafe {
        state(context).values.false_object
    }))
}
unsafe extern "C" fn propagate(
    _: *mut c_void,
    receiver: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    if count == 0 {
        return invalid();
    }
    Outcome::loaded(Operand::object(receiver))
}
unsafe extern "C" fn missing(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some(args) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Some(selector) = args.first().and_then(|arg| arg.as_object()) else {
        return invalid();
    };
    let state = unsafe { state(context) };
    answer((|| {
        let bytes = fact(state, state.symbol_bytes, selector)?
            .as_bytes()
            .ok_or_else(invalid)?;
        let name = unsafe { birnam_abi::borrowed_bytes(bytes.pointer, bytes.length) }
            .ok_or_else(invalid)?;
        let mut text = b"does not understand #".to_vec();
        text.extend_from_slice(name);
        let message = send(
            state.id,
            state.string.prototype,
            state.from_bytes,
            &[Operand::bytes(&text)],
        )?;
        let message = object_argument(state, message)?;
        make(state, state.dnu_kind, message)
    })())
}
fn construct(
    id: Protocol,
    roots: root_registry::OwnedV1,
    lifetime: lifetime::OwnedV2,
    values: base_values::ValuesV1,
    namespace: namespace_owner::OwnedV1,
    string: StringApiV1,
) -> Result<Oop, Outcome> {
    let state = Box::new(State {
        id,
        roots,
        values,
        string,
        behavior: ptr::null_mut(),
        factory_behavior: ptr::null_mut(),
        factory: ptr::null_mut(),
        wild_kind: intern(id, b"error")?,
        dnu_kind: intern(id, b"doesNotUnderstand")?,
        allocation_size: select(id, b"allocationSizeOf:")?,
        behavior_of: select(id, b"behaviorOf:")?,
        symbol_bytes: select(id, b"symbolBytesOf:")?,
        allocate: intern(id, b"allocate")?,
        from_bytes: intern(id, b"fromBytes:")?,
    });
    let state = Box::into_raw(state);
    STATE.store(state, Ordering::Release);
    unsafe { root_registry::register(roots.api, mark_roots) };
    let object = send(id, id.receiver, intern(id, b"objectPrototype")?, &[])?
        .as_object()
        .ok_or_else(invalid)?;
    let parent = send(
        id,
        id.receiver,
        intern(id, b"behaviorOf:")?,
        &[Operand::object(object)],
    )?
    .as_object()
    .ok_or_else(invalid)?;
    let delegate = intern(id, b"delegated")?;
    let behavior = send(id, parent, delegate, &[])?
        .as_object()
        .ok_or_else(invalid)?;
    unsafe {
        (*state).behavior = behavior;
    }
    let factory_behavior = send(id, parent, delegate, &[])?
        .as_object()
        .ok_or_else(invalid)?;
    unsafe {
        (*state).factory_behavior = factory_behavior;
        birnam_abi::payload_layout::set(
            lifetime.layouts,
            behavior,
            ptr::null(),
            0,
            birnam_abi::SLOT_OOP,
        );
        birnam_abi::payload_layout::set(
            lifetime.layouts,
            factory_behavior,
            ptr::null(),
            0,
            birnam_abi::SLOT_RAW,
        );
    }
    let factory = send(
        id,
        factory_behavior,
        unsafe { (*state).allocate },
        &[Operand::word(0)],
    )?
    .as_object()
    .ok_or_else(invalid)?;
    unsafe {
        (*state).factory = factory;
    }
    let add = intern(id, b"addMethod")?;
    for (target, name, method) in [
        (
            behavior,
            b"doesNotUnderstand:".as_slice(),
            propagate as native::Method,
        ),
        (behavior, b"isError".as_slice(), yes as native::Method),
        (behavior, b"kind".as_slice(), read_kind as native::Method),
        (
            behavior,
            b"message".as_slice(),
            read_message as native::Method,
        ),
        (
            factory_behavior,
            b"kind:message:".as_slice(),
            kind_message as native::Method,
        ),
        (
            factory_behavior,
            b"kind:".as_slice(),
            kind as native::Method,
        ),
        (
            factory_behavior,
            b"message:".as_slice(),
            message as native::Method,
        ),
        (parent, b"isError".as_slice(), no as native::Method),
        (
            parent,
            b"doesNotUnderstand:".as_slice(),
            missing as native::Method,
        ),
    ] {
        send(
            id,
            target,
            add,
            &[
                Operand::object(intern(id, name)?),
                Operand::entry(NativeEntry::new(method, state.cast())),
            ],
        )?;
    }
    send(
        id,
        id.receiver,
        intern(id, b"ownerOf:put:")?,
        &[Operand::object(factory_behavior), Operand::object(factory)],
    )?;
    let publish = intern(id, b"bindingAt:put:")?;
    for (name, value) in [
        (b"ErrorVt".as_slice(), behavior),
        (b"ErrorFactoryVt".as_slice(), factory_behavior),
        (b"Error".as_slice(), factory),
    ] {
        send(
            id,
            namespace.receiver,
            publish,
            &[Operand::object(intern(id, name)?), Operand::object(value)],
        )?;
    }
    READY.store(true, Ordering::Release);
    Ok(factory)
}
unsafe extern "C" fn initialize(
    id: *const Protocol,
    args: *const Operand,
    count: usize,
) -> Outcome {
    if id.is_null() {
        return invalid();
    }
    let Some([roots, lifetime, values, namespace, string]) =
        (unsafe { birnam_abi::borrowed_slice(args, count) })
    else {
        return invalid();
    };
    let (Some(roots), Some(lifetime), Some(values), Some(namespace), Some(string)) = (
        roots.as_word(),
        lifetime.as_word(),
        values.as_word(),
        namespace.as_word(),
        string.as_word(),
    ) else {
        return invalid();
    };
    let (roots, lifetime, values, namespace, string) = (
        roots as *const root_registry::OwnedV1,
        lifetime as *const lifetime::OwnedV2,
        values as *const base_values::ValuesV1,
        namespace as *const namespace_owner::OwnedV1,
        string as *const StringApiV1,
    );
    if !unsafe {
        root_registry::valid_owned(roots)
            && lifetime::valid(lifetime)
            && base_values::valid(values)
            && namespace_owner::valid(namespace)
            && birnam_abi::string::valid(string)
    } || !STATE.load(Ordering::Acquire).is_null()
    {
        return invalid();
    }
    answer(construct(
        unsafe { *id },
        unsafe { *roots },
        unsafe { *lifetime },
        unsafe { *values },
        unsafe { *namespace },
        unsafe { *string },
    ))
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module {
    version: 1,
    byte_size: size_of::<native::Module>() as u32,
    initialize: Some(initialize),
};
