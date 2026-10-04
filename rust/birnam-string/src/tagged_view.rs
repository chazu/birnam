//! Read-only indexed views over an owned, schema-decoded byte snapshot.
use super::native::*;
use std::sync::Arc;
use crate::binary_rows::Field;
pub(crate) struct Table {
    pub bytes: Vec<u8>,
    pub rows: Vec<(Field, Vec<Field>)>,
}
#[derive(Clone)]
pub(crate) enum View {
    Row(Arc<Table>, usize),
    Fields(Arc<Table>, usize),
    Field(Arc<Table>, usize, Option<usize>),
}
impl Text {
    pub(crate) unsafe fn view(&self, view: View) -> Result<Operand, Outcome> {
        let out = unsafe { self.make(Vec::new(), Kind::TaggedView) }?;
        self.views.borrow_mut().insert(object(out)?.addr(), view);
        Ok(out)
    }
}
unsafe fn run(t: &Text, r: Oop, a: &[Operand], at: bool) -> Result<Operand, Outcome> {
    unsafe {
        if a.len() != usize::from(at) { return Err(invalid()) }
        let view = t.views.borrow().get(&r.addr()).cloned().ok_or_else(invalid)?;
        let integer_key = t.id.intern_name(b"Integer")?;
        let integer = object(t.id.send_named(t.namespace, b"at:", &[Operand::object(integer_key)])?)?;
        let number = |n| t.id.send_named(integer, b"fromWord:", &[Operand::word(n)]);
        if !at {
            return number(match &view { View::Row(..) => 2, View::Field(..) => 3,
                View::Fields(table, row) => table.rows[*row].1.len() });
        }
        let i = t.id.send_named(integer, b"wordOf:", a)?.as_word().ok_or_else(invalid)?;
        t.rooted(&[r, object(a[0])?], || match view {
            View::Row(table, row) => match i {
                0 => t.view(View::Field(table, row, None)),
                1 => t.view(View::Fields(table, row)),
                _ => Ok(Operand::object(t.nil)),
            },
            View::Fields(table, row) => if i < table.rows[row].1.len() {
                t.view(View::Field(table, row, Some(i)))
            } else { Ok(Operand::object(t.nil)) },
            View::Field(table, row, index) => {
                let field = match index { Some(i) => &table.rows[row].1[i], None => &table.rows[row].0 };
                match i {
                    0 => number(field.tag as usize),
                    1 => field.index.map_or(Ok(Operand::object(t.nil)), |i| number(i as usize)),
                    2 => match &field.data {
                        Some(range) => t.make(table.bytes[range.clone()].to_vec(), Kind::Bytes),
                        None => Ok(Operand::object(t.nil)),
                    },
                    _ => Ok(Operand::object(t.nil)),
                }
            }
        })
    }
}
macro_rules! entry {
    ($name:ident, $at:expr) => {
        pub(super) unsafe extern "C" fn $name(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
            done((|| unsafe { run(state(c)?, r, borrowed_slice(a, n).ok_or_else(invalid)?, $at) })())
        }
    };
}
entry!(size, false);
entry!(at, true);
