//! Generated publication construction and build validation.
use birnam_artifact::digest::sha256_hex;
pub use birnam_project_support::*;
use std::{collections::HashSet, path::Path};
pub fn project_build_resolution_digest(
    project_root: &str,
    manifest_bytes: &[u8],
    modules: &[ProjectSourceObservation],
    wrapped: &[ProjectWrappedArtifactObservation],
) -> String {
    let source_digest = project_resolution_digest(project_root, manifest_bytes, modules);
    let mut input = Vec::new();
    let add = |input: &mut Vec<u8>, value: &str| {
        input.extend_from_slice(&(value.len() as u64).to_be_bytes());
        input.extend_from_slice(value.as_bytes());
    };
    let add_count = |input: &mut Vec<u8>, value: usize| {
        input.extend_from_slice(&(value as u64).to_be_bytes());
    };
    add(&mut input, "birnam.project-build-resolution.v3");
    add(&mut input, &source_digest);
    add_count(&mut input, wrapped.len());
    for artifact in wrapped {
        add(&mut input, &artifact.project_root);
        add(&mut input, &artifact.name);
        add(&mut input, &artifact.packaging);
        add_count(&mut input, artifact.build.len());
        for command in &artifact.build {
            add_count(&mut input, command.len());
            for argument in command {
                add(&mut input, argument);
            }
        }
    }
    String::from_utf8_lossy(&sha256_hex(&input)).into_owned()
}

pub(super) const PROJECT_MODULE_ROOT_FILENAME: &str = "project-modules.dylib";

pub(super) const PROJECT_CONFIGURATION_FILENAME: &str = "project-configuration.dylib";

pub(super) const PROJECT_APPLICATION_FILENAME: &str = "application.dylib";

pub(super) const PROJECT_REPL_GLOBAL: &str = "ProjectPublicationRepl";

pub(super) const PROJECT_SOCKET_BIND_GLOBAL: &str = "ProjectPublicationSocketBind";

pub(super) const PROJECT_SOCKET_PORT_GLOBAL: &str = "ProjectPublicationSocketPort";

fn project_publication_identity(project_name: &str, suffix: &str) -> String {
    let mut identity = String::from("__");
    for character in project_name.chars() {
        if character.is_ascii_alphanumeric() || character == '_' {
            identity.push(character);
        } else {
            identity.push('_');
        }
    }
    identity.push_str(suffix);
    identity
}

fn append_birnam_string(output: &mut String, value: &str) {
    output.push('"');
    for character in value.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            value => output.push(value),
        }
    }
    output.push('"');
}

fn source_without_module_declaration(source: &[u8]) -> Vec<u8> {
    let declaration = b"[Module named:";
    let mut cursor = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    let mut line_comment = false;
    let mut start = None;
    while cursor < source.len() {
        let byte = source[cursor];
        if line_comment {
            if byte == b'\n' {
                line_comment = false;
            }
            cursor += 1;
            continue;
        }
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
            cursor += 1;
            continue;
        }
        if byte == b'/' && source.get(cursor + 1) == Some(&b'/') {
            line_comment = true;
            cursor += 2;
            continue;
        }
        if byte == b'"' {
            quoted = true;
            cursor += 1;
            continue;
        }
        if source[cursor..].starts_with(declaration) {
            start = Some(cursor);
            break;
        }
        cursor += 1;
    }
    let Some(start) = start else {
        return source.to_vec();
    };
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    let mut line_comment = false;
    let mut finish = None;
    for (offset, byte) in source[start..].iter().copied().enumerate() {
        if line_comment {
            if byte == b'\n' {
                line_comment = false;
            }
            continue;
        }
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
            continue;
        }
        if byte == b'/' && source.get(start + offset + 1) == Some(&b'/') {
            line_comment = true;
            continue;
        }
        match byte {
            b'"' => quoted = true,
            b'[' => depth += 1,
            b']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let mut end = start + offset + 1;
                    while source
                        .get(end)
                        .is_some_and(|byte| byte.is_ascii_whitespace())
                    {
                        end += 1;
                    }
                    if source.get(end) == Some(&b';') {
                        end += 1;
                    }
                    finish = Some(end);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(finish) = finish else {
        return source.to_vec();
    };
    let mut result = Vec::with_capacity(source.len() - (finish - start));
    result.extend_from_slice(&source[..start]);
    result.extend_from_slice(&source[finish..]);
    result
}

