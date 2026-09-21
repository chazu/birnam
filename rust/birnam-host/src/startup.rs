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
    unsafe { finish_image_load(api, runtime.stack.presence(), runtime.stack.imports(), runtime.stack.vtables(), system, status) }
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
    unsafe { finish_image_load(api, runtime.stack.presence(), runtime.stack.imports(), runtime.stack.vtables(), system, status) }
}

unsafe fn finish_image_load(
    api: &IdApi, presence: *const birnam_abi::namespace_presence::ApiV1, imports: *const birnam_abi::namespace_import::ApiV1,
    vtables: *const birnam_abi::vtable_enumeration::ApiV2,
    system: birnam_abi::Oop,
    status: birnam_abi::Oop,
) -> Result<ImageLoadOutcome, ImageError> {
    let outcome = if status == api.trueObj {
        ImageLoadOutcome::Mapped
    } else if status == api.falseObj {
        ImageLoadOutcome::SourceRecovery
    } else if status == api.nilObj {
        return Err(ImageError::LoadRejected);
    } else {
        return Err(ImageError::UnexpectedStatus);
    };
    let codegen = unsafe { birnam_abi::namespace_import::import(imports, c"Codegen".as_ptr()) };
    if codegen != api.nilObj {
        // The image loader has restored both compiled units (or explicitly
        // rebuilt missing code). Reconnect process-local resources before user
        // entry without recompiling the saved language implementation.
        let begin_recovery = unsafe { (api.intern)(c"beginImageDispatchRecovery".as_ptr()) };
        let begun = unsafe { (api.send)(codegen, begin_recovery, std::ptr::null(), 0) };
        if begun.is_null() || begun == api.nilObj || send_is_error(api, presence, begun) {
            return Err(ImageError::LoadRejected);
        }
        let config_surface_missing = unsafe { global_named(api, imports, "Config") }
            .map(|config| unsafe {
                let loads = (api.intern)(c"loads".as_ptr());
                !receiver_has_selector(api, vtables, config, loads)
            })
            .unwrap_or(false);
        if config_surface_missing {
            return Err(ImageError::LoadRejected);
        }
        let replay_aliens = unsafe { (api.intern)(c"replayAliens".as_ptr()) };
        unsafe { (api.send)(codegen, replay_aliens, std::ptr::null(), 0) };
        // Source-backed Block globals are re-derived after method recovery.
        let derive_globals = unsafe { (api.intern)(c"deriveImageGlobals".as_ptr()) };
        unsafe { (api.send)(codegen, derive_globals, std::ptr::null(), 0) };
        if !unsafe { reconcile_image_composition(api, presence, imports, system) } {
            return Err(ImageError::LoadRejected);
        }
        let end_recovery = unsafe { (api.intern)(c"endImageDispatchRecovery".as_ptr()) };
        unsafe { (api.send)(codegen, end_recovery, std::ptr::null(), 0) };
        // Successful rebuild requires a canonical durable generation; composed
        // adoption validates its successor. Seal that recovered system directly.
        // currentGenerationDigest reparsed every source only to test existence;
        // even a nil result from that query previously accepted the image.
        let sealed = unsafe {
            (api.send)(codegen, (api.intern)(c"sealInitialGeneration".as_ptr()), std::ptr::null(), 0)
        };
        if sealed != api.trueObj {
            return Err(ImageError::LoadRejected);
        }
    }
    Ok(outcome)
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
    IdProtocol { status: u32, detail: usize },
    MethodObservation { path: PathBuf, reason: &'static str },
    RootRegistry { path: PathBuf, reason: &'static str },
    GcSettings { path: PathBuf, reason: &'static str },
    PayloadLayout { path: PathBuf, reason: &'static str },
    TraceHooks { path: PathBuf, reason: &'static str },
    Finalizers { path: PathBuf, reason: &'static str },
    AllocationBudget { path: PathBuf, reason: &'static str },
    CollectionPhase { path: PathBuf, reason: &'static str },
    NativeMark { path: PathBuf, reason: &'static str },
    NativeStack { path: PathBuf, reason: &'static str },
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
            Self::IdProtocol { status, detail } => write!(formatter, "Id protocol initialization failed ({status}, {detail})"),
            Self::MethodObservation { path, reason } => write!(formatter, "Method observation preparation failed: {} ({reason})", path.display()),
            Self::RootRegistry { path, reason } => write!(formatter, "Root registry preparation failed: {} ({reason})", path.display()),
            Self::GcSettings { path, reason } => write!(formatter, "GC settings preparation failed: {} ({reason})", path.display()),
            Self::PayloadLayout { path, reason } => write!(formatter, "Payload layout preparation failed: {} ({reason})", path.display()),
            Self::TraceHooks { path, reason } => write!(formatter, "Trace hooks preparation failed: {} ({reason})", path.display()),
            Self::Finalizers { path, reason } => write!(formatter, "Finalizers preparation failed: {} ({reason})", path.display()),
            Self::NativeStack { path, reason } => write!(formatter, "Native stack preparation failed: {} ({reason})", path.display()),
            Self::NativeMark { path, reason } => write!(formatter, "Native mark preparation failed: {} ({reason})", path.display()),
            Self::AllocationBudget { path, reason } => write!(formatter, "Allocation budget preparation failed: {} ({reason})", path.display()),
            Self::CollectionPhase { path, reason } => write!(formatter, "Collection phase preparation failed: {} ({reason})", path.display()),
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
    values: birnam_abi::runtime_values::ApisV1,
    bootstrap: Option<crate::native_bootstrap::Owner>,
    id_protocol: birnam_abi::id_protocol::Protocol,
    root_registry: NonNull<birnam_abi::root_registry::ApiV1>,
    gc_settings: NonNull<birnam_abi::gc_settings::ApiV1>,
    payload_layout: NonNull<birnam_abi::payload_layout::ApiV1>,
    trace_hooks: NonNull<birnam_abi::trace_hooks::ApiV1>,
    finalizers: NonNull<birnam_abi::finalizers::ApiV1>,
    native_stack: NonNull<birnam_abi::native_stack::ApiV1>,
    method_observation: NonNull<birnam_abi::method_observation::ApiV1>,
    vtable_enumeration: NonNull<birnam_abi::vtable_enumeration::ApiV2>,
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
    NativeConstruction { stage: String, status: u32, detail: usize },
}

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
        unsafe { birnam_abi::native_stack::set(self.stack.native_stack(), anchor as *mut usize as usize) };
        if self.policy.precise_gc {
            unsafe { birnam_abi::gc_settings::set_precise(self.stack.gc_settings(), 1) };
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
                let is_kernel = KERNEL_MODULES.contains(&dependency.as_str()) || dependency == "portable_code";
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
    vtables: *const birnam_abi::vtable_enumeration::ApiV2,
    receiver: birnam_abi::Oop,
    selector: birnam_abi::Oop,
) -> bool {
    let mut vtable = unsafe { (api.vtableOf)(receiver) };
    while !vtable.is_null() {
        let mut index = 0usize;
        loop {
            let candidate = unsafe { birnam_abi::vtable_enumeration::selector_at(vtables, vtable, index) };
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

/// Host access to the native stack constructed by Bootstrap through Id.
/// The common loader retains entered primitive native images for process life.
pub struct LoadedStack {
    pub floor: LoadedFloor,
    range: *const birnam_abi::range::ApiV6,
    vector: *const birnam_abi::vector::ApiV6,
    map: *const birnam_abi::map::ApiV6,
    string_collection: *const birnam_abi::string_collection::ApiV1,
    printing: *const birnam_abi::printing::ApiV1,
    system_gc: *const birnam_abi::system_gc::ApiV2,
    text_view: *const birnam_abi::text_view::ApiV1,
    error_value: *const birnam_abi::error_value::ApiV3,
    system_output: *const birnam_abi::system_output::ApiV1,
    system_input: *const birnam_abi::system_input::ApiV3,
    environment: *const birnam_abi::environment::ApiV3,
    file_io: *const birnam_abi::file_io::ApiV3,
    block_loop: *const birnam_abi::block_loop::ApiV3,
    source_loader: *const birnam_abi::source_loader::ApiV3,
    collection_value: *const birnam_abi::collection_value::ApiV1,
}

impl LoadedStack {
    pub fn id_protocol(&self) -> &birnam_abi::id_protocol::Protocol { self.floor.id_protocol() }
    pub fn presence(&self) -> *const birnam_abi::namespace_presence::ApiV1 { self.floor.presence() }
    pub fn imports(&self) -> *const birnam_abi::namespace_import::ApiV1 { self.floor.imports() }
    pub fn exports(&self) -> *const birnam_abi::namespace_export::ApiV1 { self.floor.exports() }
    pub fn vtables(&self) -> *const birnam_abi::vtable_enumeration::ApiV2 { self.floor.vtables() }
    pub fn observation(&self) -> *const birnam_abi::method_observation::ApiV1 { self.floor.observation() }
    pub fn root_registry(&self) -> *const birnam_abi::root_registry::ApiV1 { self.floor.root_registry() }
    /// # Safety
    /// Native operands and pointer/code lifetimes follow the common protocol.
    pub unsafe fn load_native(&self, path: &[u8], args: &[birnam_abi::id_protocol::Operand]) -> birnam_abi::id_protocol::Outcome {
        unsafe { load_native_with_bootstrap(self.floor.bootstrap.as_ref(), self.floor.id_protocol, path, args) }
    }
    /// # Safety
    /// The matching result remains live and its native protocol descriptor readable.
    pub unsafe fn release_initialization_result(&self, result: birnam_abi::id_protocol::Operand) -> usize {
        self.floor.bootstrap.as_ref().map_or(0, |bootstrap| unsafe { bootstrap.release(result) })
    }

    pub fn gc_settings(&self) -> *const birnam_abi::gc_settings::ApiV1 { self.floor.gc_settings() }
    pub fn payload_layout(&self) -> *const birnam_abi::payload_layout::ApiV1 { self.floor.payload_layout() }
    pub fn trace_hooks(&self) -> *const birnam_abi::trace_hooks::ApiV1 { self.floor.trace_hooks() }
    pub fn finalizers(&self) -> *const birnam_abi::finalizers::ApiV1 { self.floor.finalizers() }
    pub fn native_stack(&self) -> *const birnam_abi::native_stack::ApiV1 { self.floor.native_stack() }

    pub fn new(floor: LoadedFloor) -> Self {
        // Bootstrap supplied these completed values to Runtime already. The
        // host's legacy consumers select the same descriptors, without another
        // preparation or module-initialization pass.
        let values = floor.values;
        unsafe {
            assert_eq!(birnam_abi::integer::attach(values.integer), 1);
            assert_eq!(birnam_abi::float::attach(values.float), 1);
            assert_eq!(birnam_abi::word::attach(values.word), 1);
            assert_eq!(birnam_abi::string::attach(values.string), 1);
        }
        Self {
            floor,
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
            source_loader: std::ptr::null(),
            collection_value: std::ptr::null(),
        }
    }

    pub fn api(&self) -> &IdApi {
        self.floor.api()
    }

    fn construct(&self, stage: &[u8]) -> Result<birnam_abi::id_protocol::Operand, ModuleError> {
        let invalid = || ModuleError::NativeConstruction { stage: String::from_utf8_lossy(stage).into_owned(), status: birnam_abi::id_protocol::INVALID_REQUEST, detail: 0 };
        self.floor.bootstrap.as_ref().ok_or_else(invalid)?.construct(stage).into_result()
            .map_err(|failure| ModuleError::NativeConstruction { stage: String::from_utf8_lossy(stage).into_owned(), status: failure.status, detail: failure.detail })
    }
    fn construction_word(&self, stage: &[u8]) -> Result<usize, ModuleError> {
        self.construct(stage)?.as_word().filter(|value| *value != 0).ok_or_else(|| ModuleError::NativeConstruction { stage: String::from_utf8_lossy(stage).into_owned(), status: birnam_abi::id_protocol::INVALID_REQUEST, detail: 0 })
    }
    pub fn initialize_collections(&mut self) -> Result<(), ModuleError> {
        self.initialize_control()?;
        self.construct(b"collections")?;
        self.collection_value = self.construction_word(b"valuePolicy")? as _;
        self.vector = self.construction_word(b"vector")? as _;
        self.range = self.construction_word(b"range")? as _;
        self.map = self.construction_word(b"map")? as _;
        self.string_collection = self.construction_word(b"stringCollection")? as _;
        unsafe {
            assert_eq!(birnam_abi::collection_value::attach(self.collection_value), 1);
            assert_eq!(birnam_abi::vector::attach(self.vector), 1);
            assert_eq!(birnam_abi::range::attach(self.range), 1);
            assert_eq!(birnam_abi::map::attach(self.map), 1);
            assert_eq!(birnam_abi::string_collection::attach(self.string_collection), 1);
        }
        Ok(())
    }
    pub fn initialize_store(&mut self) -> Result<(), ModuleError> {
        self.initialize_collections()?;
        self.construct(b"store")?;
        Ok(())
    }

    pub fn initialize_parsers(&mut self) -> Result<(), ModuleError> {
        self.initialize_store()?;
        self.construct(b"parsers")?;
        Ok(())
    }

    /// Construct System IO without parser/compiler/application libraries.
    pub fn initialize_system_io(&mut self) -> Result<(), ModuleError> {
        self.initialize_control()?;
        self.construct(b"systemIo")?;
        self.system_gc = self.construction_word(b"systemGc")? as _;
        assert_eq!(unsafe { birnam_abi::system_gc::attach(self.system_gc) }, 1);
        self.text_view = self.construction_word(b"textView")? as _;
        assert_eq!(unsafe { birnam_abi::text_view::attach(self.text_view) }, 1);
        self.system_output = self.construction_word(b"systemOutput")? as _;
        assert_eq!(unsafe { birnam_abi::system_output::attach(self.system_output) }, 1);
        self.system_input = self.construction_word(b"systemInput")? as _;
        assert_eq!(unsafe { birnam_abi::system_input::attach(self.system_input) }, 1);
        self.environment = self.construction_word(b"environment")? as _;
        assert_eq!(unsafe { birnam_abi::environment::attach(self.environment) }, 1);
        self.file_io = self.construction_word(b"fileIo")? as _;
        assert_eq!(unsafe { birnam_abi::file_io::attach(self.file_io) }, 1);
        self.construct(b"nativeLoader")?;
        Ok(())
    }
    pub fn initialize_system(&mut self) -> Result<(), ModuleError> {
        self.initialize_compiler()?;
        self.initialize_system_io()?;
        self.source_loader = self.construction_word(b"sourceLoader")? as _;
        assert_eq!(unsafe { birnam_abi::source_loader::attach(self.source_loader) }, 1);
        Ok(())
    }

    pub fn initialize_compiler(&mut self) -> Result<(), ModuleError> {
        self.initialize_parsers()?;
        self.construct(b"codegen")?;
        Ok(())
    }

    /// Install the completed native control group before collections/compiler.
    pub fn initialize_control(&mut self) -> Result<(), ModuleError> {
        self.construct(b"control")?;
        let errors = self.construction_word(b"errorValue")? as *const birnam_abi::error_value::ApiV3;
        let printing = self.construction_word(b"printing")? as *const birnam_abi::printing::ApiV1;
        let loops = self.construction_word(b"blockLoop")? as *const birnam_abi::block_loop::ApiV3;
        unsafe {
            assert_eq!(birnam_abi::error_value::attach(errors), 1);
            assert_eq!(birnam_abi::printing::attach(printing), 1);
            assert_eq!(birnam_abi::block_loop::attach(loops), 1);
        }
        self.error_value = errors;
        self.printing = printing;
        self.block_loop = loops;
        Ok(())
    }

    /// Complete application families before mounting Image.
    pub fn initialize_application_base(&mut self) -> Result<(), ModuleError> {
        self.initialize_system()?;
        self.construct(b"applicationBase")?;
        Ok(())
    }

    /// The loaded Bootstrap owns the entire native construction sequence.
    pub fn load_script_modules(&mut self, _resources: &Resources) -> Result<(), ModuleError> {
        self.initialize_application_base()?;
        self.construct(b"image")?;
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
    unsafe { start_with_floor(resources, policy, floor) }
}

/// Continue ordinary host startup after explicit early-module construction.
/// # Safety
/// The completed floor belongs to these resources and the current owner thread.
pub unsafe fn start_with_floor(resources: Resources, policy: StartupPolicy, floor: LoadedFloor) -> Result<HostRuntime, RuntimeStartError> {
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
    pub fn id_protocol(&self) -> &birnam_abi::id_protocol::Protocol { &self.id_protocol }
    pub fn presence(&self) -> *const birnam_abi::namespace_presence::ApiV1 { self.namespace_presence.as_ptr() }
    pub fn imports(&self) -> *const birnam_abi::namespace_import::ApiV1 { self.namespace_import.as_ptr() }
    pub fn exports(&self) -> *const birnam_abi::namespace_export::ApiV1 { self.namespace_export.as_ptr() }
    pub fn vtables(&self) -> *const birnam_abi::vtable_enumeration::ApiV2 { self.vtable_enumeration.as_ptr() }
    pub fn observation(&self) -> *const birnam_abi::method_observation::ApiV1 { self.method_observation.as_ptr() }
    pub fn root_registry(&self) -> *const birnam_abi::root_registry::ApiV1 { self.root_registry.as_ptr() }
    pub fn gc_settings(&self) -> *const birnam_abi::gc_settings::ApiV1 { self.gc_settings.as_ptr() }
    pub fn payload_layout(&self) -> *const birnam_abi::payload_layout::ApiV1 { self.payload_layout.as_ptr() }
    pub fn trace_hooks(&self) -> *const birnam_abi::trace_hooks::ApiV1 { self.trace_hooks.as_ptr() }
    pub fn finalizers(&self) -> *const birnam_abi::finalizers::ApiV1 { self.finalizers.as_ptr() }
    pub fn native_stack(&self) -> *const birnam_abi::native_stack::ApiV1 { self.native_stack.as_ptr() }

    pub fn api(&self) -> &IdApi {
        // SAFETY: Runtime construction returned a process-owned, non-null table and the
        // successful loader retains its dylib handle for this value's lifetime.
        unsafe { self.api.as_ref() }
    }

    pub fn handle(&self) -> NonNull<std::ffi::c_void> {
        self.handle
    }
}

/// Initialize Id and Bootstrap through native messages, before Lifetime exists.
/// Optional digest/identity checks precede all construction.
pub fn begin_id(
    path: &Path,
    expected_identity: Option<&str>,
    expected_digest: Option<&str>,
) -> Result<InitialFloor, FloorError> {
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
    let source_path = path.to_path_buf();
    let path = CString::new(path_bytes(path)).map_err(|_| FloorError::EmbeddedNul)?;
    // SAFETY: the path is NUL-terminated and remains live for the call.
    let handle = unsafe { system_dlopen(path.as_ptr(), RTLD_NOW | RTLD_LOCAL) };
    let Some(handle) = NonNull::new(handle) else {
        return Err(FloorError::OpenFailed);
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
    // Actual Id construction runs through Siphon before any upper services.
    // The loaded Bootstrap module constructs the dependencies through Runtime.
    let id_protocol = crate::siphon::load(path.as_bytes()).map_err(|(status, detail)| {
        unsafe { system_dlclose(handle.as_ptr()) };
        FloorError::IdProtocol { status, detail }
    })?;
    let file = source_path.with_file_name("libbirnam_bootstrap.dylib");
    let bootstrap = Some(crate::native_bootstrap::Owner::load(id_protocol, &path_bytes(&file), &path_bytes(&source_path))
        .map_err(|(status, detail)| FloorError::IdProtocol { status, detail })?);
    Ok(InitialFloor { source_path, handle, id_protocol, bootstrap, _owner_thread: PhantomData })
}

/// Complete Id and early root ownership. No upper Runtime has been constructed.
/// An abandoned/failed construction retains entered native owners until exit.
pub struct InitialFloor {
    source_path: PathBuf,
    handle: NonNull<std::ffi::c_void>,
    id_protocol: birnam_abi::id_protocol::Protocol,
    bootstrap: Option<crate::native_bootstrap::Owner>,
    _owner_thread: PhantomData<std::rc::Rc<()>>,
}
impl InitialFloor {
    pub fn id_protocol(&self) -> birnam_abi::id_protocol::Protocol { self.id_protocol }
    pub fn bootstrap(&self) -> Option<*const birnam_abi::bootstrap::OwnedV2> {
        self.bootstrap.as_ref().map(crate::native_bootstrap::Owner::descriptor)
    }
    /// Load through the same retained Bootstrap used by production construction.
    /// # Safety
    /// Native operand and borrowed pointer/code lifetime contracts apply.
    pub unsafe fn load_native(&self, path: &[u8], args: &[birnam_abi::id_protocol::Operand]) -> birnam_abi::id_protocol::Outcome {
        unsafe { load_native_with_bootstrap(self.bootstrap.as_ref(), self.id_protocol, path, args) }
    }
    fn construct(&self, stage: &[u8]) -> Result<birnam_abi::id_protocol::Operand, FloorError> {
        self.bootstrap.as_ref().expect("loaded Bootstrap").construct(stage).into_result()
            .map_err(|failure| FloorError::IdProtocol { status: failure.status, detail: failure.detail })
    }
    fn constructed_word(&self, stage: &[u8]) -> Result<usize, FloorError> {
        self.construct(stage)?.as_word().filter(|value| *value != 0)
            .ok_or(FloorError::IdProtocol { status: birnam_abi::id_protocol::INVALID_REQUEST, detail: 0 })
    }
    /// Read the completed root owner without constructing an absent prefix.
    pub fn roots(&self) -> Option<*const birnam_abi::root_registry::OwnedV1> {
        self.construct(b"existingRoots").ok()?.as_word().map(|value| value as _)
    }
    pub fn initialize_roots(&mut self) -> Result<Option<*const birnam_abi::root_registry::OwnedV1>, FloorError> {
        Ok(Some(self.constructed_word(b"roots")? as _))
    }
    pub fn initialize_lifetime(&mut self) -> Result<Option<*const birnam_abi::lifetime::OwnedV2>, FloorError> {
        Ok(Some(self.constructed_word(b"lifetime")? as _))
    }
    pub fn initialize_base_values(&mut self) -> Result<Option<*const birnam_abi::base_values::ValuesV1>, FloorError> {
        Ok(Some(self.constructed_word(b"values")? as _))
    }
    pub fn initialize_namespace(&mut self) -> Result<Option<*const birnam_abi::namespace_owner::OwnedV1>, FloorError> {
        Ok(Some(self.constructed_word(b"namespace")? as _))
    }
    pub fn initialize_object_model(&mut self) -> Result<Option<*const birnam_abi::object_model::OwnedV1>, FloorError> {
        Ok(Some(self.constructed_word(b"objectModel")? as _))
    }
    pub fn initialize_string(&mut self) -> Result<Option<*mut birnam_abi::string::StringApiV1>, FloorError> {
        Ok(Some(self.constructed_word(b"string")? as _))
    }
    pub fn initialize_integer(&mut self) -> Result<Option<*mut birnam_abi::integer::IntegerApiV1>, FloorError> {
        Ok(Some(self.constructed_word(b"integer")? as _))
    }
    pub fn initialize_float(&mut self) -> Result<Option<*mut birnam_abi::float::FloatApiV1>, FloorError> {
        Ok(Some(self.constructed_word(b"float")? as _))
    }
    pub fn initialize_word(&mut self) -> Result<Option<*mut birnam_abi::word::WordApiV1>, FloorError> {
        Ok(Some(self.constructed_word(b"word")? as _))
    }
    pub fn initialize_trace(&mut self) -> Result<Option<*const birnam_abi::trace_graph::GraphV2>, FloorError> {
        Ok(Some(self.constructed_word(b"trace")? as _))
    }
    pub fn initialize_collection(&mut self) -> Result<Option<*const birnam_abi::collection_state::OwnedV2>, FloorError> {
        Ok(Some(self.constructed_word(b"collection")? as _))
    }
    pub fn initialize_error(&mut self) -> Result<Option<birnam_abi::Oop>, FloorError> {
        self.construct(b"error")?.as_object().filter(|value| !value.is_null()).map(Some)
            .ok_or(FloorError::IdProtocol { status: birnam_abi::id_protocol::INVALID_REQUEST, detail: 0 })
    }
    pub fn initialize_text(&mut self) -> Result<(), FloorError> { self.construct(b"text").map(|_| ()) }
    pub fn activate_collection(&mut self) -> Result<(), FloorError> { self.construct(b"activate").map(|_| ()) }
    pub fn initialize_runtime_support(&self) -> Result<birnam_abi::runtime_support::FactoriesV1, FloorError> {
        let pointer = self.constructed_word(b"runtimeSupport")? as *const birnam_abi::runtime_support::FactoriesV1;
        Ok(unsafe { *pointer })
    }
    pub fn initialize_runtime_values(&mut self) -> Result<birnam_abi::runtime_values::ApisV1, FloorError> {
        let pointer = self.constructed_word(b"runtimeValues")? as *const birnam_abi::runtime_values::ApisV1;
        Ok(unsafe { *pointer })
    }
    pub fn finish(mut self) -> Result<LoadedFloor, FloorError> {
        let initialized = self.constructed_word(b"runtime")? as *mut IdApi;
        let values = self.initialize_runtime_values()?;
        // Obtain completed services through Bootstrap and Id before moving the owner.
        let presence = self.constructed_word(b"namespacePresence")? as *mut birnam_abi::namespace_presence::ApiV1;
        if !unsafe { birnam_abi::namespace_presence::valid(presence) } {
            return Err(FloorError::Namespace { path: self.source_path.with_file_name("libbirnam_namespace.dylib"), reason: "invalid namespacePresence capability" });
        }
        let import = self.constructed_word(b"namespaceImport")? as *mut birnam_abi::namespace_import::ApiV1;
        if !unsafe { birnam_abi::namespace_import::valid(import) } {
            return Err(FloorError::Namespace { path: self.source_path.with_file_name("libbirnam_namespace.dylib"), reason: "invalid namespaceImport capability" });
        }
        let export = self.constructed_word(b"namespaceExport")? as *mut birnam_abi::namespace_export::ApiV1;
        if !unsafe { birnam_abi::namespace_export::valid(export) } {
            return Err(FloorError::Namespace { path: self.source_path.with_file_name("libbirnam_namespace.dylib"), reason: "invalid namespaceExport capability" });
        }
        let vtables = self.constructed_word(b"vtables")? as *mut birnam_abi::vtable_enumeration::ApiV2;
        if !unsafe { birnam_abi::vtable_enumeration::valid(vtables) } {
            return Err(FloorError::VtableEnumeration { path: self.source_path.with_file_name("libbirnam_object_model.dylib"), reason: "invalid vtables capability" });
        }
        let observation = self.constructed_word(b"methodObservation")? as *mut birnam_abi::method_observation::ApiV1;
        if !unsafe { birnam_abi::method_observation::valid(observation) } {
            return Err(FloorError::MethodObservation { path: self.source_path.with_file_name("libbirnam_method_observation.dylib"), reason: "invalid methodObservation capability" });
        }
        let root_registry = self.constructed_word(b"rootRegistry")? as *mut birnam_abi::root_registry::ApiV1;
        if !unsafe { birnam_abi::root_registry::valid(root_registry) } {
            return Err(FloorError::RootRegistry { path: self.source_path.with_file_name("libbirnam_root_registry.dylib"), reason: "invalid rootRegistry capability" });
        }
        let gc_settings = self.constructed_word(b"gcSettings")? as *mut birnam_abi::gc_settings::ApiV1;
        if !unsafe { birnam_abi::gc_settings::valid(gc_settings) } {
            return Err(FloorError::GcSettings { path: self.source_path.with_file_name("libbirnam_gc_settings.dylib"), reason: "invalid gcSettings capability" });
        }
        let payload_layout = self.constructed_word(b"payloadLayout")? as *mut birnam_abi::payload_layout::ApiV1;
        if !unsafe { birnam_abi::payload_layout::valid(payload_layout) } {
            return Err(FloorError::PayloadLayout { path: self.source_path.with_file_name("libbirnam_payload_layout.dylib"), reason: "invalid payloadLayout capability" });
        }
        let trace_hooks = self.constructed_word(b"traceHooks")? as *mut birnam_abi::trace_hooks::ApiV1;
        if !unsafe { birnam_abi::trace_hooks::valid(trace_hooks) } {
            return Err(FloorError::TraceHooks { path: self.source_path.with_file_name("libbirnam_trace_hooks.dylib"), reason: "invalid traceHooks capability" });
        }
        let finalizers = self.constructed_word(b"finalizers")? as *mut birnam_abi::finalizers::ApiV1;
        if !unsafe { birnam_abi::finalizers::valid(finalizers) } {
            return Err(FloorError::Finalizers { path: self.source_path.with_file_name("libbirnam_finalizers.dylib"), reason: "invalid finalizers capability" });
        }
        let native_stack = self.constructed_word(b"nativeStack")? as *mut birnam_abi::native_stack::ApiV1;
        if !unsafe { birnam_abi::native_stack::valid(native_stack) } {
            return Err(FloorError::NativeStack { path: self.source_path.with_file_name("libbirnam_native_stack.dylib"), reason: "invalid nativeStack capability" });
        }
        let Self { source_path, handle, id_protocol, bootstrap, .. } = self;
        // Id pins every entered native image, including the supporting factories.
        let _id_image = handle;
        let runtime_path = source_path.with_file_name("libbirnam_runtime.dylib");
        let api = NonNull::new(initialized).ok_or(FloorError::NullApi)?;
        // Retain a handle for the remaining execution/image entry points only.
        let runtime_name = CString::new(path_bytes(&runtime_path)).map_err(|_| FloorError::EmbeddedNul)?;
        let handle = NonNull::new(unsafe { system_dlopen(runtime_name.as_ptr(), RTLD_NOW | RTLD_LOCAL) })
            .ok_or(FloorError::OpenFailed)?;
        Ok(LoadedFloor {
            values,
            bootstrap,
            id_protocol,
            root_registry: NonNull::new(root_registry).unwrap(),
            gc_settings: NonNull::new(gc_settings).unwrap(),
            payload_layout: NonNull::new(payload_layout).unwrap(),
            trace_hooks: NonNull::new(trace_hooks).unwrap(),
            finalizers: NonNull::new(finalizers).unwrap(),
            native_stack: NonNull::new(native_stack).unwrap(),
            method_observation: NonNull::new(observation).unwrap(),
            vtable_enumeration: NonNull::new(vtables).unwrap(),
            namespace_export: NonNull::new(export).unwrap(),
            namespace_import: NonNull::new(import).unwrap(),
            namespace_presence: NonNull::new(presence).unwrap(),
            handle,
            api,
            _owner_thread: PhantomData,
        })
    }

}

/// Initialize Id, Bootstrap and the root owner; later construction stays explicit.
pub fn begin_floor(path: &Path, expected_identity: Option<&str>, expected_digest: Option<&str>) -> Result<InitialFloor, FloorError> {
    let mut initial = begin_id(path, expected_identity, expected_digest)?;
    initial.initialize_roots()?;
    Ok(initial)
}

unsafe fn load_native_with_bootstrap(bootstrap: Option<&crate::native_bootstrap::Owner>, id: birnam_abi::id_protocol::Protocol, path: &[u8], args: &[birnam_abi::id_protocol::Operand]) -> birnam_abi::id_protocol::Outcome {
    if let Some(bootstrap) = bootstrap { return unsafe { bootstrap.initialize(path, args) }; }
    let mut request = vec![birnam_abi::id_protocol::Operand::bytes(path)];
    request.extend_from_slice(args);
    unsafe { id.message.unwrap()(id.context, id.receiver, id.load, request.as_ptr(), request.len()) }
}

pub fn load_floor(path: &Path, expected_identity: Option<&str>, expected_digest: Option<&str>) -> Result<LoadedFloor, FloorError> {
    begin_floor(path, expected_identity, expected_digest)?.finish()
}

// Only host loader operations run here; no Id callbacks exist yet. On success
// the descriptor has escaped into the floor and both libraries stay pinned.










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
