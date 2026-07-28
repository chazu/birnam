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

use serde::Deserialize;
use sha2::{Digest, Sha256};

pub const MANIFEST_SCHEMA: u32 = 1;
pub const EXECUTABLE_SEMANTIC_SCOPE: &str = "full-semantic-v1";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DriverManifest {
    pub schema: u32,
    pub semantic_scope: String,
    pub graph: GraphIdentity,
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
    pub source_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub distribution: String,
    pub floor: String,
    pub providers: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleSpec {
    pub name: String,
    pub source: PathRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionSpec {
    pub name: String,
    pub tool: String,
    pub command: Vec<String>,
    pub depends_on: Vec<String>,
    pub inputs: Vec<PathRef>,
    pub outputs: Vec<PathRef>,
    pub environment: BTreeMap<String, String>,
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

fn error(message: impl Into<String>) -> ManifestError {
    ManifestError(message.into())
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
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

fn validate_path_ref(path_ref: &PathRef, root: &Path, label: &str) -> Result<(), ManifestError> {
    safe_relative_path(&path_ref.path, label)?;
    if !valid_digest(&path_ref.sha256) {
        return Err(error(format!("{label} has an invalid sha256 digest")));
    }
    let resolved = root.join(&path_ref.path);
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
        "production" | "rollback" | "shadow" => {}
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
        for variable in action.environment.keys() {
            if variable.is_empty()
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
    safe_name(&manifest.target, "target")?;
    if manifest.profile != "debug" && manifest.profile != "release" {
        return Err(error(format!("unsupported profile: {}", manifest.profile)));
    }
    safe_name(&manifest.graph.id, "graph id")?;
    if !valid_digest(&manifest.graph.source_sha256) {
        return Err(error("graph source_sha256 is malformed"));
    }
    validate_selection(&manifest.selection)?;
    safe_relative_path(&manifest.prefix, "output prefix")?;
    for (name, path) in &manifest.tools {
        safe_name(name, "tool name")?;
        safe_relative_path(path, "tool path")?;
        if !root.join(path).is_file() {
            return Err(error(format!("tool is unavailable: {}", path.display())));
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

    let action_order = validate_dependencies(&manifest.actions)?;
    let action_names = manifest
        .actions
        .iter()
        .map(|action| action.name.as_str())
        .collect::<BTreeSet<_>>();
    for action in &manifest.actions {
        if !manifest.tools.contains_key(&action.tool) {
            return Err(error(format!(
                "action {} references undeclared tool {}",
                action.name, action.tool
            )));
        }
        for (index, input) in action.inputs.iter().enumerate() {
            validate_path_ref(
                input,
                &root,
                &format!("action {} input {index}", action.name),
            )?;
        }
        for (index, output) in action.outputs.iter().enumerate() {
            safe_relative_path(
                &output.path,
                &format!("action {} output {index}", action.name),
            )?;
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

    Ok(ValidatedManifest {
        manifest,
        action_order,
        root,
    })
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
            fs::write(root.join("zig"), b"tool").expect("tool");
            let digest = hex::encode(Sha256::digest(b"module"));
            let graph_digest = "a".repeat(64);
            let manifest = json!({
                "schema": 1,
                "semantic_scope": "full-semantic-v1",
                "graph": {"id": "build-actions-v1", "source_sha256": graph_digest},
                "target": "aarch64-macos",
                "profile": "release",
                "selection": {"distribution": "production", "floor": "rust", "providers": {"store": "rust"}},
                "modules": [{"name": "module", "source": {"path": "module.birnam", "sha256": digest}}],
                "actions": [{
                    "name": "compile",
                    "tool": "zig",
                    "command": ["zig", "build-module"],
                    "depends_on": [],
                    "inputs": [{"path": "module.birnam", "sha256": digest}],
                    "outputs": [{"path": "out/module.dylib", "sha256": "b".repeat(64)}],
                    "environment": {"BIRNAM_PREFIX": "prefix"}
                }],
                "dynamic_libraries": ["libbirnam_id.dylib"],
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
        assert_eq!(parsed.action_order, ["compile"]);
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
    fn rejects_path_escape() {
        let fixture = Fixture::new();
        let bytes = mutate(&fixture, |value| {
            value["modules"][0]["source"]["path"] = json!("../escape")
        });
        assert!(parse_and_validate(&bytes, &fixture.root).is_err());
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
}
