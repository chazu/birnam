//! Portable metadata is ordinary replaceable Method behavior, not an Id tag.
use super::*;
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
unsafe fn owned_source(s: &State, method: Oop, q: &Scope<'_>) -> Result<Option<(Oop, bool)>, Oop> {
    unsafe {
        if let Some(code) = resumable::source_code(s, method, q)? {
            return Ok(Some((code, false)));
        }
        if let Some(code) = native_code::source(s, method, q)? {
            return Ok(Some((code, true)));
        }
        native_adapters::source(s, method, q)
    }
}
// Persistence asks the code owner for its retained body; Method payloads stay opaque.
pub(super) unsafe extern "C" fn method_body(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [method] = borrowed_slice(a, n).ok_or_else(invalid)? else { return Err(invalid()); };
        let q = s.scope(&[*method]);
        if let Some(body) = native_code::body(s, *method, &q)? { return Ok(body); }
        if let Some(body) = resumable::body(s, *method, &q)? { return Ok(body); }
        Ok(native_adapters::body(s, *method, &q)?.unwrap_or(s.nil))
    })().unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn for_dnu(_: Oop, c: Oop, r: Oop, _: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        if n != 0 {
            return Err(invalid());
        }
        let q = s.scope(&[r]);
        if let Some(method) = native_code::for_dnu(s, r, &q)? {
            return Ok(method);
        }
        Ok(resumable::for_dnu(s, r, &q)?.unwrap_or(r))
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn method_code(
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
        let q = s.scope(&[r]);
        Ok(owned_source(s, r, &q)?
            .map(|(code, _)| code)
            .unwrap_or(s.nil))
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn method_synchronous(
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
        let q = s.scope(&[r]);
        s.named(if owned_source(s, r, &q)?.is_some_and(|(_, synchronous)| !synchronous) {
            b"false"
        } else {
            b"true"
        })
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
        // The Method owner preserves behavior when it allocates the captured
        // executable. Capturing does not mutate a dispatch binding.
        let method = q.keep(s.invoke(s.capture_seed, r, &[])?);
        let admitted = s.admitted.borrow().get(&r.addr()).cloned();
        if let Some(admitted) = admitted {
            s.admitted.borrow_mut().insert(method.addr(), admitted);
        }
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

pub(super) unsafe extern "C" fn block_synchronous(
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
        s.send(method, b"isSynchronous", &[])
    })()
    .unwrap_or_else(abi::fault_reference)
}
// Checked code shapes establish capture layout at construction. Reflection
// is needed only for legacy entries without such a description.
pub(super) unsafe fn inspect_captures(s: &State, r: Oop) -> Result<Option<usize>, Oop> {
    unsafe {
        if r == s.block || s.storage.borrow().family_of(r) != Some(s.block_vt) {
            return Ok(None);
        }
        let q = s.scope(&[r]);
        let method = q.keep(s.field(r, 0)?);
        let checked = s.admitted.borrow().get(&method.addr()).copied();
        if let Some(shape) = checked {
            let expected = shape
                .captures
                .checked_add(1)
                .and_then(|words| words.checked_mul(size_of::<Oop>()))
                .ok_or_else(invalid)?;
            return if s.storage.borrow().payload_size(r) == Some(expected) {
                Ok(Some(shape.captures))
            } else {
                Err(invalid())
            };
        }
        if s.send(method, b"codeObject", &[])? == s.nil {
            return Ok(None);
        }
        Ok(s.storage
            .borrow()
            .payload_size(r)
            .and_then(|bytes| (bytes / size_of::<Oop>()).checked_sub(1)))
    }
}
pub(super) unsafe fn capture_index(s: &State, value: Oop) -> Result<Option<usize>, Oop> {
    unsafe {
        if let Some(i) = abi::as_size(value) {
            return Ok(Some(i));
        }
        if value.is_null() || value == s.nil {
            return Ok(None);
        }
        let _q = s.scope(&[value]);
        let lo = abi::as_size(s.send(value, b"low32", &[])?);
        let hi = abi::as_size(s.send(value, b"high32", &[])?);
        Ok(lo
            .zip(hi)
            .filter(|(lo, hi)| *lo <= u32::MAX as usize && *hi <= 0x7fffffff)
            .map(|(lo, hi)| lo | (hi << 32)))
    }
}
pub(super) unsafe extern "C" fn capture_at(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [index] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Ok(s.nil);
        };
        let q = s.scope(&[r, *index]);
        let word = q.keep(s.send(s.named(b"Integer")?, b"wordOf:", &[*index])?);
        let Some(i) = capture_index(s, word)? else {
            return Ok(s.nil);
        };
        let Some(count) = inspect_captures(s, r)? else {
            return Ok(s.nil);
        };
        if i >= count {
            return Ok(s.nil);
        }
        s.field(r, i + 1)
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn capture_word_of(
    _: Oop,
    c: Oop,
    _: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [index, target] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Ok(s.nil);
        };
        let _q = s.scope(&[*index, *target]);
        if inspect_captures(s, *target)?.is_none() {
            return Ok(s.nil);
        }
        s.send(*target, b"captureWordAt:", &[*index])
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe fn with_code(s: &State, method: Oop, code: Oop, captures: Option<Oop>) -> Result<Oop, Oop> {
    unsafe {
        let q = s.scope(&[method, code]);
        if let Some(v) = captures {
            q.keep(v);
        }
        let factory = q.keep(s.named(b"CodeObject")?);
        let count = q.keep(s.send(factory, b"captureCountOf:", &[code])?);
        let count = resumable::index(s, count)?;
        if count > 1_048_576 {
            return Err(invalid());
        }
        let admitted = q.keep(s.send(method, b"codeObject", &[])?);
        if admitted != s.nil {
            let actual_count = q.keep(s.send(factory, b"captureCountOf:", &[admitted])?);
            let actual_arity = q.keep(s.send(factory, b"parameterCountOf:", &[admitted])?);
            let arity = q.keep(s.send(factory, b"parameterCountOf:", &[code])?);
            if resumable::index(s, actual_count)? != count
                || resumable::index(s, actual_arity)? != resumable::index(s, arity)?
            {
                return Err(invalid());
            }
        }

        let owned = if let Some(owned) = resumable::with_source(s, method, code, &q)? {
            q.keep(owned)
        } else if let Some(owned) = native_code::with_source(s, method, code, &q)? {
            q.keep(owned)
        } else {
            // A reflected source getter is not authority to pair arbitrary
            // native code with a portable program. Only its code owner can do so.
            return Err(invalid());
        };
        let mut values = vec![owned];
        if let Some(captures) = captures {
            if abi::as_size(s.send(captures, b"sizeWord", &[])?) != Some(count) {
                return Err(invalid());
            }
            for i in 0..count {
                values.push(q.keep(s.send(captures, b"atWord:", &[abi::size(i).unwrap()])?));
            }
        } else {
            values.resize(count + 1, s.nil);
        }
        let block = q.keep(s.make_block(s.block_vt, &values)?);
        let signature = q.keep(s.send(code, b"nativeSignature", &[])?);
        if signature == s.nil {
            Ok(block)
        } else {
            let wrapped = q.keep(s.send(
                s.receiver,
                b"block:withNativeSignature:",
                &[block, signature],
            )?);
            if s.send(wrapped, b"isError", &[])? == s.named(b"true")? {
                return Ok(wrapped);
            }
            Ok(wrapped)
        }
    }
}
pub(super) unsafe extern "C" fn from_code(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [method, code, captures] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        with_code(s, *method, *code, Some(*captures))
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn shell(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [method, code] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[*method, *code]);
        let block = q.keep(with_code(s, *method, *code, None)?);
        if s.storage.borrow().family_of(block) != Some(s.block_vt) {
            return if s.send(block, b"isError", &[])? == s.named(b"true")? {
                Ok(block)
            } else {
                Err(invalid())
            };
        }
        s.storage.borrow_mut().set_image_shell(block, true);
        Ok(block)
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn restore_captures(
    _: Oop,
    c: Oop,
    _: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [captures, block] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[*captures, *block]);
        let count = abi::as_size(s.send(*captures, b"sizeWord", &[])?).ok_or_else(invalid)?;
        if !s.storage.borrow().image_shell(*block)
            || s.storage.borrow().payload_size(*block)
                != count
                    .checked_add(1)
                    .and_then(|v| v.checked_mul(size_of::<Oop>()))
        {
            return Err(invalid());
        }
        let mut values = Vec::new();
        for i in 0..count {
            values.push(q.keep(s.send(*captures, b"atWord:", &[abi::size(i).unwrap()])?));
        }
        s.send(s.gc, b"prepareAllocation", &[])?;
        let mut storage = s.storage.borrow_mut();
        if !storage.image_shell(*block) {
            return Err(invalid());
        }
        for (i, v) in values.iter().enumerate() {
            if !storage.write_payload_word(*block, i + 1, v.addr()) {
                return Err(invalid());
            }
        }
        storage.set_image_shell(*block, false);
        Ok(*block)
    })()
    .unwrap_or_else(abi::fault_reference)
}
