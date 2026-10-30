use super::*;
pub(super) unsafe extern "C" fn split(_: Oop, c: Oop, r: Oop, a: *const Oop, count: usize) -> Oop {
    done((|| unsafe {
        let t = instance()?;
        let Some([separator]) = borrowed_slice(a, count) else {
            return Err(invalid());
        };
        let separator = object(*separator)?;
        {
            let bytes = t.copy(r, Kind::String)?;
            let separator = if t.send(t.id.root, b"behaviorOf:", &[separator])? == t.behavior {
                Some(t.copy(separator, Kind::String)?)
            } else {
                None
            };
            let mut result = object(t.send(c, b"new", &[])?)?;
            if separator.is_none() {
                return t.send(result, b"add:", &[r]);
            }
            let separator = separator.unwrap();
            let mut from = 0;
            loop {
                let end = if separator.is_empty() {
                    None
                } else {
                    bytes[from..]
                        .windows(separator.len())
                        .position(|v| v == separator)
                        .map(|i| from + i)
                };
                let part = &bytes[from..end.unwrap_or(bytes.len())];
                // Keep only the current persistent result across construction of a segment.
                result = object(t.rooted(&[result], || {
                    let part = object(t.string(part.to_vec())?)?;
                    t.send(result, b"add:", &[part])
                })?)?;
                let Some(end) = end else { break };
                from = end + separator.len();
            }
            Ok(result)
        }
    })())
}

pub(super) unsafe extern "C" fn install(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    done((|| unsafe {
        let t = state(c)?;
        let [vector] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        if r != t.prototype {
            return Err(invalid());
        }
        let vector = object(*vector)?;
        if t.send(t.behavior, b"lookup", &[t.selector(b"split:")?])?
            .is_null()
        {
            // The Method context itself keeps the chosen Vector dependency alive.
            let method = t.method(b"split::split", vector)?;
            t.send(t.behavior, b"addMethod", &[t.selector(b"split:")?, method])?;
        }
        Ok(ptr::null_mut())
    })())
}