pub(super) fn project_module_root_source(
    resolution: &ProjectResolutionObservation,
    resolution_digest: &str,
    target: &str,
    profile: &str,
    tool_version: &str,
) -> (String, Vec<u8>) {
    let identity = project_publication_identity(&resolution.project_name, "ProjectModules");
    let mut source = format!(
        "// project-publication {resolution_digest} {target} {profile} {tool_version}\n\
         [Module named: #{identity} requires: #["
    );
    let mut seen = HashSet::new();
    for module in &resolution.modules {
        if seen.insert(module.name.as_str()) {
            source.push('#');
            source.push_str(&module.name);
            source.push(' ');
        }
    }
    source.push_str("]];\n");
    for wrapped in &resolution.wrapped {
        source.push_str("[Alien dlopen: ");
        append_birnam_string(&mut source, &format!("lib{}.dylib", wrapped.name));
        source.push_str("];\n");
    }
    (identity, source.into_bytes())
}

pub(super) fn project_configuration_source(
    resolution: &ProjectResolutionObservation,
    resolution_digest: &str,
    target: &str,
    profile: &str,
    tool_version: &str,
) -> (String, Vec<u8>) {
    let identity = project_publication_identity(&resolution.project_name, "ProjectConfiguration");
    let mut source = format!(
        "// project-configuration {resolution_digest} {target} {profile} {tool_version}\n\
         [Module named: #{identity} requires: #[]];\n\
         [Globals at: #{PROJECT_REPL_GLOBAL} put: "
    );
    append_birnam_string(&mut source, &resolution.run.repl);
    source.push_str("];\n[Globals at: #");
    source.push_str(PROJECT_SOCKET_BIND_GLOBAL);
    source.push_str(" put: ");
    append_birnam_string(&mut source, &resolution.run.socket_bind);
    source.push_str("];\n[Globals at: #");
    source.push_str(PROJECT_SOCKET_PORT_GLOBAL);
    source.push_str(" put: ");
    source.push_str(&resolution.run.socket_port.to_string());
    source.push_str("];\n");
    (identity, source.into_bytes())
}

pub(super) fn project_application_source(
    resolution: &ProjectResolutionObservation,
    configuration_identity: &str,
    resolution_digest: &str,
    target: &str,
    profile: &str,
    tool_version: &str,
    main: &[u8],
) -> (String, Vec<u8>) {
    let identity = project_publication_identity(&resolution.project_name, "Application");
    let mut source = format!(
        "// project-application {resolution_digest} {target} {profile} {tool_version}\n\
         [Module named: #{identity} requires: #[#{configuration_identity}]];\n"
    )
    .into_bytes();
    let mut seen = HashSet::new();
    for wrapped in &resolution.wrapped {
        let mut declaration = String::from("[Alien dlopen: ");
        append_birnam_string(&mut declaration, &format!("lib{}.dylib", wrapped.name));
        declaration.push_str("];\n");
        source.extend_from_slice(declaration.as_bytes());
    }
    for module in &resolution.modules {
        if seen.insert(module.name.as_str()) {
            // Preserve each bundled module's declaration semantics while
            // keeping Codegen's one-declaration metadata scanner focused on
            // the application root.
            source.extend_from_slice(b"[[Globals at: #Module] named: #");
            source.extend_from_slice(module.name.as_bytes());
            source.extend_from_slice(b" requires: #[");
            for requirement in &module.requires {
                source.push(b'#');
                source.extend_from_slice(requirement.as_bytes());
                source.push(b' ');
            }
            source.extend_from_slice(b"]];\n");
            source.extend_from_slice(&source_without_module_declaration(&module.source));
            if !source.ends_with(b"\n") {
                source.push(b'\n');
            }
            source.extend_from_slice(b"[[Globals at: #Module] markLoaded: #");
            source.extend_from_slice(module.name.as_bytes());
            source.extend_from_slice(b"];\n");
        }
    }
    source.extend_from_slice(main);
    if !source.ends_with(b"\n") {
        source.push(b'\n');
    }
    (identity, source)
}

