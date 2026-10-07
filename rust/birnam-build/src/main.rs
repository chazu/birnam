use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use sha2::{Digest, Sha256};

// Bootstrap artifacts are admitted before the source parser exists.
const BOOTSTRAP_MODULES: &[(&str, &str)] = &[
    ("error", "modules/error.birnam"),
    ("modules", "modules/modules.birnam"),
    ("lang", "modules/lang.birnam"),
    ("nativeValues", "modules/native-values.birnam"),
    ("booleanValues", "modules/boolean.birnam"),
    ("integerValues", "modules/integer.birnam"),
    ("blockLoopControl", "modules/block-loop-control.birnam"),
    ("blockLoop", "modules/block-loop.birnam"),
    ("range", "modules/range.birnam"),
    ("systemGc", "modules/system-gc.birnam"),
    ("printing", "modules/printing.birnam"),
    ("sourceLoader", "modules/source-loader.birnam"),
    ("nativeLoader", "modules/native-loader.birnam"),
    ("alienCompiler", "modules/alien-compiler.birnam"),
    ("imageFormat", "modules/image-format.birnam"),
];

// Migration candidates installed for explicit loading, outside startup images.
const OPTIONAL_MODULES: &[(&str, &str)] = &[];

// Authored production tools, compiled through the installed Id artifact path.
const TOOL_MODULES: &[(&str, &str)] = &[
    ("stdlib", "modules/stdlib.birnam"),
    ("stacklessControl", "modules/stackless-control.birnam"),
    ("compiledUnit", "modules/compiled-unit.birnam"),
    ("toml", "modules/toml.birnam"),
    ("config", "modules/config.birnam"),
    ("doc", "modules/doc.birnam"),
    ("cli", "modules/cli.birnam"),
    ("file", "modules/file.birnam"),
    ("html", "modules/html.birnam"),
    ("http", "modules/http.birnam"),
    ("json", "modules/json.birnam"),
    ("log", "modules/log.birnam"),
    ("lsp", "modules/lsp.birnam"),
    ("markdown", "modules/markdown.birnam"),
    ("os", "modules/os.birnam"),
    ("process", "modules/process.birnam"),
    ("random", "modules/random.birnam"),
    ("repl", "modules/repl.birnam"),
    ("socket", "modules/socket.birnam"),
    ("time", "modules/time.birnam"),
    ("nativedoc", "modules/nativedoc.birnam"),
    ("compiledUnitDocs", "modules/compiled-unit-docs.birnam"),
    ("concurrencyDocs", "modules/concurrency-docs.birnam"),
];

const CANONICAL_DYNAMIC_LIBRARIES: &[&str] = &[
    "libbirnam_id.dylib",
    "libbirnam_root_registry.dylib",
    "libbirnam_namespace.dylib",
    "libbirnam_boolean.dylib",
    "libbirnam_nil.dylib",
    "libbirnam_bootstrap.dylib",
    "libbirnam_host_entry.dylib",
    "libbirnam_host_interaction.dylib",
    "libbirnam_host_project.dylib",
    "libbirnam_project_build.dylib",
    "libbirnam_collect.dylib",
    "libbirnam_object_model.dylib",
    "libbirnam_string.dylib",
    "libbirnam_bytes.dylib",
    "libbirnam_integer.dylib",
    "libbirnam_float.dylib",
    "libbirnam_word.dylib",
    "libbirnam_vector.dylib",
    "libbirnam_map.dylib",
    "libbirnam_execution.dylib",
    "libbirnam_peg.dylib",
    "libbirnam_tcc.dylib",
    "libbirnam_codegen_runtime.dylib",
    "libbirnam_native_backend.dylib",
    "libbirnam_generation.dylib",
    "libbirnam_store.dylib",
    "libbirnam_relational.dylib",
    "libbirnam_prelude.dylib",
    "libbirnam_system_output.dylib",
    "libbirnam_system_input.dylib",
    "libbirnam_environment.dylib",
    "libbirnam_file_io.dylib",
    "libbirnam_storage.dylib",
    "libbirnam_trace.dylib",
    "libbirnam_artifact.dylib",
    "libbirnam_portable_code.dylib",
    "libbirnam_image.dylib",
    "libbirnam_alien.dylib",
    "libbirnam_isolate.dylib",
];

