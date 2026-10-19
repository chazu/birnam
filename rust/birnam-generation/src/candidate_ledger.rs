//! Transient observations of the durable current generation. The dictionary is
//! canonical; these snapshots detect drift and retain exact inverse Methods.
use super::*;
use observation::{Binding, Global};

// Inspect only Block values through their owner messages. Behavior ancestry
// includes compiler-created private Block behaviors; it cannot admit a scalar
// merely because that value happens to answer a similarly named message.
unsafe fn projected_program(
    s: &State,
    q: &Scope<'_>,
    value: Oop,
) -> Result<Option<Vec<u8>>, Outcome> {
    unsafe {
        let block = s.named(b"BlockVt")?;
        let mut behavior = s
            .send(s.id.receiver, b"behaviorOf:", &[Operand::object(value)])?
            .as_object();
        let mut visited = std::collections::HashSet::new();
        loop {
            let Some(current) = behavior else {
                return Ok(None);
            };
            if current == block {
                break;
            }
            if !visited.insert(current) {
                return Ok(None);
            }
            q.keep(current)?;
            behavior = s
                .send(s.id.receiver, b"parentOf:", &[Operand::object(current)])?
                .as_object();
        }
        let nil = s.named(b"nil")?;
        let code = call(s, q, value, b"codeObject", &[])?;
        if code == nil {
            return Ok(None);
        }
        let captures = call(s, q, value, b"captureCount", &[])?;
        if observation::number(s, captures)? != 0 {
            return Ok(None);
        }
        let bytes = call(
            s,
            q,
            s.named(b"CodeObject")?,
            b"canonicalBytesOf:",
            &[Operand::object(code)],
        )?;
        Ok(Some(s.bytes(bytes)?))
    }
}

