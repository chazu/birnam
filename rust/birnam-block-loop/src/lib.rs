//! Bootstrap Block loops. Condition/body evaluation and all dependencies use Id
//! sends; the existing synchronous boundary is replaced by authored control later.
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
    string: Oop,
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
                for &value in values {
                    tokens.push(
                        self.id
                            .send_named(self.roots, b"retain:", &[Operand::object(value)])?
                            .as_word()
                            .ok_or_else(invalid)?,
                    );
                }
                f()
            })();
            for token in tokens {
                self.id
                    .send_named(self.roots, b"release:", &[Operand::word(token)])?;
            }
            result
        }
    }
    unsafe fn named(&self, name: &[u8]) -> Result<Operand, Outcome> {
        unsafe {
            self.id.send_named(
                self.ns,
                b"at:",
                &[Operand::object(self.id.intern_name(name)?)],
            )
        }
    }
    unsafe fn is_error(&self, value: Oop) -> Result<bool, Outcome> {
        unsafe {
            // Preserve the bootstrap loop's exact-family recognition and lazy
            // Error publication; an overridden isError is not a new policy.
            let family = self.named(b"ErrorVt")?;
            let Some(family) = family
                .as_object()
                .filter(|o| !o.is_null() && *o != self.nil)
            else {
                return Ok(false);
            };
            let mut found = false;
            self.rooted(&[family], || {
                found = self
                    .id
                    .send_named(self.id.receiver, b"behaviorOf:", &[Operand::object(value)])?
                    .as_object()
                    == Some(family);
                Ok(Operand::unit())
            })?;
            Ok(found)
        }
    }
    unsafe fn must_be_boolean(&self) -> Result<Operand, Outcome> {
        unsafe {
            let kind = self.id.intern_name(b"mustBeBoolean")?;
            self.rooted(&[kind], || {
                let message = object(self.id.send_named(
                    self.string,
                    b"fromBytes:",
                    &[Operand::bytes(
                        b"while loop condition must answer true or false",
                    )],
                )?)?;
                self.rooted(&[message], || {
                    let factory = self.named(b"Error")?;
                    let Some(factory) = factory
                        .as_object()
                        .filter(|o| !o.is_null() && *o != self.nil)
                    else {
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
unsafe fn run(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
    continue_on_true: bool,
) -> Result<Operand, Outcome> {
    unsafe {
        let s = &*c.cast::<State>();
        if s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        let Some([body]) = borrowed_slice(a, n) else {
            return Ok(Operand::object(s.nil));
        };
        let body = object(*body)?;
        s.rooted(&[r, body], || {
            loop {
                let condition = object(s.id.send_named(r, b"value", &[])?)?;
                // Per-result roots end every iteration; a long loop does not
                // accumulate live callback results or retain dead code owners.
                let mut again = false;
                let result = s.rooted(&[condition], || {
                    if s.is_error(condition)? {
                        return Ok(Operand::object(condition));
                    }
                    if condition != s.truth && condition != s.falsity {
                        return s.must_be_boolean();
                    }
                    if (condition == s.truth) != continue_on_true {
                        return Ok(Operand::object(s.nil));
                    }
                    let value = object(s.id.send_named(body, b"value", &[])?)?;
                    s.rooted(&[value], || {
                        if s.is_error(value)? {
                            Ok(Operand::object(value))
                        } else {
                            again = true;
                            Ok(Operand::object(s.nil))
                        }
                    })
                })?;
                if !again {
                    return Ok(result);
                }
            }
        })
    }
}
unsafe extern "C" fn while_true(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe { run(c, r, a, n, true) }.map_or_else(|e| e, Outcome::loaded)
}
unsafe extern "C" fn while_false(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe { run(c, r, a, n, false) }.map_or_else(|e| e, Outcome::loaded)
}
unsafe fn construct(id: Protocol, ns: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let named = |name: &[u8]| {
            object(id.send_named(ns, b"at:", &[Operand::object(id.intern_name(name)?)])?)
        };
        let roots = named(b"Roots")?;
        let string = named(b"String")?;
        let truth = named(b"true")?;
        let falsity = named(b"false")?;
        let nil = named(b"nil")?;
        let block = named(b"Block")?;
        for value in [ns, roots, string, truth, falsity, nil, block] {
            id.send_named(roots, b"retain:", &[Operand::object(value)])?;
        }
        let behavior =
            object(id.send_named(id.receiver, b"behaviorOf:", &[Operand::object(block)])?)?;
        id.send_named(roots, b"retain:", &[Operand::object(behavior)])?;
        let state = Box::into_raw(Box::new(State {
            id,
            ns,
            roots,
            string,
            truth,
            falsity,
            nil,
            thread: std::thread::current().id(),
        }));
        for (name, method) in [
            (b"whileTrue:".as_slice(), while_true as native::Method),
            (b"whileFalse:", while_false),
        ] {
            id.send_named(
                behavior,
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
            return if s.id.same_contract(&*id)
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
    version: native::VERSION,
    byte_size: size_of::<native::Module>() as u32,
    initialize: Some(initialize),
};
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
