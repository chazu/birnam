//! Scoped Id messages and owned value projections on the creating thread.
use crate::Resources;
use birnam_abi::{
    Oop, borrowed_bytes,
    id_protocol::{Operand, Outcome, Protocol},
};
use std::{cell::RefCell, fmt, marker::PhantomData, os::unix::ffi::OsStrExt, path::Path, rc::Rc};
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostError(pub String);
impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for HostError {}
pub fn native_error(operation: &str, error: Outcome) -> HostError {
    HostError(format!(
        "{operation}: native status={} detail={}",
        error.status, error.detail
    ))
}
pub fn object(value: Operand) -> Result<Oop, HostError> {
    value
        .as_object()
        .filter(|o| !o.is_null())
        .ok_or_else(|| HostError("expected an object result".into()))
}
// Native owners and the executable facade retain intermediates before any
// subsequent send, then releases them on success and on every error path.
pub struct Scope<'a> {
    pub host: &'a Context,
    tokens: RefCell<Vec<usize>>,
}
impl Scope<'_> {
    pub fn keep(&self, value: Operand) -> Result<Oop, HostError> {
        let o = object(value)?;
        let token = self
            .host
            .send(self.host.roots, b"retain:", &[Operand::object(o)])?
            .as_word()
            .ok_or_else(|| HostError("Roots retain: expected token".into()))?;
        self.tokens.borrow_mut().push(token);
        Ok(o)
    }
    pub fn call(
        &self,
        receiver: Oop,
        name: &[u8],
        arguments: &[Operand],
    ) -> Result<Oop, HostError> {
        self.keep(self.host.send(receiver, name, arguments)?)
    }
    pub fn string(&self, bytes: &[u8]) -> Result<Oop, HostError> {
        self.call(
            self.host.named(b"String")?,
            b"fromBytes:",
            &[Operand::bytes(bytes)],
        )
    }
    pub fn text(&self, o: Oop) -> Result<String, HostError> {
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
    pub fn evaluate(
        &self,
        source: &[u8],
        path: &Path,
        module: Option<&[u8]>,
    ) -> Result<(), HostError> {
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
        self.check(cg, result)?;
        Ok(())
    }
    pub fn check(&self, cg: Oop, result: Oop) -> Result<(), HostError> {
        let diagnostic = self.call(cg, b"lastError", &[])?;
        if diagnostic != self.host.named(b"nil")? {
            return Err(HostError(self.text(diagnostic)?));
        }
        self.check_result(result)
    }
    pub fn check_result(&self, result: Oop) -> Result<(), HostError> {
        if self.call(result, b"isError", &[])? == self.host.named(b"true")? {
            let message = self.call(result, b"message", &[])?;
            if message == self.host.named(b"nil")? {
                let kind = self.call(result, b"kind", &[])?;
                return Err(HostError(format!("error(#{})", self.symbol_text(kind)?)));
            }
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

impl Scope<'_> {
    pub fn exact(&self, value: Oop, family: &[u8]) -> Result<(), HostError> {
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
    pub fn vector_size(&self, value: Oop) -> Result<usize, HostError> {
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
    pub fn vector_at(&self, value: Oop, index: usize) -> Result<Oop, HostError> {
        let index = self.call(
            self.host.named(b"Integer")?,
            b"fromWord:",
            &[Operand::word(index)],
        )?;
        self.call(value, b"at:", &[Operand::object(index)])
    }
    pub fn symbol_text(&self, value: Oop) -> Result<String, HostError> {
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
    pub fn string_bytes(&self, value: Oop) -> Result<Vec<u8>, HostError> {
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
}
pub struct Context {
    pub id: Protocol,
    pub namespace: Oop,
    pub roots: Oop,
    namespace_token: usize,
    pub thread: std::thread::ThreadId,
    pub resources: Resources,
    // Protocol owners and loaded native images live for the process. Host
    // operations and root release must stay on the creating thread.
    _owner_thread: PhantomData<Rc<()>>,
}
impl Context {
    pub fn send(&self, receiver: Oop, name: &[u8], args: &[Operand]) -> Result<Operand, HostError> {
        unsafe { self.id.send_named(receiver, name, args) }
            .map_err(|e| native_error(&String::from_utf8_lossy(name), e))
    }
    pub fn named(&self, name: &[u8]) -> Result<Oop, HostError> {
        let symbol =
            unsafe { self.id.intern_name(name) }.map_err(|e| native_error("intern name", e))?;
        object(self.send(self.namespace, b"at:", &[Operand::object(symbol)])?)
    }
    pub fn scope(&self) -> Scope<'_> {
        Scope {
            host: self,
            tokens: RefCell::new(Vec::new()),
        }
    }

    pub fn global(&self, scope: &Scope<'_>, name: &[u8]) -> Result<Oop, HostError> {
        let symbol =
            unsafe { self.id.intern_name(name) }.map_err(|e| native_error("intern global", e))?;
        scope.call(self.named(b"Globals")?, b"at:", &[Operand::object(symbol)])
    }

}
impl Drop for Context {
    fn drop(&mut self) {
        let _ = self.send(
            self.roots,
            b"release:",
            &[Operand::word(self.namespace_token)],
        );
    }
}

impl Context {
    pub fn from_namespace(
        id: Protocol,
        namespace: Oop,
        resources: Resources,
    ) -> Result<Self, HostError> {
        let roots = object(
            unsafe {
                id.send_named(
                    namespace,
                    b"at:",
                    &[Operand::object(
                        id.intern_name(b"Roots")
                            .map_err(|e| native_error("Roots", e))?,
                    )],
                )
            }
            .map_err(|e| native_error("Roots", e))?,
        )?;
        let token = unsafe { id.send_named(roots, b"retain:", &[Operand::object(namespace)]) }
            .map_err(|e| native_error("retain namespace", e))?
            .as_word()
            .ok_or_else(|| HostError("invalid namespace token".into()))?;
        Ok(Self {
            id,
            namespace,
            roots,
            namespace_token: token,
            resources,
            thread: std::thread::current().id(),
            _owner_thread: PhantomData,
        })
    }
    pub fn owner_call(
        &self,
        q: &Scope<'_>,
        owner: &[u8],
        name: &[u8],
        args: &[Operand],
    ) -> Result<Oop, HostError> {
        let result = q.call(self.named(owner)?, name, args)?;
        q.check_result(result)?;
        Ok(result)
    }
    pub fn bootstrap_source(&self) -> Result<(), HostError> {
        self.owner_call(&self.scope(), b"HostEntry", b"bootstrapSource", &[])
            .map(|_| ())
    }
    pub fn load_artifact(&self, path: &Path) -> Result<(), HostError> {
        let q = self.scope();
        let path = q.string(path.as_os_str().as_bytes())?;
        self.owner_call(&q, b"HostEntry", b"loadArtifact:", &[Operand::object(path)])
            .map(|_| ())
    }
    pub fn run_repl(&self, config: bool) -> Result<(), HostError> {
        self.owner_call(
            &self.scope(),
            b"HostInteraction",
            b"repl:",
            &[Operand::object(self.named(if config {
                b"true"
            } else {
                b"false"
            })?)],
        )
        .map(|_| ())
    }
    pub fn serve_socket(&self, port: u16, bind: &str) -> Result<(), HostError> {
        let q = self.scope();
        let port = q.call(
            self.named(b"Integer")?,
            b"fromWord:",
            &[Operand::word(port as usize)],
        )?;
        let bind = q.string(bind.as_bytes())?;
        self.owner_call(
            &q,
            b"HostInteraction",
            b"socket:bind:",
            &[Operand::object(port), Operand::object(bind)],
        )
        .map(|_| ())
    }
}

impl Context {
    pub fn eval_script(&self, path: &Path) -> Result<(), HostError> {
        let q = self.scope();
        let path = q.string(path.as_os_str().as_bytes())?;
        self.owner_call(&q, b"HostEntry", b"evalScript:", &[Operand::object(path)])
            .map(|_| ())
    }
}
