//! Range representation and operations. Dependencies are opaque Id receivers.
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as native, Method, NativeEntry, Operand, Outcome, Protocol},
};
use core::{
    cell::{Cell, RefCell},
    ffi::c_void,
    ptr,
};
use std::collections::HashMap;
struct State {
    id: Protocol,
    roots: Oop,
    model: Oop,
    behavior: Oop,
    prototype: Cell<Oop>,
    values: HashMap<&'static [u8], Oop>,
    names: HashMap<&'static [u8], Oop>,
    thread: std::thread::ThreadId,
}
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
fn object(v: Operand) -> Result<Oop, Outcome> {
    v.as_object().filter(|v| !v.is_null()).ok_or_else(invalid)
}
fn done(v: Result<Operand, Outcome>) -> Outcome {
    v.map_or_else(|e| e, Outcome::loaded)
}
unsafe fn state<'a>(c: *mut c_void) -> Result<&'a State, Outcome> {
    let s = unsafe { &*c.cast::<State>() };
    if s.thread != std::thread::current().id() {
        return Err(invalid());
    }
    Ok(s)
}
// Each synchronous operation owns its roots. No borrow spans a message send.
struct Scope<'a> {
    s: &'a State,
    tokens: RefCell<Vec<usize>>,
}
impl<'a> Scope<'a> {
    unsafe fn run<T>(
        s: &'a State,
        values: &[Oop],
        f: impl FnOnce(&Self) -> Result<T, Outcome>,
    ) -> Result<T, Outcome> {
        let scope = Self {
            s,
            tokens: RefCell::new(Vec::new()),
        };
        let result = (|| unsafe {
            for &v in values {
                scope.keep(v)?;
            }
            f(&scope)
        })();
        unsafe {
            scope.clear()?;
        }
        result
    }
    unsafe fn keep(&self, value: Oop) -> Result<Oop, Outcome> {
        if !value.is_null() {
            let token = unsafe {
                self.s
                    .send(self.s.roots, b"retain:", &[Operand::object(value)])
            }?
            .as_word()
            .filter(|n| *n != 0)
            .ok_or_else(invalid)?;
            self.tokens.borrow_mut().push(token);
        }
        Ok(value)
    }
    unsafe fn clear(&self) -> Result<(), Outcome> {
        let tokens = std::mem::take(&mut *self.tokens.borrow_mut());
        let mut error = None;
        for token in tokens {
            match unsafe {
                self.s
                    .send(self.s.roots, b"release:", &[Operand::word(token)])
            } {
                Ok(v) if v.as_word() == Some(1) => {}
                Ok(_) => error = Some(invalid()),
                Err(e) => error = Some(e),
            }
        }
        error.map_or(Ok(()), Err)
    }
    // Retain the next persistent result before discarding construction temporaries.
    unsafe fn reset(&self, value: Oop) -> Result<Oop, Outcome> {
        unsafe {
            let old = std::mem::take(&mut *self.tokens.borrow_mut());
            let result = self.keep(value);
            let new = std::mem::replace(&mut *self.tokens.borrow_mut(), old);
            let cleanup = self.clear();
            *self.tokens.borrow_mut() = new;
            cleanup?;
            result
        }
    }
}
impl State {
    unsafe fn send(
        &self,
        r: Oop,
        name: &'static [u8],
        args: &[Operand],
    ) -> Result<Operand, Outcome> {
        unsafe { self.id.send(r, self.names[name], args) }
    }
    fn value(&self, name: &'static [u8]) -> Oop {
        self.values[name]
    }
    fn nil(&self) -> Operand {
        Operand::object(self.value(b"nil"))
    }
    fn boolean(&self, yes: bool) -> Operand {
        Operand::object(self.value(if yes { b"true" } else { b"false" }))
    }
    unsafe fn allocate(
        &self,
        scope: &Scope<'_>,
        behavior: Oop,
        bytes: usize,
    ) -> Result<Oop, Outcome> {
        unsafe {
            let value = scope.keep(object(self.send(
                self.model,
                b"allocate:bytes:",
                &[Operand::object(behavior), Operand::word(bytes)],
            )?)?)?;
            if self
                .send(
                    self.id.receiver,
                    b"allocationSizeOf:",
                    &[Operand::object(value)],
                )?
                .as_word()
                != Some(bytes)
                || self
                    .send(self.id.receiver, b"behaviorOf:", &[Operand::object(value)])?
                    .as_object()
                    != Some(behavior)
            {
                return Err(invalid());
            }
            Ok(value)
        }
    }
    unsafe fn integer(&self, value: i64) -> Result<Operand, Outcome> {
        unsafe {
            self.send(
                self.value(b"Integer"),
                b"fromWord:",
                &[Operand::word(value as usize)],
            )
        }
    }
    unsafe fn index(&self, value: Oop) -> Result<Option<i64>, Outcome> {
        let v = unsafe {
            self.send(
                self.value(b"Integer"),
                b"wordOf:",
                &[Operand::object(value)],
            )
        }?;
        if v.kind() == native::UNIT {
            Ok(None)
        } else {
            Ok(Some(v.as_word().ok_or_else(invalid)? as i64))
        }
    }
    unsafe fn printable(&self, value: Oop) -> Result<Vec<u8>, Outcome> {
        unsafe {
            let printed = object(self.send(value, b"printString", &[])?)?;
            Scope::run(self, &[printed], |_| {
                let raw = self
                    .send(printed, b"bytes", &[])?
                    .as_bytes()
                    .ok_or_else(invalid)?;
                Ok(borrowed_bytes(raw.pointer, raw.length)
                    .ok_or_else(invalid)?
                    .to_vec())
            })
        }
    }
}
unsafe extern "C" fn roots(c: *mut c_void, _: Oop, a: *const Operand, count: usize) -> Outcome {
    done((|| unsafe {
        let s = state(c)?;
        let Some([visitor]) = borrowed_slice(a, count) else {
            return Err(invalid());
        };
        let visitor = object(*visitor)?;
        let mut values = vec![s.roots, s.model, s.behavior, s.prototype.get()];
        values.extend(s.values.values().copied());
        values.extend(s.names.values().copied());
        for value in values.into_iter().filter(|v| !v.is_null()) {
            s.id.send_named(visitor, b"visit:", &[Operand::object(value)])?;
        }
        Ok(Operand::unit())
    })())
}
macro_rules! entry {
    ($name:ident,$op:expr) => {
        unsafe extern "C" fn $name(
            c: *mut c_void,
            r: Oop,
            a: *const Operand,
            count: usize,
        ) -> Outcome {
            done((|| unsafe {
                let s = state(c)?;
                let args = borrowed_slice(a, count).ok_or_else(invalid)?;
                let mut values = vec![r];
                for arg in args {
                    values.push(object(*arg)?);
                }
                Scope::run(s, &values, |scope| execute(s, scope, r, &values[1..], $op))
            })())
        }
    };
}
const NAMES: &[&[u8]] = &[
    b"retain:",
    b"release:",
    b"allocate:bytes:",
    b"allocationSizeOf:",
    b"behaviorOf:",
    b"fromWord:",
    b"printString",
    b"bytes",
    b"fromBytes:",
    b"rootsDo:",
    b"register:",
    b"ownerOf:put:",
    b"addMethod",
    b"at:put:",
    b"wordOf:",
    b"value:",
    b"value:value:",
    b"new",
    b"add:",
    b",",
    b"trace:with:",
    b"referencesOf:do:",
    b"to:",
    b"lookup",
    b"Range",
    b"RangeVt",
];

