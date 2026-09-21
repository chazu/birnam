//! Complete Word construction, printing, equality and checked Integer conversion.
use super::storage::{Instance, STORAGE};
use birnam_abi::{
    Oop, base_values,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
    lifetime, namespace_owner, root_registry,
    string::StringApiV1,
};
use core::{ffi::c_void, ptr};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy)]
pub(crate) struct Access {
    pub id: Protocol,
    integer: *mut birnam_abi::integer::IntegerApiV1,
    roots: root_registry::OwnedV1,
    values: base_values::ValuesV1,
    namespace: Oop,
    string: Oop,
    allocation_size: NativeEntry,
    behavior_of: NativeEntry,
    allocate: Oop,
    binding: Oop,
    error_name: Oop,
    error_make: Oop,
    string_make: Oop,
}
impl Access {
    pub(crate) fn references(self) -> [Oop; 15] {
        [
            unsafe { (*self.integer).behavior },
            unsafe { (*self.integer).prototype },
            self.id.receiver,
            self.id.intern,
            self.id.load,
            self.namespace,
            self.string,
            self.values.nil,
            self.values.true_object,
            self.values.false_object,
            self.allocate,
            self.binding,
            self.error_name,
            self.error_make,
            self.string_make,
        ]
    }
}
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
fn fact(access: Access, entry: NativeEntry, object: Oop) -> Result<Operand, Outcome> {
    let args = [Operand::object(object)];
    unsafe { entry.method.unwrap()(entry.context, access.id.receiver, args.as_ptr(), args.len()) }
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
fn answer(result: Result<Oop, Outcome>) -> Outcome {
    match result {
        Ok(object) => Outcome::loaded(Operand::object(object)),
        Err(error) => error,
    }
}
fn known(access: Access, object: Oop) -> bool {
    fact(access, access.allocation_size, object)
        .ok()
        .and_then(|value| value.as_word())
        .is_some()
}
pub(crate) fn exact(instance: &Instance, access: Access, object: Oop) -> bool {
    fact(access, access.allocation_size, object)
        .ok()
        .and_then(|value| value.as_word())
        .is_some_and(|size| size >= STORAGE.payload_size)
        && fact(access, access.behavior_of, object)
            .ok()
            .and_then(|value| value.as_object())
            == Some(instance.identities()[0])
}
pub(crate) unsafe fn make(instance: &Instance, access: Access, bits: u64) -> Result<Oop, Outcome> {
    let behavior = instance.identities()[0];
    let object = send(
        access.id,
        behavior,
        access.allocate,
        &[Operand::word(STORAGE.payload_size)],
    )?
    .as_object()
    .filter(|object| !object.is_null())
    .ok_or_else(invalid)?;
    if !exact(instance, access, object) {
        return Err(invalid());
    }
    unsafe { (STORAGE.initialize)(object.cast(), bits) };
    Ok(object)
}
unsafe fn word(instance: &Instance, access: Access, object: Oop) -> Option<u64> {
    let ops = unsafe { *instance.operations() };
    if unsafe { (ops.is_exact)(ops.context, object) } == 0 || !exact(instance, access, object) {
        return None;
    }
    Some(unsafe { (ops.read)(ops.context, object) })
}
fn lookup_binding(access: Access, name: Oop) -> Result<Option<Oop>, Outcome> {
    Ok(send(
        access.id,
        access.namespace,
        access.binding,
        &[Operand::object(name)],
    )?
    .as_object()
    .filter(|object| !object.is_null()))
}
fn error(access: Access, kind: &[u8], message: &[u8]) -> Result<Oop, Outcome> {
    let kind = intern(access.id, kind)?;
    let _kind = Root::new(access.roots, kind);
    let message = send(
        access.id,
        access.string,
        access.string_make,
        &[Operand::bytes(message)],
    )?
    .as_object()
    .ok_or_else(invalid)?;
    let _message = Root::new(access.roots, message);
    let Some(factory) = lookup_binding(access, access.error_name)? else {
        return Ok(access.values.nil);
    };
    let _factory = Root::new(access.roots, factory);
    send(
        access.id,
        factory,
        access.error_make,
        &[Operand::object(kind), Operand::object(message)],
    )?
    .as_object()
    .ok_or_else(invalid)
}
unsafe extern "C" fn equals(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([arg]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Some(right) = arg.as_object() else {
        return invalid();
    };
    let instance = unsafe { &*context.cast::<Instance>() };
    let access = instance.native_access().unwrap();
    if !known(access, receiver) || !known(access, right) {
        return invalid();
    }
    let _receiver = Root::new(access.roots, receiver);
    let _right = Root::new(access.roots, right);
    let Some(left) = (unsafe { word(instance, access, receiver) }) else {
        return Outcome::loaded(Operand::object(access.values.nil));
    };
    let equal = unsafe { word(instance, access, right) } == Some(left);
    Outcome::loaded(Operand::object(if equal {
        access.values.true_object
    } else {
        access.values.false_object
    }))
}
unsafe extern "C" fn as_integer(
    context: *mut c_void,
    receiver: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    if count != 0 {
        return invalid();
    }
    let instance = unsafe { &*context.cast::<Instance>() };
    let access = instance.native_access().unwrap();
    if !known(access, receiver) {
        return invalid();
    }
    let _receiver = Root::new(access.roots, receiver);
    let Some(value) = (unsafe { word(instance, access, receiver) }) else {
        return Outcome::loaded(Operand::object(access.values.nil));
    };
    if value > i64::MAX as u64 {
        return answer(error(access, b"overflow", b"word out of Integer range"));
    }
    let integer = unsafe { *access.integer };
    Outcome::loaded(Operand::object(unsafe {
        (integer.make)(integer.context, value as i64)
    }))
}
unsafe extern "C" fn from_bits(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([value]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Some(value) = value.as_word() else {
        return invalid();
    };
    let instance = unsafe { &*context.cast::<Instance>() };
    answer(unsafe { make(instance, instance.native_access().unwrap(), value as u64) })
}
unsafe extern "C" fn bits(
    context: *mut c_void,
    receiver: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    if count != 0 {
        return invalid();
    }
    let instance = unsafe { &*context.cast::<Instance>() };
    let access = instance.native_access().unwrap();
    if !known(access, receiver) {
        return invalid();
    }
    let _receiver = Root::new(access.roots, receiver);
    match unsafe { word(instance, access, receiver) } {
        Some(value) => Outcome::loaded(Operand::word(value as usize)),
        None => invalid(),
    }
}
unsafe extern "C" fn print(
    context: *mut c_void,
    receiver: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    if count != 0 {
        return invalid();
    }
    let instance = unsafe { &*context.cast::<Instance>() };
    let access = instance.native_access().unwrap();
    if !known(access, receiver) {
        return invalid();
    }
    let _receiver = Root::new(access.roots, receiver);
    let Some(value) = (unsafe { word(instance, access, receiver) }) else {
        return Outcome::loaded(Operand::object(access.values.nil));
    };
    let text = format!("0x{value:x}");
    match send(
        access.id,
        access.string,
        access.string_make,
        &[Operand::bytes(text.as_bytes())],
    ) {
        Ok(value) => Outcome::loaded(value),
        Err(error) => error,
    }
}
const METHODS: &[(&[u8], native::Method)] = &[
    (b"=", equals),
    (b"asInteger", as_integer),
    (b"printString", print),
    (b"fromBits:", from_bits),
    (b"bits", bits),
];
fn construct(
    id: Protocol,
    roots: root_registry::OwnedV1,
    lifetime: lifetime::OwnedV2,
    values: base_values::ValuesV1,
    namespace: namespace_owner::OwnedV1,
    string: StringApiV1,
    error_factory: Oop,
    integer: *mut birnam_abi::integer::IntegerApiV1,
) -> Result<*mut birnam_abi::word::WordApiV1, Outcome> {
    let access = Access {
        id,
        integer,
        roots,
        values,
        namespace: namespace.receiver,
        string: string.prototype,
        allocation_size: select(id, b"allocationSizeOf:")?,
        behavior_of: select(id, b"behaviorOf:")?,
        allocate: intern(id, b"allocate")?,
        binding: intern(id, b"bindingAt:")?,
        error_name: intern(id, b"Error")?,
        error_make: intern(id, b"kind:message:")?,
        string_make: intern(id, b"fromBytes:")?,
    };
    if lookup_binding(access, access.error_name)? != Some(error_factory) {
        return Err(invalid());
    }
    let error_behavior = fact(access, access.behavior_of, error_factory)?
        .as_object()
        .ok_or_else(invalid)?;
    if send(
        id,
        error_behavior,
        intern(id, b"lookup")?,
        &[Operand::object(access.error_make)],
    )?
    .as_entry()
    .is_none()
    {
        return Err(invalid());
    }
    let instance = Box::into_raw(Instance::native(access));
    super::INSTANCE.store(instance, Ordering::Release);
    unsafe { root_registry::register(roots.api, super::word_roots) };
    let object = send(id, id.receiver, intern(id, b"objectPrototype")?, &[])?
        .as_object()
        .ok_or_else(invalid)?;
    let parent = fact(access, access.behavior_of, object)?
        .as_object()
        .ok_or_else(invalid)?;
    let behavior = send(id, parent, intern(id, b"delegated")?, &[])?
        .as_object()
        .ok_or_else(invalid)?;
    unsafe {
        (*instance).set_behavior(behavior);
        birnam_abi::payload_layout::set(
            lifetime.layouts,
            behavior,
            ptr::null(),
            0,
            birnam_abi::SLOT_RAW,
        );
    }
    let prototype = unsafe { make(&*instance, access, 0) }?;
    unsafe { (*instance).set_prototype(prototype) };
    let add_method = intern(id, b"addMethod")?;
    for (name, method) in METHODS {
        send(
            id,
            behavior,
            add_method,
            &[
                Operand::object(intern(id, name)?),
                Operand::entry(NativeEntry::new(*method, instance.cast())),
            ],
        )?;
    }
    send(
        id,
        id.receiver,
        intern(id, b"ownerOf:put:")?,
        &[Operand::object(behavior), Operand::object(prototype)],
    )?;
    let publish = intern(id, b"bindingAt:put:")?;
    for (name, value) in [
        (b"Word".as_slice(), prototype),
        (b"WordVt".as_slice(), behavior),
    ] {
        send(
            id,
            namespace.receiver,
            publish,
            &[Operand::object(intern(id, name)?), Operand::object(value)],
        )?;
    }
    let ops = unsafe { (*instance).operations() };
    assert_eq!(unsafe { birnam_abi::word::attach(ops) }, 1);
    READY.store(true, Ordering::Release);
    Ok(ops)
}
unsafe extern "C" fn initialize(
    id: *const Protocol,
    args: *const Operand,
    count: usize,
) -> Outcome {
    if id.is_null() {
        return invalid();
    }
    let Some([roots, lifetime, values, namespace, string, error, integer]) =
        (unsafe { birnam_abi::borrowed_slice(args, count) })
    else {
        return invalid();
    };
    let (
        Some(roots),
        Some(lifetime),
        Some(values),
        Some(namespace),
        Some(string),
        Some(error),
        Some(integer),
    ) = (
        roots.as_word(),
        lifetime.as_word(),
        values.as_word(),
        namespace.as_word(),
        string.as_word(),
        error.as_object(),
        integer.as_word(),
    )
    else {
        return invalid();
    };
    let (roots, lifetime, values, namespace, string, integer) = (
        roots as *const root_registry::OwnedV1,
        lifetime as *const lifetime::OwnedV2,
        values as *const base_values::ValuesV1,
        namespace as *const namespace_owner::OwnedV1,
        string as *const StringApiV1,
        integer as *mut birnam_abi::integer::IntegerApiV1,
    );
    if !unsafe {
        root_registry::valid_owned(roots)
            && lifetime::valid(lifetime)
            && base_values::valid(values)
            && namespace_owner::valid(namespace)
            && birnam_abi::string::valid(string)
            && birnam_abi::integer::valid(integer)
    } || !super::INSTANCE.load(Ordering::Acquire).is_null()
    {
        return invalid();
    }
    match construct(
        unsafe { *id },
        unsafe { *roots },
        unsafe { *lifetime },
        unsafe { *values },
        unsafe { *namespace },
        unsafe { *string },
        error,
        integer,
    ) {
        Ok(ops) => Outcome::loaded(Operand::word(ops as usize)),
        Err(error) => error,
    }
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module {
    version: 1,
    byte_size: size_of::<native::Module>() as u32,
    initialize: Some(initialize),
};
