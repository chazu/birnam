//! Persistent Vector storage belongs to Vector; dependencies are Oop receivers.
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
mod queue;
struct State {
    id: Protocol,
    namespace: Oop,
    roots: Oop,
    model: Oop,
    behavior: Oop,
    node: Oop,
    prototype: Oop,
    queue: Oop,
    queue_behavior: Oop,
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
    unsafe fn index(&self, v: Oop) -> Result<Option<i64>, Oop> {
        unsafe {
            let word = self.send(self.value(b"Integer"), b"wordOf:", &[v])?;
            if word.is_null() {
                Ok(None)
            } else {
                Ok(Some(self.word_bits(word)? as i64))
            }
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
            s.queue,
            s.queue_behavior,
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
    b"Integer", b"String", b"Word", b"Bytes", b"nil", b"true", b"false",
];
const BITS: usize = 5;
const WIDTH: usize = 1 << BITS;
const MASK: usize = WIDTH - 1;
#[derive(Clone, Copy)]
#[repr(C)]
struct Vector {
    count: usize,
    shift: usize,
    root: Oop,
    tail: Oop,
    tail_len: usize,
}
unsafe fn payload(s: &State, value: Oop) -> Result<Vector, Oop> {
    unsafe {
        if s.send(s.id.root, b"allocationSizeOf:", &[value])?
            != abi::size(size_of::<Vector>()).unwrap()
        {
            return Err(invalid());
        }
        let v = value.cast::<Vector>().read();
        if v.shift < BITS
            || v.shift >= usize::BITS as usize
            || v.shift % BITS != 0
            || v.tail_len > WIDTH
            || v.tail_len > v.count
            || (v.count != 0 && v.tail_len == 0)
        {
            return Err(invalid());
        }
        Ok(v)
    }
}
unsafe fn node(value: Oop) -> *mut [Oop; WIDTH] {
    value.cast()
}
unsafe fn check_node(s: &State, value: Oop) -> Result<(), Oop> {
    if unsafe { s.send(s.id.root, b"allocationSizeOf:", &[value]) }?
        != abi::size(size_of::<[Oop; WIDTH]>()).unwrap()
    {
        return Err(invalid());
    }
    Ok(())
}
unsafe fn new_node(s: &State, scope: &Scope<'_>) -> Result<Oop, Oop> {
    unsafe { s.allocate(scope, s.node, size_of::<[Oop; WIDTH]>()) }
}
unsafe fn copy_node(s: &State, scope: &Scope<'_>, source: Oop) -> Result<Oop, Oop> {
    unsafe {
        check_node(s, source)?;
        let value = new_node(s, scope)?;
        *node(value) = *node(source);
        Ok(value)
    }
}
unsafe fn new_path(s: &State, scope: &Scope<'_>, level: usize, tail: Oop) -> Result<Oop, Oop> {
    unsafe {
        if level == 0 {
            return Ok(tail);
        }
        let value = new_node(s, scope)?;
        (*node(value))[0] = new_path(s, scope, level - BITS, tail)?;
        Ok(value)
    }
}
unsafe fn push_tail(
    s: &State,
    scope: &Scope<'_>,
    count: usize,
    level: usize,
    parent: Oop,
    tail: Oop,
) -> Result<Oop, Oop> {
    unsafe {
        let index = ((count - 1) >> level) & MASK;
        let result = copy_node(s, scope, parent)?;
        let existing = (*node(parent))[index];
        let child = if level == BITS {
            tail
        } else if existing.is_null() {
            new_path(s, scope, level - BITS, tail)?
        } else {
            push_tail(s, scope, count, level - BITS, existing, tail)?
        };
        (*node(result))[index] = child;
        Ok(result)
    }
}
unsafe fn make(s: &State, scope: &Scope<'_>, v: Vector) -> Result<Oop, Oop> {
    unsafe {
        let result = s.allocate(scope, s.behavior, size_of::<Vector>())?;
        result.cast::<Vector>().write(v);
        Ok(result)
    }
}
unsafe fn empty(s: &State, scope: &Scope<'_>) -> Result<Oop, Oop> {
    unsafe {
        let root = new_node(s, scope)?;
        let tail = new_node(s, scope)?;
        make(
            s,
            scope,
            Vector {
                count: 0,
                shift: BITS,
                root,
                tail,
                tail_len: 0,
            },
        )
    }
}
unsafe fn at(s: &State, v: Vector, index: usize) -> Result<Oop, Oop> {
    unsafe {
        if index >= v.count {
            return Ok(ptr::null_mut());
        }
        let leaf = if index >= v.count - v.tail_len {
            v.tail
        } else {
            let mut current = v.root;
            let mut level = v.shift;
            while level > 0 {
                check_node(s, current)?;
                current = (*node(current))[(index >> level) & MASK];
                level -= BITS;
            }
            current
        };
        check_node(s, leaf)?;
        Ok((*node(leaf))[index & MASK])
    }
}
unsafe fn push(s: &State, scope: &Scope<'_>, source: Vector, value: Oop) -> Result<Oop, Oop> {
    unsafe {
        scope.keep(source.root);
        scope.keep(source.tail);
        scope.keep(value);
        let count = source.count.checked_add(1).ok_or_else(invalid)?;
        if source.tail_len < WIDTH {
            let tail = copy_node(s, scope, source.tail)?;
            (*node(tail))[source.tail_len] = value;
            return make(
                s,
                scope,
                Vector {
                    count,
                    tail,
                    tail_len: source.tail_len + 1,
                    ..source
                },
            );
        }
        let mut shift = source.shift;
        let root = if (source.count >> BITS) > (1usize << source.shift) {
            let root = new_node(s, scope)?;
            (*node(root))[0] = source.root;
            (*node(root))[1] = new_path(s, scope, source.shift, source.tail)?;
            shift += BITS;
            root
        } else {
            push_tail(
                s,
                scope,
                source.count,
                source.shift,
                source.root,
                source.tail,
            )?
        };
        let tail = new_node(s, scope)?;
        (*node(tail))[0] = value;
        make(
            s,
            scope,
            Vector {
                count,
                shift,
                root,
                tail,
                tail_len: 1,
            },
        )
    }
}
// Assemble only fresh nodes; no intermediate Vector escapes or needs persistence.
unsafe fn build(s: &State, values: impl IntoIterator<Item = Oop>) -> Result<Oop, Oop> {
    let values: Vec<_> = values.into_iter().collect();
    unsafe { Scope::run(s, &values, |work| {
        let count = values.len();
        let tail_len = if count == 0 { 0 } else { (count - 1) % WIDTH + 1 };
        let tail_start = count - tail_len;
        let tail = new_node(s, work)?;
        (&mut *node(tail))[..tail_len].copy_from_slice(&values[tail_start..]);
        let mut level = Vec::new();
        for chunk in values[..tail_start].chunks(WIDTH) {
            let leaf = new_node(s, work)?;
            (*node(leaf)).copy_from_slice(chunk);
            level.push(leaf);
        }
        let mut shift = BITS;
        while level.len() > WIDTH {
            let mut parents = Vec::new();
            for children in level.chunks(WIDTH) {
                let parent = new_node(s, work)?;
                (&mut *node(parent))[..children.len()].copy_from_slice(children);
                parents.push(parent);
            }
            level = parents;
            shift += BITS;
        }
        let root = new_node(s, work)?;
        (&mut *node(root))[..level.len()].copy_from_slice(&level);
        make(s, work, Vector {count, shift, root, tail, tail_len})
    }) }
}
// Copy the published tail once, then fill fresh tails before linking them.
unsafe fn append_many(s: &State, scope: &Scope<'_>, mut v: Vector, values: &[Oop]) -> Result<Oop, Oop> {
    unsafe {
        v.count.checked_add(values.len()).ok_or_else(invalid)?;
        v.tail = copy_node(s, scope, v.tail)?;
        for &value in values {
            if v.tail_len == WIDTH {
                if (v.count >> BITS) > (1usize << v.shift) {
                    let root = new_node(s, scope)?;
                    (*node(root))[0] = v.root;
                    (*node(root))[1] = new_path(s, scope, v.shift, v.tail)?;
                    v.root = root;
                    v.shift += BITS;
                } else {
                    v.root = push_tail(s, scope, v.count, v.shift, v.root, v.tail)?;
                }
                v.tail = new_node(s, scope)?;
                v.tail_len = 0;
            }
            (*node(v.tail))[v.tail_len] = value;
            v.tail_len += 1;
            v.count += 1;
        }
        make(s, scope, v)
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
            1 | 2 | 3 | 5 | 8 | 10 | 12 => 1,
            4 | 6 => 2,
            _ => 0,
        };
        if args.len() != arity {
            return Ok(s.nil());
        }
        if op == 11 {
            return Ok(empty(s, scope)?);
        }
        let v = payload(s, r)?;
        scope.keep(v.root);
        scope.keep(v.tail);
        match op {
            0 => s.integer(v.count as i64),
            1 => {
                let Some(index) = s.index(args[0])? else {
                    return Ok(s.nil());
                };
                let value = if index < 0 {
                    ptr::null_mut()
                } else {
                    at(s, v, index as usize)?
                };
                Ok(if value.is_null() { s.nil() } else { value })
            }
            2 => Ok(push(s, scope, v, args[0])?),
            3 => {
                for i in 0..v.count {
                    s.send(args[0], b"value:", &[at(s, v, i)?])?;
                }
                Ok(r)
            }
            4 => Scope::run(s, &[args[0]], |work| {
                let mut acc = args[0];
                for i in 0..v.count {
                    let next = s.send(args[1], b"value:value:", &[acc, at(s, v, i)?])?;
                    let Some(next) = (!next.is_null()).then_some(next) else {
                        return Ok(s.nil());
                    };
                    acc = work.reset(next);
                }
                Ok(acc)
            }),
            5 => {
                for i in 0..v.count {
                    if s.same(at(s, v, i)?, args[0])? {
                        return s.integer(i as i64);
                    }
                }
                Ok(s.nil())
            }
            6 => {
                let (Some(from), Some(to)) = (s.index(args[0])?, s.index(args[1])?) else {
                    return Ok(s.nil());
                };
                if from < 0 || to < from || to as usize > v.count {
                    return Ok(s.nil());
                }
                Ok(build(
                    s,
                    (from as usize..to as usize)
                        .map(|i| Ok(scope.keep(at(s, v, i)?)))
                        .collect::<Result<Vec<_>, _>>()?,
                )?)
            }
            7 => Ok(build(
                s,
                (0..v.count)
                    .rev()
                    .map(|i| Ok(scope.keep(at(s, v, i)?)))
                    .collect::<Result<Vec<_>, _>>()?,
            )?),
            8 | 12 => {
                if v.count < 2 {
                    return Ok(r);
                }
                let mut elements = Vec::with_capacity(v.count);
                let mut keys = Vec::with_capacity(v.count);
                for i in 0..v.count {
                    let value = scope.keep(at(s, v, i)?);
                    let result = s.send(args[0], b"value:", &[value])?;
                    let Some(key) = (!result.is_null()).then_some(result) else {
                        return Ok(s.nil());
                    };
                    elements.push(value);
                    keys.push(scope.keep(key));
                }
                // Stable insertion sort; evaluate each key exactly once.
                let mut order: Vec<usize> = (0..v.count).collect();
                if op == 12 {
                    // Explicit stable merge ordering. Keep all keys/elements
                    // rooted across each current, possibly reentrant < send.
                    let mut scratch = vec![0usize; v.count];
                    let mut width = 1usize;
                    while width < v.count {
                        for start in (0..v.count).step_by(width.saturating_mul(2)) {
                            let middle = (start + width).min(v.count);
                            let end = start.saturating_add(width.saturating_mul(2)).min(v.count);
                            let (mut left, mut right) = (start, middle);
                            for slot in &mut scratch[start..end] {
                                let take_right = left == middle
                                    || (right < end
                                        && s.send(keys[order[right]], b"<", &[keys[order[left]]])?
                                            == s.value(b"true"));
                                if take_right {
                                    *slot = order[right];
                                    right += 1;
                                } else {
                                    *slot = order[left];
                                    left += 1;
                                }
                            }
                        }
                        core::mem::swap(&mut order, &mut scratch);
                        width = width.saturating_mul(2);
                    }
                } else {
                    for i in 1..v.count {
                        let candidate = order[i];
                        let mut position = i;
                        while position > 0 {
                            if s.send(keys[candidate], b"<", &[keys[order[position - 1]]])?
                                != s.value(b"true")
                            {
                                break;
                            }
                            order[position] = order[position - 1];
                            position -= 1;
                        }
                        order[position] = candidate;
                    }
                }
                Ok(build(s, order.into_iter().map(|i| elements[i]))?)
            }
            9 => {
                let output = scope.keep(s.send(
                    s.value(b"Bytes"),
                    b"fromByteValues",
                    &[abi::size(35).unwrap(), abi::size(91).unwrap()],
                )?);
                for i in 0..v.count {
                    if i > 0 {
                        s.send(output, b"appendByteValues", &[abi::size(32).unwrap()])?;
                    }
                    Scope::run(s, &[], |work| {
                        let printed = work.keep(s.send(at(s, v, i)?, b"printString", &[])?);
                        let bytes = s.send(printed, b"bytes", &[])?;
                        s.send(output, b"appendBytes:", &[bytes])
                    })?;
                }
                s.send(output, b"appendByteValues", &[abi::size(93).unwrap()])?;
                s.send(s.value(b"String"), b"fromBytes:", &[output])
            }
            10 => {
                if s.send(s.id.root, b"behaviorOf:", &[args[0]])? != s.behavior {
                    return Ok(s.nil());
                }
                let other = payload(s, args[0])?;
                if other.count == 0 { return Ok(r); }
                let values = (0..other.count).map(|i| Ok(scope.keep(at(s, other, i)?)))
                    .collect::<Result<Vec<_>, Oop>>()?;
                append_many(s, scope, v, &values)
            }
            _ => Err(invalid()),
        }
    }
}
entry!(size, 0);
entry!(at_method, 1);
entry!(add, 2);
entry!(do_each, 3);
entry!(inject, 4);
entry!(index_of, 5);
entry!(copy, 6);
entry!(reversed, 7);
entry!(sort_by, 8);
entry!(merge_sort_by, 12);
entry!(print, 9);
entry!(comma, 10);
entry!(new, 11);

unsafe extern "C" fn size_word(_: Oop, c: Oop, r: Oop, a: *const Oop, count: usize) -> Oop {
    done((|| unsafe {
        let s = state(c)?;
        if !borrowed_slice(a, count).ok_or_else(invalid)?.is_empty() {
            return Err(invalid());
        }
        abi::size(payload(s, r)?.count).ok_or_else(invalid)
    })())
}
unsafe extern "C" fn at_word(_: Oop, c: Oop, r: Oop, a: *const Oop, count: usize) -> Oop {
    done((|| unsafe {
        let s = state(c)?;
        let [index] = borrowed_slice(a, count).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        // Preserve the Vector snapshot if index conversion reenters the runtime.
        Scope::run(s, &[], |scope| {
            let v = payload(s, r)?;
            scope.keep(v.root);
            scope.keep(v.tail);
            let index = s.word_bits(*index)? as usize;
            let result = at(s, v, index)?;
            Ok(if result.is_null() { s.nil() } else { result })
        })
    })())
}
macro_rules! literals{($($n:literal),*$(,)?)=>{const NAMES:&[&[u8]]=&[$($n),*];static LITERALS:&[Literal]=&[$(Literal::new($n)),*];};}
literals![
    b"Queue",
    b"QueueVt",
    b"takeFirst",
    b"asVector",
    b"queueNew",
    b"queueAdd",
    b"queueSize",
    b"queueTake",
    b"queueVector",
    b",",
    b"<",
    b"Bytes",
    b"Integer",
    b"ObjectModel",
    b"ObjectVt",
    b"CollectionVt",
    b"Roots",
    b"String",
    b"Vector",
    b"VectorNode",
    b"VectorVt",
    b"Word",
    b"add:",
    b"addMethod",
    b"allocate:bytes:",
    b"allocationSizeOf:",
    b"appendByteValues",
    b"appendBytes:",
    b"at:",
    b"at:put:",
    b"atWord:",
    b"behaviorEpoch",
    b"behaviorOf:",
    b"bind:to:",
    b"bytes",
    b"canOmitAllocationFor:",
    b"clone",
    b"collectionEquals:",
    b"copyFrom:to:",
    b"delegated",
    b"do:",
    b"false",
    b"fromByteValues",
    b"fromBytes:",
    b"fromUnsignedLow:high:",
    b"fromWord:",
    b"high32",
    b"indexOf:",
    b"inject:into:",
    b"low32",
    b"mergeSortBy:",
    b"methodAt:",
    b"new",
    b"nil",
    b"ownerOf:put:",
    b"printString",
    b"register:",
    b"release:",
    b"retain:",
    b"reversed",
    b"rootsDo:",
    b"sequence",
    b"setLayoutOf:to:rest:",
    b"size",
    b"sizeWord",
    b"sortBy:",
    b"true",
    b"usesDefaultAllocation",
    b"usesDefaultLookup:",
    b"value:",
    b"value:value:",
    b"visit:",
    b"withContext:",
    b"wordOf:",
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
    MethodSpec { name: index(b"queueNew"), entry: Some(queue::new) },
    MethodSpec { name: index(b"queueAdd"), entry: Some(queue::add) },
    MethodSpec { name: index(b"queueSize"), entry: Some(queue::size) },
    MethodSpec { name: index(b"queueTake"), entry: Some(queue::take) },
    MethodSpec { name: index(b"queueVector"), entry: Some(queue::vector) },
    MethodSpec {
        name: index(b"sizeWord"),
        entry: Some(size_word),
    },
    MethodSpec {
        name: index(b"atWord:"),
        entry: Some(at_word),
    },
    MethodSpec {
        name: index(b"size"),
        entry: Some(size),
    },
    MethodSpec {
        name: index(b"at:"),
        entry: Some(at_method),
    },
    MethodSpec {
        name: index(b"add:"),
        entry: Some(add),
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
        name: index(b"indexOf:"),
        entry: Some(index_of),
    },
    MethodSpec {
        name: index(b"copyFrom:to:"),
        entry: Some(copy),
    },
    MethodSpec {
        name: index(b"reversed"),
        entry: Some(reversed),
    },
    MethodSpec {
        name: index(b"sortBy:"),
        entry: Some(sort_by),
    },
    MethodSpec {
        name: index(b"mergeSortBy:"),
        entry: Some(merge_sort_by),
    },
    MethodSpec {
        name: index(b"printString"),
        entry: Some(print),
    },
    MethodSpec {
        name: index(b","),
        entry: Some(comma),
    },
    MethodSpec {
        name: index(b"new"),
        entry: Some(new),
    },
    MethodSpec {
        name: index(b"clone"),
        entry: Some(new),
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
                &[0, 0, 1, 1, 0].map(|v| abi::size(v).unwrap()),
            )?)?;
            send(
                model,
                b"setLayoutOf:to:rest:",
                &[behavior, layout, abi::size(0).unwrap()],
            )?;
            let empty = keep(send(id.root, b"sequence", &[])?)?;
            send(
                model,
                b"setLayoutOf:to:rest:",
                &[node, empty, abi::size(1).unwrap()],
            )?;
            let queue_behavior = keep(send(parent, b"delegated", &[])?)?;
            let queue_layout = keep(send(id.root,b"sequence",&[abi::size(1).unwrap();3])?)?;
            send(model,b"setLayoutOf:to:rest:",&[queue_behavior,queue_layout,abi::size(0).unwrap()])?;
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
            let root = allocate(node, size_of::<[Oop; WIDTH]>())?;
            let tail = allocate(node, size_of::<[Oop; WIDTH]>())?;
            let prototype = allocate(behavior, size_of::<Vector>())?;
            prototype.cast::<Vector>().write(Vector {
                count: 0,
                shift: BITS,
                root,
                tail,
                tail_len: 0,
            });
            let queue = allocate(queue_behavior, size_of::<queue::Queue>())?;
            queue.cast::<queue::Queue>().write(queue::Queue::empty(values[b"nil".as_slice()]));
            let s = Box::into_raw(Box::new(State {
                id,
                namespace: *namespace,
                roots,
                model,
                behavior,
                node,
                prototype,
                queue,
                queue_behavior,
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
                if let Some((_, public)) = queue::BINDINGS.iter().find(|(name,_)| *name == NAMES[spec.name]) {
                    s.send(queue_behavior,b"addMethod",&[names[*public],method])?;
                } else {
                    s.send(behavior, b"addMethod", &[symbols[spec.name], method])?;
                }
            }
            s.send(roots, b"register:", &[prototype])?;
            s.send(id.root, b"ownerOf:put:", &[behavior, prototype])?;
            s.send(id.root, b"ownerOf:put:", &[queue_behavior, queue])?;
            for (name, v) in [
                (b"Vector".as_slice(), prototype),
                (b"VectorVt", behavior),
                (b"Queue", queue),
                (b"QueueVt", queue_behavior),
                (b"VectorNode", node),
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
