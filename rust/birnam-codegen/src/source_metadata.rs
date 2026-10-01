//! Compiler-produced source information. Durable objects carry this inventory;
//! activating compiled code never needs a parser to reconstruct it.
use crate::{
    OwnedCst,
    resumable::*,
    source_revision::{self, *},
};

/// Inventory adapter arities while the compiler owns the normalized program.
pub fn named_adapter_arities(program: &crate::Program<'_>) -> Vec<u32> {
    fn visit(expr: &crate::Expr<'_>, arities: &mut std::collections::BTreeSet<u32>) {
        use crate::{Expr, Statement};
        match expr {
            Expr::Block(block) => {
                for statement in &block.body {
                    visit(
                        match statement {
                            Statement::Let { value, .. } | Statement::Expr(value) => value,
                        },
                        arities,
                    );
                }
            }
            Expr::Send {
                receiver,
                selector,
                args,
                ..
            } => {
                if matches!(
                    selector.as_ref(),
                    b"defineMethod:on:as:" | b"defineMethod:on:as:doc:"
                ) {
                    if let Some(Expr::Symbol(name)) = args.first() {
                        arities.insert(birnam_artifact::selector::arity(name) as u32);
                    } else {
                        arities.extend(0..=16);
                    }
                }
                visit(receiver, arities);
                for arg in args {
                    visit(arg, arities);
                }
            }
            _ => {}
        }
    }
    let mut arities = std::collections::BTreeSet::new();
    for statement in &program.body {
        visit(
            match statement {
                crate::Statement::Let { value, .. } | crate::Statement::Expr(value) => value,
            },
            &mut arities,
        );
    }
    arities.into_iter().collect()
}

const MAGIC: &[u8] = b"birnam.source-metadata.v1\0";
const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_ROWS: usize = 1_000_000;

