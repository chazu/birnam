//! System source loading through current Codegen and value messages.
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
    integer: Oop,
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
        f: impl FnOnce(&mut Vec<usize>) -> Result<Operand, Outcome>,
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
                f(&mut tokens)
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
            // Module paths retain their existing exact-String recognition policy.
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
    unsafe fn keep(&self, tokens: &mut Vec<usize>, value: Oop) -> Result<Oop, Outcome> {
        tokens.push(
            unsafe {
                self.id
                    .send_named(self.roots, b"retain:", &[Operand::object(value)])
            }?
            .as_word()
            .ok_or_else(invalid)?,
        );
        Ok(value)
    }
    unsafe fn error(&self, kind: &[u8], message: &[u8]) -> Result<Operand, Outcome> {
        unsafe {
            let kind = self.id.intern_name(kind)?;
            self.rooted(&[kind], |tokens| {
                let message = object(self.id.send_named(
                    self.string,
                    b"fromBytes:",
                    &[Operand::bytes(message)],
                )?)?;
                self.keep(tokens, message)?;
                // Error is selected after message construction, as in the other System services.
                let factory = self.id.send_named(
                    self.ns,
                    b"at:",
                    &[Operand::object(self.id.intern_name(b"Error")?)],
                )?;
                let Some(factory) = factory.as_object().filter(|o| !o.is_null()) else {
                    return Ok(Operand::object(self.nil));
                };
                self.keep(tokens, factory)?;
                self.id.send_named(
                    factory,
                    b"kind:message:",
                    &[Operand::object(kind), Operand::object(message)],
                )
            })
        }
    }
}
impl State {
    unsafe fn named(&self, name: &[u8]) -> Result<Option<Oop>, Outcome> {
        unsafe {
            Ok(self
                .id
                .send_named(
                    self.ns,
                    b"at:",
                    &[Operand::object(self.id.intern_name(name)?)],
                )?
                .as_object()
                .filter(|o| !o.is_null()))
        }
    }
    unsafe fn is_error(&self, tokens: &mut Vec<usize>, value: Oop) -> Result<bool, Outcome> {
        unsafe {
            let Some(vt) = self.named(b"ErrorVt")? else {
                return Ok(false);
            };
            self.keep(tokens, vt)?;
            Ok(self
                .id
                .send_named(self.id.receiver, b"behaviorOf:", &[Operand::object(value)])?
                .as_object()
                == Some(vt))
        }
    }
}
unsafe extern "C" fn load(
    c: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    (|| unsafe {
        let s = &*c.cast::<State>();
        if s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        if count != 1 {
            return Ok(Operand::object(s.nil));
        }
        let [path_object] = borrowed_slice(args, count).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let path_object = object(*path_object)?;
        s.rooted(&[receiver, path_object], |tokens| {
            let Some(path_bytes) = s.bytes(path_object)? else {
                return Ok(Operand::object(s.nil));
            };
            let Ok(path_text) = std::str::from_utf8(&path_bytes) else {
                return s.error(b"ioError", b"could not read source file");
            };
            let path = std::path::Path::new(path_text);
            use std::io::Read;
            let Ok(file) = std::fs::File::open(path) else {
                return s.error(b"ioError", b"could not read source file");
            };
            let mut source = Vec::new();
            const LIMIT: u64 = 1 << 30;
            if file.take(LIMIT + 1).read_to_end(&mut source).is_err()
                || source.len() > LIMIT as usize
            {
                return s.error(b"ioError", b"could not read source file");
            }
            let Some(codegen) = s.named(b"Codegen")?.filter(|o| *o != s.nil) else {
                return s.error(b"noCodegen", b"Codegen module is not loaded");
            };
            s.keep(tokens, codegen)?;
            let source = s.keep(
                tokens,
                object(s.id.send_named(s.string, b"fromBytes:", &[Operand::bytes(&source)])?)?,
            )?;
            let revision = s.keep(
                tokens,
                object(s.id.send_named(
                    codegen,
                    b"sourceRevisionOf:",
                    &[Operand::object(source)],
                )?)?,
            )?;
            let mut declared_name = s.nil;
            if !s.is_error(tokens, revision)? && revision != s.nil {
                let index = s.keep(
                    tokens,
                    object(s.id.send_named(s.integer, b"fromWord:", &[Operand::word(1)])?)?,
                )?;
                declared_name = s.keep(
                    tokens,
                    object(s.id.send_named(revision, b"at:", &[Operand::object(index)])?)?,
                )?;
                let recorded = s.keep(
                    tokens,
                    object(s.id.send_named(
                        codegen,
                        b"recordModuleSource:named:",
                        &[Operand::object(source), Operand::object(declared_name)],
                    )?)?,
                )?;
                if s.is_error(tokens, recorded)? {
                    return Ok(Operand::object(recorded));
                }
            }
            let result = s.keep(
                tokens,
                object(s.id.send_named(
                    codegen,
                    b"evalPersisting:from:",
                    &[Operand::object(source), Operand::object(path_object)],
                )?)?,
            )?;
            if result == s.nil {
                let error = s.keep(
                    tokens,
                    object(s.id.send_named(codegen, b"lastError", &[])?)?,
                )?;
                if error != s.nil {
                    if let Some(bytes) = s.bytes(error)? {
                        eprintln!(
                            "birnam: load {} failed:\n{}",
                            path.display(),
                            String::from_utf8_lossy(&bytes)
                        );
                    }
                    return s.error(b"syntaxError", b"source file failed to compile");
                }
                return Ok(Operand::object(result));
            }
            if s.is_error(tokens, result)? {
                let rendered = s.keep(
                    tokens,
                    object(s.id.send_named(result, b"printString", &[])?)?,
                )?;
                if let Some(bytes) = s.bytes(rendered)? {
                    eprintln!(
                        "birnam: load {}: {}",
                        path.display(),
                        String::from_utf8_lossy(&bytes)
                    );
                }
            } else if declared_name != s.nil {
                if let Some(module) = s.named(b"Module")?.filter(|o| *o != s.nil) {
                    s.keep(tokens, module)?;
                    // Ignore a language result, but preserve native send failure.
                    s.id.send_named(module, b"markLoaded:", &[Operand::object(declared_name)])?;
                }
            }
            Ok(Operand::object(result))
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
        let integer = named(b"Integer")?;
        for o in [ns, roots, nil, string_vt, string, system, integer] {
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
            integer,
            thread: std::thread::current().id(),
        }));
        for (name, method) in [(b"load:".as_slice(), load as native::Method)] {
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
