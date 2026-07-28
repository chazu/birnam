//! Dry-run receipt contract for a configuration-specific instantiated Zig graph.
//!
//! Source capture is useful drift evidence, but it is not semantic truth.
//! This module validates the receipt emitted by a future Zig Build extractor
//! after options, target, profile, floor, and providers are instantiated. It
//! intentionally has no process-execution capability.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const INSTANTIATED_GRAPH_SCHEMA: u32 = 3;
pub const INSTANTIATED_GRAPH_SCOPE: &str = "instantiated-dry-run-v3";

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
    pub source_manifest: PathRef,
    pub extractor: ExtractorBinding,
    pub build_file: PathRef,
    pub actions: Vec<InstantiatedAction>,
    pub edges: Vec<InstantiatedEdge>,
    pub raw_occurrences: Vec<RawDependencyOccurrence>,
    pub roots: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractorBinding {
    pub implementation: String,
    pub version: String,
    pub path: PathBuf,
    pub sha256: String,
    pub invocation: Vec<String>,
    pub invocation_sha256: String,
    pub source_tree_sha256: String,
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
    pub tool_path: PathBuf,
    pub tool_sha256: String,
    pub argv: Vec<String>,
    pub argv_sha256: String,
    pub cwd: PathBuf,
    pub inputs: Vec<InstantiatedArtifact>,
    pub outputs: Vec<InstantiatedArtifact>,
    pub install_paths: Vec<PathBuf>,
    pub environment_base: EnvironmentBase,
    pub environment: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstantiatedArtifact {
    pub kind: String,
    pub path: PathBuf,
    pub sha256: String,
    #[serde(default)]
    pub producer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentBase {
    Clear,
    Inherit,
}

impl EnvironmentBase {
    fn is_clear(&self) -> bool {
        matches!(self, Self::Clear)
    }
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
    pub edges: Vec<usize>,
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
            return Err(error(format!("{label} escapes the receipt root: {path:?}")));
        }
    }
    Ok(())
}

fn contained_file(
    root: &Path,
    path: &Path,
    label: &str,
) -> Result<PathBuf, InstantiatedGraphError> {
    safe_relative_path(path, label)?;
    let mut cursor = root.to_path_buf();
    for component in path.components() {
        let Component::Normal(part) = component else {
            return Err(error(format!(
                "{label} contains a non-normal path component"
            )));
        };
        cursor.push(part);
        let metadata = fs::symlink_metadata(&cursor).map_err(|source| {
            error(format!(
                "{label} is unavailable at {}: {source}",
                cursor.display()
            ))
        })?;
        if metadata.file_type().is_symlink() {
            return Err(error(format!(
                "{label} contains a symlink: {}",
                cursor.display()
            )));
        }
    }
    let canonical = cursor
        .canonicalize()
        .map_err(|source| error(format!("{label} cannot be canonicalized: {source}")))?;
    if !canonical.starts_with(root) {
        return Err(error(format!("{label} escapes the receipt root")));
    }
    Ok(canonical)
}

