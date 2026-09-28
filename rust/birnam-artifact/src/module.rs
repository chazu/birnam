//! Artifact inspection. Object storage, dependencies and GC use Id messages.
use crate as parser;
use crate::Descriptor as ParsedDescriptor;
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as native, Method, NativeEntry, Operand, Outcome, Protocol},
};
use core::{cell::Cell, ffi::c_void, ptr};
use std::{
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
    thread: std::thread::ThreadId,
}
static INSTANCE: AtomicPtr<State> = AtomicPtr::new(ptr::null_mut());
static BUSY: AtomicBool = AtomicBool::new(false);
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
fn object(v: Operand) -> Result<Oop, Outcome> {
    v.as_object().filter(|v| !v.is_null()).ok_or_else(invalid)
}
unsafe fn state<'a>(c: *mut c_void) -> Result<&'a State, Outcome> {
    let s = unsafe { &*c.cast::<State>() };
    if s.thread != std::thread::current().id() {
        return Err(invalid());
    }
    Ok(s)
}
impl State {
    unsafe fn rooted<T>(
        &self,
        values: &[Oop],
        f: impl FnOnce() -> Result<T, Outcome>,
    ) -> Result<T, Outcome> {
        unsafe {
            let mut tokens = Vec::new();
            let result = (|| {
                for &value in values {
                    tokens.push(
                        self.id
                            .send_named(self.roots, b"retain:", &[Operand::object(value)])?
                            .as_word()
                            .filter(|t| *t != 0)
                            .ok_or_else(invalid)?,
                    );
                }
                f()
            })();
            for token in tokens {
                if self
                    .id
                    .send_named(self.roots, b"release:", &[Operand::word(token)])?
                    .as_word()
                    != Some(1)
                {
                    return Err(invalid());
                }
            }
            result
        }
    }
    unsafe fn named(&self, name: &[u8]) -> Result<Operand, Outcome> {
        unsafe {
            self.id.send_named(
                self.ns,
                b"at:",
                &[Operand::object(self.id.intern_name(name)?)],
            )
        }
    }
    unsafe fn slot(&self, r: Oop, index: usize) -> Result<Oop, Outcome> {
        unsafe {
            object(self.id.send_named(
                self.model,
                b"slotAt:in:",
                &[Operand::word(index), Operand::object(r)],
            )?)
        }
    }
    unsafe fn bytes(&self, r: Oop) -> Result<Vec<u8>, Outcome> {
        unsafe {
            let b = self
                .id
                .send_named(r, b"bytes", &[])?
                .as_bytes()
                .ok_or_else(invalid)?;
            // A String borrow cannot survive another send, including rooting.
            Ok(borrowed_bytes(b.pointer, b.length)
                .ok_or_else(invalid)?
                .to_vec())
        }
    }
    unsafe fn symbol(&self, b: &[u8]) -> Result<Oop, Outcome> {
        // Preserve the former C-string projection without retaining its native API.
        let end = b.iter().position(|b| *b == 0).unwrap_or(b.len());
        unsafe { self.id.intern_name(&b[..end]) }
    }
    unsafe fn string(&self, b: &[u8]) -> Result<Oop, Outcome> {
        unsafe {
            object(
                self.id
                    .send_named(self.string, b"fromBytes:", &[Operand::bytes(b)])?,
            )
        }
    }
    unsafe fn number(&self, n: usize) -> Result<Oop, Outcome> {
        unsafe {
            object(
                self.id
                    .send_named(self.integer, b"fromWord:", &[Operand::word(n)])?,
            )
        }
    }
    unsafe fn word(&self, r: Oop) -> Result<usize, Outcome> {
        unsafe {
            self.id
                .send_named(self.integer, b"wordOf:", &[Operand::object(r)])?
                .as_word()
                .ok_or_else(invalid)
        }
    }
    unsafe fn family(&self, r: Oop, vt: Oop) -> Result<bool, Outcome> {
        unsafe {
            Ok(object(self.id.send_named(
                self.id.receiver,
                b"behaviorOf:",
                &[Operand::object(r)],
            )?)? == vt)
        }
    }
    unsafe fn fail(&self, name: &[u8]) -> Result<Operand, Outcome> {
        self.last_failure.set(unsafe { self.id.intern_name(name)? });
        Ok(Operand::object(self.nil))
    }
    unsafe fn count(&self, r: Oop, kind: ListKind) -> Result<usize, Outcome> {
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
    unsafe fn value_at(&self, r: Oop, kind: ListKind, index: usize) -> Result<Oop, Outcome> {
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
            let bytes = self.rooted(&[buffer], || self.bytes(buffer))?;
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
    unsafe fn vector(&self, r: Oop, kind: ListKind) -> Result<Operand, Outcome> {
        unsafe {
            let vector = self.named(b"Vector")?;
            let Some(vector) = vector.as_object().filter(|v| !v.is_null()) else {
                return Ok(Operand::object(self.nil));
            };
            self.rooted(&[vector], || {
                let mut result = object(self.id.send_named(vector, b"new", &[])?)?;
                // Replace the accumulator root each iteration; add: may return a new Vector.
                let mut token = self
                    .id
                    .send_named(self.roots, b"retain:", &[Operand::object(result)])?
                    .as_word()
                    .ok_or_else(invalid)?;
                let outcome = (|| {
                    let count = self.count(r, kind)?;
                    for index in 0..count {
                        let value = self.value_at(r, kind, index)?;
                        let next = self.rooted(&[value], || {
                            object(self.id.send_named(
                                result,
                                b"add:",
                                &[Operand::object(value)],
                            )?)
                        })?;
                        let next_token = self
                            .id
                            .send_named(self.roots, b"retain:", &[Operand::object(next)])?
                            .as_word()
                            .ok_or_else(invalid)?;
                        self.id
                            .send_named(self.roots, b"release:", &[Operand::word(token)])?;
                        result = next;
                        token = next_token;
                    }
                    Ok(Operand::object(result))
                })();
                self.id
                    .send_named(self.roots, b"release:", &[Operand::word(token)])?;
                outcome
            })
        }
    }
    unsafe fn inspect(&self, args: &[Operand]) -> Result<Operand, Outcome> {
        unsafe {
            let [path] = args else {
                return self.fail(b"artifactPathInvalid");
            };
            let Some(path) = path.as_object().filter(|v| !v.is_null()) else {
                return self.fail(b"artifactPathInvalid");
            };
            if !self.family(path, self.string_vt)? {
                return self.fail(b"artifactPathInvalid");
            }
            let path = self.bytes(path)?;
            let bytes = match read_artifact(path_from_bytes(&path)) {
                Ok(bytes) => bytes,
                Err(()) => return self.fail(b"artifactUnavailable"),
            };
            let parsed = match parser::inspect(&bytes) {
                Ok(parsed) => parsed,
                Err(error) => {
                    return self.fail(match error {
                        parser::Error::AbiMismatch => b"artifactAbiMismatch",
                        parser::Error::TargetMismatch => b"artifactTargetMismatch",
                        _ => b"artifactDescriptionMissing",
                    });
                }
            };
            if !native_requirements_present(parsed, &path) {
                return self.fail(b"nativeRequirementUnavailable");
            }
            self.materialize(parsed, &path)
        }
    }
    unsafe fn materialize(
        &self,
        parsed: ParsedDescriptor<'_>,
        path: &[u8],
    ) -> Result<Operand, Outcome> {
        unsafe {
            let result = object(self.id.send_named(
                self.model,
                b"allocate:bytes:",
                &[
                    Operand::object(self.descriptor_vt),
                    Operand::word(13 * size_of::<Oop>()),
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
                    self.rooted(&[value], || {
                        self.id.send_named(
                            self.model,
                            b"slotAt:in:put:",
                            &[
                                Operand::word(index),
                                Operand::object(result),
                                Operand::object(value),
                            ],
                        )
                    })?;
                }
                self.last_failure.set(self.nil);
                Ok(Operand::object(result))
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

unsafe extern "C" fn roots(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let s = state(c)?;
        let Some([visitor]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let visitor = object(*visitor)?;
        for value in [
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
        ] {
            s.id.send_named(visitor, b"visit:", &[Operand::object(value)])?;
        }
        Ok(Operand::unit())
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
unsafe fn execute(s: &State, r: Oop, args: &[Operand], op: usize) -> Result<Operand, Outcome> {
    unsafe {
        match op {
            0 => s.inspect(args),
            1 => Ok(Operand::object(s.last_failure.get())),
            2..=8 => Ok(Operand::object(s.slot(r, op - 2)?)),
            9..=20 => {
                let kind = [
                    ListKind::Roots,
                    ListKind::Requires,
                    ListKind::NativeRequires,
                    ListKind::Capabilities,
                ][(op - 9) / 3];
                match (op - 9) % 3 {
                    0 => s.vector(r, kind),
                    1 => Ok(Operand::object(s.number(s.count(r, kind)?)?)),
                    _ => {
                        let [index] = args else {
                            return Ok(Operand::object(s.nil));
                        };
                        let Some(index) = index.as_object().filter(|v| !v.is_null()) else {
                            return Ok(Operand::object(s.nil));
                        };
                        if !s.family(index, s.integer_vt)? {
                            return Ok(Operand::object(s.nil));
                        }
                        let index = s.word(index)?;
                        if (index as isize) < 0 {
                            return Ok(Operand::object(s.nil));
                        }
                        Ok(Operand::object(s.value_at(r, kind, index)?))
                    }
                }
            }
            _ => Err(invalid()),
        }
    }
}
macro_rules! entry {
    ($name:ident, $op:expr) => {
        unsafe extern "C" fn $name(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
            (|| unsafe {
                let s = state(c)?;
                let Some(args) = borrowed_slice(a, n) else {
                    return if $op == 0 {
                        s.fail(b"artifactPathInvalid")
                    } else {
                        Err(invalid())
                    };
                };
                let mut values = vec![r];
                values.extend(
                    args.iter()
                        .filter_map(|v| v.as_object())
                        .filter(|v| !v.is_null()),
                );
                s.rooted(&values, || execute(s, r, args, $op))
            })()
            .map_or_else(|e| e, Outcome::loaded)
        }
    };
}
entry!(inspect, 0);
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
unsafe fn construct(id: Protocol, ns: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let named = |name: &[u8]| {
            object(id.send_named(ns, b"at:", &[Operand::object(id.intern_name(name)?)])?)
        };
        let roots = named(b"Roots")?;
        let mut tokens = Vec::new();
        let result = (|| {
            let mut keep = |value| -> Result<Oop, Outcome> {
                tokens.push(
                    id.send_named(roots, b"retain:", &[Operand::object(value)])?
                        .as_word()
                        .filter(|t| *t != 0)
                        .ok_or_else(invalid)?,
                );
                Ok(value)
            };
            keep(ns)?;
            keep(roots)?;
            let model = keep(named(b"ObjectModel")?)?;
            let string = keep(named(b"String")?)?;
            let integer = keep(named(b"Integer")?)?;
            let string_vt = keep(named(b"StringVt")?)?;
            let integer_vt = keep(named(b"IntegerVt")?)?;
            let nil = keep(named(b"nil")?)?;
            let parent = keep(named(b"ObjectVt")?)?;
            let artifact_vt = keep(object(id.send_named(parent, b"delegated", &[])?)?)?;
            let artifact = keep(object(id.send_named(
                artifact_vt,
                b"allocate",
                &[Operand::word(0)],
            )?)?)?;
            let descriptor_vt = keep(object(id.send_named(parent, b"delegated", &[])?)?)?;
            let pointer = Box::into_raw(Box::new(State {
                id,
                ns,
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
                thread: std::thread::current().id(),
            }));
            // Once entries are installed their context must remain valid, even if publication fails.
            id.send_named(
                model,
                b"setLayoutOf:to:rest:",
                &[
                    Operand::object(descriptor_vt),
                    Operand::bytes(&[1; 13]),
                    Operand::word(0),
                ],
            )?;
            for (vt, name, method) in [
                (artifact_vt, b"inspect:".as_slice(), inspect as Method),
                (
                    artifact_vt,
                    b"lastFailure".as_slice(),
                    last_failure as Method,
                ),
                (descriptor_vt, b"kind".as_slice(), kind as Method),
                (descriptor_vt, b"identity".as_slice(), identity as Method),
                (
                    descriptor_vt,
                    b"artifactAbi".as_slice(),
                    artifact_abi as Method,
                ),
                (descriptor_vt, b"target".as_slice(), target as Method),
                (
                    descriptor_vt,
                    b"sourceDigest".as_slice(),
                    source_digest as Method,
                ),
                (
                    descriptor_vt,
                    b"revisionDigest".as_slice(),
                    revision_digest as Method,
                ),
                (descriptor_vt, b"path".as_slice(), path as Method),
                (descriptor_vt, b"roots".as_slice(), roots_list as Method),
                (
                    descriptor_vt,
                    b"rootsCount".as_slice(),
                    roots_count as Method,
                ),
                (descriptor_vt, b"rootAt:".as_slice(), root_at as Method),
                (descriptor_vt, b"requires".as_slice(), requires as Method),
                (
                    descriptor_vt,
                    b"requiresCount".as_slice(),
                    requires_count as Method,
                ),
                (
                    descriptor_vt,
                    b"requireAt:".as_slice(),
                    require_at as Method,
                ),
                (
                    descriptor_vt,
                    b"nativeRequires".as_slice(),
                    native_requires as Method,
                ),
                (
                    descriptor_vt,
                    b"nativeRequiresCount".as_slice(),
                    native_requires_count as Method,
                ),
                (
                    descriptor_vt,
                    b"nativeRequireAt:".as_slice(),
                    native_require_at as Method,
                ),
                (
                    descriptor_vt,
                    b"capabilities".as_slice(),
                    capabilities as Method,
                ),
                (
                    descriptor_vt,
                    b"capabilitiesCount".as_slice(),
                    capabilities_count as Method,
                ),
                (
                    descriptor_vt,
                    b"capabilityAt:".as_slice(),
                    capability_at as Method,
                ),
                (artifact_vt, b"rootsDo:", self::roots as Method),
            ] {
                id.send_named(
                    vt,
                    b"addMethod",
                    &[
                        Operand::object(id.intern_name(name)?),
                        Operand::entry(NativeEntry::new(method, pointer.cast())),
                    ],
                )?;
            }
            id.send_named(roots, b"register:", &[Operand::object(artifact)])?;
            id.send_named(
                id.receiver,
                b"ownerOf:put:",
                &[Operand::object(artifact_vt), Operand::object(artifact)],
            )?;
            for (name, value) in [
                (b"Artifact".as_slice(), artifact),
                (b"ArtifactVt", artifact_vt),
                (b"ArtifactDescriptorVt", descriptor_vt),
            ] {
                id.send_named(
                    ns,
                    b"at:put:",
                    &[
                        Operand::object(id.intern_name(name)?),
                        Operand::object(value),
                    ],
                )?;
            }
            INSTANCE.store(pointer, Ordering::Release);
            Ok(Operand::object(artifact))
        })();
        for token in tokens {
            id.send_named(roots, b"release:", &[Operand::word(token)])?;
        }
        result
    }
}
unsafe extern "C" fn initialize(id: *const Protocol, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        if !native::valid_protocol(id) {
            return Err(invalid());
        }
        let Some([ns]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let ns = object(*ns)?;
        let existing = INSTANCE.load(Ordering::Acquire);
        if !existing.is_null() {
            let s = &*existing;
            return if s.id.same_contract(&*id)
                && s.ns == ns
                && s.thread == std::thread::current().id()
            {
                Ok(Operand::object(s.artifact))
            } else {
                Err(invalid())
            };
        }
        if BUSY.swap(true, Ordering::AcqRel) {
            return Err(invalid());
        }
        let result = construct(*id, ns);
        BUSY.store(false, Ordering::Release);
        result
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module {
    version: native::VERSION,
    byte_size: size_of::<native::Module>() as u32,
    initialize: Some(initialize),
};
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
