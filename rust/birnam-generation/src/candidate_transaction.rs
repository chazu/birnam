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
    prior_unit: Oop,
    source_before: Oop,
    source_after: Oop,
    bindings_before: Vec<Binding>,
    bindings_after: Vec<Binding>,
    globals: Vec<GlobalWrite>,
    binding_reservation: Oop,
}
impl Activation {
    pub(super) fn roots(&self) -> Vec<Oop> {
        let mut values = vec![self.prior_unit, self.source_before, self.source_after, self.binding_reservation];
        for b in self.bindings_before.iter().chain(&self.bindings_after) {
            values.extend([b.name, b.target, b.behavior, b.selector]);
            values.extend(b.method);
        }
        for f in &self.globals {
            values.push(f.name);
            values.extend(f.before);
            values.extend(f.after);
        }
        values
    }
}
#[derive(Clone)]
enum Write {
    Field(FieldWrite),
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
pub(super) unsafe fn release_inverse(s: &State, q: &Scope<'_>, unit: Oop) -> Result<(), Outcome> {
    unsafe {
        let inverse = s.candidates.entries.borrow().iter().find(|c| c.unit == unit).and_then(|c| c.activation.clone());
        if let Some(inverse) = inverse {
            for value in inverse.roots() { q.keep(value)?; }
            release_bindings(s, q, inverse.binding_reservation, inverse.globals.len())?;
        }
        Ok(())
    }
}
pub(super) unsafe fn discard_fields(s: &State, q: &Scope<'_>, unit: Oop) -> Result<(), Outcome> {
    unsafe {
        set_field(s, q, unit, b"state", symbol(s, q, b"discarded")?)?;
        set_field(s, q, unit, b"rollbackAvailable", s.named(b"false")?)
    }
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
fn retained_predecessor(s: &State, prior: Oop, nil: Oop) -> Oop {
    if s.candidates.entries.borrow().iter().any(|c| c.unit == prior) { prior } else { nil }
}

unsafe fn publication(s: &State, q: &Scope<'_>, p: &Prepared) -> Result<Vec<FieldWrite>, Outcome> {
    unsafe {
        let [_, unit, _, prior] = p.plan;
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
        let mut p = take_prepared(s, q, token)?;
        // Releasing a predecessor does not release the saved code/source inverse.
        // Its descriptive fields cease to be part of this operation, including
        // when discard happens after preparation. Private-state exclusion keeps
        // handle ownership stable through the remaining callback-capable commit.
        let prior = p.plan[3];
        let nil = s.named(b"nil")?;
        if retained_predecessor(s, prior, nil) == nil {
            p.plan[3] = nil;
            p.fields.retain(|(owner, _, _)| *owner != prior);
        }
        if p.rollback != rollback {
            return fail(
                s,
                q,
                b"invalidPreparedToken",
                b"prepared token has a different action",
            );
        }
        if !preparation::current_snapshot(s, q, &p)? {
            return fail(s, q, b"staleGeneration", b"publication commit is stale");
        }
        let saved = s
            .candidates
            .entries
            .borrow()
            .iter()
            .find(|c| c.unit == p.unit)
            .and_then(|c| c.activation.clone());
        let (methods, global_writes, new_source) = if rollback {
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
                saved.source_before,
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
            let source = s.candidates.entries.borrow().iter().find(|c| c.unit == p.unit).ok_or_else(invalid)?.source;
            (c.methods, globals, source)
        };
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
            for f in &fields {
                attempted.push(Write::Field(f.clone()));
                restore_field(s, q, f, false)?;
            }
            published.bindings = post.clone();
            if !preparation::current_snapshot(s, q, &published)? {
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
                    if !preparation::current_snapshot(s, q, &published)? {
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
        if !s.registry.replace_record(p.plan[2], p.source, new_source) { return Err(invalid()); }
        if !rollback {
            s.candidates
                .entries
                .borrow_mut()
                .iter_mut()
                .find(|c| c.unit == p.unit)
                .ok_or_else(invalid)?
                .activation = Some(Activation {
                prior_unit: p.plan[3],
                source_before: p.source,
                source_after: new_source,
                bindings_before: p.bindings,
                bindings_after: post,
                globals: global_writes,
                binding_reservation,
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
        let request: [Oop; 2] = seq(s, q, raw_plan)?.try_into().map_err(|_| invalid())?;
        let prior = retained_predecessor(s, saved.prior_unit, s.named(b"nil")?);
        let plan = [request[0], request[1], field(s, q, unit, b"unitName")?, prior];
        if plan[0] != symbol(s, q, b"rollback")?
            || plan[1] != unit
            || plan[2] != field(s, q, unit, b"unitName")?
            || field(s, q, unit, b"state")? != symbol(s, q, b"active")?
        {
            return fail(
                s,
                q,
                b"staleGeneration",
                b"rollback plan does not name the current delta and its predecessor",
            );
        }
        let mut p = Prepared {
            rollback: true,
            token: 0,
            unit,
            plan,
            source: saved.source_after,
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
