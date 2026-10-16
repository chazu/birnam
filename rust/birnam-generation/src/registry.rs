use super::*;
#[derive(Default)]
pub(super) struct Registry {
    pub entries: RefCell<Vec<(Oop, Oop)>>,
    sealed: Cell<bool>,
    revision: Cell<usize>,
    pub stage: RefCell<Option<Stage>>,
}
pub(super) struct Stage {
    sealed: bool,
    entries: Vec<(Oop, Oop)>,
    basis: usize,
    published: bool,
}
impl Registry {
    pub(super) fn replace_entries(&self, entries: Vec<(Oop,Oop)>) {
        self.revision.set(self.revision.get().checked_add(1).expect("source revision exhausted"));
        *self.entries.borrow_mut() = entries;
    }
    pub fn roots(&self) -> Vec<Oop> {
        self.entries
            .borrow()
            .iter()
            .flat_map(|&(n, s)| [n, s])
            .chain(
                self.stage
                    .borrow()
                    .iter()
                    .flat_map(|stage| stage.entries.iter().flat_map(|&(n, s)| [n, s])),
            )
            .collect()
    }
}
#[derive(Clone, Copy)]
pub(super) enum Op {
    Snapshot,
    Sealed,
    Record,
    Find,
    Seal,
    Prepare,
    Swap,
    Rollback,
}
impl Op {
    pub fn count(self) -> usize {
        match self {
            Self::Record => 2,
            Self::Find | Self::Prepare => 1,
            _ => 0,
        }
    }
}
pub(super) unsafe fn operation(
    s: &State,
    q: &Scope<'_>,
    op: Op,
    a: &[Operand],
) -> Result<Operand, Outcome> {
    unsafe {
        let nil = q.keep(s.named(b"nil")?)?;
        let nil_value = Operand::object(nil);
        let yes = q.keep(s.named(b"true")?)?;
        match op {
            Op::Snapshot => {
                let entries = s.registry.entries.borrow().clone();
                for &(name, source) in &entries {
                    q.keep(name)?;
                    q.keep(source)?;
                }
                let mut rows = vec![s.named(if s.registry.sealed.get() {
                    b"true"
                } else {
                    b"false"
                })?];
                for (n, source) in entries {
                    rows.push(s.vector(q, &[n, source])?);
                }
                Ok(Operand::object(s.vector(q, &rows)?))
            }
            Op::Sealed => Ok(Operand::object(s.named(if s.registry.sealed.get() {
                b"true"
            } else {
                b"false"
            })?)),
            Op::Seal => {
                s.registry.revision.set(s.registry.revision.get().checked_add(1).ok_or_else(invalid)?);
                s.registry.sealed.set(true);
                Ok(Operand::object(yes))
            }
            Op::Prepare => {
                let rows = candidate::seq(s, q, object(a[0])?)?;
                let (&sealed, rows) = rows.split_first().ok_or_else(invalid)?;
                if sealed != yes && sealed != s.named(b"false")? {
                    return Err(invalid());
                }
                let mut entries = Vec::new();
                let mut names = std::collections::HashSet::new();
                for &row in rows {
                    let values = candidate::seq(s, q, row)?;
                    let [name, source] = values.as_slice() else {
                        return Err(invalid());
                    };
                    if !s.exact(q, *name, b"SymbolVt")?
                        || !s.exact(q, *source, b"StringVt")?
                        || !names.insert(*name)
                    {
                        return Err(invalid());
                    }
                    entries.push((*name, *source));
                }
                *s.registry.stage.borrow_mut() = Some(Stage { sealed: sealed == yes, entries, basis: s.registry.revision.get(), published: false });
                Ok(Operand::object(yes))
            }
            Op::Swap | Op::Rollback => {
                let no = s.named(b"false")?;
                let mut stage = s.registry.stage.borrow_mut();
                let stage = stage.as_mut().ok_or_else(invalid)?;
                let rollback = matches!(op, Op::Rollback);
                if rollback != stage.published || s.registry.revision.get() != stage.basis {
                    return Ok(Operand::object(no));
                }
                let next = stage.basis.checked_add(1).ok_or_else(invalid)?;
                std::mem::swap(&mut *s.registry.entries.borrow_mut(), &mut stage.entries);
                stage.sealed = s.registry.sealed.replace(stage.sealed);
                s.registry.revision.set(next);
                stage.basis = next;
                stage.published = !rollback;
                Ok(Operand::object(yes))
            }
            Op::Record | Op::Find => {
                let is_record = matches!(op, Op::Record);
                let name_object = object(a[if is_record { 1 } else { 0 }])?;
                let Some(name) = s.symbol_bytes(q, name_object)? else {
                    if is_record {
                        s.note(q, b"RustCodegen module name was not a Symbol")?;
                    }
                    return Ok(nil_value);
                };
                let recording = if is_record {
                    let source = object(a[0])?;
                    let Some((bytes, revision)) = s.revision(q, source)? else {
                        s.note(q, b"RustCodegen module source is not indexable")?;
                        return Ok(nil_value);
                    };
                    if revision.name != name {
                        s.note(
                            q,
                            b"RustCodegen module source identity does not match its name",
                        )?;
                        return Ok(nil_value);
                    }
                    Some((source, bytes))
                } else {
                    None
                };
                // The registry is append-only. Inspect snapshots without a borrow
                // across sends; retry if a dependency registered a new entry.
                loop {
                    if recording.is_some() && s.registry.sealed.get() {
                        s.note(q, b"RustCodegen module introduction is not available")?;
                        return Ok(nil_value);
                    }
                    let entries = s.registry.entries.borrow().clone();
                    let mut found = None;
                    for &(old_name, old_source) in &entries {
                        if s.symbol_bytes(q, old_name)?.as_deref() == Some(name.as_slice()) {
                            found = Some(old_source);
                            break;
                        }
                    }
                    let same = if let (Some(old), Some((_, bytes))) = (found, &recording) {
                        s.source_bytes(q, old)?.as_deref() == Some(bytes.as_slice())
                    } else {
                        false
                    };
                    if s.registry.entries.borrow().len() != entries.len() {
                        continue;
                    }
                    if recording.is_some() && s.registry.sealed.get() {
                        s.note(q, b"RustCodegen module introduction is not available")?;
                        return Ok(nil_value);
                    }
                    let Some((source, _)) = recording else {
                        return Ok(Operand::object(found.unwrap_or(nil)));
                    };
                    if found.is_some() {
                        if same {
                            return Ok(Operand::object(source));
                        }
                        s.note(q, b"RustCodegen duplicate module source")?;
                        return Ok(nil_value);
                    }
                    s.registry.revision.set(s.registry.revision.get().checked_add(1).ok_or_else(invalid)?);
                    s.registry.entries.borrow_mut().push((name_object, source));
                    return Ok(Operand::object(source));
                }
            }
        }
    }
}
