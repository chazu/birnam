//! Scoped value projections for authored project policy. No Oop leaves Host.
use super::data::*;
use crate::data::ProjectSourceObservation;
use birnam_abi::Oop;
use birnam_abi::id_protocol::Operand;
use birnam_host_support::{HostError, Scope};
use std::ffi::OsStr;
use std::{collections::HashSet, path::Path};
type DecodeError = ProjectManifestDecodeError;
struct Decoder<'a, 'h> {
    q: &'a Scope<'h>,
    nil: Oop,
    map_vt: Oop,
    vector_vt: Oop,
}
impl<'a, 'h> Decoder<'a, 'h> {
    fn new(q: &'a Scope<'h>) -> Result<Self, DecodeError> {
        Ok(Self {
            q,
            nil: q.host.named(b"nil")?,
            map_vt: q.host.named(b"MapVt")?,
            vector_vt: q.host.named(b"VectorVt")?,
        })
    }
    fn behavior(&self, v: Oop) -> Result<Oop, DecodeError> {
        Ok(self.q.call(
            self.q.host.id.receiver,
            b"behaviorOf:",
            &[Operand::object(v)],
        )?)
    }
    fn send(&self, r: Oop, n: &[u8], a: &[Oop]) -> Result<Oop, DecodeError> {
        Ok(self.q.call(
            r,
            n,
            &a.iter().copied().map(Operand::object).collect::<Vec<_>>(),
        )?)
    }
    fn number(&self, n: i64) -> Result<Oop, DecodeError> {
        Ok(self.q.call(
            self.q.host.named(b"Integer")?,
            b"fromWord:",
            &[Operand::word(n as usize)],
        )?)
    }
    fn is_integer(&self, v: Oop) -> Result<bool, DecodeError> {
        Ok(self.behavior(v)? == self.q.host.named(b"IntegerVt")?)
    }
    fn integer(&self, v: Oop) -> Result<i64, DecodeError> {
        Ok(self
            .q
            .host
            .send(
                self.q.host.named(b"Integer")?,
                b"wordOf:",
                &[Operand::object(v)],
            )?
            .as_word()
            .ok_or(DecodeError::InvalidPort)? as i64)
    }
    fn at(&self, v: Oop, k: &str) -> Result<Oop, DecodeError> {
        self.q.exact(v, b"MapVt")?;
        self.send(v, b"at:", &[self.q.string(k.as_bytes())?])
    }
    fn string(&self, v: Oop, _field: &'static str) -> Result<String, DecodeError> {
        Ok(String::from_utf8_lossy(&self.q.string_bytes(v)?).into_owned())
    }
    fn bytes(&self, v: Oop, _field: &'static str) -> Result<Vec<u8>, DecodeError> {
        Ok(self.q.string_bytes(v)?)
    }
    fn strings(
        &self,
        v: Oop,
        field: &'static str,
        single: bool,
    ) -> Result<Vec<String>, DecodeError> {
        if v == self.nil {
            return Ok(vec![]);
        }
        if single && self.behavior(v)? == self.q.host.named(b"StringVt")? {
            return Ok(vec![self.string(v, field)?]);
        }
        let mut result = Vec::new();
        for i in 0..self.q.vector_size(v)? {
            result.push(self.string(self.q.vector_at(v, i)?, field)?)
        }
        Ok(result)
    }
    fn run(
        &self,
        value: Oop,
        bind_key: &str,
        port_key: &str,
    ) -> Result<ProjectRunObservation, ProjectManifestDecodeError> {
        if value == self.nil {
            return Ok(ProjectRunObservation {
                repl: "none".to_owned(),
                socket_bind: "127.0.0.1".to_owned(),
                socket_port: 4400,
            });
        }
        if self.behavior(value)? != self.map_vt {
            return Err(ProjectManifestDecodeError::InvalidMap("run"));
        }
        let repl_value = self.at(value, "repl")?;
        let repl = if repl_value == self.nil {
            "none".to_owned()
        } else {
            self.string(repl_value, "run.repl")?
        };
        if !matches!(repl.as_str(), "none" | "tty" | "socket") {
            return Err(ProjectManifestDecodeError::InvalidRunMode(repl));
        }
        let bind_value = self.at(value, bind_key)?;
        let socket_bind = if bind_value == self.nil {
            "127.0.0.1".to_owned()
        } else {
            let bind = self.string(bind_value, "run.socket_bind")?;
            if bind.is_empty() {
                return Err(ProjectManifestDecodeError::EmptyValue("run.socket_bind"));
            }
            bind
        };
        let port_value = self.at(value, port_key)?;
        let socket_port = if port_value == self.nil {
            4400
        } else if !self.is_integer(port_value)? {
            return Err(ProjectManifestDecodeError::InvalidPort);
        } else {
            let port = self.integer(port_value)?;
            if !(0..=65535).contains(&port) {
                return Err(ProjectManifestDecodeError::InvalidPort);
            }
            port
        };
        Ok(ProjectRunObservation {
            repl,
            socket_bind,
            socket_port,
        })
    }

