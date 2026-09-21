//! Sequential Id loading with ownership of early and managed module results.
use birnam_abi::{
    Oop,
    bootstrap::OwnedV2,
    id_protocol::{self as native, Operand, Outcome, Protocol},
    root_registry,
};
use core::{ffi::c_void, ptr};

// Both variants implement complete retention. Moving between them transfers
// real roots; there is no empty callback waiting for a later module.
enum Roots {
    Early(Vec<(usize, Oop)>),
    Managed {
        registry: root_registry::OwnedV1,
        entries: Vec<(usize, usize)>,
    },
}
struct State {
    owner: OwnedV2,
    id: Protocol,
    thread: std::thread::ThreadId,
    roots: Roots,
    results: Vec<(usize, Operand)>,
    next: usize,
    depth: usize,
    adopting: bool,
}
unsafe fn check(context: *mut c_void) {
    assert_eq!(
        unsafe { (*context.cast::<State>()).thread },
        std::thread::current().id(),
        "Bootstrap used from another thread"
    );
}
unsafe fn objects(value: Operand) -> Vec<Oop> {
    if let Some(object) = value.as_object() {
        return (!object.is_null()).then_some(object).into_iter().collect();
    }
    if let Some(protocol) = value.as_protocol().filter(|pointer| !pointer.is_null()) {
        let protocol = unsafe { *protocol };
        return [protocol.receiver, protocol.intern, protocol.load]
            .into_iter()
            .filter(|object| !object.is_null())
            .collect();
    }
    Vec::new()
}
unsafe fn next(context: *mut c_void) -> usize {
    let state = unsafe { &mut *context.cast::<State>() };
    state.next = state
        .next
        .checked_add(1)
        .expect("Bootstrap retention IDs exhausted");
    state.next
}
unsafe fn retain(context: *mut c_void, group: usize, objects: &[Oop]) {
    let registry = unsafe {
        let state = &mut *context.cast::<State>();
        match &mut state.roots {
            Roots::Early(entries) => {
                entries.extend(objects.iter().map(|object| (group, *object)));
                return;
            }
            Roots::Managed { registry, .. } => *registry,
        }
    };
    for object in objects {
        // Admission can reenter loading. No Bootstrap borrow crosses this call.
        let token = unsafe { registry.retain.unwrap()(registry.context, *object) };
        assert_ne!(token, 0, "root registry rejected a non-null Bootstrap root");
        let Roots::Managed { entries, .. } = (unsafe { &mut (*context.cast::<State>()).roots })
        else {
            unreachable!()
        };
        entries.push((group, token));
    }
}
unsafe fn release(context: *mut c_void, group: Option<usize>) {
    let released = unsafe {
        let state = &mut *context.cast::<State>();
        match &mut state.roots {
            Roots::Early(entries) => {
                entries.retain(|(id, _)| group.is_some_and(|group| group != *id));
                None
            }
            Roots::Managed { registry, entries } => {
                let mut tokens = Vec::new();
                entries.retain(|(id, token)| {
                    if group.is_none_or(|group| group == *id) {
                        tokens.push(*token);
                        false
                    } else {
                        true
                    }
                });
                Some((*registry, tokens))
            }
        }
    };
    if let Some((registry, tokens)) = released {
        for token in tokens {
            assert_eq!(
                unsafe { registry.release.unwrap()(registry.context, token) },
                1
            );
        }
    }
}
unsafe extern "C" fn load(
    context: *mut c_void,
    path: *const u8,
    length: usize,
    args: *const Operand,
    count: usize,
) -> Outcome {
    unsafe { check(context) };
    if unsafe { (*context.cast::<State>()).adopting } {
        return invalid();
    }
    let Some(path) = (unsafe { birnam_abi::borrowed_bytes(path, length) }) else {
        return invalid();
    };
    let Some(args) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else {
        return invalid();
    };
    let input = unsafe { next(context) };
    unsafe { (*context.cast::<State>()).depth += 1 };
    for argument in args {
        unsafe { retain(context, input, &objects(*argument)) };
    }
    let id = unsafe { (*context.cast::<State>()).id };
    let mut request = vec![Operand::bytes(path)];
    request.extend_from_slice(args);
    let answer = unsafe {
        id.message.unwrap()(
            id.context,
            id.receiver,
            id.load,
            request.as_ptr(),
            request.len(),
        )
    };
    let roots = unsafe { objects(answer.value) };
    if !roots.is_empty() {
        let result = unsafe { next(context) };
        unsafe {
            retain(context, result, &roots);
            (*context.cast::<State>())
                .results
                .push((result, answer.value));
        }
    }
    // A result is retained before its input roots are removed, including when
    // an initializer returns one of its arguments or reenters this loader.
    unsafe {
        release(context, Some(input));
        (*context.cast::<State>()).depth -= 1;
    }
    answer
}
unsafe extern "C" fn adopt_roots(
    context: *mut c_void,
    registry: *const root_registry::OwnedV1,
) -> u32 {
    unsafe { check(context) };
    if !unsafe { root_registry::valid_owned(registry) } {
        return native::INVALID_REQUEST;
    }
    let registry = unsafe { *registry };
    let early = unsafe {
        let state = &mut *context.cast::<State>();
        if state.depth != 0 || state.adopting {
            return native::INVALID_REQUEST;
        }
        match &state.roots {
            Roots::Managed {
                registry: current, ..
            } => {
                return if current.context == registry.context {
                    native::LOADED
                } else {
                    native::INVALID_REQUEST
                };
            }
            Roots::Early(entries) => {
                state.adopting = true;
                entries.clone()
            }
        }
    };
    let mut entries = Vec::with_capacity(early.len());
    for (group, object) in early {
        let token = unsafe { registry.retain.unwrap()(registry.context, object) };
        assert_ne!(token, 0, "root registry rejected a Bootstrap transfer");
        entries.push((group, token));
    }
    unsafe {
        let state = &mut *context.cast::<State>();
        state.roots = Roots::Managed { registry, entries };
        state.adopting = false;
    }
    native::LOADED
}
fn same_result(first: Operand, second: Operand) -> bool {
    match (first.kind(), second.kind()) {
        (native::OBJECT, native::OBJECT) => first.as_object() == second.as_object(),
        (native::PROTOCOL, native::PROTOCOL) => first.as_protocol() == second.as_protocol(),
        _ => false,
    }
}
unsafe extern "C" fn release_result(context: *mut c_void, result: Operand) -> usize {
    unsafe { check(context) };
    let group = unsafe {
        let state = &mut *context.cast::<State>();
        if state.adopting {
            return 0;
        }
        let Some(index) = state
            .results
            .iter()
            .position(|(_, value)| same_result(*value, result))
        else {
            return 0;
        };
        state.results.swap_remove(index).0
    };
    unsafe { release(context, Some(group)) };
    1
}
unsafe extern "C" fn destroy(context: *mut c_void) {
    unsafe { check(context) };
    assert!(
        unsafe { (*context.cast::<State>()).depth == 0 && !(*context.cast::<State>()).adopting },
        "Bootstrap destroyed during construction"
    );
    // Reject nested entry while releasing the root records, then free the owner.
    unsafe {
        (*context.cast::<State>()).adopting = true;
        release(context, None);
        drop(Box::from_raw(context.cast::<State>()));
    }
}
unsafe extern "C" fn pause(context: *mut c_void, action: native::NativeEntry) -> Outcome {
    unsafe { check(context) };
    let Some(method) = action.method else { return invalid(); };
    {
        let state = unsafe { &mut *context.cast::<State>() };
        if state.depth != 0 || state.adopting || !matches!(state.roots, Roots::Managed { .. }) {
            return invalid();
        }
        state.adopting = true;
    }
    let result = unsafe { method(action.context, ptr::null_mut(), ptr::null(), 0) };
    unsafe { (*context.cast::<State>()).adopting = false };
    result
}
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
unsafe extern "C" fn initialize(id: *const Protocol, _: *const Operand, count: usize) -> Outcome {
    if id.is_null() || count != 0 {
        return invalid();
    }
    let id = unsafe { *id };
    let mut state = Box::new(State {
        owner: OwnedV2 {
            version: 2,
            byte_size: size_of::<OwnedV2>(),
            context: ptr::null_mut(),
            load,
            adopt_roots,
            release_result,
            destroy,
            pause,
        },
        id,
        thread: std::thread::current().id(),
        roots: Roots::Early(Vec::new()),
        results: Vec::new(),
        next: 0,
        depth: 0,
        adopting: false,
    });
    state.owner.context = ptr::from_mut(&mut *state).cast();
    let state = Box::into_raw(state);
    Outcome::loaded(Operand::word(unsafe { &raw const (*state).owner } as usize))
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module {
    version: 1,
    byte_size: size_of::<native::Module>() as u32,
    initialize: Some(initialize),
};
