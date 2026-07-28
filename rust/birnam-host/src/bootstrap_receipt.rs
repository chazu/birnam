//! Same-run bootstrap receipt validation for future authoritative builds.
//!
//! This is a capability checkpoint only. It proves that staged Birnam
//! executables are the products of one source/build run and that their bytes
//! still match the receipt. It does not execute a staged executable or grant
//! production authority.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const BOOTSTRAP_RECEIPT_SCHEMA: u32 = 1;
pub const BOOTSTRAP_RECEIPT_SCOPE: &str = "bootstrap-receipt-v1";
pub const BOOTSTRAP_ROLES: &[&str] = &["birnam-build", "birnam-host", "birnam-compiler"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootstrapIdentity {
    pub run_id: String,
    pub source_tree_sha256: String,
    pub source_archive_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootstrapReceipt {
    pub schema: u32,
    pub semantic_scope: String,
    pub identity: BootstrapIdentity,
    pub source_manifest: PathRef,
    pub executables: Vec<BootstrapExecutable>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootstrapExecutable {
    pub role: String,
    pub path: PathBuf,
    pub sha256: String,
    pub source_sha256: String,
    pub build_action: String,
    pub sequence: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathRef {
    pub path: PathBuf,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapReceiptError(pub String);

impl fmt::Display for BootstrapReceiptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for BootstrapReceiptError {}

fn error(message: impl Into<String>) -> BootstrapReceiptError {
    BootstrapReceiptError(message.into())
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn safe_name(value: &str, label: &str) -> Result<(), BootstrapReceiptError> {
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

fn safe_relative_path(path: &Path, label: &str) -> Result<(), BootstrapReceiptError> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(error(format!("{label} must be a relative path")));
    }
    for component in path.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(error(format!(
                "{label} escapes the bootstrap root: {path:?}"
            )));
        }
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, BootstrapReceiptError> {
    let bytes = fs::read(path)
        .map_err(|source| error(format!("cannot read {}: {source}", path.display())))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn contained_file(root: &Path, path: &Path, label: &str) -> Result<PathBuf, BootstrapReceiptError> {
    safe_relative_path(path, label)?;
    let mut cursor = root.to_path_buf();
    for component in path.components() {
        let Component::Normal(part) = component else {
            return Err(error(format!("{label} has a non-normal path component")));
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
        return Err(error(format!("{label} escapes the bootstrap root")));
    }
    Ok(canonical)
}

fn validate_identity(
    actual: &BootstrapIdentity,
    expected: &BootstrapIdentity,
) -> Result<(), BootstrapReceiptError> {
    safe_name(&actual.run_id, "bootstrap run_id")?;
    if !valid_digest(&actual.source_tree_sha256) || !valid_digest(&actual.source_archive_sha256) {
        return Err(error("bootstrap source identity digest is malformed"));
    }
    if actual != expected {
        return Err(error(
            "bootstrap receipt identity does not match the requested source run",
        ));
    }
    Ok(())
}

/// Validate a same-run bootstrap receipt against a staging prefix.
///
/// The receipt is data-only and confers no permission to execute the files.
pub fn parse_and_validate(
    bytes: &[u8],
    root: &Path,
    staging_prefix: &Path,
    expected: &BootstrapIdentity,
) -> Result<BootstrapReceipt, BootstrapReceiptError> {
    let receipt: BootstrapReceipt = serde_json::from_slice(bytes)
        .map_err(|source| error(format!("bootstrap receipt is invalid: {source}")))?;
    let root = root
        .canonicalize()
        .map_err(|source| error(format!("bootstrap root is unavailable: {source}")))?;
    safe_relative_path(staging_prefix, "staging prefix")?;
    if receipt.schema != BOOTSTRAP_RECEIPT_SCHEMA {
        return Err(error(format!(
            "unsupported bootstrap receipt schema: {}",
            receipt.schema
        )));
    }
    if receipt.semantic_scope != BOOTSTRAP_RECEIPT_SCOPE {
        return Err(error(format!(
            "bootstrap receipt has the wrong semantic scope: {}",
            receipt.semantic_scope
        )));
    }
    validate_identity(&receipt.identity, expected)?;
    if !valid_digest(&receipt.source_manifest.sha256) {
        return Err(error("bootstrap source manifest digest is malformed"));
    }
    let source_manifest = contained_file(&root, &receipt.source_manifest.path, "source manifest")?;
    if !source_manifest.is_file()
        || sha256_file(&source_manifest)? != receipt.source_manifest.sha256
    {
        return Err(error("bootstrap source manifest digest does not match"));
    }

    let expected_roles = BOOTSTRAP_ROLES.iter().copied().collect::<BTreeSet<_>>();
    let mut roles = BTreeSet::new();
    let mut sequences = BTreeSet::new();
    if receipt.executables.len() != BOOTSTRAP_ROLES.len() {
        return Err(error(format!(
            "bootstrap receipt requires exactly {} staged executables",
            BOOTSTRAP_ROLES.len()
        )));
    }
    for executable in &receipt.executables {
        if !expected_roles.contains(executable.role.as_str())
            || !roles.insert(executable.role.as_str())
        {
            return Err(error(format!(
                "bootstrap receipt has a duplicate or unknown executable role: {}",
                executable.role
            )));
        }
        if !sequences.insert(executable.sequence) {
            return Err(error("bootstrap executable sequence is duplicated"));
        }
        safe_name(&executable.build_action, "bootstrap build action")?;
        if !valid_digest(&executable.sha256)
            || !valid_digest(&executable.source_sha256)
            || executable.source_sha256 != receipt.identity.source_tree_sha256
        {
            return Err(error(format!(
                "bootstrap executable {} has a malformed digest",
                executable.role
            )));
        }
        safe_relative_path(&executable.path, "bootstrap executable path")?;
        if !executable.path.starts_with(staging_prefix) {
            return Err(error(format!(
                "bootstrap executable {} is outside the staging prefix",
                executable.role
            )));
        }
        let path = contained_file(&root, &executable.path, "bootstrap executable")?;
        if !path.is_file() || sha256_file(&path)? != executable.sha256 {
            return Err(error(format!(
                "bootstrap executable {} digest does not match",
                executable.role
            )));
        }
    }
    if roles != expected_roles {
        return Err(error(
            "bootstrap receipt does not cover every required executable role",
        ));
    }
    let expected_sequences = (0..BOOTSTRAP_ROLES.len()).collect::<BTreeSet<_>>();
    if sequences != expected_sequences {
        return Err(error(
            "bootstrap executable sequences must be contiguous from zero",
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
        expected: BootstrapIdentity,
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
                std::env::temp_dir().join(format!("birnam-bootstrap-receipt-{nonce}-{sequence}"));
            let stage = root.join("stage/bin");
            fs::create_dir_all(&stage).expect("staging directory");
            fs::write(root.join("source.json"), b"source manifest").expect("source manifest");
            for role in BOOTSTRAP_ROLES {
                fs::write(stage.join(role), role.as_bytes()).expect("executable");
            }
            let expected = BootstrapIdentity {
                run_id: "run-001".into(),
                source_tree_sha256: "a".repeat(64),
                source_archive_sha256: "b".repeat(64),
            };
            let executables = BOOTSTRAP_ROLES
                .iter()
                .enumerate()
                .map(|(sequence, role)| {
                    json!({
                        "role": role,
                        "path": format!("stage/bin/{role}"),
                        "sha256": hex::encode(Sha256::digest(role.as_bytes())),
                        "source_sha256": "a".repeat(64),
                        "build_action": format!("build-{role}"),
                        "sequence": sequence
                    })
                })
                .collect::<Vec<_>>();
            let receipt = json!({
                "schema": BOOTSTRAP_RECEIPT_SCHEMA,
                "semantic_scope": BOOTSTRAP_RECEIPT_SCOPE,
                "identity": expected,
                "source_manifest": {
                    "path": "source.json",
                    "sha256": hex::encode(Sha256::digest(b"source manifest"))
                },
                "executables": executables
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
    fn accepts_same_run_staged_executables() {
        let fixture = Fixture::new();
        let receipt = parse_and_validate(
            &fixture.bytes,
            &fixture.root,
            Path::new("stage"),
            &fixture.expected,
        )
        .expect("valid bootstrap receipt");
        assert_eq!(receipt.executables.len(), BOOTSTRAP_ROLES.len());
    }

    #[test]
    fn rejects_source_run_mismatch() {
        let fixture = Fixture::new();
        let bytes = mutate(&fixture, |value| {
            value["identity"]["run_id"] = json!("other-run")
        });
        let error =
            parse_and_validate(&bytes, &fixture.root, Path::new("stage"), &fixture.expected)
                .expect_err("different source run must be rejected");
        assert!(error.0.contains("does not match"));
    }

    #[test]
    fn rejects_stale_executable_digest() {
        let fixture = Fixture::new();
        let bytes = mutate(&fixture, |value| {
            value["executables"][0]["sha256"] = json!("d".repeat(64))
        });
        let error =
            parse_and_validate(&bytes, &fixture.root, Path::new("stage"), &fixture.expected)
                .expect_err("stale executable digest must be rejected");
        assert!(error.0.contains("digest does not match"));
    }

    #[test]
    fn rejects_missing_role_and_duplicate_sequence() {
        let fixture = Fixture::new();
        let bytes = mutate(&fixture, |value| {
            value["executables"][0]["role"] = json!("unknown");
        });
        let error =
            parse_and_validate(&bytes, &fixture.root, Path::new("stage"), &fixture.expected)
                .expect_err("unknown executable role must be rejected");
        assert!(error.0.contains("unknown executable role"));

        let bytes = mutate(&fixture, |value| {
            value["executables"][1]["sequence"] = json!(0)
        });
        let error =
            parse_and_validate(&bytes, &fixture.root, Path::new("stage"), &fixture.expected)
                .expect_err("duplicate sequence must be rejected");
        assert!(error.0.contains("sequence is duplicated"));
    }

    #[test]
    fn rejects_executable_outside_staging_prefix() {
        let fixture = Fixture::new();
        fs::write(fixture.root.join("outside"), b"outside").expect("outside executable");
        let digest = hex::encode(Sha256::digest(b"outside"));
        let bytes = mutate(&fixture, |value| {
            value["executables"][0]["path"] = json!("outside");
            value["executables"][0]["sha256"] = json!(digest);
        });
        let error =
            parse_and_validate(&bytes, &fixture.root, Path::new("stage"), &fixture.expected)
                .expect_err("executable outside staging prefix must be rejected");
        assert!(error.0.contains("outside the staging prefix"));
    }
}
