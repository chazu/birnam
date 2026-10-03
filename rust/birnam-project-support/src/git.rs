//! Bounded Git effects and pinned-cache validation. No registry or build state.
use crate::*;
use birnam_artifact::digest::sha256_hex;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};
pub fn project_git_dependency_cache_path(
    project_root: &Path,
    url: &str,
    revision: &str,
) -> PathBuf {
    let mut identity = Vec::with_capacity(url.len() + revision.len() + 1);
    identity.extend_from_slice(url.as_bytes());
    identity.push(0);
    identity.extend_from_slice(revision.as_bytes());
    let digest = String::from_utf8_lossy(&sha256_hex(&identity)).into_owned();
    project_root.join(".birnam/dependencies/git").join(digest)
}

pub fn git_output(
    name: &str,
    operation: &'static str,
    current_dir: &Path,
    arguments: &[OsString],
) -> Result<std::process::Output, ProjectDependencyResolutionError> {
    let output = std::process::Command::new("git")
        .current_dir(current_dir)
        .args(arguments)
        .output()
        .map_err(|error| ProjectDependencyResolutionError::GitSpawn {
            name: name.to_owned(),
            operation,
            kind: error.kind(),
        })?;
    if !output.status.success() {
        let mut detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if detail.is_empty() {
            detail = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        }
        if detail.len() > 16 * 1024 {
            detail.truncate(16 * 1024);
        }
        return Err(ProjectDependencyResolutionError::GitCommand {
            name: name.to_owned(),
            operation,
            code: output.status.code(),
            detail,
        });
    }
    Ok(output)
}

pub fn validate_cached_git_dependency(
    name: &str,
    revision: &str,
    path: &Path,
) -> Result<(), ProjectDependencyResolutionError> {
    if !path.is_dir() {
        return Err(ProjectDependencyResolutionError::GitCacheMissing {
            name: name.to_owned(),
            path: path.to_path_buf(),
        });
    }
    if !path.join("birnam.toml").is_file() {
        return Err(ProjectDependencyResolutionError::GitCacheInvalid {
            name: name.to_owned(),
            path: path.to_path_buf(),
            reason: "checkout does not contain birnam.toml".to_owned(),
        });
    }
    let head_path = path.join(".git/HEAD");
    let actual = std::fs::read_to_string(&head_path)
        .map_err(|error| ProjectDependencyResolutionError::GitCacheInvalid {
            name: name.to_owned(),
            path: path.to_path_buf(),
            reason: format!("cannot read detached HEAD: {}", error.kind()),
        })?
        .trim()
        .to_ascii_lowercase();
    if actual != revision {
        return Err(ProjectDependencyResolutionError::GitCacheInvalid {
            name: name.to_owned(),
            path: path.to_path_buf(),
            reason: format!("expected revision {revision}, found {actual}"),
        });
    }
    let status = git_output(
        name,
        "inspect cached checkout",
        path,
        &[
            OsString::from("status"),
            OsString::from("--porcelain"),
            OsString::from("--untracked-files=no"),
        ],
    )?;
    if !status.stdout.is_empty() {
        return Err(ProjectDependencyResolutionError::GitCacheInvalid {
            name: name.to_owned(),
            path: path.to_path_buf(),
            reason: "checkout has tracked modifications".to_owned(),
        });
    }
    Ok(())
}
