//! System getEnv: through Id messages; preserve exact names and lazy errors.
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
            // Preserve output's exact-String policy, including printString results.
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
unsafe extern "C" fn get_env(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let s = &*c.cast::<State>();
        if s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        if n != 1 {
            return Ok(Operand::object(s.nil));
        }
        let [name] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let name = object(*name)?;
        s.rooted(&[r, name], || {
            let Some(bytes) = s.bytes(name)? else {
                return Ok(Operand::object(s.nil));
            };
            let Ok(name) = std::str::from_utf8(&bytes) else {
                return s.error(b"could not read environment");
            };
            match std::env::var(name) {
                Ok(value) => s.make_string(value.as_bytes()),
                Err(std::env::VarError::NotPresent) => Ok(Operand::object(s.nil)),
                Err(std::env::VarError::NotUnicode(_)) => s.error(b"could not read environment"),
            }
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
        for (name, method) in [(b"getEnv:".as_slice(), get_env as native::Method)] {
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
