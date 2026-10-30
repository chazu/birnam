//! Scoped Id messages and owned value projections on the creating thread.
use crate::Resources;
use birnam_abi::{self as abi, Oop, id_protocol::Protocol};
use std::{cell::RefCell, fmt, marker::PhantomData, os::unix::ffi::OsStrExt, path::Path, rc::Rc};
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostError(pub String);
impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for HostError {}
pub fn native_error(operation: &str, error: Oop) -> HostError {
    HostError(format!("{operation}: native failure ({error:p})"))
}
pub fn size(value: usize) -> Result<Oop, HostError> {
    abi::size(value).ok_or_else(|| HostError("value exceeds immediate size range".into()))
}
pub fn object(value: Oop) -> Result<Oop, HostError> {
    if value.is_null() {
        Err(HostError("expected an object result".into()))
    } else {
        Ok(value)
    }
}
// Native owners and the executable facade retain intermediates before any
// subsequent send, then releases them on success and on every error path.
pub struct Scope<'a> {
    pub host: &'a Context,
    tokens: RefCell<Vec<Oop>>,
    reported: Option<crate::roots::Scope>,
    finished: bool,
}
impl Scope<'_> {
    pub fn keep(&self, value: Oop) -> Result<Oop, HostError> {
        if let Some(scope) = &self.reported { scope.keep(value); return Ok(value); }
        let token = unsafe {
            self.host
                .id
                .send(self.host.roots, self.host.retain, &[value])
        }
        .map_err(|e| native_error("retain Host value", e))?;
        self.tokens.borrow_mut().push(token);
        Ok(value)
    }
    // Protect returned Oops while replaceable release: callbacks run.
    pub fn finish(mut self, value: Oop) -> Result<Oop, HostError> {
        self.finished = true;
        self.release(value)?;
        Ok(value)
    }
    fn release(&self, protected: Oop) -> Result<(), HostError> {
        if self.reported.is_some() { return Ok(()); }
        let mut values = vec![protected];
        values.extend(self.tokens.take());
        unsafe {
            self.host
                .id
                .send(self.host.roots, self.host.release_many, &values)
        }
        .map_err(|e| native_error("release Host scope", e))?;
        Ok(())
    }
    pub fn call(&self, receiver: Oop, name: &[u8], arguments: &[Oop]) -> Result<Oop, HostError> {
        self.keep(self.host.send(receiver, name, arguments)?)
    }
    pub fn byte_values(&self, bytes: &[u8]) -> Result<Oop, HostError> {
        let values: Vec<_> = bytes
            .iter()
            .map(|b| abi::size(*b as usize).unwrap())
            .collect();
        self.call(self.host.id.root, b"sequence", &values)
    }
    pub fn string(&self, bytes: &[u8]) -> Result<Oop, HostError> {
        let factory = self.host.named(b"Bytes")?;
        let mut chunks = bytes.chunks(abi::byte_transfer::CHUNK_BYTES);
        let values = |chunk: &[u8]| chunk.iter().map(|&b| abi::size(b as usize).unwrap()).collect::<Vec<_>>();
        let source = self.call(factory, b"fromByteValues", &values(chunks.next().unwrap_or(&[])))?;
        for chunk in chunks {
            self.host.send(source, b"appendByteValues", &values(chunk))?;
        }
        self.call(
            self.host.named(b"String")?,
            b"fromBytes:",
            &[source],
        )
    }
    pub fn read_bytes(&self, value: Oop) -> Result<Vec<u8>, HostError> {
        self.keep(value)?;
        self.host.ensure_byte_reader()?;
        unsafe { self.host.byte_reader.borrow().as_ref().unwrap().read(value) }
        .map_err(|e| native_error("read Host bytes", e))
    }
    pub fn text(&self, o: Oop) -> Result<String, HostError> {
        Ok(String::from_utf8_lossy(&self.read_bytes(self.call(o, b"bytes", &[])?)?).into_owned())
    }
    pub fn word(&self, value: usize) -> Result<Oop, HostError> {
        if let Some(value) = abi::size(value) {
            return Ok(value);
        }
        self.call(
            self.host.named(b"Word")?,
            b"fromUnsignedLow:high:",
            &[size(value & 0xffff_ffff)?, size(value >> 32)?],
        )
    }
    pub fn word_bits(&self, value: Oop) -> Result<usize, HostError> {
        if let Some(v) = abi::as_size(value) {
            return Ok(v);
        }
        self.keep(value)?;
        let lo = abi::as_size(self.host.send(value, b"low32", &[])?)
            .filter(|v| *v <= u32::MAX as usize)
            .ok_or_else(|| HostError("expected Word low bits".into()))?;
        let hi = abi::as_size(self.host.send(value, b"high32", &[])?)
            .filter(|v| *v <= u32::MAX as usize)
            .ok_or_else(|| HostError("expected Word high bits".into()))?;
        Ok(lo | (hi << 32))
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
            let name = self
                .host
                .intern(name)
                .map_err(|e| native_error("intern module", e))?;
            let recorded = self.call(cg, b"recordModuleSource:named:", &[source, name])?;
            self.check(cg, recorded)?;
        }
        let path = self.string(path.as_os_str().as_bytes())?;
        let result = self.call(cg, b"evalPersisting:from:", &[source, path])?;
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
        if !self.finished {
            let _ = self.release(std::ptr::null_mut());
        }
    }
}

