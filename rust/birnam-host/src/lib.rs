//! Pure executable-relative runtime-resource resolution for the Rust host.
//!
//! This is deliberately not a loader and does not inspect the filesystem. It
//! only reproduces the installed Birnam layout and the three narrow resource
//! overrides accepted by the existing Zig host. Loading a floor or any higher
//! rung remains a later host boundary.

use std::collections::HashSet;
use std::env;
use std::ffi::{CStr, CString, OsStr, c_char, c_int, c_void};
use std::io;
use std::marker::PhantomData;
use std::path::{Component, Path, PathBuf};
use std::ptr::NonNull;

use birnam_abi::{IdApi, IdInitFn};

#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Overrides<'a> {
    pub lib_dir: Option<&'a Path>,
    pub id_dylib: Option<&'a Path>,
    pub rust_workspace_dir: Option<&'a Path>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resources {
    pub prefix: PathBuf,
    pub lib_dir: PathBuf,
    pub source_dir: PathBuf,
    pub rust_workspace_dir: PathBuf,
    pub guide_dir: PathBuf,
    pub id_dylib: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EarlyAction {
    Continue,
    Help,
    Version,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandKind {
    Help,
    Version,
    Repl,
    Listen,
    Lsp,
    Doc,
    Docgen,
    Init,
    Check,
    Build,
    Install,
    Run,
    Test,
    Wrap,
    Bake,
    Rebuild,
    CompileModule,
    Worker,
    ProjectTestWorker,
    Script(PathBuf),
    Image(PathBuf),
    Artifact(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    UnknownOption(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Human,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckOptions {
    pub directory: PathBuf,
    pub format: OutputFormat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildOptions {
    pub directory: PathBuf,
    pub profile: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallOptions {
    pub directory: PathBuf,
    pub build: bool,
    pub profile: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestOptions {
    pub directory: PathBuf,
    pub build: bool,
    pub profile: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplMode {
    None,
    Tty,
    Socket,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOptions {
    pub directory: PathBuf,
    pub build: bool,
    pub profile: String,
    pub repl: Option<ReplMode>,
    pub socket_bind: Option<String>,
    pub socket_port: Option<u16>,
    pub arguments: Vec<std::ffi::OsString>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunParseError {
    UnknownOption(String),
    MissingValue(&'static str),
    InvalidValue { option: &'static str, value: String },
    DuplicateDirectory,
    UnexpectedAfterHelp,
    NonUtf8Value(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BakeOptions {
    pub image: PathBuf,
    pub executable: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebuildOptions {
    pub image: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileModuleOptions {
    pub source: PathBuf,
    pub output: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolingParseError {
    Arity {
        command: &'static str,
        expected: usize,
        actual: usize,
    },
}

pub const BAKE_MAGIC: &[u8; 8] = b"BIRNMEXE";
pub const BAKE_TRAILER_LEN: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BakeError {
    EmptyImage,
    ImageTooLarge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptError {
    Read { path: PathBuf, kind: io::ErrorKind },
    TooLarge { path: PathBuf, limit: usize },
    MissingCodegen,
    EvaluationRejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageLoadOutcome {
    Mapped,
    SourceRecovery,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageError {
    MissingSystem,
    LoadRejected,
    UnexpectedStatus,
}

pub const MAX_SCRIPT_BYTES: usize = 16 << 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactEntry {
    pub path: PathBuf,
    pub identity: Vec<u8>,
    pub requires: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactPlan {
    pub entries: Vec<ArtifactEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactPreflightError {
    Read {
        path: PathBuf,
        kind: io::ErrorKind,
    },
    Parse {
        path: PathBuf,
        error: birnam_artifact::Error,
    },
    KindMismatch {
        path: PathBuf,
    },
    IdentityMismatch {
        path: PathBuf,
        expected: Vec<u8>,
        actual: Vec<u8>,
    },
    DependencyCycle {
        identity: Vec<u8>,
    },
    NativeRequirementMissing {
        path: PathBuf,
        requirement: Vec<u8>,
    },
    InvalidDependencyName {
        path: PathBuf,
        dependency: Vec<u8>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactActivationError {
    Preflight(ArtifactPreflightError),
    MissingModuleManager,
    ActivationRejected,
}

/// Append an image trailer to a runtime byte sequence. The runtime remains a
/// valid executable because the loader ignores trailing bytes; recovery uses
/// [`baked_image`] to select the exact image slice.
pub fn bake_executable(runtime: &[u8], image: &[u8]) -> Result<Vec<u8>, BakeError> {
    if image.is_empty() {
        return Err(BakeError::EmptyImage);
    }
    let image_len = u64::try_from(image.len()).map_err(|_| BakeError::ImageTooLarge)?;
    let capacity = runtime
        .len()
        .checked_add(image.len())
        .and_then(|length| length.checked_add(BAKE_TRAILER_LEN))
        .ok_or(BakeError::ImageTooLarge)?;
    let mut result = Vec::with_capacity(capacity);
    result.extend_from_slice(runtime);
    result.extend_from_slice(image);
    result.extend_from_slice(&image_len.to_le_bytes());
    result.extend_from_slice(BAKE_MAGIC);
    Ok(result)
}

/// Recover the image slice from a baked executable byte sequence. Invalid,
/// truncated, empty, or out-of-range trailers answer `None` without I/O.
pub fn baked_image(bytes: &[u8]) -> Option<&[u8]> {
    if bytes.len() < BAKE_TRAILER_LEN || &bytes[bytes.len() - BAKE_MAGIC.len()..] != BAKE_MAGIC {
        return None;
    }
    let length_start = bytes.len() - BAKE_TRAILER_LEN;
    let image_len = u64::from_le_bytes(bytes[length_start..length_start + 8].try_into().ok()?);
    let image_len = usize::try_from(image_len).ok()?;
    if image_len == 0 || image_len > length_start {
        return None;
    }
    Some(&bytes[length_start - image_len..length_start])
}

fn artifact_path_from_bytes(bytes: &[u8]) -> Option<&Path> {
    if bytes.is_empty() || bytes.contains(&0) {
        return None;
    }
    #[cfg(unix)]
    {
        Some(Path::new(OsStr::from_bytes(bytes)))
    }
    #[cfg(not(unix))]
    {
        std::str::from_utf8(bytes).ok().map(Path::new)
    }
}

fn sibling_artifact_path(directory: &Path, identity: &[u8]) -> Option<PathBuf> {
    let mut filename = Vec::with_capacity(b"libbirnam_".len() + identity.len() + b".dylib".len());
    filename.extend_from_slice(b"libbirnam_");
    filename.extend_from_slice(identity);
    filename.extend_from_slice(b".dylib");
    artifact_path_from_bytes(&filename).map(|path| directory.join(path))
}

fn read_artifact_bytes(path: &Path) -> Result<Vec<u8>, ArtifactPreflightError> {
    let metadata = std::fs::metadata(path).map_err(|error| ArtifactPreflightError::Read {
        path: path.to_path_buf(),
        kind: error.kind(),
    })?;
    if metadata.len() > birnam_artifact::MAX_ARTIFACT_BYTES as u64 {
        return Err(ArtifactPreflightError::Parse {
            path: path.to_path_buf(),
            error: birnam_artifact::Error::ArtifactTooLarge,
        });
    }
    std::fs::read(path).map_err(|error| ArtifactPreflightError::Read {
        path: path.to_path_buf(),
        kind: error.kind(),
    })
}

struct ArtifactPreflight {
    entries: Vec<ArtifactEntry>,
    visiting: HashSet<Vec<u8>>,
    inspected: HashSet<Vec<u8>>,
}

impl ArtifactPreflight {
    fn walk(
        &mut self,
        path: &Path,
        expected_identity: Option<&[u8]>,
    ) -> Result<(), ArtifactPreflightError> {
        let bytes = read_artifact_bytes(path)?;
        let descriptor =
            birnam_artifact::inspect(&bytes).map_err(|error| ArtifactPreflightError::Parse {
                path: path.to_path_buf(),
                error,
            })?;
        if descriptor.kind != b"module" {
            return Err(ArtifactPreflightError::KindMismatch {
                path: path.to_path_buf(),
            });
        }
        if let Some(expected) = expected_identity
            && descriptor.identity != expected
        {
            return Err(ArtifactPreflightError::IdentityMismatch {
                path: path.to_path_buf(),
                expected: expected.to_vec(),
                actual: descriptor.identity.to_vec(),
            });
        }
        let identity = descriptor.identity.to_vec();
        if self.inspected.contains(&identity) {
            return Ok(());
        }
        if !self.visiting.insert(identity.clone()) {
            return Err(ArtifactPreflightError::DependencyCycle { identity });
        }
        let directory = path.parent().unwrap_or_else(|| Path::new("."));
        for requirement in descriptor.native_requires.iter() {
            if requirement.is_empty() || requirement.contains(&0) || requirement.contains(&b'/') {
                return Err(ArtifactPreflightError::NativeRequirementMissing {
                    path: path.to_path_buf(),
                    requirement: requirement.to_vec(),
                });
            }
            let Some(requirement_path) = artifact_path_from_bytes(requirement) else {
                return Err(ArtifactPreflightError::NativeRequirementMissing {
                    path: path.to_path_buf(),
                    requirement: requirement.to_vec(),
                });
            };
            if std::fs::metadata(directory.join(requirement_path)).is_err() {
                return Err(ArtifactPreflightError::NativeRequirementMissing {
                    path: path.to_path_buf(),
                    requirement: requirement.to_vec(),
                });
            }
        }
        let requires: Vec<Vec<u8>> = descriptor
            .requires
            .iter()
            .map(|requirement| requirement.to_vec())
            .collect();
        for requirement in &requires {
            if requirement.contains(&0) || requirement.contains(&b'/') {
                return Err(ArtifactPreflightError::InvalidDependencyName {
                    path: path.to_path_buf(),
                    dependency: requirement.clone(),
                });
            }
            let Some(dependency_path) = sibling_artifact_path(directory, requirement) else {
                return Err(ArtifactPreflightError::InvalidDependencyName {
                    path: path.to_path_buf(),
                    dependency: requirement.clone(),
                });
            };
            self.walk(&dependency_path, Some(requirement))?;
        }
        self.visiting.remove(&identity);
        self.inspected.insert(identity.clone());
        self.entries.push(ArtifactEntry {
            path: path.to_path_buf(),
            identity,
            requires,
        });
        Ok(())
    }
}

/// Validate an artifact and its complete sibling dependency closure before any
/// activation. This is the Rust-host replacement seam for the Zig preflight
/// helper: it consumes the same pointer-free descriptor parser and performs no
/// IdApi allocation, module load, or initializer call.
pub fn preflight_artifact(
    path: &Path,
    expected_identity: Option<&[u8]>,
) -> Result<ArtifactPlan, ArtifactPreflightError> {
    let mut preflight = ArtifactPreflight {
        entries: Vec::new(),
        visiting: HashSet::new(),
        inspected: HashSet::new(),
    };
    preflight.walk(path, expected_identity)?;
    Ok(ArtifactPlan {
        entries: preflight.entries,
    })
}

/// Preflight and activate an artifact through the Birnam Module manager. The
/// preflight is completed before any object send; activation remains owned by
/// the self-hosted manager so its Store/MOP/closure rules stay authoritative.
///
/// # Safety
///
/// The runtime must remain on the selected floor's owner thread, and the
/// caller must establish a valid GC stack bottom for synchronous sends.
pub unsafe fn activate_artifact(
    runtime: &HostRuntime,
    path: &Path,
    expected_identity: Option<&[u8]>,
) -> Result<(), ArtifactActivationError> {
    preflight_artifact(path, expected_identity).map_err(ArtifactActivationError::Preflight)?;
    let api = runtime.stack.api();
    let Some(module_manager) = (unsafe { global_named(api, "Module") }) else {
        return Err(ArtifactActivationError::MissingModuleManager);
    };
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let directory_bytes = path_bytes(directory);
    let directory_value =
        unsafe { (api.mkString)(directory_bytes.as_ptr(), directory_bytes.len()) };
    let lib_dir_selector = unsafe { (api.intern)(c"libDir:".as_ptr()) };
    unsafe {
        (api.send)(
            module_manager,
            lib_dir_selector,
            [directory_value].as_ptr(),
            1,
        )
    };
    let alien = unsafe { (api.import)(c"Alien".as_ptr()) };
    if alien != api.nilObj {
        let selector = unsafe { (api.intern)(c"libraryDir:".as_ptr()) };
        unsafe { (api.send)(alien, selector, [directory_value].as_ptr(), 1) };
    }
    let path_bytes = path_bytes(path);
    let path_value = unsafe { (api.mkString)(path_bytes.as_ptr(), path_bytes.len()) };
    let selector = unsafe { (api.intern)(c"loadArtifact:".as_ptr()) };
    let result = unsafe { (api.send)(module_manager, selector, [path_value].as_ptr(), 1) };
    if result == api.nilObj || send_is_error(api, result) {
        return Err(ArtifactActivationError::ActivationRejected);
    }
    Ok(())
}

/// Evaluate one source file through the started host's Codegen object. This is
/// the persistent, path-aware form used by the shipped script command; it lets
/// Codegen record the source for image recovery and reports its `lastError`
/// instead of treating a nil result as success.
///
/// # Safety
///
/// The runtime must remain on the selected floor's owner thread. The caller
/// must set a valid GC stack bottom before invoking this operation and keep it
/// live for the complete synchronous evaluation.
pub unsafe fn eval_script(
    runtime: &HostRuntime,
    path: &Path,
) -> Result<birnam_abi::Oop, ScriptError> {
    let source = std::fs::read(path).map_err(|error| ScriptError::Read {
        path: path.to_path_buf(),
        kind: error.kind(),
    })?;
    if source.len() > MAX_SCRIPT_BYTES {
        return Err(ScriptError::TooLarge {
            path: path.to_path_buf(),
            limit: MAX_SCRIPT_BYTES,
        });
    }
    let api = runtime.stack.api();
    let codegen = unsafe { (api.import)(c"Codegen".as_ptr()) };
    if codegen == api.nilObj {
        return Err(ScriptError::MissingCodegen);
    }
    let source_value = unsafe { (api.mkString)(source.as_ptr(), source.len()) };
    let path_bytes = path_bytes(path);
    let path_value = unsafe { (api.mkString)(path_bytes.as_ptr(), path_bytes.len()) };
    let selector = unsafe { (api.intern)(c"evalPersisting:from:".as_ptr()) };
    let result = unsafe { (api.send)(codegen, selector, [source_value, path_value].as_ptr(), 2) };
    let last_error_selector = unsafe { (api.intern)(c"lastError".as_ptr()) };
    let last_error = unsafe { (api.send)(codegen, last_error_selector, std::ptr::null(), 0) };
    if last_error != api.nilObj {
        return Err(ScriptError::EvaluationRejected);
    }
    Ok(result)
}

/// Load an image through the Birnam `System` protocol. A true answer means
/// compiled code was mapped; false means the runtime must use its recorded
/// source recovery path, for which the existing `replayMethods` and
/// `replayAliens` hooks are invoked after the image decoder returns.
///
/// # Safety
///
/// The runtime must remain on the selected floor's owner thread, and the
/// caller must establish a valid GC stack bottom for the synchronous sends.
pub unsafe fn load_image(
    runtime: &HostRuntime,
    path: &Path,
    rebuild: bool,
) -> Result<ImageLoadOutcome, ImageError> {
    let api = runtime.stack.api();
    let system = unsafe { (api.import)(c"System".as_ptr()) };
    if system == api.nilObj {
        return Err(ImageError::MissingSystem);
    }
    let path_bytes = path_bytes(path);
    let path_value = unsafe { (api.mkString)(path_bytes.as_ptr(), path_bytes.len()) };
    let rebuild_value = if rebuild { api.trueObj } else { api.falseObj };
    let selector = unsafe { (api.intern)(c"loadImage:rebuild:".as_ptr()) };
    let status = unsafe { (api.send)(system, selector, [path_value, rebuild_value].as_ptr(), 2) };
    if status == api.nilObj {
        return Err(ImageError::LoadRejected);
    }
    if status == api.trueObj {
        return Ok(ImageLoadOutcome::Mapped);
    }
    if status != api.falseObj {
        return Err(ImageError::UnexpectedStatus);
    }

    let codegen = unsafe { (api.import)(c"Codegen".as_ptr()) };
    if codegen != api.nilObj {
        let replay_methods = unsafe { (api.intern)(c"replayMethods".as_ptr()) };
        unsafe { (api.send)(codegen, replay_methods, std::ptr::null(), 0) };
        let replay_aliens = unsafe { (api.intern)(c"replayAliens".as_ptr()) };
        unsafe { (api.send)(codegen, replay_aliens, std::ptr::null(), 0) };
    }
    Ok(ImageLoadOutcome::SourceRecovery)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseOutcome<T> {
    Parsed(T),
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectParseError {
    UnknownOption(String),
    MissingValue(&'static str),
    InvalidValue { option: &'static str, value: String },
    DuplicateDirectory,
    UnexpectedAfterHelp,
    NonUtf8Value(&'static str),
}

fn option_value<T: AsRef<std::ffi::OsStr>>(
    option: &'static str,
    value: &T,
) -> Result<String, ProjectParseError> {
    value
        .as_ref()
        .to_str()
        .map(str::to_owned)
        .ok_or(ProjectParseError::NonUtf8Value(option))
}

fn parse_profile(value: String) -> Result<String, ProjectParseError> {
    if value == "debug" || value == "release" {
        Ok(value)
    } else {
        Err(ProjectParseError::InvalidValue {
            option: "--profile",
            value,
        })
    }
}

pub fn parse_check_args<T: AsRef<std::ffi::OsStr>>(
    args: &[T],
) -> Result<ParseOutcome<CheckOptions>, ProjectParseError> {
    let mut directory = None;
    let mut format = OutputFormat::Human;
    let mut index = 0;
    while index < args.len() {
        let token = args[index].as_ref();
        if token == std::ffi::OsStr::new("--help") || token == std::ffi::OsStr::new("-h") {
            if index + 1 != args.len() {
                return Err(ProjectParseError::UnexpectedAfterHelp);
            }
            return Ok(ParseOutcome::Help);
        }
        let token_text = token.to_string_lossy();
        if token == std::ffi::OsStr::new("--format") {
            index += 1;
            let value = args
                .get(index)
                .ok_or(ProjectParseError::MissingValue("--format"))
                .and_then(|value| option_value("--format", value))?;
            format = match value.as_str() {
                "human" => OutputFormat::Human,
                "json" => OutputFormat::Json,
                _ => {
                    return Err(ProjectParseError::InvalidValue {
                        option: "--format",
                        value,
                    });
                }
            };
        } else if let Some(value) = token_text.strip_prefix("--format=") {
            format = match value {
                "human" => OutputFormat::Human,
                "json" => OutputFormat::Json,
                _ => {
                    return Err(ProjectParseError::InvalidValue {
                        option: "--format",
                        value: value.to_owned(),
                    });
                }
            };
        } else if token_text.starts_with('-') {
            return Err(ProjectParseError::UnknownOption(token_text.into_owned()));
        } else if directory.is_some() {
            return Err(ProjectParseError::DuplicateDirectory);
        } else {
            directory = Some(PathBuf::from(token));
        }
        index += 1;
    }
    Ok(ParseOutcome::Parsed(CheckOptions {
        directory: directory.unwrap_or_else(|| PathBuf::from(".")),
        format,
    }))
}

pub fn parse_build_args<T: AsRef<std::ffi::OsStr>>(
    args: &[T],
) -> Result<ParseOutcome<BuildOptions>, ProjectParseError> {
    let mut directory = None;
    let mut profile = String::from("debug");
    let mut index = 0;
    while index < args.len() {
        let token = args[index].as_ref();
        if token == std::ffi::OsStr::new("--help") || token == std::ffi::OsStr::new("-h") {
            if index + 1 != args.len() {
                return Err(ProjectParseError::UnexpectedAfterHelp);
            }
            return Ok(ParseOutcome::Help);
        }
        let token_text = token.to_string_lossy();
        if token == std::ffi::OsStr::new("--profile") {
            index += 1;
            let value = args
                .get(index)
                .ok_or(ProjectParseError::MissingValue("--profile"))
                .and_then(|value| option_value("--profile", value))?;
            profile = parse_profile(value)?;
        } else if let Some(value) = token_text.strip_prefix("--profile=") {
            profile = parse_profile(value.to_owned())?;
        } else if token_text.starts_with('-') {
            return Err(ProjectParseError::UnknownOption(token_text.into_owned()));
        } else if directory.is_some() {
            return Err(ProjectParseError::DuplicateDirectory);
        } else {
            directory = Some(PathBuf::from(token));
        }
        index += 1;
    }
    Ok(ParseOutcome::Parsed(BuildOptions {
        directory: directory.unwrap_or_else(|| PathBuf::from(".")),
        profile,
    }))
}

fn parse_buildish_args<T: AsRef<std::ffi::OsStr>>(
    args: &[T],
) -> Result<ParseOutcome<(PathBuf, bool, String)>, ProjectParseError> {
    let mut directory = None;
    let mut build = false;
    let mut profile = String::from("debug");
    let mut index = 0;
    while index < args.len() {
        let token = args[index].as_ref();
        if token == std::ffi::OsStr::new("--help") || token == std::ffi::OsStr::new("-h") {
            if index + 1 != args.len() {
                return Err(ProjectParseError::UnexpectedAfterHelp);
            }
            return Ok(ParseOutcome::Help);
        }
        let token_text = token.to_string_lossy();
        if token == std::ffi::OsStr::new("--build") {
            build = true;
        } else if token == std::ffi::OsStr::new("--profile") {
            index += 1;
            let value = args
                .get(index)
                .ok_or(ProjectParseError::MissingValue("--profile"))
                .and_then(|value| option_value("--profile", value))?;
            profile = parse_profile(value)?;
        } else if let Some(value) = token_text.strip_prefix("--profile=") {
            profile = parse_profile(value.to_owned())?;
        } else if token_text.starts_with('-') {
            return Err(ProjectParseError::UnknownOption(token_text.into_owned()));
        } else if directory.is_some() {
            return Err(ProjectParseError::DuplicateDirectory);
        } else {
            directory = Some(PathBuf::from(token));
        }
        index += 1;
    }
    Ok(ParseOutcome::Parsed((
        directory.unwrap_or_else(|| PathBuf::from(".")),
        build,
        profile,
    )))
}

pub fn parse_install_args<T: AsRef<std::ffi::OsStr>>(
    args: &[T],
) -> Result<ParseOutcome<InstallOptions>, ProjectParseError> {
    Ok(match parse_buildish_args(args)? {
        ParseOutcome::Help => ParseOutcome::Help,
        ParseOutcome::Parsed((directory, build, profile)) => ParseOutcome::Parsed(InstallOptions {
            directory,
            build,
            profile,
        }),
    })
}

pub fn parse_test_args<T: AsRef<std::ffi::OsStr>>(
    args: &[T],
) -> Result<ParseOutcome<TestOptions>, ProjectParseError> {
    Ok(match parse_buildish_args(args)? {
        ParseOutcome::Help => ParseOutcome::Help,
        ParseOutcome::Parsed((directory, build, profile)) => ParseOutcome::Parsed(TestOptions {
            directory,
            build,
            profile,
        }),
    })
}

fn run_value<T: AsRef<std::ffi::OsStr>>(
    option: &'static str,
    value: &T,
) -> Result<String, RunParseError> {
    value
        .as_ref()
        .to_str()
        .map(str::to_owned)
        .ok_or(RunParseError::NonUtf8Value(option))
}

fn run_profile(value: String) -> Result<String, RunParseError> {
    if value == "debug" || value == "release" {
        Ok(value)
    } else {
        Err(RunParseError::InvalidValue {
            option: "--profile",
            value,
        })
    }
}

fn run_repl(value: String) -> Result<ReplMode, RunParseError> {
    match value.as_str() {
        "none" => Ok(ReplMode::None),
        "tty" => Ok(ReplMode::Tty),
        "socket" => Ok(ReplMode::Socket),
        _ => Err(RunParseError::InvalidValue {
            option: "--repl",
            value,
        }),
    }
}

pub fn parse_run_args<T: AsRef<std::ffi::OsStr>>(
    args: &[T],
) -> Result<ParseOutcome<RunOptions>, RunParseError> {
    let mut directory = None;
    let mut build = false;
    let mut profile = String::from("debug");
    let mut repl = None;
    let mut socket_bind = None;
    let mut socket_port = None;
    let mut arguments = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let token = args[index].as_ref();
        if token == std::ffi::OsStr::new("--") {
            arguments.extend(args[index + 1..].iter().map(|arg| arg.as_ref().to_owned()));
            break;
        }
        if token == std::ffi::OsStr::new("--help") || token == std::ffi::OsStr::new("-h") {
            if index + 1 != args.len() {
                return Err(RunParseError::UnexpectedAfterHelp);
            }
            return Ok(ParseOutcome::Help);
        }
        let token_text = token.to_string_lossy();
        if token == std::ffi::OsStr::new("--build") {
            build = true;
        } else if token == std::ffi::OsStr::new("--profile") {
            index += 1;
            let value = args
                .get(index)
                .ok_or(RunParseError::MissingValue("--profile"))
                .and_then(|value| run_value("--profile", value))?;
            profile = run_profile(value)?;
        } else if let Some(value) = token_text.strip_prefix("--profile=") {
            profile = run_profile(value.to_owned())?;
        } else if token == std::ffi::OsStr::new("--no-repl") {
            repl = Some(ReplMode::None);
        } else if token == std::ffi::OsStr::new("--repl") {
            repl = Some(ReplMode::Tty);
        } else if let Some(value) = token_text.strip_prefix("--repl=") {
            repl = Some(run_repl(value.to_owned())?);
        } else if token == std::ffi::OsStr::new("--repl-bind") {
            index += 1;
            let value = args
                .get(index)
                .ok_or(RunParseError::MissingValue("--repl-bind"))
                .and_then(|value| run_value("--repl-bind", value))?;
            if value.is_empty() {
                return Err(RunParseError::MissingValue("--repl-bind"));
            }
            socket_bind = Some(value);
        } else if let Some(value) = token_text.strip_prefix("--repl-bind=") {
            if value.is_empty() {
                return Err(RunParseError::MissingValue("--repl-bind"));
            }
            socket_bind = Some(value.to_owned());
        } else if token == std::ffi::OsStr::new("--repl-port") {
            index += 1;
            let value = args
                .get(index)
                .ok_or(RunParseError::MissingValue("--repl-port"))
                .and_then(|value| run_value("--repl-port", value))?;
            socket_port = Some(value.parse().map_err(|_| RunParseError::InvalidValue {
                option: "--repl-port",
                value,
            })?);
        } else if let Some(value) = token_text.strip_prefix("--repl-port=") {
            socket_port = Some(value.parse().map_err(|_| RunParseError::InvalidValue {
                option: "--repl-port",
                value: value.to_owned(),
            })?);
        } else if token_text.starts_with('-') {
            return Err(RunParseError::UnknownOption(token_text.into_owned()));
        } else if directory.is_some() {
            return Err(RunParseError::DuplicateDirectory);
        } else {
            directory = Some(PathBuf::from(token));
        }
        index += 1;
    }
    Ok(ParseOutcome::Parsed(RunOptions {
        directory: directory.unwrap_or_else(|| PathBuf::from(".")),
        build,
        profile,
        repl,
        socket_bind,
        socket_port,
        arguments,
    }))
}

pub fn parse_bake_args<T: AsRef<std::ffi::OsStr>>(
    args: &[T],
) -> Result<BakeOptions, ToolingParseError> {
    if args.len() != 2 {
        return Err(ToolingParseError::Arity {
            command: "--bake",
            expected: 2,
            actual: args.len(),
        });
    }
    Ok(BakeOptions {
        image: PathBuf::from(args[0].as_ref()),
        executable: PathBuf::from(args[1].as_ref()),
    })
}

pub fn parse_rebuild_args<T: AsRef<std::ffi::OsStr>>(
    args: &[T],
) -> Result<RebuildOptions, ToolingParseError> {
    if args.len() != 1 {
        return Err(ToolingParseError::Arity {
            command: "--rebuild",
            expected: 1,
            actual: args.len(),
        });
    }
    Ok(RebuildOptions {
        image: PathBuf::from(args[0].as_ref()),
    })
}

pub fn parse_compile_module_args<T: AsRef<std::ffi::OsStr>>(
    args: &[T],
) -> Result<CompileModuleOptions, ToolingParseError> {
    if args.len() != 2 {
        return Err(ToolingParseError::Arity {
            command: "--compile-module",
            expected: 2,
            actual: args.len(),
        });
    }
    Ok(CompileModuleOptions {
        source: PathBuf::from(args[0].as_ref()),
        output: PathBuf::from(args[1].as_ref()),
    })
}

/// Classify only the top-level command family. Subcommand option validation
/// remains a later parser seam; this function therefore cannot consume a
/// project directory or reinterpret `--` arguments as host flags.
pub fn classify_command<T: AsRef<std::ffi::OsStr>>(
    args: &[T],
) -> Result<CommandKind, CommandError> {
    let Some(first) = args.first().map(AsRef::as_ref) else {
        return Ok(CommandKind::Repl);
    };
    let known = [
        ("--help", CommandKind::Help),
        ("-h", CommandKind::Help),
        ("--version", CommandKind::Version),
        ("-V", CommandKind::Version),
        ("--repl", CommandKind::Repl),
        ("-i", CommandKind::Repl),
        ("repl", CommandKind::Repl),
        ("--listen", CommandKind::Listen),
        ("--serve", CommandKind::Listen),
        ("--lsp", CommandKind::Lsp),
        ("doc", CommandKind::Doc),
        ("docgen", CommandKind::Docgen),
        ("init", CommandKind::Init),
        ("check", CommandKind::Check),
        ("build", CommandKind::Build),
        ("install", CommandKind::Install),
        ("run", CommandKind::Run),
        ("test", CommandKind::Test),
        ("wrap", CommandKind::Wrap),
        ("--bake", CommandKind::Bake),
        ("--rebuild", CommandKind::Rebuild),
        ("--compile-module", CommandKind::CompileModule),
        ("--worker", CommandKind::Worker),
        ("--project-test-worker", CommandKind::ProjectTestWorker),
    ];
    for (name, command) in known {
        if first == std::ffi::OsStr::new(name) {
            return Ok(command);
        }
    }
    if first.to_string_lossy().starts_with('-') {
        return Err(CommandError::UnknownOption(
            first.to_string_lossy().into_owned(),
        ));
    }
    let path = PathBuf::from(first);
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("image") => Ok(CommandKind::Image(path)),
        Some("dylib") => Ok(CommandKind::Artifact(path)),
        _ => Ok(CommandKind::Script(path)),
    }
}

/// Classify the pure CLI exits that must run before any runtime-resource or
/// floor access. Remaining arguments stay opaque until the host dispatch slice
/// ports the project/REPL command grammar.
pub fn classify_early_args<T: AsRef<std::ffi::OsStr>>(args: &[T]) -> EarlyAction {
    match args.first().map(AsRef::as_ref).and_then(|arg| arg.to_str()) {
        Some("--help" | "-h") => EarlyAction::Help,
        Some("--version" | "-V") => EarlyAction::Version,
        _ => EarlyAction::Continue,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FloorError {
    Read {
        path: PathBuf,
        source: io::ErrorKind,
    },
    DigestMismatch {
        expected: String,
        actual: String,
    },
    EmbeddedNul,
    OpenFailed,
    MissingIdInit,
    NullApi,
    MissingIdentity,
    IdentityMismatch {
        expected: String,
        actual: String,
    },
}

/// A successfully initialized floor. The dynamic-library handle is retained
/// for the lifetime of this value and deliberately never closed: the floor
/// owns process-wide heap/GC state and may retain function pointers in modules.
pub struct LoadedFloor {
    handle: NonNull<std::ffi::c_void>,
    api: NonNull<IdApi>,
    // Keep this type !Send/!Sync until host threading and the floor owner-thread
    // protocol are explicitly ported. The marker has no runtime state.
    _owner_thread: PhantomData<std::rc::Rc<()>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleError {
    EmbeddedNul,
    OpenFailed { path: PathBuf },
    MissingModuleInit { path: PathBuf },
}

/// The native module order is part of the host contract: Store precedes
/// Codegen, and every later rung sees the same canonical floor/API table.
pub const SCRIPT_MODULES: &[&str] = &[
    "libbirnam_id_artifact",
    "libbirnam_store",
    "libbirnam_id_peg",
    "libbirnam_id_lang",
    "libbirnam_id_tcc",
    "libbirnam_id_codegen",
    "libbirnam_id_prelude",
    "libbirnam_id_collection",
    "libbirnam_id_error",
    "libbirnam_id_isolate",
    "libbirnam_id_relational",
    "libbirnam_id_alien",
    "libbirnam_id_image",
];

/// Source-backed self-hosted modules. `modules` is intentionally separate:
/// the manager must be installed first because it implements the dependency
/// registration and ordering protocol used by the remaining modules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelfHostedModule {
    pub name: &'static str,
    pub source: &'static [u8],
}

pub const SELF_HOSTED_MODULES: &[SelfHostedModule] = &[
    SelfHostedModule {
        name: "stdlib",
        source: include_bytes!("../../../src/id/stdlib.birnam"),
    },
    SelfHostedModule {
        name: "compiledUnit",
        source: include_bytes!("../../../src/id/compiled-unit.birnam"),
    },
    SelfHostedModule {
        name: "toml",
        source: include_bytes!("../../../src/id/toml.birnam"),
    },
    SelfHostedModule {
        name: "config",
        source: include_bytes!("../../../src/id/config.birnam"),
    },
    SelfHostedModule {
        name: "appConfig",
        source: include_bytes!("../../../src/id/app-config.birnam"),
    },
    SelfHostedModule {
        name: "isolate",
        source: include_bytes!("../../../src/id/isolate.birnam"),
    },
];

pub const MODULE_MANAGER: SelfHostedModule = SelfHostedModule {
    name: "modules",
    source: include_bytes!("../../../src/id/modules.birnam"),
};

const KERNEL_MODULES: &[&str] = &[
    "artifact",
    "store",
    "peg",
    "lang",
    "tcc",
    "codegen",
    "prelude",
    "collection",
    "error",
    "relational",
    "alien",
    "image",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootstrapOrderError {
    UnknownSelfHostedDependency { module: String, dependency: String },
    Cycle { module: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapSourceError {
    RecordRejected,
    EvaluationRejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfHostedLoadOutcome {
    Precompiled,
    Source,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootstrapRuntimeError {
    Source(BootstrapSourceError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadedSelfHostedModule {
    pub name: &'static str,
    pub outcome: SelfHostedLoadOutcome,
}

#[derive(Debug)]
pub enum RuntimeStartError {
    Resources(io::ErrorKind),
    Floor(FloorError),
    Module(ModuleError),
    IsolateHooks(IsolateHookError),
    MissingCodegen,
    Bootstrap(BootstrapRuntimeError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IsolateHookError {
    EmbeddedNul,
    OpenFailed,
    MissingSymbol(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerError {
    Failed(c_int),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeModuleError {
    EmbeddedNul,
    MissingModuleManager,
    LoadRejected,
}

type IsolateDriverFn =
    unsafe extern "C" fn(*mut IdApi, c_int, birnam_abi::Oop, birnam_abi::Oop) -> c_int;
type IsolateBootErrorFn = unsafe extern "C" fn(*mut IdApi, c_int, *const c_char, *const u8, usize);
type IsolateDrainFn = unsafe extern "C" fn();
type IsolateShutdownFn = unsafe extern "C" fn();

pub struct IsolateHooks {
    handle: NonNull<c_void>,
    driver: IsolateDriverFn,
    boot_error: IsolateBootErrorFn,
    drain: IsolateDrainFn,
    shutdown: IsolateShutdownFn,
}

impl IsolateHooks {
    pub fn handle(&self) -> NonNull<c_void> {
        self.handle
    }

    /// # Safety
    ///
    /// The host must remain on the selected floor's owner thread.
    pub unsafe fn driver(
        &self,
        api: *mut IdApi,
        fd: c_int,
        receiver: birnam_abi::Oop,
        selector: birnam_abi::Oop,
    ) -> c_int {
        unsafe { (self.driver)(api, fd, receiver, selector) }
    }

    /// # Safety
    ///
    /// `kind` must be NUL-terminated and `message` must be readable for its
    /// supplied length; the hook borrows them only for the synchronous send.
    pub unsafe fn boot_error(
        &self,
        api: *mut IdApi,
        fd: c_int,
        kind: *const c_char,
        message: *const u8,
        length: usize,
    ) {
        unsafe { (self.boot_error)(api, fd, kind, message, length) }
    }

    /// # Safety
    ///
    /// The host must remain on the selected floor's owner thread.
    pub unsafe fn drain(&self) {
        unsafe { (self.drain)() };
    }

    /// # Safety
    ///
    /// The host must remain on the selected floor's owner thread and no later
    /// isolate operation may use the hooks after shutdown.
    pub unsafe fn shutdown(&self) {
        unsafe { (self.shutdown)() };
    }
}

pub struct HostRuntime {
    pub resources: Resources,
    pub stack: LoadedStack,
    pub isolate: IsolateHooks,
    pub self_hosted: Vec<LoadedSelfHostedModule>,
}

impl HostRuntime {
    /// Load one compiled host/runtime module through the Birnam Module
    /// manager. The manager owns its executable-relative path and dependency
    /// closure; Rust supplies only the requested identity.
    ///
    /// # Safety
    ///
    /// The runtime must remain on the selected floor's owner thread for the
    /// synchronous manager send.
    pub unsafe fn load_runtime_module(&self, name: &str) -> Result<(), RuntimeModuleError> {
        let Some(module_manager) = (unsafe { global_named(self.stack.api(), "Module") }) else {
            return Err(RuntimeModuleError::MissingModuleManager);
        };
        let name = CString::new(name).map_err(|_| RuntimeModuleError::EmbeddedNul)?;
        let api = self.stack.api();
        let selector = unsafe { (api.intern)(c"load:".as_ptr()) };
        let name = unsafe { (api.intern)(name.as_ptr()) };
        let result = unsafe { (api.send)(module_manager, selector, [name].as_ptr(), 1) };
        if result == api.nilObj || send_is_error(api, result) {
            return Err(RuntimeModuleError::LoadRejected);
        }
        Ok(())
    }

    /// Run one isolate worker over an inherited channel descriptor. The driver
    /// borrows the receiver/selector and never takes ownership of the fd.
    ///
    /// # Safety
    ///
    /// `fd` must be a live inherited channel owned by the caller; `receiver` and
    /// `selector` must be live Oops from this runtime; the caller must remain on
    /// the floor owner thread for the complete worker loop.
    pub unsafe fn run_worker(
        &self,
        fd: c_int,
        receiver: birnam_abi::Oop,
        selector: birnam_abi::Oop,
    ) -> Result<(), WorkerError> {
        let api = self.stack.api() as *const IdApi as *mut IdApi;
        let status = unsafe { self.isolate.driver(api, fd, receiver, selector) };
        if status == 0 {
            Ok(())
        } else {
            Err(WorkerError::Failed(status))
        }
    }

    /// # Safety
    ///
    /// The host must remain on the floor owner thread.
    pub unsafe fn drain_isolates(&self) {
        unsafe { self.isolate.drain() };
    }

    /// # Safety
    ///
    /// The host must remain on the floor owner thread and must not use this
    /// runtime's isolate hooks after shutdown.
    pub unsafe fn shutdown_isolates(&self) {
        unsafe { self.isolate.shutdown() };
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartupPolicy {
    pub force_source: bool,
    pub skip_stdlib: bool,
}

impl StartupPolicy {
    pub const fn from_flags(force_source: bool, skip_stdlib: bool) -> Self {
        Self {
            force_source,
            skip_stdlib,
        }
    }

    pub fn from_environment() -> Self {
        Self::from_flags(
            env::var_os("BIRNAM_FORCE_SOURCE").is_some(),
            env::var_os("BIRNAM_NO_STDLIB").is_some(),
        )
    }
}

/// Scan the first `requires: #[...]` declaration using the same deliberately
/// small source convention as the Zig host. Dependencies outside the
/// self-hosted inventory are native/kernel modules and are already loaded.
pub fn self_hosted_requires(source: &[u8]) -> Vec<String> {
    let Some(requires_at) = source
        .windows(b"requires:".len())
        .position(|window| window == b"requires:")
    else {
        return Vec::new();
    };
    let rest = &source[requires_at + b"requires:".len()..];
    let Some(vector_at) = rest.windows(2).position(|window| window == b"#[") else {
        return Vec::new();
    };
    let mut result = Vec::new();
    let mut index = vector_at + 2;
    while index < rest.len() && rest[index] != b']' {
        if rest[index] == b'#' {
            let start = index + 1;
            let mut end = start;
            while end < rest.len() && (rest[end].is_ascii_alphanumeric() || rest[end] == b'_') {
                end += 1;
            }
            if end > start {
                result.push(String::from_utf8_lossy(&rest[start..end]).into_owned());
            }
            index = end;
        } else {
            index += 1;
        }
    }
    result
}

/// Return the stable dependency order used by self-hosted boot. Ties retain
/// the declared inventory order, while native/kernel dependencies are treated
/// as already satisfied by `LoadedStack`.
pub fn self_hosted_boot_order() -> Result<Vec<&'static str>, BootstrapOrderError> {
    let mut dependencies: Vec<Vec<String>> = SELF_HOSTED_MODULES
        .iter()
        .map(|module| self_hosted_requires(module.source))
        .collect();
    for (index, module) in SELF_HOSTED_MODULES.iter().enumerate() {
        dependencies[index].retain(|dependency| {
            SELF_HOSTED_MODULES
                .iter()
                .any(|candidate| candidate.name == dependency)
        });
        for dependency in self_hosted_requires(module.source) {
            if !SELF_HOSTED_MODULES
                .iter()
                .any(|candidate| candidate.name == dependency)
                && dependency != "modules"
            {
                // The source declaration names a non-self-hosted module. Keep
                // the distinction explicit so a typo cannot silently reorder
                // the self-hosted layer.
                let is_kernel = SCRIPT_MODULES.iter().any(|name| {
                    name.strip_prefix("libbirnam_id_")
                        .is_some_and(|kernel| kernel == dependency)
                        || (*name == "libbirnam_store" && dependency == "store")
                });
                if !is_kernel {
                    return Err(BootstrapOrderError::UnknownSelfHostedDependency {
                        module: module.name.to_owned(),
                        dependency,
                    });
                }
            }
        }
    }

    let mut order = Vec::with_capacity(SELF_HOSTED_MODULES.len());
    while order.len() < SELF_HOSTED_MODULES.len() {
        let mut progress = false;
        for (index, module) in SELF_HOSTED_MODULES.iter().enumerate() {
            if order.contains(&module.name)
                || !dependencies[index]
                    .iter()
                    .all(|dependency| order.contains(&dependency.as_str()))
            {
                continue;
            }
            order.push(module.name);
            progress = true;
        }
        if !progress {
            let module = SELF_HOSTED_MODULES
                .iter()
                .find(|module| !order.contains(&module.name))
                .expect("incomplete order must have a remaining module");
            return Err(BootstrapOrderError::Cycle {
                module: module.name.to_owned(),
            });
        }
    }
    Ok(order)
}

fn send_is_error(api: &IdApi, value: birnam_abi::Oop) -> bool {
    if unsafe { (api.isExported)(c"Error".as_ptr()) } == 0 {
        return false;
    }
    let is_error = unsafe { (api.intern)(c"isError".as_ptr()) };
    unsafe { (api.send)(value, is_error, std::ptr::null(), 0) == api.trueObj }
}

/// Execute one embedded self-hosted module through the frozen Codegen
/// protocol. This deliberately exposes only source recovery: selecting a
/// precompiled dylib, checking its load result, and deciding whether to fall
/// back to this helper remain explicit host policy at the next layer.
///
/// # Safety
///
/// `api` must be the live table returned by the selected floor, `codegen` must
/// be a live Codegen object from that table, and both must remain valid for
/// the duration of the synchronous sends.
pub unsafe fn record_and_eval_source(
    api: &IdApi,
    codegen: birnam_abi::Oop,
    module: &SelfHostedModule,
) -> Result<birnam_abi::Oop, BootstrapSourceError> {
    let name = CString::new(module.name).map_err(|_| BootstrapSourceError::RecordRejected)?;
    let source = unsafe { (api.mkString)(module.source.as_ptr(), module.source.len()) };
    let name_symbol = unsafe { (api.intern)(name.as_ptr()) };
    let record_selector = unsafe { (api.intern)(c"recordModuleSource:named:".as_ptr()) };
    let recorded =
        unsafe { (api.send)(codegen, record_selector, [source, name_symbol].as_ptr(), 2) };
    if send_is_error(api, recorded) {
        return Err(BootstrapSourceError::RecordRejected);
    }
    let eval_selector = unsafe { (api.intern)(c"eval:".as_ptr()) };
    let evaluated = unsafe { (api.send)(codegen, eval_selector, [source].as_ptr(), 1) };
    if send_is_error(api, evaluated) {
        return Err(BootstrapSourceError::EvaluationRejected);
    }
    Ok(evaluated)
}

/// Load one self-hosted module using the same optimization policy as the Zig
/// host. The candidate is exactly `<lib_dir>/libbirnam_<name>.dylib`; the host
/// never searches other directories or opens this dylib directly. A missing,
/// rejected, or error-valued Codegen `load:` answer falls back to the embedded
/// source through [`record_and_eval_source`].
///
/// # Safety
///
/// `api` and `codegen` must satisfy the safety contract of
/// [`record_and_eval_source`] and remain live for every synchronous send.
pub unsafe fn load_self_hosted(
    api: &IdApi,
    codegen: birnam_abi::Oop,
    resources: &Resources,
    module: &SelfHostedModule,
    force_source: bool,
) -> Result<SelfHostedLoadOutcome, BootstrapSourceError> {
    if !force_source {
        let candidate = self_hosted_candidate_path(resources, module);
        if std::fs::metadata(&candidate).is_ok() {
            let candidate_bytes = path_bytes(&candidate);
            let candidate_string =
                unsafe { (api.mkString)(candidate_bytes.as_ptr(), candidate_bytes.len()) };
            let load_selector = unsafe { (api.intern)(c"load:".as_ptr()) };
            let loaded =
                unsafe { (api.send)(codegen, load_selector, [candidate_string].as_ptr(), 1) };
            if loaded != api.nilObj && !send_is_error(api, loaded) {
                return Ok(SelfHostedLoadOutcome::Precompiled);
            }
        }
    }
    // SAFETY: forwarded to the documented source-recovery boundary.
    unsafe { record_and_eval_source(api, codegen, module)? };
    Ok(SelfHostedLoadOutcome::Source)
}

fn self_hosted_candidate_path(resources: &Resources, module: &SelfHostedModule) -> PathBuf {
    resources
        .lib_dir
        .join(format!("libbirnam_{}.dylib", module.name))
}

unsafe fn intern_name(api: &IdApi, name: &str) -> Option<birnam_abi::Oop> {
    let name = CString::new(name).ok()?;
    Some(unsafe { (api.intern)(name.as_ptr()) })
}

unsafe fn global_named(api: &IdApi, name: &str) -> Option<birnam_abi::Oop> {
    let globals = unsafe { (api.import)(c"Globals".as_ptr()) };
    if globals == api.nilObj {
        return None;
    }
    let name = unsafe { intern_name(api, name) }?;
    let selector = unsafe { (api.intern)(c"at:".as_ptr()) };
    let value = unsafe { (api.send)(globals, selector, [name].as_ptr(), 1) };
    (value != api.nilObj).then_some(value)
}

unsafe fn mark_module_loaded(api: &IdApi, module_manager: birnam_abi::Oop, name: &str) {
    let Some(name) = (unsafe { intern_name(api, name) }) else {
        return;
    };
    let selector = unsafe { (api.intern)(c"markLoaded:".as_ptr()) };
    unsafe { (api.send)(module_manager, selector, [name].as_ptr(), 1) };
}

unsafe fn refresh_module_manager(
    api: &IdApi,
    module_manager: birnam_abi::Oop,
    resources: &Resources,
) {
    let selector = unsafe { (api.intern)(c"kernel:".as_ptr()) };
    for name in KERNEL_MODULES {
        let Some(name) = (unsafe { intern_name(api, name) }) else {
            continue;
        };
        unsafe { (api.send)(module_manager, selector, [name].as_ptr(), 1) };
    }
    let path = path_bytes(&resources.lib_dir);
    let value = unsafe { (api.mkString)(path.as_ptr(), path.len()) };
    let selector = unsafe { (api.intern)(c"libDir:".as_ptr()) };
    unsafe { (api.send)(module_manager, selector, [value].as_ptr(), 1) };
}

unsafe fn dependency_vector(
    api: &IdApi,
    vector: birnam_abi::Oop,
    dependencies: &[String],
) -> Option<birnam_abi::Oop> {
    let new_selector = unsafe { (api.intern)(c"new".as_ptr()) };
    let add_selector = unsafe { (api.intern)(c"add:".as_ptr()) };
    let mut result = unsafe { (api.send)(vector, new_selector, std::ptr::null(), 0) };
    if result == api.nilObj {
        return None;
    }
    for dependency in dependencies {
        let name = unsafe { intern_name(api, dependency) }?;
        result = unsafe { (api.send)(result, add_selector, [name].as_ptr(), 1) };
        if result == api.nilObj {
            return None;
        }
    }
    Some(result)
}

unsafe fn requested_names(api: &IdApi, vector: birnam_abi::Oop) -> Option<birnam_abi::Oop> {
    let new_selector = unsafe { (api.intern)(c"new".as_ptr()) };
    let add_selector = unsafe { (api.intern)(c"add:".as_ptr()) };
    let mut names = unsafe { (api.send)(vector, new_selector, std::ptr::null(), 0) };
    if names == api.nilObj {
        return None;
    }
    for module in SELF_HOSTED_MODULES {
        let name = unsafe { intern_name(api, module.name) }?;
        names = unsafe { (api.send)(names, add_selector, [name].as_ptr(), 1) };
        if names == api.nilObj {
            return None;
        }
    }
    Some(names)
}

unsafe fn order_from_manager(
    api: &IdApi,
    module_manager: birnam_abi::Oop,
    vector: birnam_abi::Oop,
) -> Option<Vec<&'static SelfHostedModule>> {
    let names = unsafe { requested_names(api, vector) }?;
    let order_selector = unsafe { (api.intern)(c"loadOrder:".as_ptr()) };
    let order = unsafe { (api.send)(module_manager, order_selector, [names].as_ptr(), 1) };
    if order == api.nilObj || send_is_error(api, order) {
        return None;
    }
    let size_selector = unsafe { (api.intern)(c"size".as_ptr()) };
    let at_selector = unsafe { (api.intern)(c"at:".as_ptr()) };
    let size = unsafe { (api.send)(order, size_selector, std::ptr::null(), 0) };
    let size = unsafe { (api.smallIntValue)(size) };
    if size < 0 {
        return None;
    }
    let mut result = Vec::with_capacity(size as usize);
    for index in 0..size as usize {
        let index = unsafe { (api.mkSmallInt)(index as i64) };
        let name = unsafe { (api.send)(order, at_selector, [index].as_ptr(), 1) };
        if name == api.nilObj {
            return None;
        }
        let name_ptr = unsafe { (api.symbolName)(name) };
        if name_ptr.is_null() {
            return None;
        }
        let name = unsafe { CStr::from_ptr(name_ptr) }.to_bytes();
        let module = SELF_HOSTED_MODULES
            .iter()
            .find(|module| module.name.as_bytes() == name)?;
        result.push(module);
    }
    Some(result)
}

/// Bring up the source-backed layer after `LoadedStack` is ready. The module
/// manager is always the first ordinary self-hosted source; it registers the
/// native kernel tier, asks the manager for dependency order, and falls back to
/// the declared order only when the manager/vector protocol is unavailable or
/// returns an Error. Module installation itself still goes through the one
/// Codegen load/source policy above.
///
/// `skip_stdlib` is the explicit `BIRNAM_NO_STDLIB` policy; when set, this
/// function performs no manager or source work. The returned list includes the
/// manager followed by every attempted self-hosted module.
///
/// # Safety
///
/// `api` and `codegen` must be the live selected-floor API and Codegen object,
/// and remain valid for the synchronous sends performed during bootstrap.
pub unsafe fn bootstrap_self_hosted(
    api: &IdApi,
    codegen: birnam_abi::Oop,
    resources: &Resources,
    force_source: bool,
    skip_stdlib: bool,
) -> Result<Vec<LoadedSelfHostedModule>, BootstrapRuntimeError> {
    if skip_stdlib {
        return Ok(Vec::new());
    }
    let mut loaded = Vec::with_capacity(SELF_HOSTED_MODULES.len() + 1);
    let manager_outcome =
        unsafe { load_self_hosted(api, codegen, resources, &MODULE_MANAGER, force_source) }
            .map_err(BootstrapRuntimeError::Source)?;
    loaded.push(LoadedSelfHostedModule {
        name: MODULE_MANAGER.name,
        outcome: manager_outcome,
    });

    let module_manager = unsafe { global_named(api, "Module") };
    let vector = unsafe { (api.import)(c"Vector".as_ptr()) };
    if let (Some(module_manager), true) = (module_manager, vector != api.nilObj) {
        unsafe { refresh_module_manager(api, module_manager, resources) };
        let named_selector = unsafe { (api.intern)(c"named:requires:".as_ptr()) };
        for module in SELF_HOSTED_MODULES {
            let Some(name) = (unsafe { intern_name(api, module.name) }) else {
                continue;
            };
            let Some(requirements) =
                (unsafe { dependency_vector(api, vector, &self_hosted_requires(module.source)) })
            else {
                continue;
            };
            unsafe {
                (api.send)(
                    module_manager,
                    named_selector,
                    [name, requirements].as_ptr(),
                    2,
                )
            };
        }
    }
    let order = module_manager
        .zip((vector != api.nilObj).then_some(vector))
        .and_then(|(module_manager, vector)| unsafe {
            order_from_manager(api, module_manager, vector)
        })
        .unwrap_or_else(|| SELF_HOSTED_MODULES.iter().collect());

    for module in order {
        let outcome = unsafe { load_self_hosted(api, codegen, resources, module, force_source) }
            .map_err(BootstrapRuntimeError::Source)?;
        if let Some(module_manager) = module_manager {
            unsafe { mark_module_loaded(api, module_manager, module.name) };
        }
        loaded.push(LoadedSelfHostedModule {
            name: module.name,
            outcome,
        });
    }
    Ok(loaded)
}

/// Process-lifetime native stack owned by the host. Higher rungs are opened
/// through `IdApi`; successful handles are retained because methods/finalizers
/// may retain code pointers after `module_init` returns.
pub struct LoadedStack {
    pub floor: LoadedFloor,
    modules: Vec<NonNull<std::ffi::c_void>>,
}

impl LoadedStack {
    pub fn new(floor: LoadedFloor) -> Self {
        Self {
            floor,
            modules: Vec::new(),
        }
    }

    pub fn module_count(&self) -> usize {
        self.modules.len()
    }

    pub fn api(&self) -> &IdApi {
        self.floor.api()
    }

    /// Load one higher rung through the selected floor's dynamic-loader API.
    /// No host-side `dlopen` is used after the floor has initialized.
    pub fn load_module(&mut self, path: &Path) -> Result<(), ModuleError> {
        let path_bytes = CString::new(path_bytes(path)).map_err(|_| ModuleError::EmbeddedNul)?;
        // SAFETY: the floor API is process-owned and the C string lives across
        // this synchronous call.
        let handle = unsafe { (self.floor.api().dlopen)(path_bytes.as_ptr(), RTLD_LAZY) };
        let Some(handle) = NonNull::new(handle) else {
            return Err(ModuleError::OpenFailed {
                path: path.to_path_buf(),
            });
        };
        let init_name = c"module_init";
        // SAFETY: the handle came from the selected floor's successful dlopen.
        let init = unsafe { (self.floor.api().dlsym)(handle.as_ptr(), init_name.as_ptr().cast()) };
        let Some(init) = NonNull::new(init) else {
            return Err(ModuleError::MissingModuleInit {
                path: path.to_path_buf(),
            });
        };
        // SAFETY: module_init is the frozen higher-rung ABI and the floor table
        // remains live for the module's initialization call.
        let init: birnam_abi::ModuleInitFn = unsafe { std::mem::transmute(init.as_ptr()) };
        unsafe {
            init(
                self.floor.api() as *const IdApi as *mut IdApi,
                std::ptr::null_mut(),
            )
        };
        self.modules.push(handle);
        Ok(())
    }

    /// Resolve and load the canonical native stack in the same order as the
    /// Zig host. Store/TCC retain their exact override paths; every other rung
    /// comes from the one executable-relative runtime library directory.
    pub fn load_script_modules(&mut self, resources: &Resources) -> Result<(), ModuleError> {
        let store_override = env::var_os("BIRNAM_STORE_DYLIB").map(PathBuf::from);
        let tcc_override = env::var_os("BIRNAM_TCC_DYLIB").map(PathBuf::from);
        for name in SCRIPT_MODULES {
            let path = if *name == "libbirnam_store" {
                store_override
                    .as_deref()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| resources.lib_dir.join("libbirnam_store.dylib"))
            } else if *name == "libbirnam_id_tcc" {
                tcc_override
                    .as_deref()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| resources.lib_dir.join("libbirnam_id_tcc.dylib"))
            } else {
                resources.lib_dir.join(format!("{name}.dylib"))
            };
            self.load_module(&path)?;
        }
        Ok(())
    }
}

fn resolve_isolate_hooks(
    stack: &LoadedStack,
    resources: &Resources,
) -> Result<IsolateHooks, IsolateHookError> {
    let path = resources.lib_dir.join("libbirnam_id_isolate.dylib");
    let path = CString::new(path_bytes(&path)).map_err(|_| IsolateHookError::EmbeddedNul)?;
    let api = stack.api();
    let handle = unsafe { (api.dlopen)(path.as_ptr(), RTLD_LAZY) };
    let Some(handle) = NonNull::new(handle) else {
        return Err(IsolateHookError::OpenFailed);
    };
    let lookup = |name: &'static std::ffi::CStr| {
        // SAFETY: the handle came from the selected floor's IdApi loader and
        // the static symbol name remains live for this synchronous lookup.
        unsafe { (api.dlsym)(handle.as_ptr(), name.as_ptr().cast()) }
    };
    let Some(driver) = NonNull::new(lookup(c"birnam_isolate_worker_driver")) else {
        return Err(IsolateHookError::MissingSymbol(
            "birnam_isolate_worker_driver",
        ));
    };
    let Some(boot_error) = NonNull::new(lookup(c"birnam_isolate_worker_boot_error")) else {
        return Err(IsolateHookError::MissingSymbol(
            "birnam_isolate_worker_boot_error",
        ));
    };
    let Some(drain) = NonNull::new(lookup(c"birnam_isolate_drain")) else {
        return Err(IsolateHookError::MissingSymbol("birnam_isolate_drain"));
    };
    let Some(shutdown) = NonNull::new(lookup(c"birnam_isolate_shutdown")) else {
        return Err(IsolateHookError::MissingSymbol("birnam_isolate_shutdown"));
    };
    Ok(IsolateHooks {
        handle,
        // SAFETY: each symbol is the frozen isolate hook ABI exported by the
        // selected isolate module.
        driver: unsafe { std::mem::transmute::<*mut c_void, IsolateDriverFn>(driver.as_ptr()) },
        boot_error: unsafe {
            std::mem::transmute::<*mut c_void, IsolateBootErrorFn>(boot_error.as_ptr())
        },
        drain: unsafe { std::mem::transmute::<*mut c_void, IsolateDrainFn>(drain.as_ptr()) },
        shutdown: unsafe {
            std::mem::transmute::<*mut c_void, IsolateShutdownFn>(shutdown.as_ptr())
        },
    })
}

/// Start the opt-in Rust host through the same floor → native stack → MOP
/// mirror → self-hosted sequence as the Zig host. Resource resolution is kept
/// outside this function so callers can inspect or override it before any
/// dylib is opened.
///
/// # Safety
///
/// The selected floor and all module initializers must satisfy the frozen ABI;
/// this function invokes their callbacks and synchronous Birnam sends. The
/// returned runtime must stay on the floor's owner thread until that protocol
/// is explicitly ported.
pub unsafe fn start(
    resources: Resources,
    policy: StartupPolicy,
) -> Result<HostRuntime, RuntimeStartError> {
    let floor = load_selected_floor(&resources.id_dylib).map_err(RuntimeStartError::Floor)?;
    let mut stack = LoadedStack::new(floor);
    stack
        .load_script_modules(&resources)
        .map_err(RuntimeStartError::Module)?;
    let isolate =
        resolve_isolate_hooks(&stack, &resources).map_err(RuntimeStartError::IsolateHooks)?;
    let api = stack.api();
    let store = unsafe { (api.import)(c"Store".as_ptr()) };
    if store != api.nilObj {
        let selector = unsafe { (api.intern)(c"backfillMirror".as_ptr()) };
        unsafe { (api.send)(store, selector, std::ptr::null(), 0) };
    }
    let codegen = unsafe { (api.import)(c"Codegen".as_ptr()) };
    if codegen == api.nilObj {
        return Err(RuntimeStartError::MissingCodegen);
    }
    let self_hosted = unsafe {
        bootstrap_self_hosted(
            api,
            codegen,
            &resources,
            policy.force_source,
            policy.skip_stdlib,
        )
    }
    .map_err(RuntimeStartError::Bootstrap)?;
    Ok(HostRuntime {
        resources,
        stack,
        isolate,
        self_hosted,
    })
}

/// Resolve production resources and start the opt-in Rust host using the
/// environment contract already shared with the Zig executable.
///
/// # Safety
///
/// Same owner-thread and frozen-ABI requirements as [`start`].
pub unsafe fn start_from_environment() -> Result<HostRuntime, RuntimeStartError> {
    let resources = resolve().map_err(|error| RuntimeStartError::Resources(error.kind()))?;
    // SAFETY: forwarded to the documented startup boundary.
    unsafe { start(resources, StartupPolicy::from_environment()) }
}

const RTLD_LAZY: i32 = 1;

impl LoadedFloor {
    pub fn api(&self) -> &IdApi {
        // SAFETY: `id_init` returned a process-owned, non-null table and the
        // successful loader retains its dylib handle for this value's lifetime.
        unsafe { self.api.as_ref() }
    }

    pub fn handle(&self) -> NonNull<std::ffi::c_void> {
        self.handle
    }
}

/// Load and initialize one selected floor after optional digest/identity
/// checks. This is the only raw loader seam in the Rust host; higher rungs are
/// loaded through the returned IdApi in later R7 slices.
pub fn load_floor(
    path: &Path,
    expected_identity: Option<&str>,
    expected_digest: Option<&str>,
) -> Result<LoadedFloor, FloorError> {
    let bytes = std::fs::read(path).map_err(|error| FloorError::Read {
        path: path.to_path_buf(),
        source: error.kind(),
    })?;
    if let Some(expected) = expected_digest {
        let actual = String::from_utf8_lossy(&sha256_hex(&bytes)).into_owned();
        if actual != expected {
            return Err(FloorError::DigestMismatch {
                expected: expected.to_owned(),
                actual,
            });
        }
    }
    let path = CString::new(path_bytes(path)).map_err(|_| FloorError::EmbeddedNul)?;
    // SAFETY: the path is NUL-terminated and remains live for the call.
    let handle = unsafe { system_dlopen(path.as_ptr(), RTLD_NOW | RTLD_LOCAL) };
    let Some(handle) = NonNull::new(handle) else {
        return Err(FloorError::OpenFailed);
    };
    let init_name = c"id_init";
    // SAFETY: `handle` came from a successful `dlopen`; the symbol name is a
    // static NUL-terminated string.
    let init = unsafe { system_dlsym(handle.as_ptr(), init_name.as_ptr().cast()) };
    let Some(init) = NonNull::new(init) else {
        unsafe { system_dlclose(handle.as_ptr()) };
        return Err(FloorError::MissingIdInit);
    };
    if let Some(expected) = expected_identity {
        let identity_name = c"birnam_id_floor_identity";
        // SAFETY: the handle is live and the static symbol name is valid.
        let identity_symbol =
            unsafe { system_dlsym(handle.as_ptr(), identity_name.as_ptr().cast()) };
        let Some(identity_symbol) = NonNull::new(identity_symbol) else {
            unsafe { system_dlclose(handle.as_ptr()) };
            return Err(FloorError::MissingIdentity);
        };
        // SAFETY: the floor export is the frozen `fn() -> *const char` identity.
        let identity_fn: unsafe extern "C" fn() -> *const std::ffi::c_char =
            unsafe { std::mem::transmute(identity_symbol.as_ptr()) };
        let actual = unsafe { CStr::from_ptr(identity_fn()) }
            .to_string_lossy()
            .into_owned();
        if actual != expected {
            unsafe { system_dlclose(handle.as_ptr()) };
            return Err(FloorError::IdentityMismatch {
                expected: expected.to_owned(),
                actual,
            });
        }
    }
    // SAFETY: `init` is the frozen `IdInitFn` export and the dylib remains live.
    let init: IdInitFn = unsafe { std::mem::transmute(init.as_ptr()) };
    let api = NonNull::new(unsafe { init() }).ok_or_else(|| {
        unsafe { system_dlclose(handle.as_ptr()) };
        FloorError::NullApi
    })?;
    Ok(LoadedFloor {
        handle,
        api,
        _owner_thread: PhantomData,
    })
}

/// Load the selected floor using the same optional environment checks as the
/// existing Zig host. Missing variables preserve the historical no-check mode.
pub fn load_selected_floor(path: &Path) -> Result<LoadedFloor, FloorError> {
    let identity = env::var("BIRNAM_ID_FLOOR_IDENTITY").ok();
    let digest = env::var("BIRNAM_ID_FLOOR_SHA256").ok();
    load_floor(path, identity.as_deref(), digest.as_deref())
}

const RTLD_NOW: i32 = 2;
const RTLD_LOCAL: i32 = 4;

unsafe extern "C" {
    #[link_name = "dlopen"]
    fn system_dlopen(path: *const std::ffi::c_char, mode: i32) -> *mut std::ffi::c_void;
    #[link_name = "dlsym"]
    fn system_dlsym(
        handle: *mut std::ffi::c_void,
        name: *const std::ffi::c_char,
    ) -> *mut std::ffi::c_void;
    #[link_name = "dlclose"]
    fn system_dlclose(handle: *mut std::ffi::c_void) -> i32;
}

fn path_bytes(path: &Path) -> Vec<u8> {
    #[cfg(unix)]
    {
        path.as_os_str().as_bytes().to_vec()
    }
    #[cfg(not(unix))]
    {
        path.to_string_lossy().as_bytes().to_vec()
    }
}

fn sha256_hex(bytes: &[u8]) -> [u8; 64] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut state = [
        0x6a09e667u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];
    let block_count = (bytes.len() + 9).div_ceil(64);
    let mut padded = Vec::with_capacity(block_count * 64);
    padded.extend_from_slice(bytes);
    padded.push(0x80);
    padded.resize(block_count * 64 - 8, 0);
    padded.extend_from_slice(&((bytes.len() as u64) * 8).to_be_bytes());
    for chunk in padded.chunks_exact(64) {
        let mut schedule = [0u32; 64];
        for (index, word) in schedule[..16].iter_mut().enumerate() {
            *word = u32::from_be_bytes(chunk[index * 4..index * 4 + 4].try_into().unwrap());
        }
        for index in 16..64 {
            let s0 = schedule[index - 15].rotate_right(7)
                ^ schedule[index - 15].rotate_right(18)
                ^ (schedule[index - 15] >> 3);
            let s1 = schedule[index - 2].rotate_right(17)
                ^ schedule[index - 2].rotate_right(19)
                ^ (schedule[index - 2] >> 10);
            schedule[index] = schedule[index - 16]
                .wrapping_add(s0)
                .wrapping_add(schedule[index - 7])
                .wrapping_add(s1);
        }
        let mut working = state;
        for index in 0..64 {
            let s1 = working[4].rotate_right(6)
                ^ working[4].rotate_right(11)
                ^ working[4].rotate_right(25);
            let ch = (working[4] & working[5]) ^ ((!working[4]) & working[6]);
            let temp1 = working[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[index])
                .wrapping_add(schedule[index]);
            let s0 = working[0].rotate_right(2)
                ^ working[0].rotate_right(13)
                ^ working[0].rotate_right(22);
            let maj =
                (working[0] & working[1]) ^ (working[0] & working[2]) ^ (working[1] & working[2]);
            let temp2 = s0.wrapping_add(maj);
            working.copy_within(0..7, 1);
            working[0] = temp1.wrapping_add(temp2);
            working[4] = working[4].wrapping_add(temp1);
        }
        for (value, update) in state.iter_mut().zip(working) {
            *value = value.wrapping_add(update);
        }
    }
    let mut output = [0u8; 64];
    for (index, byte) in state.iter().flat_map(|word| word.to_be_bytes()).enumerate() {
        output[index * 2] = b"0123456789abcdef"[(byte >> 4) as usize];
        output[index * 2 + 1] = b"0123456789abcdef"[(byte & 0x0f) as usize];
    }
    output
}

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
    let source_dir = normalize_absolute(&prefix.join("share/birnam/src/id"))?;
    let rust_workspace_dir = override_or(
        overrides.rust_workspace_dir,
        &prefix.join("share/birnam/src"),
    )?;
    let guide_dir = normalize_absolute(&prefix.join("share/birnam/docs/guide"))?;
    let id_dylib = override_or(overrides.id_dylib, &lib_dir.join("libbirnam_id.dylib"))?;

    Ok(Resources {
        prefix,
        lib_dir,
        source_dir,
        rust_workspace_dir,
        guide_dir,
        id_dylib,
    })
}

/// Resolve the production layout from the current executable and the same
/// narrow environment overrides accepted by the Zig host.
pub fn resolve() -> io::Result<Resources> {
    let executable = env::current_exe()?;
    let lib_dir = env::var_os("BIRNAM_LIB_DIR").map(PathBuf::from);
    let id_dylib = env::var_os("BIRNAM_ID_DYLIB").map(PathBuf::from);
    let rust_workspace_dir = env::var_os("BIRNAM_RUST_WORKSPACE_DIR").map(PathBuf::from);
    resolve_from_exe_dir(
        executable.parent().unwrap_or_else(|| Path::new(".")),
        Overrides {
            lib_dir: lib_dir.as_deref(),
            id_dylib: id_dylib.as_deref(),
            rust_workspace_dir: rust_workspace_dir.as_deref(),
        },
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

    fn artifact_field(value: &[u8], output: &mut Vec<u8>) {
        output.extend_from_slice(&(value.len() as u32).to_le_bytes());
        output.extend_from_slice(value);
    }

    fn artifact_list(values: &[&[u8]], output: &mut Vec<u8>) {
        output.extend_from_slice(&(values.len() as u32).to_le_bytes());
        for value in values {
            artifact_field(value, output);
        }
    }

    fn artifact_fixture(identity: &[u8], requires: &[&[u8]]) -> Vec<u8> {
        let mut payload = Vec::new();
        artifact_field(b"module", &mut payload);
        artifact_field(identity, &mut payload);
        artifact_field(birnam_artifact::ABI, &mut payload);
        artifact_field(birnam_artifact::TARGET, &mut payload);
        artifact_field(b"source", &mut payload);
        artifact_field(b"revision", &mut payload);
        artifact_list(requires, &mut payload);
        artifact_list(&[], &mut payload);
        artifact_list(&[], &mut payload);
        let mut output = Vec::new();
        output.extend_from_slice(birnam_artifact::DESCRIPTOR_MAGIC);
        output.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        output.extend_from_slice(&payload);
        output
    }

    #[test]
    fn uses_one_normalized_executable_relative_layout() {
        let resources = resolve_from_exe_dir(Path::new("/opt/birnam/bin"), Overrides::default())
            .expect("layout should resolve");
        assert_eq!(resources.prefix, Path::new("/opt/birnam"));
        assert_eq!(resources.lib_dir, Path::new("/opt/birnam/lib"));
        assert_eq!(
            resources.source_dir,
            Path::new("/opt/birnam/share/birnam/src/id")
        );
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
        assert_eq!(
            resources.source_dir,
            Path::new("/opt/birnam/share/birnam/src/id")
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

    #[test]
    fn sha256_matches_the_floor_digest_format() {
        assert_eq!(
            sha256_hex(b"abc"),
            *b"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn missing_floor_is_rejected_before_dlopen() {
        assert!(matches!(
            load_floor(Path::new("/tmp/birnam-host-no-floor.dylib"), None, None),
            Err(FloorError::Read { .. })
        ));
    }

    #[test]
    fn pure_help_and_version_exits_precede_resource_resolution() {
        assert_eq!(classify_early_args(&["--help"]), EarlyAction::Help);
        assert_eq!(classify_early_args(&["-V"]), EarlyAction::Version);
        assert_eq!(
            classify_early_args(&["project.birnam"]),
            EarlyAction::Continue
        );
        assert_eq!(classify_early_args::<&str>(&[]), EarlyAction::Continue);
    }

    #[test]
    fn embedded_self_hosted_sources_preserve_declared_dependencies() {
        let compiled_unit = SELF_HOSTED_MODULES
            .iter()
            .find(|module| module.name == "compiledUnit")
            .expect("compiledUnit source is embedded");
        assert_eq!(
            self_hosted_requires(compiled_unit.source),
            ["modules", "stdlib"]
        );
        let config = SELF_HOSTED_MODULES
            .iter()
            .find(|module| module.name == "config")
            .expect("config source is embedded");
        assert_eq!(self_hosted_requires(config.source), ["toml"]);
    }

    #[test]
    fn self_hosted_boot_order_is_stable_and_dependency_safe() {
        assert_eq!(
            self_hosted_boot_order().expect("embedded graph is acyclic"),
            [
                "stdlib",
                "compiledUnit",
                "toml",
                "config",
                "appConfig",
                "isolate"
            ]
        );
    }

    #[test]
    fn dependency_scanner_ignores_sources_without_a_declaration() {
        assert!(self_hosted_requires(b"// no dependency declaration\n").is_empty());
    }

    #[test]
    fn embedded_self_hosted_inventory_is_nonempty_and_unique() {
        for (index, module) in SELF_HOSTED_MODULES.iter().enumerate() {
            assert!(!module.name.is_empty());
            assert!(!module.source.is_empty());
            assert!(
                SELF_HOSTED_MODULES[index + 1..]
                    .iter()
                    .all(|other| other.name != module.name),
                "duplicate self-hosted module name: {}",
                module.name
            );
        }
    }

    #[test]
    fn self_hosted_candidate_stays_in_the_resolved_library_directory() {
        let resources = resolve_from_exe_dir(Path::new("/opt/birnam/bin"), Overrides::default())
            .expect("layout should resolve");
        let module = SELF_HOSTED_MODULES
            .iter()
            .find(|module| module.name == "toml")
            .expect("toml source is embedded");
        assert_eq!(
            self_hosted_candidate_path(&resources, module),
            Path::new("/opt/birnam/lib/libbirnam_toml.dylib")
        );
    }

    #[test]
    fn manager_is_a_separate_first_boot_step() {
        assert_eq!(MODULE_MANAGER.name, "modules");
        assert!(!MODULE_MANAGER.source.is_empty());
        assert_eq!(KERNEL_MODULES.first(), Some(&"artifact"));
        assert!(
            !SELF_HOSTED_MODULES
                .iter()
                .any(|module| module.name == MODULE_MANAGER.name)
        );
    }

    #[test]
    fn startup_policy_preserves_explicit_flag_semantics() {
        assert_eq!(
            StartupPolicy::from_flags(true, false),
            StartupPolicy {
                force_source: true,
                skip_stdlib: false,
            }
        );
        assert_eq!(
            StartupPolicy::from_flags(false, true),
            StartupPolicy {
                force_source: false,
                skip_stdlib: true,
            }
        );
    }

    #[test]
    fn top_level_command_classification_preserves_file_kinds() {
        assert_eq!(classify_command::<&str>(&[]), Ok(CommandKind::Repl));
        assert_eq!(
            classify_command(&["--lsp"]).expect("lsp is a top-level command"),
            CommandKind::Lsp
        );
        assert_eq!(
            classify_command(&["program.birnam"]).expect("source is a script"),
            CommandKind::Script(PathBuf::from("program.birnam"))
        );
        assert_eq!(
            classify_command(&["saved.image"]).expect("image extension is explicit"),
            CommandKind::Image(PathBuf::from("saved.image"))
        );
        assert_eq!(
            classify_command(&["libbirnam_app.dylib"]).expect("dylib extension is explicit"),
            CommandKind::Artifact(PathBuf::from("libbirnam_app.dylib"))
        );
    }

    #[test]
    fn top_level_unknown_flags_are_rejected_without_runtime_access() {
        assert_eq!(
            classify_command(&["--not-a-command"]),
            Err(CommandError::UnknownOption("--not-a-command".to_owned()))
        );
        assert_eq!(
            classify_command(&["--rebuild"]).expect("rebuild is top-level"),
            CommandKind::Rebuild
        );
        assert_eq!(
            classify_command(&["--project-test-worker"]).expect("worker is internal"),
            CommandKind::ProjectTestWorker
        );
        assert_eq!(
            classify_command(&["--compile-module"]).expect("compile module is internal"),
            CommandKind::CompileModule
        );
    }

    #[test]
    fn check_parser_preserves_defaults_formats_and_help() {
        assert_eq!(
            parse_check_args::<&str>(&[]).expect("defaults parse"),
            ParseOutcome::Parsed(CheckOptions {
                directory: PathBuf::from("."),
                format: OutputFormat::Human,
            })
        );
        assert_eq!(
            parse_check_args(&["--format=json", "project"]).expect("format and directory parse"),
            ParseOutcome::Parsed(CheckOptions {
                directory: PathBuf::from("project"),
                format: OutputFormat::Json,
            })
        );
        assert_eq!(
            parse_check_args(&["--help"]).expect("help parses"),
            ParseOutcome::Help
        );
    }

    #[test]
    fn build_parser_preserves_profile_aliases_and_validation() {
        assert_eq!(
            parse_build_args(&["--profile", "release", "project"])
                .expect("profile and directory parse"),
            ParseOutcome::Parsed(BuildOptions {
                directory: PathBuf::from("project"),
                profile: "release".to_owned(),
            })
        );
        assert_eq!(
            parse_build_args(&["--profile=fast"]),
            Err(ProjectParseError::InvalidValue {
                option: "--profile",
                value: "fast".to_owned(),
            })
        );
        assert_eq!(
            parse_build_args(&["one", "two"]),
            Err(ProjectParseError::DuplicateDirectory)
        );
    }

    #[test]
    fn install_and_test_parsers_share_build_and_profile_policy() {
        assert_eq!(
            parse_install_args(&["--build", "--profile=release"]).expect("install options parse"),
            ParseOutcome::Parsed(InstallOptions {
                directory: PathBuf::from("."),
                build: true,
                profile: "release".to_owned(),
            })
        );
        assert_eq!(
            parse_test_args(&["project", "--profile", "debug"]).expect("test options parse"),
            ParseOutcome::Parsed(TestOptions {
                directory: PathBuf::from("project"),
                build: false,
                profile: "debug".to_owned(),
            })
        );
        assert_eq!(
            parse_install_args(&["--help"]).expect("help parses"),
            ParseOutcome::Help
        );
    }

    #[test]
    fn buildish_parser_rejects_missing_profile_and_unknown_flags() {
        assert_eq!(
            parse_test_args(&["--profile"]),
            Err(ProjectParseError::MissingValue("--profile"))
        );
        assert_eq!(
            parse_install_args(&["--wat"]),
            Err(ProjectParseError::UnknownOption("--wat".to_owned()))
        );
    }

    #[test]
    fn run_parser_preserves_repl_and_socket_options() {
        assert_eq!(
            parse_run_args(&[
                "project",
                "--build",
                "--profile=release",
                "--repl=socket",
                "--repl-bind",
                "127.0.0.1",
                "--repl-port=4400",
                "--",
                "--user-flag",
                "value",
            ])
            .expect("run options parse"),
            ParseOutcome::Parsed(RunOptions {
                directory: PathBuf::from("project"),
                build: true,
                profile: "release".to_owned(),
                repl: Some(ReplMode::Socket),
                socket_bind: Some("127.0.0.1".to_owned()),
                socket_port: Some(4400),
                arguments: vec![
                    std::ffi::OsString::from("--user-flag"),
                    std::ffi::OsString::from("value"),
                ],
            })
        );
    }

    #[test]
    fn run_parser_preserves_default_and_rejects_invalid_repl_values() {
        assert_eq!(
            parse_run_args::<&str>(&[]).expect("run defaults parse"),
            ParseOutcome::Parsed(RunOptions {
                directory: PathBuf::from("."),
                build: false,
                profile: "debug".to_owned(),
                repl: None,
                socket_bind: None,
                socket_port: None,
                arguments: Vec::new(),
            })
        );
        assert_eq!(
            parse_run_args(&["--repl=web"]),
            Err(RunParseError::InvalidValue {
                option: "--repl",
                value: "web".to_owned(),
            })
        );
        assert_eq!(
            parse_run_args(&["--repl-port=70000"]),
            Err(RunParseError::InvalidValue {
                option: "--repl-port",
                value: "70000".to_owned(),
            })
        );
    }

    #[test]
    fn tooling_parsers_preserve_exact_path_operands() {
        assert_eq!(
            parse_bake_args(&["saved.image", "standalone-birnam"]).expect("bake operands parse"),
            BakeOptions {
                image: PathBuf::from("saved.image"),
                executable: PathBuf::from("standalone-birnam"),
            }
        );
        assert_eq!(
            parse_rebuild_args(&["saved.image"]).expect("rebuild operand parses"),
            RebuildOptions {
                image: PathBuf::from("saved.image"),
            }
        );
        assert_eq!(
            parse_compile_module_args(&["module.birnam", "module.dylib"])
                .expect("compile operands parse"),
            CompileModuleOptions {
                source: PathBuf::from("module.birnam"),
                output: PathBuf::from("module.dylib"),
            }
        );
    }

    #[test]
    fn tooling_parsers_reject_wrong_arity_before_runtime_access() {
        assert_eq!(
            parse_bake_args::<&str>(&["only-image"]),
            Err(ToolingParseError::Arity {
                command: "--bake",
                expected: 2,
                actual: 1,
            })
        );
        assert_eq!(
            parse_rebuild_args::<&str>(&[]),
            Err(ToolingParseError::Arity {
                command: "--rebuild",
                expected: 1,
                actual: 0,
            })
        );
    }

    #[test]
    fn bake_trailer_round_trips_exact_image_bytes() {
        let runtime = b"runtime-prefix";
        let image = b"BIRNAMIM\x01payload";
        let baked = bake_executable(runtime, image).expect("nonempty image bakes");
        assert_eq!(baked_image(&baked), Some(image.as_slice()));
        assert_eq!(&baked[..runtime.len()], runtime);
        assert!(baked_image(runtime).is_none());
    }

    #[test]
    fn bake_trailer_rejects_empty_and_corrupt_images() {
        assert_eq!(bake_executable(b"runtime", b""), Err(BakeError::EmptyImage));
        let mut corrupt = bake_executable(b"runtime", b"image").expect("fixture bakes");
        let trailer_start = corrupt.len() - BAKE_TRAILER_LEN;
        let magic_start = corrupt.len() - BAKE_MAGIC.len();
        corrupt[trailer_start..magic_start].copy_from_slice(&0u64.to_le_bytes());
        assert!(baked_image(&corrupt).is_none());
        corrupt[magic_start..].copy_from_slice(b"NOTBAKED");
        assert!(baked_image(&corrupt).is_none());
    }

    #[test]
    fn script_source_limit_matches_the_shipped_host_contract() {
        assert_eq!(MAX_SCRIPT_BYTES, 16 << 20);
    }

    #[test]
    fn image_outcomes_keep_mapping_and_source_recovery_distinct() {
        assert_ne!(ImageLoadOutcome::Mapped, ImageLoadOutcome::SourceRecovery);
    }

    #[test]
    fn worker_status_zero_is_success_and_nonzero_is_preserved() {
        assert_eq!(WorkerError::Failed(7), WorkerError::Failed(7));
        assert_ne!(WorkerError::Failed(1), WorkerError::Failed(2));
    }

    #[test]
    fn runtime_module_names_are_checked_before_manager_sends() {
        assert_ne!(
            RuntimeModuleError::EmbeddedNul,
            RuntimeModuleError::MissingModuleManager
        );
    }

    #[test]
    fn artifact_preflight_walks_siblings_dependency_first() {
        let root =
            std::env::temp_dir().join(format!("birnam-host-preflight-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("fixture directory creates");
        let root_path = root.join("libbirnam_root.dylib");
        let dependency_path = root.join("libbirnam_dep.dylib");
        std::fs::write(&root_path, artifact_fixture(b"root", &[b"dep"]))
            .expect("root fixture writes");
        std::fs::write(&dependency_path, artifact_fixture(b"dep", &[]))
            .expect("dependency fixture writes");
        let plan = preflight_artifact(&root_path, Some(b"root")).expect("closure preflights");
        assert_eq!(
            plan.entries
                .iter()
                .map(|entry| entry.identity.as_slice())
                .collect::<Vec<_>>(),
            vec![b"dep".as_slice(), b"root".as_slice()]
        );
        std::fs::remove_dir_all(&root).expect("fixture directory removes");
    }

    #[test]
    fn artifact_preflight_rejects_identity_mismatch_and_missing_sibling() {
        let root = std::env::temp_dir().join(format!(
            "birnam-host-preflight-error-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("fixture directory creates");
        let root_path = root.join("libbirnam_root.dylib");
        std::fs::write(&root_path, artifact_fixture(b"root", &[b"dep"]))
            .expect("root fixture writes");
        assert!(matches!(
            preflight_artifact(&root_path, Some(b"other")),
            Err(ArtifactPreflightError::IdentityMismatch { .. })
        ));
        assert!(matches!(
            preflight_artifact(&root_path, Some(b"root")),
            Err(ArtifactPreflightError::Read { .. })
        ));
        std::fs::remove_dir_all(&root).expect("fixture directory removes");
    }
}