#[derive(Clone)]
pub(super) struct ManagedBinding {
    pub unit: Oop,
    pub binding: Binding,
}
#[derive(Clone)]
pub(super) struct ManagedGlobal {
    pub unit: Oop,
    pub global: Global,
}
#[derive(Clone)]
pub(super) struct Ledger {
    pub generation: Oop,
    pub source_map: Oop,
    pub digest: Vec<u8>,
    pub selection: Vec<u8>,
    pub bindings: Vec<ManagedBinding>,
    pub globals: Vec<ManagedGlobal>,
    pub units: Vec<Oop>,
    pub source_revision: usize,
    pub source_owner: Oop,
}
impl Ledger {
    pub(super) fn roots(&self) -> Vec<Oop> {
        let mut roots = vec![self.generation, self.source_map, self.source_owner];
        roots.extend(&self.units);
        for b in &self.bindings {
            roots.extend([
                b.unit,
                b.binding.name,
                b.binding.target,
                b.binding.behavior,
                b.binding.selector,
            ]);
            roots.extend(b.binding.method);
        }
        for g in &self.globals {
            roots.extend([g.unit, g.global.name]);
            roots.extend(g.global.value);
        }
        roots
    }
}
// Compilation still uses the recorded source/selection lineage. This is not a
// claim that every live binding equals the source revision's certificate.
pub(super) unsafe fn matches_lineage(s: &State, q: &Scope<'_>, ledger: &Ledger) -> Result<bool, Outcome> {
    unsafe {
        Ok(sources::owner(s, q)? == ledger.source_owner && sources::revision(s, q, ledger.source_owner)? == ledger.source_revision
            && current(s, q)? == ledger.generation
            && field(s, q, ledger.generation, b"sourceMap")? == ledger.source_map
            && s.bytes(field(s, q, ledger.generation, b"baseDigest")?)? == ledger.digest
            && s.bytes(field(s, q, ledger.generation, b"generationSelection")?)? == ledger.selection)
    }
}
pub(super) unsafe fn matches(
    s: &State,
    q: &Scope<'_>,
    ledger: &Ledger,
) -> Result<Option<bool>, Outcome> {
    unsafe {
        if !matches_lineage(s, q, ledger)?
        {
            return Ok(Some(false));
        }
        // Observe binding owners coherently across callbacks. Explicit facts are
        // independent of this lifecycle record.
        let global_basis = observation::global_basis(s, q)?;
        let epoch = observation::epoch(s)?;
        for b in &ledger.bindings {
            let b = &b.binding;
            if observation::target(s, q, b.name)? != Some((b.target, b.behavior))
                || observation::direct(s, b.behavior, b.selector)? != b.entry
            {
                return Ok(Some(false));
            }
        }
        for g in &ledger.globals {
            if observation::global(s, q, g.global.name)? != g.global.value {
                return Ok(Some(false));
            }
        }
        let stable = sources::owner(s, q)? == ledger.source_owner && sources::revision(s, q, ledger.source_owner)? == ledger.source_revision
            && current(s, q)? == ledger.generation
            && observation::global_basis(s, q)? == global_basis
            && observation::epoch(s)? == epoch;
        Ok(stable.then_some(true))
    }
}
/// Explicit validation reports current agreement. It neither edits records nor
/// irreversibly revokes an inverse because a discrepancy was observed.
pub(super) unsafe fn reconcile(s: &State, q: &Scope<'_>) -> Result<bool, Outcome> {
    unsafe {
        let Some(ledger) = s.candidates.ledger.borrow().clone() else { return Ok(false); };
        Ok(matches(s, q, &ledger)? == Some(true))
    }
}
/// A saved managed source map must still describe the dispatch it authorizes.
/// Drift never becomes a second authority merely because its code is portable.
pub(crate) unsafe fn validate_image(s: &State, q: &Scope<'_>) -> Result<Operand, Outcome> {
    unsafe { image_admission(s, q, false) }
}
/// Check imageability without reconciling or changing lifecycle descriptions.
pub(crate) unsafe fn prepare_image_snapshot(s: &State, q: &Scope<'_>) -> Result<Operand, Outcome> {
    unsafe { image_admission(s, q, true) }
}
unsafe fn image_admission(s: &State, q: &Scope<'_>, snapshot: bool) -> Result<Operand, Outcome> {
    unsafe {
        if s.candidates.busy.get()
            || s.candidates.image_application.get()
        {
            return fail(
                s,
                q,
                b"candidateBusy",
                b"image save cannot interrupt generation publication",
            );
        }
        s.candidates.busy.set(true);
        struct Busy<'a>(&'a Cell<bool>);
        impl Drop for Busy<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        let _busy = Busy(&s.candidates.busy);
        if snapshot {
            // Authored source declarations identify entries that cannot silently
            // disappear from the portable inventory. Inspect current entries for
            // imageability only, without requiring equality to source or a ledger.
            // This also works after loading an image without adopting a baseline.
            let source_owner = sources::owner(s, q)?;
            let revision = sources::revision(s, q, source_owner)?;
            let entries = sources::entries(s, q, source_owner)?;
            for &(name, record) in &entries { q.keep(name)?; q.keep(record)?; }
            let mut keys = std::collections::BTreeSet::new();
            for (_, record) in entries {
                let Some((_, metadata)) = s.revision(q, record)? else { return Err(invalid()); };
                for operation in metadata.operations {
                    keys.insert((operation.target, operation.selector));
                }
            }
            let epoch = observation::epoch(s)?;
            let globals = observation::global_basis(s, q)?;
            let nil = s.named(b"nil")?;
            for (name, selector) in keys {
                let name = symbol(s, q, &name)?;
                let selector = symbol(s, q, &selector)?;
                if let Some((_, behavior)) = observation::target(s, q, name)? {
                    if let Some(entry) = observation::direct(s, behavior, selector)? {
                        let implementation = s.send(s.named(b"Execution")?, b"implementationOfEntry:", &[Operand::entry(entry)])?;
                        if implementation.as_object().is_none_or(|v| v == nil) {
                            return fail(s, q, b"nonPortableGeneration", b"authored binding has no imageable implementation");
                        }
                    }
                }
            }
            if sources::owner(s, q)? != source_owner || sources::revision(s, q, source_owner)? != revision || observation::epoch(s)? != epoch
                || observation::global_basis(s, q)? != globals {
                return fail(s, q, b"staleGeneration", b"bindings changed during snapshot admission");
            }
            return Ok(Operand::object(s.named(b"true")?));
        }
        if s.candidates.ledger.borrow().is_none() {
            if current(s, q)? == s.named(b"nil")? {
                return Ok(Operand::object(s.named(b"true")?));
            }
            return fail(s, q, b"generationNotAdopted", b"generation has no binding ledger");
        }
        if reconcile(s, q)? {
            Ok(Operand::object(s.named(b"true")?))
        } else {
            fail(s, q, b"staleGeneration", b"managed generation has drifted")
        }
    }
}