impl Scope<'_> {
    pub fn exact(&self, value: Oop, family: &[u8]) -> Result<(), HostError> {
        let behavior = self.call(self.host.id.root, b"behaviorOf:", &[value])?;
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
        self.word_bits(self.call(self.host.named(b"Integer")?, b"wordOf:", &[count])?)
    }
    pub fn vector_at(&self, value: Oop, index: usize) -> Result<Oop, HostError> {
        let index = self.call(self.host.named(b"Integer")?, b"fromWord:", &[size(index)?])?;
        self.call(value, b"at:", &[index])
    }
    pub fn symbol_text(&self, value: Oop) -> Result<String, HostError> {
        self.exact(value, b"SymbolVt")?;
        Ok(String::from_utf8_lossy(&self.read_bytes(value)?).into_owned())
    }
    pub fn string_bytes(&self, value: Oop) -> Result<Vec<u8>, HostError> {
        self.exact(value, b"StringVt")?;
        self.read_bytes(value)
    }
}

pub struct Context {
    pub id: Protocol,
    pub namespace: Oop,
    pub roots: Oop,
    constructor: Oop,
    sequence: Oop,
    byte_reader: RefCell<Option<abi::byte_transfer::Reader>>,
    root_provider: RefCell<Option<crate::roots::Provider>>,
    retain: Oop,
    release_many: Oop,
    permanent: Vec<Oop>,

