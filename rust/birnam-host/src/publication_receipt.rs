//! Atomic staging/publication receipt validation.
//!
//! This module defines the receipt needed before a future Rust driver can
//! publish a production prefix. It validates files and transaction metadata but
//! deliberately performs no rename, deletion, or process execution.

use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

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

const DIAGNOSTICS: super::receipt_validation::Diagnostics =
    super::receipt_validation::Diagnostics::Publication;

use super::receipt_validation::valid_digest;

fn safe_name(value: &str, label: &str) -> Result<(), PublicationReceiptError> {
    super::receipt_validation::safe_name(value, label).map_err(error)
}

fn safe_relative_path(path: &Path, label: &str) -> Result<(), PublicationReceiptError> {
    super::receipt_validation::safe_relative_path(path, label, &DIAGNOSTICS).map_err(error)
}

fn sha256_file(path: &Path) -> Result<String, PublicationReceiptError> {
    super::receipt_validation::sha256_file(path).map_err(error)
}

fn contained_file(
    root: &Path,
    path: &Path,
    label: &str,
) -> Result<PathBuf, PublicationReceiptError> {
    super::receipt_validation::contained_file(root, path, label, &DIAGNOSTICS).map_err(error)
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
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn pins_mechanical_policy_and_exact_diagnostics() {
        for name in ["", ".", "..", "a/b", "a\\b", "a\n"] {
            assert_eq!(
                safe_name(name, "name").unwrap_err().0,
                format!("name is not a safe name: {name:?}")
            );
        }
        for name in ["ok", "a b", "é", "..."] {
            safe_name(name, "name").unwrap();
        }
        assert!(valid_digest(&"A".repeat(64)));
        assert!(!valid_digest(&"a".repeat(63)));
        assert!(!valid_digest(&"g".repeat(64)));
        for path in ["", "/absolute"] {
            assert_eq!(
                safe_relative_path(Path::new(path), "path").unwrap_err().0,
                "path must be a relative path"
            );
        }
        for path in [".", "../outside", "a/../b"] {
            let path = Path::new(path);
            let label = "path";
            let expected = format!("{label} escapes the publication root: {path:?}");
            let actual = safe_relative_path(path, label).unwrap_err().0;
            assert_eq!(actual, expected);
            assert_eq!(actual.capacity(), expected.capacity());
        }
        // Path::components intentionally normalizes interior dots and slashes.
        safe_relative_path(Path::new("a/./b//"), "path").unwrap();
        let fixture = Fixture::new();
        let root = fixture.root.canonicalize().unwrap();
        assert_eq!(
            contained_file(&root, Path::new("missing/../x"), "file")
                .unwrap_err()
                .0,
            "file escapes the publication root: \"missing/../x\""
        );
        let missing = root.join("missing");
        let source = fs::symlink_metadata(&missing).unwrap_err();
        assert_eq!(
            contained_file(&root, Path::new("missing/child"), "file")
                .unwrap_err()
                .0,
            format!("file is unavailable at {}: {source}", missing.display())
        );
        let source = fs::read(&missing).unwrap_err();
        assert_eq!(
            sha256_file(&missing).unwrap_err().0,
            format!("cannot read {}: {source}", missing.display())
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_at_each_depth_through_public_validator() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new();
        let root = fixture.root.canonicalize().unwrap();
        symlink("absent", root.join("stage/dangling")).unwrap();
        symlink("..", root.join("stage/link")).unwrap();
        for (path, first_link) in [
            ("stage/dangling", "stage/dangling"),
            ("stage/link/source.json", "stage/link"),
        ] {
            let bytes = mutate(&fixture, |value| {
                value["artifacts"][0]["path"] = json!(path);
            });
            assert_eq!(
                parse_and_validate(&bytes, &fixture.root, &fixture.expected)
                    .unwrap_err()
                    .0,
                format!(
                    "published artifact contains a symlink: {}",
                    root.join(first_link).display()
                )
            );
        }
    }

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
