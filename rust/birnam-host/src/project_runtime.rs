//! Live project policy, decoding, orchestration and native bridge callbacks.
//! HostRuntime construction, address identity and Drop stay with the coordinator.
use super::*;

/// Decode the frozen Codegen `#moduleMetadata` vector without evaluating any
/// source. The vector shape is `[tag, name, requires, sourceDigest,
/// revisionDigest]`; symbols and strings are copied before the next send.
///
/// # Safety
///
/// `api` and `metadata` must belong to the same live runtime, and all Oops must
/// remain valid for the duration of this synchronous decode.
pub unsafe fn decode_project_source_metadata(
    api: &IdApi,
    metadata: birnam_abi::Oop,
) -> Result<ProjectSourceMetadata, ProjectMetadataError> {
    let vector_vt = unsafe { (api.import)(c"VectorVt".as_ptr()) };
    if metadata == api.nilObj
        || vector_vt == api.nilObj
        || unsafe { (api.vtableOf)(metadata) } != vector_vt
    {
        return Err(ProjectMetadataError::NotVector);
    }
    let size_selector = unsafe { (api.intern)(c"size".as_ptr()) };
    let size = unsafe { (api.send)(metadata, size_selector, std::ptr::null(), 0) };
    if unsafe { (birnam_abi::integer::is_integer)(size) } == 0 || unsafe { (birnam_abi::integer::value)(size) } != 5 {
        return Err(ProjectMetadataError::WrongSize);
    }
    let at_selector = unsafe { (api.intern)(c"at:".as_ptr()) };
    let item = |index: i64| unsafe {
        (api.send)(metadata, at_selector, [(birnam_abi::integer::make)(index)].as_ptr(), 1)
    };
    let tag = item(0);
    if tag == api.nilObj
        || unsafe { (api.vtableOf)(tag) } != api.symbol_vt
        || unsafe { (api.symbolName)(tag) }.is_null()
        || unsafe { CStr::from_ptr((api.symbolName)(tag)) }.to_bytes() != b"moduleMetadata"
    {
        return Err(ProjectMetadataError::WrongTag);
    }
    let symbol = |value: birnam_abi::Oop| -> Result<String, ProjectMetadataError> {
        if value == api.nilObj || unsafe { (api.vtableOf)(value) } != api.symbol_vt {
            return Err(ProjectMetadataError::InvalidSymbol);
        }
        let name = unsafe { (api.symbolName)(value) };
        if name.is_null() {
            return Err(ProjectMetadataError::InvalidSymbol);
        }
        Ok(unsafe { CStr::from_ptr(name) }
            .to_string_lossy()
            .into_owned())
    };
    let string = |value: birnam_abi::Oop| -> Result<String, ProjectMetadataError> {
        if value == api.nilObj || unsafe { (api.vtableOf)(value) } != unsafe { birnam_abi::string::behavior() } {
            return Err(ProjectMetadataError::InvalidString);
        }
        let mut length = 0usize;
        let bytes = unsafe { (birnam_abi::string::bytes)(value, &mut length) };
        if bytes.is_null() {
            return Err(ProjectMetadataError::InvalidString);
        }
        Ok(
            String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(bytes, length) })
                .into_owned(),
        )
    };
    let requires_value = item(2);
    if requires_value == api.nilObj || unsafe { (api.vtableOf)(requires_value) } != vector_vt {
        return Err(ProjectMetadataError::InvalidRequires);
    }
    let requires_size = unsafe { (api.send)(requires_value, size_selector, std::ptr::null(), 0) };
    if unsafe { (birnam_abi::integer::is_integer)(requires_size) } == 0
        || unsafe { (birnam_abi::integer::value)(requires_size) } < 0
    {
        return Err(ProjectMetadataError::InvalidRequires);
    }
    let mut requires = Vec::with_capacity(unsafe { (birnam_abi::integer::value)(requires_size) as usize });
    for index in 0..unsafe { (birnam_abi::integer::value)(requires_size) } {
        let value = unsafe {
            (api.send)(
                requires_value,
                at_selector,
                [(birnam_abi::integer::make)(index)].as_ptr(),
                1,
            )
        };
        requires.push(symbol(value).map_err(|_| ProjectMetadataError::InvalidRequires)?);
    }
    Ok(ProjectSourceMetadata {
        name: symbol(item(1))?,
        requires,
        source_digest: string(item(3))?,
        revision_digest: string(item(4))?,
    })
}

struct ProjectManifestDecoder<'a> {
    api: &'a IdApi,
    map_vt: birnam_abi::Oop,
    vector_vt: birnam_abi::Oop,
}

impl<'a> ProjectManifestDecoder<'a> {
    unsafe fn new(api: &'a IdApi) -> Result<Self, ProjectManifestDecodeError> {
        let map_vt = unsafe { (api.import)(c"MapVt".as_ptr()) };
        let vector_vt = unsafe { (api.import)(c"VectorVt".as_ptr()) };
        if map_vt == api.nilObj {
            return Err(ProjectManifestDecodeError::NotMap);
        }
        if vector_vt == api.nilObj {
            return Err(ProjectManifestDecodeError::InvalidList("manifest vector"));
        }
        Ok(Self {
            api,
            map_vt,
            vector_vt,
        })
    }

    unsafe fn at(
        &self,
        map: birnam_abi::Oop,
        key: &str,
    ) -> Result<birnam_abi::Oop, ProjectManifestDecodeError> {
        if map == self.api.nilObj || unsafe { (self.api.vtableOf)(map) } != self.map_vt {
            return Err(ProjectManifestDecodeError::InvalidMap("manifest map"));
        }
        let key_value = unsafe { (birnam_abi::string::make)(key.as_bytes().as_ptr(), key.len()) };
        let selector = unsafe { (self.api.intern)(c"at:".as_ptr()) };
        Ok(unsafe { (self.api.send)(map, selector, [key_value].as_ptr(), 1) })
    }

