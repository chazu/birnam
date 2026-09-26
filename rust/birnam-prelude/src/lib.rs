//! Construct System through Id, ObjectModel, Roots and Namespace messages.
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{self as native, Operand, Outcome, Protocol},
};
use core::ptr;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
struct State {
    id: Oop,
    namespace: Oop,
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
unsafe fn construct(id: Protocol, ns: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let named =
            |name: &[u8]| id.send_named(ns, b"at:", &[Operand::object(id.intern_name(name)?)]);
        let existing = named(b"System")?;
        if existing.kind() == native::OBJECT {
            return Ok(existing);
        }
        let roots = object(named(b"Roots")?)?;
        let mut tokens = Vec::new();
        let mut keep = |o| -> Result<Oop, Outcome> {
            tokens.push(
                id.send_named(roots, b"retain:", &[Operand::object(o)])?
                    .as_word()
                    .ok_or_else(invalid)?,
            );
            Ok(o)
        };
        let result = (|| {
            let vt = keep(object(id.send_named(
                object(named(b"ObjectVt")?)?,
                b"delegated",
                &[],
            )?)?)?;
            let system = keep(object(id.send_named(
                object(named(b"ObjectModel")?)?,
                b"allocate:bytes:",
                &[Operand::object(vt), Operand::word(0)],
            )?)?)?;
            id.send_named(
                id.receiver,
                b"ownerOf:put:",
                &[Operand::object(vt), Operand::object(system)],
            )?;
            for (name, value) in [(b"SystemVt".as_slice(), vt), (b"System", system)] {
                id.send_named(
                    ns,
                    b"at:put:",
                    &[
                        Operand::object(id.intern_name(name)?),
                        Operand::object(value),
                    ],
                )?;
            }
            Ok(Operand::object(system))
        })();
        for t in tokens {
            id.send_named(roots, b"release:", &[Operand::word(t)])?;
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
            return if s.id == (*id).receiver
                && s.namespace == ns
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
        let result = (|| {
            let result = construct(*id, ns)?;
            let system = object(result)?;
            let roots = object((*id).send_named(
                ns,
                b"at:",
                &[Operand::object((*id).intern_name(b"Roots")?)],
            )?)?;
            for o in [ns, system] {
                (*id).send_named(roots, b"retain:", &[Operand::object(o)])?;
            }
            INSTANCE.store(
                Box::into_raw(Box::new(State {
                    id: (*id).receiver,
                    namespace: ns,
                    system,
                    thread: std::thread::current().id(),
                })),
                Ordering::Release,
            );
            Ok(result)
        })();
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
/// Retained module identity, independent of its construction protocol.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_prelude_rust_identity() -> *const core::ffi::c_char {
    c"rust-prelude-n7.1-core".as_ptr()
}

#[cfg(test)]
mod tests {
    use super::birnam_prelude_rust_identity;
    use std::ffi::CStr;

    #[test]
    fn identity_is_a_terminated_static_string() {
        // SAFETY: the export points to immutable process-lifetime storage.
        let identity = unsafe { CStr::from_ptr(birnam_prelude_rust_identity()) };
        assert_eq!(identity.to_bytes_with_nul(), b"rust-prelude-n7.1-core\0");
    }
}
