//! Artifact inspection uses Oop messages; descriptor parsers remain unchanged.
use crate as parser;
use crate::Descriptor as ParsedDescriptor;
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
    ffi::OsStr,
    fs,
    io::Read,
    os::unix::ffi::OsStrExt,
    path::Path,
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};
// Preserve the descriptor's existing thirteen object slots.
const KIND: usize = 0;
const IDENTITY: usize = 1;
const ABI: usize = 2;
const TARGET: usize = 3;
const SOURCE: usize = 4;
const REVISION: usize = 5;
const PATH: usize = 6;
const REQUIRES: usize = 7;
const NATIVE_REQUIRES: usize = 8;
const CAPABILITIES: usize = 9;
const REQUIRES_COUNT: usize = 10;
const NATIVE_REQUIRES_COUNT: usize = 11;
const CAPABILITIES_COUNT: usize = 12;

struct State {
    pub(crate) byte_reader: abi::byte_transfer::Reader,
    id: Protocol,
    ns: Oop,
    roots: Oop,
    model: Oop,
    string: Oop,
    integer: Oop,
    string_vt: Oop,
    integer_vt: Oop,
    nil: Oop,
    artifact: Oop,
    artifact_vt: Oop,
    descriptor_vt: Oop,
    last_failure: Cell<Oop>,
    names: HashMap<&'static [u8], Oop>,
    scoped: RefCell<Vec<Oop>>,
    ready: Cell<bool>,
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
fn object(v: Oop) -> Result<Oop, Oop> {
    if v.is_null() { Err(invalid()) } else { Ok(v) }
}
unsafe fn state<'a>(c: Oop) -> Result<&'a State, Oop> {
    let p = INSTANCE.load(Ordering::Acquire);
    if p.is_null() {
        return Err(invalid());
    }
    let s = unsafe { &*p };
    if c == s.artifact {
        Ok(s)
    } else {
        Err(invalid())
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
    fn reset(&self, v: Oop) -> Oop {
        let mut values = self.s.scoped.borrow_mut();
        values.truncate(self.start);
        values.push(v);
        v
    }
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        self.s.scoped.borrow_mut().truncate(self.start);
    }
}
impl State {
    fn scope(&self) -> Scope<'_> {
        Scope {
            s: self,
            start: self.scoped.borrow().len(),
        }
    }
    unsafe fn rooted<T>(
        &self,
        values: &[Oop],
        f: impl FnOnce() -> Result<T, Oop>,
    ) -> Result<T, Oop> {
        let scope = self.scope();
        for &v in values {
            scope.keep(v);
        }
        f()
    }
    unsafe fn send(&self, r: Oop, n: &[u8], a: &[Oop]) -> Result<Oop, Oop> {
        unsafe { self.id.send(r, *self.names.get(n).ok_or_else(invalid)?, a) }
    }
    unsafe fn named(&self, n: &[u8]) -> Result<Oop, Oop> {
        unsafe { self.send(self.ns, b"at:", &[*self.names.get(n).ok_or_else(invalid)?]) }
    }
    unsafe fn slot(&self, r: Oop, index: usize) -> Result<Oop, Oop> {
        object(unsafe {
            self.send(
                self.model,
                b"slotAt:in:",
                &[abi::size(index).ok_or_else(invalid)?, r],
            )
        }?)
    }
    unsafe fn bytes(&self, r: Oop) -> Result<Vec<u8>, Oop> {
        unsafe {
            let buffer = self.send(r, b"bytes", &[])?;
            self.byte_reader.read(buffer)
        }
    }
    unsafe fn symbol(&self, b: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            let end = b.iter().position(|b| *b == 0).unwrap_or(b.len());
            let values: Vec<_> = b[..end]
                .iter()
                .map(|&b| abi::size(b as usize).unwrap())
                .collect();
            let name = self.send(self.id.root, b"nameWithBytes", &values)?;
            self.id.send(name, self.id.intern, &[])
        }
    }
    unsafe fn string(&self, b: &[u8]) -> Result<Oop, Oop> {
        unsafe { self.rooted(&[], || {
            let source = abi::byte_transfer::buffer(b, self.send(self.ns,b"at:",&[self.names[b"Bytes".as_slice()]])?,
                |r,n,a| self.send(r,n,a), |v| {self.scoped.borrow_mut().push(v); Ok(v)})?;
            self.send(self.string, b"fromBytes:", &[source])
        }) }
    }
    unsafe fn number(&self, n: usize) -> Result<Oop, Oop> {
        object(unsafe {
            self.send(
                self.integer,
                b"fromWord:",
                &[abi::size(n).ok_or_else(invalid)?],
            )
        }?)
    }
    unsafe fn word(&self, r: Oop) -> Result<usize, Oop> {
        unsafe {
            let value = self.send(self.integer, b"wordOf:", &[r])?;
            if let Some(n) = abi::as_size(value) {
                return Ok(n);
            }
            let lo = abi::as_size(self.send(value, b"low32", &[])?)
                .filter(|v| *v <= u32::MAX as usize)
                .ok_or_else(invalid)?;
            let hi = abi::as_size(self.send(value, b"high32", &[])?)
                .filter(|v| *v <= u32::MAX as usize)
                .ok_or_else(invalid)?;
            Ok(lo | (hi << 32))
        }
    }
    unsafe fn family(&self, r: Oop, vt: Oop) -> Result<bool, Oop> {
        unsafe { Ok(self.send(self.id.root, b"behaviorOf:", &[r])? == vt) }
    }
    unsafe fn fail(&self, name: &[u8]) -> Result<Oop, Oop> {
        self.last_failure
            .set(*self.names.get(name).ok_or_else(invalid)?);
        Ok(self.nil)
    }
    unsafe fn count(&self, r: Oop, kind: ListKind) -> Result<usize, Oop> {
        unsafe {
            let index = match kind {
                ListKind::Roots => return Ok(1),
                ListKind::Requires => REQUIRES_COUNT,
                ListKind::NativeRequires => NATIVE_REQUIRES_COUNT,
                ListKind::Capabilities => CAPABILITIES_COUNT,
            };
            let count = self.slot(r, index)?;
            self.rooted(&[count], || self.word(count))
        }
    }

    unsafe fn value_at(&self, r: Oop, kind: ListKind, index: usize) -> Result<Oop, Oop> {
        unsafe {
            if index >= self.count(r, kind)? {
                return Ok(self.nil);
            }
            let slot = match kind {
                ListKind::Roots => return self.slot(r, IDENTITY),
                ListKind::Requires => REQUIRES,
                ListKind::NativeRequires => NATIVE_REQUIRES,
                ListKind::Capabilities => CAPABILITIES,
            };
            let buffer = self.slot(r, slot)?;
            let bytes = self.bytes(buffer)?;
            let Some(value) = list_field(&bytes, index) else {
                return Ok(self.nil);
            };
            if kind == ListKind::NativeRequires {
                self.string(value)
            } else {
                self.symbol(value)
            }
        }
    }
    unsafe fn vector(&self, r: Oop, kind: ListKind) -> Result<Oop, Oop> {
        unsafe {
            let vector = self.named(b"Vector")?;
            if vector.is_null() {
                return Ok(self.nil);
            }
            let scope = self.scope();
            let mut result = scope.keep(object(self.send(vector, b"new", &[])?)?);
            let count = self.count(r, kind)?;
            for i in 0..count {
                let value = self.value_at(r, kind, i)?;
                result = scope.reset(object(self.send(result, b"add:", &[value])?)?);
            }
            Ok(result)
        }
    }
    unsafe fn inspect(&self, args: &[Oop]) -> Result<Oop, Oop> {
        unsafe {
            let [path] = args else {
                return self.fail(b"artifactPathInvalid");
            };
            if path.is_null() || !self.family(*path, self.string_vt)? {
                return self.fail(b"artifactPathInvalid");
            }
            let path = self.bytes(*path)?;
            let bytes = match read_artifact(path_from_bytes(&path)) {
                Ok(v) => v,
                Err(()) => return self.fail(b"artifactUnavailable"),
            };
            self.inspect_bytes(&bytes, &path)
        }
    }
    unsafe fn inspect_snapshot(&self, args: &[Oop]) -> Result<Oop, Oop> {
        unsafe {
            let [bytes, path] = args else {
                return Err(invalid());
            };
            let path = object(*path)?;
            object(*bytes)?;
            if !self.family(path, self.string_vt)? {
                return self.fail(b"artifactPathInvalid");
            }
            let path = self.bytes(path)?;
            let bytes = self.bytes(*bytes)?;
            self.inspect_bytes(&bytes, &path)
        }
    }
    unsafe fn inspect_bytes(&self, bytes: &[u8], path: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            let parsed = match parser::inspect(bytes) {
                Ok(parsed) => parsed,
                Err(error) => {
                    return self.fail(match error {
                        parser::Error::AbiMismatch => b"artifactAbiMismatch",
                        parser::Error::TargetMismatch => b"artifactTargetMismatch",
                        _ => b"artifactDescriptionMissing",
                    });
                }
            };
            if !native_requirements_present(parsed, path) {
                return self.fail(b"nativeRequirementUnavailable");
            }
            self.materialize(parsed, path)
        }
    }
    unsafe fn materialize(&self, parsed: ParsedDescriptor<'_>, path: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            let result = object(self.send(
                self.model,
                b"allocate:bytes:",
                &[
                    self.descriptor_vt,
                    abi::size(13 * size_of::<Oop>()).unwrap(),
                ],
            )?)?;
            self.rooted(&[result], || {
                // Publish each initialized edge before the next constructor can collect.
                for index in 0..13 {
                    let value = match index {
                        KIND => self.symbol(parsed.kind)?,
                        IDENTITY => self.symbol(parsed.identity)?,
                        ABI => self.string(parsed.artifact_abi)?,
                        TARGET => self.string(parsed.target)?,
                        SOURCE => self.string(parsed.source_digest)?,
                        REVISION => self.string(parsed.revision_digest)?,
                        PATH => self.string(path)?,
                        REQUIRES => self.string(parsed.requires.encoded_bytes())?,
                        NATIVE_REQUIRES => self.string(parsed.native_requires.encoded_bytes())?,
                        CAPABILITIES => self.string(parsed.capabilities.encoded_bytes())?,
                        REQUIRES_COUNT => self.number(parsed.requires.count())?,
                        NATIVE_REQUIRES_COUNT => self.number(parsed.native_requires.count())?,
                        CAPABILITIES_COUNT => self.number(parsed.capabilities.count())?,
                        _ => unreachable!(),
                    };
                    self.send(
                        self.model,
                        b"slotAt:in:put:",
                        &[abi::size(index).unwrap(), result, value],
                    )?;
                }
                self.last_failure.set(self.nil);
                Ok(result)
            })
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum ListKind {
    Roots,
    Requires,
    NativeRequires,
    Capabilities,
}
fn list_field(bytes: &[u8], index: usize) -> Option<&[u8]> {
    let mut position = 0usize;
    for current in 0..=index {
        let end = position.checked_add(4)?;
        let length = u32::from_le_bytes(bytes.get(position..end)?.try_into().ok()?) as usize;
        position = end;
        let value_end = position.checked_add(length)?;
        let value = bytes.get(position..value_end)?;
        position = value_end;
        if current == index {
            return Some(value);
        }
    }
    None
}

