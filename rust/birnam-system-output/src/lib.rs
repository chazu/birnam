//! System stdout operations. All language dependencies are current Id sends.
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
};
use core::{ffi::c_void, ptr};
use std::{
    io::{self, Write},
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};
struct State {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    string_vt: Oop,
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
fn write_system_output_to<W: Write>(writer: &mut W, bytes: &[u8], newline: bool) -> io::Result<()> {
    writer.write_all(bytes)?;
    if newline {
        writer.write_all(b"\n")?;
    }
    writer.flush()
}
fn write_system_output(bytes: &[u8], newline: bool) {
    // No language send runs while the OS output lock or a borrowed String is held.
    let mut stdout = io::stdout().lock();
    let _ = write_system_output_to(&mut stdout, bytes, newline);
}
unsafe fn output(c: *mut c_void, r: Oop, a: *const Operand, n: usize, newline: bool) -> Outcome {
    (|| unsafe {
        let s = &*c.cast::<State>();
        if s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        if n != 1 {
            return Ok(Operand::object(s.nil));
        }
        let [value] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let value = object(*value)?;
        s.rooted(&[r, value], || {
            if let Some(bytes) = s.bytes(value)? {
                write_system_output(&bytes, newline);
                return Ok(Operand::object(r));
            }
            let rendered = s.id.send_named(value, b"printString", &[])?;
            let Some(rendered) = rendered.as_object().filter(|o| !o.is_null()) else {
                return Ok(Operand::object(s.nil));
            };
            s.rooted(&[rendered], || {
                let Some(bytes) = s.bytes(rendered)? else {
                    return Ok(Operand::object(s.nil));
                };
                write_system_output(&bytes, newline);
                Ok(Operand::object(r))
            })
        })
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
unsafe extern "C" fn print(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe { output(c, r, a, n, false) }
}
unsafe extern "C" fn println(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe { output(c, r, a, n, true) }
}
unsafe fn construct(id: Protocol, ns: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let named = |name: &[u8]| {
            object(id.send_named(ns, b"at:", &[Operand::object(id.intern_name(name)?)])?)
        };
        let roots = named(b"Roots")?;
        let nil = named(b"nil")?;
        let string_vt = named(b"StringVt")?;
        let system = named(b"System")?;
        for o in [ns, roots, nil, string_vt, system] {
            id.send_named(roots, b"retain:", &[Operand::object(o)])?;
        }
        let vt = object(id.send_named(id.receiver, b"behaviorOf:", &[Operand::object(system)])?)?;
        id.send_named(roots, b"retain:", &[Operand::object(vt)])?;
        let state = Box::into_raw(Box::new(State {
            id,
            ns,
            roots,
            string_vt,
            nil,
            thread: std::thread::current().id(),
        }));
        for (name, method) in [
            (b"print:".as_slice(), print as native::Method),
            (b"println:", println),
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
    use super::write_system_output_to;
    use std::io::{self, Write};

    struct FlushProbe {
        bytes: Vec<u8>,
        flushed: bool,
    }

    impl Write for FlushProbe {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            self.flushed = true;
            Ok(())
        }
    }

    #[test]
    fn system_output_flushes_a_newline_free_prompt() {
        let mut output = FlushProbe {
            bytes: Vec::new(),
            flushed: false,
        };

        write_system_output_to(&mut output, b"birnam> ", false).expect("prompt write");

        assert_eq!(output.bytes, b"birnam> ");
        assert!(
            output.flushed,
            "a prompt must be visible before stdin blocks"
        );
    }
}
