use super::*;
struct Numeric<'a> {
    text: &'a Text,
    integer: Oop,
    float: Oop,
}
impl Numeric<'_> {
    unsafe fn send(&self, r: Oop, n: &[u8], a: &[Oop]) -> Result<Oop, Oop> {
        unsafe { self.text.send(r, n, a) }
    }
    unsafe fn integer(&self, value: i64) -> Result<Oop, Oop> {
        unsafe { self.send(self.integer, b"fromWord:", &[self.text.word(value as u64)?]) }
    }
    unsafe fn index(&self, value: Oop) -> Result<Option<i64>, Oop> {
        unsafe {
            let v = self.send(self.integer, b"wordOf:", &[value])?;
            if v.is_null() || v == self.text.nil {
                Ok(None)
            } else {
                Ok(Some(self.text.number(v)? as i64))
            }
        }
    }
}
unsafe fn run(n: &Numeric<'_>, r: Oop, args: &[Oop], op: usize) -> Result<Oop, Oop> {
    unsafe {
        let t = n.text;
        let expected = match op {
            1 | 2 | 4 | 9 => 1,
            3 => 2,
            _ => 0,
        };
        if args.len() != expected {
            return Err(invalid());
        }
        // Id already keeps this call's receiver and arguments alive.
        {
            let nil = t.nil;
            let indices = if matches!(op, 1 | 2 | 3 | 9) {
                let mut indices = Vec::new();
                for arg in args {
                    let Some(v) = n.index(*arg)? else {
                        return Ok(if op == 9 { r } else { nil });
                    };
                    indices.push(v);
                }
                indices
            } else {
                Vec::new()
            };
            if op == 0 || op == 8 {
                return n.integer(
                    t.length(r, if op == 8 { Kind::Builder } else { Kind::String })? as i64,
                );
            }
            if op == 9 {
                t.length(r, Kind::Builder)?;
                t.append(r, &[indices[0] as u8])?;
                return Ok(r);
            }
            let bytes = t.copy(r, Kind::String)?;
            match op {
                1 | 2 => {
                    let index = indices[0];
                    if index < 0 || index as usize >= bytes.len() {
                        return Ok(nil);
                    }
                    if op == 1 {
                        t.string(&[bytes[index as usize]])
                    } else {
                        n.integer(i64::from(bytes[index as usize]))
                    }
                }
                3 => {
                    let (from, to) = (indices[0], indices[1]);
                    if from < 0 || to < from || to as usize > bytes.len() {
                        return Ok(nil);
                    }
                    t.string(&bytes[from as usize..to as usize])
                }
                4 => {
                    let Ok(needle) = t.copy(args[0], Kind::String) else {
                        return Ok(nil);
                    };
                    let index = if needle.is_empty() {
                        Some(0)
                    } else {
                        bytes.windows(needle.len()).position(|w| w == needle)
                    };
                    match index {
                        Some(v) => n.integer(v as i64),
                        None => Ok(nil),
                    }
                }
                5 => n.integer(crate::text::bracket_depth(&bytes)),
                6 | 7 => {
                    // Numeric parsing historically trims four ASCII spaces; trimmed
                    // recognizes six. Preserve that distinction and signed overflow.
                    let trim = |b: &u8| matches!(*b, b' ' | b'\t' | b'\r' | b'\n');
                    let start = bytes.iter().position(|b| !trim(b)).unwrap_or(bytes.len());
                    let end = bytes
                        .iter()
                        .rposition(|b| !trim(b))
                        .map_or(start, |i| i + 1);
                    let Ok(text) = core::str::from_utf8(&bytes[start..end]) else {
                        return Ok(nil);
                    };
                    if op == 6 || !text.contains(['.', 'e', 'E']) {
                        if let Ok(v) = text.parse::<i64>() {
                            return n.integer(v);
                        }
                        if op == 6 {
                            return Ok(nil);
                        }
                    }
                    match text.parse::<f64>() {
                        Ok(v) => n.send(n.float, b"fromBits:", &[t.word(v.to_bits())?]),
                        Err(_) => Ok(nil),
                    }
                }
                _ => Err(invalid()),
            }
        }
    }
}

macro_rules! numeric {
    ($name:ident,$op:expr) => {
        pub(super) unsafe extern "C" fn $name(
            _: Oop,
            c: Oop,
            r: Oop,
            a: *const Oop,
            n: usize,
        ) -> Oop {
            done((|| unsafe {
                let t = instance()?;
                // An ordinary Sequence is the complete dependency context. Its current
                // at: implementation defines access; no native pointer is a context.
                let integer = t.send(c, b"at:", &[abi::size(0).unwrap()])?;
                t.rooted(&[integer], || {
                    let float = t.send(c, b"at:", &[abi::size(1).unwrap()])?;
                    t.rooted(&[float], || {
                        run(
                            &Numeric {
                                text: t,
                                integer,
                                float,
                            },
                            r,
                            borrowed_slice(a, n).ok_or_else(invalid)?,
                            $op,
                        )
                    })
                })
            })())
        }
    };
}
numeric!(size, 0);
numeric!(at, 1);
numeric!(byte_at, 2);
numeric!(copy, 3);
numeric!(index_of, 4);
numeric!(depth, 5);
numeric!(as_integer, 6);
numeric!(as_number, 7);
numeric!(builder_size, 8);
numeric!(add_byte, 9);
pub(super) unsafe extern "C" fn install(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    done((|| unsafe {
        let t = state(c)?;
        let [integer, float] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        if r != t.prototype || t.numeric.get() {
            return Err(invalid());
        }
        let integer = object(*integer)?;
        let float = object(*float)?;
        t.number(t.send(integer, b"wordOf:", &[integer])?)?;
        t.number(t.send(float, b"bitsOf:", &[float])?)?;
        let context = t.send(t.id.root, b"sequence", &[integer, float])?;
        t.rooted(&[context], || {
            for (builder, name, implementation) in [
                (false, b"size".as_slice(), b"numeric::size".as_slice()),
                (false, b"at:", b"numeric::at"),
                (false, b"byteAt:", b"numeric::byte_at"),
                (false, b"copyFrom:to:", b"numeric::copy"),
                (false, b"indexOf:", b"numeric::index_of"),
                (false, b"bracketDepth", b"numeric::depth"),
                (false, b"asInteger", b"numeric::as_integer"),
                (false, b"asNumber", b"numeric::as_number"),
                (true, b"size", b"numeric::builder_size"),
                (true, b"addByte:", b"numeric::add_byte"),
            ] {
                let target = if builder {
                    t.builder_behavior
                } else {
                    t.behavior
                };
                if t.send(target, b"lookup", &[t.selector(name)?])?.is_null() {
                    let method = t.method(implementation, context)?;
                    t.send(target, b"addMethod", &[t.selector(name)?, method])?;
                }
            }
            t.numeric.set(true);
            Ok(ptr::null_mut())
        })
    })())
}
