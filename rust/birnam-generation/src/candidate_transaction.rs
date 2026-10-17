//! Generation publication and exact inverse journals through their owners.
use super::*;
use preparation::{Binding, Global, Prepared};

#[derive(Clone)]
pub(super) struct Constructed {
    methods: Vec<Option<Oop>>,
    globals: Vec<Global>,
}
impl Constructed {
    pub(super) fn roots(&self) -> Vec<Oop> {
        self.methods
            .iter()
            .flatten()
            .copied()
            .chain(
                self.globals
                    .iter()
                    .flat_map(|g| std::iter::once(g.name).chain(g.value)),
            )
            .collect()
    }
}
#[derive(Clone)]
struct FieldWrite {
    entity: Oop,
    attribute: Oop,
    before: Oop,
    after: Oop,
}
#[derive(Clone)]
struct GlobalWrite {
    name: Oop,
    before: Option<Oop>,
    after: Option<Oop>,
}
#[derive(Clone)]
pub(super) struct Activation {
    before: ledger::Ledger,
    after: ledger::Ledger,
    bindings_before: Vec<Binding>,
    bindings_after: Vec<Binding>,
    globals: Vec<GlobalWrite>,
    binding_reservation: Oop,
    sources_before: Vec<(Oop, Oop)>,
}
impl Activation {
    pub(super) fn roots(&self) -> Vec<Oop> {
        let mut values = self.before.roots();
        values.push(self.binding_reservation);
        values.extend(self.after.roots());
        for b in self.bindings_before.iter().chain(&self.bindings_after) {
            values.extend([b.name, b.target, b.behavior, b.selector]);
            values.extend(b.method);
        }
        for f in &self.globals {
            values.push(f.name);
            values.extend(f.before);
            values.extend(f.after);
        }
        for &(name, source) in &self.sources_before {
            values.extend([name, source]);
        }
        values
    }
}
#[derive(Clone)]
enum Write {
    Field(FieldWrite),
    Current { before: Oop, after: Oop },
    Global(GlobalWrite),
    Binding {
        before: Binding,
        method: Option<Oop>,
    },
}
unsafe fn failure(s: &State, q: &Scope<'_>, kind: &[u8], message: &[u8]) -> Outcome {
    unsafe { fail(s, q, kind, message).map_or_else(|e| e, Outcome::loaded) }
}
unsafe fn raw_field(s: &State, q: &Scope<'_>, entity: Oop, attribute: Oop) -> Result<Oop, Outcome> {
    unsafe { field(s, q, entity, &s.symbol_bytes(q, attribute)?.ok_or_else(invalid)?) }
}
unsafe fn field_write(
    s: &State,
    q: &Scope<'_>,
    entity: Oop,
    attribute: &[u8],
    value: Oop,
) -> Result<FieldWrite, Outcome> {
    unsafe {
        let attribute = symbol(s, q, attribute)?;
        Ok(FieldWrite {
            entity,
            attribute,
            before: raw_field(s, q, entity, attribute)?,
            after: value,
        })
    }
}
unsafe fn checked_globals(s: &State, q: &Scope<'_>, selector: &[u8], args: &[Operand]) -> Result<(), Outcome> {
    unsafe {
        let globals = q.keep(s.named(b"Globals")?)?;
        let value = call(s, q, globals, selector, args)?;
        if value == globals { Ok(()) }
        else if s.is_error(value)? { Err(Outcome::loaded(Operand::object(value))) }
        else { Err(failure(s, q, b"candidateCommitFailed", b"Globals did not accept the reservation operation")) }
    }
}
unsafe fn reserve_bindings(s: &State, q: &Scope<'_>, token: Oop, required: usize) -> Result<(), Outcome> {
    unsafe {
        if required == 0 { return Ok(()) }
        let available = call(s, q, s.named(b"Globals")?, b"reservedBindingCompensationFor:", &[Operand::object(token)])?;
        let available = preparation::number(s, available)?;
        if available >= required { return Ok(()) }
        let count = call(s, q, s.named(b"Integer")?, b"fromWord:", &[Operand::word(required - available)])?;
        checked_globals(s, q, b"reserveBindingCompensationFor:count:", &[Operand::object(token), Operand::object(count)])
    }
}
unsafe fn release_bindings(s: &State, q: &Scope<'_>, binding_token: Oop, binding_count: usize) -> Result<(), Outcome> {
    unsafe {
        if binding_count != 0 {
            checked_globals(s, q, b"releaseBindingCompensationFor:", &[Operand::object(binding_token)])?;
        }
        Ok(())
    }
}
pub(super) unsafe fn discard_fields(s: &State, q: &Scope<'_>, unit: Oop) -> Result<(), Outcome> {
    unsafe { set_field(s, q, unit, b"state", symbol(s, q, b"discarded")?) }
}
unsafe fn restore_field(s: &State, q: &Scope<'_>, f: &FieldWrite, inverse: bool) -> Result<(), Outcome> {
    unsafe {
        let (expected, next) = if inverse { (f.after, f.before) } else { (f.before, f.after) };
        if raw_field(s, q, f.entity, f.attribute)? != expected { return Err(invalid()); }
        set_field(s, q, f.entity, &s.symbol_bytes(q, f.attribute)?.ok_or_else(invalid)?, next)?;
        if raw_field(s, q, f.entity, f.attribute)? != next { return Err(invalid()); }
        Ok(())
    }
}
unsafe fn set_current(s: &State, q: &Scope<'_>, expected: Oop, next: Oop) -> Result<(), Outcome> {
    unsafe {
        if current(s, q)? != expected { return Err(invalid()); }
        let result = call(s, q, prototype(s, q, b"DispatchGeneration")?, b"current:", &[Operand::object(next)])?;
        if s.is_error(result)? { return Err(Outcome::loaded(Operand::object(result))); }
        if current(s, q)? != next { return Err(invalid()); }
        Ok(())
    }
}
pub(super) unsafe fn is_method(
    s: &State,
    entry: Option<NativeEntry>,
    method: Option<Oop>,
) -> Result<bool, Outcome> {
    unsafe {
        Ok(match (entry, method) {
            (None, None) => true,
            (Some(entry), Some(method)) => {
                s.send(
                    s.named(b"Execution")?,
                    b"methodOfEntry:",
                    &[Operand::entry(entry)],
                )?
                .as_object()
                    == Some(method)
            }
            _ => false,
        })
    }
}
unsafe fn put_method(
    s: &State,
    q: &Scope<'_>,
    b: &Binding,
    method: Option<Oop>,
) -> Result<(), Outcome> {
    unsafe {
        // Use the current setter without restricting its other effects or its
        // non-error return value. Both publication and compensation inspect the
        // actual resulting binding before claiming success.
        let result = if let Some(method) = method {
            s.send(b.behavior, b"addMethod:as:",
                &[Operand::object(method), Operand::object(b.selector)])
        } else {
            s.send(b.behavior, b"removeMethod:", &[Operand::object(b.selector)])
        };
        let result = result?;
        if let Some(value) = result.as_object() {
            q.keep(value)?;
            if s.is_error(value)? { return Err(Outcome::loaded(result)); }
        }
        Ok(())
    }
}
unsafe fn restore_global(
    s: &State,
    q: &Scope<'_>,
    owner: Oop,
    f: &GlobalWrite,
    inverse: bool,
) -> Result<(), Outcome> {
    unsafe {
        let (expected, previous) = if inverse {
            (f.after, f.before)
        } else {
            (f.before, f.after)
        };
        let mut rows = vec![];
        for value in [expected, previous] {
            let mut row = call(s, q, s.named(b"Vector")?, b"new", &[])?;
            if let Some(value) = value {
                row = call(s, q, row, b"add:", &[Operand::object(value)])?;
            }
            rows.push(row);
        }
        let globals = q.keep(s.named(b"Globals")?)?;
        let result = call(
            s,
            q,
            globals,
            b"restoreBinding:expected:previous:reservation:",
            &[
                Operand::object(f.name),
                Operand::object(rows[0]),
                Operand::object(rows[1]),
                Operand::object(owner),
            ],
        )?;
        if result == globals {
            Ok(())
        } else if s.is_error(result)? {
            Err(Outcome::loaded(Operand::object(result)))
        } else {
            Err(failure(
                s,
                q,
                b"candidateCommitFailed",
                b"Globals did not accept the candidate operation",
            ))
        }
    }
}
unsafe fn compensate(s: &State, q: &Scope<'_>, binding_token: Oop, writes: &[Write]) -> bool {
    unsafe {
        let mut complete = true;
        for write in writes.iter().rev() {
            let restored = (|| {
                match write {
                    Write::Current { before, after } => {
                        let now = current(s, q)?;
                        if now != *before { set_current(s, q, *after, *before)?; }
                    }
                    Write::Global(f) => {
                        let current = preparation::global(s, q, f.name)?;
                        if current == f.before {
                            return Ok(());
                        }
                        if current != f.after {
                            return Err(invalid());
                        }
                        restore_global(s, q, binding_token, f, true)?;
                        if preparation::global(s, q, f.name)? != f.before {
                            return Err(invalid());
                        }
                    }
                    Write::Field(f) => {
                        let current = raw_field(s, q, f.entity, f.attribute)?;
                        if current == f.before {
                            return Ok(());
                        }
                        if current != f.after {
                            return Err(invalid());
                        }
                        restore_field(s, q, f, true)?;
                        if raw_field(s, q, f.entity, f.attribute)? != f.before {
                            return Err(invalid());
                        }
                    }
                    Write::Binding { before, method } => {
                        let current = preparation::direct(s, before.behavior, before.selector)?;
                        if current == before.entry {
                            return Ok(());
                        }
                        if !is_method(s, current, *method)? {
                            return Err(invalid());
                        }
                        // An observer may report failure after the exact inverse
                        // has committed. The dictionary observation decides.
                        let _ = put_method(s, q, before, before.method);
                        if preparation::direct(s, before.behavior, before.selector)? != before.entry
                        {
                            return Err(invalid());
                        }
                    }
                }
                Ok::<_, Outcome>(())
            })();
            complete &= restored.is_ok();
        }
        complete
    }
}
unsafe fn construct(s: &State, q: &Scope<'_>, unit: Oop) -> Result<Constructed, Outcome> {
    unsafe {
        let (cached, started, factory, adapters, outputs, count) = {
            let entries = s.candidates.entries.borrow();
            let c = entries
                .iter()
                .find(|c| c.unit == unit)
                .ok_or_else(invalid)?;
            let m = c.mapped.as_ref().ok_or_else(invalid)?;
            (
                c.constructed.clone(),
                c.construction_started,
                m.factory,
                m.adapters.clone(),
                c.plan.outputs.clone(),
                c.plan.metadata.operations.len(),
            )
        };
        if let Some(c) = cached {
            return Ok(c);
        }
        if started {
            return Err(failure(
                s,
                q,
                b"candidateConstructionFailed",
                b"failed candidate construction cannot be replayed",
            ));
        }
        s.candidates
            .entries
            .borrow_mut()
            .iter_mut()
            .find(|c| c.unit == unit)
            .ok_or_else(invalid)?
            .construction_started = true;
        let empty = s.vector(q, &[])?;
        let block = call(
            s,
            q,
            s.named(b"Block")?,
            b"fromMethod:captures:",
            &[Operand::object(factory), Operand::object(empty)],
        )?;
        let machine = call(s, q, s.named(b"Machine")?, b"new", &[])?;
        let output = call(s, q, machine, b"runBlock:", &[Operand::object(block)])?;
        if s.is_error(output)? {
            return Err(Outcome::loaded(Operand::object(output)));
        }
        let values = seq(s, q, output)?;
        if values.len() != outputs.len() {
            return Err(invalid());
        }
        let mut result = Constructed {
            methods: vec![None; count],
            globals: vec![],
        };
        let mut adapter = 0;
        for (output, value) in outputs.into_iter().zip(values) {
            match output {
                code::Output::Global(name) => {
                    let name = symbol(s, q, &name)?;
                    result.globals.retain(|g| g.name != name);
                    result.globals.push(Global {
                        name,
                        value: Some(value),
                    });
                }
                code::Output::Method(index) => {
                    let method = call(
                        s,
                        q,
                        *adapters.get(adapter).ok_or_else(invalid)?,
                        b"initializeWith:",
                        &[Operand::object(value)],
                    )?;
                    if s.is_error(method)? {
                        return Err(Outcome::loaded(Operand::object(method)));
                    }
                    result.methods[index] = Some(method);
                    adapter += 1;
                }
            }
        }
        if adapter != adapters.len() {
            return Err(invalid());
        }
        s.candidates
            .entries
            .borrow_mut()
            .iter_mut()
            .find(|c| c.unit == unit)
            .ok_or_else(invalid)?
            .constructed = Some(result.clone());
        Ok(result)
    }
}
unsafe fn publication(s: &State, q: &Scope<'_>, p: &Prepared) -> Result<Vec<FieldWrite>, Outcome> {
    unsafe {
        let [_, unit, _, prior, _, _] = p.plan;
        let nil = s.named(b"nil")?;
        let state = symbol(s, q, if p.rollback { b"rolledBack" } else { b"active" })?;
        let mut changes = vec![
            field_write(s, q, unit, b"state", state)?,
            field_write(
                s,
                q,
                unit,
                b"rollbackAvailable",
                s.named(if p.rollback { b"false" } else { b"true" })?,
            )?,
        ];
        if prior != nil {
            changes.push(field_write(
                s,
                q,
                prior,
                b"state",
                symbol(s, q, if p.rollback { b"active" } else { b"superseded" })?,
            )?);
        }
        Ok(changes)
    }
}
unsafe fn sources(s: &State, q: &Scope<'_>, map: Oop) -> Result<Vec<(Oop, Oop)>, Outcome> {
    unsafe {
        let mut out = vec![];
        for row in seq(s, q, map)? {
            let row = seq(s, q, row)?;
            if row.len() != 3 {
                return Err(invalid());
            }
            let revision = seq(s, q, row[1])?;
            if revision.len() != 8 {
                return Err(invalid());
            }
            out.push((row[0], revision[7]));
        }
        Ok(out)
    }
}
unsafe fn take_prepared(s: &State, q: &Scope<'_>, token: Oop) -> Result<Prepared, Outcome> {
    unsafe {
        let token = preparation::number(s, token)?;
        let roots = s
            .candidates
            .prepared
            .borrow()
            .iter()
            .find(|p| p.token == token)
            .map(|p| p.roots());
        let Some(roots) = roots else {
            return Err(failure(
                s,
                q,
                b"invalidPreparedToken",
                b"prepared token is unknown or consumed",
            ));
        };
        // Root while the registry still owns every edge. Whether rooting succeeds
        // or fails, remove the capability before returning to its caller.
        let rooted = roots.into_iter().try_for_each(|v| q.keep(v).map(|_| ()));
        let mut entries = s.candidates.prepared.borrow_mut();
        let index = entries
            .iter()
            .position(|p| p.token == token)
            .ok_or_else(invalid)?;
        let p = entries.remove(index);
        drop(entries);
        rooted?;
        Ok(p)
    }
}
pub(super) unsafe fn commit(
    s: &State,
    q: &Scope<'_>,
    token: Oop,
    rollback: bool,
) -> Result<Operand, Outcome> {
    unsafe {
        // Preserve a preparation Error when authored policy pipelines the calls.
        if s.exact(q, token, b"ErrorVt")? {
            return Ok(Operand::object(token));
        }
        if token == s.named(b"nil")? {
            return fail(
                s,
                q,
                if rollback {
                    b"candidateRollbackPreparationFailed"
                } else {
                    b"candidatePreparationFailed"
                },
                b"candidate preparation returned nil",
            );
        }
        let p = take_prepared(s, q, token)?;
        if p.rollback != rollback {
            return fail(
                s,
                q,
                b"invalidPreparedToken",
                b"prepared token has a different action",
            );
        }
        if !s.candidates.quiescent.get()
            && s.send(s.named(b"Execution")?, b"retirementReady", &[])?
                .as_word()
                != Some(1)
        {
            return fail(
                s,
                q,
                b"generationNotQuiescent",
                b"generation commit requires a Host safe point",
            );
        }
        if !preparation::current_snapshot(s, q, &p)? {
            return fail(s, q, b"staleGeneration", b"publication commit is stale");
        }
        let before = s.candidates.ledger.borrow().clone().ok_or_else(invalid)?;
        if !ledger::matches_lineage(s, q, &before)? {
            return fail(s, q, b"staleGeneration", b"candidate source lineage changed");
        }
        let old_sources = s.registry.entries.borrow().clone();
        for &(name,record) in &old_sources {q.keep(name)?;q.keep(record)?;}
        let saved = s
            .candidates
            .entries
            .borrow()
            .iter()
            .find(|c| c.unit == p.unit)
            .and_then(|c| c.activation.clone());
        let (methods, global_writes, mut after, new_sources) = if rollback {
            let saved = saved.as_ref().ok_or_else(invalid)?;
            let globals = saved
                .globals
                .iter()
                .map(|f| GlobalWrite {
                    name: f.name,
                    before: f.after,
                    after: f.before,
                })
                .collect::<Vec<_>>();
            (
                saved
                    .bindings_before
                    .iter()
                    .map(|b| b.method)
                    .collect::<Vec<_>>(),
                globals,
                saved.before.clone(),
                saved.sources_before.clone(),
            )
        } else {
            let c = construct(s, q, p.unit)?;
            // Construction may install private adapters. Revalidate the observed
            // bindings and fields below; no global epoch exemption is needed.
            let mut globals = vec![];
            for g in &c.globals {
                let prior = p
                    .globals
                    .iter()
                    .find(|old| old.name == g.name)
                    .ok_or_else(invalid)?;
                // Replacing a target Global would invalidate its dictionary
                // identity; that operation is not a dispatch-only transaction.
                if p.bindings.iter().any(|b| b.name == g.name)
                {
                    return fail(
                        s,
                        q,
                        b"unstableMethodTarget",
                        b"projected Global replaces a selected method target",
                    );
                }
                globals.push(GlobalWrite {
                    name: g.name,
                    before: prior.value,
                    after: g.value,
                });
            }
            let mut next = before.clone();
            next.generation = p.plan[5];
            next.source_map = field(s, q, next.generation, b"sourceMap")?;
            next.digest = s.bytes(field(s, q, next.generation, b"baseDigest")?)?;
            next.selection = s.bytes(field(s, q, next.generation, b"generationSelection")?)?;
            next.units = seq(s, q, next.source_map)?
                .into_iter()
                .map(|row| seq(s, q, row).map(|v| v[2]))
                .collect::<Result<Vec<_>, _>>()?;
            let next_sources = sources(s, q, next.source_map)?;
            (c.methods, globals, next, next_sources)
        };
        // Source ownership changes only at the final native commit below. During
        // activation/rollback, validate against the still-current registry stamp.
        after.source_revision=s.registry.revision();
        if methods.len() != p.bindings.len() {
            return Err(invalid());
        }
        let fields = publication(s, q, &p)?;
        // Reserve binding-owner capacity for forward writes, compensation and rollback.
        // Ordinary lifecycle fields need no database capacity bank.
        let multiplier = if rollback { 2 } else { 4 };
        let binding_required = global_writes.len().checked_mul(multiplier).ok_or_else(invalid)?;
        // Binding credits belong to this explicit activation and its inverse.
        let binding_reservation = if rollback {
            saved.as_ref().ok_or_else(invalid)?.binding_reservation
        } else if binding_required == 0 {
            s.named(b"nil")?
        } else {
            call(s, q, s.named(b"Object")?, b"clone", &[])?
        };
        let reserved = reserve_bindings(s, q, binding_reservation, binding_required);
        if let Err(error) = reserved {
            if let Some(value) = error.value.as_object() { q.keep(value)?; }
            if !rollback { release_bindings(s, q, binding_reservation, binding_required)?; }
            return Err(error);
        }
        if !preparation::current_snapshot(s, q, &p)?
            || *s.registry.entries.borrow() != old_sources
        {
            if !rollback {
                release_bindings(s, q, binding_reservation, binding_required)?;
            }
            return fail(
                s,
                q,
                b"staleGeneration",
                b"candidate construction or reservation changed its base",
            );
        }
        // The postcondition is the same dependency scope with our intended
        // writes substituted. Never learn a promise from callback-mutated state.
        let mut published = p.clone();
        published.plan[4] = p.plan[5];
        for f in &fields {
            let (_, _, value) = published.fields.iter_mut()
                .find(|(owner, key, _)| *owner == f.entity && *key == f.attribute)
                .ok_or_else(invalid)?;
            *value = f.after;
        }
        published.globals = global_writes.iter()
            .map(|f| Global { name: f.name, value: f.after }).collect();
        let mut attempted = vec![];
        let mut post = vec![];
        let applied = (|| {
            for f in &global_writes {
                attempted.push(Write::Global(f.clone()));
                restore_global(s, q, binding_reservation, f, false)?;
            }
            let indices: Vec<_> = if rollback {
                (0..p.bindings.len()).rev().collect()
            } else {
                (0..p.bindings.len()).collect()
            };
            post = p.bindings.clone();
            for i in indices {
                let b = &p.bindings[i];
                if preparation::target(s, q, b.name)? != Some((b.target, b.behavior))
                    || preparation::direct(s, b.behavior, b.selector)? != b.entry
                {
                    return Err(failure(
                        s,
                        q,
                        b"staleGeneration",
                        b"candidate binding changed during publication",
                    ));
                }
                attempted.push(Write::Binding {
                    before: b.clone(),
                    method: methods[i],
                });
                put_method(s, q, b, methods[i])?;
                let entry = preparation::direct(s, b.behavior, b.selector)?;
                if !is_method(s, entry, methods[i])? {
                    return Err(failure(
                        s,
                        q,
                        b"candidateCommitFailed",
                        b"dictionary owner did not publish the selected Method",
                    ));
                }
                post[i].entry = entry;
                post[i].method = methods[i];
            }
            if !rollback {
                for b in &post {
                    after
                        .bindings
                        .retain(|v| v.binding.name != b.name || v.binding.selector != b.selector);
                    after.bindings.push(ledger::ManagedBinding {
                        unit: p.unit,
                        binding: b.clone(),
                    });
                }
                for f in &global_writes {
                    after.globals.retain(|v| v.global.name != f.name);
                    after.globals.push(ledger::ManagedGlobal {
                        unit: p.unit,
                        global: Global {
                            name: f.name,
                            value: f.after,
                        },
                    });
                }
            }
            for f in &fields {
                attempted.push(Write::Field(f.clone()));
                restore_field(s, q, f, false)?;
            }
            attempted.push(Write::Current { before: p.plan[4], after: p.plan[5] });
            set_current(s, q, p.plan[4], p.plan[5])?;
            published.bindings = post.clone();
            if !preparation::current_snapshot(s, q, &published)?
                || *s.registry.entries.borrow() != old_sources
            {
                return Err(failure(
                    s,
                    q,
                    b"staleGeneration",
                    b"publication does not match its selected postcondition",
                ));
            }
            if rollback {
                let cleanup = (|| {
                    release_bindings(s, q, binding_reservation, binding_required)?;
                    if !preparation::current_snapshot(s, q, &published)?
                        || *s.registry.entries.borrow() != old_sources
                    {
                        return Err(failure(
                            s,
                            q,
                            b"staleGeneration",
                            b"rollback cleanup changed its live generation",
                        ));
                    }
                    Ok::<_, Outcome>(())
                })();
                if let Err(error) = cleanup {
                    if let Some(value) = error.value.as_object() {
                        q.keep(value)?;
                    }
                    // A replaced release method may report failure after
                    // consuming the bank. Reacquire inverse capacity before
                    // compensating this already-published rollback.
                    reserve_bindings(s, q, binding_reservation, binding_required)?;
                    return Err(error);
                }
            }
            Ok::<_, Outcome>(())
        })();
        if let Err(error) = applied {
            if let Some(value) = error.value.as_object() {
                q.keep(value)?;
            }
            let restored = compensate(s, q, binding_reservation, &attempted);
            if !rollback {
                release_bindings(s, q, binding_reservation, binding_required)?;
            }
            if !restored || !preparation::current_snapshot(s, q, &p)? {
                return fail(
                    s,
                    q,
                    b"candidateCompensationFailed",
                    b"publication failed and an intervening edit prevented exact compensation",
                );
            }
            return Err(error);
        }
        // Everything that can call out has completed. Install native ownership
        // together; no borrowed registry crosses an outgoing message.
        s.registry.replace_entries(new_sources);
        after.source_revision=s.registry.revision();
        if !rollback {
            s.candidates
                .entries
                .borrow_mut()
                .iter_mut()
                .find(|c| c.unit == p.unit)
                .ok_or_else(invalid)?
                .activation = Some(Activation {
                before,
                after: after.clone(),
                bindings_before: p.bindings,
                bindings_after: post,
                globals: global_writes,
                binding_reservation,
                sources_before: old_sources,
            });
        } else {
            s.candidates
                .entries
                .borrow_mut()
                .iter_mut()
                .find(|c| c.unit == p.unit)
                .ok_or_else(invalid)?
                .activation = None;
        }
        *s.candidates.ledger.borrow_mut() = Some(after);
        Ok(Operand::object(p.unit))
    }
}
pub(super) unsafe fn prepare_rollback(
    s: &State,
    q: &Scope<'_>,
    unit: Oop,
    raw_plan: Oop,
) -> Result<Operand, Outcome> {
    unsafe {
        let saved = s
            .candidates
            .entries
            .borrow()
            .iter()
            .find(|c| c.unit == unit)
            .and_then(|c| c.activation.clone());
        let Some(saved) = saved else {
            return fail(
                s,
                q,
                b"rollbackUnavailable",
                b"candidate has no retained inverse",
            );
        };
        if s.candidates
            .prepared
            .borrow()
            .iter()
            .any(|p| p.unit == unit)
        {
            return fail(
                s,
                q,
                b"candidatePreparationFailed",
                b"candidate is already prepared",
            );
        }
        if !s.exact(q, raw_plan, b"VectorVt")? {
            return Err(invalid());
        }
        let plan: [Oop; 6] = seq(s, q, raw_plan)?.try_into().map_err(|_| invalid())?;
        if plan[0] != symbol(s, q, b"rollback")?
            || plan[1] != unit
            || plan[2] != field(s, q, unit, b"unitName")?
            || plan[4] != saved.after.generation
            || plan[5] != saved.before.generation
            || field(s, q, unit, b"state")? != symbol(s, q, b"active")?
            || field(s, q, plan[4], b"deltaUnit")? != unit
            || field(s, q, plan[4], b"predecessor")? != plan[5]
        {
            return fail(
                s,
                q,
                b"staleGeneration",
                b"rollback plan does not name the current delta and its predecessor",
            );
        }
        let mut prior = s.named(b"nil")?;
        for row in seq(s, q, saved.before.source_map)? {
            let row = seq(s, q, row)?;
            if row[0] == plan[2] {
                prior = row[2];
            }
        }
        if prior != plan[3] {
            return fail(
                s,
                q,
                b"staleGeneration",
                b"rollback plan has a stale predecessor",
            );
        }
        let mut p = Prepared {
            rollback: true,
            token: 0,
            unit,
            plan,
            source_revision:s.registry.revision(),
            fields: preparation::plan_fields(s, q, &plan)?,
            bindings: saved.bindings_after.clone(),
            globals: saved
                .globals
                .iter()
                .map(|f| Global {
                    name: f.name,
                    value: f.after,
                })
                .collect(),
        };
        let token = s
            .candidates
            .next_token
            .get()
            .checked_add(1)
            .filter(|n| *n <= isize::MAX as usize)
            .ok_or_else(invalid)?;
        s.candidates.next_token.set(token);
        let value = call(
            s,
            q,
            s.named(b"Integer")?,
            b"fromWord:",
            &[Operand::word(token)],
        )?;
        if preparation::number(s, value)? != token || !preparation::current_snapshot(s, q, &p)? {
            return fail(
                s,
                q,
                b"staleGeneration",
                b"rollback preparation changed its base",
            );
        }
        p.token = token;
        s.candidates.prepared.borrow_mut().push(p);
        Ok(Operand::object(value))
    }
}