pub(super) unsafe fn reconcile_candidate(
    s: &State,
    q: &Scope<'_>,
    unit: Oop,
) -> Result<Operand, Outcome> {
    unsafe {
        let known = s
            .candidates
            .ledger
            .borrow()
            .as_ref()
            .is_some_and(|l| l.generation == unit || l.units.contains(&unit));
        if !known {
            return fail(
                s,
                q,
                b"generationNotAdopted",
                b"unit has no current binding ledger",
            );
        }
        if reconcile(s, q)? {
            Ok(Operand::object(unit))
        } else {
            fail(s, q, b"staleGeneration", b"managed generation has drifted")
        }
    }
}
pub(super) unsafe fn adopt(
    s: &State,
    q: &Scope<'_>,
    generation: Oop,
) -> Result<Operand, Outcome> {
    unsafe {
        let nil = s.named(b"nil")?;
        if current(s, q)? != generation
            || generation == nil
            || field(s, q, generation, b"predecessor")? != nil
            || field(s, q, generation, b"deltaUnit")? != nil
        {
            return fail(
                s,
                q,
                b"invalidLifecycleTransition",
                b"adoption requires the durable predecessor-free generation",
            );
        }
        let global_basis = observation::global_basis(s, q)?;
        let source_map = field(s, q, generation, b"sourceMap")?;
        let modules = modules(s, q, source_map)?;
        let rows = seq(s, q, source_map)?;
        let mut components = std::collections::BTreeMap::new();
        for row in rows {
            let row = seq(s, q, row)?;
            let [name, _, unit] = row.as_slice() else {
                return Err(invalid());
            };
            components.insert(s.symbol_bytes(q, *name)?.ok_or_else(invalid)?, *unit);
        }
        let source_owner = sources::owner(s, q)?;
        let source_revision = sources::revision(s, q, source_owner)?;
        let registered = sources::entries(s, q, source_owner)?;
        for &(name,record) in &registered {q.keep(name)?;q.keep(record)?;}
        if registered.len() != modules.len() {
            return fail(
                s,
                q,
                b"invalidGeneration",
                b"generation source map differs from registered sources",
            );
        }
        for module in &modules {
            let mut found = false;
            for &(name, source) in &registered {
                if s.symbol_bytes(q, name)?.as_deref() == Some(module.name.as_slice())
                    && s.source_data(q,source)? == module.source
                {
                    found = true;
                    break;
                }
            }
            if !found {
                return fail(
                    s,
                    q,
                    b"invalidGeneration",
                    b"generation source map differs from registered sources",
                );
            }
        }
        let digest = s.bytes(field(s, q, generation, b"baseDigest")?)?;
        let selection = s.bytes(field(s, q, generation, b"generationSelection")?)?;
        let descriptor_hashes = backend::hashes(
            s,
            q,
            modules
                .iter()
                .flat_map(|m| m.metadata.operations.iter().map(|o| o.selector.as_slice())),
        )?;
        let Some(authority) =
            generation::dispatch_generation_authority_with(&modules, &selection, |m, i, _| {
                descriptor_hashes
                    .get(&m.metadata.operations.get(i)?.selector)
                    .copied()
                    .flatten()
            })
        else {
            return fail(
                s,
                q,
                b"invalidGeneration",
                b"generation selection is not canonical",
            );
        };
        if digest != generation::raw_digest_spelling(authority) {
            return fail(s, q, b"invalidGeneration", b"generation digest mismatches");
        }
        let mut ledger = Ledger {
            generation,
            source_map,
            digest,
            selection,
            bindings: vec![],
            globals: vec![],
            units: components.values().copied().collect(),
            source_revision,
            source_owner,
        };
        // Authority parsing may collect unrelated private Block behaviors.
        // Observe dictionary stability around the live binding snapshot, after
        // that inert work. The final ledger compares the owned lifecycle fields.
        let epoch = observation::epoch(s)?;
        // Multiple source names can address one dictionary. Validate the final
        // writer in dependency order, while retaining every named observation.
        let mut programs = Vec::<(Oop, Oop, Option<&[u8]>, Option<Vec<u8>>)>::new();
        let mut global_programs = Vec::<(Oop, &[u8], Option<Oop>)>::new();
        for index in generation::generation_module_order(&modules).ok_or_else(invalid)? {
            let module = &modules[index];
            let unit = components[&module.name];
            for operation in &module.metadata.operations {
                let name = symbol(s, q, &operation.target)?;
                let Some((target, behavior)) = observation::target(s, q, name)? else {
                    return fail(
                        s,
                        q,
                        b"sourceBindingMismatch",
                        format!(
                            "managed source target is unavailable: {} in {}",
                            String::from_utf8_lossy(&operation.target),
                            String::from_utf8_lossy(&module.name)
                        )
                        .as_bytes(),
                    );
                };
                let selector = symbol(s, q, &operation.selector)?;
                let entry = observation::direct(s, behavior, selector)?;
                let mut actual_program = None;
                let method = if let Some(entry) = entry {
                    // Native entries lacking a portable implementation cannot be
                    // adopted into an image-rebuildable rollback baseline.
                    let implementation = s.send(
                        s.named(b"Execution")?,
                        b"implementationOfEntry:",
                        &[Operand::entry(entry)],
                    )?;
                    if implementation.kind() != native::OBJECT {
                        return fail(
                            s,
                            q,
                            b"nonPortableGeneration",
                            b"managed binding has no portable implementation",
                        );
                    }
                    let implementation = q.keep(object(implementation)?)?;
                    let code = call(s, q, implementation, b"codeObject", &[])?;
                    if code == nil {
                        return fail(
                            s,
                            q,
                            b"nonPortableGeneration",
                            b"managed binding has no CodeObject",
                        );
                    }
                    let bytes = call(
                        s,
                        q,
                        s.named(b"CodeObject")?,
                        b"canonicalBytesOf:",
                        &[Operand::object(code)],
                    )?;
                    actual_program = Some(s.bytes(bytes)?);
                    Some(call(
                        s,
                        q,
                        s.named(b"Execution")?,
                        b"methodOfEntry:",
                        &[Operand::entry(entry)],
                    )?)
                } else {
                    None
                };
                programs.retain(|(b, sel, _, _)| *b != behavior || *sel != selector);
                programs.push((
                    behavior,
                    selector,
                    operation.program.as_deref(),
                    actual_program,
                ));
                ledger
                    .bindings
                    .retain(|b| b.binding.name != name || b.binding.selector != selector);
                ledger.bindings.push(ManagedBinding {
                    unit,
                    binding: Binding {
                        name,
                        target,
                        behavior,
                        selector,
                        entry,
                        method,
                    },
                });
            }
            for projected in &module.metadata.projected_globals {
                let name = symbol(s, q, &projected.name)?;
                let value = observation::global(s, q, name)?;
                global_programs.retain(|(n, _, _)| *n != name);
                global_programs.push((name, &projected.program, value));
                ledger.globals.retain(|g| g.global.name != name);
                ledger.globals.push(ManagedGlobal {
                    unit,
                    global: Global { name, value },
                });
            }
        }
        for (name, expected, value) in global_programs {
            let actual = match value {
                Some(value) => projected_program(s, q, value)?,
                None => None,
            };
            if !actual
                .as_deref()
                .is_some_and(|actual| birnam_codegen::portable::matches_source(expected, actual))
            {
                let mut message = b"managed global differs from recorded source: ".to_vec();
                message.extend(s.symbol_bytes(q, name)?.ok_or_else(invalid)?);
                return fail(s, q, b"sourceBindingMismatch", &message);
            }
        }
        for (behavior, selector, expected, actual) in programs {
            let agrees = match (expected, actual) {
                (None, None) => true,
                (Some(source), Some(actual)) => {
                    birnam_codegen::portable::matches_source(source, &actual)
                }
                _ => false,
            };
            if !agrees {
                let name = ledger
                    .bindings
                    .iter()
                    .find(|b| b.binding.behavior == behavior && b.binding.selector == selector)
                    .ok_or_else(invalid)?
                    .binding
                    .name;
                let mut message = b"managed binding differs from recorded source: ".to_vec();
                message.extend(s.symbol_bytes(q, name)?.ok_or_else(invalid)?);
                message.push(b' ');
                message.extend(s.symbol_bytes(q, selector)?.ok_or_else(invalid)?);
                return fail(s, q, b"sourceBindingMismatch", &message);
            }
        }
        let changed = if observation::global_basis(s, q)? != global_basis {
            Some(b"Globals basis changed during generation adoption".as_slice())
        } else if observation::epoch(s)? != epoch {
            Some(b"behavior epoch changed during generation adoption".as_slice())
        } else if sources::owner(s, q)? != source_owner || sources::revision(s, q, source_owner)? != source_revision || sources::entries(s, q, source_owner)? != registered {
            Some(b"source registry changed during generation adoption".as_slice())
        } else if matches(s, q, &ledger)? != Some(true) {
            Some(b"live bindings changed during generation adoption".as_slice())
        } else {
            None
        };
        if let Some(message) = changed {
            return fail(s, q, b"staleGeneration", message);
        }
        *s.candidates.ledger.borrow_mut() = Some(ledger);
        Ok(Operand::object(generation))
    }
}

// Certification owns only its observation roots. Candidate inverses and physical
// code lifetimes have independent owners. Resolve the result before releasing;
// no callback or collection is required to drop this optional observation.
pub(super) unsafe fn release(s: &State, q: &Scope<'_>) -> Result<Operand, Outcome> {
    unsafe {
        let yes = q.keep(s.named(b"true")?)?;
        s.candidates.ledger.take();
        Ok(Operand::object(yes))
    }
}
