//! Operation-local observations of the authored source owner through Id only.
//! No registry state, slot-layout knowledge, cache, or rooting service lives here.
use super::*;
use candidate::{call, seq};

pub(super) unsafe fn owner(s: &State, q: &Scope<'_>) -> Result<Oop, Outcome> {
    unsafe {
        let name = q.keep(s.id.intern_name(b"SourceRegistry")?)?;
        let owner = call(s, q, s.named(b"Globals")?, b"at:", &[Operand::object(name)])?;
        if owner == s.named(b"nil")? { return Err(invalid()); }
        Ok(owner)
    }
}
pub(super) unsafe fn record(s: &State, q: &Scope<'_>, owner: Oop, name: Oop) -> Result<Option<Oop>, Outcome> {
    unsafe {
        let value = call(s, q, owner, b"recordFor:", &[Operand::object(name)])?;
        if s.is_error(value)? { return Err(Outcome::loaded(Operand::object(value))); }
        Ok((value != s.named(b"nil")?).then_some(value))
    }
}
pub(super) unsafe fn revision(s: &State, q: &Scope<'_>, owner: Oop) -> Result<usize, Outcome> {
    unsafe {
        s.index(q, owner, b"revision")
    }
}
pub(super) unsafe fn entries(s: &State, q: &Scope<'_>, owner: Oop) -> Result<Vec<(Oop, Oop)>, Outcome> {
    unsafe {
        let rows = call(s, q, owner, b"sourceRegistry", &[])?;
        let rows = seq(s, q, rows)?;
        let (&tag, rows) = rows.split_first().ok_or_else(invalid)?;
        if tag != s.id.intern_name(b"sourceRegistryV1")? { return Err(invalid()); }
        let mut entries = Vec::new();
        for &row in rows {
            let fields = seq(s, q, row)?;
            let [name, record] = fields.as_slice() else { return Err(invalid()); };
            entries.push((*name, *record));
        }
        Ok(entries)
    }
}
pub(super) unsafe fn replace(s: &State, q: &Scope<'_>, owner: Oop, name: Oop, expected: Oop, next: Oop) -> Result<(), Outcome> {
    unsafe {
        let result = call(s, q, owner, b"replaceRecord:named:expected:",
            &[Operand::object(next), Operand::object(name), Operand::object(expected)])?;
        if s.is_error(result)? { return Err(Outcome::loaded(Operand::object(result))); }
        if result != owner { return Err(invalid()); }
        Ok(())
    }
}
