//! Static project observations. Codegen owns source recognition and digests;
//! Host copies its value projections and the original file bytes. No Oop escapes.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSourceMetadata {
    pub name: String,
    pub requires: Vec<String>,
    pub source_digest: String,
    pub revision_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSourceObservation {
    pub name: String,
    pub path: String,
    pub sources: Vec<String>,
    pub project_root: String,
    pub source: Vec<u8>,
    pub requires: Vec<String>,
    pub source_digest: String,
    pub revision_digest: String,
}

impl Scope<'_> {
    pub(super) fn exact(&self, value: Oop, family: &[u8]) -> Result<(), HostError> {
        let behavior = self.call(
            self.host.id.receiver,
            b"behaviorOf:",
            &[Operand::object(value)],
        )?;
        if behavior != self.host.named(family)? {
            return Err(HostError(format!(
                "project metadata expected {}",
                String::from_utf8_lossy(family)
            )));
        }
        Ok(())
    }
    pub(super) fn vector_size(&self, value: Oop) -> Result<usize, HostError> {
        self.exact(value, b"VectorVt")?;
        let count = self.call(value, b"size", &[])?;
        self.host
            .send(
                self.host.named(b"Integer")?,
                b"wordOf:",
                &[Operand::object(count)],
            )?
            .as_word()
            .filter(|n| *n <= isize::MAX as usize)
            .ok_or_else(|| HostError("project metadata has an invalid vector size".into()))
    }
    pub(super) fn vector_at(&self, value: Oop, index: usize) -> Result<Oop, HostError> {
        let index = self.call(
            self.host.named(b"Integer")?,
            b"fromWord:",
            &[Operand::word(index)],
        )?;
        self.call(value, b"at:", &[Operand::object(index)])
    }
    pub(super) fn symbol_text(&self, value: Oop) -> Result<String, HostError> {
        let bytes = self
            .host
            .send(
                self.host.id.receiver,
                b"symbolBytesOf:",
                &[Operand::object(value)],
            )?
            .as_bytes()
            .ok_or_else(|| HostError("project metadata expected a Symbol".into()))?;
        let bytes = unsafe { borrowed_bytes(bytes.pointer, bytes.length) }
            .ok_or_else(|| HostError("invalid Symbol bytes".into()))?;
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }
    pub(super) fn string_bytes(&self, value: Oop) -> Result<Vec<u8>, HostError> {
        self.exact(value, b"StringVt")?;
        let bytes = self
            .host
            .send(value, b"bytes", &[])?
            .as_bytes()
            .ok_or_else(|| HostError("project metadata expected String bytes".into()))?;
        Ok(unsafe { borrowed_bytes(bytes.pointer, bytes.length) }
            .ok_or_else(|| HostError("invalid String bytes".into()))?
            .to_vec())
    }
    pub(super) fn source_metadata(
        &self,
        source: &[u8],
    ) -> Result<Option<ProjectSourceMetadata>, HostError> {
        let source = self.string(source)?;
        let value = self.call(
            self.host.named(b"Codegen")?,
            b"moduleMetadataOf:",
            &[Operand::object(source)],
        )?;
        if value == self.host.named(b"nil")? {
            return Ok(None);
        }
        self.check_result(value)?;
        if self.vector_size(value)? != 5
            || self.symbol_text(self.vector_at(value, 0)?)? != "moduleMetadata"
        {
            return Err(HostError("invalid module metadata record".into()));
        }
        let name = self.symbol_text(self.vector_at(value, 1)?)?;
        let required = self.vector_at(value, 2)?;
        let mut requires = Vec::new();
        for index in 0..self.vector_size(required)? {
            requires.push(self.symbol_text(self.vector_at(required, index)?)?);
        }
        let source_digest =
            String::from_utf8_lossy(&self.string_bytes(self.vector_at(value, 3)?)?).into_owned();
        let revision_digest =
            String::from_utf8_lossy(&self.string_bytes(self.vector_at(value, 4)?)?).into_owned();
        Ok(Some(ProjectSourceMetadata {
            name,
            requires,
            source_digest,
            revision_digest,
        }))
    }
}

