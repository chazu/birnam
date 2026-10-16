use super::*;
use crate::candidate::call;
#[derive(Default)]
pub(super) struct Registry {
    pub entries: RefCell<Vec<(Oop, Oop)>>,
    sealed: Cell<bool>,
    revision: Cell<usize>,
    contents_revision: Cell<usize>,
    pub stage: RefCell<Option<Stage>>,
}
pub(super) struct Stage {
    sealed: bool,
    entries: Vec<(Oop, Oop)>,
    basis: usize,
    published: bool,
}
impl Registry {
    pub(super) fn revision(&self)->usize {self.contents_revision.get()}
    pub(super) fn replace_entries(&self, entries: Vec<(Oop,Oop)>) {
        self.revision.set(self.revision.get().checked_add(1).expect("source revision exhausted"));
        if *self.entries.borrow()!=entries {
            self.contents_revision.set(self.contents_revision.get().checked_add(1).expect("source content revision exhausted"));
        }
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
    FindCompilation,
    Replace,
    Seal,
    Prepare,
    Swap,
    Rollback,
}
impl Op {
    pub fn count(self) -> usize {
        match self {
            Self::Record | Self::Replace => 2,
            Self::Find | Self::FindCompilation | Self::Prepare => 1,
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
                    if !s.exact(q,*name,b"SymbolVt")? || !names.insert(*name) {return Err(invalid())}
                    let record=call(s,q,s.named(b"Codegen")?,b"compiledSourceFor:",&[Operand::object(*source)])?;
                    if record!=*source {return Err(invalid())}
                    let Some((_,metadata))=s.revision(q,record)? else{return Err(invalid())};
                    if s.symbol_bytes(q,*name)?.as_deref()!=Some(metadata.name.as_slice()){return Err(invalid())}
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
                if *s.registry.entries.borrow()!=stage.entries {
                    s.registry.contents_revision.set(s.registry.contents_revision.get().checked_add(1).ok_or_else(invalid)?);
                }
                std::mem::swap(&mut *s.registry.entries.borrow_mut(), &mut stage.entries);
                stage.sealed = s.registry.sealed.replace(stage.sealed);
                s.registry.revision.set(next);
                stage.basis = next;
                stage.published = !rollback;
                Ok(Operand::object(yes))
            }
            Op::FindCompilation => {
                let source=object(a[0])?;
                let entries=s.registry.entries.borrow().clone();
                for &(name,record) in &entries {q.keep(name)?;q.keep(record)?;}
                for (_,record) in entries {
                    if s.source_text(q,record)?==source {return Ok(Operand::object(record))}
                }
                Ok(nil_value)
            }
            Op::Record | Op::Replace | Op::Find => {
                let is_record=!matches!(op,Op::Find);
                let replacing=matches!(op,Op::Replace);
                let name_object=object(a[if is_record {1}else{0}])?;
                let Some(name)=s.symbol_bytes(q,name_object)? else {
                    if is_record {s.note(q,b"RustCodegen module name was not a Symbol")?;}
                    return Ok(nil_value)
                };
                let recording=if is_record {
                    let record=object(a[0])?;
                    let owner=call(s,q,s.named(b"Codegen")?,b"compiledSourceFor:",&[Operand::object(record)])?;
                    if owner!=record {return Err(invalid())}
                    let Some((bytes,revision))=s.revision(q,record)? else {
                        s.note(q,b"RustCodegen module source is not indexable")?;return Ok(nil_value)
                    };
                    if revision.name!=name {s.note(q,b"RustCodegen module source identity does not match its name")?;return Ok(nil_value)}
                    Some((record,s.source_text(q,record)?,bytes))
                } else {None};
                loop {
                    if is_record && !replacing && s.registry.sealed.get() {
                        s.note(q,b"RustCodegen module introduction is not available")?;return Ok(nil_value)
                    }
                    let revision=s.registry.revision.get();
                    let entries=s.registry.entries.borrow().clone();
                    for &(key,record) in &entries {q.keep(key)?;q.keep(record)?;}
                    let mut found=None;
                    for (index,&(old_name,record)) in entries.iter().enumerate() {
                        if s.symbol_bytes(q,old_name)?.as_deref()==Some(name.as_slice()) {found=Some((index,record));break}
                    }
                    let old_text=if let Some((_,record))=found {Some(s.source_text(q,record)?)}else{None};
                    let same=if let (Some(old),Some((_,_,bytes)))=(old_text,&recording) {s.bytes(old)?==*bytes}else{false};
                    if s.registry.revision.get()!=revision {continue}
                    let Some((record,text,_))=&recording else {return Ok(Operand::object(old_text.unwrap_or(nil)))};
                    if replacing {
                        let Some((index,prior))=found else {s.note(q,b"RustCodegen module is not registered")?;return Ok(nil_value)};
                        if prior==*record {return Ok(Operand::object(*text))}
                        s.registry.revision.set(revision.checked_add(1).ok_or_else(invalid)?);
                        s.registry.contents_revision.set(s.registry.contents_revision.get().checked_add(1).ok_or_else(invalid)?);
                        s.registry.entries.borrow_mut()[index]=(name_object,*record);
                        return Ok(Operand::object(*text))
                    }
                    if found.is_some() {
                        if same{return Ok(Operand::object(*text))}
                        s.note(q,b"RustCodegen duplicate module source")?;return Ok(nil_value)
                    }
                    s.registry.revision.set(revision.checked_add(1).ok_or_else(invalid)?);
                    s.registry.contents_revision.set(s.registry.contents_revision.get().checked_add(1).ok_or_else(invalid)?);
                    s.registry.entries.borrow_mut().push((name_object,*record));
                    return Ok(Operand::object(*text))
                }
            }
        }
    }
}
