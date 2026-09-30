//! Candidate artifact admission and inert native mapping. Publication lives in
//! a later explicit transaction; no candidate initializer runs in this module.
use super::*;
use birnam_codegen::{
    artifact_stackless_selection::{self as selection, Disposition},
    candidate as code, candidate_envelope as envelope, generation, source_revision,
};
use core::cell::Cell;
#[path = "candidate_ledger.rs"]
mod ledger;
#[path = "candidate_preparation.rs"]
mod preparation;
#[path = "candidate_recovery.rs"]
mod recovery;
#[path = "candidate_retirement.rs"]
mod retirement;
#[path = "candidate_transaction.rs"]
mod transaction;
pub(super) use recovery::{begin_recovery, cancel_recovery, recover_image};
#[derive(Default)]
pub(super) struct Candidates {
    pub busy: Cell<bool>,
    quiescent: Cell<bool>,
    recovering_policy: Cell<bool>,
    pub entries: RefCell<Vec<Candidate>>,
    prepared: RefCell<Vec<preparation::Prepared>>,
    next_token: Cell<usize>,
    ledger: RefCell<Option<ledger::Ledger>>,
    recovery: RefCell<Option<recovery::Recovery>>,
    retained: RefCell<Vec<Vec<usize>>>,
}
pub(super) struct Candidate {
    pub unit: Oop,
    pub source: Oop,
    pub name: Oop,
    pub path: Oop,
    pub expected: [u8; 64],
    pub selected: Vec<u8>,
    pub plan: code::Plan,
    pub objects: Vec<Vec<u8>>,
    envelope: Vec<u8>,
    pub mapped: Option<Mapped>,
    construction_started: bool,
    constructed: Option<transaction::Constructed>,
    activation: Option<transaction::Activation>,
}
pub(super) struct Mapped {
    pub factory: Oop,
    pub adapters: Vec<Oop>,
    identities: Vec<usize>,
}
impl Candidate {
    fn roots(&self) -> Vec<Oop> {
        let mut values = vec![self.unit, self.source, self.name, self.path];
        if let Some(m) = &self.mapped {
            values.push(m.factory);
            values.extend(&m.adapters);
        }
        if let Some(v) = &self.constructed {
            values.extend(v.roots());
        }
        if let Some(v) = &self.activation {
            values.extend(v.roots());
        }
        values
    }
}
impl Candidates {
    pub fn roots(&self) -> Vec<Oop> {
        self.entries
            .borrow()
            .iter()
            .flat_map(Candidate::roots)
            .chain(self.prepared.borrow().iter().flat_map(|p| p.roots()))
            .chain(self.ledger.borrow().iter().flat_map(|l| l.roots()))
            .chain(self.recovery.borrow().iter().flat_map(|r| r.roots()))
            .collect()
    }
}
#[derive(Clone, Copy)]
pub(super) enum Op {
    Compile,
    CompileSelected,
    Successor,
    Inspect,
    Map,
    Mapped,
    Path,
    Prepare,
    DiscardPrepared,
    Discard,
    RetainedCount,
    RetainedBytes,
    AdoptInitial,
    AdoptComposed,
    RecoveryIdentity,
    Reconcile,
    Commit,
    PrepareRollback,
    CommitRollback,
}
impl Op {
    pub fn count(self) -> usize {
        match self {
            Self::RecoveryIdentity | Self::RetainedCount | Self::RetainedBytes => 0,
            Self::Compile => 3,
            Self::CompileSelected | Self::Successor => 4,
            Self::Inspect | Self::Prepare | Self::PrepareRollback => 2,
            _ => 1,
        }
    }
}
pub(super) unsafe fn call(
    s: &State,
    q: &Scope<'_>,
    r: Oop,
    name: &[u8],
    args: &[Operand],
) -> Result<Oop, Outcome> {
    unsafe { q.keep(object(s.send(r, name, args)?)?) }
}
pub(super) unsafe fn seq(s: &State, q: &Scope<'_>, v: Oop) -> Result<Vec<Oop>, Outcome> {
    unsafe {
        let count = s.index(q, v, b"size")?;
        if count > 1_048_576 {
            return Err(invalid());
        }
        let mut out = Vec::new();
        for i in 0..count {
            let i = call(
                s,
                q,
                s.named(b"Integer")?,
                b"fromWord:",
                &[Operand::word(i)],
            )?;
            out.push(call(s, q, v, b"at:", &[Operand::object(i)])?);
        }
        Ok(out)
    }
}
pub(super) unsafe fn symbol(s: &State, q: &Scope<'_>, name: &[u8]) -> Result<Oop, Outcome> {
    unsafe { q.keep(s.id.intern_name(name)?) }
}
pub(super) unsafe fn fact(
    s: &State,
    q: &Scope<'_>,
    entity: Oop,
    attribute: &[u8],
) -> Result<Oop, Outcome> {
    unsafe {
        let key = symbol(s, q, attribute)?;
        call(
            s,
            q,
            s.named(b"db")?,
            b"currentValue:attr:",
            &[Operand::object(entity), Operand::object(key)],
        )
    }
}
pub(super) unsafe fn assert(
    s: &State,
    q: &Scope<'_>,
    entity: Oop,
    attribute: &[u8],
    value: Oop,
) -> Result<(), Outcome> {
    unsafe {
        let key = symbol(s, q, attribute)?;
        let db = q.keep(s.named(b"db")?)?;
        let result = call(
            s,
            q,
            db,
            b"assert:attr:value:",
            &[
                Operand::object(entity),
                Operand::object(key),
                Operand::object(value),
            ],
        )?;
        if result != db {
            return Err(Outcome::loaded(Operand::object(result)));
        }
        Ok(())
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
        let kind = symbol(s, q, kind)?;
        let text = s.text(q, message)?;
        s.send(
            s.named(b"Error")?,
            b"kind:message:",
            &[Operand::object(kind), Operand::object(text)],
        )
    }
}
pub(super) unsafe fn current(s: &State, q: &Scope<'_>) -> Result<Oop, Outcome> {
    unsafe { fact(s, q, symbol(s, q, b"dispatch")?, b"activeGeneration") }
}
unsafe fn modules(s: &State, q: &Scope<'_>, map: Oop) -> Result<Vec<generation::Module>, Outcome> {
    unsafe {
        let mut modules = Vec::new();
        for row in seq(s, q, map)? {
            let row = seq(s, q, row)?;
            let [name, revision, _] = row.as_slice() else {
                return Err(invalid());
            };
            let fields = seq(s, q, *revision)?;
            let [tag, rname, requires, source, nonmethod, methods, digest] = fields.as_slice()
            else {
                return Err(invalid());
            };
            if *tag != symbol(s, q, b"sourceRevision")? || name != rname {
                return Err(invalid());
            }
            let (bytes, metadata) = s.revision(q, *source)?.ok_or_else(invalid)?;
            if s.symbol_bytes(q, *name)?.as_deref() != Some(metadata.name.as_slice())
                || s.bytes(*nonmethod)? != metadata.non_method_digest
                || s.bytes(*methods)? != metadata.method_digest
                || s.bytes(*digest)? != metadata.revision_digest
            {
                return Err(invalid());
            }
            let actual = seq(s, q, *requires)?
                .into_iter()
                .map(|r| s.symbol_bytes(q, r)?.ok_or_else(invalid))
                .collect::<Result<Vec<_>, _>>()?;
            if actual != metadata.requires {
                return Err(invalid());
            }
            modules.push(generation::Module::new(bytes, metadata).ok_or_else(invalid)?);
        }
        modules.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(modules)
    }
}
unsafe fn select(
    s: &State,
    q: &Scope<'_>,
    metadata: &source_revision::RevisionMetadata,
    value: Option<Oop>,
) -> Result<Vec<u8>, Outcome> {
    unsafe {
        let all = generation::artifact_selection(metadata, Disposition::ResumableRequired)
            .ok_or_else(invalid)?;
        let Some(value) = value else { return Ok(all) };
        if value == symbol(s, q, b"resumableRequired")? {
            return Ok(all);
        }
        let inventory = selection::inspect_artifact_selection(&all).map_err(|_| invalid())?;
        let rows = inventory
            .rows()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| invalid())?;
        let paths = rows
            .iter()
            .map(|r| r.path.iter().collect::<Vec<_>>())
            .collect::<Vec<_>>();
        let specs = rows
            .iter()
            .zip(&paths)
            .map(|(r, path)| selection::ImplementationSpec {
                source_revision: r.source_revision,
                root_kind: r.root_kind,
                root_index: r.root_index,
                path,
                implementation_kind: r.implementation_kind,
            })
            .collect::<Vec<_>>();
        let mut requested = Vec::new();
        let mut request_paths = Vec::new();
        for row in seq(s, q, value)? {
            let row = seq(s, q, row)?;
            let [tag, digest, root, index, path, kind, disposition] = row.as_slice() else {
                return Err(invalid());
            };
            if *tag != symbol(s, q, b"implementationDisposition")? {
                return Err(invalid());
            }
            let index = s
                .send(s.named(b"Integer")?, b"wordOf:", &[Operand::object(*index)])?
                .as_word()
                .and_then(|i| u32::try_from(i).ok())
                .ok_or_else(invalid)?;
            let path = seq(s, q, *path)?
                .into_iter()
                .map(|i| {
                    s.send(s.named(b"Integer")?, b"wordOf:", &[Operand::object(i)])?
                        .as_word()
                        .and_then(|i| u32::try_from(i).ok())
                        .ok_or_else(invalid)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let root = if *root == symbol(s, q, b"methodOperation")? {
                selection::RootKind::MethodOperation
            } else if *root == symbol(s, q, b"nonMethodProgram")? {
                selection::RootKind::NonMethodProgram
            } else {
                return Err(invalid());
            };
            let kind = if *kind == symbol(s, q, b"namedAdapter")? {
                selection::ImplementationKind::NamedAdapter
            } else if *kind == symbol(s, q, b"blockBody")? {
                selection::ImplementationKind::BlockBody
            } else {
                return Err(invalid());
            };
            let disposition = if *disposition == symbol(s, q, b"resumableRequired")? {
                Disposition::ResumableRequired
            } else if *disposition == symbol(s, q, b"synchronous")? {
                Disposition::Synchronous
            } else {
                return Err(invalid());
            };
            requested.push((
                generation::decode_hex_digest(&s.bytes(*digest)?).ok_or_else(invalid)?,
                root,
                index,
                kind,
                disposition,
            ));
            request_paths.push(path);
        }
        let requests = requested
            .iter()
            .zip(&request_paths)
            .map(
                |((revision, root, index, kind, disposition), path)| selection::SelectionRequest {
                    implementation: selection::ImplementationSpec {
                        source_revision: *revision,
                        root_kind: *root,
                        root_index: *index,
                        path,
                        implementation_kind: *kind,
                    },
                    disposition: *disposition,
                },
            )
            .collect::<Vec<_>>();
        selection::materialize_artifact_selection(
            &[generation::decode_hex_digest(&metadata.revision_digest).ok_or_else(invalid)?],
            &specs,
            &requests,
        )
        .map_err(|_| invalid())
    }
}
unsafe fn read(s: &State, q: &Scope<'_>, path: Oop) -> Result<Oop, Outcome> {
    unsafe {
        call(
            s,
            q,
            s.named(b"System")?,
            b"readFile:",
            &[Operand::object(path)],
        )
    }
}
unsafe fn write(s: &State, q: &Scope<'_>, path: Oop, bytes: &[u8]) -> Result<(), Outcome> {
    unsafe {
        let text = s.text(q, bytes)?;
        let result = call(
            s,
            q,
            s.named(b"System")?,
            b"writeFile:contents:",
            &[Operand::object(path), Operand::object(text)],
        )?;
        if result == s.named(b"nil")? || s.is_error(result)? {
            return Err(Outcome::loaded(Operand::object(result)));
        }
        Ok(())
    }
}
pub(super) unsafe fn operation(
    s: &State,
    q: &Scope<'_>,
    op: Op,
    args: &[Operand],
) -> Result<Operand, Outcome> {
    unsafe {
        if s.candidates.recovery.borrow().is_some()
            && !(matches!(op, Op::AdoptComposed | Op::RecoveryIdentity)
                && s.candidates.recovering_policy.get())
        {
            return fail(
                s,
                q,
                b"imageRecoveryActive",
                b"candidate operations are unavailable during image recovery",
            );
        }
        if s.candidates.busy.replace(true) {
            return fail(s, q, b"candidateBusy", b"candidate operation is reentrant");
        }
        struct Busy<'a>(&'a Cell<bool>);
        impl Drop for Busy<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        let _busy = Busy(&s.candidates.busy);
        s.sources.clear_error();
        match op {
            Op::Discard => retirement::discard(s, q, object(args[0])?),
            Op::RetainedCount | Op::RetainedBytes => {
                retirement::telemetry(s, q, matches!(op, Op::RetainedBytes))
            }
            Op::RecoveryIdentity => {
                let identity = s
                    .candidates
                    .recovery
                    .borrow()
                    .as_ref()
                    .filter(|_| s.candidates.recovering_policy.get())
                    .map(|r| r.identity.clone());
                Ok(Operand::object(if let Some(identity) = identity {
                    s.text(q, &identity)?
                } else {
                    s.named(b"nil")?
                }))
            }
            Op::Commit => transaction::commit(s, q, object(args[0])?, false),
            Op::CommitRollback => transaction::commit(s, q, object(args[0])?, true),
            Op::PrepareRollback => {
                transaction::prepare_rollback(s, q, object(args[0])?, object(args[1])?)
            }
            Op::AdoptInitial => ledger::adopt(s, q, object(args[0])?, false),
            Op::AdoptComposed => ledger::adopt(s, q, object(args[0])?, true),
            Op::Reconcile => ledger::reconcile_candidate(s, q, object(args[0])?),
            Op::Prepare => preparation::prepare(s, q, object(args[0])?, object(args[1])?),
            Op::DiscardPrepared => preparation::discard(s, q, object(args[0])?),
            Op::Compile | Op::CompileSelected => {
                let source = object(args[0])?;
                let name = object(args[1])?;
                let path = object(args[2])?;
                let Some((bytes, metadata)) = s.revision(q, source)? else {
                    return fail(
                        s,
                        q,
                        b"invalidCandidate",
                        b"candidate source is not indexable",
                    );
                };
                if s.symbol_bytes(q, name)?.as_deref() != Some(metadata.name.as_slice()) {
                    return fail(
                        s,
                        q,
                        b"invalidCandidate",
                        b"candidate module name mismatches",
                    );
                }
                let predecessor = current(s, q)?;
                if predecessor == s.named(b"nil")? {
                    return fail(
                        s,
                        q,
                        b"generationNotInitialized",
                        b"candidate compilation needs a current generation",
                    );
                }
                let expected = s.bytes(fact(s, q, predecessor, b"baseDigest")?)?;
                let expected = <&[u8; 64]>::try_from(expected.as_slice()).map_err(|_| invalid())?;
                let selected = select(
                    s,
                    q,
                    &metadata,
                    args.get(3).copied().map(object).transpose()?,
                )?;
                let map = fact(s, q, predecessor, b"sourceMap")?;
                let old_selection = s.bytes(fact(s, q, predecessor, b"generationSelection")?)?;
                let modules = modules(s, q, map)?;
                let projection = generation::project_candidate_generation(
                    modules,
                    generation::Module::new(bytes.clone(), metadata.clone()).ok_or_else(invalid)?,
                    &old_selection,
                    &selected,
                    |m, i, _| {
                        Some(generation::sha256_raw(
                            &emission::named_descriptor(&m.metadata.operations.get(i)?.selector)
                                .ok()?,
                        ))
                    },
                );
                if projection.is_none() {
                    return fail(
                        s,
                        q,
                        b"invalidCandidate",
                        b"candidate changes unsupported initializers or generation selection",
                    );
                }
                let tree = s.source_tree(q, source, bytes.len())?.ok_or_else(invalid)?;
                let Some(plan) = code::Plan::new(&tree, &bytes, &selected, true) else {
                    return fail(s, q, b"invalidCandidate", b"candidate cannot be lowered");
                };
                let mut objects = Vec::new();
                for c in &plan.sources {
                    let c = s.text(q, c)?;
                    let object = call(
                        s,
                        q,
                        s.named(b"TCC")?,
                        b"compileObject:",
                        &[Operand::object(c)],
                    )?;
                    if s.is_error(object)? {
                        return Ok(Operand::object(object));
                    }
                    objects.push(s.bytes(object)?);
                }
                let artifact = plan.encode(&objects).ok_or_else(invalid)?;
                let sidecar = code::envelope(&artifact, expected, &bytes, &selected, &metadata);
                let mut side_path = s.bytes(path)?;
                side_path.extend_from_slice(b".candidate");
                let side_path = s.text(q, &side_path)?;
                write(s, q, path, &artifact)?;
                write(s, q, side_path, &sidecar)?;
                Ok(Operand::object(path))
            }
            Op::Successor => {
                let map = object(args[0])?;
                let name = object(args[1])?;
                let revision = object(args[2])?;
                let unit = object(args[3])?;
                let snapshot = s
                    .candidates
                    .entries
                    .borrow()
                    .iter()
                    .find(|c| c.unit == unit)
                    .map(|c| {
                        (
                            c.source,
                            c.name,
                            c.expected,
                            c.selected.clone(),
                            c.plan.metadata.clone(),
                        )
                    });
                let Some((source, candidate_name, expected, selected, metadata)) = snapshot else {
                    return fail(s, q, b"invalidCandidate", b"candidate carrier is unknown");
                };
                let predecessor = current(s, q)?;
                if predecessor == s.named(b"nil")?
                    || fact(s, q, predecessor, b"sourceMap")? != map
                    || s.bytes(fact(s, q, predecessor, b"baseDigest")?)? != expected
                    || candidate_name != name
                {
                    return fail(
                        s,
                        q,
                        b"staleGeneration",
                        b"candidate predecessor has changed",
                    );
                }
                let fields = seq(s, q, revision)?;
                if fields.len() != 7
                    || fields[0] != symbol(s, q, b"sourceRevision")?
                    || fields[1] != name
                    || s.bytes(fields[3])? != s.bytes(source)?
                    || s.bytes(fields[6])? != metadata.revision_digest
                {
                    return fail(s, q, b"invalidCandidate", b"candidate revision mismatches");
                }
                let prior = modules(s, q, map)?;
                let selection = s.bytes(fact(s, q, predecessor, b"generationSelection")?)?;
                let Some(projected) = generation::project_candidate_generation(
                    prior,
                    generation::Module::new(s.bytes(source)?, metadata).ok_or_else(invalid)?,
                    &selection,
                    &selected,
                    |m, i, _| {
                        Some(generation::sha256_raw(
                            &emission::named_descriptor(&m.metadata.operations.get(i)?.selector)
                                .ok()?,
                        ))
                    },
                ) else {
                    return fail(
                        s,
                        q,
                        b"invalidCandidate",
                        b"candidate successor cannot be projected",
                    );
                };
                let mut rows = Vec::new();
                for row in seq(s, q, map)? {
                    let values = seq(s, q, row)?;
                    if values.len() != 3 {
                        return Err(invalid());
                    }
                    rows.push(if values[0] == name {
                        s.vector(q, &[name, revision, unit])?
                    } else {
                        row
                    });
                }
                let map = s.vector(q, &rows)?;
                let tag = symbol(s, q, b"generationProjection")?;
                let digest = s.text(q, &generation::raw_digest_spelling(projected.authority))?;
                let selected = s.text(q, &projected.selection)?;
                s.vector(q, &[tag, map, digest, selected])
                    .map(Operand::object)
            }
            Op::Inspect => {
                let path = object(args[0])?;
                let unit = object(args[1])?;
                if s.candidates.entries.borrow().iter().any(|c| c.unit == unit) {
                    return fail(
                        s,
                        q,
                        b"invalidLifecycleTransition",
                        b"candidate carrier is already admitted",
                    );
                }
                let artifact = read(s, q, path)?;
                if s.is_error(artifact)? {
                    return Ok(Operand::object(artifact));
                }
                let artifact = s.bytes(artifact)?;
                let mut side = s.bytes(path)?;
                side.extend_from_slice(b".candidate");
                let side = s.text(q, &side)?;
                let side = read(s, q, side)?;
                if s.is_error(side)? {
                    return Ok(Operand::object(side));
                }
                let side = s.bytes(side)?;
                let Ok(env) = envelope::inspect(&side) else {
                    return fail(s, q, b"invalidCandidate", b"candidate envelope is invalid");
                };
                if env.artifact_digest != source_revision::source_digest(&artifact) {
                    return fail(
                        s,
                        q,
                        b"invalidCandidate",
                        b"candidate artifact checksum mismatches",
                    );
                }
                let source = s.text(q, env.source)?;
                let tree = s
                    .source_tree(q, source, env.source.len())?
                    .ok_or_else(invalid)?;
                let Some(plan) = code::Plan::new(&tree, env.source, env.artifact_selection, false)
                else {
                    return fail(s, q, b"invalidCandidate", b"candidate inventory is invalid");
                };
                if code::manifest(
                    &plan.metadata,
                    &source_revision::source_digest(env.source),
                    &source_revision::source_digest(env.artifact_selection),
                ) != env.manifest
                {
                    return fail(
                        s,
                        q,
                        b"invalidCandidate",
                        b"candidate manifest does not match its source",
                    );
                }
                let Some(objects) = plan.decode(&artifact) else {
                    return fail(
                        s,
                        q,
                        b"invalidCandidate",
                        b"candidate object bundle is invalid",
                    );
                };
                let name = symbol(s, q, &plan.metadata.name)?;
                let revision = q.keep(object(source::operation(
                    s,
                    q,
                    source::Op::Revision,
                    &[Operand::object(source)],
                )?)?)?;
                let requires = plan
                    .metadata
                    .requires
                    .iter()
                    .map(|name| symbol(s, q, name))
                    .collect::<Result<Vec<_>, _>>()?;
                let requires = s.vector(q, &requires)?;
                let mut operations = Vec::new();
                for op in &plan.metadata.operations {
                    let row = call(s, q, s.named(b"Object")?, b"clone", &[])?;
                    for (key, value) in [
                        (b"targetName".as_slice(), symbol(s, q, &op.target)?),
                        (b"selector", symbol(s, q, &op.selector)?),
                        (
                            b"operation",
                            symbol(s, q, if op.install { b"install" } else { b"remove" })?,
                        ),
                        (b"priorPresent", s.named(b"false")?),
                    ] {
                        assert(s, q, row, key, value)?;
                    }
                    operations.push(row);
                }
                let operations = s.vector(q, &operations)?;
                for (key, value) in [
                    (b"unitName".as_slice(), name),
                    (b"authoredSource", source),
                    (b"sourceRevision", revision),
                    (b"requires", requires),
                    (b"provides", operations),
                    (
                        b"revisionDigest",
                        s.text(q, &plan.metadata.revision_digest)?,
                    ),
                    (b"artifactChecksum", s.text(q, env.artifact_digest)?),
                    (
                        b"expectedBaseDigest",
                        s.text(q, env.expected_predecessor_digest)?,
                    ),
                    (b"importStatus", symbol(s, q, b"notMapped")?),
                    (b"state", symbol(s, q, b"candidate")?),
                    (b"rollbackAvailable", s.named(b"false")?),
                ] {
                    assert(s, q, unit, key, value)?;
                }
                s.candidates.entries.borrow_mut().push(Candidate {
                    unit,
                    source,
                    name,
                    path,
                    expected: env
                        .expected_predecessor_digest
                        .try_into()
                        .map_err(|_| invalid())?,
                    selected: env.artifact_selection.to_vec(),
                    plan,
                    objects,
                    envelope: side,
                    mapped: None,
                    construction_started: false,
                    constructed: None,
                    activation: None,
                });
                Ok(Operand::object(unit))
            }
            Op::Mapped | Op::Path | Op::Map => {
                let unit = object(args[0])?;
                let snapshot = s
                    .candidates
                    .entries
                    .borrow()
                    .iter()
                    .find(|c| c.unit == unit)
                    .map(|c| {
                        (
                            c.path,
                            c.mapped.is_some(),
                            c.objects.clone(),
                            c.plan.metadata.requires.clone(),
                        )
                    });
                let Some((path, already, objects, requires)) = snapshot else {
                    return fail(s, q, b"invalidCandidate", b"candidate carrier is unknown");
                };
                if matches!(op, Op::Path) {
                    return Ok(Operand::object(path));
                }
                if matches!(op, Op::Mapped) {
                    return Ok(Operand::object(s.named(if already {
                        b"true"
                    } else {
                        b"false"
                    })?));
                }
                if already {
                    return Ok(Operand::object(unit));
                }
                for dependency in requires {
                    let module_name = symbol(s, q, b"Module")?;
                    let module = call(
                        s,
                        q,
                        s.named(b"Globals")?,
                        b"at:",
                        &[Operand::object(module_name)],
                    )?;
                    let dep = symbol(s, q, &dependency)?;
                    if module == s.named(b"nil")?
                        || call(s, q, module, b"isLoaded:", &[Operand::object(dep)])?
                            != s.named(b"true")?
                    {
                        return fail(
                            s,
                            q,
                            b"dependenciesMissing",
                            b"candidate dependency is not loaded",
                        );
                    }
                }
                let mut mapped = Vec::new();
                let mut identities = Vec::new();
                for bytes in objects {
                    let bytes = s.text(q, &bytes)?;
                    let blob = call(s, q, bytes, b"asBytes", &[])?;
                    let unit = call(
                        s,
                        q,
                        s.named(b"TCC")?,
                        b"loadObject:",
                        &[Operand::object(blob)],
                    )?;
                    if s.is_error(unit)? {
                        return Ok(Operand::object(unit));
                    }
                    identities.push(
                        s.send(
                            s.named(b"TCC")?,
                            b"mappingIdentityOf:",
                            &[Operand::object(unit)],
                        )?
                        .as_word()
                        .filter(|v| *v != 0)
                        .ok_or_else(invalid)?,
                    );
                    mapped.push(unit);
                }
                let factory = call(s, q, mapped[0], b"initialize", &[])?;
                if factory == s.named(b"nil")? || s.is_error(factory)? {
                    return Ok(Operand::object(factory));
                }
                // Publish ownership only after every relocation and registration succeeds.
                let adapters = mapped.into_iter().skip(1).collect();
                s.candidates
                    .entries
                    .borrow_mut()
                    .iter_mut()
                    .find(|c| c.unit == unit)
                    .ok_or_else(invalid)?
                    .mapped = Some(Mapped {
                    factory,
                    adapters,
                    identities,
                });
                Ok(Operand::object(unit))
            }
        }
    }
}

