//! File I/O uses Oop messages and completed owner-local dependencies.
use birnam_abi::{
    self as abi, Oop, borrowed_slice,
    id_protocol::{Literal, MethodSpec, Module, Protocol},
};
use core::{cell::Cell, ptr};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};
struct State {
    id: Protocol,
    ns: Oop,
    string: Oop,
    string_vt: Oop,
    path_bytes: Oop,
    content_bytes: Oop,
    construct_bytes: Oop,
    snapshot_start: Oop,
    snapshot_contents: Oop,
    finish_snapshot: Oop,
    nil: Oop,
    names: HashMap<&'static [u8], Oop>,
    finish_error: Oop,
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
unsafe fn state<'a>(c: Oop) -> Result<&'a State, Oop> {
    let p = INSTANCE.load(Ordering::Acquire);
    if p.is_null() {
        return Err(invalid());
    }
    let s = unsafe { &*p };
    if c == s.ns { Ok(s) } else { Err(invalid()) }
}
impl State {
    unsafe fn send(&self, r: Oop, name: &[u8], a: &[Oop]) -> Result<Oop, Oop> {
        unsafe { self.id.send(r, self.names[name], a) }
    }
    unsafe fn container(&self, value: Oop) -> Result<Option<Oop>, Oop> {
        unsafe {
            if value.is_null() {
                return Err(invalid());
            }
            if self.send(self.id.root, b"behaviorOf:", &[value])? != self.string_vt {
                return Ok(None);
            }
            let v = self.send(value, b"bytes", &[])?;
            Ok((!v.is_null()).then_some(v))
        }
    }
    unsafe fn binding(&self, name: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            let value = self.send(self.ns, b"at:", &[self.names[name]])?;
            if value.is_null() {
                Err(invalid())
            } else {
                Ok(value)
            }
        }
    }
    unsafe fn make_string(&self, bytes: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            let values: Vec<_> = bytes
                .iter()
                .map(|&b| abi::size(b as usize).unwrap())
                .collect();
            let source = self.send(self.id.root, b"sequence", &values)?;
            self.send(self.string, b"fromBytes:", &[source])
        }
    }
    unsafe fn error(&self, text: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            let message = self.make_string(text)?;
            if message.is_null() {
                return Err(invalid());
            }
            // The diagnostic is the helper's receiver, so Id protects it through
            // lazy Error lookup and construction, with no root-token cleanup send.
            self.send(self.finish_error, b"apply", &[message])
        }
    }
}
unsafe extern "C" fn finish_error(_: Oop, c: Oop, message: Oop, _: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        if n != 0 || message.is_null() {
            return Err(invalid());
        }
        let factory = s.send(s.ns, b"at:", &[s.names[b"Error".as_slice()]])?;
        if factory.is_null() {
            return Ok(s.nil);
        }
        s.send(
            factory,
            b"kind:message:",
            &[s.names[b"ioError".as_slice()], message],
        )
    })()
    .unwrap_or_else(abi::fault_reference)
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

