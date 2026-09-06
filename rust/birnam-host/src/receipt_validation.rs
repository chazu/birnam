//! Mechanical checks shared by the data-only receipt validators.
//! Receipt schemas, policy, error types and check ordering stay with callers.

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub(super) enum Diagnostics {
    Bootstrap,
    Publication,
}

impl Diagnostics {
    // Keep the original format literals, not only their resulting text: the
    // literal lengths also affect String's initial allocation capacity.
    #[inline]
    fn escaping_path(&self, label: &str, path: &Path) -> String {
        match self {
            Self::Bootstrap => format!("{label} escapes the bootstrap root: {path:?}"),
            Self::Publication => format!("{label} escapes the publication root: {path:?}"),
        }
    }

    #[inline]
    fn escaping_root(&self, label: &str) -> String {
        match self {
            Self::Bootstrap => format!("{label} escapes the bootstrap root"),
            Self::Publication => format!("{label} escapes the publication root"),
        }
    }

    #[inline]
    fn non_normal_component(&self, label: &str) -> String {
        match self {
            Self::Bootstrap => format!("{label} has a non-normal path component"),
            Self::Publication => format!("{label} contains a non-normal component"),
        }
    }
}

#[inline]
pub(super) fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[inline]
pub(super) fn safe_name(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty()
        || value.contains('/')
        || value.contains('\\')
        || value == "."
        || value == ".."
        || value.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err(format!("{label} is not a safe name: {value:?}"));
    }
    Ok(())
}

#[inline]
pub(super) fn safe_relative_path(
    path: &Path,
    label: &str,
    diagnostics: &Diagnostics,
) -> Result<(), String> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(format!("{label} must be a relative path"));
    }
    for component in path.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(diagnostics.escaping_path(label, path));
        }
    }
    Ok(())
}

#[inline]
pub(super) fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes =
        fs::read(path).map_err(|source| format!("cannot read {}: {source}", path.display()))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

#[inline]
pub(super) fn contained_file(
    root: &Path,
    path: &Path,
    label: &str,
    diagnostics: &Diagnostics,
) -> Result<PathBuf, String> {
    safe_relative_path(path, label, diagnostics)?;
    let mut cursor = root.to_path_buf();
    for component in path.components() {
        let Component::Normal(part) = component else {
            return Err(diagnostics.non_normal_component(label));
        };
        cursor.push(part);
        let metadata = fs::symlink_metadata(&cursor).map_err(|source| {
            format!("{label} is unavailable at {}: {source}", cursor.display())
        })?;
        if metadata.file_type().is_symlink() {
            return Err(format!("{label} contains a symlink: {}", cursor.display()));
        }
    }
    let canonical = cursor
        .canonicalize()
        .map_err(|source| format!("{label} cannot be canonicalized: {source}"))?;
    if !canonical.starts_with(root) {
        return Err(diagnostics.escaping_root(label));
    }
    Ok(canonical)
}
