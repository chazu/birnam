use super::*;
struct Numeric {
    text: *const Text,
    integer: Oop,
    float: Oop,
    names: HashMap<&'static [u8], Oop>,
}
impl Numeric {
    unsafe fn send(
        &self,
        r: Oop,
        name: &'static [u8],
        args: &[Operand],
    ) -> Result<Operand, Outcome> {
        unsafe { (*self.text).id.send(r, self.names[name], args) }
    }
    unsafe fn integer(&self, value: i64) -> Result<Operand, Outcome> {
        unsafe { self.send(self.integer, b"fromWord:", &[Operand::word(value as usize)]) }
    }
    unsafe fn index(&self, value: Operand) -> Result<Option<i64>, Outcome> {
        unsafe {
            let v = self.send(self.integer, b"wordOf:", &[Operand::object(object(value)?)])?;
            if v.kind() == native::UNIT {
                Ok(None)
            } else {
                Ok(Some(v.as_word().ok_or_else(invalid)? as i64))
            }
        }
    }
}
unsafe fn run(n: &Numeric, r: Oop, args: &[Operand], op: usize) -> Result<Operand, Outcome> {
    unsafe {
        let t = state(n.text.cast_mut().cast())?;
        let expected = match op {
            1 | 2 | 4 | 9 | 11 | 13 | 14 => 1,
            12 | 17 | 18 => 2,
            3 => 2,
            _ => 0,
        };
        if args.len() != expected {
            return Err(invalid());
        }
        let mut values = vec![r];
        for arg in args {
            values.push(object(*arg)?);
        }
        t.rooted(&values, || {
            let nil = Operand::object(t.nil);
            let indices = if matches!(op, 1 | 2 | 3 | 9 | 11 | 12 | 13 | 14) {
                let mut indices = Vec::new();
                for arg in args {
                    let Some(v) = n.index(*arg)? else {
                        return Ok(if op == 9 { Operand::object(r) } else { nil });
                    };
                    indices.push(v);
                }
                indices
            } else {
                Vec::new()
            };
            if op == 18 {
                let schema = t.copy(object(args[0])?, Kind::String)?;
                let Some(limit) = n.index(args[1])?.and_then(|i| usize::try_from(i).ok()) else { return Ok(nil) };
                if limit > 1_048_576 { return Ok(nil) }
                let bytes = t.copy(r, Kind::Bytes)?;
                let Some(rows) = crate::binary_rows::rows(&bytes, &schema, limit) else { return Ok(nil) };
                let key = t.id.intern_name(b"Vector")?;
                let factory = object(t.id.send_named(t.namespace, b"at:", &[Operand::object(key)])?)?;
                let vector = |items: &[Oop]| -> Result<Oop, Outcome> {
                    let mut out = object(t.id.send_named(factory, b"new", &[])?)?;
                    for &item in items {
                        out = object(t.rooted(&[out], || t.id.send_named(out, b"add:", &[Operand::object(item)]))?)?;
                    }
                    Ok(out)
                };
                let table = std::sync::Arc::new(crate::tagged_view::Table { bytes, rows });
                let mut out = vector(&[])?;
                for i in 0..table.rows.len() {
                    out = object(t.rooted(&[out], || {
                        let row = object(t.view(crate::tagged_view::View::Row(table.clone(), i))?)?;
                        t.rooted(&[row], || t.id.send_named(out, b"add:", &[Operand::object(row)]))
                    })?)?;
                }
                return Ok(Operand::object(out));
            }
            if op == 17 {
                // Caller supplies the tagged-field schema: empty, fixed 8 bytes,
                // u32 reference, length-prefixed bytes, or nonempty C name.
                // No container magic, heap rules or image authority live here.
                let Some(start) = n.index(args[0])?.and_then(|i| usize::try_from(i).ok()) else { return Ok(nil) };
                let schema = t.copy(object(args[1])?, Kind::String)?;
                let parsed = {
                    let buffers = t.buffers.borrow();
                    let b = &buffers.get(&r.addr()).filter(|b| b.kind == Kind::Bytes).ok_or_else(invalid)?.bytes;
                    (|| {
                        let tag = *b.get(start)?;
                        let shape = *schema.get(tag as usize)?;
                        let mut end = start.checked_add(1)?;
                        let mut index = None; let mut data = None;
                        match shape {
                            b'0' => (),
                            b'1' => end = end.checked_add(8)?,
                            b'2' => { let finish = end.checked_add(4)?; index = Some(u32::from_le_bytes(b.get(end..finish)?.try_into().ok()?)); end = finish; },
                            b'3' | b'4' => {
                                let finish = end.checked_add(4)?;
                                let length = u32::from_le_bytes(b.get(end..finish)?.try_into().ok()?) as usize;
                                end = finish.checked_add(length)?;
                                let bytes = b.get(finish..end)?;
                                if shape == b'4' && (bytes.is_empty() || bytes.contains(&0)) { return None }
                                data = Some(bytes.to_vec());
                            }
                            _ => return None,
                        }
                        if end > b.len() { return None }
                        Some((tag, index, data, end))
                    })()
                };
                let Some((tag, index, data, end)) = parsed else { return Ok(nil) };
                let tag = object(n.integer(tag as i64)?)?;
                return t.rooted(&[tag], || {
                    let index = match index { Some(i) => object(n.integer(i as i64)?)?, None => t.nil };
                    t.rooted(&[index], || {
                        let data = match data { Some(b) => object(t.make(b, Kind::Bytes)?)?, None => t.nil };
                        t.rooted(&[data], || {
                            let end = object(n.integer(end as i64)?)?;
                            t.rooted(&[end], || {
                                let key = t.id.intern_name(b"Vector")?;
                                let vector = object(t.id.send_named(t.namespace, b"at:", &[Operand::object(key)])?)?;
                                let mut out = object(t.id.send_named(vector, b"new", &[])?)?;
                                for o in [tag, index, data, end] {
                                    out = object(t.rooted(&[out], || t.id.send_named(out, b"add:", &[Operand::object(o)]))?)?;
                                }
                                Ok(Operand::object(out))
                            })
                        })
                    })
                });
            }
            if (12..=16).contains(&op) {
                let kind = {
                    let buffers = t.buffers.borrow();
                    buffers.get(&r.addr()).map(|b| b.kind).ok_or_else(invalid)?
                };
                if !matches!(kind, Kind::String | Kind::Bytes) { return Err(invalid()); }
                if op == 12 {
                    let copied = {
                        let buffers = t.buffers.borrow();
                        let b = &buffers.get(&r.addr()).unwrap().bytes;
                        let (from, to) = (indices[0], indices[1]);
                        if from < 0 || to < from { return Ok(nil); }
                        b.get(from as usize..to as usize).map(<[u8]>::to_vec)
                    };
                    return match copied { Some(b) => t.make(b, Kind::Bytes), None => Ok(nil) };
                }
                if op == 13 || op == 14 {
                    let value = {
                        let buffers = t.buffers.borrow();
                        let b = &buffers.get(&r.addr()).unwrap().bytes;
                        let width = if op == 13 { 4 } else { 8 };
                        let Some(start) = usize::try_from(indices[0]).ok() else { return Ok(nil) };
                        let Some(end) = start.checked_add(width) else { return Ok(nil) };
                        let Some(b) = b.get(start..end) else { return Ok(nil) };
                        if width == 4 { u32::from_le_bytes(b.try_into().unwrap()) as u64 }
                        else { u64::from_le_bytes(b.try_into().unwrap()) }
                    };
                    return n.integer(value as i64);
                }
                let result = {
                    let buffers = t.buffers.borrow();
                    let b = &buffers.get(&r.addr()).unwrap().bytes;
                    if op == 15 {
                        use sha2::{Digest, Sha256};
                        Sha256::digest(b).to_vec()
                    } else { crate::binary_hash::wyhash(0, b).to_le_bytes().to_vec() }
                };
                return t.make(result, Kind::Bytes);
            }
            if op == 10 {
                return n.integer(t.length(r, Kind::Bytes)? as i64);
            }
            if op == 11 {
                let byte = { let buffers = t.buffers.borrow();
                    let b = buffers.get(&r.addr()).filter(|b| b.kind == Kind::Bytes).ok_or_else(invalid)?;
                    usize::try_from(indices[0]).ok().and_then(|i| b.bytes.get(i)).copied() };
                return match byte {
                    Some(byte) => n.integer(i64::from(byte)),
                    None => Ok(nil),
                };
            }
            if op == 0 || op == 8 {
                return n.integer(
                    t.length(r, if op == 8 { Kind::Builder } else { Kind::String })? as i64,
                );
            }
            if op == 9 {
                t.length(r, Kind::Builder)?;
                t.send(t.gc, b"prepareAllocation", &[])?;
                let mut buffers = t.buffers.borrow_mut();
                let b = buffers
                    .get_mut(&r.addr())
                    .filter(|b| b.kind == Kind::Builder)
                    .ok_or_else(invalid)?;
                b.bytes.push(indices[0] as u8);
                mirror(r, b);
                return Ok(Operand::object(r));
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
                    let Ok(needle) = t.copy(values[1], Kind::String) else {
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
                        Ok(v) => n.send(
                            n.float,
                            b"fromBits:",
                            &[Operand::word(v.to_bits() as usize)],
                        ),
                        Err(_) => Ok(nil),
                    }
                }
                _ => Err(invalid()),
            }
        })
    }
}
macro_rules! numeric {
    ($name:ident,$op:expr) => {
        unsafe extern "C" fn $name(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
            done((|| unsafe {
                run(
                    &*c.cast::<Numeric>(),
                    r,
                    borrowed_slice(a, n).ok_or_else(invalid)?,
                    $op,
                )
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
numeric!(bytes_size, 10);
numeric!(bytes_at, 11);
numeric!(binary_slice, 12);
numeric!(read_u32, 13);
numeric!(read_u64, 14);
numeric!(sha256, 15);
numeric!(wyhash64, 16);
numeric!(tagged_field, 17);
numeric!(tagged_rows, 18);
pub(super) unsafe extern "C" fn install(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let t = state(c)?;
        let Some([integer, float]) = borrowed_slice(a, count) else {
            return Err(invalid());
        };
        let (integer, float) = (object(*integer)?, object(*float)?);
        if r != t.prototype.get() || t.numeric.get() {
            return Err(invalid());
        }
        t.rooted(&[integer, float], || {
            let methods = [
                (t.behavior, b"size".as_slice(), size as Method),
                (t.behavior, b"at:", at),
                (t.behavior, b"byteAt:", byte_at),
                (t.behavior, b"copyFrom:to:", copy),
                (t.behavior, b"indexOf:", index_of),
                (t.behavior, b"bracketDepth", depth),
                (t.behavior, b"asInteger", as_integer),
                (t.behavior, b"asNumber", as_number),
                (t.builder_behavior, b"size", builder_size),
                (t.builder_behavior, b"addByte:", add_byte),
                (t.bytes_behavior, b"size", bytes_size),
                (t.view_behavior, b"size", crate::tagged_view::size),
                (t.view_behavior, b"at:", crate::tagged_view::at),
                (t.bytes_behavior, b"at:", bytes_at),
                (t.bytes_behavior, b"copyFrom:to:", binary_slice),
                (t.bytes_behavior, b"readU32At:", read_u32),
                (t.bytes_behavior, b"readU64At:", read_u64),
                (t.bytes_behavior, b"sha256", sha256),
                (t.bytes_behavior, b"wyhash64", wyhash64),
                (t.bytes_behavior, b"readTaggedFieldAt:schema:", tagged_field),
                (t.bytes_behavior, b"readTaggedRowsWithSchema:limit:", tagged_rows),
            ];
            let mut names = HashMap::new();
            for name in [
                b"fromWord:".as_slice(),
                b"wordOf:",
                b"fromBits:",
                b"bitsOf:",
                b"lookup",
            ]
            .into_iter()
            .chain(methods.iter().map(|(_, name, _)| *name))
            {
                let symbol = t.id.intern_name(name)?;
                t.send(t.roots, b"retain:", &[Operand::object(symbol)])?
                    .as_word()
                    .filter(|n| *n != 0)
                    .ok_or_else(invalid)?;
                names.insert(name, symbol);
            }
            let n = Box::into_raw(Box::new(Numeric {
                text: t,
                integer,
                float,
                names,
            }));
            (*n).send(integer, b"wordOf:", &[Operand::object(integer)])?
                .as_word()
                .ok_or_else(invalid)?;
            (*n).send(float, b"bitsOf:", &[Operand::object(float)])?
                .as_word()
                .ok_or_else(invalid)?;
            // Complete extension dependencies and selectors stay rooted for its methods.
            t.send(t.roots, b"retain:", &[Operand::object(integer)])?;
            t.send(t.roots, b"retain:", &[Operand::object(float)])?;
            for (target, name, method) in methods {
                if (*n)
                    .send(target, b"lookup", &[Operand::object((&(*n).names)[name])])?
                    .kind()
                    == native::UNIT
                {
                    t.send(
                        target,
                        b"addMethod",
                        &[
                            Operand::object((&(*n).names)[name]),
                            Operand::entry(NativeEntry::new(method, if target == t.view_behavior { c } else { n.cast() })),
                        ],
                    )?;
                }
            }
            t.numeric.set(true);
            Ok(Operand::unit())
        })
    })())
}
