use std::path::{Path, PathBuf};
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplMode {
    None,
    Tty,
    Socket,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RunOptions {
    #[serde(with = "crate::native::path_bytes")]
    pub directory: PathBuf,
    pub build: bool,
    pub profile: String,
    pub repl: Option<ReplMode>,
    pub socket_bind: Option<String>,
    pub socket_port: Option<u16>,
    #[serde(with = "crate::native::argv_bytes")]
    pub arguments: Vec<std::ffi::OsString>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ProjectRunObservation {
    pub repl: String,
    pub socket_bind: String,
    pub socket_port: i64,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ProjectWrappedArtifactObservation {
    pub project_root: String,
    pub name: String,
    pub build: Vec<Vec<String>>,
    #[serde(default = "adjacent_packaging")]
    pub packaging: String,
}

pub fn adjacent_packaging() -> String { "adjacent".into() }

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ProjectFetchResult {
    #[serde(with = "crate::native::path_bytes")]
    pub project_root: PathBuf,
    pub fetched: usize,
    pub reused: usize,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
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

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ProjectCompileDiagnostic {
    pub module: String,
    pub path: String,
    pub detail: String,
    pub message: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ProjectCheckResult {
    pub resolution: ProjectResolutionObservation,
    pub resolution_digest: String,
    pub wrapped: Vec<ProjectWrappedArtifactObservation>,
    pub test_sources: Option<Vec<String>>,
    pub diagnostics: Vec<ProjectCompileDiagnostic>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ProjectStagedArtifact {
    pub name: String,
    pub path: String,
    pub action: String,
    pub checksum: String,
}

#[derive(Debug, Clone, Copy)]
pub struct ProjectBuildRequest<'a> {
    pub start: &'a Path,
    pub target: &'a str,
    pub profile: &'a str,
    pub tool_version: &'a str,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ProjectBuildResult {
    #[serde(with = "crate::native::path_bytes")]
    pub output_root: PathBuf,
    #[serde(with = "crate::native::path_bytes")]
    pub module_root: PathBuf,
    #[serde(with = "crate::native::path_bytes")]
    pub configuration: PathBuf,
    #[serde(with = "crate::native::optional_path_bytes")]
    pub application: Option<PathBuf>,
    pub artifacts: Vec<ProjectStagedArtifact>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ProjectSourceMetadata {
    pub name: String,
    pub requires: Vec<String>,
    pub source_digest: String,
    pub revision_digest: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    #[test]
    fn command_transport_preserves_non_utf8_paths_and_arguments() {
        let path = PathBuf::from(OsString::from_vec(b"/tmp/non-utf8-\xff".to_vec()));
        let options = RunOptions {
            directory: path.clone(),
            build: false,
            profile: "release".into(),
            repl: Some(ReplMode::Socket),
            socket_bind: Some("127.0.0.1".into()),
            socket_port: Some(1234),
            arguments: vec![
                OsString::from_vec(b"argument-\xfe".to_vec()),
                OsString::new(),
            ],
        };
        let bytes = serde_json::to_vec(&options).unwrap();
        assert_eq!(
            serde_json::from_slice::<RunOptions>(&bytes).unwrap(),
            options
        );
        let build = ProjectBuildResult {
            output_root: path.clone(),
            module_root: path.clone(),
            configuration: path.clone(),
            application: Some(path),
            artifacts: vec![],
        };
        let bytes = serde_json::to_vec(&build).unwrap();
        assert_eq!(
            serde_json::from_slice::<ProjectBuildResult>(&bytes).unwrap(),
            build
        );
    }
}