#[derive(Clone)]
pub struct Metadata {
    pub source_digest: [u8; 64],
    pub module: Option<(Vec<u8>, Vec<Vec<u8>>)>,
    pub revision: Option<RevisionMetadata>,
    pub method_sources: Vec<Vec<u8>>,
    pub embedded_files: Vec<Vec<u8>>,
}
impl Metadata {
    pub fn inspect(tree: &OwnedCst, source: &[u8]) -> Self {
        Self {
            source_digest: source_revision::source_digest(source),
            module: source_revision::artifact_module(tree, source),
            revision: source_revision::inspect(tree, source),
            method_sources: source_revision::method_sources(tree, source),
            embedded_files: source_revision::embedded_files(tree, source),
        }
    }
    pub fn matches_source(&self, source: &[u8]) -> bool {
        self.source_digest == source_revision::source_digest(source)
    }
    pub fn encode(&self) -> Option<Vec<u8>> {
        let mut w = Writer(MAGIC.to_vec());
        w.bytes(&self.source_digest)?;
        w.0.push(u8::from(self.module.is_some()));
        if let Some((name, requires)) = &self.module {
            w.bytes(name)?;
            w.list(requires)?;
        }
        w.0.push(u8::from(self.revision.is_some()));
        if let Some(m) = &self.revision {
            w.bytes(&m.name)?;
            w.list(&m.requires)?;
            w.count(m.operations.len())?;
            for op in &m.operations {
                w.bytes(&op.target)?;
                w.bytes(&op.selector)?;
                w.0.push(u8::from(op.install));
                w.bytes(&op.source)?;
                w.0.push(u8::from(op.program.is_some()));
                if let Some(p) = &op.program {
                    w.bytes(p)?;
                }
            }
            w.count(m.projected_globals.len())?;
            for g in &m.projected_globals {
                w.bytes(&g.name)?;
                w.bytes(&g.program)?;
            }
            w.count(m.inventory.len())?;
            for i in &m.inventory {
                w.0.push(match i.root_kind {
                    InventoryRootKind::NonMethodProgram => 0,
                    InventoryRootKind::MethodOperation => 1,
                });
                w.0.extend_from_slice(&i.root_index.to_le_bytes());
                w.count(i.path.len())?;
                for p in &i.path {
                    w.0.extend_from_slice(&p.to_le_bytes());
                }
                w.0.push(match i.implementation_kind {
                    InventoryImplementationKind::NamedAdapter => 0,
                    InventoryImplementationKind::BlockBody => 1,
                });
            }
            for d in [
                &m.initializer_residual_digest,
                &m.non_method_digest,
                &m.method_digest,
                &m.revision_digest,
            ] {
                w.bytes(d)?;
            }
        }
        w.list(&self.method_sources)?;
        w.list(&self.embedded_files)?;
        (w.0.len() <= MAX_BYTES).then_some(w.0)
    }
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() > MAX_BYTES {
            return None;
        }
        let mut r = Reader(bytes.strip_prefix(MAGIC)?);
        let source_digest = r.digest()?;
        let module = if r.flag()? {
            Some((r.bytes()?, r.list()?))
        } else {
            None
        };
        let revision = if r.flag()? {
            let name = r.bytes()?;
            let requires = r.list()?;
            let mut operations = Vec::new();
            for _ in 0..r.count()? {
                let target = r.bytes()?;
                let selector = r.bytes()?;
                let install = r.flag()?;
                let source = r.bytes()?;
                let program = if r.flag()? { Some(r.bytes()?) } else { None };
                if install != program.is_some() {
                    return None;
                }
                operations.push(RevisionOperation {
                    target,
                    selector,
                    install,
                    source,
                    program,
                });
            }
            let mut projected_globals = Vec::new();
            for _ in 0..r.count()? {
                projected_globals.push(ProjectedGlobal {
                    name: r.bytes()?,
                    program: r.bytes()?,
                });
            }
            let mut inventory = Vec::new();
            for _ in 0..r.count()? {
                let root_kind = if r.flag()? {
                    InventoryRootKind::MethodOperation
                } else {
                    InventoryRootKind::NonMethodProgram
                };
                let root_index = r.u32()?;
                let mut path = Vec::new();
                for _ in 0..r.count()? {
                    path.push(r.u32()?);
                }
                let implementation_kind = if r.flag()? {
                    InventoryImplementationKind::BlockBody
                } else {
                    InventoryImplementationKind::NamedAdapter
                };
                inventory.push(ImplementationKey {
                    root_kind,
                    root_index,
                    path,
                    implementation_kind,
                });
            }
            if inventory.windows(2).any(|w| w[0] >= w[1]) {
                return None;
            }
            Some(RevisionMetadata {
                name,
                requires,
                operations,
                projected_globals,
                inventory,
                initializer_residual_digest: r.digest()?,
                non_method_digest: r.digest()?,
                method_digest: r.digest()?,
                revision_digest: r.digest()?,
            })
        } else {
            None
        };
        let result = Self {
            source_digest,
            module,
            revision,
            method_sources: r.list()?,
            embedded_files: r.list()?,
        };
        r.0.is_empty().then_some(result)
    }
}
struct Writer(Vec<u8>);
impl Writer {
    fn count(&mut self, n: usize) -> Option<()> {
        if n > MAX_ROWS {
            return None;
        }
        self.0
            .extend_from_slice(&u32::try_from(n).ok()?.to_le_bytes());
        Some(())
    }
    fn bytes(&mut self, b: &[u8]) -> Option<()> {
        if b.len() > MAX_BYTES || self.0.len().checked_add(b.len() + 4)? > MAX_BYTES {
            return None;
        }
        self.0
            .extend_from_slice(&u32::try_from(b.len()).ok()?.to_le_bytes());
        self.0.extend_from_slice(b);
        Some(())
    }
    fn list(&mut self, rows: &[Vec<u8>]) -> Option<()> {
        self.count(rows.len())?;
        for row in rows {
            self.bytes(row)?;
        }
        Some(())
    }
}
struct Reader<'a>(&'a [u8]);
impl Reader<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let (v, rest) = self.0.split_at_checked(n)?;
        self.0 = rest;
        Some(v)
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn count(&mut self) -> Option<usize> {
        let n = self.u32()? as usize;
        (n <= MAX_ROWS && n <= self.0.len()).then_some(n)
    }
    fn flag(&mut self) -> Option<bool> {
        match self.take(1)?[0] {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }
    fn bytes(&mut self) -> Option<Vec<u8>> {
        let n = self.u32()? as usize;
        Some(self.take(n)?.to_vec())
    }
    fn list(&mut self) -> Option<Vec<Vec<u8>>> {
        (0..self.count()?).map(|_| self.bytes()).collect()
    }
    fn digest(&mut self) -> Option<[u8; 64]> {
        let d: [u8; 64] = self.bytes()?.try_into().ok()?;
        d.iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
            .then_some(d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Metadata {
        let source = b"a compiler source snapshot";
        Metadata {
            source_digest: source_revision::source_digest(source),
            module: Some((b"test".to_vec(), vec![b"stdlib".to_vec()])),
            revision: Some(RevisionMetadata {
                name: b"test".to_vec(),
                requires: vec![b"stdlib".to_vec()],
                operations: vec![RevisionOperation {
                    target: b"Object".to_vec(),
                    selector: b"answer".to_vec(),
                    install: true,
                    source: b"method source".to_vec(),
                    program: Some(vec![0, 255, 3]),
                }],
                projected_globals: vec![ProjectedGlobal {
                    name: b"main".to_vec(),
                    program: vec![4, 0, 9],
                }],
                inventory: vec![ImplementationKey {
                    root_kind: InventoryRootKind::MethodOperation,
                    root_index: 0,
                    path: vec![0, 2],
                    implementation_kind: InventoryImplementationKind::BlockBody,
                }],
                initializer_residual_digest: source_revision::source_digest(b"residual"),
                non_method_digest: source_revision::source_digest(b"nonmethod"),
                method_digest: source_revision::source_digest(b"methods"),
                revision_digest: source_revision::source_digest(b"revision"),
            }),
            method_sources: vec![b"method source".to_vec()],
            embedded_files: vec![b"file.txt".to_vec()],
        }
    }
    #[test]
    fn metadata_preserves_inventory_programs_and_source_identity() {
        let original = fixture();
        let bytes = original.encode().unwrap();
        let decoded = Metadata::decode(&bytes).unwrap();
        assert_eq!(decoded.encode().unwrap(), bytes);
        assert!(decoded.matches_source(b"a compiler source snapshot"));
        assert!(!decoded.matches_source(b"changed source"));
        let revision = decoded.revision.unwrap();
        assert_eq!(revision.operations[0].program, Some(vec![0, 255, 3]));
        assert_eq!(revision.projected_globals[0].program, vec![4, 0, 9]);
        assert_eq!(revision.inventory[0].path, vec![0, 2]);
    }
    #[test]
    fn rejects_truncation_trailing_data_and_noncanonical_inventory() {
        let mut fixture = fixture();
        let bytes = fixture.encode().unwrap();
        for n in 0..bytes.len() {
            assert!(Metadata::decode(&bytes[..n]).is_none(), "{n}");
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(Metadata::decode(&trailing).is_none());
        let revision = fixture.revision.as_mut().unwrap();
        revision.inventory.push(revision.inventory[0].clone());
        assert!(Metadata::decode(&fixture.encode().unwrap()).is_none());
    }
    #[test]
    fn non_module_programs_keep_documentation_and_asset_inventory() {
        let mut fixture = fixture();
        fixture.module = None;
        fixture.revision = None;
        let decoded = Metadata::decode(&fixture.encode().unwrap()).unwrap();
        assert!(decoded.revision.is_none());
        assert!(decoded.module.is_none());
        assert_eq!(decoded.embedded_files, vec![b"file.txt".to_vec()]);
    }
}
