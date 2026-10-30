//! Build-only grammar seed. The grammar remains authored by grammar.rs;
//! all owner operations now pass Oops. Production Lang remains authored Birnam.
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
mod grammar;
struct State {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    grammar: Cell<Oop>,
    receiver: Cell<Oop>,
    names: HashMap<&'static [u8], Oop>,
    thread: std::thread::ThreadId,
    ready: Cell<bool>,
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
fn object(v: Oop) -> Result<Oop, Oop> {
    if v.is_null() { Err(invalid()) } else { Ok(v) }
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
impl State {
    unsafe fn send(&self, r: Oop, n: &[u8], a: &[Oop]) -> Result<Oop, Oop> {
        unsafe {
            self.id
                .send(r, *self.names.get(name(n)).ok_or_else(invalid)?, a)
        }
    }
}
struct Build<'a> {
    s: &'a State,
    tokens: RefCell<Vec<Oop>>,
}
impl Build<'_> {
    unsafe fn keep(&self, v: Oop) -> Result<Oop, Oop> {
        let token = unsafe { self.s.send(self.s.roots, b"retain:", &[v]) }?;
        self.tokens.borrow_mut().push(token);
        Ok(v)
    }
}
impl Drop for Build<'_> {
    fn drop(&mut self) {
        for token in self.tokens.get_mut().drain(..) {
            unsafe {
                self.s
                    .send(self.s.roots, b"release:", &[token])
                    .expect("release grammar construction root");
            }
        }
    }
}
fn name(n: &[u8]) -> &[u8] {
    n.strip_suffix(&[0]).unwrap_or(n)
}
unsafe fn named(s: &State, n: &[u8]) -> Result<Oop, Oop> {
    unsafe { object(s.send(s.ns, b"at:", &[*s.names.get(n).ok_or_else(invalid)?])?) }
}
unsafe fn intern(b: &Build<'_>, n: &[u8]) -> Result<Oop, Oop> {
    b.s.names.get(name(n)).copied().ok_or_else(invalid)
}
unsafe fn send(b: &Build<'_>, r: Oop, n: &[u8], a: &[Oop]) -> Result<Oop, Oop> {
    unsafe { b.keep(object(b.s.send(r, n, a)?)?) }
}
unsafe fn send0(b: &Build<'_>, r: Oop, n: &[u8]) -> Result<Oop, Oop> {
    unsafe { send(b, r, n, &[]) }
}
unsafe fn send1(b: &Build<'_>, r: Oop, n: &[u8], a: Oop) -> Result<Oop, Oop> {
    unsafe { send(b, r, n, &[a]) }
}
unsafe fn send2(b: &Build<'_>, r: Oop, n: &[u8], a: Oop, c: Oop) -> Result<Oop, Oop> {
    unsafe { send(b, r, n, &[a, c]) }
}
unsafe fn string(b: &Build<'_>, bytes: &[u8]) -> Result<Oop, Oop> {
    unsafe {
        let factory = b.keep(named(b.s, b"String")?)?;
        let source = abi::byte_transfer::buffer(bytes, named(b.s,b"Bytes")?,
            |r,n,a| b.s.send(r,n,a), |v| b.keep(v))?;
        send1(b, factory, b"fromBytes:", source)
    }
}
unsafe fn integer(b: &Build<'_>, v: usize) -> Result<Oop, Oop> {
    unsafe {
        send1(
            b,
            named(b.s, b"Integer")?,
            b"fromWord:",
            abi::size(v).ok_or_else(invalid)?,
        )
    }
}
unsafe extern "C" fn parse(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [source] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        s.send(
            s.grammar.get(),
            b"parse:from:",
            &[*source, s.names[b"program".as_slice()]],
        )
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn get_grammar(_: Oop, c: Oop, _: Oop, _: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        if n != 0 {
            return Err(invalid());
        }
        Ok(s.grammar.get())
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn furthest(_: Oop, c: Oop, _: Oop, _: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        if n != 0 {
            return Err(invalid());
        }
        s.send(s.grammar.get(), b"furthestFail", &[])
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn roots(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [visitor] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        for value in [s.ns, s.roots, s.grammar.get(), s.receiver.get()] {
            if !value.is_null() {
                s.send(*visitor, b"visit:", &[value])?;
            }
        }
        Ok(ptr::null_mut())
    })()
    .unwrap_or_else(abi::fault_reference)
}
const NAMES: &[&[u8]] = &[
    b"#",
    b"#[",
    b",",
    b"-",
    b"->",
    b".",
    b"//",
    b":",
    b":=",
    b";",
    b"=>",
    b"?",
    b"BirnamGrammar",
    b"Integer",
    b"Lang",
    b"LangVt",
    b"ObjectModel",
    b"ObjectVt",
    b"Peg",
    b"Roots",
    b"String",
    b"[",
    b"]",
    b"add:",
    b"addMethod",
    b"allocate:bytes:",
    b"any",
    b"at:",
    b"at:put:",
    b"binaryMsg",
    b"binop",
    b"binops",
    b"block",
    b"charset",
    b"choice",
    b"delegated",
    b"eof",
    b"expr",
    b"float",
    b"from:to:",
    b"fromBytes:",
    b"fromWord:",
    b"furthestFail",
    b"grammar",
    b"ident",
    b"keyword",
    b"keywordMsg",
    b"let",
    b"letBinding",
    b"literal:",
    b"logicVar",
    b"mapEntry",
    b"mapLit",
    b"message",
    b"methodAt:",
    b"nameWithBytes",
    b"negate",
    b"number",
    b"opt",
    b"parse:",
    b"parse:from:",
    b"plus",
    b"primary",
    b"program",
    b"register:",
    b"release:",
    b"retain:",
    b"rootsDo:",
    b"rule:is:",
    b"ruleRef:",
    b"send",
    b"seq",
    b"sequence",
    b"star",
    b"statement",
    b"statements",
    b"string",
    b"symbol",
    b"threadDispatch",
    b"threadedMsg",
    b"unaryMsg",
    b"vectorLit",
    b"visit:",
    b"withContext:",
    b"{",
    b"|",
    b"}",
    b"Bytes",
    b"fromByteValues",
    b"appendByteValues",
];
static LITERALS: &[Literal] = &abi::id_protocol::literal_table::<{ NAMES.len() }>(NAMES);
static METHODS: &[MethodSpec] = &[
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"parse:"),
        entry: Some(parse),
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"grammar"),
        entry: Some(get_grammar),
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"furthestFail"),
        entry: Some(furthest),
    },
    MethodSpec {
        name: birnam_abi::id_protocol::literal_index(NAMES, b"rootsDo:"),
        entry: Some(roots),
    },
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
            s.receiver.get()
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
        let root =
            object(id.send(*ns, names[b"at:".as_slice()], &[names[b"Roots".as_slice()]])?)?;
        let s = Box::leak(Box::new(State {
            id,
            ns: *ns,
            roots: root,
            grammar: Cell::new(ptr::null_mut()),
            receiver: Cell::new(ptr::null_mut()),
            names,
            thread: std::thread::current().id(),
            ready: Cell::new(false),
        }));
        INSTANCE.store(s, Ordering::Release);
        let b = Build {
            s,
            tokens: RefCell::new(Vec::new()),
        };
        let grammar = grammar::build_grammar(&b, named(s, b"Peg")?)?;
        let vt = send0(&b, named(s, b"ObjectVt")?, b"delegated")?;
        let receiver = send(
            &b,
            named(s, b"ObjectModel")?,
            b"allocate:bytes:",
            &[vt, abi::size(0).unwrap()],
        )?;
        s.grammar.set(grammar);
        s.receiver.set(receiver);
        for n in [
            b"parse:".as_slice(),
            b"grammar",
            b"furthestFail",
            b"rootsDo:",
        ] {
            let method = send1(&b, image, b"methodAt:", s.names[n])?;
            let method = send1(&b, method, b"withContext:", *ns)?;
            send2(&b, vt, b"addMethod", s.names[n], method)?;
        }
        send1(&b, root, b"register:", receiver)?;
        for (n, v) in [
            (b"Lang".as_slice(), receiver),
            (b"LangVt", vt),
            (b"BirnamGrammar", grammar),
        ] {
            send2(&b, *ns, b"at:put:", s.names[n], v)?;
        }
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
