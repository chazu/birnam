//! Pure dispatch-generation identity and selection algebra (Design 47).
//! Runtime owners supply validated source metadata; no Oops or loader state live here.
use crate::resumable::{InventoryImplementationKind, InventoryRootKind};
use crate::source_revision::RevisionMetadata;
use birnam_artifact::stackless_selection::{
    self as selection, Disposition as SelectionDisposition,
    ImplementationKind as SelectionImplementationKind, RootKind, compose_generation_selection,
    inspect_artifact_selection, inspect_generation_selection, replace_generation_selection,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Clone)]
pub struct Module {
    pub name: Vec<u8>,
    pub revision: [u8; 32],
    pub source: Vec<u8>,
    pub metadata: RevisionMetadata,
}
pub struct Projection {
    pub selection: Vec<u8>,
    pub authority: [u8; 32],
}
pub fn sha256_raw(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
pub fn decode_hex_digest(bytes: &[u8]) -> Option<[u8; 32]> {
    if bytes.len() != 64 {
        return None;
    }
    let digit = |b| match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    };
    let mut result = [0; 32];
    for (out, pair) in result.iter_mut().zip(bytes.chunks_exact(2)) {
        *out = digit(pair[0])? * 16 + digit(pair[1])?;
    }
    Some(result)
}
impl Module {
    pub fn new(source: Vec<u8>, metadata: RevisionMetadata) -> Option<Self> {
        Some(Self {
            name: metadata.name.clone(),
            revision: decode_hex_digest(&metadata.revision_digest)?,
            source,
            metadata,
        })
    }
}
fn push_be_field(output: &mut Vec<u8>, bytes: &[u8]) -> Option<()> {
    output.extend_from_slice(&u32::try_from(bytes.len()).ok()?.to_be_bytes());
    output.extend_from_slice(bytes);
    Some(())
}
fn canonical_modules(modules: &[Module]) -> bool {
    modules.windows(2).all(|p| p[0].name < p[1].name)
        && modules.iter().all(|m| {
            m.name == m.metadata.name
                && decode_hex_digest(&m.metadata.revision_digest) == Some(m.revision)
        })
}
pub fn artifact_selection(
    metadata: &RevisionMetadata,
    disposition: SelectionDisposition,
) -> Option<Vec<u8>> {
    let revision = decode_hex_digest(&metadata.revision_digest)?;
    let rows = metadata
        .inventory
        .iter()
        .map(|key| selection::SelectionSpec {
            source_revision: revision,
            root_kind: match key.root_kind {
                InventoryRootKind::NonMethodProgram => RootKind::NonMethodProgram,
                InventoryRootKind::MethodOperation => RootKind::MethodOperation,
            },
            root_index: key.root_index,
            path: &key.path,
            implementation_kind: match key.implementation_kind {
                InventoryImplementationKind::NamedAdapter => {
                    SelectionImplementationKind::NamedAdapter
                }
                InventoryImplementationKind::BlockBody => SelectionImplementationKind::BlockBody,
            },
            disposition,
        })
        .collect::<Vec<_>>();
    selection::encode_artifact_selection(&[revision], &rows).ok()
}
pub fn artifact_selection_matches_inventory(metadata: &RevisionMetadata, bytes: &[u8]) -> bool {
    let Some(revision) = decode_hex_digest(&metadata.revision_digest) else {
        return false;
    };
    let Ok(selection) = inspect_artifact_selection(bytes) else {
        return false;
    };
    if selection.revision_count() != 1
        || selection.revision(0) != Some(revision)
        || selection.row_count() != metadata.inventory.len()
    {
        return false;
    }
    selection
        .rows()
        .zip(&metadata.inventory)
        .all(|(row, expected)| {
            let Ok(row) = row else {
                return false;
            };
            row.source_revision == revision
                && row.root_kind
                    == match expected.root_kind {
                        InventoryRootKind::NonMethodProgram => RootKind::NonMethodProgram,
                        InventoryRootKind::MethodOperation => RootKind::MethodOperation,
                    }
                && row.root_index == expected.root_index
                && row.path.iter().eq(expected.path.iter().copied())
                && row.implementation_kind
                    == match expected.implementation_kind {
                        InventoryImplementationKind::NamedAdapter => {
                            SelectionImplementationKind::NamedAdapter
                        }
                        InventoryImplementationKind::BlockBody => {
                            SelectionImplementationKind::BlockBody
                        }
                    }
        })
}
fn generation_revisions(modules: &[Module]) -> Option<Vec<[u8; 32]>> {
    let mut revisions = modules
        .iter()
        .map(|module| module.revision)
        .collect::<Vec<_>>();
    revisions.sort_unstable();
    if revisions.windows(2).any(|pair| pair[0] == pair[1]) {
        return None;
    }
    Some(revisions)
}
pub fn generation_selection_with_overrides(
    modules: &[Module],
    overrides: &[(Vec<u8>, Vec<u8>)],
    default: SelectionDisposition,
) -> Option<Vec<u8>> {
    if !canonical_modules(modules)
        || overrides.iter().enumerate().any(|(i, (name, _))| {
            !modules.iter().any(|m| &m.name == name)
                || overrides[..i].iter().any(|(n, _)| n == name)
        })
    {
        return None;
    }
    let revisions = generation_revisions(modules)?;
    let slices = modules
        .iter()
        .map(|module| {
            if let Some((_, selection)) = overrides.iter().find(|(name, _)| name == &module.name) {
                artifact_selection_matches_inventory(&module.metadata, selection)
                    .then(|| selection.clone())
            } else {
                artifact_selection(&module.metadata, default)
            }
        })
        .collect::<Option<Vec<_>>>()?;
    let inspected = slices
        .iter()
        .map(|slice| inspect_artifact_selection(slice).ok())
        .collect::<Option<Vec<_>>>()?;
    compose_generation_selection(&revisions, &inspected).ok()
}
fn visit_generation_module(
    modules: &[Module],
    index: usize,
    state: &mut [u8],
    ordered: &mut Vec<usize>,
) -> bool {
    if state[index] == 2 {
        return true;
    }
    if state[index] == 1 {
        return false;
    }
    state[index] = 1;
    for requirement in &modules[index].metadata.requires {
        if let Ok(dependency) =
            modules.binary_search_by(|module| module.name.as_slice().cmp(requirement))
            && !visit_generation_module(modules, dependency, state, ordered)
        {
            return false;
        }
    }
    state[index] = 2;
    ordered.push(index);
    true
}

