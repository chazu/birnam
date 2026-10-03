//! Inert candidate construction. Artifact code returns a factory Method; only
//! executing that factory constructs projected Blocks, without publishing them.
use crate::{Block, Expr, OwnedCst, Statement, generation, portable, resumable, source_revision};
use birnam_artifact::{
    candidate as envelope,
    stackless_selection::{self as selection, Disposition},
};
use std::{borrow::Cow, collections::BTreeMap};

pub struct Plan {
    pub metadata: source_revision::RevisionMetadata,
    /// Each output is either a method-operation index or a projected Globals name.
    pub outputs: Vec<Output>,
    /// Factory first, then one named adapter per installed operation.
    pub requests: Vec<crate::backend_request::Request>,
    authority: [u8; 32],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Output {
    Method(usize),
    Global(Vec<u8>),
}
fn blocks(expr: &Expr<'_>, styles: &BTreeMap<usize, bool>, output: &mut Vec<bool>) -> Option<()> {
    match expr {
        Expr::Block(block) => {
            output.push(*styles.get(&(block.as_ref() as *const Block<'_> as usize))?);
            for statement in &block.body {
                blocks(
                    match statement {
                        Statement::Let { value, .. } | Statement::Expr(value) => value,
                    },
                    styles,
                    output,
                )?;
            }
        }
        Expr::Send { receiver, args, .. } => {
            blocks(receiver, styles, output)?;
            for arg in args {
                blocks(arg, styles, output)?;
            }
        }
        _ => {}
    }
    Some(())
}
fn send<'a>(receiver: Expr<'a>, selector: &'static [u8], args: Vec<Expr<'a>>) -> Expr<'a> {
    Expr::Send {
        receiver: Box::new(receiver),
        selector: Cow::Borrowed(selector),
        args,
        source: b"",
    }
}
impl Plan {
    pub fn new(tree: &OwnedCst, source: &[u8], selected: &[u8], _emit: bool) -> Option<Self> {
        let metadata = source_revision::inspect(tree, source)?;
        if metadata.operations.is_empty()
            || !generation::artifact_selection_matches_inventory(&metadata, selected)
        {
            return None;
        }
        let program = crate::lower_program(tree, source).ok()?;
        let projected = resumable::projected_block_bindings(&program)
            .into_iter()
            .filter(|p| p.captures.is_empty())
            .map(|p| (p.statement_index, p.name))
            .collect::<BTreeMap<_, _>>();
        let selection = selection::inspect_artifact_selection(selected).ok()?;
        let rows = selection.rows().collect::<Result<Vec<_>, _>>().ok()?;
        let mut styles = BTreeMap::new();
        let inventory = resumable::implementation_inventory_with_blocks(&program);
        for ((_, block), row) in inventory.iter().zip(&rows) {
            if let Some(block) = block {
                styles.insert(
                    *block as *const Block<'_> as usize,
                    row.disposition == Disposition::Synchronous,
                );
            }
        }
        let mut name = b"__candidate_values".to_vec();
        while source.windows(name.len()).any(|part| part == name) {
            name.push(b'_');
        }
        let mut body = vec![Statement::Let {
            name: &name,
            value: send(Expr::Global(b"Vector"), b"new", vec![]),
        }];
        let mut dispositions = vec![false];
        let mut outputs = Vec::new();
        let mut operation = 0;
        for (index, statement) in program.body.into_iter().enumerate() {
            let expression = match &statement {
                Statement::Let { value, .. } | Statement::Expr(value) => value,
            };
            if let Some(install) = resumable::method_operation(expression) {
                let Statement::Expr(Expr::Send { mut args, .. }) = statement else {
                    return None;
                };
                if install {
                    let value = args.remove(2);
                    if !matches!(value, Expr::Block(_)) {
                        return None;
                    }
                    blocks(&value, &styles, &mut dispositions)?;
                    body.push(Statement::Let {
                        name: &name,
                        value: send(Expr::Global(&name), b"add:", vec![value]),
                    });
                    outputs.push(Output::Method(operation));
                }
                operation += 1;
            } else if let Some(global) = projected.get(&index) {
                blocks(expression, &styles, &mut dispositions)?;
                let Statement::Let {
                    name: global_name, ..
                } = &statement
                else {
                    return None;
                };
                let global_name = *global_name;
                body.push(statement);
                body.push(Statement::Let {
                    name: &name,
                    value: send(
                        Expr::Global(&name),
                        b"add:",
                        vec![Expr::Global(global_name)],
                    ),
                });
                outputs.push(Output::Global(global.clone()));
            }
        }
        if operation != metadata.operations.len() {
            return None;
        }
        body.push(Statement::Expr(Expr::Global(&name)));
        let factory = portable::encode_block(
            &Block {
                params: vec![],
                body,
            },
            &[],
        )
        .ok()?;
        use crate::backend_request::Request;
        let mut requests = vec![Request::Factory {
            program: factory,
            source: source.to_vec(),
            dispositions,
        }];
        for (index, operation) in metadata
            .operations
            .iter()
            .enumerate()
            .filter(|(_, op)| op.install)
        {
            let row = rows.iter().find(|r| {
                r.root_kind == selection::RootKind::MethodOperation
                    && r.root_index as usize == index
                    && r.path.is_empty()
                    && r.implementation_kind == selection::ImplementationKind::NamedAdapter
            })?;
            let arity =
                u32::try_from(birnam_artifact::selector::arity(&operation.selector)).ok()?;
            requests.push(Request::Named {
                arity,
                synchronous: row.disposition == Disposition::Synchronous,
            });
        }
        let mut identity = b"birnam.candidate.objects.v1\0".to_vec();
        for bytes in [source, selected] {
            identity.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
            identity.extend_from_slice(bytes);
        }
        Some(Self {
            metadata,
            outputs,
            requests,
            authority: generation::sha256_raw(&identity),
        })
    }
    /// Reconstitute a compiler inventory received as ordinary values. The
    /// canonical programs stay independent of the compiler's private CST/IR.
    pub fn from_inventory(
        metadata: source_revision::RevisionMetadata,
        outputs: Vec<Output>,
        requests: Vec<crate::backend_request::Request>,
        source: &[u8],
        selected: &[u8],
    ) -> Option<Self> {
        use crate::backend_request::Request;
        if !generation::artifact_selection_matches_inventory(&metadata, selected)
            || metadata.operations.is_empty()
            || requests.len() != 1 + metadata.operations.iter().filter(|op| op.install).count()
            || outputs.iter().any(|o| match o {
                Output::Method(i) => metadata.operations.get(*i).is_none_or(|op| !op.install),
                Output::Global(n) => !metadata.projected_globals.iter().any(|g| &g.name == n),
            })
        {
            return None;
        }
        let Some(Request::Factory {
            program,
            source: provenance,
            dispositions,
        }) = requests.first()
        else {
            return None;
        };
        if provenance != source
            || program.is_empty()
            || dispositions.first() != Some(&false)
            || requests[1..]
                .iter()
                .any(|r| !matches!(r, Request::Named { arity: 0..=16, .. }))
        {
            return None;
        }
        let mut identity = b"birnam.candidate.objects.v1\0".to_vec();
        for bytes in [source, selected] {
            identity.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
            identity.extend_from_slice(bytes);
        }
        Some(Self {
            metadata,
            outputs,
            requests,
            authority: generation::sha256_raw(&identity),
        })
    }
    pub fn encode(&self, objects: &[Vec<u8>]) -> Option<Vec<u8>> {
        if objects.len() != self.requests.len() {
            return None;
        }
        let mut out = b"BIRNAMC1".to_vec();
        out.extend_from_slice(&self.authority);
        out.extend_from_slice(&(objects.len() as u32).to_le_bytes());
        for bytes in objects {
            if bytes.is_empty() {
                return None;
            }
            out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
            out.extend_from_slice(bytes);
            if out.len() > envelope::MAX_CANDIDATE_BYTES {
                return None;
            }
        }
        Some(out)
    }
    pub fn decode(&self, bytes: &[u8]) -> Option<Vec<Vec<u8>>> {
        if bytes.len() > envelope::MAX_CANDIDATE_BYTES
            || bytes.get(..8)? != b"BIRNAMC1"
            || bytes.get(8..40)? != self.authority
        {
            return None;
        }
        let count = u32::from_le_bytes(bytes.get(40..44)?.try_into().ok()?) as usize;
        if count != self.requests.len() {
            return None;
        }
        let mut cursor = 44usize;
        let mut out = Vec::new();
        for _ in 0..count {
            let len = usize::try_from(u64::from_le_bytes(
                bytes.get(cursor..cursor.checked_add(8)?)?.try_into().ok()?,
            ))
            .ok()?;
            cursor += 8;
            if len == 0 {
                return None;
            }
            let end = cursor.checked_add(len)?;
            out.push(bytes.get(cursor..end)?.to_vec());
            cursor = end;
        }
        (cursor == bytes.len()).then_some(out)
    }
}

pub fn manifest(
    metadata: &source_revision::RevisionMetadata,
    source_digest: &[u8; 64],
    selection_digest: &[u8; 64],
) -> Vec<u8> {
    let mut manifest = Vec::new();
    manifest.extend_from_slice(b"v4|");
    manifest.extend_from_slice(birnam_artifact::ABI);
    manifest.push(b'|');
    manifest.extend_from_slice(metadata.name.len().to_string().as_bytes());
    manifest.push(b':');
    manifest.extend_from_slice(&metadata.name);
    manifest.push(b'|');
    manifest.extend_from_slice(source_digest);
    manifest.push(b'|');
    manifest.extend_from_slice(&metadata.non_method_digest);
    manifest.push(b'|');
    manifest.extend_from_slice(&metadata.method_digest);
    manifest.push(b'|');
    manifest.extend_from_slice(&metadata.revision_digest);
    manifest.push(b'|');
    manifest.extend_from_slice(selection_digest);
    manifest.push(b'|');
    manifest.extend_from_slice(b"1:");
    manifest.extend_from_slice(&metadata.revision_digest);
    manifest.push(b'|');
    manifest.extend_from_slice(metadata.requires.len().to_string().as_bytes());
    manifest.push(b':');
    for requirement in &metadata.requires {
        manifest.extend_from_slice(requirement.len().to_string().as_bytes());
        manifest.push(b':');
        manifest.extend_from_slice(requirement);
    }
    manifest.push(b'|');
    manifest.extend_from_slice(metadata.operations.len().to_string().as_bytes());
    manifest.push(b':');
    for operation in &metadata.operations {
        manifest.extend_from_slice(operation.target.len().to_string().as_bytes());
        manifest.push(b':');
        manifest.extend_from_slice(&operation.target);
        manifest.extend_from_slice(operation.selector.len().to_string().as_bytes());
        manifest.push(b':');
        manifest.extend_from_slice(&operation.selector);
        manifest.push(if operation.install { b'I' } else { b'R' });
        manifest.extend_from_slice(
            birnam_artifact::selector::arity(&operation.selector)
                .to_string()
                .as_bytes(),
        );
        manifest.push(b':');
    }
    manifest
}

/// The existing pointer-free envelope binds all artifact bytes and exact source.
pub fn envelope(
    artifact: &[u8],
    predecessor: &[u8; 64],
    source: &[u8],
    selected: &[u8],
    metadata: &source_revision::RevisionMetadata,
) -> Vec<u8> {
    let mut out = envelope::CANDIDATE_MAGIC.to_vec();
    for (key, value) in [
        (
            b"artifact=".as_slice(),
            source_revision::source_digest(artifact).to_vec(),
        ),
        (b"runtime=", birnam_artifact::ABI.to_vec()),
        (
            b"candidate=",
            envelope::CANDIDATE_ABI.to_string().into_bytes(),
        ),
        (b"platform=", envelope::CANDIDATE_PLATFORM.to_vec()),
        (b"predecessor=", predecessor.to_vec()),
        (
            b"manifest=",
            manifest(
                metadata,
                &source_revision::source_digest(source),
                &source_revision::source_digest(selected),
            ),
        ),
    ] {
        out.push(b'\n');
        out.extend_from_slice(key);
        out.extend_from_slice(&value);
    }
    out.extend_from_slice(format!("\nselection={}\n", selected.len()).as_bytes());
    out.extend_from_slice(selected);
    out.extend_from_slice(format!("\nsource={}\n", source.len()).as_bytes());
    out.extend_from_slice(source);
    out
}
