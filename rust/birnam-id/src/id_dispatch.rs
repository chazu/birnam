//! Id's metacircular binding and fixed native invocation. No upper runtime policy.
use crate::abi::Oop;
use crate::boundary;
use crate::object_memory::ObjectMemory;
use crate::raw::MethodAddress;
use crate::vtable::Vtables;
use core::cell::Cell;

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
/// The caller serializes this live dictionary registry on its owning thread.
/// No reference into it may span a method invocation. The dynamic extent must
/// end before moving or releasing the registry or its objects.
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Unbound;

/// # Safety
/// An Id entry scope and live receiver/selector are required. Native lookup
/// implementations return only null or a correctly typed, live Method address.
pub(crate) unsafe fn bind(receiver: Oop, selector: Oop) -> Result<Option<MethodAddress>, Unbound> {
    let context = context();
    let behavior = unsafe { ObjectMemory::vtable_of_valid_object(receiver) };
    if behavior.is_null() {
        return Ok(None);
    }
    if behavior == context.metavtable && selector == context.lookup {
        return Ok(unsafe { (&*context.dictionaries).lookup(context.metavtable, context.lookup) });
    }
    let method = unsafe { send(behavior, context.lookup, &[selector]) }?;
    Ok(unsafe { MethodAddress::from_oop(method) })
}

/// # Safety
/// The entry scope and native Method contracts apply. Arguments remain live
/// throughout lookup and invocation. Upper callers retain reclaimable code for
/// the complete call; Id neither classifies it nor owns execution turns.
pub(crate) unsafe fn send(receiver: Oop, selector: Oop, arguments: &[Oop]) -> Result<Oop, Unbound> {
    if let Some(method) = unsafe { bind(receiver, selector) }? {
        return Ok(unsafe { method.to_method()(receiver, arguments.as_ptr(), arguments.len()) });
    }
    let context = context();
    // Missing root lookup cannot be resolved through another lookup of itself.
    if selector == context.does_not_understand
        || (selector == context.lookup
            && unsafe { ObjectMemory::vtable_of_valid_object(receiver) } == context.metavtable)
    {
        return Err(Unbound);
    }
    let Some(handler) = (unsafe { bind(receiver, context.does_not_understand) })? else {
        return Err(Unbound);
    };
    let mut forwarded = [core::ptr::null_mut(); 17];
    forwarded[0] = selector;
    let count = arguments.len().min(16);
    forwarded[1..1 + count].copy_from_slice(&arguments[..count]);
    Ok(unsafe { handler.to_method()(receiver, forwarded.as_ptr(), count + 1) })
}

pub(crate) unsafe extern "C" fn vtable_lookup_method(
    receiver: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    if count == 0 || args.is_null() {
        boundary::fatal_invariant("vtable lookup requires one selector");
    }
    let selector = unsafe { args.read() };
    unsafe { (&*context().dictionaries).lookup(receiver, selector) }
        .map(MethodAddress::as_oop)
        .unwrap_or(core::ptr::null_mut())
}

pub(crate) unsafe extern "C" fn vtable_add_method(
    receiver: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    if count < 2 || args.is_null() {
        boundary::fatal_invariant("vtable addMethod requires selector and method");
    }
    let selector = unsafe { args.read() };
    let encoded = unsafe { args.add(1).read() };
    let method = unsafe { MethodAddress::from_oop(encoded) }
        .unwrap_or_else(|| boundary::fatal_invariant("vtable addMethod received null method"));
    unsafe { (&mut *context().dictionaries).add_method(receiver, selector, method.to_method()) };
    encoded
}

pub(crate) unsafe extern "C" fn vtable_remove_method(
    receiver: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    if count == 0 || args.is_null() {
        boundary::fatal_invariant("vtable removeMethod: requires one selector");
    }
    let selector = unsafe { args.read() };
    unsafe { (&mut *context().dictionaries).remove_method(receiver, selector) }
        .map(MethodAddress::as_oop)
        .unwrap_or(core::ptr::null_mut())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::initial_objects::InitialObjects;
    unsafe extern "C" fn identity(receiver: Oop, _: *const Oop, _: usize) -> Oop {
        receiver
    }
    unsafe extern "C" fn dnu(_: Oop, args: *const Oop, count: usize) -> Oop {
        assert_eq!(count, 2);
        unsafe { args.add(1).read() }
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
                &mut id.dictionaries,
                id.vtable_vt,
                id.lookup,
                id.does_not_understand,
                || {
                    assert_eq!(
                        send(
                            id.object_vt,
                            add,
                            &[selector, MethodAddress::from_method(identity).as_oop()]
                        ),
                        Ok(MethodAddress::from_method(identity).as_oop())
                    );
                    assert_eq!(
                        send(id.object_prototype, selector, &[]),
                        Ok(id.object_prototype)
                    );
                    assert_eq!(send(id.object_prototype, missing, &[]), Err(Unbound));
                    send(
                        id.object_vt,
                        add,
                        &[
                            id.does_not_understand,
                            MethodAddress::from_method(dnu).as_oop(),
                        ],
                    )
                    .unwrap();
                    assert_eq!(
                        send(id.object_prototype, missing, &[id.symbol_prototype]),
                        Ok(id.symbol_prototype)
                    );
                    send(id.vtable_vt, remove, &[id.lookup]).unwrap();
                    assert_eq!(send(id.object_prototype, selector, &[]), Err(Unbound));
                },
            )
        };
    }
}
