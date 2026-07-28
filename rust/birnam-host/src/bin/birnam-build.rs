use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use sha2::{Digest, Sha256};

use birnam_host::build_driver::{
    CANONICAL_DYNAMIC_LIBRARIES, STAGE1_MODULES, capture_zig_dependencies, execute_shadow_graph,
    parse_and_validate,
};
use birnam_host::instantiated_graph::{
    InstantiationIdentity, parse_and_validate as parse_instantiated_graph,
};

fn usage() {
    eprintln!(
        "usage: birnam-build install --root DIR --prefix DIR [--profile debug|release]\n\
         \x20      birnam-build dry-run --manifest FILE --root DIR\n\
         \x20      birnam-build execute-shadow --manifest FILE --root DIR\n\
         \x20      birnam-build inspect-zig --build-file FILE\n\
         \x20      birnam-build inspect-instantiated --receipt FILE --root DIR \\
         \x20          --target TARGET --profile PROFILE --floor FLOOR \\
         \x20          --distribution DISTRIBUTION \\
         \x20          --provider-selection-sha256 DIGEST \\
         \x20          --host-options-sha256 DIGEST"
    );
}

#[derive(Debug)]
struct InstallOptions {
    root: PathBuf,
    prefix: PathBuf,
    profile: String,
}

fn absolute(path: PathBuf) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path)
    } else {
        env::current_dir()
            .map(|cwd| cwd.join(path))
            .map_err(|error| format!("resolve path: {error}"))
    }
}

fn parse_install_options(
    args: impl Iterator<Item = std::ffi::OsString>,
) -> Result<InstallOptions, String> {
    let mut root = None;
    let mut prefix = None;
    let mut profile = String::from("debug");
    let mut args = args.peekable();
    while let Some(option) = args.next() {
        match option.to_string_lossy().as_ref() {
            "--root" => root = args.next().map(PathBuf::from),
            "--prefix" => prefix = args.next().map(PathBuf::from),
            "--profile" => {
                profile = args
                    .next()
                    .ok_or_else(|| "--profile requires a value".to_owned())?
                    .to_string_lossy()
                    .into_owned();
            }
            "--help" | "-h" => return Err("help".to_owned()),
            other => return Err(format!("unknown option: {other}")),
        }
    }
    if profile != "debug" && profile != "release" {
        return Err(format!("invalid --profile {profile:?}"));
    }
    Ok(InstallOptions {
        root: absolute(root.ok_or_else(|| "--root is required".to_owned())?)?,
        prefix: absolute(prefix.ok_or_else(|| "--prefix is required".to_owned())?)?,
        profile,
    })
}

fn copy_file(source: &Path, destination: &Path) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| format!("destination has no parent: {}", destination.display()))?;
    fs::create_dir_all(parent).map_err(|error| format!("create {}: {error}", parent.display()))?;
    fs::copy(source, destination).map_err(|error| {
        format!(
            "copy {} -> {}: {error}",
            source.display(),
            destination.display()
        )
    })?;
    #[cfg(unix)]
    {
        let mode = fs::metadata(source)
            .map_err(|error| format!("stat {}: {error}", source.display()))?
            .permissions()
            .mode();
        fs::set_permissions(destination, fs::Permissions::from_mode(mode))
            .map_err(|error| format!("chmod {}: {error}", destination.display()))?;
    }
    Ok(())
}

