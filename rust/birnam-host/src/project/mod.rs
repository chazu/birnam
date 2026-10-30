//! Rust command adapter to the native HostProject owner. Implementations live
//! in libbirnam_host_project; only owned result values cross this boundary.
use crate::{HostError, HostRuntime};
use birnam_abi::Oop;
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
        args: &[Oop],
    ) -> Result<birnam_abi::Oop, HostError> {
        let owner = match name {
            b"testSources:" | b"index:" | b"observe:root:composition:" | b"embeddedFiles:" => {
                b"HostProject".as_slice()
            }
            _ => b"ProjectBuild".as_slice(),
        };
        self.host.context.owner_call(q, owner, name, args)
    }
    fn decode<T: birnam_host_support::project_values::CommandValue>(
        &self,
        q: &Scope<'_>,
        name: &[u8],
        args: &[Oop],
    ) -> Result<T, HostError> {
        let value = self.call(q, name, args)?;
        T::from_object(q, value)
    }
    pub fn check_project(&self, path: &Path) -> Result<ProjectCheckResult, HostError> {
        let q = self.host.scope();
        let path = q.string(path.as_os_str().as_bytes())?;
        self.decode(&q, b"check:", &[path])
    }
    pub fn fetch_project_dependencies(&self, path: &Path) -> Result<ProjectFetchResult, HostError> {
        let q = self.host.scope();
        let path = q.string(path.as_os_str().as_bytes())?;
        self.decode(&q, b"fetch:", &[path])
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
                start,
                target,
                profile,
                version,
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
            &[path, profile],
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
        let result = self.call(&q, b"install:directory:", &[build, dir])?;
        q.word_bits(q.call(self.host.named(b"Integer")?,b"wordOf:",&[result])?)
    }
    pub fn project_test_sources(&self, path: &Path) -> Result<(PathBuf, Vec<PathBuf>), HostError> {
        let q = self.host.scope();
        let path = q.string(path.as_os_str().as_bytes())?;
        let (root, files): (Vec<u8>, Vec<Vec<u8>>) =
            self.decode(&q, b"testSources:", &[path])?;
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
                root,
                path,
                profile,
            ],
        )
        .map(|_| ())
    }
    pub fn run_project(&self, options: &RunOptions, version: &str) -> Result<(), HostError> {
        let q = self.host.scope();
        let options = native::encode(&q, options)?;
        let version = q.string(version.as_bytes())?;
        self.call(&q, b"run:version:", &[options, version])
            .map(|_| ())
    }
    pub fn index_project_source(
        &self,
        path: &Path,
    ) -> Result<Option<ProjectSourceMetadata>, HostError> {
        let q = self.host.scope();
        let path = q.string(path.as_os_str().as_bytes())?;
        self.decode(&q, b"index:", &[path])
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
        let composition = native::encode(&q, &composition.to_vec())?;
        self.decode(
            &q,
            b"observe:root:composition:",
            &[path, root, composition],
        )
    }
    pub fn project_embedded_files(&self, path: &Path) -> Result<Vec<Vec<u8>>, HostError> {
        let q = self.host.scope();
        let path = q.string(path.as_os_str().as_bytes())?;
        self.decode(&q, b"embeddedFiles:", &[path])
    }
}
