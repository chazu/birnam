//! Project orchestration through scoped Id messages and the existing authored policy.
use super::source::SourceScope;
use super::{Project, ProjectSourceObservation, data::*, files::*, values};
use crate::{
    program::{MAX_SCRIPT_BYTES, read_source_file},
    runtime::{HostError, Scope, native_error},
};
use birnam_abi::{Oop, id_protocol::Operand};
use std::{
    collections::HashSet,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};
fn project_error(e: impl std::fmt::Debug) -> HostError {
    HostError(format!("{e:?}"))
}
impl Project<'_> {
    pub(crate) fn project_support(&self) -> Result<(), HostError> {
        let q = self.host.scope();
        for (name, global, source) in [
            (
                b"toml".as_slice(),
                b"Toml".as_slice(),
                include_bytes!("../../../../modules/toml.birnam").as_slice(),
            ),
            (
                b"projectPolicy",
                b"ProjectPolicy",
                include_bytes!("../../../../modules/project-policy.birnam").as_slice(),
            ),
        ] {
            if self.host.global(&q, global)? == self.host.named(b"nil")? {
                q.evaluate(source, Path::new("<project-support>"), Some(name))?;
                let manager = self.host.global(&q, b"Module")?;
                let name = unsafe { self.host.id.intern_name(name) }
                    .map_err(|e| native_error("project module", e))?;
                let result = q.call(manager, b"markLoaded:", &[Operand::object(name)])?;
                q.check_result(result)?;
            }
        }
        Ok(())
    }
    pub(crate) fn project_manifest(
        &self,
        root: &Path,
    ) -> Result<ProjectManifestObservation, HostError> {
        self.project_support()?;
        let q = self.host.scope();
        let source = q.string(&read_source_file(&root.join("birnam.toml"))?)?;
        let parsed = q.call(
            self.host.global(&q, b"Toml")?,
            b"parse:",
            &[Operand::object(source)],
        )?;
        q.check_result(parsed)?;
        values::decode_manifest(&q, parsed).map_err(project_error)
    }
    fn project_policy(&self, q: &Scope<'_>, name: &[u8], input: Oop) -> Result<Oop, HostError> {
        let result = q.call(
            self.host.global(q, b"ProjectPolicy")?,
            name,
            &[Operand::object(input)],
        )?;
        q.check_result(result)?;
        if result == self.host.named(b"nil")? {
            return Err(HostError("project policy rejected the observation".into()));
        }
        Ok(result)
    }
    fn project_observation(
        &self,
        root: &Path,
        is_root: bool,
    ) -> Result<ProjectObservation, HostError> {
        let manifest = self.project_manifest(root)?;
        let dependencies = resolve_project_dependencies_cached(root, &manifest.dependencies)
            .map_err(project_error)?;
        let root_string = root.to_string_lossy().into_owned();
        let mut sources = Vec::new();
        let mut explicit = HashSet::new();
        for module in &manifest.modules {
            let paths = project_composition_paths(root, &module.entry, &module.files)
                .map_err(project_error)?;
            for path in paths.iter().skip(1) {
                if self.index_project_source(path)?.is_some() {
                    return Err(HostError(format!(
                        "module part declares a module: {}",
                        path.display()
                    )));
                }
            }
            explicit.extend(paths.iter().cloned());
            let source = self.compose_project_sources(&paths)?;
            if let Some(mut observed) =
                self.project_source_bytes(source, &paths[0], &root_string)?
            {
                observed.sources = paths
                    .iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect();
                sources.push(observed);
            }
        }
        for path in
            collect_project_source_files(root, &manifest.source_roots).map_err(project_error)?
        {
            if !explicit.contains(&path) {
                if let Some(source) = self.observe_project_source(
                    &path,
                    &root_string,
                    &[path.to_string_lossy().into_owned()],
                )? {
                    sources.push(source)
                }
            }
        }
        validate_project_observations(&sources).map_err(project_error)?;
        let main = if is_root {
            manifest
                .main
                .map(|p| root.join(p).to_string_lossy().into_owned())
        } else {
            None
        };
        if let Some(main) = &main {
            if !Path::new(main).is_file() {
                return Err(HostError(format!("main unavailable: {main}")));
            }
        }
        Ok(ProjectObservation {
            root: root_string,
            manifest: root.join("birnam.toml").to_string_lossy().into_owned(),
            name: manifest.name,
            main,
            loads: manifest.loads,
            run: manifest.run,
            wrapped: manifest.wrapped,
            dependencies,
            sources,
            test_sources: manifest.test_sources,
        })
    }
    /// Module owns composition; only an owned byte copy crosses back to Host.
    pub fn compose_project_sources(&self, paths: &[PathBuf]) -> Result<Vec<u8>, HostError> {
        let q = self.host.scope();
        let mut vector = q.call(self.host.named(b"Vector")?, b"new", &[])?;
        for path in paths {
            let path = q.string(path.as_os_str().as_bytes())?;
            vector = q.call(vector, b"add:", &[Operand::object(path)])?;
        }
        let source = q.call(
            self.host.global(&q, b"Module")?,
            b"composeSources:",
            &[Operand::object(vector)],
        )?;
        q.check_result(source)?;
        let bytes = q.string_bytes(source)?;
        if bytes.len() as u64 > MAX_SCRIPT_BYTES {
            return Err(HostError("composed source exceeds limit".into()));
        }
        Ok(bytes)
    }
    pub(crate) fn project_source_bytes(
        &self,
        source: Vec<u8>,
        path: &Path,
        root: &str,
    ) -> Result<Option<ProjectSourceObservation>, HostError> {
        Ok(self
            .host
            .scope()
            .source_metadata(&source)?
            .map(|m| ProjectSourceObservation {
                name: m.name,
                path: path.to_string_lossy().into_owned(),
                sources: vec![path.to_string_lossy().into_owned()],
                project_root: root.into(),
                source,
                requires: m.requires,
                source_digest: m.source_digest,
                revision_digest: m.revision_digest,
            }))
    }
    pub fn observe_project_snapshot(&self, start: &Path) -> Result<ProjectSnapshot, HostError> {
        fn visit(
            h: &Project<'_>,
            root: &Path,
            is_root: bool,
            visiting: &mut HashSet<PathBuf>,
            seen: &mut HashSet<PathBuf>,
            out: &mut Vec<ProjectObservation>,
        ) -> Result<(), HostError> {
            if seen.contains(root) {
                return Ok(());
            }
            if !visiting.insert(root.into()) {
                return Err(HostError(format!(
                    "project dependency cycle: {}",
                    root.display()
                )));
            }
            let observation = h.project_observation(root, is_root)?;
            let deps = observation.dependencies.clone();
            out.push(observation);
            for dep in deps {
                visit(h, Path::new(&dep.root), false, visiting, seen, out)?
            }
            visiting.remove(root);
            seen.insert(root.into());
            Ok(())
        }
        let root = nearest_project_root(start).map_err(project_error)?;
        let mut projects = Vec::new();
        visit(
            self,
            &root,
            true,
            &mut HashSet::new(),
            &mut HashSet::new(),
            &mut projects,
        )?;
        Ok(ProjectSnapshot {
            root: root.to_string_lossy().into_owned(),
            projects,
        })
    }
    pub(crate) fn project_resolution(
        &self,
        snapshot: &ProjectSnapshot,
    ) -> Result<ProjectResolutionObservation, HostError> {
        let q = self.host.scope();
        let input = values::Builder { q: &q }.snapshot(snapshot)?;
        let result = self.project_policy(&q, b"resolveSnapshot:", input)?;
        values::decode_resolution(&q, result).map_err(project_error)
    }
}

impl Project<'_> {
    pub fn project_test_sources(&self, start: &Path) -> Result<(PathBuf, Vec<PathBuf>), HostError> {
        let root = nearest_project_root(start).map_err(project_error)?;
        let manifest = self.project_manifest(&root)?;
        let sources = manifest
            .test_sources
            .unwrap_or_else(|| vec!["tests".into()]);
        let mut files = Vec::new();
        for source in sources {
            let path = root.join(&source);
            if path.is_dir() {
                files.extend(collect_project_source_files(&root, &[source]).map_err(project_error)?)
            } else if path.is_file() {
                files.push(path)
            } else {
                return Err(HostError(format!(
                    "test source unavailable: {}",
                    path.display()
                )));
            }
        }
        files.sort();
        files.dedup();
        if files.is_empty() {
            return Err(HostError("project has no test sources".into()));
        }
        Ok((root, files))
    }
}
