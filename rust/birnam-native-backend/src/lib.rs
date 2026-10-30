//! Native emission from portable programs through Oop owner messages.
mod c_string;
pub mod oop_emission;
use birnam_abi::{
    self as abi, Oop, borrowed_slice,
    id_protocol::{Literal, MethodSpec, Module, Protocol},
};
use core::{
    cell::{Cell, RefCell},
    ptr,
};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};
struct State {
    pub(crate) byte_reader: abi::byte_transfer::Reader,
    id: Protocol,
    ns: Oop,
    receiver: Oop,
    names: HashMap<&'static [u8], Oop>,
    scoped: RefCell<Vec<Oop>>,
    ready: Cell<bool>,
    thread: std::thread::ThreadId,
}
static INSTANCE: AtomicPtr<State> = AtomicPtr::new(ptr::null_mut());
static API: AtomicPtr<Protocol> = AtomicPtr::new(ptr::null_mut());
static BUSY: AtomicBool = AtomicBool::new(false);
struct Constructing;
impl Drop for Constructing {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::Release);
    }
}
fn invalid() -> Oop {
    let p = API.load(Ordering::Acquire);
    if p.is_null() {
        std::process::abort();
    }
    unsafe { abi::object_reference((*p).invalid) }
}
unsafe fn state<'a>(c: Oop) -> Result<&'a State, Oop> {
    let p = INSTANCE.load(Ordering::Acquire);
    if p.is_null() {
        return Err(invalid());
    }
    let s = unsafe { &*p };
    if c != s.ns || s.thread != std::thread::current().id() {
        return Err(invalid());
    }
    Ok(s)
}
struct Scope<'a> {
    s: &'a State,
    start: usize,
}
impl Scope<'_> {
    fn keep(&self, v: Oop) -> Oop {
        self.s.scoped.borrow_mut().push(v);
        v
    }
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        self.s.scoped.borrow_mut().truncate(self.start);
    }
}
impl State {
    fn scope(&self, values: &[Oop]) -> Scope<'_> {
        let start = self.scoped.borrow().len();
        self.scoped.borrow_mut().extend_from_slice(values);
        Scope { s: self, start }
    }
    unsafe fn send(&self, r: Oop, n: &[u8], a: &[Oop]) -> Result<Oop, Oop> {
        unsafe { self.id.send(r, self.names[n], a) }
    }
    unsafe fn named(&self, n: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            let v = self.send(self.ns, b"at:", &[self.names[n]])?;
            if v.is_null() { Err(invalid()) } else { Ok(v) }
        }
    }
    unsafe fn bytes(&self, v: Oop) -> Result<Vec<u8>, Oop> {
        unsafe {
            let q = self.scope(&[v]);
            let buffer = q.keep(self.send(v, b"bytes", &[])?);
            self.byte_reader.read(buffer)
        }
    }
    unsafe fn text(&self, bytes: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            let q = self.scope(&[]);
            let factory = q.keep(self.named(b"String")?);
            let values: Vec<_> = bytes
                .iter()
                .map(|&b| abi::size(b as usize).unwrap())
                .collect();
            let data = self.send(self.id.root, b"sequence", &values)?;
            self.send(factory, b"fromBytes:", &[data])
        }
    }
    unsafe fn error(&self, message: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            let q = self.scope(&[]);
            let message = q.keep(self.text(message)?);
            self.send(
                self.named(b"Error")?,
                b"kind:message:",
                &[self.names[b"compileError".as_slice()], message],
            )
        }
    }
}
unsafe extern "C" fn roots(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [v] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let mut values = vec![s.ns, s.receiver];
        values.extend_from_slice(&s.scoped.borrow());
        for value in values {
            s.send(*v, b"visit:", &[value])?;
        }
        Ok(ptr::null_mut())
    })()
    .unwrap_or_else(abi::fault_reference)
}

unsafe extern "C" fn emit(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [program] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let bytes = s.bytes(*program)?;
        match oop_emission::compile(&bytes) {
            Ok(c) => s.text(&c),
            Err(e) => s.error(format!("synchronous emission: {e:?}").as_bytes()),
        }
    })()
    .unwrap_or_else(abi::fault_reference)
}