fn path_from_bytes(bytes: &[u8]) -> &Path {
    Path::new(OsStr::from_bytes(bytes))
}

fn read_artifact(path: &Path) -> Result<Vec<u8>, ()> {
    let file = fs::File::open(path).map_err(|_| ())?;
    if file.metadata().map_err(|_| ())?.len() > parser::MAX_ARTIFACT_BYTES as u64 {
        return Err(());
    }
    let mut bytes = Vec::new();
    file.take((parser::MAX_ARTIFACT_BYTES as u64) + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    if bytes.len() > parser::MAX_ARTIFACT_BYTES {
        return Err(());
    }
    Ok(bytes)
}

fn native_requirements_present(descriptor: ParsedDescriptor<'_>, path: &[u8]) -> bool {
    let artifact_path = path_from_bytes(path);
    let directory = artifact_path.parent().unwrap_or_else(|| Path::new("."));
    descriptor.native_requires.iter().all(|requirement| {
        !requirement.is_empty()
            && !requirement.contains(&0)
            && !requirement.contains(&b'/')
            && fs::metadata(directory.join(OsStr::from_bytes(requirement))).is_ok()
    })
}

unsafe fn execute(s: &State, r: Oop, args: &[Oop], op: usize) -> Result<Oop, Oop> {
    unsafe {
        match op {
            0 => s.inspect(args),
            21 => s.inspect_snapshot(args),
            1 => Ok(s.last_failure.get()),
            2..=8 => Ok(s.slot(r, op - 2)?),
            9..=20 => {
                let kind = [
                    ListKind::Roots,
                    ListKind::Requires,
                    ListKind::NativeRequires,
                    ListKind::Capabilities,
                ][(op - 9) / 3];
                match (op - 9) % 3 {
                    0 => s.vector(r, kind),
                    1 => Ok(s.number(s.count(r, kind)?)?),
                    _ => {
                        let [index] = args else {
                            return Ok(s.nil);
                        };
                        let Some(index) = (!index.is_null()).then_some(*index) else {
                            return Ok(s.nil);
                        };
                        if !s.family(index, s.integer_vt)? {
                            return Ok(s.nil);
                        }
                        let index = s.word(index)?;
                        if (index as isize) < 0 {
                            return Ok(s.nil);
                        }
                        Ok(s.value_at(r, kind, index)?)
                    }
                }
            }
            _ => Err(invalid()),
        }
    }
}

macro_rules! entry {
    ($name:ident,$op:expr) => {
        unsafe extern "C" fn $name(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
            (|| unsafe {
                let s = state(c)?;
                let Some(a) = borrowed_slice(a, n) else {
                    return if $op == 0 {
                        s.fail(b"artifactPathInvalid")
                    } else {
                        Err(invalid())
                    };
                };
                execute(s, r, a, $op)
            })()
            .unwrap_or_else(abi::fault_reference)
        }
    };
}
entry!(inspect, 0);
entry!(inspect_snapshot, 21);
entry!(last_failure, 1);
entry!(kind, 2);
entry!(identity, 3);
entry!(artifact_abi, 4);
entry!(target, 5);
entry!(source_digest, 6);
entry!(revision_digest, 7);
entry!(path, 8);
entry!(roots_list, 9);
entry!(roots_count, 10);
entry!(root_at, 11);
entry!(requires, 12);
entry!(requires_count, 13);
entry!(require_at, 14);
entry!(native_requires, 15);
entry!(native_requires_count, 16);
entry!(native_require_at, 17);
entry!(capabilities, 18);
entry!(capabilities_count, 19);
entry!(capability_at, 20);

unsafe extern "C" fn roots(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [visitor] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let mut values = vec![
            s.ns,
            s.roots,
            s.model,
            s.string,
            s.integer,
            s.string_vt,
            s.integer_vt,
            s.nil,
            s.artifact,
            s.artifact_vt,
            s.descriptor_vt,
            s.last_failure.get(),
        ];
        values.extend(s.names.values().copied());
        values.extend(s.scoped.borrow().iter().copied());
        for value in values {
            s.send(*visitor, b"visit:", &[value])?;
        }
        Ok(ptr::null_mut())
    })()
    .unwrap_or_else(abi::fault_reference)
}
macro_rules! literals{($($n:literal),*$(,)?)=>{const NAMES:&[&[u8]]=&[$($n),*];static LITERALS:&[Literal]=&[$(Literal::new($n)),*];};}
literals![
    b"Artifact",
    b"ArtifactDescriptorVt",
    b"ArtifactVt",
    b"Integer",
    b"IntegerVt",
    b"ObjectModel",
    b"ObjectVt",
    b"Roots",
    b"String",
    b"StringVt",
    b"Vector",
    b"add:",
    b"addMethod",
    b"allocate",
    b"allocate:bytes:",
    b"sizeWord",
    b"atWord:",
    b"artifactAbi",
    b"artifactAbiMismatch",
    b"artifactDescriptionMissing",
    b"artifactPathInvalid",
    b"artifactTargetMismatch",
    b"artifactUnavailable",
    b"at:",
    b"at:put:",
    b"behaviorOf:",
    b"bytes",
    b"capabilities",
    b"capabilitiesCount",
    b"capabilityAt:",
    b"delegated",
    b"fromBytes:",
    b"fromWord:",
    b"high32",
    b"identity",
    b"inspect:",
    b"inspectBytes:from:",
    b"kind",
    b"kind:message:",
    b"lastFailure",
    b"low32",
    b"methodAt:",
    b"nameWithBytes",
    b"nativeRequireAt:",
    b"nativeRequirementUnavailable",
    b"nativeRequires",
    b"nativeRequiresCount",
    b"new",
    b"nil",
    b"ownerOf:put:",
    b"path",
    b"register:",
    b"release:",
    b"requireAt:",
    b"requires",
    b"requiresCount",
    b"retain:",
    b"revisionDigest",
    b"rootAt:",
    b"roots",
    b"rootsCount",
    b"rootsDo:",
    b"sequence",
    b"setLayoutOf:to:rest:",
    b"slotAt:in:",
    b"slotAt:in:put:",
    b"sourceDigest",
    b"target",
    b"visit:",
    b"withContext:",
    b"wordOf:",
    b"copyByteValuesFrom:count:to:",
    b"appendByteValues",
    b"objectBehavior",
    b"releaseTokensProtecting",
    b"Bytes",
    b"fromByteValues",
    b"receiveByteValues",
];
const fn index(n: &[u8]) -> usize {
    let mut i = 0;
    while i < NAMES.len() {
        let c = NAMES[i];
        if c.len() == n.len() {
            let mut j = 0;
            while j < n.len() && c[j] == n[j] {
                j += 1;
            }
            if j == n.len() {
                return i;
            }
        }
        i += 1;
    }
    panic!("missing literal")
}
static METHODS: &[MethodSpec] = &[
    MethodSpec {
        name: index(b"inspect:"),
        entry: Some(inspect),
    },
    MethodSpec {
        name: index(b"inspectBytes:from:"),
        entry: Some(inspect_snapshot),
    },
    MethodSpec {
        name: index(b"lastFailure"),
        entry: Some(last_failure),
    },
    MethodSpec {
        name: index(b"rootsDo:"),
        entry: Some(roots),
    },
    MethodSpec {
        name: index(b"kind"),
        entry: Some(kind),
    },
    MethodSpec {
        name: index(b"identity"),
        entry: Some(identity),
    },
    MethodSpec {
        name: index(b"artifactAbi"),
        entry: Some(artifact_abi),
    },
    MethodSpec {
        name: index(b"target"),
        entry: Some(target),
    },
    MethodSpec {
        name: index(b"sourceDigest"),
        entry: Some(source_digest),
    },
    MethodSpec {
        name: index(b"revisionDigest"),
        entry: Some(revision_digest),
    },
    MethodSpec {
        name: index(b"path"),
        entry: Some(path),
    },
    MethodSpec {
        name: index(b"roots"),
        entry: Some(roots_list),
    },
    MethodSpec {
        name: index(b"rootsCount"),
        entry: Some(roots_count),
    },
    MethodSpec {
        name: index(b"rootAt:"),
        entry: Some(root_at),
    },
    MethodSpec {
        name: index(b"requires"),
        entry: Some(requires),
    },
    MethodSpec {
        name: index(b"requiresCount"),
        entry: Some(requires_count),
    },
    MethodSpec {
        name: index(b"requireAt:"),
        entry: Some(require_at),
    },
    MethodSpec {
        name: index(b"nativeRequires"),
        entry: Some(native_requires),
    },
    MethodSpec {
        name: index(b"nativeRequiresCount"),
        entry: Some(native_requires_count),
    },
    MethodSpec {
        name: index(b"nativeRequireAt:"),
        entry: Some(native_require_at),
    },
    MethodSpec {
        name: index(b"capabilities"),
        entry: Some(capabilities),
    },
    MethodSpec {
        name: index(b"capabilitiesCount"),
        entry: Some(capabilities_count),
    },
    MethodSpec {
        name: index(b"capabilityAt:"),
        entry: Some(capability_at),
    },

    MethodSpec { name: NAMES.len()-1, entry: Some(abi::byte_transfer::receive_entry) },
];
const DESCRIPTOR_METHODS: &[&[u8]] = &[
    b"kind",
    b"identity",
    b"artifactAbi",
    b"target",
    b"sourceDigest",
    b"revisionDigest",
    b"path",
    b"roots",
    b"rootsCount",
    b"rootAt:",
    b"requires",
    b"requiresCount",
    b"requireAt:",
    b"nativeRequires",
    b"nativeRequiresCount",
    b"nativeRequireAt:",
    b"capabilities",
    b"capabilitiesCount",
    b"capabilityAt:",
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
    let id = unsafe { *p };
    let Some([ns]) = (unsafe { borrowed_slice(a, count) }) else {
        return id.invalid;
    };
    let Some(symbols) = (unsafe { borrowed_slice(l, n) }) else {
        return id.invalid;
    };
    if n != NAMES.len() || ns.is_null() {
        return id.invalid;
    }
    let old = INSTANCE.load(Ordering::Acquire);
    if !old.is_null() {
        let s = unsafe { &*old };
        return if s.ready.get() && s.id.root == id.root && s.ns == *ns {
            s.artifact
        } else {
            id.invalid
        };
    }
    if BUSY.swap(true, Ordering::AcqRel) {
        return id.invalid;
    }
    let _constructing = Constructing;
    (|| unsafe {
        let names: HashMap<_, _> = NAMES.iter().copied().zip(symbols.iter().copied()).collect();
        let send = |r, n: &[u8], a: &[Oop]| id.send(r, names[n], a);
        let named = |n: &[u8]| object(send(*ns, b"at:", &[names[n]])?);
        let roots = named(b"Roots")?;
        let mut tokens = Vec::new();
        let result = (|| {
            let mut keep = |v| -> Result<Oop, Oop> {
                tokens.push(send(roots, b"retain:", &[v])?);
                Ok(v)
            };
            keep(*ns)?;
            keep(roots)?;
            let model = keep(named(b"ObjectModel")?)?;
            let string = keep(named(b"String")?)?;
            let integer = keep(named(b"Integer")?)?;
            let string_vt = keep(named(b"StringVt")?)?;
            let integer_vt = keep(named(b"IntegerVt")?)?;
            let nil = keep(named(b"nil")?)?;
            let parent = keep(named(b"ObjectVt")?)?;
            let artifact_vt = keep(send(parent, b"delegated", &[])?)?;
            let artifact = keep(send(artifact_vt, b"allocate", &[abi::size(0).unwrap()])?)?;
            let descriptor_vt = keep(send(parent, b"delegated", &[])?)?;
            let layout = keep(send(id.root, b"sequence", &[abi::size(1).unwrap(); 13])?)?;
            send(
                model,
                b"setLayoutOf:to:rest:",
                &[descriptor_vt, layout, abi::size(0).unwrap()],
            )?;
            let s = Box::into_raw(Box::new(State {
                byte_reader: abi::byte_transfer::Reader::new(id, image, roots, |n| names[n])?,
                id,
                ns: *ns,
                roots,
                model,
                string,
                integer,
                string_vt,
                integer_vt,
                nil,
                artifact,
                artifact_vt,
                descriptor_vt,
                last_failure: Cell::new(nil),
                names: names.clone(),
                scoped: RefCell::new(Vec::new()),
                ready: Cell::new(false),
            }));
            INSTANCE.store(s, Ordering::Release);
            let s = &*s;
            for spec in &METHODS[..METHODS.len()-1] {
                let name = NAMES[spec.name];
                let method = send(image, b"methodAt:", &[symbols[spec.name]])?;
                let method = send(method, b"withContext:", &[artifact])?;
                send(
                    if DESCRIPTOR_METHODS.contains(&name) {
                        descriptor_vt
                    } else {
                        artifact_vt
                    },
                    b"addMethod",
                    &[symbols[spec.name], method],
                )?;
            }
            send(roots, b"register:", &[artifact])?;
            send(id.root, b"ownerOf:put:", &[artifact_vt, artifact])?;
            for (name, v) in [
                (b"Artifact".as_slice(), artifact),
                (b"ArtifactVt", artifact_vt),
                (b"ArtifactDescriptorVt", descriptor_vt),
            ] {
                send(*ns, b"at:put:", &[names[name], v])?;
            }
            s.ready.set(true);
            Ok(artifact)
        })();
        for token in tokens {
            send(roots, b"release:", &[token])?;
        }
        result
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
