//! Project discovery, source walking, Git cache acquisition and build publication.
//! Filesystem staging and rollback retain their existing order and ownership.
use crate::project_data::*;
use std::env;
use birnam_artifact::digest::sha256_hex;
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};

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
pub fn nearest_project_root(start: &Path) -> Result<PathBuf, ProjectDiscoveryError> {
    let mut current =
        std::fs::canonicalize(start).map_err(|error| ProjectDiscoveryError::Canonicalize {
            path: start.to_path_buf(),
            kind: error.kind(),
        })?;
    if !current.is_dir()
        && let Some(parent) = current.parent()
    {
        current = parent.to_path_buf();
    }
    let original = current.clone();
    loop {
        let manifest = current.join("birnam.toml");
        match std::fs::metadata(&manifest) {
            Ok(metadata) if metadata.is_file() => return Ok(current),
            Ok(_) => {
                return Err(ProjectDiscoveryError::Inspect {
                    path: manifest,
                    kind: io::ErrorKind::InvalidData,
                });
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(ProjectDiscoveryError::Inspect {
                    path: manifest,
                    kind: error.kind(),
                });
            }
        }
        let Some(parent) = current.parent() else {
            return Err(ProjectDiscoveryError::NotFound { start: original });
        };
        if parent == current {
            return Err(ProjectDiscoveryError::NotFound { start: original });
        }
        current = parent.to_path_buf();
    }
}

/// Resolve manifest dependency paths against one canonical project root. This
/// is filesystem observation only: it preserves manifest names, canonicalizes
/// each existing target, and does not parse a dependency manifest or choose a
/// provider. Recursive closure assembly remains a separate operation.
pub fn resolve_project_dependency_paths(
    project_root: &Path,
    dependencies: &[ProjectDependencyObservation],
) -> Result<Vec<ProjectDependencyObservation>, ProjectDependencyResolutionError> {
    let mut resolved = Vec::with_capacity(dependencies.len());
    for dependency in dependencies {
        let relative = Path::new(&dependency.root);
        if relative.is_absolute() {
            return Err(ProjectDependencyResolutionError::InvalidPath {
                name: dependency.name.clone(),
                path: relative.to_path_buf(),
            });
        }
        let joined = project_root.join(relative);
        let root = std::fs::canonicalize(&joined).map_err(|error| {
            ProjectDependencyResolutionError::Canonicalize {
                name: dependency.name.clone(),
                path: joined.clone(),
                kind: error.kind(),
            }
        })?;
        resolved.push(ProjectDependencyObservation {
            name: dependency.name.clone(),
            root: root.to_string_lossy().into_owned(),
        });
    }
    resolved.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(resolved)
}

/// Derive a project-local cache path from the complete immutable Git origin.
/// Dependency aliases do not affect identity, so two aliases of one URL/rev
/// pair share a checkout within the owning project.
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

fn git_output(
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

fn validate_cached_git_dependency(
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

/// Resolve manifest dependencies without performing network or cache writes.
/// Git dependencies must already have an exact, clean checkout produced by an
/// explicit fetch operation.
pub fn resolve_project_dependencies_cached(
    project_root: &Path,
    dependencies: &[ProjectManifestDependencyObservation],
) -> Result<Vec<ProjectDependencyObservation>, ProjectDependencyResolutionError> {
    let mut resolved = Vec::with_capacity(dependencies.len());
    for dependency in dependencies {
        let root = match &dependency.source {
            ProjectDependencySourceObservation::Path(path) => {
                let path_dependency = ProjectDependencyObservation {
                    name: dependency.name.clone(),
                    root: path.clone(),
                };
                PathBuf::from(
                    &resolve_project_dependency_paths(project_root, &[path_dependency])?[0].root,
                )
            }
            ProjectDependencySourceObservation::Git { url, revision } => {
                let path = project_git_dependency_cache_path(project_root, url, revision);
                validate_cached_git_dependency(&dependency.name, revision, &path)?;
                std::fs::canonicalize(&path).map_err(|error| {
                    ProjectDependencyResolutionError::Canonicalize {
                        name: dependency.name.clone(),
                        path: path.clone(),
                        kind: error.kind(),
                    }
                })?
            }
        };
        resolved.push(ProjectDependencyObservation {
            name: dependency.name.clone(),
            root: root.to_string_lossy().into_owned(),
        });
    }
    resolved.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(resolved)
}

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

fn collect_project_source_tree(
    root: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), ProjectSourceWalkError> {
    let entries =
        std::fs::read_dir(root).map_err(|error| ProjectSourceWalkError::ReadDirectory {
            path: root.to_path_buf(),
            kind: error.kind(),
        })?;
    for entry in entries {
        let entry = entry.map_err(|error| ProjectSourceWalkError::ReadEntry {
            path: root.to_path_buf(),
            kind: error.kind(),
        })?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| ProjectSourceWalkError::ReadEntry {
                path: path.clone(),
                kind: error.kind(),
            })?;
        if file_type.is_dir() && entry.file_name() == OsStr::new(".birnam") {
            continue;
        }
        if file_type.is_dir() {
            collect_project_source_tree(&path, files)?;
        } else if file_type.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension == "birnam")
        {
            files.push(path);
        }
    }
    Ok(())
}

