//! Sequential loading above Id. The build-only lang-seed feature boots the
//! compiler closure; published startup additionally requires its authored and
//! platform owners. All initializer arguments/results are Oops.
use birnam_abi::{
    self as abi, Oop, borrowed_slice,
    id_protocol::{Literal, MethodSpec, Module, Protocol},
};
use core::{
    cell::{Cell, RefCell},
    ptr,
};
use std::{
    collections::HashMap,
    os::unix::ffi::OsStrExt,
    path::Path,
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};
struct State {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    receiver: Oop,
    directory: Vec<u8>,
    names: HashMap<&'static [u8], Oop>,
    scoped: RefCell<Vec<Oop>>,
    results: RefCell<Vec<(Oop, Oop)>>,
    ready: Cell<bool>,
    thread: std::thread::ThreadId,
}
static INSTANCE: AtomicPtr<State> = AtomicPtr::new(ptr::null_mut());
static API: AtomicPtr<Protocol> = AtomicPtr::new(ptr::null_mut());
static BUSY: AtomicBool = AtomicBool::new(false);
struct Constructing;
impl Drop for Constructing {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::Release);
    }
}
fn invalid() -> Oop {
    let p = API.load(Ordering::Acquire);
    if p.is_null() {
        std::process::abort();
    }
    unsafe { abi::object_reference((*p).invalid) }
}
unsafe fn state<'a>(context: Oop) -> Result<&'a State, Oop> {
    unsafe {
        let p = INSTANCE.load(Ordering::Acquire);
        if p.is_null() {
            return Err(invalid());
        }
        let s = &*p;
        if context != s.ns || s.thread != std::thread::current().id() {
            Err(invalid())
        } else {
            Ok(s)
        }
    }
}
struct Scope<'a> {
    s: &'a State,
    start: usize,
}
impl Scope<'_> {
    fn keep(&self, v: Oop) -> Oop {
        self.s.scoped.borrow_mut().push(v);
        v
    }
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        self.s.scoped.borrow_mut().truncate(self.start);
    }
}
impl State {
    fn scope(&self, values: &[Oop]) -> Scope<'_> {
        let start = self.scoped.borrow().len();
        self.scoped.borrow_mut().extend_from_slice(values);
        Scope { s: self, start }
    }
    unsafe fn send(&self, r: Oop, name: &[u8], a: &[Oop]) -> Result<Oop, Oop> {
        unsafe { self.id.send(r, self.names[name], a) }
    }
    unsafe fn intern(&self, bytes: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            let q = self.scope(&[]);
            let values: Vec<_> = bytes
                .iter()
                .map(|b| abi::size(*b as usize).unwrap())
                .collect();
            let name = q.keep(self.send(self.id.root, b"nameWithBytes", &values)?);
            self.id.send(name, self.id.intern, &[])
        }
    }
    unsafe fn named(&self, name: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            let q = self.scope(&[]);
            let key = q.keep(self.intern(name)?);
            self.send(self.ns, b"at:", &[key])
        }
    }
    unsafe fn publish(&self, name: &[u8], value: Oop) -> Result<(), Oop> {
        unsafe {
            let q = self.scope(&[value]);
            let key = q.keep(self.intern(name)?);
            self.send(self.ns, b"at:put:", &[key, value])?;
            Ok(())
        }
    }
    unsafe fn text(&self, bytes: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            let q = self.scope(&[]);
            let factory = q.keep(self.named(b"String")?);
            let values: Vec<_> = bytes
                .iter()
                .map(|b| abi::size(*b as usize).unwrap())
                .collect();
            let data = q.keep(self.send(self.id.root, b"sequence", &values)?);
            self.send(factory, b"fromBytes:", &[data])
        }
    }
    unsafe fn load_path(&self, path: &[u8], args: &[Oop]) -> Result<Oop, Oop> {
        unsafe {
            let q = self.scope(args);
            let values: Vec<_> = path
                .iter()
                .map(|b| abi::size(*b as usize).unwrap())
                .collect();
            let name = q.keep(self.send(self.id.root, b"nameWithBytes", &values)?);
            let arguments = q.keep(self.send(self.id.root, b"sequence", args)?);
            let result = q.keep(self.id.send(name, self.id.load, &[arguments])?);
            let token = self.send(self.roots, b"retain:", &[result])?;
            self.results.borrow_mut().push((result, token));
            Ok(result)
        }
    }
    unsafe fn native(&self, name: &str, args: &[Oop]) -> Result<Oop, Oop> {
        unsafe { self.load_path(&module_path(&self.directory, name), args) }
    }
    unsafe fn artifact(&self, name: &str, source: Option<&[u8]>) -> Result<Oop, Oop> {
        unsafe {
            let q = self.scope(&[]);
            let file = module_path(&self.directory, name);
            let path = q.keep(self.text(&file)?);
            let codegen = q.keep(self.named(b"Codegen")?);
            if let Some(source) = source {
                let source = q.keep(self.text(source)?);
                let filename = q.keep(
                    self.text(
                        format!(
                            "modules/{}.birnam",
                            match name {
                                "globalsOwner" => "globals-owner",
                                "sourceRegistry" => "source-registry",
                                n => n,
                            }
                        )
                        .as_bytes(),
                    )?,
                );
                let flag = q.keep(self.named(b"true")?);
                let result = q.keep(self.send(
                    codegen,
                    b"compile:output:from:synchronousBodies:",
                    &[source, path, filename, flag],
                )?);
                self.require_value(result)?;
            }
            let result = q.keep(self.send(codegen, b"load:", &[path])?);
            self.require_value(result)?;
            Ok(result)
        }
    }
    unsafe fn require_value(&self, value: Oop) -> Result<(), Oop> {
        unsafe {
            let q = self.scope(&[value]);
            if value.is_null() || value == self.named(b"nil")? {
                return Err(invalid());
            }
            let error = q.keep(self.named(b"Error")?);
            if !error.is_null() && self.send(value, b"isError", &[])? == self.named(b"true")? {
                return Err(value);
            }
            Ok(())
        }
    }
}
fn module_path(directory: &[u8], name: &str) -> Vec<u8> {
    Path::new(std::ffi::OsStr::from_bytes(directory))
        .join(format!("libbirnam_{name}.dylib"))
        .as_os_str()
        .as_bytes()
        .to_vec()
}
unsafe extern "C" fn roots(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [visitor] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let mut values = vec![s.ns, s.roots, s.receiver];
        values.extend_from_slice(&s.scoped.borrow());
        for value in values {
            s.send(*visitor, b"visit:", &[value])?;
        }
        Ok(ptr::null_mut())
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn load_native(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [path, args @ ..] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let path = s.id.read_bytes(
            *path,
            s.names[b"sizeWord".as_slice()],
            s.names[b"atWord:".as_slice()],
        )?;
        s.load_path(&path, args)
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn release_result(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [value] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let token = {
            let mut results = s.results.borrow_mut();
            let Some(index) = results.iter().position(|(v, _)| v == value) else {
                return Ok(abi::size(0).unwrap());
            };
            results.swap_remove(index).1
        };
        if let Err(e) = s.send(s.roots, b"release:", &[token]) {
            s.results.borrow_mut().push((*value, token));
            return Err(e);
        }
        Ok(abi::size(1).unwrap())
    })()
    .unwrap_or_else(abi::fault_reference)
}
// Loading reports an ordinary [status Symbol, value/cause] Vector. There is no
// native value-kind packet: successful results are already language Oops.
unsafe extern "C" fn load_result(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [path, arguments] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[*path, *arguments]);
        let bytes = s.id.read_bytes(
            *path,
            s.names[b"sizeWord".as_slice()],
            s.names[b"atWord:".as_slice()],
        )?;
        let integer = q.keep(s.named(b"Integer")?);
        let size = q.keep(s.send(*arguments, b"size", &[])?);
        let count = abi::as_size(s.send(integer, b"wordOf:", &[size])?).ok_or_else(invalid)?;
        let mut args = Vec::new();
        for i in 0..count {
            let index =
                q.keep(s.send(integer, b"fromWord:", &[abi::size(i).ok_or_else(invalid)?])?);
            args.push(q.keep(s.send(*arguments, b"at:", &[index])?));
        }
        let (kind, value) = match s.load_path(&bytes, &args) {
            Ok(v) => (b"loaded".as_slice(), q.keep(v)),
            Err(e) => {
                q.keep(e);
                let kind = s.send(e, b"faultKind", &[]).ok();
                (
                    if kind == Some(s.names[b"mappingFailed".as_slice()]) {
                        b"mappingFailed".as_slice()
                    } else if kind == Some(s.names[b"incompatibleModule".as_slice()]) {
                        b"incompatibleModule".as_slice()
                    } else {
                        b"initializationFailed".as_slice()
                    },
                    e,
                )
            }
        };
        let kind = q.keep(s.intern(kind)?);
        let factory = q.keep(s.named(b"Vector")?);
        let mut result = q.keep(s.send(factory, b"new", &[])?);
        // Native absence is not language nil. Omit an absent result so the
        // language loader can preserve its receiver-returning void convention.
        for value in [kind, value].into_iter().filter(|value| !value.is_null()) {
            result = q.keep(s.send(result, b"add:", &[value])?);
        }
        Ok(result)
    })()
    .unwrap_or_else(abi::fault_reference)
}
macro_rules! literals{($($v:literal),*$(,)?)=>{const NAMES:&[&[u8]]=&[$($v),*];static LITERALS:&[Literal]=&[$(Literal::new($v)),*];};}
literals![
    b"loadNative:",
    b"release:",
    b"loadResultAt:arguments:",
    b"rootsDo:",
    b"sizeWord",
    b"atWord:",
    b"nameWithBytes",
    b"sequence",
    b"objectBehavior",
    b"objectPrototype",
    b"delegated",
    b"allocate",
    b"methodAt:",
    b"withContext:",
    b"addMethod",
    b"retain:",
    b"register:",
    b"at:",
    b"at:put:",
    b"activate",
    b"automaticCollection:",
    b"fromBytes:",
    b"behaviorOf:",
    b"installPrinting:",
    b"installFloat:",
    b"installNumbers:",
    b"parentOf:put:",
    b"installCollections:",
    b"compile:output:from:synchronousBodies:",
    b"load:",
    b"isError",
    b"libDir:",
    b"visit:",
    b"size",
    b"wordOf:",
    b"fromWord:",
    b"new",
    b"add:",
    b"faultKind",
    b"mappingFailed",
    b"incompatibleModule"
];
static METHODS: &[MethodSpec] = &[
    MethodSpec {
        name: 0,
        entry: Some(load_native),
    },
    MethodSpec {
        name: 1,
        entry: Some(release_result),
    },
    MethodSpec {
        name: 2,
        entry: Some(load_result),
    },
    MethodSpec {
        name: 3,
        entry: Some(roots),
    },
];
unsafe extern "C" fn initialize(
    p: *const Protocol,
    image: Oop,
    l: *const Oop,
    n: usize,
    a: *const Oop,
    count: usize,
) -> Oop {
    API.store(p.cast_mut(), Ordering::Release);
    (|| unsafe {
        let id = *p;
        let [directory] = borrowed_slice(a, count).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let literals = borrowed_slice(l, n)
            .filter(|v| v.len() == NAMES.len())
            .ok_or_else(invalid)?;
        let names: HashMap<_, _> = NAMES
            .iter()
            .copied()
            .zip(literals.iter().copied())
            .collect();
        let directory = id.read_bytes(
            *directory,
            names[b"sizeWord".as_slice()],
            names[b"atWord:".as_slice()],
        )?;
        let prior = INSTANCE.load(Ordering::Acquire);
        if !prior.is_null() {
            let s = &*prior;
            return if s.ready.get() && s.directory == directory && s.id.root == id.root {
                Ok(s.ns)
            } else {
                Err(invalid())
            };
        }
        if BUSY.swap(true, Ordering::AcqRel) {
            return Err(invalid());
        }
        let _busy = Constructing;
        let send = |r, name: &[u8], a: &[Oop]| id.send(r, names[name], a);
        let load = |name: &str, args: &[Oop]| -> Result<Oop, Oop> {
            let bytes = module_path(&directory, name);
            let values: Vec<_> = bytes
                .iter()
                .map(|b| abi::size(*b as usize).unwrap())
                .collect();
            let path = send(id.root, b"nameWithBytes", &values)?;
            let args = send(id.root, b"sequence", args)?;
            id.send(path, id.load, &[args])
        };
        let roots = load("root_registry", &[])?;
        let ns = load("namespace", &[roots])?;
        let base = send(id.root, b"objectBehavior", &[])?;
        let vt = send(base, b"delegated", &[])?;
        let vt_token = send(roots, b"retain:", &[vt])?;
        let receiver = send(vt, b"allocate", &[abi::size(0).unwrap()])?;
        let receiver_token = send(roots, b"retain:", &[receiver])?;
        let s = Box::leak(Box::new(State {
            id,
            ns,
            roots,
            receiver,
            directory,
            names,
            scoped: RefCell::new(Vec::new()),
            results: RefCell::new(Vec::new()),
            ready: Cell::new(false),
            thread: std::thread::current().id(),
        }));
        INSTANCE.store(s, Ordering::Release);
        for spec in METHODS {
            let method = s.send(image, b"methodAt:", &[literals[spec.name]])?;
            let method = s.send(method, b"withContext:", &[ns])?;
            s.send(vt, b"addMethod", &[literals[spec.name], method])?;
        }
        s.send(roots, b"register:", &[receiver])?;
        for (name, value) in [
            (b"Bootstrap".as_slice(), receiver),
            (b"Roots", roots),
            (b"Namespace", ns),
            (b"ObjectVt", base),
            (b"Vtable", id.root),
        ] {
            s.publish(name, value)?;
        }
        s.native("boolean", &[ns])?;
        s.native("nil", &[ns])?;
        let gc = s.native("collect", &[roots])?;
        s.publish(b"GC", gc)?;
        s.send(gc, b"activate", &[])?;
        s.native("object_model", &[ns])?;
        for name in ["word", "bytes", "execution", "string", "integer", "float"] {
            s.native(name, &[ns])?;
        }
        let string = s.named(b"String")?;
        for name in [b"true".as_slice(), b"nil"] {
            s.send(s.named(name)?, b"installPrinting:", &[string])?;
        }
        s.publish(
            b"SymbolVt",
            s.send(id.root, b"behaviorOf:", &[s.intern(b"Symbol")?])?,
        )?;
        for name in [
            "prelude",
            "system_output",
            "system_input",
            "environment",
            "file_io",
            "storage",
        ] {
            s.native(name, &[ns])?;
        }
        let integer = s.named(b"Integer")?;
        let float = s.named(b"Float")?;
        s.send(integer, b"installFloat:", &[float])?;
        s.send(string, b"installNumbers:", &[integer, float])?;
        let collection = s.send(base, b"delegated", &[])?;
        let token = s.send(roots, b"retain:", &[collection])?;
        s.publish(b"Collection", collection)?;
        s.publish(b"CollectionVt", collection)?;
        s.send(
            id.root,
            b"parentOf:put:",
            &[s.named(b"StringVt")?, collection],
        )?;
        s.send(roots, b"release:", &[token])?;
        for name in ["vector", "map"] {
            s.native(name, &[ns])?;
        }
        s.send(string, b"installCollections:", &[s.named(b"Vector")?])?;
        s.native("portable_code", &[ns])?;
        for name in [
            "peg",
            "artifact",
            "tcc",
            "native_backend",
            "codegen_runtime",
        ] {
            s.native(name, &[ns])?;
        }
        #[cfg(feature = "lang-seed")]
        s.native("lang_seed", &[ns])?;
        for (name, source) in [
            (
                "error",
                include_bytes!("../../../modules/error.birnam").as_slice(),
            ),
            (
                "globalsOwner",
                include_bytes!("../../../modules/globals-owner.birnam").as_slice(),
            ),
            (
                "sourceRegistry",
                include_bytes!("../../../modules/source-registry.birnam").as_slice(),
            ),
        ] {
            let source = if cfg!(feature = "lang-seed") {
                Some(source)
            } else {
                None
            };
            s.artifact(name, source)?;
        }
        #[cfg(not(feature = "lang-seed"))]
        {
            // Preserve the complete published startup requirements. Missing or
            // unmigrated owners fail visibly; seed mode does not claim this closure.

            for name in [
                "cell",
                "modules",
                "lang",
                "nativeValues",
                "booleanValues",
                "integerValues",
                "blockLoopControl",
                "blockLoop",
                "range",
                "systemGc",
                "printing",
                "sourceLoader",
                "nativeLoader",
                "alienCompiler",
                "alienRecipes",
                "imageFormat",
                "trace",
                "globals",
            ] {
                s.artifact(name, None)?;
            }
            for name in ["generation", "image", "alien", "isolate"] {
                s.native(name, &[ns])?;
            }
        }
        let globals = s.named(b"Globals")?;
        let module_name = s.intern(b"Module")?;
        let manager = s.send(globals, b"at:", &[module_name])?;
        if manager != s.named(b"nil")? {
            let q = s.scope(&[manager]);
            let path = q.keep(s.text(&s.directory)?);
            s.send(manager, b"libDir:", &[path])?;
        }
        s.send(roots, b"release:", &[receiver_token])?;
        s.send(roots, b"release:", &[vt_token])?;
        s.ready.set(true);
        Ok(ns)
    })()
    .unwrap_or_else(abi::fault_reference)
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v2: Module = Module {
    version: 2,
    byte_size: size_of::<Module>() as u32,
    literals: LITERALS.as_ptr(),
    literal_count: LITERALS.len(),
    methods: METHODS.as_ptr(),
    method_count: METHODS.len(),
    initialize: Some(initialize),
};
