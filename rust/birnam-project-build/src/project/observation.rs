//! Current Id queries to the read-only project owner. Only owned bytes and value
//! records cross; no observer state or scoped native callback is borrowed.
use super::{Project, data::*};
use birnam_host_support::{HostError, native};
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
    fn query<T: serde::de::DeserializeOwned>(
        &self,
        name: &[u8],
        path: &Path,
    ) -> Result<T, HostError> {
        let q = self.host.scope();
        let path = q.string(path.as_os_str().as_bytes())?;
        let result = self
            .host
            .owner_call(&q, b"HostProject", name, &[path])?;
        serde_json::from_slice(&q.string_bytes(result)?).map_err(|e| HostError(e.to_string()))
    }
    pub(super) fn observe_project_snapshot(
        &self,
        start: &Path,
    ) -> Result<ProjectSnapshot, HostError> {
        self.query(b"snapshot:", start)
    }
    pub(super) fn project_manifest(
        &self,
        root: &Path,
    ) -> Result<ProjectManifestObservation, HostError> {
        self.query(b"manifest:", root)
    }
    pub(super) fn project_root(&self, start: &Path) -> Result<PathBuf, HostError> {
        let q = self.host.scope();
        let path = q.string(start.as_os_str().as_bytes())?;
        let result =
            self.host
                .owner_call(&q, b"HostProject", b"root:", &[path])?;
        Ok(PathBuf::from(OsString::from_vec(q.string_bytes(result)?)))
    }
    pub(super) fn project_resolution(
        &self,
        snapshot: &ProjectSnapshot,
    ) -> Result<ProjectResolutionObservation, HostError> {
        let q = self.host.scope();
        let input = native::encode(&q, snapshot)?;
        let result = self
            .host
            .owner_call(&q, b"HostProject", b"resolveSnapshot:", &[input])?;
        serde_json::from_slice(&q.string_bytes(result)?).map_err(|e| HostError(e.to_string()))
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
        serde_json::from_slice(&q.string_bytes(result)?).map_err(|e| HostError(e.to_string()))
    }
    pub(super) fn resolve_dependency_paths(
        &self,
        root: &Path,
        deps: &[ProjectDependencyObservation],
    ) -> Result<Vec<ProjectDependencyObservation>, HostError> {
        let q = self.host.scope();
        let root = q.string(root.as_os_str().as_bytes())?;
        let deps = native::encode(&q, &deps)?;
        let result = self.host.owner_call(
            &q,
            b"HostProject",
            b"dependencyPaths:dependencies:",
            &[root, deps],
        )?;
        serde_json::from_slice(&q.string_bytes(result)?).map_err(|e| HostError(e.to_string()))
    }
}
