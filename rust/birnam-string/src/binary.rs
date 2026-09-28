//! The Bytes prototype shares this owner's buffer lifetime machinery, but has
//! its own behavior and representation kind. Binary contents are never edges.
use super::*;
unsafe fn run(t: &Text, r: Oop, a: &[Operand], op: usize) -> Result<Operand, Outcome> {
    unsafe {
        if a.len() != usize::from(op == 0) {
            return Err(invalid());
        }
        if op == 0 {
            let raw = a[0].as_bytes().ok_or_else(invalid)?;
            // Native borrows are copied before even a retaining send.
            let bytes = borrowed_bytes(raw.pointer, raw.length)
                .ok_or_else(invalid)?
                .to_vec();
            return t.rooted(&[r], || t.make(bytes, Kind::Bytes));
        }
        if op == 1 || op == 2 {
            let buffers = t.buffers.borrow();
            let b = buffers
                .get(&r.addr())
                .filter(|b| b.kind == Kind::Bytes)
                .ok_or_else(invalid)?;
            return Ok(if op == 1 {
                Operand::bytes(&b.bytes)
            } else {
                Operand::word(b.bytes.len())
            });
        }
        t.rooted(&[r], || match op {
            3 => t.make(t.copy(r, Kind::Bytes)?, Kind::Bytes),
            4 => t.string(&t.copy(r, Kind::Bytes)?),
            5 => {
                let bytes = t.copy(r, Kind::String)?;
                t.send(
                    t.bytes_prototype.get(),
                    b"fromBytes:",
                    &[Operand::bytes(&bytes)],
                )
            }
            _ => Err(invalid()),
        })
    }
}
macro_rules! binary {
    ($name:ident, $op:expr) => {
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
binary!(from_bytes, 0);
binary!(bytes, 1);
binary!(length, 2);
binary!(clone, 3);
binary!(as_string, 4);
binary!(from_string, 5);
