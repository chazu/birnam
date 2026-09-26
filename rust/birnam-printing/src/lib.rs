//! Object and Symbol printing through current Id and String messages.
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as native, Method, NativeEntry, Operand, Outcome, Protocol},
};
use core::{ffi::c_void, ptr};
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
struct State {
    id: Protocol,
    ns: Oop,
    roots: Oop,
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
unsafe fn print(c: *mut c_void, r: Oop, n: usize, op: u8) -> Outcome {
    (|| unsafe {
        let s = &*c.cast::<State>();
        if s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        if n != 0 {
            return Ok(Operand::object(s.nil));
        }
        let token =
            s.id.send_named(s.roots, b"retain:", &[Operand::object(r)])?
                .as_word()
                .ok_or_else(invalid)?;
        let result = (|| {
            if op == 1 {
                return s.id.send_named(r, b"printString", &[]);
            }
            let bytes = if op == 2 {
                let raw =
                    s.id.send_named(s.id.receiver, b"symbolBytesOf:", &[Operand::object(r)])?;
                let Some(raw) = raw.as_bytes() else {
                    return Ok(Operand::object(s.nil));
                };
                // Copy the borrowed name before String construction can collect.
                let name = borrowed_bytes(raw.pointer, raw.length).ok_or_else(invalid)?;
                let mut bytes = Vec::with_capacity(name.len().saturating_add(1));
                bytes.push(b'#');
                bytes.extend_from_slice(name);
                bytes
            } else {
                b"anObject".to_vec()
            };
            s.id.send_named(s.string, b"fromBytes:", &[Operand::bytes(&bytes)])
        })();
        s.id.send_named(s.roots, b"release:", &[Operand::word(token)])?;
        result
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
unsafe extern "C" fn object_print(c: *mut c_void, r: Oop, _: *const Operand, n: usize) -> Outcome {
    unsafe { print(c, r, n, 0) }
}
unsafe extern "C" fn as_string(c: *mut c_void, r: Oop, _: *const Operand, n: usize) -> Outcome {
    unsafe { print(c, r, n, 1) }
}
unsafe extern "C" fn symbol_print(c: *mut c_void, r: Oop, _: *const Operand, n: usize) -> Outcome {
    unsafe { print(c, r, n, 2) }
}
unsafe fn construct(id: Protocol, ns: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let named = |name: &[u8]| {
            object(id.send_named(ns, b"at:", &[Operand::object(id.intern_name(name)?)])?)
        };
        let roots = named(b"Roots")?;
        let string = named(b"String")?;
        let nil = named(b"nil")?;
        let base = named(b"ObjectVt")?;
        // Id owns Symbols; ask it for their behavior instead of importing a layout.
        let symbol = id.intern_name(b"Symbol")?;
        let symbol_root = id
            .send_named(roots, b"retain:", &[Operand::object(symbol)])?
            .as_word()
            .ok_or_else(invalid)?;
        let symbol_vt =
            object(id.send_named(id.receiver, b"behaviorOf:", &[Operand::object(symbol)])?)?;
        for o in [ns, roots, string, nil, base, symbol_vt] {
            id.send_named(roots, b"retain:", &[Operand::object(o)])?;
        }
        id.send_named(roots, b"release:", &[Operand::word(symbol_root)])?;
        let state = Box::into_raw(Box::new(State {
            id,
            ns,
            roots,
            string,
            nil,
            thread: std::thread::current().id(),
        }));
        for (vt, name, method) in [
            (base, b"printString".as_slice(), object_print as Method),
            (base, b"asString", as_string),
            (symbol_vt, b"printString", symbol_print),
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
        id.send_named(
            ns,
            b"at:put:",
            &[
                Operand::object(id.intern_name(b"SymbolVt")?),
                Operand::object(symbol_vt),
            ],
        )?;
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
