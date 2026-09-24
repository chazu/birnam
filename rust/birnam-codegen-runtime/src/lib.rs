//! Source compiler facade. Parsing, values, installation and code resources are
//! live message consumers; lowering and byte codecs remain private pure helpers.
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
};
use birnam_codegen::{OwnedCst, lower_program, native as emission};
use core::{cell::RefCell, ffi::c_void};
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
    permanent: Vec<Oop>,
    thread: std::thread::ThreadId,
}
struct Scope<'a> {
    s: &'a State,
    tokens: RefCell<Vec<usize>>,
}
impl Scope<'_> {
    unsafe fn keep(&self, o: Oop) -> Result<Oop, Outcome> {
        let t = unsafe {
            self.s
                .id
                .send_named(self.s.roots, b"retain:", &[Operand::object(o)])
        }?
        .as_word()
        .ok_or_else(invalid)?;
        self.tokens.borrow_mut().push(t);
        Ok(o)
    }
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        for t in self.tokens.get_mut().drain(..) {
            unsafe {
                self.s
                    .id
                    .send_named(self.s.roots, b"release:", &[Operand::word(t)])
                    .expect("release Codegen root");
            }
        }
    }
}
impl State {
    unsafe fn send(&self, r: Oop, n: &[u8], a: &[Operand]) -> Result<Operand, Outcome> {
        unsafe { self.id.send_named(r, n, a) }
    }
    unsafe fn named(&self, n: &[u8]) -> Result<Oop, Outcome> {
        unsafe { object(self.send(self.ns, b"at:", &[Operand::object(self.id.intern_name(n)?)])?) }
    }
    unsafe fn bytes(&self, o: Oop) -> Result<Vec<u8>, Outcome> {
        unsafe {
            let b = self
                .send(o, b"bytes", &[])?
                .as_bytes()
                .ok_or_else(invalid)?;
            Ok(borrowed_bytes(b.pointer, b.length)
                .ok_or_else(invalid)?
                .to_vec())
        }
    }
    unsafe fn error(&self, scope: &Scope<'_>, text: &[u8]) -> Result<Operand, Outcome> {
        unsafe {
            let m = scope.keep(object(self.send(
                self.named(b"String")?,
                b"fromBytes:",
                &[Operand::bytes(text)],
            )?)?)?;
            self.send(
                self.named(b"Error")?,
                b"kind:message:",
                &[
                    Operand::object(self.id.intern_name(b"compileError")?),
                    Operand::object(m),
                ],
            )
        }
    }
    unsafe fn is_error(&self, o: Oop) -> Result<bool, Outcome> {
        unsafe { Ok(self.send(o, b"isError", &[])?.as_object() == Some(self.named(b"true")?)) }
    }
    unsafe fn index(&self, scope: &Scope<'_>, o: Oop, n: &[u8]) -> Result<usize, Outcome> {
        unsafe {
            let v = scope.keep(object(self.send(o, n, &[])?)?)?;
            self.send(self.named(b"Integer")?, b"wordOf:", &[Operand::object(v)])?
                .as_word()
                .filter(|i| *i <= isize::MAX as usize)
                .ok_or_else(invalid)
        }
    }
    unsafe fn project(
        &self,
        scope: &Scope<'_>,
        o: Oop,
        source_len: usize,
        depth: usize,
        budget: &mut usize,
    ) -> Result<OwnedCst, Outcome> {
        unsafe {
            if depth > 4096 || *budget == 0 {
                return Err(invalid());
            }
            *budget -= 1;
            let rule = scope.keep(object(self.send(o, b"ruleName", &[])?)?)?;
            let rule = self.bytes(rule)?;
            let start = self.index(scope, o, b"start")?;
            let end = self.index(scope, o, b"end")?;
            if start > end || end > source_len {
                return Err(invalid());
            }
            let count = self.index(scope, o, b"childCount")?;
            if count > *budget {
                return Err(invalid());
            }
            let mut children = Vec::new();
            for i in 0..count {
                let index = scope.keep(object(self.send(
                    self.named(b"Integer")?,
                    b"fromWord:",
                    &[Operand::word(i)],
                )?)?)?;
                let child = scope.keep(object(self.send(
                    o,
                    b"childAt:",
                    &[Operand::object(index)],
                )?)?)?;
                children.push(self.project(scope, child, source_len, depth + 1, budget)?);
            }
            Ok(OwnedCst::new(&rule, start, end, children))
        }
    }
    unsafe fn native_method(
        &self,
        scope: &Scope<'_>,
        c: &[u8],
        implementation: Option<Oop>,
    ) -> Result<Operand, Outcome> {
        unsafe {
            let source = scope.keep(object(self.send(
                self.named(b"String")?,
                b"fromBytes:",
                &[Operand::bytes(c)],
            )?)?)?;
            let unit = scope.keep(object(self.send(
                self.named(b"TCC")?,
                b"compile:",
                &[Operand::object(source)],
            )?)?)?;
            if self.is_error(unit)? {
                return Ok(Operand::object(unit));
            }
            if let Some(implementation) = implementation {
                self.send(unit, b"initializeWith:", &[Operand::object(implementation)])
            } else {
                self.send(unit, b"initialize", &[])
            }
        }
    }
}
#[derive(Clone, Copy)]
enum Op {
    Emit,
    Compile,
    Eval,
    Define,
}
struct Entry {
    state: *const State,
    op: Op,
}
unsafe extern "C" fn operation(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let e = &*c.cast::<Entry>();
        let s = &*e.state;
        if s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        let a = borrowed_slice(a, n).ok_or_else(invalid)?;
        let count = if matches!(e.op, Op::Define) { 3 } else { 1 };
        if a.len() != count {
            return Err(invalid());
        }
        let scope = Scope {
            s,
            tokens: RefCell::new(Vec::new()),
        };
        for &v in a {
            scope.keep(object(v)?)?;
        }
        if matches!(e.op, Op::Define) {
            let selector = object(a[0])?;
            let b = s
                .send(s.id.receiver, b"symbolBytesOf:", &[a[0]])?
                .as_bytes()
                .ok_or_else(invalid)?;
            let name = borrowed_bytes(b.pointer, b.length)
                .ok_or_else(invalid)?
                .to_vec();
            if name.is_empty() {
                return Err(invalid());
            }
            let colons = name.iter().filter(|&&b| b == b':').count();
            let arity = if colons > 0 {
                colons
            } else if name[0].is_ascii_alphabetic() || name[0] == b'_' {
                0
            } else {
                1
            };
            if arity > 16 {
                return Err(invalid());
            }
            let c = emission::compile_named(arity as u32).map_err(|_| invalid())?;
            let method =
                scope.keep(object(s.native_method(&scope, &c, Some(object(a[2])?))?)?)?;
            if s.is_error(method)? {
                return Ok(Operand::object(method));
            }
            let behavior = scope.keep(object(s.send(
                s.id.receiver,
                b"ensurePrivateVtableFor:",
                &[a[1]],
            )?)?)?;
            s.send(
                behavior,
                b"addMethod:as:",
                &[Operand::object(method), Operand::object(selector)],
            )?;
            return Ok(a[1]);
        }
        let source = object(a[0])?;
        let bytes = s.bytes(source)?;
        let cst = scope.keep(object(s.send(s.named(b"Lang")?, b"parse:", &[a[0]])?)?)?;
        if s.is_error(cst)? {
            return Ok(Operand::object(cst));
        }
        let cst = s.project(&scope, cst, bytes.len(), 0, &mut 1_000_000)?;
        let program = match lower_program(&cst, &bytes) {
            Ok(p) => p,
            Err(e) => return s.error(&scope, format!("source lowering: {e:?}").as_bytes()),
        };
        let c = match emission::compile_program(program, &bytes) {
            Ok(c) => c,
            Err(e) => return s.error(&scope, format!("native emission: {e:?}").as_bytes()),
        };
        if matches!(e.op, Op::Emit) {
            return s.send(s.named(b"String")?, b"fromBytes:", &[Operand::bytes(&c)]);
        }
        let method = scope.keep(object(s.native_method(&scope, &c, None)?)?)?;
        if s.is_error(method)? {
            return Ok(Operand::object(method));
        }
        if matches!(e.op, Op::Compile) {
            return Ok(Operand::object(method));
        }
        let captures = scope.keep(object(s.send(s.named(b"Vector")?, b"new", &[])?)?)?;
        let block = scope.keep(object(s.send(
            s.named(b"Block")?,
            b"fromMethod:captures:",
            &[Operand::object(method), Operand::object(captures)],
        )?)?)?;
        let machine = scope.keep(object(s.send(s.named(b"Machine")?, b"new", &[])?)?)?;
        s.send(machine, b"runBlock:", &[Operand::object(block)])
    })())
}
unsafe extern "C" fn roots(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = &*c.cast::<State>();
        let Some([v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let v = v.as_entry().ok_or_else(invalid)?;
        for &o in &s.permanent {
            v.method.ok_or_else(invalid)?(v.context, s.ns, &Operand::object(o), 1).into_result()?;
        }
        Ok(Operand::unit())
    })())
}
unsafe fn construct(id: Protocol, ns: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let named =
            |n: &[u8]| object(id.send_named(ns, b"at:", &[Operand::object(id.intern_name(n)?)])?);
        let existing =
            id.send_named(ns, b"at:", &[Operand::object(id.intern_name(b"Codegen")?)])?;
        if existing.kind() == native::OBJECT {
            return Ok(existing);
        }
        let root = named(b"Roots")?;
        let mut tokens = Vec::new();
        let mut pin = |v: Oop| -> Result<Oop, Outcome> {
            tokens.push(
                id.send_named(root, b"retain:", &[Operand::object(v)])?
                    .as_word()
                    .ok_or_else(invalid)?,
            );
            Ok(v)
        };
        let vt = pin(object(id.send_named(
            named(b"ObjectVt")?,
            b"delegated",
            &[],
        )?)?)?;
        let reflect_vt = pin(object(id.send_named(
            named(b"ObjectVt")?,
            b"delegated",
            &[],
        )?)?)?;
        let receiver = pin(object(id.send_named(
            named(b"ObjectModel")?,
            b"allocate:bytes:",
            &[Operand::object(vt), Operand::word(0)],
        )?)?)?;
        let reflect = pin(object(id.send_named(
            named(b"ObjectModel")?,
            b"allocate:bytes:",
            &[Operand::object(reflect_vt), Operand::word(0)],
        )?)?)?;
        let s = Box::into_raw(Box::new(State {
            id,
            ns,
            roots: root,
            permanent: vec![ns, root, vt, reflect_vt, receiver, reflect],
            thread: std::thread::current().id(),
        }));
        for (target, n, op) in [
            (vt, b"emit:".as_slice(), Op::Emit),
            (vt, b"compile:", Op::Compile),
            (vt, b"eval:", Op::Eval),
            (reflect_vt, b"defineMethod:on:as:", Op::Define),
        ] {
            let c = Box::into_raw(Box::new(Entry { state: s, op }));
            id.send_named(
                target,
                b"addMethod",
                &[
                    Operand::object(id.intern_name(n)?),
                    Operand::entry(NativeEntry::new(operation, c.cast())),
                ],
            )?;
        }
        id.send_named(
            vt,
            b"addMethod",
            &[
                Operand::object(id.intern_name(b"rootsDo:")?),
                Operand::entry(NativeEntry::new(roots, s.cast())),
            ],
        )?;
        id.send_named(root, b"register:", &[Operand::object(receiver)])?;
        for (n, v) in [
            (b"Codegen".as_slice(), receiver),
            (b"RustCodegen", receiver),
            (b"CodegenVt", vt),
            (b"Reflect", reflect),
        ] {
            id.send_named(
                ns,
                b"at:put:",
                &[Operand::object(id.intern_name(n)?), Operand::object(v)],
            )?;
        }
        for t in tokens {
            id.send_named(root, b"release:", &[Operand::word(t)])?;
        }
        Ok(Operand::object(receiver))
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
