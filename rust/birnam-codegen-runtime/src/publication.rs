//! Live Id calls to the generation owner; no state or private payload borrow.
use super::*;
pub(super) unsafe fn send(
    s: &State,
    q: &Scope<'_>,
    name: &[u8],
    a: &[Operand],
) -> Result<Operand, Outcome> {
    unsafe {
        let owner = q.keep(s.named(b"Generation")?)?;
        s.send(owner, name, a)
    }
}
pub(super) unsafe fn snapshot(s: &State, q: &Scope<'_>) -> Result<Vec<(Oop, Oop)>, Outcome> {
    unsafe {
        let rows = q.keep(object(send(s, q, b"sourceRegistry", &[])?)?)?;
        let n = s.index(q, rows, b"size")?;
        if n == 0 || n > 1_000_001 { return Err(invalid()); }
        let zero = q.keep(object(s.send(s.named(b"Integer")?, b"fromWord:", &[Operand::word(0)])?)?)?;
        let tag = q.keep(object(s.send(rows, b"at:", &[Operand::object(zero)])?)?)?;
        if tag != s.id.intern_name(b"sourceRegistryV1")? { return Err(invalid()); }
        let mut entries = Vec::new();
        for i in 1..n {
            let index = q.keep(object(s.send(
                s.named(b"Integer")?,
                b"fromWord:",
                &[Operand::word(i)],
            )?)?)?;
            let row = q.keep(object(s.send(rows, b"at:", &[Operand::object(index)])?)?)?;
            if s.index(q, row, b"size")? != 2 {
                return Err(invalid());
            }
            let mut values = Vec::new();
            for i in 0..2 {
                let index = q.keep(object(s.send(
                    s.named(b"Integer")?,
                    b"fromWord:",
                    &[Operand::word(i)],
                )?)?)?;
                values.push(q.keep(object(s.send(row, b"at:", &[Operand::object(index)])?)?)?);
            }
            entries.push((values[0], values[1]));
        }
        Ok(entries)
    }
}
pub(super) unsafe fn fail(
    s: &State,
    q: &Scope<'_>,
    kind: &[u8],
    message: &[u8],
) -> Result<Operand, Outcome> {
    unsafe {
        s.sources.note(message);
        error(s, q, kind, message)
    }
}
pub(super) unsafe fn error(
    s: &State,
    q: &Scope<'_>,
    kind: &[u8],
    message: &[u8],
) -> Result<Operand, Outcome> {
    unsafe {
        let kind = q.keep(s.id.intern_name(kind)?)?;
        let text = s.text(q, message)?;
        s.send(
            s.named(b"Error")?,
            b"kind:message:",
            &[Operand::object(kind), Operand::object(text)],
        )
    }
}