/// Validate a decoded artifact plan against the host-observed resolution and
/// the request identity. This is a fail-closed adapter check: Birnam policy
/// chooses compile versus reuse, while Rust rejects plans whose metadata or
/// paths could make a later effectful stage address a different artifact.
pub fn validate_project_artifact_plan(
    plan: &ProjectArtifactPlan,
    request: &ProjectArtifactPlanRequest<'_>,
) -> Result<(), ProjectArtifactPlanValidationError> {
    if plan.publication != "publish" {
        return Err(ProjectArtifactPlanValidationError::InvalidPublication(
            plan.publication.clone(),
        ));
    }
    if plan.artifacts.len() != request.resolution.modules.len() {
        return Err(ProjectArtifactPlanValidationError::ArtifactCount {
            expected: request.resolution.modules.len(),
            actual: plan.artifacts.len(),
        });
    }
    for (index, (artifact, module)) in plan
        .artifacts
        .iter()
        .zip(&request.resolution.modules)
        .enumerate()
    {
        let expected_path = format!("lib/libbirnam_{}.dylib", module.name);
        if artifact.name != module.name {
            return Err(ProjectArtifactPlanValidationError::ArtifactMismatch {
                index,
                field: "name",
            });
        }
        if artifact.path != expected_path {
            return Err(ProjectArtifactPlanValidationError::ArtifactMismatch {
                index,
                field: "path",
            });
        }
        if artifact.source != module.path {
            return Err(ProjectArtifactPlanValidationError::ArtifactMismatch {
                index,
                field: "source",
            });
        }
        if artifact.sources != module.sources {
            return Err(ProjectArtifactPlanValidationError::ArtifactMismatch {
                index,
                field: "sources",
            });
        }
        if artifact.source_digest != module.source_digest {
            return Err(ProjectArtifactPlanValidationError::ArtifactMismatch {
                index,
                field: "sourceDigest",
            });
        }
        if artifact.revision_digest != module.revision_digest {
            return Err(ProjectArtifactPlanValidationError::ArtifactMismatch {
                index,
                field: "revisionDigest",
            });
        }
        if artifact.requires != module.requires {
            return Err(ProjectArtifactPlanValidationError::ArtifactMismatch {
                index,
                field: "requires",
            });
        }
        match artifact.action.as_str() {
            "compile" if artifact.checksum.is_none() => {}
            "reuse" if artifact.checksum.is_some() => {}
            "compile" | "reuse" => {
                return Err(ProjectArtifactPlanValidationError::ChecksumMismatch {
                    index,
                    action: artifact.action.clone(),
                });
            }
            action => {
                return Err(ProjectArtifactPlanValidationError::InvalidAction {
                    index,
                    action: action.to_owned(),
                });
            }
        }
    }
    Ok(())
}

/// Validate one Codegen-produced module artifact before it can be copied into
/// a published build. The descriptor is inert bytes at this boundary; no
/// dynamic library is loaded and no module is activated.
pub fn validate_project_compiled_artifact(
    module: &ProjectSourceObservation,
    bytes: &[u8],
) -> Result<(), ProjectCompiledArtifactValidationError> {
    let descriptor = birnam_artifact::inspect(bytes)
        .map_err(ProjectCompiledArtifactValidationError::Descriptor)?;
    if descriptor.kind != b"module" {
        return Err(ProjectCompiledArtifactValidationError::KindMismatch);
    }
    if descriptor.identity != module.name.as_bytes() {
        return Err(ProjectCompiledArtifactValidationError::IdentityMismatch {
            expected: module.name.clone(),
            actual: descriptor.identity.to_vec(),
        });
    }
    if descriptor.source_digest != module.source_digest.as_bytes() {
        return Err(ProjectCompiledArtifactValidationError::SourceDigestMismatch);
    }
    if descriptor.revision_digest != module.revision_digest.as_bytes() {
        return Err(ProjectCompiledArtifactValidationError::RevisionDigestMismatch);
    }
    for (index, (expected, actual)) in module
        .requires
        .iter()
        .zip(descriptor.requires.iter())
        .enumerate()
    {
        if actual != expected.as_bytes() {
            return Err(ProjectCompiledArtifactValidationError::DependencyMismatch {
                index,
                actual: actual.to_vec(),
            });
        }
    }
    if descriptor.requires.count() != module.requires.len() {
        return Err(ProjectCompiledArtifactValidationError::DependencyMismatch {
            index: module.requires.len(),
            actual: Vec::new(),
        });
    }
    Ok(())
}

