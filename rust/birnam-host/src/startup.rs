use crate::Resources;
use birnam_abi::{
    Oop, borrowed_bytes,
    id_protocol::{Operand, Outcome, Protocol},
};
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
    pub fn start(resources: &Resources, policy: StartupPolicy) -> Result<Self, HostError> {
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
            &[Operand::word(usize::from(policy.precise_gc))],
        )?;
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
            let name = unsafe { id.intern_name(b"Module") }
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
            ] {
                let symbol = unsafe { id.intern_name(name.as_bytes()) }
                    .map_err(|e| native_error("intern kernel module", e))?;
                scope.call(module, b"kernel:", &[Operand::object(symbol)])?;
            }
            let lib = scope.string(resources.lib_dir.as_os_str().as_bytes())?;
            scope.call(module, b"libDir:", &[Operand::object(lib)])?;
        }
        Ok(host)
    }
    pub fn eval_script(&self, path: &Path) -> Result<(), HostError> {
        let evaluate = || -> Result<(), HostError> {
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
            self.scope().evaluate(&source, path, None)
        };
        evaluate().map_err(|e| HostError(format!("{}: {e}", path.display())))
    }
}
impl Drop for HostRuntime {
    fn drop(&mut self) {
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
}
