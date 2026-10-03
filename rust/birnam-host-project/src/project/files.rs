//! Read-only discovery, pinned-cache inspection and source walking.
use super::data::*;
use birnam_project_support::git::*;
use std::{
    ffi::OsStr,
    io,
    path::{Path, PathBuf},
};
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

#[cfg(test)]
mod tests {
    use super::*;
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
