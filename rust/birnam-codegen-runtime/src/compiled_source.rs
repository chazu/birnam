//! A compilation is an ordinary persistent Vector: tag, original source, metadata.
//! Registries and revision snapshots own these values. There is no native table,
//! String attachment, or new bootstrap object family.
use super::*;
pub(super) unsafe fn is_record(s: &State, q: &Scope<'_>, value: Oop) -> Result<bool, Outcome> {
    unsafe {
        if !s.exact(q, value, b"VectorVt")? || s.index(q, value, b"size")? != 3 {
            return Ok(false);
        }
        let zero = q.keep(object(s.send(
            s.named(b"Integer")?,
            b"fromWord:",
            &[Operand::word(0)],
        )?)?)?;
        let tag = q.keep(object(s.send(value, b"at:", &[Operand::object(zero)])?)?)?;
        Ok(s.symbol_bytes(q, tag)?.as_deref() == Some(b"compiledSourceV1"))
    }
}
pub(super) unsafe fn field(
    s: &State,
    q: &Scope<'_>,
    value: Oop,
    index: usize,
) -> Result<Oop, Outcome> {
    unsafe {
        let index = q.keep(object(s.send(
            s.named(b"Integer")?,
            b"fromWord:",
            &[Operand::word(index)],
        )?)?)?;
        q.keep(object(s.send(value, b"at:", &[Operand::object(index)])?)?)
    }
}
pub(super) unsafe fn text(s: &State, q: &Scope<'_>, value: Oop) -> Result<Option<Oop>, Outcome> {
    unsafe {
        if s.exact(q, value, b"StringVt")? {
            return Ok(Some(value));
        }
        if !is_record(s, q, value)? {
            return Ok(None);
        }
        let text = field(s, q, value, 1)?;
        Ok(s.exact(q, text, b"StringVt")?.then_some(text))
    }
}
pub(super) unsafe fn find(s: &State, q: &Scope<'_>, value: Oop) -> Result<Option<Oop>, Outcome> {
    unsafe {
        if is_record(s, q, value)? {
            return Ok(Some(value));
        }
        if !s.exact(q, value, b"StringVt")? {
            return Ok(None);
        }
        let owner = s
            .send(
                s.ns,
                b"at:",
                &[Operand::object(s.id.intern_name(b"Generation")?)],
            )?
            .as_object();
        let Some(owner) = owner else { return Ok(None) };
        q.keep(owner)?;
        let found = q.keep(object(s.send(
            owner,
            b"compilationForSource:",
            &[Operand::object(value)],
        )?)?)?;
        Ok((found != s.named(b"nil")?).then_some(found))
    }
}
pub(super) unsafe fn make(
    s: &State,
    q: &Scope<'_>,
    text: Oop,
    metadata: Oop,
) -> Result<Oop, Outcome> {
    unsafe {
        let tag = q.keep(s.id.intern_name(b"compiledSourceV1")?)?;
        s.vector(q, &[tag, text, metadata])
    }
}
