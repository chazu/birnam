use crate::Resources;
#[path = "project_source.rs"]
mod project_source;
use birnam_abi::{
    Oop, borrowed_bytes,
    id_protocol::{Operand, Outcome, Protocol},
};
pub use project_source::{ProjectSourceMetadata, ProjectSourceObservation};
use std::{
    cell::RefCell, env, fmt, fs, io::Read, marker::PhantomData, os::unix::ffi::OsStrExt,
    path::Path, rc::Rc,
};

pub const MAX_SCRIPT_BYTES: u64 = 16 * 1024 * 1024;
#[derive(Debug)]
pub struct HostError(pub String);
impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for HostError {}
fn native_error(operation: &str, error: Outcome) -> HostError {
    HostError(format!(
        "{operation}: native status={} detail={}",
        error.status, error.detail
    ))
}
fn object(value: Operand) -> Result<Oop, HostError> {
    value
        .as_object()
        .filter(|o| !o.is_null())
        .ok_or_else(|| HostError("expected an object result".into()))
}
#[derive(Clone, Copy, Debug, Default)]
pub struct StartupPolicy {
    pub skip_stdlib: bool,
    pub precise_gc: bool,
}
impl StartupPolicy {
    pub fn from_environment() -> Self {
        Self {
            skip_stdlib: env::var_os("BIRNAM_NO_STDLIB").is_some(),
            precise_gc: env::var_os("BIRNAM_GC_PRECISE").is_some(),
        }
    }
}

// No Oop escapes the host API. Each scope retains intermediates before any
// subsequent send, then releases them on success and on every error path.
struct Scope<'a> {
    host: &'a HostRuntime,
    tokens: RefCell<Vec<usize>>,
}
impl Scope<'_> {
    fn keep(&self, value: Operand) -> Result<Oop, HostError> {
        let o = object(value)?;
        let token = self
            .host
            .send(self.host.roots, b"retain:", &[Operand::object(o)])?
            .as_word()
            .ok_or_else(|| HostError("Roots retain: expected token".into()))?;
        self.tokens.borrow_mut().push(token);
        Ok(o)
    }
    fn call(&self, receiver: Oop, name: &[u8], arguments: &[Operand]) -> Result<Oop, HostError> {
        self.keep(self.host.send(receiver, name, arguments)?)
    }
    fn string(&self, bytes: &[u8]) -> Result<Oop, HostError> {
        self.call(
            self.host.named(b"String")?,
            b"fromBytes:",
            &[Operand::bytes(bytes)],
        )
    }
    fn text(&self, o: Oop) -> Result<String, HostError> {
        let b = self
            .host
            .send(o, b"bytes", &[])?
            .as_bytes()
            .ok_or_else(|| HostError("expected diagnostic bytes".into()))?;
        // Copy the borrow before another message can collect its owner.
        let bytes = unsafe { borrowed_bytes(b.pointer, b.length) }
            .ok_or_else(|| HostError("invalid diagnostic bytes".into()))?;
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }
    fn evaluate(&self, source: &[u8], path: &Path, module: Option<&[u8]>) -> Result<(), HostError> {
        let cg = self.host.named(b"Codegen")?;
        let source = self.string(source)?;
        if let Some(name) = module {
            let name = unsafe { self.host.id.intern_name(name) }
                .map_err(|e| native_error("intern module", e))?;
            let recorded = self.call(
                cg,
                b"recordModuleSource:named:",
                &[Operand::object(source), Operand::object(name)],
            )?;
            self.check(cg, recorded)?;
        }
        let path = self.string(path.as_os_str().as_bytes())?;
        let result = self.call(
            cg,
            b"evalPersisting:from:",
            &[Operand::object(source), Operand::object(path)],
        )?;
        self.check(cg, result)
    }
    fn check(&self, cg: Oop, result: Oop) -> Result<(), HostError> {
        let diagnostic = self.call(cg, b"lastError", &[])?;
        if diagnostic != self.host.named(b"nil")? {
            return Err(HostError(self.text(diagnostic)?));
        }
        self.check_result(result)
    }
    fn check_result(&self, result: Oop) -> Result<(), HostError> {
        if self.call(result, b"isError", &[])? == self.host.named(b"true")? {
            let message = self.call(result, b"message", &[])?;
            return Err(HostError(self.text(message)?));
        }
        Ok(())
    }
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        for token in self.tokens.get_mut().drain(..).rev() {
            let _ = self
                .host
                .send(self.host.roots, b"release:", &[Operand::word(token)]);
        }
    }
}

