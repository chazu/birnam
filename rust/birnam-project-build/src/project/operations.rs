//! Build orchestration consumes owned observations through Id.
use super::{Project, data::*, files::*, values};
#[cfg(test)]
use crate::{StartupPolicy, runtime::HostRuntime};
use crate::{
    program::read_source_file,
    runtime::{HostError, Scope, native_error},
};
use birnam_abi::{Oop};
use birnam_artifact::digest::sha256_hex;
#[cfg(test)]
use std::env;
use std::{
    collections::HashSet,
    fs,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};
fn project_error(e: impl std::fmt::Debug) -> HostError {
    HostError(format!("{e:?}"))
}
impl Project<'_> {
    fn project_policy(&self, q: &Scope<'_>, name: &[u8], input: Oop) -> Result<Oop, HostError> {
        let result = q.call(
            self.host.global(q, b"ProjectPolicy")?,
            name,
            &[input],
        )?;
        q.check_result(result)?;
        if result == self.host.named(b"nil")? {
            return Err(HostError("project policy rejected the observation".into()));
        }
        Ok(result)
    }
    pub fn compile_project_artifact(
        &self,
        module: &ProjectSourceObservation,
        output: &Path,
    ) -> Result<String, HostError> {
        self.compile_project_artifact_with_libraries(module, output, &[])
    }
    fn compile_project_artifact_with_libraries(
        &self, module: &ProjectSourceObservation, output: &Path,
        embedded: &[(String, Vec<u8>)],
    ) -> Result<String, HostError> {
        let q = self.host.scope();
        let source = q.string(&module.source)?;
        let output_value = q.string(output.as_os_str().as_bytes())?;
        let root = q.string(module.project_root.as_bytes())?;
        let cg = self.host.named(b"Codegen")?;
        let mut arguments = vec![source, output_value, root];
        let selector: &[u8] = if embedded.is_empty() {
            // Preserve the ordinary compiler protocol for check/adjacent builds.
            b"compile:output:root:"
        } else {
            let vector = self.host.named(b"Vector")?;
            let mut libraries = q.call(vector, b"new", &[])?;
            for (name, bytes) in embedded {
                let mut row = q.call(vector, b"new", &[])?;
                let name = q.string(name.as_bytes())?;
                row = q.call(row, b"add:", &[name])?;
                let content = q.string(bytes)?;
                row = q.call(row, b"add:", &[content])?;
                libraries = q.call(libraries, b"add:", &[row])?;
            }
            arguments.push(libraries);
            b"compile:output:root:embeddedLibraries:"
        };
        let result = q.call(cg, selector, &arguments)?;
        q.check(cg, result)?;
        if result == self.host.named(b"nil")? {
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
impl Project<'_> {
    pub fn plan_project_artifacts(
        &self,
        request: &ProjectArtifactPlanRequest<'_>,
    ) -> Result<ProjectArtifactPlan, HostError> {
        self.project_support()?;
        let q = self.host.scope();
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
                || !d.capabilities.iter().any(|c| c == b"id-object-v3")
                || !d.capabilities.iter().any(|c| c == b"prepared-alien-v1")
                || d.capabilities.iter().any(|c| c == b"embedded-native-hex-v1")
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
        let q = self.host.scope();
        let manager = self.host.global(&q, b"Module")?;
        let directory = q.string(
            path.parent()
                .ok_or_else(|| HostError("artifact directory missing".into()))?
                .as_os_str()
                .as_bytes(),
        )?;
        let set = q.call(manager, b"libDir:", &[directory])?;
        q.check_result(set)?;
        let path = q.string(path.as_os_str().as_bytes())?;
        let descriptor = q.call(
            self.host.named(b"Artifact")?,
            b"inspect:",
            &[path],
        )?;
        if descriptor == self.host.named(b"nil")? {
            return Err(HostError(
                "publication artifact unavailable or incompatible".into(),
            ));
        }
        q.check_result(descriptor)?;
        let name = q.call(descriptor, b"identity", &[])?;
        let plan = q.call(
            manager,
            b"planFor:path:",
            &[name, path],
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
            // Explicit native builds can change payload bytes independently of
            // Birnam source. Embedded publications recompile rather than reuse
            // an artifact carrying stale native code.
            previous: if resolution.wrapped.iter().any(|w| w.packaging == "embedded") { None } else { previous.as_ref() },
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
            self.stage_project_wrapped(&resolution.wrapped, &stage).map_err(project_error)?;
            let embedded = resolution.wrapped.iter().filter(|w| w.packaging == "embedded")
                .map(|w| { let name = format!("lib{}.dylib", w.name); fs::read(stage.join("lib").join(&name)).map(|bytes| (name, bytes)) })
                .collect::<Result<Vec<_>, _>>().map_err(project_error)?;
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
                    self.compile_project_artifact_with_libraries(module, &output, &embedded)?
                };
                artifacts.push(ProjectStagedArtifact {
                    name: module.name.clone(),
                    path: item.path.clone(),
                    action: item.action.clone(),
                    checksum,
                });
            }
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
                self.compile_project_artifact_with_libraries(&module, &output, &embedded)?;
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
            // No embedded native sibling is published or installed. The build
            // scratch is also removed before validating the actual publication.
            for (name, _) in &embedded { fs::remove_file(stage.join("lib").join(name)).map_err(project_error)?; }
            if stage.join("native").exists() { fs::remove_dir_all(stage.join("native")).map_err(project_error)?; }
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
            project_publication_paths(&self.project_root(start)?, "aarch64-macos", profile)
                .map_err(project_error)?;
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
        self.host.load_artifact(&build.configuration)?;
        let q = self.host.scope();
        let published = self.host.global(&q, PROJECT_REPL_GLOBAL.as_bytes())?;
        let repl = options.repl.unwrap_or(match q.text(published)?.as_str() {
            "none" => ReplMode::None,
            "tty" => ReplMode::Tty,
            "socket" => ReplMode::Socket,
            _ => return Err(HostError("invalid published REPL mode".into())),
        });
        let mut args = q.call(self.host.named(b"Vector")?, b"new", &[])?;
        for arg in &options.arguments {
            let value = q.string(arg.as_os_str().as_bytes())?;
            args = q.call(args, b"add:", &[value])?;
        }
        let key = self.host.intern(b"ProjectArgs")
            .map_err(|e| native_error("ProjectArgs", e))?;
        let result = q.call(
            self.host.named(b"Globals")?,
            b"at:put:",
            &[key, args],
        )?;
        q.check_result(result)?;
        let directory = q.string(application.parent().unwrap().as_os_str().as_bytes())?;
        let configured = q.call(
            self.host.named(b"Alien")?,
            b"libraryDir:",
            &[directory],
        )?;
        q.check_result(configured)?;
        self.host.load_artifact(application)?;
        match repl {
            ReplMode::None => Ok(()),
            ReplMode::Tty => self.host.run_repl(false),
            ReplMode::Socket => {
                let bind = match &options.socket_bind {
                    Some(bind) => bind.clone(),
                    None => q.text(
                        self.host
                            .global(&q, PROJECT_SOCKET_BIND_GLOBAL.as_bytes())?,
                    )?,
                };
                let port = match options.socket_port {
                    Some(port) => port,
                    None => {
                        let value = self
                            .host
                            .global(&q, PROJECT_SOCKET_PORT_GLOBAL.as_bytes())?;
                        q.exact(value, b"IntegerVt")?;
                        let port=q.word_bits(q.call(self.host.named(b"Integer")?,b"wordOf:",&[value])?)?;
                        u16::try_from(port)
                            .map_err(|_| HostError("invalid published socket port".into()))?
                    }
                };
                self.host.serve_socket(port, &bind)
            }
        }
    }
}

