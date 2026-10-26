//! Portable metadata is ordinary replaceable Method behavior, not an Id tag.
use super::*;
pub(super) unsafe extern "C" fn code_value(_: Oop, c: Oop, _: Oop, _: *const Oop, n: usize) -> Oop {
    if n == 0 {
        c
    } else {
        abi::fault_reference(invalid())
    }
}
pub(super) unsafe extern "C" fn code_object(
    _: Oop,
    c: Oop,
    r: Oop,
    _: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        if n != 0 {
            return Err(invalid());
        }
        if r == s.block {
            return Ok(s.nil);
        }
        let q = s.scope(&[r]);
        let method = q.keep(s.field(r, 0)?);
        s.send(method, b"codeObject", &[])
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn admit(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [code] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[r, *code]);
        let behavior = q.keep(s.send(s.id.root, b"behaviorOf:", &[r])?);
        let private = q.keep(s.send(behavior, b"delegated", &[])?);
        let getter = q.keep(s.send(s.code_getter, b"withContext:", &[*code])?);
        s.send(
            private,
            b"addMethod",
            &[s.names[b"codeObject".as_slice()], getter],
        )?;
        s.send(s.id.root, b"reassociateBehaviorOf:with:", &[r, private])?;
        Ok(r)
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn captured(_: Oop, c: Oop, r: Oop, _: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        if n != 0 {
            return Err(invalid());
        }
        let q = s.scope(&[r]);
        let behavior = q.keep(s.send(s.id.root, b"behaviorOf:", &[r])?);
        let method = q.keep(s.invoke(s.capture_seed, r, &[])?);
        s.send(
            s.id.root,
            b"reassociateBehaviorOf:with:",
            &[method, behavior],
        )?;
        Ok(method)
    })()
    .unwrap_or_else(abi::fault_reference)
}

unsafe fn direct(s: &State, q: &Scope<'_>, selector: Oop, behavior: Oop) -> Result<Oop, Oop> {
    unsafe {
        let model = q.keep(s.named(b"ObjectModel")?);
        let visitor = q.keep(s.send(
            model,
            b"allocate:bytes:",
            &[
                s.binding_visitor_vt,
                abi::size(2 * size_of::<Oop>()).unwrap(),
            ],
        )?);
        s.send(
            model,
            b"slotAt:in:put:",
            &[abi::size(0).unwrap(), visitor, selector],
        )?;
        s.send(
            model,
            b"slotAt:in:put:",
            &[abi::size(1).unwrap(), visitor, s.nil],
        )?;
        s.send(behavior, b"bindingsDo:", &[visitor])?;
        s.send(model, b"slotAt:in:", &[abi::size(1).unwrap(), visitor])
    }
}
pub(super) unsafe extern "C" fn binding(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [key, value, _weak] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[r, *key, *value]);
        let model = q.keep(s.named(b"ObjectModel")?);
        if s.send(model, b"slotAt:in:", &[abi::size(0).unwrap(), r])? == *key {
            s.send(
                model,
                b"slotAt:in:put:",
                &[abi::size(1).unwrap(), r, *value],
            )?;
        }
        Ok(ptr::null_mut())
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn direct_method(
    _: Oop,
    c: Oop,
    _: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [selector, behavior] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[*selector, *behavior]);
        direct(s, &q, *selector, *behavior)
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn binding_matches(
    _: Oop,
    c: Oop,
    r: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [behavior, selector] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[r, *selector, *behavior]);
        let actual = q.keep(direct(s, &q, *selector, *behavior)?);
        let matches = actual != s.nil
            && abi::as_size(s.send(s.id.root, b"sameNativeMethod:as:", &[r, actual])?) == Some(1);
        s.named(if matches { b"true" } else { b"false" })
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn from_method(
    _: Oop,
    c: Oop,
    _: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [method, captures] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[*method, *captures]);
        let count = abi::as_size(s.send(*captures, b"sizeWord", &[])?).ok_or_else(invalid)?;
        let mut values = vec![*method];
        values.try_reserve(count).map_err(|_| invalid())?;
        for i in 0..count {
            values.push(q.keep(s.send(
                *captures,
                b"atWord:",
                &[abi::size(i).ok_or_else(invalid)?],
            )?));
        }
        s.make_block(s.block_vt, &values)
    })()
    .unwrap_or_else(abi::fault_reference)
}
