//! Library/resource startup, worker and REPL mechanics.
//! The parent keeps HostRuntime construction shape and its address-fenced Drop.
use super::*;
#[path = "resources.rs"]
mod resources;
pub use resources::{Overrides, Resources, resolve_from_exe_dir, resolve};

#[path = "interaction.rs"]
mod interaction;
pub use interaction::{CliError, ReplError, ReplSession, ReplStep, SocketReplError, WorkerError, WorkerStartupError};




/// Return the frozen `<arch>-<os>` build target identity used by the project
/// adapter and artifact policy.
pub fn host_target_identity() -> String {
    format!("{}-{}", env::consts::ARCH, env::consts::OS)
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
    CompilationRejected(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageLoadOutcome {
    Mapped,
    SourceRecovery,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageError {
    MissingSystem,
    InvalidHeader,
    VersionMismatch { expected: u32, observed: u32 },
    LoadRejected,
    UnexpectedStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageEntryError {
    MissingDispatchGeneration,
    GenerationRejected,
    Phase(PhaseHookError),
    EvaluationRejected,
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
    pub native: Vec<PathBuf>,
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

/// Recover only the embedded image from a baked executable without reading the
/// runtime prefix into memory. A file without a valid trailer answers `None`.
pub fn read_baked_image(path: &Path) -> io::Result<Option<Vec<u8>>> {
    let mut file = std::fs::File::open(path)?;
    let file_len = file.metadata()?.len();
    if file_len < BAKE_TRAILER_LEN as u64 {
        return Ok(None);
    }
    file.seek(SeekFrom::End(-(BAKE_TRAILER_LEN as i64)))?;
    let mut trailer = [0u8; BAKE_TRAILER_LEN];
    file.read_exact(&mut trailer)?;
    if &trailer[8..] != BAKE_MAGIC {
        return Ok(None);
    }
    let image_len = u64::from_le_bytes(trailer[..8].try_into().expect("bake image length"));
    let prefix_len = file_len
        .checked_sub(BAKE_TRAILER_LEN as u64)
        .and_then(|length| length.checked_sub(image_len));
    let Some(prefix_len) = prefix_len else {
        return Ok(None);
    };
    if image_len == 0 || image_len > usize::MAX as u64 {
        return Ok(None);
    }
    file.seek(SeekFrom::Start(prefix_len))?;
    let mut image = vec![0u8; image_len as usize];
    file.read_exact(&mut image)?;
    Ok(Some(image))
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

pub(super) fn read_artifact_bytes(path: &Path) -> Result<Vec<u8>, ArtifactPreflightError> {
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
    native: Vec<PathBuf>,
    native_seen: HashSet<PathBuf>,
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
            let native_path = directory.join(requirement_path);
            if std::fs::metadata(&native_path).is_err() {
                return Err(ArtifactPreflightError::NativeRequirementMissing {
                    path: path.to_path_buf(),
                    requirement: requirement.to_vec(),
                });
            }
            if self.native_seen.insert(native_path.clone()) {
                self.native.push(native_path);
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
                    // typed diagnostic instead of collapsing it
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
/// activation. This is the Rust-host validation seam for the preflight
/// helper: it consumes the same pointer-free descriptor parser and performs no
/// IdApi allocation, module load, or initializer call.
pub fn preflight_artifact(
    path: &Path,
    expected_identity: Option<&[u8]>,
) -> Result<ArtifactPlan, ArtifactPreflightError> {
    let mut preflight = ArtifactPreflight {
        entries: Vec::new(),
        native: Vec::new(),
        native_seen: HashSet::new(),
        visiting: HashSet::new(),
        inspected: HashSet::new(),
    };
    preflight.walk(path, expected_identity)?;
    Ok(ArtifactPlan {
        entries: preflight.entries,
        native: preflight.native,
    })
}

/// Activate one artifact after its complete closure has already passed
/// [`preflight_artifact`]. This is the execution half of the artifact module's
/// interface; callers must retain the successful plan until activation ends.
///
/// # Safety
///
/// The runtime must remain on the selected floor's owner thread, the caller
/// must establish a valid GC stack bottom, and `entry` must belong to a
/// successfully preflighted plan whose files have not changed.
pub unsafe fn activate_preflighted_artifact(
    runtime: &HostRuntime,
    entry: &ArtifactEntry,
) -> Result<(), ArtifactActivationError> {
    let api = runtime.stack.api();
    let Some(module_manager) = (unsafe { global_named(api, runtime.stack.imports(), "Module") }) else {
        return Err(ArtifactActivationError::MissingModuleManager);
    };
    let identity = CString::new(entry.identity.as_slice())
        .map_err(|_| ArtifactActivationError::ActivationRejected)?;
    let identity = unsafe { (api.intern)(identity.as_ptr()) };
    let is_loaded = unsafe { (api.intern)(c"isLoaded:".as_ptr()) };
    if unsafe { (api.send)(module_manager, is_loaded, [identity].as_ptr(), 1) } == api.trueObj {
        return Ok(());
    }
    let path = &entry.path;
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let directory_bytes = path_bytes(directory);
    let directory_value =
        unsafe { (birnam_abi::string::make)(directory_bytes.as_ptr(), directory_bytes.len()) };
    let lib_dir_selector = unsafe { (api.intern)(c"libDir:".as_ptr()) };
    unsafe {
        (api.send)(
            module_manager,
            lib_dir_selector,
            [directory_value].as_ptr(),
            1,
        )
    };
    let alien = unsafe { birnam_abi::namespace_import::import(runtime.stack.imports(), c"Alien".as_ptr()) };
    if alien != api.nilObj {
        let selector = unsafe { (api.intern)(c"libraryDir:".as_ptr()) };
        unsafe { (api.send)(alien, selector, [directory_value].as_ptr(), 1) };
    }
    let path_bytes = path_bytes(path);
    let path_value = unsafe { (birnam_abi::string::make)(path_bytes.as_ptr(), path_bytes.len()) };
    let selector = unsafe { (api.intern)(c"load:".as_ptr()) };
    let result = unsafe { (api.send)(runtime.codegen, selector, [path_value].as_ptr(), 1) };
    if result == api.nilObj || send_is_error(api, runtime.stack.presence(), result) {
        return Err(ArtifactActivationError::ActivationRejected);
    }
    let mark_loaded = unsafe { (api.intern)(c"markLoaded:".as_ptr()) };
    let marked = unsafe { (api.send)(module_manager, mark_loaded, [identity].as_ptr(), 1) };
    if marked == api.nilObj || send_is_error(api, runtime.stack.presence(), marked) {
        return Err(ArtifactActivationError::ActivationRejected);
    }
    Ok(())
}

/// Preflight and activate an artifact closure dependency-first. The host uses
/// the checked plan directly, while Codegen remains the installation authority
/// and the Birnam Module manager retains loaded-state bookkeeping.
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
    let plan =
        preflight_artifact(path, expected_identity).map_err(ArtifactActivationError::Preflight)?;
    for entry in &plan.entries {
        unsafe { activate_preflighted_artifact(runtime, entry)? };
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
    let codegen = unsafe { birnam_abi::namespace_import::import(runtime.stack.imports(), c"Codegen".as_ptr()) };
    if codegen == api.nilObj {
        return Err(ScriptError::MissingCodegen);
    }
    let source_value = unsafe { (birnam_abi::string::make)(source.as_ptr(), source.len()) };
    let path_bytes = path_bytes(path);
    let path_value = unsafe { (birnam_abi::string::make)(path_bytes.as_ptr(), path_bytes.len()) };
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
/// This is the Rust-host implementation of the private
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
    initial_generation_resumable: bool,
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
    let codegen = unsafe { birnam_abi::namespace_import::import(runtime.stack.imports(), c"Codegen".as_ptr()) };
    if codegen == api.nilObj {
        return Err(CompileModuleError::MissingCodegen);
    }
    let source_value = unsafe { (birnam_abi::string::make)(source.as_ptr(), source.len()) };
    let output_bytes = path_bytes(output_path);
    let output_value = unsafe { (birnam_abi::string::make)(output_bytes.as_ptr(), output_bytes.len()) };
    let source_bytes = path_bytes(source_path);
    let source_path_value = unsafe { (birnam_abi::string::make)(source_bytes.as_ptr(), source_bytes.len()) };
    let result = if initial_generation_resumable {
        let record_selector = unsafe { (api.intern)(c"recordModuleSource:named:".as_ptr()) };
        for module in std::iter::once(&MODULE_MANAGER).chain(SELF_HOSTED_MODULES.iter()) {
            if module.name == "stacklessControl" {
                continue;
            }
            let name = CString::new(module.name).map_err(|_| {
                CompileModuleError::CompilationRejected("invalid module name".to_owned())
            })?;
            let name = unsafe { (api.intern)(name.as_ptr()) };
            let source = unsafe { (birnam_abi::string::make)(module.source.as_ptr(), module.source.len()) };
            let recorded =
                unsafe { (api.send)(codegen, record_selector, [source, name].as_ptr(), 2) };
            if recorded == api.nilObj || send_is_error(api, runtime.stack.presence(), recorded) {
                return Err(CompileModuleError::CompilationRejected(
                    "initial generation source registration failed".to_owned(),
                ));
            }
        }
        let selection = unsafe { (api.intern)(c"resumableRequired".as_ptr()) };
        let selector = unsafe { (api.intern)(c"compileInitialModule:output:selection:".as_ptr()) };
        unsafe {
            (api.send)(
                codegen,
                selector,
                [source_value, output_value, selection].as_ptr(),
                3,
            )
        }
    } else {
        let selector = unsafe { (api.intern)(c"compile:output:from:".as_ptr()) };
        unsafe {
            (api.send)(
                codegen,
                selector,
                [source_value, output_value, source_path_value].as_ptr(),
                3,
            )
        }
    };
    if result == api.nilObj || send_is_error(api, runtime.stack.presence(), result) {
        let selector = unsafe { (api.intern)(c"lastError".as_ptr()) };
        let diagnostic = unsafe { (api.send)(codegen, selector, std::ptr::null(), 0) };
        let diagnostic =
            if diagnostic != api.nilObj && unsafe { (api.vtableOf)(diagnostic) } == unsafe { birnam_abi::string::behavior() } {
                let mut length = 0usize;
                let bytes = unsafe { (birnam_abi::string::bytes)(diagnostic, &mut length) };
                if bytes.is_null() {
                    "compilation rejected".to_owned()
                } else {
                    String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(bytes, length) })
                        .into_owned()
                }
            } else {
                "compilation rejected".to_owned()
            };
        return Err(CompileModuleError::CompilationRejected(diagnostic));
    }
    Ok(())
}

fn validate_image_header(fixed_header: &[u8]) -> Result<(), ImageError> {
    if fixed_header.len() < 12 || fixed_header[..8] != *b"BIRNAMIM" {
        return Err(ImageError::InvalidHeader);
    }
    let observed = u32::from_le_bytes(
        fixed_header[8..12]
            .try_into()
            .expect("validated fixed image version"),
    );
    if observed != 8 {
        return Err(ImageError::VersionMismatch {
            expected: 8,
            observed,
        });
    }
    Ok(())
}

/// Load an image through the Birnam `System` protocol. A true answer means
/// compiled code was mapped; false means the runtime rebuilt the image's exact
/// portable programs and materialization selection.
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
    let mut fixed_header = [0u8; 12];
    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut fixed_header))
        .map_err(|_| ImageError::InvalidHeader)?;
    validate_image_header(&fixed_header)?;
    let api_ptr = runtime.stack.api() as *const IdApi;
    // SAFETY: the retained floor table outlives this synchronous load and the
    // owner thread remains exclusive for the whole operation.
    let api = unsafe { &*api_ptr };
    let system = unsafe { birnam_abi::namespace_import::import(runtime.stack.imports(), c"System".as_ptr()) };
    if system == api.nilObj {
        return Err(ImageError::MissingSystem);
    }
    let path_bytes = path_bytes(path);
    let path_value = unsafe { (birnam_abi::string::make)(path_bytes.as_ptr(), path_bytes.len()) };
    let rebuild_value = if rebuild { api.trueObj } else { api.falseObj };
    let selector = unsafe { (api.intern)(c"loadImage:rebuild:".as_ptr()) };
    let status = unsafe { (api.send)(system, selector, [path_value, rebuild_value].as_ptr(), 2) };
    unsafe { finish_image_load(api, runtime.stack.presence(), runtime.stack.imports(), system, status) }
}

/// Load complete image bytes through the ordinary Birnam `System` send surface.
/// This is the baked-executable path; it avoids temporary files and keeps the
/// image module authoritative for decoding, mapping, and recovery.
///
/// # Safety
///
/// The runtime must remain on the selected floor's owner thread, and the
/// caller must establish a valid GC stack bottom for the synchronous sends.
pub unsafe fn load_image_bytes(
    runtime: &mut HostRuntime,
    bytes: &[u8],
    rebuild: bool,
) -> Result<ImageLoadOutcome, ImageError> {
    validate_image_header(bytes)?;
    let api_ptr = runtime.stack.api() as *const IdApi;
    // SAFETY: the retained floor table outlives this synchronous load and the
    // owner thread remains exclusive for the whole operation.
    let api = unsafe { &*api_ptr };
    let system = unsafe { birnam_abi::namespace_import::import(runtime.stack.imports(), c"System".as_ptr()) };
    if system == api.nilObj {
        return Err(ImageError::MissingSystem);
    }
    let image_value = unsafe { (birnam_abi::string::make)(bytes.as_ptr(), bytes.len()) };
    let rebuild_value = if rebuild { api.trueObj } else { api.falseObj };
    let selector = unsafe { (api.intern)(c"loadImageBytes:rebuild:".as_ptr()) };
    let status = unsafe { (api.send)(system, selector, [image_value, rebuild_value].as_ptr(), 2) };
    unsafe { finish_image_load(api, runtime.stack.presence(), runtime.stack.imports(), system, status) }
}

unsafe fn finish_image_load(
    api: &IdApi, presence: *const birnam_abi::namespace_presence::ApiV1, imports: *const birnam_abi::namespace_import::ApiV1,
    system: birnam_abi::Oop,
    status: birnam_abi::Oop,
) -> Result<ImageLoadOutcome, ImageError> {
    if status == api.nilObj {
        return Err(ImageError::LoadRejected);
    }
    if status == api.trueObj {
        // A mapped image still cannot carry native Alien trampolines. Reopen
        // the Rust Codegen recovery boundary for the same replay/derivation
        // hooks used by source recovery before exposing the image to the REPL.
        // Without this, image-resident methods that call an Alien wrapper keep
        // the stale process from the saving runtime and silently read zeros.
        let codegen = unsafe { birnam_abi::namespace_import::import(imports, c"Codegen".as_ptr()) };
        if codegen != api.nilObj {
            let begin_recovery = unsafe { (api.intern)(c"beginImageDispatchRecovery".as_ptr()) };
            let begun = unsafe { (api.send)(codegen, begin_recovery, std::ptr::null(), 0) };
            if begun == api.nilObj || send_is_error(api, presence, begun) {
                return Err(ImageError::LoadRejected);
            }
            // The v8 loader already admitted and published the exact portable
            // generation. Recompile its recovered self-hosted sources into
            // methods without introducing a new source generation.
            let rebuild_image_code = unsafe { (api.intern)(c"rebuildImageCode".as_ptr()) };
            let rebuilt = unsafe { (api.send)(codegen, rebuild_image_code, std::ptr::null(), 0) };
            if rebuilt.is_null() || rebuilt == api.nilObj || send_is_error(api, presence, rebuilt) {
                return Err(ImageError::LoadRejected);
            }
            let config_surface_missing = unsafe { global_named(api, imports, "Config") }
                .map(|config| unsafe {
                    let loads = (api.intern)(c"loads".as_ptr());
                    !receiver_has_selector(api, config, loads)
                })
                .unwrap_or(false);
            if config_surface_missing {
                return Err(ImageError::LoadRejected);
            }
            let replay_aliens = unsafe { (api.intern)(c"replayAliens".as_ptr()) };
            unsafe { (api.send)(codegen, replay_aliens, std::ptr::null(), 0) };
            let derive_globals = unsafe { (api.intern)(c"deriveImageGlobals".as_ptr()) };
            unsafe { (api.send)(codegen, derive_globals, std::ptr::null(), 0) };
            if !unsafe { reconcile_image_composition(api, presence, imports, system) } {
                return Err(ImageError::LoadRejected);
            }
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

    let codegen = unsafe { birnam_abi::namespace_import::import(imports, c"Codegen".as_ptr()) };
    if codegen != api.nilObj {
        // `System loadImage:rebuild:` closes the image module's internal
        // recovery phase before returning. Rust Codegen deliberately requires
        // replay/derivation to run inside that phase, so reopen the host-owned
        // recovery boundary around the post-load operations within the image
        // load transaction.
        let begin_recovery = unsafe { (api.intern)(c"beginImageDispatchRecovery".as_ptr()) };
        let begun = unsafe { (api.send)(codegen, begin_recovery, std::ptr::null(), 0) };
        if begun == api.nilObj || send_is_error(api, presence, begun) {
            return Err(ImageError::LoadRejected);
        }
        // The v8 loader has already rebuilt the exact stored PortablePrograms
        // and selection before returning false. Recompile the recovered
        // self-hosted sources without bootstrapping a new durable generation.
        let rebuild_image_code = unsafe { (api.intern)(c"rebuildImageCode".as_ptr()) };
        let rebuilt = unsafe { (api.send)(codegen, rebuild_image_code, std::ptr::null(), 0) };
        if rebuilt.is_null() || rebuilt == api.nilObj || send_is_error(api, presence, rebuilt) {
            return Err(ImageError::LoadRejected);
        }
        let config_surface_missing = unsafe { global_named(api, imports, "Config") }
            .map(|config| unsafe {
                let loads = (api.intern)(c"loads".as_ptr());
                !receiver_has_selector(api, config, loads)
            })
            .unwrap_or(false);
        if config_surface_missing {
            return Err(ImageError::LoadRejected);
        }
        let replay_aliens = unsafe { (api.intern)(c"replayAliens".as_ptr()) };
        unsafe { (api.send)(codegen, replay_aliens, std::ptr::null(), 0) };
        // Source-backed block globals are intentionally not serialized as
        // durable values. Re-derive them after code/method recovery so an
        // ordinary image boot and an explicit rebuild expose the same globals
        // before the REPL evaluates its first expression.
        let derive_globals = unsafe { (api.intern)(c"deriveImageGlobals".as_ptr()) };
        unsafe { (api.send)(codegen, derive_globals, std::ptr::null(), 0) };
        if !unsafe { reconcile_image_composition(api, presence, imports, system) } {
            return Err(ImageError::LoadRejected);
        }
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

/// Reify Design 47's composed executable generation after image dispatch and
/// portable facts are both live, but before the recovery transaction exposes a
/// user entry point. Data-only images have no composition identity and need no
/// lifecycle reconciliation.
unsafe fn reconcile_image_composition(api: &IdApi, presence: *const birnam_abi::namespace_presence::ApiV1, imports: *const birnam_abi::namespace_import::ApiV1, system: birnam_abi::Oop) -> bool {
    let identity_selector = unsafe { (api.intern)(c"imageCompositionIdentity".as_ptr()) };
    let identity = unsafe { (api.send)(system, identity_selector, std::ptr::null(), 0) };
    if identity.is_null() || identity == api.nilObj {
        return true;
    }
    let Some(compiled_unit) = (unsafe { global_named(api, imports, "CompiledUnit") }) else {
        return false;
    };
    let reconcile = unsafe { (api.intern)(c"reconcileImageComposition:".as_ptr()) };
    let result = unsafe { (api.send)(compiled_unit, reconcile, [identity].as_ptr(), 1) };
    !result.is_null() && result != api.nilObj && !send_is_error(api, presence, result)
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
    VtableEnumeration { path: PathBuf, reason: &'static str },
    Namespace { path: PathBuf, reason: &'static str },
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
            Self::VtableEnumeration { path, reason } => write!(formatter, "Vtable enumeration preparation failed: {} ({reason})", path.display()),
            Self::Namespace { path, reason } => write!(formatter, "Namespace preparation failed: {} ({reason})", path.display()),
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
    _namespace: NonNull<std::ffi::c_void>,
    _object_model: NonNull<std::ffi::c_void>,
    vtable_enumeration: NonNull<birnam_abi::vtable_enumeration::ApiV1>,
    namespace_names: NonNull<birnam_abi::namespace_names::ApiV1>,
    namespace_presence: NonNull<birnam_abi::namespace_presence::ApiV1>,
    namespace_import: NonNull<birnam_abi::namespace_import::ApiV1>,
    namespace_export: NonNull<birnam_abi::namespace_export::ApiV1>,
    handle: NonNull<std::ffi::c_void>,
    api: NonNull<IdApi>,
    // Keep this type !Send/!Sync until host threading and the floor owner-thread
    // protocol are explicitly ported. The marker has no runtime state.
    _owner_thread: PhantomData<std::rc::Rc<()>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleError {
    EmbeddedNul,
    OpenFailed {
        path: PathBuf,
    },
    IntegerDependencyRejected { path: PathBuf },
    FloatDependencyRejected { path: PathBuf },
    WordDependencyRejected { path: PathBuf },
    StringDependencyRejected { path: PathBuf },
    RangeDependencyRejected { path: PathBuf },
    VectorDependencyRejected { path: PathBuf },
    CollectionValueDependencyRejected { path: PathBuf },
    MapDependencyRejected { path: PathBuf },
    StringCollectionDependencyRejected { path: PathBuf },
    PrintingDependencyRejected { path: PathBuf },
    SystemGcDependencyRejected { path: PathBuf },
    TextViewDependencyRejected { path: PathBuf },
    ErrorValueDependencyRejected { path: PathBuf },
    SystemOutputDependencyRejected { path: PathBuf },
    SystemInputDependencyRejected { path: PathBuf },
    EnvironmentDependencyRejected { path: PathBuf },
    FileIoDependencyRejected { path: PathBuf },
    BlockLoopDependencyRejected { path: PathBuf },
    NativeLoaderDependencyRejected { path: PathBuf },
    SourceLoaderDependencyRejected { path: PathBuf },
    NamespaceNamesDependencyRejected { path: PathBuf },
    TraceDependencyRejected { path: PathBuf },
    CollectDependencyRejected { path: PathBuf },
    MissingModuleInit {
        path: PathBuf,
    },
    MissingSymbolCompactionControl {
        path: PathBuf,
    },
    MissingSymbolCompatibilityLimit {
        path: PathBuf,
        variable: &'static str,
    },
    InvalidSymbolCompatibilityLimit {
        path: PathBuf,
        variable: &'static str,
    },
    SymbolCompatibilityLimitRejected {
        path: PathBuf,
    },
    MissingStoreCompensationApi {
        path: PathBuf,
    },
    MissingCodegenCompensationConfig {
        path: PathBuf,
    },
    StoreCompensationConfigRejected {
        path: PathBuf,
    },
}

const SYMBOL_COUNT_LIMIT_ENV: &str = "BIRNAM_SYMBOL_LIMIT";

const SYMBOL_NAME_BYTES_LIMIT_ENV: &str = "BIRNAM_SYMBOL_BYTES_LIMIT";

type SymbolCompactionControl = unsafe extern "C" fn(c_int, usize, usize) -> c_int;

type SymbolBorrowDeclaration = unsafe extern "C" fn() -> c_int;

type StoreCompensationApiGetter = unsafe extern "C" fn() -> *const std::ffi::c_void;

type CodegenCompensationConfigure =
    unsafe extern "C" fn(*const std::ffi::c_void) -> std::ffi::c_int;

fn parse_positive_limit(value: &OsStr) -> Option<usize> {
    value
        .to_str()?
        .parse::<usize>()
        .ok()
        .filter(|limit| *limit != 0)
}

fn symbol_compatibility_limits(path: &Path) -> Result<(usize, usize), ModuleError> {
    let count = env::var_os(SYMBOL_COUNT_LIMIT_ENV).ok_or_else(|| {
        ModuleError::MissingSymbolCompatibilityLimit {
            path: path.to_path_buf(),
            variable: SYMBOL_COUNT_LIMIT_ENV,
        }
    })?;
    let bytes = env::var_os(SYMBOL_NAME_BYTES_LIMIT_ENV).ok_or_else(|| {
        ModuleError::MissingSymbolCompatibilityLimit {
            path: path.to_path_buf(),
            variable: SYMBOL_NAME_BYTES_LIMIT_ENV,
        }
    })?;
    let count = parse_positive_limit(&count).ok_or_else(|| {
        ModuleError::InvalidSymbolCompatibilityLimit {
            path: path.to_path_buf(),
            variable: SYMBOL_COUNT_LIMIT_ENV,
        }
    })?;
    let bytes = parse_positive_limit(&bytes).ok_or_else(|| {
        ModuleError::InvalidSymbolCompatibilityLimit {
            path: path.to_path_buf(),
            variable: SYMBOL_NAME_BYTES_LIMIT_ENV,
        }
    })?;
    Ok((count, bytes))
}

/// The native module order is part of the host contract: Store precedes
/// Codegen, and every later rung sees the same canonical floor/API table.
pub const SCRIPT_MODULES: &[&str] = &[
    "libbirnam_namespace",
    "libbirnam_storage",
    "libbirnam_trace",
    "libbirnam_collect",
    "libbirnam_error_value",
    "libbirnam_integer",
    "libbirnam_float",
    "libbirnam_word",
    "libbirnam_object_model",
    "libbirnam_id_artifact",
    "libbirnam_store",
    "libbirnam_id_peg",
    "libbirnam_id_lang",
    "libbirnam_id_tcc",
    "libbirnam_id_codegen",
    "libbirnam_boolean",
    "libbirnam_printing",
    "libbirnam_system_gc",
    "libbirnam_text_view",
    "libbirnam_system_output",
    "libbirnam_system_input",
    "libbirnam_environment",
    "libbirnam_file_io",
    "libbirnam_block_loop",
    "libbirnam_native_loader",
    "libbirnam_source_loader",
    "libbirnam_id_prelude",
    "libbirnam_nil",
    "libbirnam_string",
    "libbirnam_string_builder",
    "libbirnam_collection_value",
    "libbirnam_vector",
    "libbirnam_range",
    "libbirnam_map",
    "libbirnam_string_collection",
    "libbirnam_id_collection",
    "libbirnam_id_error",
    "libbirnam_portable_code",
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
    pub aot: ModuleAotPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleAotPolicy {
    Synchronous,
    InitialGenerationResumable,
}

pub const SELF_HOSTED_MODULES: &[SelfHostedModule] = &[
    SelfHostedModule {
        name: "stdlib",
        source: include_bytes!("../../../src/id/stdlib.birnam"),
        aot: ModuleAotPolicy::Synchronous,
    },
    SelfHostedModule {
        name: "compiledUnit",
        source: include_bytes!("../../../src/id/compiled-unit.birnam"),
        aot: ModuleAotPolicy::Synchronous,
    },
    SelfHostedModule {
        name: "toml",
        source: include_bytes!("../../../src/id/toml.birnam"),
        aot: ModuleAotPolicy::Synchronous,
    },
    SelfHostedModule {
        name: "config",
        source: include_bytes!("../../../src/id/config.birnam"),
        aot: ModuleAotPolicy::Synchronous,
    },
    SelfHostedModule {
        name: "appConfig",
        source: include_bytes!("../../../src/id/app-config.birnam"),
        aot: ModuleAotPolicy::Synchronous,
    },
    SelfHostedModule {
        name: "isolate",
        source: include_bytes!("../../../src/id/isolate.birnam"),
        aot: ModuleAotPolicy::Synchronous,
    },
    SelfHostedModule {
        name: "stacklessControl",
        source: include_bytes!("../../../src/id/stackless-control.birnam"),
        aot: ModuleAotPolicy::InitialGenerationResumable,
    },
];

pub const MODULE_MANAGER: SelfHostedModule = SelfHostedModule {
    name: "modules",
    source: include_bytes!("../../../src/id/modules.birnam"),
    aot: ModuleAotPolicy::Synchronous,
};

/// Project policy is intentionally lazy: it participates in check/build/run
/// workflows but must not become part of ordinary runtime/image bootstrap.
pub const PROJECT_POLICY_MODULE: SelfHostedModule = SelfHostedModule {
    name: "projectPolicy",
    source: include_bytes!("../../../src/id/project-policy.birnam"),
    aot: ModuleAotPolicy::Synchronous,
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





type IsolateDriverFn =
    unsafe extern "C" fn(*mut IdApi, *const birnam_abi::namespace_import::ApiV1, c_int, birnam_abi::Oop, birnam_abi::Oop) -> c_int;

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
        imports: *const birnam_abi::namespace_import::ApiV1,
        fd: c_int,
        receiver: birnam_abi::Oop,
        selector: birnam_abi::Oop,
    ) -> c_int {
        unsafe { (self.driver)(api, imports, fd, receiver, selector) }
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

pub(super) unsafe fn bridge_string(api: &IdApi, value: birnam_abi::Oop) -> Option<String> {
    if value == api.nilObj || unsafe { (api.vtableOf)(value) } != unsafe { birnam_abi::string::behavior() } {
        return None;
    }
    let mut length = 0usize;
    let bytes = unsafe { (birnam_abi::string::bytes)(value, &mut length) };
    if bytes.is_null() {
        return (length == 0).then(String::new);
    }
    Some(String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(bytes, length) }).into_owned())
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
        let Some(module_manager) = (unsafe { global_named(self.stack.api(), self.stack.imports(), "Module") }) else {
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
        if result == api.nilObj || send_is_error(api, self.stack.presence(), result) {
            return Err(RuntimeModuleError::LoadRejected { path });
        }
        Ok(())
    }

    /// Load a host-tool module outside the sealed application generation. This
    /// is the same capability boundary used by the host for a process-local
    /// REPL fallback: the module may publish its tool globals, but it is not a
    /// durable application-generation introduction.
    ///
    /// # Safety
    ///
    /// The runtime must remain on the floor owner thread for the synchronous
    /// Codegen/Module sends.
    pub(super) unsafe fn load_host_runtime_module(&self, name: &str) -> Result<(), RuntimeModuleError> {
        let name = CString::new(name).map_err(|_| RuntimeModuleError::EmbeddedNul)?;
        let filename = format!("libbirnam_{}.dylib", name.to_string_lossy());
        let path = self.resources.lib_dir.join(filename);
        if std::fs::metadata(&path).is_err() {
            return Err(RuntimeModuleError::ArtifactUnavailable { path });
        }
        let path = CString::new(path_bytes(&path)).map_err(|_| RuntimeModuleError::EmbeddedNul)?;
        let selector = unsafe { (self.stack.api().intern)(c"loadHostCapability:".as_ptr()) };
        let value =
            unsafe { (birnam_abi::string::make)(path.as_bytes().as_ptr(), path.as_bytes().len()) };
        let api = self.stack.api();
        let result = unsafe { (api.send)(self.codegen, selector, [value].as_ptr(), 1) };
        if result == self.stack.api().nilObj || send_is_error(self.stack.api(), self.stack.presence(), result) {
            return Err(RuntimeModuleError::LoadRejected {
                path: PathBuf::from(path.to_string_lossy().trim_end_matches('\0')),
            });
        }
        if let Some(module_manager) = unsafe { global_named(self.stack.api(), self.stack.imports(), "Module") } {
            let mark = unsafe { (self.stack.api().intern)(c"markLoaded:".as_ptr()) };
            let symbol = unsafe { (self.stack.api().intern)(name.as_ptr()) };
            // The canonical host treats this as bookkeeping only: a host
            // capability has already been installed by Codegen and does not
            // become an application-generation module. Preserve that boundary
            // by ignoring the manager's return value here as well.
            unsafe { (self.stack.api().send)(module_manager, mark, [symbol].as_ptr(), 1) };
        }
        Ok(())
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
/// small source convention as the host. Dependencies outside the
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

pub(super) fn send_is_error(api: &IdApi, presence: *const birnam_abi::namespace_presence::ApiV1, value: birnam_abi::Oop) -> bool {
    if unsafe { birnam_abi::namespace_presence::contains(presence, c"Error".as_ptr()) } == 0 {
        return false;
    }
    let is_error = unsafe { (api.intern)(c"isError".as_ptr()) };
    unsafe { (api.send)(value, is_error, std::ptr::null(), 0) == api.trueObj }
}

pub(super) unsafe fn error_diagnostic(api: &IdApi, presence: *const birnam_abi::namespace_presence::ApiV1, value: birnam_abi::Oop) -> Option<(String, String)> {
    if value == api.nilObj || !send_is_error(api, presence, value) {
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

pub(super) unsafe fn native_os_string_vector(
    api: &IdApi, presence: *const birnam_abi::namespace_presence::ApiV1, imports: *const birnam_abi::namespace_import::ApiV1,
    args: &[OsString],
) -> Result<birnam_abi::Oop, CliError> {
    let vector_class = unsafe { birnam_abi::namespace_import::import(imports, c"Vector".as_ptr()) };
    if vector_class == api.nilObj {
        return Err(CliError::MissingVector);
    }
    let new_selector = unsafe { (api.intern)(c"new".as_ptr()) };
    let add_selector = unsafe { (api.intern)(c"add:".as_ptr()) };
    let mut vector = unsafe { (api.send)(vector_class, new_selector, std::ptr::null(), 0) };
    if vector == api.nilObj || send_is_error(api, presence, vector) {
        return Err(CliError::ArgumentVectorRejected);
    }
    for (index, argument) in args.iter().enumerate() {
        let Some(argument) = argument.to_str() else {
            return Err(CliError::NonUtf8Argument(index));
        };
        let value = unsafe { (birnam_abi::string::make)(argument.as_bytes().as_ptr(), argument.len()) };
        let updated = unsafe { (api.send)(vector, add_selector, [value].as_ptr(), 1) };
        if updated == api.nilObj || send_is_error(api, presence, updated) {
            return Err(CliError::ArgumentVectorRejected);
        }
        vector = updated;
    }
    Ok(vector)
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
    api: &IdApi, presence: *const birnam_abi::namespace_presence::ApiV1,
    codegen: birnam_abi::Oop,
    module: &SelfHostedModule,
) -> Result<birnam_abi::Oop, BootstrapSourceError> {
    let name = CString::new(module.name).map_err(|_| BootstrapSourceError::RecordRejected)?;
    let source = unsafe { (birnam_abi::string::make)(module.source.as_ptr(), module.source.len()) };
    let name_symbol = unsafe { (api.intern)(name.as_ptr()) };
    let record_selector = unsafe { (api.intern)(c"recordModuleSource:named:".as_ptr()) };
    let recorded =
        unsafe { (api.send)(codegen, record_selector, [source, name_symbol].as_ptr(), 2) };
    if send_is_error(api, presence, recorded) {
        return Err(BootstrapSourceError::RecordRejected);
    }
    let eval_selector = unsafe { (api.intern)(c"eval:".as_ptr()) };
    let evaluated = unsafe { (api.send)(codegen, eval_selector, [source].as_ptr(), 1) };
    if send_is_error(api, presence, evaluated) {
        return Err(BootstrapSourceError::EvaluationRejected);
    }
    Ok(evaluated)
}

/// Load one self-hosted module using the same optimization policy as the
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
    api: &IdApi, presence: *const birnam_abi::namespace_presence::ApiV1,
    codegen: birnam_abi::Oop,
    resources: &Resources,
    module: &SelfHostedModule,
    force_source: bool,
) -> Result<SelfHostedLoadOutcome, BootstrapSourceError> {
    let selected = module.aot == ModuleAotPolicy::InitialGenerationResumable;
    if !force_source {
        let candidate = self_hosted_candidate_path(resources, module);
        if std::fs::metadata(&candidate).is_ok() {
            let candidate_bytes = path_bytes(&candidate);
            let candidate_string =
                unsafe { (birnam_abi::string::make)(candidate_bytes.as_ptr(), candidate_bytes.len()) };
            let load_selector = if selected {
                unsafe { (api.intern)(c"loadInitialModule:".as_ptr()) }
            } else {
                unsafe { (api.intern)(c"load:".as_ptr()) }
            };
            let loaded =
                unsafe { (api.send)(codegen, load_selector, [candidate_string].as_ptr(), 1) };
            if loaded != api.nilObj && !send_is_error(api, presence, loaded) {
                return Ok(SelfHostedLoadOutcome::Precompiled);
            }
        }
    }
    if selected {
        static SOURCE_COMPILE_SEQUENCE: AtomicU64 = AtomicU64::new(1);
        let sequence = SOURCE_COMPILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let output = env::temp_dir().join(format!(
            "birnam-initial-module-{}-{sequence}.dylib",
            std::process::id()
        ));
        struct RemoveCompiled(PathBuf);
        impl Drop for RemoveCompiled {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let _remove = RemoveCompiled(output.clone());
        let source = unsafe { (birnam_abi::string::make)(module.source.as_ptr(), module.source.len()) };
        let output_bytes = path_bytes(&output);
        let output_value = unsafe { (birnam_abi::string::make)(output_bytes.as_ptr(), output_bytes.len()) };
        let selection = unsafe { (api.intern)(c"resumableRequired".as_ptr()) };
        let compile_selector =
            unsafe { (api.intern)(c"compileInitialModule:output:selection:".as_ptr()) };
        let compiled = unsafe {
            (api.send)(
                codegen,
                compile_selector,
                [source, output_value, selection].as_ptr(),
                3,
            )
        };
        if compiled == api.nilObj || send_is_error(api, presence, compiled) {
            return Err(BootstrapSourceError::EvaluationRejected);
        }
        let load_selector = unsafe { (api.intern)(c"loadInitialModule:".as_ptr()) };
        let loaded = unsafe { (api.send)(codegen, load_selector, [output_value].as_ptr(), 1) };
        if loaded == api.nilObj || send_is_error(api, presence, loaded) {
            return Err(BootstrapSourceError::EvaluationRejected);
        }
        return Ok(SelfHostedLoadOutcome::Source);
    }
    // SAFETY: forwarded to the documented source-recovery boundary.
    unsafe { record_and_eval_source(api, presence, codegen, module)? };
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

pub(super) unsafe fn global_named(api: &IdApi, imports: *const birnam_abi::namespace_import::ApiV1, name: &str) -> Option<birnam_abi::Oop> {
    let imported_name = CString::new(name).ok()?;
    let imported = unsafe { birnam_abi::namespace_import::import(imports, imported_name.as_ptr()) };
    if !imported.is_null() && imported != api.nilObj {
        return Some(imported);
    }
    let globals = unsafe { birnam_abi::namespace_import::import(imports, c"Globals".as_ptr()) };
    if globals.is_null() || globals == api.nilObj {
        return None;
    }
    let name = unsafe { intern_name(api, name) }?;
    let selector = unsafe { (api.intern)(c"at:".as_ptr()) };
    let value = unsafe { (api.send)(globals, selector, [name].as_ptr(), 1) };
    (value != api.nilObj).then_some(value)
}

pub(super) fn selector_has_arity_one(selector: &str) -> bool {
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
        vtable = unsafe { birnam_abi::vtable_parent(api, vtable) };
    }
    false
}

/// Recover a missing worker entry point from the image's durable method source
/// journal. Mapped code remains authoritative when it exposes the requested
/// selector; this fallback is only for a worker image whose code unit was
/// admitted but did not bind the target's selector in the live vtable. The
/// recovery transaction mirrors the ordinary image source path and is closed
/// before the worker driver is entered.
pub(super) unsafe fn mark_module_loaded(api: &IdApi, module_manager: birnam_abi::Oop, name: &str) {
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
    let value = unsafe { (birnam_abi::string::make)(path.as_ptr(), path.len()) };
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
    api: &IdApi, presence: *const birnam_abi::namespace_presence::ApiV1,
    module_manager: birnam_abi::Oop,
    vector: birnam_abi::Oop,
) -> Option<Vec<&'static SelfHostedModule>> {
    let names = unsafe { requested_names(api, vector) }?;
    let order_selector = unsafe { (api.intern)(c"loadOrder:".as_ptr()) };
    let order = unsafe { (api.send)(module_manager, order_selector, [names].as_ptr(), 1) };
    if order == api.nilObj || send_is_error(api, presence, order) {
        return None;
    }
    let size_selector = unsafe { (api.intern)(c"size".as_ptr()) };
    let at_selector = unsafe { (api.intern)(c"at:".as_ptr()) };
    let size = unsafe { (api.send)(order, size_selector, std::ptr::null(), 0) };
    let size = unsafe { (birnam_abi::integer::value)(size) };
    if size < 0 {
        return None;
    }
    let mut result = Vec::with_capacity(size as usize);
    for index in 0..size as usize {
        let index = unsafe { (birnam_abi::integer::make)(index as i64) };
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
    api: &IdApi, presence: *const birnam_abi::namespace_presence::ApiV1, imports: *const birnam_abi::namespace_import::ApiV1,
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
        unsafe { load_self_hosted(api, presence, codegen, resources, &MODULE_MANAGER, force_source) }
            .map_err(BootstrapRuntimeError::Source)?;
    loaded.push(LoadedSelfHostedModule {
        name: MODULE_MANAGER.name,
        outcome: manager_outcome,
    });

    let module_manager = unsafe { global_named(api, imports, "Module") };
    let vector = unsafe { birnam_abi::namespace_import::import(imports, c"Vector".as_ptr()) };
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
    let mut order = module_manager
        .zip((vector != api.nilObj).then_some(vector))
        .and_then(|(module_manager, vector)| unsafe {
            order_from_manager(api, presence, module_manager, vector)
        })
        .unwrap_or_else(|| SELF_HOSTED_MODULES.iter().collect());
    // G0-selected artifacts are authority-bound to the complete initial source
    // inventory, so every synchronous source must be registered before one is
    // admitted and staged.
    order.sort_by_key(|module| module.aot == ModuleAotPolicy::InitialGenerationResumable);

    for module in order {
        let outcome = unsafe { load_self_hosted(api, presence, codegen, resources, module, force_source) }
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
    symbol_compaction_control: Option<SymbolCompactionControl>,
    all_modules_borrow_safe: bool,
    integer: *mut birnam_abi::integer::IntegerApiV1,
    float: *mut birnam_abi::float::FloatApiV1,
    word: *mut birnam_abi::word::WordApiV1,
    string: *mut birnam_abi::string::StringApiV1,
    range: *const birnam_abi::range::ApiV4,
    vector: *const birnam_abi::vector::ApiV4,
    map: *const birnam_abi::map::ApiV4,
    string_collection: *const birnam_abi::string_collection::ApiV1,
    printing: *const birnam_abi::printing::ApiV1,
    system_gc: *const birnam_abi::system_gc::ApiV1,
    text_view: *const birnam_abi::text_view::ApiV1,
    error_value: *const birnam_abi::error_value::ApiV3,
    system_output: *const birnam_abi::system_output::ApiV1,
    system_input: *const birnam_abi::system_input::ApiV3,
    environment: *const birnam_abi::environment::ApiV3,
    file_io: *const birnam_abi::file_io::ApiV3,
    block_loop: *const birnam_abi::block_loop::ApiV3,
    native_loader: *const birnam_abi::native_loader::ApiV5,
    source_loader: *const birnam_abi::source_loader::ApiV3,
    collection_value: *const birnam_abi::collection_value::ApiV1,
    prepared_float: Option<(PathBuf, NonNull<std::ffi::c_void>)>,
    prepared_word: Option<(PathBuf, NonNull<std::ffi::c_void>)>,
    prepared_string: Option<(PathBuf, NonNull<std::ffi::c_void>)>,
}

impl LoadedStack {
    pub fn presence(&self) -> *const birnam_abi::namespace_presence::ApiV1 { self.floor.presence() }
    pub fn imports(&self) -> *const birnam_abi::namespace_import::ApiV1 { self.floor.imports() }
    pub fn exports(&self) -> *const birnam_abi::namespace_export::ApiV1 { self.floor.exports() }

    pub fn new(floor: LoadedFloor) -> Self {
        let control = unsafe {
            (floor.api().dlsym)(
                floor.handle().as_ptr(),
                c"birnam_id_symbol_compaction_configure_v1".as_ptr().cast(),
            )
        };
        let symbol_compaction_control = NonNull::new(control).map(|control| {
            // SAFETY: the private floor symbol has this versioned frozen shape.
            unsafe { std::mem::transmute(control.as_ptr()) }
        });
        Self {
            floor,
            modules: Vec::new(),
            symbol_compaction_control,
            all_modules_borrow_safe: true,
            integer: std::ptr::null_mut(),
            float: std::ptr::null_mut(),
            word: std::ptr::null_mut(),
            string: std::ptr::null_mut(),
            range: std::ptr::null(),
            vector: std::ptr::null(),
            map: std::ptr::null(),
            string_collection: std::ptr::null(),
            printing: std::ptr::null(),
            system_gc: std::ptr::null(),
            text_view: std::ptr::null(),
            error_value: std::ptr::null(),
            system_output: std::ptr::null(),
            system_input: std::ptr::null(),
            environment: std::ptr::null(),
            file_io: std::ptr::null(),
            block_loop: std::ptr::null(),
            native_loader: std::ptr::null(),
            source_loader: std::ptr::null(),
            collection_value: std::ptr::null(),
            prepared_float: None,
            prepared_word: None,
            prepared_string: None,
        }
    }

    pub fn module_count(&self) -> usize {
        self.modules.len()
    }

    pub fn api(&self) -> &IdApi {
        self.floor.api()
    }

    fn configure_symbol_borrow(&mut self, path: &Path, handle: NonNull<std::ffi::c_void>) -> Result<(), ModuleError> {
        let declaration = unsafe {
            (self.floor.api().dlsym)(handle.as_ptr(), c"birnam_symbol_borrow_v1".as_ptr().cast())
        };
        let borrow_safe = NonNull::new(declaration).is_some_and(|declaration| {
            // SAFETY: a module opting into the versioned marker promises this
            // exact no-argument declaration signature.
            let declaration: SymbolBorrowDeclaration =
                unsafe { std::mem::transmute(declaration.as_ptr()) };
            unsafe { declaration() == 1 }
        });
        let Some(control) = self.symbol_compaction_control else {
            return Err(ModuleError::MissingSymbolCompactionControl {
                path: path.to_path_buf(),
            });
        };
        if borrow_safe && self.all_modules_borrow_safe {
            if unsafe { control(1, 0, 0) } != 1 {
                return Err(ModuleError::SymbolCompatibilityLimitRejected {
                    path: path.to_path_buf(),
                });
            }
        } else if !borrow_safe {
            let (count_limit, name_byte_limit) = symbol_compatibility_limits(path)?;
            if unsafe { control(0, count_limit, name_byte_limit) } != 1 {
                return Err(ModuleError::SymbolCompatibilityLimitRejected {
                    path: path.to_path_buf(),
                });
            }
            self.all_modules_borrow_safe = false;
        }
        Ok(())
    }

    /// Prepare Float before Integer installs any mixed numeric methods. The
    /// handle is pinned before publishing callbacks, including on startup failure.
    fn prepare_float(&mut self, path: &Path) -> Result<(), ModuleError> {
        if self.prepared_float.is_some() { return Ok(()); }
        let encoded = CString::new(path_bytes(path)).map_err(|_| ModuleError::EmbeddedNul)?;
        let handle = unsafe { (self.api().dlopen)(encoded.as_ptr(), RTLD_LAZY) };
        let handle = NonNull::new(handle).ok_or_else(|| ModuleError::OpenFailed { path: path.to_path_buf() })?;
        self.prepared_float = Some((path.to_path_buf(), handle));
        self.configure_symbol_borrow(path, handle)?;
        let prepare = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_float_prepare_v4".as_ptr()) };
        if prepare.is_null() { return Err(ModuleError::FloatDependencyRejected { path: path.to_path_buf() }); }
        let prepare: birnam_abi::float::FloatPrepareV4 = unsafe { std::mem::transmute(prepare) };
        let ops = unsafe { prepare(self.api() as *const IdApi as *mut IdApi, self.presence(), self.imports(), self.exports()) };
        self.attach_float(path, ops)
    }

    /// Prepare Word before Integer installs any numeric methods. The
    /// handle is pinned before publishing callbacks, including on startup failure.
    fn prepare_word(&mut self, path: &Path) -> Result<(), ModuleError> {
        if self.prepared_word.is_some() { return Ok(()); }
        let encoded = CString::new(path_bytes(path)).map_err(|_| ModuleError::EmbeddedNul)?;
        let handle = unsafe { (self.api().dlopen)(encoded.as_ptr(), RTLD_LAZY) };
        let handle = NonNull::new(handle).ok_or_else(|| ModuleError::OpenFailed { path: path.to_path_buf() })?;
        self.prepared_word = Some((path.to_path_buf(), handle));
        self.configure_symbol_borrow(path, handle)?;
        let prepare = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_word_prepare_v4".as_ptr()) };
        if prepare.is_null() { return Err(ModuleError::WordDependencyRejected { path: path.to_path_buf() }); }
        let prepare: birnam_abi::word::WordPrepareV4 = unsafe { std::mem::transmute(prepare) };
        let ops = unsafe { prepare(self.api() as *const IdApi as *mut IdApi, self.presence(), self.imports(), self.exports()) };
        self.attach_word(path, ops)
    }

    fn prepare_string(&mut self, path: &Path) -> Result<(), ModuleError> {
        if self.prepared_string.is_some() { return Ok(()); }
        let encoded = CString::new(path_bytes(path)).map_err(|_| ModuleError::EmbeddedNul)?;
        let handle = unsafe { (self.api().dlopen)(encoded.as_ptr(), RTLD_LAZY) };
        let handle = NonNull::new(handle).ok_or_else(|| ModuleError::OpenFailed { path: path.to_path_buf() })?;
        self.prepared_string = Some((path.to_path_buf(), handle));
        self.configure_symbol_borrow(path, handle)?;
        let prepare = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_string_prepare_v2".as_ptr()) };
        if prepare.is_null() { return Err(ModuleError::StringDependencyRejected { path: path.to_path_buf() }); }
        let prepare: birnam_abi::string::StringPrepareV2 = unsafe { std::mem::transmute(prepare) };
        let provider = unsafe { (self.api().dlsym)(self.floor.handle().as_ptr(), c"birnam_native_allocation_provider_v1".as_ptr()) };
        if provider.is_null() { return Err(ModuleError::StringDependencyRejected { path: path.to_path_buf() }); }
        let provider: birnam_abi::native_allocation::NativeAllocationProviderV1 = unsafe { std::mem::transmute(provider) };
        let allocation = unsafe { provider() };
        if allocation.is_null() { return Err(ModuleError::StringDependencyRejected { path: path.to_path_buf() }); }
        let ops = unsafe { prepare(self.api() as *const IdApi as *mut IdApi, self.exports(), allocation) };
        self.attach_string(path, ops)
    }

    fn attach_float(&mut self, path: &Path, ops: *mut birnam_abi::float::FloatApiV1) -> Result<(), ModuleError> {
        if (!self.float.is_null() && self.float != ops) || unsafe { birnam_abi::float::attach(ops) } != 1 {
            return Err(ModuleError::FloatDependencyRejected { path: path.to_path_buf() });
        }
        let dependency = unsafe { (self.api().dlsym)(self.floor.handle().as_ptr(), c"birnam_float_dependency_v1".as_ptr()) };
        if dependency.is_null() { return Err(ModuleError::FloatDependencyRejected { path: path.to_path_buf() }); }
        let attach: birnam_abi::float::FloatDependencyV1 = unsafe { std::mem::transmute(dependency) };
        if unsafe { attach(ops) } != 1 { return Err(ModuleError::FloatDependencyRejected { path: path.to_path_buf() }); }
        self.float = ops;
        Ok(())
    }

    fn attach_word(&mut self, path: &Path, ops: *mut birnam_abi::word::WordApiV1) -> Result<(), ModuleError> {
        if (!self.word.is_null() && self.word != ops) || unsafe { birnam_abi::word::attach(ops) } != 1 {
            return Err(ModuleError::WordDependencyRejected { path: path.to_path_buf() });
        }
        let dependency = unsafe { (self.api().dlsym)(self.floor.handle().as_ptr(), c"birnam_word_dependency_v1".as_ptr()) };
        if dependency.is_null() { return Err(ModuleError::WordDependencyRejected { path: path.to_path_buf() }); }
        let attach: birnam_abi::word::WordDependencyV1 = unsafe { std::mem::transmute(dependency) };
        if unsafe { attach(ops) } != 1 { return Err(ModuleError::WordDependencyRejected { path: path.to_path_buf() }); }
        self.word = ops;
        Ok(())
    }

    fn attach_string(&mut self, path: &Path, ops: *mut birnam_abi::string::StringApiV1) -> Result<(), ModuleError> {
        if (!self.string.is_null() && self.string != ops) || unsafe { birnam_abi::string::attach(ops) } != 1 {
            return Err(ModuleError::StringDependencyRejected { path: path.to_path_buf() });
        }
        let dependency = unsafe { (self.api().dlsym)(self.floor.handle().as_ptr(), c"birnam_string_dependency_v1".as_ptr()) };
        if dependency.is_null() { return Err(ModuleError::StringDependencyRejected { path: path.to_path_buf() }); }
        let attach: birnam_abi::string::StringDependencyV1 = unsafe { std::mem::transmute(dependency) };
        if unsafe { attach(ops) } != 1 { return Err(ModuleError::StringDependencyRejected { path: path.to_path_buf() }); }
        self.string = ops;
        Ok(())
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
        let prepared = self.prepared_float.as_ref().filter(|(prepared, _)| prepared == path)
            .or_else(|| self.prepared_word.as_ref().filter(|(prepared, _)| prepared == path))
            .or_else(|| self.prepared_string.as_ref().filter(|(prepared, _)| prepared == path));
        let handle = match prepared {
            Some((_, handle)) => handle.as_ptr(),
            None => unsafe { (self.floor.api().dlopen)(path_bytes.as_ptr(), RTLD_LAZY) },
        };
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
        self.configure_symbol_borrow(path, handle)?;
        let dependency = unsafe {
            (self.api().dlsym)(
                handle.as_ptr(),
                c"birnam_namespace_names_dependency_v1".as_ptr(),
            )
        };
        if !dependency.is_null() {
            let attach: birnam_abi::namespace_names::DependencyV1 =
                unsafe { std::mem::transmute(dependency) };
            if unsafe { attach(self.floor.namespace_names.as_ptr()) } != 1 {
                return Err(ModuleError::NamespaceNamesDependencyRejected {
                    path: path.to_path_buf(),
                });
            }
        }
        let dependency = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_integer_dependency_v1".as_ptr()) };
        if !dependency.is_null() {
            let attach: birnam_abi::integer::IntegerDependencyV1 = unsafe { std::mem::transmute(dependency) };
            if self.integer.is_null() || unsafe { attach(self.integer) } != 1 {
                return Err(ModuleError::IntegerDependencyRejected { path: path.to_path_buf() });
            }
        }
        let dependency = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_float_dependency_v1".as_ptr()) };
        if !dependency.is_null() {
            let attach: birnam_abi::float::FloatDependencyV1 = unsafe { std::mem::transmute(dependency) };
            if self.float.is_null() || unsafe { attach(self.float) } != 1 {
                return Err(ModuleError::FloatDependencyRejected { path: path.to_path_buf() });
            }
        }
        let dependency = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_string_dependency_v1".as_ptr()) };
        if !dependency.is_null() {
            let attach: birnam_abi::string::StringDependencyV1 = unsafe { std::mem::transmute(dependency) };
            if self.string.is_null() || unsafe { attach(self.string) } != 1 {
                return Err(ModuleError::StringDependencyRejected { path: path.to_path_buf() });
            }
        }
        let dependency = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_range_dependency_v4".as_ptr()) };
        if !dependency.is_null() {
            let attach: birnam_abi::range::DependencyV4 = unsafe { std::mem::transmute(dependency) };
            if self.range.is_null() || unsafe { attach(self.range) } != 1 {
                return Err(ModuleError::RangeDependencyRejected { path: path.to_path_buf() });
            }
        }
        let dependency = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_vector_dependency_v4".as_ptr()) };
        if !dependency.is_null() {
            let attach: birnam_abi::vector::DependencyV4 = unsafe { std::mem::transmute(dependency) };
            if self.vector.is_null() || unsafe { attach(self.vector) } != 1 {
                return Err(ModuleError::VectorDependencyRejected { path: path.to_path_buf() });
            }
        }
        let dependency = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_collection_value_dependency_v1".as_ptr()) };
        if !dependency.is_null() {
            let attach: birnam_abi::collection_value::DependencyV1 = unsafe { std::mem::transmute(dependency) };
            if self.collection_value.is_null() || unsafe { attach(self.collection_value) } != 1 {
                return Err(ModuleError::CollectionValueDependencyRejected { path: path.to_path_buf() });
            }
        }
        let dependency = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_error_value_dependency_v3".as_ptr()) };
        if !dependency.is_null() {
            let attach: birnam_abi::error_value::DependencyV3 = unsafe { std::mem::transmute(dependency) };
            if self.error_value.is_null() || unsafe { attach(self.error_value) } != 1 {
                return Err(ModuleError::ErrorValueDependencyRejected { path: path.to_path_buf() });
            }
        }
        let dependency = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_text_view_dependency_v1".as_ptr()) };
        if !dependency.is_null() {
            let attach: birnam_abi::text_view::DependencyV1 = unsafe { std::mem::transmute(dependency) };
            if self.text_view.is_null() || unsafe { attach(self.text_view) } != 1 {
                return Err(ModuleError::TextViewDependencyRejected { path: path.to_path_buf() });
            }
        }
        let dependency = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_system_output_dependency_v1".as_ptr()) };
        if !dependency.is_null() {
            let attach: birnam_abi::system_output::DependencyV1 = unsafe { std::mem::transmute(dependency) };
            if self.system_output.is_null() || unsafe { attach(self.system_output) } != 1 {
                return Err(ModuleError::SystemOutputDependencyRejected { path: path.to_path_buf() });
            }
        }
        let dependency = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_system_input_dependency_v3".as_ptr()) };
        if !dependency.is_null() {
            let attach: birnam_abi::system_input::DependencyV3 = unsafe { std::mem::transmute(dependency) };
            if self.system_input.is_null() || unsafe { attach(self.system_input) } != 1 {
                return Err(ModuleError::SystemInputDependencyRejected { path: path.to_path_buf() });
            }
        }
        let dependency = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_environment_dependency_v3".as_ptr()) };
        if !dependency.is_null() {
            let attach: birnam_abi::environment::DependencyV3 = unsafe { std::mem::transmute(dependency) };
            if self.environment.is_null() || unsafe { attach(self.environment) } != 1 {
                return Err(ModuleError::EnvironmentDependencyRejected { path: path.to_path_buf() });
            }
        }
        let dependency = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_file_io_dependency_v3".as_ptr()) };
        if !dependency.is_null() {
            let attach: birnam_abi::file_io::DependencyV3 = unsafe { std::mem::transmute(dependency) };
            if self.file_io.is_null() || unsafe { attach(self.file_io) } != 1 {
                return Err(ModuleError::FileIoDependencyRejected { path: path.to_path_buf() });
            }
        }
        let dependency = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_block_loop_dependency_v3".as_ptr()) };
        if !dependency.is_null() {
            let attach: birnam_abi::block_loop::DependencyV3 = unsafe { std::mem::transmute(dependency) };
            if self.block_loop.is_null() || unsafe { attach(self.block_loop) } != 1 {
                return Err(ModuleError::BlockLoopDependencyRejected { path: path.to_path_buf() });
            }
        }
        let dependency = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_native_loader_dependency_v5".as_ptr()) };
        if !dependency.is_null() {
            let attach: birnam_abi::native_loader::DependencyV5 = unsafe { std::mem::transmute(dependency) };
            if self.native_loader.is_null() || unsafe { attach(self.native_loader) } != 1 {
                return Err(ModuleError::NativeLoaderDependencyRejected { path: path.to_path_buf() });
            }
        }
        let dependency = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_source_loader_dependency_v3".as_ptr()) };
        if !dependency.is_null() {
            let attach: birnam_abi::source_loader::DependencyV3 = unsafe { std::mem::transmute(dependency) };
            if self.source_loader.is_null() || unsafe { attach(self.source_loader) } != 1 {
                return Err(ModuleError::SourceLoaderDependencyRejected { path: path.to_path_buf() });
            }
        }
        let dependency = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_system_gc_dependency_v1".as_ptr()) };
        if !dependency.is_null() {
            let attach: birnam_abi::system_gc::DependencyV1 = unsafe { std::mem::transmute(dependency) };
            if self.system_gc.is_null() || unsafe { attach(self.system_gc) } != 1 {
                return Err(ModuleError::SystemGcDependencyRejected { path: path.to_path_buf() });
            }
        }
        let dependency = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_printing_dependency_v1".as_ptr()) };
        if !dependency.is_null() {
            let attach: birnam_abi::printing::DependencyV1 = unsafe { std::mem::transmute(dependency) };
            if self.printing.is_null() || unsafe { attach(self.printing) } != 1 {
                return Err(ModuleError::PrintingDependencyRejected { path: path.to_path_buf() });
            }
        }
        let dependency = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_string_collection_dependency_v1".as_ptr()) };
        if !dependency.is_null() {
            let attach: birnam_abi::string_collection::DependencyV1 = unsafe { std::mem::transmute(dependency) };
            if self.string_collection.is_null() || unsafe { attach(self.string_collection) } != 1 {
                return Err(ModuleError::StringCollectionDependencyRejected { path: path.to_path_buf() });
            }
        }
        let dependency = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_map_dependency_v4".as_ptr()) };
        if !dependency.is_null() {
            let attach: birnam_abi::map::DependencyV4 = unsafe { std::mem::transmute(dependency) };
            if self.map.is_null() || unsafe { attach(self.map) } != 1 {
                return Err(ModuleError::MapDependencyRejected { path: path.to_path_buf() });
            }
        }
        let prepare = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_word_prepare_v4".as_ptr()) };
        let word_provider = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_word_provider_v1".as_ptr()) };
        let canonical_word = path.file_name().and_then(|name| name.to_str()) == Some("libbirnam_word.dylib");
        if prepare.is_null() != word_provider.is_null() || (canonical_word && prepare.is_null()) {
            return Err(ModuleError::WordDependencyRejected { path: path.to_path_buf() });
        }
        if !prepare.is_null() {
            // This opened library remains pinned even if startup fails after
            // it publishes roots. Attach primitives before publication reentry.
            let prepare: birnam_abi::word::WordPrepareV4 = unsafe { std::mem::transmute(prepare) };
            let operations = unsafe { prepare(self.api() as *const IdApi as *mut IdApi, self.presence(), self.imports(), self.exports()) };
            self.attach_word(path, operations)?;
        }
        let dependency = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_word_dependency_v1".as_ptr()) };
        if !dependency.is_null() {
            let attach: birnam_abi::word::WordDependencyV1 = unsafe { std::mem::transmute(dependency) };
            if self.word.is_null() || unsafe { attach(self.word) } != 1 {
                return Err(ModuleError::WordDependencyRejected { path: path.to_path_buf() });
            }
        }
        // SAFETY: module_init is the frozen higher-rung ABI and the floor table
        // remains live for the module's initialization call.
        let init: birnam_abi::ModuleInitFn = unsafe { std::mem::transmute(init.as_ptr()) };
        unsafe {
            init(
                self.floor.api() as *const IdApi as *mut IdApi,
                std::ptr::null_mut(),
                self.floor.presence(),
                self.floor.imports(),
                self.floor.exports(),
                self.floor.vtables(),
            )
        };
        let range_provider = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_range_provider_v4".as_ptr()) };
        if !range_provider.is_null() {
            let provide: birnam_abi::range::ProviderV4 = unsafe { std::mem::transmute(range_provider) };
            let provided = unsafe { provide() };
            if unsafe { birnam_abi::range::attach(provided) } != 1 {
                return Err(ModuleError::RangeDependencyRejected { path: path.to_path_buf() });
            }
            self.range = provided;
        }
        let vector_provider = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_vector_provider_v4".as_ptr()) };
        if !vector_provider.is_null() {
            let provide: birnam_abi::vector::ProviderV4 = unsafe { std::mem::transmute(vector_provider) };
            let provided = unsafe { provide() };
            if unsafe { birnam_abi::vector::attach(provided) } != 1 {
                return Err(ModuleError::VectorDependencyRejected { path: path.to_path_buf() });
            }
            self.vector = provided;
        }
        let collection_value_provider = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_collection_value_provider_v1".as_ptr()) };
        if !collection_value_provider.is_null() {
            let provide: birnam_abi::collection_value::ProviderV1 = unsafe { std::mem::transmute(collection_value_provider) };
            let provided = unsafe { provide() };
            if unsafe { birnam_abi::collection_value::attach(provided) } != 1 {
                return Err(ModuleError::CollectionValueDependencyRejected { path: path.to_path_buf() });
            }
            self.collection_value = provided;
        }
        let error_value_provider = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_error_value_provider_v3".as_ptr()) };
        if !error_value_provider.is_null() {
            let provide: birnam_abi::error_value::ProviderV3 = unsafe { std::mem::transmute(error_value_provider) };
            let provided = unsafe { provide() };
            if unsafe { birnam_abi::error_value::attach(provided) } != 1 {
                return Err(ModuleError::ErrorValueDependencyRejected { path: path.to_path_buf() });
            }
            self.error_value = provided;
        }
        let text_view_provider = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_text_view_provider_v1".as_ptr()) };
        if !text_view_provider.is_null() {
            let provide: birnam_abi::text_view::ProviderV1 = unsafe { std::mem::transmute(text_view_provider) };
            let provided = unsafe { provide() };
            if unsafe { birnam_abi::text_view::attach(provided) } != 1 {
                return Err(ModuleError::TextViewDependencyRejected { path: path.to_path_buf() });
            }
            self.text_view = provided;
        }
        let system_output_provider = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_system_output_provider_v1".as_ptr()) };
        if !system_output_provider.is_null() {
            let provide: birnam_abi::system_output::ProviderV1 = unsafe { std::mem::transmute(system_output_provider) };
            let provided = unsafe { provide() };
            if unsafe { birnam_abi::system_output::attach(provided) } != 1 {
                return Err(ModuleError::SystemOutputDependencyRejected { path: path.to_path_buf() });
            }
            self.system_output = provided;
        }
        let system_input_provider = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_system_input_provider_v3".as_ptr()) };
        if !system_input_provider.is_null() {
            let provide: birnam_abi::system_input::ProviderV3 = unsafe { std::mem::transmute(system_input_provider) };
            let provided = unsafe { provide() };
            if unsafe { birnam_abi::system_input::attach(provided) } != 1 {
                return Err(ModuleError::SystemInputDependencyRejected { path: path.to_path_buf() });
            }
            self.system_input = provided;
        }
        let environment_provider = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_environment_provider_v3".as_ptr()) };
        if !environment_provider.is_null() {
            let provide: birnam_abi::environment::ProviderV3 = unsafe { std::mem::transmute(environment_provider) };
            let provided = unsafe { provide() };
            if unsafe { birnam_abi::environment::attach(provided) } != 1 {
                return Err(ModuleError::EnvironmentDependencyRejected { path: path.to_path_buf() });
            }
            self.environment = provided;
        }
        let file_io_provider = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_file_io_provider_v3".as_ptr()) };
        if !file_io_provider.is_null() {
            let provide: birnam_abi::file_io::ProviderV3 = unsafe { std::mem::transmute(file_io_provider) };
            let provided = unsafe { provide() };
            if unsafe { birnam_abi::file_io::attach(provided) } != 1 {
                return Err(ModuleError::FileIoDependencyRejected { path: path.to_path_buf() });
            }
            self.file_io = provided;
        }
        let block_loop_provider = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_block_loop_provider_v3".as_ptr()) };
        if !block_loop_provider.is_null() {
            let provide: birnam_abi::block_loop::ProviderV3 = unsafe { std::mem::transmute(block_loop_provider) };
            let provided = unsafe { provide() };
            if unsafe { birnam_abi::block_loop::attach(provided) } != 1 {
                return Err(ModuleError::BlockLoopDependencyRejected { path: path.to_path_buf() });
            }
            self.block_loop = provided;
        }
        let native_loader_provider = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_native_loader_provider_v5".as_ptr()) };
        if !native_loader_provider.is_null() {
            let provide: birnam_abi::native_loader::ProviderV5 = unsafe { std::mem::transmute(native_loader_provider) };
            let provided = unsafe { provide() };
            if unsafe { birnam_abi::native_loader::attach(provided) } != 1 {
                return Err(ModuleError::NativeLoaderDependencyRejected { path: path.to_path_buf() });
            }
            self.native_loader = provided;
        }
        let source_loader_provider = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_source_loader_provider_v3".as_ptr()) };
        if !source_loader_provider.is_null() {
            let provide: birnam_abi::source_loader::ProviderV3 = unsafe { std::mem::transmute(source_loader_provider) };
            let provided = unsafe { provide() };
            if unsafe { birnam_abi::source_loader::attach(provided) } != 1 {
                return Err(ModuleError::SourceLoaderDependencyRejected { path: path.to_path_buf() });
            }
            self.source_loader = provided;
        }
        let system_gc_provider = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_system_gc_provider_v1".as_ptr()) };
        if !system_gc_provider.is_null() {
            let provide: birnam_abi::system_gc::ProviderV1 = unsafe { std::mem::transmute(system_gc_provider) };
            let provided = unsafe { provide() };
            if unsafe { birnam_abi::system_gc::attach(provided) } != 1 {
                return Err(ModuleError::SystemGcDependencyRejected { path: path.to_path_buf() });
            }
            self.system_gc = provided;
        }
        let printing_provider = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_printing_provider_v1".as_ptr()) };
        if !printing_provider.is_null() {
            let provide: birnam_abi::printing::ProviderV1 = unsafe { std::mem::transmute(printing_provider) };
            let provided = unsafe { provide() };
            if unsafe { birnam_abi::printing::attach(provided) } != 1 {
                return Err(ModuleError::PrintingDependencyRejected { path: path.to_path_buf() });
            }
            self.printing = provided;
        }
        let string_collection_provider = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_string_collection_provider_v1".as_ptr()) };
        if !string_collection_provider.is_null() {
            let provide: birnam_abi::string_collection::ProviderV1 = unsafe { std::mem::transmute(string_collection_provider) };
            let provided = unsafe { provide() };
            if unsafe { birnam_abi::string_collection::attach(provided) } != 1 {
                return Err(ModuleError::StringCollectionDependencyRejected { path: path.to_path_buf() });
            }
            self.string_collection = provided;
        }
        let map_provider = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_map_provider_v4".as_ptr()) };
        if !map_provider.is_null() {
            let provide: birnam_abi::map::ProviderV4 = unsafe { std::mem::transmute(map_provider) };
            let provided = unsafe { provide() };
            if unsafe { birnam_abi::map::attach(provided) } != 1 {
                return Err(ModuleError::MapDependencyRejected { path: path.to_path_buf() });
            }
            self.map = provided;
        }
        let provider = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_integer_provider_v1".as_ptr()) };
        if !provider.is_null() {
            let provide: unsafe extern "C" fn() -> *mut birnam_abi::integer::IntegerApiV1 = unsafe { std::mem::transmute(provider) };
            let integer = unsafe { provide() };
            if !self.integer.is_null() || unsafe { birnam_abi::integer::attach(integer) } != 1 {
                return Err(ModuleError::IntegerDependencyRejected { path: path.to_path_buf() });
            }
            let dependency = unsafe { (self.api().dlsym)(self.floor.handle().as_ptr(), c"birnam_integer_dependency_v1".as_ptr()) };
            if dependency.is_null() { return Err(ModuleError::IntegerDependencyRejected { path: path.to_path_buf() }); }
            let attach: birnam_abi::integer::IntegerDependencyV1 = unsafe { std::mem::transmute(dependency) };
            if unsafe { attach(integer) } != 1 { return Err(ModuleError::IntegerDependencyRejected { path: path.to_path_buf() }); }
            self.integer = integer;
        }
        let provider = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_float_provider_v1".as_ptr()) };
        if !provider.is_null() {
            let provide: birnam_abi::float::FloatProviderV1 = unsafe { std::mem::transmute(provider) };
            self.attach_float(path, unsafe { provide() })?;
        } else if self.prepared_float.as_ref().is_some_and(|(prepared, _)| prepared == path) {
            return Err(ModuleError::FloatDependencyRejected { path: path.to_path_buf() });
        }
        let string_provider = unsafe { (self.api().dlsym)(handle.as_ptr(), c"birnam_string_provider_v1".as_ptr()) };
        if !string_provider.is_null() {
            let provide: birnam_abi::string::StringProviderV1 = unsafe { std::mem::transmute(string_provider) };
            self.attach_string(path, unsafe { provide() })?;
        } else if self.prepared_string.as_ref().is_some_and(|(prepared, _)| prepared == path) {
            return Err(ModuleError::StringDependencyRejected { path: path.to_path_buf() });
        }
        if !word_provider.is_null() {
            let provide: birnam_abi::word::WordProviderV1 = unsafe { std::mem::transmute(word_provider) };
            self.attach_word(path, unsafe { provide() })?;
        }
        let provider = unsafe {
            (self.api().dlsym)(handle.as_ptr(), c"birnam_trace_provider_v1".as_ptr())
        };
        if !provider.is_null() {
            let provide: birnam_abi::trace_live::ProviderV1 = unsafe { std::mem::transmute(provider) };
            let dependency = unsafe {
                (self.api().dlsym)(self.floor.handle().as_ptr(), c"birnam_trace_dependency_v1".as_ptr())
            };
            if dependency.is_null() {
                return Err(ModuleError::TraceDependencyRejected { path: path.to_path_buf() });
            }
            let attach: birnam_abi::trace_live::DependencyV1 = unsafe { std::mem::transmute(dependency) };
            if unsafe { attach(provide()) } != 1 {
                return Err(ModuleError::TraceDependencyRejected { path: path.to_path_buf() });
            }
        }
        let provider = unsafe {
            (self.api().dlsym)(handle.as_ptr(), c"birnam_collect_provider_v1".as_ptr())
        };
        if !provider.is_null() {
            let provide: birnam_abi::collect::ProviderV1 = unsafe { std::mem::transmute(provider) };
            let dependency = unsafe {
                (self.api().dlsym)(self.floor.handle().as_ptr(), c"birnam_collect_dependency_v1".as_ptr())
            };
            if dependency.is_null() {
                return Err(ModuleError::CollectDependencyRejected { path: path.to_path_buf() });
            }
            let attach: birnam_abi::collect::DependencyV1 = unsafe { std::mem::transmute(dependency) };
            if unsafe { attach(provide()) } != 1 {
                return Err(ModuleError::CollectDependencyRejected { path: path.to_path_buf() });
            }
        }
        self.modules.push(handle);
        Ok(handle)
    }

    pub fn load_module(&mut self, path: &Path) -> Result<(), ModuleError> {
        self.load_module_handle(path).map(|_| ())
    }

    /// Resolve and load the canonical native stack in dependency order. TCC
    /// retains its exact override path; every other rung, including Store,
    /// comes from the one executable-relative runtime library directory.
    pub fn load_script_modules(&mut self, resources: &Resources) -> Result<(), ModuleError> {
        let tcc_override = env::var_os("BIRNAM_TCC_DYLIB").map(PathBuf::from);
        let mut store_compensation_api = None;
        for name in SCRIPT_MODULES {
            if *name == "libbirnam_error_value" {
                self.prepare_string(&resources.lib_dir.join("libbirnam_string.dylib"))?;
                self.prepare_word(&resources.lib_dir.join("libbirnam_word.dylib"))?;
                self.prepare_float(&resources.lib_dir.join("libbirnam_float.dylib"))?;
            }
            let path = if *name == "libbirnam_id_tcc" {
                tcc_override
                    .as_deref()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| resources.lib_dir.join("libbirnam_id_tcc.dylib"))
            } else {
                resources.lib_dir.join(format!("{name}.dylib"))
            };
            let handle = self.load_module_handle(&path)?;
            if *name == "libbirnam_trace" {
                let provider = unsafe {
                    (self.api().dlsym)(handle.as_ptr(), c"birnam_trace_provider_v1".as_ptr())
                };
                if provider.is_null() {
                    return Err(ModuleError::TraceDependencyRejected { path });
                }
            }
            if *name == "libbirnam_collect" {
                let provider = unsafe {
                    (self.api().dlsym)(handle.as_ptr(), c"birnam_collect_provider_v1".as_ptr())
                };
                if provider.is_null() {
                    return Err(ModuleError::CollectDependencyRejected { path });
                }
            }
            if *name == "libbirnam_store" {
                let getter = unsafe {
                    (self.floor.api().dlsym)(
                        handle.as_ptr(),
                        c"birnam_store_compensation_api_v1".as_ptr().cast(),
                    )
                };
                let Some(getter) = NonNull::new(getter) else {
                    return Err(ModuleError::MissingStoreCompensationApi { path });
                };
                let getter: StoreCompensationApiGetter =
                    unsafe { std::mem::transmute(getter.as_ptr()) };
                let table = unsafe { getter() };
                let Some(table) = NonNull::new(table.cast_mut()) else {
                    return Err(ModuleError::MissingStoreCompensationApi { path });
                };
                store_compensation_api = Some(table);
            }
            if *name == "libbirnam_id_codegen" {
                let Some(table) = store_compensation_api else {
                    return Err(ModuleError::MissingStoreCompensationApi { path });
                };
                let configure = unsafe {
                    (self.floor.api().dlsym)(
                        handle.as_ptr(),
                        c"birnam_codegen_store_compensation_configure_v1"
                            .as_ptr()
                            .cast(),
                    )
                };
                let Some(configure) = NonNull::new(configure) else {
                    return Err(ModuleError::MissingCodegenCompensationConfig { path });
                };
                let configure: CodegenCompensationConfigure =
                    unsafe { std::mem::transmute(configure.as_ptr()) };
                if unsafe { configure(table.as_ptr()) } != 1 {
                    return Err(ModuleError::StoreCompensationConfigRejected { path });
                }
            }
            if *name == "libbirnam_id_image"
                && path.file_name().and_then(|file| file.to_str())
                    == Some("libbirnam_id_image.dylib")
            {
                // The canonical Rust Image module must be allowed to publish
                // through the ordinary host path.
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
    let Some(driver) = NonNull::new(lookup(c"birnam_isolate_worker_driver_v2")) else {
        return Err(IsolateHookError::MissingSymbol(
            "birnam_isolate_worker_driver_v2",
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

/// Start the Rust host through the floor → native stack → MOP mirror →
/// self-hosted sequence. Resource resolution is kept outside this function so
/// callers can inspect or override it before any dylib is opened.
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
    let store = unsafe { birnam_abi::namespace_import::import(stack.imports(), c"Store".as_ptr()) };
    if store != api.nilObj {
        let selector = unsafe { (api.intern)(c"backfillMirror".as_ptr()) };
        unsafe { (api.send)(store, selector, std::ptr::null(), 0) };
    }
    let codegen = unsafe { birnam_abi::namespace_import::import(stack.imports(), c"Codegen".as_ptr()) };
    if codegen == api.nilObj {
        return Err(RuntimeStartError::MissingCodegen);
    }
    let self_hosted = unsafe {
        bootstrap_self_hosted(api, stack.presence(), stack.imports(),
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

/// Resolve production resources and start the Rust host using its environment
/// contract.
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
    pub fn presence(&self) -> *const birnam_abi::namespace_presence::ApiV1 { self.namespace_presence.as_ptr() }
    pub fn imports(&self) -> *const birnam_abi::namespace_import::ApiV1 { self.namespace_import.as_ptr() }
    pub fn exports(&self) -> *const birnam_abi::namespace_export::ApiV1 { self.namespace_export.as_ptr() }
    pub fn vtables(&self) -> *const birnam_abi::vtable_enumeration::ApiV1 { self.vtable_enumeration.as_ptr() }

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
    let namespace_path = path.with_file_name("libbirnam_namespace.dylib");
    let object_model_path = path.with_file_name("libbirnam_object_model.dylib");
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
    // Namespace supplies only an inert factory before Id exists. The floor
    // constructs its own instance at the existing bootstrap point. Selection
    // is explicit here; Id has no loader fallback or linked implementation.
    let namespace = match unsafe { prepare_namespace(handle, &namespace_path) } {
        Ok(namespace) => namespace,
        Err(error) => {
            unsafe { system_dlclose(handle.as_ptr()) };
            return Err(error);
        }
    };
    let object_model = match unsafe { prepare_vtable_enumeration(handle, &object_model_path) } {
        Ok(image) => image,
        Err(error) => {
            unsafe { system_dlclose(handle.as_ptr()) };
            return Err(error);
        }
    };
    // SAFETY: `init` is the frozen `IdInitFn` export and the dylib remains live.
    let init: IdInitFn = unsafe { std::mem::transmute(init.as_ptr()) };
    let api = NonNull::new(unsafe { init() }).ok_or_else(|| {
        unsafe { system_dlclose(handle.as_ptr()) };
        FloorError::NullApi
    })?;
    let names_provider = unsafe {
        system_dlsym(
            handle.as_ptr(),
            c"birnam_namespace_names_provider_v1".as_ptr(),
        )
    };
    if names_provider.is_null() {
        return Err(FloorError::Namespace {
            path: namespace_path,
            reason: "missing names capability",
        });
    }
    let provide: birnam_abi::namespace_names::ProviderV1 =
        unsafe { std::mem::transmute(names_provider) };
    let names = unsafe { provide() };
    if !unsafe { birnam_abi::namespace_names::valid(names) } {
        return Err(FloorError::Namespace {
            path: namespace_path,
            reason: "invalid names capability",
        });
    }
    let presence_provider = unsafe {
        system_dlsym(handle.as_ptr(), c"birnam_namespace_presence_provider_v1".as_ptr())
    };
    if presence_provider.is_null() {
        return Err(FloorError::Namespace { path: namespace_path, reason: "missing presence capability" });
    }
    let provide: birnam_abi::namespace_presence::ProviderV1 = unsafe { std::mem::transmute(presence_provider) };
    let presence = unsafe { provide() };
    if !unsafe { birnam_abi::namespace_presence::valid(presence) } {
        return Err(FloorError::Namespace { path: namespace_path, reason: "invalid presence capability" });
    }
    let import_provider = unsafe {
        system_dlsym(handle.as_ptr(), c"birnam_namespace_import_provider_v1".as_ptr())
    };
    if import_provider.is_null() {
        return Err(FloorError::Namespace { path: namespace_path, reason: "missing import capability" });
    }
    let provide: birnam_abi::namespace_import::ProviderV1 = unsafe { std::mem::transmute(import_provider) };
    let import = unsafe { provide() };
    if !unsafe { birnam_abi::namespace_import::valid(import) } {
        return Err(FloorError::Namespace { path: namespace_path, reason: "invalid import capability" });
    }
    let export_provider = unsafe {
        system_dlsym(handle.as_ptr(), c"birnam_namespace_export_provider_v1".as_ptr())
    };
    if export_provider.is_null() {
        return Err(FloorError::Namespace { path: namespace_path, reason: "missing export capability" });
    }
    let provide: birnam_abi::namespace_export::ProviderV1 = unsafe { std::mem::transmute(export_provider) };
    let export = unsafe { provide() };
    if !unsafe { birnam_abi::namespace_export::valid(export) } {
        return Err(FloorError::Namespace { path: namespace_path, reason: "invalid export capability" });
    }
    let vtables_provider = unsafe { system_dlsym(handle.as_ptr(), c"birnam_vtable_enumeration_provider_v1".as_ptr()) };
    if vtables_provider.is_null() {
        return Err(FloorError::VtableEnumeration { path: object_model_path, reason: "missing capability" });
    }
    let provide: birnam_abi::vtable_enumeration::ProviderV1 = unsafe { std::mem::transmute(vtables_provider) };
    let vtables = unsafe { provide() };
    if !unsafe { birnam_abi::vtable_enumeration::valid(vtables) } {
        return Err(FloorError::VtableEnumeration { path: object_model_path, reason: "invalid capability" });
    }
    Ok(LoadedFloor {
        _object_model: object_model,
        vtable_enumeration: NonNull::new(vtables).unwrap(),
        namespace_export: NonNull::new(export).unwrap(),
        namespace_import: NonNull::new(import).unwrap(),
        namespace_presence: NonNull::new(presence).unwrap(),
        namespace_names: NonNull::new(names).unwrap(),
        _namespace: namespace,
        handle,
        api,
        _owner_thread: PhantomData,
    })
}

// Only host loader operations run here; no Id callbacks exist yet. On success
// the descriptor has escaped into the floor and both libraries stay pinned.
unsafe fn prepare_namespace(
    floor: NonNull<std::ffi::c_void>,
    path: &Path,
) -> Result<NonNull<std::ffi::c_void>, FloorError> {
    let error = |reason| FloorError::Namespace {
        path: path.to_path_buf(),
        reason,
    };
    let attach =
        unsafe { system_dlsym(floor.as_ptr(), c"birnam_namespace_dependency_v8".as_ptr()) };
    if attach.is_null() {
        return Err(error("missing floor dependency"));
    }
    let path_c = CString::new(path_bytes(path)).map_err(|_| error("embedded NUL"))?;
    let image = NonNull::new(unsafe { system_dlopen(path_c.as_ptr(), RTLD_NOW | RTLD_LOCAL) })
        .ok_or_else(|| error("open failed"))?;
    let result = (|| {
        let provider =
            unsafe { system_dlsym(image.as_ptr(), c"birnam_namespace_provider_v8".as_ptr()) };
        if provider.is_null() {
            return Err(error("missing provider"));
        }
        let provide: birnam_abi::namespace::ProviderV8 = unsafe { std::mem::transmute(provider) };
        let descriptor = unsafe { provide() };
        if !unsafe { birnam_abi::namespace::valid(descriptor) } { return Err(error("invalid descriptor")); }
        let attach: birnam_abi::namespace::DependencyV8 = unsafe { std::mem::transmute(attach) };
        if unsafe { attach(descriptor) } != 1 { return Err(error("dependency rejected")); }
        Ok(image)
    })();
    if result.is_err() { unsafe { system_dlclose(image.as_ptr()) }; }
    result
}

unsafe fn prepare_vtable_enumeration(
    floor: NonNull<std::ffi::c_void>,
    path: &Path,
) -> Result<NonNull<std::ffi::c_void>, FloorError> {
    let error = |reason| FloorError::VtableEnumeration {
        path: path.to_path_buf(),
        reason,
    };
    let attach =
        unsafe { system_dlsym(floor.as_ptr(), c"birnam_vtable_enumeration_dependency_v1".as_ptr()) };
    if attach.is_null() {
        return Err(error("missing floor dependency"));
    }
    let path_c = CString::new(path_bytes(path)).map_err(|_| error("embedded NUL"))?;
    let image = NonNull::new(unsafe { system_dlopen(path_c.as_ptr(), RTLD_NOW | RTLD_LOCAL) })
        .ok_or_else(|| error("open failed"))?;
    let result = (|| {
        let provider =
            unsafe { system_dlsym(image.as_ptr(), c"birnam_vtable_enumeration_factory_v1".as_ptr()) };
        if provider.is_null() {
            return Err(error("missing provider"));
        }
        let provide: birnam_abi::vtable_enumeration::FactoryProviderV1 = unsafe { std::mem::transmute(provider) };
        let descriptor = unsafe { provide() };
        if !unsafe { birnam_abi::vtable_enumeration::valid_factory(descriptor) } { return Err(error("invalid descriptor")); }
        let attach: birnam_abi::vtable_enumeration::DependencyV1 = unsafe { std::mem::transmute(attach) };
        if unsafe { attach(descriptor) } != 1 { return Err(error("dependency rejected")); }
        Ok(image)
    })();
    if result.is_err() { unsafe { system_dlclose(image.as_ptr()) }; }
    result
}

/// Load the selected Rust floor using optional identity and digest checks.
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

pub(super) fn path_bytes(path: &Path) -> Vec<u8> {
    #[cfg(unix)]
    {
        path.as_os_str().as_bytes().to_vec()
    }
    #[cfg(not(unix))]
    {
        path.to_string_lossy().as_bytes().to_vec()
    }
}







#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn compatibility_symbol_limits_require_positive_machine_word_values() {
        assert_eq!(parse_positive_limit(OsStr::new("1")), Some(1));
        assert_eq!(parse_positive_limit(OsStr::new("1048576")), Some(1_048_576));
        assert_eq!(parse_positive_limit(OsStr::new("0")), None);
        assert_eq!(parse_positive_limit(OsStr::new("-1")), None);
        assert_eq!(parse_positive_limit(OsStr::new("unbounded")), None);
    }

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

    fn artifact_fixture_with_kind(kind: &[u8], identity: &[u8], requires: &[&[u8]]) -> Vec<u8> {
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
                let path = std::env::var("BIRNAM_PREFLIGHT_SENTINEL_DYLIB")
                    .expect("shared sentinel case requires a real sentinel dylib");
                let mut dylib = std::fs::read(path).expect("real sentinel dylib reads");
                dylib.extend_from_slice(&bytes);
                bytes = dylib;
            }
            let path = root.join(&filename);
            std::fs::write(&path, bytes).expect("shared fixture writes");
            if filename == "root.dylib" {
                root_path = Some(path);
            }
        }
        root_path.expect("shared fixture root")
    }

    fn shared_path_token(path: &Path) -> String {
        path.file_name()
            .expect("shared fixture path has a filename")
            .to_string_lossy()
            .into_owned()
    }

    fn shared_kind(path: &Path) -> String {
        let bytes = std::fs::read(path).expect("shared fixture descriptor reads");
        let descriptor =
            birnam_artifact::inspect(&bytes).expect("shared fixture descriptor parses");
        String::from_utf8_lossy(descriptor.kind).into_owned()
    }

    fn record_shared_parity_row(case: &str, result: &Result<ArtifactPlan, ArtifactPreflightError>) {
        let Some(output) = std::env::var_os("BIRNAM_PREFLIGHT_PARITY_OUTPUT") else {
            return;
        };
        let (outcome, detail, first) = match result {
            Ok(plan) => (
                "ok".to_owned(),
                format!(
                    "order={}",
                    plan.entries
                        .iter()
                        .map(|entry| String::from_utf8_lossy(&entry.identity).into_owned())
                        .collect::<Vec<_>>()
                        .join(",")
                ),
                "none".to_owned(),
            ),
            Err(ArtifactPreflightError::IdentityMismatch {
                path,
                expected,
                actual,
            }) => (
                "identityMismatch".to_owned(),
                format!(
                    "expected={};actual={}",
                    String::from_utf8_lossy(expected),
                    String::from_utf8_lossy(actual)
                ),
                shared_path_token(path),
            ),
            Err(ArtifactPreflightError::KindMismatch { path }) => (
                "kindMismatch".to_owned(),
                format!("expected=module;actual={}", shared_kind(path)),
                shared_path_token(path),
            ),
            Err(ArtifactPreflightError::DependencyInvalid { path, dependency }) => (
                "dependencyInvalid".to_owned(),
                format!("dependency={}", String::from_utf8_lossy(dependency)),
                shared_path_token(path),
            ),
            Err(error) => panic!("shared parity case {case} emitted unsupported {error:?}"),
        };
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(output)
            .expect("shared parity output opens");
        writeln!(file, "{case}|{outcome}|{detail}|first={first}")
            .expect("shared parity row writes");
    }






    #[test]
    fn sha256_matches_the_floor_digest_format() {
        assert_eq!(
            sha256_hex(b"abc"),
            *b"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
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

        let mut wrong_source = module.clone();
        wrong_source.source_digest = "changed".to_owned();
        assert!(matches!(
            validate_project_compiled_artifact(&wrong_source, &bytes),
            Err(ProjectCompiledArtifactValidationError::SourceDigestMismatch)
        ));
    }






    #[test]
    fn missing_floor_is_rejected_before_dlopen() {
        assert!(matches!(
            load_floor(Path::new("/tmp/birnam-host-no-floor.dylib"), None, None),
            Err(FloorError::Read { .. })
        ));
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
                "isolate",
                "stacklessControl"
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
    fn baked_image_file_reader_skips_the_runtime_prefix() {
        let path =
            std::env::temp_dir().join(format!("birnam-host-baked-reader-{}", std::process::id()));
        let image = b"BIRNAMIM\x08\0\0\0payload";
        let baked = bake_executable(&vec![b'r'; 1 << 20], image).expect("fixture bakes");
        std::fs::write(&path, baked).expect("fixture writes");
        assert_eq!(
            read_baked_image(&path).expect("fixture reads"),
            Some(image.to_vec())
        );
        std::fs::write(&path, b"plain runtime").expect("plain fixture writes");
        assert_eq!(read_baked_image(&path).expect("plain fixture reads"), None);
        std::fs::remove_file(path).expect("fixture removes");
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
        let root =
            std::env::temp_dir().join(format!("birnam-host-preflight-kind-{}", std::process::id()));
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
    fn artifact_preflight_shared_fixture_matrix_matches_reference_cases() {
        let mut cases = vec![
            "path-only-root",
            "supplied-identity-mismatch",
            "wrong-kind-before-walk",
            "nested-wrong-kind",
            "sibling-identity-mismatch",
            "missing-sibling",
            "ordered-valid-closure",
        ];
        if std::env::var_os("BIRNAM_PREFLIGHT_SENTINEL_DYLIB").is_some() {
            // The cold-bootstrap parity harnesses provide a real
            // aborting initializer. Ordinary Cargo tests remain self-contained
            // and exercise the inert corpus without requiring an external
            // fixture outside the workspace.
            cases.push("sentinel");
        }
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
                "nested-wrong-kind" => preflight_artifact(&root_path, Some(b"root")),
                "sibling-identity-mismatch" => preflight_artifact(&root_path, None),
                "missing-sibling" => preflight_artifact(&root_path, None),
                "ordered-valid-closure" => preflight_artifact(&root_path, Some(b"root")),
                "sentinel" => preflight_artifact(&root_path, Some(b"sentinel")),
                _ => unreachable!(),
            };
            record_shared_parity_row(case, &result);
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
                "nested-wrong-kind" => {
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

#[cfg(test)]
mod worker_arity_contract_tests {
    #[test]
    fn underscore_worker_predicate_is_not_corrected_by_byte_rule_sharing() {
        assert!(super::selector_has_arity_one("_private"));
        assert!(!super::selector_has_arity_one(""));
        assert!(!super::selector_has_arity_one("value"));
    }
}