    fn modules(&self, value: Oop) -> Result<Vec<ProjectModulePlan>, ProjectManifestDecodeError> {
        if value == self.nil {
            return Ok(Vec::new());
        }
        if self.behavior(value)? != self.vector_vt {
            return Err(ProjectManifestDecodeError::InvalidList("module"));
        }
        let size_selector = b"size".as_slice();
        let at_selector = b"at:".as_slice();
        let size = self.send(value, size_selector, &[])?;
        if !self.is_integer(size)? || self.integer(size)? < 0 {
            return Err(ProjectManifestDecodeError::InvalidList("module"));
        }
        let mut result = Vec::with_capacity(self.integer(size)? as usize);
        for index in 0..(self.integer(size)?) {
            let item = self.send(value, at_selector, &[self.number(index)?])?;
            if item == self.nil || self.behavior(item)? != self.map_vt {
                return Err(ProjectManifestDecodeError::InvalidMap("module entry"));
            }
            let entry = self.string(self.at(item, "entry")?, "module.entry")?;
            if entry.is_empty()
                || Path::new(&entry).is_absolute()
                || Path::new(&entry).extension() != Some(OsStr::new("birnam"))
            {
                return Err(ProjectManifestDecodeError::InvalidPath("module.entry"));
            }
            let files = self.strings(self.at(item, "files")?, "module.files", false)?;
            let _ = project_composition_paths(Path::new("."), &entry, &files)
                .map_err(|_| ProjectManifestDecodeError::InvalidPath("module.files"))?;
            result.push(ProjectModulePlan { entry, files });
        }
        Ok(result)
    }

    fn wrapped(
        &self,
        value: Oop,
    ) -> Result<Vec<ProjectWrappedObservation>, ProjectManifestDecodeError> {
        if value == self.nil {
            return Ok(Vec::new());
        }
        if self.behavior(value)? != self.vector_vt {
            return Err(ProjectManifestDecodeError::InvalidList("wrapped"));
        }
        let size_selector = b"size".as_slice();
        let at_selector = b"at:".as_slice();
        let size = self.send(value, size_selector, &[])?;
        if !self.is_integer(size)? || self.integer(size)? < 0 {
            return Err(ProjectManifestDecodeError::InvalidList("wrapped"));
        }
        let mut result = Vec::with_capacity(self.integer(size)? as usize);
        for index in 0..(self.integer(size)?) {
            let item = self.send(value, at_selector, &[self.number(index)?])?;
            if item == self.nil || self.behavior(item)? != self.map_vt {
                return Err(ProjectManifestDecodeError::InvalidMap("wrapped entry"));
            }
            let name = self.string(self.at(item, "name")?, "wrapped.name")?;
            if name.is_empty() || name.contains('/') || Path::new(&name).is_absolute() {
                return Err(ProjectManifestDecodeError::InvalidPath("wrapped.name"));
            }
            if result
                .iter()
                .any(|entry: &ProjectWrappedObservation| entry.name == name)
            {
                return Err(ProjectManifestDecodeError::DuplicateWrapped(name));
            }
            let build_value = self.at(item, "build")?;
            if build_value == self.nil {
                return Err(ProjectManifestDecodeError::MissingValue("wrapped.build"));
            }
            if self.behavior(build_value)? != self.vector_vt {
                return Err(ProjectManifestDecodeError::InvalidList("wrapped.build"));
            }
            let command_count = self.send(build_value, size_selector, &[])?;
            if !self.is_integer(command_count)? || self.integer(command_count)? <= 0 {
                return Err(ProjectManifestDecodeError::EmptyValue("wrapped.build"));
            }
            let mut build = Vec::with_capacity(self.integer(command_count)? as usize);
            let mut has_output = false;
            for command_index in 0..(self.integer(command_count)?) {
                let command =
                    self.send(build_value, at_selector, &[self.number(command_index)?])?;
                let argv = self.strings(command, "wrapped.build command", false)?;
                if argv.is_empty() {
                    return Err(ProjectManifestDecodeError::EmptyValue(
                        "wrapped.build command",
                    ));
                }
                has_output |= argv.iter().any(|argument| argument.contains("{output}"));
                build.push(argv);
            }
            if !has_output {
                return Err(ProjectManifestDecodeError::MissingWrappedOutput(name));
            }
            result.push(ProjectWrappedObservation { name, build });
        }
        Ok(result)
    }