fn sha256_file(path: &Path) -> Result<String, InstantiatedGraphError> {
    let bytes = fs::read(path)
        .map_err(|source| error(format!("cannot read {}: {source}", path.display())))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn argv_sha256(argv: &[String]) -> Result<String, InstantiatedGraphError> {
    let bytes = serde_json::to_vec(argv)
        .map_err(|source| error(format!("cannot encode action argv: {source}")))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn safe_environment_name(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('=')
        && !value
            .bytes()
            .any(|byte| byte == 0 || byte.is_ascii_whitespace())
}

fn validate_artifact_shape(
    artifact: &InstantiatedArtifact,
    label: &str,
) -> Result<(), InstantiatedGraphError> {
    if !matches!(
        artifact.kind.as_str(),
        "external" | "produced" | "generated" | "install"
    ) {
        return Err(error(format!(
            "{label} has an unknown artifact kind: {}",
            artifact.kind
        )));
    }
    safe_relative_path(&artifact.path, label)?;
    if !valid_digest(&artifact.sha256) {
        return Err(error(format!("{label} has an invalid sha256 digest")));
    }
    match artifact.kind.as_str() {
        "external" if artifact.producer.is_some() => {
            return Err(error(format!("{label} external artifact has a producer")))
        }
        "produced" | "generated" | "install" if artifact.producer.is_none() => {
            return Err(error(format!("{label} produced artifact has no producer")))
        }
        _ => {}
    }
    if let Some(producer) = &artifact.producer {
        safe_name(producer, &format!("{label} producer"))?;
    }
    Ok(())
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
        return Err(error(format!(
            "unknown floor selection: {}",
            identity.floor
        )));
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
    safe_name(
        &receipt.extractor.implementation,
        "extractor implementation",
    )?;
    safe_name(&receipt.extractor.version, "extractor version")?;
    if !valid_digest(&receipt.extractor.sha256)
        || !valid_digest(&receipt.extractor.invocation_sha256)
        || !valid_digest(&receipt.extractor.source_tree_sha256)
        || !valid_digest(&receipt.source_manifest.sha256)
    {
        return Err(error(
            "instantiated extractor or source digest is malformed",
        ));
    }
    let source_manifest = contained_file(&root, &receipt.source_manifest.path, "source manifest")?;
    if !source_manifest.is_file()
        || sha256_file(&source_manifest)? != receipt.source_manifest.sha256
    {
        return Err(error("source manifest digest does not match the receipt"));
    }
    if receipt.extractor.source_tree_sha256 != sha256_file(&source_manifest)? {
        return Err(error(
            "extractor source tree digest does not match the manifest",
        ));
    }
    let extractor_path = contained_file(&root, &receipt.extractor.path, "extractor path")?;
    if !extractor_path.is_file() || sha256_file(&extractor_path)? != receipt.extractor.sha256 {
        return Err(error("extractor implementation digest does not match"));
    }
    if receipt.extractor.invocation.is_empty()
        || receipt
            .extractor
            .invocation
            .iter()
            .any(|part| part.is_empty())
        || receipt
            .extractor
            .invocation
            .iter()
            .any(|part| part.contains('\0'))
    {
        return Err(error("extractor invocation has an empty or NUL component"));
    }
    let extractor_name = extractor_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| error("extractor path has no UTF-8 file name"))?;
    if receipt.extractor.invocation.first().map(String::as_str) != Some(extractor_name) {
        return Err(error("extractor invocation does not bind its executable"));
    }
    if argv_sha256(&receipt.extractor.invocation)? != receipt.extractor.invocation_sha256 {
        return Err(error("extractor invocation digest does not match"));
    }
    if !valid_digest(&receipt.build_file.sha256) {
        return Err(error("build_file has an invalid sha256 digest"));
    }
    let build_file = contained_file(&root, &receipt.build_file.path, "build_file")?;
    if !build_file.is_file() || sha256_file(&build_file)? != receipt.build_file.sha256 {
        return Err(error("build_file digest does not match the receipt"));
    }

    let mut action_names = BTreeSet::new();
    for action in &receipt.actions {
        safe_name(&action.name, "action name")?;
        if !action_names.insert(action.name.clone()) {
            return Err(error(format!(
                "duplicate instantiated action: {}",
                action.name
            )));
        }
        safe_name(&action.tool, "tool name")?;
        if !valid_digest(&action.tool_sha256) || !valid_digest(&action.argv_sha256) {
            return Err(error(format!(
                "instantiated action {} has a malformed tool or argv digest",
                action.name
            )));
        }
        let tool_path = contained_file(&root, &action.tool_path, "action tool path")?;
        if !tool_path.is_file() || sha256_file(&tool_path)? != action.tool_sha256 {
            return Err(error(format!(
                "instantiated action {} tool digest does not match",
                action.name
            )));
        }
        if action.argv.is_empty() || action.argv.iter().any(|part| part.is_empty()) {
            return Err(error(format!(
                "instantiated action {} has an empty argv component",
                action.name
            )));
        }
        if action.argv.first() != Some(&action.tool)
            || action.argv.iter().any(|part| part.contains('\0'))
        {
            return Err(error(format!(
                "instantiated action {} argv[0] or NUL closure is invalid",
                action.name
            )));
        }
        if argv_sha256(&action.argv)? != action.argv_sha256 {
            return Err(error(format!(
                "instantiated action {} argv digest does not match",
                action.name
            )));
        }
        let cwd = contained_file(&root, &action.cwd, "action cwd")?;
        if !cwd.is_dir() {
            return Err(error(format!(
                "instantiated action {} cwd is not a directory",
                action.name
            )));
        }
        if receipt.identity.distribution == "production" && !action.environment_base.is_clear() {
            return Err(error(format!(
                "production action {} does not clear its environment",
                action.name
            )));
        }
        for (name, value) in &action.environment {
            if !safe_environment_name(name) || value.contains('\0') {
                return Err(error(format!(
                    "instantiated action {} has an invalid environment entry",
                    action.name
                )));
            }
        }
        for (index, input) in action.inputs.iter().enumerate() {
            let label = format!("action {} input {index}", action.name);
            validate_artifact_shape(input, &label)?;
            if input.kind == "external" {
                let path = contained_file(&root, &input.path, &label)?;
                if !path.is_file() || sha256_file(&path)? != input.sha256 {
                    return Err(error(format!("{label} digest does not match")));
                }
            }
        }
        let mut output_paths = BTreeSet::new();
        for (index, output) in action.outputs.iter().enumerate() {
            let label = format!("action {} output {index}", action.name);
            validate_artifact_shape(output, &label)?;
            if output.producer.as_deref() != Some(action.name.as_str()) {
                return Err(error(format!("{label} producer must be the owning action")));
            }
            if !output_paths.insert(output.path.clone()) {
                return Err(error(format!("{label} path is duplicated")));
            }
        }
        let mut install_paths = BTreeSet::new();
        for path in &action.install_paths {
            safe_relative_path(path, "install path")?;
            if !install_paths.insert(path) || !output_paths.contains(path) {
                return Err(error(format!(
                    "action {} install path is not a unique declared output",
                    action.name
                )));
            }
        }
        let mut dependencies = BTreeSet::new();
        for dependency in &action.depends_on {
            safe_name(dependency, "dependency name")?;
            if !action_names.contains(dependency)
                && !receipt
                    .actions
                    .iter()
                    .any(|candidate| candidate.name == *dependency)
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

    let mut output_owners = BTreeMap::new();
    for action in &receipt.actions {
        for input in &action.inputs {
            if let Some(producer) = &input.producer {
                if !action_names.contains(producer) {
                    return Err(error(format!(
                        "action {} input names an unknown producer {}",
                        action.name, producer
                    )));
                }
            }
        }
        for output in &action.outputs {
            if let Some(previous) = output_owners.insert(output.path.clone(), &action.name) {
                return Err(error(format!(
                    "instantiated output {} is owned by both {} and {}",
                    output.path.display(),
                    previous,
                    action.name
                )));
            }
        }
    }
    let mut output_paths = output_owners.keys().cloned().collect::<Vec<_>>();
    output_paths.sort();
    for pair in output_paths.windows(2) {
        if pair[1].starts_with(&pair[0]) {
            return Err(error(format!(
                "instantiated output paths overlap: {} and {}",
                pair[0].display(),
                pair[1].display()
            )));
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
            return Err(error(format!(
                "unknown instantiated edge kind: {}",
                edge.kind
            )));
        }
        if !edge_keys.insert((edge.source.clone(), edge.dependency.clone())) {
            return Err(error("instantiated graph contains a duplicate edge"));
        }
    }

    let action_by_name = receipt
        .actions
        .iter()
        .map(|action| (action.name.as_str(), action))
        .collect::<BTreeMap<_, _>>();
    fn visit<'a>(
        name: &'a str,
        actions: &BTreeMap<&'a str, &'a InstantiatedAction>,
        state: &mut BTreeMap<&'a str, u8>,
    ) -> Result<(), InstantiatedGraphError> {
        match state.get(name).copied() {
            Some(1) => {
                return Err(error(format!(
                    "instantiated graph contains a cycle at {name}"
                )))
            }
            Some(2) => return Ok(()),
            _ => {}
        }
        state.insert(name, 1);
        for dependency in &actions
            .get(name)
            .expect("action names were collected")
            .depends_on
        {
            visit(dependency, actions, state)?;
        }
        state.insert(name, 2);
        Ok(())
    }
    let mut state = BTreeMap::new();
    for name in action_by_name.keys().copied() {
        visit(name, &action_by_name, &mut state)?;
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
            return Err(error(
                "raw dependency occurrences must be contiguous and ordered",
            ));
        }
        if occurrence.source_expression.trim().is_empty()
            || occurrence.dependency_expression.trim().is_empty()
        {
            return Err(error("raw dependency occurrence has an empty expression"));
        }
        let mut occurrence_edges = BTreeSet::new();
        for edge_index in &occurrence.edges {
            if !occurrence_edges.insert(*edge_index) {
                return Err(error(format!(
                    "raw dependency occurrence {} repeats an edge",
                    occurrence.occurrence
                )));
            }
            let edge = receipt.edges.get(*edge_index).ok_or_else(|| {
                error(format!(
                    "raw dependency occurrence {} references an unknown edge",
                    occurrence.occurrence
                ))
            })?;
            if edge.kind == "source" {
                mapped_source_edges.insert(*edge_index);
            }
        }
    }
    let source_edges = receipt
        .edges
        .iter()
        .enumerate()
        .filter(|(_, edge)| edge.kind == "source")
        .map(|(index, _)| index)
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
            let root =
                std::env::temp_dir().join(format!("birnam-instantiated-graph-{nonce}-{sequence}"));
            fs::create_dir_all(&root).expect("fixture root");
            let build = b"compile.dependOn(prepare);";
            fs::write(root.join("build.zig"), build).expect("build source");
            let digest = hex::encode(Sha256::digest(build));
            fs::write(root.join("zig"), b"zig-tool").expect("tool");
            let source_manifest = b"source-tree-manifest";
            fs::write(root.join("source-manifest.json"), source_manifest).expect("source manifest");
            fs::create_dir_all(root.join("work")).expect("work directory");
            let tool_digest = hex::encode(Sha256::digest(b"zig-tool"));
            let prepare_argv = vec!["zig".to_string(), "prepare".to_string()];
            let compile_argv = vec!["zig".to_string(), "compile".to_string()];
            let extractor_argv = vec![
                "zig".to_string(),
                "build".to_string(),
                "inspect-instantiated".to_string(),
            ];
            let prepare_argv_digest = hex::encode(Sha256::digest(
                serde_json::to_vec(&prepare_argv).expect("prepare argv"),
            ));
            let compile_argv_digest = hex::encode(Sha256::digest(
                serde_json::to_vec(&compile_argv).expect("compile argv"),
            ));
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
                "source_manifest": {
                    "path": "source-manifest.json",
                    "sha256": hex::encode(Sha256::digest(source_manifest))
                },
                "extractor": {
                    "implementation": "zig-build-extractor",
                    "version": "2026-07-28",
                    "path": "zig",
                    "sha256": tool_digest,
                    "invocation": extractor_argv,
                    "invocation_sha256": hex::encode(Sha256::digest(
                        serde_json::to_vec(&vec![
                            "zig".to_string(),
                            "build".to_string(),
                            "inspect-instantiated".to_string()
                        ]).expect("extractor argv")
                    )),
                    "source_tree_sha256": hex::encode(Sha256::digest(source_manifest))
                },
                "build_file": {"path": "build.zig", "sha256": digest},
                "actions": [
                    {
                        "name": "prepare", "depends_on": [], "tool": "zig",
                        "tool_path": "zig", "tool_sha256": tool_digest,
                        "argv": prepare_argv, "argv_sha256": prepare_argv_digest,
                        "cwd": "work", "inputs": [{"kind": "external", "path": "build.zig", "sha256": digest}],
                        "outputs": [{"kind": "generated", "path": "out/prepare", "sha256": "e".repeat(64), "producer": "prepare"}],
                        "install_paths": ["out/prepare"], "environment_base": "clear",
                        "environment": {"BIRNAM_TARGET": "aarch64-macos"}
                    },
                    {
                        "name": "compile", "depends_on": ["prepare"], "tool": "zig",
                        "tool_path": "zig", "tool_sha256": tool_digest,
                        "argv": compile_argv, "argv_sha256": compile_argv_digest,
                        "cwd": "work", "inputs": [{"kind": "produced", "path": "out/prepare", "sha256": "e".repeat(64), "producer": "prepare"}],
                        "outputs": [{"kind": "install", "path": "out/module.dylib", "sha256": "f".repeat(64), "producer": "compile"}],
                        "install_paths": ["out/module.dylib"], "environment_base": "clear",
                        "environment": {"BIRNAM_TARGET": "aarch64-macos"}
                    }
                ],
                "edges": [{"source": "compile", "dependency": "prepare", "kind": "source"}],
                "raw_occurrences": [{"occurrence": 0, "source_expression": "compile", "dependency_expression": "prepare", "edges": [0]}],
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
        assert_eq!(receipt.raw_occurrences[0].edges, [0]);
    }

    #[test]
    fn retains_duplicate_raw_occurrences_when_they_map_to_one_edge() {
        let fixture = Fixture::new();
        let bytes = mutate(&fixture, |value| {
            let first = value["raw_occurrences"][0].clone();
            value["raw_occurrences"] = json!([
                {"occurrence": 0, "source_expression": "compile", "dependency_expression": "prepare", "edges": [0]},
                {"occurrence": 1, "source_expression": "compile", "dependency_expression": "prepare", "edges": [0]}
            ]);
            assert_eq!(first["edges"], value["raw_occurrences"][0]["edges"]);
        });
        let receipt = parse_and_validate(&bytes, &fixture.root, &fixture.expected)
            .expect("duplicate syntactic occurrence is retained");
        assert_eq!(receipt.raw_occurrences.len(), 2);
        assert_eq!(
            receipt.raw_occurrences[0].edges,
            receipt.raw_occurrences[1].edges
        );
    }

    #[test]
    fn accepts_one_raw_occurrence_expanding_to_many_edges() {
        let fixture = Fixture::new();
        let bytes = mutate(&fixture, |value| {
            let mut expanded = value["actions"][0].clone();
            expanded["name"] = json!("expanded");
            expanded["outputs"][0]["path"] = json!("out/expanded");
            expanded["outputs"][0]["producer"] = json!("expanded");
            expanded["install_paths"] = json!(["out/expanded"]);
            value["actions"]
                .as_array_mut()
                .expect("actions")
                .push(expanded);
            value["actions"][1]["depends_on"] = json!(["prepare", "expanded"]);
            value["edges"] = json!([
                {"source": "compile", "dependency": "prepare", "kind": "source"},
                {"source": "compile", "dependency": "expanded", "kind": "source"}
            ]);
            value["raw_occurrences"][0]["edges"] = json!([0, 1]);
        });
        let receipt = parse_and_validate(&bytes, &fixture.root, &fixture.expected)
            .expect("one raw occurrence may expand to many concrete edges");
        assert_eq!(receipt.raw_occurrences[0].edges, [0, 1]);
    }

    #[test]
    fn rejects_configuration_mismatch() {
        let fixture = Fixture::new();
        let bytes = mutate(&fixture, |value| {
            value["identity"]["profile"] = json!("debug")
        });
        let error = parse_and_validate(&bytes, &fixture.root, &fixture.expected)
            .expect_err("receipt from another profile must be rejected");
        assert!(error
            .0
            .contains("does not match the requested configuration"));

        let bytes = mutate(&fixture, |value| {
            value["extractor"]["invocation"][0] = json!("other-zig");
        });
        let error = parse_and_validate(&bytes, &fixture.root, &fixture.expected)
            .expect_err("extractor executable binding must be authenticated");
        assert!(error.0.contains("does not bind its executable"));
    }

    #[test]
    fn rejects_unmapped_or_mismatched_raw_edge() {
        let fixture = Fixture::new();
        let bytes = mutate(&fixture, |value| {
            value["raw_occurrences"][0]["edges"] = json!([1])
        });
        let error = parse_and_validate(&bytes, &fixture.root, &fixture.expected)
            .expect_err("raw mapping to a missing edge must be rejected");
        assert!(error.0.contains("unknown edge"));

        let bytes = mutate(&fixture, |value| {
            value["raw_occurrences"][0]["edges"] = json!([0, 0])
        });
        let error = parse_and_validate(&bytes, &fixture.root, &fixture.expected)
            .expect_err("raw mapping with a repeated edge must be rejected");
        assert!(error.0.contains("repeats an edge"));
    }

    #[test]
    fn rejects_action_edge_disagreement_and_disconnected_root() {
        let fixture = Fixture::new();
        let bytes = mutate(&fixture, |value| {
            value["actions"][1]["depends_on"] = json!([])
        });
        let error = parse_and_validate(&bytes, &fixture.root, &fixture.expected)
            .expect_err("action dependency set must agree with instantiated edges");
        assert!(error.0.contains("disagree with edges"));

        let bytes = mutate(&fixture, |value| value["roots"] = json!(["prepare"]));
        let error = parse_and_validate(&bytes, &fixture.root, &fixture.expected)
            .expect_err("roots must reach every action");
        assert!(error.0.contains("do not reach"));

        let bytes = mutate(&fixture, |value| {
            value["actions"][0]["depends_on"] = json!(["compile"]);
            value["edges"] = json!([
                {"source": "compile", "dependency": "prepare", "kind": "source"},
                {"source": "prepare", "dependency": "compile", "kind": "generated"}
            ]);
        });
        let error = parse_and_validate(&bytes, &fixture.root, &fixture.expected)
            .expect_err("cycles must be rejected even when roots reach every action");
        assert!(error.0.contains("contains a cycle"));
    }
}