    pub thread: std::thread::ThreadId,
    pub resources: Resources,
    // Protocol owners and loaded native images live for the process. Host
    // operations and root release must stay on the creating thread.
    _owner_thread: PhantomData<Rc<()>>,
}
impl Context {
    pub(crate) fn install_byte_reader(&self, image: Oop) -> Result<(), HostError> {
        let keys = abi::byte_transfer::NAMES.iter().map(|&n| self.intern(n)
            .map(|v|(n,v)).map_err(|e|native_error("byte transfer selector",e)))
            .collect::<Result<std::collections::HashMap<_,_>,_>>()?;
        let reader = unsafe { abi::byte_transfer::Reader::new(self.id,image,self.roots,|n|keys[n]) }
            .map_err(|e|native_error("byte reader",e))?;
        *self.byte_reader.borrow_mut()=Some(reader);
        let method = self.send(image, b"methodAt:", &[self.intern(b"traceHostRoots").map_err(|e|native_error("Host roots",e))?])?;
        self.install_root_provider(method)

    }
    fn install_root_provider(&self, method: Oop) -> Result<(), HostError> {
        if self.root_provider.borrow().is_none() {
            let provider = crate::roots::Provider::new(self, method)?;
            *self.root_provider.borrow_mut() = Some(provider);
        }
        Ok(())
    }
    fn ensure_byte_reader(&self) -> Result<(), HostError> {
        if self.byte_reader.borrow().is_some() { return Ok(()); }
        // The executable has no native Module descriptor. Declare its callback
        // through the existing TCC owner; the Unit retains the forwarding code.
        let literals=abi::byte_transfer::NAMES.iter().copied().chain([b"traceHostRoots".as_slice()]).map(|n|format!("{{\"{}\",{}}}",String::from_utf8_lossy(n),n.len())).collect::<Vec<_>>().join(",");
        let source=format!(r#"typedef void *O; typedef unsigned long U;
struct L{{const unsigned char *p;U n;}}; struct M{{U name;O(*entry)(O,O,O,const O*,U);}};
static O receive(O root,O c,O r,const O*a,U n){{return ((O(*)(O,O,O,const O*,U))0x{:x})(root,c,r,a,n);}}
static O trace(O root,O c,O r,const O*a,U n){{return ((O(*)(O,O,O,const O*,U))0x{:x})(root,c,r,a,n);}}
static O init(const void*p,O image,const O*l,U n,const O*a,U count){{return image;}}
static const struct L literals[]={{{}}}; static const struct M methods[]={{{{0,receive}},{{{},trace}}}};
const struct {{unsigned version,bytes;const struct L*l;U nl;const struct M*m;U nm;O(*init)(const void*,O,const O*,U,const O*,U);}} birnam_native_module_v2={{2,sizeof(birnam_native_module_v2),literals,{},methods,2,init}};
"#,abi::byte_transfer::receive_entry as *const () as usize,crate::roots::trace_entry as *const () as usize,literals,abi::byte_transfer::NAMES.len(),abi::byte_transfer::NAMES.len()+1);
        let q=self.scope(); let source=q.string(source.as_bytes())?;
        let unit=q.call(self.named(b"TCC")?,b"compile:",&[source])?;
        q.call(unit,b"initialize",&[])?;
        let receive=q.call(unit,b"declaredMethodAt:",&[size(0)?])?;
        let keys=abi::byte_transfer::NAMES.iter().map(|&n|self.intern(n)
            .map(|v|(n,v)).map_err(|e|native_error("byte transfer selector",e)))
            .collect::<Result<std::collections::HashMap<_,_>,_>>()?;
        let reader=unsafe { abi::byte_transfer::Reader::from_method(self.id,receive,self.roots,|n|keys[n]) }
            .map_err(|e|native_error("byte reader",e))?;
        *self.byte_reader.borrow_mut()=Some(reader);
        let method=q.call(unit,b"declaredMethodAt:",&[size(1)?])?;
        self.install_root_provider(method)
    }
    pub fn intern(&self, name: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            let values: Vec<_> = name
                .iter()
                .map(|b| abi::size(*b as usize).unwrap())
                .collect();
            let name = self.id.send(self.id.root, self.constructor, &values)?;
            self.id.send(name, self.id.intern, &[])
        }
    }
    pub fn send(&self, receiver: Oop, name: &[u8], args: &[Oop]) -> Result<Oop, HostError> {
        unsafe {
            if self.thread != std::thread::current().id() {
                return Err(HostError("Host context used from another thread".into()));
            }
            let q = self.scope();
            if q.reported.is_some() {
                q.keep(receiver)?;
                for &argument in args { q.keep(argument)?; }
            } else {
                // Before the tracing callback exists, retain one argument group;
                // never turn a bulk byte message into one retain send per byte.
                let values: Vec<_> = std::iter::once(receiver).chain(args.iter().copied()).collect();
                let snapshot = self.id.send(self.id.root, self.sequence, &values)
                    .map_err(|e| native_error("snapshot Host arguments", e))?;
                q.keep(snapshot)?;
            }
            match self.intern(name).and_then(|name| self.id.send(receiver, name, args)) {
                Ok(value) => q.finish(value),
                Err(error) => {
                    let _ = q.finish(error);
                    Err(native_error(&String::from_utf8_lossy(name), error))
                }
            }
        }
    }
    pub fn named(&self, name: &[u8]) -> Result<Oop, HostError> {
        let symbol = self
            .intern(name)
            .map_err(|e| native_error("intern name", e))?;
        object(self.send(self.namespace, b"at:", &[symbol])?)
    }
    pub fn scope(&self) -> Scope<'_> {
        Scope {
            host: self,
            tokens: RefCell::new(Vec::new()),
            reported: self.root_provider.borrow().as_ref().map(|provider| provider.scope()),
            finished: false,
        }
    }
    pub fn load_native(&self, q: &Scope<'_>, path: &Path, args: &[Oop]) -> Result<Oop, HostError> {
        let values: Vec<_> = path
            .as_os_str()
            .as_bytes()
            .iter()
            .map(|b| abi::size(*b as usize).unwrap())
            .collect();
        let path = q.keep(self.send(self.id.root, b"nameWithBytes", &values)?)?;
        let args = q.keep(self.send(self.id.root, b"sequence", args)?)?;
        q.keep(
            unsafe { self.id.send(path, self.id.load, &[args]) }
                .map_err(|e| native_error("load native Host owner", e))?,
        )
    }

    pub fn global(&self, scope: &Scope<'_>, name: &[u8]) -> Result<Oop, HostError> {
        let symbol = self
            .intern(name)
            .map_err(|e| native_error("intern global", e))?;
        scope.keep(symbol)?;
        scope.call(self.named(b"Globals")?, b"at:", &[symbol])
    }
}
impl Drop for Context {
    fn drop(&mut self) {
        self.root_provider.get_mut().take();
        let mut args = vec![std::ptr::null_mut()];
        args.append(&mut self.permanent);
        let _ = unsafe { self.id.send(self.roots, self.release_many, &args) };
    }
}
impl Context {
    pub fn from_namespace(
        id: Protocol,
        namespace: Oop,
        resources: Resources,
        constructor: Oop,
    ) -> Result<Self, HostError> {
        let host=Self::from_namespace_unprepared(id,namespace,resources,constructor)?;
        let tcc=host.intern(b"TCC").map_err(|e|native_error("TCC selector",e))?;
        if !unsafe { id.send(namespace,id.at,&[tcc]) }.map_err(|e|native_error("TCC",e))?.is_null() {
            host.ensure_byte_reader()?;
        }
        Ok(host)
    }
    pub(crate) fn from_namespace_unprepared(
        id: Protocol,
        namespace: Oop,
        resources: Resources,
        constructor: Oop,
    ) -> Result<Self, HostError> {
        unsafe {
            let intern = |name: &[u8]| -> Result<Oop, Oop> {
                let values: Vec<_> = name
                    .iter()
                    .map(|b| abi::size(*b as usize).unwrap())
                    .collect();
                let name = id.send(id.root, constructor, &values)?;
                id.send(name, id.intern, &[])
            };
            let roots = id
                .send(
                    namespace,
                    id.at,
                    &[intern(b"Roots").map_err(|e| native_error("Roots name", e))?],
                )
                .map_err(|e| native_error("Roots", e))?;
            let retain = intern(b"retain:").map_err(|e| native_error("retain name", e))?;
            let mut permanent = Vec::new();
            for v in [namespace, roots, constructor, retain] {
                permanent.push(
                    id.send(roots, retain, &[v])
                        .map_err(|e| native_error("retain Host context", e))?,
                );
            }
            let release_many =
                intern(b"releaseTokensProtecting").map_err(|e| native_error("release name", e))?;
            permanent.push(
                id.send(roots, retain, &[release_many])
                    .map_err(|e| native_error("retain release selector", e))?,
            );
            let sequence = intern(b"sequence").map_err(|e| native_error("sequence name", e))?;
            permanent.push(id.send(roots, retain, &[sequence]).map_err(|e| native_error("retain sequence", e))?);
            Ok(Self {
                id,
                namespace,
                roots,
                constructor,
                sequence,
                byte_reader: RefCell::new(None),
                root_provider: RefCell::new(None),
                retain,
                release_many,
                permanent,
                resources,
                thread: std::thread::current().id(),
                _owner_thread: PhantomData,
            })
        }
    }
    pub fn owner_call(
        &self,
        q: &Scope<'_>,
        owner: &[u8],
        name: &[u8],
        args: &[Oop],
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
        self.owner_call(&q, b"HostEntry", b"loadArtifact:", &[path])
            .map(|_| ())
    }
    pub fn run_repl(&self, config: bool) -> Result<(), HostError> {
        self.owner_call(
            &self.scope(),
            b"HostInteraction",
            b"repl:",
            &[self.named(if config { b"true" } else { b"false" })?],
        )
        .map(|_| ())
    }
    pub fn serve_socket(&self, port: u16, bind: &str) -> Result<(), HostError> {
        let q = self.scope();
        let port = q.call(
            self.named(b"Integer")?,
            b"fromWord:",
            &[size(port as usize)?],
        )?;
        let bind = q.string(bind.as_bytes())?;
        self.owner_call(&q, b"HostInteraction", b"socket:bind:", &[port, bind])
            .map(|_| ())
    }
}

impl Context {
    pub fn eval_script(&self, path: &Path) -> Result<(), HostError> {
        let q = self.scope();
        let path = q.string(path.as_os_str().as_bytes())?;
        self.owner_call(&q, b"HostEntry", b"evalScript:", &[path])
            .map(|_| ())
    }
}
