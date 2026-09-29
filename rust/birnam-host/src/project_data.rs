//! Project data validation, dependency order and publication source construction.
use crate::{HostError, ProjectSourceObservation};
use birnam_artifact::digest::sha256_hex;
use std::collections::{HashMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::io;
#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;
use std::path::{Component, Path, PathBuf};

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
pub enum ProjectPolicyError {
    Source(HostError),
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
    pub dependencies: Vec<ProjectManifestDependencyObservation>,
    pub run: ProjectRunObservation,
    pub wrapped: Vec<ProjectWrappedObservation>,
    pub test_sources: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectManifestDependencyObservation {
    pub name: String,
    pub source: ProjectDependencySourceObservation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectDependencySourceObservation {
    Path(String),
    Git { url: String, revision: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectWrappedObservation {
    pub name: String,
    pub build: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectWrappedArtifactObservation {
    pub project_root: String,
    pub name: String,
    pub build: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDependencyObservation {
    pub name: String,
    pub root: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectFetchResult {
    pub project_root: PathBuf,
    pub fetched: usize,
    pub reused: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDependencyClosureNode {
    pub root: PathBuf,
    pub dependencies: Vec<ProjectDependencyObservation>,
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
    pub run: ProjectRunObservation,
    pub load_roots: Vec<String>,
    pub modules: Vec<ProjectSourceObservation>,
    pub wrapped: Vec<ProjectWrappedArtifactObservation>,
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
    pub wrapped: Vec<ProjectWrappedArtifactObservation>,
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
    pub artifacts: Vec<ProjectArtifactPlanEntry>,
    pub publication: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectArtifactPlanValidationError {
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
    pub previous: Option<&'a ProjectPreviousPublication>,
    pub previous_root: Option<&'a Path>,
    pub stage_root: &'a Path,
}

#[derive(Debug, Clone, Copy)]
pub struct ProjectBuildRequest<'a> {
    pub start: &'a Path,
    pub target: &'a str,
    pub profile: &'a str,
    pub tool_version: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectBuildResult {
    pub output_root: PathBuf,
    pub module_root: PathBuf,
    pub configuration: PathBuf,
    pub application: Option<PathBuf>,
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
pub enum ProjectBuildError {
    Observation(ProjectObservationBuildError),
    Snapshot(ProjectSnapshotError),
    Policy(ProjectPolicyError),
    Resolution(ProjectManifestDecodeError),
    ManifestRead {
        path: PathBuf,
        kind: io::ErrorKind,
    },
    GeneratedSource(ProjectSourceIndexError),
    MainRead {
        path: PathBuf,
        kind: io::ErrorKind,
    },
    Plan(ProjectArtifactPlanError),
    StageDirectory {
        path: PathBuf,
        kind: io::ErrorKind,
    },
    Wrapped(ProjectWrappedBuildError),
    Stage(ProjectArtifactStageError),
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
    Publication(HostError),
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
    Preflight(HostError),
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
    ApplicationUnavailable(PathBuf),
    PublishedConfiguration,
    InvalidReplMode(String),
    Module(HostError),
    TestHarnessUnavailable,
    TestSuiteMissing,
    TestFailed,
    NativeDirectoryRejected,
    ArtifactPreflight(HostError),
    ArtifactActivation(HostError),
    ArgumentsRejected,
    Main(HostError),
    Repl(HostError),
    Socket(HostError),
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
    pub previous: Option<&'a ProjectPreviousPublication>,
    pub target: &'a str,
    pub profile: &'a str,
    pub tool_version: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPreviousPublishedArtifact {
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
pub struct ProjectPreviousPublication {
    pub project_root: String,
    pub target: String,
    pub profile: String,
    pub tool_version: String,
    pub artifact_abi: String,
    pub artifacts: Vec<ProjectPreviousPublishedArtifact>,
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
pub enum ProjectFetchError {
    Discovery(ProjectDiscoveryError),
    Manifest(ProjectManifestError),
    Decode(ProjectManifestDecodeError),
    Dependency(ProjectDependencyResolutionError),
    Canonicalize { path: PathBuf, kind: io::ErrorKind },
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
    Host(HostError),
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
    InvalidDependency(String),
    InvalidGitRevision { name: String, revision: String },
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
    GitCacheMissing {
        name: String,
        path: PathBuf,
    },
    GitCacheInvalid {
        name: String,
        path: PathBuf,
        reason: String,
    },
    CacheIo {
        path: PathBuf,
        operation: &'static str,
        kind: io::ErrorKind,
    },
    GitSpawn {
        name: String,
        operation: &'static str,
        kind: io::ErrorKind,
    },
    GitCommand {
        name: String,
        operation: &'static str,
        code: Option<i32>,
        detail: String,
    },
}

/// Convert a file URI (or a plain path used by tests/tools) into a filesystem
/// path. This mirrors the narrow project bridge boundary: only
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

/// Accept only immutable, full Git object identifiers. SHA-1 repositories use
/// 40 hexadecimal digits and object-format=sha256 repositories use 64.
pub fn normalize_git_revision(revision: &str) -> Option<String> {
    if !matches!(revision.len(), 40 | 64) || !revision.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return None;
    }
    Some(revision.to_ascii_lowercase())
}

pub(super) fn valid_git_remote(url: &str) -> bool {
    !url.is_empty()
        && !url.starts_with('-')
        && !url.starts_with("ext::")
        && !url.bytes().any(|byte| matches!(byte, 0 | b'\n' | b'\r'))
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

/// Extend the source-resolution identity with the complete wrapped-artifact
/// closure. This invalidates a published build when a dependency changes a
/// native recipe even if its selected Birnam module source is unchanged.
pub fn project_build_resolution_digest(
    project_root: &str,
    manifest_bytes: &[u8],
    modules: &[ProjectSourceObservation],
    wrapped: &[ProjectWrappedArtifactObservation],
) -> String {
    let source_digest = project_resolution_digest(project_root, manifest_bytes, modules);
    let mut input = Vec::new();
    let add = |input: &mut Vec<u8>, value: &str| {
        input.extend_from_slice(&(value.len() as u64).to_be_bytes());
        input.extend_from_slice(value.as_bytes());
    };
    let add_count = |input: &mut Vec<u8>, value: usize| {
        input.extend_from_slice(&(value as u64).to_be_bytes());
    };
    add(&mut input, "birnam.project-build-resolution.v2");
    add(&mut input, &source_digest);
    add_count(&mut input, wrapped.len());
    for artifact in wrapped {
        add(&mut input, &artifact.project_root);
        add(&mut input, &artifact.name);
        add_count(&mut input, artifact.build.len());
        for command in &artifact.build {
            add_count(&mut input, command.len());
            for argument in command {
                add(&mut input, argument);
            }
        }
    }
    String::from_utf8_lossy(&sha256_hex(&input)).into_owned()
}

pub(super) const PROJECT_MODULE_ROOT_FILENAME: &str = "project-modules.dylib";

pub(super) const PROJECT_CONFIGURATION_FILENAME: &str = "project-configuration.dylib";

pub(super) const PROJECT_APPLICATION_FILENAME: &str = "application.dylib";

pub(super) const PROJECT_REPL_GLOBAL: &str = "ProjectPublicationRepl";

pub(super) const PROJECT_SOCKET_BIND_GLOBAL: &str = "ProjectPublicationSocketBind";

pub(super) const PROJECT_SOCKET_PORT_GLOBAL: &str = "ProjectPublicationSocketPort";

fn project_publication_identity(project_name: &str, suffix: &str) -> String {
    let mut identity = String::from("__");
    for character in project_name.chars() {
        if character.is_ascii_alphanumeric() || character == '_' {
            identity.push(character);
        } else {
            identity.push('_');
        }
    }
    identity.push_str(suffix);
    identity
}

fn append_birnam_string(output: &mut String, value: &str) {
    output.push('"');
    for character in value.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            value => output.push(value),
        }
    }
    output.push('"');
}

fn source_without_module_declaration(source: &[u8]) -> Vec<u8> {
    let declaration = b"[Module named:";
    let mut cursor = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    let mut line_comment = false;
    let mut start = None;
    while cursor < source.len() {
        let byte = source[cursor];
        if line_comment {
            if byte == b'\n' {
                line_comment = false;
            }
            cursor += 1;
            continue;
        }
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
            cursor += 1;
            continue;
        }
        if byte == b'/' && source.get(cursor + 1) == Some(&b'/') {
            line_comment = true;
            cursor += 2;
            continue;
        }
        if byte == b'"' {
            quoted = true;
            cursor += 1;
            continue;
        }
        if source[cursor..].starts_with(declaration) {
            start = Some(cursor);
            break;
        }
        cursor += 1;
    }
    let Some(start) = start else {
        return source.to_vec();
    };
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    let mut line_comment = false;
    let mut finish = None;
    for (offset, byte) in source[start..].iter().copied().enumerate() {
        if line_comment {
            if byte == b'\n' {
                line_comment = false;
            }
            continue;
        }
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
            continue;
        }
        if byte == b'/' && source.get(start + offset + 1) == Some(&b'/') {
            line_comment = true;
            continue;
        }
        match byte {
            b'"' => quoted = true,
            b'[' => depth += 1,
            b']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let mut end = start + offset + 1;
                    while source
                        .get(end)
                        .is_some_and(|byte| byte.is_ascii_whitespace())
                    {
                        end += 1;
                    }
                    if source.get(end) == Some(&b';') {
                        end += 1;
                    }
                    finish = Some(end);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(finish) = finish else {
        return source.to_vec();
    };
    let mut result = Vec::with_capacity(source.len() - (finish - start));
    result.extend_from_slice(&source[..start]);
    result.extend_from_slice(&source[finish..]);
    result
}

pub(super) fn project_module_root_source(
    resolution: &ProjectResolutionObservation,
    resolution_digest: &str,
    target: &str,
    profile: &str,
    tool_version: &str,
) -> (String, Vec<u8>) {
    let identity = project_publication_identity(&resolution.project_name, "ProjectModules");
    let mut source = format!(
        "// project-publication {resolution_digest} {target} {profile} {tool_version}\n\
         [Module named: #{identity} requires: #["
    );
    let mut seen = HashSet::new();
    for module in &resolution.modules {
        if seen.insert(module.name.as_str()) {
            source.push('#');
            source.push_str(&module.name);
            source.push(' ');
        }
    }
    source.push_str("]];\n");
    for wrapped in &resolution.wrapped {
        source.push_str("[Alien dlopen: ");
        append_birnam_string(&mut source, &format!("lib{}.dylib", wrapped.name));
        source.push_str("];\n");
    }
    (identity, source.into_bytes())
}

pub(super) fn project_configuration_source(
    resolution: &ProjectResolutionObservation,
    resolution_digest: &str,
    target: &str,
    profile: &str,
    tool_version: &str,
) -> (String, Vec<u8>) {
    let identity = project_publication_identity(&resolution.project_name, "ProjectConfiguration");
    let mut source = format!(
        "// project-configuration {resolution_digest} {target} {profile} {tool_version}\n\
         [Module named: #{identity} requires: #[]];\n\
         [Globals at: #{PROJECT_REPL_GLOBAL} put: "
    );
    append_birnam_string(&mut source, &resolution.run.repl);
    source.push_str("];\n[Globals at: #");
    source.push_str(PROJECT_SOCKET_BIND_GLOBAL);
    source.push_str(" put: ");
    append_birnam_string(&mut source, &resolution.run.socket_bind);
    source.push_str("];\n[Globals at: #");
    source.push_str(PROJECT_SOCKET_PORT_GLOBAL);
    source.push_str(" put: ");
    source.push_str(&resolution.run.socket_port.to_string());
    source.push_str("];\n");
    (identity, source.into_bytes())
}

pub(super) fn project_application_source(
    resolution: &ProjectResolutionObservation,
    configuration_identity: &str,
    resolution_digest: &str,
    target: &str,
    profile: &str,
    tool_version: &str,
    main: &[u8],
) -> (String, Vec<u8>) {
    let identity = project_publication_identity(&resolution.project_name, "Application");
    let mut source = format!(
        "// project-application {resolution_digest} {target} {profile} {tool_version}\n\
         [Module named: #{identity} requires: #[#{configuration_identity}]];\n"
    )
    .into_bytes();
    let mut seen = HashSet::new();
    for wrapped in &resolution.wrapped {
        let mut declaration = String::from("[Alien dlopen: ");
        append_birnam_string(&mut declaration, &format!("lib{}.dylib", wrapped.name));
        declaration.push_str("];\n");
        source.extend_from_slice(declaration.as_bytes());
    }
    for module in &resolution.modules {
        if seen.insert(module.name.as_str()) {
            // Preserve each bundled module's declaration semantics while
            // keeping Codegen's one-declaration metadata scanner focused on
            // the application root.
            source.extend_from_slice(b"[[Globals at: #Module] named: #");
            source.extend_from_slice(module.name.as_bytes());
            source.extend_from_slice(b" requires: #[");
            for requirement in &module.requires {
                source.push(b'#');
                source.extend_from_slice(requirement.as_bytes());
                source.push(b' ');
            }
            source.extend_from_slice(b"]];\n");
            source.extend_from_slice(&source_without_module_declaration(&module.source));
            if !source.ends_with(b"\n") {
                source.push(b'\n');
            }
            source.extend_from_slice(b"[[Globals at: #Module] markLoaded: #");
            source.extend_from_slice(module.name.as_bytes());
            source.extend_from_slice(b"];\n");
        }
    }
    source.extend_from_slice(main);
    if !source.ends_with(b"\n") {
        source.push(b'\n');
    }
    (identity, source)
}

/// Validate a decoded artifact plan against the host-observed resolution and
/// the request identity. This is a fail-closed adapter check: Birnam policy
/// chooses compile versus reuse, while Rust rejects plans whose metadata or
/// paths could make a later effectful stage address a different artifact.
pub fn validate_project_artifact_plan(
    plan: &ProjectArtifactPlan,
    request: &ProjectArtifactPlanRequest<'_>,
) -> Result<(), ProjectArtifactPlanValidationError> {
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

pub(super) fn safe_artifact_relative_path(path: &str) -> Option<&Path> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_source_removes_only_the_module_declaration() {
        let source = b"// prefix mentioning [Module named: #fake with an unmatched [\n[Module named: #demo requires: #[#one #two]];\n[Reflect defineMethod: #value on: Demo as: [:self | \"[not syntax]\"]]\n";
        assert_eq!(
            source_without_module_declaration(source),
            b"// prefix mentioning [Module named: #fake with an unmatched [\n\n[Reflect defineMethod: #value on: Demo as: [:self | \"[not syntax]\"]]\n"
        );
    }

    #[test]
    fn application_publication_bundles_modules_dependency_first_and_marks_them_loaded() {
        let module = |name: &str, requires: Vec<String>, source: &[u8]| ProjectSourceObservation {
            name: name.to_owned(),
            path: format!("/tmp/project/src/{name}.birnam"),
            sources: vec![format!("/tmp/project/src/{name}.birnam")],
            project_root: "/tmp/project".to_owned(),
            source: source.to_vec(),
            requires,
            source_digest: format!("{name}-source"),
            revision_digest: format!("{name}-revision"),
        };
        let resolution = ProjectResolutionObservation {
            project_root: "/tmp/project".to_owned(),
            manifest: "/tmp/project/birnam.toml".to_owned(),
            project_name: "demo".to_owned(),
            main: Some("main.birnam".to_owned()),
            run: ProjectRunObservation {
                repl: "none".to_owned(),
                socket_bind: "127.0.0.1".to_owned(),
                socket_port: 0,
            },
            load_roots: vec!["app".to_owned()],
            modules: vec![
                module(
                    "dep",
                    Vec::new(),
                    b"[Module named: #dep requires: #[]];\nDep := Object clone\n",
                ),
                module(
                    "app",
                    vec!["dep".to_owned()],
                    b"[Module named: #app requires: #[#dep]];\nApp := Object clone\n",
                ),
            ],
            wrapped: Vec::new(),
        };
        let (_, source) = project_application_source(
            &resolution,
            "__demoProjectConfiguration",
            "digest",
            "target",
            "debug",
            "dev",
            b"[App value]\n",
        );
        let source = String::from_utf8(source).expect("generated source is UTF-8");
        let dep = source.find("Dep :=").expect("dependency body is bundled");
        let app = source.find("App :=").expect("application body is bundled");
        let main = source.find("[App value]").expect("main is bundled");
        assert!(dep < app && app < main);
        assert!(source.contains("[[Globals at: #Module] named: #dep requires: #[]]"));
        assert!(source.contains("[[Globals at: #Module] markLoaded: #dep]"));
        assert!(source.contains("[[Globals at: #Module] named: #app requires: #[#dep ]]"));
        assert!(source.contains("[[Globals at: #Module] markLoaded: #app]"));
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
            run: ProjectRunObservation {
                repl: "none".to_owned(),
                socket_bind: "127.0.0.1".to_owned(),
                socket_port: 4400,
            },
            load_roots: vec!["/tmp/project/src".to_owned()],
            modules: vec![module.clone()],
            wrapped: Vec::new(),
        };
        let request = ProjectArtifactPlanRequest {
            resolution: &resolution,
            previous: None,
            target: "aarch64-macos",
            profile: "debug",
            tool_version: "dev",
        };
        let mut plan = ProjectArtifactPlan {
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
        plan.artifacts[0].path = "/outside/libbirnam_app.dylib".to_owned();
        assert!(matches!(
            validate_project_artifact_plan(&plan, &request),
            Err(ProjectArtifactPlanValidationError::ArtifactMismatch {
                index: 0,
                field: "path"
            })
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
    fn project_build_resolution_digest_tracks_transitive_wrapped_provenance() {
        let wrapped = |root: &str, argument: &str| ProjectWrappedArtifactObservation {
            project_root: root.to_owned(),
            name: "native".to_owned(),
            build: vec![vec![
                "cc".to_owned(),
                argument.to_owned(),
                "-o".to_owned(),
                "{output}".to_owned(),
            ]],
        };
        let first = project_build_resolution_digest(
            "/tmp/project",
            b"name = \"project\"\n",
            &[],
            &[wrapped("/tmp/dependency", "native/one.c")],
        );
        assert_eq!(
            first,
            project_build_resolution_digest(
                "/tmp/project",
                b"name = \"project\"\n",
                &[],
                &[wrapped("/tmp/dependency", "native/one.c")],
            )
        );
        assert_ne!(
            first,
            project_build_resolution_digest(
                "/tmp/project",
                b"name = \"project\"\n",
                &[],
                &[wrapped("/tmp/dependency", "native/two.c")],
            )
        );
        assert_ne!(
            first,
            project_build_resolution_digest(
                "/tmp/project",
                b"name = \"project\"\n",
                &[],
                &[wrapped("/tmp/other-dependency", "native/one.c")],
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
    fn git_dependency_revisions_are_full_hex_object_ids() {
        assert_eq!(
            normalize_git_revision("ABCDEF0123456789ABCDEF0123456789ABCDEF01"),
            Some("abcdef0123456789abcdef0123456789abcdef01".to_owned())
        );
        assert!(normalize_git_revision("main").is_none());
        assert!(normalize_git_revision("abcdef0").is_none());
        assert!(normalize_git_revision("zbcdef0123456789abcdef0123456789abcdef01").is_none());
        assert!(valid_git_remote("https://github.com/chazu/example.git"));
        assert!(valid_git_remote("git@github.com:chazu/example.git"));
        assert!(valid_git_remote("/tmp/example.git"));
        assert!(!valid_git_remote("ext::sh -c exploit"));
        assert!(!valid_git_remote("--upload-pack=exploit"));
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
}

impl From<HostError> for ProjectManifestDecodeError {
    fn from(e: HostError) -> Self {
        Self::Host(e)
    }
}
