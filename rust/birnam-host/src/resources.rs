//! Executable-relative resource resolution with the existing lexical and symlink rules.
use std::{
    env, io,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Overrides<'a> {
    pub lib_dir: Option<&'a Path>,
    pub id_dylib: Option<&'a Path>,
    pub rust_workspace_dir: Option<&'a Path>,
}

pub use birnam_host_support::Resources;

/// Resolve one installed layout from the directory containing the executable.
///
/// The resolver normalizes paths lexically and never requires a resource to
/// exist. This keeps startup deterministic for diagnostics and preserves the
/// host's ability to report one missing resource at the actual load boundary.
pub fn resolve_from_exe_dir(exe_dir: &Path, overrides: Overrides<'_>) -> io::Result<Resources> {
    let exe_dir = absolute_normalized(exe_dir)?;
    let prefix = exe_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| exe_dir.clone());
    let lib_dir = override_or(overrides.lib_dir, &prefix.join("lib"))?;
    let rust_workspace_dir = override_or(
        overrides.rust_workspace_dir,
        &prefix.join("share/birnam/src"),
    )?;
    let guide_dir = normalize_absolute(&prefix.join("share/birnam/docs/guide"))?;
    let id_dylib = override_or(overrides.id_dylib, &lib_dir.join("libbirnam_id.dylib"))?;

    Ok(Resources {
        prefix,
        lib_dir,
        rust_workspace_dir,
        guide_dir,
        id_dylib,
    })
}

/// Resolve the production layout from the current executable and its narrow
/// environment overrides.
pub fn resolve() -> io::Result<Resources> {
    let executable = env::current_exe()?;
    let lib_dir = env::var_os("BIRNAM_LIB_DIR").map(PathBuf::from);
    let id_dylib = env::var_os("BIRNAM_ID_DYLIB").map(PathBuf::from);
    let rust_workspace_dir = env::var_os("BIRNAM_RUST_WORKSPACE_DIR").map(PathBuf::from);
    resolve_from_executable(
        &executable,
        Overrides {
            lib_dir: lib_dir.as_deref(),
            id_dylib: id_dylib.as_deref(),
            rust_workspace_dir: rust_workspace_dir.as_deref(),
        },
    )
}

fn resolve_from_executable(executable: &Path, overrides: Overrides<'_>) -> io::Result<Resources> {
    let executable = std::fs::canonicalize(executable).unwrap_or_else(|_| executable.to_path_buf());
    resolve_from_exe_dir(
        executable.parent().unwrap_or_else(|| Path::new(".")),
        overrides,
    )
}

fn override_or(override_path: Option<&Path>, default_path: &Path) -> io::Result<PathBuf> {
    match override_path {
        Some(path) => normalize_absolute(path),
        None => normalize_absolute(default_path),
    }
}

fn absolute_normalized(path: &Path) -> io::Result<PathBuf> {
    if path.is_absolute() {
        normalize_absolute(path)
    } else {
        normalize_absolute(&env::current_dir()?.join(path))
    }
}

fn normalize_absolute(path: &Path) -> io::Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()?.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(Path::new(std::path::MAIN_SEPARATOR_STR)),
            Component::CurDir => {}
            Component::ParentDir => {
                if normalized.file_name().is_some() {
                    normalized.pop();
                }
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    if !normalized.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "runtime resource path did not resolve absolute",
        ));
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uses_one_normalized_executable_relative_layout() {
        let resources = resolve_from_exe_dir(Path::new("/opt/birnam/bin"), Overrides::default())
            .expect("layout should resolve");
        assert_eq!(resources.prefix, Path::new("/opt/birnam"));
        assert_eq!(resources.lib_dir, Path::new("/opt/birnam/lib"));
        assert_eq!(
            resources.rust_workspace_dir,
            Path::new("/opt/birnam/share/birnam/src")
        );
        assert_eq!(
            resources.guide_dir,
            Path::new("/opt/birnam/share/birnam/docs/guide")
        );
        assert_eq!(
            resources.id_dylib,
            Path::new("/opt/birnam/lib/libbirnam_id.dylib")
        );
    }

    #[cfg(unix)]
    #[test]
    fn executable_launcher_symlink_uses_the_target_prefix() {
        use std::os::unix::fs::symlink;

        let scratch = std::fs::canonicalize(std::env::temp_dir())
            .expect("temporary directory canonicalizes")
            .join(format!(
                "birnam-host-launcher-symlink-{}",
                std::process::id()
            ));
        let target = scratch.join("opt/birnam/bin/birnam");
        let launcher = scratch.join("local/bin/birnam");
        std::fs::create_dir_all(target.parent().expect("target has parent"))
            .expect("create target directory");
        std::fs::create_dir_all(launcher.parent().expect("launcher has parent"))
            .expect("create launcher directory");
        std::fs::write(&target, b"host").expect("write target");
        symlink(&target, &launcher).expect("create launcher symlink");

        let resources = resolve_from_executable(&launcher, Overrides::default())
            .expect("launcher should resolve");
        assert_eq!(resources.prefix, scratch.join("opt/birnam"));
        assert_eq!(resources.lib_dir, scratch.join("opt/birnam/lib"));

        std::fs::remove_dir_all(scratch).expect("remove scratch layout");
    }

    #[test]
    fn overrides_remain_narrow_and_exact() {
        let lib_dir = Path::new("/tmp/runtime/lib/../lib");
        let id_dylib = Path::new("/tmp/floor/custom-id.dylib");
        let workspace = Path::new("/tmp/native-workspace/../native-workspace");
        let resources = resolve_from_exe_dir(
            Path::new("/opt/birnam/bin"),
            Overrides {
                lib_dir: Some(lib_dir),
                id_dylib: Some(id_dylib),
                rust_workspace_dir: Some(workspace),
            },
        )
        .expect("overrides should resolve");
        assert_eq!(resources.lib_dir, Path::new("/tmp/runtime/lib"));
        assert_eq!(resources.id_dylib, Path::new("/tmp/floor/custom-id.dylib"));
        assert_eq!(
            resources.rust_workspace_dir,
            Path::new("/tmp/native-workspace")
        );
    }

    #[test]
    fn a_lib_override_supplies_the_default_id_dylib() {
        let resources = resolve_from_exe_dir(
            Path::new("/opt/birnam/bin"),
            Overrides {
                lib_dir: Some(Path::new("/tmp/runtime-lib")),
                ..Overrides::default()
            },
        )
        .expect("lib override should resolve");
        assert_eq!(
            resources.id_dylib,
            Path::new("/tmp/runtime-lib/libbirnam_id.dylib")
        );
    }

    #[test]
    fn lexical_normalization_does_not_require_files_to_exist() {
        let resources = resolve_from_exe_dir(
            Path::new("/tmp/nonexistent/../birnam/bin"),
            Overrides::default(),
        )
        .expect("lexical paths should resolve without filesystem access");
        assert_eq!(resources.prefix, Path::new("/tmp/birnam"));
    }
}
