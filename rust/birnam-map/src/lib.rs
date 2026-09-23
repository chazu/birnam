//! Map representation and operations. Dependencies are opaque Id receivers.
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
    node: Oop,
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
    unsafe fn same(&self, a: Oop, b: Oop) -> Result<bool, Outcome> {
        unsafe {
            Ok(a == b
                || self
                    .send(a, b"collectionEquals:", &[Operand::object(b)])?
                    .as_object()
                    == Some(self.value(b"true")))
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
        let visitor = visitor
            .as_entry()
            .filter(|v| v.method.is_some())
            .ok_or_else(invalid)?;
        let mut values = vec![s.roots, s.model, s.behavior, s.node, s.prototype.get()];
        values.extend(s.values.values().copied());
        values.extend(s.names.values().copied());
        for value in values.into_iter().filter(|v| !v.is_null()) {
            visitor.method.unwrap()(
                visitor.context,
                s.prototype.get(),
                &Operand::object(value),
                1,
            )
            .into_result()?;
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
    b"collectionEquals:",
    b"collectionHash",
    b"value",
    b"value:value:",
    b"new",
    b"add:",
    b"do:",
    b"inject:into:",
    b"Map",
    b"MapVt",
    b"MapNode",
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
        let node = keep(object(id.send_named(
            named(b"ObjectVt")?,
            b"delegated",
            &[],
        )?)?)?;
        let p = Box::into_raw(Box::new(State {
            id,
            roots,
            model,
            behavior,
            node,
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
                Operand::bytes(&[1, 0]),
                Operand::word(0),
            ],
        )?;
        id.send_named(
            model,
            b"setLayoutOf:to:rest:",
            &[
                Operand::object(node),
                Operand::bytes(&[0, 0]),
                Operand::word(1),
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
            for (name, value) in [(b"Map".as_slice(), prototype), (b"MapVt", behavior)] {
                s.send(
                    namespace,
                    b"at:put:",
                    &[Operand::object(s.names[name]), Operand::object(value)],
                )?;
            }
            s.send(
                namespace,
                b"at:put:",
                &[
                    Operand::object(s.names[b"MapNode".as_slice()]),
                    Operand::object(node),
                ],
            )?;
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
            &[Operand::object(id.intern_name(b"Map")?)],
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
const DEPENDENCIES: &[&[u8]] = &[b"Integer", b"String", b"nil", b"true", b"false", b"Vector"];
const BITS: usize = 5;
const MASK: u32 = 31;
const BITMAP: u32 = 0;
const COLLISION: u32 = 1;
#[derive(Clone, Copy)]
#[repr(C)]
struct Header {
    tag: u32,
    n: u32,
    hash: u32,
    pad: u32,
}
#[derive(Clone, Copy)]
#[repr(C)]
struct Map {
    root: Oop,
    count: usize,
}
unsafe fn payload(s: &State, value: Oop) -> Result<Map, Outcome> {
    unsafe {
        if s.send(
            s.id.receiver,
            b"allocationSizeOf:",
            &[Operand::object(value)],
        )?
        .as_word()
            != Some(size_of::<Map>())
        {
            return Err(invalid());
        }
        Ok(value.cast::<Map>().read())
    }
}
unsafe fn header(s: &State, node: Oop) -> Result<Header, Outcome> {
    unsafe {
        let bytes = s
            .send(
                s.id.receiver,
                b"allocationSizeOf:",
                &[Operand::object(node)],
            )?
            .as_word()
            .ok_or_else(invalid)?;
        if bytes < size_of::<Header>() {
            return Err(invalid());
        }
        let h = node.cast::<Header>().read();
        if h.tag > COLLISION || size_of::<Header>() + slots(h) * size_of::<Oop>() != bytes {
            return Err(invalid());
        }
        Ok(h)
    }
}
fn slots(h: Header) -> usize {
    2 * if h.tag == BITMAP {
        h.n.count_ones() as usize
    } else {
        h.n as usize
    }
}
unsafe fn slot(node: Oop, i: usize) -> Oop {
    unsafe {
        node.cast::<u8>()
            .add(size_of::<Header>())
            .cast::<Oop>()
            .add(i)
            .read()
    }
}
unsafe fn put_slot(node: Oop, i: usize, value: Oop) {
    unsafe {
        node.cast::<u8>()
            .add(size_of::<Header>())
            .cast::<Oop>()
            .add(i)
            .write(value);
    }
}
fn chunk(hash: u32, shift: usize) -> u32 {
    (hash >> shift.min(31)) & MASK
}
fn bit(hash: u32, shift: usize) -> u32 {
    1 << chunk(hash, shift)
}
fn sparse(bitmap: u32, bit: u32) -> usize {
    (bitmap & bit.wrapping_sub(1)).count_ones() as usize
}
unsafe fn key_hash(s: &State, key: Oop) -> Result<u32, Outcome> {
    unsafe { s.send(key, b"collectionHash", &[]) }?
        .as_word()
        .map(|v| v as u32)
        .ok_or_else(invalid)
}
unsafe fn new_node(
    s: &State,
    scope: &Scope<'_>,
    tag: u32,
    n: u32,
    hash: u32,
) -> Result<Oop, Outcome> {
    unsafe {
        let h = Header {
            tag,
            n,
            hash,
            pad: 0,
        };
        let node = s.allocate(
            scope,
            s.node,
            size_of::<Header>() + slots(h) * size_of::<Oop>(),
        )?;
        node.cast::<Header>().write(h);
        Ok(node)
    }
}
unsafe fn copy_node(s: &State, scope: &Scope<'_>, source: Oop) -> Result<Oop, Outcome> {
    unsafe {
        let h = header(s, source)?;
        let node = new_node(s, scope, h.tag, h.n, h.hash)?;
        for i in 0..slots(h) {
            put_slot(node, i, slot(source, i));
        }
        Ok(node)
    }
}
unsafe fn make(s: &State, scope: &Scope<'_>, v: Map) -> Result<Oop, Outcome> {
    unsafe {
        let value = s.allocate(scope, s.behavior, size_of::<Map>())?;
        value.cast::<Map>().write(v);
        Ok(value)
    }
}
unsafe fn empty(s: &State, scope: &Scope<'_>) -> Result<Oop, Outcome> {
    unsafe {
        make(
            s,
            scope,
            Map {
                root: ptr::null_mut(),
                count: 0,
            },
        )
    }
}
unsafe fn find(s: &State, node: Oop, shift: usize, hash: u32, key: Oop) -> Result<Oop, Outcome> {
    unsafe {
        if node.is_null() {
            return Ok(ptr::null_mut());
        }
        let h = header(s, node)?;
        if h.tag == BITMAP {
            let b = bit(hash, shift);
            if h.n & b == 0 {
                return Ok(ptr::null_mut());
            }
            let i = 2 * sparse(h.n, b);
            let stored = slot(node, i);
            if stored.is_null() {
                return find(s, slot(node, i + 1), shift + BITS, hash, key);
            }
            if s.same(stored, key)? {
                return Ok(slot(node, i + 1));
            }
        } else if hash == h.hash {
            for i in (0..slots(h)).step_by(2) {
                if s.same(slot(node, i), key)? {
                    return Ok(slot(node, i + 1));
                }
            }
        }
        Ok(ptr::null_mut())
    }
}
// Hash/key/value triples are local snapshots, never foreign representations.
unsafe fn merge(
    s: &State,
    scope: &Scope<'_>,
    shift: usize,
    a: (u32, Oop, Oop),
    b: (u32, Oop, Oop),
) -> Result<Oop, Outcome> {
    unsafe {
        for value in [a.1, a.2, b.1, b.2] {
            scope.keep(value)?;
        }
        if a.0 == b.0 {
            let node = new_node(s, scope, COLLISION, 2, a.0)?;
            for (i, v) in [a.1, a.2, b.1, b.2].into_iter().enumerate() {
                put_slot(node, i, v);
            }
            return Ok(node);
        }
        let (ac, bc) = (chunk(a.0, shift), chunk(b.0, shift));
        if ac == bc {
            let child = merge(s, scope, shift + BITS, a, b)?;
            let node = new_node(s, scope, BITMAP, 1 << ac, 0)?;
            put_slot(node, 1, child);
            return Ok(node);
        }
        let node = new_node(s, scope, BITMAP, (1 << ac) | (1 << bc), 0)?;
        let pairs = if ac < bc {
            [a.1, a.2, b.1, b.2]
        } else {
            [b.1, b.2, a.1, a.2]
        };
        for (i, v) in pairs.into_iter().enumerate() {
            put_slot(node, i, v);
        }
        Ok(node)
    }
}
#[allow(clippy::too_many_arguments)]
unsafe fn assoc(
    s: &State,
    scope: &Scope<'_>,
    node: Oop,
    shift: usize,
    hash: u32,
    key: Oop,
    value: Oop,
    added: &mut bool,
) -> Result<Oop, Outcome> {
    unsafe {
        if node.is_null() {
            *added = true;
            let result = new_node(s, scope, BITMAP, bit(hash, shift), 0)?;
            put_slot(result, 0, key);
            put_slot(result, 1, value);
            return Ok(result);
        }
        let h = header(s, node)?;
        if h.tag == BITMAP {
            let b = bit(hash, shift);
            let i = 2 * sparse(h.n, b);
            if h.n & b != 0 {
                let stored = slot(node, i);
                if stored.is_null() {
                    let child = slot(node, i + 1);
                    let replacement =
                        assoc(s, scope, child, shift + BITS, hash, key, value, added)?;
                    if child == replacement {
                        return Ok(node);
                    }
                    let result = copy_node(s, scope, node)?;
                    put_slot(result, i + 1, replacement);
                    return Ok(result);
                }
                if s.same(stored, key)? {
                    if slot(node, i + 1) == value {
                        return Ok(node);
                    }
                    let result = copy_node(s, scope, node)?;
                    put_slot(result, i + 1, value);
                    return Ok(result);
                }
                *added = true;
                let child = merge(
                    s,
                    scope,
                    shift + BITS,
                    (key_hash(s, stored)?, stored, slot(node, i + 1)),
                    (hash, key, value),
                )?;
                let result = copy_node(s, scope, node)?;
                put_slot(result, i, ptr::null_mut());
                put_slot(result, i + 1, child);
                return Ok(result);
            }
            *added = true;
            let result = new_node(s, scope, BITMAP, h.n | b, 0)?;
            for old in 0..slots(h) {
                put_slot(result, if old < i { old } else { old + 2 }, slot(node, old));
            }
            put_slot(result, i, key);
            put_slot(result, i + 1, value);
            return Ok(result);
        }
        if hash == h.hash {
            for i in (0..slots(h)).step_by(2) {
                if s.same(slot(node, i), key)? {
                    if slot(node, i + 1) == value {
                        return Ok(node);
                    }
                    let result = copy_node(s, scope, node)?;
                    put_slot(result, i + 1, value);
                    return Ok(result);
                }
            }
            *added = true;
            let result = new_node(s, scope, COLLISION, h.n + 1, h.hash)?;
            for i in 0..slots(h) {
                put_slot(result, i, slot(node, i));
            }
            put_slot(result, slots(h), key);
            put_slot(result, slots(h) + 1, value);
            return Ok(result);
        }
        let wrapper = new_node(s, scope, BITMAP, bit(h.hash, shift), 0)?;
        put_slot(wrapper, 1, node);
        assoc(s, scope, wrapper, shift, hash, key, value, added)
    }
}
unsafe fn entries(s: &State, node: Oop, values: &mut Vec<(Oop, Oop)>) -> Result<(), Outcome> {
    unsafe {
        if node.is_null() {
            return Ok(());
        }
        let h = header(s, node)?;
        for i in (0..slots(h)).step_by(2) {
            let key = slot(node, i);
            let value = slot(node, i + 1);
            if h.tag == BITMAP && key.is_null() {
                entries(s, value, values)?;
            } else {
                values.push((key, value));
            }
        }
        Ok(())
    }
}
unsafe fn vector(s: &State, values: impl IntoIterator<Item = Oop>) -> Result<Oop, Outcome> {
    unsafe {
        Scope::run(s, &[], |work| {
            let mut result = work.keep(object(s.send(s.value(b"Vector"), b"new", &[])?)?)?;
            for value in values {
                result = work.reset(object(s.send(
                    result,
                    b"add:",
                    &[Operand::object(value)],
                )?)?)?;
            }
            Ok(result)
        })
    }
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
            1 | 3 | 8 | 10 => 1,
            2 | 4 | 9 => 2,
            _ => 0,
        };
        if args.len() != arity {
            return Ok(s.nil());
        }
        if op == 0 {
            return Ok(Operand::object(empty(s, scope)?));
        }
        let v = payload(s, r)?;
        scope.keep(v.root)?;
        if matches!(op, 1..=4) {
            let hash = key_hash(s, args[0])?;
            if op == 2 {
                let mut added = false;
                let root = assoc(s, scope, v.root, 0, hash, args[0], args[1], &mut added)?;
                return Ok(Operand::object(if root == v.root {
                    r
                } else {
                    make(
                        s,
                        scope,
                        Map {
                            root,
                            count: v.count + usize::from(added),
                        },
                    )?
                }));
            }
            let found = find(s, v.root, 0, hash, args[0])?;
            if op == 3 {
                return Ok(s.boolean(!found.is_null()));
            }
            if !found.is_null() {
                return Ok(Operand::object(found));
            }
            return if op == 4 {
                s.send(args[1], b"value", &[])
            } else {
                Ok(s.nil())
            };
        }
        if op == 5 {
            return s.integer(v.count as i64);
        }
        let mut pairs = Vec::with_capacity(v.count);
        entries(s, v.root, &mut pairs)?;
        for &(key, value) in &pairs {
            scope.keep(key)?;
            scope.keep(value)?;
        }
        match op {
            6 | 7 => Ok(Operand::object(vector(
                s,
                pairs.into_iter().map(|(k, v)| if op == 6 { k } else { v }),
            )?)),
            8 | 9 => {
                let values = scope.keep(vector(s, pairs.into_iter().map(|(_, v)| v))?)?;
                let operands: Vec<_> = args.iter().map(|v| Operand::object(*v)).collect();
                let result = s.send(
                    values,
                    if op == 8 { b"do:" } else { b"inject:into:" },
                    &operands,
                )?;
                Ok(if op == 8 { Operand::object(r) } else { result })
            }
            10 => {
                for (k, v) in pairs {
                    s.send(
                        args[0],
                        b"value:value:",
                        &[Operand::object(k), Operand::object(v)],
                    )?;
                }
                Ok(Operand::object(r))
            }
            11 => {
                let mut bytes = b"{".to_vec();
                for (i, (k, v)) in pairs.into_iter().enumerate() {
                    if i > 0 {
                        bytes.extend_from_slice(b", ");
                    }
                    bytes.extend_from_slice(&s.printable(k)?);
                    bytes.extend_from_slice(b" -> ");
                    bytes.extend_from_slice(&s.printable(v)?);
                }
                bytes.push(b'}');
                s.send(s.value(b"String"), b"fromBytes:", &[Operand::bytes(&bytes)])
            }
            _ => Err(invalid()),
        }
    }
}
entry!(new, 0);
entry!(at, 1);
entry!(put, 2);
entry!(includes, 3);
entry!(absent, 4);
entry!(size, 5);
entry!(keys, 6);
entry!(values, 7);
entry!(do_each, 8);
entry!(inject, 9);
entry!(pairs, 10);
entry!(print, 11);
const METHODS: &[(&[u8], Method)] = &[
    (b"new", new),
    (b"clone", new),
    (b"at:", at),
    (b"at:put:", put),
    (b"includesKey:", includes),
    (b"at:ifAbsent:", absent),
    (b"size", size),
    (b"keys", keys),
    (b"values", values),
    (b"do:", do_each),
    (b"inject:into:", inject),
    (b"keysAndValuesDo:", pairs),
    (b"printString", print),
];