// Host enters with no running Machine. Only this dynamic extent may execute the
// authored lifecycle policy and commit while its policy frames are running.
// No ambient phase survives a returned Error or native failure.
pub(super) unsafe fn fulfill_intent(
    s: &State,
    q: &Scope<'_>,
    intent: Oop,
) -> Result<Operand, Outcome> {
    unsafe {
        if s.candidates.quiescent.get()
            || s.candidates.recovery.borrow().is_some()
            || s.candidates.busy.get()
            || s.send(s.named(b"Execution")?, b"retirementReady", &[])?
                .as_word()
                != Some(1)
        {
            return fail(
                s,
                q,
                b"generationNotQuiescent",
                b"intent fulfillment requires a Host safe point",
            );
        }
        let prototype = call(
            s,
            q,
            s.named(b"Globals")?,
            b"at:",
            &[Operand::object(symbol(s, q, b"EvolutionIntent")?)],
        )?;
        let family = call(
            s,
            q,
            s.id.receiver,
            b"behaviorOf:",
            &[Operand::object(prototype)],
        )?;
        if prototype == s.named(b"nil")?
            || call(
                s,
                q,
                s.id.receiver,
                b"behaviorOf:",
                &[Operand::object(intent)],
            )? != family
        {
            return fail(
                s,
                q,
                b"invalidEvolutionIntent",
                b"expected an EvolutionIntent",
            );
        }
        struct Quiescence<'a>(&'a Cell<bool>);
        impl Drop for Quiescence<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        s.candidates.quiescent.set(true);
        let _quiescence = Quiescence(&s.candidates.quiescent);
        s.send(intent, b"fulfill", &[])
    }
}