pub struct HostRuntime {
    id: Protocol,
    namespace: Oop,
    roots: Oop,
    namespace_token: usize,
    // Protocol owners and loaded native images live for the process. Host
    // operations and root release must stay on the creating thread.
    _owner_thread: PhantomData<Rc<()>>,
}
impl HostRuntime {
    fn send(&self, receiver: Oop, name: &[u8], args: &[Operand]) -> Result<Operand, HostError> {
        unsafe { self.id.send_named(receiver, name, args) }
            .map_err(|e| native_error(&String::from_utf8_lossy(name), e))
    }
    fn named(&self, name: &[u8]) -> Result<Oop, HostError> {
        let symbol =
            unsafe { self.id.intern_name(name) }.map_err(|e| native_error("intern name", e))?;
        object(self.send(self.namespace, b"at:", &[Operand::object(symbol)])?)
    }
    fn scope(&self) -> Scope<'_> {
        Scope {
            host: self,
            tokens: RefCell::new(Vec::new()),
        }
    }
    fn start_native(resources: &Resources, precise_gc: bool) -> Result<Self, HostError> {
        let id = birnam_siphon::load(resources.id_dylib.as_os_str().as_bytes()).map_err(
            |(status, detail)| {
                HostError(format!(
                    "load {}: native status={status} detail={detail}",
                    resources.id_dylib.display()
                ))
            },
        )?;
        let bootstrap = resources.lib_dir.join("libbirnam_bootstrap.dylib");
        let namespace = object(
            unsafe {
                id.send(
                    id.receiver,
                    id.load,
                    &[
                        Operand::bytes(bootstrap.as_os_str().as_bytes()),
                        Operand::bytes(resources.lib_dir.as_os_str().as_bytes()),
                    ],
                )
            }
            .map_err(|e| native_error(&format!("load {}", bootstrap.display()), e))?,
        )?;
        let roots_name =
            unsafe { id.intern_name(b"Roots") }.map_err(|e| native_error("intern Roots", e))?;
        let roots = object(
            unsafe { id.send_named(namespace, b"at:", &[Operand::object(roots_name)]) }
                .map_err(|e| native_error("find Roots", e))?,
        )?;
        let token = unsafe { id.send_named(roots, b"retain:", &[Operand::object(namespace)]) }
            .map_err(|e| native_error("retain namespace", e))?
            .as_word()
            .ok_or_else(|| HostError("Roots retain: expected token".into()))?;
        let host = Self {
            id,
            namespace,
            roots,
            namespace_token: token,
            _owner_thread: PhantomData,
        };
        host.send(
            host.named(b"GC")?,
            b"precisePayloads:",
            &[Operand::word(usize::from(precise_gc))],
        )?;
        {
            let scope = host.scope();
            let directory = scope.string(resources.lib_dir.as_os_str().as_bytes())?;
            scope.call(
                host.named(b"Alien")?,
                b"libraryDir:",
                &[Operand::object(directory)],
            )?;
        }
        Ok(host)
    }
    pub fn start(resources: &Resources, policy: StartupPolicy) -> Result<Self, HostError> {
        let host = Self::start_native(resources, policy.precise_gc)?;
        if !policy.skip_stdlib {
            let scope = host.scope();
            for (name, source) in [
                (
                    b"modules".as_slice(),
                    include_bytes!("../../../src/id/modules.birnam").as_slice(),
                ),
                (
                    b"stdlib".as_slice(),
                    include_bytes!("../../../src/id/stdlib.birnam").as_slice(),
                ),
            ] {
                scope
                    .evaluate(
                        source,
                        Path::new(&format!("<{}>", String::from_utf8_lossy(name))),
                        Some(name),
                    )
                    .map_err(|e| {
                        HostError(format!("bootstrap {}: {e}", String::from_utf8_lossy(name)))
                    })?;
            }
            scope.evaluate(b"[Module markLoaded: #stdlib]", Path::new("<host>"), None)?;
            let globals = host.named(b"Globals")?;
            let name = unsafe { host.id.intern_name(b"Module") }
                .map_err(|e| native_error("intern Module", e))?;
            let module = scope.call(globals, b"at:", &[Operand::object(name)])?;
            for name in [
                "artifact",
                "store",
                "peg",
                "lang",
                "tcc",
                "codegen",
                "prelude",
                "collection",
                "error",
                "relational",
                "image",
                "alien",
                "isolate",
            ] {
                let symbol = unsafe { host.id.intern_name(name.as_bytes()) }
                    .map_err(|e| native_error("intern kernel module", e))?;
                scope.call(module, b"kernel:", &[Operand::object(symbol)])?;
            }
            let lib = scope.string(resources.lib_dir.as_os_str().as_bytes())?;
            scope.call(module, b"libDir:", &[Operand::object(lib)])?;
        }
        Ok(host)
    }
    /// Restore into the native runtime without evaluating source bootstrap.
    /// Images own their saved methods, globals and source registry.
    pub fn start_image(
        resources: &Resources,
        policy: StartupPolicy,
        path: &Path,
    ) -> Result<Self, HostError> {
        let host = Self::start_native(resources, policy.precise_gc)?;
        host.load_image(path)?;
        let scope = host.scope();
        host.rebase_module(&scope, resources)?;
        drop(scope);
        Ok(host)
    }
    fn rebase_module(&self, scope: &Scope<'_>, resources: &Resources) -> Result<(), HostError> {
        // The runtime library directory belongs to this executable, not the saving process.
        let module = self.global(scope, b"Module")?;
        if module != self.named(b"nil")? {
            let lib = scope.string(resources.lib_dir.as_os_str().as_bytes())?;
            scope.call(module, b"libDir:", &[Operand::object(lib)])?;
        }
        Ok(())
    }
    /// Enter a restored worker without evaluating stdlib or global main.
    pub fn run_worker(
        resources: &Resources,
        policy: StartupPolicy,
        path: &Path,
        receiver: &str,
        selector: &str,
        fd: i32,
    ) -> Result<(), HostError> {
        if fd < 0 {
            return Err(HostError("worker channel must be nonnegative".into()));
        }
        let host = Self::start_native(resources, policy.precise_gc)?;
        let scope = host.scope();
        let isolate = host.named(b"Isolate")?;
        let admission = (|| {
            host.load_image(path)?;
            host.rebase_module(&scope, resources)?;
            let mut target = host.global(&scope, receiver.as_bytes())?;
            if target == host.named(b"nil")? {
                target = host.named(receiver.as_bytes())?;
            }
            scope.keep(Operand::object(target))?;
            let selector = unsafe { host.id.intern_name(selector.as_bytes()) }
                .map_err(|e| native_error("intern worker selector", e))?;
            scope.keep(Operand::object(selector))?;
            let behavior =
                scope.call(host.id.receiver, b"behaviorOf:", &[Operand::object(target)])?;
            if host
                .send(behavior, b"lookup", &[Operand::object(selector)])?
                .as_entry()
                .is_none()
            {
                return Err(HostError("worker selector is not callable".into()));
            }
            if std::env::var("BIRNAM_TASK_SERVICE").is_ok_and(|s| s == "1")
                && host.global(&scope, b"TaskService")? == host.named(b"nil")?
            {
                return Err(HostError("image has no TaskService".into()));
            }
            Ok((target, selector))
        })();
        let (target, selector) = match admission {
            Ok(v) => v,
            Err(e) => {
                let kind = scope.string(b"imageAdmission")?;
                let message = scope.string(e.0.as_bytes())?;
                let _ = host.send(
                    isolate,
                    b"workerBootError:kind:message:",
                    &[
                        Operand::word(fd as usize),
                        Operand::object(kind),
                        Operand::object(message),
                    ],
                );
                return Err(e);
            }
        };
        let result = host.send(
            isolate,
            b"runWorker:receiver:selector:",
            &[
                Operand::word(fd as usize),
                Operand::object(target),
                Operand::object(selector),
            ],
        )?;
        if result.as_word() == Some(0) {
            Ok(())
        } else {
            Err(HostError("worker channel failed".into()))
        }
    }
    fn global(&self, scope: &Scope<'_>, name: &[u8]) -> Result<Oop, HostError> {
        let symbol =
            unsafe { self.id.intern_name(name) }.map_err(|e| native_error("intern global", e))?;
        scope.call(self.named(b"Globals")?, b"at:", &[Operand::object(symbol)])
    }
    fn load_image(&self, path: &Path) -> Result<(), HostError> {
        let load = || -> Result<(), HostError> {
            let metadata = fs::metadata(path).map_err(|e| HostError(e.to_string()))?;
            if !metadata.is_file() {
                return Err(HostError("image must be a regular file".into()));
            }
            let scope = self.scope();
            let filename = scope.string(path.as_os_str().as_bytes())?;
            let system = self.named(b"System")?;
            let result = scope.call(system, b"loadImage:", &[Operand::object(filename)])?;
            if result == self.named(b"true")? || result == self.named(b"false")? {
                return Ok(());
            }
            scope.check_result(result)?;
            let report = scope.call(system, b"imageSaveReport", &[])?;
            let status = scope.call(report, b"status", &[])?;
            let status = scope.call(status, b"printString", &[])?;
            let status = scope.text(status)?;
            let detail = scope.call(report, b"detail", &[])?;
            Err(HostError(format!(
                "image load {status}: {}",
                scope.text(detail)?
            )))
        };
        load().map_err(|e| HostError(format!("{}: {e}", path.display())))
    }
    /// Enter the current global main through ordinary dispatch. Nil and false
    /// results succeed; missing main and returned Errors are failures.
    pub fn run_image_main(&self) -> Result<(), HostError> {
        let run = || -> Result<(), HostError> {
            let scope = self.scope();
            let main = self.global(&scope, b"main")?;
            if main == self.named(b"nil")? {
                return Err(HostError(
                    "image has no global main; REPL is not yet supported".into(),
                ));
            }
            let result = scope.call(main, b"value", &[])?;
            scope.check_result(result)
        };
        run().map_err(|e| HostError(format!("image main: {e}")))
    }
    pub fn eval_script(&self, path: &Path) -> Result<(), HostError> {
        let source = read_source_file(path)?;
        self.scope()
            .evaluate(&source, path, None)
            .map_err(|e| HostError(format!("{}: {e}", path.display())))
    }
    /// Compile to a durable native-object artifact without executing source.
    pub fn compile_module(&self, path: &Path, output: &Path) -> Result<(), HostError> {
        let source = read_source_file(path)?;
        let compile = || {
            let scope = self.scope();
            let source = scope.string(&source)?;
            let output = scope.string(output.as_os_str().as_bytes())?;
            let filename = scope.string(path.as_os_str().as_bytes())?;
            let cg = self.named(b"Codegen")?;
            let result = scope.call(
                cg,
                b"compile:output:from:",
                &[
                    Operand::object(source),
                    Operand::object(output),
                    Operand::object(filename),
                ],
            )?;
            scope.check(cg, result)?;
            if result == self.named(b"nil")? {
                return Err(HostError("artifact compilation failed".into()));
            }
            Ok(())
        };
        compile().map_err(|e: HostError| HostError(format!("{}: {e}", path.display())))
    }
    /// Let the authored Module manager preflight and activate the closure.
    pub fn load_artifact(&self, path: &Path) -> Result<(), HostError> {
        let load = || {
            let path = fs::canonicalize(path).map_err(|e| HostError(e.to_string()))?;
            let scope = self.scope();
            let manager = self.global(&scope, b"Module")?;
            if manager == self.named(b"nil")? {
                return Err(HostError(
                    "artifact loading requires the Module manager".into(),
                ));
            }
            let directory = scope.string(
                path.parent()
                    .unwrap_or(Path::new("."))
                    .as_os_str()
                    .as_bytes(),
            )?;
            let configured = scope.call(manager, b"libDir:", &[Operand::object(directory)])?;
            scope.check_result(configured)?;
            let path = scope.string(path.as_os_str().as_bytes())?;
            let result = scope.call(manager, b"loadArtifact:", &[Operand::object(path)])?;
            scope.check(self.named(b"Codegen")?, result)?;
            if result == self.named(b"nil")? {
                return Err(HostError("artifact activation failed".into()));
            }
            Ok(())
        };
        load().map_err(|e: HostError| HostError(format!("{}: {e}", path.display())))
    }
}
fn read_source_file(path: &Path) -> Result<Vec<u8>, HostError> {
    let read = || -> Result<Vec<u8>, HostError> {
        let metadata = fs::metadata(path).map_err(|e| HostError(e.to_string()))?;
        if !metadata.is_file() {
            return Err(HostError("source must be a regular file".into()));
        }
        let mut source = Vec::new();
        fs::File::open(path)
            .and_then(|f| f.take(MAX_SCRIPT_BYTES + 1).read_to_end(&mut source))
            .map_err(|e| HostError(e.to_string()))?;
        if source.len() as u64 > MAX_SCRIPT_BYTES {
            return Err(HostError(format!(
                "source exceeds {MAX_SCRIPT_BYTES} bytes"
            )));
        }
        Ok(source)
    };
    read().map_err(|e| HostError(format!("{}: {e}", path.display())))
}
impl Drop for HostRuntime {
    fn drop(&mut self) {
        if let Ok(isolate) = self.named(b"Isolate") {
            let _ = self.send(isolate, b"shutdown", &[]);
        }
        let _ = self.send(
            self.roots,
            b"release:",
            &[Operand::word(self.namespace_token)],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use birnam_abi::id_protocol::{self as native, NativeEntry};
    use std::{cell::Cell, ffi::c_void, path::PathBuf};

    struct CollectingSend {
        id: Protocol,
        gc: Oop,
        original: NativeEntry,
        calls: Cell<usize>,
        fail: Cell<bool>,
    }
    unsafe extern "C" fn collecting_send(
        context: *mut c_void,
        receiver: Oop,
        args: *const Operand,
        count: usize,
    ) -> Outcome {
        let state = unsafe { &*context.cast::<CollectingSend>() };
        if let Err(error) = unsafe { state.id.send_named(state.gc, b"collectPrecisely", &[]) } {
            return error;
        }
        state.calls.set(state.calls.get() + 1);
        if state.fail.get() {
            return Outcome::failed(native::UNBOUND, 12345);
        }
        unsafe { state.original.method.unwrap()(state.original.context, receiver, args, count) }
    }
    #[test]
    #[ignore = "requires production installed native modules; scripts/host-cli-test.sh"]
    fn host_roots_survive_collecting_sends() {
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
            },
        )
        .unwrap();
        let cg = host.named(b"CodegenVt").unwrap();
        let gc = host.named(b"GC").unwrap();
        let mut hooks = Vec::new();
        for name in [b"evalPersisting:from:".as_slice(), b"lastError"] {
            let symbol = unsafe { host.id.intern_name(name) }.unwrap();
            let original = host
                .send(
                    host.id.receiver,
                    b"bindingOf:in:",
                    &[Operand::object(symbol), Operand::object(cg)],
                )
                .unwrap()
                .as_entry()
                .unwrap();
            let mut hook = Box::new(CollectingSend {
                id: host.id,
                gc,
                original,
                calls: Cell::new(0),
                fail: Cell::new(false),
            });
            host.send(
                cg,
                b"addMethod",
                &[
                    Operand::object(symbol),
                    Operand::entry(NativeEntry::new(
                        collecting_send,
                        (&mut *hook as *mut CollectingSend).cast(),
                    )),
                ],
            )
            .unwrap();
            hooks.push((symbol, hook));
        }
        for source in [
            b"nil".as_slice(),
            b"false",
            b"#[1 2 3]",
            b"let value := \"rooted\"; [System gc]; value",
        ] {
            host.scope()
                .evaluate(source, Path::new("<root-test>"), None)
                .unwrap();
        }
        let error = host
            .scope()
            .evaluate(
                b"[System readFile: \"/absent/birnam-host-fixture\"]",
                Path::new("<root-test>"),
                None,
            )
            .unwrap_err();
        assert!(error.0.contains("could not read file"), "{error}");
        let error = host
            .scope()
            .evaluate(b"[", Path::new("<root-test>"), None)
            .unwrap_err();
        assert!(error.0.contains("syntax error"), "{error}");
        hooks[0].1.fail.set(true);
        let error = host
            .scope()
            .evaluate(b"nil", Path::new("<root-test>"), None)
            .unwrap_err();
        assert!(error.0.contains("native status=6 detail=12345"), "{error}");
        for (symbol, hook) in hooks {
            assert!(hook.calls.get() >= 6);
            host.send(
                cg,
                b"addMethod",
                &[Operand::object(symbol), Operand::entry(hook.original)],
            )
            .unwrap();
        }
        // Exact roots retain an intermediate and release it when the scope exits.
        let temporary;
        {
            let scope = host.scope();
            temporary = scope.string(b"host-only transient").unwrap();
            host.send(gc, b"collectPrecisely", &[]).unwrap();
            assert_eq!(scope.text(temporary).unwrap(), "host-only transient");
        }
        host.send(gc, b"collectPrecisely", &[]).unwrap();
        assert_ne!(
            host.send(
                host.id.receiver,
                b"allocationAt:",
                &[Operand::word(temporary.addr())]
            )
            .unwrap()
            .as_object(),
            Some(temporary)
        );
    }
    unsafe extern "C" fn deny_compile(
        _: *mut c_void,
        _: Oop,
        _: *const Operand,
        _: usize,
    ) -> Outcome {
        Outcome::failed(native::UNBOUND, 54321)
    }
    struct MainSend {
        hook: CollectingSend,
        globals: Oop,
        name: Oop,
        nil: Oop,
    }
    unsafe extern "C" fn remove_main_and_collect(
        context: *mut c_void,
        receiver: Oop,
        args: *const Operand,
        count: usize,
    ) -> Outcome {
        let state = unsafe { &*context.cast::<MainSend>() };
        if let Err(error) = unsafe {
            state.hook.id.send_named(
                state.globals,
                b"at:put:",
                &[Operand::object(state.name), Operand::object(state.nil)],
            )
        } {
            return error;
        }
        unsafe {
            collecting_send(
                (&state.hook as *const CollectingSend).cast_mut().cast(),
                receiver,
                args,
                count,
            )
        }
    }
    #[test]
    #[ignore = "requires installed modules and a saved CLI image; scripts/host-cli-test.sh"]
    fn image_entry_uses_saved_code_and_roots() {
        let lib = PathBuf::from(env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").unwrap());
        let path = PathBuf::from(env::var_os("BIRNAM_TEST_IMAGE").unwrap());
        let resources = crate::resolve_from_exe_dir(
            Path::new("/unused/bin"),
            crate::Overrides {
                lib_dir: Some(&lib),
                ..Default::default()
            },
        )
        .unwrap();
        let host = HostRuntime::start_native(&resources, true).unwrap();
        let symbol = |name: &[u8]| unsafe { host.id.intern_name(name) }.unwrap();
        let binding = |vt, name| {
            host.send(
                host.id.receiver,
                b"bindingOf:in:",
                &[Operand::object(name), Operand::object(vt)],
            )
            .unwrap()
            .as_entry()
            .unwrap()
        };
        let bind = |vt, name, entry| {
            host.send(
                vt,
                b"addMethod",
                &[Operand::object(name), Operand::entry(entry)],
            )
            .unwrap();
        };
        let behavior = |o| {
            object(
                host.send(host.id.receiver, b"behaviorOf:", &[Operand::object(o)])
                    .unwrap(),
            )
            .unwrap()
        };
        let mut denied = Vec::new();
        for (receiver, selector) in [
            (b"TCC".as_slice(), b"compile:".as_slice()),
            (b"TCC", b"compileObject:"),
            (b"Codegen", b"evalPersisting:from:"),
        ] {
            let vt = behavior(host.named(receiver).unwrap());
            let name = symbol(selector);
            denied.push((vt, name, binding(vt, name)));
            bind(
                vt,
                name,
                NativeEntry::new(deny_compile, core::ptr::null_mut()),
            );
        }
        let model = behavior(host.named(b"ObjectModel").unwrap());
        let restore = symbol(b"restoreImageSlotAt:in:put:");
        let mut hook = Box::new(CollectingSend {
            id: host.id,
            gc: host.named(b"GC").unwrap(),
            original: binding(model, restore),
            calls: Cell::new(0),
            fail: Cell::new(false),
        });
        bind(
            model,
            restore,
            NativeEntry::new(collecting_send, (&mut *hook as *mut CollectingSend).cast()),
        );
        host.load_image(&path).unwrap();
        assert!(hook.calls.get() > 0);
        bind(model, restore, hook.original);
        let scope = host.scope();
        let main = host.global(&scope, b"main").unwrap();
        let vt = behavior(main);
        let value = symbol(b"value");
        let mut main_send = Box::new(MainSend {
            hook: CollectingSend {
                id: host.id,
                gc: host.named(b"GC").unwrap(),
                original: binding(vt, value),
                calls: Cell::new(0),
                fail: Cell::new(true),
            },
            globals: host.named(b"Globals").unwrap(),
            name: symbol(b"main"),
            nil: host.named(b"nil").unwrap(),
        });
        // A native execution failure keeps its status and detail through Host.
        bind(
            vt,
            value,
            NativeEntry::new(
                collecting_send,
                (&mut main_send.hook as *mut CollectingSend).cast(),
            ),
        );
        let error = host.run_image_main().unwrap_err();
        assert!(error.0.contains("native status=6 detail=12345"), "{error}");
        main_send.hook.fail.set(false);
        bind(
            vt,
            value,
            NativeEntry::new(
                remove_main_and_collect,
                (&mut *main_send as *mut MainSend).cast(),
            ),
        );
        drop(scope); // The next call must root main itself, before it loses Globals.
        host.run_image_main().unwrap();
        // The fixture callback context must stay live until its binding is gone.
        bind(vt, value, main_send.hook.original);
        assert_eq!(main_send.hook.calls.get(), 2);
        for (vt, name, entry) in denied {
            bind(vt, name, entry);
        }
        assert!(
            host.run_image_main()
                .unwrap_err()
                .0
                .contains("no global main")
        );
        host.send(host.named(b"GC").unwrap(), b"collectPrecisely", &[])
            .unwrap();
    }
}
