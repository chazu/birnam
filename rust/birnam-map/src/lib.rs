//! Persistent Map storage belongs to Map; dependencies are Oop receivers.
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
    sync::atomic::{AtomicPtr, Ordering},
};
struct State {
    id: Protocol,
    namespace: Oop,
    roots: Oop,
    model: Oop,
    behavior: Oop,
    node: Oop,
    prototype: Oop,
    values: HashMap<&'static [u8], Oop>,
    names: HashMap<&'static [u8], Oop>,
    scoped: RefCell<Vec<Oop>>,
    ready: Cell<bool>,
}
static INSTANCE: AtomicPtr<State> = AtomicPtr::new(ptr::null_mut());
static API: AtomicPtr<Protocol> = AtomicPtr::new(ptr::null_mut());
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
fn done(v: Result<Oop, Oop>) -> Oop {
    v.unwrap_or_else(abi::fault_reference)
}
unsafe fn state<'a>(c: Oop) -> Result<&'a State, Oop> {
    let p = INSTANCE.load(Ordering::Acquire);
    if p.is_null() {
        return Err(invalid());
    }
    let s = unsafe { &*p };
    if c == s.prototype {
        Ok(s)
    } else {
        Err(invalid())
    }
}
struct Scope<'a> {
    s: &'a State,
    start: usize,
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        self.s.scoped.borrow_mut().truncate(self.start);
    }
}
impl<'a> Scope<'a> {
    unsafe fn run<T>(
        s: &'a State,
        values: &[Oop],
        f: impl FnOnce(&Self) -> Result<T, Oop>,
    ) -> Result<T, Oop> {
        let scope = Self {
            s,
            start: s.scoped.borrow().len(),
        };
        s.scoped.borrow_mut().extend_from_slice(values);
        f(&scope)
    }
    fn keep(&self, v: Oop) -> Oop {
        self.s.scoped.borrow_mut().push(v);
        v
    }
    fn reset(&self, v: Oop) -> Oop {
        let mut values = self.s.scoped.borrow_mut();
        values.truncate(self.start);
        values.push(v);
        v
    }
}
impl State {
    unsafe fn send(&self, r: Oop, name: &[u8], a: &[Oop]) -> Result<Oop, Oop> {
        unsafe {
            self.id
                .send(r, *self.names.get(name).ok_or_else(invalid)?, a)
        }
    }
    fn value(&self, name: &[u8]) -> Oop {
        self.values[name]
    }
    fn nil(&self) -> Oop {
        self.value(b"nil")
    }
    fn boolean(&self, yes: bool) -> Oop {
        self.value(if yes { b"true" } else { b"false" })
    }
    unsafe fn allocate(&self, scope: &Scope<'_>, behavior: Oop, bytes: usize) -> Result<Oop, Oop> {
        unsafe {
            let value = scope.keep(self.send(
                self.model,
                b"allocate:bytes:",
                &[behavior, abi::size(bytes).ok_or_else(invalid)?],
            )?);
            if self.send(self.id.root, b"allocationSizeOf:", &[value])? != abi::size(bytes).unwrap()
                || self.send(self.id.root, b"behaviorOf:", &[value])? != behavior
            {
                return Err(invalid());
            }
            Ok(value)
        }
    }
    unsafe fn word(&self, bits: u64) -> Result<Oop, Oop> {
        unsafe {
            if let Some(v) = abi::size(bits as usize) {
                return Ok(v);
            }
            self.send(
                self.value(b"Word"),
                b"fromUnsignedLow:high:",
                &[
                    abi::size(bits as u32 as usize).unwrap(),
                    abi::size((bits >> 32) as usize).unwrap(),
                ],
            )
        }
    }
    unsafe fn word_bits(&self, v: Oop) -> Result<u64, Oop> {
        unsafe {
            if let Some(v) = abi::as_size(v) {
                return Ok(v as u64);
            }
            let lo = abi::as_size(self.send(v, b"low32", &[])?)
                .filter(|v| *v <= u32::MAX as usize)
                .ok_or_else(invalid)?;
            let hi = abi::as_size(self.send(v, b"high32", &[])?)
                .filter(|v| *v <= u32::MAX as usize)
                .ok_or_else(invalid)?;
            Ok((lo as u64) | ((hi as u64) << 32))
        }
    }
    unsafe fn integer(&self, v: i64) -> Result<Oop, Oop> {
        unsafe {
            self.send(
                self.value(b"Integer"),
                b"fromWord:",
                &[self.word(v as u64)?],
            )
        }
    }
    unsafe fn same(&self, a: Oop, b: Oop) -> Result<bool, Oop> {
        unsafe { Ok(a == b || self.send(a, b"collectionEquals:", &[b])? == self.value(b"true")) }
    }
}
unsafe extern "C" fn roots(_: Oop, c: Oop, _: Oop, a: *const Oop, count: usize) -> Oop {
    done((|| unsafe {
        let s = state(c)?;
        let [visitor] = borrowed_slice(a, count).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let mut values = vec![
            s.namespace,
            s.roots,
            s.model,
            s.behavior,
            s.node,
            s.prototype,
        ];
        values.extend(s.values.values().copied());
        values.extend(s.names.values().copied());
        values.extend(s.scoped.borrow().iter().copied());
        for v in values {
            s.send(*visitor, b"visit:", &[v])?;
        }
        Ok(ptr::null_mut())
    })())
}
macro_rules! entry {
    ($name:ident,$op:expr) => {
        unsafe extern "C" fn $name(_: Oop, c: Oop, r: Oop, a: *const Oop, count: usize) -> Oop {
            done((|| unsafe {
                let s = state(c)?;
                let a = borrowed_slice(a, count).ok_or_else(invalid)?;
                if a.iter().any(|v| v.is_null()) {
                    return Err(invalid());
                }
                Scope::run(s, &[], |scope| execute(s, scope, r, a, $op))
            })())
        }
    };
}
const DEPENDENCIES: &[&[u8]] = &[
    b"Integer", b"String", b"Word", b"Bytes", b"Vector", b"GC", b"nil", b"true", b"false",
];
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
unsafe fn payload(s: &State, value: Oop) -> Result<Map, Oop> {
    unsafe {
        if s.send(s.id.root, b"allocationSizeOf:", &[value])?
            != abi::size(size_of::<Map>()).unwrap()
        {
            return Err(invalid());
        }
        Ok(value.cast::<Map>().read())
    }
}
unsafe fn header(s: &State, node: Oop) -> Result<Header, Oop> {
    unsafe {
        let bytes =
            abi::as_size(s.send(s.id.root, b"allocationSizeOf:", &[node])?).ok_or_else(invalid)?;
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
unsafe fn key_hash(s: &State, key: Oop) -> Result<u32, Oop> {
    unsafe {
        s.word_bits(s.send(key, b"collectionHash", &[])?)
            .map(|v| v as u32)
    }
}
unsafe fn new_node(s: &State, scope: &Scope<'_>, tag: u32, n: u32, hash: u32) -> Result<Oop, Oop> {
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
unsafe fn copy_node(s: &State, scope: &Scope<'_>, source: Oop) -> Result<Oop, Oop> {
    unsafe {
        let h = header(s, source)?;
        let node = new_node(s, scope, h.tag, h.n, h.hash)?;
        for i in 0..slots(h) {
            put_slot(node, i, slot(source, i));
        }
        Ok(node)
    }
}
unsafe fn make(s: &State, scope: &Scope<'_>, v: Map) -> Result<Oop, Oop> {
    unsafe {
        let value = s.allocate(scope, s.behavior, size_of::<Map>())?;
        value.cast::<Map>().write(v);
        Ok(value)
    }
}
unsafe fn empty(s: &State, scope: &Scope<'_>) -> Result<Oop, Oop> {
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
unsafe fn find(s: &State, node: Oop, shift: usize, hash: u32, key: Oop) -> Result<Oop, Oop> {
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
) -> Result<Oop, Oop> {
    unsafe {
        for value in [a.1, a.2, b.1, b.2] {
            scope.keep(value);
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
) -> Result<Oop, Oop> {
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
unsafe fn entries(s: &State, node: Oop, values: &mut Vec<(Oop, Oop)>) -> Result<(), Oop> {
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
unsafe fn vector(s: &State, values: impl IntoIterator<Item = Oop>) -> Result<Oop, Oop> {
    unsafe {
        Scope::run(s, &[], |work| {
            let mut result = work.keep(object(s.send(s.value(b"Vector"), b"new", &[])?)?);
            for value in values {
                result = work.reset(object(s.send(result, b"add:", &[value])?)?);
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
) -> Result<Oop, Oop> {
    unsafe {
        let arity = match op {
            1 | 3 | 8 | 10 | 12 => 1,
            2 | 4 | 9 => 2,
            _ => 0,
        };
        if args.len() != arity {
            return Ok(s.nil());
        }
        if op == 0 {
            return Ok(empty(s, scope)?);
        }
        if op == 12 {
            s.send(s.values[b"GC".as_slice()], b"prepareAllocation", &[])?;
            let mut behavior = object(s.send(s.id.root, b"behaviorOf:", &[args[0]])?)?;
            let mut seen = std::collections::HashSet::new();
            while behavior != s.behavior {
                if !seen.insert(behavior) {
                    return Err(invalid());
                }
                scope.keep(behavior);
                behavior = object(s.send(s.id.root, b"parentOf:", &[behavior])?)?;
            }
            let target = args[0];
            let old = payload(s, target)?;
            scope.keep(old.root);
            let mut pairs = Vec::new();
            entries(s, old.root, &mut pairs)?;
            for &(key, value) in &pairs {
                scope.keep(key);
                scope.keep(value);
            }
            let mut root = ptr::null_mut();
            let mut count = 0;
            for (key, value) in pairs {
                let hash = key_hash(s, key)?;
                let mut added = false;
                root = assoc(s, scope, root, 0, hash, key, value, &mut added)?;
                scope.keep(root);
                count += usize::from(added);
            }
            target.cast::<Map>().write(Map { root, count });
            return Ok(target);
        }
        let v = payload(s, r)?;
        scope.keep(v.root);
        if matches!(op, 1..=4) {
            let hash = key_hash(s, args[0])?;
            if op == 2 {
                let mut added = false;
                let root = assoc(s, scope, v.root, 0, hash, args[0], args[1], &mut added)?;
                return Ok(if root == v.root {
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
                });
            }
            let found = find(s, v.root, 0, hash, args[0])?;
            if op == 3 {
                return Ok(s.boolean(!found.is_null()));
            }
            if !found.is_null() {
                return Ok(found);
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
            scope.keep(key);
            scope.keep(value);
        }
        match op {
            6 | 7 => Ok(vector(
                s,
                pairs.into_iter().map(|(k, v)| if op == 6 { k } else { v }),
            )?),
            8 | 9 => {
                let values = scope.keep(vector(s, pairs.into_iter().map(|(_, v)| v))?);

                let result =
                    s.send(values, if op == 8 { b"do:" } else { b"inject:into:" }, args)?;
                Ok(if op == 8 { r } else { result })
            }
            10 => {
                for (k, v) in pairs {
                    s.send(args[0], b"value:value:", &[k, v])?;
                }
                Ok(r)
            }
            11 => {
                let output = scope.keep(s.send(
                    s.value(b"Bytes"),
                    b"fromByteValues",
                    &[abi::size(123).unwrap()],
                )?);
                for (i, (key, value)) in pairs.into_iter().enumerate() {
                    if i > 0 {
                        s.send(
                            output,
                            b"appendByteValues",
                            &[abi::size(44).unwrap(), abi::size(32).unwrap()],
                        )?;
                    }
                    for (j, value) in [key, value].into_iter().enumerate() {
                        if j > 0 {
                            s.send(
                                output,
                                b"appendByteValues",
                                &[32, 45, 62, 32].map(|v| abi::size(v).unwrap()),
                            )?;
                        }
                        Scope::run(s, &[], |work| {
                            let printed = work.keep(s.send(value, b"printString", &[])?);
                            let bytes = s.send(printed, b"bytes", &[])?;
                            s.send(output, b"appendBytes:", &[bytes])
                        })?;
                    }
                }
                s.send(output, b"appendByteValues", &[abi::size(125).unwrap()])?;
                s.send(s.value(b"String"), b"fromBytes:", &[output])
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
entry!(rehash_image, 12);
entry!(values, 7);
entry!(do_each, 8);
entry!(inject, 9);
entry!(pairs, 10);
entry!(print, 11);
macro_rules! literals{($($n:literal),*$(,)?)=>{const NAMES:&[&[u8]]=&[$($n),*];static LITERALS:&[Literal]=&[$(Literal::new($n)),*];};}
literals![
    b"Bytes",
    b"CollectionVt",
    b"GC",
    b"Integer",
    b"Map",
    b"MapNode",
    b"MapVt",
    b"ObjectModel",
    b"ObjectVt",
    b"Roots",
    b"String",
    b"Vector",
    b"Word",
    b"add:",
    b"addMethod",
    b"allocate:bytes:",
    b"allocationSizeOf:",
    b"appendByteValues",
    b"appendBytes:",
    b"at:",
    b"at:ifAbsent:",
    b"at:put:",
    b"behaviorOf:",
    b"bytes",
    b"clone",
    b"collectionEquals:",
    b"collectionHash",
    b"delegated",
    b"do:",
    b"false",
    b"fromByteValues",
    b"fromBytes:",
    b"fromUnsignedLow:high:",
    b"fromWord:",
    b"high32",
    b"includesKey:",
    b"inject:into:",
    b"keys",
    b"keysAndValuesDo:",
    b"low32",
    b"methodAt:",
    b"new",
    b"nil",
    b"ownerOf:put:",
    b"parentOf:",
    b"prepareAllocation",
    b"printString",
    b"register:",
    b"rehashImage:",
    b"release:",
    b"retain:",
    b"rootsDo:",
    b"sequence",
    b"setLayoutOf:to:rest:",
    b"size",
    b"true",
    b"value",
    b"value:value:",
    b"values",
    b"visit:",
    b"withContext:",
];
const fn index(n: &[u8]) -> usize {
    let mut i = 0;
    while i < NAMES.len() {
        let c = NAMES[i];
        if c.len() == n.len() {
            let mut j = 0;
            while j < n.len() && c[j] == n[j] {
                j += 1;
            }
            if j == n.len() {
                return i;
            }
        }
        i += 1;
    }
    panic!("missing literal")
}
static METHODS: &[MethodSpec] = &[
    MethodSpec {
        name: index(b"new"),
        entry: Some(new),
    },
    MethodSpec {
        name: index(b"clone"),
        entry: Some(new),
    },
    MethodSpec {
        name: index(b"at:"),
        entry: Some(at),
    },
    MethodSpec {
        name: index(b"at:put:"),
        entry: Some(put),
    },
    MethodSpec {
        name: index(b"includesKey:"),
        entry: Some(includes),
    },
    MethodSpec {
        name: index(b"at:ifAbsent:"),
        entry: Some(absent),
    },
    MethodSpec {
        name: index(b"size"),
        entry: Some(size),
    },
    MethodSpec {
        name: index(b"keys"),
        entry: Some(keys),
    },
    MethodSpec {
        name: index(b"rehashImage:"),
        entry: Some(rehash_image),
    },
    MethodSpec {
        name: index(b"values"),
        entry: Some(values),
    },
    MethodSpec {
        name: index(b"do:"),
        entry: Some(do_each),
    },
    MethodSpec {
        name: index(b"inject:into:"),
        entry: Some(inject),
    },
    MethodSpec {
        name: index(b"keysAndValuesDo:"),
        entry: Some(pairs),
    },
    MethodSpec {
        name: index(b"printString"),
        entry: Some(print),
    },
    MethodSpec {
        name: index(b"rootsDo:"),
        entry: Some(roots),
    },
];
unsafe extern "C" fn initialize(
    p: *const Protocol,
    image: Oop,
    l: *const Oop,
    n: usize,
    a: *const Oop,
    argc: usize,
) -> Oop {
    API.store(p.cast_mut(), Ordering::Release);
    let id = unsafe { *p };
    let Some([namespace]) = (unsafe { borrowed_slice(a, argc) }) else {
        return id.invalid;
    };
    let Some(symbols) = (unsafe { borrowed_slice(l, n) }) else {
        return id.invalid;
    };
    if symbols.len() != NAMES.len() {
        return id.invalid;
    }
    let old = INSTANCE.load(Ordering::Acquire);
    if !old.is_null() {
        let s = unsafe { &*old };
        return if s.ready.get() && s.namespace == *namespace {
            s.prototype
        } else {
            id.invalid
        };
    }
    done((|| unsafe {
        let names: HashMap<_, _> = NAMES.iter().copied().zip(symbols.iter().copied()).collect();
        let send = |r, n: &[u8], a: &[Oop]| id.send(r, names[n], a);
        let named = |n: &[u8]| object(send(*namespace, b"at:", &[names[n]])?);
        let roots = named(b"Roots")?;
        let model = named(b"ObjectModel")?;
        let parent = named(b"CollectionVt")?;
        let object_parent = named(b"ObjectVt")?;
        let mut tokens = Vec::new();
        let result = (|| {
            let mut keep = |v| -> Result<Oop, Oop> {
                tokens.push(send(roots, b"retain:", &[v])?);
                Ok(v)
            };
            let mut values = HashMap::new();
            for &name in DEPENDENCIES {
                values.insert(name, keep(named(name)?)?);
            }
            let behavior = keep(send(parent, b"delegated", &[])?)?;
            let node = keep(send(object_parent, b"delegated", &[])?)?;
            let layout = keep(send(
                id.root,
                b"sequence",
                &[1, 0].map(|v| abi::size(v).unwrap()),
            )?)?;
            send(
                model,
                b"setLayoutOf:to:rest:",
                &[behavior, layout, abi::size(0).unwrap()],
            )?;
            let empty = keep(send(
                id.root,
                b"sequence",
                &[abi::size(0).unwrap(), abi::size(0).unwrap()],
            )?)?;
            send(
                model,
                b"setLayoutOf:to:rest:",
                &[node, empty, abi::size(1).unwrap()],
            )?;
            let mut allocate = |behavior, bytes| -> Result<Oop, Oop> {
                let value = keep(send(
                    model,
                    b"allocate:bytes:",
                    &[behavior, abi::size(bytes).ok_or_else(invalid)?],
                )?)?;
                if send(id.root, b"allocationSizeOf:", &[value])? != abi::size(bytes).unwrap()
                    || send(id.root, b"behaviorOf:", &[value])? != behavior
                {
                    return Err(invalid());
                }
                Ok(value)
            };
            let prototype = allocate(behavior, size_of::<Map>())?;
            prototype.cast::<Map>().write(Map {
                root: ptr::null_mut(),
                count: 0,
            });
            let s = Box::into_raw(Box::new(State {
                id,
                namespace: *namespace,
                roots,
                model,
                behavior,
                node,
                prototype,
                values,
                names: names.clone(),
                scoped: RefCell::new(Vec::new()),
                ready: Cell::new(false),
            }));
            INSTANCE.store(s, Ordering::Release);
            let s = &*s;
            for spec in METHODS {
                let method = s.send(image, b"methodAt:", &[symbols[spec.name]])?;
                let method = s.send(method, b"withContext:", &[prototype])?;
                s.send(behavior, b"addMethod", &[symbols[spec.name], method])?;
            }
            s.send(roots, b"register:", &[prototype])?;
            s.send(id.root, b"ownerOf:put:", &[behavior, prototype])?;
            for (name, v) in [
                (b"Map".as_slice(), prototype),
                (b"MapVt", behavior),
                (b"MapNode", node),
            ] {
                s.send(*namespace, b"at:put:", &[names[name], v])?;
            }
            s.ready.set(true);
            Ok(prototype)
        })();
        for token in tokens {
            send(roots, b"release:", &[token])?;
        }
        result
    })())
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
