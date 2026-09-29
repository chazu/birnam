//! Image compilation and staged native ownership through existing owners.
use super::*;
use birnam_codegen::image::Plan;
#[derive(Clone, Copy)]
pub(super) enum Op {
    Bindings,
    Compile,
    Rebuild,
    Mapped,
    Method,
    Prepare,
    Install,
    Finish,
    Cancel,
    Sources,
    PrepareSources,
    SwapSources,
}
impl Op {
    pub fn count(self) -> usize {
        match self {
            Self::Bindings | Self::Finish | Self::Cancel | Self::Sources | Self::SwapSources => 0,
            Self::PrepareSources => 1,
            Self::Compile | Self::Rebuild => 3,
            Self::Mapped | Self::Install => 4,
            Self::Method | Self::Prepare => 2,
        }
    }
}
pub(super) struct Stage {
    methods: Vec<Oop>,
    units: Vec<Oop>,
    prepared: Vec<Option<Oop>>,
    finished: bool,
    sources: Option<(bool, Vec<(Oop, Oop)>)>,
}
impl Stage {
    pub fn roots(&self) -> Vec<Oop> {
        self.methods
            .iter()
            .chain(&self.units)
            .copied()
            .chain(self.prepared.iter().flatten().copied())
            .chain(
                self.sources
                    .iter()
                    .flat_map(|(_, entries)| entries.iter().flat_map(|&(n, s)| [n, s])),
            )
            .collect()
    }
}
unsafe fn value(s: &State, q: &Scope<'_>, family: &[u8], bytes: &[u8]) -> Result<Oop, Outcome> {
    unsafe {
        q.keep(object(s.send(
            s.named(family)?,
            b"fromBytes:",
            &[Operand::bytes(bytes)],
        )?)?)
    }
}
unsafe fn vector(s: &State, q: &Scope<'_>, values: &[Oop]) -> Result<Oop, Outcome> {
    unsafe {
        let mut v = q.keep(object(s.send(s.named(b"Vector")?, b"new", &[])?)?)?;
        for &x in values {
            v = q.keep(object(s.send(v, b"add:", &[Operand::object(x)])?)?)?;
        }
        Ok(v)
    }
}
unsafe fn sequence(s: &State, q: &Scope<'_>, v: Oop) -> Result<Vec<Oop>, Outcome> {
    unsafe {
        let count = q.keep(object(s.send(v, b"size", &[])?)?)?;
        let count = s
            .send(s.named(b"Integer")?, b"wordOf:", &[Operand::object(count)])?
            .as_word()
            .filter(|n| *n <= 1_048_576)
            .ok_or_else(invalid)?;
        let mut values = Vec::new();
        for i in 0..count {
            let index = q.keep(object(s.send(
                s.named(b"Integer")?,
                b"fromWord:",
                &[Operand::word(i)],
            )?)?)?;
            values.push(q.keep(object(s.send(v, b"at:", &[Operand::object(index)])?)?)?);
        }
        Ok(values)
    }
}
pub(super) unsafe fn operation(
    s: &State,
    q: &Scope<'_>,
    op: Op,
    args: &[Operand],
) -> Result<Operand, Outcome> {
    unsafe {
        if s.image_busy.replace(true) {
            return Err(invalid());
        }
        struct Busy<'a>(&'a std::cell::Cell<bool>);
        impl Drop for Busy<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        let _busy = Busy(&s.image_busy);
        let nil = q.keep(s.named(b"nil")?)?;
        let yes = q.keep(s.named(b"true")?)?;
        match op {
            Op::Sources => {
                let entries = s.sources.entries.borrow().clone();
                for &(name, source) in &entries {
                    q.keep(name)?;
                    q.keep(source)?;
                }
                let mut rows = vec![if s.sources.sealed.get() {
                    yes
                } else {
                    s.named(b"false")?
                }];
                for (name, source) in entries {
                    rows.push(vector(s, q, &[name, source])?);
                }
                return vector(s, q, &rows).map(Operand::object);
            }
            Op::PrepareSources => {
                let rows = sequence(s, q, object(args[0])?)?;
                let (&sealed, rows) = rows.split_first().ok_or_else(invalid)?;
                if sealed != yes && sealed != s.named(b"false")? {
                    return Err(invalid());
                }
                let mut entries = Vec::new();
                let mut names = std::collections::HashSet::new();
                for &row in rows {
                    let values = sequence(s, q, row)?;
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
                let mut st = s.image_stage.borrow_mut();
                let st = st.as_mut().ok_or_else(invalid)?;
                if st.finished {
                    return Err(invalid());
                }
                st.sources = Some((sealed == yes, entries));
                return Ok(Operand::object(yes));
            }
            Op::SwapSources => {
                let mut st = s.image_stage.borrow_mut();
                let st = st.as_mut().ok_or_else(invalid)?;
                if !st.finished {
                    return Err(invalid());
                }
                let (sealed, entries) = st.sources.as_mut().ok_or_else(invalid)?;
                // No callbacks or allocation after exchanging ownership. The
                // stage retains the old registry for exact compensation.
                std::mem::swap(&mut *s.sources.entries.borrow_mut(), entries);
                *sealed = s.sources.sealed.replace(*sealed);
                return Ok(Operand::object(yes));
            }
            Op::Bindings => {
                let mut rows = Vec::new();
                let count = s
                    .send(s.id.receiver, b"behaviorCount", &[])?
                    .as_word()
                    .ok_or_else(invalid)?;
                for i in 0..count {
                    let vt = q.keep(object(s.send(
                        s.id.receiver,
                        b"behaviorAt:",
                        &[Operand::word(i)],
                    )?)?)?;
                    let target = match s
                        .send(s.id.receiver, b"ownerOf:", &[Operand::object(vt)])?
                        .as_object()
                    {
                        Some(owner) => owner,
                        None if s
                            .send(s.ns, b"nameOf:", &[Operand::object(vt)])?
                            .as_object()
                            .is_some() =>
                        {
                            vt
                        }
                        None => continue,
                    };
                    // Exported behaviors such as Collection host methods directly
                    // and have no ordinary owner. Their identity is the target.
                    q.keep(target)?;
                    for j in 0..1_048_576usize {
                        let Some(selector) = s
                            .send(
                                s.id.receiver,
                                b"selectorAt:in:",
                                &[Operand::word(j), Operand::object(vt)],
                            )?
                            .as_object()
                        else {
                            break;
                        };
                        q.keep(selector)?;
                        let entry = s.send(
                            s.id.receiver,
                            b"bindingOf:in:",
                            &[Operand::object(selector), Operand::object(vt)],
                        )?;
                        if entry.as_entry().is_none() {
                            continue;
                        }
                        let Some(implementation) = s
                            .send(s.named(b"Execution")?, b"implementationOfEntry:", &[entry])?
                            .as_object()
                        else {
                            continue;
                        };
                        q.keep(implementation)?;
                        rows.push(vector(s, q, &[target, selector, implementation, nil])?);
                    }
                }
                return vector(s, q, &rows).map(Operand::object);
            }
            Op::Compile | Op::Rebuild | Op::Mapped => {
                if !matches!(op, Op::Compile) && s.image_stage.borrow().is_some() {
                    return Err(invalid());
                }
                let offset = usize::from(matches!(op, Op::Mapped));
                let p = s.bytes(object(args[offset])?)?;
                let b = s.bytes(object(args[offset + 1])?)?;
                let sel = s.bytes(object(args[offset + 2])?)?;
                let Some(plan) = Plan::new(&p, &b, &sel, !matches!(op, Op::Mapped)) else {
                    return Ok(Operand::object(nil));
                };
                if matches!(op, Op::Compile) {
                    let mut objects = Vec::new();
                    for c in &plan.sources {
                        let source = value(s, q, b"String", c)?;
                        let object = q.keep(object(s.send(
                            s.named(b"TCC")?,
                            b"compileObject:",
                            &[Operand::object(source)],
                        )?)?)?;
                        if s.is_error(object)? {
                            return Ok(Operand::object(object));
                        }
                        objects.push(s.bytes(object)?);
                    }
                    return value(s, q, b"Bytes", &plan.encode(&objects).ok_or_else(invalid)?)
                        .map(Operand::object);
                }
                let objects = if matches!(op, Op::Mapped) {
                    let bytes = s.bytes(object(args[0])?)?;
                    Some(plan.decode(&bytes).ok_or_else(invalid)?)
                } else {
                    None
                };
                let mut stage = Stage {
                    methods: Vec::new(),
                    units: Vec::new(),
                    prepared: vec![None; plan.bindings],
                    finished: false,
                    sources: None,
                };
                for i in 0..plan.sources.len() {
                    let unit = if let Some(ref objects) = objects {
                        let bytes = value(s, q, b"Bytes", &objects[i])?;
                        q.keep(object(s.send(
                            s.named(b"TCC")?,
                            b"loadObject:",
                            &[Operand::object(bytes)],
                        )?)?)?
                    } else {
                        s.native_unit(q, &plan.sources[i])?
                    };
                    if s.is_error(unit)? {
                        return Ok(Operand::object(unit));
                    }
                    if i < 2 * plan.programs {
                        let method = q.keep(object(s.send(unit, b"initialize", &[])?)?)?;
                        if method == nil || s.is_error(method)? {
                            return Ok(Operand::object(method));
                        }
                        stage.methods.push(method);
                    } else {
                        stage.units.push(unit);
                    }
                }
                *s.image_stage.borrow_mut() = Some(stage);
                return Ok(Operand::object(yes));
            }
            Op::Method => {
                let i = s
                    .send(s.named(b"Integer")?, b"wordOf:", &[args[0]])?
                    .as_word()
                    .ok_or_else(invalid)?;
                let sync = object(args[1])? == yes;
                let v = s
                    .image_stage
                    .borrow()
                    .as_ref()
                    .and_then(|st| {
                        st.methods
                            .get(i.checked_mul(2)?.checked_add(usize::from(sync))?)
                    })
                    .copied()
                    .ok_or_else(invalid)?;
                return Ok(Operand::object(v));
            }
            Op::Prepare => {
                let i = s
                    .send(s.named(b"Integer")?, b"wordOf:", &[args[0]])?
                    .as_word()
                    .ok_or_else(invalid)?;
                let unit = {
                    let stage = s.image_stage.borrow();
                    let stage = stage.as_ref().ok_or_else(invalid)?;
                    if stage.finished {
                        return Err(invalid());
                    }
                    *stage.units.get(i).ok_or_else(invalid)?
                };
                q.keep(unit)?;
                let method = q.keep(object(s.send(unit, b"initializeWith:", &[args[1]])?)?)?;
                if method == nil || s.is_error(method)? {
                    return Ok(Operand::object(method));
                }
                s.image_stage
                    .borrow_mut()
                    .as_mut()
                    .ok_or_else(invalid)?
                    .prepared[i] = Some(method);
                return Ok(Operand::object(yes));
            }
            Op::Finish => {
                let mut stage = s.image_stage.borrow_mut();
                let st = stage.as_mut().ok_or_else(invalid)?;
                if st.prepared.iter().any(Option::is_none) {
                    return Ok(Operand::object(nil));
                }
                st.finished = true;
                return Ok(Operand::object(yes));
            }
            Op::Install => {
                let i = s
                    .send(s.named(b"Integer")?, b"wordOf:", &[args[0]])?
                    .as_word()
                    .ok_or_else(invalid)?;
                let method = {
                    let stage = s.image_stage.borrow();
                    let st = stage.as_ref().ok_or_else(invalid)?;
                    if !st.finished {
                        return Err(invalid());
                    }
                    st.prepared.get(i).copied().flatten().ok_or_else(invalid)?
                };
                q.keep(method)?;
                let behavior = q.keep(object(s.send(
                    s.id.receiver,
                    b"ensurePrivateVtableFor:",
                    &[args[1]],
                )?)?)?;
                let store = q.keep(s.named(b"Store")?)?;
                return s.send(
                    store,
                    b"addMethod:on:with:",
                    &[args[2], Operand::object(behavior), Operand::object(method)],
                );
            }
            Op::Cancel => {
                *s.image_stage.borrow_mut() = None;
                return Ok(Operand::object(yes));
            }
        }
    }
}