    fn wrapped_artifacts(
        &self,
        value: Oop,
    ) -> Result<Vec<ProjectWrappedArtifactObservation>, ProjectManifestDecodeError> {
        let wrapped = self.wrapped(value)?;
        if wrapped.is_empty() {
            return Ok(Vec::new());
        }
        let at_selector = b"at:".as_slice();
        let mut result = Vec::with_capacity(wrapped.len());
        for (index, artifact) in wrapped.into_iter().enumerate() {
            let item = self.send(value, at_selector, &[self.number(index as i64)?])?;
            let project_root = self.string(self.at(item, "projectRoot")?, "wrapped.projectRoot")?;
            result.push(ProjectWrappedArtifactObservation {
                project_root,
                name: artifact.name,
                build: artifact.build,
            });
        }
        Ok(result)
    }

    fn test_sources(&self, value: Oop) -> Result<Option<Vec<String>>, ProjectManifestDecodeError> {
        if value == self.nil {
            return Ok(None);
        }
        if self.behavior(value)? != self.map_vt {
            return Err(ProjectManifestDecodeError::InvalidMap("test"));
        }
        let sources = self.at(value, "sources")?;
        if sources == self.nil {
            return Err(ProjectManifestDecodeError::MissingValue("test.sources"));
        }
        let sources = self.strings(sources, "test.sources", false)?;
        for source in &sources {
            let path = Path::new(source);
            if source.is_empty()
                || path.is_absolute()
                || path.components().any(|component| {
                    matches!(
                        component,
                        std::path::Component::CurDir | std::path::Component::ParentDir
                    )
                })
            {
                return Err(ProjectManifestDecodeError::InvalidTestSource(
                    source.clone(),
                ));
            }
        }
        Ok(Some(sources))
    }