fn copy_tree_extension(source: &Path, destination: &Path, extension: &str) -> Result<(), String> {
    let entries =
        fs::read_dir(source).map_err(|error| format!("read {}: {error}", source.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("read directory entry: {error}"))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = fs::symlink_metadata(&source_path)
            .map_err(|error| format!("stat {}: {error}", source_path.display()))?
            .file_type();
        if file_type.is_symlink() {
            return Err(format!(
                "source tree contains a symlink: {}",
                source_path.display()
            ));
        }
        if file_type.is_dir() {
            copy_tree_extension(&source_path, &destination_path, extension)?;
        } else if file_type.is_file()
            && source_path.extension().and_then(|value| value.to_str()) == Some(extension)
        {
            copy_file(&source_path, &destination_path)?;
        }
    }
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    let entries =
        fs::read_dir(source).map_err(|error| format!("read {}: {error}", source.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("read directory entry: {error}"))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = fs::symlink_metadata(&source_path)
            .map_err(|error| format!("stat {}: {error}", source_path.display()))?
            .file_type();
        if file_type.is_symlink() {
            return Err(format!(
                "source tree contains a symlink: {}",
                source_path.display()
            ));
        }
        if file_type.is_dir() {
            copy_tree(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            copy_file(&source_path, &destination_path)?;
        }
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path)
        .map_err(|error| format!("open {} for hashing: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("read {} for hashing: {error}", path.display()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn collect_files(
    root: &Path,
    relative: &Path,
    entries: &mut BTreeMap<String, String>,
    include: &dyn Fn(&Path) -> bool,
) -> Result<(), String> {
    let directory = root.join(relative);
    let mut children = fs::read_dir(&directory)
        .map_err(|error| format!("read {}: {error}", directory.display()))?
        .map(|entry| entry.map_err(|error| format!("read directory entry: {error}")))
        .collect::<Result<Vec<_>, _>>()?;
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        let child_relative = relative.join(child.file_name());
        let child_path = root.join(&child_relative);
        let file_type = fs::symlink_metadata(&child_path)
            .map_err(|error| format!("stat {}: {error}", child_path.display()))?
            .file_type();
        if file_type.is_symlink() {
            return Err(format!(
                "source tree contains a symlink: {}",
                child_path.display()
            ));
        }
        if file_type.is_dir() {
            collect_files(root, &child_relative, entries, include)?;
        } else if file_type.is_file() && include(&child_relative) {
            let key = child_relative.to_string_lossy().replace('\\', "/");
            entries.insert(key, sha256_file(&child_path)?);
        }
    }
    Ok(())
}

fn source_manifest(root: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut entries = BTreeMap::new();
    for relative in [Path::new("Cargo.toml"), Path::new("Cargo.lock")] {
        if !root.join(relative).is_file() {
            return Err(format!("source root is missing {}", relative.display()));
        }
        entries.insert(
            relative.to_string_lossy().into_owned(),
            sha256_file(&root.join(relative))?,
        );
    }
    collect_files(root, Path::new("rust"), &mut entries, &|_| true)?;
    collect_files(root, Path::new("src/id"), &mut entries, &|path| {
        path.extension().and_then(|value| value.to_str()) == Some("birnam")
    })?;
    collect_files(root, Path::new("modules"), &mut entries, &|path| {
        path.extension().and_then(|value| value.to_str()) == Some("birnam")
    })?;
    collect_files(root, Path::new("docs/guide"), &mut entries, &|path| {
        path.extension().and_then(|value| value.to_str()) == Some("md")
    })?;
    Ok(entries)
}

fn manifest_digest(entries: &BTreeMap<String, String>) -> Result<String, String> {
    let bytes = serde_json::to_vec(entries).map_err(|error| format!("encode manifest: {error}"))?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(hex::encode(hasher.finalize()))
}

fn installed_manifest(root: &Path, excluded: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut entries = BTreeMap::new();
    let excluded = excluded.to_string_lossy().replace('\\', "/");
    fn visit(
        root: &Path,
        directory: &Path,
        excluded: &str,
        entries: &mut BTreeMap<String, String>,
    ) -> Result<(), String> {
        let mut children = fs::read_dir(directory)
            .map_err(|error| format!("read {}: {error}", directory.display()))?
            .map(|entry| entry.map_err(|error| format!("read directory entry: {error}")))
            .collect::<Result<Vec<_>, _>>()?;
        children.sort_by_key(|entry| entry.file_name());
        for child in children {
            let path = child.path();
            let file_type = fs::symlink_metadata(&path)
                .map_err(|error| format!("stat {}: {error}", path.display()))?
                .file_type();
            if file_type.is_symlink() {
                return Err(format!(
                    "installed tree contains a symlink: {}",
                    path.display()
                ));
            }
            if file_type.is_dir() {
                visit(root, &path, excluded, entries)?;
            } else if file_type.is_file() {
                let relative = path
                    .strip_prefix(root)
                    .expect("installed path is below root")
                    .to_string_lossy()
                    .replace('\\', "/");
                if relative != excluded {
                    entries.insert(relative, sha256_file(&path)?);
                }
            }
        }
        Ok(())
    }
    visit(root, root, &excluded, &mut entries)?;
    Ok(entries)
}

fn sync_tree(root: &Path) -> Result<(), String> {
    fn visit(path: &Path) -> Result<(), String> {
        let metadata = fs::symlink_metadata(path)
            .map_err(|error| format!("stat {}: {error}", path.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!("cannot sync symlink: {}", path.display()));
        }
        if metadata.is_dir() {
            let mut children = fs::read_dir(path)
                .map_err(|error| format!("read {}: {error}", path.display()))?
                .map(|entry| entry.map_err(|error| format!("read directory entry: {error}")))
                .collect::<Result<Vec<_>, _>>()?;
            children.sort_by_key(|entry| entry.file_name());
            for child in children {
                visit(&child.path())?;
            }
        }
        fs::File::open(path)
            .and_then(|file| file.sync_all())
            .map_err(|error| format!("sync {}: {error}", path.display()))
    }

    visit(root)
}

fn command_version(program: &str) -> String {
    Command::new(program)
        .arg("--version")
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unavailable".to_owned())
}

struct StagingGuard {
    stage: PathBuf,
    target: PathBuf,
}

impl Drop for StagingGuard {
    fn drop(&mut self) {
        if self.stage.exists() {
            let _ = fs::remove_dir_all(&self.stage);
        }
        if self.target.exists() {
            let _ = fs::remove_dir_all(&self.target);
        }
    }
}

fn run_status(command: &mut Command, label: &str) -> Result<(), String> {
    let status = command
        .status()
        .map_err(|error| format!("{label}: spawn: {error}"))?;
    if !status.success() {
        return Err(format!("{label}: exited with {status}"));
    }
    Ok(())
}

fn native_library_source(canonical: &str) -> &'static str {
    match canonical {
        "libbirnam_id.dylib" => "libbirnam_id.dylib",
        "libbirnam_id_artifact.dylib" => "libbirnam_artifact.dylib",
        "libbirnam_store.dylib" => "libbirnam_store.dylib",
        "libbirnam_id_peg.dylib" => "libbirnam_peg.dylib",
        "libbirnam_id_lang.dylib" => "libbirnam_lang.dylib",
        "libbirnam_id_tcc.dylib" => "libbirnam_tcc.dylib",
        "libbirnam_id_codegen.dylib" => "libbirnam_codegen_runtime.dylib",
        "libbirnam_id_prelude.dylib" => "libbirnam_prelude.dylib",
        "libbirnam_id_collection.dylib" => "libbirnam_collection.dylib",
        "libbirnam_id_error.dylib" => "libbirnam_error.dylib",
        "libbirnam_id_isolate.dylib" => "libbirnam_isolate.dylib",
        "libbirnam_id_relational.dylib" => "libbirnam_relational.dylib",
        "libbirnam_id_alien.dylib" => "libbirnam_alien.dylib",
        "libbirnam_id_image.dylib" => "libbirnam_image.dylib",
        _ => unreachable!("canonical dynamic library list is closed"),
    }
}

fn install_rust_prefix(options: InstallOptions) -> Result<(), String> {
    let root = fs::canonicalize(&options.root).map_err(|error| {
        format!(
            "canonicalize source root {}: {error}",
            options.root.display()
        )
    })?;
    if !root.join("Cargo.toml").is_file() {
        return Err(format!("source root has no Cargo.toml: {}", root.display()));
    }
    let prefix_name = options
        .prefix
        .file_name()
        .ok_or_else(|| "--prefix has no final path component".to_owned())?;
    let parent = options
        .prefix
        .parent()
        .ok_or_else(|| "--prefix has no parent".to_owned())?;
    fs::create_dir_all(parent).map_err(|error| format!("create prefix parent: {error}"))?;
    let parent = fs::canonicalize(parent)
        .map_err(|error| format!("canonicalize prefix parent {}: {error}", parent.display()))?;
    let prefix = parent.join(prefix_name);
    if parent.starts_with(&root) || prefix == root || prefix.starts_with(&root) {
        return Err("--prefix must not be inside the source root".to_owned());
    }
    if prefix.exists() {
        let resolved = fs::canonicalize(&prefix).map_err(|error| {
            format!("canonicalize existing prefix {}: {error}", prefix.display())
        })?;
        if resolved == root || resolved.starts_with(&root) {
            return Err("--prefix must not resolve inside the source root".to_owned());
        }
    }

    let source_before = source_manifest(&root)?;
    let source_digest = manifest_digest(&source_before)?;
    let process = std::process::id();
    let stage = parent.join(format!(".birnam-rust-install-{process}"));
    if stage.exists() {
        return Err(format!(
            "staging prefix already exists: {}",
            stage.display()
        ));
    }
    fs::create_dir_all(stage.join("bin"))
        .map_err(|error| format!("create staging prefix: {error}"))?;

    // Keep the bootstrap build's compiler products outside both the source
    // tree and the published prefix. This makes each invocation consume only
    // the authored workspace, never a stale Birnam binary/dylib from a prior
    // target directory.
    let target_dir = parent.join(format!(".birnam-rust-target-{process}"));
    if target_dir.exists() {
        return Err(format!(
            "Rust target staging directory already exists: {}",
            target_dir.display()
        ));
    }
    let _staging_guard = StagingGuard {
        stage: stage.clone(),
        target: target_dir.clone(),
    };
    let cargo_profile = if options.profile == "release" {
        vec!["--release"]
    } else {
        Vec::new()
    };
    let mut cargo = Command::new("cargo");
    cargo
        .current_dir(&root)
        .env("CARGO_TARGET_DIR", &target_dir)
        .arg("build")
        .arg("--workspace")
        .arg("--locked")
        .arg("--offline")
        .args(&cargo_profile);
    run_status(&mut cargo, "cargo Rust workspace build")?;

    let profile_dir = target_dir.join(&options.profile);
    let host_source = profile_dir.join("birnam");
    let host_destination = stage.join("bin/birnam");
    copy_file(&host_source, &host_destination)?;
    let lib_dir = stage.join("lib");
    for canonical in CANONICAL_DYNAMIC_LIBRARIES {
        let source = profile_dir.join(native_library_source(canonical));
        copy_file(&source, &lib_dir.join(canonical))?;
    }
    fs::remove_dir_all(&target_dir)
        .map_err(|error| format!("remove Rust target staging directory: {error}"))?;

    let source_root = stage.join("share/birnam/src");
    copy_file(&root.join("Cargo.toml"), &source_root.join("Cargo.toml"))?;
    copy_file(&root.join("Cargo.lock"), &source_root.join("Cargo.lock"))?;
    copy_tree(&root.join("rust"), &source_root.join("rust"))?;
    copy_tree_extension(
        &root.join("docs/guide"),
        &stage.join("share/birnam/docs/guide"),
        "md",
    )?;

    for (source, name) in STAGE1_MODULES {
        let source = root.join(source);
        let output = lib_dir.join(format!("libbirnam_{name}.dylib"));
        let mut compile = Command::new(&host_destination);
        compile
            .current_dir(&root)
            .env_clear()
            .env("PATH", env::var_os("PATH").unwrap_or_default())
            .env("BIRNAM_LIB_DIR", &lib_dir)
            .arg("--compile-module")
            .arg(&source)
            .arg(&output);
        run_status(&mut compile, &format!("stage1 compile {name}"))?;
    }

    let source_after = source_manifest(&root)?;
    let source_after_digest = manifest_digest(&source_after)?;
    if source_after_digest != source_digest {
        return Err(format!(
            "source root changed during Rust install: before={source_digest} after={source_after_digest}"
        ));
    }

    let receipt_relative = Path::new("share/birnam/rust-install.json");
    let staged_manifest = installed_manifest(&stage, receipt_relative)?;
    let expected_libraries = CANONICAL_DYNAMIC_LIBRARIES
        .iter()
        .map(|name| format!("lib/{name}"))
        .chain(
            STAGE1_MODULES
                .iter()
                .map(|(_, name)| format!("lib/libbirnam_{name}.dylib")),
        )
        .collect::<Vec<_>>();
    for relative in &expected_libraries {
        if !staged_manifest.contains_key(relative) {
            return Err(format!(
                "Rust install is missing expected artifact: {relative}"
            ));
        }
    }
    if staged_manifest.keys().any(|path| {
        path.ends_with(".zig") || path.ends_with("_zig.dylib") || path == "bin/birnam-zig-oracle"
    }) {
        return Err("Rust install staged a Zig payload".to_owned());
    }
    let staged_digest = manifest_digest(&staged_manifest)?;
    let mut cargo_invocation = vec![
        "cargo".to_owned(),
        "build".to_owned(),
        "--workspace".to_owned(),
        "--locked".to_owned(),
        "--offline".to_owned(),
    ];
    cargo_invocation.extend(cargo_profile.iter().map(|value| (*value).to_owned()));

    let receipt = serde_json::json!({
        "schema": 2,
        "authority": "rust",
        "profile": options.profile,
        "source_root": root,
        "source_manifest_sha256": source_digest,
        "source_manifest_files": source_before.len(),
        "toolchain": {
            "cargo": command_version("cargo"),
            "rustc": command_version("rustc"),
        },
        "cargo_invocation": cargo_invocation,
        "dynamic_libraries": CANONICAL_DYNAMIC_LIBRARIES,
        "stage1_modules": STAGE1_MODULES.iter().map(|(_, name)| *name).collect::<Vec<_>>(),
        "staged_manifest_sha256": staged_digest,
        "staged_manifest_files": staged_manifest.len(),
        "staged_manifest": staged_manifest,
        "publication": {
            "state": "published",
            "method": "atomic-sibling-rename",
            "durability": "fsync-staged-files-and-directories-before-and-after-rename",
            "requested_prefix": prefix,
            "generation": process,
            "receipt_excludes_itself_from_staged_manifest": true,
        },
        "zig": "unreachable",
    });
    let receipt_path = stage.join("share/birnam/rust-install.json");
    fs::create_dir_all(receipt_path.parent().expect("receipt parent"))
        .map_err(|error| format!("create receipt directory: {error}"))?;
    fs::write(
        &receipt_path,
        serde_json::to_vec_pretty(&receipt).expect("receipt JSON"),
    )
    .map_err(|error| format!("write {}: {error}", receipt_path.display()))?;

    // Make the complete staged closure durable before it can become visible.
    // The parent directory is synced again after each rename below, so a
    // crash cannot expose a published name whose contents were never committed.
    sync_tree(&stage)?;
    fs::File::open(&parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("sync install parent before publication: {error}"))?;

    let backup = parent.join(format!(".birnam-rust-install-backup-{process}"));
    if backup.exists() {
        return Err(format!(
            "backup prefix already exists: {}",
            backup.display()
        ));
    }
    if prefix.exists() {
        fs::rename(&prefix, &backup).map_err(|error| format!("stage existing prefix: {error}"))?;
        fs::File::open(&parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| format!("sync install parent after backup rename: {error}"))?;
    }
    if let Err(error) = fs::rename(&stage, &prefix) {
        if backup.exists() {
            let _ = fs::rename(&backup, &prefix);
            let _ = fs::File::open(&parent).and_then(|directory| directory.sync_all());
        }
        return Err(format!("publish Rust prefix: {error}"));
    }
    fs::File::open(&parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("sync install parent after publication: {error}"))?;
    if backup.exists() {
        fs::remove_dir_all(&backup)
            .map_err(|error| format!("remove old prefix backup: {error}"))?;
        fs::File::open(&parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| format!("sync install parent after backup removal: {error}"))?;
    }
    println!(
        "rust-install: published {} ({} native libraries, {} stage1 modules)",
        prefix.display(),
        CANONICAL_DYNAMIC_LIBRARIES.len(),
        STAGE1_MODULES.len()
    );
    Ok(())
}

fn fail(message: impl std::fmt::Display) -> ExitCode {
    eprintln!("birnam-build: {message}");
    usage();
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let mut args = env::args_os().skip(1);
    let Some(command) = args.next() else {
        return fail("missing command");
    };
    if command == "install" {
        let options = match parse_install_options(args) {
            Ok(options) => options,
            Err(error) if error == "help" => {
                usage();
                return ExitCode::SUCCESS;
            }
            Err(error) => return fail(error),
        };
        return match install_rust_prefix(options) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => fail(error),
        };
    }
    if command != "dry-run"
        && command != "execute-shadow"
        && command != "inspect-zig"
        && command != "inspect-instantiated"
    {
        return fail(format!("unknown command: {}", command.to_string_lossy()));
    }
    if command == "inspect-zig" {
        let Some(option) = args.next() else {
            return fail("--build-file is required");
        };
        if option != "--build-file" {
            return fail(format!("unknown option: {}", option.to_string_lossy()));
        }
        let Some(path) = args.next() else {
            return fail("--build-file is required");
        };
        if args.next().is_some() {
            return fail("inspect-zig accepts only --build-file");
        }
        let path = PathBuf::from(path);
        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error) => return fail(format!("cannot read {}: {error}", path.display())),
        };
        let capture = match capture_zig_dependencies(&source) {
            Ok(capture) => capture,
            Err(error) => return fail(error),
        };
        println!("dependOn-occurrences: {}", capture.occurrence_count);
        for edge in capture.edges {
            println!(
                "line {}: {} -> {}",
                edge.source_line, edge.source, edge.dependency
            );
        }
        return ExitCode::SUCCESS;
    }
    if command == "inspect-instantiated" {
        let mut receipt = None;
        let mut root = None;
        let mut target = None;
        let mut profile = None;
        let mut floor = None;
        let mut distribution = None;
        let mut provider_selection_sha256 = None;
        let mut host_options_sha256 = None;
        while let Some(option) = args.next() {
            let mut value = || args.next().map(PathBuf::from);
            match option.to_string_lossy().as_ref() {
                "--receipt" => receipt = value(),
                "--root" => root = value(),
                "--target" => {
                    target = args
                        .next()
                        .map(|value| value.to_string_lossy().into_owned())
                }
                "--profile" => {
                    profile = args
                        .next()
                        .map(|value| value.to_string_lossy().into_owned())
                }
                "--floor" => {
                    floor = args
                        .next()
                        .map(|value| value.to_string_lossy().into_owned())
                }
                "--distribution" => {
                    distribution = args
                        .next()
                        .map(|value| value.to_string_lossy().into_owned())
                }
                "--provider-selection-sha256" => {
                    provider_selection_sha256 = args
                        .next()
                        .map(|value| value.to_string_lossy().into_owned())
                }
                "--host-options-sha256" => {
                    host_options_sha256 = args
                        .next()
                        .map(|value| value.to_string_lossy().into_owned())
                }
                other => return fail(format!("unknown option: {other}")),
            }
        }
        let Some(receipt_path) = receipt else {
            return fail("--receipt is required");
        };
        let Some(root) = root else {
            return fail("--root is required");
        };
        let Some(target) = target else {
            return fail("--target is required");
        };
        let Some(profile) = profile else {
            return fail("--profile is required");
        };
        let Some(floor) = floor else {
            return fail("--floor is required");
        };
        let Some(distribution) = distribution else {
            return fail("--distribution is required");
        };
        let Some(provider_selection_sha256) = provider_selection_sha256 else {
            return fail("--provider-selection-sha256 is required");
        };
        let Some(host_options_sha256) = host_options_sha256 else {
            return fail("--host-options-sha256 is required");
        };
        let bytes = match fs::read(&receipt_path) {
            Ok(bytes) => bytes,
            Err(error) => {
                return fail(format!("cannot read {}: {error}", receipt_path.display()));
            }
        };
        let expected = InstantiationIdentity {
            target,
            profile,
            floor,
            distribution,
            provider_selection_sha256,
            host_options_sha256,
        };
        let validated = match parse_instantiated_graph(&bytes, &root, &expected) {
            Ok(receipt) => receipt,
            Err(error) => return fail(error),
        };
        println!(
            "inspect-instantiated: target={} profile={} floor={} distribution={} actions={} edges={} raw_occurrences={}",
            validated.identity.target,
            validated.identity.profile,
            validated.identity.floor,
            validated.identity.distribution,
            validated.actions.len(),
            validated.edges.len(),
            validated.raw_occurrences.len(),
        );
        return ExitCode::SUCCESS;
    }
    let mut manifest = None;
    let mut root = None;
    while let Some(option) = args.next() {
        match option.to_string_lossy().as_ref() {
            "--manifest" => manifest = args.next().map(PathBuf::from),
            "--root" => root = args.next().map(PathBuf::from),
            other => return fail(format!("unknown option: {other}")),
        }
    }
    let Some(manifest) = manifest else {
        return fail("--manifest is required");
    };
    let Some(root) = root else {
        return fail("--root is required");
    };
    let bytes = match fs::read(&manifest) {
        Ok(bytes) => bytes,
        Err(error) => return fail(format!("cannot read {}: {error}", manifest.display())),
    };
    let validated = match parse_and_validate(&bytes, &root) {
        Ok(validated) => validated,
        Err(error) => return fail(error),
    };
    if command == "execute-shadow" {
        let executions = match execute_shadow_graph(&validated) {
            Ok(executions) => executions,
            Err(error) => return fail(error),
        };
        println!("execute-shadow: actions={}", executions.len());
        for (index, execution) in executions.iter().enumerate() {
            println!(
                "action[{index}]: name={} status={} tool={} tool_sha256={} cwd={} argv={:?} inputs={:?} outputs={:?} output_digests={:?} stdout={:?} stderr={:?}",
                execution.action,
                execution.status,
                execution.tool.display(),
                execution.tool_sha256,
                execution.cwd.display(),
                execution.argv,
                execution.inputs,
                execution.outputs,
                execution.output_digests,
                execution.stdout,
                execution.stderr
            );
        }
        return ExitCode::SUCCESS;
    }
    println!(
        "dry-run: target={} profile={} distribution={} actions={}",
        validated.target(),
        validated.profile(),
        validated.distribution(),
        validated.action_order().len()
    );
    for (index, action) in validated.action_order().iter().enumerate() {
        let spec = validated.action(action).expect("validated action order");
        println!(
            "action[{index}]: name={} context={} authority={} delegation={} tool={} tool_sha256={} cwd={} argv={:?} depends_on={:?} inputs={:?} outputs={:?} environment={:?}",
            spec.name,
            spec.context,
            spec.authority,
            spec.delegation,
            spec.tool,
            spec.tool_sha256,
            spec.cwd.display(),
            spec.command,
            spec.depends_on,
            spec.inputs,
            spec.outputs,
            spec.environment,
        );
    }
    ExitCode::SUCCESS
}
