//! Project data validation, dependency order and publication source construction.
use crate::HostError;
use birnam_artifact::digest::sha256_hex;
pub use birnam_host_support::project::*;
use std::collections::{HashMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::io;
#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectPolicyError {
    Source(HostError),
    MissingPolicy,
    InvalidSnapshot,
    ResolutionRejected,
    ResolutionDiagnostic { kind: String, message: String },
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ProjectModulePlan {
    pub entry: String,
    pub files: Vec<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
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

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ProjectManifestDependencyObservation {
    pub name: String,
    pub source: ProjectDependencySourceObservation,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum ProjectDependencySourceObservation {
    Path(String),
    Git { url: String, revision: String },
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ProjectWrappedObservation {
    pub name: String,
    pub build: Vec<Vec<String>>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ProjectDependencyObservation {
    pub name: String,
    pub root: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDependencyClosureNode {
    pub root: PathBuf,
    pub dependencies: Vec<ProjectDependencyObservation>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
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

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ProjectSnapshot {
    pub root: String,
    pub projects: Vec<ProjectObservation>,
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
pub struct ProjectArtifactStageRequest<'a> {
    pub resolution: &'a ProjectResolutionObservation,
    pub plan: &'a ProjectArtifactPlan,
    pub previous: Option<&'a ProjectPreviousPublication>,
    pub previous_root: Option<&'a Path>,
    pub stage_root: &'a Path,
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

pub fn valid_git_remote(url: &str) -> bool {
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
impl From<HostError> for ProjectManifestDecodeError {
    fn from(e: HostError) -> Self {
        Self::Host(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

pub fn safe_artifact_relative_path(path: &str) -> Option<&Path> {
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
