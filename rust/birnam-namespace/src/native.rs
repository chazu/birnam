//! Native Symbol-based publication and enumeration before upper Runtime.
use birnam_abi::{Oop, base_values, lifetime, namespace_owner::OwnedV1, root_registry,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol}};
use core::{ffi::c_void, ptr};
use std::sync::atomic::{AtomicPtr, Ordering};

struct Namespace {
    owned: OwnedV1,
    id: Protocol,
}
// The registry's existing C root-provider convention has no context argument.
// This points to the one completed production Namespace, never future services.
static NAMESPACE: AtomicPtr<Namespace> = AtomicPtr::new(ptr::null_mut());
fn invalid() -> Outcome { Outcome::failed(native::INVALID_REQUEST, 0) }
fn value(object: Oop) -> Operand { if object.is_null() { Operand::unit() } else { Operand::object(object) } }
fn send(id: Protocol, receiver: Oop, selector: Oop, args: &[Operand]) -> Result<Operand, Outcome> {
    let result = unsafe { id.message.unwrap()(id.context, receiver, selector, args.as_ptr(), args.len()) };
    if result.status == native::LOADED { Ok(result.value) } else { Err(result) }
}
fn intern(id: Protocol, name: &[u8]) -> Result<Oop, Outcome> {
    send(id, id.receiver, id.intern, &[Operand::bytes(name)])?.as_object().ok_or_else(invalid)
}
unsafe fn selection(context: *mut c_void) -> (Protocol, *mut c_void) {
    let namespace = unsafe { &*context.cast::<Namespace>() };
    (namespace.id, namespace.owned.storage)
}
fn symbol(id: Protocol, argument: Operand) -> Result<Oop, Outcome> {
    let key = argument.as_object().filter(|key| !key.is_null()).ok_or_else(invalid)?;
    if send(id, id.receiver, intern(id, b"symbolBytesOf:")?, &[argument])?.kind() != native::BYTES { return Err(invalid()); }
    Ok(key)
}
unsafe extern "C" fn find(context: *mut c_void, _: Oop, args: *const Operand, count: usize) -> Outcome {
    let Some([key]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid(); };
    let (id, storage) = unsafe { selection(context) };
    match symbol(id, *key) {
        Ok(key) => Outcome::loaded(value(unsafe { super::find(storage, key) })), Err(error) => error,
    }
}
unsafe extern "C" fn bind(context: *mut c_void, _: Oop, args: *const Operand, count: usize) -> Outcome {
    let Some([key, argument]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid(); };
    let object = match argument.kind() {
        native::UNIT => ptr::null_mut(),
        native::OBJECT => match argument.as_object().filter(|object| !object.is_null()) { Some(object) => object, None => return invalid() },
        _ => return invalid(),
    };
    let (id, storage) = unsafe { selection(context) };
    match symbol(id, *key) {
        Ok(key) => Outcome::loaded(value(unsafe { super::bind(storage, key, object) })), Err(error) => error,
    }
}
unsafe extern "C" fn count(context: *mut c_void, _: Oop, _: *const Operand, count: usize) -> Outcome {
    if count != 0 { return invalid(); }
    let (_, storage) = unsafe { selection(context) };
    Outcome::loaded(Operand::word(unsafe { super::count(storage) }))
}
unsafe extern "C" fn name_at(context: *mut c_void, _: Oop, args: *const Operand, count: usize) -> Outcome {
    let Some([index]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid(); };
    let Some(index) = index.as_word() else { return invalid(); };
    let (_, storage) = unsafe { selection(context) };
    Outcome::loaded(value(unsafe { super::name_at(storage, index) }))
}
unsafe extern "C" fn reverse(context: *mut c_void, _: Oop, args: *const Operand, count: usize) -> Outcome {
    let Some([argument]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid(); };
    let Some(object) = argument.as_object() else { return invalid(); };
    let (_, storage) = unsafe { selection(context) };
    Outcome::loaded(value(unsafe { super::reverse(storage, object) }))
}
unsafe extern "C" fn roots(mark: birnam_abi::MarkFn) {
    let namespace = NAMESPACE.load(Ordering::Acquire);
    if namespace.is_null() { return; }
    let snapshot = {
        let namespace = unsafe { &*namespace };
        let view = unsafe { super::bindings(namespace.owned.storage) };
        let mut snapshot = vec![namespace.owned.receiver];
        for binding in unsafe { birnam_abi::borrowed_slice(view.bindings, view.count) }.unwrap() {
            snapshot.extend([binding.name, binding.value]);
        }
        snapshot
    };
    // No native namespace borrow crosses the collector's mark callback.
    for object in snapshot { if !object.is_null() { unsafe { mark(object) }; } }
}
unsafe extern "C" fn destroy(context: *mut c_void) {
    let namespace = context.cast::<Namespace>();
    let _ = NAMESPACE.compare_exchange(namespace, ptr::null_mut(), Ordering::AcqRel, Ordering::Acquire);
    let namespace = unsafe { Box::from_raw(namespace) };
    unsafe { super::destroy(namespace.owned.storage) };
}
fn construct(id: Protocol, registry: root_registry::OwnedV1, records: lifetime::OwnedV1, values: base_values::ValuesV1) -> Result<*const OwnedV1, Outcome> {
    let prototype = send(id, id.receiver, intern(id, b"objectPrototype")?, &[])?;
    let parent = send(id, id.receiver, intern(id, b"behaviorOf:")?, &[prototype])?.as_object().ok_or_else(invalid)?;
    let behavior = send(id, parent, intern(id, b"delegated")?, &[])?.as_object().ok_or_else(invalid)?;
    unsafe { birnam_abi::payload_layout::set(records.layouts, behavior, ptr::null(), 0, birnam_abi::SLOT_RAW) };
    let receiver = send(id, behavior, intern(id, b"allocate")?, &[Operand::word(0)])?.as_object().ok_or_else(invalid)?;
    let mut namespace = Box::new(Namespace {
        owned: OwnedV1 { version: 1, byte_size: size_of::<OwnedV1>(), context: ptr::null_mut(), api: &super::API,
            storage: unsafe { super::create() }, receiver, destroy: Some(destroy) },
        id,
    });
    namespace.owned.context = ptr::from_mut(namespace.as_mut()).cast();
    let namespace = Box::into_raw(namespace);
    // Publish a complete owner and root it before entering any more Id methods.
    // Code is pinned even if a later installation fails.
    NAMESPACE.store(namespace, Ordering::Release);
    unsafe { root_registry::register(registry.api, roots) };
    let context = namespace.cast();
    for (name, method) in [
        (b"bindingAt:".as_slice(), find as native::Method),
        (b"bindingAt:put:".as_slice(), bind as native::Method),
        (b"bindingCount".as_slice(), count as native::Method),
        (b"bindingNameAt:".as_slice(), name_at as native::Method),
        (b"nameOf:".as_slice(), reverse as native::Method),
    ] {
        send(id, behavior, intern(id, b"addMethod")?, &[Operand::object(intern(id, name)?), Operand::entry(NativeEntry::new(method, context))])?;
    }
    let symbol = intern(id, b"Symbol")?;
    let symbol_behavior = send(id, id.receiver, intern(id, b"behaviorOf:")?, &[Operand::object(symbol)])?.as_object().ok_or_else(invalid)?;
    // Publish only completed predecessors, with their existing production names.
    for (name, value) in [
        (b"Namespace".as_slice(), receiver), (b"Object".as_slice(), prototype.as_object().unwrap()),
        (b"Boolean".as_slice(), values.true_object), (b"False".as_slice(), values.false_object),
        (b"Vtable".as_slice(), id.receiver), (b"ObjectVt".as_slice(), parent),
        (b"Symbol".as_slice(), symbol), (b"SymbolVt".as_slice(), symbol_behavior),
    ] {
        let key = intern(id, name)?;
        unsafe { super::bind((*namespace).owned.storage, key, value) };
    }
    Ok(unsafe { &raw const (*namespace).owned })
}
unsafe extern "C" fn initialize(id: *const Protocol, args: *const Operand, count: usize) -> Outcome {
    if id.is_null() { return invalid(); }
    let Some([registry, records, values]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid(); };
    let (Some(registry), Some(records), Some(values)) = (registry.as_word(), records.as_word(), values.as_word()) else { return invalid(); };
    let (registry, records, values) = (registry as *const root_registry::OwnedV1, records as *const lifetime::OwnedV1, values as *const base_values::ValuesV1);
    if !unsafe { root_registry::valid_owned(registry) && lifetime::valid(records) && base_values::valid(values) } { return invalid(); }
    let id = unsafe { *id };
    // Duplicate construction is an error, including after partial publication.
    // The caller owns the first result; Id pins published code on failure.
    if !NAMESPACE.load(Ordering::Acquire).is_null() { return invalid(); }
    match construct(id, unsafe { *registry }, unsafe { *records }, unsafe { *values }) {
        Ok(owner) => Outcome::loaded(Operand::word(owner as usize)), Err(error) => error,
    }
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module { version: 1, byte_size: size_of::<native::Module>() as u32, initialize: Some(initialize) };
