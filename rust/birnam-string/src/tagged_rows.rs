//! Generic projections and first-match scans over schema-decoded tagged rows.
//! Masks, data matches and numeric bounds are supplied by the language caller.
use super::native::*;
use crate::tagged_view::View;
use std::sync::Arc;

pub(super) unsafe fn run(t: &Text, r: Oop, a: &[Operand], op: usize) -> Result<Operand, Outcome> {
    unsafe {
        let argc = match op { 0 => 0, 8 | 10 | 12 | 14 => 2, _ => 1 };
        if a.len() != argc { return Err(invalid()) }
        let view = t.views.borrow().get(&r.addr()).cloned().ok_or_else(invalid)?;
        let mut roots = vec![r];
        for &v in a { roots.push(object(v)?); }
        t.rooted(&roots, || {
            let integer = object(t.id.send_named(t.namespace, b"at:", &[Operand::object(t.id.intern_name(b"Integer")?)])?)?;
            let number = |n| t.id.send_named(integer, b"fromWord:", &[Operand::word(n)]);
            if op == 0 {
                return match view { View::Row(_, _, position) => number(position), _ => Err(invalid()) };
            }
            let View::Rows(rows) = view else { return Err(invalid()) };
            if op == 1 {
                let other = t.views.borrow().get(&object(a[0])?.addr()).cloned().ok_or_else(invalid)?;
                let View::Rows(other) = other else { return Err(invalid()) };
                let mut joined = Vec::with_capacity(rows.len().checked_add(other.len()).ok_or_else(invalid)?);
                for item in rows.iter().chain(other.iter()) {
                    let mut item = item.clone(); item.position = joined.len(); joined.push(item);
                }
                return t.view(View::Rows(Arc::new(joined)));
            }
            let word = |v| -> Result<usize, Outcome> {
                t.id.send_named(integer, b"wordOf:", &[v])?.as_word().filter(|n| *n <= isize::MAX as usize).ok_or_else(invalid)
            };
            if op == 10 {
                let (from, to) = (word(a[0])?, word(a[1])?);
                let Some(slice) = rows.get(from..to) else { return Ok(Operand::object(t.nil)) };
                return t.view(View::Rows(Arc::new(slice.to_vec())));
            }
            let (text, limit) = if op == 7 { (Vec::new(), word(a[0])?) }
                else { (t.copy(object(a[0])?, Kind::String)?, if matches!(op, 8 | 12 | 14) { word(a[1])? } else { 0 }) };
            if matches!(op, 2 | 5 | 6 | 8 | 9 | 11 | 12 | 13 | 14) && (text.len() > 256 || text.iter().any(|v| !matches!(v, b'0' | b'1'))) { return Err(invalid()) }
            let matches = |tag: u8| text.get(tag as usize) == Some(&b'1');
            if matches!(op, 2..=4) {
                let selected = rows.iter().filter(|row| {
                    let f = &row.table.rows[row.row].0;
                    if op == 2 { matches(f.tag) } else {
                        let equal = f.data.as_ref().is_some_and(|range| row.table.bytes[range.clone()] == text);
                        if op == 3 { equal } else { !equal }
                    }
                }).cloned().collect();
                return t.view(View::Rows(Arc::new(selected)));
            }
            if matches!(op, 11 | 12 | 14) {
                for row in rows.iter() {
                    let (prefix, fields) = &row.table.rows[row.row];
                    if op == 14 {
                        let data = prefix.data.as_ref().ok_or_else(invalid)?;
                        if limit > data.len() { return Err(invalid()) }
                        for i in 0..limit {
                            if !matches(row.table.bytes[data.start+i]) { return number(data.start+i) }
                        }
                    } else {
                        for f in fields {
                            let bad = if op == 11 { !matches(f.tag) }
                                else { matches(f.tag) && f.index.is_none_or(|i| i as usize >= limit) };
                            if bad { return number(f.offset) }
                        }
                    }
                }
                return Ok(Operand::object(t.nil));
            }
            if op == 9 || op == 13 {
                let mut bytes = Vec::new();
                for row in rows.iter() {
                    let (locator, fields) = &row.table.rows[row.row];
                    for field in std::iter::once(locator).filter(|_| op == 9).chain(fields.iter()) {
                        if matches(field.tag) {
                            bytes.extend_from_slice(&u32::try_from(if op == 9 { row.position } else { field.offset }).map_err(|_| invalid())?.to_le_bytes());
                            bytes.extend_from_slice(&field.index.ok_or_else(invalid)?.to_le_bytes());
                        }
                    }
                }
                return t.make(bytes, Kind::Bytes);
            }
            let failed = rows.iter().find(|row| {
                let (locator, fields) = &row.table.rows[row.row];
                match op {
                    5 => !matches(locator.tag),
                    6 => fields.iter().any(|f| !matches(f.tag)),
                    7 => fields.len() != limit,
                    8 => std::iter::once(locator).chain(fields.iter()).any(|f| matches(f.tag) && f.index.is_none_or(|i| i as usize >= limit)),
                    _ => false,
                }
            });
            if !matches!(op, 5..=8) { return Err(invalid()) }
            failed.map_or(Ok(Operand::object(t.nil)), |row| number(row.position))
        })
    }
}
macro_rules! entry {
    ($name:ident, $op:expr) => {
        pub(super) unsafe extern "C" fn $name(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
            done((|| unsafe { run(state(c)?, r, borrowed_slice(a, n).ok_or_else(invalid)?, $op) })())
        }
    };
}
entry!(source_index, 0);
entry!(concat, 1);
entry!(where_tags, 2);
entry!(where_data, 3);
entry!(without_data, 4);
entry!(first_locator_outside, 5);
entry!(first_fields_outside, 6);
entry!(first_count_not, 7);
entry!(first_index_outside, 8);
entry!(indices, 9);

entry!(first_field_offset, 11);
entry!(first_reference_offset, 12);
entry!(field_references, 13);
entry!(first_prefix, 14);

entry!(slice, 10);
