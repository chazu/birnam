//! Project observations remain ordinary objects across messages and policy resolution.
use super::{Project, data::*};
use birnam_host_support::{HostError, Scope};
use birnam_abi::Oop;
use birnam_project_support::values;
use std::{
    ffi::OsString,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};
impl Project<'_> {
    pub(super) fn project_support(&self) -> Result<(), HostError> {
        let q = self.host.scope();
        self.host.owner_call(&q, b"HostProject", b"support", &[])?;
        Ok(())
    }
    pub(super) fn observe_project_snapshot(&self, q: &Scope<'_>, start: &Path) -> Result<Oop, HostError> {
        let path = q.string(start.as_os_str().as_bytes())?;
        self.host.owner_call(q, b"HostProject", b"snapshot:", &[path])
    }
    pub(super) fn project_manifest(&self, root: &Path) -> Result<ProjectManifestObservation, HostError> {
        let q = self.host.scope();
        let path = q.string(root.as_os_str().as_bytes())?;
        let result = self.host.owner_call(&q, b"HostProject", b"manifest:", &[path])?;
        values::decode_manifest(&q, result).map_err(|e| HostError(format!("{e:?}")))
    }
    pub(super) fn project_root(&self, start: &Path) -> Result<PathBuf, HostError> {
        let q = self.host.scope();
        let path = q.string(start.as_os_str().as_bytes())?;
        let result =
            self.host
                .owner_call(&q, b"HostProject", b"root:", &[path])?;
        Ok(PathBuf::from(OsString::from_vec(q.string_bytes(result)?)))
    }
    pub(super) fn project_resolution(&self, q: &Scope<'_>, snapshot: Oop) -> Result<ProjectResolutionObservation, HostError> {
        let result = self.host.owner_call(q, b"HostProject", b"resolveSnapshot:", &[snapshot])?;
        values::decode_resolution(q, result).map_err(|e| HostError(format!("{e:?}")))
    }
    pub(super) fn project_source_bytes(
        &self,
        source: Vec<u8>,
        path: &Path,
        root: &str,
    ) -> Result<Option<ProjectSourceObservation>, HostError> {
        let q = self.host.scope();
        let source = q.string(&source)?;
        let path = q.string(path.as_os_str().as_bytes())?;
        let root = q.string(root.as_bytes())?;
        let result = self.host.owner_call(
            &q,
            b"HostProject",
            b"observeBytes:path:root:",
            &[
                source,
                path,
                root,
            ],
        )?;
        if result == self.host.named(b"nil")? { Ok(None) }
        else { values::decode_source(&q, result).map(Some).map_err(|e| HostError(format!("{e:?}"))) }
    }
    pub(super) fn resolve_dependency_paths(
        &self,
        root: &Path,
        deps: &[ProjectDependencyObservation],
    ) -> Result<Vec<ProjectDependencyObservation>, HostError> {
        let q = self.host.scope();
        let root = q.string(root.as_os_str().as_bytes())?;
        let deps = values::Builder { q: &q }.dependencies(deps)?;
        let result = self.host.owner_call(
            &q,
            b"HostProject",
            b"dependencyPaths:dependencies:",
            &[root, deps],
        )?;
        values::decode_dependencies(&q, result).map_err(|e| HostError(format!("{e:?}")))
    }
}
