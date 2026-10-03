//! Complete image container encoding/admission; no live heap publication.
use birnam_image_codec::v8;
// Preserve Image's distinguished semantic refusal; unrelated native failures propagate.
fn refused() -> Outcome {
    Outcome::failed(abi::INVALID_REQUEST, 0x494d4147)
}
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
    unsafe fn bytes(&self, o: Oop) -> Result<Vec<u8>, Outcome> {
        unsafe {
            if self
                .s
                .send(self.s.id.receiver, b"behaviorOf:", &[Operand::object(o)])?
                .as_object()
                != Some(self.s.named(b"BytesVt")?)
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
    unsafe fn number(&self, o: Oop) -> Result<usize, Outcome> {
        unsafe {
            self.s
                .send(self.s.named(b"Integer")?, b"wordOf:", &[Operand::object(o)])?
                .as_word()
                .ok_or_else(invalid)
        }
    }
    unsafe fn blob(&self, b: &[u8]) -> Result<Oop, Outcome> {
        unsafe { self.call(self.s.named(b"Bytes")?, b"fromBytes:", &[Operand::bytes(b)]) }
    }
    unsafe fn vector(&self, values: &[Oop]) -> Result<Oop, Outcome> {
        unsafe {
            let mut v = self.call(self.s.named(b"Vector")?, b"new", &[])?;
            for &o in values {
                v = self.call(v, b"add:", &[Operand::object(o)])?;
            }
            Ok(v)
        }
    }
    unsafe fn sequence(&self, v: Oop) -> Result<Vec<Oop>, Outcome> {
        unsafe {
            if self
                .s
                .send(self.s.id.receiver, b"behaviorOf:", &[Operand::object(v)])?
                .as_object()
                != Some(self.s.named(b"VectorVt")?)
            {
                return Err(invalid());
            }
            let n = self.number(self.call(v, b"size", &[])?)?;
            if n > 1_000_000 {
                return Err(invalid());
            }
            let mut out = Vec::new();
            for i in 0..n {
                let index =
                    self.call(self.s.named(b"Integer")?, b"fromWord:", &[Operand::word(i)])?;
                out.push(self.call(v, b"at:", &[Operand::object(index)])?);
            }
            Ok(out)
        }
    }
    unsafe fn optional(&self, o: Oop) -> Result<Option<Vec<u8>>, Outcome> {
        unsafe {
            if o == self.s.named(b"nil")? {
                Ok(None)
            } else {
                self.bytes(o).map(Some)
            }
        }
    }
    unsafe fn blobs(&self, v: Oop) -> Result<Vec<Vec<u8>>, Outcome> {
        unsafe {
            self.sequence(v)?
                .into_iter()
                .map(|o| self.bytes(o))
                .collect()
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
unsafe extern "C" fn compose(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let s = &*c.cast::<State>();
        if s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        let Some([input]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let q = Scope {
            s,
            tokens: RefCell::new(Vec::new()),
        };
        q.keep(Operand::object(r))?;
        let input = q.keep(*input)?;
        let fields = q.sequence(input)?;
        let [
            architecture,
            abi,
            programs,
            bindings,
            selection,
            heaps,
            facts,
            code,
            sources,
            behaviors,
        ] = fields.as_slice()
        else {
            return Err(invalid());
        };
        let architecture = u32::try_from(q.number(*architecture)?).map_err(|_| invalid())?;
        let runtime_abi = q.number(*abi)? as u64;
        let programs = q.bytes(*programs)?;
        let bindings = q.bytes(*bindings)?;
        let selection = q.bytes(*selection)?;
        let heaps = q.blobs(*heaps)?;
        let facts = q.blobs(*facts)?;
        let code = q.optional(*code)?;
        let sources = q.optional(*sources)?;
        let behaviors = q.optional(*behaviors)?;
        let heaps = heaps.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let facts = facts.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let bytes = v8::compose(v8::ImageSpec {
            architecture,
            runtime_abi,
            programs: &programs,
            bindings: &bindings,
            selection: &selection,
            heaps: &heaps,
            facts: &facts,
            code: code.as_deref(),
            sources: sources.as_deref(),
            behaviors: behaviors.as_deref(),
        })
        .map_err(|_| refused())?;
        q.blob(&bytes).map(Operand::object)
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
unsafe extern "C" fn inspect(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let s = &*c.cast::<State>();
        if s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        let Some([input, architecture, abi]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let q = Scope {
            s,
            tokens: RefCell::new(Vec::new()),
        };
        q.keep(Operand::object(r))?;
        let input = q.keep(*input)?;
        let architecture = q.keep(*architecture)?;
        let abi = q.keep(*abi)?;
        let bytes = q.bytes(input)?;
        let architecture = u32::try_from(q.number(architecture)?).map_err(|_| invalid())?;
        let abi = q.number(abi)? as u64;
        let decoded = v8::decode(&bytes, architecture, abi).map_err(|_| refused())?;
        let blobs = |parts: &[&[u8]]| -> Result<Oop, Outcome> {
            let values = parts
                .iter()
                .map(|b| q.blob(b))
                .collect::<Result<Vec<_>, _>>()?;
            q.vector(&values)
        };
        let optional = |b: Option<&[u8]>| -> Result<Oop, Outcome> {
            if let Some(b) = b {
                q.blob(b)
            } else {
                s.named(b"nil")
            }
        };
        let fields = [
            q.blob(decoded.programs)?,
            q.blob(decoded.bindings)?,
            q.blob(decoded.selection)?,
            blobs(&decoded.heaps)?,
            blobs(&decoded.facts)?,
            optional(decoded.code)?,
            optional(decoded.sources)?,
            optional(decoded.behaviors)?,
        ];
        q.vector(&fields).map(Operand::object)
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
                Operand::object(id.intern_name(b"compose:")?),
                Operand::entry(NativeEntry::new(compose, state.cast())),
            ],
        )?;
        id.send_named(
            vt,
            b"addMethod",
            &[
                Operand::object(id.intern_name(b"inspect:architecture:abi:")?),
                Operand::entry(NativeEntry::new(inspect, state.cast())),
            ],
        )?;
        for (name, value) in [
            (b"ImageFormat".as_slice(), receiver),
            (b"ImageFormatVt", vt),
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
