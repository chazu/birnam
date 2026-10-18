use super::*;
use crate::candidate::call;
#[derive(Default)]
pub(super) struct Registry {
    pub entries: RefCell<Vec<(Oop, Oop)>>,
    revision: Cell<usize>,
    contents_revision: Cell<usize>,
    pub stage: RefCell<Option<Vec<(Oop, Oop)>>>,
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
    pub(super) fn record(&self, name: Oop) -> Option<Oop> {
        self.entries.borrow().iter().find(|(n, _)| *n == name).map(|(_, r)| *r)
    }
    // Called only after all callback-capable validation. Preserve other modules.
    pub(super) fn replace_record(&self, name: Oop, expected: Oop, next: Oop) -> bool {
        let mut entries = self.entries.borrow().clone();
        let Some((_, record)) = entries.iter_mut().find(|(n, _)| *n == name) else { return false; };
        if *record != expected { return false; }
        *record = next;
        self.replace_entries(entries);
        true
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
                    .flat_map(|stage| stage.iter().flat_map(|&(n, s)| [n, s])),
            )
            .collect()
    }
}
#[derive(Clone, Copy)]
pub(super) enum Op {
    Snapshot,
    Record,
    Find,
    FindCompilation,
    Replace,
    Prepare,
    Install,
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
                let mut rows = vec![q.keep(s.id.intern_name(b"sourceRegistryV1")?)?];
                for (n, source) in entries {
                    rows.push(s.vector(q, &[n, source])?);
                }
                Ok(Operand::object(s.vector(q, &rows)?))
            }
            Op::Prepare => {
                let rows = candidate::seq(s, q, object(a[0])?)?;
                let (&tag, rows) = rows.split_first().ok_or_else(invalid)?;
                if tag != s.id.intern_name(b"sourceRegistryV1")? {
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
                *s.registry.stage.borrow_mut() = Some(entries);
                Ok(Operand::object(yes))
            }
            Op::Install => {
                // One-way application of validated data. No predecessor or inverse
                // survives installation; image failure leaves applied state intact.
                let entries = s.registry.stage.take().ok_or_else(invalid)?;
                s.registry.replace_entries(entries);
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