/// Collect `.birnam` files below project-relative source roots in deterministic
/// lexical order. Directory traversal is read-only; source composition and
/// Codegen metadata remain separate operations.
pub fn collect_project_source_files(
    project_root: &Path,
    source_roots: &[String],
) -> Result<Vec<PathBuf>, ProjectSourceWalkError> {
    let mut files = Vec::new();
    for source_root in source_roots {
        let relative = Path::new(source_root);
        if relative.is_absolute() {
            return Err(ProjectSourceWalkError::InvalidRoot(relative.to_path_buf()));
        }
        collect_project_source_tree(&project_root.join(relative), &mut files)?;
    }
    files.sort();
    Ok(files)
}

pub(super) fn project_publication_paths(
    start: &Path,
    target: &str,
    profile: &str,
) -> Result<(PathBuf, PathBuf, PathBuf, PathBuf), ProjectDiscoveryError> {
    let root = nearest_project_root(start)?;
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

    #[test]
    fn nearest_project_root_walks_from_nested_source_directory() {
        let root =
            std::env::temp_dir().join(format!("birnam-host-project-root-{}", std::process::id()));
        let nested = root.join("src/nested");
        std::fs::create_dir_all(&nested).expect("project fixture creates");
        std::fs::write(root.join("birnam.toml"), b"name = \"fixture\"\n")
            .expect("manifest fixture writes");
        let found = nearest_project_root(&nested).expect("nearest manifest resolves");
        assert_eq!(
            found,
            std::fs::canonicalize(&root).expect("fixture root canonicalizes")
        );
        std::fs::remove_dir_all(&root).expect("project fixture removes");
    }

    #[test]
    fn source_walk_collects_only_birnam_files_in_lexical_order() {
        let root =
            std::env::temp_dir().join(format!("birnam-host-source-walk-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src/nested")).expect("source fixture creates");
        std::fs::write(root.join("src/z.birnam"), b"z").expect("source fixture writes");
        std::fs::write(root.join("src/nested/a.birnam"), b"a").expect("source fixture writes");
        std::fs::write(root.join("src/ignore.txt"), b"ignore").expect("source fixture writes");
        std::fs::create_dir_all(root.join("src/.birnam/dependencies"))
            .expect("internal fixture creates");
        std::fs::write(
            root.join("src/.birnam/dependencies/cached.birnam"),
            b"cached",
        )
        .expect("internal source fixture writes");
        let files =
            collect_project_source_files(&root, &["src".to_owned()]).expect("source walk succeeds");
        assert_eq!(
            files,
            vec![root.join("src/nested/a.birnam"), root.join("src/z.birnam")]
        );
        std::fs::remove_dir_all(&root).expect("source fixture removes");
    }

    #[test]
    fn dependency_paths_canonicalize_without_reordering_names() {
        let root = std::env::temp_dir().join(format!(
            "birnam-host-dependency-paths-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("deps/zeta")).expect("dependency fixture creates");
        std::fs::create_dir_all(root.join("deps/alpha")).expect("dependency fixture creates");
        let resolved = resolve_project_dependency_paths(
            &root,
            &[
                ProjectDependencyObservation {
                    name: "zeta".to_owned(),
                    root: "deps/zeta".to_owned(),
                },
                ProjectDependencyObservation {
                    name: "alpha".to_owned(),
                    root: "deps/alpha".to_owned(),
                },
            ],
        )
        .expect("dependency paths resolve");
        assert_eq!(
            resolved
                .iter()
                .map(|dependency| dependency.name.as_str())
                .collect::<Vec<_>>(),
            ["alpha", "zeta"]
        );
        assert_eq!(
            resolved[0].root,
            std::fs::canonicalize(root.join("deps/alpha"))
                .expect("dependency root canonicalizes")
                .to_string_lossy()
        );
        std::fs::remove_dir_all(&root).expect("dependency fixture removes");
    }

    #[test]
    fn git_dependency_cache_identity_uses_url_and_revision() {
        let root = Path::new("/tmp/project");
        let first = project_git_dependency_cache_path(
            root,
            "https://example.test/library.git",
            "0123456789abcdef0123456789abcdef01234567",
        );
        assert!(first.starts_with(root.join(".birnam/dependencies/git")));
        assert_eq!(
            first.file_name().and_then(OsStr::to_str).map(str::len),
            Some(64)
        );
        assert_ne!(
            first,
            project_git_dependency_cache_path(
                root,
                "https://example.test/library.git",
                "1123456789abcdef0123456789abcdef01234567",
            )
        );
    }

    #[test]
    fn ordinary_resolution_never_fetches_a_missing_git_dependency() {
        let root = std::env::temp_dir().join(format!(
            "birnam-host-missing-git-dependency-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("fixture root creates");
        let dependency = ProjectManifestDependencyObservation {
            name: "library".to_owned(),
            source: ProjectDependencySourceObservation::Git {
                url: "https://example.test/library.git".to_owned(),
                revision: "0123456789abcdef0123456789abcdef01234567".to_owned(),
            },
        };
        assert!(matches!(
            resolve_project_dependencies_cached(&root, &[dependency]),
            Err(ProjectDependencyResolutionError::GitCacheMissing { name, .. })
                if name == "library"
        ));
        assert!(!root.join(".birnam").exists());
        std::fs::remove_dir_all(root).expect("fixture removes");
    }

    #[test]
    fn dependency_paths_reject_absolute_targets_before_filesystem_access() {
        let result = resolve_project_dependency_paths(
            Path::new("/tmp/project"),
            &[ProjectDependencyObservation {
                name: "escape".to_owned(),
                root: "/tmp/outside".to_owned(),
            }],
        );
        assert!(matches!(
            result,
            Err(ProjectDependencyResolutionError::InvalidPath { name, .. }) if name == "escape"
        ));
    }
}
