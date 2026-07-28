//! Strict M4.2 build-driver manifest parsing and dry-run validation.
//!
//! This module describes the Rust driver's input contract.  It is deliberately
//! independent of the legacy `build.zig` JSON receipt: M4.1 supplies a
//! named-target projection, while this schema is the executable graph that M4.2
//! must resolve before a compiler process can be started.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use serde::Deserialize;
use sha2::{Digest, Sha256};

pub const MANIFEST_SCHEMA: u32 = 1;
pub const EXECUTABLE_SEMANTIC_SCOPE: &str = "full-semantic-v1";
pub const CANONICAL_DYNAMIC_LIBRARIES: &[&str] = &[
    "libbirnam_id.dylib",
    "libbirnam_id_artifact.dylib",
    "libbirnam_store.dylib",
    "libbirnam_id_peg.dylib",
    "libbirnam_id_lang.dylib",
    "libbirnam_id_tcc.dylib",
    "libbirnam_id_codegen.dylib",
    "libbirnam_id_prelude.dylib",
    "libbirnam_id_collection.dylib",
    "libbirnam_id_error.dylib",
    "libbirnam_id_isolate.dylib",
    "libbirnam_id_relational.dylib",
    "libbirnam_id_alien.dylib",
    "libbirnam_id_image.dylib",
];

