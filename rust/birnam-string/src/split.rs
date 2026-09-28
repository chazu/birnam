use super::*;
struct Split {
    text: *const Text,
    vector: Oop,
    new: Oop,
    add: Oop,
}
unsafe extern "C" fn split(c: *mut c_void, r: Oop, a: *const Operand, count: usize) -> Outcome {
    done((|| unsafe {
        let extension = &*c.cast::<Split>();
        let t = state(extension.text.cast_mut().cast())?;
        let Some([separator]) = borrowed_slice(a, count) else {
            return Err(invalid());
        };
        let separator = object(*separator)?;
        t.rooted(&[r, separator], || {
            let bytes = t.copy(r, Kind::String)?;
            let separator = if t
                .send(t.id.receiver, b"behaviorOf:", &[Operand::object(separator)])?
                .as_object()
                == Some(t.behavior)
            {
                Some(t.copy(separator, Kind::String)?)
            } else {
                None
            };
            let mut result = object(t.id.send(extension.vector, extension.new, &[])?)?;
            if separator.is_none() {
                return t.rooted(&[result], || {
                    t.id.send(result, extension.add, &[Operand::object(r)])
                });
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
                    let part = object(t.string(part)?)?;
                    t.rooted(&[part], || {
                        t.id.send(result, extension.add, &[Operand::object(part)])
                    })
                })?)?;
                let Some(end) = end else { break };
                from = end + separator.len();
            }
            Ok(Operand::object(result))
        })
    })())
}
pub(super) unsafe extern "C" fn install(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let t = state(c)?;
        let Some([vector]) = borrowed_slice(a, count) else {
            return Err(invalid());
        };
        let vector = object(*vector)?;
        if r != t.prototype.get() {
            return Err(invalid());
        }
        t.rooted(&[vector], || {
            let selector = t.id.intern_name(b"split:")?;
            t.rooted(&[selector], || {
                if t.id
                    .send_named(t.behavior, b"lookup", &[Operand::object(selector)])?
                    .kind()
                    != native::UNIT
                {
                    return Ok(Operand::unit());
                }
                // Root the completed extension dependencies before installing its method.
                let new = t.id.intern_name(b"new")?;
                t.send(t.roots, b"retain:", &[Operand::object(new)])?;
                let add = t.id.intern_name(b"add:")?;
                t.send(t.roots, b"retain:", &[Operand::object(add)])?;
                t.send(t.roots, b"retain:", &[Operand::object(vector)])?;
                let extension = Box::into_raw(Box::new(Split {
                    text: t,
                    vector,
                    new,
                    add,
                }));
                t.send(
                    t.behavior,
                    b"addMethod",
                    &[
                        Operand::object(selector),
                        Operand::entry(NativeEntry::new(split, extension.cast())),
                    ],
                )?;
                Ok(Operand::unit())
            })
        })
    })())
}
