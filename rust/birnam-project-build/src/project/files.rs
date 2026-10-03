//! Build staging, explicit Git fetch and publication effects.
use super::data::*;
use birnam_project_support::git::*;
use std::{
    env,
    ffi::OsString,
    io,
    path::{Path, PathBuf},
};
pub(super) struct CheckTempDirectory(pub(super) PathBuf);

impl Drop for CheckTempDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(super) fn create_check_temp_directory() -> Result<CheckTempDirectory, io::ErrorKind> {
    let base = env::temp_dir();
    let process = std::process::id();
    for attempt in 0..128u32 {
        let path = base.join(format!("birnam-check-{process}-{attempt}"));
        match std::fs::create_dir(&path) {
            Ok(()) => return Ok(CheckTempDirectory(path)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.kind()),
        }
    }
    Err(io::ErrorKind::AlreadyExists)
}

/// Find the nearest canonical directory containing `birnam.toml`. This is the
/// filesystem half of project observation only; it never parses the manifest
/// or selects a module provider.
pub(super) fn acquire_project_git_dependency(
    project_root: &Path,
    dependency: &ProjectManifestDependencyObservation,
) -> Result<(PathBuf, bool), ProjectDependencyResolutionError> {
    let ProjectDependencySourceObservation::Git { url, revision } = &dependency.source else {
        unreachable!("only Git dependencies are acquired")
    };
    let final_path = project_git_dependency_cache_path(project_root, url, revision);
    if final_path.exists() {
        validate_cached_git_dependency(&dependency.name, revision, &final_path)?;
        return Ok((final_path, false));
    }
    let cache_root = final_path
        .parent()
        .expect("Git dependency cache path always has a parent");
    std::fs::create_dir_all(cache_root).map_err(|error| {
        ProjectDependencyResolutionError::CacheIo {
            path: cache_root.to_path_buf(),
            operation: "create cache directory",
            kind: error.kind(),
        }
    })?;
    let process = std::process::id();
    let mut staging = None;
    for attempt in 0..128u32 {
        let candidate = cache_root.join(format!(".fetch-{process}-{attempt}"));
        match std::fs::create_dir(&candidate) {
            Ok(()) => {
                staging = Some(candidate);
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(ProjectDependencyResolutionError::CacheIo {
                    path: candidate,
                    operation: "create fetch staging directory",
                    kind: error.kind(),
                });
            }
        }
    }
    let staging = staging.ok_or_else(|| ProjectDependencyResolutionError::CacheIo {
        path: cache_root.to_path_buf(),
        operation: "allocate fetch staging directory",
        kind: io::ErrorKind::AlreadyExists,
    })?;
    let acquired = (|| {
        git_output(
            &dependency.name,
            "initialize checkout",
            &staging,
            &[OsString::from("init"), OsString::from("--quiet")],
        )?;
        git_output(
            &dependency.name,
            "configure origin",
            &staging,
            &[
                OsString::from("remote"),
                OsString::from("add"),
                OsString::from("origin"),
                OsString::from(url),
            ],
        )?;
        git_output(
            &dependency.name,
            "fetch pinned revision",
            &staging,
            &[
                OsString::from("fetch"),
                OsString::from("--quiet"),
                OsString::from("--depth=1"),
                OsString::from("origin"),
                OsString::from(revision),
            ],
        )?;
        git_output(
            &dependency.name,
            "checkout pinned revision",
            &staging,
            &[
                OsString::from("checkout"),
                OsString::from("--quiet"),
                OsString::from("--detach"),
                OsString::from("FETCH_HEAD"),
            ],
        )?;
        validate_cached_git_dependency(&dependency.name, revision, &staging)?;
        match std::fs::rename(&staging, &final_path) {
            Ok(()) => Ok((final_path.clone(), true)),
            Err(_) if final_path.exists() => {
                validate_cached_git_dependency(&dependency.name, revision, &final_path)?;
                Ok((final_path.clone(), false))
            }
            Err(error) => Err(ProjectDependencyResolutionError::CacheIo {
                path: final_path.clone(),
                operation: "publish fetched checkout",
                kind: error.kind(),
            }),
        }
    })();
    if staging.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    acquired
}

