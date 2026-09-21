//! Byte ownership and construction using only completed Id/Lifetime services.
use birnam_abi::{
    Oop,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
    lifetime, namespace_owner,
    native_allocation::NativeAllocationPlanV1,
    root_registry,
};
use core::{ffi::c_void, ptr};
use std::sync::atomic::Ordering;

#[derive(Clone, Copy)]
pub(crate) struct Access {
    pub id: Protocol,
    pub lifetime: lifetime::OwnedV1,
    roots: root_registry::OwnedV1,
    allocate: Oop,
    allocation_size: NativeEntry,
    behavior_of: NativeEntry,
}
impl Access {
    pub(crate) fn references(self) -> [Oop; 4] {
        [
            self.id.receiver,
            self.id.intern,
            self.id.load,
            self.allocate,
        ]
    }
}
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
fn send(id: Protocol, receiver: Oop, selector: Oop, args: &[Operand]) -> Result<Operand, Outcome> {
    unsafe { id.message.unwrap()(id.context, receiver, selector, args.as_ptr(), args.len()) }
        .into_result()
}
fn intern(id: Protocol, name: &[u8]) -> Result<Oop, Outcome> {
    send(id, id.receiver, id.intern, &[Operand::bytes(name)])?
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

pub(crate) unsafe fn allocate(
    access: Access,
    plan: &NativeAllocationPlanV1,
    source: Oop,
    instance: &super::storage::Instance,
) -> Result<Oop, Outcome> {
    let _source = (!source.is_null()).then(|| Root::new(access.roots, source));
    // No String map borrow crosses the message, which can collect or send again.
    let object = send(
        access.id,
        plan.behavior,
        access.allocate,
        &[Operand::word(plan.payload_size)],
    )?
    .as_object()
    .filter(|object| !object.is_null())
    .ok_or_else(invalid)?;
    // Construction is replaceable, but a language replacement cannot make us
    // write through an undersized or incompatible object. These selected Id
    // facts inspect the actual allocation, without another application lookup.
    if instance.owns(object)
        || !fact(access, access.allocation_size, object)?
            .as_word()
            .is_some_and(|size| size >= plan.payload_size)
        || fact(access, access.behavior_of, object)?.as_object() != Some(plan.behavior)
        || unsafe { (access.lifetime.finalizer)(access.lifetime.context, object).callback }
            .is_some()
    {
        return Err(invalid());
    }
    let _object = Root::new(access.roots, object);
    // These callbacks only copy bytes and write our own payload. Lifetime's
    // canonical replacement records the attachment without application calls.
    unsafe {
        (plan.prepare)(plan.context);
        (plan.initialize)(plan.context, object);
    }
    if let Some(callback) = plan.finalizer {
        unsafe {
            (access.lifetime.replace_finalizer)(
                access.lifetime.context,
                object,
                birnam_abi::finalizers::SlotV1 {
                    callback: Some(callback),
                },
            )
        };
    }
    Ok(object)
}
unsafe fn instance<'a>(context: *mut c_void) -> &'a super::storage::Instance {
    unsafe { &*context.cast::<super::storage::Instance>() }
}
unsafe extern "C" fn from_bytes(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([bytes]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Some(bytes) = bytes.as_bytes() else {
        return invalid();
    };
    let Some(bytes) = (unsafe { birnam_abi::borrowed_bytes(bytes.pointer, bytes.length) }) else {
        return invalid();
    };
    match unsafe { instance(context).construct_native(bytes, ptr::null_mut()) } {
        Ok(object) => Outcome::loaded(Operand::object(object)),
        Err(error) => error,
    }
}
unsafe fn borrowed(context: *mut c_void, receiver: Oop) -> Option<Operand> {
    let ops = unsafe { *instance(context).operations() };
    let mut length = 0;
    let pointer = unsafe { ((*ops.storage).borrow)(ops.context, receiver, &mut length) };
    if pointer.is_null() {
        None
    } else {
        Some(Operand::bytes(unsafe {
            core::slice::from_raw_parts(pointer, length)
        }))
    }
}
unsafe extern "C" fn bytes(
    context: *mut c_void,
    receiver: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    if count != 0 {
        return invalid();
    }
    unsafe { borrowed(context, receiver) }
        .map(Outcome::loaded)
        .unwrap_or_else(invalid)
}
unsafe extern "C" fn length(
    context: *mut c_void,
    receiver: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    if count != 0 {
        return invalid();
    }
    unsafe { borrowed(context, receiver) }
        .map(|bytes| Outcome::loaded(Operand::word(bytes.as_bytes().unwrap().length)))
        .unwrap_or_else(invalid)
}
unsafe extern "C" fn clone(
    context: *mut c_void,
    receiver: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    if count != 0 || unsafe { borrowed(context, receiver) }.is_none() {
        return invalid();
    }
    match unsafe { instance(context).construct_native(&[], receiver) } {
        Ok(object) => Outcome::loaded(Operand::object(object)),
        Err(error) => error,
    }
}
unsafe extern "C" fn initialize(
    id: *const Protocol,
    args: *const Operand,
    count: usize,
) -> Outcome {
    if id.is_null() {
        return invalid();
    }
    let Some([roots, lifetime, namespace]) = (unsafe { birnam_abi::borrowed_slice(args, count) })
    else {
        return invalid();
    };
    let (Some(roots), Some(lifetime), Some(namespace)) =
        (roots.as_word(), lifetime.as_word(), namespace.as_word())
    else {
        return invalid();
    };
    let (roots, lifetime, namespace) = (
        roots as *const root_registry::OwnedV1,
        lifetime as *const lifetime::OwnedV1,
        namespace as *const namespace_owner::OwnedV1,
    );
    if !unsafe {
        root_registry::valid_owned(roots)
            && lifetime::valid(lifetime)
            && namespace_owner::valid(namespace)
    } {
        return invalid();
    }
    if super::PHASE
        .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return invalid();
    }
    match construct(
        unsafe { *id },
        unsafe { *roots },
        unsafe { *lifetime },
        unsafe { *namespace },
    ) {
        Ok(ops) => Outcome::loaded(Operand::word(ops as usize)),
        Err(error) => error,
    }
}
fn construct(
    id: Protocol,
    roots: root_registry::OwnedV1,
    lifetime: lifetime::OwnedV1,
    namespace: namespace_owner::OwnedV1,
) -> Result<*mut birnam_abi::string::StringApiV1, Outcome> {
    let access = Access {
        id,
        roots,
        lifetime,
        allocate: intern(id, b"allocate")?,
        allocation_size: select(id, b"allocationSizeOf:")?,
        behavior_of: select(id, b"behaviorOf:")?,
    };
    let instance = Box::into_raw(super::storage::Instance::native(
        access,
        super::cleanup,
        super::cleanup_owner,
    ));
    super::INSTANCE.store(instance, Ordering::Release);
    unsafe { root_registry::register(roots.api, super::storage_roots) };
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
    let prototype = unsafe { (*instance).construct_native(&[], ptr::null_mut()) }?;
    unsafe { (*instance).set_prototype(prototype) };
    let context = instance.cast();
    let add = intern(id, b"addMethod")?;
    for (name, method) in [
        (b"fromBytes:".as_slice(), from_bytes as native::Method),
        (b"bytes".as_slice(), bytes as native::Method),
        (b"length".as_slice(), length as native::Method),
        (b"clone".as_slice(), clone as native::Method),
        (b"new".as_slice(), clone as native::Method),
    ] {
        send(
            id,
            behavior,
            add,
            &[
                Operand::object(intern(id, name)?),
                Operand::entry(NativeEntry::new(method, context)),
            ],
        )?;
    }
    send(
        id,
        id.receiver,
        intern(id, b"ownerOf:put:")?,
        &[Operand::object(behavior), Operand::object(prototype)],
    )?;
    let binding = intern(id, b"bindingAt:put:")?;
    for (name, value) in [
        (b"String".as_slice(), prototype),
        (b"StringVt".as_slice(), behavior),
    ] {
        send(
            id,
            namespace.receiver,
            binding,
            &[Operand::object(intern(id, name)?), Operand::object(value)],
        )?;
    }
    let ops = unsafe { (*instance).operations() };
    if unsafe { birnam_abi::string::attach(ops) } != 1 {
        return Err(invalid());
    }
    super::PHASE.store(2, Ordering::Release);
    Ok(ops)
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module {
    version: 1,
    byte_size: size_of::<native::Module>() as u32,
    initialize: Some(initialize),
};