impl Project<'_> {
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
impl Project<'_> {
    pub fn fetch_project_dependencies(
        &self,
        start: &Path,
    ) -> Result<ProjectFetchResult, HostError> {
        fn visit(
            h: &Project<'_>,
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
                        &h.resolve_dependency_paths(
                            &root,
                            &[ProjectDependencyObservation {
                                name: dep.name.clone(),
                                root: p.clone(),
                            }],
                        )?[0]
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
        let root = self.project_root(start)?;
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
impl Project<'_> {
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
    pub fn run_project_test_source(
        &self,
        root: &Path,
        source: &Path,
        profile: &str,
    ) -> Result<(), HostError> {
        let build = self.locate_project_build(root, profile)?;
        let q = self.host.scope();
        let directory = q.string(build.module_root.parent().unwrap().as_os_str().as_bytes())?;
        let result = q.call(
            self.host.named(b"Alien")?,
            b"libraryDir:",
            &[directory],
        )?;
        q.check_result(result)?;
        self.host.load_artifact(&build.module_root)?;
        q.evaluate(
            include_bytes!("../../../../modules/test.birnam"),
            Path::new("<test-harness>"),
            None,
        )?;
        self.host.eval_script(source)?;
        let test = self.host.global(&q, b"Test")?;
        let runner = self.host.global(&q, b"ConsoleRunner")?;
        let suites = q.call(test, b"suites", &[])?;
        if q.vector_size(suites)? == 0 {
            return Err(HostError("test source registered no suites".into()));
        }
        let result = q.call(test, b"runAllSuitesWithRunner:", &[runner])?;
        q.check_result(result)?;
        if q.call(result, b"allPassed", &[])? != self.host.named(b"true")? {
            return Err(HostError("project tests failed".into()));
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
                source_bootstrap: env::var_os("BIRNAM_TEST_SOURCE_BOOTSTRAP").is_some(),
                tool_bootstrap: false,
            },
        )
        .unwrap();
        host.project().project_support().unwrap();
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
        let snapshot = host.project().observe_project_snapshot(&temp.0).unwrap();
        assert_eq!(snapshot.projects[0].sources[0].sources.len(), 2);
        let q = host.scope();
        let policy = host.global(&q, b"ProjectPolicy").unwrap();
        let behavior = q.call(host.id.root, b"behaviorOf:", &[policy]).unwrap();
        let fixture = temp.0.join("project-policy.dylib");
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/oop-host/project-policy.c");
        assert!(
            std::process::Command::new("clang")
                .args(["-dynamiclib", "-O2"])
                .arg(source)
                .arg("-o")
                .arg(&fixture)
                .status()
                .unwrap()
                .success()
        );
        let module = host.load_native(&q, &fixture, &[]).unwrap();
        let forward_name = q.keep(host.intern(b"collectingForward").unwrap()).unwrap();
        let forward = q.call(module, b"methodAt:", &[forward_name]).unwrap();
        let mut hooks = Vec::new();
        for selector in [b"resolveSnapshot:".as_slice(), b"artifactPlan:"] {
            let key = q.keep(host.intern(selector).unwrap()).unwrap();
            let original = q.call(policy, b"methodFor:", &[key]).unwrap();
            let context = q
                .call(
                    host.id.root,
                    b"sequence",
                    &[
                        original,
                        host.named(b"GC").unwrap(),
                        birnam_abi::size(0).unwrap(),
                    ],
                )
                .unwrap();
            let method = q.call(forward, b"withContext:", &[context]).unwrap();
            q.call(behavior, b"addMethod", &[key, method]).unwrap();
            hooks.push((key, original, context));
        }
        let resolution = host.project().project_resolution(&snapshot).unwrap();
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
        let plan = host.project().plan_project_artifacts(&request).unwrap();
        assert_eq!(plan.artifacts[0].action, "compile");
        let checked = host.project().check_project(&temp.0).unwrap();
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert!(!temp.0.join(".birnam").exists());
        assert_eq!(
            host.global(&q, b"neverPublished").unwrap(),
            host.named(b"nil").unwrap()
        );
        for &(key, original, context) in &hooks {
            let calls = q
                .call(context, b"at:", &[birnam_abi::size(2).unwrap()])
                .unwrap();
            assert!(birnam_abi::as_size(calls).unwrap() > 0);
            q.call(behavior, b"addMethod", &[key, original]).unwrap();
        }
        // A current policy's native failure must not become an empty plan.
        // Compare the propagated Fault identity without reading its payload.
        let kind = q
            .keep(host.intern(b"projectPolicyFailure").unwrap())
            .unwrap();
        let constructor = q.keep(host.intern(b"fault:detail:").unwrap()).unwrap();
        let failure = unsafe {
            host.id.send(
                host.id.root,
                constructor,
                &[kind, birnam_abi::size(919).unwrap()],
            )
        }
        .unwrap_err();
        let failure = q.keep(failure).unwrap();
        let name = q.keep(host.intern(b"fail").unwrap()).unwrap();
        let fail = q.call(module, b"methodAt:", &[name]).unwrap();
        let fail = q.call(fail, b"withContext:", &[failure]).unwrap();
        q.call(behavior, b"addMethod", &[hooks[0].0, fail]).unwrap();
        assert_eq!(
            host.project().project_resolution(&snapshot).unwrap_err(),
            native_error("resolveSnapshot:", failure)
        );
        q.call(behavior, b"addMethod", &[hooks[0].0, hooks[0].1])
            .unwrap();
    }
}
