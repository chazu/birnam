//! Build-only Rust grammar seed; production Lang is modules/lang.birnam.
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{self as native, Method, NativeEntry, Operand, Outcome, Protocol},
};
use core::{
    cell::{Cell, RefCell},
    ffi::c_void,
};
mod grammar;
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
fn object(v: Operand) -> Result<Oop, Outcome> {
    v.as_object().filter(|v| !v.is_null()).ok_or_else(invalid)
}
fn done(v: Result<Operand, Outcome>) -> Outcome {
    v.map_or_else(|e| e, Outcome::loaded)
}
struct State {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    grammar: Cell<Oop>,
    thread: std::thread::ThreadId,
}
struct Build<'a> {
    s: &'a State,
    tokens: RefCell<Vec<usize>>,
}
impl Build<'_> {
    unsafe fn keep(&self, v: Oop) -> Result<Oop, Outcome> {
        let t = unsafe {
            self.s
                .id
                .send_named(self.s.roots, b"retain:", &[Operand::object(v)])
        }?
        .as_word()
        .ok_or_else(invalid)?;
        self.tokens.borrow_mut().push(t);
        Ok(v)
    }
}
impl Drop for Build<'_> {
    fn drop(&mut self) {
        for t in self.tokens.get_mut().drain(..) {
            unsafe {
                self.s
                    .id
                    .send_named(self.s.roots, b"release:", &[Operand::word(t)])
                    .expect("release Lang construction roots");
            }
        }
    }
}
unsafe fn named(s: &State, n: &[u8]) -> Result<Oop, Outcome> {
    unsafe { object(s.id.send_named(s.ns, b"at:", &[Operand::object(s.id.intern_name(n)?)])?) }
}
fn name(n: &[u8]) -> &[u8] {
    n.strip_suffix(&[0]).unwrap_or(n)
}
unsafe fn intern(b: &Build<'_>, n: &[u8]) -> Result<Oop, Outcome> {
    unsafe { b.s.id.intern_name(name(n)) }
}
unsafe fn send(b: &Build<'_>, r: Oop, n: &[u8], a: &[Oop]) -> Result<Oop, Outcome> {
    unsafe {
        b.keep(object(b.s.id.send_named(
            r,
            name(n),
            &a.iter().copied().map(Operand::object).collect::<Vec<_>>(),
        )?)?)
    }
}
unsafe fn send0(b: &Build<'_>, r: Oop, n: &[u8]) -> Result<Oop, Outcome> {
    unsafe { send(b, r, n, &[]) }
}
unsafe fn send1(b: &Build<'_>, r: Oop, n: &[u8], a: Oop) -> Result<Oop, Outcome> {
    unsafe { send(b, r, n, &[a]) }
}
unsafe fn send2(b: &Build<'_>, r: Oop, n: &[u8], a: Oop, c: Oop) -> Result<Oop, Outcome> {
    unsafe { send(b, r, n, &[a, c]) }
}
unsafe fn string(b: &Build<'_>, bytes: &[u8]) -> Result<Oop, Outcome> {
    unsafe {
        b.keep(object(b.s.id.send_named(
            named(b.s, b"String")?,
            b"fromBytes:",
            &[Operand::bytes(bytes)],
        )?)?)
    }
}
unsafe fn integer(b: &Build<'_>, i: usize) -> Result<Oop, Outcome> {
    unsafe {
        b.keep(object(b.s.id.send_named(
            named(b.s, b"Integer")?,
            b"fromWord:",
            &[Operand::word(i)],
        )?)?)
    }
}
unsafe extern "C" fn parse(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = &*c.cast::<State>();
        if s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        let Some([source]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let b = Build {
            s,
            tokens: RefCell::new(Vec::new()),
        };
        b.keep(object(*source)?)?;
        s.id.send_named(
            s.grammar.get(),
            b"parse:from:",
            &[*source, Operand::object(s.id.intern_name(b"program")?)],
        )
    })())
}
unsafe extern "C" fn get_grammar(c: *mut c_void, _: Oop, _: *const Operand, n: usize) -> Outcome {
    if n != 0 {
        return invalid();
    }
    Outcome::loaded(Operand::object(
        unsafe { &*c.cast::<State>() }.grammar.get(),
    ))
}
unsafe extern "C" fn furthest(c: *mut c_void, _: Oop, _: *const Operand, n: usize) -> Outcome {
    if n != 0 {
        return invalid();
    }
    let s = unsafe { &*c.cast::<State>() };
    done(unsafe { s.id.send_named(s.grammar.get(), b"furthestFail", &[]) })
}
unsafe extern "C" fn roots(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = &*c.cast::<State>();
        let Some([v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let v = object(*v)?;
        for o in [s.ns, s.roots, s.grammar.get()] {
            if !o.is_null() {
                s.id.send_named(v, b"visit:", &[Operand::object(o)])?;
            }
        }
        Ok(Operand::unit())
    })())
}
unsafe fn construct(id: Protocol, ns: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let root =
            object(id.send_named(ns, b"at:", &[Operand::object(id.intern_name(b"Roots")?)])?)?;
        let existing = id.send_named(ns, b"at:", &[Operand::object(id.intern_name(b"Lang")?)])?;
        if existing.kind() == native::OBJECT {
            return Ok(existing);
        }
        let s = Box::leak(Box::new(State {
            id,
            ns,
            roots: root,
            grammar: Cell::new(core::ptr::null_mut()),
            thread: std::thread::current().id(),
        }));
        let b = Build {
            s,
            tokens: RefCell::new(Vec::new()),
        };
        let grammar = grammar::build_grammar(&b, named(s, b"Peg")?)?;
        let vt = b.keep(object(id.send_named(
            named(s, b"ObjectVt")?,
            b"delegated",
            &[],
        )?)?)?;
        let lang = b.keep(object(id.send_named(
            named(s, b"ObjectModel")?,
            b"allocate:bytes:",
            &[Operand::object(vt), Operand::word(0)],
        )?)?)?;
        // Grammar remains rooted in b while the completed instance is published.
        let context = (s as *const State).cast_mut();
        s.grammar.set(grammar);
        for (n, f) in [
            (b"parse:".as_slice(), parse as Method),
            (b"grammar", get_grammar),
            (b"furthestFail", furthest),
            (b"rootsDo:", roots),
        ] {
            id.send_named(
                vt,
                b"addMethod",
                &[
                    Operand::object(id.intern_name(n)?),
                    Operand::entry(NativeEntry::new(f, context.cast())),
                ],
            )?;
        }
        id.send_named(root, b"register:", &[Operand::object(lang)])?;
        for (n, v) in [
            (b"Lang".as_slice(), lang),
            (b"LangVt", vt),
            (b"BirnamGrammar", grammar),
        ] {
            id.send_named(
                ns,
                b"at:put:",
                &[Operand::object(id.intern_name(n)?), Operand::object(v)],
            )?;
        }
        Ok(Operand::object(lang))
    }
}
unsafe extern "C" fn initialize(id: *const Protocol, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        if !native::valid_protocol(id) {
            return Err(invalid());
        }
        let Some([ns]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        construct(*id, object(*ns)?)
    })())
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module {
    version: 1,
    byte_size: size_of::<native::Module>() as u32,
    initialize: Some(initialize),
};
