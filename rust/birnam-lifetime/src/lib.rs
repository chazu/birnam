//! Payload descriptions and native callback records above Id, before collection.
use birnam_abi::{
    Oop, finalizers,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
    lifetime::{self as abi, AccessV1, OwnedV1},
    payload_layout as layouts, trace_hooks as hooks,
};
use core::{
    ffi::{c_int, c_void},
    ptr,
};
use std::collections::HashMap;

struct State {
    owner: OwnedV1,
    thread: std::thread::ThreadId,
    access: AccessV1,
    id: Protocol,
    references: NativeEntry,
    layouts: HashMap<usize, layouts::OwnedV1>,
    hooks: HashMap<usize, hooks::SlotV1>,
    finalizers: HashMap<usize, finalizers::SlotV1>,
}
unsafe fn check(context: *mut c_void) {
    assert_eq!(
        unsafe { (*context.cast::<State>()).thread },
        std::thread::current().id(),
        "Lifetime used from another thread"
    );
}
unsafe extern "C" fn admit(context: *mut c_void) {
    unsafe { check(context) };
    let access = unsafe { (*context.cast::<State>()).access };
    unsafe { (access.admit)(access.context) };
}
unsafe extern "C" fn initial_admit(context: *mut c_void) {
    unsafe { check(context) };
}
unsafe extern "C" fn ignore(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::unit())
}
unsafe extern "C" fn initial_facts(context: *mut c_void, object: Oop) -> u32 {
    let (entry, receiver) = unsafe {
        let state = &*context.cast::<State>();
        (state.references, state.id.receiver)
    };
    let args = [
        Operand::object(object),
        Operand::entry(NativeEntry::new(ignore, ptr::null_mut())),
    ];
    let answer =
        unsafe { entry.method.unwrap()(entry.context, receiver, args.as_ptr(), args.len()) };
    assert_eq!(answer.status, native::LOADED, "Id lifetime facts failed");
    answer.value.as_word().map_or(0, |kind| {
        abi::ALLOCATED | if kind == 1 { abi::BEHAVIOR } else { 0 }
    })
}
unsafe fn facts(context: *mut c_void, object: Oop) -> u32 {
    let access = unsafe { (*context.cast::<State>()).access };
    unsafe { (access.facts)(access.context, object) }
}
unsafe fn require_behavior(context: *mut c_void, object: Oop) {
    assert_ne!(
        unsafe { facts(context, object) } & abi::BEHAVIOR,
        0,
        "unregistered vtable"
    );
}
unsafe extern "C" fn layout(context: *mut c_void, object: Oop) -> layouts::ViewV1 {
    unsafe { check(context) };
    unsafe { &*context.cast::<State>() }
        .layouts
        .get(&object.addr())
        .map_or(layouts::ViewV1::UNSET, |owned| owned.view)
}
unsafe extern "C" fn replace_layout(context: *mut c_void, object: Oop, value: layouts::OwnedV1) {
    unsafe { check(context) };
    let (previous, release) = unsafe {
        let state = &mut *context.cast::<State>();
        (
            state.layouts.insert(object.addr(), value),
            (*state.owner.layout_factory).release.unwrap(),
        )
    };
    // Release only the owned bytes, after ending the registry borrow.
    if let Some(previous) = previous {
        unsafe { release(previous) };
    }
}
unsafe extern "C" fn public_replace_layout(
    context: *mut c_void,
    object: Oop,
    value: layouts::OwnedV1,
) {
    unsafe {
        require_behavior(context, object);
        replace_layout(context, object, value)
    };
}
unsafe extern "C" fn public_layout(context: *mut c_void, object: Oop) -> layouts::ViewV1 {
    unsafe {
        require_behavior(context, object);
        layout(context, object)
    }
}
unsafe extern "C" fn hook(context: *mut c_void, object: Oop) -> hooks::SlotV1 {
    unsafe { check(context) };
    unsafe { &*context.cast::<State>() }
        .hooks
        .get(&object.addr())
        .copied()
        .unwrap_or(hooks::SlotV1::EMPTY)
}
unsafe extern "C" fn replace_hook(context: *mut c_void, object: Oop, value: hooks::SlotV1) {
    unsafe { check(context) };
    let state = unsafe { &mut *context.cast::<State>() };
    if value.hook.is_none() {
        state.hooks.remove(&object.addr());
    } else {
        state.hooks.insert(object.addr(), value);
    }
}
unsafe extern "C" fn public_replace_hook(context: *mut c_void, object: Oop, value: hooks::SlotV1) {
    unsafe {
        require_behavior(context, object);
        replace_hook(context, object, value)
    };
}
unsafe extern "C" fn finalizer(context: *mut c_void, object: Oop) -> finalizers::SlotV1 {
    unsafe { check(context) };
    unsafe { &*context.cast::<State>() }
        .finalizers
        .get(&object.addr())
        .copied()
        .unwrap_or(finalizers::SlotV1::EMPTY)
}
unsafe extern "C" fn replace_finalizer(
    context: *mut c_void,
    object: Oop,
    value: finalizers::SlotV1,
) {
    unsafe { check(context) };
    let state = unsafe { &mut *context.cast::<State>() };
    if value.callback.is_none() {
        state.finalizers.remove(&object.addr());
    } else {
        state.finalizers.insert(object.addr(), value);
    }
}
unsafe extern "C" fn read_permanent(
    context: *mut c_void,
    object: Oop,
    output: *mut c_int,
) -> c_int {
    let facts = unsafe { facts(context, object) };
    if facts & abi::ALLOCATED == 0 {
        return 0;
    }
    unsafe { output.write(c_int::from(facts & abi::PERMANENT != 0)) };
    1
}
unsafe extern "C" fn retire(context: *mut c_void, addresses: *const usize, count: usize) {
    unsafe { check(context) };
    let addresses = unsafe { birnam_abi::borrowed_slice(addresses, count) }
        .expect("invalid lifetime retirement addresses");
    let release = unsafe {
        (*(*context.cast::<State>()).owner.layout_factory)
            .release
            .unwrap()
    };
    for address in addresses {
        let previous = unsafe {
            let state = &mut *context.cast::<State>();
            state.hooks.remove(address);
            state.layouts.remove(address)
        };
        if let Some(previous) = previous {
            unsafe { release(previous) };
        }
    }
}
unsafe extern "C" fn set_access(context: *mut c_void, access: AccessV1) {
    unsafe {
        check(context);
        (*context.cast::<State>()).access = access;
    }
}
unsafe extern "C" fn destroy(context: *mut c_void) {
    unsafe { check(context) };
    let mut state = unsafe { Box::from_raw(context.cast::<State>()) };
    let owner = state.owner;
    for (_, record) in state.layouts.drain() {
        unsafe { (*owner.layout_factory).release.unwrap()(record) };
    }
    unsafe {
        (*owner.layout_factory).destroy.unwrap()(owner.layouts);
        (*owner.hook_factory).destroy.unwrap()(owner.hooks);
        (*owner.finalizer_factory).destroy.unwrap()(owner.finalizers);
    }
}
fn bind(id: Protocol, name: &[u8]) -> Result<NativeEntry, Outcome> {
    let send = |selector, args: &[Operand]| {
        let answer = unsafe {
            id.message.unwrap()(id.context, id.receiver, selector, args.as_ptr(), args.len())
        };
        if answer.status == native::LOADED {
            Ok(answer.value)
        } else {
            Err(answer)
        }
    };
    let lookup = send(id.intern, &[Operand::bytes(b"lookup")])?
        .as_object()
        .ok_or_else(invalid)?;
    let selector = send(id.intern, &[Operand::bytes(name)])?;
    send(lookup, &[selector])?
        .as_entry()
        .filter(|entry| entry.method.is_some())
        .ok_or_else(invalid)
}
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
unsafe extern "C" fn initialize(
    id: *const Protocol,
    args: *const Operand,
    count: usize,
) -> Outcome {
    if id.is_null() {
        return invalid();
    }
    let Some([layout_factory, hook_factory, finalizer_factory]) =
        (unsafe { birnam_abi::borrowed_slice(args, count) })
    else {
        return invalid();
    };
    let Some((layout_factory, hook_factory, finalizer_factory)) = layout_factory
        .as_word()
        .zip(hook_factory.as_word())
        .zip(finalizer_factory.as_word())
        .map(|((l, h), f)| {
            (
                l as *const layouts::FactoryV1,
                h as *const hooks::FactoryV1,
                f as *const finalizers::FactoryV1,
            )
        })
    else {
        return invalid();
    };
    if !unsafe {
        layouts::valid_factory(layout_factory)
            && hooks::valid_factory(hook_factory)
            && finalizers::valid_factory(finalizer_factory)
    } {
        return invalid();
    }
    let id = unsafe { *id };
    let references = match bind(id, b"referencesOf:do:") {
        Ok(entry) => entry,
        Err(answer) => return answer,
    };
    let mut state = Box::new(State {
        owner: OwnedV1 {
            version: 1,
            byte_size: size_of::<OwnedV1>(),
            context: ptr::null_mut(),
            layouts: ptr::null_mut(),
            hooks: ptr::null_mut(),
            finalizers: ptr::null_mut(),
            layout_factory,
            hook_factory,
            finalizer_factory,
            layout,
            replace_layout,
            hook,
            replace_hook,
            finalizer,
            replace_finalizer,
            retire,
            set_access,
            destroy,
        },
        thread: std::thread::current().id(),
        access: AccessV1 {
            context: ptr::null_mut(),
            admit: initial_admit,
            facts: initial_facts,
        },
        id,
        references,
        layouts: HashMap::new(),
        hooks: HashMap::new(),
        finalizers: HashMap::new(),
    });
    let context = ptr::from_mut(&mut *state).cast();
    state.owner.context = context;
    state.access.context = context;
    unsafe {
        state.owner.layouts = (*layout_factory).create.unwrap()(&layouts::FloorV1 {
            version: 1,
            byte_size: size_of::<layouts::FloorV1>(),
            context,
            admit: Some(admit),
            replace: Some(public_replace_layout),
            view: Some(public_layout),
        });
        state.owner.hooks = (*hook_factory).create.unwrap()(&hooks::FloorV1 {
            version: 1,
            byte_size: size_of::<hooks::FloorV1>(),
            context,
            admit: Some(admit),
            replace: Some(public_replace_hook),
        });
        state.owner.finalizers = (*finalizer_factory).create.unwrap()(&finalizers::FloorV1 {
            version: 1,
            byte_size: size_of::<finalizers::FloorV1>(),
            context,
            admit: Some(admit),
            read_permanent: Some(read_permanent),
            replace: Some(replace_finalizer),
        });
    }
    assert!(unsafe { abi::valid(&state.owner) });
    let state = Box::into_raw(state);
    Outcome::loaded(Operand::word(unsafe { &raw const (*state).owner } as usize))
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module {
    version: 1,
    byte_size: size_of::<native::Module>() as u32,
    initialize: Some(initialize),
};
