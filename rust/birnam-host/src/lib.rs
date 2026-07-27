//! Pure executable-relative runtime-resource resolution for the Rust host.
//!
//! This is deliberately not a loader and does not inspect the filesystem. It
//! only reproduces the installed Birnam layout and the three narrow resource
//! overrides accepted by the existing Zig host. Loading a floor or any higher
//! rung remains a later host boundary.

use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::{CStr, CString, OsStr, OsString, c_char, c_int, c_void};
use std::fmt;
use std::io;
use std::io::Write;
use std::marker::PhantomData;
use std::path::{Component, Path, PathBuf};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicPtr, Ordering};

use birnam_abi::{IdApi, IdInitFn};

#[cfg(unix)]
use std::os::unix::ffi::{OsStrExt, OsStringExt};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectTestWorkerOptions {
    pub project_root: PathBuf,
    pub profile: String,
    pub test_source: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectTestWorkerParseError {
    WrongArity,
    NonUtf8Profile,
    InvalidProfile(String),
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

/// Return the frozen `<arch>-<os>` build target identity used by the Zig
/// project adapter and artifact policy.
pub fn host_target_identity() -> String {
    format!("{}-{}", env::consts::ARCH, env::consts::OS)
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerOptions {
    pub image: PathBuf,
    pub receiver: String,
    pub selector: String,
    pub channel_fd: c_int,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerParseError {
    Arity { expected: usize, actual: usize },
    MissingChannelFlag(String),
    InvalidChannelFd(String),
    NonUtf8Receiver,
    NonUtf8Selector,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileModuleError {
    Read { path: PathBuf, kind: io::ErrorKind },
    TooLarge { path: PathBuf, limit: usize },
    MissingCodegen,
    CompilationRejected,
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

#[derive(Clone, PartialEq, Eq)]
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
    DependencyInvalid {
        path: PathBuf,
        dependency: Vec<u8>,
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

impl fmt::Debug for ArtifactPreflightError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, kind } => {
                write!(
                    formatter,
                    "artifactUnavailable: {} ({kind:?})",
                    path.display()
                )
            }
            Self::Parse { path, error } => {
                let diagnostic = match error {
                    birnam_artifact::Error::AbiMismatch => "artifactAbiMismatch",
                    birnam_artifact::Error::TargetMismatch => "artifactTargetMismatch",
                    birnam_artifact::Error::EmptyIdentity => "artifactDescriptionMissing",
                    birnam_artifact::Error::ArtifactTooLarge
                    | birnam_artifact::Error::MissingDescriptor
                    | birnam_artifact::Error::MultipleDescriptors
                    | birnam_artifact::Error::TruncatedDescriptor
                    | birnam_artifact::Error::InvalidDescriptor
                    | birnam_artifact::Error::ListTooLarge => "artifactDescriptionMissing",
                };
                write!(formatter, "{diagnostic}: {}", path.display())
            }
            Self::KindMismatch { path } => {
                write!(formatter, "artifactKindMismatch: {}", path.display())
            }
            Self::IdentityMismatch { path, .. } => {
                write!(formatter, "artifactIdentityMismatch: {}", path.display())
            }
            Self::DependencyInvalid { path, .. } => {
                write!(formatter, "artifactDependencyInvalid: {}", path.display())
            }
            Self::DependencyCycle { identity } => {
                write!(formatter, "artifactDependencyCycle: {:?}", identity)
            }
            Self::NativeRequirementMissing {
                path, requirement, ..
            } => write!(
                formatter,
                "artifactNativeRequirementMissing: {} ({:?})",
                path.display(),
                requirement
            ),
            Self::InvalidDependencyName { path, dependency } => write!(
                formatter,
                "artifactDependencyInvalid: {} ({:?})",
                path.display(),
                dependency
            ),
        }
    }
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
            if let Err(error) = self.walk(&dependency_path, Some(requirement)) {
                match error {
                    ArtifactPreflightError::IdentityMismatch { .. }
                    | ArtifactPreflightError::DependencyCycle { .. } => return Err(error),
                    // A nested descriptor's kind is a structural contract
                    // failure, not an unavailable dependency. Preserve the
                    // Zig oracle's typed diagnostic instead of collapsing it
                    // into the recursive dependency wrapper.
                    ArtifactPreflightError::KindMismatch { .. } => return Err(error),
                    _ => {
                        return Err(ArtifactPreflightError::DependencyInvalid {
                            path: dependency_path,
                            dependency: requirement.clone(),
                        });
                    }
                }
            }
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

/// Compile one authored self-hosted module into an installable compiled unit.
///
/// This is the Rust-host equivalent of the Zig siphon's private
/// `--compile-module` route. The caller deliberately starts the runtime with
/// `skip_stdlib`, so stage-1 can compile the stdlib itself without depending on
/// a pre-existing self-hosted source or compiled-unit closure.
///
/// # Safety
///
/// The runtime must remain on the selected floor's owner thread. The caller
/// must set a valid GC stack bottom before invoking this operation and keep it
/// live for the complete synchronous compiler send.
pub unsafe fn compile_module(
    runtime: &HostRuntime,
    source_path: &Path,
    output_path: &Path,
) -> Result<(), CompileModuleError> {
    let source = std::fs::read(source_path).map_err(|error| CompileModuleError::Read {
        path: source_path.to_path_buf(),
        kind: error.kind(),
    })?;
    if source.len() > MAX_SCRIPT_BYTES {
        return Err(CompileModuleError::TooLarge {
            path: source_path.to_path_buf(),
            limit: MAX_SCRIPT_BYTES,
        });
    }
    let api = runtime.stack.api();
    let codegen = unsafe { (api.import)(c"Codegen".as_ptr()) };
    if codegen == api.nilObj {
        return Err(CompileModuleError::MissingCodegen);
    }
    let source_value = unsafe { (api.mkString)(source.as_ptr(), source.len()) };
    let output_bytes = path_bytes(output_path);
    let output_value = unsafe { (api.mkString)(output_bytes.as_ptr(), output_bytes.len()) };
    let source_bytes = path_bytes(source_path);
    let source_path_value = unsafe { (api.mkString)(source_bytes.as_ptr(), source_bytes.len()) };
    let selector = unsafe { (api.intern)(c"compile:output:from:".as_ptr()) };
    let result = unsafe {
        (api.send)(
            codegen,
            selector,
            [source_value, output_value, source_path_value].as_ptr(),
            3,
        )
    };
    if result == api.nilObj || send_is_error(api, result) {
        return Err(CompileModuleError::CompilationRejected);
    }
    Ok(())
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
    runtime: &mut HostRuntime,
    path: &Path,
    rebuild: bool,
) -> Result<ImageLoadOutcome, ImageError> {
    let api_ptr = runtime.stack.api() as *const IdApi;
    // SAFETY: the retained floor table outlives this synchronous load and the
    // owner thread remains exclusive for the whole operation.
    let api = unsafe { &*api_ptr };
    let system = unsafe { (api.import)(c"System".as_ptr()) };
    if system == api.nilObj {
        return Err(ImageError::MissingSystem);
    }
    let path_bytes = path_bytes(path);
    let path_value = unsafe { (api.mkString)(path_bytes.as_ptr(), path_bytes.len()) };
    let rebuild_value = if rebuild { api.trueObj } else { api.falseObj };
    let selector = unsafe { (api.intern)(c"loadImage:rebuild:".as_ptr()) };
    let mut status =
        unsafe { (api.send)(system, selector, [path_value, rebuild_value].as_ptr(), 2) };
    if status == api.falseObj
        && env::var("BIRNAM_RUST_IMAGE_CANONICAL").ok().as_deref() == Some("rust")
    {
        // Rust Image owns the canonical selector surface, while Zig Image is
        // retained as the explicit v4 reader rollback until Rust Codegen's
        // reader is canonical. Load that reader only for this failed mixed
        // authority attempt, then restore Rust Image's methods immediately.
        let zig_image = runtime
            .resources
            .lib_dir
            .join("libbirnam_id_image_zig.dylib");
        runtime
            .stack
            .load_module(&zig_image)
            .map_err(|_| ImageError::LoadRejected)?;
        status = unsafe { (api.send)(system, selector, [path_value, rebuild_value].as_ptr(), 2) };
        let rust_image = runtime.resources.lib_dir.join("libbirnam_id_image.dylib");
        runtime
            .stack
            .load_module(&rust_image)
            .map_err(|_| ImageError::LoadRejected)?;
    }
    if status == api.nilObj {
        return Err(ImageError::LoadRejected);
    }
    if status == api.trueObj {
        // A mapped image still cannot carry native Alien trampolines. Reopen
        // the Rust Codegen recovery boundary for the same replay/derivation
        // hooks used by source recovery before exposing the image to the REPL.
        // Without this, image-resident methods that call an Alien wrapper keep
        // the stale process from the saving runtime and silently read zeros.
        let codegen = unsafe { (api.import)(c"Codegen".as_ptr()) };
        if codegen != api.nilObj {
            let begin_recovery = unsafe { (api.intern)(c"beginImageDispatchRecovery".as_ptr()) };
            let begun = unsafe { (api.send)(codegen, begin_recovery, std::ptr::null(), 0) };
            if begun == api.nilObj || send_is_error(api, begun) {
                return Err(ImageError::LoadRejected);
            }
            let replay_aliens = unsafe { (api.intern)(c"replayAliens".as_ptr()) };
            unsafe { (api.send)(codegen, replay_aliens, std::ptr::null(), 0) };
            let derive_globals = unsafe { (api.intern)(c"deriveImageGlobals".as_ptr()) };
            unsafe { (api.send)(codegen, derive_globals, std::ptr::null(), 0) };
            let end_recovery = unsafe { (api.intern)(c"endImageDispatchRecovery".as_ptr()) };
            unsafe { (api.send)(codegen, end_recovery, std::ptr::null(), 0) };
            let current_digest = unsafe {
                (api.send)(
                    codegen,
                    (api.intern)(c"currentGenerationDigest".as_ptr()),
                    std::ptr::null(),
                    0,
                )
            };
            if current_digest != api.nilObj && !current_digest.is_null() {
                let sealed = unsafe {
                    (api.send)(
                        codegen,
                        (api.intern)(c"sealInitialGeneration".as_ptr()),
                        std::ptr::null(),
                        0,
                    )
                };
                if sealed != api.trueObj {
                    return Err(ImageError::LoadRejected);
                }
            }
        }
        return Ok(ImageLoadOutcome::Mapped);
    }
    if status != api.falseObj {
        return Err(ImageError::UnexpectedStatus);
    }

    let codegen = unsafe { (api.import)(c"Codegen".as_ptr()) };
    if codegen != api.nilObj {
        // `System loadImage:rebuild:` closes the image module's internal
        // recovery phase before returning. Rust Codegen deliberately requires
        // replay/derivation to run inside that phase, so reopen the host-owned
        // recovery boundary around the post-load operations just as the Zig
        // image driver does within its load transaction.
        let begin_recovery = unsafe { (api.intern)(c"beginImageDispatchRecovery".as_ptr()) };
        let begun = unsafe { (api.send)(codegen, begin_recovery, std::ptr::null(), 0) };
        if begun == api.nilObj || send_is_error(api, begun) {
            return Err(ImageError::LoadRejected);
        }
        let replay_methods = unsafe { (api.intern)(c"replayMethods".as_ptr()) };
        let replayed = unsafe { (api.send)(codegen, replay_methods, std::ptr::null(), 0) };
        // A data-only image (and an explicit --rebuild of an image whose
        // journal contains no user methods) still needs the self-hosted
        // protocol layer. The canonical Zig recovery path reloads the
        // embedded stdlib in this zero-replay case; do the same through the
        // Rust host bootstrap so Config/Module/REPL globals cannot remain as
        // stale image objects without their method surface.
        let replay_count = if unsafe { (api.isSmallInt)(replayed) } != 0 {
            unsafe { (api.smallIntValue)(replayed) }
        } else {
            0
        };
        let config_surface_missing = unsafe { global_named(api, "Config") }
            .map(|config| unsafe {
                let loads = (api.intern)(c"loads".as_ptr());
                !receiver_has_selector(api, config, loads)
            })
            .unwrap_or(false);
        if replay_count == 0 || config_surface_missing {
            unsafe {
                bootstrap_self_hosted(
                    api,
                    codegen,
                    &runtime.resources,
                    runtime.policy.force_source,
                    false,
                )
            }
            .map_err(|_| ImageError::LoadRejected)?;
        }
        let replay_aliens = unsafe { (api.intern)(c"replayAliens".as_ptr()) };
        unsafe { (api.send)(codegen, replay_aliens, std::ptr::null(), 0) };
        // Source-backed block globals are intentionally not serialized as
        // durable values. Re-derive them after code/method recovery so an
        // ordinary image boot and an explicit rebuild expose the same globals
        // before the REPL evaluates its first expression.
        let derive_globals = unsafe { (api.intern)(c"deriveImageGlobals".as_ptr()) };
        unsafe { (api.send)(codegen, derive_globals, std::ptr::null(), 0) };
        let end_recovery = unsafe { (api.intern)(c"endImageDispatchRecovery".as_ptr()) };
        unsafe { (api.send)(codegen, end_recovery, std::ptr::null(), 0) };
        let current_digest = unsafe {
            (api.send)(
                codegen,
                (api.intern)(c"currentGenerationDigest".as_ptr()),
                std::ptr::null(),
                0,
            )
        };
        if current_digest != api.nilObj && !current_digest.is_null() {
            let sealed = unsafe {
                (api.send)(
                    codegen,
                    (api.intern)(c"sealInitialGeneration".as_ptr()),
                    std::ptr::null(),
                    0,
                )
            };
            if sealed != api.trueObj {
                return Err(ImageError::LoadRejected);
            }
        }
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

/// Parse the private project-test worker triplet emitted by the Rust test
/// driver. The worker receives no user flags and preserves exact paths.
pub fn parse_project_test_worker_args<T: AsRef<std::ffi::OsStr>>(
    args: &[T],
) -> Result<ProjectTestWorkerOptions, ProjectTestWorkerParseError> {
    if args.len() != 3 {
        return Err(ProjectTestWorkerParseError::WrongArity);
    }
    let profile = args[1]
        .as_ref()
        .to_str()
        .ok_or(ProjectTestWorkerParseError::NonUtf8Profile)?;
    if profile != "debug" && profile != "release" {
        return Err(ProjectTestWorkerParseError::InvalidProfile(
            profile.to_owned(),
        ));
    }
    Ok(ProjectTestWorkerOptions {
        project_root: PathBuf::from(args[0].as_ref()),
        profile: profile.to_owned(),
        test_source: PathBuf::from(args[2].as_ref()),
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

/// Parse the private isolate-worker route without opening the image or floor.
/// The channel descriptor remains an inherited integer; ownership and worker
/// protocol execution stay with the isolate hook.
pub fn parse_worker_args<T: AsRef<std::ffi::OsStr>>(
    args: &[T],
) -> Result<WorkerOptions, WorkerParseError> {
    if args.len() != 5 {
        return Err(WorkerParseError::Arity {
            expected: 5,
            actual: args.len(),
        });
    }
    if args[3].as_ref() != std::ffi::OsStr::new("--channel-fd") {
        return Err(WorkerParseError::MissingChannelFlag(
            args[3].as_ref().to_string_lossy().into_owned(),
        ));
    }
    let receiver = args[1]
        .as_ref()
        .to_str()
        .ok_or(WorkerParseError::NonUtf8Receiver)?
        .to_owned();
    let selector = args[2]
        .as_ref()
        .to_str()
        .ok_or(WorkerParseError::NonUtf8Selector)?
        .to_owned();
    let channel_fd = args[4]
        .as_ref()
        .to_str()
        .ok_or_else(|| WorkerParseError::InvalidChannelFd("non-UTF-8".to_owned()))?
        .parse::<c_int>()
        .map_err(|_| {
            WorkerParseError::InvalidChannelFd(args[4].as_ref().to_string_lossy().into_owned())
        })?;
    Ok(WorkerOptions {
        image: PathBuf::from(args[0].as_ref()),
        receiver,
        selector,
        channel_fd,
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

#[derive(Clone, PartialEq, Eq)]
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

impl fmt::Debug for FloorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } if *source == io::ErrorKind::NotFound => write!(
                formatter,
                "required runtime dylib is unavailable: {}",
                path.display()
            ),
            Self::Read { path, source } => {
                write!(
                    formatter,
                    "floor read failed: {} ({source:?})",
                    path.display()
                )
            }
            Self::DigestMismatch { expected, actual } => {
                write!(
                    formatter,
                    "DigestMismatch {{ expected: {expected:?}, actual: {actual:?} }}"
                )
            }
            Self::EmbeddedNul => formatter.write_str("EmbeddedNul"),
            Self::OpenFailed => formatter.write_str("OpenFailed"),
            Self::MissingIdInit => formatter.write_str("MissingIdInit"),
            Self::NullApi => formatter.write_str("NullApi"),
            Self::MissingIdentity => formatter.write_str("MissingIdentity"),
            Self::IdentityMismatch { expected, actual } => write!(
                formatter,
                "IdentityMismatch {{ expected: {expected:?}, actual: {actual:?} }}"
            ),
        }
    }
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

/// Project policy is intentionally lazy: it participates in check/build/run
/// workflows but must not become part of ordinary runtime/image bootstrap.
pub const PROJECT_POLICY_MODULE: SelfHostedModule = SelfHostedModule {
    name: "projectPolicy",
    source: include_bytes!("../../../src/id/project-policy.birnam"),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectPolicyError {
    Source(BootstrapSourceError),
    MissingPolicy,
    InvalidSnapshot,
    ResolutionRejected,
    ResolutionDiagnostic { kind: String, message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRunObservation {
    pub repl: String,
    pub socket_bind: String,
    pub socket_port: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectModulePlan {
    pub entry: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectManifestObservation {
    pub name: String,
    pub main: Option<String>,
    pub source_roots: Vec<String>,
    pub loads: Vec<String>,
    pub modules: Vec<ProjectModulePlan>,
    pub dependencies: Vec<ProjectDependencyObservation>,
    pub run: ProjectRunObservation,
    pub wrapped: Vec<ProjectWrappedObservation>,
    pub test_sources: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectWrappedObservation {
    pub name: String,
    pub build: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDependencyObservation {
    pub name: String,
    pub root: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDependencyClosureNode {
    pub root: PathBuf,
    pub dependencies: Vec<ProjectDependencyObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSourceObservation {
    pub name: String,
    pub path: String,
    pub sources: Vec<String>,
    pub project_root: String,
    pub source: Vec<u8>,
    pub requires: Vec<String>,
    pub source_digest: String,
    pub revision_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectObservation {
    pub root: String,
    pub manifest: String,
    pub name: String,
    pub main: Option<String>,
    pub loads: Vec<String>,
    pub run: ProjectRunObservation,
    pub wrapped: Vec<ProjectWrappedObservation>,
    pub dependencies: Vec<ProjectDependencyObservation>,
    pub sources: Vec<ProjectSourceObservation>,
    pub test_sources: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSnapshot {
    pub root: String,
    pub projects: Vec<ProjectObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectResolutionObservation {
    pub project_root: String,
    pub manifest: String,
    pub project_name: String,
    pub main: Option<String>,
    pub load_roots: Vec<String>,
    pub modules: Vec<ProjectSourceObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectCompileDiagnostic {
    pub module: String,
    pub path: String,
    pub detail: String,
    pub message: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectCheckResult {
    pub resolution: ProjectResolutionObservation,
    pub resolution_digest: String,
    pub wrapped: Vec<ProjectWrappedObservation>,
    pub test_sources: Option<Vec<String>>,
    pub diagnostics: Vec<ProjectCompileDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectArtifactPlanEntry {
    pub name: String,
    pub path: String,
    pub action: String,
    pub source: String,
    pub sources: Vec<String>,
    pub source_digest: String,
    pub revision_digest: String,
    pub requires: Vec<String>,
    pub checksum: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectArtifactPlan {
    pub project_root: String,
    pub target: String,
    pub profile: String,
    pub tool_version: String,
    pub artifact_abi: String,
    pub built_at: Option<String>,
    pub resolution_digest: String,
    pub artifacts: Vec<ProjectArtifactPlanEntry>,
    pub publication: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectArtifactPlanValidationError {
    MetadataMismatch(&'static str),
    ArtifactCount { expected: usize, actual: usize },
    ArtifactMismatch { index: usize, field: &'static str },
    InvalidAction { index: usize, action: String },
    ChecksumMismatch { index: usize, action: String },
    InvalidPublication(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectCompiledArtifactValidationError {
    Descriptor(birnam_artifact::Error),
    KindMismatch,
    IdentityMismatch { expected: String, actual: Vec<u8> },
    SourceDigestMismatch,
    RevisionDigestMismatch,
    DependencyMismatch { index: usize, actual: Vec<u8> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectArtifactCompileError {
    CodegenRejected { module: String, detail: String },
    ReadOutput { path: PathBuf, kind: io::ErrorKind },
    Descriptor(ProjectCompiledArtifactValidationError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectStagedArtifact {
    pub name: String,
    pub path: String,
    pub action: String,
    pub checksum: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectArtifactStageRequest<'a> {
    pub resolution: &'a ProjectResolutionObservation,
    pub plan: &'a ProjectArtifactPlan,
    pub previous: Option<&'a ProjectBuildRecord>,
    pub previous_root: Option<&'a Path>,
    pub stage_root: &'a Path,
}

#[derive(Debug, Clone, Copy)]
pub struct ProjectArtifactRecordRequest<'a> {
    pub resolution: &'a ProjectResolutionObservation,
    pub staged: &'a [ProjectStagedArtifact],
    pub native: &'a [ProjectPreviousNativeArtifact],
    pub wrapped: &'a [ProjectWrappedObservation],
    pub target: &'a str,
    pub profile: &'a str,
    pub tool_version: &'a str,
    pub built_at: &'a str,
    pub resolution_digest: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct ProjectBuildRequest<'a> {
    pub start: &'a Path,
    pub target: &'a str,
    pub profile: &'a str,
    pub tool_version: &'a str,
    pub built_at: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectBuildResult {
    pub output_root: PathBuf,
    pub record: ProjectBuildRecord,
    pub artifacts: Vec<ProjectStagedArtifact>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectArtifactStageError {
    CreateDirectory {
        path: PathBuf,
        kind: io::ErrorKind,
    },
    ArtifactCount {
        expected: usize,
        actual: usize,
    },
    InvalidPlanPath {
        index: usize,
        path: String,
    },
    InvalidAction {
        index: usize,
        action: String,
    },
    MissingPrevious {
        name: String,
    },
    MissingPreviousRoot {
        name: String,
    },
    Copy {
        source: PathBuf,
        destination: PathBuf,
        kind: io::ErrorKind,
    },
    Read {
        path: PathBuf,
        kind: io::ErrorKind,
    },
    Compile(ProjectArtifactCompileError),
    Descriptor(ProjectCompiledArtifactValidationError),
    ChecksumMismatch {
        name: String,
        expected: String,
        actual: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectArtifactRecordError {
    Snapshot(ProjectSnapshotError),
    Policy(ProjectPolicyError),
    Decode(ProjectManifestDecodeError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectBuildError {
    Observation(ProjectObservationBuildError),
    Snapshot(ProjectSnapshotError),
    Policy(ProjectPolicyError),
    Resolution(ProjectManifestDecodeError),
    ManifestRead {
        path: PathBuf,
        kind: io::ErrorKind,
    },
    BuildRecord(ProjectBuildRecordLoadError),
    Plan(ProjectArtifactPlanError),
    StageDirectory {
        path: PathBuf,
        kind: io::ErrorKind,
    },
    Wrapped(ProjectWrappedBuildError),
    Stage(ProjectArtifactStageError),
    Record(ProjectArtifactRecordError),
    Write(ProjectBuildRecordWriteError),
    Publish(ProjectBuildPublicationError),
    PublishedPolicy(ProjectPolicyError),
    PublishedArtifact {
        name: String,
        path: PathBuf,
        kind: io::ErrorKind,
    },
    PublishedDescriptor {
        name: String,
        error: ProjectCompiledArtifactValidationError,
    },
    PublishedChecksum {
        name: String,
        expected: String,
        actual: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectWrappedBuildError {
    EmptyCommand {
        name: String,
    },
    Spawn {
        name: String,
        kind: io::ErrorKind,
    },
    Failed {
        name: String,
        code: Option<i32>,
        detail: String,
    },
    MissingOutput {
        name: String,
        path: PathBuf,
    },
    ReadOutput {
        path: PathBuf,
        kind: io::ErrorKind,
    },
    CopyOutput {
        source: PathBuf,
        destination: PathBuf,
        kind: io::ErrorKind,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectInstallError {
    CreateStage {
        path: PathBuf,
        kind: io::ErrorKind,
    },
    InvalidArtifactPath {
        name: String,
        path: String,
    },
    ReadSource {
        path: PathBuf,
        kind: io::ErrorKind,
    },
    CopyStage {
        source: PathBuf,
        destination: PathBuf,
        kind: io::ErrorKind,
    },
    SyncStage {
        path: PathBuf,
        kind: io::ErrorKind,
    },
    Publish {
        source: PathBuf,
        destination: PathBuf,
        kind: io::ErrorKind,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectRunError {
    Build(ProjectBuildError),
    MissingMain,
    InvalidReplMode(String),
    Module(RuntimeModuleError),
    TestHarnessUnavailable,
    TestSuiteMissing,
    TestFailed,
    NativeDirectoryRejected,
    ArtifactPreflight(ArtifactPreflightError),
    ArtifactActivation(ArtifactActivationError),
    ArgumentsRejected,
    Main(ScriptError),
    Repl(ReplError),
    Socket(SocketReplError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectBuildRecordWriteError {
    Create { path: PathBuf, kind: io::ErrorKind },
    Write { path: PathBuf, kind: io::ErrorKind },
    Sync { path: PathBuf, kind: io::ErrorKind },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectBuildPublicationError {
    InvalidPaths,
    CreateTarget {
        path: PathBuf,
        kind: io::ErrorKind,
    },
    Backup {
        path: PathBuf,
        kind: io::ErrorKind,
    },
    Publish {
        stage: PathBuf,
        final_root: PathBuf,
        kind: io::ErrorKind,
    },
    Restore {
        path: PathBuf,
        kind: io::ErrorKind,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct ProjectArtifactPlanRequest<'a> {
    pub resolution: &'a ProjectResolutionObservation,
    pub wrapped: &'a [ProjectWrappedObservation],
    pub previous: Option<&'a ProjectBuildRecord>,
    pub target: &'a str,
    pub profile: &'a str,
    pub tool_version: &'a str,
    pub built_at: &'a str,
    pub resolution_digest: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPreviousArtifact {
    pub name: String,
    pub path: String,
    pub source: String,
    pub sources: Vec<String>,
    pub source_digest: String,
    pub revision_digest: String,
    pub requires: Vec<String>,
    pub checksum: String,
    pub observed_checksum: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPreviousNativeArtifact {
    pub name: String,
    pub path: String,
    pub checksum: String,
    pub observed_checksum: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectBuildRecord {
    pub project_root: String,
    pub target: String,
    pub profile: String,
    pub tool_version: String,
    pub artifact_abi: String,
    pub resolution_digest: String,
    pub built_at: String,
    pub artifacts: Vec<ProjectPreviousArtifact>,
    pub native: Vec<ProjectPreviousNativeArtifact>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectSnapshotError {
    MissingVector,
    MissingMap,
    AllocationRejected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectDiscoveryError {
    Canonicalize { path: PathBuf, kind: io::ErrorKind },
    Inspect { path: PathBuf, kind: io::ErrorKind },
    NotFound { start: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectUriError {
    UnsupportedScheme,
    InvalidEscape,
    CurrentDirectory(io::ErrorKind),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectManifestError {
    Read { path: PathBuf, kind: io::ErrorKind },
    MissingToml,
    ParseRejected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectSourceWalkError {
    ReadDirectory { path: PathBuf, kind: io::ErrorKind },
    ReadEntry { path: PathBuf, kind: io::ErrorKind },
    InvalidRoot(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectCompositionError {
    MissingModule,
    MissingVector,
    AllocationRejected,
    Rejected,
    NonString,
    PartDeclaration(PathBuf),
    TooLarge { limit: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectCompositionPlanError {
    InvalidEntry,
    InvalidPart(String),
    DuplicatePart(String),
    EntryListedAsPart,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectObservationError {
    DuplicateModuleProvider {
        name: String,
        first: PathBuf,
        second: PathBuf,
    },
    DuplicateModulePart(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectObservationBuildError {
    Discovery(ProjectDiscoveryError),
    Manifest(ProjectManifestError),
    Decode(ProjectManifestDecodeError),
    Dependency(ProjectDependencyResolutionError),
    SourceWalk(ProjectSourceWalkError),
    Source(ProjectSourceIndexError),
    Observation(ProjectObservationError),
    MissingMain(PathBuf),
    EmptyClosure,
    DependencyCycle(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectCheckError {
    Observation(ProjectObservationBuildError),
    Snapshot(ProjectSnapshotError),
    Policy(ProjectPolicyError),
    Resolution(ProjectManifestDecodeError),
    ManifestRead { path: PathBuf, kind: io::ErrorKind },
    TempDirectory { kind: io::ErrorKind },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectArtifactPlanError {
    Snapshot(ProjectSnapshotError),
    Policy(ProjectPolicyError),
    Decode(ProjectManifestDecodeError),
    Validation(ProjectArtifactPlanValidationError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectBuildRecordLoadError {
    Manifest(ProjectManifestError),
    Decode(ProjectManifestDecodeError),
    Observe(ProjectBuildRecordObservationError),
    InvalidPath(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectBuildRecordObservationError {
    InvalidPath(PathBuf),
    Read { path: PathBuf, kind: io::ErrorKind },
}

struct CheckTempDirectory(PathBuf);

impl Drop for CheckTempDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn create_check_temp_directory() -> Result<CheckTempDirectory, io::ErrorKind> {
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

/// Parse the first line of Codegen's frozen `line:column: message` diagnostic
/// text. Non-conforming diagnostics preserve their complete first line and do
/// not invent source positions.
pub fn parse_project_compile_diagnostic(
    module: &str,
    path: &str,
    detail: &str,
) -> ProjectCompileDiagnostic {
    let first_line = detail.split('\n').next().unwrap_or(detail);
    let mut fields = first_line.splitn(3, ':');
    let line = fields
        .next()
        .and_then(|value| value.trim().parse::<usize>().ok());
    let column = fields
        .next()
        .and_then(|value| value.trim().parse::<usize>().ok());
    let message = if line.is_some() && column.is_some() {
        fields.next().unwrap_or_default().trim().to_owned()
    } else {
        first_line.to_owned()
    };
    ProjectCompileDiagnostic {
        module: module.to_owned(),
        path: path.to_owned(),
        detail: detail.to_owned(),
        message,
        line,
        column,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectDependencyClosureError {
    DuplicateRoot(PathBuf),
    RootMissing(PathBuf),
    Missing {
        from: PathBuf,
        name: String,
        root: PathBuf,
    },
    Cycle(PathBuf),
}

/// Validate one manifest module composition and return its project-rooted path
/// order. This consumes already-parsed manifest values; it does not parse TOML
/// or inspect source contents.
pub fn project_composition_paths(
    project_root: &Path,
    entry: &str,
    parts: &[String],
) -> Result<Vec<PathBuf>, ProjectCompositionPlanError> {
    let entry_path = Path::new(entry);
    if entry.is_empty()
        || entry_path.is_absolute()
        || entry_path.extension() != Some(OsStr::new("birnam"))
    {
        return Err(ProjectCompositionPlanError::InvalidEntry);
    }
    let mut seen = HashSet::with_capacity(parts.len() + 1);
    seen.insert(entry.to_owned());
    let mut paths = Vec::with_capacity(parts.len() + 1);
    paths.push(project_root.join(entry_path));
    for part in parts {
        let path = Path::new(part);
        if part.is_empty() || path.is_absolute() || path.extension() != Some(OsStr::new("birnam")) {
            return Err(ProjectCompositionPlanError::InvalidPart(part.clone()));
        }
        if !seen.insert(part.clone()) {
            if part == entry {
                return Err(ProjectCompositionPlanError::EntryListedAsPart);
            }
            return Err(ProjectCompositionPlanError::DuplicatePart(part.clone()));
        }
        paths.push(project_root.join(path));
    }
    Ok(paths)
}

/// Enforce the project observation's one-provider and one-part ownership laws
/// before a snapshot reaches Birnam policy.
pub fn validate_project_observations(
    modules: &[ProjectSourceObservation],
) -> Result<(), ProjectObservationError> {
    let mut providers = HashSet::with_capacity(modules.len());
    let mut parts = HashSet::new();
    for module in modules {
        if !providers.insert(module.name.clone()) {
            let first = modules
                .iter()
                .find(|candidate| candidate.name == module.name && candidate.path != module.path)
                .map(|candidate| PathBuf::from(&candidate.path))
                .unwrap_or_else(|| PathBuf::from(&module.path));
            return Err(ProjectObservationError::DuplicateModuleProvider {
                name: module.name.clone(),
                first,
                second: PathBuf::from(&module.path),
            });
        }
        for path in &module.sources {
            if !parts.insert(path.clone()) {
                return Err(ProjectObservationError::DuplicateModulePart(PathBuf::from(
                    path,
                )));
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectPreparationError {
    Discovery(ProjectDiscoveryError),
    Manifest(ProjectManifestError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectSourceIndexError {
    Read { path: PathBuf, kind: io::ErrorKind },
    TooLarge { path: PathBuf, limit: usize },
    MissingCodegen,
    MetadataRejected,
    MetadataMalformed(ProjectMetadataError),
    CompositionPlan(ProjectCompositionPlanError),
    Composition(ProjectCompositionError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectSourceIndexOutcome {
    Metadata(birnam_abi::Oop),
    Ignored,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSourceMetadata {
    pub name: String,
    pub requires: Vec<String>,
    pub source_digest: String,
    pub revision_digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectMetadataError {
    NotVector,
    WrongSize,
    WrongTag,
    InvalidSymbol,
    InvalidString,
    InvalidRequires,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectManifestDecodeError {
    NotMap,
    MissingValue(&'static str),
    InvalidString(&'static str),
    InvalidList(&'static str),
    InvalidMap(&'static str),
    InvalidPath(&'static str),
    EmptyValue(&'static str),
    InvalidRunMode(String),
    InvalidPort,
    DuplicateWrapped(String),
    MissingWrappedOutput(String),
    InvalidTestSource(String),
    InvalidSchema(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectDependencyResolutionError {
    InvalidPath {
        name: String,
        path: PathBuf,
    },
    Canonicalize {
        name: String,
        path: PathBuf,
        kind: io::ErrorKind,
    },
}

/// Convert a file URI (or a plain path used by tests/tools) into a filesystem
/// path. This mirrors the Zig host's narrow project bridge boundary: only
/// `file://` URIs and plain paths are admitted, percent decoding happens here,
/// and other URI schemes fail closed before project observation.
pub fn project_path_from_uri(raw: &str) -> Result<PathBuf, ProjectUriError> {
    if raw.is_empty() {
        return std::env::current_dir()
            .map_err(|error| ProjectUriError::CurrentDirectory(error.kind()));
    }

    let (input, is_file_uri, add_leading_slash) = if let Some(input) = raw.strip_prefix("file://") {
        if let Some(input) = input.strip_prefix("localhost/") {
            (input, true, true)
        } else {
            (input, true, false)
        }
    } else if raw.contains("://") {
        return Err(ProjectUriError::UnsupportedScheme);
    } else {
        (raw, false, false)
    };

    let mut bytes = Vec::with_capacity(input.len() + usize::from(add_leading_slash));
    if add_leading_slash {
        bytes.push(b'/');
    }
    let input = input.as_bytes();
    let mut index = 0;
    while index < input.len() {
        if is_file_uri && input[index] == b'%' && index + 2 < input.len() {
            let Some(high) = hex_nibble(input[index + 1]) else {
                return Err(ProjectUriError::InvalidEscape);
            };
            let Some(low) = hex_nibble(input[index + 2]) else {
                return Err(ProjectUriError::InvalidEscape);
            };
            bytes.push((high << 4) | low);
            index += 3;
        } else {
            bytes.push(input[index]);
            index += 1;
        }
    }
    if bytes.is_empty() {
        return std::env::current_dir()
            .map_err(|error| ProjectUriError::CurrentDirectory(error.kind()));
    }

    #[cfg(unix)]
    {
        Ok(PathBuf::from(OsString::from_vec(bytes)))
    }
    #[cfg(not(unix))]
    {
        String::from_utf8(bytes)
            .map(PathBuf::from)
            .map_err(|_| ProjectUriError::InvalidEscape)
    }
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
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

/// Plan a root-first dependency closure from already-observed project nodes.
/// The function is deliberately filesystem-free: callers resolve and observe
/// manifests separately, then provide canonical project roots here. Edges are
/// visited by dependency name, matching Birnam's sorted manifest order.
pub fn project_dependency_closure_order(
    root: &Path,
    nodes: &[ProjectDependencyClosureNode],
) -> Result<Vec<PathBuf>, ProjectDependencyClosureError> {
    let mut index = HashMap::with_capacity(nodes.len());
    for (position, node) in nodes.iter().enumerate() {
        if index.insert(node.root.clone(), position).is_some() {
            return Err(ProjectDependencyClosureError::DuplicateRoot(
                node.root.clone(),
            ));
        }
    }
    let mut state = HashMap::with_capacity(nodes.len());
    let mut order = Vec::with_capacity(nodes.len());
    fn visit(
        root: &Path,
        nodes: &[ProjectDependencyClosureNode],
        index: &HashMap<PathBuf, usize>,
        state: &mut HashMap<PathBuf, bool>,
        order: &mut Vec<PathBuf>,
    ) -> Result<(), ProjectDependencyClosureError> {
        match state.get(root) {
            Some(true) => return Ok(()),
            Some(false) => return Err(ProjectDependencyClosureError::Cycle(root.to_path_buf())),
            None => {}
        }
        let Some(&position) = index.get(root) else {
            return Err(ProjectDependencyClosureError::RootMissing(
                root.to_path_buf(),
            ));
        };
        state.insert(root.to_path_buf(), false);
        order.push(root.to_path_buf());
        let mut dependencies = nodes[position].dependencies.clone();
        dependencies.sort_by(|left, right| left.name.cmp(&right.name));
        for dependency in dependencies {
            let dependency_root = PathBuf::from(&dependency.root);
            if !index.contains_key(&dependency_root) {
                return Err(ProjectDependencyClosureError::Missing {
                    from: root.to_path_buf(),
                    name: dependency.name,
                    root: dependency_root,
                });
            }
            visit(&dependency_root, nodes, index, state, order)?;
        }
        state.insert(root.to_path_buf(), true);
        Ok(())
    }
    visit(root, nodes, &index, &mut state, &mut order)?;
    Ok(order)
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

/// Decode the frozen Codegen `#moduleMetadata` vector without evaluating any
/// source. The vector shape is `[tag, name, requires, sourceDigest,
/// revisionDigest]`; symbols and strings are copied before the next send.
///
/// # Safety
///
/// `api` and `metadata` must belong to the same live runtime, and all Oops must
/// remain valid for the duration of this synchronous decode.
pub unsafe fn decode_project_source_metadata(
    api: &IdApi,
    metadata: birnam_abi::Oop,
) -> Result<ProjectSourceMetadata, ProjectMetadataError> {
    let vector_vt = unsafe { (api.import)(c"VectorVt".as_ptr()) };
    if metadata == api.nilObj
        || vector_vt == api.nilObj
        || unsafe { (api.vtableOf)(metadata) } != vector_vt
    {
        return Err(ProjectMetadataError::NotVector);
    }
    let size_selector = unsafe { (api.intern)(c"size".as_ptr()) };
    let size = unsafe { (api.send)(metadata, size_selector, std::ptr::null(), 0) };
    if unsafe { (api.isSmallInt)(size) } == 0 || unsafe { (api.smallIntValue)(size) } != 5 {
        return Err(ProjectMetadataError::WrongSize);
    }
    let at_selector = unsafe { (api.intern)(c"at:".as_ptr()) };
    let item = |index: i64| unsafe {
        (api.send)(metadata, at_selector, [(api.mkSmallInt)(index)].as_ptr(), 1)
    };
    let tag = item(0);
    if tag == api.nilObj
        || unsafe { (api.vtableOf)(tag) } != api.symbol_vt
        || unsafe { (api.symbolName)(tag) }.is_null()
        || unsafe { CStr::from_ptr((api.symbolName)(tag)) }.to_bytes() != b"moduleMetadata"
    {
        return Err(ProjectMetadataError::WrongTag);
    }
    let symbol = |value: birnam_abi::Oop| -> Result<String, ProjectMetadataError> {
        if value == api.nilObj || unsafe { (api.vtableOf)(value) } != api.symbol_vt {
            return Err(ProjectMetadataError::InvalidSymbol);
        }
        let name = unsafe { (api.symbolName)(value) };
        if name.is_null() {
            return Err(ProjectMetadataError::InvalidSymbol);
        }
        Ok(unsafe { CStr::from_ptr(name) }
            .to_string_lossy()
            .into_owned())
    };
    let string = |value: birnam_abi::Oop| -> Result<String, ProjectMetadataError> {
        if value == api.nilObj || unsafe { (api.vtableOf)(value) } != api.string_vt {
            return Err(ProjectMetadataError::InvalidString);
        }
        let mut length = 0usize;
        let bytes = unsafe { (api.stringBytes)(value, &mut length) };
        if bytes.is_null() {
            return Err(ProjectMetadataError::InvalidString);
        }
        Ok(
            String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(bytes, length) })
                .into_owned(),
        )
    };
    let requires_value = item(2);
    if requires_value == api.nilObj || unsafe { (api.vtableOf)(requires_value) } != vector_vt {
        return Err(ProjectMetadataError::InvalidRequires);
    }
    let requires_size = unsafe { (api.send)(requires_value, size_selector, std::ptr::null(), 0) };
    if unsafe { (api.isSmallInt)(requires_size) } == 0
        || unsafe { (api.smallIntValue)(requires_size) } < 0
    {
        return Err(ProjectMetadataError::InvalidRequires);
    }
    let mut requires = Vec::with_capacity(unsafe { (api.smallIntValue)(requires_size) as usize });
    for index in 0..unsafe { (api.smallIntValue)(requires_size) } {
        let value = unsafe {
            (api.send)(
                requires_value,
                at_selector,
                [(api.mkSmallInt)(index)].as_ptr(),
                1,
            )
        };
        requires.push(symbol(value).map_err(|_| ProjectMetadataError::InvalidRequires)?);
    }
    Ok(ProjectSourceMetadata {
        name: symbol(item(1))?,
        requires,
        source_digest: string(item(3))?,
        revision_digest: string(item(4))?,
    })
}

struct ProjectManifestDecoder<'a> {
    api: &'a IdApi,
    map_vt: birnam_abi::Oop,
    vector_vt: birnam_abi::Oop,
}

impl<'a> ProjectManifestDecoder<'a> {
    unsafe fn new(api: &'a IdApi) -> Result<Self, ProjectManifestDecodeError> {
        let map_vt = unsafe { (api.import)(c"MapVt".as_ptr()) };
        let vector_vt = unsafe { (api.import)(c"VectorVt".as_ptr()) };
        if map_vt == api.nilObj {
            return Err(ProjectManifestDecodeError::NotMap);
        }
        if vector_vt == api.nilObj {
            return Err(ProjectManifestDecodeError::InvalidList("manifest vector"));
        }
        Ok(Self {
            api,
            map_vt,
            vector_vt,
        })
    }

    unsafe fn at(
        &self,
        map: birnam_abi::Oop,
        key: &str,
    ) -> Result<birnam_abi::Oop, ProjectManifestDecodeError> {
        if map == self.api.nilObj || unsafe { (self.api.vtableOf)(map) } != self.map_vt {
            return Err(ProjectManifestDecodeError::InvalidMap("manifest map"));
        }
        let key_value = unsafe { (self.api.mkString)(key.as_bytes().as_ptr(), key.len()) };
        let selector = unsafe { (self.api.intern)(c"at:".as_ptr()) };
        Ok(unsafe { (self.api.send)(map, selector, [key_value].as_ptr(), 1) })
    }

    unsafe fn string(
        &self,
        value: birnam_abi::Oop,
        field: &'static str,
    ) -> Result<String, ProjectManifestDecodeError> {
        if value == self.api.nilObj || unsafe { (self.api.vtableOf)(value) } != self.api.string_vt {
            return Err(ProjectManifestDecodeError::InvalidString(field));
        }
        let mut length = 0usize;
        let bytes = unsafe { (self.api.stringBytes)(value, &mut length) };
        if bytes.is_null() {
            return Err(ProjectManifestDecodeError::InvalidString(field));
        }
        Ok(
            String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(bytes, length) })
                .into_owned(),
        )
    }

    unsafe fn bytes(
        &self,
        value: birnam_abi::Oop,
        field: &'static str,
    ) -> Result<Vec<u8>, ProjectManifestDecodeError> {
        if value == self.api.nilObj || unsafe { (self.api.vtableOf)(value) } != self.api.string_vt {
            return Err(ProjectManifestDecodeError::InvalidString(field));
        }
        let mut length = 0usize;
        let bytes = unsafe { (self.api.stringBytes)(value, &mut length) };
        if bytes.is_null() {
            return Err(ProjectManifestDecodeError::InvalidString(field));
        }
        Ok(unsafe { std::slice::from_raw_parts(bytes, length) }.to_vec())
    }

    unsafe fn strings(
        &self,
        value: birnam_abi::Oop,
        field: &'static str,
        allow_single: bool,
    ) -> Result<Vec<String>, ProjectManifestDecodeError> {
        if value == self.api.nilObj {
            return Ok(Vec::new());
        }
        if allow_single && unsafe { (self.api.vtableOf)(value) } == self.api.string_vt {
            return Ok(vec![unsafe { self.string(value, field)? }]);
        }
        if unsafe { (self.api.vtableOf)(value) } != self.vector_vt {
            return Err(ProjectManifestDecodeError::InvalidList(field));
        }
        let size_selector = unsafe { (self.api.intern)(c"size".as_ptr()) };
        let size = unsafe { (self.api.send)(value, size_selector, std::ptr::null(), 0) };
        if unsafe { (self.api.isSmallInt)(size) } == 0
            || unsafe { (self.api.smallIntValue)(size) } < 0
        {
            return Err(ProjectManifestDecodeError::InvalidList(field));
        }
        let at_selector = unsafe { (self.api.intern)(c"at:".as_ptr()) };
        let mut result = Vec::with_capacity(unsafe { (self.api.smallIntValue)(size) as usize });
        for index in 0..unsafe { (self.api.smallIntValue)(size) } {
            let item = unsafe {
                (self.api.send)(
                    value,
                    at_selector,
                    [(self.api.mkSmallInt)(index)].as_ptr(),
                    1,
                )
            };
            result.push(unsafe { self.string(item, field)? });
        }
        Ok(result)
    }

    unsafe fn run(
        &self,
        value: birnam_abi::Oop,
    ) -> Result<ProjectRunObservation, ProjectManifestDecodeError> {
        if value == self.api.nilObj {
            return Ok(ProjectRunObservation {
                repl: "none".to_owned(),
                socket_bind: "127.0.0.1".to_owned(),
                socket_port: 4400,
            });
        }
        if unsafe { (self.api.vtableOf)(value) } != self.map_vt {
            return Err(ProjectManifestDecodeError::InvalidMap("run"));
        }
        let repl_value = unsafe { self.at(value, "repl")? };
        let repl = if repl_value == self.api.nilObj {
            "none".to_owned()
        } else {
            unsafe { self.string(repl_value, "run.repl")? }
        };
        if !matches!(repl.as_str(), "none" | "tty" | "socket") {
            return Err(ProjectManifestDecodeError::InvalidRunMode(repl));
        }
        let bind_value = unsafe { self.at(value, "socket_bind")? };
        let socket_bind = if bind_value == self.api.nilObj {
            "127.0.0.1".to_owned()
        } else {
            let bind = unsafe { self.string(bind_value, "run.socket_bind")? };
            if bind.is_empty() {
                return Err(ProjectManifestDecodeError::EmptyValue("run.socket_bind"));
            }
            bind
        };
        let port_value = unsafe { self.at(value, "socket_port")? };
        let socket_port = if port_value == self.api.nilObj {
            4400
        } else if unsafe { (self.api.isSmallInt)(port_value) } == 0 {
            return Err(ProjectManifestDecodeError::InvalidPort);
        } else {
            let port = unsafe { (self.api.smallIntValue)(port_value) };
            if !(0..=65535).contains(&port) {
                return Err(ProjectManifestDecodeError::InvalidPort);
            }
            port
        };
        Ok(ProjectRunObservation {
            repl,
            socket_bind,
            socket_port,
        })
    }

    unsafe fn modules(
        &self,
        value: birnam_abi::Oop,
    ) -> Result<Vec<ProjectModulePlan>, ProjectManifestDecodeError> {
        if value == self.api.nilObj {
            return Ok(Vec::new());
        }
        if unsafe { (self.api.vtableOf)(value) } != self.vector_vt {
            return Err(ProjectManifestDecodeError::InvalidList("module"));
        }
        let size_selector = unsafe { (self.api.intern)(c"size".as_ptr()) };
        let at_selector = unsafe { (self.api.intern)(c"at:".as_ptr()) };
        let size = unsafe { (self.api.send)(value, size_selector, std::ptr::null(), 0) };
        if unsafe { (self.api.isSmallInt)(size) } == 0
            || unsafe { (self.api.smallIntValue)(size) } < 0
        {
            return Err(ProjectManifestDecodeError::InvalidList("module"));
        }
        let mut result = Vec::with_capacity(unsafe { (self.api.smallIntValue)(size) as usize });
        for index in 0..unsafe { (self.api.smallIntValue)(size) } {
            let item = unsafe {
                (self.api.send)(
                    value,
                    at_selector,
                    [(self.api.mkSmallInt)(index)].as_ptr(),
                    1,
                )
            };
            if item == self.api.nilObj || unsafe { (self.api.vtableOf)(item) } != self.map_vt {
                return Err(ProjectManifestDecodeError::InvalidMap("module entry"));
            }
            let entry = unsafe { self.string(self.at(item, "entry")?, "module.entry")? };
            if entry.is_empty()
                || Path::new(&entry).is_absolute()
                || Path::new(&entry).extension() != Some(OsStr::new("birnam"))
            {
                return Err(ProjectManifestDecodeError::InvalidPath("module.entry"));
            }
            let files = unsafe { self.strings(self.at(item, "files")?, "module.files", false)? };
            let _ = project_composition_paths(Path::new("."), &entry, &files)
                .map_err(|_| ProjectManifestDecodeError::InvalidPath("module.files"))?;
            result.push(ProjectModulePlan { entry, files });
        }
        Ok(result)
    }

    unsafe fn wrapped(
        &self,
        value: birnam_abi::Oop,
    ) -> Result<Vec<ProjectWrappedObservation>, ProjectManifestDecodeError> {
        if value == self.api.nilObj {
            return Ok(Vec::new());
        }
        if unsafe { (self.api.vtableOf)(value) } != self.vector_vt {
            return Err(ProjectManifestDecodeError::InvalidList("wrapped"));
        }
        let size_selector = unsafe { (self.api.intern)(c"size".as_ptr()) };
        let at_selector = unsafe { (self.api.intern)(c"at:".as_ptr()) };
        let size = unsafe { (self.api.send)(value, size_selector, std::ptr::null(), 0) };
        if unsafe { (self.api.isSmallInt)(size) } == 0
            || unsafe { (self.api.smallIntValue)(size) } < 0
        {
            return Err(ProjectManifestDecodeError::InvalidList("wrapped"));
        }
        let mut result = Vec::with_capacity(unsafe { (self.api.smallIntValue)(size) as usize });
        for index in 0..unsafe { (self.api.smallIntValue)(size) } {
            let item = unsafe {
                (self.api.send)(
                    value,
                    at_selector,
                    [(self.api.mkSmallInt)(index)].as_ptr(),
                    1,
                )
            };
            if item == self.api.nilObj || unsafe { (self.api.vtableOf)(item) } != self.map_vt {
                return Err(ProjectManifestDecodeError::InvalidMap("wrapped entry"));
            }
            let name = unsafe { self.string(self.at(item, "name")?, "wrapped.name")? };
            if name.is_empty() || name.contains('/') || Path::new(&name).is_absolute() {
                return Err(ProjectManifestDecodeError::InvalidPath("wrapped.name"));
            }
            if result
                .iter()
                .any(|entry: &ProjectWrappedObservation| entry.name == name)
            {
                return Err(ProjectManifestDecodeError::DuplicateWrapped(name));
            }
            let build_value = unsafe { self.at(item, "build")? };
            if build_value == self.api.nilObj {
                return Err(ProjectManifestDecodeError::MissingValue("wrapped.build"));
            }
            if unsafe { (self.api.vtableOf)(build_value) } != self.vector_vt {
                return Err(ProjectManifestDecodeError::InvalidList("wrapped.build"));
            }
            let command_count =
                unsafe { (self.api.send)(build_value, size_selector, std::ptr::null(), 0) };
            if unsafe { (self.api.isSmallInt)(command_count) } == 0
                || unsafe { (self.api.smallIntValue)(command_count) } <= 0
            {
                return Err(ProjectManifestDecodeError::EmptyValue("wrapped.build"));
            }
            let mut build =
                Vec::with_capacity(unsafe { (self.api.smallIntValue)(command_count) as usize });
            let mut has_output = false;
            for command_index in 0..unsafe { (self.api.smallIntValue)(command_count) } {
                let command = unsafe {
                    (self.api.send)(
                        build_value,
                        at_selector,
                        [(self.api.mkSmallInt)(command_index)].as_ptr(),
                        1,
                    )
                };
                let argv = unsafe { self.strings(command, "wrapped.build command", false)? };
                if argv.is_empty() {
                    return Err(ProjectManifestDecodeError::EmptyValue(
                        "wrapped.build command",
                    ));
                }
                has_output |= argv.iter().any(|argument| argument.contains("{output}"));
                build.push(argv);
            }
            if !has_output {
                return Err(ProjectManifestDecodeError::MissingWrappedOutput(name));
            }
            result.push(ProjectWrappedObservation { name, build });
        }
        Ok(result)
    }

    unsafe fn test_sources(
        &self,
        value: birnam_abi::Oop,
    ) -> Result<Option<Vec<String>>, ProjectManifestDecodeError> {
        if value == self.api.nilObj {
            return Ok(None);
        }
        if unsafe { (self.api.vtableOf)(value) } != self.map_vt {
            return Err(ProjectManifestDecodeError::InvalidMap("test"));
        }
        let sources = unsafe { self.at(value, "sources")? };
        if sources == self.api.nilObj {
            return Err(ProjectManifestDecodeError::MissingValue("test.sources"));
        }
        let sources = unsafe { self.strings(sources, "test.sources", false)? };
        for source in &sources {
            let path = Path::new(source);
            if source.is_empty()
                || path.is_absolute()
                || path.components().any(|component| {
                    matches!(
                        component,
                        std::path::Component::CurDir | std::path::Component::ParentDir
                    )
                })
            {
                return Err(ProjectManifestDecodeError::InvalidTestSource(
                    source.clone(),
                ));
            }
        }
        Ok(Some(sources))
    }

    unsafe fn dependencies(
        &self,
        value: birnam_abi::Oop,
    ) -> Result<Vec<ProjectDependencyObservation>, ProjectManifestDecodeError> {
        if value == self.api.nilObj {
            return Ok(Vec::new());
        }
        if unsafe { (self.api.vtableOf)(value) } != self.map_vt {
            return Err(ProjectManifestDecodeError::InvalidMap("dependencies"));
        }
        let keys_selector = unsafe { (self.api.intern)(c"keys".as_ptr()) };
        let keys = unsafe { (self.api.send)(value, keys_selector, std::ptr::null(), 0) };
        let names = unsafe { self.strings(keys, "dependency names", false)? };
        let mut result = Vec::with_capacity(names.len());
        for name in names {
            let spec = unsafe { self.at(value, &name) }
                .map_err(|_| ProjectManifestDecodeError::InvalidMap("dependency spec"))?;
            let path = unsafe { self.string(self.at(spec, "path")?, "dependency.path")? };
            if Path::new(&path).is_absolute() {
                return Err(ProjectManifestDecodeError::InvalidPath("dependency.path"));
            }
            result.push(ProjectDependencyObservation { name, root: path });
        }
        result.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(result)
    }
}

/// Decode host-owned manifest fields from a parsed TOML map. TOML syntax and
/// semantic provider decisions remain in Birnam; this only validates the
/// adapter's required value shapes and defaults.
///
/// # Safety
///
/// `api` and `parsed` must belong to the same live runtime, and all synchronous
/// sends must occur on the floor owner thread with a live GC anchor.
pub unsafe fn decode_project_manifest(
    api: &IdApi,
    parsed: birnam_abi::Oop,
) -> Result<ProjectManifestObservation, ProjectManifestDecodeError> {
    let decoder = unsafe { ProjectManifestDecoder::new(api)? };
    if parsed == api.nilObj || unsafe { (api.vtableOf)(parsed) } != decoder.map_vt {
        return Err(ProjectManifestDecodeError::NotMap);
    }
    let name_value = unsafe { decoder.at(parsed, "name")? };
    let name = if name_value == api.nilObj {
        String::new()
    } else {
        unsafe { decoder.string(name_value, "name")? }
    };
    let main_value = unsafe { decoder.at(parsed, "main")? };
    let main = if main_value == api.nilObj {
        None
    } else {
        let main = unsafe { decoder.string(main_value, "main")? };
        if Path::new(&main).is_absolute() {
            return Err(ProjectManifestDecodeError::InvalidPath("main"));
        }
        Some(main)
    };
    let roots_value = unsafe { decoder.at(parsed, "source_roots")? };
    let source_roots = if roots_value == api.nilObj {
        vec!["src".to_owned()]
    } else {
        unsafe { decoder.strings(roots_value, "source_roots", false)? }
    };
    if source_roots.is_empty() {
        return Err(ProjectManifestDecodeError::EmptyValue("source_roots"));
    }
    if source_roots
        .iter()
        .any(|root| Path::new(root).is_absolute())
    {
        return Err(ProjectManifestDecodeError::InvalidPath("source_roots"));
    }
    let loads = unsafe { decoder.strings(decoder.at(parsed, "load")?, "load", true)? };
    let run = unsafe { decoder.run(decoder.at(parsed, "run")?)? };
    let modules = unsafe { decoder.modules(decoder.at(parsed, "module")?)? };
    let dependencies = unsafe { decoder.dependencies(decoder.at(parsed, "dependencies")?)? };
    let wrapped = unsafe { decoder.wrapped(decoder.at(parsed, "wrapped")?)? };
    let test_sources = unsafe { decoder.test_sources(decoder.at(parsed, "test")?)? };
    Ok(ProjectManifestObservation {
        name,
        main,
        source_roots,
        loads,
        modules,
        dependencies,
        run,
        wrapped,
        test_sources,
    })
}

/// Decode the pointer-free resolution map returned by Birnam's
/// `ProjectPolicy resolveSnapshot:` selector. The policy remains responsible
/// for provider selection and ordering; this copies only its selected module
/// records for the host's later compile/check boundary.
///
/// # Safety
///
/// `api` and `resolution` must belong to the same live runtime, and all Oops
/// must remain valid for the duration of this synchronous decode.
pub unsafe fn decode_project_resolution(
    api: &IdApi,
    resolution: birnam_abi::Oop,
) -> Result<ProjectResolutionObservation, ProjectManifestDecodeError> {
    let decoder = unsafe { ProjectManifestDecoder::new(api)? };
    if resolution == api.nilObj || unsafe { (api.vtableOf)(resolution) } != decoder.map_vt {
        return Err(ProjectManifestDecodeError::NotMap);
    }
    let project_root =
        unsafe { decoder.string(decoder.at(resolution, "projectRoot")?, "projectRoot")? };
    let manifest = unsafe { decoder.string(decoder.at(resolution, "manifest")?, "manifest")? };
    let project_name =
        unsafe { decoder.string(decoder.at(resolution, "projectName")?, "projectName")? };
    let main_value = unsafe { decoder.at(resolution, "main")? };
    let main = if main_value == api.nilObj {
        None
    } else {
        Some(unsafe { decoder.string(main_value, "main")? })
    };
    let load_roots =
        unsafe { decoder.strings(decoder.at(resolution, "loadRoots")?, "loadRoots", false)? };
    let modules_value = unsafe { decoder.at(resolution, "modules")? };
    if modules_value == api.nilObj || unsafe { (api.vtableOf)(modules_value) } != decoder.vector_vt
    {
        return Err(ProjectManifestDecodeError::InvalidList("modules"));
    }
    let size_selector = unsafe { (api.intern)(c"size".as_ptr()) };
    let at_selector = unsafe { (api.intern)(c"at:".as_ptr()) };
    let size = unsafe { (api.send)(modules_value, size_selector, std::ptr::null(), 0) };
    if unsafe { (api.isSmallInt)(size) } == 0 || unsafe { (api.smallIntValue)(size) } < 0 {
        return Err(ProjectManifestDecodeError::InvalidList("modules"));
    }
    let mut modules = Vec::with_capacity(unsafe { (api.smallIntValue)(size) as usize });
    for index in 0..unsafe { (api.smallIntValue)(size) } {
        let item = unsafe {
            (api.send)(
                modules_value,
                at_selector,
                [(api.mkSmallInt)(index)].as_ptr(),
                1,
            )
        };
        if item == api.nilObj || unsafe { (api.vtableOf)(item) } != decoder.map_vt {
            return Err(ProjectManifestDecodeError::InvalidMap("module"));
        }
        let name = unsafe { decoder.string(decoder.at(item, "name")?, "module.name")? };
        let path = unsafe { decoder.string(decoder.at(item, "path")?, "module.path")? };
        let sources =
            unsafe { decoder.strings(decoder.at(item, "sources")?, "module.sources", false)? };
        let project_root =
            unsafe { decoder.string(decoder.at(item, "projectRoot")?, "module.projectRoot")? };
        let source = unsafe { decoder.bytes(decoder.at(item, "source")?, "module.source")? };
        let requires =
            unsafe { decoder.strings(decoder.at(item, "requires")?, "module.requires", false)? };
        let source_digest =
            unsafe { decoder.string(decoder.at(item, "sourceDigest")?, "module.sourceDigest")? };
        let revision_digest = unsafe {
            decoder.string(decoder.at(item, "revisionDigest")?, "module.revisionDigest")?
        };
        modules.push(ProjectSourceObservation {
            name,
            path,
            sources,
            project_root,
            source,
            requires,
            source_digest,
            revision_digest,
        });
    }
    Ok(ProjectResolutionObservation {
        project_root,
        manifest,
        project_name,
        main,
        load_roots,
        modules,
    })
}

/// Reproduce the frozen `project-resolution.v1` digest over the host's
/// pointer-free resolution records. This is an observation checksum only; it
/// does not influence Birnam policy or provider selection.
pub fn project_resolution_digest(
    project_root: &str,
    manifest_bytes: &[u8],
    modules: &[ProjectSourceObservation],
) -> String {
    let manifest_digest = String::from_utf8_lossy(&sha256_hex(manifest_bytes)).into_owned();
    let mut input = Vec::new();
    let add = |input: &mut Vec<u8>, value: &str| {
        input.extend_from_slice(&(value.len() as u64).to_be_bytes());
        input.extend_from_slice(value.as_bytes());
    };
    add(&mut input, "birnam.project-resolution.v1");
    add(&mut input, project_root);
    add(&mut input, &manifest_digest);
    for module in modules {
        add(&mut input, &module.name);
        add(&mut input, &module.path);
        for source in &module.sources {
            add(&mut input, source);
        }
        add(&mut input, &module.source_digest);
        for dependency in &module.requires {
            add(&mut input, dependency);
        }
    }
    String::from_utf8_lossy(&sha256_hex(&input)).into_owned()
}

/// Decode Birnam's side-effect-free `ProjectPolicy artifactPlan:` result.
/// Artifact actions and reuse checks remain policy-owned; Rust only copies the
/// typed plan before compilation/publication adapters consume it.
///
/// # Safety
///
/// `api` and `plan` must belong to the same live runtime, and all Oops must
/// remain valid for the duration of this synchronous decode.
pub unsafe fn decode_project_artifact_plan(
    api: &IdApi,
    plan: birnam_abi::Oop,
) -> Result<ProjectArtifactPlan, ProjectManifestDecodeError> {
    let decoder = unsafe { ProjectManifestDecoder::new(api)? };
    if plan == api.nilObj || unsafe { (api.vtableOf)(plan) } != decoder.map_vt {
        return Err(ProjectManifestDecodeError::NotMap);
    }
    let record = unsafe { decoder.at(plan, "record")? };
    if record == api.nilObj || unsafe { (api.vtableOf)(record) } != decoder.map_vt {
        return Err(ProjectManifestDecodeError::InvalidMap("artifact record"));
    }
    let schema = unsafe { decoder.string(decoder.at(record, "schema")?, "schema")? };
    if schema != "birnam-artifacts-v3" {
        return Err(ProjectManifestDecodeError::InvalidSchema(schema));
    }
    let project_root =
        unsafe { decoder.string(decoder.at(record, "projectRoot")?, "projectRoot")? };
    let target = unsafe { decoder.string(decoder.at(record, "target")?, "target")? };
    let profile = unsafe { decoder.string(decoder.at(record, "profile")?, "profile")? };
    let tool_version =
        unsafe { decoder.string(decoder.at(record, "toolVersion")?, "toolVersion")? };
    let artifact_abi =
        unsafe { decoder.string(decoder.at(record, "artifactAbi")?, "artifactAbi")? };
    if artifact_abi.as_bytes() != birnam_artifact::ABI {
        return Err(ProjectManifestDecodeError::InvalidSchema(format!(
            "unsupported artifact ABI {artifact_abi}"
        )));
    }
    let built_at_value = unsafe { decoder.at(record, "builtAt")? };
    let built_at = if built_at_value == api.nilObj {
        None
    } else {
        Some(unsafe { decoder.string(built_at_value, "builtAt")? })
    };
    let resolution_digest =
        unsafe { decoder.string(decoder.at(record, "resolutionDigest")?, "resolutionDigest")? };
    let publication = unsafe { decoder.string(decoder.at(plan, "publication")?, "publication")? };
    let artifacts_value = unsafe { decoder.at(plan, "artifacts")? };
    if artifacts_value == api.nilObj
        || unsafe { (api.vtableOf)(artifacts_value) } != decoder.vector_vt
    {
        return Err(ProjectManifestDecodeError::InvalidList("artifacts"));
    }
    let size_selector = unsafe { (api.intern)(c"size".as_ptr()) };
    let at_selector = unsafe { (api.intern)(c"at:".as_ptr()) };
    let size = unsafe { (api.send)(artifacts_value, size_selector, std::ptr::null(), 0) };
    if unsafe { (api.isSmallInt)(size) } == 0 || unsafe { (api.smallIntValue)(size) } < 0 {
        return Err(ProjectManifestDecodeError::InvalidList("artifacts"));
    }
    let mut artifacts = Vec::with_capacity(unsafe { (api.smallIntValue)(size) as usize });
    for index in 0..unsafe { (api.smallIntValue)(size) } {
        let item = unsafe {
            (api.send)(
                artifacts_value,
                at_selector,
                [(api.mkSmallInt)(index)].as_ptr(),
                1,
            )
        };
        if item == api.nilObj || unsafe { (api.vtableOf)(item) } != decoder.map_vt {
            return Err(ProjectManifestDecodeError::InvalidMap("artifact"));
        }
        let name = unsafe { decoder.string(decoder.at(item, "name")?, "artifact.name")? };
        let path = unsafe { decoder.string(decoder.at(item, "path")?, "artifact.path")? };
        let action = unsafe { decoder.string(decoder.at(item, "action")?, "artifact.action")? };
        let source = unsafe { decoder.string(decoder.at(item, "source")?, "artifact.source")? };
        let sources =
            unsafe { decoder.strings(decoder.at(item, "sources")?, "artifact.sources", false)? };
        let source_digest =
            unsafe { decoder.string(decoder.at(item, "sourceDigest")?, "artifact.sourceDigest")? };
        let revision_digest = unsafe {
            decoder.string(
                decoder.at(item, "revisionDigest")?,
                "artifact.revisionDigest",
            )?
        };
        let requires =
            unsafe { decoder.strings(decoder.at(item, "requires")?, "artifact.requires", false)? };
        let checksum_value = unsafe { decoder.at(item, "checksum")? };
        let checksum = if checksum_value == api.nilObj {
            None
        } else {
            Some(unsafe { decoder.string(checksum_value, "artifact.checksum")? })
        };
        artifacts.push(ProjectArtifactPlanEntry {
            name,
            path,
            action,
            source,
            sources,
            source_digest,
            revision_digest,
            requires,
            checksum,
        });
    }
    Ok(ProjectArtifactPlan {
        project_root,
        target,
        profile,
        tool_version,
        artifact_abi,
        built_at,
        resolution_digest,
        artifacts,
        publication,
    })
}

/// Validate a decoded artifact plan against the host-observed resolution and
/// the request identity. This is a fail-closed adapter check: Birnam policy
/// chooses compile versus reuse, while Rust rejects plans whose metadata or
/// paths could make a later effectful stage address a different artifact.
pub fn validate_project_artifact_plan(
    plan: &ProjectArtifactPlan,
    request: &ProjectArtifactPlanRequest<'_>,
) -> Result<(), ProjectArtifactPlanValidationError> {
    if plan.project_root != request.resolution.project_root {
        return Err(ProjectArtifactPlanValidationError::MetadataMismatch(
            "projectRoot",
        ));
    }
    if plan.target != request.target {
        return Err(ProjectArtifactPlanValidationError::MetadataMismatch(
            "target",
        ));
    }
    if plan.profile != request.profile {
        return Err(ProjectArtifactPlanValidationError::MetadataMismatch(
            "profile",
        ));
    }
    if plan.tool_version != request.tool_version {
        return Err(ProjectArtifactPlanValidationError::MetadataMismatch(
            "toolVersion",
        ));
    }
    if plan.artifact_abi.as_bytes() != birnam_artifact::ABI {
        return Err(ProjectArtifactPlanValidationError::MetadataMismatch(
            "artifactAbi",
        ));
    }
    if plan.resolution_digest != request.resolution_digest {
        return Err(ProjectArtifactPlanValidationError::MetadataMismatch(
            "resolutionDigest",
        ));
    }
    if let Some(built_at) = &plan.built_at
        && built_at != request.built_at
    {
        return Err(ProjectArtifactPlanValidationError::MetadataMismatch(
            "builtAt",
        ));
    }
    if plan.publication != "publish" {
        return Err(ProjectArtifactPlanValidationError::InvalidPublication(
            plan.publication.clone(),
        ));
    }
    if plan.artifacts.len() != request.resolution.modules.len() {
        return Err(ProjectArtifactPlanValidationError::ArtifactCount {
            expected: request.resolution.modules.len(),
            actual: plan.artifacts.len(),
        });
    }
    for (index, (artifact, module)) in plan
        .artifacts
        .iter()
        .zip(&request.resolution.modules)
        .enumerate()
    {
        let expected_path = format!("lib/libbirnam_{}.dylib", module.name);
        if artifact.name != module.name {
            return Err(ProjectArtifactPlanValidationError::ArtifactMismatch {
                index,
                field: "name",
            });
        }
        if artifact.path != expected_path {
            return Err(ProjectArtifactPlanValidationError::ArtifactMismatch {
                index,
                field: "path",
            });
        }
        if artifact.source != module.path {
            return Err(ProjectArtifactPlanValidationError::ArtifactMismatch {
                index,
                field: "source",
            });
        }
        if artifact.sources != module.sources {
            return Err(ProjectArtifactPlanValidationError::ArtifactMismatch {
                index,
                field: "sources",
            });
        }
        if artifact.source_digest != module.source_digest {
            return Err(ProjectArtifactPlanValidationError::ArtifactMismatch {
                index,
                field: "sourceDigest",
            });
        }
        if artifact.revision_digest != module.revision_digest {
            return Err(ProjectArtifactPlanValidationError::ArtifactMismatch {
                index,
                field: "revisionDigest",
            });
        }
        if artifact.requires != module.requires {
            return Err(ProjectArtifactPlanValidationError::ArtifactMismatch {
                index,
                field: "requires",
            });
        }
        match artifact.action.as_str() {
            "compile" if artifact.checksum.is_none() => {}
            "reuse" if artifact.checksum.is_some() => {}
            "compile" | "reuse" => {
                return Err(ProjectArtifactPlanValidationError::ChecksumMismatch {
                    index,
                    action: artifact.action.clone(),
                });
            }
            action => {
                return Err(ProjectArtifactPlanValidationError::InvalidAction {
                    index,
                    action: action.to_owned(),
                });
            }
        }
    }
    Ok(())
}

/// Validate one Codegen-produced module artifact before it can be copied into
/// a published build. The descriptor is inert bytes at this boundary; no
/// dynamic library is loaded and no module is activated.
pub fn validate_project_compiled_artifact(
    module: &ProjectSourceObservation,
    bytes: &[u8],
) -> Result<(), ProjectCompiledArtifactValidationError> {
    let descriptor = birnam_artifact::inspect(bytes)
        .map_err(ProjectCompiledArtifactValidationError::Descriptor)?;
    if descriptor.kind != b"module" {
        return Err(ProjectCompiledArtifactValidationError::KindMismatch);
    }
    if descriptor.identity != module.name.as_bytes() {
        return Err(ProjectCompiledArtifactValidationError::IdentityMismatch {
            expected: module.name.clone(),
            actual: descriptor.identity.to_vec(),
        });
    }
    if descriptor.source_digest != module.source_digest.as_bytes() {
        return Err(ProjectCompiledArtifactValidationError::SourceDigestMismatch);
    }
    if descriptor.revision_digest != module.revision_digest.as_bytes() {
        return Err(ProjectCompiledArtifactValidationError::RevisionDigestMismatch);
    }
    for (index, (expected, actual)) in module
        .requires
        .iter()
        .zip(descriptor.requires.iter())
        .enumerate()
    {
        if actual != expected.as_bytes() {
            return Err(ProjectCompiledArtifactValidationError::DependencyMismatch {
                index,
                actual: actual.to_vec(),
            });
        }
    }
    if descriptor.requires.count() != module.requires.len() {
        return Err(ProjectCompiledArtifactValidationError::DependencyMismatch {
            index: module.requires.len(),
            actual: Vec::new(),
        });
    }
    Ok(())
}

fn previous_artifact<'a>(
    record: &'a ProjectBuildRecord,
    name: &str,
) -> Option<&'a ProjectPreviousArtifact> {
    record
        .artifacts
        .iter()
        .find(|artifact| artifact.name == name)
}

fn previous_native_artifact<'a>(
    record: &'a ProjectBuildRecord,
    name: &str,
) -> Option<&'a ProjectPreviousNativeArtifact> {
    record.native.iter().find(|artifact| artifact.name == name)
}

/// Expand the placeholder protocol used by wrapped build commands without
/// invoking a shell. Unknown braces are preserved as literal command text.
pub fn expand_project_wrapped_argument(
    input: &str,
    project_root: &Path,
    output: &Path,
    build_root: &Path,
    lib_root: &Path,
) -> String {
    let replacements = [
        ("{output}", output),
        ("{project}", project_root),
        ("{build}", build_root),
        ("{lib}", lib_root),
    ];
    let mut result = String::with_capacity(input.len());
    let mut rest = input;
    while !rest.is_empty() {
        let Some((index, token, path)) = replacements
            .iter()
            .filter_map(|(token, path)| rest.find(token).map(|index| (index, *token, *path)))
            .min_by_key(|(index, _, _)| *index)
        else {
            result.push_str(rest);
            break;
        };
        result.push_str(&rest[..index]);
        result.push_str(&path.to_string_lossy());
        rest = &rest[index + token.len()..];
    }
    result
}

/// Decode the persisted `birnam-artifacts-v3` record before it participates in
/// a later reuse decision. The file remains an untrusted input; this function
/// only copies and validates its structural metadata.
///
/// # Safety
///
/// `api` and `record` must belong to the same live runtime, and all Oops must
/// remain valid for the duration of this synchronous decode.
pub unsafe fn decode_project_build_record(
    api: &IdApi,
    record: birnam_abi::Oop,
) -> Result<ProjectBuildRecord, ProjectManifestDecodeError> {
    let decoder = unsafe { ProjectManifestDecoder::new(api)? };
    if record == api.nilObj || unsafe { (api.vtableOf)(record) } != decoder.map_vt {
        return Err(ProjectManifestDecodeError::NotMap);
    }
    let schema = unsafe { decoder.string(decoder.at(record, "schema")?, "schema")? };
    if schema != "birnam-artifacts-v3" {
        return Err(ProjectManifestDecodeError::InvalidSchema(schema));
    }
    let project_root =
        unsafe { decoder.string(decoder.at(record, "project_root")?, "project_root")? };
    let target = unsafe { decoder.string(decoder.at(record, "target")?, "target")? };
    let profile = unsafe { decoder.string(decoder.at(record, "profile")?, "profile")? };
    let tool_version =
        unsafe { decoder.string(decoder.at(record, "tool_version")?, "tool_version")? };
    let artifact_abi =
        unsafe { decoder.string(decoder.at(record, "artifact_abi")?, "artifact_abi")? };
    let resolution_digest = unsafe {
        decoder.string(
            decoder.at(record, "resolution_digest")?,
            "resolution_digest",
        )?
    };
    let built_at = unsafe { decoder.string(decoder.at(record, "built_at")?, "built_at")? };
    let artifacts_value = unsafe { decoder.at(record, "artifacts")? };
    let native_value = unsafe { decoder.at(record, "native")? };
    let decode_vector_size =
        |value: birnam_abi::Oop, field: &'static str| -> Result<i64, ProjectManifestDecodeError> {
            if value == api.nilObj || unsafe { (api.vtableOf)(value) } != decoder.vector_vt {
                return Err(ProjectManifestDecodeError::InvalidList(field));
            }
            let selector = unsafe { (api.intern)(c"size".as_ptr()) };
            let size = unsafe { (api.send)(value, selector, std::ptr::null(), 0) };
            if unsafe { (api.isSmallInt)(size) } == 0 || unsafe { (api.smallIntValue)(size) } < 0 {
                return Err(ProjectManifestDecodeError::InvalidList(field));
            }
            Ok(unsafe { (api.smallIntValue)(size) })
        };
    let at_selector = unsafe { (api.intern)(c"at:".as_ptr()) };
    let artifact_count = decode_vector_size(artifacts_value, "artifacts")?;
    let mut artifacts = Vec::with_capacity(artifact_count as usize);
    for index in 0..artifact_count {
        let item = unsafe {
            (api.send)(
                artifacts_value,
                at_selector,
                [(api.mkSmallInt)(index)].as_ptr(),
                1,
            )
        };
        if item == api.nilObj || unsafe { (api.vtableOf)(item) } != decoder.map_vt {
            return Err(ProjectManifestDecodeError::InvalidMap(
                "artifact record entry",
            ));
        }
        let sources_value = unsafe { decoder.at(item, "sources")? };
        let sources = if sources_value == api.nilObj {
            vec![unsafe { decoder.string(decoder.at(item, "source")?, "source")? }]
        } else {
            unsafe { decoder.strings(sources_value, "sources", false)? }
        };
        artifacts.push(ProjectPreviousArtifact {
            name: unsafe { decoder.string(decoder.at(item, "name")?, "name")? },
            path: unsafe { decoder.string(decoder.at(item, "path")?, "path")? },
            source: unsafe { decoder.string(decoder.at(item, "source")?, "source")? },
            sources,
            source_digest: unsafe {
                decoder.string(decoder.at(item, "source_digest")?, "source_digest")?
            },
            revision_digest: unsafe {
                decoder.string(decoder.at(item, "revision_digest")?, "revision_digest")?
            },
            requires: unsafe { decoder.strings(decoder.at(item, "requires")?, "requires", false)? },
            checksum: unsafe { decoder.string(decoder.at(item, "checksum")?, "checksum")? },
            observed_checksum: None,
        });
    }
    let native_count = decode_vector_size(native_value, "native")?;
    let mut native = Vec::with_capacity(native_count as usize);
    for index in 0..native_count {
        let item = unsafe {
            (api.send)(
                native_value,
                at_selector,
                [(api.mkSmallInt)(index)].as_ptr(),
                1,
            )
        };
        if item == api.nilObj || unsafe { (api.vtableOf)(item) } != decoder.map_vt {
            return Err(ProjectManifestDecodeError::InvalidMap(
                "native record entry",
            ));
        }
        native.push(ProjectPreviousNativeArtifact {
            name: unsafe { decoder.string(decoder.at(item, "name")?, "name")? },
            path: unsafe { decoder.string(decoder.at(item, "path")?, "path")? },
            checksum: unsafe { decoder.string(decoder.at(item, "checksum")?, "checksum")? },
            observed_checksum: None,
        });
    }
    Ok(ProjectBuildRecord {
        project_root,
        target,
        profile,
        tool_version,
        artifact_abi,
        resolution_digest,
        built_at,
        artifacts,
        native,
    })
}

/// Decode the camelCase record returned by Birnam
/// `ProjectPolicy artifactRecord:artifacts:native:` before it is serialized
/// to the persisted snake_case TOML form.
///
/// # Safety
///
/// `api` and `record` must belong to the same live runtime, and all Oops must
/// remain valid for the duration of this synchronous decode.
pub unsafe fn decode_project_artifact_record(
    api: &IdApi,
    record: birnam_abi::Oop,
) -> Result<ProjectBuildRecord, ProjectManifestDecodeError> {
    let decoder = unsafe { ProjectManifestDecoder::new(api)? };
    if record == api.nilObj || unsafe { (api.vtableOf)(record) } != decoder.map_vt {
        return Err(ProjectManifestDecodeError::NotMap);
    }
    let schema = unsafe { decoder.string(decoder.at(record, "schema")?, "schema")? };
    if schema != "birnam-artifacts-v3" {
        return Err(ProjectManifestDecodeError::InvalidSchema(schema));
    }
    let project_root =
        unsafe { decoder.string(decoder.at(record, "projectRoot")?, "projectRoot")? };
    let target = unsafe { decoder.string(decoder.at(record, "target")?, "target")? };
    let profile = unsafe { decoder.string(decoder.at(record, "profile")?, "profile")? };
    let tool_version =
        unsafe { decoder.string(decoder.at(record, "toolVersion")?, "toolVersion")? };
    let built_at = unsafe { decoder.string(decoder.at(record, "builtAt")?, "builtAt")? };
    let artifact_abi =
        unsafe { decoder.string(decoder.at(record, "artifactAbi")?, "artifactAbi")? };
    if artifact_abi.as_bytes() != birnam_artifact::ABI {
        return Err(ProjectManifestDecodeError::InvalidSchema(format!(
            "unsupported artifact ABI {artifact_abi}"
        )));
    }
    let resolution_digest =
        unsafe { decoder.string(decoder.at(record, "resolutionDigest")?, "resolutionDigest")? };
    let artifacts_value = unsafe { decoder.at(record, "artifacts")? };
    if artifacts_value == api.nilObj
        || unsafe { (api.vtableOf)(artifacts_value) } != decoder.vector_vt
    {
        return Err(ProjectManifestDecodeError::InvalidList("artifacts"));
    }
    let size_selector = unsafe { (api.intern)(c"size".as_ptr()) };
    let at_selector = unsafe { (api.intern)(c"at:".as_ptr()) };
    let size = unsafe { (api.send)(artifacts_value, size_selector, std::ptr::null(), 0) };
    if unsafe { (api.isSmallInt)(size) } == 0 || unsafe { (api.smallIntValue)(size) } < 0 {
        return Err(ProjectManifestDecodeError::InvalidList("artifacts"));
    }
    let mut artifacts = Vec::with_capacity(unsafe { (api.smallIntValue)(size) as usize });
    for index in 0..unsafe { (api.smallIntValue)(size) } {
        let item = unsafe {
            (api.send)(
                artifacts_value,
                at_selector,
                [(api.mkSmallInt)(index)].as_ptr(),
                1,
            )
        };
        if item == api.nilObj || unsafe { (api.vtableOf)(item) } != decoder.map_vt {
            return Err(ProjectManifestDecodeError::InvalidMap(
                "artifact record entry",
            ));
        }
        let source = unsafe { decoder.string(decoder.at(item, "source")?, "artifact.source")? };
        let sources_value = unsafe { decoder.at(item, "sources")? };
        let sources = if sources_value == api.nilObj {
            vec![source.clone()]
        } else {
            unsafe { decoder.strings(sources_value, "artifact.sources", false)? }
        };
        artifacts.push(ProjectPreviousArtifact {
            name: unsafe { decoder.string(decoder.at(item, "name")?, "artifact.name")? },
            path: unsafe { decoder.string(decoder.at(item, "path")?, "artifact.path")? },
            source,
            sources,
            source_digest: unsafe {
                decoder.string(decoder.at(item, "sourceDigest")?, "artifact.sourceDigest")?
            },
            revision_digest: unsafe {
                decoder.string(
                    decoder.at(item, "revisionDigest")?,
                    "artifact.revisionDigest",
                )?
            },
            requires: unsafe {
                decoder.strings(decoder.at(item, "requires")?, "artifact.requires", false)?
            },
            checksum: unsafe {
                decoder.string(decoder.at(item, "checksum")?, "artifact.checksum")?
            },
            observed_checksum: None,
        });
    }
    let native_value = unsafe { decoder.at(record, "native")? };
    if native_value == api.nilObj || unsafe { (api.vtableOf)(native_value) } != decoder.vector_vt {
        return Err(ProjectManifestDecodeError::InvalidList("native"));
    }
    let native_size = unsafe { (api.send)(native_value, size_selector, std::ptr::null(), 0) };
    if unsafe { (api.isSmallInt)(native_size) } == 0
        || unsafe { (api.smallIntValue)(native_size) } < 0
    {
        return Err(ProjectManifestDecodeError::InvalidList("native"));
    }
    let mut native = Vec::with_capacity(unsafe { (api.smallIntValue)(native_size) as usize });
    for index in 0..unsafe { (api.smallIntValue)(native_size) } {
        let item = unsafe {
            (api.send)(
                native_value,
                at_selector,
                [(api.mkSmallInt)(index)].as_ptr(),
                1,
            )
        };
        if item == api.nilObj || unsafe { (api.vtableOf)(item) } != decoder.map_vt {
            return Err(ProjectManifestDecodeError::InvalidMap(
                "native record entry",
            ));
        }
        native.push(ProjectPreviousNativeArtifact {
            name: unsafe { decoder.string(decoder.at(item, "name")?, "native.name")? },
            path: unsafe { decoder.string(decoder.at(item, "path")?, "native.path")? },
            checksum: unsafe { decoder.string(decoder.at(item, "checksum")?, "native.checksum")? },
            observed_checksum: None,
        });
    }
    Ok(ProjectBuildRecord {
        project_root,
        target,
        profile,
        tool_version,
        artifact_abi,
        resolution_digest,
        built_at,
        artifacts,
        native,
    })
}

fn append_toml_string(output: &mut String, key: &str, value: &str) {
    output.push_str(key);
    output.push_str(" = \"");
    append_toml_escaped(output, value);
    output.push_str("\"\n");
}

fn append_toml_escaped(output: &mut String, value: &str) {
    for byte in value.bytes() {
        match byte {
            b'\\' => output.push_str("\\\\"),
            b'"' => output.push_str("\\\""),
            b'\n' => output.push_str("\\n"),
            b'\r' => output.push_str("\\r"),
            b'\t' => output.push_str("\\t"),
            0x00..=0x1f => output.push_str(&format!("\\u{byte:04x}")),
            _ => output.push(byte as char),
        }
    }
}

fn append_toml_inline_string(output: &mut String, first: &mut bool, key: &str, value: &str) {
    if !*first {
        output.push_str(", ");
    }
    *first = false;
    output.push_str(key);
    output.push_str(" = \"");
    append_toml_escaped(output, value);
    output.push('"');
}

fn append_toml_inline_array(output: &mut String, first: &mut bool, key: &str, values: &[String]) {
    if !*first {
        output.push_str(", ");
    }
    *first = false;
    output.push_str(key);
    output.push_str(" = [");
    for (index, value) in values.iter().enumerate() {
        if index != 0 {
            output.push_str(", ");
        }
        output.push('"');
        append_toml_escaped(output, value);
        output.push('"');
    }
    output.push(']');
}

fn append_toml_artifact(output: &mut String, artifact: &ProjectPreviousArtifact) {
    output.push_str("{ ");
    let mut first = true;
    append_toml_inline_string(output, &mut first, "name", &artifact.name);
    append_toml_inline_string(output, &mut first, "path", &artifact.path);
    append_toml_inline_string(output, &mut first, "source", &artifact.source);
    append_toml_inline_array(output, &mut first, "sources", &artifact.sources);
    append_toml_inline_string(output, &mut first, "source_digest", &artifact.source_digest);
    append_toml_inline_string(
        output,
        &mut first,
        "revision_digest",
        &artifact.revision_digest,
    );
    append_toml_inline_array(output, &mut first, "requires", &artifact.requires);
    append_toml_inline_string(output, &mut first, "checksum", &artifact.checksum);
    output.push_str(" },\n");
}

fn append_toml_native(output: &mut String, artifact: &ProjectPreviousNativeArtifact) {
    output.push_str("{ ");
    let mut first = true;
    append_toml_inline_string(output, &mut first, "name", &artifact.name);
    append_toml_inline_string(output, &mut first, "path", &artifact.path);
    append_toml_inline_string(output, &mut first, "checksum", &artifact.checksum);
    output.push_str(" },\n");
}

/// Serialize the policy-owned artifact record in the persisted
/// `birnam-artifacts-v3` snake_case form. Transient observed checksums are
/// intentionally omitted; they are recomputed from bytes whenever the record
/// is loaded for a later policy decision.
pub fn encode_project_build_record(record: &ProjectBuildRecord) -> Vec<u8> {
    let mut output = String::new();
    append_toml_string(&mut output, "schema", "birnam-artifacts-v3");
    append_toml_string(&mut output, "project_root", &record.project_root);
    append_toml_string(&mut output, "target", &record.target);
    append_toml_string(&mut output, "profile", &record.profile);
    append_toml_string(&mut output, "tool_version", &record.tool_version);
    append_toml_string(&mut output, "built_at", &record.built_at);
    append_toml_string(&mut output, "artifact_abi", &record.artifact_abi);
    append_toml_string(&mut output, "resolution_digest", &record.resolution_digest);
    output.push_str("artifacts = [\n");
    for artifact in &record.artifacts {
        append_toml_artifact(&mut output, artifact);
    }
    output.push_str("]\n");
    output.push_str("native = [\n");
    for artifact in &record.native {
        append_toml_native(&mut output, artifact);
    }
    output.push_str("]\n");
    output.into_bytes()
}

/// Write one encoded build record to an explicit staging path and flush it to
/// stable storage before publication can observe it.
pub fn write_project_build_record(
    path: &Path,
    record: &ProjectBuildRecord,
) -> Result<(), ProjectBuildRecordWriteError> {
    let bytes = encode_project_build_record(record);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .map_err(|error| ProjectBuildRecordWriteError::Create {
            path: path.to_path_buf(),
            kind: error.kind(),
        })?;
    file.write_all(&bytes)
        .map_err(|error| ProjectBuildRecordWriteError::Write {
            path: path.to_path_buf(),
            kind: error.kind(),
        })?;
    file.sync_all()
        .map_err(|error| ProjectBuildRecordWriteError::Sync {
            path: path.to_path_buf(),
            kind: error.kind(),
        })
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

fn safe_record_relative_path(path: &str) -> Option<&Path> {
    let path = Path::new(path);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::CurDir | Component::ParentDir | Component::RootDir
            )
        })
    {
        return None;
    }
    Some(path)
}

/// Observe checksums for a decoded build record without trusting its stored
/// values. Only safe relative paths below `build_root` are read; absent files
/// remain observable as `None` for Birnam's later reuse policy.
pub fn observe_project_build_record(
    build_root: &Path,
    mut record: ProjectBuildRecord,
) -> Result<ProjectBuildRecord, ProjectBuildRecordObservationError> {
    for artifact in &mut record.artifacts {
        let Some(relative) = safe_record_relative_path(&artifact.path) else {
            return Err(ProjectBuildRecordObservationError::InvalidPath(
                PathBuf::from(&artifact.path),
            ));
        };
        let path = build_root.join(relative);
        match std::fs::read(&path) {
            Ok(bytes) => {
                artifact.observed_checksum =
                    Some(String::from_utf8_lossy(&sha256_hex(&bytes)).into_owned())
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                artifact.observed_checksum = None
            }
            Err(error) => {
                return Err(ProjectBuildRecordObservationError::Read {
                    path,
                    kind: error.kind(),
                });
            }
        }
    }
    for artifact in &mut record.native {
        let Some(relative) = safe_record_relative_path(&artifact.path) else {
            return Err(ProjectBuildRecordObservationError::InvalidPath(
                PathBuf::from(&artifact.path),
            ));
        };
        let path = build_root.join(relative);
        match std::fs::read(&path) {
            Ok(bytes) => {
                artifact.observed_checksum =
                    Some(String::from_utf8_lossy(&sha256_hex(&bytes)).into_owned())
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                artifact.observed_checksum = None
            }
            Err(error) => {
                return Err(ProjectBuildRecordObservationError::Read {
                    path,
                    kind: error.kind(),
                });
            }
        }
    }
    Ok(record)
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
pub enum WorkerStartupError {
    Image(ImageError),
    EmbeddedNul,
    MissingDispatchGeneration,
    InvalidDispatchGeneration,
    MissingCodegen,
    InvalidGenerationDigest,
    WrongArity,
    MissingGlobals,
    ReceiverNotFound,
    SelectorNotFound,
    Driver(WorkerError),
}

#[derive(Clone, PartialEq, Eq)]
pub enum RuntimeModuleError {
    EmbeddedNul,
    MissingModuleManager,
    LoadRejected { path: PathBuf },
    ArtifactUnavailable { path: PathBuf },
}

impl fmt::Debug for RuntimeModuleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmbeddedNul => formatter.write_str("EmbeddedNul"),
            Self::MissingModuleManager => formatter.write_str("MissingModuleManager"),
            Self::LoadRejected { path } => {
                write!(formatter, "LoadRejected {{ path: {:?} }}", path)
            }
            Self::ArtifactUnavailable { path } => {
                write!(formatter, "artifactUnavailable: {}", path.display())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    Module(RuntimeModuleError),
    Policy(ProjectPolicyError),
    Bridge(ProjectBridgeError),
    MissingCli,
    MissingVector,
    NonUtf8Argument(usize),
    ArgumentVectorRejected,
    MalformedStatus,
    Failed(i64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectBridgeError {
    AlreadyOwned,
    MissingObject,
    CloneRejected,
    GlobalRejected,
    InvalidArgument,
    Uri(ProjectUriError),
    Observation(String),
    Snapshot(ProjectSnapshotError),
    Policy(ProjectPolicyError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhaseHookError {
    EmbeddedNul,
    OpenFailed,
    MissingSymbol(&'static str),
    InvalidPhase,
}

type HostPhaseFn = unsafe extern "C" fn() -> c_int;

pub struct PhaseHooks {
    handle: NonNull<c_void>,
    enter_running: HostPhaseFn,
    begin_quiescence: HostPhaseFn,
    end_quiescence: HostPhaseFn,
}

impl PhaseHooks {
    fn enter_running(&self) -> bool {
        unsafe { (self.enter_running)() == 1 }
    }

    fn begin_quiescence(&self) -> bool {
        unsafe { (self.begin_quiescence)() == 1 }
    }

    fn end_quiescence(&self) -> bool {
        unsafe { (self.end_quiescence)() == 1 }
    }

    pub fn handle(&self) -> NonNull<c_void> {
        self.handle
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplError {
    Module(RuntimeModuleError),
    MissingRepl,
    MissingDispatchGeneration,
    MissingEvolutionIntent,
    InitializationRejected,
    GenerationRejected,
    Phase(PhaseHookError),
    ReadyRejected,
    MalformedStep,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SocketReplError {
    Module(RuntimeModuleError),
    MissingRepl,
    ServeRejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplStep {
    Done,
    Evaluated(birnam_abi::Oop),
    Other,
}

pub struct ReplSession {
    repl: birnam_abi::Oop,
    phases: PhaseHooks,
    done: bool,
    finished: bool,
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
    pub policy: StartupPolicy,
    pub codegen: birnam_abi::Oop,
    pub self_hosted: Vec<LoadedSelfHostedModule>,
}

static PROJECT_BRIDGE_RUNTIME: AtomicPtr<HostRuntime> = AtomicPtr::new(std::ptr::null_mut());

impl Drop for HostRuntime {
    fn drop(&mut self) {
        let _ = PROJECT_BRIDGE_RUNTIME.compare_exchange(
            self as *mut HostRuntime,
            std::ptr::null_mut(),
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }
}

unsafe fn active_project_bridge_runtime() -> Option<&'static HostRuntime> {
    let pointer = PROJECT_BRIDGE_RUNTIME.load(Ordering::Acquire);
    if pointer.is_null() {
        None
    } else {
        // SAFETY: the host installs this pointer only for its own live
        // HostRuntime and clears it from HostRuntime::drop.
        Some(unsafe { &*pointer })
    }
}

unsafe fn bridge_string(api: &IdApi, value: birnam_abi::Oop) -> Option<String> {
    if value == api.nilObj || unsafe { (api.vtableOf)(value) } != api.string_vt {
        return None;
    }
    let mut length = 0usize;
    let bytes = unsafe { (api.stringBytes)(value, &mut length) };
    if bytes.is_null() {
        return (length == 0).then(String::new);
    }
    Some(String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(bytes, length) }).into_owned())
}

unsafe fn project_bridge_failure(api: &IdApi, kind: &str, message: &str) -> birnam_abi::Oop {
    let Ok(builder) = (unsafe { ProjectSnapshotBuilder::new(api) }) else {
        return api.nilObj;
    };
    let Ok(mut result) = (unsafe { builder.new_map() }) else {
        return api.nilObj;
    };
    let Ok(updated) = (unsafe { builder.put(result, "ok", api.falseObj) }) else {
        return api.nilObj;
    };
    result = updated;
    let Ok(updated) =
        (unsafe { builder.put(result, "kind", (api.mkString)(kind.as_ptr(), kind.len())) })
    else {
        return api.nilObj;
    };
    result = updated;
    let Ok(updated) = (unsafe {
        builder.put(
            result,
            "message",
            (api.mkString)(message.as_ptr(), message.len()),
        )
    }) else {
        return api.nilObj;
    };
    updated
}

unsafe extern "C" fn project_path_for_uri_native(
    _: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    count: usize,
) -> birnam_abi::Oop {
    let Some(runtime) = (unsafe { active_project_bridge_runtime() }) else {
        return std::ptr::null_mut();
    };
    let api = runtime.stack.api();
    if count != 1 || args.is_null() {
        return api.nilObj;
    }
    let argument = unsafe { *args };
    let Some(uri) = (unsafe { bridge_string(api, argument) }) else {
        return api.nilObj;
    };
    let Ok(path) = project_path_from_uri(&uri) else {
        return unsafe {
            project_bridge_failure(api, "projectUriInvalid", "could not decode workspace URI")
        };
    };
    let bytes = path_bytes(&path);
    unsafe { (api.mkString)(bytes.as_ptr(), bytes.len()) }
}

unsafe extern "C" fn project_snapshot_for_uri_native(
    _: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    count: usize,
) -> birnam_abi::Oop {
    let Some(runtime) = (unsafe { active_project_bridge_runtime() }) else {
        return std::ptr::null_mut();
    };
    let api = runtime.stack.api();
    if count != 1 || args.is_null() {
        return api.nilObj;
    }
    let argument = unsafe { *args };
    let Some(uri) = (unsafe { bridge_string(api, argument) }) else {
        return api.nilObj;
    };
    let Ok(path) = project_path_from_uri(&uri) else {
        return unsafe {
            project_bridge_failure(api, "projectUriInvalid", "could not decode workspace URI")
        };
    };
    let start = if std::fs::metadata(&path)
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
    {
        path
    } else {
        path.parent().map(Path::to_path_buf).unwrap_or(path)
    };
    let snapshot = match unsafe { runtime.observe_project_snapshot(&start) } {
        Ok(snapshot) => snapshot,
        Err(error) => {
            let message = format!("{error:?}");
            return unsafe { project_bridge_failure(api, "projectResolutionFailed", &message) };
        }
    };
    let snapshot = match unsafe { runtime.materialize_project_snapshot(&snapshot) } {
        Ok(snapshot) => snapshot,
        Err(error) => {
            let message = format!("{error:?}");
            return unsafe { project_bridge_failure(api, "projectSnapshotFailed", &message) };
        }
    };
    match unsafe { runtime.resolve_project_snapshot(snapshot) } {
        Ok(resolution) => {
            // ProjectPolicy returns the resolution payload itself. The shared
            // LSP bridge contract adds an explicit success bit so Birnam can
            // distinguish a valid snapshot from its typed failure maps.
            let Ok(builder) = (unsafe { ProjectSnapshotBuilder::new(api) }) else {
                return api.nilObj;
            };
            match unsafe { builder.put(resolution, "ok", api.trueObj) } {
                Ok(resolution) => resolution,
                Err(_) => api.nilObj,
            }
        }
        Err(ProjectPolicyError::ResolutionDiagnostic { kind, message }) => unsafe {
            project_bridge_failure(api, &kind, &message)
        },
        Err(error) => {
            let message = format!("{error:?}");
            unsafe { project_bridge_failure(api, "projectResolutionFailed", &message) }
        }
    }
}

impl HostRuntime {
    /// Establish the GC execution boundary used by every host operation. The
    /// anchor must remain live until all synchronous sends/evaluations finish.
    ///
    /// # Safety
    ///
    /// The runtime must remain on the floor owner thread and `anchor` must stay
    /// in scope while the evaluation boundary is active.
    pub unsafe fn prepare_execution(&self, anchor: &mut usize) {
        let api = self.stack.api();
        unsafe { (api.gcSetStackBottom)(anchor as *mut usize as usize) };
        if self.policy.precise_gc {
            unsafe { (api.gcSetPrecise)(1) };
        }
    }

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
        let path = self
            .resources
            .lib_dir
            .join(format!("libbirnam_{}.dylib", name.to_string_lossy()));
        if std::fs::metadata(&path).is_err() {
            return Err(RuntimeModuleError::ArtifactUnavailable { path });
        }
        let api = self.stack.api();
        let selector = unsafe { (api.intern)(c"load:".as_ptr()) };
        let name = unsafe { (api.intern)(name.as_ptr()) };
        let result = unsafe { (api.send)(module_manager, selector, [name].as_ptr(), 1) };
        if result == api.nilObj || send_is_error(api, result) {
            return Err(RuntimeModuleError::LoadRejected { path });
        }
        Ok(())
    }

    /// Load a host-tool module outside the sealed application generation. This
    /// is the same capability boundary used by the Zig host for a process-local
    /// REPL fallback: the module may publish its tool globals, but it is not a
    /// durable application-generation introduction.
    ///
    /// # Safety
    ///
    /// The runtime must remain on the floor owner thread for the synchronous
    /// Codegen/Module sends.
    unsafe fn load_host_runtime_module(&self, name: &str) -> Result<(), RuntimeModuleError> {
        let name = CString::new(name).map_err(|_| RuntimeModuleError::EmbeddedNul)?;
        let filename = format!("libbirnam_{}.dylib", name.to_string_lossy());
        let path = self.resources.lib_dir.join(filename);
        if std::fs::metadata(&path).is_err() {
            return Err(RuntimeModuleError::ArtifactUnavailable { path });
        }
        let path = CString::new(path_bytes(&path)).map_err(|_| RuntimeModuleError::EmbeddedNul)?;
        let selector = unsafe { (self.stack.api().intern)(c"loadHostCapability:".as_ptr()) };
        let value =
            unsafe { (self.stack.api().mkString)(path.as_bytes().as_ptr(), path.as_bytes().len()) };
        let api = self.stack.api();
        let result = unsafe { (api.send)(self.codegen, selector, [value].as_ptr(), 1) };
        if result == self.stack.api().nilObj || send_is_error(self.stack.api(), result) {
            return Err(RuntimeModuleError::LoadRejected {
                path: PathBuf::from(path.to_string_lossy().trim_end_matches('\0')),
            });
        }
        if let Some(module_manager) = unsafe { global_named(self.stack.api(), "Module") } {
            let mark = unsafe { (self.stack.api().intern)(c"markLoaded:".as_ptr()) };
            let symbol = unsafe { (self.stack.api().intern)(name.as_ptr()) };
            // The canonical Zig host treats this as bookkeeping only: a host
            // capability has already been installed by Codegen and does not
            // become an application-generation module. Preserve that boundary
            // by ignoring the manager's return value here as well.
            unsafe { (self.stack.api().send)(module_manager, mark, [symbol].as_ptr(), 1) };
        }
        Ok(())
    }

    unsafe fn activate_project_build(
        &self,
        build: &ProjectBuildResult,
    ) -> Result<(), ProjectRunError> {
        let project_lib = build.output_root.join("lib");
        let api = self.stack.api();
        let alien = unsafe { (api.import)(c"Alien".as_ptr()) };
        if alien != api.nilObj {
            let path = path_bytes(&project_lib);
            let value = unsafe { (api.mkString)(path.as_ptr(), path.len()) };
            let selector = unsafe { (api.intern)(c"libraryDir:".as_ptr()) };
            let result = unsafe { (api.send)(alien, selector, [value].as_ptr(), 1) };
            if result == api.nilObj || send_is_error(api, result) {
                return Err(ProjectRunError::NativeDirectoryRejected);
            }
        }
        let mut paths = Vec::with_capacity(build.record.artifacts.len());
        for artifact in &build.record.artifacts {
            let Some(relative) = safe_record_relative_path(&artifact.path) else {
                return Err(ProjectRunError::ArtifactPreflight(
                    ArtifactPreflightError::Read {
                        path: PathBuf::from(&artifact.path),
                        kind: io::ErrorKind::InvalidInput,
                    },
                ));
            };
            let path = build.output_root.join(relative);
            preflight_artifact(&path, Some(artifact.name.as_bytes()))
                .map_err(ProjectRunError::ArtifactPreflight)?;
            paths.push((path, artifact.name.clone()));
        }
        for (path, name) in paths {
            unsafe { activate_artifact(self, &path, Some(name.as_bytes())) }
                .map_err(ProjectRunError::ArtifactActivation)?;
        }
        Ok(())
    }

    /// Run a project main after validating/building its publication, loading
    /// project modules through the canonical Module manager, and installing
    /// ProjectArgs. Repl/socket follow the observed run policy unless the CLI
    /// supplies an explicit mode.
    ///
    /// # Safety
    ///
    /// The caller must keep a live GC anchor and remain on the floor owner
    /// thread for every policy, activation, evaluation, and REPL send.
    pub unsafe fn run_project_with_tool_version(
        &self,
        options: &RunOptions,
        tool_version: &str,
    ) -> Result<(), ProjectRunError> {
        let target = host_target_identity();
        let build = if options.build {
            let built_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| {
                    ProjectRunError::Build(ProjectBuildError::ManifestRead {
                        path: options.directory.clone(),
                        kind: io::ErrorKind::Other,
                    })
                })?
                .as_secs()
                .to_string();
            let request = ProjectBuildRequest {
                start: &options.directory,
                target: &target,
                profile: &options.profile,
                tool_version,
                built_at: &built_at,
            };
            unsafe { self.build_project(&request) }
        } else {
            unsafe {
                self.validate_project_build(
                    &options.directory,
                    &target,
                    &options.profile,
                    tool_version,
                )
            }
        }
        .map_err(ProjectRunError::Build)?;
        let root = unsafe { self.observe_project_snapshot(&options.directory) }
            .map_err(|error| ProjectRunError::Build(ProjectBuildError::Observation(error)))?;
        let root = root.projects.first().ok_or(ProjectRunError::MissingMain)?;
        let main = root.main.as_deref().ok_or(ProjectRunError::MissingMain)?;
        let mode = if let Some(mode) = options.repl {
            mode
        } else {
            match root.run.repl.as_str() {
                "none" => ReplMode::None,
                "tty" => ReplMode::Tty,
                "socket" => ReplMode::Socket,
                value => return Err(ProjectRunError::InvalidReplMode(value.to_owned())),
            }
        };
        match mode {
            ReplMode::None => {}
            ReplMode::Tty => {
                unsafe { self.load_runtime_module("repl") }.map_err(ProjectRunError::Module)?
            }
            ReplMode::Socket => {
                unsafe { self.load_runtime_module("socket") }.map_err(ProjectRunError::Module)?;
                unsafe { self.load_runtime_module("repl") }.map_err(ProjectRunError::Module)?;
            }
        }
        let project_lib = build.output_root.join("lib");
        let api = self.stack.api();
        let alien = unsafe { (api.import)(c"Alien".as_ptr()) };
        if alien != api.nilObj {
            let path = path_bytes(&project_lib);
            let value = unsafe { (api.mkString)(path.as_ptr(), path.len()) };
            let selector = unsafe { (api.intern)(c"libraryDir:".as_ptr()) };
            let result = unsafe { (api.send)(alien, selector, [value].as_ptr(), 1) };
            if result == api.nilObj || send_is_error(api, result) {
                return Err(ProjectRunError::NativeDirectoryRejected);
            }
        }
        let mut paths = Vec::with_capacity(build.record.artifacts.len());
        for artifact in &build.record.artifacts {
            let Some(relative) = safe_record_relative_path(&artifact.path) else {
                return Err(ProjectRunError::ArtifactPreflight(
                    ArtifactPreflightError::Read {
                        path: PathBuf::from(&artifact.path),
                        kind: io::ErrorKind::InvalidInput,
                    },
                ));
            };
            let path = build.output_root.join(relative);
            preflight_artifact(&path, Some(artifact.name.as_bytes()))
                .map_err(ProjectRunError::ArtifactPreflight)?;
            paths.push((path, artifact.name.clone()));
        }
        for (path, name) in paths {
            unsafe { activate_artifact(self, &path, Some(name.as_bytes())) }
                .map_err(ProjectRunError::ArtifactActivation)?;
        }
        let globals =
            unsafe { global_named(api, "Globals") }.ok_or(ProjectRunError::ArgumentsRejected)?;
        let values = unsafe { native_os_string_vector(api, &options.arguments) }
            .map_err(|_| ProjectRunError::ArgumentsRejected)?;
        let key = unsafe { (api.intern)(c"ProjectArgs".as_ptr()) };
        let put = unsafe { (api.intern)(c"at:put:".as_ptr()) };
        let result = unsafe { (api.send)(globals, put, [key, values].as_ptr(), 2) };
        if result == api.nilObj || send_is_error(api, result) {
            return Err(ProjectRunError::ArgumentsRejected);
        }
        unsafe { eval_script(self, Path::new(main)) }.map_err(ProjectRunError::Main)?;
        match mode {
            ReplMode::None => Ok(()),
            ReplMode::Tty => {
                let mut session = unsafe { self.start_repl() }.map_err(ProjectRunError::Repl)?;
                unsafe { self.run_repl(&mut session) }.map_err(ProjectRunError::Repl)
            }
            ReplMode::Socket => {
                let bind = options
                    .socket_bind
                    .as_deref()
                    .unwrap_or(&root.run.socket_bind);
                let port = options.socket_port.unwrap_or(root.run.socket_port as u16);
                unsafe { self.serve_socket(port, bind) }.map_err(ProjectRunError::Socket)
            }
        }
    }

    /// Run a project using the host crate's package version when no external
    /// toolchain version is available.
    ///
    /// # Safety
    ///
    /// See [`HostRuntime::run_project_with_tool_version`].
    pub unsafe fn run_project(&self, options: &RunOptions) -> Result<(), ProjectRunError> {
        unsafe { self.run_project_with_tool_version(options, env!("CARGO_PKG_VERSION")) }
    }

    /// Validate/activate one published project and evaluate one test source in
    /// a fresh worker process. The caller owns worker lifecycle and output.
    ///
    /// # Safety
    ///
    /// See [`HostRuntime::run_project_with_tool_version`].
    pub unsafe fn run_project_test_source(
        &self,
        project_root: &Path,
        test_source: &Path,
        target: &str,
        profile: &str,
        tool_version: &str,
    ) -> Result<(), ProjectRunError> {
        let build =
            unsafe { self.validate_project_build(project_root, target, profile, tool_version) }
                .map_err(ProjectRunError::Build)?;
        unsafe { self.activate_project_build(&build) }?;
        // Match the Zig worker boundary: install the test harness before the
        // project source registers suites, then run those suites through the
        // canonical ConsoleRunner. Evaluating a source file alone is not a
        // test run; it would silently accept a suite-registration script
        // without producing the established `ok`/`FAIL` result lines.
        unsafe { self.load_host_runtime_module("test") }.map_err(ProjectRunError::Module)?;
        unsafe { eval_script(self, test_source) }.map_err(ProjectRunError::Main)?;
        let api = self.stack.api();
        let test =
            unsafe { global_named(api, "Test") }.ok_or(ProjectRunError::TestHarnessUnavailable)?;
        let runner = unsafe { global_named(api, "ConsoleRunner") }
            .ok_or(ProjectRunError::TestHarnessUnavailable)?;
        let suites_selector = unsafe { (api.intern)(c"suites".as_ptr()) };
        let suites = unsafe { (api.send)(test, suites_selector, std::ptr::null(), 0) };
        if suites == api.nilObj || send_is_error(api, suites) {
            return Err(ProjectRunError::TestHarnessUnavailable);
        }
        let size_selector = unsafe { (api.intern)(c"size".as_ptr()) };
        let count = unsafe { (api.send)(suites, size_selector, std::ptr::null(), 0) };
        if unsafe { (api.isSmallInt)(count) } == 0 || unsafe { (api.smallIntValue)(count) } == 0 {
            return Err(ProjectRunError::TestSuiteMissing);
        }
        let run_selector = unsafe { (api.intern)(c"runAllSuitesWithRunner:".as_ptr()) };
        let result = unsafe { (api.send)(test, run_selector, [runner].as_ptr(), 1) };
        if result == api.nilObj || send_is_error(api, result) {
            return Err(ProjectRunError::TestFailed);
        }
        let passed_selector = unsafe { (api.intern)(c"allPassed".as_ptr()) };
        let passed = unsafe { (api.send)(result, passed_selector, std::ptr::null(), 0) };
        if passed != api.trueObj {
            return Err(ProjectRunError::TestFailed);
        }
        Ok(())
    }

    /// Lazily install the Birnam-owned project policy module. Filesystem
    /// observation and the `Project` bridge remain a later host seam; this
    /// method only makes the semantic policy available through the existing
    /// compiled-module/source-recovery rule.
    ///
    /// # Safety
    ///
    /// The caller must have prepared a live GC anchor and remain on the floor
    /// owner thread for all synchronous sends.
    pub unsafe fn load_project_policy(&self) -> Result<(), ProjectPolicyError> {
        if unsafe { global_named(self.stack.api(), "ProjectPolicy") }.is_some() {
            return Ok(());
        }
        unsafe {
            load_self_hosted(
                self.stack.api(),
                self.codegen,
                &self.resources,
                &PROJECT_POLICY_MODULE,
                self.policy.force_source,
            )
        }
        .map_err(ProjectPolicyError::Source)?;
        if let Some(manager) = unsafe { global_named(self.stack.api(), "Module") } {
            unsafe { mark_module_loaded(self.stack.api(), manager, PROJECT_POLICY_MODULE.name) };
        }
        Ok(())
    }

    /// Apply Birnam's project-resolution policy to one host-observed snapshot.
    /// The snapshot is an Oop map/vector graph owned by the live Store; this
    /// method does not inspect filesystem data or reimplement dependency
    /// closure rules in Rust.
    ///
    /// # Safety
    ///
    /// `snapshot` must be a live value from this runtime. The caller must have
    /// prepared a live GC anchor and remain on the floor owner thread.
    pub unsafe fn resolve_project_snapshot(
        &self,
        snapshot: birnam_abi::Oop,
    ) -> Result<birnam_abi::Oop, ProjectPolicyError> {
        let api = self.stack.api();
        if snapshot == api.nilObj || send_is_error(api, snapshot) {
            return Err(ProjectPolicyError::InvalidSnapshot);
        }
        unsafe { self.load_project_policy()? };
        let Some(policy) = (unsafe { global_named(api, "ProjectPolicy") }) else {
            return Err(ProjectPolicyError::MissingPolicy);
        };
        let selector = unsafe { (api.intern)(c"resolveSnapshot:".as_ptr()) };
        let result = unsafe { (api.send)(policy, selector, [snapshot].as_ptr(), 1) };
        if result == api.nilObj || send_is_error(api, result) {
            if let Some((kind, message)) = unsafe { error_diagnostic(api, result) } {
                return Err(ProjectPolicyError::ResolutionDiagnostic { kind, message });
            }
            return Err(ProjectPolicyError::ResolutionRejected);
        }
        Ok(result)
    }

    /// Materialize a typed host observation into the exact map/vector shape
    /// consumed by [`HostRuntime::resolve_project_snapshot`]. This is a
    /// serialization boundary only; it performs no policy decisions.
    ///
    /// # Safety
    ///
    /// The runtime must remain on the floor owner thread with a live GC anchor
    /// for all allocations and sends.
    pub unsafe fn materialize_project_snapshot(
        &self,
        snapshot: &ProjectSnapshot,
    ) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let builder = unsafe { ProjectSnapshotBuilder::new(self.stack.api())? };
        unsafe { builder.snapshot(snapshot) }
    }

    /// Read one manifest and delegate TOML parsing to the live self-hosted
    /// `Toml parse:` protocol. Rust owns only file I/O and returns the
    /// resulting Map Oop for the observation adapter; it does not duplicate a
    /// TOML grammar or reinterpret parser errors.
    ///
    /// # Safety
    ///
    /// The returned map is owned by this runtime's Store. The caller must keep
    /// a live GC anchor and remain on the floor owner thread while using it.
    pub unsafe fn parse_project_manifest(
        &self,
        path: &Path,
    ) -> Result<birnam_abi::Oop, ProjectManifestError> {
        let source = std::fs::read(path).map_err(|error| ProjectManifestError::Read {
            path: path.to_path_buf(),
            kind: error.kind(),
        })?;
        let api = self.stack.api();
        let toml = unsafe { global_named(api, "Toml") }.ok_or(ProjectManifestError::MissingToml)?;
        let source = unsafe { (api.mkString)(source.as_ptr(), source.len()) };
        let selector = unsafe { (api.intern)(c"parse:".as_ptr()) };
        let result = unsafe { (api.send)(toml, selector, [source].as_ptr(), 1) };
        if result == api.nilObj || send_is_error(api, result) {
            return Err(ProjectManifestError::ParseRejected);
        }
        Ok(result)
    }

    /// Parse and re-observe one persisted `birnam-artifacts-v3` record. The
    /// TOML map is decoded through the self-hosted parser, then every relative
    /// artifact path is hashed below the record's parent directory. Stored
    /// checksums never cross into policy as trusted observations.
    ///
    /// # Safety
    ///
    /// The returned record owns Rust strings only. The caller must keep a live
    /// GC anchor and remain on the floor owner thread while parsing TOML.
    pub unsafe fn load_project_build_record(
        &self,
        path: &Path,
    ) -> Result<ProjectBuildRecord, ProjectBuildRecordLoadError> {
        let parsed = unsafe { self.parse_project_manifest(path) }
            .map_err(ProjectBuildRecordLoadError::Manifest)?;
        let record = unsafe { decode_project_build_record(self.stack.api(), parsed) }
            .map_err(ProjectBuildRecordLoadError::Decode)?;
        let Some(build_root) = path.parent() else {
            return Err(ProjectBuildRecordLoadError::InvalidPath(path.to_path_buf()));
        };
        observe_project_build_record(build_root, record)
            .map_err(ProjectBuildRecordLoadError::Observe)
    }

    /// Compose nearest-root discovery with self-hosted manifest parsing. The
    /// returned path is canonical and the returned Map is the live parsed TOML
    /// value; source indexing and ProjectPolicy invocation remain separate
    /// operations so callers cannot accidentally mix observation and policy.
    ///
    /// # Safety
    ///
    /// The returned Map is owned by this runtime's Store. Keep a live GC anchor
    /// and remain on the floor owner thread while using it.
    pub unsafe fn prepare_project_manifest(
        &self,
        start: &Path,
    ) -> Result<(PathBuf, birnam_abi::Oop), ProjectPreparationError> {
        let root = nearest_project_root(start).map_err(ProjectPreparationError::Discovery)?;
        let manifest = root.join("birnam.toml");
        let parsed = unsafe { self.parse_project_manifest(&manifest) }
            .map_err(ProjectPreparationError::Manifest)?;
        Ok((root, parsed))
    }

    unsafe fn observe_project_at(
        &self,
        root: &Path,
        is_root: bool,
    ) -> Result<(ProjectObservation, Vec<ProjectDependencyObservation>), ProjectObservationBuildError>
    {
        let manifest_path = root.join("birnam.toml");
        let parsed = unsafe { self.parse_project_manifest(&manifest_path) }
            .map_err(ProjectObservationBuildError::Manifest)?;
        let manifest = unsafe { decode_project_manifest(self.stack.api(), parsed) }
            .map_err(ProjectObservationBuildError::Decode)?;
        let dependencies = resolve_project_dependency_paths(root, &manifest.dependencies)
            .map_err(ProjectObservationBuildError::Dependency)?;
        let root_string = root.to_string_lossy().into_owned();
        let mut sources = Vec::new();
        let mut explicit_paths = HashSet::new();
        for module in &manifest.modules {
            let paths = project_composition_paths(root, &module.entry, &module.files)
                .map_err(ProjectSourceIndexError::CompositionPlan)
                .map_err(ProjectObservationBuildError::Source)?;
            for path in paths.iter().skip(1) {
                if matches!(
                    unsafe { self.index_project_source(path) }
                        .map_err(ProjectObservationBuildError::Source)?,
                    ProjectSourceIndexOutcome::Metadata(_)
                ) {
                    return Err(ProjectObservationBuildError::Source(
                        ProjectSourceIndexError::Composition(
                            ProjectCompositionError::PartDeclaration(path.clone()),
                        ),
                    ));
                }
            }
            explicit_paths.extend(paths.iter().cloned());
            if let Some(source) =
                unsafe { self.observe_composed_project_source(&paths, &root_string) }
                    .map_err(ProjectObservationBuildError::Source)?
            {
                sources.push(source);
            }
        }
        let files = collect_project_source_files(root, &manifest.source_roots)
            .map_err(ProjectObservationBuildError::SourceWalk)?;
        for path in files {
            if explicit_paths.contains(&path) {
                continue;
            }
            let source_path = path.to_string_lossy().into_owned();
            if let Some(source) =
                unsafe { self.observe_project_source(&path, &root_string, &[source_path]) }
                    .map_err(ProjectObservationBuildError::Source)?
            {
                sources.push(source);
            }
        }
        validate_project_observations(&sources)
            .map_err(ProjectObservationBuildError::Observation)?;
        let main = if is_root {
            if let Some(main) = manifest.main.as_deref() {
                let main_path = root.join(main);
                if std::fs::metadata(&main_path).is_err() {
                    return Err(ProjectObservationBuildError::MissingMain(main_path));
                }
                Some(main_path.to_string_lossy().into_owned())
            } else {
                None
            }
        } else {
            None
        };
        Ok((
            ProjectObservation {
                root: root_string,
                manifest: manifest_path.to_string_lossy().into_owned(),
                name: manifest.name,
                main,
                loads: manifest.loads,
                run: manifest.run,
                wrapped: manifest.wrapped,
                dependencies: dependencies.clone(),
                sources,
                test_sources: manifest.test_sources,
            },
            dependencies,
        ))
    }

    unsafe fn observe_project_recursive(
        &self,
        root: &Path,
        is_root: bool,
        visiting: &mut HashSet<PathBuf>,
        visited: &mut HashSet<PathBuf>,
        projects: &mut Vec<ProjectObservation>,
    ) -> Result<(), ProjectObservationBuildError> {
        if visited.contains(root) {
            return Ok(());
        }
        if !visiting.insert(root.to_path_buf()) {
            return Err(ProjectObservationBuildError::DependencyCycle(
                root.to_path_buf(),
            ));
        }
        let result = (|| {
            let (project, dependencies) = unsafe { self.observe_project_at(root, is_root) }?;
            projects.push(project);
            for dependency in dependencies {
                let dependency_root = PathBuf::from(dependency.root);
                unsafe {
                    self.observe_project_recursive(
                        &dependency_root,
                        false,
                        visiting,
                        visited,
                        projects,
                    )?
                };
            }
            Ok::<(), ProjectObservationBuildError>(())
        })();
        visiting.remove(root);
        if result.is_ok() {
            visited.insert(root.to_path_buf());
        }
        result
    }

    /// Observe the root project and its canonical dependency closure into the
    /// pointer-free snapshot consumed by `ProjectPolicy`. Rust owns only
    /// filesystem reads, Codegen metadata, and deterministic ordering; TOML
    /// parsing and provider/policy decisions remain Birnam-owned.
    ///
    /// # Safety
    ///
    /// The caller must keep a live GC anchor and remain on the floor owner
    /// thread for every parse, composition, and metadata send.
    pub unsafe fn observe_project_snapshot(
        &self,
        start: &Path,
    ) -> Result<ProjectSnapshot, ProjectObservationBuildError> {
        let root = nearest_project_root(start).map_err(ProjectObservationBuildError::Discovery)?;
        let mut visiting = HashSet::new();
        let mut visited = HashSet::new();
        let mut projects = Vec::new();
        unsafe {
            self.observe_project_recursive(&root, true, &mut visiting, &mut visited, &mut projects)?
        };
        Ok(ProjectSnapshot {
            root: root.to_string_lossy().into_owned(),
            projects,
        })
    }

    /// Run the compile-only project check over a policy-selected resolution.
    /// Each module is compiled into a private temporary directory; no output
    /// is activated or published, and compiler failures are returned as owned
    /// diagnostics rather than crossing the host boundary as live Oops.
    ///
    /// # Safety
    ///
    /// The caller must keep a live GC anchor and remain on the floor owner
    /// thread for all observation, policy, and Codegen sends.
    pub unsafe fn check_project(
        &self,
        start: &Path,
    ) -> Result<ProjectCheckResult, ProjectCheckError> {
        let snapshot = unsafe { self.observe_project_snapshot(start) }
            .map_err(ProjectCheckError::Observation)?;
        let raw_snapshot = unsafe { self.materialize_project_snapshot(&snapshot) }
            .map_err(ProjectCheckError::Snapshot)?;
        let raw_resolution = unsafe { self.resolve_project_snapshot(raw_snapshot) }
            .map_err(ProjectCheckError::Policy)?;
        let resolution = unsafe { decode_project_resolution(self.stack.api(), raw_resolution) }
            .map_err(ProjectCheckError::Resolution)?;
        let manifest = std::fs::read(&resolution.manifest).map_err(|error| {
            ProjectCheckError::ManifestRead {
                path: PathBuf::from(&resolution.manifest),
                kind: error.kind(),
            }
        })?;
        let resolution_digest =
            project_resolution_digest(&resolution.project_root, &manifest, &resolution.modules);
        let root_observation = snapshot
            .projects
            .iter()
            .find(|project| project.root == resolution.project_root);
        let wrapped = root_observation
            .map(|project| project.wrapped.clone())
            .unwrap_or_default();
        let test_sources = root_observation.and_then(|project| project.test_sources.clone());
        let temporary = create_check_temp_directory()
            .map_err(|kind| ProjectCheckError::TempDirectory { kind })?;
        let api = self.stack.api();
        let compile_selector = unsafe { (api.intern)(c"compile:output:root:".as_ptr()) };
        let last_error_selector = unsafe { (api.intern)(c"lastError".as_ptr()) };
        let mut diagnostics = Vec::new();
        for (index, module) in resolution.modules.iter().enumerate() {
            let output = temporary.0.join(format!("module-{index}.dylib"));
            let source = unsafe { (api.mkString)(module.source.as_ptr(), module.source.len()) };
            let output_bytes = path_bytes(&output);
            let output = unsafe { (api.mkString)(output_bytes.as_ptr(), output_bytes.len()) };
            let root_bytes = module.project_root.as_bytes();
            let root = unsafe { (api.mkString)(root_bytes.as_ptr(), root_bytes.len()) };
            let result = unsafe {
                (api.send)(
                    self.codegen,
                    compile_selector,
                    [source, output, root].as_ptr(),
                    3,
                )
            };
            if result == api.nilObj || send_is_error(api, result) {
                let detail_value =
                    unsafe { (api.send)(self.codegen, last_error_selector, std::ptr::null(), 0) };
                let detail = if detail_value != api.nilObj
                    && unsafe { (api.vtableOf)(detail_value) } == api.string_vt
                {
                    let mut length = 0usize;
                    let bytes = unsafe { (api.stringBytes)(detail_value, &mut length) };
                    if bytes.is_null() {
                        "compiler did not provide a diagnostic".to_owned()
                    } else {
                        String::from_utf8_lossy(unsafe {
                            std::slice::from_raw_parts(bytes, length)
                        })
                        .into_owned()
                    }
                } else {
                    "compiler did not provide a diagnostic".to_owned()
                };
                diagnostics.push(parse_project_compile_diagnostic(
                    &module.name,
                    &module.path,
                    &detail,
                ));
            }
        }
        drop(temporary);
        Ok(ProjectCheckResult {
            resolution,
            resolution_digest,
            wrapped,
            test_sources,
            diagnostics,
        })
    }

    /// Compile one observed module into a caller-owned staging path. The
    /// Codegen result is read back and checked against the module's inert
    /// descriptor before its checksum is returned; activation and publication
    /// remain separate host operations.
    ///
    /// # Safety
    ///
    /// The caller must keep a live GC anchor and remain on the floor owner
    /// thread for the Codegen send. `output` must be an isolated staging path
    /// whose parent already exists.
    pub unsafe fn compile_project_artifact(
        &self,
        module: &ProjectSourceObservation,
        output: &Path,
    ) -> Result<String, ProjectArtifactCompileError> {
        let api = self.stack.api();
        let selector = unsafe { (api.intern)(c"compile:output:root:".as_ptr()) };
        let source = unsafe { (api.mkString)(module.source.as_ptr(), module.source.len()) };
        let output_bytes = path_bytes(output);
        let output_value = unsafe { (api.mkString)(output_bytes.as_ptr(), output_bytes.len()) };
        let root_value = unsafe {
            (api.mkString)(
                module.project_root.as_bytes().as_ptr(),
                module.project_root.len(),
            )
        };
        let result = unsafe {
            (api.send)(
                self.codegen,
                selector,
                [source, output_value, root_value].as_ptr(),
                3,
            )
        };
        if result == api.nilObj || send_is_error(api, result) {
            let last_error_selector = unsafe { (api.intern)(c"lastError".as_ptr()) };
            let detail_value =
                unsafe { (api.send)(self.codegen, last_error_selector, std::ptr::null(), 0) };
            let detail = if detail_value != api.nilObj
                && unsafe { (api.vtableOf)(detail_value) } == api.string_vt
            {
                let mut length = 0usize;
                let bytes = unsafe { (api.stringBytes)(detail_value, &mut length) };
                if bytes.is_null() {
                    "compiler did not provide a diagnostic".to_owned()
                } else {
                    String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(bytes, length) })
                        .into_owned()
                }
            } else {
                "compiler did not provide a diagnostic".to_owned()
            };
            return Err(ProjectArtifactCompileError::CodegenRejected {
                module: module.name.clone(),
                detail,
            });
        }
        let bytes =
            std::fs::read(output).map_err(|error| ProjectArtifactCompileError::ReadOutput {
                path: output.to_path_buf(),
                kind: error.kind(),
            })?;
        validate_project_compiled_artifact(module, &bytes)
            .map_err(ProjectArtifactCompileError::Descriptor)?;
        Ok(String::from_utf8_lossy(&sha256_hex(&bytes)).into_owned())
    }

    /// Materialize module decisions into an isolated staging directory. A
    /// compile action invokes Codegen; a reuse action copies the prior artifact
    /// only after re-reading and validating its descriptor and checksum. The
    /// final build directory remains untouched by this method.
    ///
    /// # Safety
    ///
    /// The caller must keep a live GC anchor and remain on the floor owner
    /// thread for compile actions. `stage_root` must be a disposable staging
    /// directory and must not be the published build root.
    pub unsafe fn stage_project_artifacts(
        &self,
        request: &ProjectArtifactStageRequest<'_>,
    ) -> Result<Vec<ProjectStagedArtifact>, ProjectArtifactStageError> {
        std::fs::create_dir_all(request.stage_root.join("lib")).map_err(|error| {
            ProjectArtifactStageError::CreateDirectory {
                path: request.stage_root.join("lib"),
                kind: error.kind(),
            }
        })?;
        if request.plan.artifacts.len() != request.resolution.modules.len() {
            return Err(ProjectArtifactStageError::ArtifactCount {
                expected: request.resolution.modules.len(),
                actual: request.plan.artifacts.len(),
            });
        }
        let mut staged = Vec::with_capacity(request.plan.artifacts.len());
        for (index, (module, plan)) in request
            .resolution
            .modules
            .iter()
            .zip(&request.plan.artifacts)
            .enumerate()
        {
            let Some(relative) = safe_record_relative_path(&plan.path) else {
                return Err(ProjectArtifactStageError::InvalidPlanPath {
                    index,
                    path: plan.path.clone(),
                });
            };
            let destination = request.stage_root.join(relative);
            let checksum = match plan.action.as_str() {
                "compile" => unsafe { self.compile_project_artifact(module, &destination) }
                    .map_err(ProjectArtifactStageError::Compile)?,
                "reuse" => {
                    let Some(previous) = request.previous else {
                        return Err(ProjectArtifactStageError::MissingPrevious {
                            name: module.name.clone(),
                        });
                    };
                    let Some(previous_root) = request.previous_root else {
                        return Err(ProjectArtifactStageError::MissingPreviousRoot {
                            name: module.name.clone(),
                        });
                    };
                    let Some(old) = previous_artifact(previous, &module.name) else {
                        return Err(ProjectArtifactStageError::MissingPrevious {
                            name: module.name.clone(),
                        });
                    };
                    let Some(old_relative) = safe_record_relative_path(&old.path) else {
                        return Err(ProjectArtifactStageError::InvalidPlanPath {
                            index,
                            path: old.path.clone(),
                        });
                    };
                    let source = previous_root.join(old_relative);
                    std::fs::copy(&source, &destination).map_err(|error| {
                        ProjectArtifactStageError::Copy {
                            source: source.clone(),
                            destination: destination.clone(),
                            kind: error.kind(),
                        }
                    })?;
                    let bytes = std::fs::read(&destination).map_err(|error| {
                        ProjectArtifactStageError::Read {
                            path: destination.clone(),
                            kind: error.kind(),
                        }
                    })?;
                    validate_project_compiled_artifact(module, &bytes)
                        .map_err(ProjectArtifactStageError::Descriptor)?;
                    let checksum = String::from_utf8_lossy(&sha256_hex(&bytes)).into_owned();
                    let Some(expected) = plan.checksum.as_deref() else {
                        return Err(ProjectArtifactStageError::ChecksumMismatch {
                            name: module.name.clone(),
                            expected: "present".to_owned(),
                            actual: "missing".to_owned(),
                        });
                    };
                    if checksum != expected {
                        return Err(ProjectArtifactStageError::ChecksumMismatch {
                            name: module.name.clone(),
                            expected: expected.to_owned(),
                            actual: checksum,
                        });
                    }
                    checksum
                }
                action => {
                    return Err(ProjectArtifactStageError::InvalidAction {
                        index,
                        action: action.to_owned(),
                    });
                }
            };
            staged.push(ProjectStagedArtifact {
                name: module.name.clone(),
                path: plan.path.clone(),
                action: plan.action.clone(),
                checksum,
            });
        }
        Ok(staged)
    }

    /// Execute wrapped native build commands directly (never through a shell),
    /// expand the frozen placeholders, and copy each produced dylib into the
    /// module library area of the same disposable stage.
    pub fn stage_project_wrapped(
        &self,
        project_root: &Path,
        wrapped: &[ProjectWrappedObservation],
        stage_root: &Path,
    ) -> Result<Vec<ProjectPreviousNativeArtifact>, ProjectWrappedBuildError> {
        let lib_root = stage_root.join("lib");
        std::fs::create_dir_all(&lib_root).map_err(|error| {
            ProjectWrappedBuildError::CopyOutput {
                source: stage_root.to_path_buf(),
                destination: lib_root.clone(),
                kind: error.kind(),
            }
        })?;
        let native_root = stage_root.join("native");
        let mut result = Vec::with_capacity(wrapped.len());
        for item in wrapped {
            let command_root = native_root.join(&item.name);
            std::fs::create_dir_all(&command_root).map_err(|error| {
                ProjectWrappedBuildError::CopyOutput {
                    source: native_root.clone(),
                    destination: command_root.clone(),
                    kind: error.kind(),
                }
            })?;
            let filename = format!("lib{}.dylib", item.name);
            let output = command_root.join(&filename);
            let staged_output = lib_root.join(&filename);
            let _ = std::fs::remove_file(&output);
            for command in &item.build {
                if command.is_empty() {
                    return Err(ProjectWrappedBuildError::EmptyCommand {
                        name: item.name.clone(),
                    });
                }
                let expanded = command
                    .iter()
                    .map(|argument| {
                        expand_project_wrapped_argument(
                            argument,
                            project_root,
                            &output,
                            stage_root,
                            &lib_root,
                        )
                    })
                    .collect::<Vec<_>>();
                let output_result = std::process::Command::new(&expanded[0])
                    .args(&expanded[1..])
                    .current_dir(project_root)
                    .output()
                    .map_err(|error| ProjectWrappedBuildError::Spawn {
                        name: item.name.clone(),
                        kind: error.kind(),
                    })?;
                if !output_result.status.success() {
                    let mut detail = String::from_utf8_lossy(&output_result.stdout).into_owned();
                    detail.push_str(&String::from_utf8_lossy(&output_result.stderr));
                    detail.truncate(16 << 20);
                    return Err(ProjectWrappedBuildError::Failed {
                        name: item.name.clone(),
                        code: output_result.status.code(),
                        detail,
                    });
                }
            }
            if !output.is_file() {
                return Err(ProjectWrappedBuildError::MissingOutput {
                    name: item.name.clone(),
                    path: output,
                });
            }
            std::fs::copy(&output, &staged_output).map_err(|error| {
                ProjectWrappedBuildError::CopyOutput {
                    source: output.clone(),
                    destination: staged_output.clone(),
                    kind: error.kind(),
                }
            })?;
            let bytes = std::fs::read(&staged_output).map_err(|error| {
                ProjectWrappedBuildError::ReadOutput {
                    path: staged_output.clone(),
                    kind: error.kind(),
                }
            })?;
            result.push(ProjectPreviousNativeArtifact {
                name: item.name.clone(),
                path: format!("lib/{filename}"),
                checksum: String::from_utf8_lossy(&sha256_hex(&bytes)).into_owned(),
                observed_checksum: None,
            });
        }
        Ok(result)
    }

    /// Ask Birnam policy for a side-effect-free artifact plan. The request
    /// carries only typed resolution metadata; no files are compiled, loaded,
    /// or published by this boundary.
    ///
    /// # Safety
    ///
    /// The caller must keep a live GC anchor and remain on the floor owner
    /// thread for all map/vector allocation and policy sends.
    pub unsafe fn plan_project_artifacts(
        &self,
        request: &ProjectArtifactPlanRequest<'_>,
    ) -> Result<ProjectArtifactPlan, ProjectArtifactPlanError> {
        let builder = unsafe { ProjectSnapshotBuilder::new(self.stack.api()) }
            .map_err(ProjectArtifactPlanError::Snapshot)?;
        let snapshot = unsafe { builder.artifact_plan_input(request) }
            .map_err(ProjectArtifactPlanError::Snapshot)?;
        unsafe { self.load_project_policy() }.map_err(ProjectArtifactPlanError::Policy)?;
        let api = self.stack.api();
        let Some(policy) = (unsafe { global_named(api, "ProjectPolicy") }) else {
            return Err(ProjectArtifactPlanError::Policy(
                ProjectPolicyError::MissingPolicy,
            ));
        };
        let selector = unsafe { (api.intern)(c"artifactPlan:".as_ptr()) };
        let plan = unsafe { (api.send)(policy, selector, [snapshot].as_ptr(), 1) };
        if plan == api.nilObj || send_is_error(api, plan) {
            return Err(ProjectArtifactPlanError::Policy(
                ProjectPolicyError::ResolutionRejected,
            ));
        }
        let plan = unsafe { decode_project_artifact_plan(api, plan) }
            .map_err(ProjectArtifactPlanError::Decode)?;
        validate_project_artifact_plan(&plan, request)
            .map_err(ProjectArtifactPlanError::Validation)?;
        Ok(plan)
    }

    /// Ask Birnam policy to own the persisted artifact record after staged
    /// bytes have been compiled/copied. Rust supplies only observed metadata
    /// and checksums; policy remains the source of the record schema.
    ///
    /// # Safety
    ///
    /// The caller must keep a live GC anchor and remain on the floor owner
    /// thread for all map/vector allocation and policy sends.
    pub unsafe fn record_project_artifacts(
        &self,
        request: &ProjectArtifactRecordRequest<'_>,
    ) -> Result<ProjectBuildRecord, ProjectArtifactRecordError> {
        if request.staged.len() != request.resolution.modules.len() {
            return Err(ProjectArtifactRecordError::Decode(
                ProjectManifestDecodeError::InvalidList("staged artifacts"),
            ));
        }
        let builder = unsafe { ProjectSnapshotBuilder::new(self.stack.api()) }
            .map_err(ProjectArtifactRecordError::Snapshot)?;
        let plan_request = ProjectArtifactPlanRequest {
            resolution: request.resolution,
            wrapped: request.wrapped,
            previous: None,
            target: request.target,
            profile: request.profile,
            tool_version: request.tool_version,
            built_at: request.built_at,
            resolution_digest: request.resolution_digest,
        };
        let snapshot = unsafe { builder.artifact_plan_input(&plan_request) }
            .map_err(ProjectArtifactRecordError::Snapshot)?;
        let mut artifacts =
            unsafe { builder.new_vector() }.map_err(ProjectArtifactRecordError::Snapshot)?;
        for (module, staged) in request.resolution.modules.iter().zip(request.staged) {
            let value = unsafe { builder.staged_artifact(module, staged) }
                .map_err(ProjectArtifactRecordError::Snapshot)?;
            artifacts = unsafe { builder.add(artifacts, value) }
                .map_err(ProjectArtifactRecordError::Snapshot)?;
        }
        let mut native =
            unsafe { builder.new_vector() }.map_err(ProjectArtifactRecordError::Snapshot)?;
        for item in request.native {
            let value = unsafe { builder.staged_native_artifact(item) }
                .map_err(ProjectArtifactRecordError::Snapshot)?;
            native = unsafe { builder.add(native, value) }
                .map_err(ProjectArtifactRecordError::Snapshot)?;
        }
        unsafe { self.load_project_policy() }.map_err(ProjectArtifactRecordError::Policy)?;
        let api = self.stack.api();
        let Some(policy) = (unsafe { global_named(api, "ProjectPolicy") }) else {
            return Err(ProjectArtifactRecordError::Policy(
                ProjectPolicyError::MissingPolicy,
            ));
        };
        let selector = unsafe { (api.intern)(c"artifactRecord:artifacts:native:".as_ptr()) };
        let record =
            unsafe { (api.send)(policy, selector, [snapshot, artifacts, native].as_ptr(), 3) };
        if record == api.nilObj || send_is_error(api, record) {
            return Err(ProjectArtifactRecordError::Policy(
                ProjectPolicyError::ResolutionRejected,
            ));
        }
        unsafe { decode_project_artifact_record(api, record) }
            .map_err(ProjectArtifactRecordError::Decode)
    }

    /// Execute the Rust-hosted core project build for projects without wrapped
    /// native modules. Observation and policy remain ahead of every effect;
    /// the stage is removed on all failures and only a complete stage is
    /// published atomically.
    ///
    /// # Safety
    ///
    /// The caller must keep a live GC anchor and remain on the floor owner
    /// thread for all observation, policy, and Codegen sends.
    pub unsafe fn build_project(
        &self,
        request: &ProjectBuildRequest<'_>,
    ) -> Result<ProjectBuildResult, ProjectBuildError> {
        let snapshot = unsafe { self.observe_project_snapshot(request.start) }
            .map_err(ProjectBuildError::Observation)?;
        let root = snapshot
            .projects
            .first()
            .ok_or(ProjectBuildError::Observation(
                ProjectObservationBuildError::EmptyClosure,
            ))?;
        let raw_snapshot = unsafe { self.materialize_project_snapshot(&snapshot) }
            .map_err(ProjectBuildError::Snapshot)?;
        let raw_resolution = unsafe { self.resolve_project_snapshot(raw_snapshot) }
            .map_err(ProjectBuildError::Policy)?;
        let resolution = unsafe { decode_project_resolution(self.stack.api(), raw_resolution) }
            .map_err(ProjectBuildError::Resolution)?;
        let manifest = std::fs::read(&resolution.manifest).map_err(|error| {
            ProjectBuildError::ManifestRead {
                path: PathBuf::from(&resolution.manifest),
                kind: error.kind(),
            }
        })?;
        let resolution_digest =
            project_resolution_digest(&resolution.project_root, &manifest, &resolution.modules);
        let build_root = PathBuf::from(&resolution.project_root).join(".birnam/build");
        let target_root = build_root.join(request.target);
        std::fs::create_dir_all(&target_root).map_err(|error| {
            ProjectBuildError::StageDirectory {
                path: target_root.clone(),
                kind: error.kind(),
            }
        })?;
        let final_root = target_root.join(request.profile);
        let record_path = final_root.join("artifacts.toml");
        let previous = match unsafe { self.load_project_build_record(&record_path) } {
            Ok(record) => Some(record),
            Err(ProjectBuildRecordLoadError::Manifest(ProjectManifestError::Read {
                kind: io::ErrorKind::NotFound,
                ..
            })) => None,
            Err(error) => return Err(ProjectBuildError::BuildRecord(error)),
        };
        let plan_request = ProjectArtifactPlanRequest {
            resolution: &resolution,
            wrapped: &root.wrapped,
            previous: previous.as_ref(),
            target: request.target,
            profile: request.profile,
            tool_version: request.tool_version,
            built_at: request.built_at,
            resolution_digest: &resolution_digest,
        };
        let plan = unsafe { self.plan_project_artifacts(&plan_request) }
            .map_err(ProjectBuildError::Plan)?;
        let process = std::process::id();
        let stage_root = (0..128u32)
            .map(|attempt| target_root.join(format!(".staging-{process}-{attempt}")))
            .find(|path| !path.exists())
            .ok_or(ProjectBuildError::StageDirectory {
                path: target_root.clone(),
                kind: io::ErrorKind::AlreadyExists,
            })?;
        std::fs::create_dir(&stage_root).map_err(|error| ProjectBuildError::StageDirectory {
            path: stage_root.clone(),
            kind: error.kind(),
        })?;
        let result = (|| {
            let stage_request = ProjectArtifactStageRequest {
                resolution: &resolution,
                plan: &plan,
                previous: previous.as_ref(),
                previous_root: previous.as_ref().map(|_| final_root.as_path()),
                stage_root: &stage_root,
            };
            let artifacts = unsafe { self.stage_project_artifacts(&stage_request) }
                .map_err(ProjectBuildError::Stage)?;
            let native = self
                .stage_project_wrapped(
                    Path::new(&resolution.project_root),
                    &root.wrapped,
                    &stage_root,
                )
                .map_err(ProjectBuildError::Wrapped)?;
            let record_request = ProjectArtifactRecordRequest {
                resolution: &resolution,
                staged: &artifacts,
                native: &native,
                wrapped: &root.wrapped,
                target: request.target,
                profile: request.profile,
                tool_version: request.tool_version,
                built_at: request.built_at,
                resolution_digest: &resolution_digest,
            };
            let record = unsafe { self.record_project_artifacts(&record_request) }
                .map_err(ProjectBuildError::Record)?;
            write_project_build_record(&stage_root.join("artifacts.toml"), &record)
                .map_err(ProjectBuildError::Write)?;
            publish_project_build(&stage_root, &final_root, &target_root)
                .map_err(ProjectBuildError::Publish)?;
            Ok(ProjectBuildResult {
                output_root: final_root.clone(),
                record,
                artifacts,
            })
        })();
        if result.is_err() {
            let _ = std::fs::remove_dir_all(&stage_root);
        }
        result
    }

    /// Validate an already-published project without compiling or mutating it.
    /// The persisted record is re-observed, Birnam owns the final policy
    /// decision, and every selected module/native file is checked before a
    /// result is returned to install/run/test adapters.
    ///
    /// # Safety
    ///
    /// The caller must keep a live GC anchor and remain on the floor owner
    /// thread for observation, TOML parsing, and policy sends.
    pub unsafe fn validate_project_build(
        &self,
        start: &Path,
        target: &str,
        profile: &str,
        tool_version: &str,
    ) -> Result<ProjectBuildResult, ProjectBuildError> {
        let snapshot = unsafe { self.observe_project_snapshot(start) }
            .map_err(ProjectBuildError::Observation)?;
        let root = snapshot
            .projects
            .first()
            .ok_or(ProjectBuildError::Observation(
                ProjectObservationBuildError::EmptyClosure,
            ))?;
        let raw_snapshot = unsafe { self.materialize_project_snapshot(&snapshot) }
            .map_err(ProjectBuildError::Snapshot)?;
        let raw_resolution = unsafe { self.resolve_project_snapshot(raw_snapshot) }
            .map_err(ProjectBuildError::Policy)?;
        let resolution = unsafe { decode_project_resolution(self.stack.api(), raw_resolution) }
            .map_err(ProjectBuildError::Resolution)?;
        let manifest = std::fs::read(&resolution.manifest).map_err(|error| {
            ProjectBuildError::ManifestRead {
                path: PathBuf::from(&resolution.manifest),
                kind: error.kind(),
            }
        })?;
        let resolution_digest =
            project_resolution_digest(&resolution.project_root, &manifest, &resolution.modules);
        let final_root = PathBuf::from(&resolution.project_root)
            .join(".birnam/build")
            .join(target)
            .join(profile);
        let record = unsafe { self.load_project_build_record(&final_root.join("artifacts.toml")) }
            .map_err(ProjectBuildError::BuildRecord)?;
        let request = ProjectArtifactPlanRequest {
            resolution: &resolution,
            wrapped: &root.wrapped,
            previous: Some(&record),
            target,
            profile,
            tool_version,
            built_at: &record.built_at,
            resolution_digest: &resolution_digest,
        };
        let builder = unsafe { ProjectSnapshotBuilder::new(self.stack.api()) }
            .map_err(ProjectBuildError::Snapshot)?;
        let snapshot_value = unsafe { builder.artifact_plan_input(&request) }
            .map_err(ProjectBuildError::Snapshot)?;
        unsafe { self.load_project_policy() }.map_err(ProjectBuildError::PublishedPolicy)?;
        let api = self.stack.api();
        let Some(policy) = (unsafe { global_named(api, "ProjectPolicy") }) else {
            return Err(ProjectBuildError::PublishedPolicy(
                ProjectPolicyError::MissingPolicy,
            ));
        };
        let selector = unsafe { (api.intern)(c"validateArtifacts:".as_ptr()) };
        let valid = unsafe { (api.send)(policy, selector, [snapshot_value].as_ptr(), 1) };
        if valid != api.trueObj || send_is_error(api, valid) {
            return Err(ProjectBuildError::PublishedPolicy(
                ProjectPolicyError::ResolutionRejected,
            ));
        }
        let mut artifacts = Vec::with_capacity(resolution.modules.len());
        for module in &resolution.modules {
            let Some(old) = previous_artifact(&record, &module.name) else {
                return Err(ProjectBuildError::PublishedArtifact {
                    name: module.name.clone(),
                    path: final_root.clone(),
                    kind: io::ErrorKind::NotFound,
                });
            };
            let Some(relative) = safe_record_relative_path(&old.path) else {
                return Err(ProjectBuildError::PublishedArtifact {
                    name: module.name.clone(),
                    path: PathBuf::from(&old.path),
                    kind: io::ErrorKind::InvalidInput,
                });
            };
            let path = final_root.join(relative);
            let bytes =
                std::fs::read(&path).map_err(|error| ProjectBuildError::PublishedArtifact {
                    name: module.name.clone(),
                    path: path.clone(),
                    kind: error.kind(),
                })?;
            validate_project_compiled_artifact(module, &bytes).map_err(|error| {
                ProjectBuildError::PublishedDescriptor {
                    name: module.name.clone(),
                    error,
                }
            })?;
            let actual = String::from_utf8_lossy(&sha256_hex(&bytes)).into_owned();
            if actual != old.checksum {
                return Err(ProjectBuildError::PublishedChecksum {
                    name: module.name.clone(),
                    expected: old.checksum.clone(),
                    actual,
                });
            }
            artifacts.push(ProjectStagedArtifact {
                name: module.name.clone(),
                path: old.path.clone(),
                action: "reuse".to_owned(),
                checksum: old.checksum.clone(),
            });
        }
        for wrapped in &root.wrapped {
            let Some(old) = previous_native_artifact(&record, &wrapped.name) else {
                return Err(ProjectBuildError::PublishedArtifact {
                    name: wrapped.name.clone(),
                    path: final_root.clone(),
                    kind: io::ErrorKind::NotFound,
                });
            };
            let expected_path = format!("lib/lib{}.dylib", wrapped.name);
            if old.path != expected_path {
                return Err(ProjectBuildError::PublishedArtifact {
                    name: wrapped.name.clone(),
                    path: PathBuf::from(&old.path),
                    kind: io::ErrorKind::InvalidData,
                });
            }
            let Some(relative) = safe_record_relative_path(&old.path) else {
                return Err(ProjectBuildError::PublishedArtifact {
                    name: wrapped.name.clone(),
                    path: PathBuf::from(&old.path),
                    kind: io::ErrorKind::InvalidInput,
                });
            };
            let path = final_root.join(relative);
            let bytes =
                std::fs::read(&path).map_err(|error| ProjectBuildError::PublishedArtifact {
                    name: wrapped.name.clone(),
                    path: path.clone(),
                    kind: error.kind(),
                })?;
            let actual = String::from_utf8_lossy(&sha256_hex(&bytes)).into_owned();
            if actual != old.checksum {
                return Err(ProjectBuildError::PublishedChecksum {
                    name: wrapped.name.clone(),
                    expected: old.checksum.clone(),
                    actual,
                });
            }
        }
        Ok(ProjectBuildResult {
            output_root: final_root,
            record,
            artifacts,
        })
    }

    /// Install a verified build's exact module/native set into the resolved
    /// runtime library directory. Files are copied and synced in a private
    /// staging directory, then renamed individually; no source compilation or
    /// runtime activation occurs here.
    pub fn install_project_build(
        &self,
        build: &ProjectBuildResult,
    ) -> Result<usize, ProjectInstallError> {
        std::fs::create_dir_all(&self.resources.lib_dir).map_err(|error| {
            ProjectInstallError::CreateStage {
                path: self.resources.lib_dir.clone(),
                kind: error.kind(),
            }
        })?;
        let process = std::process::id();
        let stage = (0..128u32)
            .map(|attempt| {
                self.resources
                    .lib_dir
                    .join(format!(".birnam-install-{process}-{attempt}"))
            })
            .find(|path| !path.exists())
            .ok_or(ProjectInstallError::CreateStage {
                path: self.resources.lib_dir.clone(),
                kind: io::ErrorKind::AlreadyExists,
            })?;
        std::fs::create_dir(&stage).map_err(|error| ProjectInstallError::CreateStage {
            path: stage.clone(),
            kind: error.kind(),
        })?;
        let result = (|| {
            let mut entries =
                Vec::with_capacity(build.record.artifacts.len() + build.record.native.len());
            entries.extend(
                build
                    .record
                    .artifacts
                    .iter()
                    .map(|artifact| (artifact.name.as_str(), artifact.path.as_str())),
            );
            entries.extend(
                build
                    .record
                    .native
                    .iter()
                    .map(|artifact| (artifact.name.as_str(), artifact.path.as_str())),
            );
            let mut sources = Vec::with_capacity(entries.len());
            for (name, relative_path) in entries {
                let Some(relative) = safe_record_relative_path(relative_path) else {
                    return Err(ProjectInstallError::InvalidArtifactPath {
                        name: name.to_owned(),
                        path: relative_path.to_owned(),
                    });
                };
                let Some(filename) = relative.file_name().and_then(|value| value.to_str()) else {
                    return Err(ProjectInstallError::InvalidArtifactPath {
                        name: name.to_owned(),
                        path: relative_path.to_owned(),
                    });
                };
                let source = build.output_root.join(relative);
                let destination = stage.join(filename);
                let bytes =
                    std::fs::read(&source).map_err(|error| ProjectInstallError::ReadSource {
                        path: source.clone(),
                        kind: error.kind(),
                    })?;
                std::fs::write(&destination, &bytes).map_err(|error| {
                    ProjectInstallError::CopyStage {
                        source: source.clone(),
                        destination: destination.clone(),
                        kind: error.kind(),
                    }
                })?;
                let file = std::fs::OpenOptions::new()
                    .read(true)
                    .open(&destination)
                    .map_err(|error| ProjectInstallError::CopyStage {
                        source: source.clone(),
                        destination: destination.clone(),
                        kind: error.kind(),
                    })?;
                file.sync_all()
                    .map_err(|error| ProjectInstallError::SyncStage {
                        path: destination.clone(),
                        kind: error.kind(),
                    })?;
                sources.push(destination);
            }
            for source in &sources {
                let Some(filename) = source.file_name() else {
                    return Err(ProjectInstallError::Publish {
                        source: source.clone(),
                        destination: self.resources.lib_dir.clone(),
                        kind: io::ErrorKind::InvalidInput,
                    });
                };
                let destination = self.resources.lib_dir.join(filename);
                std::fs::rename(source, &destination).map_err(|error| {
                    ProjectInstallError::Publish {
                        source: source.clone(),
                        destination: destination.clone(),
                        kind: error.kind(),
                    }
                })?;
            }
            Ok(sources.len())
        })();
        let _ = std::fs::remove_dir_all(&stage);
        result
    }

    /// Read one source file and ask Codegen for its static module metadata.
    /// This is observation only: no source evaluation, Module installation, or
    /// provider selection occurs here.
    ///
    /// # Safety
    ///
    /// The returned metadata Oop is owned by this runtime's Store. Keep a live
    /// GC anchor and remain on the floor owner thread while using it.
    pub unsafe fn index_project_source(
        &self,
        path: &Path,
    ) -> Result<ProjectSourceIndexOutcome, ProjectSourceIndexError> {
        let source = std::fs::read(path).map_err(|error| ProjectSourceIndexError::Read {
            path: path.to_path_buf(),
            kind: error.kind(),
        })?;
        if source.len() > MAX_SCRIPT_BYTES {
            return Err(ProjectSourceIndexError::TooLarge {
                path: path.to_path_buf(),
                limit: MAX_SCRIPT_BYTES,
            });
        }
        let api = self.stack.api();
        let codegen = unsafe { global_named(api, "Codegen") }
            .ok_or(ProjectSourceIndexError::MissingCodegen)?;
        let source = unsafe { (api.mkString)(source.as_ptr(), source.len()) };
        let selector = unsafe { (api.intern)(c"moduleMetadataOf:".as_ptr()) };
        let metadata = unsafe { (api.send)(codegen, selector, [source].as_ptr(), 1) };
        if metadata == api.nilObj {
            return Ok(ProjectSourceIndexOutcome::Ignored);
        }
        if send_is_error(api, metadata) {
            return Err(ProjectSourceIndexError::MetadataRejected);
        }
        Ok(ProjectSourceIndexOutcome::Metadata(metadata))
    }

    /// Combine static metadata with the host-owned source/path composition
    /// fields needed by `ProjectPolicy` observations. Source bytes are retained
    /// verbatim so non-UTF-8 authored content is not silently rewritten.
    ///
    /// # Safety
    ///
    /// The returned record borrows no runtime Oops. The caller must still keep
    /// the owner-thread/GC anchor live during the underlying metadata send.
    pub unsafe fn observe_project_source(
        &self,
        path: &Path,
        project_root: &str,
        composition: &[String],
    ) -> Result<Option<ProjectSourceObservation>, ProjectSourceIndexError> {
        let source = std::fs::read(path).map_err(|error| ProjectSourceIndexError::Read {
            path: path.to_path_buf(),
            kind: error.kind(),
        })?;
        if source.len() > MAX_SCRIPT_BYTES {
            return Err(ProjectSourceIndexError::TooLarge {
                path: path.to_path_buf(),
                limit: MAX_SCRIPT_BYTES,
            });
        }
        let api = self.stack.api();
        let codegen = unsafe { global_named(api, "Codegen") }
            .ok_or(ProjectSourceIndexError::MissingCodegen)?;
        let source_value = unsafe { (api.mkString)(source.as_ptr(), source.len()) };
        let selector = unsafe { (api.intern)(c"moduleMetadataOf:".as_ptr()) };
        let metadata = unsafe { (api.send)(codegen, selector, [source_value].as_ptr(), 1) };
        if metadata == api.nilObj {
            return Ok(None);
        }
        if send_is_error(api, metadata) {
            return Err(ProjectSourceIndexError::MetadataRejected);
        }
        let metadata = unsafe { decode_project_source_metadata(api, metadata) }
            .map_err(ProjectSourceIndexError::MetadataMalformed)?;
        Ok(Some(ProjectSourceObservation {
            name: metadata.name,
            path: path.to_string_lossy().into_owned(),
            sources: composition.to_vec(),
            project_root: project_root.to_owned(),
            source,
            requires: metadata.requires,
            source_digest: metadata.source_digest,
            revision_digest: metadata.revision_digest,
        }))
    }

    /// Observe one explicit module entry and its ordered source parts after
    /// Birnam Module composition. The first path is the provider identity;
    /// every path is retained in the observation's `sources` list.
    ///
    /// # Safety
    ///
    /// The returned record owns copied bytes and strings. The runtime must
    /// remain on the owner thread for composition and metadata sends.
    pub unsafe fn observe_composed_project_source(
        &self,
        paths: &[PathBuf],
        project_root: &str,
    ) -> Result<Option<ProjectSourceObservation>, ProjectSourceIndexError> {
        let source = unsafe { self.compose_project_sources(paths) }
            .map_err(ProjectSourceIndexError::Composition)?;
        let api = self.stack.api();
        let codegen = unsafe { global_named(api, "Codegen") }
            .ok_or(ProjectSourceIndexError::MissingCodegen)?;
        let source_value = unsafe { (api.mkString)(source.as_ptr(), source.len()) };
        let selector = unsafe { (api.intern)(c"moduleMetadataOf:".as_ptr()) };
        let metadata = unsafe { (api.send)(codegen, selector, [source_value].as_ptr(), 1) };
        if metadata == api.nilObj {
            return Ok(None);
        }
        if send_is_error(api, metadata) {
            return Err(ProjectSourceIndexError::MetadataRejected);
        }
        let metadata = unsafe { decode_project_source_metadata(api, metadata) }
            .map_err(ProjectSourceIndexError::MetadataMalformed)?;
        let path = paths
            .first()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(Some(ProjectSourceObservation {
            name: metadata.name,
            path,
            sources: paths
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect(),
            project_root: project_root.to_owned(),
            source,
            requires: metadata.requires,
            source_digest: metadata.source_digest,
            revision_digest: metadata.revision_digest,
        }))
    }

    /// Ask the Birnam Module manager to compose ordered source parts. The
    /// manager owns file reading and composition semantics; Rust only builds
    /// the path vector and copies the returned source bytes.
    ///
    /// # Safety
    ///
    /// The returned bytes are an owned Rust copy. The runtime must remain on
    /// the floor owner thread for the synchronous Module send.
    pub unsafe fn compose_project_sources(
        &self,
        paths: &[PathBuf],
    ) -> Result<Vec<u8>, ProjectCompositionError> {
        let api = self.stack.api();
        let module =
            unsafe { global_named(api, "Module") }.ok_or(ProjectCompositionError::MissingModule)?;
        let vector_class = unsafe { (api.import)(c"Vector".as_ptr()) };
        if vector_class == api.nilObj {
            return Err(ProjectCompositionError::MissingVector);
        }
        let new_selector = unsafe { (api.intern)(c"new".as_ptr()) };
        let add_selector = unsafe { (api.intern)(c"add:".as_ptr()) };
        let mut source_paths =
            unsafe { (api.send)(vector_class, new_selector, std::ptr::null(), 0) };
        if source_paths == api.nilObj || send_is_error(api, source_paths) {
            return Err(ProjectCompositionError::AllocationRejected);
        }
        for path in paths {
            let bytes = path_bytes(path);
            let value = unsafe { (api.mkString)(bytes.as_ptr(), bytes.len()) };
            let updated = unsafe { (api.send)(source_paths, add_selector, [value].as_ptr(), 1) };
            if updated == api.nilObj || send_is_error(api, updated) {
                return Err(ProjectCompositionError::AllocationRejected);
            }
            source_paths = updated;
        }
        let selector = unsafe { (api.intern)(c"composeSources:".as_ptr()) };
        let result = unsafe { (api.send)(module, selector, [source_paths].as_ptr(), 1) };
        if send_is_error(api, result) {
            return Err(ProjectCompositionError::Rejected);
        }
        if result == api.nilObj || unsafe { (api.vtableOf)(result) } != api.string_vt {
            return Err(ProjectCompositionError::NonString);
        }
        let mut length = 0usize;
        let bytes = unsafe { (api.stringBytes)(result, &mut length) };
        if bytes.is_null() {
            return Err(ProjectCompositionError::NonString);
        }
        if length > MAX_SCRIPT_BYTES {
            return Err(ProjectCompositionError::TooLarge {
                limit: MAX_SCRIPT_BYTES,
            });
        }
        Ok(unsafe { std::slice::from_raw_parts(bytes, length) }.to_vec())
    }

    /// Install the host-owned `Project` bridge used by the authored LSP. The
    /// bridge delegates URI decoding, filesystem observation, and policy
    /// resolution to this runtime; it does not retain a second snapshot or
    /// project graph. The pointer is cleared when this HostRuntime drops.
    ///
    /// # Safety
    ///
    /// The caller must have prepared a live GC anchor and remain on the floor
    /// owner thread for all synchronous sends.
    pub unsafe fn install_project_bridge(&self) -> Result<(), ProjectBridgeError> {
        let api = self.stack.api();
        let runtime_pointer = self as *const HostRuntime as *mut HostRuntime;
        let owner = PROJECT_BRIDGE_RUNTIME.load(Ordering::Acquire);
        if !owner.is_null() && owner != runtime_pointer {
            return Err(ProjectBridgeError::AlreadyOwned);
        }
        let project = if let Some(project) = unsafe { global_named(api, "Project") } {
            project
        } else {
            let object = {
                let imported = unsafe { (api.import)(c"Object".as_ptr()) };
                if imported != api.nilObj {
                    imported
                } else {
                    unsafe { global_named(api, "Object") }
                        .ok_or(ProjectBridgeError::MissingObject)?
                }
            };
            let selector = unsafe { (api.intern)(c"clone".as_ptr()) };
            let project = unsafe { (api.send)(object, selector, std::ptr::null(), 0) };
            if project == api.nilObj || send_is_error(api, project) {
                return Err(ProjectBridgeError::CloneRejected);
            }
            project
        };
        let vtable = unsafe { (api.vtableOf)(project) };
        if vtable == api.nilObj {
            return Err(ProjectBridgeError::CloneRejected);
        }
        let snapshot_selector = unsafe { (api.intern)(c"snapshotForUri:".as_ptr()) };
        let path_selector = unsafe { (api.intern)(c"pathForUri:".as_ptr()) };
        unsafe {
            (api.method)(vtable, snapshot_selector, project_snapshot_for_uri_native);
            (api.method)(vtable, path_selector, project_path_for_uri_native);
        }
        let exported = unsafe { (api.exportObj)(c"Project".as_ptr(), project) };
        if exported == api.nilObj {
            return Err(ProjectBridgeError::GlobalRejected);
        }
        let globals = unsafe { (api.import)(c"Globals".as_ptr()) };
        if globals == api.nilObj {
            return Err(ProjectBridgeError::GlobalRejected);
        }
        let put_selector = unsafe { (api.intern)(c"at:put:".as_ptr()) };
        let name = unsafe { (api.intern)(c"Project".as_ptr()) };
        let result = unsafe { (api.send)(globals, put_selector, [name, project].as_ptr(), 2) };
        if send_is_error(api, result) {
            return Err(ProjectBridgeError::GlobalRejected);
        }
        if PROJECT_BRIDGE_RUNTIME
            .compare_exchange(
                std::ptr::null_mut(),
                runtime_pointer,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err_and(|owner| owner != runtime_pointer)
        {
            return Err(ProjectBridgeError::AlreadyOwned);
        }
        Ok(())
    }

    /// Dispatch one Birnam-owned CLI subcommand through the compiled `Cli`
    /// module. Rust supplies opaque argv and the installed resource roots;
    /// subcommand parsing, diagnostics, and module policy remain language
    /// owned. Project-aware LSP setup installs the scoped Rust `Project`
    /// bridge before dispatch and remains on the same host observation path.
    ///
    /// # Safety
    ///
    /// The caller must have called [`HostRuntime::prepare_execution`] with a
    /// live stack anchor and remain on the floor owner thread for all sends.
    pub unsafe fn run_cli(&self, args: &[OsString]) -> Result<(), CliError> {
        let is_lsp = args.iter().any(|argument| argument == "--lsp");
        if is_lsp {
            unsafe { self.load_project_policy() }.map_err(CliError::Policy)?;
            unsafe { self.install_project_bridge() }.map_err(CliError::Bridge)?;
        }
        unsafe { self.load_runtime_module("cli") }.map_err(CliError::Module)?;
        let api = self.stack.api();
        let Some(cli) = (unsafe { global_named(api, "Cli") }) else {
            return Err(CliError::MissingCli);
        };
        let values = unsafe { native_os_string_vector(api, args) }?;
        let source = path_bytes(&self.resources.source_dir);
        let rust_workspace = path_bytes(&self.resources.rust_workspace_dir);
        let guides = path_bytes(&self.resources.guide_dir);
        let selector = unsafe { (api.intern)(c"run:source:rustWorkspace:guides:".as_ptr()) };
        let result = unsafe {
            (api.send)(
                cli,
                selector,
                [
                    values,
                    (api.mkString)(source.as_ptr(), source.len()),
                    (api.mkString)(rust_workspace.as_ptr(), rust_workspace.len()),
                    (api.mkString)(guides.as_ptr(), guides.len()),
                ]
                .as_ptr(),
                4,
            )
        };
        if unsafe { (api.isSmallInt)(result) } == 0 {
            return Err(CliError::MalformedStatus);
        }
        let status = unsafe { (api.smallIntValue)(result) };
        if status == 0 {
            Ok(())
        } else {
            Err(CliError::Failed(status))
        }
    }

    /// Initialize a terminal REPL session and enter the runtime's running
    /// phase. Input/output is still owned by the self-hosted Repl module; Rust
    /// only drives its frozen lifecycle selectors.
    ///
    /// # Safety
    ///
    /// The caller must have called [`HostRuntime::prepare_execution`] with a
    /// live stack anchor and remain on the floor owner thread.
    pub unsafe fn start_repl(&self) -> Result<ReplSession, ReplError> {
        unsafe { self.start_repl_mode(false) }
    }

    /// Start a REPL after image recovery. The image's Module manager may be a
    /// restored application object, so ordinary `Module load:` is deliberately
    /// not called here; the host-capability path is the process-local recovery
    /// boundary and is used only when the restored Repl surface is absent.
    ///
    /// # Safety
    ///
    /// Same owner-thread and GC-anchor requirements as [`HostRuntime::start_repl`].
    pub unsafe fn start_repl_after_image(&self) -> Result<ReplSession, ReplError> {
        unsafe { self.start_repl_mode(true) }
    }

    unsafe fn start_repl_mode(&self, image_booted: bool) -> Result<ReplSession, ReplError> {
        // A normal boot can introduce Repl into G0. An image may already have
        // sealed its application generation, so recover the same process-local
        // host capability used by the Zig image path when ordinary loading is
        // rejected.
        let normal_load_rejected =
            !image_booted && unsafe { self.load_runtime_module("repl") }.is_err();
        let init_selector = unsafe { (self.stack.api().intern)(c"consoleHostInit".as_ptr()) };
        // Image recovery may leave the restored Globals/Module objects in a
        // partially rebuilt state where even reading `Repl` through the normal
        // namespace path can recurse. Always refresh this process-local tool
        // after an image; normal boots retain the selector-surface check.
        let stale_repl = if image_booted {
            true
        } else {
            unsafe { global_named(self.stack.api(), "Repl") }
                .map(|repl| unsafe {
                    !receiver_has_selector(self.stack.api(), repl, init_selector)
                })
                .unwrap_or(true)
        };
        if normal_load_rejected || stale_repl {
            unsafe { self.load_host_runtime_module("repl") }.map_err(ReplError::Module)?;
        }
        let api = self.stack.api();
        let Some(repl) = (unsafe { global_named(api, "Repl") }) else {
            return Err(ReplError::MissingRepl);
        };
        let initialized = unsafe { (api.send)(repl, init_selector, std::ptr::null(), 0) };
        if send_is_error(api, initialized) {
            return Err(ReplError::InitializationRejected);
        }
        let Some(generations) = (unsafe { global_named(api, "DispatchGeneration") }) else {
            return Err(ReplError::MissingDispatchGeneration);
        };
        let initialize_selector = unsafe { (api.intern)(c"initializeCurrent".as_ptr()) };
        let generation =
            unsafe { (api.send)(generations, initialize_selector, std::ptr::null(), 0) };
        if send_is_error(api, generation) {
            return Err(ReplError::GenerationRejected);
        }
        let phases = resolve_phase_hooks(&self.stack, &self.resources).map_err(ReplError::Phase)?;
        if !phases.enter_running() {
            return Err(ReplError::Phase(PhaseHookError::InvalidPhase));
        }
        let ready_selector = unsafe { (api.intern)(c"consoleHostReady".as_ptr()) };
        let ready = unsafe { (api.send)(repl, ready_selector, std::ptr::null(), 0) };
        if send_is_error(api, ready) {
            return Err(ReplError::ReadyRejected);
        }
        Ok(ReplSession {
            repl,
            phases,
            done: false,
            finished: false,
        })
    }

    /// Drive one Repl `hostStep`; the module owns input parsing and output
    /// policy. An evaluated value is returned as an Oop so callers can handle
    /// EvolutionIntent/quiescence exactly at the host boundary.
    ///
    /// # Safety
    ///
    /// `session` must belong to this runtime, and the caller must keep the GC
    /// anchor from `prepare_execution` live.
    pub unsafe fn repl_step(&self, session: &mut ReplSession) -> Result<ReplStep, ReplError> {
        if session.done || session.finished {
            return Ok(ReplStep::Done);
        }
        let api = self.stack.api();
        let step_selector = unsafe { (api.intern)(c"hostStep".as_ptr()) };
        let step = unsafe { (api.send)(session.repl, step_selector, std::ptr::null(), 0) };
        if step == api.nilObj {
            return Err(ReplError::MalformedStep);
        }
        let at_selector = unsafe { (api.intern)(c"at:".as_ptr()) };
        let tag = unsafe { (api.send)(step, at_selector, [(api.mkSmallInt)(0)].as_ptr(), 1) };
        if tag == api.nilObj {
            return Err(ReplError::MalformedStep);
        }
        let tag_name = unsafe { (api.symbolName)(tag) };
        if tag_name.is_null() {
            return Err(ReplError::MalformedStep);
        }
        if unsafe { CStr::from_ptr(tag_name) }.to_bytes() == b"done" {
            session.done = true;
            return Ok(ReplStep::Done);
        }
        if unsafe { CStr::from_ptr(tag_name) }.to_bytes() == b"evaluated" {
            let value = unsafe { (api.send)(step, at_selector, [(api.mkSmallInt)(1)].as_ptr(), 1) };
            return Ok(ReplStep::Evaluated(value));
        }
        Ok(ReplStep::Other)
    }

    /// Finish a REPL session and release its running-phase ownership.
    ///
    /// # Safety
    ///
    /// `session` must belong to this runtime and the caller must remain on the
    /// floor owner thread.
    pub unsafe fn finish_repl(&self, session: &mut ReplSession) {
        if session.finished {
            return;
        }
        let api = self.stack.api();
        let selector = unsafe { (api.intern)(c"consoleDone".as_ptr()) };
        unsafe { (api.send)(session.repl, selector, std::ptr::null(), 0) };
        session.finished = true;
    }

    /// Quiesce the runtime around an EvolutionIntent fulfillment.
    ///
    /// # Safety
    ///
    /// The caller must be on the owner thread and invoke `end_quiescence` only
    /// after a successful `begin_quiescence`.
    pub unsafe fn begin_quiescence(&self, session: &ReplSession) -> Result<(), ReplError> {
        if session.phases.begin_quiescence() {
            Ok(())
        } else {
            Err(ReplError::Phase(PhaseHookError::InvalidPhase))
        }
    }

    /// # Safety
    ///
    /// See [`HostRuntime::begin_quiescence`].
    pub unsafe fn end_quiescence(&self, session: &ReplSession) -> Result<(), ReplError> {
        if session.phases.end_quiescence() {
            Ok(())
        } else {
            Err(ReplError::Phase(PhaseHookError::InvalidPhase))
        }
    }

    /// Run the complete terminal REPL loop until `hostStep` reports `done`.
    /// Evolution intents are fulfilled only inside the Codegen quiescence
    /// phase, and result presentation remains delegated to Repl.
    ///
    /// # Safety
    ///
    /// `session` must belong to this runtime; the caller must have established
    /// a live GC anchor and remain on the floor owner thread.
    pub unsafe fn run_repl(&self, session: &mut ReplSession) -> Result<(), ReplError> {
        let api = self.stack.api();
        let Some(intents) = (unsafe { global_named(api, "EvolutionIntent") }) else {
            return Err(ReplError::MissingEvolutionIntent);
        };
        let intent_selector = unsafe { (api.intern)(c"isIntent:".as_ptr()) };
        let fulfill_selector = unsafe { (api.intern)(c"fulfill".as_ptr()) };
        let print_selector = unsafe { (api.intern)(c"printHostResult:".as_ptr()) };
        loop {
            match unsafe { self.repl_step(session) }? {
                ReplStep::Done => break,
                ReplStep::Other => {}
                ReplStep::Evaluated(mut value) => {
                    let is_intent =
                        unsafe { (api.send)(intents, intent_selector, [value].as_ptr(), 1) }
                            == api.trueObj;
                    if is_intent {
                        unsafe { self.begin_quiescence(session) }?;
                        value = unsafe { (api.send)(value, fulfill_selector, std::ptr::null(), 0) };
                        unsafe { self.end_quiescence(session) }?;
                    }
                    unsafe { (api.send)(session.repl, print_selector, [value].as_ptr(), 1) };
                }
            }
            unsafe { self.drain_isolates() };
        }
        unsafe { self.finish_repl(session) };
        Ok(())
    }

    /// Serve the socket REPL through the self-hosted `Repl serveOn:bind:`
    /// protocol. The socket module is loaded before Repl, matching the Zig
    /// host's runtime-tool dependency order.
    ///
    /// # Safety
    ///
    /// The runtime must remain on the selected floor's owner thread and the
    /// bind string must remain valid for the synchronous send.
    pub unsafe fn serve_socket(&self, port: u16, bind: &str) -> Result<(), SocketReplError> {
        unsafe { self.load_runtime_module("socket") }.map_err(SocketReplError::Module)?;
        unsafe { self.load_runtime_module("repl") }.map_err(SocketReplError::Module)?;
        let api = self.stack.api();
        let Some(repl) = (unsafe { global_named(api, "Repl") }) else {
            return Err(SocketReplError::MissingRepl);
        };
        let bind = CString::new(bind).map_err(|_| SocketReplError::ServeRejected)?;
        let bind = unsafe { (api.mkString)(bind.as_bytes().as_ptr(), bind.as_bytes().len()) };
        let selector = unsafe { (api.intern)(c"serveOn:bind:".as_ptr()) };
        let result = unsafe {
            (api.send)(
                repl,
                selector,
                [(api.mkSmallInt)(i64::from(port)), bind].as_ptr(),
                2,
            )
        };
        if send_is_error(api, result) {
            return Err(SocketReplError::ServeRejected);
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

    /// Recover one worker image, validate its durable generation, resolve the
    /// named receiver/selector through Globals/import and the live vtable
    /// chain, then invoke the retained isolate driver. The inherited channel
    /// descriptor remains borrowed; the caller owns close/error framing.
    ///
    /// # Safety
    ///
    /// `fd` must be a live inherited isolate channel, `image` must be readable
    /// by the selected runtime, and the caller must keep the GC anchor live on
    /// the floor owner thread for the complete operation.
    pub unsafe fn run_worker_image(
        &mut self,
        image: &Path,
        receiver_name: &str,
        selector_name: &str,
        fd: c_int,
    ) -> Result<(), WorkerStartupError> {
        unsafe { load_image(self, image, false) }.map_err(WorkerStartupError::Image)?;
        let api = self.stack.api();
        let Some(generations) = (unsafe { global_named(api, "DispatchGeneration") }) else {
            return Err(WorkerStartupError::MissingDispatchGeneration);
        };
        let current_selector = unsafe { (api.intern)(c"current".as_ptr()) };
        let current = unsafe { (api.send)(generations, current_selector, std::ptr::null(), 0) };
        if current == api.nilObj || send_is_error(api, current) {
            return Err(WorkerStartupError::InvalidDispatchGeneration);
        }
        let codegen =
            unsafe { global_named(api, "Codegen") }.ok_or(WorkerStartupError::MissingCodegen)?;
        let digest_selector = unsafe { (api.intern)(c"currentGenerationDigest".as_ptr()) };
        let digest = unsafe { (api.send)(codegen, digest_selector, std::ptr::null(), 0) };
        if digest == api.nilObj || unsafe { (api.vtableOf)(digest) } != api.string_vt {
            return Err(WorkerStartupError::InvalidGenerationDigest);
        }
        let mut digest_len = 0usize;
        let digest_bytes = unsafe { (api.stringBytes)(digest, &mut digest_len) };
        if digest_bytes.is_null() || digest_len != 64 {
            return Err(WorkerStartupError::InvalidGenerationDigest);
        }
        if !selector_has_arity_one(selector_name) {
            return Err(WorkerStartupError::WrongArity);
        }
        let receiver_name =
            CString::new(receiver_name).map_err(|_| WorkerStartupError::EmbeddedNul)?;
        let selector_name =
            CString::new(selector_name).map_err(|_| WorkerStartupError::EmbeddedNul)?;
        let globals =
            unsafe { global_named(api, "Globals") }.ok_or(WorkerStartupError::MissingGlobals)?;
        let receiver_symbol = unsafe { (api.intern)(receiver_name.as_ptr()) };
        let at_selector = unsafe { (api.intern)(c"at:".as_ptr()) };
        let mut receiver =
            unsafe { (api.send)(globals, at_selector, [receiver_symbol].as_ptr(), 1) };
        if receiver == api.nilObj {
            receiver = unsafe { (api.import)(receiver_name.as_ptr()) };
        }
        if receiver == api.nilObj {
            return Err(WorkerStartupError::ReceiverNotFound);
        }
        let selector = unsafe { (api.intern)(selector_name.as_ptr()) };
        if !unsafe { receiver_has_selector(api, receiver, selector) }
            && unsafe { replay_worker_methods(api) }
        {
            receiver = unsafe { (api.send)(globals, at_selector, [receiver_symbol].as_ptr(), 1) };
            if receiver == api.nilObj {
                receiver = unsafe { (api.import)(receiver_name.as_ptr()) };
            }
        }
        if receiver == api.nilObj || !unsafe { receiver_has_selector(api, receiver, selector) } {
            return Err(WorkerStartupError::SelectorNotFound);
        }
        unsafe { self.run_worker(fd, receiver, selector) }.map_err(WorkerStartupError::Driver)
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
    pub precise_gc: bool,
}

impl StartupPolicy {
    pub const fn from_flags(force_source: bool, skip_stdlib: bool) -> Self {
        Self::from_flags_with_precision(force_source, skip_stdlib, false)
    }

    pub const fn from_flags_with_precision(
        force_source: bool,
        skip_stdlib: bool,
        precise_gc: bool,
    ) -> Self {
        Self {
            force_source,
            skip_stdlib,
            precise_gc,
        }
    }

    pub fn from_environment() -> Self {
        Self::from_flags_with_precision(
            env::var_os("BIRNAM_FORCE_SOURCE").is_some(),
            env::var_os("BIRNAM_NO_STDLIB").is_some(),
            env::var_os("BIRNAM_GC_PRECISE").is_some(),
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

unsafe fn error_diagnostic(api: &IdApi, value: birnam_abi::Oop) -> Option<(String, String)> {
    if value == api.nilObj || !send_is_error(api, value) {
        return None;
    }
    let kind_selector = unsafe { (api.intern)(c"kind".as_ptr()) };
    let kind = unsafe { (api.send)(value, kind_selector, std::ptr::null(), 0) };
    let kind_name = unsafe { (api.symbolName)(kind) };
    if kind_name.is_null() {
        return None;
    }
    let kind = unsafe { CStr::from_ptr(kind_name) }
        .to_string_lossy()
        .into_owned();
    let message_selector = unsafe { (api.intern)(c"message".as_ptr()) };
    let message = unsafe { (api.send)(value, message_selector, std::ptr::null(), 0) };
    let message = unsafe { bridge_string(api, message) }.unwrap_or_default();
    Some((kind, message))
}

unsafe fn native_os_string_vector(
    api: &IdApi,
    args: &[OsString],
) -> Result<birnam_abi::Oop, CliError> {
    let vector_class = unsafe { (api.import)(c"Vector".as_ptr()) };
    if vector_class == api.nilObj {
        return Err(CliError::MissingVector);
    }
    let new_selector = unsafe { (api.intern)(c"new".as_ptr()) };
    let add_selector = unsafe { (api.intern)(c"add:".as_ptr()) };
    let mut vector = unsafe { (api.send)(vector_class, new_selector, std::ptr::null(), 0) };
    if vector == api.nilObj || send_is_error(api, vector) {
        return Err(CliError::ArgumentVectorRejected);
    }
    for (index, argument) in args.iter().enumerate() {
        let Some(argument) = argument.to_str() else {
            return Err(CliError::NonUtf8Argument(index));
        };
        let value = unsafe { (api.mkString)(argument.as_bytes().as_ptr(), argument.len()) };
        let updated = unsafe { (api.send)(vector, add_selector, [value].as_ptr(), 1) };
        if updated == api.nilObj || send_is_error(api, updated) {
            return Err(CliError::ArgumentVectorRejected);
        }
        vector = updated;
    }
    Ok(vector)
}

struct ProjectSnapshotBuilder<'a> {
    api: &'a IdApi,
    vector_class: birnam_abi::Oop,
    map_class: birnam_abi::Oop,
}

impl<'a> ProjectSnapshotBuilder<'a> {
    unsafe fn new(api: &'a IdApi) -> Result<Self, ProjectSnapshotError> {
        let vector_class = unsafe { (api.import)(c"Vector".as_ptr()) };
        let map_class = unsafe { (api.import)(c"Map".as_ptr()) };
        if vector_class == api.nilObj {
            return Err(ProjectSnapshotError::MissingVector);
        }
        if map_class == api.nilObj {
            return Err(ProjectSnapshotError::MissingMap);
        }
        Ok(Self {
            api,
            vector_class,
            map_class,
        })
    }

    unsafe fn new_vector(&self) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let selector = unsafe { (self.api.intern)(c"new".as_ptr()) };
        let value = unsafe { (self.api.send)(self.vector_class, selector, std::ptr::null(), 0) };
        if value == self.api.nilObj || send_is_error(self.api, value) {
            Err(ProjectSnapshotError::AllocationRejected)
        } else {
            Ok(value)
        }
    }

    unsafe fn new_map(&self) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let selector = unsafe { (self.api.intern)(c"new".as_ptr()) };
        let value = unsafe { (self.api.send)(self.map_class, selector, std::ptr::null(), 0) };
        if value == self.api.nilObj || send_is_error(self.api, value) {
            Err(ProjectSnapshotError::AllocationRejected)
        } else {
            Ok(value)
        }
    }

    unsafe fn add(
        &self,
        vector: birnam_abi::Oop,
        value: birnam_abi::Oop,
    ) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let selector = unsafe { (self.api.intern)(c"add:".as_ptr()) };
        let updated = unsafe { (self.api.send)(vector, selector, [value].as_ptr(), 1) };
        if updated == self.api.nilObj || send_is_error(self.api, updated) {
            Err(ProjectSnapshotError::AllocationRejected)
        } else {
            Ok(updated)
        }
    }

    unsafe fn put(
        &self,
        map: birnam_abi::Oop,
        key: &str,
        value: birnam_abi::Oop,
    ) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let key = unsafe { (self.api.mkString)(key.as_bytes().as_ptr(), key.len()) };
        let selector = unsafe { (self.api.intern)(c"at:put:".as_ptr()) };
        let updated = unsafe { (self.api.send)(map, selector, [key, value].as_ptr(), 2) };
        if updated == self.api.nilObj || send_is_error(self.api, updated) {
            Err(ProjectSnapshotError::AllocationRejected)
        } else {
            Ok(updated)
        }
    }

    unsafe fn string(&self, value: &str) -> birnam_abi::Oop {
        unsafe { (self.api.mkString)(value.as_bytes().as_ptr(), value.len()) }
    }

    unsafe fn bytes(&self, value: &[u8]) -> birnam_abi::Oop {
        unsafe { (self.api.mkString)(value.as_ptr(), value.len()) }
    }

    unsafe fn strings(&self, values: &[String]) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let mut vector = unsafe { self.new_vector()? };
        for value in values {
            vector = unsafe { self.add(vector, self.string(value))? };
        }
        Ok(vector)
    }

    unsafe fn previous_artifact(
        &self,
        value: &ProjectPreviousArtifact,
    ) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let mut item = unsafe { self.new_map()? };
        item = unsafe { self.put(item, "name", self.string(&value.name))? };
        item = unsafe { self.put(item, "path", self.string(&value.path))? };
        item = unsafe { self.put(item, "source", self.string(&value.source))? };
        item = unsafe { self.put(item, "sources", self.strings(&value.sources)?)? };
        item = unsafe { self.put(item, "sourceDigest", self.string(&value.source_digest))? };
        item = unsafe { self.put(item, "revisionDigest", self.string(&value.revision_digest))? };
        item = unsafe { self.put(item, "requires", self.strings(&value.requires)?)? };
        item = unsafe { self.put(item, "checksum", self.string(&value.checksum))? };
        if let Some(observed) = &value.observed_checksum {
            item = unsafe { self.put(item, "observedChecksum", self.string(observed))? };
        }
        Ok(item)
    }

    unsafe fn previous_native_artifact(
        &self,
        value: &ProjectPreviousNativeArtifact,
    ) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let mut item = unsafe { self.new_map()? };
        item = unsafe { self.put(item, "name", self.string(&value.name))? };
        item = unsafe { self.put(item, "path", self.string(&value.path))? };
        item = unsafe { self.put(item, "checksum", self.string(&value.checksum))? };
        if let Some(observed) = &value.observed_checksum {
            item = unsafe { self.put(item, "observedChecksum", self.string(observed))? };
        }
        Ok(item)
    }

    unsafe fn previous_build_record(
        &self,
        value: &ProjectBuildRecord,
    ) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let mut result = unsafe { self.new_map()? };
        result = unsafe { self.put(result, "projectRoot", self.string(&value.project_root))? };
        result = unsafe { self.put(result, "target", self.string(&value.target))? };
        result = unsafe { self.put(result, "profile", self.string(&value.profile))? };
        result = unsafe { self.put(result, "toolVersion", self.string(&value.tool_version))? };
        result = unsafe { self.put(result, "artifactAbi", self.string(&value.artifact_abi))? };
        result = unsafe {
            self.put(
                result,
                "resolutionDigest",
                self.string(&value.resolution_digest),
            )?
        };
        let mut artifacts = unsafe { self.new_vector()? };
        for artifact in &value.artifacts {
            artifacts = unsafe { self.add(artifacts, self.previous_artifact(artifact)?)? };
        }
        result = unsafe { self.put(result, "artifacts", artifacts)? };
        let mut native = unsafe { self.new_vector()? };
        for artifact in &value.native {
            native = unsafe { self.add(native, self.previous_native_artifact(artifact)?)? };
        }
        unsafe { self.put(result, "native", native) }
    }

    unsafe fn staged_artifact(
        &self,
        module: &ProjectSourceObservation,
        staged: &ProjectStagedArtifact,
    ) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let mut item = unsafe { self.new_map()? };
        item = unsafe { self.put(item, "name", self.string(&module.name))? };
        item = unsafe { self.put(item, "path", self.string(&staged.path))? };
        item = unsafe { self.put(item, "source", self.string(&module.path))? };
        item = unsafe { self.put(item, "sources", self.strings(&module.sources)?)? };
        item = unsafe { self.put(item, "sourceDigest", self.string(&module.source_digest))? };
        item = unsafe { self.put(item, "revisionDigest", self.string(&module.revision_digest))? };
        item = unsafe { self.put(item, "requires", self.strings(&module.requires)?)? };
        unsafe { self.put(item, "checksum", self.string(&staged.checksum)) }
    }

    unsafe fn staged_native_artifact(
        &self,
        value: &ProjectPreviousNativeArtifact,
    ) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let mut item = unsafe { self.new_map()? };
        item = unsafe { self.put(item, "name", self.string(&value.name))? };
        item = unsafe { self.put(item, "path", self.string(&value.path))? };
        unsafe { self.put(item, "checksum", self.string(&value.checksum)) }
    }

    unsafe fn artifact_module(
        &self,
        value: &ProjectSourceObservation,
    ) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let mut item = unsafe { self.new_map()? };
        item = unsafe { self.put(item, "name", self.string(&value.name))? };
        item = unsafe { self.put(item, "path", self.string(&value.path))? };
        item = unsafe { self.put(item, "sources", self.strings(&value.sources)?)? };
        item = unsafe { self.put(item, "sourceDigest", self.string(&value.source_digest))? };
        item = unsafe { self.put(item, "revisionDigest", self.string(&value.revision_digest))? };
        unsafe { self.put(item, "requires", self.strings(&value.requires)?) }
    }

    unsafe fn artifact_plan_input(
        &self,
        request: &ProjectArtifactPlanRequest<'_>,
    ) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let mut snapshot = unsafe { self.new_map()? };
        snapshot = unsafe {
            self.put(
                snapshot,
                "projectRoot",
                self.string(&request.resolution.project_root),
            )?
        };
        snapshot = unsafe { self.put(snapshot, "target", self.string(request.target))? };
        snapshot = unsafe { self.put(snapshot, "profile", self.string(request.profile))? };
        snapshot = unsafe { self.put(snapshot, "toolVersion", self.string(request.tool_version))? };
        let artifact_abi = String::from_utf8_lossy(birnam_artifact::ABI);
        snapshot = unsafe { self.put(snapshot, "artifactAbi", self.string(&artifact_abi))? };
        snapshot = unsafe {
            self.put(
                snapshot,
                "resolutionDigest",
                self.string(request.resolution_digest),
            )?
        };
        snapshot = unsafe { self.put(snapshot, "builtAt", self.string(request.built_at))? };
        let mut modules = unsafe { self.new_vector()? };
        for module in &request.resolution.modules {
            modules = unsafe { self.add(modules, self.artifact_module(module)?)? };
        }
        snapshot = unsafe { self.put(snapshot, "modules", modules)? };
        let mut wrapped = unsafe { self.new_vector()? };
        for value in request.wrapped {
            let mut item = unsafe { self.new_map()? };
            item = unsafe { self.put(item, "name", self.string(&value.name))? };
            wrapped = unsafe { self.add(wrapped, item)? };
        }
        snapshot = unsafe { self.put(snapshot, "wrapped", wrapped)? };
        let previous = if let Some(previous) = request.previous {
            unsafe { self.previous_build_record(previous)? }
        } else {
            self.api.nilObj
        };
        unsafe { self.put(snapshot, "previous", previous) }
    }

    unsafe fn wrapped(
        &self,
        values: &[ProjectWrappedObservation],
    ) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let mut result = unsafe { self.new_vector()? };
        for value in values {
            let mut item = unsafe { self.new_map()? };
            item = unsafe { self.put(item, "name", self.string(&value.name))? };
            let mut commands = unsafe { self.new_vector()? };
            for command in &value.build {
                commands = unsafe { self.add(commands, self.strings(command)?)? };
            }
            item = unsafe { self.put(item, "build", commands)? };
            result = unsafe { self.add(result, item)? };
        }
        Ok(result)
    }

    unsafe fn dependencies(
        &self,
        values: &[ProjectDependencyObservation],
    ) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let mut result = unsafe { self.new_vector()? };
        for value in values {
            let mut item = unsafe { self.new_map()? };
            item = unsafe { self.put(item, "name", self.string(&value.name))? };
            item = unsafe { self.put(item, "root", self.string(&value.root))? };
            result = unsafe { self.add(result, item)? };
        }
        Ok(result)
    }

    unsafe fn source(
        &self,
        value: &ProjectSourceObservation,
    ) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let mut item = unsafe { self.new_map()? };
        item = unsafe { self.put(item, "name", self.string(&value.name))? };
        item = unsafe { self.put(item, "path", self.string(&value.path))? };
        item = unsafe { self.put(item, "sources", self.strings(&value.sources)?)? };
        item = unsafe { self.put(item, "projectRoot", self.string(&value.project_root))? };
        item = unsafe { self.put(item, "source", self.bytes(&value.source))? };
        item = unsafe { self.put(item, "requires", self.strings(&value.requires)?)? };
        item = unsafe { self.put(item, "sourceDigest", self.string(&value.source_digest))? };
        item = unsafe { self.put(item, "revisionDigest", self.string(&value.revision_digest))? };
        Ok(item)
    }

    unsafe fn project(
        &self,
        value: &ProjectObservation,
    ) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let mut item = unsafe { self.new_map()? };
        item = unsafe { self.put(item, "root", self.string(&value.root))? };
        item = unsafe { self.put(item, "manifest", self.string(&value.manifest))? };
        item = unsafe { self.put(item, "name", self.string(&value.name))? };
        item = unsafe {
            self.put(
                item,
                "main",
                value
                    .main
                    .as_deref()
                    .map(|main| self.string(main))
                    .unwrap_or(self.api.nilObj),
            )?
        };
        item = unsafe { self.put(item, "loads", self.strings(&value.loads)?)? };
        let mut run = unsafe { self.new_map()? };
        run = unsafe { self.put(run, "repl", self.string(&value.run.repl))? };
        run = unsafe { self.put(run, "socketBind", self.string(&value.run.socket_bind))? };
        run = unsafe {
            self.put(
                run,
                "socketPort",
                (self.api.mkSmallInt)(value.run.socket_port),
            )?
        };
        item = unsafe { self.put(item, "run", run)? };
        item = unsafe { self.put(item, "wrapped", self.wrapped(&value.wrapped)?)? };
        item = unsafe {
            self.put(
                item,
                "dependencies",
                self.dependencies(&value.dependencies)?,
            )?
        };
        let mut sources = unsafe { self.new_vector()? };
        for source in &value.sources {
            sources = unsafe { self.add(sources, self.source(source)?)? };
        }
        unsafe { self.put(item, "sources", sources) }
    }

    unsafe fn snapshot(
        &self,
        value: &ProjectSnapshot,
    ) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let mut projects = unsafe { self.new_vector()? };
        for project in &value.projects {
            projects = unsafe { self.add(projects, self.project(project)?)? };
        }
        let mut snapshot = unsafe { self.new_map()? };
        snapshot = unsafe { self.put(snapshot, "root", self.string(&value.root))? };
        unsafe { self.put(snapshot, "projects", projects) }
    }
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
    let imported_name = CString::new(name).ok()?;
    let imported = unsafe { (api.import)(imported_name.as_ptr()) };
    if !imported.is_null() && imported != api.nilObj {
        return Some(imported);
    }
    let globals = unsafe { (api.import)(c"Globals".as_ptr()) };
    if globals.is_null() || globals == api.nilObj {
        return None;
    }
    let name = unsafe { intern_name(api, name) }?;
    let selector = unsafe { (api.intern)(c"at:".as_ptr()) };
    let value = unsafe { (api.send)(globals, selector, [name].as_ptr(), 1) };
    (value != api.nilObj).then_some(value)
}

fn selector_has_arity_one(selector: &str) -> bool {
    let colons = selector.bytes().filter(|byte| *byte == b':').count();
    if colons != 0 {
        colons == 1
    } else {
        selector
            .bytes()
            .next()
            .is_some_and(|byte| !byte.is_ascii_alphabetic())
    }
}

unsafe fn receiver_has_selector(
    api: &IdApi,
    receiver: birnam_abi::Oop,
    selector: birnam_abi::Oop,
) -> bool {
    let mut vtable = unsafe { (api.vtableOf)(receiver) };
    while !vtable.is_null() {
        let mut index = 0usize;
        loop {
            let candidate = unsafe { (api.vtableSelectorAt)(vtable, index) };
            if candidate.is_null() {
                break;
            }
            if candidate == selector {
                return true;
            }
            index = index.saturating_add(1);
        }
        vtable = unsafe { (api.vtableParent)(vtable) };
    }
    false
}

/// Recover a missing worker entry point from the image's durable method source
/// journal. Mapped code remains authoritative when it exposes the requested
/// selector; this fallback is only for a worker image whose code unit was
/// admitted but did not bind the target's selector in the live vtable. The
/// recovery transaction mirrors the ordinary image source path and is closed
/// before the worker driver is entered.
unsafe fn replay_worker_methods(api: &IdApi) -> bool {
    let Some(codegen) = (unsafe { global_named(api, "Codegen") }) else {
        return false;
    };
    let begin = unsafe { (api.intern)(c"beginImageDispatchRecovery".as_ptr()) };
    if unsafe { (api.send)(codegen, begin, std::ptr::null(), 0) } == api.nilObj {
        return false;
    }
    let replay = unsafe { (api.intern)(c"replayMethods".as_ptr()) };
    let replayed = unsafe { (api.send)(codegen, replay, std::ptr::null(), 0) };
    let replayed_any = unsafe { (api.isSmallInt)(replayed) } != 0
        && unsafe { (api.smallIntValue)(replayed) } > 0;
    let targets = unsafe { (api.intern)(c"deriveImageMethodTargets".as_ptr()) };
    unsafe { (api.send)(codegen, targets, std::ptr::null(), 0) };
    let aliens = unsafe { (api.intern)(c"replayAliens".as_ptr()) };
    unsafe { (api.send)(codegen, aliens, std::ptr::null(), 0) };
    let derive = unsafe { (api.intern)(c"deriveImageGlobals".as_ptr()) };
    unsafe { (api.send)(codegen, derive, std::ptr::null(), 0) };
    let end = unsafe { (api.intern)(c"endImageDispatchRecovery".as_ptr()) };
    let ended = unsafe { (api.send)(codegen, end, std::ptr::null(), 0) };
    replayed_any && ended != api.nilObj
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
    fn load_module_handle(
        &mut self,
        path: &Path,
    ) -> Result<NonNull<std::ffi::c_void>, ModuleError> {
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
        Ok(handle)
    }

    pub fn load_module(&mut self, path: &Path) -> Result<(), ModuleError> {
        self.load_module_handle(path).map(|_| ())
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
            let handle = self.load_module_handle(&path)?;
            if *name == "libbirnam_id_image"
                && path.file_name().and_then(|file| file.to_str())
                    == Some("libbirnam_id_image.dylib")
            {
                // The canonical Rust Image module must be allowed to publish
                // through the ordinary host path. Explicit `_rust` fixture
                // loads used by cross-floor tests do not take this branch.
                // SAFETY: `handle` is a live loader handle retained for the
                // process lifetime, and the symbol has the frozen identity
                // export signature when present.
                let rust_identity = unsafe {
                    (self.floor.api().dlsym)(
                        handle.as_ptr(),
                        c"birnam_image_rust_identity".as_ptr().cast(),
                    )
                };
                if !rust_identity.is_null() {
                    // Startup is single-threaded until this stack is returned;
                    // publish the canonical choice before any saveImage send.
                    unsafe { std::env::set_var("BIRNAM_RUST_IMAGE_CANONICAL", "rust") };
                }
            }
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

fn resolve_phase_hooks(
    stack: &LoadedStack,
    resources: &Resources,
) -> Result<PhaseHooks, PhaseHookError> {
    let path = resources.lib_dir.join("libbirnam_id_codegen.dylib");
    let path = CString::new(path_bytes(&path)).map_err(|_| PhaseHookError::EmbeddedNul)?;
    let api = stack.api();
    let handle = unsafe { (api.dlopen)(path.as_ptr(), RTLD_LAZY) };
    let Some(handle) = NonNull::new(handle) else {
        return Err(PhaseHookError::OpenFailed);
    };
    let lookup = |name: &'static std::ffi::CStr| {
        // SAFETY: the handle came from the selected floor's IdApi loader and
        // the static symbol name remains live for this synchronous lookup.
        unsafe { (api.dlsym)(handle.as_ptr(), name.as_ptr().cast()) }
    };
    let Some(enter_running) = NonNull::new(lookup(c"birnam_codegen_enter_running")) else {
        return Err(PhaseHookError::MissingSymbol(
            "birnam_codegen_enter_running",
        ));
    };
    let Some(begin_quiescence) = NonNull::new(lookup(c"birnam_codegen_begin_quiescence")) else {
        return Err(PhaseHookError::MissingSymbol(
            "birnam_codegen_begin_quiescence",
        ));
    };
    let Some(end_quiescence) = NonNull::new(lookup(c"birnam_codegen_end_quiescence")) else {
        return Err(PhaseHookError::MissingSymbol(
            "birnam_codegen_end_quiescence",
        ));
    };
    Ok(PhaseHooks {
        handle,
        enter_running: unsafe {
            std::mem::transmute::<*mut c_void, HostPhaseFn>(enter_running.as_ptr())
        },
        begin_quiescence: unsafe {
            std::mem::transmute::<*mut c_void, HostPhaseFn>(begin_quiescence.as_ptr())
        },
        end_quiescence: unsafe {
            std::mem::transmute::<*mut c_void, HostPhaseFn>(end_quiescence.as_ptr())
        },
    })
}

/// Start the Rust host through the same floor → native stack → MOP mirror →
/// self-hosted sequence as the Zig oracle. Resource resolution is kept outside
/// this function so callers can inspect or override it before any dylib is
/// opened.
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
        policy,
        codegen,
        self_hosted,
    })
}

/// Resolve production resources and start the Rust host using the environment
/// contract already shared with the Zig oracle executable.
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

    fn artifact_fixture_with_kind(
        kind: &[u8],
        identity: &[u8],
        requires: &[&[u8]],
    ) -> Vec<u8> {
        artifact_fixture_with_kind_and_native(kind, identity, requires, &[])
    }

    fn artifact_fixture_with_kind_and_native(
        kind: &[u8],
        identity: &[u8],
        requires: &[&[u8]],
        native_requires: &[&[u8]],
    ) -> Vec<u8> {
        let mut payload = Vec::new();
        artifact_field(kind, &mut payload);
        artifact_field(identity, &mut payload);
        artifact_field(birnam_artifact::ABI, &mut payload);
        artifact_field(birnam_artifact::TARGET, &mut payload);
        artifact_field(b"source", &mut payload);
        artifact_field(b"revision", &mut payload);
        artifact_list(requires, &mut payload);
        artifact_list(native_requires, &mut payload);
        artifact_list(&[], &mut payload);
        let mut output = Vec::new();
        output.extend_from_slice(birnam_artifact::DESCRIPTOR_MAGIC);
        output.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        output.extend_from_slice(&payload);
        output
    }

    fn artifact_fixture(identity: &[u8], requires: &[&[u8]]) -> Vec<u8> {
        artifact_fixture_with_kind(b"module", identity, requires)
    }

    fn decode_shared_fixture() -> Vec<(String, String, Vec<u8>)> {
        include_str!("../../../src/id/artifact-preflight-v1.txt")
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    return None;
                }
                let (key, encoded) = line.split_once('|').expect("shared fixture row");
                assert!(encoded.len() % 2 == 0, "shared fixture hex is even");
                let bytes = (0..encoded.len())
                    .step_by(2)
                    .map(|index| {
                        u8::from_str_radix(&encoded[index..index + 2], 16)
                            .expect("shared fixture hex byte")
                    })
                    .collect();
                let (case, filename) = key.split_once('/').expect("shared fixture path");
                Some((case.to_owned(), filename.to_owned(), bytes))
            })
            .collect()
    }

    fn write_shared_case(root: &std::path::Path, case: &str) -> std::path::PathBuf {
        std::fs::create_dir_all(root).expect("shared fixture directory creates");
        let rows = decode_shared_fixture()
            .into_iter()
            .filter(|(name, _, _)| name == case)
            .collect::<Vec<_>>();
        assert!(!rows.is_empty(), "shared fixture case exists");
        let mut root_path = None;
        for (_, filename, mut bytes) in rows {
            if case == "sentinel" && filename == "root.dylib" {
                if let Ok(path) = std::env::var("BIRNAM_PREFLIGHT_SENTINEL_DYLIB") {
                    let mut dylib = std::fs::read(path).expect("real sentinel dylib reads");
                    dylib.extend_from_slice(&bytes);
                    bytes = dylib;
                }
            }
            let path = root.join(&filename);
            std::fs::write(&path, bytes).expect("shared fixture writes");
            if filename == "root.dylib" {
                root_path = Some(path);
            }
        }
        root_path.expect("shared fixture root")
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
    fn build_record_observation_hashes_present_artifacts_and_leaves_missing_unobserved() {
        let root = std::env::temp_dir().join(format!(
            "birnam-host-build-record-observation-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("native")).expect("fixture directory creates");
        std::fs::write(root.join("module.dylib"), b"module-bytes").expect("module writes");
        std::fs::write(root.join("native/helper.dylib"), b"native-bytes").expect("native writes");
        let record = ProjectBuildRecord {
            project_root: "/tmp/project".to_owned(),
            target: "host".to_owned(),
            profile: "debug".to_owned(),
            tool_version: "dev".to_owned(),
            artifact_abi: "birnam-artifact-v1".to_owned(),
            resolution_digest: "resolution".to_owned(),
            built_at: "now".to_owned(),
            artifacts: vec![
                ProjectPreviousArtifact {
                    name: "module".to_owned(),
                    path: "module.dylib".to_owned(),
                    source: "module.birnam".to_owned(),
                    sources: vec!["module.birnam".to_owned()],
                    source_digest: "source".to_owned(),
                    revision_digest: "revision".to_owned(),
                    requires: Vec::new(),
                    checksum: "stale".to_owned(),
                    observed_checksum: None,
                },
                ProjectPreviousArtifact {
                    name: "missing".to_owned(),
                    path: "missing.dylib".to_owned(),
                    source: "missing.birnam".to_owned(),
                    sources: vec!["missing.birnam".to_owned()],
                    source_digest: "source".to_owned(),
                    revision_digest: "revision".to_owned(),
                    requires: Vec::new(),
                    checksum: "stale".to_owned(),
                    observed_checksum: Some("old-observation".to_owned()),
                },
            ],
            native: vec![ProjectPreviousNativeArtifact {
                name: "helper".to_owned(),
                path: "native/helper.dylib".to_owned(),
                checksum: "stale".to_owned(),
                observed_checksum: None,
            }],
        };

        let observed = observe_project_build_record(&root, record).expect("record observes");
        assert_eq!(
            observed.artifacts[0].observed_checksum,
            Some(String::from_utf8_lossy(&sha256_hex(b"module-bytes")).into_owned())
        );
        assert_eq!(observed.artifacts[1].observed_checksum, None);
        assert_eq!(
            observed.native[0].observed_checksum,
            Some(String::from_utf8_lossy(&sha256_hex(b"native-bytes")).into_owned())
        );
        std::fs::remove_dir_all(root).expect("fixture directory removes");
    }

    #[test]
    fn build_record_encoder_writes_snake_case_and_omits_observations() {
        let record = ProjectBuildRecord {
            project_root: "/tmp/project".to_owned(),
            target: "aarch64-macos".to_owned(),
            profile: "debug".to_owned(),
            tool_version: "dev".to_owned(),
            artifact_abi: String::from_utf8_lossy(birnam_artifact::ABI).into_owned(),
            resolution_digest: "digest".to_owned(),
            built_at: "now".to_owned(),
            artifacts: vec![ProjectPreviousArtifact {
                name: "app".to_owned(),
                path: "lib/libbirnam_app.dylib".to_owned(),
                source: "/tmp/project/src/app.birnam".to_owned(),
                sources: vec!["/tmp/project/src/app.birnam".to_owned()],
                source_digest: "source".to_owned(),
                revision_digest: "revision".to_owned(),
                requires: vec!["core".to_owned()],
                checksum: "checksum".to_owned(),
                observed_checksum: Some("observed".to_owned()),
            }],
            native: Vec::new(),
        };
        let encoded = String::from_utf8(encode_project_build_record(&record)).expect("TOML UTF-8");
        assert!(encoded.contains("artifact_abi = \""));
        assert!(encoded.contains("source_digest = \"source\""));
        assert!(encoded.contains("requires = [\"core\"]"));
        assert!(!encoded.contains("observedChecksum"));
        assert!(!encoded.contains("observed_checksum"));
    }

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
    fn build_record_observation_rejects_path_escape_before_reading() {
        let record = ProjectBuildRecord {
            project_root: "/tmp/project".to_owned(),
            target: "host".to_owned(),
            profile: "debug".to_owned(),
            tool_version: "dev".to_owned(),
            artifact_abi: "birnam-artifact-v1".to_owned(),
            resolution_digest: "resolution".to_owned(),
            built_at: "now".to_owned(),
            artifacts: vec![ProjectPreviousArtifact {
                name: "escape".to_owned(),
                path: "../outside.dylib".to_owned(),
                source: "escape.birnam".to_owned(),
                sources: vec!["escape.birnam".to_owned()],
                source_digest: "source".to_owned(),
                revision_digest: "revision".to_owned(),
                requires: Vec::new(),
                checksum: "stale".to_owned(),
                observed_checksum: None,
            }],
            native: Vec::new(),
        };
        assert!(matches!(
            observe_project_build_record(Path::new("/tmp/build"), record),
            Err(ProjectBuildRecordObservationError::InvalidPath(path))
                if path == Path::new("../outside.dylib")
        ));
    }

    #[test]
    fn artifact_plan_validation_preserves_policy_identity_and_action_pairing() {
        let module = ProjectSourceObservation {
            name: "app".to_owned(),
            path: "/tmp/project/src/app.birnam".to_owned(),
            sources: vec!["/tmp/project/src/app.birnam".to_owned()],
            project_root: "/tmp/project".to_owned(),
            source: b"App := Object clone".to_vec(),
            requires: vec!["core".to_owned()],
            source_digest: "source".to_owned(),
            revision_digest: "revision".to_owned(),
        };
        let resolution = ProjectResolutionObservation {
            project_root: "/tmp/project".to_owned(),
            manifest: "/tmp/project/birnam.toml".to_owned(),
            project_name: "demo".to_owned(),
            main: None,
            load_roots: vec!["/tmp/project/src".to_owned()],
            modules: vec![module.clone()],
        };
        let request = ProjectArtifactPlanRequest {
            resolution: &resolution,
            wrapped: &[],
            previous: None,
            target: "aarch64-macos",
            profile: "debug",
            tool_version: "dev",
            built_at: "now",
            resolution_digest: "digest",
        };
        let mut plan = ProjectArtifactPlan {
            project_root: "/tmp/project".to_owned(),
            target: "aarch64-macos".to_owned(),
            profile: "debug".to_owned(),
            tool_version: "dev".to_owned(),
            artifact_abi: String::from_utf8_lossy(birnam_artifact::ABI).into_owned(),
            built_at: Some("now".to_owned()),
            resolution_digest: "digest".to_owned(),
            artifacts: vec![ProjectArtifactPlanEntry {
                name: "app".to_owned(),
                path: "lib/libbirnam_app.dylib".to_owned(),
                action: "compile".to_owned(),
                source: module.path.clone(),
                sources: module.sources.clone(),
                source_digest: module.source_digest.clone(),
                revision_digest: module.revision_digest.clone(),
                requires: module.requires.clone(),
                checksum: None,
            }],
            publication: "publish".to_owned(),
        };
        validate_project_artifact_plan(&plan, &request).expect("policy plan validates");

        plan.artifacts[0].action = "reuse".to_owned();
        assert!(matches!(
            validate_project_artifact_plan(&plan, &request),
            Err(ProjectArtifactPlanValidationError::ChecksumMismatch {
                index: 0,
                action
            }) if action == "reuse"
        ));
        plan.artifacts[0].action = "compile".to_owned();
        plan.artifacts[0].path = "../escape.dylib".to_owned();
        assert!(matches!(
            validate_project_artifact_plan(&plan, &request),
            Err(ProjectArtifactPlanValidationError::ArtifactMismatch {
                index: 0,
                field: "path"
            })
        ));
    }

    #[test]
    fn compiled_artifact_validation_checks_descriptor_identity_and_metadata() {
        let module = ProjectSourceObservation {
            name: "root".to_owned(),
            path: "/tmp/project/root.birnam".to_owned(),
            sources: vec!["/tmp/project/root.birnam".to_owned()],
            project_root: "/tmp/project".to_owned(),
            source: Vec::new(),
            requires: vec!["dep".to_owned()],
            source_digest: "source".to_owned(),
            revision_digest: "revision".to_owned(),
        };
        let bytes = artifact_fixture(b"root", &[b"dep"]);
        validate_project_compiled_artifact(&module, &bytes)
            .expect("compiled descriptor matches module observation");

        let mut wrong_revision = module.clone();
        wrong_revision.revision_digest = "changed".to_owned();
        assert!(matches!(
            validate_project_compiled_artifact(&wrong_revision, &bytes),
            Err(ProjectCompiledArtifactValidationError::RevisionDigestMismatch)
        ));
    }

    #[test]
    fn wrapped_argument_expansion_replaces_all_frozen_placeholders() {
        let expanded = expand_project_wrapped_argument(
            "cc -o {output} -I{project} -L{lib} {build}/obj.o {unknown}",
            Path::new("/tmp/project"),
            Path::new("/tmp/build/native/libdemo.dylib"),
            Path::new("/tmp/build"),
            Path::new("/tmp/build/lib"),
        );
        assert_eq!(
            expanded,
            "cc -o /tmp/build/native/libdemo.dylib -I/tmp/project -L/tmp/build/lib /tmp/build/obj.o {unknown}"
        );
    }

    #[test]
    fn project_test_worker_parser_preserves_paths_and_profile() {
        let parsed = parse_project_test_worker_args(&[
            "/tmp/project",
            "release",
            "/tmp/project/tests/app.birnam",
        ])
        .expect("worker arguments parse");
        assert_eq!(parsed.project_root, Path::new("/tmp/project"));
        assert_eq!(parsed.profile, "release");
        assert_eq!(
            parsed.test_source,
            Path::new("/tmp/project/tests/app.birnam")
        );
        assert!(matches!(
            parse_project_test_worker_args(&["/tmp/project", "fast", "test.birnam"]),
            Err(ProjectTestWorkerParseError::InvalidProfile(value)) if value == "fast"
        ));
    }

    #[test]
    fn project_resolution_digest_is_stable_and_order_sensitive() {
        let module = |name: &str| ProjectSourceObservation {
            name: name.to_owned(),
            path: format!("/tmp/{name}.birnam"),
            sources: vec![format!("/tmp/{name}.birnam")],
            project_root: "/tmp/project".to_owned(),
            source: Vec::new(),
            requires: Vec::new(),
            source_digest: format!("{name}-source"),
            revision_digest: format!("{name}-revision"),
        };
        let first =
            project_resolution_digest("/tmp/project", b"name = \"one\"\n", &[module("one")]);
        assert_eq!(
            first,
            project_resolution_digest("/tmp/project", b"name = \"one\"\n", &[module("one")])
        );
        assert_ne!(
            first,
            project_resolution_digest("/tmp/project", b"name = \"two\"\n", &[module("one")])
        );
        assert_ne!(
            first,
            project_resolution_digest(
                "/tmp/project",
                b"name = \"one\"\n",
                &[module("two"), module("one")]
            )
        );
    }

    #[test]
    fn compile_diagnostic_parser_preserves_positions_and_fallback_text() {
        let parsed = parse_project_compile_diagnostic(
            "demo",
            "/tmp/demo.birnam",
            "12:7: unexpected selector\nfull detail",
        );
        assert_eq!(parsed.module, "demo");
        assert_eq!(parsed.path, "/tmp/demo.birnam");
        assert_eq!(parsed.line, Some(12));
        assert_eq!(parsed.column, Some(7));
        assert_eq!(parsed.message, "unexpected selector");
        assert_eq!(parsed.detail, "12:7: unexpected selector\nfull detail");

        let fallback = parse_project_compile_diagnostic("demo", "/tmp/demo.birnam", "not located");
        assert_eq!(fallback.line, None);
        assert_eq!(fallback.column, None);
        assert_eq!(fallback.message, "not located");
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
    fn project_policy_is_embedded_but_not_ordinary_bootstrap() {
        assert_eq!(PROJECT_POLICY_MODULE.name, "projectPolicy");
        assert!(!PROJECT_POLICY_MODULE.source.is_empty());
        assert_eq!(
            self_hosted_requires(PROJECT_POLICY_MODULE.source),
            ["stdlib"]
        );
        assert!(
            SELF_HOSTED_MODULES
                .iter()
                .all(|module| module.name != PROJECT_POLICY_MODULE.name)
        );
    }

    #[test]
    fn project_uri_path_boundary_matches_file_uri_contract() {
        assert_eq!(
            project_path_from_uri("file:///tmp/project%20name/src/app.birnam")
                .expect("file URI decodes"),
            PathBuf::from("/tmp/project name/src/app.birnam")
        );
        assert_eq!(
            project_path_from_uri("file://localhost/tmp/project/src/app.birnam")
                .expect("localhost file URI decodes"),
            PathBuf::from("/tmp/project/src/app.birnam")
        );
        assert_eq!(
            project_path_from_uri("src/app.birnam").expect("plain path passes through"),
            PathBuf::from("src/app.birnam")
        );
        assert_eq!(
            project_path_from_uri("file:///tmp/%GG").expect_err("invalid escape rejects"),
            ProjectUriError::InvalidEscape
        );
        assert_eq!(
            project_path_from_uri("https://example.test/app.birnam")
                .expect_err("non-file URI rejects"),
            ProjectUriError::UnsupportedScheme
        );
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

    #[test]
    fn dependency_closure_is_root_first_and_name_sorted() {
        let root = Path::new("/tmp/root");
        let nodes = vec![
            ProjectDependencyClosureNode {
                root: PathBuf::from("/tmp/root"),
                dependencies: vec![
                    ProjectDependencyObservation {
                        name: "zeta".to_owned(),
                        root: "/tmp/zeta".to_owned(),
                    },
                    ProjectDependencyObservation {
                        name: "alpha".to_owned(),
                        root: "/tmp/alpha".to_owned(),
                    },
                ],
            },
            ProjectDependencyClosureNode {
                root: PathBuf::from("/tmp/alpha"),
                dependencies: Vec::new(),
            },
            ProjectDependencyClosureNode {
                root: PathBuf::from("/tmp/zeta"),
                dependencies: vec![ProjectDependencyObservation {
                    name: "nested".to_owned(),
                    root: "/tmp/nested".to_owned(),
                }],
            },
            ProjectDependencyClosureNode {
                root: PathBuf::from("/tmp/nested"),
                dependencies: Vec::new(),
            },
        ];
        assert_eq!(
            project_dependency_closure_order(root, &nodes).expect("closure plans"),
            [
                PathBuf::from("/tmp/root"),
                PathBuf::from("/tmp/alpha"),
                PathBuf::from("/tmp/zeta"),
                PathBuf::from("/tmp/nested"),
            ]
        );
    }

    #[test]
    fn dependency_closure_rejects_missing_and_cyclic_nodes() {
        let missing = project_dependency_closure_order(
            Path::new("/tmp/root"),
            &[ProjectDependencyClosureNode {
                root: PathBuf::from("/tmp/root"),
                dependencies: vec![ProjectDependencyObservation {
                    name: "missing".to_owned(),
                    root: "/tmp/missing".to_owned(),
                }],
            }],
        );
        assert!(matches!(
            missing,
            Err(ProjectDependencyClosureError::Missing { name, .. }) if name == "missing"
        ));

        let cycle = project_dependency_closure_order(
            Path::new("/tmp/root"),
            &[
                ProjectDependencyClosureNode {
                    root: PathBuf::from("/tmp/root"),
                    dependencies: vec![ProjectDependencyObservation {
                        name: "dep".to_owned(),
                        root: "/tmp/dep".to_owned(),
                    }],
                },
                ProjectDependencyClosureNode {
                    root: PathBuf::from("/tmp/dep"),
                    dependencies: vec![ProjectDependencyObservation {
                        name: "root".to_owned(),
                        root: "/tmp/root".to_owned(),
                    }],
                },
            ],
        );
        assert_eq!(
            cycle,
            Err(ProjectDependencyClosureError::Cycle(PathBuf::from(
                "/tmp/root",
            )))
        );
    }

    #[test]
    fn composition_plan_preserves_entry_order_and_rejects_duplicate_parts() {
        let root = Path::new("/tmp/project");
        assert_eq!(
            project_composition_paths(root, "src/main.birnam", &["src/part.birnam".to_owned()])
                .expect("composition paths validate"),
            vec![root.join("src/main.birnam"), root.join("src/part.birnam")]
        );
        assert_eq!(
            project_composition_paths(root, "src/main.birnam", &["src/main.birnam".to_owned()]),
            Err(ProjectCompositionPlanError::EntryListedAsPart)
        );
        assert_eq!(
            project_composition_paths(
                root,
                "src/main.birnam",
                &["src/part.birnam".to_owned(), "src/part.birnam".to_owned()]
            ),
            Err(ProjectCompositionPlanError::DuplicatePart(
                "src/part.birnam".to_owned()
            ))
        );
    }

    #[test]
    fn observation_validation_rejects_duplicate_providers_and_parts() {
        let module = |name: &str, path: &str, sources: &[&str]| ProjectSourceObservation {
            name: name.to_owned(),
            path: path.to_owned(),
            sources: sources.iter().map(|source| (*source).to_owned()).collect(),
            project_root: "/tmp/project".to_owned(),
            source: Vec::new(),
            requires: Vec::new(),
            source_digest: "source".to_owned(),
            revision_digest: "revision".to_owned(),
        };
        assert!(matches!(
            validate_project_observations(&[
                module("one", "/tmp/one", &["/tmp/one"]),
                module("one", "/tmp/two", &["/tmp/two"]),
            ]),
            Err(ProjectObservationError::DuplicateModuleProvider { .. })
        ));
        assert_eq!(
            validate_project_observations(&[
                module("one", "/tmp/one", &["/tmp/shared"]),
                module("two", "/tmp/two", &["/tmp/shared"]),
            ]),
            Err(ProjectObservationError::DuplicateModulePart(PathBuf::from(
                "/tmp/shared"
            )))
        );
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
                precise_gc: false,
            }
        );
        assert_eq!(
            StartupPolicy::from_flags(false, true),
            StartupPolicy {
                force_source: false,
                skip_stdlib: true,
                precise_gc: false,
            }
        );
        assert!(StartupPolicy::from_flags_with_precision(false, false, true).precise_gc);
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
    fn worker_parser_preserves_channel_and_named_dispatch_operands() {
        assert!(selector_has_arity_one("handle:"));
        assert!(selector_has_arity_one("+"));
        assert!(!selector_has_arity_one("handle"));
        assert!(!selector_has_arity_one("a:b:"));
        assert_eq!(
            parse_worker_args(&["saved.image", "Receiver", "handle:", "--channel-fd", "17",])
                .expect("worker operands parse"),
            WorkerOptions {
                image: PathBuf::from("saved.image"),
                receiver: "Receiver".to_owned(),
                selector: "handle:".to_owned(),
                channel_fd: 17,
            }
        );
        assert_eq!(
            parse_worker_args::<&str>(&["image", "receiver"]),
            Err(WorkerParseError::Arity {
                expected: 5,
                actual: 2,
            })
        );
        assert_eq!(
            parse_worker_args(&["image", "receiver", "handle:", "--wrong", "17"]),
            Err(WorkerParseError::MissingChannelFlag("--wrong".to_owned()))
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
    fn repl_step_tags_keep_done_evaluated_and_other_distinct() {
        assert_ne!(ReplStep::Done, ReplStep::Other);
        assert_ne!(ReplStep::Evaluated(std::ptr::null_mut()), ReplStep::Done);
    }

    #[test]
    fn socket_repl_errors_remain_distinct_from_module_errors() {
        assert_ne!(SocketReplError::MissingRepl, SocketReplError::ServeRejected);
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
    fn artifact_preflight_path_only_root_does_not_infer_filename_identity() {
        let root = std::env::temp_dir().join(format!(
            "birnam-host-preflight-path-only-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("fixture directory creates");
        let path = root.join("unrelated-filename.dylib");
        std::fs::write(&path, artifact_fixture(b"actual", &[])).expect("fixture writes");
        let plan = preflight_artifact(&path, None).expect("path-only root preflights");
        assert_eq!(plan.entries[0].identity, b"actual");
        std::fs::remove_dir_all(&root).expect("fixture directory removes");
    }

    #[test]
    fn artifact_preflight_rejects_sibling_identity_mismatch() {
        let root = std::env::temp_dir().join(format!(
            "birnam-host-preflight-impostor-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("fixture directory creates");
        let root_path = root.join("root.dylib");
        let dependency_path = root.join("libbirnam_dep.dylib");
        std::fs::write(&root_path, artifact_fixture(b"root", &[b"dep"]))
            .expect("root fixture writes");
        std::fs::write(&dependency_path, artifact_fixture(b"impostor", &[]))
            .expect("impostor fixture writes");
        assert!(matches!(
            preflight_artifact(&root_path, Some(b"root")),
            Err(ArtifactPreflightError::IdentityMismatch { .. })
        ));
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
        let error = preflight_artifact(&root_path, Some(b"root"))
            .expect_err("missing sibling must reject as a dependency failure");
        assert!(
            format!("{error:?}").starts_with("artifactDependencyInvalid:"),
            "missing sibling diagnostic drifted: {error:?}"
        );
        std::fs::remove_dir_all(&root).expect("fixture directory removes");
    }

    #[test]
    fn artifact_preflight_rejects_wrong_kind_before_dependency_walk() {
        let root = std::env::temp_dir().join(format!(
            "birnam-host-preflight-kind-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("fixture directory creates");
        let root_path = root.join("libbirnam_root.dylib");
        std::fs::write(
            &root_path,
            artifact_fixture_with_kind(b"image", b"root", &[b"missing"]),
        )
        .expect("root fixture writes");
        assert!(matches!(
            preflight_artifact(&root_path, Some(b"root")),
            Err(ArtifactPreflightError::KindMismatch { .. })
        ));
        std::fs::remove_dir_all(&root).expect("fixture directory removes");
    }

    #[test]
    fn artifact_preflight_rejects_malformed_descriptor_bytes() {
        let root = std::env::temp_dir().join(format!(
            "birnam-host-preflight-malformed-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("fixture directory creates");
        let path = root.join("candidate.dylib");
        std::fs::write(&path, b"not a descriptor").expect("malformed fixture writes");
        assert!(matches!(
            preflight_artifact(&path, None),
            Err(ArtifactPreflightError::Parse { .. })
        ));
        std::fs::remove_dir_all(&root).expect("fixture directory removes");
    }

    #[test]
    fn artifact_preflight_rejects_duplicate_descriptor_providers() {
        let root = std::env::temp_dir().join(format!(
            "birnam-host-preflight-duplicate-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("fixture directory creates");
        let path = root.join("candidate.dylib");
        let mut bytes = artifact_fixture(b"candidate", &[]);
        bytes.extend_from_slice(&artifact_fixture(b"candidate", &[]));
        std::fs::write(&path, bytes).expect("duplicate fixture writes");
        assert!(matches!(
            preflight_artifact(&path, None),
            Err(ArtifactPreflightError::Parse { .. })
        ));
        std::fs::remove_dir_all(&root).expect("fixture directory removes");
    }

    #[test]
    fn artifact_preflight_rejects_dependency_path_escape() {
        let root = std::env::temp_dir().join(format!(
            "birnam-host-preflight-path-escape-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("fixture directory creates");
        let path = root.join("candidate.dylib");
        std::fs::write(&path, artifact_fixture(b"candidate", &[b"../escape"]))
            .expect("path escape fixture writes");
        assert!(matches!(
            preflight_artifact(&path, None),
            Err(ArtifactPreflightError::InvalidDependencyName { .. })
        ));
        std::fs::remove_dir_all(&root).expect("fixture directory removes");
    }

    #[test]
    fn artifact_preflight_rejects_missing_native_requirement() {
        let root = std::env::temp_dir().join(format!(
            "birnam-host-preflight-native-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("fixture directory creates");
        let path = root.join("candidate.dylib");
        std::fs::write(
            &path,
            artifact_fixture_with_kind_and_native(
                b"module",
                b"candidate",
                &[],
                &[b"libmissing-native.dylib"],
            ),
        )
        .expect("native requirement fixture writes");
        assert!(matches!(
            preflight_artifact(&path, None),
            Err(ArtifactPreflightError::NativeRequirementMissing { .. })
        ));
        std::fs::remove_dir_all(&root).expect("fixture directory removes");
    }

    #[test]
    fn artifact_preflight_sentinel_bytes_are_never_activated() {
        let root = std::env::temp_dir().join(format!(
            "birnam-host-preflight-sentinel-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("fixture directory creates");
        let path = root.join("candidate.dylib");
        let mut bytes = b"sentinel initializer: MUST NOT EXECUTE\n".to_vec();
        bytes.extend_from_slice(&artifact_fixture(b"sentinel", &[]));
        std::fs::write(&path, &bytes).expect("sentinel fixture writes");
        let target = root.join("existing-target");
        std::fs::write(&target, b"prior-publication").expect("target writes");
        let plan = preflight_artifact(&path, Some(b"sentinel"))
            .expect("inert sentinel descriptor preflights");
        assert_eq!(plan.entries.len(), 1);
        assert_eq!(
            std::fs::read(&target).expect("target remains readable"),
            b"prior-publication"
        );
        std::fs::remove_dir_all(&root).expect("fixture directory removes");
    }

    #[test]
    fn artifact_preflight_shared_fixture_matrix_matches_zig_cases() {
        let cases = [
            "path-only-root",
            "supplied-identity-mismatch",
            "wrong-kind-before-walk",
            "sibling-identity-mismatch",
            "missing-sibling",
            "ordered-valid-closure",
            "sentinel",
        ];
        for case in cases {
            let root = std::env::temp_dir().join(format!(
                "birnam-host-shared-preflight-{case}-{}",
                std::process::id()
            ));
            let root_path = write_shared_case(&root, case);
            let existing_target = root.join("existing-target");
            std::fs::write(&existing_target, b"prior-publication").expect("target writes");
            let result = match case {
                "path-only-root" => preflight_artifact(&root_path, None),
                "supplied-identity-mismatch" => {
                    preflight_artifact(&root_path, Some(b"expectedIdentity"))
                }
                "wrong-kind-before-walk" => preflight_artifact(&root_path, Some(b"candidate")),
                "sibling-identity-mismatch" => preflight_artifact(&root_path, None),
                "missing-sibling" => preflight_artifact(&root_path, None),
                "ordered-valid-closure" => preflight_artifact(&root_path, Some(b"root")),
                "sentinel" => preflight_artifact(&root_path, Some(b"sentinel")),
                _ => unreachable!(),
            };
            match case {
                "path-only-root" | "ordered-valid-closure" | "sentinel" => {
                    let plan = result.expect("shared fixture valid case");
                    let identities = plan
                        .entries
                        .iter()
                        .map(|entry| entry.identity.as_slice())
                        .collect::<Vec<_>>();
                    let expected = match case {
                        "path-only-root" => vec![b"actualIdentity".as_slice()],
                        "ordered-valid-closure" => {
                            vec![b"dependency".as_slice(), b"root".as_slice()]
                        }
                        "sentinel" => vec![b"sentinel".as_slice()],
                        _ => unreachable!(),
                    };
                    assert_eq!(identities, expected, "shared fixture plan drifted: {case}");
                }
                "supplied-identity-mismatch" | "sibling-identity-mismatch" => {
                    assert!(matches!(
                        result,
                        Err(ArtifactPreflightError::IdentityMismatch { .. })
                    ));
                }
                "wrong-kind-before-walk" => {
                    assert!(matches!(
                        result,
                        Err(ArtifactPreflightError::KindMismatch { .. })
                    ));
                }
                "missing-sibling" => {
                    assert!(matches!(
                        result,
                        Err(ArtifactPreflightError::DependencyInvalid { .. })
                    ));
                    assert_eq!(
                        std::fs::read(&existing_target).expect("target remains readable"),
                        b"prior-publication"
                    );
                }
                _ => unreachable!(),
            }
            std::fs::remove_dir_all(&root).expect("shared fixture directory removes");
        }
    }
}
