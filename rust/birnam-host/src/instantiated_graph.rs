//! Dry-run receipt contract for a configuration-specific instantiated Zig graph.
//!
//! Source capture is useful drift evidence, but it is not semantic truth.
//! This module validates the receipt emitted by a future Zig Build extractor
//! after options, target, profile, floor, and providers are instantiated. It
//! intentionally has no process-execution capability.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const INSTANTIATED_GRAPH_SCHEMA: u32 = 1;
pub const INSTANTIATED_GRAPH_SCOPE: &str = "instantiated-dry-run-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstantiationIdentity {
    pub target: String,
    pub profile: String,
    pub floor: String,
    pub distribution: String,
    pub provider_selection_sha256: String,
    pub host_options_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstantiatedGraphReceipt {
    pub schema: u32,
    pub semantic_scope: String,
    pub identity: InstantiationIdentity,
    pub build_file: PathRef,
    pub actions: Vec<InstantiatedAction>,
    pub edges: Vec<InstantiatedEdge>,
    pub raw_occurrences: Vec<RawDependencyOccurrence>,
    pub roots: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathRef {
    pub path: PathBuf,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstantiatedAction {
    pub name: String,
    pub depends_on: Vec<String>,
    pub tool: String,
    pub tool_sha256: String,
    pub argv_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstantiatedEdge {
    pub source: String,
    pub dependency: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawDependencyOccurrence {
    pub occurrence: usize,
    pub source_expression: String,
    pub dependency_expression: String,
    pub source: String,
    pub dependency: String,
    pub edge: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstantiatedGraphError(pub String);

impl fmt::Display for InstantiatedGraphError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for InstantiatedGraphError {}

fn error(message: impl Into<String>) -> InstantiatedGraphError {
    InstantiatedGraphError(message.into())
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn safe_name(value: &str, label: &str) -> Result<(), InstantiatedGraphError> {
    if value.is_empty()
        || value.contains('/')
        || value.contains('\\')
        || value == "."
        || value == ".."
        || value.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err(error(format!("{label} is not a safe name: {value:?}")));
    }
    Ok(())
}

fn safe_relative_path(path: &Path, label: &str) -> Result<(), InstantiatedGraphError> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(error(format!("{label} must be a relative path")));
    }
    for component in path.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(error(format!(
                "{label} escapes the receipt root: {path:?}"
            )));
        }
    }
    Ok(())
}

fn contained_build_file(root: &Path, path: &Path) -> Result<PathBuf, InstantiatedGraphError> {
    safe_relative_path(path, "build_file")?;
    let mut cursor = root.to_path_buf();
    for component in path.components() {
        let Component::Normal(part) = component else {
            return Err(error("build_file contains a non-normal path component"));
        };
        cursor.push(part);
        let metadata = fs::symlink_metadata(&cursor).map_err(|source| {
            error(format!(
                "build_file is unavailable at {}: {source}",
                cursor.display()
            ))
        })?;
        if metadata.file_type().is_symlink() {
            return Err(error(format!(
                "build_file contains a symlink: {}",
                cursor.display()
            )));
        }
    }
    let canonical = cursor
        .canonicalize()
        .map_err(|source| error(format!("build_file cannot be canonicalized: {source}")))?;
    if !canonical.starts_with(root) {
        return Err(error("build_file escapes the receipt root"));
    }
    Ok(canonical)
}

fn sha256_file(path: &Path) -> Result<String, InstantiatedGraphError> {
    let bytes = fs::read(path)
        .map_err(|source| error(format!("cannot read {}: {source}", path.display())))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn validate_identity(
    identity: &InstantiationIdentity,
    expected: &InstantiationIdentity,
) -> Result<(), InstantiatedGraphError> {
    for (label, value) in [
        ("target", identity.target.as_str()),
        ("profile", identity.profile.as_str()),
        ("floor", identity.floor.as_str()),
        ("distribution", identity.distribution.as_str()),
    ] {
        safe_name(value, label)?;
    }
    if identity.floor != "rust" && identity.floor != "zig" {
        return Err(error(format!("unknown floor selection: {}", identity.floor)));
    }
    if identity.distribution != "production"
        && identity.distribution != "rollback"
        && identity.distribution != "shadow"
        && identity.distribution != "test_bundle"
    {
        return Err(error(format!(
            "unknown distribution: {}",
            identity.distribution
        )));
    }
    if !valid_digest(&identity.provider_selection_sha256)
        || !valid_digest(&identity.host_options_sha256)
    {
        return Err(error("instantiation identity digest is malformed"));
    }
    if identity != expected {
        return Err(error(
            "instantiated graph identity does not match the requested configuration",
        ));
    }
    Ok(())
}

/// Parse and validate a dry-run receipt against the requested configuration.
///
/// The result is data only. It does not authorize or execute any action.
pub fn parse_and_validate(
    bytes: &[u8],
    root: &Path,
    expected: &InstantiationIdentity,
) -> Result<InstantiatedGraphReceipt, InstantiatedGraphError> {
    let receipt: InstantiatedGraphReceipt = serde_json::from_slice(bytes)
        .map_err(|source| error(format!("instantiated graph receipt is invalid: {source}")))?;
    let root = root
        .canonicalize()
        .map_err(|source| error(format!("receipt root is unavailable: {source}")))?;
    if receipt.schema != INSTANTIATED_GRAPH_SCHEMA {
        return Err(error(format!(
            "unsupported instantiated graph schema: {}",
            receipt.schema
        )));
    }
    if receipt.semantic_scope != INSTANTIATED_GRAPH_SCOPE {
        return Err(error(format!(
            "receipt semantic_scope is not instantiated dry-run truth: {}",
            receipt.semantic_scope
        )));
    }
    validate_identity(&receipt.identity, expected)?;
    if !valid_digest(&receipt.build_file.sha256) {
        return Err(error("build_file has an invalid sha256 digest"));
    }
    let build_file = contained_build_file(&root, &receipt.build_file.path)?;
    if !build_file.is_file() || sha256_file(&build_file)? != receipt.build_file.sha256 {
        return Err(error("build_file digest does not match the receipt"));
    }

    let mut action_names = BTreeSet::new();
    for action in &receipt.actions {
        safe_name(&action.name, "action name")?;
        if !action_names.insert(action.name.clone()) {
            return Err(error(format!("duplicate instantiated action: {}", action.name)));
        }
        safe_name(&action.tool, "tool name")?;
        if !valid_digest(&action.tool_sha256) || !valid_digest(&action.argv_sha256) {
            return Err(error(format!(
                "instantiated action {} has a malformed tool or argv digest",
                action.name
            )));
        }
        let mut dependencies = BTreeSet::new();
        for dependency in &action.depends_on {
            safe_name(dependency, "dependency name")?;
            if !action_names.contains(dependency)
                && !receipt.actions.iter().any(|candidate| candidate.name == *dependency)
            {
                return Err(error(format!(
                    "instantiated action {} depends on undeclared action {}",
                    action.name, dependency
                )));
            }
            if !dependencies.insert(dependency) {
                return Err(error(format!(
                    "instantiated action {} repeats a dependency",
                    action.name
                )));
            }
        }
    }

    let mut edge_keys = BTreeSet::new();
    for edge in &receipt.edges {
        safe_name(&edge.source, "edge source")?;
        safe_name(&edge.dependency, "edge dependency")?;
        if edge.source == edge.dependency
            || !action_names.contains(&edge.source)
            || !action_names.contains(&edge.dependency)
        {
            return Err(error("instantiated edge is outside the action set"));
        }
        if !matches!(
            edge.kind.as_str(),
            "artifact" | "install" | "lazy" | "generated" | "source"
        ) {
            return Err(error(format!("unknown instantiated edge kind: {}", edge.kind)));
        }
        if !edge_keys.insert((edge.source.clone(), edge.dependency.clone())) {
            return Err(error("instantiated graph contains a duplicate edge"));
        }
    }

    for action in &receipt.actions {
        let declared = action.depends_on.iter().cloned().collect::<BTreeSet<_>>();
        let resolved = receipt
            .edges
            .iter()
            .filter(|edge| edge.source == action.name)
            .map(|edge| edge.dependency.clone())
            .collect::<BTreeSet<_>>();
        if declared != resolved {
            return Err(error(format!(
                "instantiated action {} dependencies disagree with edges",
                action.name
            )));
        }
    }

    let mut mapped_source_edges = BTreeSet::new();
    for (expected_occurrence, occurrence) in receipt.raw_occurrences.iter().enumerate() {
        if occurrence.occurrence != expected_occurrence {
            return Err(error("raw dependency occurrences must be contiguous and ordered"));
        }
        if occurrence.source_expression.trim().is_empty()
            || occurrence.dependency_expression.trim().is_empty()
        {
            return Err(error("raw dependency occurrence has an empty expression"));
        }
        let edge = receipt.edges.get(occurrence.edge).ok_or_else(|| {
            error(format!(
                "raw dependency occurrence {} references an unknown edge",
                occurrence.occurrence
            ))
        })?;
        if edge.source != occurrence.source || edge.dependency != occurrence.dependency {
            return Err(error(format!(
                "raw dependency occurrence {} does not match its instantiated edge",
                occurrence.occurrence
            )));
        }
        if edge.kind == "source" {
            mapped_source_edges.insert((edge.source.clone(), edge.dependency.clone()));
        }
    }
    let source_edges = receipt
        .edges
        .iter()
        .filter(|edge| edge.kind == "source")
        .map(|edge| (edge.source.clone(), edge.dependency.clone()))
        .collect::<BTreeSet<_>>();
    if mapped_source_edges != source_edges {
        return Err(error(
            "every source edge must be mapped to a raw dependOn occurrence",
        ));
    }

    let mut roots = BTreeSet::new();
    if receipt.roots.is_empty() {
        return Err(error("instantiated graph has no roots"));
    }
    for root_name in &receipt.roots {
        safe_name(root_name, "graph root")?;
        if !action_names.contains(root_name) || !roots.insert(root_name) {
            return Err(error("instantiated graph roots are invalid or duplicated"));
        }
    }
    let mut reachable = BTreeSet::new();
    let mut pending = receipt.roots.clone();
    while let Some(action) = pending.pop() {
        if !reachable.insert(action.clone()) {
            continue;
        }
        pending.extend(
            receipt
                .edges
                .iter()
                .filter(|edge| edge.source == action)
                .map(|edge| edge.dependency.clone()),
        );
    }
    if reachable != action_names {
        return Err(error(
            "instantiated graph roots do not reach the closed action set",
        ));
    }
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Fixture {
        root: PathBuf,
        bytes: Vec<u8>,
        expected: InstantiationIdentity,
    }

    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "birnam-instantiated-graph-{nonce}-{sequence}"
            ));
            fs::create_dir_all(&root).expect("fixture root");
            let build = b"compile.dependOn(prepare);";
            fs::write(root.join("build.zig"), build).expect("build source");
            let digest = hex::encode(Sha256::digest(build));
            let expected = InstantiationIdentity {
                target: "aarch64-macos".into(),
                profile: "release".into(),
                floor: "rust".into(),
                distribution: "shadow".into(),
                provider_selection_sha256: "a".repeat(64),
                host_options_sha256: "b".repeat(64),
            };
            let receipt = json!({
                "schema": INSTANTIATED_GRAPH_SCHEMA,
                "semantic_scope": INSTANTIATED_GRAPH_SCOPE,
                "identity": expected,
                "build_file": {"path": "build.zig", "sha256": digest},
                "actions": [
                    {"name": "prepare", "depends_on": [], "tool": "zig", "tool_sha256": "c".repeat(64), "argv_sha256": "d".repeat(64)},
                    {"name": "compile", "depends_on": ["prepare"], "tool": "zig", "tool_sha256": "c".repeat(64), "argv_sha256": "e".repeat(64)}
                ],
                "edges": [{"source": "compile", "dependency": "prepare", "kind": "source"}],
                "raw_occurrences": [{"occurrence": 0, "source_expression": "compile", "dependency_expression": "prepare", "source": "compile", "dependency": "prepare", "edge": 0}],
                "roots": ["compile"]
            });
            Self {
                root,
                bytes: serde_json::to_vec(&receipt).expect("receipt JSON"),
                expected,
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn mutate(fixture: &Fixture, update: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
        let mut value: serde_json::Value = serde_json::from_slice(&fixture.bytes).expect("JSON");
        update(&mut value);
        serde_json::to_vec(&value).expect("JSON")
    }

    #[test]
    fn accepts_configuration_bound_receipt() {
        let fixture = Fixture::new();
        let receipt = parse_and_validate(&fixture.bytes, &fixture.root, &fixture.expected)
            .expect("valid instantiated receipt");
        assert_eq!(receipt.actions.len(), 2);
        assert_eq!(receipt.raw_occurrences[0].edge, 0);
    }

    #[test]
    fn retains_duplicate_raw_occurrences_when_they_map_to_one_edge() {
        let fixture = Fixture::new();
        let bytes = mutate(&fixture, |value| {
            let first = value["raw_occurrences"][0].clone();
            value["raw_occurrences"] = json!([
                {"occurrence": 0, "source_expression": "compile", "dependency_expression": "prepare", "source": "compile", "dependency": "prepare", "edge": 0},
                {"occurrence": 1, "source_expression": "compile", "dependency_expression": "prepare", "source": "compile", "dependency": "prepare", "edge": 0}
            ]);
            assert_eq!(first["edge"], value["raw_occurrences"][0]["edge"]);
        });
        let receipt = parse_and_validate(&bytes, &fixture.root, &fixture.expected)
            .expect("duplicate syntactic occurrence is retained");
        assert_eq!(receipt.raw_occurrences.len(), 2);
        assert_eq!(receipt.raw_occurrences[0].edge, receipt.raw_occurrences[1].edge);
    }

    #[test]
    fn rejects_configuration_mismatch() {
        let fixture = Fixture::new();
        let bytes = mutate(&fixture, |value| value["identity"]["profile"] = json!("debug"));
        let error = parse_and_validate(&bytes, &fixture.root, &fixture.expected)
            .expect_err("receipt from another profile must be rejected");
        assert!(error.0.contains("does not match the requested configuration"));
    }

    #[test]
    fn rejects_unmapped_or_mismatched_raw_edge() {
        let fixture = Fixture::new();
        let bytes = mutate(&fixture, |value| value["raw_occurrences"][0]["edge"] = json!(1));
        let error = parse_and_validate(&bytes, &fixture.root, &fixture.expected)
            .expect_err("raw mapping to a missing edge must be rejected");
        assert!(error.0.contains("unknown edge"));

        let bytes = mutate(&fixture, |value| {
            value["raw_occurrences"][0]["dependency"] = json!("other")
        });
        let error = parse_and_validate(&bytes, &fixture.root, &fixture.expected)
            .expect_err("raw mapping with the wrong edge identity must be rejected");
        assert!(error.0.contains("does not match"));
    }

    #[test]
    fn rejects_action_edge_disagreement_and_disconnected_root() {
        let fixture = Fixture::new();
        let bytes = mutate(&fixture, |value| value["actions"][1]["depends_on"] = json!([]));
        let error = parse_and_validate(&bytes, &fixture.root, &fixture.expected)
            .expect_err("action dependency set must agree with instantiated edges");
        assert!(error.0.contains("disagree with edges"));

        let bytes = mutate(&fixture, |value| value["roots"] = json!(["prepare"]));
        let error = parse_and_validate(&bytes, &fixture.root, &fixture.expected)
            .expect_err("roots must reach every action");
        assert!(error.0.contains("do not reach"));
    }
}
