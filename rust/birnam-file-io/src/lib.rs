//! System file I/O through Id sends, with owned argument snapshots and lazy errors.
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
};
use core::{ffi::c_void, ptr};
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
struct State {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    string_vt: Oop,
    string: Oop,
    nil: Oop,
    thread: std::thread::ThreadId,
}
static INSTANCE: AtomicPtr<State> = AtomicPtr::new(ptr::null_mut());
static BUSY: AtomicBool = AtomicBool::new(false);
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
fn object(v: Operand) -> Result<Oop, Outcome> {
    v.as_object().filter(|o| !o.is_null()).ok_or_else(invalid)
}
impl State {
    unsafe fn rooted(
        &self,
        values: &[Oop],
        f: impl FnOnce() -> Result<Operand, Outcome>,
    ) -> Result<Operand, Outcome> {
        unsafe {
            let mut tokens = Vec::new();
            let result = (|| {
                for &v in values {
                    tokens.push(
                        self.id
                            .send_named(self.roots, b"retain:", &[Operand::object(v)])?
                            .as_word()
                            .ok_or_else(invalid)?,
                    );
                }
                f()
            })();
            for t in tokens {
                self.id
                    .send_named(self.roots, b"release:", &[Operand::word(t)])?;
            }
            result
        }
    }
    unsafe fn bytes(&self, value: Oop) -> Result<Option<Vec<u8>>, Outcome> {
        unsafe {
            // File arguments retain their existing exact-String recognition policy.
            if self
                .id
                .send_named(self.id.receiver, b"behaviorOf:", &[Operand::object(value)])?
                .as_object()
                != Some(self.string_vt)
            {
                return Ok(None);
            }
            let raw = self.id.send_named(value, b"bytes", &[])?;
            Ok(raw
                .as_bytes()
                .and_then(|b| borrowed_bytes(b.pointer, b.length))
                .map(<[u8]>::to_vec))
        }
    }
}

impl State {
    unsafe fn make_string(&self, bytes: &[u8]) -> Result<Operand, Outcome> {
        unsafe {
            self.id
                .send_named(self.string, b"fromBytes:", &[Operand::bytes(bytes)])
        }
    }
    unsafe fn error(&self, message: &[u8]) -> Result<Operand, Outcome> {
        unsafe {
            let kind = self.id.intern_name(b"ioError")?;
            self.rooted(&[kind], || {
                let message = object(self.make_string(message)?)?;
                self.rooted(&[message], || {
                    // Resolve Error after constructing the message, preserving lazy publication.
                    let factory = self.id.send_named(
                        self.ns,
                        b"at:",
                        &[Operand::object(self.id.intern_name(b"Error")?)],
                    )?;
                    let Some(factory) = factory.as_object().filter(|o| !o.is_null()) else {
                        return Ok(Operand::object(self.nil));
                    };
                    self.rooted(&[factory], || {
                        self.id.send_named(
                            factory,
                            b"kind:message:",
                            &[Operand::object(kind), Operand::object(message)],
                        )
                    })
                })
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
unsafe extern "C" fn read_file(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let s = &*c.cast::<State>();
        if s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        if n != 1 {
            return Ok(Operand::object(s.nil));
        }
        let [path] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let path = object(*path)?;
        s.rooted(&[r, path], || {
            let Some(path_bytes) = s.bytes(path)? else {
                return Ok(Operand::object(s.nil));
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
        })
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
// The compiler sends this operation with a resolved filename. Source-relative
// validation belongs to the compilation form; this owner only performs bounded I/O.
unsafe extern "C" fn embed_file(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let s = &*c.cast::<State>();
        if s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        let Some([path]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let path = object(*path)?;
        s.rooted(&[r, path], || {
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
            let factory = object(s.id.send_named(
                s.ns,
                b"at:",
                &[Operand::object(s.id.intern_name(b"Bytes")?)],
            )?)?;
            s.rooted(&[factory], || {
                s.id.send_named(factory, b"fromBytes:", &[Operand::bytes(&bytes)])
            })
        })
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
unsafe extern "C" fn write_file(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let s = &*c.cast::<State>();
        if s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        if n != 2 {
            return Ok(Operand::object(s.nil));
        }
        let [path, contents] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let path = object(*path)?;
        let contents = object(*contents)?;
        s.rooted(&[r, path, contents], || {
            // Copy the filename before the next send. Contents callbacks cannot
            // redirect this write by mutating the earlier borrowed byte view.
            let Some(path_bytes) = s.bytes(path)? else {
                return Ok(Operand::object(s.nil));
            };
            let Some(contents) = s.bytes(contents)? else {
                return Ok(Operand::object(s.nil));
            };
            // Preserve argument-read/error precedence: validate after both reads.
            let Some(path) = path_from_bytes(&path_bytes) else {
                return s.error(b"could not write file");
            };
            if fs::write(path, contents).is_err() {
                return s.error(b"could not write file");
            }
            Ok(Operand::object(r))
        })
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
unsafe fn construct(id: Protocol, ns: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let named = |name: &[u8]| {
            object(id.send_named(ns, b"at:", &[Operand::object(id.intern_name(name)?)])?)
        };
        let roots = named(b"Roots")?;
        let nil = named(b"nil")?;
        let string_vt = named(b"StringVt")?;
        let string = named(b"String")?;
        let system = named(b"System")?;
        for o in [ns, roots, nil, string_vt, string, system] {
            id.send_named(roots, b"retain:", &[Operand::object(o)])?;
        }
        let vt = object(id.send_named(id.receiver, b"behaviorOf:", &[Operand::object(system)])?)?;
        id.send_named(roots, b"retain:", &[Operand::object(vt)])?;
        let state = Box::into_raw(Box::new(State {
            id,
            ns,
            roots,
            string_vt,
            string,
            nil,
            thread: std::thread::current().id(),
        }));
        for (name, method) in [
            (b"readFile:".as_slice(), read_file as native::Method),
            (b"embedFile:", embed_file),
            (b"writeFile:contents:", write_file),
        ] {
            id.send_named(
                vt,
                b"addMethod",
                &[
                    Operand::object(id.intern_name(name)?),
                    Operand::entry(NativeEntry::new(method, state.cast())),
                ],
            )?;
        }
        INSTANCE.store(state, Ordering::Release);
        Ok(Operand::unit())
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
            return if s.id.receiver == (*id).receiver
                && s.ns == ns
                && s.thread == std::thread::current().id()
            {
                Ok(Operand::unit())
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
    version: 1,
    byte_size: size_of::<native::Module>() as u32,
    initialize: Some(initialize),
};
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}

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