    fn dependencies(
        &self,
        value: Oop,
    ) -> Result<Vec<ProjectManifestDependencyObservation>, ProjectManifestDecodeError> {
        if value == self.nil {
            return Ok(Vec::new());
        }
        if self.behavior(value)? != self.map_vt {
            return Err(ProjectManifestDecodeError::InvalidMap("dependencies"));
        }
        let keys_selector = b"keys".as_slice();
        let keys = self.send(value, keys_selector, &[])?;
        let names = self.strings(keys, "dependency names", false)?;
        let mut result = Vec::with_capacity(names.len());
        for name in names {
            let spec = self
                .at(value, &name)
                .map_err(|_| ProjectManifestDecodeError::InvalidMap("dependency spec"))?;
            if spec == self.nil || self.behavior(spec)? != self.map_vt {
                return Err(ProjectManifestDecodeError::InvalidMap("dependency spec"));
            }
            let path_value = self.at(spec, "path")?;
            let git_value = self.at(spec, "git")?;
            let revision_value = self.at(spec, "rev")?;
            let source = match (
                path_value != self.nil,
                git_value != self.nil,
                revision_value != self.nil,
            ) {
                (true, false, false) => {
                    let path = self.string(path_value, "dependency.path")?;
                    if path.is_empty() || Path::new(&path).is_absolute() {
                        return Err(ProjectManifestDecodeError::InvalidPath("dependency.path"));
                    }
                    ProjectDependencySourceObservation::Path(path)
                }
                (false, true, true) => {
                    let url = self.string(git_value, "dependency.git")?;
                    let revision = self.string(revision_value, "dependency.rev")?;
                    if !valid_git_remote(&url) {
                        return Err(ProjectManifestDecodeError::InvalidDependency(name));
                    }
                    let revision = normalize_git_revision(&revision).ok_or_else(|| {
                        ProjectManifestDecodeError::InvalidGitRevision {
                            name: name.clone(),
                            revision,
                        }
                    })?;
                    ProjectDependencySourceObservation::Git { url, revision }
                }
                _ => return Err(ProjectManifestDecodeError::InvalidDependency(name)),
            };
            result.push(ProjectManifestDependencyObservation { name, source });
        }
        result.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(result)
    }
}
pub fn decode_manifest(
    q: &Scope<'_>,
    parsed: Oop,
) -> Result<ProjectManifestObservation, DecodeError> {
    let decoder = Decoder::new(q)?;
    if parsed == decoder.nil || decoder.behavior(parsed)? != decoder.map_vt {
        return Err(ProjectManifestDecodeError::NotMap);
    }
    let name_value = decoder.at(parsed, "name")?;
    let name = if name_value == decoder.nil {
        String::new()
    } else {
        decoder.string(name_value, "name")?
    };
    let main_value = decoder.at(parsed, "main")?;
    let main = if main_value == decoder.nil {
        None
    } else {
        let main = decoder.string(main_value, "main")?;
        if Path::new(&main).is_absolute() {
            return Err(ProjectManifestDecodeError::InvalidPath("main"));
        }
        Some(main)
    };
    let roots_value = decoder.at(parsed, "source_roots")?;
    let source_roots = if roots_value == decoder.nil {
        vec!["src".to_owned()]
    } else {
        decoder.strings(roots_value, "source_roots", false)?
    };
    if source_roots.is_empty() {
        return Err(ProjectManifestDecodeError::EmptyValue("source_roots"));
    }
    if source_roots
        .iter()
        .any(|root| Path::new(root).is_absolute())
    {
        return Err(ProjectManifestDecodeError::InvalidPath("source_roots"));
    }
    let loads = decoder.strings(decoder.at(parsed, "load")?, "load", true)?;
    let run = decoder.run(decoder.at(parsed, "run")?, "socket_bind", "socket_port")?;
    let modules = decoder.modules(decoder.at(parsed, "module")?)?;
    let dependencies = decoder.dependencies(decoder.at(parsed, "dependencies")?)?;
    let wrapped = decoder.wrapped(decoder.at(parsed, "wrapped")?)?;
    let test_sources = decoder.test_sources(decoder.at(parsed, "test")?)?;
    Ok(ProjectManifestObservation {
        name,
        main,
        source_roots,
        loads,
        modules,
        dependencies,
        run,
        wrapped,
        test_sources,
    })
}

