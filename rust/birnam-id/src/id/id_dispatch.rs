//! Metacircular binding over Method objects. Selected code and incoming values
//! are protected by Id's explicit owned invocation-root storage.
use super::native_id::State;
use crate::abi::Oop;
pub(crate) unsafe fn bind(s: &State, r: Oop, selector: Oop) -> Result<Oop, Oop> {
    let behavior = s.behavior(r)?;
    if (behavior == s.meta && selector == s.lookup)
        || (s.behavior(behavior)? == s.meta
            && s.tables
                .borrow()
                .has_default_lookup(s.meta, s.lookup, s.default_lookup.get()))
    {
        return Ok(s
            .tables
            .borrow()
            .lookup(behavior, selector)
            .unwrap_or(core::ptr::null_mut()));
    }
    let method = unsafe { send(s, behavior, s.lookup, &[selector]) }?;
    if method.is_null()
        || s.methods
            .borrow()
            .contains_key(&crate::abi::heap_address(method))
    {
        Ok(method)
    } else {
        Err(s.invalid())
    }
}
pub(crate) unsafe fn send(s: &State, r: Oop, selector: Oop, args: &[Oop]) -> Result<Oop, Oop> {
    s.check()?;
    let scope = s.scope(r, selector, args);
    let method = unsafe { bind(s, r, selector) }?;
    if !method.is_null() {
        return unsafe { s.invoke(method, r, args, &scope) };
    }
    if selector == s.dnu || (selector == s.lookup && s.behavior(r)? == s.meta) {
        return Err(s.unbound());
    }
    let handler = unsafe { bind(s, r, s.dnu) }?;
    if handler.is_null() {
        return Err(s.unbound());
    }
    // Keep all arguments; no native-stack scratch or truncating fixed array.
    let mut forwarded = Vec::with_capacity(args.len() + 1);
    forwarded.push(selector);
    forwarded.extend_from_slice(args);
    unsafe { s.invoke(handler, r, &forwarded, &scope) }
}
