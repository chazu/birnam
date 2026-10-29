//! Native I/O uses ordinary Id sends and owner-local temporary roots.
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
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};
struct State {
    id: Protocol,
    ns: Oop,
    nil: Oop,
    names: HashMap<&'static [u8], Oop>,
    scoped: RefCell<Vec<Oop>>,
    ready: Cell<bool>,
    string: Oop,
    string_vt: Oop,
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
    if c == s.ns { Ok(s) } else { Err(invalid()) }
}
struct Scope<'a> {
    s: &'a State,
    start: usize,
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        self.s.scoped.borrow_mut().truncate(self.start);
    }
}
impl State {
    unsafe fn send(&self, r: Oop, n: &[u8], a: &[Oop]) -> Result<Oop, Oop> {
        unsafe { self.id.send(r, self.names[n], a) }
    }
    unsafe fn rooted<T>(
        &self,
        values: &[Oop],
        f: impl FnOnce() -> Result<T, Oop>,
    ) -> Result<T, Oop> {
        let scope = Scope {
            s: self,
            start: self.scoped.borrow().len(),
        };
        self.scoped.borrow_mut().extend_from_slice(values);
        let result = f();
        drop(scope);
        result
    }
    unsafe fn bytes(&self, value: Oop) -> Result<Option<Vec<u8>>, Oop> {
        unsafe {
            if self.send(self.id.root, b"behaviorOf:", &[value])? != self.string_vt {
                return Ok(None);
            }
            let bytes = self.send(value, b"bytes", &[])?;
            if bytes.is_null() {
                return Ok(None);
            }
            self.id
                .read_bytes(
                    bytes,
                    self.names[b"sizeWord".as_slice()],
                    self.names[b"atWord:".as_slice()],
                )
                .map(Some)
        }
    }
    unsafe fn make_string(&self, bytes: &[u8]) -> Result<Oop, Oop> {
        unsafe { self.from_bytes(self.string, bytes) }
    }
    unsafe fn error(&self, message: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            let message = object(self.make_string(message)?)?;
            self.rooted(&[message], || {
                let factory = self.send(self.ns, b"at:", &[self.names[b"Error".as_slice()]])?;
                if factory.is_null() {
                    return Ok(self.nil);
                }
                self.send(
                    factory,
                    b"kind:message:",
                    &[self.names[b"ioError".as_slice()], message],
                )
            })
        }
    }
    unsafe fn from_bytes(&self, factory: Oop, bytes: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            let buffers = object(self.send(
                self.ns, b"at:", &[self.names[b"Bytes".as_slice()]],
            )?)?;
            self.rooted(&[factory, buffers], || {
                // A file-sized Sequence makes GC trace one reference per byte.
                // Keep byte storage with Bytes and retain the current constructor.
                let mut chunks = bytes.chunks(8192);
                let packet = |chunk: &[u8]| {
                    chunk.iter().map(|&b| abi::size(b as usize).unwrap()).collect::<Vec<_>>()
                };
                let mut source = object(self.send(
                    buffers, b"fromByteValues", &packet(chunks.next().unwrap_or(&[])),
                )?)?;
                let source_root = self.scoped.borrow().len();
                self.scoped.borrow_mut().push(source);
                for chunk in chunks {
                    source = object(self.send(source, b"appendByteValues", &packet(chunk))?)?;
                    self.scoped.borrow_mut()[source_root] = source;
                }
                self.send(factory, b"fromBytes:", &[source])
            })
        }
    }
}
use std::{
    fs,
    io::{self, Read},
    path::Path,
};
const MAX_FILE_BYTES: u64 = 1 << 30;
fn path_from_bytes(bytes: &[u8]) -> Option<&Path> {
    std::str::from_utf8(bytes).ok().map(Path::new)
}
fn read_bounded(reader: impl Read, limit: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::other("file exceeds read limit"));
    }
    Ok(bytes)
}
unsafe extern "C" fn read_file(_id: Oop, c: Oop, _r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        if n != 1 {
            return Ok(s.nil);
        }
        let [path] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let path = object(*path)?;
        (|| {
            let Some(path_bytes) = s.bytes(path)? else {
                return Ok(s.nil);
            };
            let Some(path) = path_from_bytes(&path_bytes) else {
                return s.error(b"could not read file");
            };
            let Ok(file) = fs::File::open(path) else {
                return s.error(b"could not read file");
            };
            let Ok(bytes) = read_bounded(file, MAX_FILE_BYTES) else {
                return s.error(b"could not read file");
            };
            s.make_string(&bytes)
        })()
    })()
    .unwrap_or_else(abi::fault_reference)
}
// Reusable bounded binary I/O: nil on file/encoding failure. The snapshot
// carries owned path/content Bytes, with no source evaluation or registration.
unsafe extern "C" fn read_snapshot(_id: Oop, c: Oop, _r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let Some([path]) = borrowed_slice(a, n) else {
            return Ok(s.nil);
        };
        let path = object(*path)?;
        (|| {
            let Some(path_bytes) = s.bytes(path)? else {
                return Ok(s.nil);
            };
            let Some(name) = path_from_bytes(&path_bytes) else {
                return Ok(s.nil);
            };
            let bytes =
                match fs::File::open(name).and_then(|file| read_bounded(file, MAX_FILE_BYTES)) {
                    Ok(bytes) => bytes,
                    Err(_) => return Ok(s.nil),
                };
            let named = |name: &[u8]| -> Result<Oop, Oop> {
                object(s.send(s.ns, b"at:", &[s.names[name]])?)
            };
            let factory = named(b"Bytes")?;
            s.rooted(&[factory], || {
                let name = object(s.from_bytes(factory, &path_bytes)?)?;
                s.rooted(&[name], || {
                    let contents = object(s.from_bytes(factory, &bytes)?)?;
                    s.rooted(&[contents], || {
                        let vector = named(b"Vector")?;
                        s.rooted(&[vector], || {
                            let out = object(s.send(vector, b"new", &[])?)?;
                            s.rooted(&[out], || {
                                let out = object(s.send(out, b"add:", &[name])?)?;
                                s.rooted(&[out], || s.send(out, b"add:", &[contents]))
                            })
                        })
                    })
                })
            })
        })()
    })()
    .unwrap_or_else(abi::fault_reference)
}
// The compiler sends this operation with a resolved filename. Source-relative
// validation belongs to the compilation form; this owner only performs bounded I/O.
unsafe extern "C" fn embed_file(_id: Oop, c: Oop, _r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let Some([path]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let path = object(*path)?;
        (|| {
            let Some(path_bytes) = s.bytes(path)? else {
                return s.error(b"embedFile: expected a String path");
            };
            let Some(path) = path_from_bytes(&path_bytes) else {
                return s.error(b"embedFile: invalid path");
            };
            let Ok(file) = fs::File::open(path) else {
                return s.error(b"embedFile: could not read the file");
            };
            let Ok(bytes) = read_bounded(file, 16 * 1024 * 1024) else {
                return s.error(b"embedFile: could not read the file within the 16 MiB limit");
            };
            let factory = object(s.send(s.ns, b"at:", &[s.names[b"Bytes".as_slice()]])?)?;
            s.rooted(&[factory], || s.from_bytes(factory, &bytes))
        })()
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn write_file(_id: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        if n != 2 {
            return Ok(s.nil);
        }
        let [path, contents] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let path = object(*path)?;
        let contents = object(*contents)?;
        (|| {
            // Copy the filename before the next send. Contents callbacks cannot
            // redirect this write by mutating the earlier borrowed byte view.
            let Some(path_bytes) = s.bytes(path)? else {
                return Ok(s.nil);
            };
            let Some(contents) = s.bytes(contents)? else {
                return Ok(s.nil);
            };
            // Preserve argument-read/error precedence: validate after both reads.
            let Some(path) = path_from_bytes(&path_bytes) else {
                return s.error(b"could not write file");
            };
            if fs::write(path, contents).is_err() {
                return s.error(b"could not write file");
            }
            Ok(r)
        })()
    })()
    .unwrap_or_else(abi::fault_reference)
}
// Candidate cleanup only removes the exact bytes that its caller admitted.
// A missing path is already removed; a replaced author file is left untouched.
unsafe extern "C" fn remove_matching(_id: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let Some([path, contents]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let path = object(*path)?;
        let contents = object(*contents)?;
        (|| {
            let Some(path_bytes) = s.bytes(path)? else {
                return s.error(b"removeFile: expected a String path");
            };
            let Some(expected) = s.bytes(contents)? else {
                return s.error(b"removeFile: expected String contents");
            };
            let Some(path) = path_from_bytes(&path_bytes) else {
                return s.error(b"could not remove file");
            };
            let file = match fs::File::open(path) {
                Ok(file) => file,
                Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(r),
                Err(_) => return s.error(b"could not inspect file for removal"),
            };
            match read_bounded(file, MAX_FILE_BYTES) {
                Ok(actual) if actual == expected => (),
                Ok(_) => return s.error(b"file changed since candidate admission"),
                Err(_) => return s.error(b"could not inspect file for removal"),
            }
            match fs::remove_file(path) {
                Ok(()) => Ok(r),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(r),
                Err(_) => s.error(b"could not remove file"),
            }
        })()
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn roots_do(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [visitor] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let values = s.scoped.borrow().clone();
        for value in values {
            s.send(*visitor, b"visit:", &[value])?;
        }
        Ok(ptr::null_mut())
    })()
    .unwrap_or_else(abi::fault_reference)
}
macro_rules! literals{($($n:literal),*$(,)?)=>{const NAMES:&[&[u8]]=&[$($n),*];static LITERALS:&[Literal]=&[$(Literal::new($n)),*];};}
literals![
    b"Bytes",
    b"Error",
    b"Roots",
    b"String",
    b"StringVt",
    b"System",
    b"Vector",
    b"add:",
    b"addMethod",
    b"at:",
    b"at:put:",
    b"atWord:",
    b"appendByteValues",
    b"behaviorOf:",
    b"bytes",
    b"delegated",
    b"embedFile:",
    b"fromBytes:",
    b"fromByteValues",
    b"ioError",
    b"kind:message:",
    b"methodAt:",
    b"new",
    b"nil",
    b"printString",
    b"readFile:",
    b"readFileSnapshot:",
    b"reassociateBehaviorOf:with:",
    b"register:",
    b"release:",
    b"removeFile:ifContents:",
    b"retain:",
    b"rootsDo:",
    b"sequence",
    b"sizeWord",
    b"visit:",
    b"withContext:",
    b"writeFile:contents:"
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
        name: index(b"readFile:"),
        entry: Some(read_file),
    },
    MethodSpec {
        name: index(b"readFileSnapshot:"),
        entry: Some(read_snapshot),
    },
    MethodSpec {
        name: index(b"embedFile:"),
        entry: Some(embed_file),
    },
    MethodSpec {
        name: index(b"writeFile:contents:"),
        entry: Some(write_file),
    },
    MethodSpec {
        name: index(b"removeFile:ifContents:"),
        entry: Some(remove_matching),
    },
    MethodSpec {
        name: index(b"rootsDo:"),
        entry: Some(roots_do),
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
        return if s.ready.get() && s.ns == *ns && s.id.root == id.root {
            ptr::null_mut()
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
        let nil = named(b"nil")?;
        let system = named(b"System")?;
        let vt = send(id.root, b"behaviorOf:", &[system])?;
        let string = named(b"String")?;
        let string_vt = named(b"StringVt")?;
        let owner = send(
            id.root,
            b"sequence",
            &[*ns, roots, nil, system, vt, string, string_vt],
        )?;
        let token = send(roots, b"retain:", &[owner])?;
        let s = Box::into_raw(Box::new(State {
            id,
            ns: *ns,
            nil,
            names: names.clone(),
            scoped: RefCell::new(Vec::new()),
            ready: Cell::new(false),
            string,
            string_vt,
        }));
        INSTANCE.store(s, Ordering::Release);
        let s = &*s;
        let owner_type = send(id.root, b"behaviorOf:", &[owner])?;
        let private = send(owner_type, b"delegated", &[])?;
        send(id.root, b"reassociateBehaviorOf:with:", &[owner, private])?;
        let method = send(image, b"methodAt:", &[names[b"rootsDo:".as_slice()]])?;
        let method = send(method, b"withContext:", &[*ns])?;
        send(
            private,
            b"addMethod",
            &[names[b"rootsDo:".as_slice()], method],
        )?;
        send(roots, b"register:", &[owner])?;
        for name in [
            b"readFile:".as_slice(),
            b"readFileSnapshot:".as_slice(),
            b"embedFile:".as_slice(),
            b"writeFile:contents:".as_slice(),
            b"removeFile:ifContents:".as_slice(),
        ] {
            let method = send(image, b"methodAt:", &[names[name]])?;
            let method = send(method, b"withContext:", &[*ns])?;
            send(vt, b"addMethod", &[names[name], method])?;
        }
        send(roots, b"release:", &[token])?;
        s.ready.set(true);
        Ok(ptr::null_mut())
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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_read_accepts_limit_and_rejects_one_more_byte() {
        assert_eq!(read_bounded(&b""[..], 3).unwrap(), b"");
        assert_eq!(read_bounded(&b"a\0\xff"[..], 3).unwrap(), b"a\0\xff");
        assert!(read_bounded(&b"four"[..], 3).is_err());
    }
    #[test]
    fn bounded_read_stops_after_limit_plus_one_and_preserves_read_errors() {
        struct Reader {
            remaining: usize,
        }
        impl Read for Reader {
            fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
                assert!(output.len() <= self.remaining);
                self.remaining -= output.len();
                output.fill(b'x');
                Ok(output.len())
            }
        }
        let mut reader = Reader { remaining: 4 };
        assert!(read_bounded(&mut reader, 3).is_err());
        assert_eq!(reader.remaining, 0);
        struct Failed;
        impl Read for Failed {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::from(io::ErrorKind::PermissionDenied))
            }
        }
        assert_eq!(
            read_bounded(Failed, 3).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
    }
}
