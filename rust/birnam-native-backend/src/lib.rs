//! Canonical portable programs to native C and frame descriptors. This owner
//! never parses source or owns executable mappings; TCC owns compilation.
mod c_string;
mod literal;
mod native;
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as abi, NativeEntry, Operand, Outcome, Protocol},
};
#[cfg(test)]
use birnam_codegen::{Block, Expr, Statement, portable};
use std::{
    cell::RefCell,
    ffi::c_void,
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};
struct State {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    receiver: Oop,
    thread: std::thread::ThreadId,
}
struct Scope<'a> {
    s: &'a State,
    tokens: RefCell<Vec<usize>>,
}
impl State {
    unsafe fn send(&self, o: Oop, name: &[u8], a: &[Operand]) -> Result<Operand, Outcome> {
        unsafe { self.id.send_named(o, name, a) }
    }
    unsafe fn named(&self, name: &[u8]) -> Result<Oop, Outcome> {
        unsafe {
            object(self.send(
                self.ns,
                b"at:",
                &[Operand::object(self.id.intern_name(name)?)],
            )?)
        }
    }
}
impl Scope<'_> {
    unsafe fn keep(&self, a: Operand) -> Result<Oop, Outcome> {
        let o = object(a)?;
        let token = unsafe { self.s.send(self.s.roots, b"retain:", &[Operand::object(o)]) }?
            .as_word()
            .ok_or_else(invalid)?;
        self.tokens.borrow_mut().push(token);
        Ok(o)
    }
    unsafe fn call(&self, o: Oop, name: &[u8], a: &[Operand]) -> Result<Oop, Outcome> {
        unsafe { self.keep(self.s.send(o, name, a)?) }
    }
    unsafe fn string(&self, b: &[u8]) -> Result<Oop, Outcome> {
        unsafe {
            self.call(
                self.s.named(b"String")?,
                b"fromBytes:",
                &[Operand::bytes(b)],
            )
        }
    }
    unsafe fn bytes(&self, o: Oop) -> Result<Vec<u8>, Outcome> {
        unsafe {
            if self
                .s
                .send(self.s.id.receiver, b"behaviorOf:", &[Operand::object(o)])?
                .as_object()
                != Some(self.s.named(b"StringVt")?)
            {
                return Err(invalid());
            }
            let b = self
                .s
                .send(o, b"bytes", &[])?
                .as_bytes()
                .ok_or_else(invalid)?;
            Ok(borrowed_bytes(b.pointer, b.length)
                .ok_or_else(invalid)?
                .to_vec())
        }
    }
    unsafe fn boolean(&self, o: Oop) -> Result<bool, Outcome> {
        unsafe {
            if o == self.s.named(b"true")? {
                Ok(true)
            } else if o == self.s.named(b"false")? {
                Ok(false)
            } else {
                Err(invalid())
            }
        }
    }
    unsafe fn word(&self, o: Oop) -> Result<usize, Outcome> {
        unsafe {
            if self
                .s
                .send(self.s.id.receiver, b"behaviorOf:", &[Operand::object(o)])?
                .as_object()
                != Some(self.s.named(b"IntegerVt")?)
            {
                return Err(invalid());
            }
            self.s
                .send(self.s.named(b"Integer")?, b"wordOf:", &[Operand::object(o)])?
                .as_word()
                .filter(|n| *n <= isize::MAX as usize)
                .ok_or_else(invalid)
        }
    }
    unsafe fn error(&self, e: native::Error) -> Result<Operand, Outcome> {
        unsafe {
            let kind = self.keep(Operand::object(
                self.s.id.intern_name(b"nativeEmissionRejected")?,
            ))?;
            let message = self.string(format!("{e:?}").as_bytes())?;
            self.call(
                self.s.named(b"Error")?,
                b"kind:message:",
                &[Operand::object(kind), Operand::object(message)],
            )
            .map(Operand::object)
        }
    }
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        for t in self.tokens.get_mut().drain(..).rev() {
            let _ = unsafe { self.s.send(self.s.roots, b"release:", &[Operand::word(t)]) };
        }
    }
}
fn invalid() -> Outcome {
    Outcome::failed(abi::INVALID_REQUEST, 0)
}
fn object(a: Operand) -> Result<Oop, Outcome> {
    a.as_object().filter(|o| !o.is_null()).ok_or_else(invalid)
}
#[derive(Clone, Copy)]
enum Op {
    Program,
    Factory,
    Named,
    Descriptor,
}
struct Entry {
    s: *const State,
    op: Op,
}
unsafe extern "C" fn invoke(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let e = &*c.cast::<Entry>();
        let s = &*e.s;
        if s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        let a = borrowed_slice(a, n).ok_or_else(invalid)?;
        if n != match e.op {
            Op::Program => 5,
            Op::Factory => 3,
            Op::Named => 2,
            Op::Descriptor => 1,
        } {
            return Err(invalid());
        }
        let q = Scope {
            s,
            tokens: RefCell::new(vec![]),
        };
        q.keep(Operand::object(r))?;
        for value in a {
            q.keep(*value)?;
        }
        let arg = |i| object(a[i]);
        let answer = match e.op {
            Op::Program => native::compile_portable_from(
                &q.bytes(arg(0)?)?,
                &q.bytes(arg(1)?)?,
                &q.bytes(arg(2)?)?,
                q.boolean(arg(3)?)?,
                q.boolean(arg(4)?)?,
            ),
            Op::Factory => {
                let program = q.bytes(arg(0)?)?;
                let source = q.bytes(arg(1)?)?;
                let v = arg(2)?;
                if s.send(s.id.receiver, b"behaviorOf:", &[Operand::object(v)])?
                    .as_object()
                    != Some(s.named(b"VectorVt")?)
                {
                    return Err(invalid());
                }
                let count = q.word(q.call(v, b"size", &[])?)?;
                if count > 1_000_000 {
                    return Err(invalid());
                }
                let mut flags = Vec::with_capacity(count);
                for i in 0..count {
                    let index = q.call(s.named(b"Integer")?, b"fromWord:", &[Operand::word(i)])?;
                    flags.push(q.boolean(q.call(v, b"at:", &[Operand::object(index)])?)?);
                }
                native::compile_factory(&program, &source, &flags)
            }
            Op::Named => {
                let arity = u32::try_from(q.word(arg(0)?)?).map_err(|_| invalid())?;
                if q.boolean(arg(1)?)? {
                    native::compile_named_synchronous(arity)
                } else {
                    native::compile_named(arity)
                }
            }
            Op::Descriptor => native::named_descriptor(&q.bytes(arg(0)?)?),
        };
        match answer {
            Ok(bytes) => q.string(&bytes).map(Operand::object),
            Err(error) => q.error(error),
        }
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
static INSTANCE: AtomicPtr<State> = AtomicPtr::new(std::ptr::null_mut());
static BUSY: AtomicBool = AtomicBool::new(false);
unsafe fn construct(id: Protocol, ns: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let key = id.intern_name(b"Roots")?;
        let roots = object(id.send_named(ns, b"at:", &[Operand::object(key)])?)?;
        let mut state = Box::new(State {
            id,
            ns,
            roots,
            receiver: std::ptr::null_mut(),
            thread: std::thread::current().id(),
        });
        let q = Scope {
            s: &state,
            tokens: RefCell::new(vec![]),
        };
        q.keep(Operand::object(ns))?;
        let receiver = q.call(state.named(b"Object")?, b"clone", &[])?;
        let vt = q.call(
            id.receiver,
            b"ensurePrivateVtableFor:",
            &[Operand::object(receiver)],
        )?;
        for o in [ns, receiver, vt] {
            state.send(roots, b"retain:", &[Operand::object(o)])?;
        }
        drop(q);
        state.receiver = receiver;
        let state = Box::into_raw(state);
        for (name, op) in [
            ("emit:source:filename:persistent:synchronous:", Op::Program),
            ("factory:source:dispositions:", Op::Factory),
            ("named:synchronous:", Op::Named),
            ("descriptorFor:", Op::Descriptor),
        ] {
            let entry = Box::into_raw(Box::new(Entry { s: state, op }));
            id.send_named(
                vt,
                b"addMethod",
                &[
                    Operand::object(id.intern_name(name.as_bytes())?),
                    Operand::entry(NativeEntry::new(invoke, entry.cast())),
                ],
            )?;
        }
        for (name, value) in [
            (b"NativeBackend".as_slice(), receiver),
            (b"NativeBackendVt", vt),
        ] {
            id.send_named(
                ns,
                b"at:put:",
                &[
                    Operand::object(id.intern_name(name)?),
                    Operand::object(value),
                ],
            )?;
        }
        INSTANCE.store(state, Ordering::Release);
        Ok(Operand::object(receiver))
    }
}
unsafe extern "C" fn initialize(id: *const Protocol, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        if !abi::valid_protocol(id) {
            return Err(invalid());
        }
        let [ns] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let ns = object(*ns)?;
        let s = INSTANCE.load(Ordering::Acquire);
        if !s.is_null() {
            return if (*s).id.receiver == (*id).receiver
                && (*s).ns == ns
                && (*s).thread == std::thread::current().id()
            {
                Ok(Operand::object((*s).receiver))
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
pub static birnam_native_module_v1: abi::Module = abi::Module {
    version: 1,
    byte_size: size_of::<abi::Module>() as u32,
    initialize: Some(initialize),
};
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
