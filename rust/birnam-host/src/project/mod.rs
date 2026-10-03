//! Rust command adapter to the native HostProject owner. Implementations live
//! in libbirnam_host_project; only owned result values cross this boundary.
use crate::{HostError, HostRuntime};
use birnam_abi::id_protocol::Operand;
pub use birnam_host_support::project::*;
use birnam_host_support::{Scope, native};
use std::{
    ffi::OsString,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};
pub struct Project<'h> {
    host: &'h HostRuntime,
}
impl HostRuntime {
    pub fn project(&self) -> Project<'_> {
        Project { host: self }
    }
}
impl Project<'_> {
    fn call(
        &self,
        q: &Scope<'_>,
        name: &[u8],
        args: &[Operand],
    ) -> Result<birnam_abi::Oop, HostError> {
        let owner = match name {
            b"testSources:" | b"index:" | b"observe:root:composition:" | b"embeddedFiles:" => {
                b"HostProject".as_slice()
            }
            _ => b"ProjectBuild".as_slice(),
        };
        self.host.context.owner_call(q, owner, name, args)
    }
    fn decode<T: serde::de::DeserializeOwned>(
        &self,
        q: &Scope<'_>,
        name: &[u8],
        args: &[Operand],
    ) -> Result<T, HostError> {
        let value = self.call(q, name, args)?;
        serde_json::from_slice(&q.string_bytes(value)?).map_err(|e| HostError(e.to_string()))
    }
    pub fn check_project(&self, path: &Path) -> Result<ProjectCheckResult, HostError> {
        let q = self.host.scope();
        let path = q.string(path.as_os_str().as_bytes())?;
        self.decode(&q, b"check:", &[Operand::object(path)])
    }
    pub fn fetch_project_dependencies(&self, path: &Path) -> Result<ProjectFetchResult, HostError> {
        let q = self.host.scope();
        let path = q.string(path.as_os_str().as_bytes())?;
        self.decode(&q, b"fetch:", &[Operand::object(path)])
    }
    pub fn build_project(
        &self,
        r: &ProjectBuildRequest<'_>,
    ) -> Result<ProjectBuildResult, HostError> {
        let q = self.host.scope();
        let start = q.string(r.start.as_os_str().as_bytes())?;
        let target = q.string(r.target.as_bytes())?;
        let profile = q.string(r.profile.as_bytes())?;
        let version = q.string(r.tool_version.as_bytes())?;
        self.decode(
            &q,
            b"build:target:profile:version:",
            &[
                Operand::object(start),
                Operand::object(target),
                Operand::object(profile),
                Operand::object(version),
            ],
        )
    }
    pub fn locate_project_build(
        &self,
        path: &Path,
        profile: &str,
    ) -> Result<ProjectBuildResult, HostError> {
        let q = self.host.scope();
        let path = q.string(path.as_os_str().as_bytes())?;
        let profile = q.string(profile.as_bytes())?;
        self.decode(
            &q,
            b"locate:profile:",
            &[Operand::object(path), Operand::object(profile)],
        )
    }
    pub fn install_project_build(
        &self,
        build: &ProjectBuildResult,
        dir: &Path,
    ) -> Result<usize, HostError> {
        let q = self.host.scope();
        let build = native::encode(&q, build)?;
        let dir = q.string(dir.as_os_str().as_bytes())?;
        let result = self.call(&q, b"install:directory:", &[build, Operand::object(dir)])?;
        self.host
            .context
            .send(
                self.host.named(b"Integer")?,
                b"wordOf:",
                &[Operand::object(result)],
            )?
            .as_word()
            .ok_or_else(|| HostError("invalid install count".into()))
    }
    pub fn project_test_sources(&self, path: &Path) -> Result<(PathBuf, Vec<PathBuf>), HostError> {
        let q = self.host.scope();
        let path = q.string(path.as_os_str().as_bytes())?;
        let (root, files): (Vec<u8>, Vec<Vec<u8>>) =
            self.decode(&q, b"testSources:", &[Operand::object(path)])?;
        Ok((
            PathBuf::from(OsString::from_vec(root)),
            files
                .into_iter()
                .map(|b| PathBuf::from(OsString::from_vec(b)))
                .collect(),
        ))
    }
    pub fn run_project_test_source(
        &self,
        root: &Path,
        path: &Path,
        profile: &str,
    ) -> Result<(), HostError> {
        let q = self.host.scope();
        let root = q.string(root.as_os_str().as_bytes())?;
        let path = q.string(path.as_os_str().as_bytes())?;
        let profile = q.string(profile.as_bytes())?;
        self.call(
            &q,
            b"test:source:profile:",
            &[
                Operand::object(root),
                Operand::object(path),
                Operand::object(profile),
            ],
        )
        .map(|_| ())
    }
    pub fn run_project(&self, options: &RunOptions, version: &str) -> Result<(), HostError> {
        let q = self.host.scope();
        let options = native::encode(&q, options)?;
        let version = q.string(version.as_bytes())?;
        self.call(&q, b"run:version:", &[options, Operand::object(version)])
            .map(|_| ())
    }
    pub fn index_project_source(
        &self,
        path: &Path,
    ) -> Result<Option<ProjectSourceMetadata>, HostError> {
        let q = self.host.scope();
        let path = q.string(path.as_os_str().as_bytes())?;
        self.decode(&q, b"index:", &[Operand::object(path)])
    }
    pub fn observe_project_source(
        &self,
        path: &Path,
        root: &str,
        composition: &[String],
    ) -> Result<Option<ProjectSourceObservation>, HostError> {
        let q = self.host.scope();
        let path = q.string(path.as_os_str().as_bytes())?;
        let root = q.string(root.as_bytes())?;
        let composition = native::encode(&q, &composition)?;
        self.decode(
            &q,
            b"observe:root:composition:",
            &[Operand::object(path), Operand::object(root), composition],
        )
    }
    pub fn project_embedded_files(&self, path: &Path) -> Result<Vec<Vec<u8>>, HostError> {
        let q = self.host.scope();
        let path = q.string(path.as_os_str().as_bytes())?;
        self.decode(&q, b"embeddedFiles:", &[Operand::object(path)])
    }
}