impl HostRuntime {
    /// Read and statically index one file. Unindexable source answers None,
    /// preserving Codegen's existing contract; this is not a compile check.
    pub fn index_project_source(
        &self,
        path: &Path,
    ) -> Result<Option<ProjectSourceMetadata>, HostError> {
        let source = read_source_file(path)?;
        self.scope()
            .source_metadata(&source)
            .map_err(|e| HostError(format!("{}: {e}", path.display())))
    }
    /// Observe one file snapshot without evaluating or registering the module.
    pub fn observe_project_source(
        &self,
        path: &Path,
        project_root: &str,
        composition: &[String],
    ) -> Result<Option<ProjectSourceObservation>, HostError> {
        let source = read_source_file(path)?;
        let metadata = self
            .scope()
            .source_metadata(&source)
            .map_err(|e| HostError(format!("{}: {e}", path.display())))?;
        Ok(metadata.map(|m| ProjectSourceObservation {
            name: m.name,
            path: path.to_string_lossy().into_owned(),
            sources: composition.to_vec(),
            project_root: project_root.to_owned(),
            source,
            requires: m.requires,
            source_digest: m.source_digest,
            revision_digest: m.revision_digest,
        }))
    }
    /// Copy the compiler's literal asset inventory; no asset is opened here.
    pub fn project_embedded_files(&self, path: &Path) -> Result<Vec<Vec<u8>>, HostError> {
        let source = read_source_file(path)?;
        let observe = || {
            let scope = self.scope();
            let source = scope.string(&source)?;
            let paths = scope.call(
                self.named(b"Codegen")?,
                b"embeddedFilesOf:",
                &[Operand::object(source)],
            )?;
            scope.check_result(paths)?;
            let mut result = Vec::new();
            for index in 0..scope.vector_size(paths)? {
                result.push(scope.string_bytes(scope.vector_at(paths, index)?)?);
            }
            Ok(result)
        };
        observe().map_err(|e: HostError| HostError(format!("{}: {e}", path.display())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use birnam_abi::id_protocol::{INVALID_REQUEST, NativeEntry};
    use core::ffi::c_void;
    use sha2::{Digest, Sha256};
    use std::{cell::Cell, path::PathBuf};

    struct Hook {
        id: Protocol,
        gc: Oop,
        original: NativeEntry,
        calls: Cell<usize>,
    }
    unsafe extern "C" fn collecting(
        c: *mut c_void,
        r: Oop,
        a: *const Operand,
        n: usize,
    ) -> Outcome {
        let hook = unsafe { &*c.cast::<Hook>() };
        if let Err(e) = unsafe { hook.id.send_named(hook.gc, b"collectPrecisely", &[]) } {
            return e;
        }
        hook.calls.set(hook.calls.get() + 1);
        unsafe { hook.original.method.unwrap()(hook.original.context, r, a, n) }
    }
    unsafe extern "C" fn fail(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
        Outcome::failed(INVALID_REQUEST, 909)
    }
    unsafe extern "C" fn fixed(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
        Outcome::loaded(Operand::object(c.cast()))
    }
    struct Temp(PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    #[ignore = "requires production installed modules; scripts/host-cli-test.sh"]
    fn project_source_observation_uses_live_messages() {
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
                skip_stdlib: true,
                precise_gc: true,
                source_bootstrap: false,
                tool_bootstrap: false,
            },
        )
        .unwrap();
        let temp = Temp(
            env::temp_dir().join(format!("birnam-project-observation-{}", std::process::id())),
        );
        fs::create_dir(&temp.0).unwrap();
        let path = temp.0.join("module.birnam");
        let source = br#"[Module named: #observed requires: #[#one #two]];
let shouldNeverBeDefined := 42;
[System println: "must not execute"];
let missing := [System embedFile: "missing.bin"];
let nested := [| [System embedFile: "second\nfile"]];
[System embedFile: "missing.bin"];
[Other embedFile: "ignored"];
[System embedFile: dynamicPath]
"#;
        fs::write(&path, source).unwrap();
        let scope = host.scope();
        let gc = host.named(b"GC").unwrap();
        let mut hooks = Vec::new();
        for (family, name) in [
            (b"LangVt".as_slice(), b"parse:".as_slice()),
            (b"VectorVt", b"at:"),
            (b"VectorVt", b"add:"),
            (b"StringVt", b"bytes"),
        ] {
            let vt = host.named(family).unwrap();
            let selector = unsafe { host.id.intern_name(name) }.unwrap();
            let original = host
                .send(
                    host.id.receiver,
                    b"bindingOf:in:",
                    &[Operand::object(selector), Operand::object(vt)],
                )
                .unwrap()
                .as_entry()
                .unwrap();
            let mut hook = Box::new(Hook {
                id: host.id,
                gc,
                original,
                calls: Cell::new(0),
            });
            host.send(
                vt,
                b"addMethod",
                &[
                    Operand::object(selector),
                    Operand::entry(NativeEntry::new(
                        collecting,
                        (&mut *hook as *mut Hook).cast(),
                    )),
                ],
            )
            .unwrap();
            hooks.push((vt, selector, hook));
        }
        let observation = host
            .observe_project_source(&path, "/project", &["module.birnam".into()])
            .unwrap()
            .unwrap();
        assert_eq!(observation.name, "observed");
        assert_eq!(observation.requires, ["one", "two"]);
        assert_eq!(observation.source, source);
        assert_eq!(observation.sources, ["module.birnam"]);
        assert_eq!(observation.project_root, "/project");
        assert_eq!(
            observation.source_digest,
            hex::encode(Sha256::digest(source))
        );
        assert_eq!(
            host.index_project_source(&path)
                .unwrap()
                .unwrap()
                .revision_digest,
            observation.revision_digest
        );
        assert_eq!(
            host.project_embedded_files(&path).unwrap(),
            [b"missing.bin".to_vec(), b"second\nfile".to_vec()]
        );
        let cg = host.named(b"Codegen").unwrap();
        let name = unsafe { host.id.intern_name(b"observed") }.unwrap();
        let nil = host.named(b"nil").unwrap();
        assert_eq!(
            scope
                .call(cg, b"moduleSourceFor:", &[Operand::object(name)])
                .unwrap(),
            nil
        );
        assert_eq!(host.global(&scope, b"shouldNeverBeDefined").unwrap(), nil);
        for (vt, selector, hook) in hooks {
            assert!(hook.calls.get() > 0);
            host.send(
                vt,
                b"addMethod",
                &[Operand::object(selector), Operand::entry(hook.original)],
            )
            .unwrap();
        }
        // Metadata and syntax rejection remain distinct from compile diagnostics.
        for source in [b"42".as_slice(), b"[Module named:"] {
            fs::write(&path, source).unwrap();
            assert_eq!(host.index_project_source(&path).unwrap(), None);
            assert!(host.project_embedded_files(&path).unwrap().is_empty());
        }
        fs::write(&path, b"[Module named: #gold requires: #[#dep]]").unwrap();
        let gold = host.index_project_source(&path).unwrap().unwrap();
        assert_eq!(
            gold.revision_digest,
            "92749bc6da04d6045dc5a0eb386d10ad952542a70ca6754c431c1d2b8bbeed9f"
        );
        let vt = host.named(b"CodegenVt").unwrap();
        let selector = unsafe { host.id.intern_name(b"moduleMetadataOf:") }.unwrap();
        let original = host
            .send(
                host.id.receiver,
                b"bindingOf:in:",
                &[Operand::object(selector), Operand::object(vt)],
            )
            .unwrap()
            .as_entry()
            .unwrap();
        host.send(
            vt,
            b"addMethod",
            &[
                Operand::object(selector),
                Operand::entry(NativeEntry::new(fail, core::ptr::null_mut())),
            ],
        )
        .unwrap();
        assert!(
            host.index_project_source(&path)
                .unwrap_err()
                .0
                .contains("detail=909")
        );
        let malformed = scope
            .call(host.named(b"Vector").unwrap(), b"new", &[])
            .unwrap();
        host.send(
            vt,
            b"addMethod",
            &[
                Operand::object(selector),
                Operand::entry(NativeEntry::new(fixed, malformed.cast())),
            ],
        )
        .unwrap();
        assert!(
            host.index_project_source(&path)
                .unwrap_err()
                .0
                .contains("invalid module metadata")
        );
        host.send(
            vt,
            b"addMethod",
            &[Operand::object(selector), Operand::entry(original)],
        )
        .unwrap();
        let large = temp.0.join("large.birnam");
        fs::File::create(&large)
            .unwrap()
            .set_len(MAX_SCRIPT_BYTES + 1)
            .unwrap();
        assert!(
            host.index_project_source(&large)
                .unwrap_err()
                .0
                .contains("exceeds")
        );
        assert!(
            host.index_project_source(&temp.0)
                .unwrap_err()
                .0
                .contains("regular file")
        );
        drop(scope);
        host.send(gc, b"collectPrecisely", &[]).unwrap();
        drop(host);
        assert_eq!(observation.source, source); // owned data survives the host
    }
}