pub fn decode_resolution(
    q: &Scope<'_>,
    resolution: Oop,
) -> Result<ProjectResolutionObservation, DecodeError> {
    let decoder = Decoder::new(q)?;
    if resolution == decoder.nil || decoder.behavior(resolution)? != decoder.map_vt {
        return Err(ProjectManifestDecodeError::NotMap);
    }
    let project_root = decoder.string(decoder.at(resolution, "projectRoot")?, "projectRoot")?;
    let manifest = decoder.string(decoder.at(resolution, "manifest")?, "manifest")?;
    let project_name = decoder.string(decoder.at(resolution, "projectName")?, "projectName")?;
    let main_value = decoder.at(resolution, "main")?;
    let main = if main_value == decoder.nil {
        None
    } else {
        Some(decoder.string(main_value, "main")?)
    };
    let run = decoder.run(decoder.at(resolution, "run")?, "socketBind", "socketPort")?;
    let load_roots = decoder.strings(decoder.at(resolution, "loadRoots")?, "loadRoots", false)?;
    let wrapped = decoder.wrapped_artifacts(decoder.at(resolution, "wrapped")?)?;
    let modules_value = decoder.at(resolution, "modules")?;
    if modules_value == decoder.nil || decoder.behavior(modules_value)? != decoder.vector_vt {
        return Err(ProjectManifestDecodeError::InvalidList("modules"));
    }
    let size_selector = b"size".as_slice();
    let at_selector = b"at:".as_slice();
    let size = decoder.send(modules_value, size_selector, &[])?;
    if !decoder.is_integer(size)? || decoder.integer(size)? < 0 {
        return Err(ProjectManifestDecodeError::InvalidList("modules"));
    }
    let mut modules = Vec::with_capacity(decoder.integer(size)? as usize);
    for index in 0..(decoder.integer(size)?) {
        let item = decoder.send(modules_value, at_selector, &[decoder.number(index)?])?;
        if item == decoder.nil || decoder.behavior(item)? != decoder.map_vt {
            return Err(ProjectManifestDecodeError::InvalidMap("module"));
        }
        let name = decoder.string(decoder.at(item, "name")?, "module.name")?;
        let path = decoder.string(decoder.at(item, "path")?, "module.path")?;
        let sources = decoder.strings(decoder.at(item, "sources")?, "module.sources", false)?;
        let project_root =
            decoder.string(decoder.at(item, "projectRoot")?, "module.projectRoot")?;
        let source = decoder.bytes(decoder.at(item, "source")?, "module.source")?;
        let requires = decoder.strings(decoder.at(item, "requires")?, "module.requires", false)?;
        let source_digest =
            decoder.string(decoder.at(item, "sourceDigest")?, "module.sourceDigest")?;
        let revision_digest =
            decoder.string(decoder.at(item, "revisionDigest")?, "module.revisionDigest")?;
        modules.push(ProjectSourceObservation {
            name,
            path,
            sources,
            project_root,
            source,
            requires,
            source_digest,
            revision_digest,
        });
    }
    // The source policy's dependency walk is allowed to encounter the same
    // provider through several roots. Preserve its first dependency-first
    // occurrence at the host boundary so one logical module is checked,
    // compiled, and bundled once.
    let mut seen = HashSet::new();
    modules.retain(|module| seen.insert(module.name.clone()));
    Ok(ProjectResolutionObservation {
        project_root,
        manifest,
        project_name,
        main,
        run,
        load_roots,
        modules,
        wrapped,
    })
}

pub fn decode_artifact_plan(q: &Scope<'_>, plan: Oop) -> Result<ProjectArtifactPlan, DecodeError> {
    let decoder = Decoder::new(q)?;
    if plan == decoder.nil || decoder.behavior(plan)? != decoder.map_vt {
        return Err(ProjectManifestDecodeError::NotMap);
    }
    let publication = decoder.string(decoder.at(plan, "publication")?, "publication")?;
    let artifacts_value = decoder.at(plan, "artifacts")?;
    if artifacts_value == decoder.nil || decoder.behavior(artifacts_value)? != decoder.vector_vt {
        return Err(ProjectManifestDecodeError::InvalidList("artifacts"));
    }
    let size_selector = b"size".as_slice();
    let at_selector = b"at:".as_slice();
    let size = decoder.send(artifacts_value, size_selector, &[])?;
    if !decoder.is_integer(size)? || decoder.integer(size)? < 0 {
        return Err(ProjectManifestDecodeError::InvalidList("artifacts"));
    }
    let mut artifacts = Vec::with_capacity(decoder.integer(size)? as usize);
    for index in 0..(decoder.integer(size)?) {
        let item = decoder.send(artifacts_value, at_selector, &[decoder.number(index)?])?;
        if item == decoder.nil || decoder.behavior(item)? != decoder.map_vt {
            return Err(ProjectManifestDecodeError::InvalidMap("artifact"));
        }
        let name = decoder.string(decoder.at(item, "name")?, "artifact.name")?;
        let path = decoder.string(decoder.at(item, "path")?, "artifact.path")?;
        let action = decoder.string(decoder.at(item, "action")?, "artifact.action")?;
        let source = decoder.string(decoder.at(item, "source")?, "artifact.source")?;
        let sources = decoder.strings(decoder.at(item, "sources")?, "artifact.sources", false)?;
        let source_digest =
            decoder.string(decoder.at(item, "sourceDigest")?, "artifact.sourceDigest")?;
        let revision_digest = decoder.string(
            decoder.at(item, "revisionDigest")?,
            "artifact.revisionDigest",
        )?;
        let requires =
            decoder.strings(decoder.at(item, "requires")?, "artifact.requires", false)?;
        let checksum_value = decoder.at(item, "checksum")?;
        let checksum = if checksum_value == decoder.nil {
            None
        } else {
            Some(decoder.string(checksum_value, "artifact.checksum")?)
        };
        artifacts.push(ProjectArtifactPlanEntry {
            name,
            path,
            action,
            source,
            sources,
            source_digest,
            revision_digest,
            requires,
            checksum,
        });
    }
    Ok(ProjectArtifactPlan {
        artifacts,
        publication,
    })
}

