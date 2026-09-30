//! Project orchestration through scoped Id messages and the existing authored policy.
use super::*;
use crate::project_data::*;
use crate::project_files::*;
use birnam_artifact::digest::sha256_hex;
use std::{collections::HashSet, path::PathBuf};
#[path = "project_values.rs"]
pub(super) mod values;
fn project_error(e: impl std::fmt::Debug) -> HostError {
    HostError(format!("{e:?}"))
}
impl HostRuntime {
    pub(super) fn project_support(&self) -> Result<(), HostError> {
        let q = self.scope();
        for (name, global, source) in [
            (
                b"toml".as_slice(),
                b"Toml".as_slice(),
                include_bytes!("../../../src/id/toml.birnam").as_slice(),
            ),
            (
                b"projectPolicy",
                b"ProjectPolicy",
                include_bytes!("../../../src/id/project-policy.birnam").as_slice(),
            ),
        ] {
            if self.global(&q, global)? == self.named(b"nil")? {
                q.evaluate(source, Path::new("<project-support>"), Some(name))?;
                let manager = self.global(&q, b"Module")?;
                let name = unsafe { self.id.intern_name(name) }
                    .map_err(|e| native_error("project module", e))?;
                let result = q.call(manager, b"markLoaded:", &[Operand::object(name)])?;
                q.check_result(result)?;
            }
        }
        Ok(())
    }
    fn project_manifest(&self, root: &Path) -> Result<ProjectManifestObservation, HostError> {
        self.project_support()?;
        let q = self.scope();
        let source = q.string(&read_source_file(&root.join("birnam.toml"))?)?;
        let parsed = q.call(
            self.global(&q, b"Toml")?,
            b"parse:",
            &[Operand::object(source)],
        )?;
        q.check_result(parsed)?;
        values::decode_manifest(&q, parsed).map_err(project_error)
    }
    fn project_policy(&self, q: &Scope<'_>, name: &[u8], input: Oop) -> Result<Oop, HostError> {
        let result = q.call(
            self.global(q, b"ProjectPolicy")?,
            name,
            &[Operand::object(input)],
        )?;
        q.check_result(result)?;
        if result == self.named(b"nil")? {
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
        let q = self.scope();
        let mut vector = q.call(self.named(b"Vector")?, b"new", &[])?;
        for path in paths {
            let path = q.string(path.as_os_str().as_bytes())?;
            vector = q.call(vector, b"add:", &[Operand::object(path)])?;
        }
        let source = q.call(
            self.global(&q, b"Module")?,
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
    fn project_source_bytes(
        &self,
        source: Vec<u8>,
        path: &Path,
        root: &str,
    ) -> Result<Option<ProjectSourceObservation>, HostError> {
        Ok(self
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
            h: &HostRuntime,
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
    fn project_resolution(
        &self,
        snapshot: &ProjectSnapshot,
    ) -> Result<ProjectResolutionObservation, HostError> {
        let q = self.scope();
        let input = values::Builder { q: &q }.snapshot(snapshot)?;
        let result = self.project_policy(&q, b"resolveSnapshot:", input)?;
        values::decode_resolution(&q, result).map_err(project_error)
    }
    pub fn compile_project_artifact(
        &self,
        module: &ProjectSourceObservation,
        output: &Path,
    ) -> Result<String, HostError> {
        let q = self.scope();
        let source = q.string(&module.source)?;
        let output_value = q.string(output.as_os_str().as_bytes())?;
        let root = q.string(module.project_root.as_bytes())?;
        let cg = self.named(b"Codegen")?;
        let result = q.call(
            cg,
            b"compile:output:root:",
            &[
                Operand::object(source),
                Operand::object(output_value),
                Operand::object(root),
            ],
        )?;
        q.check(cg, result)?;
        if result == self.named(b"nil")? {
            return Err(HostError(format!("compiler rejected {}", module.name)));
        }
        let bytes = fs::read(output).map_err(|e| HostError(e.to_string()))?;
        validate_project_compiled_artifact(module, &bytes).map_err(project_error)?;
        Ok(String::from_utf8_lossy(&sha256_hex(&bytes)).into_owned())
    }
    pub fn check_project(&self, start: &Path) -> Result<ProjectCheckResult, HostError> {
        let snapshot = self.observe_project_snapshot(start)?;
        let resolution = self.project_resolution(&snapshot)?;
        let manifest = read_source_file(Path::new(&resolution.manifest))?;
        let resolution_digest = project_build_resolution_digest(
            &resolution.project_root,
            &manifest,
            &resolution.modules,
            &resolution.wrapped,
        );
        let temp = create_check_temp_directory().map_err(project_error)?;
        let mut diagnostics = Vec::new();
        for (i, module) in resolution.modules.iter().enumerate() {
            if let Err(e) =
                self.compile_project_artifact(module, &temp.0.join(format!("module-{i}.dylib")))
            {
                diagnostics.push(parse_project_compile_diagnostic(
                    &module.name,
                    &module.path,
                    &e.0,
                ))
            }
        }
        Ok(ProjectCheckResult {
            wrapped: resolution.wrapped.clone(),
            test_sources: snapshot
                .projects
                .iter()
                .find(|p| p.root == snapshot.root)
                .and_then(|p| p.test_sources.clone()),
            resolution,
            resolution_digest,
            diagnostics,
        })
    }
}
impl HostRuntime {
    pub fn plan_project_artifacts(
        &self,
        request: &ProjectArtifactPlanRequest<'_>,
    ) -> Result<ProjectArtifactPlan, HostError> {
        self.project_support()?;
        let q = self.scope();
        let input = values::Builder { q: &q }.artifact_plan_input(request)?;
        let result = self.project_policy(&q, b"artifactPlan:", input)?;
        let plan = values::decode_artifact_plan(&q, result).map_err(project_error)?;
        validate_project_artifact_plan(&plan, request).map_err(project_error)?;
        Ok(plan)
    }
    fn previous_project_artifacts(
        &self,
        root: &Path,
        resolution: &ProjectResolutionObservation,
        target: &str,
        profile: &str,
        version: &str,
    ) -> Option<ProjectPreviousPublication> {
        let mut artifacts = Vec::new();
        for m in &resolution.modules {
            let path = format!("lib/libbirnam_{}.dylib", m.name);
            let Ok(bytes) = fs::read(root.join(&path)) else {
                continue;
            };
            let Ok(d) = birnam_artifact::inspect(&bytes) else {
                continue;
            };
            if d.kind != b"module"
                || d.identity != m.name.as_bytes()
                || !d.capabilities.iter().any(|c| c == b"id-object-v1")
            {
                continue;
            }
            let checksum = String::from_utf8_lossy(&sha256_hex(&bytes)).into_owned();
            artifacts.push(ProjectPreviousPublishedArtifact {
                name: m.name.clone(),
                path,
                source: m.path.clone(),
                sources: m.sources.clone(),
                source_digest: String::from_utf8_lossy(d.source_digest).into_owned(),
                revision_digest: String::from_utf8_lossy(d.revision_digest).into_owned(),
                requires: d
                    .requires
                    .iter()
                    .map(|b| String::from_utf8_lossy(b).into_owned())
                    .collect(),
                checksum: checksum.clone(),
                observed_checksum: Some(checksum),
            });
        }
        Some(ProjectPreviousPublication {
            project_root: resolution.project_root.clone(),
            target: target.into(),
            profile: profile.into(),
            tool_version: version.into(),
            artifact_abi: String::from_utf8_lossy(birnam_artifact::ABI).into_owned(),
            artifacts,
        })
    }
    fn project_artifact_plan(
        &self,
        path: &Path,
    ) -> Result<Vec<(String, PathBuf, Vec<String>)>, HostError> {
        let q = self.scope();
        let manager = self.global(&q, b"Module")?;
        let directory = q.string(
            path.parent()
                .ok_or_else(|| HostError("artifact directory missing".into()))?
                .as_os_str()
                .as_bytes(),
        )?;
        let set = q.call(manager, b"libDir:", &[Operand::object(directory)])?;
        q.check_result(set)?;
        let path = q.string(path.as_os_str().as_bytes())?;
        let descriptor = q.call(
            self.named(b"Artifact")?,
            b"inspect:",
            &[Operand::object(path)],
        )?;
        if descriptor == self.named(b"nil")? {
            return Err(HostError(
                "publication artifact unavailable or incompatible".into(),
            ));
        }
        q.check_result(descriptor)?;
        let name = q.call(descriptor, b"identity", &[])?;
        let plan = q.call(
            manager,
            b"planFor:path:",
            &[Operand::object(name), Operand::object(path)],
        )?;
        q.check_result(plan)?;
        let mut entries = Vec::new();
        for i in 0..q.vector_size(plan)? {
            let d = q.vector_at(plan, i)?;
            let name = q.symbol_text(q.call(d, b"identity", &[])?)?;
            let path = PathBuf::from(q.text(q.call(d, b"path", &[])?)?);
            let requirements = q.call(d, b"nativeRequires", &[])?;
            let mut native = Vec::new();
            for j in 0..q.vector_size(requirements)? {
                native.push(q.text(q.vector_at(requirements, j)?)?)
            }
            entries.push((name, path, native));
        }
        Ok(entries)
    }
    fn preflight_project(&self, path: &Path) -> Result<(), HostError> {
        self.project_artifact_plan(path).map(|_| ())
    }
    pub fn build_project(
        &self,
        request: &ProjectBuildRequest<'_>,
    ) -> Result<ProjectBuildResult, HostError> {
        if request.target != "aarch64-macos" || !matches!(request.profile, "debug" | "release") {
            return Err(HostError("unsupported project target/profile".into()));
        }
        let snapshot = self.observe_project_snapshot(request.start)?;
        let resolution = self.project_resolution(&snapshot)?;
        let digest = project_build_resolution_digest(
            &resolution.project_root,
            &read_source_file(Path::new(&resolution.manifest))?,
            &resolution.modules,
            &resolution.wrapped,
        );
        let target = Path::new(&resolution.project_root)
            .join(".birnam/build")
            .join(request.target);
        let final_root = target.join(request.profile);
        let previous = self.previous_project_artifacts(
            &final_root,
            &resolution,
            request.target,
            request.profile,
            request.tool_version,
        );
        let plan = self.plan_project_artifacts(&ProjectArtifactPlanRequest {
            resolution: &resolution,
            previous: previous.as_ref(),
            target: request.target,
            profile: request.profile,
            tool_version: request.tool_version,
        })?;
        fs::create_dir_all(&target).map_err(project_error)?;
        let stage = (0..128)
            .map(|i| target.join(format!(".staging-{}-{i}", std::process::id())))
            .find(|p| !p.exists())
            .ok_or_else(|| HostError("no project staging directory available".into()))?;
        fs::create_dir(&stage).map_err(project_error)?;
        let staged = (|| {
            fs::create_dir(stage.join("lib")).map_err(project_error)?;
            let mut artifacts = Vec::new();
            for (module, item) in resolution.modules.iter().zip(&plan.artifacts) {
                let output = stage.join(
                    safe_artifact_relative_path(&item.path)
                        .ok_or_else(|| HostError("invalid plan path".into()))?,
                );
                let checksum = if item.action == "reuse" {
                    let bytes = fs::read(final_root.join(&item.path)).map_err(project_error)?;
                    validate_project_compiled_artifact(module, &bytes).map_err(project_error)?;
                    let checksum = String::from_utf8_lossy(&sha256_hex(&bytes)).into_owned();
                    if Some(&checksum) != item.checksum.as_ref() {
                        return Err(HostError("reused artifact changed during build".into()));
                    }
                    fs::write(&output, &bytes).map_err(project_error)?;
                    checksum
                } else {
                    self.compile_project_artifact(module, &output)?
                };
                artifacts.push(ProjectStagedArtifact {
                    name: module.name.clone(),
                    path: item.path.clone(),
                    action: item.action.clone(),
                    checksum,
                });
            }
            self.stage_project_wrapped(&resolution.wrapped, &stage)
                .map_err(project_error)?;
            let (module_name, module_source) = project_module_root_source(
                &resolution,
                &digest,
                request.target,
                request.profile,
                request.tool_version,
            );
            let (config_name, config_source) = project_configuration_source(
                &resolution,
                &digest,
                request.target,
                request.profile,
                request.tool_version,
            );
            let compile_generated = |name: &str,
                                     source: Vec<u8>,
                                     filename: &str|
             -> Result<(), HostError> {
                let label = Path::new(&resolution.project_root).join(format!("<generated-{name}>"));
                let module = self
                    .project_source_bytes(source, &label, &resolution.project_root)?
                    .ok_or_else(|| HostError(format!("generated source not indexable: {name}")))?;
                let output = stage.join("lib").join(filename);
                self.compile_project_artifact(&module, &output)?;
                let canonical = stage.join("lib").join(format!("libbirnam_{name}.dylib"));
                fs::hard_link(output, canonical).map_err(project_error)?;
                Ok(())
            };
            compile_generated(&module_name, module_source, PROJECT_MODULE_ROOT_FILENAME)?;
            compile_generated(&config_name, config_source, PROJECT_CONFIGURATION_FILENAME)?;
            let application = if let Some(main) = &resolution.main {
                let source = read_source_file(Path::new(main))?;
                let (name, source) = project_application_source(
                    &resolution,
                    &config_name,
                    &digest,
                    request.target,
                    request.profile,
                    request.tool_version,
                    &source,
                );
                compile_generated(&name, source, PROJECT_APPLICATION_FILENAME)?;
                Some(final_root.join("lib").join(PROJECT_APPLICATION_FILENAME))
            } else {
                None
            };
            self.preflight_project(&stage.join("lib").join(PROJECT_MODULE_ROOT_FILENAME))?;
            self.preflight_project(&stage.join("lib").join(PROJECT_CONFIGURATION_FILENAME))?;
            if application.is_some() {
                self.preflight_project(&stage.join("lib").join(PROJECT_APPLICATION_FILENAME))?
            }
            publish_project_build(&stage, &final_root, &target).map_err(project_error)?;
            Ok(ProjectBuildResult {
                output_root: final_root.clone(),
                module_root: final_root.join("lib").join(PROJECT_MODULE_ROOT_FILENAME),
                configuration: final_root.join("lib").join(PROJECT_CONFIGURATION_FILENAME),
                application,
                artifacts,
            })
        })();
        if staged.is_err() {
            let _ = fs::remove_dir_all(&stage);
        }
        staged
    }
    pub fn locate_project_build(
        &self,
        start: &Path,
        profile: &str,
    ) -> Result<ProjectBuildResult, HostError> {
        let (output_root, module_root, configuration, application) =
            project_publication_paths(start, "aarch64-macos", profile).map_err(project_error)?;
        Ok(ProjectBuildResult {
            output_root,
            module_root,
            configuration,
            application: application.is_file().then_some(application),
            artifacts: vec![],
        })
    }
    pub fn run_project(&self, options: &RunOptions, version: &str) -> Result<(), HostError> {
        let build = if options.build {
            self.build_project(&ProjectBuildRequest {
                start: &options.directory,
                target: "aarch64-macos",
                profile: &options.profile,
                tool_version: version,
            })?
        } else {
            self.locate_project_build(&options.directory, &options.profile)?
        };
        let application=build.application.as_ref().ok_or_else(||HostError("application publication unavailable; run `birnam build` or retry with `birnam run --build`".into()))?;
        self.preflight_project(application)?;
        self.load_artifact(&build.configuration)?;
        let q = self.scope();
        let published = self.global(&q, PROJECT_REPL_GLOBAL.as_bytes())?;
        let repl = options.repl.unwrap_or(match q.text(published)?.as_str() {
            "none" => ReplMode::None,
            "tty" => ReplMode::Tty,
            "socket" => ReplMode::Socket,
            _ => return Err(HostError("invalid published REPL mode".into())),
        });
        let mut args = q.call(self.named(b"Vector")?, b"new", &[])?;
        for arg in &options.arguments {
            let value = q.string(arg.as_os_str().as_bytes())?;
            args = q.call(args, b"add:", &[Operand::object(value)])?;
        }
        let key = unsafe { self.id.intern_name(b"ProjectArgs") }
            .map_err(|e| native_error("ProjectArgs", e))?;
        let result = q.call(
            self.named(b"Globals")?,
            b"at:put:",
            &[Operand::object(key), Operand::object(args)],
        )?;
        q.check_result(result)?;
        let directory = q.string(application.parent().unwrap().as_os_str().as_bytes())?;
        let configured = q.call(
            self.named(b"Alien")?,
            b"libraryDir:",
            &[Operand::object(directory)],
        )?;
        q.check_result(configured)?;
        self.load_artifact(application)?;
        match repl {
            ReplMode::None => Ok(()),
            ReplMode::Tty => self.run_repl(false),
            ReplMode::Socket => {
                let bind = match &options.socket_bind {
                    Some(bind) => bind.clone(),
                    None => q.text(self.global(&q, PROJECT_SOCKET_BIND_GLOBAL.as_bytes())?)?,
                };
                let port = match options.socket_port {
                    Some(port) => port,
                    None => {
                        let value = self.global(&q, PROJECT_SOCKET_PORT_GLOBAL.as_bytes())?;
                        q.exact(value, b"IntegerVt")?;
                        let port = self
                            .send(
                                self.named(b"Integer")?,
                                b"wordOf:",
                                &[Operand::object(value)],
                            )?
                            .as_word()
                            .ok_or_else(|| HostError("invalid published socket port".into()))?;
                        u16::try_from(port)
                            .map_err(|_| HostError("invalid published socket port".into()))?
                    }
                };
                self.serve_socket(port, &bind)
            }
        }
    }
}

impl HostRuntime {
    pub fn stage_project_wrapped(
        &self,
        wrapped: &[ProjectWrappedArtifactObservation],
        stage_root: &Path,
    ) -> Result<(), ProjectWrappedBuildError> {
        let lib_root = stage_root.join("lib");
        std::fs::create_dir_all(&lib_root).map_err(|error| {
            ProjectWrappedBuildError::CopyOutput {
                source: stage_root.to_path_buf(),
                destination: lib_root.clone(),
                kind: error.kind(),
            }
        })?;
        let native_root = stage_root.join("native");
        for item in wrapped {
            let project_root = Path::new(&item.project_root);
            let command_root = native_root.join(&item.name);
            std::fs::create_dir_all(&command_root).map_err(|error| {
                ProjectWrappedBuildError::CopyOutput {
                    source: native_root.clone(),
                    destination: command_root.clone(),
                    kind: error.kind(),
                }
            })?;
            let filename = format!("lib{}.dylib", item.name);
            let output = command_root.join(&filename);
            let staged_output = lib_root.join(&filename);
            let _ = std::fs::remove_file(&output);
            for command in &item.build {
                if command.is_empty() {
                    return Err(ProjectWrappedBuildError::EmptyCommand {
                        name: item.name.clone(),
                    });
                }
                let expanded = command
                    .iter()
                    .map(|argument| {
                        expand_project_wrapped_argument(
                            argument,
                            project_root,
                            &output,
                            stage_root,
                            &lib_root,
                        )
                    })
                    .collect::<Vec<_>>();
                let output_result = std::process::Command::new(&expanded[0])
                    .args(&expanded[1..])
                    .current_dir(project_root)
                    .output()
                    .map_err(|error| ProjectWrappedBuildError::Spawn {
                        name: item.name.clone(),
                        kind: error.kind(),
                    })?;
                if !output_result.status.success() {
                    let mut detail = String::from_utf8_lossy(&output_result.stdout).into_owned();
                    detail.push_str(&String::from_utf8_lossy(&output_result.stderr));
                    let mut end = detail.len().min(16 << 20);
                    while !detail.is_char_boundary(end) {
                        end -= 1;
                    }
                    detail.truncate(end);
                    return Err(ProjectWrappedBuildError::Failed {
                        name: item.name.clone(),
                        code: output_result.status.code(),
                        detail,
                    });
                }
            }
            if !output.is_file() {
                return Err(ProjectWrappedBuildError::MissingOutput {
                    name: item.name.clone(),
                    path: output,
                });
            }
            std::fs::copy(&output, &staged_output).map_err(|error| {
                ProjectWrappedBuildError::CopyOutput {
                    source: output.clone(),
                    destination: staged_output.clone(),
                    kind: error.kind(),
                }
            })?;
        }
        Ok(())
    }
}
impl HostRuntime {
    pub fn fetch_project_dependencies(
        &self,
        start: &Path,
    ) -> Result<ProjectFetchResult, HostError> {
        fn visit(
            h: &HostRuntime,
            root: &Path,
            visiting: &mut HashSet<PathBuf>,
            seen: &mut HashSet<PathBuf>,
            fetched: &mut usize,
            reused: &mut usize,
        ) -> Result<(), HostError> {
            let root = fs::canonicalize(root).map_err(project_error)?;
            if seen.contains(&root) {
                return Ok(());
            }
            if !visiting.insert(root.clone()) {
                return Err(HostError(format!(
                    "project dependency cycle: {}",
                    root.display()
                )));
            }
            for dep in h.project_manifest(&root)?.dependencies {
                let path = match &dep.source {
                    ProjectDependencySourceObservation::Path(p) => PathBuf::from(
                        &resolve_project_dependency_paths(
                            &root,
                            &[ProjectDependencyObservation {
                                name: dep.name.clone(),
                                root: p.clone(),
                            }],
                        )
                        .map_err(project_error)?[0]
                            .root,
                    ),
                    ProjectDependencySourceObservation::Git { .. } => {
                        let (path, acquired) =
                            acquire_project_git_dependency(&root, &dep).map_err(project_error)?;
                        if acquired {
                            *fetched += 1
                        } else {
                            *reused += 1
                        }
                        path
                    }
                };
                visit(h, &path, visiting, seen, fetched, reused)?;
            }
            visiting.remove(&root);
            seen.insert(root);
            Ok(())
        }
        let root = nearest_project_root(start).map_err(project_error)?;
        let (mut fetched, mut reused) = (0, 0);
        visit(
            self,
            &root,
            &mut HashSet::new(),
            &mut HashSet::new(),
            &mut fetched,
            &mut reused,
        )?;
        Ok(ProjectFetchResult {
            project_root: root,
            fetched,
            reused,
        })
    }
}
impl HostRuntime {
    pub fn install_project_build(
        &self,
        build: &ProjectBuildResult,
        destination: &Path,
    ) -> Result<usize, HostError> {
        let mut entries = self.project_artifact_plan(&build.module_root)?;
        if let Some(app) = &build.application {
            entries.extend(self.project_artifact_plan(app)?)
        }
        fs::create_dir_all(destination).map_err(project_error)?;
        let stage = (0..128)
            .map(|i| destination.join(format!(".birnam-install-{}-{i}", std::process::id())))
            .find(|p| !p.exists())
            .ok_or_else(|| HostError("install staging unavailable".into()))?;
        fs::create_dir(&stage).map_err(project_error)?;
        let result = (|| {
            let mut filenames = HashSet::new();
            for (identity, path, native) in entries {
                let mut copies = vec![(format!("libbirnam_{identity}.dylib"), path.clone())];
                for name in native {
                    copies.push((name.clone(), path.parent().unwrap().join(name)));
                }
                for (name, source) in copies {
                    if name.is_empty() || name.contains('/') || name.contains('\0') {
                        return Err(HostError("invalid installed artifact filename".into()));
                    }
                    if filenames.insert(name.clone()) {
                        let output = stage.join(&name);
                        fs::copy(source, &output).map_err(project_error)?;
                        fs::File::open(&output)
                            .map_err(project_error)?
                            .sync_all()
                            .map_err(project_error)?;
                    }
                }
            }
            for name in &filenames {
                fs::rename(stage.join(name), destination.join(name)).map_err(project_error)?;
            }
            Ok(filenames.len())
        })();
        let _ = fs::remove_dir_all(stage);
        result
    }
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
    pub fn run_project_test_source(
        &self,
        root: &Path,
        source: &Path,
        profile: &str,
    ) -> Result<(), HostError> {
        let build = self.locate_project_build(root, profile)?;
        let q = self.scope();
        let directory = q.string(build.module_root.parent().unwrap().as_os_str().as_bytes())?;
        let result = q.call(
            self.named(b"Alien")?,
            b"libraryDir:",
            &[Operand::object(directory)],
        )?;
        q.check_result(result)?;
        self.load_artifact(&build.module_root)?;
        q.evaluate(
            include_bytes!("../../../modules/test.birnam"),
            Path::new("<test-harness>"),
            None,
        )?;
        self.eval_script(source)?;
        let test = self.global(&q, b"Test")?;
        let runner = self.global(&q, b"ConsoleRunner")?;
        let suites = q.call(test, b"suites", &[])?;
        if q.vector_size(suites)? == 0 {
            return Err(HostError("test source registered no suites".into()));
        }
        let result = q.call(test, b"runAllSuitesWithRunner:", &[Operand::object(runner)])?;
        q.check_result(result)?;
        if q.call(result, b"allPassed", &[])? != self.named(b"true")? {
            return Err(HostError("project tests failed".into()));
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use birnam_abi::id_protocol::NativeEntry;
    use core::{cell::Cell, ffi::c_void};
    struct Hook {
        id: Protocol,
        gc: Oop,
        original: NativeEntry,
        calls: Cell<usize>,
    }
    unsafe extern "C" fn collect(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
        let h = unsafe { &*c.cast::<Hook>() };
        h.calls.set(h.calls.get() + 1);
        if let Err(e) = unsafe { h.id.send_named(h.gc, b"collectPrecisely", &[]) } {
            return e;
        }
        unsafe { h.original.method.unwrap()(h.original.context, r, a, n) }
    }
    unsafe extern "C" fn fail(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
        Outcome::failed(1, 919)
    }
    #[test]
    #[ignore = "requires installed production modules"]
    fn project_policy_values_survive_collecting_messages() {
        let lib = PathBuf::from(env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").unwrap());
        let resources = crate::resolve_from_exe_dir(
            Path::new("/unused/bin"),
            crate::Overrides {
                lib_dir: Some(&lib),
                ..Default::default()
            },
        )
        .unwrap();
        let host = HostRuntime::start(
            &resources,
            StartupPolicy {
                skip_stdlib: false,
                precise_gc: true,
            },
        )
        .unwrap();
        host.project_support().unwrap();
        let temp = create_check_temp_directory().unwrap();
        fs::create_dir(temp.0.join("src")).unwrap();
        fs::write(
            temp.0.join("birnam.toml"),
            b"name=\"collecting-project\"\nload=[\"example\"]\n[[module]]\nentry=\"src/example.birnam\"\nfiles=[\"src/part.birnam\"]\n\n[run]\nrepl=\"socket\"\nsocket_bind=\"localhost\"\nsocket_port=7777\n",
        )
        .unwrap();
        fs::write(
            temp.0.join("src/example.birnam"),
            b"[Module named: #example requires: #[]];",
        )
        .unwrap();
        fs::write(
            temp.0.join("src/part.birnam"),
            b"let neverPublished := 42; true",
        )
        .unwrap();
        let snapshot = host.observe_project_snapshot(&temp.0).unwrap();
        assert_eq!(snapshot.projects[0].sources[0].sources.len(), 2);
        let q = host.scope();
        let policy = host.global(&q, b"ProjectPolicy").unwrap();
        let behavior = q
            .call(host.id.receiver, b"behaviorOf:", &[Operand::object(policy)])
            .unwrap();
        let mut hooks = Vec::new();
        for selector in [b"resolveSnapshot:".as_slice(), b"artifactPlan:"] {
            let key = unsafe { host.id.intern_name(selector) }.unwrap();
            // Replacing the binding removes its normal Method root. Retain the
            // owned Method while the forwarding fixture holds its native entry.
            q.call(policy, b"methodFor:", &[Operand::object(key)])
                .unwrap();
            let original = host
                .send(
                    host.id.receiver,
                    b"bindingOf:in:",
                    &[Operand::object(key), Operand::object(behavior)],
                )
                .unwrap()
                .as_entry()
                .unwrap();
            let mut hook = Box::new(Hook {
                id: host.id,
                gc: host.named(b"GC").unwrap(),
                original,
                calls: Cell::new(0),
            });
            host.send(
                behavior,
                b"addMethod",
                &[
                    Operand::object(key),
                    Operand::entry(NativeEntry::new(collect, (&mut *hook as *mut Hook).cast())),
                ],
            )
            .unwrap();
            hooks.push((key, hook));
        }
        let resolution = host.project_resolution(&snapshot).unwrap();
        assert_eq!(resolution.modules[0].name, "example");
        assert_eq!(resolution.run.repl, "socket");
        assert_eq!(resolution.run.socket_bind, "localhost");
        assert_eq!(resolution.run.socket_port, 7777);
        let request = ProjectArtifactPlanRequest {
            resolution: &resolution,
            previous: None,
            target: "aarch64-macos",
            profile: "debug",
            tool_version: "test",
        };
        let plan = host.plan_project_artifacts(&request).unwrap();
        assert_eq!(plan.artifacts[0].action, "compile");
        let checked = host.check_project(&temp.0).unwrap();
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert!(!temp.0.join(".birnam").exists());
        assert_eq!(
            host.global(&q, b"neverPublished").unwrap(),
            host.named(b"nil").unwrap()
        );
        for (key, hook) in &hooks {
            assert!(hook.calls.get() > 0);
            host.send(
                behavior,
                b"addMethod",
                &[Operand::object(*key), Operand::entry(hook.original)],
            )
            .unwrap();
        }
        // A current policy's typed native failure must not become an empty plan.
        host.send(
            behavior,
            b"addMethod",
            &[
                Operand::object(hooks[0].0),
                Operand::entry(NativeEntry::new(fail, std::ptr::null_mut())),
            ],
        )
        .unwrap();
        assert!(
            host.project_resolution(&snapshot)
                .unwrap_err()
                .0
                .contains("detail=919")
        );
        host.send(
            behavior,
            b"addMethod",
            &[
                Operand::object(hooks[0].0),
                Operand::entry(hooks[0].1.original),
            ],
        )
        .unwrap();
    }
}
