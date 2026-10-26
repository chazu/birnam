//! Read-only indexed views over an owned, schema-decoded byte snapshot.
use super::native::*;
use crate::binary_rows::Field;
use std::sync::Arc;
pub(crate) struct Table {
    pub bytes: Vec<u8>,
    pub rows: Vec<(Field, Vec<Field>)>,
}
#[derive(Clone)]
pub(crate) struct RowRef {
    pub table: Arc<Table>,
    pub row: usize,
    pub position: usize,
}
#[derive(Clone)]
pub(crate) enum View {
    Rows(Arc<Vec<RowRef>>),
    Row(Arc<Table>, usize, usize),
    Fields(Arc<Table>, usize),
    Field(Arc<Table>, usize, Option<usize>),
}
unsafe fn run(t: &Bytes, r: Oop, a: &[Oop], at: bool) -> Result<Oop, Oop> {
    unsafe {
        if a.len() != usize::from(at) {
            return Err(invalid());
        }
        let view = t
            .views
            .borrow()
            .get(&r.addr())
            .cloned()
            .ok_or_else(invalid)?;
        let integer_key = t.selector(b"Integer")?;
        let integer = object(t.send(t.namespace, b"at:", &[integer_key])?)?;
        let number = |n| t.send(integer, b"fromWord:", &[abi::size(n).ok_or_else(invalid)?]);
        if !at {
            return number(match &view {
                View::Row(..) => 2,
                View::Field(..) => 3,
                View::Rows(rows) => rows.len(),
                View::Fields(table, row) => table.rows[*row].1.len(),
            });
        }
        let i = t.number(t.send(integer, b"wordOf:", a)?)?;
        t.rooted(&[r, object(a[0])?], || match view {
            View::Rows(rows) => match rows.get(i) {
                Some(row) => t.view(View::Row(row.table.clone(), row.row, row.position)),
                None => Ok(t.nil),
            },
            View::Row(table, row, _) => match i {
                0 => t.view(View::Field(table, row, None)),
                1 => t.view(View::Fields(table, row)),
                _ => Ok(t.nil),
            },
            View::Fields(table, row) => {
                if i < table.rows[row].1.len() {
                    t.view(View::Field(table, row, Some(i)))
                } else {
                    Ok(t.nil)
                }
            }
            View::Field(table, row, index) => {
                let field = match index {
                    Some(i) => &table.rows[row].1[i],
                    None => &table.rows[row].0,
                };
                match i {
                    0 => number(field.tag as usize),
                    1 => field.index.map_or(Ok(t.nil), |i| number(i as usize)),
                    2 => match &field.data {
                        Some(range) => t.make(table.bytes[range.clone()].to_vec()),
                        None => Ok(t.nil),
                    },
                    _ => Ok(t.nil),
                }
            }
        })
    }
}
macro_rules! entry {
    ($name:ident, $at:expr) => {
        pub(super) unsafe extern "C" fn $name(
            _id: Oop,
            c: Oop,
            r: Oop,
            a: *const Oop,
            n: usize,
        ) -> Oop {
            done((|| unsafe {
                run(state(c)?, r, borrowed_slice(a, n).ok_or_else(invalid)?, $at)
            })())
        }
    };
}
entry!(size, false);
entry!(at, true);