pub(super) fn project_publication_paths(
    root: &Path,
    target: &str,
    profile: &str,
) -> Result<(PathBuf, PathBuf, PathBuf, PathBuf), ProjectDiscoveryError> {
    let output_root = root.join(".birnam/build").join(target).join(profile);
    let module_root = output_root.join("lib").join(PROJECT_MODULE_ROOT_FILENAME);
    let configuration = output_root.join("lib").join(PROJECT_CONFIGURATION_FILENAME);
    let application = output_root.join("lib").join(PROJECT_APPLICATION_FILENAME);
    Ok((output_root, module_root, configuration, application))
}

/// Publish a complete staging directory by rotating the old profile directory
/// aside and renaming the stage into place. If the final rename fails, the old
/// directory is restored; successful publication leaves no generated backup.
pub fn publish_project_build(
    stage_root: &Path,
    final_root: &Path,
    target_root: &Path,
) -> Result<(), ProjectBuildPublicationError> {
    if stage_root == final_root || stage_root == target_root || final_root == target_root {
        return Err(ProjectBuildPublicationError::InvalidPaths);
    }
    std::fs::create_dir_all(target_root).map_err(|error| {
        ProjectBuildPublicationError::CreateTarget {
            path: target_root.to_path_buf(),
            kind: error.kind(),
        }
    })?;
    let process = std::process::id();
    let mut backup = None;
    for attempt in 0..128u32 {
        let candidate = target_root.join(format!(".previous-{process}-{attempt}"));
        if !candidate.exists() {
            backup = Some(candidate);
            break;
        }
    }
    let backup = backup.ok_or(ProjectBuildPublicationError::Backup {
        path: target_root.to_path_buf(),
        kind: io::ErrorKind::AlreadyExists,
    })?;
    let had_previous = match std::fs::rename(final_root, &backup) {
        Ok(()) => true,
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => {
            return Err(ProjectBuildPublicationError::Backup {
                path: final_root.to_path_buf(),
                kind: error.kind(),
            });
        }
    };
    if let Err(error) = std::fs::rename(stage_root, final_root) {
        if had_previous {
            std::fs::rename(&backup, final_root).map_err(|restore| {
                ProjectBuildPublicationError::Restore {
                    path: final_root.to_path_buf(),
                    kind: restore.kind(),
                }
            })?;
        }
        return Err(ProjectBuildPublicationError::Publish {
            stage: stage_root.to_path_buf(),
            final_root: final_root.to_path_buf(),
            kind: error.kind(),
        });
    }
    if had_previous {
        let _ = std::fs::remove_dir_all(backup);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn build_publication_rotates_a_complete_stage_and_removes_backup() {
        let root = std::env::temp_dir().join(format!(
            "birnam-host-build-publication-{}",
            std::process::id()
        ));
        let target = root.join("target");
        let stage = target.join(".staging");
        let final_root = target.join("debug");
        std::fs::create_dir_all(stage.join("lib")).expect("stage creates");
        std::fs::create_dir_all(final_root.join("lib")).expect("final creates");
        std::fs::write(stage.join("lib/new.dylib"), b"new").expect("new artifact writes");
        std::fs::write(final_root.join("lib/old.dylib"), b"old").expect("old artifact writes");

        publish_project_build(&stage, &final_root, &target).expect("publication succeeds");
        assert_eq!(
            std::fs::read(final_root.join("lib/new.dylib")).expect("new artifact remains"),
            b"new"
        );
        assert!(!final_root.join("lib/old.dylib").exists());
        assert!(
            std::fs::read_dir(&target)
                .expect("target reads")
                .filter_map(Result::ok)
                .all(|entry| !entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".previous-"))
        );
        std::fs::remove_dir_all(root).expect("publication fixture removes");
    }
}