pub fn generation_module_order(modules: &[Module]) -> Option<Vec<usize>> {
    if !canonical_modules(modules) {
        return None;
    }
    let mut state = vec![0u8; modules.len()];
    let mut ordered = Vec::with_capacity(modules.len());
    for index in 0..modules.len() {
        if !visit_generation_module(modules, index, &mut state, &mut ordered) {
            return None;
        }
    }
    Some(ordered)
}

/// Require the complete canonical inventory, including nested Blocks, while
/// preserving the caller's selected disposition for every implementation.
pub fn generation_selection_matches_inventory(modules: &[Module], bytes: &[u8]) -> bool {
    let Some(expected) =
        generation_selection_with_overrides(modules, &[], SelectionDisposition::Synchronous)
    else {
        return false;
    };
    let Ok(expected) = inspect_generation_selection(&expected) else {
        return false;
    };
    let Ok(actual) = inspect_generation_selection(bytes) else {
        return false;
    };
    expected.revision_count() == actual.revision_count()
        && (0..expected.revision_count()).all(|i| expected.revision(i) == actual.revision(i))
        && expected.row_count() == actual.row_count()
        && expected.rows().zip(actual.rows()).all(|(a, b)| {
            let (Ok(a), Ok(b)) = (a, b) else {
                return false;
            };
            a.source_revision == b.source_revision
                && a.root_kind == b.root_kind
                && a.root_index == b.root_index
                && a.implementation_kind == b.implementation_kind
                && a.path.iter().eq(b.path.iter())
        })
}

