//! Live method lookup. Callers keep the receiver and arguments alive before
//! lookup can call user code; the selected method is then entered directly.
use super::{native_id::State, object_memory::ObjectMemory};
use crate::abi::Oop;
pub(crate) unsafe fn bind(s: &State, r: Oop, selector: Oop) -> Result<Option<Oop>, Oop> {
    let behavior = s.behavior(r)?;
    // The validated receiver's vtable was admitted when assigned and is kept
    // alive by that receiver. Read its current vtable, without a second check
    // of the same allocation. This does not cache method selection.
    // A detached receiver's former vtable may already have been released.
    // Keep the checked path during finalization; the live path has the
    // receiver's strong reference to its admitted vtable.
    let has_detached = !s.detached.borrow().is_empty();
    let meta = if !has_detached {
        unsafe { ObjectMemory::vtable_of_valid_object(behavior) }
    } else {
        s.behavior(behavior)?
    };
    // A self-metavtable supplies the bootstrap base case for lookup. The
    // default fast path is valid only at such a root; a custom meta-meta lookup
    // must still be consulted for other objects in the behavior graph.
    if has_detached { s.behavior(meta)?; }
    if (behavior == meta && selector == s.lookup)
        || s.tables.borrow().has_default_root_lookup(meta, s.lookup, s.default_lookup.get())
    {
        let method = s.tables.borrow().lookup(behavior, selector);
        return Ok(method);
    }
    let method = unsafe { send(s, behavior, s.lookup, &[selector]) }?;
    if method.is_null() {
        Ok(None)
    } else {
        Ok(Some(method))
    }
}
pub(crate) unsafe fn send(s: &State, r: Oop, selector: Oop, args: &[Oop]) -> Result<Oop, Oop> {
    let r=crate::abi::object_reference(r);
    let selector=crate::abi::object_reference(selector);
    s.check()?;
    let scope = s.scope(r, selector, args);
    if let Some(method) = unsafe { bind(s, r, selector) }? {
        return unsafe { s.enter_method(method, r, args, &scope) };
    }
    if selector == s.dnu || (selector == s.lookup && s.behavior(r)? == s.meta) {
        return Err(s.unbound());
    }
    let Some(handler) = unsafe { bind(s, r, s.dnu) }? else {
        return Err(s.unbound());
    };
    let mut forwarded = Vec::with_capacity(args.len() + 1);
    forwarded.push(selector);
    forwarded.extend_from_slice(args);
    unsafe { s.enter_method(handler, r, &forwarded, &scope) }
}
