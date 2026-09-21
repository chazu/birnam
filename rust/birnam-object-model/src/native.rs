//! Private-behavior records and construction, using only completed predecessors.
use birnam_abi::{Oop, base_values, namespace_owner, object_model::OwnedV1, root_registry,
    vtable_enumeration::ApiV2, id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol}};
use core::{ffi::c_void, ptr};
use std::{collections::HashMap, sync::atomic::{AtomicPtr, Ordering}};
struct Model {
    owned: OwnedV1,
    enumeration: ApiV2,
    id: Protocol,
    roots: root_registry::OwnedV1,
    owners: HashMap<usize, Oop>,
    references: NativeEntry,
    count: NativeEntry,
    at: NativeEntry,
    selector_at: NativeEntry,
}
static MODEL: AtomicPtr<Model> = AtomicPtr::new(ptr::null_mut());
fn invalid() -> Outcome { Outcome::failed(native::INVALID_REQUEST, 0) }
fn object(value: Oop) -> Operand { if value.is_null() { Operand::unit() } else { Operand::object(value) } }
fn invoke(entry: NativeEntry, receiver: Oop, args: &[Operand]) -> Outcome {
    unsafe { entry.method.unwrap()(entry.context, receiver, args.as_ptr(), args.len()) }
}
fn send(id: Protocol, receiver: Oop, selector: Oop, args: &[Operand]) -> Result<Operand, Outcome> {
    let result = unsafe { id.message.unwrap()(id.context, receiver, selector, args.as_ptr(), args.len()) };
    if result.status == native::LOADED { Ok(result.value) } else { Err(result) }
}
fn intern(id: Protocol, name: &[u8]) -> Result<Oop, Outcome> {
    send(id, id.receiver, id.intern, &[Operand::bytes(name)])?.as_object().ok_or_else(invalid)
}
fn select(id: Protocol, name: &[u8]) -> Result<NativeEntry, Outcome> {
    send(id, id.receiver, intern(id, b"lookup")?, &[Operand::object(intern(id, name)?)])?.as_entry().ok_or_else(invalid)
}
unsafe extern "C" fn owner(context: *const c_void, behavior: Oop) -> Oop {
    unsafe { (*context.cast::<Model>()).owners.get(&behavior.addr()).copied().unwrap_or(ptr::null_mut()) }
}
unsafe extern "C" fn set_owner(context: *mut c_void, behavior: Oop, object: Oop) {
    let owners = unsafe { &mut (*context.cast::<Model>()).owners };
    if object.is_null() { owners.remove(&behavior.addr()); } else { owners.insert(behavior.addr(), object); }
}
unsafe extern "C" fn retire(context: *mut c_void, addresses: *const usize, count: usize) {
    let addresses = unsafe { birnam_abi::borrowed_slice(addresses, count) }.unwrap();
    let owners = unsafe { &mut (*context.cast::<Model>()).owners };
    for address in addresses { owners.remove(address); }
}
unsafe extern "C" fn destroy(context: *mut c_void) {
    let model = context.cast::<Model>();
    let _ = MODEL.compare_exchange(model, ptr::null_mut(), Ordering::AcqRel, Ordering::Acquire);
    unsafe { drop(Box::from_raw(model)) };
}
unsafe extern "C" fn count(context: *mut c_void) -> usize {
    let (entry, receiver) = unsafe { ((*context.cast::<Model>()).count, (*context.cast::<Model>()).id.receiver) };
    let result = invoke(entry, receiver, &[]); assert_eq!(result.status, native::LOADED);
    result.value.as_word().unwrap()
}
unsafe extern "C" fn at(context: *mut c_void, index: usize) -> Oop {
    let (entry, receiver) = unsafe { ((*context.cast::<Model>()).at, (*context.cast::<Model>()).id.receiver) };
    let result = invoke(entry, receiver, &[Operand::word(index)]); assert_eq!(result.status, native::LOADED);
    result.value.as_object().unwrap_or(ptr::null_mut())
}
unsafe extern "C" fn selector_at(context: *mut c_void, behavior: Oop, index: usize) -> Oop {
    let (entry, receiver) = unsafe { ((*context.cast::<Model>()).selector_at, (*context.cast::<Model>()).id.receiver) };
    let result = invoke(entry, receiver, &[Operand::word(index), Operand::object(behavior)]);
    assert_eq!(result.status, native::LOADED, "selector query requires a registered behavior");
    result.value.as_object().unwrap_or(ptr::null_mut())
}
unsafe extern "C" fn ignore(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome { Outcome::loaded(Operand::unit()) }
fn registered(context: *mut c_void, behavior: Oop) -> bool {
    let (entry, receiver) = unsafe { ((*context.cast::<Model>()).references, (*context.cast::<Model>()).id.receiver) };
    let result = invoke(entry, receiver, &[Operand::object(behavior), Operand::entry(NativeEntry::new(ignore, ptr::null_mut()))]);
    result.status == native::LOADED && result.value.as_word() == Some(1)
}
unsafe extern "C" fn query_owner(context: *mut c_void, _: Oop, args: *const Operand, count: usize) -> Outcome {
    let Some([behavior]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid(); };
    let Some(behavior) = behavior.as_object() else { return invalid(); };
    Outcome::loaded(object(unsafe { owner(context, behavior) }))
}
unsafe extern "C" fn assign_owner(context: *mut c_void, _: Oop, args: *const Operand, count: usize) -> Outcome {
    let Some([behavior, value]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid(); };
    let Some(behavior) = behavior.as_object() else { return invalid(); };
    let value = if value.kind() == native::UNIT { ptr::null_mut() }
        else { let Some(value) = value.as_object() else { return invalid(); }; value };
    if !registered(context, behavior) { return invalid(); }
    unsafe { set_owner(context, behavior, value) };
    Outcome::loaded(Operand::object(behavior))
}
unsafe extern "C" fn prepare(context: *mut c_void, _: Oop, args: *const Operand, count: usize) -> Outcome {
    let Some([object]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid(); };
    if object.as_object().is_none() { return invalid(); }
    let id = unsafe { (*context.cast::<Model>()).id };
    match intern(id, b"behaviorOf:").and_then(|selector| send(id, id.receiver, selector, &[*object])) {
        Ok(value) if value.as_object().is_some() => Outcome::loaded(value), Ok(_) => invalid(), Err(error) => error,
    }
}
fn ensure(context: *mut c_void, object: Oop) -> Result<Operand, Outcome> {
    let (id, roots) = unsafe { ((*context.cast::<Model>()).id, (*context.cast::<Model>()).roots) };
    // Selectors may be retained; every send still selects the current method.
    let prepare = intern(id, b"prepareBehaviorMutationOf:")?;
    let get_owner = intern(id, b"ownerOf:")?;
    let put_owner = intern(id, b"ownerOf:put:")?;
    let delegate = intern(id, b"delegated")?;
    let attach = intern(id, b"reassociateBehaviorOf:with:")?;
    let behavior = send(id, id.receiver, prepare, &[Operand::object(object)])?.as_object().ok_or_else(invalid)?;
    if behavior == id.receiver { return Ok(Operand::object(object)); }
    if send(id, id.receiver, get_owner, &[Operand::object(behavior)])?.as_object() == Some(object) { return Ok(Operand::object(behavior)); }
    let private = send(id, behavior, delegate, &[])?.as_object().ok_or_else(invalid)?;
    let token = unsafe { roots.retain.unwrap()(roots.context, private) }; assert_ne!(token, 0);
    let result = (|| {
        send(id, id.receiver, put_owner, &[Operand::object(private), Operand::object(object)])?;
        send(id, id.receiver, attach, &[Operand::object(object), Operand::object(private)])?;
        Ok(Operand::object(private))
    })();
    assert_eq!(unsafe { roots.release.unwrap()(roots.context, token) }, 1);
    result
}
unsafe extern "C" fn ensure_private(context: *mut c_void, _: Oop, args: *const Operand, count: usize) -> Outcome {
    let Some([object]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid(); };
    let Some(object) = object.as_object().filter(|object| !object.is_null()) else { return invalid(); };
    match ensure(context, object) { Ok(value) => Outcome::loaded(value), Err(error) => error }
}
fn construct(id: Protocol, roots: root_registry::OwnedV1, namespace: namespace_owner::OwnedV1, values: base_values::ValuesV1) -> Result<*const OwnedV1, Outcome> {
    let mut model = Box::new(Model {
        owned: OwnedV1 { version: 1, byte_size: size_of::<OwnedV1>(), context: ptr::null_mut(), enumeration: ptr::null_mut(),
            owner: Some(owner), set_owner: Some(set_owner), retire: Some(retire), destroy: Some(destroy) },
        enumeration: ApiV2 { version: 2, byte_size: size_of::<ApiV2>(), context: ptr::null_mut(), count: Some(count), at: Some(at), selector_at: Some(selector_at) },
        id, roots, owners: HashMap::new(), references: select(id, b"referencesOf:do:")?,
        count: select(id, b"behaviorCount")?, at: select(id, b"behaviorAt:")?, selector_at: select(id, b"selectorAt:in:")?,
    });
    let context = ptr::from_mut(model.as_mut()).cast();
    model.owned.context = context; model.enumeration.context = context;
    model.owned.enumeration = &raw mut model.enumeration;
    // Initialize all canonical associations from actual completed objects.
    let prototype = send(id, id.receiver, intern(id, b"objectPrototype")?, &[])?.as_object().ok_or_else(invalid)?;
    for value in [prototype, intern(id, b"Symbol")?, values.nil, values.true_object, namespace.receiver] {
        let behavior = send(id, id.receiver, intern(id, b"behaviorOf:")?, &[Operand::object(value)])?.as_object().ok_or_else(invalid)?;
        model.owners.insert(behavior.addr(), value);
    }
    let model = Box::into_raw(model);
    MODEL.store(model, Ordering::Release);
    for (name, method) in [
        (b"ownerOf:".as_slice(), query_owner as native::Method),
        (b"ownerOf:put:".as_slice(), assign_owner as native::Method),
        (b"prepareBehaviorMutationOf:".as_slice(), prepare as native::Method),
        (b"ensurePrivateVtableFor:".as_slice(), ensure_private as native::Method),
    ] {
        send(id, id.receiver, intern(id, b"addMethod")?, &[Operand::object(intern(id, name)?), Operand::entry(NativeEntry::new(method, context))])?;
    }
    Ok(unsafe { &raw const (*model).owned })
}
unsafe extern "C" fn initialize(id: *const Protocol, args: *const Operand, count: usize) -> Outcome {
    if id.is_null() { return invalid(); }
    let Some([roots, namespace, values]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid(); };
    let (Some(roots), Some(namespace), Some(values)) = (roots.as_word(), namespace.as_word(), values.as_word()) else { return invalid(); };
    let (roots, namespace, values) = (roots as *const root_registry::OwnedV1, namespace as *const namespace_owner::OwnedV1, values as *const base_values::ValuesV1);
    if !unsafe { root_registry::valid_owned(roots) && namespace_owner::valid(namespace) && base_values::valid(values) } || !MODEL.load(Ordering::Acquire).is_null() { return invalid(); }
    match construct(unsafe { *id }, unsafe { *roots }, unsafe { *namespace }, unsafe { *values }) {
        Ok(owner) => Outcome::loaded(Operand::word(owner as usize)), Err(error) => error,
    }
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module { version: 1, byte_size: size_of::<native::Module>() as u32, initialize: Some(initialize) };
