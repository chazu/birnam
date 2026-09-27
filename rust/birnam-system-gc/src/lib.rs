//! System GC controls through Id messages; collection policy belongs to GC.
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
};
use core::{ffi::c_void, ptr};
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
struct State {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    gc: Oop,
    integer: Oop,
    truth: Oop,
    falsity: Oop,
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
}

unsafe fn operate(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
    op: usize,
) -> Result<Operand, Outcome> {
    unsafe {
        let s = &*c.cast::<State>();
        if s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        if n != usize::from(op >= 2) {
            return Ok(Operand::object(s.nil));
        }
        if op < 2 {
            return s.rooted(&[r], || {
                let count = if op == 0 {
                    s.id.send_named(s.gc, b"collect", &[])?
                } else {
                    s.id.send_named(s.id.receiver, b"allocationCount", &[])?
                }
                .as_word()
                .ok_or_else(invalid)?;
                // Select the live constructor after the query, preserving usize/i64 bits.
                s.id.send_named(s.integer, b"fromWord:", &[Operand::word(count)])
            });
        }
        let Some([value]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let value = object(*value)?;
        if value != s.truth && value != s.falsity {
            return Ok(Operand::object(s.nil));
        }
        s.rooted(&[r, value], || {
            s.id.send_named(
                s.gc,
                if op == 2 {
                    b"automaticCollection:"
                } else {
                    b"precisePayloads:"
                },
                &[Operand::word(usize::from(value == s.truth))],
            )?;
            Ok(Operand::object(r))
        })
    }
}
macro_rules! method {
    ($name:ident, $op:expr) => {
        unsafe extern "C" fn $name(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
            unsafe { operate(c, r, a, n, $op) }.map_or_else(|e| e, Outcome::loaded)
        }
    };
}
method!(collect, 0);
method!(live_objects, 1);
method!(automatic, 2);
method!(precise, 3);
unsafe fn construct(id: Protocol, ns: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let named = |name: &[u8]| {
            object(id.send_named(ns, b"at:", &[Operand::object(id.intern_name(name)?)])?)
        };
        let roots = named(b"Roots")?;
        let nil = named(b"nil")?;
        let gc = named(b"GC")?;
        let integer = named(b"Integer")?;
        let truth = named(b"true")?;
        let falsity = named(b"false")?;
        let system = named(b"System")?;
        for o in [ns, roots, nil, gc, integer, truth, falsity, system] {
            id.send_named(roots, b"retain:", &[Operand::object(o)])?;
        }
        let vt = object(id.send_named(id.receiver, b"behaviorOf:", &[Operand::object(system)])?)?;
        id.send_named(roots, b"retain:", &[Operand::object(vt)])?;
        let state = Box::into_raw(Box::new(State {
            id,
            ns,
            roots,
            gc,
            integer,
            truth,
            falsity,
            nil,
            thread: std::thread::current().id(),
        }));
        for (name, method) in [
            (b"gc".as_slice(), collect as native::Method),
            (b"liveObjects", live_objects),
            (b"autoGc:", automatic),
            (b"preciseGc:", precise),
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