fn decode(a: &[Oop]) -> Option<Vec<u8>> {
    a.iter()
        .map(|&v| abi::as_size(v).and_then(|v| u8::try_from(v).ok()))
        .collect()
}
unsafe fn operation(c: Oop, r: Oop, a: *const Oop, count: usize, mode: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let args = borrowed_slice(a, count).ok_or_else(invalid)?;
        if args.len() != if mode < 3 { 1 } else { 2 } {
            return if matches!(mode, 2 | 4) {
                Err(invalid())
            } else {
                Ok(s.nil)
            };
        }
        if args.iter().any(|v| v.is_null()) {
            return Err(invalid());
        }
        let Some(path) = s.container(args[0])? else {
            return match mode {
                2 => s.error(b"embedFile: expected a String path"),
                4 => s.error(b"removeFile: expected a String path"),
                _ => Ok(s.nil),
            };
        };
        s.send(
            path,
            b"apply:to:with:",
            &[
                s.path_bytes,
                r,
                abi::size(mode).unwrap(),
                args.get(1).copied().unwrap_or(ptr::null_mut()),
            ],
        )
    })()
    .unwrap_or_else(abi::fault_reference)
}
macro_rules! entry {
    ($name:ident,$mode:expr) => {
        unsafe extern "C" fn $name(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
            unsafe { operation(c, r, a, n, $mode) }
        }
    };
}
entry!(read_file, 0);
entry!(read_snapshot, 1);
entry!(embed_file, 2);
entry!(write_file, 3);
entry!(remove_matching, 4);
unsafe extern "C" fn path_bytes(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let a = borrowed_slice(a, n).ok_or_else(invalid)?;
        let split = a.len().checked_sub(2).ok_or_else(invalid)?;
        let mode = abi::as_size(a[split])
            .filter(|v| *v < 5)
            .ok_or_else(invalid)?;
        let path_values = &a[..split];
        if mode >= 3 {
            if !path_values
                .iter()
                .all(|&v| abi::as_size(v).is_some_and(|v| v <= u8::MAX as usize))
            {
                return if mode == 4 {
                    s.error(b"removeFile: expected a String path")
                } else {
                    Ok(s.nil)
                };
            }
            // Snapshot the filename as byte Oops before reading contents. The
            // contents provider receives those immutable values as a suffix.
            let Some(contents) = s.container(a[split + 1])? else {
                return if mode == 4 {
                    s.error(b"removeFile: expected String contents")
                } else {
                    Ok(s.nil)
                };
            };
            let mut arguments = Vec::with_capacity(path_values.len() + 4);
            arguments.extend([s.content_bytes, r]);
            arguments.extend_from_slice(path_values);
            arguments.extend([
                abi::size(path_values.len()).ok_or_else(invalid)?,
                abi::size(mode).unwrap(),
            ]);
            return s.send(contents, b"apply:to:with:", &arguments);
        }
        let Some(path_bytes) = decode(path_values) else {
            return match mode {
                2 => s.error(b"embedFile: expected a String path"),
                4 => s.error(b"removeFile: expected a String path"),
                _ => Ok(s.nil),
            };
        };
        let Some(path) = path_from_bytes(&path_bytes) else {
            return match mode {
                1 => Ok(s.nil),
                2 => s.error(b"embedFile: invalid path"),
                _ => s.error(b"could not read file"),
            };
        };
        let file = match fs::File::open(path) {
            Ok(file) => file,
            Err(_) => {
                return match mode {
                    1 => Ok(s.nil),
                    2 => s.error(b"embedFile: could not read the file"),
                    _ => s.error(b"could not read file"),
                };
            }
        };
        let bytes = match read_bounded(
            file,
            if mode == 2 {
                16 * 1024 * 1024
            } else {
                MAX_FILE_BYTES
            },
        ) {
            Ok(bytes) => bytes,
            Err(_) => {
                return match mode {
                    1 => Ok(s.nil),
                    2 => s.error(b"embedFile: could not read the file within the 16 MiB limit"),
                    _ => s.error(b"could not read file"),
                };
            }
        };
        if mode == 0 {
            return s.make_string(&bytes);
        }
        let factory = s.binding(b"Bytes")?;
        let mut args =
            Vec::with_capacity(1 + bytes.len() + if mode == 1 { path_values.len() + 1 } else { 0 });
        args.push(factory);
        args.extend(bytes.iter().map(|&b| abi::size(b as usize).unwrap()));
        if mode == 1 {
            args.extend_from_slice(path_values);
            args.push(abi::size(path_values.len()).unwrap());
        }
        s.send(
            if mode == 1 {
                s.snapshot_start
            } else {
                s.construct_bytes
            },
            b"apply",
            &args,
        )
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn content_bytes(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let a = borrowed_slice(a, n).ok_or_else(invalid)?;
        let end = a.len().checked_sub(2).ok_or_else(invalid)?;
        let path_len = abi::as_size(a[end]).ok_or_else(invalid)?;
        let mode = abi::as_size(a[end + 1])
            .filter(|v| matches!(v, 3 | 4))
            .ok_or_else(invalid)?;
        let split = end.checked_sub(path_len).ok_or_else(invalid)?;
        let Some(contents) = decode(&a[..split]) else {
            return if mode == 4 {
                s.error(b"removeFile: expected String contents")
            } else {
                Ok(s.nil)
            };
        };
        let path_bytes = decode(&a[split..end]).ok_or_else(invalid)?;
        // Preserve error precedence: validate the filename after both reads.
        let Some(path) = path_from_bytes(&path_bytes) else {
            return s.error(if mode == 3 {
                b"could not write file"
            } else {
                b"could not remove file"
            });
        };
        if mode == 3 {
            return if fs::write(path, contents).is_err() {
                s.error(b"could not write file")
            } else {
                Ok(r)
            };
        }
        let file = match fs::File::open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(r),
            Err(_) => return s.error(b"could not inspect file for removal"),
        };
        match read_bounded(file, MAX_FILE_BYTES) {
            Ok(actual) if actual == contents => (),
            Ok(_) => return s.error(b"file changed since candidate admission"),
            Err(_) => return s.error(b"could not inspect file for removal"),
        };
        match fs::remove_file(path) {
            Ok(()) => Ok(r),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(r),
            Err(_) => s.error(b"could not remove file"),
        }
    })()
    .unwrap_or_else(abi::fault_reference)
}
// These receivers are the newly constructed objects. Ordinary Id call roots
// keep them alive through later constructors, avoiding a separate temporary table.
unsafe extern "C" fn construct_bytes(_: Oop, c: Oop, factory: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let values = borrowed_slice(a, n).ok_or_else(invalid)?;
        let source = s.send(s.id.root, b"sequence", values)?;
        s.send(factory, b"fromBytes:", &[source])
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn snapshot_start(_: Oop, c: Oop, factory: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let a = borrowed_slice(a, n).ok_or_else(invalid)?;
        let (last, values) = a.split_last().ok_or_else(invalid)?;
        let path_len = abi::as_size(*last).ok_or_else(invalid)?;
        let split = values.len().checked_sub(path_len).ok_or_else(invalid)?;
        let source = s.send(s.id.root, b"sequence", &values[split..])?;
        let name = s.send(factory, b"fromBytes:", &[source])?;
        if name.is_null() {
            return Err(invalid());
        }
        let mut args = Vec::with_capacity(split + 2);
        args.extend([name, factory]);
        args.extend_from_slice(&values[..split]);
        s.send(s.snapshot_contents, b"apply", &args)
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn snapshot_contents(_: Oop, c: Oop, name: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [factory, values @ ..] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let source = s.send(s.id.root, b"sequence", values)?;
        let contents = s.send(*factory, b"fromBytes:", &[source])?;
        if contents.is_null() {
            return Err(invalid());
        }
        s.send(s.finish_snapshot, b"apply", &[contents, name])
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn finish_snapshot(
    _: Oop,
    c: Oop,
    contents: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [name] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let vector = s.binding(b"Vector")?;
        let out = s.send(vector, b"new", &[])?;
        if out.is_null() {
            return Err(invalid());
        }
        let out = s.send(out, b"add:", &[*name])?;
        if out.is_null() {
            return Err(invalid());
        }
        s.send(out, b"add:", &[contents])
    })()
    .unwrap_or_else(abi::fault_reference)
}
macro_rules! literals{($($n:literal),*$(,)?)=>{const NAMES:&[&[u8]]=&[$($n),*];static LITERALS:&[Literal]=&[$(Literal::new($n)),*];};}
literals![
    b"at:",
    b"at:put:",
    b"Roots",
    b"String",
    b"StringVt",
    b"System",
    b"nil",
    b"Error",
    b"ioError",
    b"Bytes",
    b"Vector",
    b"new",
    b"add:",
    b"behaviorOf:",
    b"fromBytes:",
    b"sequence",
    b"kind:message:",
    b"retain:",
    b"release:",
    b"methodAt:",
    b"withContext:",
    b"addMethod",
    b"apply",
    b"apply:to:with:",
    b"bytes",
    b"readFile:",
    b"readFileSnapshot:",
    b"embedFile:",
    b"writeFile:contents:",
    b"removeFile:ifContents:",
    b"pathBytes",
    b"contentBytes",
    b"constructBytes",
    b"snapshotStart",
    b"snapshotContents",
    b"finishSnapshot",
    b"finishError"
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
        name: index(b"pathBytes"),
        entry: Some(path_bytes),
    },
    MethodSpec {
        name: index(b"contentBytes"),
        entry: Some(content_bytes),
    },
    MethodSpec {
        name: index(b"constructBytes"),
        entry: Some(construct_bytes),
    },
    MethodSpec {
        name: index(b"snapshotStart"),
        entry: Some(snapshot_start),
    },
    MethodSpec {
        name: index(b"snapshotContents"),
        entry: Some(snapshot_contents),
    },
    MethodSpec {
        name: index(b"finishSnapshot"),
        entry: Some(finish_snapshot),
    },
    MethodSpec {
        name: index(b"finishError"),
        entry: Some(finish_error),
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
        return if s.ready.get() && s.id.root == id.root && s.ns == *ns {
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
        let named = |n: &[u8]| -> Result<Oop, Oop> {
            let v = send(*ns, b"at:", &[names[n]])?;
            if v.is_null() { Err(invalid()) } else { Ok(v) }
        };
        let roots = named(b"Roots")?;
        let string = named(b"String")?;
        let string_vt = named(b"StringVt")?;
        let nil = named(b"nil")?;
        let system = named(b"System")?;
        let vt = send(id.root, b"behaviorOf:", &[system])?;
        let mut captures = vec![*ns, roots, string, nil, system, vt, string_vt];
        captures.extend([ptr::null_mut(); 7]);
        let owner = send(id.root, b"sequence", &captures)?;
        let token = send(roots, b"retain:", &[owner])?;
        let prepared = (|| {
            let mut helpers = [ptr::null_mut(); 7];
            for (i, name) in [
                b"finishError".as_slice(),
                b"pathBytes",
                b"contentBytes",
                b"constructBytes",
                b"snapshotStart",
                b"snapshotContents",
                b"finishSnapshot",
            ]
            .into_iter()
            .enumerate()
            {
                let method = send(image, b"methodAt:", &[names[name]])?;
                helpers[i] = send(method, b"withContext:", &[*ns])?;
                send(owner, b"at:put:", &[abi::size(i + 7).unwrap(), helpers[i]])?;
            }
            Ok(helpers)
        })();
        let [
            finish_error,
            path_bytes,
            content_bytes,
            construct_bytes,
            snapshot_start,
            snapshot_contents,
            finish_snapshot,
        ] = match prepared {
            Ok(h) => h,
            Err(e) => {
                send(roots, b"release:", &[token])?;
                return Err(e);
            }
        };
        let s = Box::into_raw(Box::new(State {
            id,
            ns: *ns,
            string,
            string_vt,
            path_bytes,
            content_bytes,
            construct_bytes,
            snapshot_start,
            snapshot_contents,
            finish_snapshot,
            nil,
            names: names.clone(),
            finish_error,
            ready: Cell::new(false),
        }));
        INSTANCE.store(s, Ordering::Release);
        let s = &*s;
        let result = (|| {
            for name in [
                b"readFile:".as_slice(),
                b"readFileSnapshot:",
                b"embedFile:",
                b"writeFile:contents:",
                b"removeFile:ifContents:",
            ] {
                let method = s.send(image, b"methodAt:", &[s.names[name]])?;
                let method = s.send(method, b"withContext:", &[*ns])?;
                s.send(vt, b"addMethod", &[s.names[name], method])?;
            }
            s.ready.set(true);
            Ok(ptr::null_mut())
        })();
        // Publication can fail after a replacement addMethod has exposed the
        // Method. Its dependencies must remain live with the resident state.
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