pub const STAGE1_MODULES: &[(&str, &str)] = &[
    ("src/id/modules.birnam", "modules"),
    ("src/id/stdlib.birnam", "stdlib"),
    ("src/id/compiled-unit.birnam", "compiledUnit"),
    ("src/id/toml.birnam", "toml"),
    ("src/id/config.birnam", "config"),
    ("src/id/app-config.birnam", "appConfig"),
    ("src/id/project-policy.birnam", "projectPolicy"),
    ("src/id/isolate.birnam", "isolate"),
    ("modules/html.birnam", "html"),
    ("modules/file.birnam", "file"),
    ("modules/json.birnam", "json"),
    ("modules/socket.birnam", "socket"),
    ("modules/time.birnam", "time"),
    ("modules/doc.birnam", "doc"),
    ("modules/zigdoc.birnam", "zigdoc"),
    ("modules/lsp.birnam", "lsp"),
    ("modules/repl.birnam", "repl"),
    ("modules/cli.birnam", "cli"),
    ("modules/http.birnam", "http"),
    ("modules/log.birnam", "log"),
    ("modules/markdown.birnam", "markdown"),
    ("modules/os.birnam", "os"),
    ("modules/process.birnam", "process"),
    ("modules/random.birnam", "random"),
    ("modules/test.birnam", "test"),
    ("modules/compiled-unit-docs.birnam", "compiledUnitDocs"),
];

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DriverManifest {
    pub schema: u32,
    pub semantic_scope: String,
    pub graph: GraphIdentity,
    pub source_graph: SourceGraph,
    pub target: String,
    pub profile: String,
    pub selection: Selection,
    pub modules: Vec<ModuleSpec>,
    pub actions: Vec<ActionSpec>,
    pub dynamic_libraries: Vec<String>,
    pub prefix: PathBuf,
    pub tools: BTreeMap<String, PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphIdentity {
    pub id: String,
    pub source_path: PathBuf,
    pub source_sha256: String,
    pub receipt_commit: String,
    pub receipt_tree: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceGraph {
    pub build_file: PathRef,
    pub depend_on_occurrences: usize,
    pub bindings: Vec<ExpressionBinding>,
    pub implicit_edges: Vec<ResolvedEdge>,
    pub roots: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpressionBinding {
    pub expression: String,
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedEdge {
    pub source: String,
    pub dependency: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub distribution: String,
    pub floor: String,
    pub providers: BTreeMap<String, String>,
    pub provider_actions: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleSpec {
    pub name: String,
    pub source: PathRef,
    pub producer: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionSpec {
    pub name: String,
    pub context: String,
    pub authority: String,
    pub delegation: String,
    pub tool: String,
    pub tool_sha256: String,
    pub cwd: PathBuf,
    pub command: Vec<String>,
    pub depends_on: Vec<String>,
    pub inputs: Vec<PathRef>,
    pub outputs: Vec<PathRef>,
    pub environment: BTreeMap<String, EnvironmentValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "mode", content = "value", deny_unknown_fields)]
pub enum EnvironmentValue {
    #[serde(rename = "inherit")]
    Inherit,
    #[serde(rename = "set")]
    Set(String),
    #[serde(rename = "unset")]
    Unset,
    #[serde(rename = "clear")]
    Clear,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathRef {
    pub path: PathBuf,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedManifest {
    pub manifest: DriverManifest,
    pub action_order: Vec<String>,
    pub root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestError(pub String);

impl fmt::Display for ManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ManifestError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionExecution {
    pub action: String,
    pub status: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

fn error(message: impl Into<String>) -> ManifestError {
    ManifestError(message.into())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZigDependencyEdge {
    pub source: String,
    pub dependency: String,
    pub source_line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZigDependencyCapture {
    pub occurrence_count: usize,
    pub edges: Vec<ZigDependencyEdge>,
}

fn normalize_expression(expression: &str) -> String {
    expression.split_whitespace().collect::<String>()
}

fn source_line(source: &str, offset: usize) -> usize {
    source[..offset]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

fn source_expression(source: &str, depend_on_offset: usize) -> Result<String, ManifestError> {
    let before = source[..depend_on_offset].trim_end();
    if before.ends_with(')') {
        let bytes = before.as_bytes();
        let mut depth = 0usize;
        let mut open = None;
        for index in (0..bytes.len()).rev() {
            match bytes[index] {
                b')' => depth += 1,
                b'(' if depth > 0 => {
                    depth -= 1;
                    if depth == 0 {
                        open = Some(index);
                        break;
                    }
                }
                _ => {}
            }
        }
        if let Some(open) = open {
            let mut start = open;
            while start > 0 && !matches!(bytes[start - 1], b';' | b'\n') {
                start -= 1;
            }
            let expression = normalize_expression(&before[start..]);
            if !expression.is_empty() {
                return Ok(expression);
            }
        }
    }
    let start = before
        .rfind(|character: char| {
            !(character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '&'))
        })
        .map_or(0, |index| index + 1);
    let expression = normalize_expression(&before[start..]);
    if expression.is_empty() {
        Err(error(format!(
            "cannot resolve .dependOn source at line {}",
            source_line(source, depend_on_offset)
        )))
    } else {
        Ok(expression)
    }
}

/// Capture every syntactic `.dependOn(...)` call without silently dropping
/// inline expressions or implicit install-step calls.
pub fn capture_zig_dependencies(source: &str) -> Result<ZigDependencyCapture, ManifestError> {
    const NEEDLE: &[u8] = b".dependOn";
    let bytes = source.as_bytes();
    let mut index = 0usize;
    let mut edges = Vec::new();
    while index < bytes.len() {
        if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'/') {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if bytes[index] == b'"' {
            index += 1;
            while index < bytes.len() {
                if bytes[index] == b'\\' {
                    index = (index + 2).min(bytes.len());
                } else if bytes[index] == b'"' {
                    index += 1;
                    break;
                } else {
                    index += 1;
                }
            }
            continue;
        }
        if bytes[index..].starts_with(NEEDLE) {
            let line = source_line(source, index);
            let source_expr = source_expression(source, index)?;
            let mut open = index + NEEDLE.len();
            while bytes.get(open).is_some_and(u8::is_ascii_whitespace) {
                open += 1;
            }
            if bytes.get(open) != Some(&b'(') {
                return Err(error(format!(
                    ".dependOn at line {line} has no argument list"
                )));
            }
            let mut cursor = open + 1;
            let mut depth = 1usize;
            let mut in_string = false;
            while cursor < bytes.len() && depth != 0 {
                match bytes[cursor] {
                    b'\\' if in_string => cursor = (cursor + 2).min(bytes.len()),
                    b'"' => {
                        in_string = !in_string;
                        cursor += 1;
                    }
                    b'(' if !in_string => {
                        depth += 1;
                        cursor += 1;
                    }
                    b')' if !in_string => {
                        depth -= 1;
                        cursor += 1;
                    }
                    _ => cursor += 1,
                }
            }
            if depth != 0 {
                return Err(error(format!(
                    ".dependOn at line {line} has unbalanced parentheses"
                )));
            }
            let argument = normalize_expression(&source[open + 1..cursor - 1]);
            if argument.is_empty() {
                return Err(error(format!(
                    ".dependOn at line {line} has an empty argument"
                )));
            }
            edges.push(ZigDependencyEdge {
                source: source_expr,
                dependency: argument,
                source_line: line,
            });
            index = cursor;
            continue;
        }
        index += 1;
    }
    Ok(ZigDependencyCapture {
        occurrence_count: edges.len(),
        edges,
    })
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_git_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn safe_name(value: &str, label: &str) -> Result<(), ManifestError> {
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

fn safe_relative_path(path: &Path, label: &str) -> Result<(), ManifestError> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(error(format!("{label} must be a relative path")));
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {
                return Err(error(format!(
                    "{label} escapes the manifest root: {path:?}"
                )));
            }
        }
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, ManifestError> {
    let bytes = fs::read(path)
        .map_err(|source| error(format!("cannot read input {}: {source}", path.display())))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn contained_existing_path(
    root: &Path,
    relative: &Path,
    label: &str,
) -> Result<PathBuf, ManifestError> {
    safe_relative_path(relative, label)?;
    let mut cursor = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(part) = component else {
            return Err(error(format!("{label} is not a normal relative path")));
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
        return Err(error(format!("{label} escapes the manifest root")));
    }
    Ok(canonical)
}

fn contained_output_path(root: &Path, relative: &Path, label: &str) -> Result<(), ManifestError> {
    safe_relative_path(relative, label)?;
    let mut cursor = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(part) = component else {
            return Err(error(format!("{label} is not a normal relative path")));
        };
        cursor.push(part);
        match fs::symlink_metadata(&cursor) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(error(format!(
                    "{label} contains a symlink: {}",
                    cursor.display()
                )));
            }
            Ok(_) => {}
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => break,
            Err(source) => {
                return Err(error(format!("{label} cannot be inspected: {source}")));
            }
        }
    }
    Ok(())
}

fn validate_path_ref(path_ref: &PathRef, root: &Path, label: &str) -> Result<(), ManifestError> {
    if !valid_digest(&path_ref.sha256) {
        return Err(error(format!("{label} has an invalid sha256 digest")));
    }
    let resolved = contained_existing_path(root, &path_ref.path, label)?;
    if !resolved.is_file() {
        return Err(error(format!(
            "{label} does not name an input file: {}",
            resolved.display()
        )));
    }
    let observed = sha256_file(&resolved)?;
    if observed != path_ref.sha256 {
        return Err(error(format!(
            "{label} digest mismatch for {}: manifest {}, observed {}",
            path_ref.path.display(),
            path_ref.sha256,
            observed
        )));
    }
    Ok(())
}

fn validate_selection(selection: &Selection) -> Result<(), ManifestError> {
    match selection.distribution.as_str() {
        "production" | "rollback" | "shadow" | "test_bundle" => {}
        other => return Err(error(format!("unknown distribution: {other}"))),
    }
    if selection.floor != "rust" && selection.floor != "zig" {
        return Err(error(format!(
            "unknown floor selection: {}",
            selection.floor
        )));
    }
    for (provider, implementation) in &selection.providers {
        safe_name(provider, "provider name")?;
        if implementation != "rust" && implementation != "zig" {
            return Err(error(format!(
                "unknown implementation for {provider}: {implementation}"
            )));
        }
    }
    let implementations = selection
        .providers
        .values()
        .chain(std::iter::once(&selection.floor))
        .collect::<BTreeSet<_>>();
    let expected_providers = CANONICAL_DYNAMIC_LIBRARIES
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let actual_providers = selection
        .providers
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if actual_providers != expected_providers {
        return Err(error(
            "provider selection does not cover the canonical 14-provider closure",
        ));
    }
    if selection.providers.get("libbirnam_id.dylib") != Some(&selection.floor) {
        return Err(error(
            "floor selection disagrees with the canonical id provider",
        ));
    }
    if selection.distribution == "production" && implementations.contains(&"zig".to_string()) {
        return Err(error("production manifest contains a Zig implementation"));
    }
    if selection.distribution == "rollback" && implementations.contains(&"rust".to_string()) {
        return Err(error("rollback manifest is not a complete Zig vector"));
    }
    Ok(())
}

fn validate_dependencies(actions: &[ActionSpec]) -> Result<Vec<String>, ManifestError> {
    let mut names = BTreeSet::new();
    for action in actions {
        safe_name(&action.name, "action name")?;
        if !names.insert(action.name.clone()) {
            return Err(error(format!("duplicate action name: {}", action.name)));
        }
        if !matches!(
            action.context.as_str(),
            "production" | "rollback" | "oracle" | "test_bundle" | "shadow"
        ) {
            return Err(error(format!("unknown action context: {}", action.context)));
        }
        if !matches!(action.authority.as_str(), "canonical" | "shadow" | "oracle") {
            return Err(error(format!(
                "unknown action authority: {}",
                action.authority
            )));
        }
        if !matches!(action.delegation.as_str(), "none" | "explicit-shadow") {
            return Err(error(format!(
                "unknown action delegation: {}",
                action.delegation
            )));
        }
        if action.context == "production"
            && (action.authority != "canonical" || action.delegation != "none")
        {
            return Err(error(format!(
                "production action {} is delegated or non-canonical",
                action.name
            )));
        }
        safe_relative_path(&action.cwd, "action cwd")?;
        if !valid_digest(&action.tool_sha256) {
            return Err(error(format!(
                "action {} has an invalid tool digest",
                action.name
            )));
        }
        if action.command.first() != Some(&action.tool) {
            return Err(error(format!(
                "action {} argv[0] is not its declared tool",
                action.name
            )));
        }
        if action.command.is_empty() || action.command.iter().any(|part| part.is_empty()) {
            return Err(error(format!(
                "action {} has an empty command component",
                action.name
            )));
        }
        for dependency in &action.depends_on {
            safe_name(dependency, "dependency name")?;
            if !names.contains(dependency)
                && !actions
                    .iter()
                    .any(|candidate| candidate.name == *dependency)
            {
                return Err(error(format!(
                    "action {} depends on undeclared action {}",
                    action.name, dependency
                )));
            }
        }
        if action.depends_on.len() != action.depends_on.iter().collect::<BTreeSet<_>>().len() {
            return Err(error(format!(
                "action {} repeats a dependency",
                action.name
            )));
        }
        for variable in action.environment.keys() {
            if variable.is_empty()
                || variable.contains('=')
                || variable
                    .bytes()
                    .any(|byte| byte == 0 || byte.is_ascii_whitespace())
            {
                return Err(error(format!(
                    "action {} has an invalid environment name",
                    action.name
                )));
            }
        }
        if action
            .environment
            .values()
            .any(|value| matches!(value, EnvironmentValue::Set(value) if value.contains('\0')))
        {
            return Err(error(format!(
                "action {} has a NUL environment value",
                action.name
            )));
        }
    }

    let by_name = actions
        .iter()
        .map(|action| (action.name.as_str(), action))
        .collect::<HashMap<_, _>>();
    let mut state = HashMap::<&str, u8>::new();
    let mut order = Vec::with_capacity(actions.len());
    fn visit<'a>(
        name: &'a str,
        by_name: &HashMap<&'a str, &'a ActionSpec>,
        state: &mut HashMap<&'a str, u8>,
        order: &mut Vec<String>,
    ) -> Result<(), ManifestError> {
        match state.get(name).copied() {
            Some(1) => return Err(error(format!("dependency cycle reaches action {name}"))),
            Some(2) => return Ok(()),
            _ => {}
        }
        let action = by_name
            .get(name)
            .ok_or_else(|| error(format!("missing action {name}")))?;
        state.insert(name, 1);
        for dependency in &action.depends_on {
            visit(dependency, by_name, state, order)?;
        }
        state.insert(name, 2);
        order.push(name.to_string());
        Ok(())
    }
    for action in actions {
        visit(action.name.as_str(), &by_name, &mut state, &mut order)?;
    }
    Ok(order)
}

/// Parse and validate a driver manifest against a checkout/prefix root.
pub fn parse_and_validate(bytes: &[u8], root: &Path) -> Result<ValidatedManifest, ManifestError> {
    let manifest: DriverManifest = serde_json::from_slice(bytes)
        .map_err(|source| error(format!("manifest JSON is invalid: {source}")))?;
    let root = root
        .canonicalize()
        .map_err(|source| error(format!("manifest root is unavailable: {source}")))?;
    if manifest.schema != MANIFEST_SCHEMA {
        return Err(error(format!(
            "unsupported manifest schema: {}",
            manifest.schema
        )));
    }
    if manifest.semantic_scope != EXECUTABLE_SEMANTIC_SCOPE {
        return Err(error(format!(
            "manifest semantic_scope is not executable: {}",
            manifest.semantic_scope
        )));
    }
    if manifest.graph.source_path != Path::new("abi/birnam-build-actions-v1.json") {
        return Err(error("graph source must be the M4.1 action manifest"));
    }
    if manifest.source_graph.build_file.path != Path::new("build.zig") {
        return Err(error("source graph build_file must be build.zig"));
    }
    safe_name(&manifest.target, "target")?;
    if !matches!(
        manifest.profile.as_str(),
        "debug" | "release" | "release-safe" | "release-fast" | "release-small"
    ) {
        return Err(error(format!("unsupported profile: {}", manifest.profile)));
    }
    safe_name(&manifest.graph.id, "graph id")?;
    safe_relative_path(&manifest.graph.source_path, "graph source")?;
    if !valid_digest(&manifest.graph.source_sha256) {
        return Err(error("graph source_sha256 is malformed"));
    }
    if !valid_git_id(&manifest.graph.receipt_commit) {
        return Err(error("graph receipt_commit is malformed"));
    }
    if !valid_git_id(&manifest.graph.receipt_tree) {
        return Err(error("graph receipt_tree is malformed"));
    }
    let graph_source = contained_existing_path(&root, &manifest.graph.source_path, "graph source")?;
    if !graph_source.is_file() {
        return Err(error(format!(
            "graph source is not a file: {}",
            graph_source.display()
        )));
    }
    let observed_graph_digest = sha256_file(&graph_source)?;
    if observed_graph_digest != manifest.graph.source_sha256 {
        return Err(error(format!(
            "graph source digest mismatch: manifest {}, observed {}",
            manifest.graph.source_sha256, observed_graph_digest
        )));
    }
    let graph_document: serde_json::Value = serde_json::from_slice(
        &fs::read(&graph_source)
            .map_err(|source| error(format!("cannot read graph source: {source}")))?,
    )
    .map_err(|source| error(format!("graph source JSON is invalid: {source}")))?;
    let transaction = graph_document
        .get("transaction")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| error("graph source has no transaction receipt"))?;
    let receipt_commit = transaction
        .get("receipt_commit")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| error("graph source transaction has no receipt_commit"))?;
    let receipt_tree = transaction
        .get("receipt_tree")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| error("graph source transaction has no receipt_tree"))?;
    if receipt_commit != manifest.graph.receipt_commit
        || receipt_tree != manifest.graph.receipt_tree
    {
        return Err(error(
            "graph receipt anchor disagrees with the M4.1 source receipt",
        ));
    }
    let receipt_source_digests = graph_document
        .get("source_digests")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| error("graph source has no source_digests"))?;
    let receipt_build_digest = receipt_source_digests
        .get("build.zig")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| error("graph source receipt has no build.zig digest"))?;
    if receipt_build_digest != manifest.source_graph.build_file.sha256 {
        return Err(error(
            "build.zig digest disagrees with the M4.1 source receipt",
        ));
    }
    validate_selection(&manifest.selection)?;
    contained_output_path(&root, &manifest.prefix, "output prefix")?;
    for (name, path) in &manifest.tools {
        safe_name(name, "tool name")?;
        let tool = contained_existing_path(&root, path, "tool path")?;
        if !tool.is_file() {
            return Err(error(format!("tool is unavailable: {}", tool.display())));
        }
    }

    let mut module_names = BTreeSet::new();
    for module in &manifest.modules {
        safe_name(&module.name, "module name")?;
        if !module_names.insert(module.name.clone()) {
            return Err(error(format!("duplicate module name: {}", module.name)));
        }
        validate_path_ref(&module.source, &root, "module source")?;
    }

    let action_names = manifest
        .actions
        .iter()
        .map(|action| action.name.as_str())
        .collect::<BTreeSet<_>>();
    for module in &manifest.modules {
        if !action_names.contains(module.producer.as_str()) {
            return Err(error(format!(
                "module {} names an unknown producer action {}",
                module.name, module.producer
            )));
        }
    }
    if manifest.modules.len() != STAGE1_MODULES.len() {
        return Err(error(format!(
            "stage-1 module closure requires exactly {} ordered modules",
            STAGE1_MODULES.len()
        )));
    }
    for ((expected_source, expected_name), module) in STAGE1_MODULES.iter().zip(&manifest.modules) {
        if module.name != *expected_name || module.source.path != Path::new(expected_source) {
            return Err(error(format!(
                "stage-1 module order/source mismatch at {}: expected {} ({expected_source})",
                module.name, expected_name
            )));
        }
        let producer = manifest
            .actions
            .iter()
            .find(|action| action.name == module.producer)
            .expect("module producer validated above");
        let expected_output = format!("libbirnam_{expected_name}.dylib");
        let output_count = producer
            .outputs
            .iter()
            .filter(|output| {
                output
                    .path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy() == expected_output)
            })
            .count();
        if output_count != 1 {
            return Err(error(format!(
                "stage-1 module {expected_name} producer {} must declare exactly one {expected_output} output",
                producer.name
            )));
        }
    }
    let expected_provider_actions = CANONICAL_DYNAMIC_LIBRARIES
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let actual_provider_actions = manifest
        .selection
        .provider_actions
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if actual_provider_actions != expected_provider_actions {
        return Err(error(
            "provider action map does not cover the canonical 14-provider closure",
        ));
    }
    for (library, producer) in &manifest.selection.provider_actions {
        if !action_names.contains(producer.as_str()) {
            return Err(error(format!(
                "provider {library} names an unknown producer action {producer}"
            )));
        }
    }
    if manifest.source_graph.roots.is_empty()
        || manifest
            .source_graph
            .roots
            .iter()
            .any(|root| !action_names.contains(root.as_str()))
    {
        return Err(error(
            "source graph roots are missing or outside the action set",
        ));
    }
    validate_path_ref(&manifest.source_graph.build_file, &root, "build source")?;
    let build_source_path = root.join(&manifest.source_graph.build_file.path);
    let build_source = fs::read_to_string(&build_source_path)
        .map_err(|source| error(format!("cannot read build source: {source}")))?;
    let captured = capture_zig_dependencies(&build_source)?;
    if captured.occurrence_count != manifest.source_graph.depend_on_occurrences {
        return Err(error(format!(
            "build source dependOn count mismatch: manifest {}, observed {}",
            manifest.source_graph.depend_on_occurrences, captured.occurrence_count
        )));
    }
    let mut bindings = BTreeMap::new();
    for binding in &manifest.source_graph.bindings {
        let expression = normalize_expression(&binding.expression);
        if expression.is_empty() || !action_names.contains(binding.action.as_str()) {
            return Err(error("source graph binding names an undeclared action"));
        }
        if bindings
            .insert(expression, binding.action.clone())
            .is_some()
        {
            return Err(error("source graph has duplicate expression bindings"));
        }
    }
    let captured_expressions = captured
        .edges
        .iter()
        .flat_map(|edge| [&edge.source, &edge.dependency])
        .cloned()
        .collect::<BTreeSet<_>>();
    if bindings.keys().cloned().collect::<BTreeSet<_>>() != captured_expressions {
        return Err(error(
            "source graph bindings must exactly cover captured .dependOn expressions",
        ));
    }
    let mut resolved_dependencies = action_names
        .iter()
        .map(|name| ((*name).to_string(), BTreeSet::new()))
        .collect::<BTreeMap<String, BTreeSet<String>>>();
    let mut captured_edge_keys = BTreeSet::new();
    for edge in &captured.edges {
        let source_action = bindings.get(&edge.source).ok_or_else(|| {
            error(format!(
                "unresolved .dependOn source expression at line {}: {}",
                edge.source_line, edge.source
            ))
        })?;
        let dependency_action = bindings.get(&edge.dependency).ok_or_else(|| {
            error(format!(
                "unresolved .dependOn dependency expression at line {}: {}",
                edge.source_line, edge.dependency
            ))
        })?;
        if source_action == dependency_action {
            return Err(error("source graph contains a self-dependency"));
        }
        captured_edge_keys.insert((source_action.clone(), dependency_action.clone()));
    }
    let mut resolved_edge_keys = BTreeSet::new();
    let mut declared_source_edge_keys = BTreeSet::new();
    for edge in &manifest.source_graph.implicit_edges {
        if !matches!(
            edge.kind.as_str(),
            "artifact" | "install" | "lazy" | "generated" | "source"
        ) {
            return Err(error(format!(
                "unknown implicit dependency kind: {}",
                edge.kind
            )));
        }
        if !action_names.contains(edge.source.as_str())
            || !action_names.contains(edge.dependency.as_str())
            || edge.source == edge.dependency
        {
            return Err(error(
                "implicit dependency is outside the closed action set",
            ));
        }
        if !resolved_edge_keys.insert((edge.source.clone(), edge.dependency.clone())) {
            return Err(error("source graph contains a duplicate implicit edge"));
        }
        if edge.kind == "source" {
            declared_source_edge_keys.insert((edge.source.clone(), edge.dependency.clone()));
        }
        resolved_dependencies
            .get_mut(&edge.source)
            .expect("source action checked above")
            .insert(edge.dependency.clone());
    }
    if declared_source_edge_keys != captured_edge_keys {
        return Err(error(
            "typed source edges must exactly match captured .dependOn edges",
        ));
    }
    for action in &manifest.actions {
        let declared = action.depends_on.iter().cloned().collect::<BTreeSet<_>>();
        let resolved = resolved_dependencies
            .get(&action.name)
            .expect("action names checked above");
        if &declared != resolved {
            return Err(error(format!(
                "action {} dependencies disagree with the resolved source graph",
                action.name
            )));
        }
    }
    let mut reachable = BTreeSet::new();
    let mut pending = manifest.source_graph.roots.to_vec();
    if pending.len()
        != manifest
            .source_graph
            .roots
            .iter()
            .collect::<BTreeSet<_>>()
            .len()
    {
        return Err(error("source graph roots contain duplicates"));
    }
    while let Some(action) = pending.pop() {
        if !reachable.insert(action.clone()) {
            continue;
        }
        pending.extend(
            resolved_dependencies
                .get(&action)
                .expect("root action checked above")
                .iter()
                .cloned(),
        );
    }
    if reachable
        != action_names
            .iter()
            .map(|name| (*name).to_string())
            .collect()
    {
        return Err(error(
            "source graph roots do not reach the closed action set",
        ));
    }
    let action_order = validate_dependencies(&manifest.actions)?;
    for action in &manifest.actions {
        if action.delegation == "explicit-shadow" && action.authority != "shadow" {
            return Err(error(format!(
                "action {} uses explicit-shadow delegation without shadow authority",
                action.name
            )));
        }
        if manifest.selection.distribution == "production" && action.context != "production" {
            return Err(error(format!(
                "production selection contains non-production action {}",
                action.name
            )));
        }
        if manifest.selection.distribution == "rollback" && action.context != "rollback" {
            return Err(error(format!(
                "rollback selection contains non-rollback action {}",
                action.name
            )));
        }
        if !manifest.tools.contains_key(&action.tool) {
            return Err(error(format!(
                "action {} references undeclared tool {}",
                action.name, action.tool
            )));
        }
        contained_output_path(&root, &action.cwd, "action cwd")?;
        let tool_path = contained_existing_path(
            &root,
            manifest
                .tools
                .get(&action.tool)
                .expect("tool checked above"),
            "tool path",
        )?;
        if sha256_file(&tool_path)? != action.tool_sha256 {
            return Err(error(format!(
                "action {} tool identity digest is stale",
                action.name
            )));
        }
        if action.context == "production" {
            let forbidden = |value: &str| {
                value.contains("birnam-zig-oracle")
                    || value.contains("_zig.dylib")
                    || value.contains("_zig_")
            };
            if forbidden(&action.tool)
                || forbidden(&tool_path.to_string_lossy())
                || forbidden(&action.cwd.to_string_lossy())
                || action.command.iter().any(|part| forbidden(part))
                || action
                    .inputs
                    .iter()
                    .any(|input| forbidden(&input.path.to_string_lossy()))
                || action
                    .outputs
                    .iter()
                    .any(|output| forbidden(&output.path.to_string_lossy()))
                || action.environment.iter().any(|(name, value)| {
                    forbidden(name)
                        || matches!(value, EnvironmentValue::Set(value) if forbidden(value))
                })
            {
                return Err(error(format!(
                    "production action {} reaches a Zig-only path",
                    action.name
                )));
            }
            if action
                .environment
                .values()
                .any(|value| matches!(value, EnvironmentValue::Inherit))
            {
                return Err(error(format!(
                    "production action {} inherits an unspecified environment",
                    action.name
                )));
            }
        }
        for (index, input) in action.inputs.iter().enumerate() {
            validate_path_ref(
                input,
                &root,
                &format!("action {} input {index}", action.name),
            )?;
        }
        for (index, output) in action.outputs.iter().enumerate() {
            let output_label = format!("action {} output {index}", action.name);
            contained_output_path(&root, &output.path, &output_label)?;
            if !output.path.starts_with(&manifest.prefix) {
                return Err(error(format!(
                    "{output_label} is outside the output prefix"
                )));
            }
            if !valid_digest(&output.sha256) {
                return Err(error(format!(
                    "action {} output {index} has an invalid digest",
                    action.name
                )));
            }
        }
        if action
            .depends_on
            .iter()
            .any(|dependency| !action_names.contains(dependency.as_str()))
        {
            return Err(error(format!(
                "action {} has an undeclared dependency",
                action.name
            )));
        }
    }

    let mut output_owners = BTreeMap::new();
    let mut input_paths = BTreeSet::new();
    for action in &manifest.actions {
        input_paths.extend(action.inputs.iter().map(|input| input.path.clone()));
    }
    for action in &manifest.actions {
        for output in &action.outputs {
            if input_paths.contains(&output.path) {
                return Err(error(format!(
                    "output {} overwrites a declared input",
                    output.path.display()
                )));
            }
            if let Some(previous) = output_owners.insert(output.path.clone(), &action.name) {
                return Err(error(format!(
                    "output {} is owned by both {} and {}",
                    output.path.display(),
                    previous,
                    action.name
                )));
            }
        }
    }
    let mut owned_paths = output_owners.keys().cloned().collect::<Vec<_>>();
    owned_paths.sort();
    for pair in owned_paths.windows(2) {
        if pair[1].starts_with(&pair[0]) {
            return Err(error(format!(
                "output paths overlap hierarchically: {} and {}",
                pair[0].display(),
                pair[1].display()
            )));
        }
    }

    let mut dynamic_names = BTreeSet::new();
    for library in &manifest.dynamic_libraries {
        if !library.ends_with(".dylib") || library.contains('/') || library.contains('\\') {
            return Err(error(format!(
                "dynamic library is not a bare dylib name: {library}"
            )));
        }
        if manifest.selection.distribution == "production" && library.contains("_zig") {
            return Err(error(format!(
                "production manifest names a Zig dylib: {library}"
            )));
        }
        if !dynamic_names.insert(library) {
            return Err(error(format!("duplicate dynamic library: {library}")));
        }
    }
    let expected_dynamic = CANONICAL_DYNAMIC_LIBRARIES
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let actual_dynamic = dynamic_names
        .iter()
        .map(|name| name.as_str())
        .collect::<BTreeSet<_>>();
    if actual_dynamic != expected_dynamic {
        return Err(error(
            "dynamic library set does not match the canonical 14-provider closure",
        ));
    }
    for library in CANONICAL_DYNAMIC_LIBRARIES {
        let implementation = manifest
            .selection
            .providers
            .get(*library)
            .expect("canonical provider validated above");
        let producer_name = manifest
            .selection
            .provider_actions
            .get(*library)
            .expect("provider action map validated above");
        let expected_output = if implementation == "zig" {
            format!(
                "{}_zig.dylib",
                library
                    .strip_suffix(".dylib")
                    .expect("canonical provider has dylib suffix")
            )
        } else {
            (*library).to_string()
        };
        let producer = manifest
            .actions
            .iter()
            .find(|action| action.name == *producer_name)
            .expect("provider producer action validated above");
        let output_count = producer
            .outputs
            .iter()
            .filter(|output| {
                output
                    .path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy() == expected_output)
            })
            .count();
        if output_count != 1 {
            return Err(error(format!(
                "provider {library} producer {} must declare exactly one {expected_output} output",
                producer.name
            )));
        }
    }

    Ok(ValidatedManifest {
        manifest,
        action_order,
        root,
    })
}

/// Execute a validated shadow/test graph in topological order.
///
/// This is deliberately unavailable to production and rollback selections.
/// M4.3 may exercise declared non-Birnam tools in shadow mode, but production
/// authority waits for the complete semantic graph and provenance receipt.
pub fn execute_shadow_graph(
    validated: &ValidatedManifest,
) -> Result<Vec<ActionExecution>, ManifestError> {
    if !matches!(
        validated.manifest.selection.distribution.as_str(),
        "shadow" | "test_bundle"
    ) {
        return Err(error(
            "shadow executor refuses production and rollback distributions",
        ));
    }
    if validated.manifest.actions.iter().any(|action| {
        action.context != "shadow"
            || action.authority != "shadow"
            || action.delegation != "explicit-shadow"
    }) {
        return Err(error(
            "shadow executor requires every action to use shadow context, authority, and explicit-shadow delegation",
        ));
    }
    let mut executions = Vec::with_capacity(validated.action_order.len());
    for action_name in &validated.action_order {
        let action = validated
            .manifest
            .actions
            .iter()
            .find(|candidate| candidate.name == *action_name)
            .expect("validated action order names an action");
        let tool_path = contained_existing_path(
            &validated.root,
            validated
                .manifest
                .tools
                .get(&action.tool)
                .expect("validated action tool exists"),
            "tool path",
        )?;
        if sha256_file(&tool_path)? != action.tool_sha256 {
            return Err(error(format!(
                "action {} tool identity changed before shadow execution",
                action.name
            )));
        }
        for (index, input) in action.inputs.iter().enumerate() {
            validate_path_ref(
                input,
                &validated.root,
                &format!("action {} input {index}", action.name),
            )?;
        }
        let mut command = Command::new(tool_path);
        command.args(action.command.iter().skip(1));
        command.current_dir(validated.root.join(&action.cwd));
        let clear_environment = action
            .environment
            .values()
            .any(|value| matches!(value, EnvironmentValue::Clear));
        if clear_environment {
            command.env_clear();
        }
        for (name, value) in &action.environment {
            match value {
                EnvironmentValue::Inherit | EnvironmentValue::Clear => {}
                EnvironmentValue::Set(value) => {
                    command.env(name, value);
                }
                EnvironmentValue::Unset => {
                    command.env_remove(name);
                }
            }
        }
        let output = command
            .output()
            .map_err(|source| error(format!("action {} failed to spawn: {source}", action.name)))?;
        let status = output.status.code().unwrap_or(-1);
        if !output.status.success() {
            return Err(error(format!(
                "action {} failed with status {}: {}",
                action.name,
                status,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        for (index, declared) in action.outputs.iter().enumerate() {
            let output_path = contained_existing_path(
                &validated.root,
                &declared.path,
                &format!("action {} output {index}", action.name),
            )?;
            if !output_path.is_file() {
                return Err(error(format!(
                    "action {} did not produce declared output {}",
                    action.name,
                    declared.path.display()
                )));
            }
            if sha256_file(&output_path)? != declared.sha256 {
                return Err(error(format!(
                    "action {} output digest mismatch for {}",
                    action.name,
                    declared.path.display()
                )));
            }
        }
        executions.push(ActionExecution {
            action: action.name.clone(),
            status,
            stdout: output.stdout,
            stderr: output.stderr,
        });
    }
    Ok(executions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Fixture {
        root: PathBuf,
        bytes: Vec<u8>,
    }

    impl Fixture {
        fn new() -> Self {
            static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
            let root =
                std::env::temp_dir().join(format!("birnam-build-driver-{}-{sequence}", nonce));
            fs::create_dir_all(&root).expect("fixture root");
            fs::write(root.join("module.birnam"), b"module").expect("module");
            for (source, _) in STAGE1_MODULES {
                let path = root.join(source);
                fs::create_dir_all(path.parent().expect("stage-1 source parent"))
                    .expect("stage-1 source directory");
                fs::write(path, b"module").expect("stage-1 source");
            }
            fs::write(root.join("zig"), b"tool").expect("tool");
            fs::create_dir_all(root.join("abi")).expect("abi directory");
            fs::create_dir_all(root.join("work")).expect("work directory");
            let build_source = b"compile.dependOn(prepare);";
            fs::write(root.join("build.zig"), build_source).expect("build source");
            let receipt_commit = "a".repeat(40);
            let receipt_tree = "b".repeat(40);
            let build_digest = hex::encode(Sha256::digest(build_source));
            let graph_document = json!({
                "transaction": {
                    "receipt_commit": receipt_commit.clone(),
                    "receipt_tree": receipt_tree.clone()
                },
                "source_digests": {"build.zig": build_digest.clone()}
            });
            let graph_bytes = serde_json::to_vec(&graph_document).expect("graph JSON");
            fs::write(root.join("abi/birnam-build-actions-v1.json"), &graph_bytes).expect("graph");
            let digest = hex::encode(Sha256::digest(b"module"));
            let graph_digest = hex::encode(Sha256::digest(&graph_bytes));
            let provider_actions = CANONICAL_DYNAMIC_LIBRARIES
                .iter()
                .map(|name| (*name, "compile"))
                .collect::<BTreeMap<_, _>>();
            let provider_outputs = CANONICAL_DYNAMIC_LIBRARIES
                .iter()
                .map(|name| {
                    json!({
                        "path": format!("out/{name}"),
                        "sha256": "c".repeat(64)
                    })
                })
                .collect::<Vec<_>>();
            let stage1_modules = STAGE1_MODULES
                .iter()
                .map(|(source, name)| {
                    json!({
                        "name": name,
                        "source": {"path": source, "sha256": digest},
                        "producer": "compile"
                    })
                })
                .collect::<Vec<_>>();
            let stage1_outputs = STAGE1_MODULES
                .iter()
                .map(|(_, name)| {
                    json!({
                        "path": format!("out/libbirnam_{name}.dylib"),
                        "sha256": "c".repeat(64)
                    })
                })
                .collect::<Vec<_>>();
            let manifest = json!({
                "schema": 1,
                "semantic_scope": "full-semantic-v1",
                "graph": {"id": "build-actions-v1", "source_path": "abi/birnam-build-actions-v1.json", "source_sha256": graph_digest, "receipt_commit": receipt_commit, "receipt_tree": receipt_tree},
                "source_graph": {
                    "build_file": {"path": "build.zig", "sha256": build_digest},
                    "depend_on_occurrences": 1,
                    "bindings": [
                        {"expression": "compile", "action": "compile"},
                        {"expression": "prepare", "action": "prepare"}
                    ],
                    "implicit_edges": [{"source": "compile", "dependency": "prepare", "kind": "source"}],
                    "roots": ["compile"]
                },
                "target": "aarch64-macos",
                "profile": "release",
                "selection": {"distribution": "production", "floor": "rust", "providers": CANONICAL_DYNAMIC_LIBRARIES.iter().map(|name| (*name, "rust")).collect::<BTreeMap<_, _>>(), "provider_actions": provider_actions},
                "modules": stage1_modules,
                "actions": [{
                    "name": "prepare",
                    "context": "production",
                    "authority": "canonical",
                    "delegation": "none",
                    "tool": "zig",
                    "tool_sha256": hex::encode(Sha256::digest(b"tool")),
                    "cwd": "work",
                    "command": ["zig", "prepare"],
                    "depends_on": [],
                    "inputs": [{"path": "module.birnam", "sha256": digest}],
                    "outputs": [{"path": "out/prepare", "sha256": "b".repeat(64)}],
                    "environment": {"BIRNAM_PREFIX": {"mode": "set", "value": "prefix"}}
                }, {
                    "name": "compile",
                    "context": "production",
                    "authority": "canonical",
                    "delegation": "none",
                    "tool": "zig",
                    "tool_sha256": hex::encode(Sha256::digest(b"tool")),
                    "cwd": "work",
                    "command": ["zig", "build-module"],
                    "depends_on": ["prepare"],
                    "inputs": [{"path": "module.birnam", "sha256": digest}],
                    "outputs": std::iter::once(json!({"path": "out/module.dylib", "sha256": "c".repeat(64)})).chain(stage1_outputs).chain(provider_outputs).collect::<Vec<_>>(),
                    "environment": {"BIRNAM_PREFIX": {"mode": "set", "value": "prefix"}}
                }],
                "dynamic_libraries": CANONICAL_DYNAMIC_LIBRARIES,
                "prefix": "out",
                "tools": {"zig": "zig"}
            });
            Self {
                root,
                bytes: serde_json::to_vec(&manifest).expect("JSON"),
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn mutate(fixture: &Fixture, update: impl FnOnce(&mut Value)) -> Vec<u8> {
        let mut value: Value = serde_json::from_slice(&fixture.bytes).expect("fixture JSON");
        update(&mut value);
        serde_json::to_vec(&value).expect("JSON")
    }

    #[test]
    fn accepts_valid_manifest_and_topological_order() {
        let fixture = Fixture::new();
        let parsed = parse_and_validate(&fixture.bytes, &fixture.root).expect("valid manifest");
        assert_eq!(parsed.action_order, ["prepare", "compile"]);
    }

    #[test]
    fn rejects_malformed_json_and_unknown_fields() {
        let fixture = Fixture::new();
        assert!(parse_and_validate(b"{}", &fixture.root).is_err());
        let bytes = mutate(&fixture, |value| value["unexpected"] = json!(true));
        assert!(parse_and_validate(&bytes, &fixture.root).is_err());
    }

    #[test]
    fn rejects_the_m41_named_target_projection() {
        let fixture = Fixture::new();
        let bytes = mutate(&fixture, |value| {
            value["semantic_scope"] = json!("named-target-projection");
        });
        let error =
            parse_and_validate(&bytes, &fixture.root).expect_err("projection must not execute");
        assert!(error.0.contains("not executable"));
    }

    #[test]
    fn rejects_graph_receipt_anchor_mismatch() {
        let fixture = Fixture::new();
        let bytes = mutate(&fixture, |value| {
            value["graph"]["receipt_commit"] = json!("c".repeat(40));
        });
        let error = parse_and_validate(&bytes, &fixture.root)
            .expect_err("graph receipt anchor must match the source receipt");
        assert!(error.0.contains("receipt anchor"));
        let bytes = mutate(&fixture, |value| {
            value["source_graph"]["build_file"]["sha256"] = json!("d".repeat(64));
        });
        let error = parse_and_validate(&bytes, &fixture.root)
            .expect_err("build digest must match the source receipt");
        assert!(error.0.contains("build.zig digest"));
    }

    #[test]
    fn rejects_path_escape() {
        let fixture = Fixture::new();
        let bytes = mutate(&fixture, |value| {
            value["modules"][0]["source"]["path"] = json!("../escape")
        });
        assert!(parse_and_validate(&bytes, &fixture.root).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape() {
        let fixture = Fixture::new();
        let outside = fixture
            .root
            .parent()
            .expect("temporary parent")
            .join(format!(
                "birnam-build-driver-outside-{}",
                std::process::id()
            ));
        fs::write(&outside, b"outside").expect("outside");
        std::os::unix::fs::symlink(&outside, fixture.root.join("link")).expect("symlink");
        let digest = hex::encode(Sha256::digest(b"outside"));
        let bytes = mutate(&fixture, |value| {
            value["modules"][0]["source"] = json!({"path": "link", "sha256": digest});
        });
        assert!(parse_and_validate(&bytes, &fixture.root).is_err());
        let _ = fs::remove_file(&outside);
    }

    #[test]
    fn rejects_duplicate_action_names() {
        let fixture = Fixture::new();
        let bytes = mutate(&fixture, |value| {
            let duplicate = value["actions"][0].clone();
            value["actions"]
                .as_array_mut()
                .expect("actions")
                .push(duplicate);
        });
        assert!(parse_and_validate(&bytes, &fixture.root).is_err());
    }

    #[test]
    fn rejects_dependency_cycles() {
        let fixture = Fixture::new();
        let bytes = mutate(&fixture, |value| {
            let action = &mut value["actions"][0];
            action["depends_on"] = json!(["compile"]);
        });
        assert!(parse_and_validate(&bytes, &fixture.root).is_err());
    }

    #[test]
    fn rejects_source_edge_action_order_mismatch() {
        let fixture = Fixture::new();
        let bytes = mutate(&fixture, |value| {
            value["source_graph"]["implicit_edges"] = json!([]);
        });
        assert!(parse_and_validate(&bytes, &fixture.root).is_err());
    }

    #[test]
    fn rejects_disconnected_action_and_floor_provider_mismatch() {
        let fixture = Fixture::new();
        let disconnected = mutate(&fixture, |value| {
            let mut orphan = value["actions"][0].clone();
            orphan["name"] = json!("orphan");
            value["actions"]
                .as_array_mut()
                .expect("actions")
                .push(orphan);
        });
        assert!(parse_and_validate(&disconnected, &fixture.root).is_err());
        let mismatch = mutate(&fixture, |value| {
            value["selection"]["providers"][CANONICAL_DYNAMIC_LIBRARIES[0]] = json!("zig");
        });
        assert!(parse_and_validate(&mismatch, &fixture.root).is_err());
        let missing_provider_output = mutate(&fixture, |value| {
            value["selection"]["provider_actions"][CANONICAL_DYNAMIC_LIBRARIES[0]] =
                json!("prepare");
        });
        let error = parse_and_validate(&missing_provider_output, &fixture.root)
            .expect_err("provider producer output must be declared");
        assert!(error.0.contains("producer"));
    }

    #[test]
    fn rejects_output_collision() {
        let fixture = Fixture::new();
        let bytes = mutate(&fixture, |value| {
            let output = value["actions"][0]["outputs"][0].clone();
            value["actions"][1]["outputs"][0] = output;
        });
        assert!(parse_and_validate(&bytes, &fixture.root).is_err());
    }

    #[test]
    fn rejects_stale_input_digest() {
        let fixture = Fixture::new();
        let bytes = mutate(&fixture, |value| {
            value["modules"][0]["source"]["sha256"] = json!("c".repeat(64));
        });
        assert!(parse_and_validate(&bytes, &fixture.root).is_err());
    }

    #[test]
    fn rejects_mixed_production_vector_and_zig_dynamic_name() {
        let fixture = Fixture::new();
        let mixed = mutate(&fixture, |value| value["selection"]["floor"] = json!("zig"));
        assert!(parse_and_validate(&mixed, &fixture.root).is_err());
        let zig_name = mutate(&fixture, |value| {
            value["dynamic_libraries"][0] = json!("libbirnam_id_zig.dylib")
        });
        assert!(parse_and_validate(&zig_name, &fixture.root).is_err());
    }

    #[test]
    fn rejects_production_tool_path_with_zig_oracle_marker() {
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.root.join("bin")).expect("tool directory");
        fs::write(fixture.root.join("bin/birnam-zig-oracle"), b"tool").expect("oracle tool");
        let bytes = mutate(&fixture, |value| {
            value["tools"]["zig"] = json!("bin/birnam-zig-oracle");
        });
        let error = parse_and_validate(&bytes, &fixture.root)
            .expect_err("production must reject a Zig oracle tool path");
        assert!(error.0.contains("Zig-only path"));
    }

    #[test]
    fn rejects_explicit_shadow_delegation_without_shadow_authority() {
        let fixture = Fixture::new();
        let bytes = mutate(&fixture, |value| {
            value["selection"]["distribution"] = json!("shadow");
            value["actions"][0]["context"] = json!("shadow");
            value["actions"][0]["delegation"] = json!("explicit-shadow");
        });
        let error = parse_and_validate(&bytes, &fixture.root)
            .expect_err("explicit-shadow must require shadow authority");
        assert!(error.0.contains("without shadow authority"));
    }

    #[cfg(unix)]
    #[test]
    fn executes_shadow_graph_with_deterministic_environment() {
        use std::os::unix::fs::PermissionsExt;

        let fixture = Fixture::new();
        let runner = fixture.root.join("runner");
        let provider_writes = CANONICAL_DYNAMIC_LIBRARIES
            .iter()
            .map(|library| format!("printf '%s' shadow-output > ../out/{library}"))
            .collect::<Vec<_>>()
            .join("\n");
        let stage1_writes = STAGE1_MODULES
            .iter()
            .map(|(_, name)| format!("printf '%s' shadow-output > ../out/libbirnam_{name}.dylib"))
            .collect::<Vec<_>>()
            .join("\n");
        let runner_source = format!(
            "#!/bin/sh\nset -eu\nmkdir -p ../out\ncase \"$1\" in\nprepare) printf '%s' shadow-output > ../out/prepare ;;\ncompile) printf '%s' shadow-output > ../out/module.dylib\n{stage1_writes}\n{provider_writes} ;;\nesac\nprintf '%s' \"$BIRNAM_VALUE\"\n"
        );
        fs::write(&runner, runner_source).expect("runner");
        fs::set_permissions(&runner, fs::Permissions::from_mode(0o755)).expect("executable");
        let runner_digest = hex::encode(Sha256::digest(fs::read(&runner).expect("runner bytes")));
        let output_digest = hex::encode(Sha256::digest(b"shadow-output"));
        let bytes = mutate(&fixture, |value| {
            value["selection"]["distribution"] = json!("shadow");
            value["tools"]["runner"] = json!("runner");
            for action in value["actions"].as_array_mut().expect("actions") {
                let action_name = action["name"].as_str().expect("action name").to_string();
                action["context"] = json!("shadow");
                action["authority"] = json!("shadow");
                action["delegation"] = json!("explicit-shadow");
                action["tool"] = json!("runner");
                action["tool_sha256"] = json!(runner_digest.clone());
                action["command"] = json!(["runner", action_name]);
                for output in action["outputs"].as_array_mut().expect("outputs") {
                    output["sha256"] = json!(output_digest.clone());
                }
                action["environment"] = json!({
                    "BIRNAM_VALUE": {"mode": "set", "value": "shadow-ok"},
                    "BIRNAM_CLEAR": {"mode": "clear"}
                });
            }
        });
        let validated = parse_and_validate(&bytes, &fixture.root).expect("shadow manifest");
        let executions = execute_shadow_graph(&validated).expect("shadow execution");
        assert_eq!(
            executions
                .iter()
                .map(|result| result.action.as_str())
                .collect::<Vec<_>>(),
            ["prepare", "compile"]
        );
        assert!(
            executions
                .iter()
                .all(|result| result.status == 0 && result.stdout == b"shadow-ok")
        );
        let mut wrong_context = validated.clone();
        wrong_context.manifest.actions[0].context = "production".to_string();
        let context_error = execute_shadow_graph(&wrong_context)
            .expect_err("shadow execution must reject canonical action context");
        assert!(context_error.0.contains("requires every action"));
        let mut stale_output = validated;
        stale_output.manifest.actions[0].outputs[0].sha256 = "0".repeat(64);
        let output_error = execute_shadow_graph(&stale_output)
            .expect_err("shadow execution must verify declared output digests");
        assert!(output_error.0.contains("output digest mismatch"));
    }

    #[test]
    fn captures_every_build_zig_depend_on_form() {
        let source =
            fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../build.zig"))
                .expect("build.zig");
        let graph: Value = serde_json::from_str(
            &fs::read_to_string(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../abi/birnam-build-actions-v1.json"),
            )
            .expect("M4.1 graph receipt"),
        )
        .expect("M4.1 graph JSON");
        assert!(graph.get("source_digests").is_some());
        assert!(graph["transaction"].get("source_digests").is_none());
        let capture = capture_zig_dependencies(&source).expect("all dependOn calls resolve");
        assert_eq!(capture.occurrence_count, 464);
        assert_eq!(capture.edges.len(), capture.occurrence_count);
        let unique_edges = capture
            .edges
            .iter()
            .map(|edge| (edge.source.as_str(), edge.dependency.as_str()))
            .collect::<BTreeSet<_>>();
        assert_eq!(unique_edges.len(), 463);
        assert!(
            capture
                .edges
                .iter()
                .any(|edge| edge.source_line == 3519 && edge.dependency == "b.getInstallStep()")
        );
        assert!(
            capture
                .edges
                .iter()
                .any(|edge| edge.source_line == 3520 && edge.source.contains("b.step("))
        );
    }
}