    unsafe fn string(
        &self,
        value: birnam_abi::Oop,
        field: &'static str,
    ) -> Result<String, ProjectManifestDecodeError> {
        if value == self.api.nilObj || unsafe { (self.api.vtableOf)(value) } != unsafe { birnam_abi::string::behavior() } {
            return Err(ProjectManifestDecodeError::InvalidString(field));
        }
        let mut length = 0usize;
        let bytes = unsafe { (birnam_abi::string::bytes)(value, &mut length) };
        if bytes.is_null() {
            return Err(ProjectManifestDecodeError::InvalidString(field));
        }
        Ok(
            String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(bytes, length) })
                .into_owned(),
        )
    }

    unsafe fn bytes(
        &self,
        value: birnam_abi::Oop,
        field: &'static str,
    ) -> Result<Vec<u8>, ProjectManifestDecodeError> {
        if value == self.api.nilObj || unsafe { (self.api.vtableOf)(value) } != unsafe { birnam_abi::string::behavior() } {
            return Err(ProjectManifestDecodeError::InvalidString(field));
        }
        let mut length = 0usize;
        let bytes = unsafe { (birnam_abi::string::bytes)(value, &mut length) };
        if bytes.is_null() {
            return Err(ProjectManifestDecodeError::InvalidString(field));
        }
        Ok(unsafe { std::slice::from_raw_parts(bytes, length) }.to_vec())
    }

    unsafe fn strings(
        &self,
        value: birnam_abi::Oop,
        field: &'static str,
        allow_single: bool,
    ) -> Result<Vec<String>, ProjectManifestDecodeError> {
        if value == self.api.nilObj {
            return Ok(Vec::new());
        }
        if allow_single && unsafe { (self.api.vtableOf)(value) } == unsafe { birnam_abi::string::behavior() } {
            return Ok(vec![unsafe { self.string(value, field)? }]);
        }
        if unsafe { (self.api.vtableOf)(value) } != self.vector_vt {
            return Err(ProjectManifestDecodeError::InvalidList(field));
        }
        let size_selector = unsafe { (self.api.intern)(c"size".as_ptr()) };
        let size = unsafe { (self.api.send)(value, size_selector, std::ptr::null(), 0) };
        if unsafe { (birnam_abi::integer::is_integer)(size) } == 0
            || unsafe { (birnam_abi::integer::value)(size) } < 0
        {
            return Err(ProjectManifestDecodeError::InvalidList(field));
        }
        let at_selector = unsafe { (self.api.intern)(c"at:".as_ptr()) };
        let mut result = Vec::with_capacity(unsafe { (birnam_abi::integer::value)(size) as usize });
        for index in 0..unsafe { (birnam_abi::integer::value)(size) } {
            let item = unsafe {
                (self.api.send)(
                    value,
                    at_selector,
                    [(birnam_abi::integer::make)(index)].as_ptr(),
                    1,
                )
            };
            result.push(unsafe { self.string(item, field)? });
        }
        Ok(result)
    }

    unsafe fn run(
        &self,
        value: birnam_abi::Oop,
    ) -> Result<ProjectRunObservation, ProjectManifestDecodeError> {
        if value == self.api.nilObj {
            return Ok(ProjectRunObservation {
                repl: "none".to_owned(),
                socket_bind: "127.0.0.1".to_owned(),
                socket_port: 4400,
            });
        }
        if unsafe { (self.api.vtableOf)(value) } != self.map_vt {
            return Err(ProjectManifestDecodeError::InvalidMap("run"));
        }
        let repl_value = unsafe { self.at(value, "repl")? };
        let repl = if repl_value == self.api.nilObj {
            "none".to_owned()
        } else {
            unsafe { self.string(repl_value, "run.repl")? }
        };
        if !matches!(repl.as_str(), "none" | "tty" | "socket") {
            return Err(ProjectManifestDecodeError::InvalidRunMode(repl));
        }
        let bind_value = unsafe { self.at(value, "socket_bind")? };
        let socket_bind = if bind_value == self.api.nilObj {
            "127.0.0.1".to_owned()
        } else {
            let bind = unsafe { self.string(bind_value, "run.socket_bind")? };
            if bind.is_empty() {
                return Err(ProjectManifestDecodeError::EmptyValue("run.socket_bind"));
            }
            bind
        };
        let port_value = unsafe { self.at(value, "socket_port")? };
        let socket_port = if port_value == self.api.nilObj {
            4400
        } else if unsafe { (birnam_abi::integer::is_integer)(port_value) } == 0 {
            return Err(ProjectManifestDecodeError::InvalidPort);
        } else {
            let port = unsafe { (birnam_abi::integer::value)(port_value) };
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

    unsafe fn modules(
        &self,
        value: birnam_abi::Oop,
    ) -> Result<Vec<ProjectModulePlan>, ProjectManifestDecodeError> {
        if value == self.api.nilObj {
            return Ok(Vec::new());
        }
        if unsafe { (self.api.vtableOf)(value) } != self.vector_vt {
            return Err(ProjectManifestDecodeError::InvalidList("module"));
        }
        let size_selector = unsafe { (self.api.intern)(c"size".as_ptr()) };
        let at_selector = unsafe { (self.api.intern)(c"at:".as_ptr()) };
        let size = unsafe { (self.api.send)(value, size_selector, std::ptr::null(), 0) };
        if unsafe { (birnam_abi::integer::is_integer)(size) } == 0
            || unsafe { (birnam_abi::integer::value)(size) } < 0
        {
            return Err(ProjectManifestDecodeError::InvalidList("module"));
        }
        let mut result = Vec::with_capacity(unsafe { (birnam_abi::integer::value)(size) as usize });
        for index in 0..unsafe { (birnam_abi::integer::value)(size) } {
            let item = unsafe {
                (self.api.send)(
                    value,
                    at_selector,
                    [(birnam_abi::integer::make)(index)].as_ptr(),
                    1,
                )
            };
            if item == self.api.nilObj || unsafe { (self.api.vtableOf)(item) } != self.map_vt {
                return Err(ProjectManifestDecodeError::InvalidMap("module entry"));
            }
            let entry = unsafe { self.string(self.at(item, "entry")?, "module.entry")? };
            if entry.is_empty()
                || Path::new(&entry).is_absolute()
                || Path::new(&entry).extension() != Some(OsStr::new("birnam"))
            {
                return Err(ProjectManifestDecodeError::InvalidPath("module.entry"));
            }
            let files = unsafe { self.strings(self.at(item, "files")?, "module.files", false)? };
            let _ = project_composition_paths(Path::new("."), &entry, &files)
                .map_err(|_| ProjectManifestDecodeError::InvalidPath("module.files"))?;
            result.push(ProjectModulePlan { entry, files });
        }
        Ok(result)
    }

    unsafe fn wrapped(
        &self,
        value: birnam_abi::Oop,
    ) -> Result<Vec<ProjectWrappedObservation>, ProjectManifestDecodeError> {
        if value == self.api.nilObj {
            return Ok(Vec::new());
        }
        if unsafe { (self.api.vtableOf)(value) } != self.vector_vt {
            return Err(ProjectManifestDecodeError::InvalidList("wrapped"));
        }
        let size_selector = unsafe { (self.api.intern)(c"size".as_ptr()) };
        let at_selector = unsafe { (self.api.intern)(c"at:".as_ptr()) };
        let size = unsafe { (self.api.send)(value, size_selector, std::ptr::null(), 0) };
        if unsafe { (birnam_abi::integer::is_integer)(size) } == 0
            || unsafe { (birnam_abi::integer::value)(size) } < 0
        {
            return Err(ProjectManifestDecodeError::InvalidList("wrapped"));
        }
        let mut result = Vec::with_capacity(unsafe { (birnam_abi::integer::value)(size) as usize });
        for index in 0..unsafe { (birnam_abi::integer::value)(size) } {
            let item = unsafe {
                (self.api.send)(
                    value,
                    at_selector,
                    [(birnam_abi::integer::make)(index)].as_ptr(),
                    1,
                )
            };
            if item == self.api.nilObj || unsafe { (self.api.vtableOf)(item) } != self.map_vt {
                return Err(ProjectManifestDecodeError::InvalidMap("wrapped entry"));
            }
            let name = unsafe { self.string(self.at(item, "name")?, "wrapped.name")? };
            if name.is_empty() || name.contains('/') || Path::new(&name).is_absolute() {
                return Err(ProjectManifestDecodeError::InvalidPath("wrapped.name"));
            }
            if result
                .iter()
                .any(|entry: &ProjectWrappedObservation| entry.name == name)
            {
                return Err(ProjectManifestDecodeError::DuplicateWrapped(name));
            }
            let build_value = unsafe { self.at(item, "build")? };
            if build_value == self.api.nilObj {
                return Err(ProjectManifestDecodeError::MissingValue("wrapped.build"));
            }
            if unsafe { (self.api.vtableOf)(build_value) } != self.vector_vt {
                return Err(ProjectManifestDecodeError::InvalidList("wrapped.build"));
            }
            let command_count =
                unsafe { (self.api.send)(build_value, size_selector, std::ptr::null(), 0) };
            if unsafe { (birnam_abi::integer::is_integer)(command_count) } == 0
                || unsafe { (birnam_abi::integer::value)(command_count) } <= 0
            {
                return Err(ProjectManifestDecodeError::EmptyValue("wrapped.build"));
            }
            let mut build =
                Vec::with_capacity(unsafe { (birnam_abi::integer::value)(command_count) as usize });
            let mut has_output = false;
            for command_index in 0..unsafe { (birnam_abi::integer::value)(command_count) } {
                let command = unsafe {
                    (self.api.send)(
                        build_value,
                        at_selector,
                        [(birnam_abi::integer::make)(command_index)].as_ptr(),
                        1,
                    )
                };
                let argv = unsafe { self.strings(command, "wrapped.build command", false)? };
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

    unsafe fn wrapped_artifacts(
        &self,
        value: birnam_abi::Oop,
    ) -> Result<Vec<ProjectWrappedArtifactObservation>, ProjectManifestDecodeError> {
        let wrapped = unsafe { self.wrapped(value)? };
        if wrapped.is_empty() {
            return Ok(Vec::new());
        }
        let at_selector = unsafe { (self.api.intern)(c"at:".as_ptr()) };
        let mut result = Vec::with_capacity(wrapped.len());
        for (index, artifact) in wrapped.into_iter().enumerate() {
            let item = unsafe {
                (self.api.send)(
                    value,
                    at_selector,
                    [(birnam_abi::integer::make)(index as i64)].as_ptr(),
                    1,
                )
            };
            let project_root =
                unsafe { self.string(self.at(item, "projectRoot")?, "wrapped.projectRoot")? };
            result.push(ProjectWrappedArtifactObservation {
                project_root,
                name: artifact.name,
                build: artifact.build,
            });
        }
        Ok(result)
    }

    unsafe fn test_sources(
        &self,
        value: birnam_abi::Oop,
    ) -> Result<Option<Vec<String>>, ProjectManifestDecodeError> {
        if value == self.api.nilObj {
            return Ok(None);
        }
        if unsafe { (self.api.vtableOf)(value) } != self.map_vt {
            return Err(ProjectManifestDecodeError::InvalidMap("test"));
        }
        let sources = unsafe { self.at(value, "sources")? };
        if sources == self.api.nilObj {
            return Err(ProjectManifestDecodeError::MissingValue("test.sources"));
        }
        let sources = unsafe { self.strings(sources, "test.sources", false)? };
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

    unsafe fn dependencies(
        &self,
        value: birnam_abi::Oop,
    ) -> Result<Vec<ProjectManifestDependencyObservation>, ProjectManifestDecodeError> {
        if value == self.api.nilObj {
            return Ok(Vec::new());
        }
        if unsafe { (self.api.vtableOf)(value) } != self.map_vt {
            return Err(ProjectManifestDecodeError::InvalidMap("dependencies"));
        }
        let keys_selector = unsafe { (self.api.intern)(c"keys".as_ptr()) };
        let keys = unsafe { (self.api.send)(value, keys_selector, std::ptr::null(), 0) };
        let names = unsafe { self.strings(keys, "dependency names", false)? };
        let mut result = Vec::with_capacity(names.len());
        for name in names {
            let spec = unsafe { self.at(value, &name) }
                .map_err(|_| ProjectManifestDecodeError::InvalidMap("dependency spec"))?;
            if spec == self.api.nilObj || unsafe { (self.api.vtableOf)(spec) } != self.map_vt {
                return Err(ProjectManifestDecodeError::InvalidMap("dependency spec"));
            }
            let path_value = unsafe { self.at(spec, "path")? };
            let git_value = unsafe { self.at(spec, "git")? };
            let revision_value = unsafe { self.at(spec, "rev")? };
            let source = match (
                path_value != self.api.nilObj,
                git_value != self.api.nilObj,
                revision_value != self.api.nilObj,
            ) {
                (true, false, false) => {
                    let path = unsafe { self.string(path_value, "dependency.path")? };
                    if path.is_empty() || Path::new(&path).is_absolute() {
                        return Err(ProjectManifestDecodeError::InvalidPath("dependency.path"));
                    }
                    ProjectDependencySourceObservation::Path(path)
                }
                (false, true, true) => {
                    let url = unsafe { self.string(git_value, "dependency.git")? };
                    let revision = unsafe { self.string(revision_value, "dependency.rev")? };
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

/// Decode host-owned manifest fields from a parsed TOML map. TOML syntax and
/// semantic provider decisions remain in Birnam; this only validates the
/// adapter's required value shapes and defaults.
///
/// # Safety
///
/// `api` and `parsed` must belong to the same live runtime, and all synchronous
/// sends must occur on the floor owner thread with a live GC anchor.
pub unsafe fn decode_project_manifest(
    api: &IdApi,
    parsed: birnam_abi::Oop,
) -> Result<ProjectManifestObservation, ProjectManifestDecodeError> {
    let decoder = unsafe { ProjectManifestDecoder::new(api)? };
    if parsed == api.nilObj || unsafe { (api.vtableOf)(parsed) } != decoder.map_vt {
        return Err(ProjectManifestDecodeError::NotMap);
    }
    let name_value = unsafe { decoder.at(parsed, "name")? };
    let name = if name_value == api.nilObj {
        String::new()
    } else {
        unsafe { decoder.string(name_value, "name")? }
    };
    let main_value = unsafe { decoder.at(parsed, "main")? };
    let main = if main_value == api.nilObj {
        None
    } else {
        let main = unsafe { decoder.string(main_value, "main")? };
        if Path::new(&main).is_absolute() {
            return Err(ProjectManifestDecodeError::InvalidPath("main"));
        }
        Some(main)
    };
    let roots_value = unsafe { decoder.at(parsed, "source_roots")? };
    let source_roots = if roots_value == api.nilObj {
        vec!["src".to_owned()]
    } else {
        unsafe { decoder.strings(roots_value, "source_roots", false)? }
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
    let loads = unsafe { decoder.strings(decoder.at(parsed, "load")?, "load", true)? };
    let run = unsafe { decoder.run(decoder.at(parsed, "run")?)? };
    let modules = unsafe { decoder.modules(decoder.at(parsed, "module")?)? };
    let dependencies = unsafe { decoder.dependencies(decoder.at(parsed, "dependencies")?)? };
    let wrapped = unsafe { decoder.wrapped(decoder.at(parsed, "wrapped")?)? };
    let test_sources = unsafe { decoder.test_sources(decoder.at(parsed, "test")?)? };
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

/// Decode the pointer-free resolution map returned by Birnam's
/// `ProjectPolicy resolveSnapshot:` selector. The policy remains responsible
/// for provider selection and ordering; this copies only its selected module
/// records for the host's later compile/check boundary.
///
/// # Safety
///
/// `api` and `resolution` must belong to the same live runtime, and all Oops
/// must remain valid for the duration of this synchronous decode.
pub unsafe fn decode_project_resolution(
    api: &IdApi,
    resolution: birnam_abi::Oop,
) -> Result<ProjectResolutionObservation, ProjectManifestDecodeError> {
    let decoder = unsafe { ProjectManifestDecoder::new(api)? };
    if resolution == api.nilObj || unsafe { (api.vtableOf)(resolution) } != decoder.map_vt {
        return Err(ProjectManifestDecodeError::NotMap);
    }
    let project_root =
        unsafe { decoder.string(decoder.at(resolution, "projectRoot")?, "projectRoot")? };
    let manifest = unsafe { decoder.string(decoder.at(resolution, "manifest")?, "manifest")? };
    let project_name =
        unsafe { decoder.string(decoder.at(resolution, "projectName")?, "projectName")? };
    let main_value = unsafe { decoder.at(resolution, "main")? };
    let main = if main_value == api.nilObj {
        None
    } else {
        Some(unsafe { decoder.string(main_value, "main")? })
    };
    let run = unsafe { decoder.run(decoder.at(resolution, "run")?)? };
    let load_roots =
        unsafe { decoder.strings(decoder.at(resolution, "loadRoots")?, "loadRoots", false)? };
    let wrapped = unsafe { decoder.wrapped_artifacts(decoder.at(resolution, "wrapped")?)? };
    let modules_value = unsafe { decoder.at(resolution, "modules")? };
    if modules_value == api.nilObj || unsafe { (api.vtableOf)(modules_value) } != decoder.vector_vt
    {
        return Err(ProjectManifestDecodeError::InvalidList("modules"));
    }
    let size_selector = unsafe { (api.intern)(c"size".as_ptr()) };
    let at_selector = unsafe { (api.intern)(c"at:".as_ptr()) };
    let size = unsafe { (api.send)(modules_value, size_selector, std::ptr::null(), 0) };
    if unsafe { (birnam_abi::integer::is_integer)(size) } == 0 || unsafe { (birnam_abi::integer::value)(size) } < 0 {
        return Err(ProjectManifestDecodeError::InvalidList("modules"));
    }
    let mut modules = Vec::with_capacity(unsafe { (birnam_abi::integer::value)(size) as usize });
    for index in 0..unsafe { (birnam_abi::integer::value)(size) } {
        let item = unsafe {
            (api.send)(
                modules_value,
                at_selector,
                [(birnam_abi::integer::make)(index)].as_ptr(),
                1,
            )
        };
        if item == api.nilObj || unsafe { (api.vtableOf)(item) } != decoder.map_vt {
            return Err(ProjectManifestDecodeError::InvalidMap("module"));
        }
        let name = unsafe { decoder.string(decoder.at(item, "name")?, "module.name")? };
        let path = unsafe { decoder.string(decoder.at(item, "path")?, "module.path")? };
        let sources =
            unsafe { decoder.strings(decoder.at(item, "sources")?, "module.sources", false)? };
        let project_root =
            unsafe { decoder.string(decoder.at(item, "projectRoot")?, "module.projectRoot")? };
        let source = unsafe { decoder.bytes(decoder.at(item, "source")?, "module.source")? };
        let requires =
            unsafe { decoder.strings(decoder.at(item, "requires")?, "module.requires", false)? };
        let source_digest =
            unsafe { decoder.string(decoder.at(item, "sourceDigest")?, "module.sourceDigest")? };
        let revision_digest = unsafe {
            decoder.string(decoder.at(item, "revisionDigest")?, "module.revisionDigest")?
        };
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

/// Decode Birnam's side-effect-free `ProjectPolicy artifactPlan:` result.
/// Artifact actions and reuse checks remain policy-owned; Rust only copies the
/// typed plan before compilation/publication adapters consume it.
///
/// # Safety
///
/// `api` and `plan` must belong to the same live runtime, and all Oops must
/// remain valid for the duration of this synchronous decode.
pub unsafe fn decode_project_artifact_plan(
    api: &IdApi,
    plan: birnam_abi::Oop,
) -> Result<ProjectArtifactPlan, ProjectManifestDecodeError> {
    let decoder = unsafe { ProjectManifestDecoder::new(api)? };
    if plan == api.nilObj || unsafe { (api.vtableOf)(plan) } != decoder.map_vt {
        return Err(ProjectManifestDecodeError::NotMap);
    }
    let publication = unsafe { decoder.string(decoder.at(plan, "publication")?, "publication")? };
    let artifacts_value = unsafe { decoder.at(plan, "artifacts")? };
    if artifacts_value == api.nilObj
        || unsafe { (api.vtableOf)(artifacts_value) } != decoder.vector_vt
    {
        return Err(ProjectManifestDecodeError::InvalidList("artifacts"));
    }
    let size_selector = unsafe { (api.intern)(c"size".as_ptr()) };
    let at_selector = unsafe { (api.intern)(c"at:".as_ptr()) };
    let size = unsafe { (api.send)(artifacts_value, size_selector, std::ptr::null(), 0) };
    if unsafe { (birnam_abi::integer::is_integer)(size) } == 0 || unsafe { (birnam_abi::integer::value)(size) } < 0 {
        return Err(ProjectManifestDecodeError::InvalidList("artifacts"));
    }
    let mut artifacts = Vec::with_capacity(unsafe { (birnam_abi::integer::value)(size) as usize });
    for index in 0..unsafe { (birnam_abi::integer::value)(size) } {
        let item = unsafe {
            (api.send)(
                artifacts_value,
                at_selector,
                [(birnam_abi::integer::make)(index)].as_ptr(),
                1,
            )
        };
        if item == api.nilObj || unsafe { (api.vtableOf)(item) } != decoder.map_vt {
            return Err(ProjectManifestDecodeError::InvalidMap("artifact"));
        }
        let name = unsafe { decoder.string(decoder.at(item, "name")?, "artifact.name")? };
        let path = unsafe { decoder.string(decoder.at(item, "path")?, "artifact.path")? };
        let action = unsafe { decoder.string(decoder.at(item, "action")?, "artifact.action")? };
        let source = unsafe { decoder.string(decoder.at(item, "source")?, "artifact.source")? };
        let sources =
            unsafe { decoder.strings(decoder.at(item, "sources")?, "artifact.sources", false)? };
        let source_digest =
            unsafe { decoder.string(decoder.at(item, "sourceDigest")?, "artifact.sourceDigest")? };
        let revision_digest = unsafe {
            decoder.string(
                decoder.at(item, "revisionDigest")?,
                "artifact.revisionDigest",
            )?
        };
        let requires =
            unsafe { decoder.strings(decoder.at(item, "requires")?, "artifact.requires", false)? };
        let checksum_value = unsafe { decoder.at(item, "checksum")? };
        let checksum = if checksum_value == api.nilObj {
            None
        } else {
            Some(unsafe { decoder.string(checksum_value, "artifact.checksum")? })
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

unsafe fn active_project_bridge_runtime() -> Option<&'static HostRuntime> {
    let pointer = PROJECT_BRIDGE_RUNTIME.load(Ordering::Acquire);
    if pointer.is_null() {
        None
    } else {
        // SAFETY: the host installs this pointer only for its own live
        // HostRuntime and clears it from HostRuntime::drop.
        Some(unsafe { &*pointer })
    }
}

unsafe fn project_bridge_failure(api: &IdApi, kind: &str, message: &str) -> birnam_abi::Oop {
    let Ok(builder) = (unsafe { ProjectSnapshotBuilder::new(api) }) else {
        return api.nilObj;
    };
    let Ok(mut result) = (unsafe { builder.new_map() }) else {
        return api.nilObj;
    };
    let Ok(updated) = (unsafe { builder.put(result, "ok", api.falseObj) }) else {
        return api.nilObj;
    };
    result = updated;
    let Ok(updated) =
        (unsafe { builder.put(result, "kind", (birnam_abi::string::make)(kind.as_ptr(), kind.len())) })
    else {
        return api.nilObj;
    };
    result = updated;
    let Ok(updated) = (unsafe {
        builder.put(
            result,
            "message",
            (birnam_abi::string::make)(message.as_ptr(), message.len()),
        )
    }) else {
        return api.nilObj;
    };
    updated
}

unsafe extern "C" fn project_path_for_uri_native(
    _: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    count: usize,
) -> birnam_abi::Oop {
    let Some(runtime) = (unsafe { active_project_bridge_runtime() }) else {
        return std::ptr::null_mut();
    };
    let api = runtime.stack.api();
    if count != 1 || args.is_null() {
        return api.nilObj;
    }
    let argument = unsafe { *args };
    let Some(uri) = (unsafe { bridge_string(api, argument) }) else {
        return api.nilObj;
    };
    let Ok(path) = project_path_from_uri(&uri) else {
        return unsafe {
            project_bridge_failure(api, "projectUriInvalid", "could not decode workspace URI")
        };
    };
    let bytes = path_bytes(&path);
    unsafe { (birnam_abi::string::make)(bytes.as_ptr(), bytes.len()) }
}

unsafe extern "C" fn project_snapshot_for_uri_native(
    _: birnam_abi::Oop,
    args: *const birnam_abi::Oop,
    count: usize,
) -> birnam_abi::Oop {
    let Some(runtime) = (unsafe { active_project_bridge_runtime() }) else {
        return std::ptr::null_mut();
    };
    let api = runtime.stack.api();
    if count != 1 || args.is_null() {
        return api.nilObj;
    }
    let argument = unsafe { *args };
    let Some(uri) = (unsafe { bridge_string(api, argument) }) else {
        return api.nilObj;
    };
    let Ok(path) = project_path_from_uri(&uri) else {
        return unsafe {
            project_bridge_failure(api, "projectUriInvalid", "could not decode workspace URI")
        };
    };
    let start = if std::fs::metadata(&path)
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
    {
        path
    } else {
        path.parent().map(Path::to_path_buf).unwrap_or(path)
    };
    let snapshot = match unsafe { runtime.observe_project_snapshot(&start) } {
        Ok(snapshot) => snapshot,
        Err(error) => {
            let message = format!("{error:?}");
            return unsafe { project_bridge_failure(api, "projectResolutionFailed", &message) };
        }
    };
    let snapshot = match unsafe { runtime.materialize_project_snapshot(&snapshot) } {
        Ok(snapshot) => snapshot,
        Err(error) => {
            let message = format!("{error:?}");
            return unsafe { project_bridge_failure(api, "projectSnapshotFailed", &message) };
        }
    };
    match unsafe { runtime.resolve_project_snapshot(snapshot) } {
        Ok(resolution) => {
            // ProjectPolicy returns the resolution payload itself. The shared
            // LSP bridge contract adds an explicit success bit so Birnam can
            // distinguish a valid snapshot from its typed failure maps.
            let Ok(builder) = (unsafe { ProjectSnapshotBuilder::new(api) }) else {
                return api.nilObj;
            };
            match unsafe { builder.put(resolution, "ok", api.trueObj) } {
                Ok(resolution) => resolution,
                Err(_) => api.nilObj,
            }
        }
        Err(ProjectPolicyError::ResolutionDiagnostic { kind, message }) => unsafe {
            project_bridge_failure(api, &kind, &message)
        },
        Err(error) => {
            let message = format!("{error:?}");
            unsafe { project_bridge_failure(api, "projectResolutionFailed", &message) }
        }
    }
}

struct ProjectSnapshotBuilder<'a> {
    api: &'a IdApi,
    vector_class: birnam_abi::Oop,
    map_class: birnam_abi::Oop,
}

impl<'a> ProjectSnapshotBuilder<'a> {
    unsafe fn new(api: &'a IdApi) -> Result<Self, ProjectSnapshotError> {
        let vector_class = unsafe { (api.import)(c"Vector".as_ptr()) };
        let map_class = unsafe { (api.import)(c"Map".as_ptr()) };
        if vector_class == api.nilObj {
            return Err(ProjectSnapshotError::MissingVector);
        }
        if map_class == api.nilObj {
            return Err(ProjectSnapshotError::MissingMap);
        }
        Ok(Self {
            api,
            vector_class,
            map_class,
        })
    }

    unsafe fn new_vector(&self) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let selector = unsafe { (self.api.intern)(c"new".as_ptr()) };
        let value = unsafe { (self.api.send)(self.vector_class, selector, std::ptr::null(), 0) };
        if value == self.api.nilObj || send_is_error(self.api, value) {
            Err(ProjectSnapshotError::AllocationRejected)
        } else {
            Ok(value)
        }
    }

    unsafe fn new_map(&self) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let selector = unsafe { (self.api.intern)(c"new".as_ptr()) };
        let value = unsafe { (self.api.send)(self.map_class, selector, std::ptr::null(), 0) };
        if value == self.api.nilObj || send_is_error(self.api, value) {
            Err(ProjectSnapshotError::AllocationRejected)
        } else {
            Ok(value)
        }
    }

    unsafe fn add(
        &self,
        vector: birnam_abi::Oop,
        value: birnam_abi::Oop,
    ) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let selector = unsafe { (self.api.intern)(c"add:".as_ptr()) };
        let updated = unsafe { (self.api.send)(vector, selector, [value].as_ptr(), 1) };
        if updated == self.api.nilObj || send_is_error(self.api, updated) {
            Err(ProjectSnapshotError::AllocationRejected)
        } else {
            Ok(updated)
        }
    }

    unsafe fn put(
        &self,
        map: birnam_abi::Oop,
        key: &str,
        value: birnam_abi::Oop,
    ) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let key = unsafe { (birnam_abi::string::make)(key.as_bytes().as_ptr(), key.len()) };
        let selector = unsafe { (self.api.intern)(c"at:put:".as_ptr()) };
        let updated = unsafe { (self.api.send)(map, selector, [key, value].as_ptr(), 2) };
        if updated == self.api.nilObj || send_is_error(self.api, updated) {
            Err(ProjectSnapshotError::AllocationRejected)
        } else {
            Ok(updated)
        }
    }

    unsafe fn string(&self, value: &str) -> birnam_abi::Oop {
        unsafe { (birnam_abi::string::make)(value.as_bytes().as_ptr(), value.len()) }
    }

    unsafe fn bytes(&self, value: &[u8]) -> birnam_abi::Oop {
        unsafe { (birnam_abi::string::make)(value.as_ptr(), value.len()) }
    }

    unsafe fn strings(&self, values: &[String]) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let mut vector = unsafe { self.new_vector()? };
        for value in values {
            vector = unsafe { self.add(vector, self.string(value))? };
        }
        Ok(vector)
    }

    unsafe fn previous_artifact(
        &self,
        value: &ProjectPreviousPublishedArtifact,
    ) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let mut item = unsafe { self.new_map()? };
        item = unsafe { self.put(item, "name", self.string(&value.name))? };
        item = unsafe { self.put(item, "path", self.string(&value.path))? };
        item = unsafe { self.put(item, "source", self.string(&value.source))? };
        item = unsafe { self.put(item, "sources", self.strings(&value.sources)?)? };
        item = unsafe { self.put(item, "sourceDigest", self.string(&value.source_digest))? };
        item = unsafe { self.put(item, "revisionDigest", self.string(&value.revision_digest))? };
        item = unsafe { self.put(item, "requires", self.strings(&value.requires)?)? };
        item = unsafe { self.put(item, "checksum", self.string(&value.checksum))? };
        if let Some(observed) = &value.observed_checksum {
            item = unsafe { self.put(item, "observedChecksum", self.string(observed))? };
        }
        Ok(item)
    }

    unsafe fn previous_publication(
        &self,
        value: &ProjectPreviousPublication,
    ) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let mut result = unsafe { self.new_map()? };
        result = unsafe { self.put(result, "projectRoot", self.string(&value.project_root))? };
        result = unsafe { self.put(result, "target", self.string(&value.target))? };
        result = unsafe { self.put(result, "profile", self.string(&value.profile))? };
        result = unsafe { self.put(result, "toolVersion", self.string(&value.tool_version))? };
        result = unsafe { self.put(result, "artifactAbi", self.string(&value.artifact_abi))? };
        let mut artifacts = unsafe { self.new_vector()? };
        for artifact in &value.artifacts {
            artifacts = unsafe { self.add(artifacts, self.previous_artifact(artifact)?)? };
        }
        unsafe { self.put(result, "artifacts", artifacts) }
    }

    unsafe fn artifact_module(
        &self,
        value: &ProjectSourceObservation,
    ) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let mut item = unsafe { self.new_map()? };
        item = unsafe { self.put(item, "name", self.string(&value.name))? };
        item = unsafe { self.put(item, "path", self.string(&value.path))? };
        item = unsafe { self.put(item, "sources", self.strings(&value.sources)?)? };
        item = unsafe { self.put(item, "sourceDigest", self.string(&value.source_digest))? };
        item = unsafe { self.put(item, "revisionDigest", self.string(&value.revision_digest))? };
        unsafe { self.put(item, "requires", self.strings(&value.requires)?) }
    }

    unsafe fn artifact_plan_input(
        &self,
        request: &ProjectArtifactPlanRequest<'_>,
    ) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let mut snapshot = unsafe { self.new_map()? };
        snapshot = unsafe {
            self.put(
                snapshot,
                "projectRoot",
                self.string(&request.resolution.project_root),
            )?
        };
        snapshot = unsafe { self.put(snapshot, "target", self.string(request.target))? };
        snapshot = unsafe { self.put(snapshot, "profile", self.string(request.profile))? };
        snapshot = unsafe { self.put(snapshot, "toolVersion", self.string(request.tool_version))? };
        let artifact_abi = String::from_utf8_lossy(birnam_artifact::ABI);
        snapshot = unsafe { self.put(snapshot, "artifactAbi", self.string(&artifact_abi))? };
        let mut modules = unsafe { self.new_vector()? };
        for module in &request.resolution.modules {
            modules = unsafe { self.add(modules, self.artifact_module(module)?)? };
        }
        snapshot = unsafe { self.put(snapshot, "modules", modules)? };
        let previous = if let Some(previous) = request.previous {
            unsafe { self.previous_publication(previous)? }
        } else {
            self.api.nilObj
        };
        unsafe { self.put(snapshot, "previous", previous) }
    }

    unsafe fn wrapped(
        &self,
        values: &[ProjectWrappedObservation],
    ) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let mut result = unsafe { self.new_vector()? };
        for value in values {
            let mut item = unsafe { self.new_map()? };
            item = unsafe { self.put(item, "name", self.string(&value.name))? };
            let mut commands = unsafe { self.new_vector()? };
            for command in &value.build {
                commands = unsafe { self.add(commands, self.strings(command)?)? };
            }
            item = unsafe { self.put(item, "build", commands)? };
            result = unsafe { self.add(result, item)? };
        }
        Ok(result)
    }

    unsafe fn dependencies(
        &self,
        values: &[ProjectDependencyObservation],
    ) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let mut result = unsafe { self.new_vector()? };
        for value in values {
            let mut item = unsafe { self.new_map()? };
            item = unsafe { self.put(item, "name", self.string(&value.name))? };
            item = unsafe { self.put(item, "root", self.string(&value.root))? };
            result = unsafe { self.add(result, item)? };
        }
        Ok(result)
    }

    unsafe fn source(
        &self,
        value: &ProjectSourceObservation,
    ) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let mut item = unsafe { self.new_map()? };
        item = unsafe { self.put(item, "name", self.string(&value.name))? };
        item = unsafe { self.put(item, "path", self.string(&value.path))? };
        item = unsafe { self.put(item, "sources", self.strings(&value.sources)?)? };
        item = unsafe { self.put(item, "projectRoot", self.string(&value.project_root))? };
        item = unsafe { self.put(item, "source", self.bytes(&value.source))? };
        item = unsafe { self.put(item, "requires", self.strings(&value.requires)?)? };
        item = unsafe { self.put(item, "sourceDigest", self.string(&value.source_digest))? };
        item = unsafe { self.put(item, "revisionDigest", self.string(&value.revision_digest))? };
        Ok(item)
    }

    unsafe fn project(
        &self,
        value: &ProjectObservation,
    ) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let mut item = unsafe { self.new_map()? };
        item = unsafe { self.put(item, "root", self.string(&value.root))? };
        item = unsafe { self.put(item, "manifest", self.string(&value.manifest))? };
        item = unsafe { self.put(item, "name", self.string(&value.name))? };
        item = unsafe {
            self.put(
                item,
                "main",
                value
                    .main
                    .as_deref()
                    .map(|main| self.string(main))
                    .unwrap_or(self.api.nilObj),
            )?
        };
        item = unsafe { self.put(item, "loads", self.strings(&value.loads)?)? };
        let mut run = unsafe { self.new_map()? };
        run = unsafe { self.put(run, "repl", self.string(&value.run.repl))? };
        run = unsafe { self.put(run, "socketBind", self.string(&value.run.socket_bind))? };
        run = unsafe {
            self.put(
                run,
                "socketPort",
                (birnam_abi::integer::make)(value.run.socket_port),
            )?
        };
        item = unsafe { self.put(item, "run", run)? };
        item = unsafe { self.put(item, "wrapped", self.wrapped(&value.wrapped)?)? };
        item = unsafe {
            self.put(
                item,
                "dependencies",
                self.dependencies(&value.dependencies)?,
            )?
        };
        let mut sources = unsafe { self.new_vector()? };
        for source in &value.sources {
            sources = unsafe { self.add(sources, self.source(source)?)? };
        }
        unsafe { self.put(item, "sources", sources) }
    }

    unsafe fn snapshot(
        &self,
        value: &ProjectSnapshot,
    ) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let mut projects = unsafe { self.new_vector()? };
        for project in &value.projects {
            projects = unsafe { self.add(projects, self.project(project)?)? };
        }
        let mut snapshot = unsafe { self.new_map()? };
        snapshot = unsafe { self.put(snapshot, "root", self.string(&value.root))? };
        unsafe { self.put(snapshot, "projects", projects) }
    }
}

impl HostRuntime {
    unsafe fn activate_project_build(
        &self,
        build: &ProjectBuildResult,
    ) -> Result<(), ProjectRunError> {
        let plan = preflight_artifact(&build.module_root, None)
            .map_err(ProjectRunError::ArtifactPreflight)?;
        for entry in &plan.entries {
            unsafe { activate_preflighted_artifact(self, entry) }
                .map_err(ProjectRunError::ArtifactActivation)?;
        }
        Ok(())
    }

    unsafe fn project_publication_run_configuration(
        &self,
    ) -> Result<ProjectRunObservation, ProjectRunError> {
        let api = self.stack.api();
        let repl = unsafe { global_named(api, PROJECT_REPL_GLOBAL) }
            .and_then(|value| unsafe { bridge_string(api, value) })
            .ok_or(ProjectRunError::PublishedConfiguration)?;
        let socket_bind = unsafe { global_named(api, PROJECT_SOCKET_BIND_GLOBAL) }
            .and_then(|value| unsafe { bridge_string(api, value) })
            .ok_or(ProjectRunError::PublishedConfiguration)?;
        let port = unsafe { global_named(api, PROJECT_SOCKET_PORT_GLOBAL) }
            .ok_or(ProjectRunError::PublishedConfiguration)?;
        if unsafe { (birnam_abi::integer::is_integer)(port) } == 0 {
            return Err(ProjectRunError::PublishedConfiguration);
        }
        let socket_port = unsafe { (birnam_abi::integer::value)(port) };
        if !(0..=65535).contains(&socket_port) {
            return Err(ProjectRunError::PublishedConfiguration);
        }
        Ok(ProjectRunObservation {
            repl,
            socket_bind,
            socket_port,
        })
    }

    /// Run a project main after validating/building its publication, loading
    /// project modules through the canonical Module manager, and installing
    /// ProjectArgs. Repl/socket follow the observed run policy unless the CLI
    /// supplies an explicit mode.
    ///
    /// # Safety
    ///
    /// The caller must keep a live GC anchor and remain on the floor owner
    /// thread for every policy, activation, evaluation, and REPL send.
    pub unsafe fn run_project_with_tool_version(
        &self,
        options: &RunOptions,
        tool_version: &str,
    ) -> Result<(), ProjectRunError> {
        let target = host_target_identity();
        let build = if options.build {
            let request = ProjectBuildRequest {
                start: &options.directory,
                target: &target,
                profile: &options.profile,
                tool_version,
            };
            unsafe { self.build_project(&request) }
        } else {
            unsafe {
                self.locate_project_build(
                    &options.directory,
                    &target,
                    &options.profile,
                    tool_version,
                )
            }
        }
        .map_err(ProjectRunError::Build)?;
        let application = build.application.as_deref().ok_or_else(|| {
            ProjectRunError::ApplicationUnavailable(
                build
                    .output_root
                    .join("lib")
                    .join(PROJECT_APPLICATION_FILENAME),
            )
        })?;
        let plan =
            preflight_artifact(application, None).map_err(ProjectRunError::ArtifactPreflight)?;
        let configuration_bytes = read_artifact_bytes(&build.configuration)
            .map_err(ProjectRunError::ArtifactPreflight)?;
        let configuration_descriptor =
            birnam_artifact::inspect(&configuration_bytes).map_err(|error| {
                ProjectRunError::ArtifactPreflight(ArtifactPreflightError::Parse {
                    path: build.configuration.clone(),
                    error,
                })
            })?;
        let configuration = plan
            .entries
            .iter()
            .find(|entry| entry.identity == configuration_descriptor.identity)
            .ok_or(ProjectRunError::PublishedConfiguration)?;
        unsafe { activate_preflighted_artifact(self, configuration) }
            .map_err(ProjectRunError::ArtifactActivation)?;
        let published_run = unsafe { self.project_publication_run_configuration() }?;
        let mode = if let Some(mode) = options.repl {
            mode
        } else {
            match published_run.repl.as_str() {
                "none" => ReplMode::None,
                "tty" => ReplMode::Tty,
                "socket" => ReplMode::Socket,
                value => return Err(ProjectRunError::InvalidReplMode(value.to_owned())),
            }
        };
        match mode {
            ReplMode::None => {}
            ReplMode::Tty => {
                unsafe { self.load_runtime_module("repl") }.map_err(ProjectRunError::Module)?
            }
            ReplMode::Socket => {
                unsafe { self.load_runtime_module("socket") }.map_err(ProjectRunError::Module)?;
                unsafe { self.load_runtime_module("repl") }.map_err(ProjectRunError::Module)?;
            }
        }
        let api = self.stack.api();
        let globals =
            unsafe { global_named(api, "Globals") }.ok_or(ProjectRunError::ArgumentsRejected)?;
        let values = unsafe { native_os_string_vector(api, &options.arguments) }
            .map_err(|_| ProjectRunError::ArgumentsRejected)?;
        let key = unsafe { (api.intern)(c"ProjectArgs".as_ptr()) };
        let put = unsafe { (api.intern)(c"at:put:".as_ptr()) };
        let result = unsafe { (api.send)(globals, put, [key, values].as_ptr(), 2) };
        if result == api.nilObj || send_is_error(api, result) {
            return Err(ProjectRunError::ArgumentsRejected);
        }
        let application = plan
            .entries
            .iter()
            .find(|entry| entry.path == application)
            .ok_or(ProjectRunError::ArtifactActivation(
                ArtifactActivationError::ActivationRejected,
            ))?;
        unsafe { activate_preflighted_artifact(self, application) }
            .map_err(ProjectRunError::ArtifactActivation)?;
        match mode {
            ReplMode::None => Ok(()),
            ReplMode::Tty => {
                let mut session = unsafe { self.start_repl() }.map_err(ProjectRunError::Repl)?;
                unsafe { self.run_repl(&mut session) }.map_err(ProjectRunError::Repl)
            }
            ReplMode::Socket => {
                let bind = options
                    .socket_bind
                    .as_deref()
                    .unwrap_or(&published_run.socket_bind);
                let port = options
                    .socket_port
                    .unwrap_or(published_run.socket_port as u16);
                unsafe { self.serve_socket(port, bind) }.map_err(ProjectRunError::Socket)
            }
        }
    }

    /// Run a project using the host crate's package version when no external
    /// toolchain version is available.
    ///
    /// # Safety
    ///
    /// See [`HostRuntime::run_project_with_tool_version`].
    pub unsafe fn run_project(&self, options: &RunOptions) -> Result<(), ProjectRunError> {
        unsafe { self.run_project_with_tool_version(options, env!("CARGO_PKG_VERSION")) }
    }

    /// Validate/activate one published project and evaluate one test source in
    /// a fresh worker process. The caller owns worker lifecycle and output.
    ///
    /// # Safety
    ///
    /// See [`HostRuntime::run_project_with_tool_version`].
    pub unsafe fn run_project_test_source(
        &self,
        project_root: &Path,
        test_source: &Path,
        target: &str,
        profile: &str,
        tool_version: &str,
    ) -> Result<(), ProjectRunError> {
        let build =
            unsafe { self.locate_project_build(project_root, target, profile, tool_version) }
                .map_err(ProjectRunError::Build)?;
        unsafe { self.activate_project_build(&build) }?;
        // Match the worker boundary: install the test harness before the
        // project source registers suites, then run those suites through the
        // canonical ConsoleRunner. Evaluating a source file alone is not a
        // test run; it would silently accept a suite-registration script
        // without producing the established `ok`/`FAIL` result lines.
        unsafe { self.load_host_runtime_module("test") }.map_err(ProjectRunError::Module)?;
        unsafe { eval_script(self, test_source) }.map_err(ProjectRunError::Main)?;
        let api = self.stack.api();
        let test =
            unsafe { global_named(api, "Test") }.ok_or(ProjectRunError::TestHarnessUnavailable)?;
        let runner = unsafe { global_named(api, "ConsoleRunner") }
            .ok_or(ProjectRunError::TestHarnessUnavailable)?;
        let suites_selector = unsafe { (api.intern)(c"suites".as_ptr()) };
        let suites = unsafe { (api.send)(test, suites_selector, std::ptr::null(), 0) };
        if suites == api.nilObj || send_is_error(api, suites) {
            return Err(ProjectRunError::TestHarnessUnavailable);
        }
        let size_selector = unsafe { (api.intern)(c"size".as_ptr()) };
        let count = unsafe { (api.send)(suites, size_selector, std::ptr::null(), 0) };
        if unsafe { (birnam_abi::integer::is_integer)(count) } == 0 || unsafe { (birnam_abi::integer::value)(count) } == 0 {
            return Err(ProjectRunError::TestSuiteMissing);
        }
        let run_selector = unsafe { (api.intern)(c"runAllSuitesWithRunner:".as_ptr()) };
        let result = unsafe { (api.send)(test, run_selector, [runner].as_ptr(), 1) };
        if result == api.nilObj || send_is_error(api, result) {
            return Err(ProjectRunError::TestFailed);
        }
        let passed_selector = unsafe { (api.intern)(c"allPassed".as_ptr()) };
        let passed = unsafe { (api.send)(result, passed_selector, std::ptr::null(), 0) };
        if passed != api.trueObj {
            return Err(ProjectRunError::TestFailed);
        }
        Ok(())
    }

    /// Lazily install the Birnam-owned project policy module. Filesystem
    /// observation and the `Project` bridge remain a later host seam; this
    /// method only makes the semantic policy available through the existing
    /// compiled-module/source-recovery rule.
    ///
    /// # Safety
    ///
    /// The caller must have prepared a live GC anchor and remain on the floor
    /// owner thread for all synchronous sends.
    pub unsafe fn load_project_policy(&self) -> Result<(), ProjectPolicyError> {
        if unsafe { global_named(self.stack.api(), "ProjectPolicy") }.is_some() {
            return Ok(());
        }
        unsafe {
            load_self_hosted(
                self.stack.api(),
                self.codegen,
                &self.resources,
                &PROJECT_POLICY_MODULE,
                self.policy.force_source,
            )
        }
        .map_err(ProjectPolicyError::Source)?;
        if let Some(manager) = unsafe { global_named(self.stack.api(), "Module") } {
            unsafe { mark_module_loaded(self.stack.api(), manager, PROJECT_POLICY_MODULE.name) };
        }
        Ok(())
    }

    /// Apply Birnam's project-resolution policy to one host-observed snapshot.
    /// The snapshot is an Oop map/vector graph owned by the live Store; this
    /// method does not inspect filesystem data or reimplement dependency
    /// closure rules in Rust.
    ///
    /// # Safety
    ///
    /// `snapshot` must be a live value from this runtime. The caller must have
    /// prepared a live GC anchor and remain on the floor owner thread.
    pub unsafe fn resolve_project_snapshot(
        &self,
        snapshot: birnam_abi::Oop,
    ) -> Result<birnam_abi::Oop, ProjectPolicyError> {
        let api = self.stack.api();
        if snapshot == api.nilObj || send_is_error(api, snapshot) {
            return Err(ProjectPolicyError::InvalidSnapshot);
        }
        unsafe { self.load_project_policy()? };
        let Some(policy) = (unsafe { global_named(api, "ProjectPolicy") }) else {
            return Err(ProjectPolicyError::MissingPolicy);
        };
        let selector = unsafe { (api.intern)(c"resolveSnapshot:".as_ptr()) };
        let result = unsafe { (api.send)(policy, selector, [snapshot].as_ptr(), 1) };
        if result == api.nilObj || send_is_error(api, result) {
            if let Some((kind, message)) = unsafe { error_diagnostic(api, result) } {
                return Err(ProjectPolicyError::ResolutionDiagnostic { kind, message });
            }
            return Err(ProjectPolicyError::ResolutionRejected);
        }
        Ok(result)
    }

    /// Materialize a typed host observation into the exact map/vector shape
    /// consumed by [`HostRuntime::resolve_project_snapshot`]. This is a
    /// serialization boundary only; it performs no policy decisions.
    ///
    /// # Safety
    ///
    /// The runtime must remain on the floor owner thread with a live GC anchor
    /// for all allocations and sends.
    pub unsafe fn materialize_project_snapshot(
        &self,
        snapshot: &ProjectSnapshot,
    ) -> Result<birnam_abi::Oop, ProjectSnapshotError> {
        let builder = unsafe { ProjectSnapshotBuilder::new(self.stack.api())? };
        unsafe { builder.snapshot(snapshot) }
    }

    /// Read one manifest and delegate TOML parsing to the live self-hosted
    /// `Toml parse:` protocol. Rust owns only file I/O and returns the
    /// resulting Map Oop for the observation adapter; it does not duplicate a
    /// TOML grammar or reinterpret parser errors.
    ///
    /// # Safety
    ///
    /// The returned map is owned by this runtime's Store. The caller must keep
    /// a live GC anchor and remain on the floor owner thread while using it.
    pub unsafe fn parse_project_manifest(
        &self,
        path: &Path,
    ) -> Result<birnam_abi::Oop, ProjectManifestError> {
        let source = std::fs::read(path).map_err(|error| ProjectManifestError::Read {
            path: path.to_path_buf(),
            kind: error.kind(),
        })?;
        let api = self.stack.api();
        let toml = unsafe { global_named(api, "Toml") }.ok_or(ProjectManifestError::MissingToml)?;
        let source = unsafe { (birnam_abi::string::make)(source.as_ptr(), source.len()) };
        let selector = unsafe { (api.intern)(c"parse:".as_ptr()) };
        let result = unsafe { (api.send)(toml, selector, [source].as_ptr(), 1) };
        if result == api.nilObj || send_is_error(api, result) {
            return Err(ProjectManifestError::ParseRejected);
        }
        Ok(result)
    }

    /// Compose nearest-root discovery with self-hosted manifest parsing. The
    /// returned path is canonical and the returned Map is the live parsed TOML
    /// value; source indexing and ProjectPolicy invocation remain separate
    /// operations so callers cannot accidentally mix observation and policy.
    ///
    /// # Safety
    ///
    /// The returned Map is owned by this runtime's Store. Keep a live GC anchor
    /// and remain on the floor owner thread while using it.
    pub unsafe fn prepare_project_manifest(
        &self,
        start: &Path,
    ) -> Result<(PathBuf, birnam_abi::Oop), ProjectPreparationError> {
        let root = nearest_project_root(start).map_err(ProjectPreparationError::Discovery)?;
        let manifest = root.join("birnam.toml");
        let parsed = unsafe { self.parse_project_manifest(&manifest) }
            .map_err(ProjectPreparationError::Manifest)?;
        Ok((root, parsed))
    }

    unsafe fn fetch_project_dependencies_at(
        &self,
        root: &Path,
        visiting: &mut HashSet<PathBuf>,
        visited: &mut HashSet<PathBuf>,
        fetched: &mut usize,
        reused: &mut usize,
    ) -> Result<(), ProjectFetchError> {
        let root =
            std::fs::canonicalize(root).map_err(|error| ProjectFetchError::Canonicalize {
                path: root.to_path_buf(),
                kind: error.kind(),
            })?;
        if visited.contains(&root) {
            return Ok(());
        }
        if !visiting.insert(root.clone()) {
            return Err(ProjectFetchError::DependencyCycle(root));
        }
        let result = (|| {
            let manifest_path = root.join("birnam.toml");
            let parsed = unsafe { self.parse_project_manifest(&manifest_path) }
                .map_err(ProjectFetchError::Manifest)?;
            let manifest = unsafe { decode_project_manifest(self.stack.api(), parsed) }
                .map_err(ProjectFetchError::Decode)?;
            for dependency in &manifest.dependencies {
                let dependency_root = match &dependency.source {
                    ProjectDependencySourceObservation::Path(path) => {
                        let path_dependency = ProjectDependencyObservation {
                            name: dependency.name.clone(),
                            root: path.clone(),
                        };
                        PathBuf::from(
                            &resolve_project_dependency_paths(&root, &[path_dependency])
                                .map_err(ProjectFetchError::Dependency)?[0]
                                .root,
                        )
                    }
                    ProjectDependencySourceObservation::Git { .. } => {
                        let (path, acquired) = acquire_project_git_dependency(&root, dependency)
                            .map_err(ProjectFetchError::Dependency)?;
                        if acquired {
                            *fetched += 1;
                        } else {
                            *reused += 1;
                        }
                        path
                    }
                };
                unsafe {
                    self.fetch_project_dependencies_at(
                        &dependency_root,
                        visiting,
                        visited,
                        fetched,
                        reused,
                    )?
                };
            }
            Ok::<(), ProjectFetchError>(())
        })();
        visiting.remove(&root);
        if result.is_ok() {
            visited.insert(root);
        }
        result
    }

    /// Acquire every commit-pinned Git dependency in the complete project
    /// closure. This is the sole network-capable dependency operation; normal
    /// project observation only validates these cached checkouts.
    ///
    /// # Safety
    ///
    /// The caller must keep a live GC anchor and remain on the floor owner
    /// thread while each dependency manifest is parsed.
    pub unsafe fn fetch_project_dependencies(
        &self,
        start: &Path,
    ) -> Result<ProjectFetchResult, ProjectFetchError> {
        let root = nearest_project_root(start).map_err(ProjectFetchError::Discovery)?;
        let mut visiting = HashSet::new();
        let mut visited = HashSet::new();
        let mut fetched = 0;
        let mut reused = 0;
        unsafe {
            self.fetch_project_dependencies_at(
                &root,
                &mut visiting,
                &mut visited,
                &mut fetched,
                &mut reused,
            )?
        };
        Ok(ProjectFetchResult {
            project_root: root,
            fetched,
            reused,
        })
    }

    unsafe fn observe_project_at(
        &self,
        root: &Path,
        is_root: bool,
    ) -> Result<(ProjectObservation, Vec<ProjectDependencyObservation>), ProjectObservationBuildError>
    {
        let manifest_path = root.join("birnam.toml");
        let parsed = unsafe { self.parse_project_manifest(&manifest_path) }
            .map_err(ProjectObservationBuildError::Manifest)?;
        let manifest = unsafe { decode_project_manifest(self.stack.api(), parsed) }
            .map_err(ProjectObservationBuildError::Decode)?;
        let dependencies = resolve_project_dependencies_cached(root, &manifest.dependencies)
            .map_err(ProjectObservationBuildError::Dependency)?;
        let root_string = root.to_string_lossy().into_owned();
        let mut sources = Vec::new();
        let mut explicit_paths = HashSet::new();
        for module in &manifest.modules {
            let paths = project_composition_paths(root, &module.entry, &module.files)
                .map_err(ProjectSourceIndexError::CompositionPlan)
                .map_err(ProjectObservationBuildError::Source)?;
            for path in paths.iter().skip(1) {
                if matches!(
                    unsafe { self.index_project_source(path) }
                        .map_err(ProjectObservationBuildError::Source)?,
                    ProjectSourceIndexOutcome::Metadata(_)
                ) {
                    return Err(ProjectObservationBuildError::Source(
                        ProjectSourceIndexError::Composition(
                            ProjectCompositionError::PartDeclaration(path.clone()),
                        ),
                    ));
                }
            }
            explicit_paths.extend(paths.iter().cloned());
            if let Some(source) =
                unsafe { self.observe_composed_project_source(&paths, &root_string) }
                    .map_err(ProjectObservationBuildError::Source)?
            {
                sources.push(source);
            }
        }
        let files = collect_project_source_files(root, &manifest.source_roots)
            .map_err(ProjectObservationBuildError::SourceWalk)?;
        for path in files {
            if explicit_paths.contains(&path) {
                continue;
            }
            let source_path = path.to_string_lossy().into_owned();
            if let Some(source) =
                unsafe { self.observe_project_source(&path, &root_string, &[source_path]) }
                    .map_err(ProjectObservationBuildError::Source)?
            {
                sources.push(source);
            }
        }
        validate_project_observations(&sources)
            .map_err(ProjectObservationBuildError::Observation)?;
        let main = if is_root {
            if let Some(main) = manifest.main.as_deref() {
                let main_path = root.join(main);
                if std::fs::metadata(&main_path).is_err() {
                    return Err(ProjectObservationBuildError::MissingMain(main_path));
                }
                Some(main_path.to_string_lossy().into_owned())
            } else {
                None
            }
        } else {
            None
        };
        Ok((
            ProjectObservation {
                root: root_string,
                manifest: manifest_path.to_string_lossy().into_owned(),
                name: manifest.name,
                main,
                loads: manifest.loads,
                run: manifest.run,
                wrapped: manifest.wrapped,
                dependencies: dependencies.clone(),
                sources,
                test_sources: manifest.test_sources,
            },
            dependencies,
        ))
    }

    unsafe fn observe_project_recursive(
        &self,
        root: &Path,
        is_root: bool,
        visiting: &mut HashSet<PathBuf>,
        visited: &mut HashSet<PathBuf>,
        projects: &mut Vec<ProjectObservation>,
    ) -> Result<(), ProjectObservationBuildError> {
        if visited.contains(root) {
            return Ok(());
        }
        if !visiting.insert(root.to_path_buf()) {
            return Err(ProjectObservationBuildError::DependencyCycle(
                root.to_path_buf(),
            ));
        }
        let result = (|| {
            let (project, dependencies) = unsafe { self.observe_project_at(root, is_root) }?;
            projects.push(project);
            for dependency in dependencies {
                let dependency_root = PathBuf::from(dependency.root);
                unsafe {
                    self.observe_project_recursive(
                        &dependency_root,
                        false,
                        visiting,
                        visited,
                        projects,
                    )?
                };
            }
            Ok::<(), ProjectObservationBuildError>(())
        })();
        visiting.remove(root);
        if result.is_ok() {
            visited.insert(root.to_path_buf());
        }
        result
    }

    /// Observe the root project and its canonical dependency closure into the
    /// pointer-free snapshot consumed by `ProjectPolicy`. Rust owns only
    /// filesystem reads, Codegen metadata, and deterministic ordering; TOML
    /// parsing and provider/policy decisions remain Birnam-owned.
    ///
    /// # Safety
    ///
    /// The caller must keep a live GC anchor and remain on the floor owner
    /// thread for every parse, composition, and metadata send.
    pub unsafe fn observe_project_snapshot(
        &self,
        start: &Path,
    ) -> Result<ProjectSnapshot, ProjectObservationBuildError> {
        let root = nearest_project_root(start).map_err(ProjectObservationBuildError::Discovery)?;
        let mut visiting = HashSet::new();
        let mut visited = HashSet::new();
        let mut projects = Vec::new();
        unsafe {
            self.observe_project_recursive(&root, true, &mut visiting, &mut visited, &mut projects)?
        };
        Ok(ProjectSnapshot {
            root: root.to_string_lossy().into_owned(),
            projects,
        })
    }

    /// Run the compile-only project check over a policy-selected resolution.
    /// Each module is compiled into a private temporary directory; no output
    /// is activated or published, and compiler failures are returned as owned
    /// diagnostics rather than crossing the host boundary as live Oops.
    ///
    /// # Safety
    ///
    /// The caller must keep a live GC anchor and remain on the floor owner
    /// thread for all observation, policy, and Codegen sends.
    pub unsafe fn check_project(
        &self,
        start: &Path,
    ) -> Result<ProjectCheckResult, ProjectCheckError> {
        let snapshot = unsafe { self.observe_project_snapshot(start) }
            .map_err(ProjectCheckError::Observation)?;
        let raw_snapshot = unsafe { self.materialize_project_snapshot(&snapshot) }
            .map_err(ProjectCheckError::Snapshot)?;
        let raw_resolution = unsafe { self.resolve_project_snapshot(raw_snapshot) }
            .map_err(ProjectCheckError::Policy)?;
        let resolution = unsafe { decode_project_resolution(self.stack.api(), raw_resolution) }
            .map_err(ProjectCheckError::Resolution)?;
        let wrapped = resolution.wrapped.clone();
        let manifest = std::fs::read(&resolution.manifest).map_err(|error| {
            ProjectCheckError::ManifestRead {
                path: PathBuf::from(&resolution.manifest),
                kind: error.kind(),
            }
        })?;
        let resolution_digest = project_build_resolution_digest(
            &resolution.project_root,
            &manifest,
            &resolution.modules,
            &wrapped,
        );
        let root_observation = snapshot
            .projects
            .iter()
            .find(|project| project.root == resolution.project_root);
        let test_sources = root_observation.and_then(|project| project.test_sources.clone());
        let temporary = create_check_temp_directory()
            .map_err(|kind| ProjectCheckError::TempDirectory { kind })?;
        let api = self.stack.api();
        let compile_selector = unsafe { (api.intern)(c"compile:output:root:".as_ptr()) };
        let last_error_selector = unsafe { (api.intern)(c"lastError".as_ptr()) };
        let mut diagnostics = Vec::new();
        for (index, module) in resolution.modules.iter().enumerate() {
            let output = temporary.0.join(format!("module-{index}.dylib"));
            let source = unsafe { (birnam_abi::string::make)(module.source.as_ptr(), module.source.len()) };
            let output_bytes = path_bytes(&output);
            let output = unsafe { (birnam_abi::string::make)(output_bytes.as_ptr(), output_bytes.len()) };
            let root_bytes = module.project_root.as_bytes();
            let root = unsafe { (birnam_abi::string::make)(root_bytes.as_ptr(), root_bytes.len()) };
            let result = unsafe {
                (api.send)(
                    self.codegen,
                    compile_selector,
                    [source, output, root].as_ptr(),
                    3,
                )
            };
            if result == api.nilObj || send_is_error(api, result) {
                let detail_value =
                    unsafe { (api.send)(self.codegen, last_error_selector, std::ptr::null(), 0) };
                let detail = if detail_value != api.nilObj
                    && unsafe { (api.vtableOf)(detail_value) } == unsafe { birnam_abi::string::behavior() }
                {
                    let mut length = 0usize;
                    let bytes = unsafe { (birnam_abi::string::bytes)(detail_value, &mut length) };
                    if bytes.is_null() {
                        "compiler did not provide a diagnostic".to_owned()
                    } else {
                        String::from_utf8_lossy(unsafe {
                            std::slice::from_raw_parts(bytes, length)
                        })
                        .into_owned()
                    }
                } else {
                    "compiler did not provide a diagnostic".to_owned()
                };
                diagnostics.push(parse_project_compile_diagnostic(
                    &module.name,
                    &module.path,
                    &detail,
                ));
            }
        }
        drop(temporary);
        Ok(ProjectCheckResult {
            resolution,
            resolution_digest,
            wrapped,
            test_sources,
            diagnostics,
        })
    }

    /// Compile one observed module into a caller-owned staging path. The
    /// Codegen result is read back and checked against the module's inert
    /// descriptor before its checksum is returned; activation and publication
    /// remain separate host operations.
    ///
    /// # Safety
    ///
    /// The caller must keep a live GC anchor and remain on the floor owner
    /// thread for the Codegen send. `output` must be an isolated staging path
    /// whose parent already exists.
    pub unsafe fn compile_project_artifact(
        &self,
        module: &ProjectSourceObservation,
        output: &Path,
    ) -> Result<String, ProjectArtifactCompileError> {
        let api = self.stack.api();
        let selector = unsafe { (api.intern)(c"compile:output:root:".as_ptr()) };
        let source = unsafe { (birnam_abi::string::make)(module.source.as_ptr(), module.source.len()) };
        let output_bytes = path_bytes(output);
        let output_value = unsafe { (birnam_abi::string::make)(output_bytes.as_ptr(), output_bytes.len()) };
        let root_value = unsafe {
            (birnam_abi::string::make)(
                module.project_root.as_bytes().as_ptr(),
                module.project_root.len(),
            )
        };
        let result = unsafe {
            (api.send)(
                self.codegen,
                selector,
                [source, output_value, root_value].as_ptr(),
                3,
            )
        };
        if result == api.nilObj || send_is_error(api, result) {
            let last_error_selector = unsafe { (api.intern)(c"lastError".as_ptr()) };
            let detail_value =
                unsafe { (api.send)(self.codegen, last_error_selector, std::ptr::null(), 0) };
            let detail = if detail_value != api.nilObj
                && unsafe { (api.vtableOf)(detail_value) } == unsafe { birnam_abi::string::behavior() }
            {
                let mut length = 0usize;
                let bytes = unsafe { (birnam_abi::string::bytes)(detail_value, &mut length) };
                if bytes.is_null() {
                    "compiler did not provide a diagnostic".to_owned()
                } else {
                    String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(bytes, length) })
                        .into_owned()
                }
            } else {
                "compiler did not provide a diagnostic".to_owned()
            };
            return Err(ProjectArtifactCompileError::CodegenRejected {
                module: module.name.clone(),
                detail,
            });
        }
        let bytes =
            std::fs::read(output).map_err(|error| ProjectArtifactCompileError::ReadOutput {
                path: output.to_path_buf(),
                kind: error.kind(),
            })?;
        validate_project_compiled_artifact(module, &bytes)
            .map_err(ProjectArtifactCompileError::Descriptor)?;
        Ok(String::from_utf8_lossy(&sha256_hex(&bytes)).into_owned())
    }

    unsafe fn observe_generated_project_source(
        &self,
        source: Vec<u8>,
        label: &Path,
        project_root: &str,
    ) -> Result<ProjectSourceObservation, ProjectSourceIndexError> {
        if source.len() > MAX_SCRIPT_BYTES {
            return Err(ProjectSourceIndexError::TooLarge {
                path: label.to_path_buf(),
                limit: MAX_SCRIPT_BYTES,
            });
        }
        let api = self.stack.api();
        let codegen = unsafe { global_named(api, "Codegen") }
            .ok_or(ProjectSourceIndexError::MissingCodegen)?;
        let source_value = unsafe { (birnam_abi::string::make)(source.as_ptr(), source.len()) };
        let selector = unsafe { (api.intern)(c"moduleMetadataOf:".as_ptr()) };
        let metadata = unsafe { (api.send)(codegen, selector, [source_value].as_ptr(), 1) };
        if metadata == api.nilObj || send_is_error(api, metadata) {
            return Err(ProjectSourceIndexError::MetadataRejected);
        }
        let metadata = unsafe { decode_project_source_metadata(api, metadata) }
            .map_err(ProjectSourceIndexError::MetadataMalformed)?;
        Ok(ProjectSourceObservation {
            name: metadata.name,
            path: label.to_string_lossy().into_owned(),
            sources: vec![label.to_string_lossy().into_owned()],
            project_root: project_root.to_owned(),
            source,
            requires: metadata.requires,
            source_digest: metadata.source_digest,
            revision_digest: metadata.revision_digest,
        })
    }

    /// Materialize module decisions into an isolated staging directory. A
    /// compile action invokes Codegen; a reuse action copies the prior artifact
    /// only after re-reading and validating its descriptor and checksum. The
    /// final build directory remains untouched by this method.
    ///
    /// # Safety
    ///
    /// The caller must keep a live GC anchor and remain on the floor owner
    /// thread for compile actions. `stage_root` must be a disposable staging
    /// directory and must not be the published build root.
    pub unsafe fn stage_project_artifacts(
        &self,
        request: &ProjectArtifactStageRequest<'_>,
    ) -> Result<Vec<ProjectStagedArtifact>, ProjectArtifactStageError> {
        std::fs::create_dir_all(request.stage_root.join("lib")).map_err(|error| {
            ProjectArtifactStageError::CreateDirectory {
                path: request.stage_root.join("lib"),
                kind: error.kind(),
            }
        })?;
        if request.plan.artifacts.len() != request.resolution.modules.len() {
            return Err(ProjectArtifactStageError::ArtifactCount {
                expected: request.resolution.modules.len(),
                actual: request.plan.artifacts.len(),
            });
        }
        let mut staged = Vec::with_capacity(request.plan.artifacts.len());
        for (index, (module, plan)) in request
            .resolution
            .modules
            .iter()
            .zip(&request.plan.artifacts)
            .enumerate()
        {
            let Some(relative) = safe_artifact_relative_path(&plan.path) else {
                return Err(ProjectArtifactStageError::InvalidPlanPath {
                    index,
                    path: plan.path.clone(),
                });
            };
            let destination = request.stage_root.join(relative);
            let checksum = match plan.action.as_str() {
                "compile" => unsafe { self.compile_project_artifact(module, &destination) }
                    .map_err(ProjectArtifactStageError::Compile)?,
                "reuse" => {
                    let Some(previous) = request.previous else {
                        return Err(ProjectArtifactStageError::MissingPrevious {
                            name: module.name.clone(),
                        });
                    };
                    let Some(previous_root) = request.previous_root else {
                        return Err(ProjectArtifactStageError::MissingPreviousRoot {
                            name: module.name.clone(),
                        });
                    };
                    let Some(old) = previous_artifact(previous, &module.name) else {
                        return Err(ProjectArtifactStageError::MissingPrevious {
                            name: module.name.clone(),
                        });
                    };
                    let Some(old_relative) = safe_artifact_relative_path(&old.path) else {
                        return Err(ProjectArtifactStageError::InvalidPlanPath {
                            index,
                            path: old.path.clone(),
                        });
                    };
                    let source = previous_root.join(old_relative);
                    std::fs::copy(&source, &destination).map_err(|error| {
                        ProjectArtifactStageError::Copy {
                            source: source.clone(),
                            destination: destination.clone(),
                            kind: error.kind(),
                        }
                    })?;
                    let bytes = std::fs::read(&destination).map_err(|error| {
                        ProjectArtifactStageError::Read {
                            path: destination.clone(),
                            kind: error.kind(),
                        }
                    })?;
                    validate_project_compiled_artifact(module, &bytes)
                        .map_err(ProjectArtifactStageError::Descriptor)?;
                    let checksum = String::from_utf8_lossy(&sha256_hex(&bytes)).into_owned();
                    let Some(expected) = plan.checksum.as_deref() else {
                        return Err(ProjectArtifactStageError::ChecksumMismatch {
                            name: module.name.clone(),
                            expected: "present".to_owned(),
                            actual: "missing".to_owned(),
                        });
                    };
                    if checksum != expected {
                        return Err(ProjectArtifactStageError::ChecksumMismatch {
                            name: module.name.clone(),
                            expected: expected.to_owned(),
                            actual: checksum,
                        });
                    }
                    checksum
                }
                action => {
                    return Err(ProjectArtifactStageError::InvalidAction {
                        index,
                        action: action.to_owned(),
                    });
                }
            };
            staged.push(ProjectStagedArtifact {
                name: module.name.clone(),
                path: plan.path.clone(),
                action: plan.action.clone(),
                checksum,
            });
        }
        Ok(staged)
    }

    /// Execute policy-ordered wrapped native build commands directly (never
    /// through a shell) from each declaring project, expand the frozen
    /// placeholders, and copy every produced dylib into the shared module
    /// library area of the same disposable stage.
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
                    detail.truncate(16 << 20);
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

    /// Ask Birnam policy for a side-effect-free artifact plan. The request
    /// carries only typed resolution metadata; no files are compiled, loaded,
    /// or published by this boundary.
    ///
    /// # Safety
    ///
    /// The caller must keep a live GC anchor and remain on the floor owner
    /// thread for all map/vector allocation and policy sends.
    pub unsafe fn plan_project_artifacts(
        &self,
        request: &ProjectArtifactPlanRequest<'_>,
    ) -> Result<ProjectArtifactPlan, ProjectArtifactPlanError> {
        let builder = unsafe { ProjectSnapshotBuilder::new(self.stack.api()) }
            .map_err(ProjectArtifactPlanError::Snapshot)?;
        let snapshot = unsafe { builder.artifact_plan_input(request) }
            .map_err(ProjectArtifactPlanError::Snapshot)?;
        unsafe { self.load_project_policy() }.map_err(ProjectArtifactPlanError::Policy)?;
        let api = self.stack.api();
        let Some(policy) = (unsafe { global_named(api, "ProjectPolicy") }) else {
            return Err(ProjectArtifactPlanError::Policy(
                ProjectPolicyError::MissingPolicy,
            ));
        };
        let selector = unsafe { (api.intern)(c"artifactPlan:".as_ptr()) };
        let plan = unsafe { (api.send)(policy, selector, [snapshot].as_ptr(), 1) };
        if plan == api.nilObj || send_is_error(api, plan) {
            return Err(ProjectArtifactPlanError::Policy(
                ProjectPolicyError::ResolutionRejected,
            ));
        }
        let plan = unsafe { decode_project_artifact_plan(api, plan) }
            .map_err(ProjectArtifactPlanError::Decode)?;
        validate_project_artifact_plan(&plan, request)
            .map_err(ProjectArtifactPlanError::Validation)?;
        Ok(plan)
    }

    fn observe_previous_project_artifacts(
        &self,
        final_root: &Path,
        resolution: &ProjectResolutionObservation,
        target: &str,
        profile: &str,
        tool_version: &str,
    ) -> Option<ProjectPreviousPublication> {
        let mut artifacts = Vec::new();
        for module in &resolution.modules {
            let relative = format!("lib/libbirnam_{}.dylib", module.name);
            let path = final_root.join(&relative);
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let Ok(descriptor) = birnam_artifact::inspect(&bytes) else {
                continue;
            };
            if descriptor.kind != b"module" || descriptor.identity != module.name.as_bytes() {
                continue;
            }
            let checksum = String::from_utf8_lossy(&sha256_hex(&bytes)).into_owned();
            artifacts.push(ProjectPreviousPublishedArtifact {
                name: String::from_utf8_lossy(descriptor.identity).into_owned(),
                path: relative,
                source: module.path.clone(),
                sources: module.sources.clone(),
                source_digest: String::from_utf8_lossy(descriptor.source_digest).into_owned(),
                revision_digest: String::from_utf8_lossy(descriptor.revision_digest).into_owned(),
                requires: descriptor
                    .requires
                    .iter()
                    .map(|value| String::from_utf8_lossy(value).into_owned())
                    .collect(),
                checksum: checksum.clone(),
                observed_checksum: Some(checksum),
            });
        }
        if artifacts.is_empty() {
            return None;
        }
        Some(ProjectPreviousPublication {
            project_root: resolution.project_root.clone(),
            target: target.to_owned(),
            profile: profile.to_owned(),
            tool_version: tool_version.to_owned(),
            artifact_abi: String::from_utf8_lossy(birnam_artifact::ABI).into_owned(),
            artifacts,
        })
    }

    unsafe fn stage_generated_project_artifact(
        &self,
        module: &ProjectSourceObservation,
        previous: &Path,
        output: &Path,
    ) -> Result<ProjectStagedArtifact, ProjectBuildError> {
        if let Ok(bytes) = std::fs::read(previous)
            && validate_project_compiled_artifact(module, &bytes).is_ok()
        {
            std::fs::copy(previous, output).map_err(|error| {
                ProjectBuildError::Stage(ProjectArtifactStageError::Copy {
                    source: previous.to_path_buf(),
                    destination: output.to_path_buf(),
                    kind: error.kind(),
                })
            })?;
            return Ok(ProjectStagedArtifact {
                name: module.name.clone(),
                path: output.to_string_lossy().into_owned(),
                action: "reuse".to_owned(),
                checksum: String::from_utf8_lossy(&sha256_hex(&bytes)).into_owned(),
            });
        }
        let checksum = unsafe { self.compile_project_artifact(module, output) }
            .map_err(|error| ProjectBuildError::Stage(ProjectArtifactStageError::Compile(error)))?;
        Ok(ProjectStagedArtifact {
            name: module.name.clone(),
            path: output.to_string_lossy().into_owned(),
            action: "compile".to_owned(),
            checksum,
        })
    }

    /// Execute the Rust-hosted core project build. Observation and policy
    /// remain ahead of every effect; the stage is removed on all failures and
    /// only a complete module/native closure is published atomically.
    ///
    /// # Safety
    ///
    /// The caller must keep a live GC anchor and remain on the floor owner
    /// thread for all observation, policy, and Codegen sends.
    pub unsafe fn build_project(
        &self,
        request: &ProjectBuildRequest<'_>,
    ) -> Result<ProjectBuildResult, ProjectBuildError> {
        let snapshot = unsafe { self.observe_project_snapshot(request.start) }
            .map_err(ProjectBuildError::Observation)?;
        let raw_snapshot = unsafe { self.materialize_project_snapshot(&snapshot) }
            .map_err(ProjectBuildError::Snapshot)?;
        let raw_resolution = unsafe { self.resolve_project_snapshot(raw_snapshot) }
            .map_err(ProjectBuildError::Policy)?;
        let resolution = unsafe { decode_project_resolution(self.stack.api(), raw_resolution) }
            .map_err(ProjectBuildError::Resolution)?;
        let wrapped = resolution.wrapped.clone();
        let manifest = std::fs::read(&resolution.manifest).map_err(|error| {
            ProjectBuildError::ManifestRead {
                path: PathBuf::from(&resolution.manifest),
                kind: error.kind(),
            }
        })?;
        let resolution_digest = project_build_resolution_digest(
            &resolution.project_root,
            &manifest,
            &resolution.modules,
            &wrapped,
        );
        let build_root = PathBuf::from(&resolution.project_root).join(".birnam/build");
        let target_root = build_root.join(request.target);
        std::fs::create_dir_all(&target_root).map_err(|error| {
            ProjectBuildError::StageDirectory {
                path: target_root.clone(),
                kind: error.kind(),
            }
        })?;
        let final_root = target_root.join(request.profile);
        let previous = self.observe_previous_project_artifacts(
            &final_root,
            &resolution,
            request.target,
            request.profile,
            request.tool_version,
        );
        let plan_request = ProjectArtifactPlanRequest {
            resolution: &resolution,
            previous: previous.as_ref(),
            target: request.target,
            profile: request.profile,
            tool_version: request.tool_version,
        };
        let plan = unsafe { self.plan_project_artifacts(&plan_request) }
            .map_err(ProjectBuildError::Plan)?;
        let process = std::process::id();
        let stage_root = (0..128u32)
            .map(|attempt| target_root.join(format!(".staging-{process}-{attempt}")))
            .find(|path| !path.exists())
            .ok_or(ProjectBuildError::StageDirectory {
                path: target_root.clone(),
                kind: io::ErrorKind::AlreadyExists,
            })?;
        std::fs::create_dir(&stage_root).map_err(|error| ProjectBuildError::StageDirectory {
            path: stage_root.clone(),
            kind: error.kind(),
        })?;
        let result = (|| {
            let stage_request = ProjectArtifactStageRequest {
                resolution: &resolution,
                plan: &plan,
                previous: previous.as_ref(),
                previous_root: previous.as_ref().map(|_| final_root.as_path()),
                stage_root: &stage_root,
            };
            let artifacts = unsafe { self.stage_project_artifacts(&stage_request) }
                .map_err(ProjectBuildError::Stage)?;
            self.stage_project_wrapped(&wrapped, &stage_root)
                .map_err(ProjectBuildError::Wrapped)?;
            let (module_identity, module_source) = project_module_root_source(
                &resolution,
                &resolution_digest,
                request.target,
                request.profile,
                request.tool_version,
            );
            let module_label =
                PathBuf::from(&resolution.project_root).join("<generated-project-modules>");
            let module = unsafe {
                self.observe_generated_project_source(
                    module_source,
                    &module_label,
                    &resolution.project_root,
                )
            }
            .map_err(ProjectBuildError::GeneratedSource)?;
            let staged_module_root = stage_root.join("lib").join(PROJECT_MODULE_ROOT_FILENAME);
            unsafe {
                self.stage_generated_project_artifact(
                    &module,
                    &final_root.join("lib").join(PROJECT_MODULE_ROOT_FILENAME),
                    &staged_module_root,
                )
            }?;
            let canonical_module_root = stage_root
                .join("lib")
                .join(format!("libbirnam_{module_identity}.dylib"));
            std::fs::hard_link(&staged_module_root, &canonical_module_root).map_err(|error| {
                ProjectBuildError::StageDirectory {
                    path: canonical_module_root.clone(),
                    kind: error.kind(),
                }
            })?;

            let (configuration_identity, configuration_source) = project_configuration_source(
                &resolution,
                &resolution_digest,
                request.target,
                request.profile,
                request.tool_version,
            );
            let configuration_label =
                PathBuf::from(&resolution.project_root).join("<generated-project-configuration>");
            let configuration = unsafe {
                self.observe_generated_project_source(
                    configuration_source,
                    &configuration_label,
                    &resolution.project_root,
                )
            }
            .map_err(ProjectBuildError::GeneratedSource)?;
            let staged_configuration = stage_root.join("lib").join(PROJECT_CONFIGURATION_FILENAME);
            unsafe {
                self.stage_generated_project_artifact(
                    &configuration,
                    &final_root.join("lib").join(PROJECT_CONFIGURATION_FILENAME),
                    &staged_configuration,
                )
            }?;
            let canonical_configuration = stage_root
                .join("lib")
                .join(format!("libbirnam_{configuration_identity}.dylib"));
            std::fs::hard_link(&staged_configuration, &canonical_configuration).map_err(
                |error| ProjectBuildError::StageDirectory {
                    path: canonical_configuration.clone(),
                    kind: error.kind(),
                },
            )?;

            let staged_application = if let Some(main_path) = resolution.main.as_deref() {
                let main_path = PathBuf::from(main_path);
                let main =
                    std::fs::read(&main_path).map_err(|error| ProjectBuildError::MainRead {
                        path: main_path.clone(),
                        kind: error.kind(),
                    })?;
                let (_, application_source) = project_application_source(
                    &resolution,
                    &configuration_identity,
                    &resolution_digest,
                    request.target,
                    request.profile,
                    request.tool_version,
                    &main,
                );
                let application_label =
                    PathBuf::from(&resolution.project_root).join("<generated-project-application>");
                let application = unsafe {
                    self.observe_generated_project_source(
                        application_source,
                        &application_label,
                        &resolution.project_root,
                    )
                }
                .map_err(ProjectBuildError::GeneratedSource)?;
                let output = stage_root.join("lib").join(PROJECT_APPLICATION_FILENAME);
                unsafe {
                    self.stage_generated_project_artifact(
                        &application,
                        &final_root.join("lib").join(PROJECT_APPLICATION_FILENAME),
                        &output,
                    )
                }?;
                Some(output)
            } else {
                None
            };
            let publication_root = staged_application
                .as_deref()
                .unwrap_or(staged_module_root.as_path());
            preflight_artifact(publication_root, None).map_err(ProjectBuildError::Publication)?;
            preflight_artifact(&staged_module_root, None)
                .map_err(ProjectBuildError::Publication)?;
            publish_project_build(&stage_root, &final_root, &target_root)
                .map_err(ProjectBuildError::Publish)?;
            Ok(ProjectBuildResult {
                output_root: final_root.clone(),
                module_root: final_root.join("lib").join(PROJECT_MODULE_ROOT_FILENAME),
                configuration: final_root.join("lib").join(PROJECT_CONFIGURATION_FILENAME),
                application: staged_application
                    .map(|_| final_root.join("lib").join(PROJECT_APPLICATION_FILENAME)),
                artifacts,
            })
        })();
        if result.is_err() {
            let _ = std::fs::remove_dir_all(&stage_root);
        }
        result
    }

    /// Locate an already-published project without compiling, observing source,
    /// or consulting an external receipt. Each consumer preflights its selected
    /// root before activation or installation: run selects the application,
    /// tests select the module root, and install selects their union. Discovery
    /// does not preflight a second time or require tests to inspect the app.
    ///
    /// # Safety
    ///
    /// The caller must keep a live GC anchor and remain on the floor owner
    /// thread for subsequent activation.
    pub unsafe fn locate_project_build(
        &self,
        start: &Path,
        target: &str,
        profile: &str,
        _tool_version: &str,
    ) -> Result<ProjectBuildResult, ProjectBuildError> {
        let (final_root, module_root, configuration, application_path) =
            project_publication_paths(start, target, profile).map_err(|error| {
                ProjectBuildError::Observation(ProjectObservationBuildError::Discovery(error))
            })?;
        let application = if application_path.exists() {
            Some(application_path)
        } else {
            None
        };
        Ok(ProjectBuildResult {
            output_root: final_root,
            module_root,
            configuration,
            application,
            artifacts: Vec::new(),
        })
    }

    /// Install a verified build's exact module/native set into the resolved
    /// runtime library directory. Files are copied and synced in a private
    /// staging directory, then renamed individually; no source compilation or
    /// runtime activation occurs here.
    pub fn install_project_build(
        &self,
        build: &ProjectBuildResult,
    ) -> Result<usize, ProjectInstallError> {
        std::fs::create_dir_all(&self.resources.lib_dir).map_err(|error| {
            ProjectInstallError::CreateStage {
                path: self.resources.lib_dir.clone(),
                kind: error.kind(),
            }
        })?;
        let process = std::process::id();
        let stage = (0..128u32)
            .map(|attempt| {
                self.resources
                    .lib_dir
                    .join(format!(".birnam-install-{process}-{attempt}"))
            })
            .find(|path| !path.exists())
            .ok_or(ProjectInstallError::CreateStage {
                path: self.resources.lib_dir.clone(),
                kind: io::ErrorKind::AlreadyExists,
            })?;
        std::fs::create_dir(&stage).map_err(|error| ProjectInstallError::CreateStage {
            path: stage.clone(),
            kind: error.kind(),
        })?;
        let result = (|| {
            let mut plans = vec![
                preflight_artifact(&build.module_root, None)
                    .map_err(ProjectInstallError::Preflight)?,
            ];
            if let Some(application) = &build.application {
                plans.push(
                    preflight_artifact(application, None)
                        .map_err(ProjectInstallError::Preflight)?,
                );
            }
            let mut entries = Vec::new();
            let mut filenames = HashSet::new();
            for plan in plans {
                for artifact in plan.entries {
                    let identity = String::from_utf8_lossy(&artifact.identity).into_owned();
                    let filename = format!("libbirnam_{identity}.dylib");
                    if filenames.insert(filename.clone()) {
                        entries.push((identity, artifact.path, filename));
                    }
                }
                for native in plan.native {
                    let Some(filename) = native.file_name().and_then(|value| value.to_str()) else {
                        return Err(ProjectInstallError::InvalidArtifactPath {
                            name: "native".to_owned(),
                            path: native.to_string_lossy().into_owned(),
                        });
                    };
                    if filenames.insert(filename.to_owned()) {
                        entries.push((filename.to_owned(), native.clone(), filename.to_owned()));
                    }
                }
            }
            let mut sources = Vec::with_capacity(entries.len());
            for (_name, source, filename) in entries {
                let destination = stage.join(&filename);
                let bytes =
                    std::fs::read(&source).map_err(|error| ProjectInstallError::ReadSource {
                        path: source.clone(),
                        kind: error.kind(),
                    })?;
                std::fs::write(&destination, &bytes).map_err(|error| {
                    ProjectInstallError::CopyStage {
                        source: source.clone(),
                        destination: destination.clone(),
                        kind: error.kind(),
                    }
                })?;
                let file = std::fs::OpenOptions::new()
                    .read(true)
                    .open(&destination)
                    .map_err(|error| ProjectInstallError::CopyStage {
                        source: source.clone(),
                        destination: destination.clone(),
                        kind: error.kind(),
                    })?;
                file.sync_all()
                    .map_err(|error| ProjectInstallError::SyncStage {
                        path: destination.clone(),
                        kind: error.kind(),
                    })?;
                sources.push(destination);
            }
            for source in &sources {
                let Some(filename) = source.file_name() else {
                    return Err(ProjectInstallError::Publish {
                        source: source.clone(),
                        destination: self.resources.lib_dir.clone(),
                        kind: io::ErrorKind::InvalidInput,
                    });
                };
                let destination = self.resources.lib_dir.join(filename);
                std::fs::rename(source, &destination).map_err(|error| {
                    ProjectInstallError::Publish {
                        source: source.clone(),
                        destination: destination.clone(),
                        kind: error.kind(),
                    }
                })?;
            }
            Ok(sources.len())
        })();
        let _ = std::fs::remove_dir_all(&stage);
        result
    }

    /// Read one source file and ask Codegen for its static module metadata.
    /// This is observation only: no source evaluation, Module installation, or
    /// provider selection occurs here.
    ///
    /// # Safety
    ///
    /// The returned metadata Oop is owned by this runtime's Store. Keep a live
    /// GC anchor and remain on the floor owner thread while using it.
    pub unsafe fn index_project_source(
        &self,
        path: &Path,
    ) -> Result<ProjectSourceIndexOutcome, ProjectSourceIndexError> {
        let source = std::fs::read(path).map_err(|error| ProjectSourceIndexError::Read {
            path: path.to_path_buf(),
            kind: error.kind(),
        })?;
        if source.len() > MAX_SCRIPT_BYTES {
            return Err(ProjectSourceIndexError::TooLarge {
                path: path.to_path_buf(),
                limit: MAX_SCRIPT_BYTES,
            });
        }
        let api = self.stack.api();
        let codegen = unsafe { global_named(api, "Codegen") }
            .ok_or(ProjectSourceIndexError::MissingCodegen)?;
        let source = unsafe { (birnam_abi::string::make)(source.as_ptr(), source.len()) };
        let selector = unsafe { (api.intern)(c"moduleMetadataOf:".as_ptr()) };
        let metadata = unsafe { (api.send)(codegen, selector, [source].as_ptr(), 1) };
        if metadata == api.nilObj {
            return Ok(ProjectSourceIndexOutcome::Ignored);
        }
        if send_is_error(api, metadata) {
            return Err(ProjectSourceIndexError::MetadataRejected);
        }
        Ok(ProjectSourceIndexOutcome::Metadata(metadata))
    }

    /// Combine static metadata with the host-owned source/path composition
    /// fields needed by `ProjectPolicy` observations. Source bytes are retained
    /// verbatim so non-UTF-8 authored content is not silently rewritten.
    ///
    /// # Safety
    ///
    /// The returned record borrows no runtime Oops. The caller must still keep
    /// the owner-thread/GC anchor live during the underlying metadata send.
    pub unsafe fn observe_project_source(
        &self,
        path: &Path,
        project_root: &str,
        composition: &[String],
    ) -> Result<Option<ProjectSourceObservation>, ProjectSourceIndexError> {
        let source = std::fs::read(path).map_err(|error| ProjectSourceIndexError::Read {
            path: path.to_path_buf(),
            kind: error.kind(),
        })?;
        if source.len() > MAX_SCRIPT_BYTES {
            return Err(ProjectSourceIndexError::TooLarge {
                path: path.to_path_buf(),
                limit: MAX_SCRIPT_BYTES,
            });
        }
        let api = self.stack.api();
        let codegen = unsafe { global_named(api, "Codegen") }
            .ok_or(ProjectSourceIndexError::MissingCodegen)?;
        let source_value = unsafe { (birnam_abi::string::make)(source.as_ptr(), source.len()) };
        let selector = unsafe { (api.intern)(c"moduleMetadataOf:".as_ptr()) };
        let metadata = unsafe { (api.send)(codegen, selector, [source_value].as_ptr(), 1) };
        if metadata == api.nilObj {
            return Ok(None);
        }
        if send_is_error(api, metadata) {
            return Err(ProjectSourceIndexError::MetadataRejected);
        }
        let metadata = unsafe { decode_project_source_metadata(api, metadata) }
            .map_err(ProjectSourceIndexError::MetadataMalformed)?;
        Ok(Some(ProjectSourceObservation {
            name: metadata.name,
            path: path.to_string_lossy().into_owned(),
            sources: composition.to_vec(),
            project_root: project_root.to_owned(),
            source,
            requires: metadata.requires,
            source_digest: metadata.source_digest,
            revision_digest: metadata.revision_digest,
        }))
    }

    /// Observe one explicit module entry and its ordered source parts after
    /// Birnam Module composition. The first path is the provider identity;
    /// every path is retained in the observation's `sources` list.
    ///
    /// # Safety
    ///
    /// The returned record owns copied bytes and strings. The runtime must
    /// remain on the owner thread for composition and metadata sends.
    pub unsafe fn observe_composed_project_source(
        &self,
        paths: &[PathBuf],
        project_root: &str,
    ) -> Result<Option<ProjectSourceObservation>, ProjectSourceIndexError> {
        let source = unsafe { self.compose_project_sources(paths) }
            .map_err(ProjectSourceIndexError::Composition)?;
        let api = self.stack.api();
        let codegen = unsafe { global_named(api, "Codegen") }
            .ok_or(ProjectSourceIndexError::MissingCodegen)?;
        let source_value = unsafe { (birnam_abi::string::make)(source.as_ptr(), source.len()) };
        let selector = unsafe { (api.intern)(c"moduleMetadataOf:".as_ptr()) };
        let metadata = unsafe { (api.send)(codegen, selector, [source_value].as_ptr(), 1) };
        if metadata == api.nilObj {
            return Ok(None);
        }
        if send_is_error(api, metadata) {
            return Err(ProjectSourceIndexError::MetadataRejected);
        }
        let metadata = unsafe { decode_project_source_metadata(api, metadata) }
            .map_err(ProjectSourceIndexError::MetadataMalformed)?;
        let path = paths
            .first()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(Some(ProjectSourceObservation {
            name: metadata.name,
            path,
            sources: paths
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect(),
            project_root: project_root.to_owned(),
            source,
            requires: metadata.requires,
            source_digest: metadata.source_digest,
            revision_digest: metadata.revision_digest,
        }))
    }

    /// Ask the Birnam Module manager to compose ordered source parts. The
    /// manager owns file reading and composition semantics; Rust only builds
    /// the path vector and copies the returned source bytes.
    ///
    /// # Safety
    ///
    /// The returned bytes are an owned Rust copy. The runtime must remain on
    /// the floor owner thread for the synchronous Module send.
    pub unsafe fn compose_project_sources(
        &self,
        paths: &[PathBuf],
    ) -> Result<Vec<u8>, ProjectCompositionError> {
        let api = self.stack.api();
        let module =
            unsafe { global_named(api, "Module") }.ok_or(ProjectCompositionError::MissingModule)?;
        let vector_class = unsafe { (api.import)(c"Vector".as_ptr()) };
        if vector_class == api.nilObj {
            return Err(ProjectCompositionError::MissingVector);
        }
        let new_selector = unsafe { (api.intern)(c"new".as_ptr()) };
        let add_selector = unsafe { (api.intern)(c"add:".as_ptr()) };
        let mut source_paths =
            unsafe { (api.send)(vector_class, new_selector, std::ptr::null(), 0) };
        if source_paths == api.nilObj || send_is_error(api, source_paths) {
            return Err(ProjectCompositionError::AllocationRejected);
        }
        for path in paths {
            let bytes = path_bytes(path);
            let value = unsafe { (birnam_abi::string::make)(bytes.as_ptr(), bytes.len()) };
            let updated = unsafe { (api.send)(source_paths, add_selector, [value].as_ptr(), 1) };
            if updated == api.nilObj || send_is_error(api, updated) {
                return Err(ProjectCompositionError::AllocationRejected);
            }
            source_paths = updated;
        }
        let selector = unsafe { (api.intern)(c"composeSources:".as_ptr()) };
        let result = unsafe { (api.send)(module, selector, [source_paths].as_ptr(), 1) };
        if send_is_error(api, result) {
            return Err(ProjectCompositionError::Rejected);
        }
        if result == api.nilObj || unsafe { (api.vtableOf)(result) } != unsafe { birnam_abi::string::behavior() } {
            return Err(ProjectCompositionError::NonString);
        }
        let mut length = 0usize;
        let bytes = unsafe { (birnam_abi::string::bytes)(result, &mut length) };
        if bytes.is_null() {
            return Err(ProjectCompositionError::NonString);
        }
        if length > MAX_SCRIPT_BYTES {
            return Err(ProjectCompositionError::TooLarge {
                limit: MAX_SCRIPT_BYTES,
            });
        }
        Ok(unsafe { std::slice::from_raw_parts(bytes, length) }.to_vec())
    }

    /// Install the host-owned `Project` bridge used by the authored LSP. The
    /// bridge delegates URI decoding, filesystem observation, and policy
    /// resolution to this runtime; it does not retain a second snapshot or
    /// project graph. The pointer is cleared when this HostRuntime drops.
    ///
    /// # Safety
    ///
    /// The caller must have prepared a live GC anchor and remain on the floor
    /// owner thread for all synchronous sends.
    pub unsafe fn install_project_bridge(&self) -> Result<(), ProjectBridgeError> {
        let api = self.stack.api();
        let runtime_pointer = self as *const HostRuntime as *mut HostRuntime;
        let owner = PROJECT_BRIDGE_RUNTIME.load(Ordering::Acquire);
        if !owner.is_null() && owner != runtime_pointer {
            return Err(ProjectBridgeError::AlreadyOwned);
        }
        let project = if let Some(project) = unsafe { global_named(api, "Project") } {
            project
        } else {
            let object = {
                let imported = unsafe { (api.import)(c"Object".as_ptr()) };
                if imported != api.nilObj {
                    imported
                } else {
                    unsafe { global_named(api, "Object") }
                        .ok_or(ProjectBridgeError::MissingObject)?
                }
            };
            let selector = unsafe { (api.intern)(c"clone".as_ptr()) };
            let project = unsafe { (api.send)(object, selector, std::ptr::null(), 0) };
            if project == api.nilObj || send_is_error(api, project) {
                return Err(ProjectBridgeError::CloneRejected);
            }
            project
        };
        let vtable = unsafe { (api.vtableOf)(project) };
        if vtable == api.nilObj {
            return Err(ProjectBridgeError::CloneRejected);
        }
        let snapshot_selector = unsafe { (api.intern)(c"snapshotForUri:".as_ptr()) };
        let path_selector = unsafe { (api.intern)(c"pathForUri:".as_ptr()) };
        unsafe {
            (api.method)(vtable, snapshot_selector, project_snapshot_for_uri_native);
            (api.method)(vtable, path_selector, project_path_for_uri_native);
        }
        let exported = unsafe { (api.exportObj)(c"Project".as_ptr(), project) };
        if exported == api.nilObj {
            return Err(ProjectBridgeError::GlobalRejected);
        }
        let globals = unsafe { (api.import)(c"Globals".as_ptr()) };
        if globals == api.nilObj {
            return Err(ProjectBridgeError::GlobalRejected);
        }
        let put_selector = unsafe { (api.intern)(c"at:put:".as_ptr()) };
        let name = unsafe { (api.intern)(c"Project".as_ptr()) };
        let result = unsafe { (api.send)(globals, put_selector, [name, project].as_ptr(), 2) };
        if send_is_error(api, result) {
            return Err(ProjectBridgeError::GlobalRejected);
        }
        if PROJECT_BRIDGE_RUNTIME
            .compare_exchange(
                std::ptr::null_mut(),
                runtime_pointer,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err_and(|owner| owner != runtime_pointer)
        {
            return Err(ProjectBridgeError::AlreadyOwned);
        }
        Ok(())
    }
}