pub struct Builder<'a, 'h> {
    pub q: &'a Scope<'h>,
}
impl Builder<'_, '_> {
    fn new_vector(&self) -> Result<Oop, HostError> {
        self.q.call(self.q.host.named(b"Vector")?, b"new", &[])
    }
    fn new_map(&self) -> Result<Oop, HostError> {
        self.q.call(self.q.host.named(b"Map")?, b"new", &[])
    }
    fn add(&self, v: Oop, item: Oop) -> Result<Oop, HostError> {
        self.q.call(v, b"add:", &[Operand::object(item)])
    }
    fn put(&self, v: Oop, k: &str, item: Oop) -> Result<Oop, HostError> {
        let key = self.q.string(k.as_bytes())?;
        self.q.call(
            v,
            b"at:put:",
            &[Operand::object(key), Operand::object(item)],
        )
    }
    fn string(&self, v: &str) -> Result<Oop, HostError> {
        self.q.string(v.as_bytes())
    }
    fn bytes(&self, v: &[u8]) -> Result<Oop, HostError> {
        self.q.string(v)
    }
    fn strings(&self, values: &[String]) -> Result<Oop, HostError> {
        let mut vector = self.new_vector()?;
        for value in values {
            vector = self.add(vector, self.string(value)?)?;
        }
        Ok(vector)
    }

    fn previous_artifact(
        &self,
        value: &ProjectPreviousPublishedArtifact,
    ) -> Result<Oop, HostError> {
        let mut item = self.new_map()?;
        item = self.put(item, "name", self.string(&value.name)?)?;
        item = self.put(item, "path", self.string(&value.path)?)?;
        item = self.put(item, "source", self.string(&value.source)?)?;
        item = self.put(item, "sources", self.strings(&value.sources)?)?;
        item = self.put(item, "sourceDigest", self.string(&value.source_digest)?)?;
        item = self.put(item, "revisionDigest", self.string(&value.revision_digest)?)?;
        item = self.put(item, "requires", self.strings(&value.requires)?)?;
        item = self.put(item, "checksum", self.string(&value.checksum)?)?;
        if let Some(observed) = &value.observed_checksum {
            item = self.put(item, "observedChecksum", self.string(observed)?)?;
        }
        Ok(item)
    }

    fn previous_publication(&self, value: &ProjectPreviousPublication) -> Result<Oop, HostError> {
        let mut result = self.new_map()?;
        result = self.put(result, "projectRoot", self.string(&value.project_root)?)?;
        result = self.put(result, "target", self.string(&value.target)?)?;
        result = self.put(result, "profile", self.string(&value.profile)?)?;
        result = self.put(result, "toolVersion", self.string(&value.tool_version)?)?;
        result = self.put(result, "artifactAbi", self.string(&value.artifact_abi)?)?;
        let mut artifacts = self.new_vector()?;
        for artifact in &value.artifacts {
            artifacts = self.add(artifacts, self.previous_artifact(artifact)?)?;
        }
        self.put(result, "artifacts", artifacts)
    }

    fn artifact_module(&self, value: &ProjectSourceObservation) -> Result<Oop, HostError> {
        let mut item = self.new_map()?;
        item = self.put(item, "name", self.string(&value.name)?)?;
        item = self.put(item, "path", self.string(&value.path)?)?;
        item = self.put(item, "sources", self.strings(&value.sources)?)?;
        item = self.put(item, "sourceDigest", self.string(&value.source_digest)?)?;
        item = self.put(item, "revisionDigest", self.string(&value.revision_digest)?)?;
        self.put(item, "requires", self.strings(&value.requires)?)
    }

