//! System language wrappers around Id loading, using messages for every dependency.
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
    system: Oop,
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
unsafe fn load_impl(
    c: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
    explicit: bool,
) -> Result<Operand, Outcome> {
    unsafe {
        let s = &*c.cast::<State>();
        if s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        if count != if explicit { 2 } else { 1 } {
            return Ok(Operand::object(s.nil));
        }
        let arguments = borrowed_slice(args, count).ok_or_else(invalid)?;
        let mut values = vec![receiver];
        for &argument in arguments {
            values.push(object(argument)?);
        }
        s.rooted(&values, |tokens| {
            let Some(path) = s.bytes(values[1])? else {
                return Ok(Operand::object(s.nil));
            };
            // Sequence sends may collect or mutate the String: the path is already owned.
            let mut operands = vec![Operand::bytes(&path)];
            if explicit {
                let sequence = values[2];
                let length = object(s.id.send_named(sequence, b"size", &[])?)?;
                s.keep(tokens, length)?;
                let length =
                    s.id.send_named(s.integer, b"wordOf:", &[Operand::object(length)])?;
                let Some(length) = length.as_word() else {
                    return s.error(b"typeError", b"module arguments need an Integer size");
                };
                let length = length as i64;
                if length < 0 {
                    return s.error(b"typeError", b"module argument count must be nonnegative");
                }
                for index in 0..length {
                    let index = object(s.id.send_named(
                        s.integer,
                        b"fromWord:",
                        &[Operand::word(index as usize)],
                    )?)?;
                    s.keep(tokens, index)?;
                    let value =
                        object(s.id.send_named(sequence, b"at:", &[Operand::object(index)])?)?;
                    s.keep(tokens, value)?;
                    operands.push(Operand::object(value));
                }
            }
            // Id alone owns descriptor admission and image lifetime. This is a fresh send.
            match s.id.send(s.id.receiver, s.id.load, &operands) {
                Ok(value) => match value.kind() {
                    native::UNIT => Ok(Operand::object(receiver)),
                    native::OBJECT => Ok(value),
                    _ => s.error(b"typeError", b"native module returned a non-language value"),
                },
                Err(e) => match e.status {
                    native::MAPPING_FAILED => s.error(b"ioError", b"could not load module dylib"),
                    native::MISSING_ENTRY | native::INCOMPATIBLE_ABI => s.error(
                        b"notAModule",
                        b"missing or incompatible birnam_native_module_v1",
                    ),
                    _ => s.error(b"moduleError", b"native module initialization failed"),
                },
            }
        })
    }
}
unsafe extern "C" fn load(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe { load_impl(c, r, a, n, false) }.map_or_else(|e| e, Outcome::loaded)
}
unsafe extern "C" fn load_arguments(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    unsafe { load_impl(c, r, a, n, true) }.map_or_else(|e| e, Outcome::loaded)
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
            system,
            thread: std::thread::current().id(),
        }));
        for (name, method) in [
            (b"loadModule:".as_slice(), load as native::Method),
            (b"loadModule:arguments:", load_arguments),
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
        Ok(Operand::object(system))
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
                Ok(Operand::object(s.system))
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