unsafe extern "C" fn emit_resumable(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [program, source] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let bytes = s.bytes(*program)?;
        let source = s.bytes(*source)?;
        match oop_emission::compile_resumable(&bytes, &source) {
            Ok(c) => s.text(&c),
            Err(e) => s.error(format!("resumable emission: {e:?}").as_bytes()),
        }
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn emit_program(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [program, source, filename, persistent, synchronous] =
            borrowed_slice(a, n).ok_or_else(invalid)?
        else {
            return Err(invalid());
        };
        let q = s.scope(borrowed_slice(a, n).ok_or_else(invalid)?);
        let truth = q.keep(s.named(b"true")?);
        let falsity = q.keep(s.named(b"false")?);
        let flag = |v| {
            if v == truth {
                Ok(true)
            } else if v == falsity {
                Ok(false)
            } else {
                Err(invalid())
            }
        };
        let persistent = flag(*persistent)?;
        let synchronous = flag(*synchronous)?;
        let program = s.bytes(*program)?;
        let source = s.bytes(*source)?;
        let filename = s.bytes(*filename)?;
        match oop_emission::compile_from(&program, &source, &filename, persistent, synchronous) {
            Ok(c) => s.text(&c),
            Err(e) => s.error(format!("source emission: {e:?}").as_bytes()),
        }
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe fn factory(c: Oop, a: *const Oop, n: usize, persistent: bool) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [program, source, choices] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[*program, *source, *choices]);
        let integer = q.keep(s.named(b"Integer")?);
        let truth = q.keep(s.named(b"true")?);
        let falsity = q.keep(s.named(b"false")?);
        let size = q.keep(s.send(*choices, b"size", &[])?);
        let count = abi::as_size(s.send(integer, b"wordOf:", &[size])?)
            .filter(|n| *n <= 65536)
            .ok_or_else(invalid)?;
        let mut dispositions = Vec::new();
        for i in 0..count {
            let index = q.keep(s.send(integer, b"fromWord:", &[abi::size(i).unwrap()])?);
            let flag = s.send(*choices, b"at:", &[index])?;
            dispositions.push(if flag == truth {
                true
            } else if flag == falsity {
                false
            } else {
                return Err(invalid());
            });
        }
        match oop_emission::compile_factory(
            &s.bytes(*program)?,
            &s.bytes(*source)?,
            &dispositions,
            persistent,
        ) {
            Ok(c) => s.text(&c),
            Err(e) => s.error(format!("factory emission: {e:?}").as_bytes()),
        }
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn module_factory(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { factory(c, a, n, true) }
}
unsafe extern "C" fn block_factory(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { factory(c, a, n, false) }
}
const OWNER_NAME: &[u8] = b"NativeBackend";
const NAMES: &[&[u8]] = &[
    b"Error",
    b"NativeBackend",
    b"ObjectVt",
    b"Roots",
    b"String",
    b"addMethod",
    b"allocate",
    b"at:",
    b"at:put:",
    b"atWord:",
    b"bytes",
    b"compileError",
    b"delegated",
    b"emitSynchronous:",
    b"fromBytes:",
    b"kind:message:",
    b"methodAt:",
    b"register:",
    b"release:",
    b"retain:",
    b"rootsDo:",
    b"sequence",
    b"sizeWord",
    b"visit:",
    b"withContext:",
    b"emitResumable:source:",
    b"emit:source:filename:persistent:synchronous:",
    b"true",
    b"false",
    b"moduleFactory:source:dispositions:",
    b"factory:source:dispositions:",
    b"Integer",
    b"size",
    b"wordOf:",
    b"fromWord:",
    b"copyByteValuesFrom:count:to:",
    b"appendByteValues",
    b"objectBehavior",
    b"releaseTokensProtecting",
    b"receiveByteValues",
];
static LITERALS: &[Literal] = &[
    Literal::new(b"Error"),
    Literal::new(b"NativeBackend"),
    Literal::new(b"ObjectVt"),
    Literal::new(b"Roots"),
    Literal::new(b"String"),
    Literal::new(b"addMethod"),
    Literal::new(b"allocate"),
    Literal::new(b"at:"),
    Literal::new(b"at:put:"),
    Literal::new(b"atWord:"),
    Literal::new(b"bytes"),
    Literal::new(b"compileError"),
    Literal::new(b"delegated"),
    Literal::new(b"emitSynchronous:"),
    Literal::new(b"fromBytes:"),
    Literal::new(b"kind:message:"),
    Literal::new(b"methodAt:"),
    Literal::new(b"register:"),
    Literal::new(b"release:"),
    Literal::new(b"retain:"),
    Literal::new(b"rootsDo:"),
    Literal::new(b"sequence"),
    Literal::new(b"sizeWord"),
    Literal::new(b"visit:"),
    Literal::new(b"withContext:"),
    Literal::new(b"emitResumable:source:"),
    Literal::new(b"emit:source:filename:persistent:synchronous:"),
    Literal::new(b"true"),
    Literal::new(b"false"),
    Literal::new(b"moduleFactory:source:dispositions:"),
    Literal::new(b"factory:source:dispositions:"),
    Literal::new(b"Integer"),
    Literal::new(b"size"),
    Literal::new(b"wordOf:"),
    Literal::new(b"fromWord:"),
    Literal::new(b"copyByteValuesFrom:count:to:"),
    Literal::new(b"appendByteValues"),
    Literal::new(b"objectBehavior"),
    Literal::new(b"releaseTokensProtecting"),
    Literal::new(b"receiveByteValues"),
];
static METHODS: &[MethodSpec] = &[
    MethodSpec {
        name: 13,
        entry: Some(emit),
    },
    MethodSpec {
        name: 20,
        entry: Some(roots),
    },
    MethodSpec {
        name: 25,
        entry: Some(emit_resumable),
    },
    MethodSpec {
        name: 26,
        entry: Some(emit_program),
    },
    MethodSpec {
        name: 29,
        entry: Some(module_factory),
    },
    MethodSpec {
        name: 30,
        entry: Some(block_factory),
    },

    MethodSpec { name: NAMES.len()-1, entry: Some(abi::byte_transfer::receive_entry) },
];
unsafe extern "C" fn initialize(
    p: *const Protocol,
    image: Oop,
    l: *const Oop,
    n: usize,
    a: *const Oop,
    count: usize,
) -> Oop {
    API.store(p.cast_mut(), Ordering::Release);
    let id = unsafe { *p };
    let Some([ns]) = (unsafe { borrowed_slice(a, count) }) else {
        return id.invalid;
    };
    let Some(symbols) = (unsafe { borrowed_slice(l, n) }) else {
        return id.invalid;
    };
    if n != NAMES.len() {
        return id.invalid;
    }
    let old = INSTANCE.load(Ordering::Acquire);
    if !old.is_null() {
        let s = unsafe { &*old };
        return if s.ready.get() && s.ns == *ns && s.id.root == id.root {
            s.receiver
        } else {
            id.invalid
        };
    }
    if BUSY.swap(true, Ordering::AcqRel) {
        return id.invalid;
    }
    let _busy = Constructing;
    (|| unsafe {
        let names: HashMap<_, _> = NAMES.iter().copied().zip(symbols.iter().copied()).collect();
        let send = |r, n: &[u8], a: &[Oop]| id.send(r, names[n], a);
        let named = |n: &[u8]| send(*ns, b"at:", &[names[n]]);
        let roots = named(b"Roots")?;
        let vt = send(named(b"ObjectVt")?, b"delegated", &[])?;
        let token = send(roots, b"retain:", &[vt])?;
        let receiver = send(vt, b"allocate", &[abi::size(0).unwrap()])?;
        let receiver_token = send(roots, b"retain:", &[receiver])?;
        let s = Box::leak(Box::new(State {
                byte_reader: abi::byte_transfer::Reader::new(id, image, roots, |n| names[n])?,
            id,
            ns: *ns,
            receiver,
            names: names.clone(),
            scoped: RefCell::new(Vec::new()),
            ready: Cell::new(false),
            thread: std::thread::current().id(),
        }));
        INSTANCE.store(s, Ordering::Release);
        for spec in &METHODS[..METHODS.len()-1] {
            let method = send(image, b"methodAt:", &[symbols[spec.name]])?;
            let method = send(method, b"withContext:", &[*ns])?;
            send(vt, b"addMethod", &[symbols[spec.name], method])?;
        }
        send(roots, b"register:", &[receiver])?;
        send(*ns, b"at:put:", &[names[OWNER_NAME], receiver])?;
        send(roots, b"release:", &[token])?;
        send(roots, b"release:", &[receiver_token])?;
        s.ready.set(true);
        Ok(receiver)
    })()
    .unwrap_or_else(abi::fault_reference)
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v2: Module = Module {
    version: 2,
    byte_size: size_of::<Module>() as u32,
    literals: LITERALS.as_ptr(),
    literal_count: LITERALS.len(),
    methods: METHODS.as_ptr(),
    method_count: METHODS.len(),
    initialize: Some(initialize),
};
