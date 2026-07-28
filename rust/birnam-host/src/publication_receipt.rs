//! Atomic staging/publication receipt validation.
//!
//! This module defines the receipt needed before a future Rust driver can
//! publish a production prefix. It validates files and transaction metadata but
//! deliberately performs no rename, deletion, or process execution.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PUBLICATION_RECEIPT_SCHEMA: u32 = 1;
pub const PUBLICATION_RECEIPT_SCOPE: &str = "staging-publication-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationIdentity {
    pub transaction_id: String,
    pub source_tree_sha256: String,
    pub action_receipts_sha256: String,
    pub bootstrap_receipt_sha256: String,
    pub parity_receipt_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationReceipt {
    pub schema: u32,
    pub semantic_scope: String,
    pub identity: PublicationIdentity,
    pub staging_prefix: PathBuf,
    pub final_prefix: PathBuf,
    pub state: PublicationState,
    pub atomic_rename: AtomicRename,
    pub artifacts: Vec<PublishedArtifact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationState {
    Staged,
    Published,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AtomicRename {
    pub mechanism: String,
    pub same_filesystem: bool,
    pub completed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedArtifact {
    pub path: PathBuf,
    pub sha256: String,
    pub owner_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicationReceiptError(pub String);

impl fmt::Display for PublicationReceiptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for PublicationReceiptError {}

fn error(message: impl Into<String>) -> PublicationReceiptError {
    PublicationReceiptError(message.into())
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn safe_name(value: &str, label: &str) -> Result<(), PublicationReceiptError> {
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

fn safe_relative_path(path: &Path, label: &str) -> Result<(), PublicationReceiptError> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(error(format!("{label} must be a relative path")));
    }
    for component in path.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(error(format!(
                "{label} escapes the publication root: {path:?}"
            )));
        }
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, PublicationReceiptError> {
    let bytes = fs::read(path)
        .map_err(|source| error(format!("cannot read {}: {source}", path.display())))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn contained_file(
    root: &Path,
    relative: &Path,
    label: &str,
) -> Result<PathBuf, PublicationReceiptError> {
    safe_relative_path(relative, label)?;
    let mut cursor = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(part) = component else {
            return Err(error(format!("{label} contains a non-normal component")));
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
        return Err(error(format!("{label} escapes the publication root")));
    }
    Ok(canonical)
}

fn validate_identity(
    actual: &PublicationIdentity,
    expected: &PublicationIdentity,
) -> Result<(), PublicationReceiptError> {
    safe_name(&actual.transaction_id, "publication transaction_id")?;
    if !valid_digest(&actual.source_tree_sha256)
        || !valid_digest(&actual.action_receipts_sha256)
        || !valid_digest(&actual.bootstrap_receipt_sha256)
        || !valid_digest(&actual.parity_receipt_sha256)
    {
        return Err(error("publication identity contains a malformed digest"));
    }
    if actual != expected {
        return Err(error(
            "publication receipt identity does not match the requested transaction",
        ));
    }
    Ok(())
}

/// Validate a staging/publication receipt against a checkout root.
///
/// This function never mutates the filesystem and never authorizes execution.
pub fn parse_and_validate(
    bytes: &[u8],
    root: &Path,
    expected: &PublicationIdentity,
) -> Result<PublicationReceipt, PublicationReceiptError> {
    let receipt: PublicationReceipt = serde_json::from_slice(bytes)
        .map_err(|source| error(format!("publication receipt is invalid: {source}")))?;
    let root = root
        .canonicalize()
        .map_err(|source| error(format!("publication root is unavailable: {source}")))?;
    if receipt.schema != PUBLICATION_RECEIPT_SCHEMA {
        return Err(error(format!(
            "unsupported publication receipt schema: {}",
            receipt.schema
        )));
    }
    if receipt.semantic_scope != PUBLICATION_RECEIPT_SCOPE {
        return Err(error(format!(
            "publication receipt has the wrong semantic scope: {}",
            receipt.semantic_scope
        )));
    }
    validate_identity(&receipt.identity, expected)?;
    safe_relative_path(&receipt.staging_prefix, "staging prefix")?;
    safe_relative_path(&receipt.final_prefix, "final prefix")?;
    if receipt.staging_prefix == receipt.final_prefix
        || receipt.staging_prefix.starts_with(&receipt.final_prefix)
        || receipt.final_prefix.starts_with(&receipt.staging_prefix)
    {
        return Err(error("staging and final prefixes overlap"));
    }
    if receipt.atomic_rename.mechanism != "renameat" {
        return Err(error(format!(
            "unsupported atomic publication mechanism: {}",
            receipt.atomic_rename.mechanism
        )));
    }
    if !receipt.atomic_rename.same_filesystem {
        return Err(error("publication prefixes are not on the same filesystem"));
    }
    match receipt.state {
        PublicationState::Staged if receipt.atomic_rename.completed => {
            return Err(error("staged publication cannot claim a completed rename"));
        }
        PublicationState::Published if !receipt.atomic_rename.completed => {
            return Err(error("published receipt has no completed atomic rename"));
        }
        _ => {}
    }
    if receipt.artifacts.is_empty() {
        return Err(error("publication receipt has no artifacts"));
    }

    let selected_prefix = match receipt.state {
        PublicationState::Staged => &receipt.staging_prefix,
        PublicationState::Published => &receipt.final_prefix,
    };
    let mut paths = BTreeSet::new();
    for artifact in &receipt.artifacts {
        safe_relative_path(&artifact.path, "published artifact path")?;
        safe_name(&artifact.owner_action, "artifact owner action")?;
        if !valid_digest(&artifact.sha256) {
            return Err(error(format!(
                "artifact {} has a malformed digest",
                artifact.path.display()
            )));
        }
        if !artifact.path.starts_with(selected_prefix) {
            return Err(error(format!(
                "artifact {} is outside the selected publication prefix",
                artifact.path.display()
            )));
        }
        if !paths.insert(artifact.path.clone()) {
            return Err(error(format!(
                "publication artifact is listed twice: {}",
                artifact.path.display()
            )));
        }
        let file = contained_file(&root, &artifact.path, "published artifact")?;
        if !file.is_file() || sha256_file(&file)? != artifact.sha256 {
            return Err(error(format!(
                "publication artifact digest does not match: {}",
                artifact.path.display()
            )));
        }
    }
    let mut ordered = paths.iter().collect::<Vec<_>>();
    ordered.sort();
    for pair in ordered.windows(2) {
        if pair[1].starts_with(pair[0]) {
            return Err(error(format!(
                "publication artifact paths overlap: {} and {}",
                pair[0].display(),
                pair[1].display()
            )));
        }
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
        expected: PublicationIdentity,
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
                std::env::temp_dir().join(format!("birnam-publication-receipt-{nonce}-{sequence}"));
            let staging = root.join("stage");
            fs::create_dir_all(staging.join("lib")).expect("staging directory");
            fs::write(staging.join("lib/module.dylib"), b"artifact").expect("artifact");
            let expected = PublicationIdentity {
                transaction_id: "txn-001".into(),
                source_tree_sha256: "a".repeat(64),
                action_receipts_sha256: "b".repeat(64),
                bootstrap_receipt_sha256: "c".repeat(64),
                parity_receipt_sha256: "d".repeat(64),
            };
            let receipt = json!({
                "schema": PUBLICATION_RECEIPT_SCHEMA,
                "semantic_scope": PUBLICATION_RECEIPT_SCOPE,
                "identity": expected,
                "staging_prefix": "stage",
                "final_prefix": "published",
                "state": "staged",
                "atomic_rename": {
                    "mechanism": "renameat",
                    "same_filesystem": true,
                    "completed": false
                },
                "artifacts": [{
                    "path": "stage/lib/module.dylib",
                    "sha256": hex::encode(Sha256::digest(b"artifact")),
                    "owner_action": "compile-module"
                }]
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
    fn accepts_staged_receipt_with_matching_artifact() {
        let fixture = Fixture::new();
        let receipt =
            parse_and_validate(&fixture.bytes, &fixture.root, &fixture.expected).expect("valid");
        assert_eq!(receipt.state, PublicationState::Staged);
        assert!(!receipt.atomic_rename.completed);
    }

    #[test]
    fn accepts_published_receipt_only_after_completed_rename() {
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.root.join("published/lib")).expect("published directory");
        fs::write(fixture.root.join("published/lib/module.dylib"), b"artifact")
            .expect("published artifact");
        let bytes = mutate(&fixture, |value| {
            value["state"] = json!("published");
            value["atomic_rename"]["completed"] = json!(true);
            value["artifacts"][0]["path"] = json!("published/lib/module.dylib");
        });
        parse_and_validate(&bytes, &fixture.root, &fixture.expected)
            .expect("published receipt with completed rename");
    }

    #[test]
    fn rejects_identity_and_digest_mismatch() {
        let fixture = Fixture::new();
        let bytes = mutate(&fixture, |value| {
            value["identity"]["transaction_id"] = json!("other");
        });
        let error = parse_and_validate(&bytes, &fixture.root, &fixture.expected)
            .expect_err("transaction identity must match");
        assert!(error.0.contains("does not match"));

        let bytes = mutate(&fixture, |value| {
            value["artifacts"][0]["sha256"] = json!("e".repeat(64));
        });
        let error = parse_and_validate(&bytes, &fixture.root, &fixture.expected)
            .expect_err("artifact digest must match");
        assert!(error.0.contains("digest does not match"));
    }

    #[test]
    fn rejects_overlap_and_non_atomic_metadata() {
        let fixture = Fixture::new();
        let bytes = mutate(&fixture, |value| {
            value["final_prefix"] = json!("stage/lib");
        });
        let error = parse_and_validate(&bytes, &fixture.root, &fixture.expected)
            .expect_err("prefix overlap must be rejected");
        assert!(error.0.contains("prefixes overlap"));

        let bytes = mutate(&fixture, |value| {
            value["atomic_rename"]["same_filesystem"] = json!(false);
        });
        let error = parse_and_validate(&bytes, &fixture.root, &fixture.expected)
            .expect_err("cross-filesystem publication must be rejected");
        assert!(error.0.contains("same filesystem"));
    }

    #[test]
    fn rejects_staged_or_published_state_inconsistency() {
        let fixture = Fixture::new();
        let bytes = mutate(&fixture, |value| {
            value["atomic_rename"]["completed"] = json!(true);
        });
        let error = parse_and_validate(&bytes, &fixture.root, &fixture.expected)
            .expect_err("staged receipt cannot claim rename");
        assert!(error.0.contains("staged publication"));

        let bytes = mutate(&fixture, |value| {
            value["state"] = json!("published");
            value["atomic_rename"]["completed"] = json!(false);
        });
        let error = parse_and_validate(&bytes, &fixture.root, &fixture.expected)
            .expect_err("published receipt requires rename");
        assert!(error.0.contains("no completed atomic rename"));
    }
}