unsafe fn construct(id: Protocol, namespace: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let named = |name: &[u8]| {
            object(id.send_named(namespace, b"at:", &[Operand::object(id.intern_name(name)?)])?)
        };
        let roots = named(b"Roots")?;
        let model = named(b"ObjectModel")?;
        let retain = id.intern_name(b"retain:")?;
        let release = id.intern_name(b"release:")?;
        let mut tokens = Vec::new();
        let mut keep = |v: Oop| -> Result<Oop, Outcome> {
            tokens.push(
                id.send(roots, retain, &[Operand::object(v)])?
                    .as_word()
                    .filter(|v| *v != 0)
                    .ok_or_else(invalid)?,
            );
            Ok(v)
        };
        let mut names = HashMap::new();
        for name in NAMES
            .iter()
            .copied()
            .chain(METHODS.iter().map(|(name, _)| *name))
        {
            names.insert(name, keep(id.intern_name(name)?)?);
        }
        let mut values = HashMap::new();
        for name in DEPENDENCIES {
            values.insert(*name, named(name)?);
        }
        let behavior = keep(object(id.send_named(
            named(b"CollectionVt")?,
            b"delegated",
            &[],
        )?)?)?;
        let p = Box::into_raw(Box::new(State {
            id,
            roots,
            model,
            behavior,
            prototype: Cell::new(ptr::null_mut()),
            values,
            names,
            thread: std::thread::current().id(),
        }));
        let s = &*p;
        for (name, method) in METHODS
            .iter()
            .copied()
            .chain([(b"rootsDo:".as_slice(), self::roots as Method)])
        {
            s.send(
                behavior,
                b"addMethod",
                &[
                    Operand::object(s.names[name]),
                    Operand::entry(NativeEntry::new(method, p.cast())),
                ],
            )?;
        }
        id.send_named(
            model,
            b"setLayoutOf:to:rest:",
            &[
                Operand::object(behavior),
                Operand::bytes(&[0, 0]),
                Operand::word(0),
            ],
        )?;
        Scope::run(s, &[], |scope| {
            let prototype = empty(s, scope)?;
            s.prototype.set(prototype);
            s.send(roots, b"register:", &[Operand::object(prototype)])?;
            s.send(
                id.receiver,
                b"ownerOf:put:",
                &[Operand::object(behavior), Operand::object(prototype)],
            )?;
            for (name, value) in [(b"Range".as_slice(), prototype), (b"RangeVt", behavior)] {
                s.send(
                    namespace,
                    b"at:put:",
                    &[Operand::object(s.names[name]), Operand::object(value)],
                )?;
            }

            s.send(
                behavior,
                b"addMethod",
                &[
                    Operand::object(s.names[b"referencesOf:do:".as_slice()]),
                    Operand::entry(NativeEntry::new(references, p.cast())),
                ],
            )?;
            s.send(
                s.value(b"GC"),
                b"trace:with:",
                &[Operand::object(behavior), Operand::object(prototype)],
            )?;
            if s.send(
                s.value(b"IntegerVt"),
                b"lookup",
                &[Operand::object(s.names[b"to:".as_slice()])],
            )?
            .kind()
                == native::UNIT
            {
                s.send(
                    s.value(b"IntegerVt"),
                    b"addMethod",
                    &[
                        Operand::object(s.names[b"to:".as_slice()]),
                        Operand::entry(NativeEntry::new(to, p.cast())),
                    ],
                )?;
            }

            Ok(())
        })?;
        for token in tokens {
            id.send(roots, release, &[Operand::word(token)])?;
        }
        Ok(Operand::object(s.prototype.get()))
    }
}
unsafe extern "C" fn initialize(id: *const Protocol, a: *const Operand, count: usize) -> Outcome {
    done((|| unsafe {
        if !native::valid_protocol(id) {
            return Err(invalid());
        }
        let Some([namespace]) = borrowed_slice(a, count) else {
            return Err(invalid());
        };
        let namespace = object(*namespace)?;
        let id = *id;
        let current = id.send_named(
            namespace,
            b"at:",
            &[Operand::object(id.intern_name(b"Range")?)],
        )?;
        if current.kind() == native::OBJECT {
            return Ok(current);
        }
        construct(id, namespace)
    })())
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
const DEPENDENCIES: &[&[u8]] = &[
    b"Integer",
    b"String",
    b"nil",
    b"true",
    b"false",
    b"Vector",
    b"VectorVt",
    b"IntegerVt",
    b"GC",
];
#[derive(Clone, Copy)]
#[repr(C)]
struct Range {
    start: i64,
    end: i64,
}
unsafe fn payload(s: &State, r: Oop) -> Result<Range, Outcome> {
    unsafe {
        if s.send(s.id.receiver, b"allocationSizeOf:", &[Operand::object(r)])?
            .as_word()
            != Some(size_of::<Range>())
        {
            return Err(invalid());
        }
        Ok(r.cast::<Range>().read())
    }
}
unsafe fn make(s: &State, scope: &Scope<'_>, start: i64, end: i64) -> Result<Oop, Outcome> {
    unsafe {
        let value = s.allocate(scope, s.behavior, size_of::<Range>())?;
        value.cast::<Range>().write(Range { start, end });
        Ok(value)
    }
}
unsafe fn empty(s: &State, scope: &Scope<'_>) -> Result<Oop, Outcome> {
    unsafe { make(s, scope, 0, 0) }
}
unsafe extern "C" fn references(
    c: *mut c_void,
    _: Oop,
    a: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        state(c)?;
        let Some([value, visitor]) = borrowed_slice(a, count) else {
            return Err(invalid());
        };
        object(*value)?;
        object(*visitor)?;
        Ok(Operand::unit())
    })())
}
unsafe fn execute(
    s: &State,
    scope: &Scope<'_>,
    r: Oop,
    args: &[Oop],
    op: usize,
) -> Result<Operand, Outcome> {
    unsafe {
        let arity = match op {
            0 | 2 | 3 | 4 | 8 => 1,
            5 => 2,
            _ => 0,
        };
        if args.len() != arity {
            return Ok(s.nil());
        }
        if op == 0 {
            let (Some(start), Some(end)) = (s.index(r)?, s.index(args[0])?) else {
                return Ok(s.nil());
            };
            return Ok(Operand::object(make(s, scope, start, end)?));
        }
        let v = payload(s, r)?;
        let (start, end) = (i128::from(v.start), i128::from(v.end));
        match op {
            1 => {
                let count = if end < start { 0 } else { end - start + 1 };
                if count > i128::from(i64::MAX) {
                    Ok(s.nil())
                } else {
                    s.integer(count as i64)
                }
            }
            2 => {
                let val = s.index(args[0])?;
                Ok(s.boolean(val.is_some_and(|v| v >= start as i64 && v <= end as i64)))
            }
            3 => {
                let Some(index) = s.index(args[0])? else {
                    return Ok(s.nil());
                };
                let val = start + i128::from(index);
                if index < 0 || val > end {
                    Ok(s.nil())
                } else {
                    s.integer(val as i64)
                }
            }
            4 | 5 => Scope::run(s, if op == 5 { &args[..1] } else { &[] }, |work| {
                let mut acc = if op == 5 { args[0] } else { r };
                for value in start..=end {
                    let value = work.keep(object(s.integer(value as i64)?)?)?;
                    if op == 4 {
                        s.send(args[0], b"value:", &[Operand::object(value)])?;
                        work.clear()?;
                    } else {
                        let next = s.send(
                            args[1],
                            b"value:value:",
                            &[Operand::object(acc), Operand::object(value)],
                        )?;
                        let Some(next) = next.as_object().filter(|v| !v.is_null()) else {
                            return Ok(s.nil());
                        };
                        acc = work.reset(next)?;
                    }
                }
                Ok(Operand::object(acc))
            }),
            6 => {
                let mut bytes = b"#[".to_vec();
                for value in start..=end {
                    if value != start {
                        bytes.push(b' ');
                    }
                    let value = object(s.integer(value as i64)?)?;
                    bytes.extend_from_slice(&Scope::run(s, &[value], |_| s.printable(value))?);
                }
                bytes.push(b']');
                s.send(s.value(b"String"), b"fromBytes:", &[Operand::bytes(&bytes)])
            }
            7 | 8 => {
                if op == 8
                    && s.send(s.id.receiver, b"behaviorOf:", &[Operand::object(args[0])])?
                        .as_object()
                        != Some(s.value(b"VectorVt"))
                {
                    return Ok(s.nil());
                }
                Scope::run(s, &[], |work| {
                    let mut result =
                        work.keep(object(s.send(s.value(b"Vector"), b"new", &[])?)?)?;
                    let mut value = if op == 7 { end } else { start };
                    while value >= start && value <= end {
                        let item = work.keep(object(s.integer(value as i64)?)?)?;
                        result = work.reset(object(s.send(
                            result,
                            b"add:",
                            &[Operand::object(item)],
                        )?)?)?;
                        value += if op == 7 { -1 } else { 1 };
                    }
                    if op == 8 {
                        s.send(result, b",", &[Operand::object(args[0])])
                    } else {
                        Ok(Operand::object(result))
                    }
                })
            }
            _ => Err(invalid()),
        }
    }
}
entry!(to, 0);
entry!(size, 1);
entry!(includes, 2);
entry!(at, 3);
entry!(do_each, 4);
entry!(inject, 5);
entry!(print, 6);
entry!(reversed, 7);
entry!(comma, 8);
const METHODS: &[(&[u8], Method)] = &[
    (b"size", size),
    (b"includes:", includes),
    (b"at:", at),
    (b"do:", do_each),
    (b"inject:into:", inject),
    (b"printString", print),
    (b"reversed", reversed),
    (b",", comma),
];
