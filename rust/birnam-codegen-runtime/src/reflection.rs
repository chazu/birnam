//! Live reflection over current owner messages; no foreign Method payload reads.
use super::*;
use std::collections::HashSet;
pub(super) unsafe extern "C" fn global_named(
    _: Oop,
    c: Oop,
    _: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [name] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[*name]);
        // Reflection's established order is native exports, then authored Globals.
        let exported = s.send(s.ns, b"at:", &[*name])?;
        if !exported.is_null() {
            return Ok(exported);
        }
        let globals = q.keep(s.named(b"Globals")?);
        s.send(globals, b"at:", &[*name])
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn global_names(
    _: Oop,
    c: Oop,
    _: Oop,
    _: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        if n != 0 {
            return Err(invalid());
        }
        let q = s.scope(&[]);
        let mut out = call(s, &q, s.named(b"Vector")?, b"new", &[])?;
        let count = abi::as_size(s.send(s.ns, b"count", &[])?).ok_or_else(invalid)?;
        for index in 0..count {
            let name = call(
                s,
                &q,
                s.ns,
                b"nameAt:",
                &[abi::size(index).ok_or_else(invalid)?],
            )?;
            if name.is_null() {
                return Err(invalid());
            }
            out = call(s, &q, out, b"add:", &[name])?;
        }
        Ok(out)
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn parent(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [behavior] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        s.send(s.id.root, b"parentOf:", &[*behavior])
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe fn call(s: &State, q: &Scope<'_>, r: Oop, n: &[u8], a: &[Oop]) -> Result<Oop, Oop> {
    unsafe { Ok(q.keep(s.send(r, n, a)?)) }
}
unsafe fn target_name(s: &State, q: &Scope<'_>, target: Oop) -> Result<Oop, Oop> {
    unsafe {
        let exported = call(s, q, s.ns, b"nameOf:", &[target])?;
        if !exported.is_null() {
            return Ok(exported);
        }
        call(s, q, s.named(b"Globals")?, b"nameOf:", &[target])
    }
}
unsafe fn methods(c: Oop, a: *const Oop, n: usize, direct: bool) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [target] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[*target]);
        let nil = s.named(b"nil")?;
        let mut behavior =
            if direct && abi::as_size(s.send(s.id.root, b"isBehavior:", &[*target])?) == Some(1) {
                *target
            } else {
                call(s, &q, s.id.root, b"behaviorOf:", &[*target])?
            };
        let mut seen = HashSet::new();
        let mut visited = HashSet::new();
        let mut rows = Vec::new();
        loop {
            if !visited.insert(behavior) {
                return Err(invalid());
            }
            q.keep(behavior);
            let owner = call(s, &q, s.id.root, b"ownerOf:", &[behavior])?;
            let owner = if owner.is_null() || owner == nil {
                behavior
            } else {
                owner
            };
            let mut target = target_name(s, &q, owner)?;
            if (target.is_null() || target == nil) && owner != behavior {
                target = target_name(s, &q, behavior)?;
            }
            if target.is_null() {
                target = nil;
            }
            for (selector, method) in image::direct_bindings(s, &q, behavior)? {
                if !seen.insert(selector) {
                    continue;
                }
                let implementation = call(s, &q, s.receiver, b"implementationOf:", &[method])?;
                let mut row = call(s, &q, s.named(b"Map")?, b"new", &[])?;
                for (key, value) in [
                    (b"selector".as_slice(), selector),
                    (b"target", target),
                    (b"method", implementation),
                ] {
                    let key = q.keep(s.intern(key)?);
                    row = call(s, &q, row, b"at:put:", &[key, value])?;
                }
                if direct {
                    let key = q.keep(s.intern(b"vtable")?);
                    row = call(s, &q, row, b"at:put:", &[key, behavior])?;
                }
                rows.push(row);
            }
            if direct {
                break;
            }
            behavior = call(s, &q, s.id.root, b"parentOf:", &[behavior])?;
            if behavior.is_null() {
                break;
            }
        }
        s.vector(&q, &rows)
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe extern "C" fn all_methods(
    _: Oop,
    c: Oop,
    _: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    unsafe { methods(c, a, n, false) }
}
pub(super) unsafe extern "C" fn direct_methods(
    _: Oop,
    c: Oop,
    _: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    unsafe { methods(c, a, n, true) }
}
pub(super) unsafe extern "C" fn remove(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [selector, target] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[*selector, *target]);
        let behavior = if abi::as_size(s.send(s.id.root, b"isBehavior:", &[*target])?) == Some(1) {
            *target
        } else {
            call(s, &q, s.id.root, b"behaviorOf:", &[*target])?
        };
        let method = call(
            s,
            &q,
            s.named(b"Execution")?,
            b"directMethodFor:in:",
            &[*selector, behavior],
        )?;
        if method == s.named(b"nil")? {
            let kind = q.keep(s.intern(b"missingDirectMethod")?);
            return s.send(s.named(b"Error")?, b"kind:", &[kind]);
        }
        let result = call(s, &q, behavior, b"removeMethod:", &[*selector])?;
        if !result.is_null() && s.is_error(result)? {
            return Ok(result);
        }
        Ok(*target)
    })()
    .unwrap_or_else(abi::fault_reference)
}

pub(super) unsafe extern "C" fn name_of(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [symbol] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[*symbol]);
        if !s.exact(&q, *symbol, b"SymbolVt")? {
            let kind = q.keep(s.intern(b"notASymbol")?);
            let message = q.keep(s.text(b"nameOf: expects a symbol")?);
            return s.send(s.named(b"Error")?, b"kind:message:", &[kind, message]);
        }
        s.text(&s.byte_reader.read(*symbol)?)
    })()
    .unwrap_or_else(abi::fault_reference)
}