/// Expand the placeholder protocol used by wrapped build commands without
/// invoking a shell. Unknown braces are preserved as literal command text.
pub fn expand_project_wrapped_argument(
    input: &str,
    project_root: &Path,
    output: &Path,
    build_root: &Path,
    lib_root: &Path,
) -> String {
    let replacements = [
        ("{output}", output),
        ("{project}", project_root),
        ("{build}", build_root),
        ("{lib}", lib_root),
    ];
    let mut result = String::with_capacity(input.len());
    let mut rest = input;
    while !rest.is_empty() {
        let Some((index, token, path)) = replacements
            .iter()
            .filter_map(|(token, path)| rest.find(token).map(|index| (index, *token, *path)))
            .min_by_key(|(index, _, _)| *index)
        else {
            result.push_str(rest);
            break;
        };
        result.push_str(&rest[..index]);
        result.push_str(&path.to_string_lossy());
        rest = &rest[index + token.len()..];
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_source_removes_only_the_module_declaration() {
        let source = b"// prefix mentioning [Module named: #fake with an unmatched [\n[Module named: #demo requires: #[#one #two]];\n[Reflect defineMethod: #value on: Demo as: [:self | \"[not syntax]\"]]\n";
        assert_eq!(
            source_without_module_declaration(source),
            b"// prefix mentioning [Module named: #fake with an unmatched [\n\n[Reflect defineMethod: #value on: Demo as: [:self | \"[not syntax]\"]]\n"
        );
    }

    #[test]
    fn application_publication_bundles_modules_dependency_first_and_marks_them_loaded() {
        let module = |name: &str, requires: Vec<String>, source: &[u8]| ProjectSourceObservation {
            name: name.to_owned(),
            path: format!("/tmp/project/src/{name}.birnam"),
            sources: vec![format!("/tmp/project/src/{name}.birnam")],
            project_root: "/tmp/project".to_owned(),
            source: source.to_vec(),
            requires,
            source_digest: format!("{name}-source"),
            revision_digest: format!("{name}-revision"),
        };
        let resolution = ProjectResolutionObservation {
            project_root: "/tmp/project".to_owned(),
            manifest: "/tmp/project/birnam.toml".to_owned(),
            project_name: "demo".to_owned(),
            main: Some("main.birnam".to_owned()),
            run: ProjectRunObservation {
                repl: "none".to_owned(),
                socket_bind: "127.0.0.1".to_owned(),
                socket_port: 0,
            },
            load_roots: vec!["app".to_owned()],
            modules: vec![
                module(
                    "dep",
                    Vec::new(),
                    b"[Module named: #dep requires: #[]];\nDep := Object clone\n",
                ),
                module(
                    "app",
                    vec!["dep".to_owned()],
                    b"[Module named: #app requires: #[#dep]];\nApp := Object clone\n",
                ),
            ],
            wrapped: Vec::new(),
        };
        let (_, source) = project_application_source(
            &resolution,
            "__demoProjectConfiguration",
            "digest",
            "target",
            "debug",
            "dev",
            b"[App value]\n",
        );
        let source = String::from_utf8(source).expect("generated source is UTF-8");
        let dep = source.find("Dep :=").expect("dependency body is bundled");
        let app = source.find("App :=").expect("application body is bundled");
        let main = source.find("[App value]").expect("main is bundled");
        assert!(dep < app && app < main);
        assert!(source.contains("[[Globals at: #Module] named: #dep requires: #[]]"));
        assert!(source.contains("[[Globals at: #Module] markLoaded: #dep]"));
        assert!(source.contains("[[Globals at: #Module] named: #app requires: #[#dep ]]"));
        assert!(source.contains("[[Globals at: #Module] markLoaded: #app]"));
    }

    #[test]
    fn artifact_plan_validation_preserves_policy_identity_and_action_pairing() {
        let module = ProjectSourceObservation {
            name: "app".to_owned(),
            path: "/tmp/project/src/app.birnam".to_owned(),
            sources: vec!["/tmp/project/src/app.birnam".to_owned()],
            project_root: "/tmp/project".to_owned(),
            source: b"App := Object clone".to_vec(),
            requires: vec!["core".to_owned()],
            source_digest: "source".to_owned(),
            revision_digest: "revision".to_owned(),
        };
        let resolution = ProjectResolutionObservation {
            project_root: "/tmp/project".to_owned(),
            manifest: "/tmp/project/birnam.toml".to_owned(),
            project_name: "demo".to_owned(),
            main: None,
            run: ProjectRunObservation {
                repl: "none".to_owned(),
                socket_bind: "127.0.0.1".to_owned(),
                socket_port: 4400,
            },
            load_roots: vec!["/tmp/project/src".to_owned()],
            modules: vec![module.clone()],
            wrapped: Vec::new(),
        };
        let request = ProjectArtifactPlanRequest {
            resolution: &resolution,
            previous: None,
            target: "aarch64-macos",
            profile: "debug",
            tool_version: "dev",
        };
        let mut plan = ProjectArtifactPlan {
            artifacts: vec![ProjectArtifactPlanEntry {
                name: "app".to_owned(),
                path: "lib/libbirnam_app.dylib".to_owned(),
                action: "compile".to_owned(),
                source: module.path.clone(),
                sources: module.sources.clone(),
                source_digest: module.source_digest.clone(),
                revision_digest: module.revision_digest.clone(),
                requires: module.requires.clone(),
                checksum: None,
            }],
            publication: "publish".to_owned(),
        };
        validate_project_artifact_plan(&plan, &request).expect("policy plan validates");

        plan.artifacts[0].action = "reuse".to_owned();
        assert!(matches!(
            validate_project_artifact_plan(&plan, &request),
            Err(ProjectArtifactPlanValidationError::ChecksumMismatch {
                index: 0,
                action
            }) if action == "reuse"
        ));
        plan.artifacts[0].action = "compile".to_owned();
        plan.artifacts[0].path = "../escape.dylib".to_owned();
        assert!(matches!(
            validate_project_artifact_plan(&plan, &request),
            Err(ProjectArtifactPlanValidationError::ArtifactMismatch {
                index: 0,
                field: "path"
            })
        ));
        plan.artifacts[0].path = "/outside/libbirnam_app.dylib".to_owned();
        assert!(matches!(
            validate_project_artifact_plan(&plan, &request),
            Err(ProjectArtifactPlanValidationError::ArtifactMismatch {
                index: 0,
                field: "path"
            })
        ));
    }

    #[test]
    fn wrapped_argument_expansion_replaces_all_frozen_placeholders() {
        let expanded = expand_project_wrapped_argument(
            "cc -o {output} -I{project} -L{lib} {build}/obj.o {unknown}",
            Path::new("/tmp/project"),
            Path::new("/tmp/build/native/libdemo.dylib"),
            Path::new("/tmp/build"),
            Path::new("/tmp/build/lib"),
        );
        assert_eq!(
            expanded,
            "cc -o /tmp/build/native/libdemo.dylib -I/tmp/project -L/tmp/build/lib /tmp/build/obj.o {unknown}"
        );
    }

    #[test]
    fn project_build_resolution_digest_tracks_transitive_wrapped_provenance() {
        let wrapped = |root: &str, argument: &str| ProjectWrappedArtifactObservation {
            project_root: root.to_owned(),
            name: "native".to_owned(),
            packaging: "adjacent".into(),
            build: vec![vec![
                "cc".to_owned(),
                argument.to_owned(),
                "-o".to_owned(),
                "{output}".to_owned(),
            ]],
        };
        let first = project_build_resolution_digest(
            "/tmp/project",
            b"name = \"project\"\n",
            &[],
            &[wrapped("/tmp/dependency", "native/one.c")],
        );
        assert_eq!(
            first,
            project_build_resolution_digest(
                "/tmp/project",
                b"name = \"project\"\n",
                &[],
                &[wrapped("/tmp/dependency", "native/one.c")],
            )
        );
        assert_ne!(
            first,
            project_build_resolution_digest(
                "/tmp/project",
                b"name = \"project\"\n",
                &[],
                &[wrapped("/tmp/dependency", "native/two.c")],
            )
        );
        assert_ne!(
            first,
            project_build_resolution_digest(
                "/tmp/project",
                b"name = \"project\"\n",
                &[],
                &[wrapped("/tmp/other-dependency", "native/one.c")],
            )
        );
    }
}