/// Descriptors come from the actual selected backend. Hashing a legacy emitted
/// entry here would silently claim an authority different from the mapped code.
pub fn dispatch_generation_authority_with(
    modules: &[Module],
    generation_selection: &[u8],
    mut descriptor_hash: impl FnMut(
        &Module,
        usize,
        birnam_artifact::stackless_selection::SelectionRow<'_>,
    ) -> Option<[u8; 32]>,
) -> Option<[u8; 32]> {
    if !generation_selection_matches_inventory(modules, generation_selection) {
        return None;
    }
    let selection = inspect_generation_selection(generation_selection).ok()?;
    let revisions = generation_revisions(modules)?;
    if selection.revision_count() != revisions.len()
        || !(0..selection.revision_count())
            .all(|index| selection.revision(index) == revisions.get(index).copied())
    {
        return None;
    }

    // The admitted selection has unique canonical rows. Decode each once:
    // looking up every method by rescanning the wire table is quadratic during
    // startup and image recovery, where a generation contains many methods.
    let named_rows = selection
        .rows()
        .filter_map(|row| {
            let row = row.expect("inspected generation selection");
            (row.root_kind == RootKind::MethodOperation
                && row.path.is_empty()
                && row.implementation_kind == SelectionImplementationKind::NamedAdapter)
                .then_some(((row.source_revision, row.root_index), row))
        })
        .collect::<BTreeMap<_, _>>();

    let mut input = b"birnam.dispatch-base.v2\0".to_vec();
    input.extend_from_slice(&u32::try_from(modules.len()).ok()?.to_be_bytes());
    for module in modules {
        push_be_field(&mut input, &module.name)?;
        input.extend_from_slice(&module.revision);
    }
    let operation_count = modules.iter().try_fold(0usize, |count, module| {
        count.checked_add(module.metadata.operations.len())
    })?;
    input.extend_from_slice(&u32::try_from(operation_count).ok()?.to_be_bytes());
    for module_index in generation_module_order(modules)? {
        let module = &modules[module_index];
        for (operation_index, operation) in module.metadata.operations.iter().enumerate() {
            push_be_field(&mut input, &module.name)?;
            push_be_field(&mut input, &operation.target)?;
            push_be_field(&mut input, &operation.selector)?;
            input.push(if operation.install { 1 } else { 2 });
            if operation.install {
                let mut definition = b"birnam.definition.v1\0".to_vec();
                definition.extend_from_slice(&operation.source);
                input.extend_from_slice(&sha256_raw(&definition));
                let row =
                    *named_rows.get(&(module.revision, u32::try_from(operation_index).ok()?))?;
                match row.disposition {
                    SelectionDisposition::ResumableRequired => {
                        let descriptor = descriptor_hash(module, operation_index, row)?;
                        input.push(2);
                        input.extend_from_slice(&descriptor);
                    }
                    SelectionDisposition::Synchronous => {
                        input.push(1);
                        input.extend_from_slice(&[0; 32]);
                    }
                }
            } else {
                input.extend_from_slice(&[0; 32]);
                input.push(0);
                input.extend_from_slice(&[0; 32]);
            }
        }
    }
    input.extend_from_slice(&sha256_raw(generation_selection));
    Some(sha256_raw(&input))
}

