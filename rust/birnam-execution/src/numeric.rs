//! Successful numeric calls stay inside their original synchronous boundary.
//! Hotness is private; declaration, input validation and result construction use
//! the live Integer owner. No native service or foreign numeric payload is cached.
use super::*;
use crate::numeric_operator::Operator;
use std::collections::BTreeMap;
const HOT_CALLS: usize = 64;
const MAX_RECORDS: usize = 256;
#[derive(Default)]
pub(super) struct Registry {
    visits: BTreeMap<(usize, usize), usize>,
}
impl Registry {
    fn observe(&mut self, entry: NativeEntry) -> bool {
        let key = (
            entry.method.map_or(0, |f| f as usize),
            entry.context as usize,
        );
        if !self.visits.contains_key(&key) && self.visits.len() == MAX_RECORDS {
            self.visits.pop_first();
        }
        let visits = self.visits.entry(key).or_default();
        *visits = visits.saturating_add(1);
        *visits >= HOT_CALLS
    }
}
pub(super) unsafe extern "C" fn evaluate(
    c: *mut c_void,
    r: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let s = state(c)?;
        if r != s.receiver {
            return Err(invalid());
        }
        let Some([op, left, right]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let op = op
            .as_word()
            .and_then(|v| u32::try_from(v).ok())
            .and_then(Operator::from_declaration)
            .ok_or_else(invalid)?;
        let left = left.as_word().ok_or_else(invalid)? as i64;
        let right = right.as_word().ok_or_else(invalid)? as i64;
        Ok(op
            .evaluate(left, right)
            .map_or_else(Operand::unit, |v| Operand::word(v as usize)))
    })())
}
pub(super) unsafe fn invoke(
    s: &Execution,
    selected: NativeEntry,
    receiver: Oop,
    arguments: &[Operand],
) -> Result<Operand, Outcome> {
    unsafe {
        let roots = s.scope(&[receiver]);
        for arg in arguments {
            if let Some(value) = arg.as_object() {
                roots.keep(value);
            }
        }
        let executable = if let Some(method) = s.selected(selected) {
            roots.keep(method);
            s.methods
                .borrow()
                .get(&method.addr())
                .ok_or_else(invalid)?
                .entry
        } else {
            selected
        };
        let warm = arguments.len() == 1 && s.numeric.borrow_mut().observe(executable);
        if warm {
            let answer = s.optional_query(
                s.integer_factory()?,
                b"evaluate:receiver:argument:with:",
                &[
                    Operand::entry(executable),
                    Operand::object(receiver),
                    arguments[0],
                    Operand::object(s.receiver),
                ],
            )?;
            if answer.kind() != native::UNIT {
                return Ok(answer);
            }
        }
        selected.method.ok_or_else(invalid)?(
            selected.context,
            receiver,
            arguments.as_ptr(),
            arguments.len(),
        )
        .into_result()
    }
}
