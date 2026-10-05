//! Language-indexed binary operations; all number and sequence access is through Id.
use super::native::*;
struct Numeric<'a> {
    text: &'a Bytes,
}
impl Numeric<'_> {
    unsafe fn integer(&self, value: i64) -> Result<Operand, Outcome> {
        unsafe {
            self.text.id.send_named(
                self.text.binding(b"Integer")?,
                b"fromWord:",
                &[Operand::word(value as usize)],
            )
        }
    }
    unsafe fn index(&self, value: Operand) -> Result<Option<i64>, Outcome> {
        unsafe {
            let v =
                self.text
                    .id
                    .send_named(self.text.binding(b"Integer")?, b"wordOf:", &[value])?;
            Ok(if v.kind() == native::UNIT {
                None
            } else {
                Some(v.as_word().ok_or_else(invalid)? as i64)
            })
        }
    }
}
unsafe fn run(t: &Bytes, r: Oop, args: &[Operand], op: usize) -> Result<Operand, Outcome> {
    unsafe {
        let n = Numeric { text: t };
        let expected = match op {
            11 | 13 | 14 | 19 => 1,
            12 | 17 | 18 | 20 | 23 => 2,
            22 => 5,
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
            let mut indices = Vec::new();
            if matches!(op, 11 | 12 | 13 | 14) {
                for arg in args {
                    let Some(value) = n.index(*arg)? else {
                        return Ok(nil);
                    };
                    indices.push(value);
                }
            }
            if op == 19 {
                let mut bytes = t.copy(r, Kind::Bytes)?;
                let name = t.id.send_named(t.id.receiver, b"symbolBytesOf:", args)?;
                let Some(name) = name.as_bytes() else {
                    return Ok(nil);
                };
                // Own the borrowed Id name before allocating the result Bytes.
                bytes.extend_from_slice(
                    borrowed_bytes(name.pointer, name.length).ok_or_else(invalid)?,
                );
                return t.make(bytes, Kind::Bytes);
            }
            if op == 22 {
                let mut values = Vec::new();
                for arg in &args[..4] {
                    let Some(v) = n.index(*arg)?.and_then(|v| usize::try_from(v).ok()) else {
                        return Ok(nil);
                    };
                    values.push(v);
                }
                let bytes = t.copy(r, Kind::Bytes)?;
                let schema = t.input_bytes(object(args[4])?)?;
                let Some(rows) = crate::binary_rows::records(
                    &bytes, values[0], values[1], values[2], values[3], &schema,
                ) else {
                    return Ok(nil);
                };
                let table = std::sync::Arc::new(crate::tagged_view::Table { bytes, rows });
                let rows = (0..table.rows.len())
                    .map(|i| crate::tagged_view::RowRef {
                        table: table.clone(),
                        row: i,
                        position: i,
                    })
                    .collect();
                return t.view(crate::tagged_view::View::Rows(std::sync::Arc::new(rows)));
            }
            if op == 21 {
                return t.view(crate::tagged_view::View::Rows(std::sync::Arc::new(
                    Vec::new(),
                )));
            }
            if op == 18 || op == 20 {
                let schema = t.input_bytes(object(args[0])?)?;
                let Some(limit) = n.index(args[1])?.and_then(|i| usize::try_from(i).ok()) else {
                    return Ok(nil);
                };
                if limit > 1_048_576 {
                    return Ok(nil);
                }
                let bytes = t.copy(r, Kind::Bytes)?;
                let Some(rows) = crate::binary_rows::rows(&bytes, &schema, limit) else {
                    return Ok(nil);
                };
                let key = t.id.intern_name(b"Vector")?;
                let factory =
                    object(t.id.send_named(t.namespace, b"at:", &[Operand::object(key)])?)?;
                let vector = |items: &[Oop]| -> Result<Oop, Outcome> {
                    let mut out = object(t.id.send_named(factory, b"new", &[])?)?;
                    for &item in items {
                        out = object(t.rooted(&[out], || {
                            t.id.send_named(out, b"add:", &[Operand::object(item)])
                        })?)?;
                    }
                    Ok(out)
                };
                let table = std::sync::Arc::new(crate::tagged_view::Table { bytes, rows });
                if op == 20 {
                    let rows = (0..table.rows.len())
                        .map(|i| crate::tagged_view::RowRef {
                            table: table.clone(),
                            row: i,
                            position: i,
                        })
                        .collect();
                    return t.view(crate::tagged_view::View::Rows(std::sync::Arc::new(rows)));
                }
                let mut out = vector(&[])?;
                for i in 0..table.rows.len() {
                    out = object(t.rooted(&[out], || {
                        let row =
                            object(t.view(crate::tagged_view::View::Row(table.clone(), i, i))?)?;
                        t.rooted(&[row], || {
                            t.id.send_named(out, b"add:", &[Operand::object(row)])
                        })
                    })?)?;
                }
                return Ok(Operand::object(out));
            }
            if op == 17 {
                // Caller supplies the tagged-field schema: empty, fixed 8 bytes,
                // u32 reference, length-prefixed bytes, or nonempty C name.
                // No container magic, heap rules or image authority live here.
                let Some(start) = n.index(args[0])?.and_then(|i| usize::try_from(i).ok()) else {
                    return Ok(nil);
                };
                let schema = t.input_bytes(object(args[1])?)?;
                let parsed = {
                    let buffers = t.buffers.borrow();
                    let b = &buffers
                        .get(&r.addr())
                        .filter(|b| b.kind == Kind::Bytes)
                        .ok_or_else(invalid)?
                        .bytes;
                    (|| {
                        let tag = *b.get(start)?;
                        let shape = *schema.get(tag as usize)?;
                        let mut end = start.checked_add(1)?;
                        let mut index = None;
                        let mut data = None;
                        match shape {
                            b'0' => (),
                            b'1' => end = end.checked_add(8)?,
                            b'2' => {
                                let finish = end.checked_add(4)?;
                                index =
                                    Some(u32::from_le_bytes(b.get(end..finish)?.try_into().ok()?));
                                end = finish;
                            }
                            b'3' | b'4' => {
                                let finish = end.checked_add(4)?;
                                let length =
                                    u32::from_le_bytes(b.get(end..finish)?.try_into().ok()?)
                                        as usize;
                                end = finish.checked_add(length)?;
                                let bytes = b.get(finish..end)?;
                                if shape == b'4' && (bytes.is_empty() || bytes.contains(&0)) {
                                    return None;
                                }
                                data = Some(bytes.to_vec());
                            }
                            _ => return None,
                        }
                        if end > b.len() {
                            return None;
                        }
                        Some((tag, index, data, end))
                    })()
                };
                let Some((tag, index, data, end)) = parsed else {
                    return Ok(nil);
                };
                let tag = object(n.integer(tag as i64)?)?;
                return t.rooted(&[tag], || {
                    let index = match index {
                        Some(i) => object(n.integer(i as i64)?)?,
                        None => t.nil,
                    };
                    t.rooted(&[index], || {
                        let data = match data {
                            Some(b) => object(t.make(b, Kind::Bytes)?)?,
                            None => t.nil,
                        };
                        t.rooted(&[data], || {
                            let end = object(n.integer(end as i64)?)?;
                            t.rooted(&[end], || {
                                let key = t.id.intern_name(b"Vector")?;
                                let vector = object(t.id.send_named(
                                    t.namespace,
                                    b"at:",
                                    &[Operand::object(key)],
                                )?)?;
                                let mut out = object(t.id.send_named(vector, b"new", &[])?)?;
                                for o in [tag, index, data, end] {
                                    out = object(t.rooted(&[out], || {
                                        t.id.send_named(out, b"add:", &[Operand::object(o)])
                                    })?)?;
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
                if !matches!(kind, Kind::Bytes) {
                    return Err(invalid());
                }
                if op == 12 {
                    let copied = {
                        let buffers = t.buffers.borrow();
                        let b = &buffers.get(&r.addr()).unwrap().bytes;
                        let (from, to) = (indices[0], indices[1]);
                        if from < 0 || to < from {
                            return Ok(nil);
                        }
                        b.get(from as usize..to as usize).map(<[u8]>::to_vec)
                    };
                    return match copied {
                        Some(b) => t.make(b, Kind::Bytes),
                        None => Ok(nil),
                    };
                }
                if op == 13 || op == 14 {
                    let value = {
                        let buffers = t.buffers.borrow();
                        let b = &buffers.get(&r.addr()).unwrap().bytes;
                        let width = if op == 13 { 4 } else { 8 };
                        let Some(start) = usize::try_from(indices[0]).ok() else {
                            return Ok(nil);
                        };
                        let Some(end) = start.checked_add(width) else {
                            return Ok(nil);
                        };
                        let Some(b) = b.get(start..end) else {
                            return Ok(nil);
                        };
                        if width == 4 {
                            u32::from_le_bytes(b.try_into().unwrap()) as u64
                        } else {
                            u64::from_le_bytes(b.try_into().unwrap())
                        }
                    };
                    return n.integer(value as i64);
                }
                let result = {
                    let buffers = t.buffers.borrow();
                    let b = &buffers.get(&r.addr()).unwrap().bytes;
                    if op == 15 {
                        use sha2::{Digest, Sha256};
                        Sha256::digest(b).to_vec()
                    } else {
                        crate::binary_hash::wyhash(0, b).to_le_bytes().to_vec()
                    }
                };
                return t.make(result, Kind::Bytes);
            }
            if op == 10 {
                return n.integer(t.length(r, Kind::Bytes)? as i64);
            }
            if op == 11 {
                let byte = {
                    let buffers = t.buffers.borrow();
                    let b = buffers
                        .get(&r.addr())
                        .filter(|b| b.kind == Kind::Bytes)
                        .ok_or_else(invalid)?;
                    usize::try_from(indices[0])
                        .ok()
                        .and_then(|i| b.bytes.get(i))
                        .copied()
                };
                return match byte {
                    Some(byte) => n.integer(i64::from(byte)),
                    None => Ok(nil),
                };
            }
            Err(invalid())
        })
    }
}
macro_rules! numeric {
    ($name:ident,$op:expr) => {
        pub(super) unsafe extern "C" fn $name(
            c: *mut c_void,
            r: Oop,
            a: *const Operand,
            n: usize,
        ) -> Outcome {
            done((|| unsafe {
                run(state(c)?, r, borrowed_slice(a, n).ok_or_else(invalid)?, $op)
            })())
        }
    };
}
numeric!(bytes_size, 10);
numeric!(bytes_at, 11);
numeric!(binary_slice, 12);
numeric!(read_u32, 13);
numeric!(read_u64, 14);
numeric!(sha256, 15);
numeric!(wyhash64, 16);
numeric!(tagged_field, 17);
numeric!(tagged_rows, 18);
numeric!(with_symbol_name, 19);
numeric!(tagged_table, 20);
numeric!(empty_rows, 21);
numeric!(tagged_records, 22);