    pub fn artifact_plan_input(
        &self,
        request: &ProjectArtifactPlanRequest<'_>,
    ) -> Result<Oop, HostError> {
        let mut snapshot = self.new_map()?;
        snapshot = {
            self.put(
                snapshot,
                "projectRoot",
                self.string(&request.resolution.project_root)?,
            )?
        };
        snapshot = self.put(snapshot, "target", self.string(request.target)?)?;
        snapshot = self.put(snapshot, "profile", self.string(request.profile)?)?;
        snapshot = self.put(snapshot, "toolVersion", self.string(request.tool_version)?)?;
        let artifact_abi = String::from_utf8_lossy(birnam_artifact::ABI);
        snapshot = self.put(snapshot, "artifactAbi", self.string(&artifact_abi)?)?;
        let mut modules = self.new_vector()?;
        for module in &request.resolution.modules {
            modules = self.add(modules, self.artifact_module(module)?)?;
        }
        snapshot = self.put(snapshot, "modules", modules)?;
        let previous = if let Some(previous) = request.previous {
            self.previous_publication(previous)?
        } else {
            self.q.host.named(b"nil")?
        };
        self.put(snapshot, "previous", previous)
    }

    fn wrapped(&self, values: &[ProjectWrappedObservation]) -> Result<Oop, HostError> {
        let mut result = self.new_vector()?;
        for value in values {
            let mut item = self.new_map()?;
            item = self.put(item, "name", self.string(&value.name)?)?;
            let mut commands = self.new_vector()?;
            for command in &value.build {
                commands = self.add(commands, self.strings(command)?)?;
            }
            item = self.put(item, "build", commands)?;
            result = self.add(result, item)?;
        }
        Ok(result)
    }

    fn dependencies(&self, values: &[ProjectDependencyObservation]) -> Result<Oop, HostError> {
        let mut result = self.new_vector()?;
        for value in values {
            let mut item = self.new_map()?;
            item = self.put(item, "name", self.string(&value.name)?)?;
            item = self.put(item, "root", self.string(&value.root)?)?;
            result = self.add(result, item)?;
        }
        Ok(result)
    }

    fn source(&self, value: &ProjectSourceObservation) -> Result<Oop, HostError> {
        let mut item = self.new_map()?;
        item = self.put(item, "name", self.string(&value.name)?)?;
        item = self.put(item, "path", self.string(&value.path)?)?;
        item = self.put(item, "sources", self.strings(&value.sources)?)?;
        item = self.put(item, "projectRoot", self.string(&value.project_root)?)?;
        item = self.put(item, "source", self.bytes(&value.source)?)?;
        item = self.put(item, "requires", self.strings(&value.requires)?)?;
        item = self.put(item, "sourceDigest", self.string(&value.source_digest)?)?;
        item = self.put(item, "revisionDigest", self.string(&value.revision_digest)?)?;
        Ok(item)
    }

    fn project(&self, value: &ProjectObservation) -> Result<Oop, HostError> {
        let mut item = self.new_map()?;
        item = self.put(item, "root", self.string(&value.root)?)?;
        item = self.put(item, "manifest", self.string(&value.manifest)?)?;
        item = self.put(item, "name", self.string(&value.name)?)?;
        item = {
            self.put(
                item,
                "main",
                value
                    .main
                    .as_deref()
                    .map(|main| self.string(main))
                    .transpose()?
                    .unwrap_or(self.q.host.named(b"nil")?),
            )?
        };
        item = self.put(item, "loads", self.strings(&value.loads)?)?;
        let mut run = self.new_map()?;
        run = self.put(run, "repl", self.string(&value.run.repl)?)?;
        run = self.put(run, "socketBind", self.string(&value.run.socket_bind)?)?;
        run = {
            self.put(
                run,
                "socketPort",
                self.q.call(
                    self.q.host.named(b"Integer")?,
                    b"fromWord:",
                    &[Operand::word(value.run.socket_port as usize)],
                )?,
            )?
        };
        item = self.put(item, "run", run)?;
        item = self.put(item, "wrapped", self.wrapped(&value.wrapped)?)?;
        item = {
            self.put(
                item,
                "dependencies",
                self.dependencies(&value.dependencies)?,
            )?
        };
        let mut sources = self.new_vector()?;
        for source in &value.sources {
            sources = self.add(sources, self.source(source)?)?;
        }
        self.put(item, "sources", sources)
    }

    pub fn snapshot(&self, value: &ProjectSnapshot) -> Result<Oop, HostError> {
        let mut projects = self.new_vector()?;
        for project in &value.projects {
            projects = self.add(projects, self.project(project)?)?;
        }
        let mut snapshot = self.new_map()?;
        snapshot = self.put(snapshot, "root", self.string(&value.root)?)?;
        self.put(snapshot, "projects", projects)
    }
}