const WRAPPED_DYNAMIC_LIBRARIES: &[(&str, &str)] = &[
    ("wrapped/linenoise/liblinenoise.dylib", "liblinenoise.dylib"),
    ("wrapped/termbox2/libtermbox2.dylib", "libtermbox2.dylib"),
];

fn usage() {
    eprintln!(
        "usage: birnam-build install --root DIR --prefix DIR [--profile debug|release] [--build-cache DIR]"
    );
}

#[derive(Debug)]
struct InstallOptions {
    root: PathBuf,
    prefix: PathBuf,
    profile: String,
    build_cache: Option<PathBuf>,
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
    let mut build_cache = None;
    let mut args = args.peekable();
    while let Some(option) = args.next() {
        match option.to_string_lossy().as_ref() {
            "--root" => root = args.next().map(PathBuf::from),
            "--prefix" => prefix = args.next().map(PathBuf::from),
            "--build-cache" => {
                build_cache =
                    Some(absolute(PathBuf::from(args.next().ok_or_else(|| {
                        "--build-cache requires a value".to_owned()
                    })?))?);
            }
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
        build_cache,
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
    for relative in [
        Path::new("Cargo.toml"),
        Path::new("Cargo.lock"),
        Path::new("VERSION"),
    ] {
        if !root.join(relative).is_file() {
            return Err(format!("source root is missing {}", relative.display()));
        }
        entries.insert(
            relative.to_string_lossy().into_owned(),
            sha256_file(&root.join(relative))?,
        );
    }
    collect_files(root, Path::new("rust"), &mut entries, &|_| true)?;
    collect_files(root, Path::new("vendor/libtcc-src"), &mut entries, &|_| {
        true
    })?;
    collect_files(root, Path::new("modules"), &mut entries, &|path| {
        path.extension().and_then(|value| value.to_str()) == Some("birnam")
    })?;
    collect_files(root, Path::new("modules/alien-compiler-assets"), &mut entries, &|_| true)?;
    collect_files(root, Path::new("docs/guide"), &mut entries, &|path| {
        path.extension().and_then(|value| value.to_str()) == Some("md")
    })?;
    for (relative, _) in WRAPPED_DYNAMIC_LIBRARIES {
        let relative = Path::new(relative);
        if root.join(relative).is_file() {
            entries.insert(
                relative.to_string_lossy().into_owned(),
                sha256_file(&root.join(relative))?,
            );
        }
    }
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

fn ensure_replaceable_prefix(prefix: &Path) -> Result<(), String> {
    if !prefix.exists() {
        return Ok(());
    }
    if fs::symlink_metadata(prefix)
        .map_err(|error| format!("stat existing prefix {}: {error}", prefix.display()))?
        .file_type()
        .is_symlink()
    {
        return Err(format!(
            "refusing to replace symlink prefix: {}",
            prefix.display()
        ));
    }
    if !prefix.is_dir() {
        return Err(format!(
            "refusing to replace non-directory prefix: {}",
            prefix.display()
        ));
    }
    let mut entries = fs::read_dir(prefix)
        .map_err(|error| format!("read existing prefix {}: {error}", prefix.display()))?;
    if entries.next().is_none() {
        return Ok(());
    }

    let receipt_relative = Path::new("share/birnam/rust-install.json");
    let receipt_path = prefix.join(receipt_relative);
    let receipt_bytes = fs::read(&receipt_path).map_err(|error| {
        format!(
            "refusing to replace non-empty prefix without a readable Birnam install receipt at {}: {error}; choose a dedicated --prefix",
            receipt_path.display()
        )
    })?;
    let receipt: serde_json::Value = serde_json::from_slice(&receipt_bytes).map_err(|error| {
        format!(
            "refusing to replace prefix with invalid Birnam install receipt {}: {error}",
            receipt_path.display()
        )
    })?;
    let manifest = receipt
        .get("staged_manifest")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| {
            format!(
                "refusing to replace prefix whose Birnam receipt has no staged_manifest: {}",
                receipt_path.display()
            )
        })?;

    let current = installed_manifest(prefix, receipt_relative)?;
    let unexpected = current
        .keys()
        .filter(|relative| {
            !manifest.contains_key(*relative)
                && !WRAPPED_DYNAMIC_LIBRARIES
                    .iter()
                    .any(|(_, name)| *relative == &format!("lib/{name}"))
        })
        .take(5)
        .cloned()
        .collect::<Vec<_>>();
    if !unexpected.is_empty() {
        return Err(format!(
            "refusing to replace prefix containing files not owned by Birnam ({}); choose a dedicated --prefix",
            unexpected.join(", ")
        ));
    }
    Ok(())
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

fn command_stdout(program: &str, arguments: &[&str], directory: &Path) -> Option<String> {
    let output = Command::new(program)
        .args(arguments)
        .current_dir(directory)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

struct StagingGuard {
    stage: PathBuf,
    target: Option<PathBuf>,
}

impl Drop for StagingGuard {
    fn drop(&mut self) {
        if self.stage.exists() {
            let _ = fs::remove_dir_all(&self.stage);
        }
        if let Some(target) = &self.target {
            let _ = fs::remove_dir_all(target);
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

fn native_packages() -> impl Iterator<Item = String> {
    CANONICAL_DYNAMIC_LIBRARIES.iter().map(|name| {
        name.strip_prefix("lib")
            .unwrap()
            .strip_suffix(".dylib")
            .unwrap()
            .replace('_', "-")
    })
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
    ensure_replaceable_prefix(&prefix)?;

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

    let mut staging_guard = StagingGuard {
        stage: stage.clone(),
        target: None,
    };
    let cargo_features = env::var("BIRNAM_BUILD_CARGO_FEATURES")
        .ok()
        .filter(|value| !value.trim().is_empty());
    // Reuse is explicitly requested for development. Cold installs keep their
    // independent targets. Cargo fingerprints source edits inside a configuration;
    // separate directories prevent normal/diagnostic products from being mixed.
    let target_dir = if let Some(cache) = &options.build_cache {
        let configuration = format!(
            "{root:?}|{}|{:?}|{}|{:?}|{:?}|{:?}",
            options.profile,
            cargo_features,
            command_version("rustc"),
            env::var_os("CARGO_BUILD_TARGET"),
            env::var_os("RUSTFLAGS"),
            env::var_os("CARGO_ENCODED_RUSTFLAGS")
        );
        cache
            .join("workspace")
            .join(hex::encode(Sha256::digest(configuration.as_bytes())))
    } else {
        parent.join(format!(".birnam-rust-target-{process}"))
    };
    if options.build_cache.is_none() && target_dir.exists() {
        return Err(format!(
            "Rust target staging directory already exists: {}",
            target_dir.display()
        ));
    }
    staging_guard.target = options.build_cache.is_none().then(|| target_dir.clone());
    fs::create_dir_all(&target_dir).map_err(|error| format!("create build target: {error}"))?;
    // Keep the lock through copying products, not just Cargo's build. Otherwise a
    // second installer can replace a dylib between this build and publication.
    let target_lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(target_dir.join(".birnam-install.lock"))
        .map_err(|error| format!("open build target lock: {error}"))?;
    target_lock
        .lock()
        .map_err(|error| format!("lock build target: {error}"))?;
    let cargo_profile = if options.profile == "release" {
        vec!["--release"]
    } else {
        Vec::new()
    };
    if cargo_features.as_ref().is_some_and(|features| features.contains("lang-seed")) {
        return Err("the Lang seed feature is build-only and cannot be requested for a published prefix".into());
    }
    // Explicit Cargo features are recorded in the receipt so diagnostic builds
    // are distinguishable from ordinary installs.
    let mut cargo_arguments = vec![
        "build".to_owned(),
        "-p".to_owned(),
        "birnam-cli".to_owned(),
        "--locked".to_owned(),
        "--offline".to_owned(),
    ];
    cargo_arguments.extend(cargo_profile.iter().map(|value| (*value).to_owned()));
    for package in native_packages() {
        cargo_arguments.extend(["-p".to_owned(), package]);
    }
    if let Some(features) = &cargo_features {
        cargo_arguments.extend(["--features".to_owned(), features.clone()]);
    }
    let mut cargo = Command::new("cargo");
    cargo
        .current_dir(&root)
        .env("CARGO_TARGET_DIR", &target_dir)
        .args(&cargo_arguments)
        .args(["-p", "birnam-lang-seed", "--features", "birnam-bootstrap/lang-seed"]);
    run_status(&mut cargo, "build compiler seed and native modules")?;

    let profile_dir = env::var_os("CARGO_BUILD_TARGET")
        .map_or_else(|| target_dir.clone(), |target| target_dir.join(target))
        .join(&options.profile);
    let host_source = profile_dir.join("birnam");
    let host_destination = stage.join("bin/birnam");
    copy_file(&host_source, &host_destination)?;
    let lib_dir = stage.join("lib");
    for canonical in CANONICAL_DYNAMIC_LIBRARIES {
        let source = profile_dir.join(canonical);
        copy_file(&source, &lib_dir.join(canonical))?;
    }
    for (relative, name) in WRAPPED_DYNAMIC_LIBRARIES {
        let source = root.join(relative);
        if source.is_file() {
            copy_file(&source, &lib_dir.join(name))?;
        }
    }
    // The Rust grammar is a build seed only. Compile the early authored artifacts
    // without evaluating source bootstrap, then remove the seed from the prefix.
    let seed = lib_dir.join("libbirnam_lang_seed.dylib");
    copy_file(&profile_dir.join("libbirnam_lang_seed.dylib"), &seed)?;
    for (name, source) in BOOTSTRAP_MODULES.iter().chain(OPTIONAL_MODULES) {
        let mut compile = Command::new(&host_destination);
        compile.current_dir(&root);
        for (key, _) in env::vars_os() {
            if key.as_encoded_bytes().starts_with(b"BIRNAM_") { compile.env_remove(key); }
        }
        compile.env("BIRNAM_NO_STDLIB", "1");
        if matches!(*name, "imageFormat" | "printing" | "sourceLoader" | "blockLoop" | "range" | "systemGc" | "nativeLoader" | "error" | "nativeValues" | "booleanValues" | "integerValues") {
            // These authored policies use synchronous bodies. Select
            // that representation at build time; startup only maps saved objects.
            let source_path = root.join(source);
            let output_path = lib_dir.join(format!("libbirnam_{name}.dylib"));
            let quote = |p: &Path| serde_json::to_string(&p.to_string_lossy()).unwrap();
            let script = stage.join(format!("compile-{name}.birnam"));
            fs::write(&script, format!(
                "let source := [System readFile: {src}]; let answer := [Codegen compile: source output: {out} from: {src} synchronousBodies: true]; [[[answer isNil] | [answer isError]] ifTrue: [| [System println: answer]; [System exit: 1]]];",
                src = quote(&source_path), out = quote(&output_path)))
                .map_err(|e| format!("write pure module compile request: {e}"))?;
            compile.arg(&script);
            run_status(&mut compile, &format!("compile bootstrap module {name}"))?;
            fs::remove_file(script).map_err(|e| format!("remove compile request: {e}"))?;
        } else {
            compile.arg("--compile-module").arg(root.join(source))
                .arg(lib_dir.join(format!("libbirnam_{name}.dylib")));
            run_status(&mut compile, &format!("compile bootstrap module {name}"))?;
        }
    }
    let production_arguments = ["build", "-p", "birnam-bootstrap", "--locked", "--offline"]
        .into_iter().chain(cargo_profile.iter().copied()).map(str::to_owned).collect::<Vec<_>>();
    let mut production = Command::new("cargo");
    production.current_dir(&root).env("CARGO_TARGET_DIR", &target_dir)
        .args(&production_arguments);
    run_status(&mut production, "build bootstrap without the grammar seed")?;
    copy_file(&profile_dir.join("libbirnam_bootstrap.dylib"), &lib_dir.join("libbirnam_bootstrap.dylib"))?;
    fs::remove_file(&seed).map_err(|e| format!("remove build-only Lang seed: {e}"))?;
    for (name, source) in TOOL_MODULES {
        let mut compile = Command::new(&host_destination);
        compile
            .current_dir(&root)
            .env_remove("BIRNAM_LIB_DIR")
            .env_remove("BIRNAM_ID_DYLIB")
            .env("BIRNAM_FORCE_SOURCE", "1")
            .args(["--compile-module"])
            .arg(root.join(source))
            .arg(lib_dir.join(format!("libbirnam_{name}.dylib")));
        run_status(&mut compile, &format!("compile authored tool {name}"))?;
    }
    // Prepare authored methods and tool adapters once, before atomic publication.
    // This source path keeps cold installs independent of prebuilt Birnam artifacts.
    let bootstrap_script = stage.join("bootstrap.birnam");
    // Return the first language Error instead of discarding a failed load.
    let checked = |operation: &str, next: String| -> String {
        format!(
            "[[:result | [[result isError] ifTrue: [| result] ifFalse: [| {next}]]] value: {operation}]"
        )
    };
    let mut bootstrap_source = String::from("[System saveImage: \"lib/birnam-tools.image\"]");
    for (name, _) in TOOL_MODULES.iter().rev() {
        bootstrap_source = checked(&format!("[Module load: #{name}]"), bootstrap_source);
    }
    bootstrap_source = checked(
        "[System saveImage: \"lib/birnam-bootstrap.image\"]",
        bootstrap_source,
    );
    bootstrap_source = checked("[Repl wrapStdio]", bootstrap_source);
    for name in ["file", "toml", "config", "repl", "socket"].iter().rev() {
        bootstrap_source = checked(&format!("[Module load: #{name}]"), bootstrap_source);
    }
    fs::write(&bootstrap_script, bootstrap_source)
        .map_err(|error| format!("write bootstrap script: {error}"))?;
    let mut prepare = Command::new(&host_destination);
    prepare.current_dir(&stage);
    for (name, _) in std::env::vars_os() {
        if name.as_encoded_bytes().starts_with(b"BIRNAM_") {
            prepare.env_remove(name);
        }
    }
    prepare
        .env("BIRNAM_FORCE_SOURCE", "1")
        .arg(&bootstrap_script);
    run_status(&mut prepare, "prepare compiled startup image")?;
    fs::remove_file(&bootstrap_script)
        .map_err(|error| format!("remove bootstrap script: {error}"))?;
    for name in ["birnam-bootstrap.image", "birnam-tools.image"] {
        if !lib_dir.join(name).is_file() {
            return Err(format!("bootstrap image was not published: {name}"));
        }
    }
    // Qualify the staged image in a fresh process before publication. This also
    // checks Module admission, wrapper relocation and normal console startup.
    let mut probe = Command::new(&host_destination);
    probe
        .current_dir(&stage)
        .arg("--repl")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null());
    for (name, _) in std::env::vars_os() {
        if name.as_encoded_bytes().starts_with(b"BIRNAM_") {
            probe.env_remove(name);
        }
    }
    run_status(&mut probe, "verify compiled startup image and console")?;
    let mut probe = Command::new(&host_destination);
    probe
        .current_dir(&stage)
        .args(["doc", "stdlib"])
        .stdout(std::process::Stdio::null());
    for (name, _) in std::env::vars_os() {
        if name.as_encoded_bytes().starts_with(b"BIRNAM_") {
            probe.env_remove(name);
        }
    }
    run_status(&mut probe, "verify compiled CLI tool image")?;
    drop(target_lock);
    if options.build_cache.is_none() {
        fs::remove_dir_all(&target_dir)
            .map_err(|error| format!("remove Rust target staging directory: {error}"))?;
    }

    let source_root = stage.join("share/birnam/src");
    copy_file(&root.join("Cargo.toml"), &source_root.join("Cargo.toml"))?;
    copy_file(&root.join("Cargo.lock"), &source_root.join("Cargo.lock"))?;
    copy_tree(&root.join("rust"), &source_root.join("rust"))?;
    copy_file(&root.join("VERSION"), &source_root.join("VERSION"))?;
    copy_tree(
        &root.join("vendor/libtcc-src"),
        &source_root.join("vendor/libtcc-src"),
    )?;
    copy_tree_extension(
        &root.join("docs/guide"),
        &stage.join("share/birnam/docs/guide"),
        "md",
    )?;

    copy_tree_extension(
        &root.join("modules"),
        &source_root.join("modules"),
        "birnam",
    )?;

    copy_tree(
        &root.join("modules/alien-compiler-assets"),
        &source_root.join("modules/alien-compiler-assets"),
    )?;

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
        .collect::<Vec<_>>();
    for relative in &expected_libraries {
        if !staged_manifest.contains_key(relative) {
            return Err(format!(
                "Rust install is missing expected artifact: {relative}"
            ));
        }
    }
    for (name, _) in BOOTSTRAP_MODULES.iter().chain(OPTIONAL_MODULES) {
        if !staged_manifest.contains_key(&format!("lib/libbirnam_{name}.dylib")) {
            return Err(format!("missing compiled authored module: {name}"));
        }
    }
    if lib_dir.join("libbirnam_lang_seed.dylib").exists() {
        return Err("build-only Lang seed leaked into the published prefix".into());
    }
    let staged_digest = manifest_digest(&staged_manifest)?;
    let cargo_invocation = std::iter::once("cargo".to_owned())
        .chain(cargo_arguments)
        .chain(["-p", "birnam-lang-seed", "--features", "birnam-bootstrap/lang-seed"].map(str::to_owned))
        .collect::<Vec<_>>();

    let git_commit = command_stdout("git", &["rev-parse", "HEAD"], &root)
        .unwrap_or_else(|| "unavailable".to_owned());
    let git_clean = command_stdout(
        "git",
        &["status", "--porcelain=v1", "--untracked-files=all"],
        &root,
    )
    .is_some_and(|status| status.is_empty());
    let receipt = serde_json::json!({
        "schema": 4,
        "authority": "rust",
        "profile": options.profile,
        "build_mode": if options.build_cache.is_some() { "development-cache" } else { "cold" },
        "source_identity": {
            "git_commit": git_commit,
            "git_clean": git_clean,
        },
        "admission_contract": {
            "target": "aarch64-macos",
            "id_protocol_version": 1,
            "host_commands": ["source", "image", "worker", "compile-module", "artifact", "check", "build", "run", "fetch", "install", "test", "repl", "listen", "lsp", "doc", "docgen", "init", "wrap", "bake", "rebuild", "help", "version"],
            "stdlib_loading": "compiled-image",
        },
        "source_root": root,
        "source_manifest_sha256": source_digest,
        "source_manifest_files": source_before.len(),
        "toolchain": {
            "cargo": command_version("cargo"),
            "rustc": command_version("rustc"),
        },
        "cargo_invocation": cargo_invocation,
        "dynamic_libraries": CANONICAL_DYNAMIC_LIBRARIES,
        "bootstrap_modules": BOOTSTRAP_MODULES.iter().map(|(name, _)| name).collect::<Vec<_>>(),
        "optional_modules": OPTIONAL_MODULES.iter().map(|(name, _)| name).collect::<Vec<_>>(),
        "build_seed": { "grammar": "birnam-lang-seed", "published": false },
        "production_bootstrap_invocation": std::iter::once("cargo".to_owned()).chain(production_arguments).collect::<Vec<_>>(),
        "stage1_modules": TOOL_MODULES.iter().map(|(name, _)| name).collect::<Vec<_>>(),
        "source_bootstrap_modules": ["stdlib", "stacklessControl", "compiledUnit", "isolate"],
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
        "rust-install: published {} ({} native libraries, {} source bootstrap modules, {} authored tools)",
        prefix.display(),
        CANONICAL_DYNAMIC_LIBRARIES.len(),
        4,
        TOOL_MODULES.len()
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
    if command != "install" {
        return fail(format!("unknown command: {}", command.to_string_lossy()));
    }
    let options = match parse_install_options(args) {
        Ok(options) => options,
        Err(error) if error == "help" => {
            usage();
            return ExitCode::SUCCESS;
        }
        Err(error) => return fail(error),
    };
    match install_rust_prefix(options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => fail(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn scratch(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock is after epoch")
            .as_nanos();
        env::temp_dir().join(format!(
            "birnam-build-{name}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn development_cache_is_explicit_and_requires_a_path() {
        let args = ["--root", ".", "--prefix", "/tmp/birnam-test-prefix"];
        let cold = parse_install_options(args.into_iter().map(Into::into)).unwrap();
        assert!(cold.build_cache.is_none());
        let cached = parse_install_options(
            args.into_iter()
                .chain(["--build-cache", ".cargo-target/install"])
                .map(Into::into),
        )
        .unwrap();
        assert!(cached.build_cache.unwrap().is_absolute());
        assert!(
            parse_install_options(args.into_iter().chain(["--build-cache"]).map(Into::into))
                .is_err()
        );
    }

    #[test]
    fn staging_cleanup_preserves_development_cache_and_removes_owned_targets() {
        let root = scratch("staging-cleanup");
        let stage = root.join("stage");
        let cache = root.join("cache");
        fs::create_dir_all(&stage).unwrap();
        fs::create_dir_all(&cache).unwrap();
        drop(StagingGuard {
            stage: stage.clone(),
            target: None,
        });
        assert!(!stage.exists());
        assert!(cache.is_dir());
        fs::create_dir_all(&stage).unwrap();
        drop(StagingGuard {
            stage: stage.clone(),
            target: Some(cache.clone()),
        });
        assert!(!stage.exists());
        assert!(!cache.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mixed_prefix_is_never_replaceable() {
        let prefix = scratch("mixed-prefix");
        fs::create_dir_all(prefix.join("share/chezmoi")).expect("create mixed prefix");
        fs::write(prefix.join("share/chezmoi/README"), b"personal data")
            .expect("write unrelated file");

        let error = ensure_replaceable_prefix(&prefix).expect_err("mixed prefix must be refused");
        assert!(error.contains("without a readable Birnam install receipt"));
        assert_eq!(
            fs::read(prefix.join("share/chezmoi/README")).expect("unrelated file survives"),
            b"personal data"
        );
        fs::remove_dir_all(prefix).expect("remove scratch prefix");
    }

    #[test]
    fn receipt_does_not_authorize_unowned_files() {
        let prefix = scratch("unowned-file");
        fs::create_dir_all(prefix.join("bin")).expect("create bin directory");
        fs::create_dir_all(prefix.join("share/birnam")).expect("create receipt directory");
        fs::write(prefix.join("bin/birnam"), b"owned").expect("write owned file");
        fs::write(
            prefix.join("share/birnam/rust-install.json"),
            br#"{"staged_manifest":{"bin/birnam":"digest"}}"#,
        )
        .expect("write receipt");
        ensure_replaceable_prefix(&prefix).expect("owned prefix is replaceable");

        fs::create_dir_all(prefix.join("lib")).expect("create library directory");
        fs::write(prefix.join("lib/liblinenoise.dylib"), b"legacy wrapper")
            .expect("write legacy wrapped library");
        ensure_replaceable_prefix(&prefix)
            .expect("known Birnam wrapper from the old install task is replaceable");

        fs::write(prefix.join("notes.txt"), b"unowned").expect("write unowned file");
        let error = ensure_replaceable_prefix(&prefix).expect_err("unowned file must be refused");
        assert!(error.contains("notes.txt"));
        assert_eq!(
            fs::read(prefix.join("notes.txt")).expect("unowned file survives"),
            b"unowned"
        );
        fs::remove_dir_all(prefix).expect("remove scratch prefix");
    }
}
