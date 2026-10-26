//! Generation admission, publication, inverse ledgers and image application.
//! All compiler and backend dependencies are live Id-message consumers.
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
    sync::atomic::{AtomicPtr, Ordering},
};
mod backend;
mod candidate;
mod compiler;
mod sources;
mod values;
#[derive(Debug)]
enum Failure {
    Native(Oop),
    Value(Oop),
}
impl From<Oop> for Failure {
    fn from(v: Oop) -> Self {
        Self::Native(v)
    }
}
static API: AtomicPtr<Protocol> = AtomicPtr::new(ptr::null_mut());
static INSTANCE: AtomicPtr<State> = AtomicPtr::new(ptr::null_mut());
fn invalid() -> Failure {
    Failure::Native(unsafe { abi::object_reference((*API.load(Ordering::Acquire)).invalid) })
}
fn word(v: usize) -> Result<Oop, Failure> {
    abi::size(v).ok_or_else(invalid)
}
fn object(v: Oop) -> Result<Oop, Failure> {
    if v.is_null() { Err(invalid()) } else { Ok(v) }
}
fn optional(v: Oop) -> Option<Oop> {
    (!v.is_null()).then_some(v)
}
fn done(v: Result<Oop, Failure>) -> Oop {
    match v {
        Ok(v) | Err(Failure::Value(v)) => v,
        Err(Failure::Native(e)) => abi::fault_reference(e),
    }
}
struct State {
    id: Protocol,
    ns: Oop,
    names: HashMap<&'static [u8], Oop>,
    permanent: Vec<Oop>,
    scoped: RefCell<Vec<Oop>>,
    receiver: Oop,
    candidates: candidate::Candidates,
    ready: Cell<bool>,
    thread: std::thread::ThreadId,
}
unsafe fn state(c: Oop) -> Result<&'static State, Failure> {
    unsafe {
        let p = INSTANCE.load(Ordering::Acquire);
        if p.is_null() {
            return Err(invalid());
        }
        let s = &*p;
        if s.ns != c || s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        Ok(s)
    }
}
struct Scope<'a> {
    s: &'a State,
    start: usize,
}
impl<'a> Scope<'a> {
    fn new(s: &'a State) -> Self {
        Self {
            s,
            start: s.scoped.borrow().len(),
        }
    }
    unsafe fn keep(&self, o: Oop) -> Result<Oop, Failure> {
        self.s.scoped.borrow_mut().push(o);
        Ok(o)
    }
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        self.s.scoped.borrow_mut().truncate(self.start);
    }
}
impl State {
    unsafe fn intern(&self, n: &[u8]) -> Result<Oop, Failure> {
        unsafe {
            if let Some(v) = self.names.get(n) {
                return Ok(*v);
            }
            let values: Vec<_> = n.iter().map(|b| abi::size(*b as usize).unwrap()).collect();
            let name = self.id.send(
                self.id.root,
                self.names[b"nameWithBytes".as_slice()],
                &values,
            )?;
            Ok(self.id.send(name, self.id.intern, &[])?)
        }
    }
    unsafe fn send(&self, r: Oop, n: &[u8], a: &[Oop]) -> Result<Oop, Failure> {
        unsafe {
            let q = Scope::new(self);
            q.keep(r)?;
            for &v in a {
                q.keep(v)?;
            }
            Ok(self.id.send(r, self.intern(n)?, a)?)
        }
    }
    unsafe fn named(&self, n: &[u8]) -> Result<Oop, Failure> {
        unsafe { object(self.send(self.ns, b"at:", &[self.intern(n)?])?) }
    }
    unsafe fn bytes(&self, o: Oop) -> Result<Vec<u8>, Failure> {
        unsafe {
            let q = Scope::new(self);
            q.keep(o)?;
            let buffer = q.keep(self.send(o, b"bytes", &[])?)?;
            Ok(self.id.read_bytes(
                buffer,
                self.names[b"sizeWord".as_slice()],
                self.names[b"atWord:".as_slice()],
            )?)
        }
    }
    unsafe fn is_error(&self, o: Oop) -> Result<bool, Failure> {
        unsafe { Ok(self.send(o, b"isError", &[])? == self.named(b"true")?) }
    }
    unsafe fn index(&self, q: &Scope<'_>, o: Oop, n: &[u8]) -> Result<usize, Failure> {
        unsafe {
            let value = q.keep(self.send(o, n, &[])?)?;
            abi::as_size(self.send(self.named(b"Integer")?, b"wordOf:", &[value])?)
                .ok_or_else(invalid)
        }
    }
}
#[derive(Clone, Copy)]
enum Op {
    Candidate(candidate::Op),
    ValidateImage,
    PrepareImageSnapshot,
    BeginImageApplication,
    FinishImageApplication,
}
unsafe fn operation(c: Oop, receiver: Oop, a: *const Oop, n: usize, op: Op) -> Oop {
    done((|| unsafe {
        let s = state(c)?;
        let a = borrowed_slice(a, n).ok_or_else(invalid)?;
        let count = match op {
            Op::Candidate(op) => op.count(),
            _ => 0,
        };
        if a.len() != count {
            return Err(invalid());
        }
        let q = Scope::new(s);
        q.keep(receiver)?;
        for &v in a {
            q.keep(object(v)?)?;
        }
        match op {
            Op::Candidate(op) => candidate::operation(s, &q, op, a),
            Op::ValidateImage => candidate::validate_image(s, &q),
            Op::PrepareImageSnapshot => candidate::prepare_image_snapshot(s, &q),
            Op::BeginImageApplication => candidate::begin_image_application(s, &q),
            Op::FinishImageApplication => candidate::finish_image_application(s, &q),
        }
    })())
}
unsafe extern "C" fn roots(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    done((|| unsafe {
        let s = state(c)?;
        let Some([v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let v = object(*v)?;
        let mut values = s.candidates.roots();
        values.extend(s.permanent.iter().copied());
        values.extend_from_slice(&s.scoped.borrow());
        values.extend(s.names.values().copied());
        for o in values {
            s.send(v, b"visit:", &[o])?;
        }
        Ok(core::ptr::null_mut())
    })())
}
unsafe extern "C" fn operation_0(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::Candidate(candidate::Op::Watch)) }
}
unsafe extern "C" fn operation_1(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::Candidate(candidate::Op::RemoveFiles)) }
}
unsafe extern "C" fn operation_2(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::ValidateImage) }
}
unsafe extern "C" fn operation_3(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::PrepareImageSnapshot) }
}
unsafe extern "C" fn operation_4(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::Candidate(candidate::Op::Release)) }
}
unsafe extern "C" fn operation_5(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::Candidate(candidate::Op::RetainedCount)) }
}
unsafe extern "C" fn operation_6(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::Candidate(candidate::Op::RetainedBytes)) }
}
unsafe extern "C" fn operation_7(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::Candidate(candidate::Op::Map)) }
}
unsafe extern "C" fn operation_8(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::Candidate(candidate::Op::Units)) }
}
unsafe extern "C" fn operation_9(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::Candidate(candidate::Op::Data)) }
}
unsafe extern "C" fn operation_10(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::Candidate(candidate::Op::PolicyCell)) }
}
unsafe extern "C" fn operation_11(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::Candidate(candidate::Op::AdoptInitial)) }
}
unsafe extern "C" fn operation_12(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe {
        operation(
            c,
            r,
            a,
            n,
            Op::Candidate(candidate::Op::ReleaseCertification),
        )
    }
}
unsafe extern "C" fn operation_13(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::Candidate(candidate::Op::Reconcile)) }
}
unsafe extern "C" fn operation_14(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::BeginImageApplication) }
}
unsafe extern "C" fn operation_15(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::FinishImageApplication) }
}
unsafe extern "C" fn operation_16(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::Candidate(candidate::Op::Compile)) }
}
unsafe extern "C" fn operation_17(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::Candidate(candidate::Op::CompileSelected)) }
}
unsafe extern "C" fn operation_18(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::Candidate(candidate::Op::Inspect)) }
}
unsafe extern "C" fn operation_19(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::Candidate(candidate::Op::Mapped)) }
}
unsafe extern "C" fn operation_20(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::Candidate(candidate::Op::Path)) }
}
unsafe extern "C" fn initialize(
    p: *const Protocol,
    image: Oop,
    l: *const Oop,
    n: usize,
    a: *const Oop,
    count: usize,
) -> Oop {
    API.store(p.cast_mut(), Ordering::Release);
    done((|| unsafe {
        let id = *p;
        let [ns] = borrowed_slice(a, count).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let symbols = borrowed_slice(l, n)
            .filter(|v| v.len() == NAMES.len())
            .ok_or_else(invalid)?;
        let prior = INSTANCE.load(Ordering::Acquire);
        if !prior.is_null() {
            let s = &*prior;
            return if s.ready.get() && s.ns == *ns && s.id.root == id.root {
                Ok(s.receiver)
            } else {
                Err(invalid())
            };
        }
        let names: HashMap<_, _> = NAMES.iter().copied().zip(symbols.iter().copied()).collect();
        let send = |r, n: &[u8], a: &[Oop]| id.send(r, names[n], a);
        let named = |n: &[u8]| send(*ns, b"at:", &[names[n]]);
        let roots = named(b"Roots")?;
        let vt = send(named(b"ObjectVt")?, b"delegated", &[])?;
        let t = send(roots, b"retain:", &[vt])?;
        let receiver = send(vt, b"allocate", &[word(0)?])?;
        let rt = send(roots, b"retain:", &[receiver])?;
        let s = Box::leak(Box::new(State {
            id,
            ns: *ns,
            names,
            permanent: vec![*ns, roots, vt, receiver],
            scoped: RefCell::new(Vec::new()),
            receiver,
            candidates: Default::default(),
            ready: Cell::new(false),
            thread: std::thread::current().id(),
        }));
        INSTANCE.store(s, Ordering::Release);
        for (i, name) in INSTALL.iter().enumerate() {
            let method = s.send(image, b"newMethodAt:", &[word(i)?])?;
            let method = s.send(method, b"withContext:", &[*ns])?;
            s.send(vt, b"addMethod", &[s.intern(name)?, method])?;
        }
        s.send(roots, b"register:", &[receiver])?;
        for (name, value) in [(b"Generation".as_slice(), receiver), (b"GenerationVt", vt)] {
            s.send(*ns, b"at:put:", &[s.intern(name)?, value])?;
        }
        for token in [t, rt] {
            s.send(roots, b"release:", &[token])?;
        }
        s.ready.set(true);
        Ok(receiver)
    })())
}
static INSTALL: &[&[u8]] = &[
    b"watchCandidateRetirement:",
    b"removeCandidateFiles:",
    b"validateImageGeneration",
    b"prepareImageSnapshot",
    b"releaseCandidate:",
    b"retainedGenerationCount",
    b"retainedGenerationBytes",
    b"mapCandidate:",
    b"candidateUnits",
    b"candidateDataFor:",
    b"candidatePolicyCellFor:",
    b"adoptInitialGeneration:",
    b"releaseCertification",
    b"reconcileCandidate:",
    b"beginImageApplication",
    b"finishImageApplication",
    b"compileCandidate:named:output:",
    b"compileCandidate:named:output:selection:",
    b"inspectCandidate:into:",
    b"candidateMapped:",
    b"candidateContentPath:",
    b"rootsDo:",
];
macro_rules! literals{($($v:literal),*$(,)?)=>{const NAMES:&[&[u8]]=&[$($v),*];static LITERALS:&[Literal]=&[$(Literal::new($v)),*];};}
literals![
    b"nameWithBytes",
    b"at:",
    b"bytes",
    b"sizeWord",
    b"atWord:",
    b"isError",
    b"true",
    b"Integer",
    b"wordOf:",
    b"visit:",
    b"Roots",
    b"ObjectVt",
    b"delegated",
    b"retain:",
    b"allocate",
    b"methodAt:",
    b"binding:value:weak:",
    b"newMethodAt:",
    b"withContext:",
    b"addMethod",
    b"register:",
    b"Generation",
    b"GenerationVt",
    b"at:put:",
    b"release:",
    b"watchCandidateRetirement:",
    b"removeCandidateFiles:",
    b"validateImageGeneration",
    b"prepareImageSnapshot",
    b"releaseCandidate:",
    b"retainedGenerationCount",
    b"retainedGenerationBytes",
    b"mapCandidate:",
    b"candidateUnits",
    b"candidateDataFor:",
    b"candidatePolicyCellFor:",
    b"adoptInitialGeneration:",
    b"releaseCertification",
    b"reconcileCandidate:",
    b"beginImageApplication",
    b"finishImageApplication",
    b"compileCandidate:named:output:",
    b"compileCandidate:named:output:selection:",
    b"inspectCandidate:into:",
    b"candidateMapped:",
    b"candidateContentPath:",
    b"rootsDo:",
    b"Codegen",
    b"noteCompilationError:",
    b"clearCompilationError",
    b"analyzeSourceMetadata:",
    b"compiledSourceMetadataFor:",
    b"nil",
    b"candidateInventory:selection:",
    b"false",
    b"method",
    b"global",
    b"behaviorOf:",
    b"sourceTextOf:",
    b"SymbolVt",
    b"symbolBytesOf:",
    b"String",
    b"fromBytes:",
    b"Vector",
    b"new",
    b"add:",
    b"NativeBackend",
    b"emit:source:filename:persistent:synchronous:",
    b"fromWord:",
    b"named:synchronous:",
    b"moduleFactory:source:dispositions:",
    b"factory:source:dispositions:",
    b"descriptorFor:",
    b"TCC",
    b"mappingBytes:",
    b"invalidCandidate",
    b"mappingIdentityOf:",
    b"System",
    b".candidate",
    b"removeFile:ifContents:",
    b"candidateFileCleanupFailed",
    b"BlockVt",
    b"parentOf:",
    b"codeObject",
    b"captureCount",
    b"CodeObject",
    b"canonicalBytesOf:",
    b"sourceMap",
    b"baseDigest",
    b"generationSelection",
    b"candidateBusy",
    b"Execution",
    b"implementationOfEntry:",
    b"nonPortableGeneration",
    b"staleGeneration",
    b"generationNotAdopted",
    b"predecessor",
    b"deltaUnit",
    b"invalidLifecycleTransition",
    b"invalidGeneration",
    b"sourceBindingMismatch",
    b"methodOfEntry:",
    b"size",
    b"Globals",
    b"clone",
    b"Error",
    b"kind:message:",
    b"DispatchGeneration",
    b"current",
    b"sourceRevision",
    b"resumableRequired",
    b"implementationDisposition",
    b"methodOperation",
    b"nonMethodProgram",
    b"namedAdapter",
    b"blockBody",
    b"synchronous",
    b"readFile:",
    b"writeFile:contents:",
    b"imageApplicationActive",
    b"moduleNotRegistered",
    b"compileObject:",
    b"candidatePayloadV1",
    b"sourceRevisionOf:",
    b"CompiledOperation",
    b"targetName",
    b"selector",
    b"operation",
    b"install",
    b"remove",
    b"priorPresent",
    b"unitName",
    b"authoredSource",
    b"requires",
    b"provides",
    b"revisionDigest",
    b"artifactChecksum",
    b"expectedBaseDigest",
    b"importStatus",
    b"notMapped",
    b"state",
    b"candidate",
    b"rollbackAvailable",
    b"Cell",
    b"with:",
    b"Module",
    b"isLoaded:",
    b"dependenciesMissing",
    b"asBytes",
    b"loadObject:",
    b"initialize",
    b"bindingEpoch",
    b"bindingOf:in:",
    b"bindingAt:",
    b"bindingBasis",
    b"isBehavior:",
    b"nameOf:",
    b"ownerOf:",
    b"SourceRegistry",
    b"recordFor:",
    b"revision",
    b"sourceRegistry",
    b"sourceRegistryV1",
    b"bindingsDo:",
    b"sequence",
];
static METHODS: &[MethodSpec] = &[
    MethodSpec {
        name: 25,
        entry: Some(operation_0),
    },
    MethodSpec {
        name: 26,
        entry: Some(operation_1),
    },
    MethodSpec {
        name: 27,
        entry: Some(operation_2),
    },
    MethodSpec {
        name: 28,
        entry: Some(operation_3),
    },
    MethodSpec {
        name: 29,
        entry: Some(operation_4),
    },
    MethodSpec {
        name: 30,
        entry: Some(operation_5),
    },
    MethodSpec {
        name: 31,
        entry: Some(operation_6),
    },
    MethodSpec {
        name: 32,
        entry: Some(operation_7),
    },
    MethodSpec {
        name: 33,
        entry: Some(operation_8),
    },
    MethodSpec {
        name: 34,
        entry: Some(operation_9),
    },
    MethodSpec {
        name: 35,
        entry: Some(operation_10),
    },
    MethodSpec {
        name: 36,
        entry: Some(operation_11),
    },
    MethodSpec {
        name: 37,
        entry: Some(operation_12),
    },
    MethodSpec {
        name: 38,
        entry: Some(operation_13),
    },
    MethodSpec {
        name: 39,
        entry: Some(operation_14),
    },
    MethodSpec {
        name: 40,
        entry: Some(operation_15),
    },
    MethodSpec {
        name: 41,
        entry: Some(operation_16),
    },
    MethodSpec {
        name: 42,
        entry: Some(operation_17),
    },
    MethodSpec {
        name: 43,
        entry: Some(operation_18),
    },
    MethodSpec {
        name: 44,
        entry: Some(operation_19),
    },
    MethodSpec {
        name: 45,
        entry: Some(operation_20),
    },
    MethodSpec {
        name: 46,
        entry: Some(roots),
    },
];
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
