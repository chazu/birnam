//! Value-only wrapper compilation over Id.
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as abi, NativeEntry, Operand, Outcome, Protocol},
};
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
    unsafe fn error(&self, text: &[u8]) -> Result<Operand, Outcome> {
        unsafe {
            let kind = self.keep(Operand::object(self.s.id.intern_name(b"badSignature")?))?;
            let text = self.string(text)?;
            self.s.send(
                self.s.named(b"Error")?,
                b"kind:message:",
                &[Operand::object(kind), Operand::object(text)],
            )
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
unsafe extern "C" fn invoke(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let s = &*c.cast::<State>();
        if s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        let Some([name, signature]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let q = Scope {
            s,
            tokens: RefCell::new(Vec::new()),
        };
        q.keep(Operand::object(r))?;
        let name = q.keep(*name)?;
        let signature = q.keep(*signature)?;
        let name = if name == s.named(b"nil")? {
            None
        } else {
            Some(q.bytes(name)?)
        };
        let parsed = match crate::parse_signature(&q.bytes(signature)?) {
            Ok(s) => s,
            Err(e) => return q.error(e.to_string().as_bytes()),
        };
        let _classes = crate::abi::classify_signature(&parsed);
        match crate::codegen::emit_native(name.as_deref(), &parsed) {
            Ok(c) => q.string(c.as_bytes()).map(Operand::object),
            Err(e) => q.error(e.to_string().as_bytes()),
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
        id.send_named(
            vt,
            b"addMethod",
            &[
                Operand::object(id.intern_name(b"compile:signature:")?),
                Operand::entry(NativeEntry::new(invoke, state.cast())),
            ],
        )?;
        for (name, value) in [
            (b"AlienCompiler".as_slice(), receiver),
            (b"AlienCompilerVt", vt),
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
