//! Ordinary project records shared through messages with authored policy and native build code.
use super::data::*;
use crate::data::ProjectSourceObservation;
use birnam_abi::Oop;
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
            self.q.host.id.root,
            b"behaviorOf:",
            &[v],
        )?)
    }
    fn send(&self, r: Oop, n: &[u8], a: &[Oop]) -> Result<Oop, DecodeError> {
        Ok(self.q.call(
            r,
            n,
            a,
        )?)
    }
    fn number(&self, n: i64) -> Result<Oop, DecodeError> {
        Ok(self.q.call(
            self.q.host.named(b"Integer")?,
            b"fromWord:",
            &[self.q.word(n as usize)?],
        )?)
    }
    fn is_integer(&self, v: Oop) -> Result<bool, DecodeError> {
        Ok(self.behavior(v)? == self.q.host.named(b"IntegerVt")?)
    }
    fn integer(&self, v: Oop) -> Result<i64, DecodeError> {
        Ok(self.q.word_bits(self.q.call(self.q.host.named(b"Integer")?,b"wordOf:",&[v])?)? as i64)
    }
    fn at(&self, v: Oop, k: &str) -> Result<Oop, DecodeError> {
        self.q.exact(v, b"MapVt")?;
        self.send(v, b"at:", &[self.q.string(k.as_bytes())?])
    }
    fn string(&self, v: Oop) -> Result<String, DecodeError> {
        Ok(String::from_utf8_lossy(&self.q.string_bytes(v)?).into_owned())
    }
    fn bytes(&self, v: Oop) -> Result<Vec<u8>, DecodeError> {
        Ok(self.q.string_bytes(v)?)
    }
    fn text_field(&self, record: Oop, key: &str) -> Result<String, DecodeError> {
        self.string(self.at(record, key)?)
    }
    fn strings(
        &self,
        v: Oop,
        single: bool,
    ) -> Result<Vec<String>, DecodeError> {
        if v == self.nil {
            return Ok(vec![]);
        }
        if single && self.behavior(v)? == self.q.host.named(b"StringVt")? {
            return Ok(vec![self.string(v)?]);
        }
        let mut result = Vec::new();
        for i in 0..self.q.vector_size(v)? {
            result.push(self.string(self.q.vector_at(v, i)?)?)
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
            self.string(repl_value)?
        };
        if !matches!(repl.as_str(), "none" | "tty" | "socket") {
            return Err(ProjectManifestDecodeError::InvalidRunMode(repl));
        }
        let bind_value = self.at(value, bind_key)?;
        let socket_bind = if bind_value == self.nil {
            "127.0.0.1".to_owned()
        } else {
            let bind = self.string(bind_value)?;
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

    fn list_size(&self, value: Oop, field: &'static str) -> Result<i64, DecodeError> {
        if value == self.nil || self.behavior(value)? != self.vector_vt {
            return Err(DecodeError::InvalidList(field));
        }
        let size = self.send(value, b"size", &[])?;
        if !self.is_integer(size)? { return Err(DecodeError::InvalidList(field)); }
        let size = self.integer(size)?;
        if size < 0 { return Err(DecodeError::InvalidList(field)); }
        Ok(size)
    }
    fn item(&self, value: Oop, index: i64) -> Result<Oop, DecodeError> {
        self.send(value, b"at:", &[self.number(index)?])
    }

    fn modules(&self, value: Oop) -> Result<Vec<ProjectModulePlan>, ProjectManifestDecodeError> {
        if value == self.nil {
            return Ok(Vec::new());
        }
        let size = self.list_size(value, "module")?;
        let mut result = Vec::with_capacity(size as usize);
        for index in 0..size {
            let item = self.item(value, index)?;
            if item == self.nil || self.behavior(item)? != self.map_vt {
                return Err(ProjectManifestDecodeError::InvalidMap("module entry"));
            }
            let entry = self.text_field(item, "entry")?;
            if entry.is_empty()
                || Path::new(&entry).is_absolute()
                || Path::new(&entry).extension() != Some(OsStr::new("birnam"))
            {
                return Err(ProjectManifestDecodeError::InvalidPath("module.entry"));
            }
            let files = self.strings(self.at(item, "files")?, false)?;
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
        let size = self.list_size(value, "wrapped")?;
        let mut result = Vec::with_capacity(size as usize);
        for index in 0..size {
            let item = self.item(value, index)?;
            if item == self.nil || self.behavior(item)? != self.map_vt {
                return Err(ProjectManifestDecodeError::InvalidMap("wrapped entry"));
            }
            let name = self.text_field(item, "name")?;
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
            let command_count = self.send(build_value, b"size", &[])?;
            if !self.is_integer(command_count)? || self.integer(command_count)? <= 0 {
                return Err(ProjectManifestDecodeError::EmptyValue("wrapped.build"));
            }
            let mut build = Vec::with_capacity(self.integer(command_count)? as usize);
            let mut has_output = false;
            for command_index in 0..(self.integer(command_count)?) {
                let command =
                    self.item(build_value, command_index)?;
                let argv = self.strings(command, false)?;
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
            let packaging_value = self.at(item, "packaging")?;
            let packaging = if packaging_value == self.nil { "adjacent".into() }
                else { self.string(packaging_value)? };
            if !matches!(packaging.as_str(), "adjacent" | "embedded") {
                return Err(ProjectManifestDecodeError::InvalidPath("wrapped.packaging (adjacent or embedded)"));
            }
            result.push(ProjectWrappedObservation { name, build, packaging });
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
            let project_root = self.text_field(item, "projectRoot")?;
            result.push(ProjectWrappedArtifactObservation {
                project_root,
                name: artifact.name,
                build: artifact.build,
                packaging: artifact.packaging,
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
        let sources = self.strings(sources, false)?;
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
        let names = self.strings(keys, false)?;
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
                    let path = self.string(path_value)?;
                    if path.is_empty() || Path::new(&path).is_absolute() {
                        return Err(ProjectManifestDecodeError::InvalidPath("dependency.path"));
                    }
                    ProjectDependencySourceObservation::Path(path)
                }
                (false, true, true) => {
                    let url = self.string(git_value)?;
                    let revision = self.string(revision_value)?;
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
        decoder.string(name_value)?
    };
    let main_value = decoder.at(parsed, "main")?;
    let main = if main_value == decoder.nil {
        None
    } else {
        let main = decoder.string(main_value)?;
        if Path::new(&main).is_absolute() {
            return Err(ProjectManifestDecodeError::InvalidPath("main"));
        }
        Some(main)
    };
    let roots_value = decoder.at(parsed, "source_roots")?;
    let source_roots = if roots_value == decoder.nil {
        vec!["src".to_owned()]
    } else {
        decoder.strings(roots_value, false)?
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
    let loads = decoder.strings(decoder.at(parsed, "load")?, true)?;
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
    let project_root = decoder.text_field(resolution, "projectRoot")?;
    let manifest = decoder.text_field(resolution, "manifest")?;
    let project_name = decoder.text_field(resolution, "projectName")?;
    let main_value = decoder.at(resolution, "main")?;
    let main = if main_value == decoder.nil {
        None
    } else {
        Some(decoder.string(main_value)?)
    };
    let run = decoder.run(decoder.at(resolution, "run")?, "socketBind", "socketPort")?;
    let load_roots = decoder.strings(decoder.at(resolution, "loadRoots")?, false)?;
    let wrapped = decoder.wrapped_artifacts(decoder.at(resolution, "wrapped")?)?;
    let modules_value = decoder.at(resolution, "modules")?;
    let size = decoder.list_size(modules_value, "modules")?;
    let mut modules = Vec::with_capacity(size as usize);
    for index in 0..size {
        let item = decoder.item(modules_value, index)?;
        modules.push(decode_source(q, item)?);
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

/// Decode only when native build code needs the source record's values.
pub fn decode_source(q: &Scope<'_>, item: Oop) -> Result<ProjectSourceObservation, DecodeError> {
    let decoder = Decoder::new(q)?;
    if item == decoder.nil || decoder.behavior(item)? != decoder.map_vt {
        return Err(ProjectManifestDecodeError::InvalidMap("module"));
    }
    let name = decoder.text_field(item, "name")?;
    let path = decoder.text_field(item, "path")?;
    let sources = decoder.strings(decoder.at(item, "sources")?, false)?;
    let project_root =
        decoder.text_field(item, "projectRoot")?;
    let source = decoder.bytes(decoder.at(item, "source")?)?;
    let requires = decoder.strings(decoder.at(item, "requires")?, false)?;
    let source_digest =
        decoder.text_field(item, "sourceDigest")?;
    let revision_digest =
        decoder.text_field(item, "revisionDigest")?;
    Ok(ProjectSourceObservation {
        name,
        path,
        sources,
        project_root,
        source,
        requires,
        source_digest,
        revision_digest,
    })
}

pub fn decode_dependencies(q: &Scope<'_>, value: Oop) -> Result<Vec<ProjectDependencyObservation>, DecodeError> {
    let d = Decoder::new(q)?;
    let mut result = Vec::new();
    for i in 0..q.vector_size(value)? {
        let item = q.vector_at(value, i)?;
        result.push(ProjectDependencyObservation {
            name: d.text_field(item, "name")?,
            root: d.text_field(item, "root")?,
        });
    }
    Ok(result)
}

pub fn snapshot_test_sources(q: &Scope<'_>, snapshot: Oop) -> Result<Option<Vec<String>>, DecodeError> {
    let d = Decoder::new(q)?;
    let root = d.text_field(snapshot, "root")?;
    let projects = d.at(snapshot, "projects")?;
    for i in 0..q.vector_size(projects)? {
        let project = q.vector_at(projects, i)?;
        if d.text_field(project, "root")? == root {
            let value = d.at(project, "testSources")?;
            return if value == d.nil { Ok(None) } else { d.strings(value, false).map(Some) };
        }
    }
    Ok(None)
}

pub fn decode_artifact_plan(q: &Scope<'_>, plan: Oop) -> Result<ProjectArtifactPlan, DecodeError> {
    let decoder = Decoder::new(q)?;
    if plan == decoder.nil || decoder.behavior(plan)? != decoder.map_vt {
        return Err(ProjectManifestDecodeError::NotMap);
    }
    let publication = decoder.text_field(plan, "publication")?;
    let artifacts_value = decoder.at(plan, "artifacts")?;
    let size = decoder.list_size(artifacts_value, "artifacts")?;
    let mut artifacts = Vec::with_capacity(size as usize);
    for index in 0..size {
        let item = decoder.item(artifacts_value, index)?;
        if item == decoder.nil || decoder.behavior(item)? != decoder.map_vt {
            return Err(ProjectManifestDecodeError::InvalidMap("artifact"));
        }
        let name = decoder.text_field(item, "name")?;
        let path = decoder.text_field(item, "path")?;
        let action = decoder.text_field(item, "action")?;
        let source = decoder.text_field(item, "source")?;
        let sources = decoder.strings(decoder.at(item, "sources")?, false)?;
        let source_digest =
            decoder.text_field(item, "sourceDigest")?;
        let revision_digest = decoder.text_field(item, "revisionDigest")?;
        let requires =
            decoder.strings(decoder.at(item, "requires")?, false)?;
        let checksum_value = decoder.at(item, "checksum")?;
        let checksum = if checksum_value == decoder.nil {
            None
        } else {
            Some(decoder.string(checksum_value)?)
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
    fn vector<T>(&self, values: &[T], convert: impl Fn(&T) -> Result<Oop, HostError>) -> Result<Oop, HostError> {
        birnam_host_support::project_values::vector(self.q, values, |_, value| convert(value))
    }
    fn new_map(&self) -> Result<Oop, HostError> {
        self.q.call(self.q.host.named(b"Map")?, b"new", &[])
    }
    fn put(&self, v: Oop, k: &str, item: Oop) -> Result<Oop, HostError> {
        let key = self.q.string(k.as_bytes())?;
        self.q.replace_kept(v, self.q.host.send(v, b"at:put:", &[key, item])?)
    }
    fn string(&self, v: &str) -> Result<Oop, HostError> {
        self.q.string(v.as_bytes())
    }
    fn bytes(&self, v: &[u8]) -> Result<Oop, HostError> {
        self.q.string(v)
    }
    fn strings(&self, values: &[String]) -> Result<Oop, HostError> {
        self.vector(values, |value| self.string(value))
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
        let artifacts = self.vector(&value.artifacts, |artifact| self.previous_artifact(artifact))?;
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
        snapshot = self.put(snapshot, "projectRoot", self.string(&request.resolution.project_root)?)?;
        snapshot = self.put(snapshot, "target", self.string(request.target)?)?;
        snapshot = self.put(snapshot, "profile", self.string(request.profile)?)?;
        snapshot = self.put(snapshot, "toolVersion", self.string(request.tool_version)?)?;
        let artifact_abi = String::from_utf8_lossy(birnam_artifact::ABI);
        snapshot = self.put(snapshot, "artifactAbi", self.string(&artifact_abi)?)?;
        let modules = self.vector(&request.resolution.modules, |module| self.artifact_module(module))?;
        snapshot = self.put(snapshot, "modules", modules)?;
        let previous = if let Some(previous) = request.previous {
            self.previous_publication(previous)?
        } else {
            self.q.host.named(b"nil")?
        };
        self.put(snapshot, "previous", previous)
    }

    fn wrapped(&self, values: &[ProjectWrappedObservation]) -> Result<Oop, HostError> {
        self.vector(values, |value| {
            let mut item = self.new_map()?;
            item = self.put(item, "name", self.string(&value.name)?)?;
            item = self.put(item, "packaging", self.string(&value.packaging)?)?;
            let commands = self.vector(&value.build, |command| self.strings(command))?;
            item = self.put(item, "build", commands)?;
            Ok(item)
        })
    }

    pub fn dependencies(&self, values: &[ProjectDependencyObservation]) -> Result<Oop, HostError> {
        self.vector(values, |value| {
            let mut item = self.new_map()?;
            item = self.put(item, "name", self.string(&value.name)?)?;
            item = self.put(item, "root", self.string(&value.root)?)?;
            Ok(item)
        })
    }

    pub fn source(&self, value: &ProjectSourceObservation) -> Result<Oop, HostError> {
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
        let main = value.main.as_deref().map(|main| self.string(main)).transpose()?
            .unwrap_or(self.q.host.named(b"nil")?);
        item = self.put(item, "main", main)?;
        item = self.put(item, "loads", self.strings(&value.loads)?)?;
        let test_sources = match &value.test_sources { Some(v) => self.strings(v)?, None => self.q.host.named(b"nil")? };
        item = self.put(item, "testSources", test_sources)?;
        let mut run = self.new_map()?;
        run = self.put(run, "repl", self.string(&value.run.repl)?)?;
        run = self.put(run, "socketBind", self.string(&value.run.socket_bind)?)?;
        let port = self.q.call(self.q.host.named(b"Integer")?, b"fromWord:",
            &[birnam_host_support::size(value.run.socket_port as usize)?])?;
        run = self.put(run, "socketPort", port)?;
        item = self.put(item, "run", run)?;
        item = self.put(item, "wrapped", self.wrapped(&value.wrapped)?)?;
        item = self.put(item, "dependencies", self.dependencies(&value.dependencies)?)?;
        let sources = self.vector(&value.sources, |source| self.source(source))?;
        self.put(item, "sources", sources)
    }

    pub fn snapshot(&self, value: &ProjectSnapshot) -> Result<Oop, HostError> {
        let projects = self.vector(&value.projects, |project| self.project(project))?;
        let mut snapshot = self.new_map()?;
        snapshot = self.put(snapshot, "root", self.string(&value.root)?)?;
        self.put(snapshot, "projects", projects)
    }
}