pub fn project_candidate_generation(
    mut modules: Vec<Module>,
    candidate: Module,
    predecessor_selection: &[u8],
    artifact_selection: &[u8],
    descriptor_hash: impl FnMut(&Module, usize, selection::SelectionRow<'_>) -> Option<[u8; 32]>,
) -> Option<Projection> {
    if !canonical_modules(&modules)
        || !artifact_selection_matches_inventory(&candidate.metadata, artifact_selection)
    {
        return None;
    }
    let module_index = modules
        .binary_search_by(|module| module.name.cmp(&candidate.name))
        .ok()?;
    if !generation_selection_matches_inventory(&modules, predecessor_selection)
        || !transition_allowed(&modules[module_index].metadata, &candidate.metadata)
    {
        return None;
    }
    let removed_revision = modules[module_index].revision;
    modules[module_index] = candidate;
    let revisions = generation_revisions(&modules)?;
    let predecessor = inspect_generation_selection(predecessor_selection).ok()?;
    let replacement = inspect_artifact_selection(artifact_selection).ok()?;
    let selection =
        replace_generation_selection(predecessor, &[removed_revision], &revisions, replacement)
            .ok()?;
    let authority = dispatch_generation_authority_with(&modules, &selection, descriptor_hash)?;
    Some(Projection {
        selection,
        authority,
    })
}
pub fn raw_digest_spelling(authority: [u8; 32]) -> [u8; 64] {
    let mut spelling = [0u8; 64];
    for (index, byte) in authority.iter().copied().enumerate() {
        spelling[index * 2] = b"0123456789abcdef"[(byte >> 4) as usize];
        spelling[index * 2 + 1] = b"0123456789abcdef"[(byte & 0x0f) as usize];
    }
    spelling
}

/// Projected capture-free Blocks may change; arbitrary top-level effects and
/// dependency declarations must remain identical across a replacement.
pub fn transition_allowed(prior: &RevisionMetadata, next: &RevisionMetadata) -> bool {
    prior.name == next.name
        && prior.requires == next.requires
        && prior.initializer_residual_digest == next.initializer_residual_digest
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{resumable::ImplementationKey, source_revision::RevisionOperation};
    fn module(name: &[u8], revision: u8) -> Module {
        Module::new(
            b"source".to_vec(),
            RevisionMetadata {
                name: name.to_vec(),
                requires: vec![],
                operations: vec![RevisionOperation {
                    target: b"Integer".to_vec(),
                    selector: b"answer".to_vec(),
                    install: true,
                    source: b"definition".to_vec(),
                }],
                non_method_digest: [b'0'; 64],
                initializer_residual_digest: [b'1'; 64],
                method_digest: [b'2'; 64],
                revision_digest: raw_digest_spelling([revision; 32]),
                inventory: vec![
                    ImplementationKey {
                        root_kind: InventoryRootKind::MethodOperation,
                        root_index: 0,
                        path: vec![],
                        implementation_kind: InventoryImplementationKind::NamedAdapter,
                    },
                    ImplementationKey {
                        root_kind: InventoryRootKind::MethodOperation,
                        root_index: 0,
                        path: vec![0],
                        implementation_kind: InventoryImplementationKind::BlockBody,
                    },
                ],
            },
        )
        .unwrap()
    }
    #[test]
    fn replacement_preserves_other_modules_selection_and_rejects_incomplete_inventory() {
        let modules = vec![module(b"a", 1), module(b"b", 2)];
        let b_selection = artifact_selection(
            &modules[1].metadata,
            SelectionDisposition::ResumableRequired,
        )
        .unwrap();
        let initial = generation_selection_with_overrides(
            &modules,
            &[(b"b".to_vec(), b_selection)],
            SelectionDisposition::Synchronous,
        )
        .unwrap();
        let candidate = module(b"a", 3);
        let selected =
            artifact_selection(&candidate.metadata, SelectionDisposition::Synchronous).unwrap();
        let next = project_candidate_generation(
            modules.clone(),
            candidate.clone(),
            &initial,
            &selected,
            |_, _, _| Some([9; 32]),
        )
        .unwrap();
        let rows = inspect_generation_selection(&next.selection).unwrap();
        for row in rows.rows() {
            let row = row.unwrap();
            assert_eq!(
                row.disposition,
                if row.source_revision == [2; 32] {
                    SelectionDisposition::ResumableRequired
                } else {
                    SelectionDisposition::Synchronous
                }
            );
            assert_ne!(row.source_revision, [1; 32]);
        }
        let incomplete = selection::encode_generation_selection(&[[1; 32], [2; 32]], &[]).unwrap();
        assert!(!generation_selection_matches_inventory(
            &modules,
            &incomplete
        ));
        assert!(
            project_candidate_generation(
                modules.clone(),
                candidate.clone(),
                &incomplete,
                &selected,
                |_, _, _| Some([9; 32])
            )
            .is_none()
        );
        let foreign = module(b"new", 3);
        assert!(
            project_candidate_generation(modules, foreign, &initial, &selected, |_, _, _| Some(
                [9; 32]
            ))
            .is_none()
        );
    }
    #[test]
    fn canonical_order_dependencies_and_overrides_fail_closed() {
        let mut a = module(b"a", 1);
        let b = module(b"b", 2);
        a.metadata.requires.push(b"b".to_vec());
        assert_eq!(
            generation_module_order(&[a.clone(), b.clone()]),
            Some(vec![1, 0])
        );
        assert!(generation_module_order(&[b.clone(), a.clone()]).is_none());
        let mut cyclic = b.clone();
        cyclic.metadata.requires.push(b"a".to_vec());
        assert!(generation_module_order(&[a.clone(), cyclic]).is_none());
        let bytes = artifact_selection(&a.metadata, SelectionDisposition::Synchronous).unwrap();
        for overrides in [
            vec![(b"missing".to_vec(), bytes.clone())],
            vec![
                (b"a".to_vec(), bytes.clone()),
                (b"a".to_vec(), bytes.clone()),
            ],
        ] {
            assert!(
                generation_selection_with_overrides(
                    &[a.clone(), b.clone()],
                    &overrides,
                    SelectionDisposition::Synchronous
                )
                .is_none()
            );
        }
        assert!(
            generation_selection_with_overrides(
                &[a.clone(), a],
                &[],
                SelectionDisposition::Synchronous
            )
            .is_none()
        );
    }
    #[test]
    fn authority_uses_selected_descriptors_and_residual_effects_cannot_change() {
        let modules = vec![module(b"a", 1)];
        let bytes = generation_selection_with_overrides(
            &modules,
            &[],
            SelectionDisposition::ResumableRequired,
        )
        .unwrap();
        assert!(dispatch_generation_authority_with(&modules, &bytes, |_, _, _| None).is_none());
        let first =
            dispatch_generation_authority_with(&modules, &bytes, |_, _, _| Some([1; 32])).unwrap();
        let second =
            dispatch_generation_authority_with(&modules, &bytes, |_, _, _| Some([2; 32])).unwrap();
        assert_ne!(first, second);
        let mut next = module(b"a", 2);
        assert!(transition_allowed(&modules[0].metadata, &next.metadata));
        next.metadata.initializer_residual_digest = [b'f'; 64];
        assert!(!transition_allowed(&modules[0].metadata, &next.metadata));
        let selected =
            artifact_selection(&next.metadata, SelectionDisposition::ResumableRequired).unwrap();
        assert!(
            project_candidate_generation(modules, next, &bytes, &selected, |_, _, _| Some([1; 32]))
                .is_none()
        );
    }
    #[test]
    fn dispatch_base_v2_reproduces_the_frozen_non_circular_trace() {
        let revision_digest = *b"0b0261b15fce452514549a5369bb41624215eaa4e6469a3c370495bdffdaa8a1";
        let module = Module {
            name: b"trace".to_vec(),
            revision: decode_hex_digest(&revision_digest).unwrap(),
            source: b"run => self\nblock => [ self ]\n".to_vec(),
            metadata: RevisionMetadata {
                name: b"trace".to_vec(),
                requires: Vec::new(),
                operations: vec![RevisionOperation {
                    target: b"Trace".to_vec(),
                    selector: b"run".to_vec(),
                    install: true,
                    source: b"run => self".to_vec(),
                }],
                non_method_digest: [b'0'; 64],
                initializer_residual_digest: [b'1'; 64],
                method_digest: [b'0'; 64],
                revision_digest,
                inventory: vec![
                    ImplementationKey {
                        root_kind: InventoryRootKind::MethodOperation,
                        root_index: 0,
                        path: vec![],
                        implementation_kind: InventoryImplementationKind::NamedAdapter,
                    },
                    ImplementationKey {
                        root_kind: InventoryRootKind::MethodOperation,
                        root_index: 0,
                        path: vec![0],
                        implementation_kind: InventoryImplementationKind::BlockBody,
                    },
                ],
            },
        };
        let descriptor =
            decode_hex_digest(b"d670335de8efd9c6db228e19a70affb3fe738e0864dee726dde273928a5171cf")
                .unwrap();
        let generation_selection = include_bytes!(
            "../../../abi/fixtures/stackless-s2/generation-selection-construction-trace.bin"
        );
        let authority = dispatch_generation_authority_with(
            &[module],
            generation_selection,
            |_module, index, _selection| (index == 0).then_some(descriptor),
        )
        .unwrap();
        assert_eq!(
            authority,
            decode_hex_digest(b"7d1a817173f4b47139581aaf2996009012ae398f1e251a3bfe5e75498c5f823c")
                .unwrap()
        );
    }
}
