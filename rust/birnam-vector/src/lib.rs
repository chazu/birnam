//! Vector representation and operations. Dependencies are opaque Id receivers.
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
        let visitor = object(*visitor)?;
        let mut values = vec![s.roots, s.model, s.behavior, s.node, s.prototype.get()];
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
    b"collectionEquals:",
    b"value:",
    b"value:value:",
    b"<",
    b"Vector",
    b"VectorVt",
    b"VectorNode",
    b"behaviorEpoch",
    b"usesDefaultAllocation",
    b"usesDefaultLookup:",
    b"bind:to:",
    b"canOmitAllocationFor:",
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
                Operand::bytes(&[0, 0, 1, 1, 0]),
                Operand::word(0),
            ],
        )?;
        id.send_named(
            model,
            b"setLayoutOf:to:rest:",
            &[Operand::object(node), Operand::bytes(&[]), Operand::word(1)],
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
            for (name, value) in [(b"Vector".as_slice(), prototype), (b"VectorVt", behavior)] {
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
                    Operand::object(s.names[b"VectorNode".as_slice()]),
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
            &[Operand::object(id.intern_name(b"Vector")?)],
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

const DEPENDENCIES: &[&[u8]] = &[b"Integer", b"String", b"nil", b"true", b"false"];
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
unsafe fn payload(s: &State, value: Oop) -> Result<Vector, Outcome> {
    unsafe {
        if s.send(
            s.id.receiver,
            b"allocationSizeOf:",
            &[Operand::object(value)],
        )?
        .as_word()
            != Some(size_of::<Vector>())
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
unsafe fn check_node(s: &State, value: Oop) -> Result<(), Outcome> {
    if unsafe {
        s.send(
            s.id.receiver,
            b"allocationSizeOf:",
            &[Operand::object(value)],
        )
    }?
    .as_word()
        != Some(size_of::<[Oop; WIDTH]>())
    {
        return Err(invalid());
    }
    Ok(())
}
unsafe fn new_node(s: &State, scope: &Scope<'_>) -> Result<Oop, Outcome> {
    unsafe { s.allocate(scope, s.node, size_of::<[Oop; WIDTH]>()) }
}
unsafe fn copy_node(s: &State, scope: &Scope<'_>, source: Oop) -> Result<Oop, Outcome> {
    unsafe {
        check_node(s, source)?;
        let value = new_node(s, scope)?;
        *node(value) = *node(source);
        Ok(value)
    }
}
unsafe fn new_path(s: &State, scope: &Scope<'_>, level: usize, tail: Oop) -> Result<Oop, Outcome> {
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
) -> Result<Oop, Outcome> {
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
unsafe fn make(s: &State, scope: &Scope<'_>, v: Vector) -> Result<Oop, Outcome> {
    unsafe {
        let result = s.allocate(scope, s.behavior, size_of::<Vector>())?;
        result.cast::<Vector>().write(v);
        Ok(result)
    }
}
unsafe fn empty(s: &State, scope: &Scope<'_>) -> Result<Oop, Outcome> {
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
unsafe fn at(s: &State, v: Vector, index: usize) -> Result<Oop, Outcome> {
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
unsafe fn push(s: &State, scope: &Scope<'_>, source: Vector, value: Oop) -> Result<Oop, Outcome> {
    unsafe {
        scope.keep(source.root)?;
        scope.keep(source.tail)?;
        scope.keep(value)?;
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
unsafe fn build(s: &State, values: impl IntoIterator<Item = Oop>) -> Result<Oop, Outcome> {
    unsafe {
        Scope::run(s, &[], |work| {
            let mut result = empty(s, work)?;
            for value in values {
                result = push(s, work, payload(s, result)?, value)?;
                work.reset(result)?;
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
            1 | 2 | 3 | 5 | 8 | 10 | 12 => 1,
            4 | 6 => 2,
            _ => 0,
        };
        if args.len() != arity {
            return Ok(s.nil());
        }
        if op == 11 {
            return Ok(Operand::object(empty(s, scope)?));
        }
        let v = payload(s, r)?;
        scope.keep(v.root)?;
        scope.keep(v.tail)?;
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
                Ok(if value.is_null() {
                    s.nil()
                } else {
                    Operand::object(value)
                })
            }
            2 => Ok(Operand::object(push(s, scope, v, args[0])?)),
            3 => {
                for i in 0..v.count {
                    s.send(args[0], b"value:", &[Operand::object(at(s, v, i)?)])?;
                }
                Ok(Operand::object(r))
            }
            4 => Scope::run(s, &[args[0]], |work| {
                let mut acc = args[0];
                for i in 0..v.count {
                    let next = s.send(
                        args[1],
                        b"value:value:",
                        &[Operand::object(acc), Operand::object(at(s, v, i)?)],
                    )?;
                    let Some(next) = next.as_object().filter(|v| !v.is_null()) else {
                        return Ok(s.nil());
                    };
                    acc = work.reset(next)?;
                }
                Ok(Operand::object(acc))
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
                Ok(Operand::object(build(
                    s,
                    (from as usize..to as usize)
                        .map(|i| scope.keep(at(s, v, i)?))
                        .collect::<Result<Vec<_>, _>>()?,
                )?))
            }
            7 => Ok(Operand::object(build(
                s,
                (0..v.count)
                    .rev()
                    .map(|i| scope.keep(at(s, v, i)?))
                    .collect::<Result<Vec<_>, _>>()?,
            )?)),
            8 | 12 => {
                if v.count < 2 {
                    return Ok(Operand::object(r));
                }
                let mut elements = Vec::with_capacity(v.count);
                let mut keys = Vec::with_capacity(v.count);
                for i in 0..v.count {
                    let value = scope.keep(at(s, v, i)?)?;
                    let result = s.send(args[0], b"value:", &[Operand::object(value)])?;
                    let Some(key) = result.as_object().filter(|v| !v.is_null()) else {
                        return Ok(s.nil());
                    };
                    elements.push(value);
                    keys.push(scope.keep(key)?);
                }
                // Stable insertion sort; evaluate each key exactly once.
                let mut order: Vec<usize> = (0..v.count).collect();
                if op == 12 {
                    // Explicit stable merge ordering. Keep all keys/elements
                    // rooted across each current, possibly reentrant < send.
                    let mut scratch=vec![0usize; v.count];
                    let mut width=1usize;
                    while width < v.count {
                        for start in (0..v.count).step_by(width.saturating_mul(2)) {
                            let middle=(start+width).min(v.count);
                            let end=start.saturating_add(width.saturating_mul(2)).min(v.count);
                            let (mut left,mut right)=(start,middle);
                            for slot in &mut scratch[start..end] {
                                let take_right = left==middle || (right<end && s.send(
                                    keys[order[right]], b"<", &[Operand::object(keys[order[left]])])?
                                    .as_object()==Some(s.value(b"true")));
                                if take_right { *slot=order[right];right+=1; }
                                else { *slot=order[left];left+=1; }
                            }
                        }
                        core::mem::swap(&mut order,&mut scratch);
                        width=width.saturating_mul(2);
                    }
                } else {
                for i in 1..v.count {
                    let candidate = order[i];
                    let mut position = i;
                    while position > 0 {
                        if s.send(
                            keys[candidate],
                            b"<",
                            &[Operand::object(keys[order[position - 1]])],
                        )?
                        .as_object()
                            != Some(s.value(b"true"))
                        {
                            break;
                        }
                        order[position] = order[position - 1];
                        position -= 1;
                    }
                    order[position] = candidate;
                }
                }
                Ok(Operand::object(build(
                    s,
                    order.into_iter().map(|i| elements[i]),
                )?))
            }
            9 => {
                let mut bytes = b"#[".to_vec();
                for i in 0..v.count {
                    if i > 0 {
                        bytes.push(b' ');
                    }
                    bytes.extend_from_slice(&s.printable(at(s, v, i)?)?);
                }
                bytes.push(b']');
                s.send(s.value(b"String"), b"fromBytes:", &[Operand::bytes(&bytes)])
            }
            10 => {
                if s.send(s.id.receiver, b"behaviorOf:", &[Operand::object(args[0])])?
                    .as_object()
                    != Some(s.behavior)
                {
                    return Ok(s.nil());
                }
                let other = payload(s, args[0])?;
                Scope::run(s, &[r], |work| {
                    let mut result = r;
                    for i in 0..other.count {
                        result = push(s, work, payload(s, result)?, at(s, other, i)?)?;
                        work.reset(result)?;
                    }
                    Ok(Operand::object(result))
                })
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
// Raw index access for generic ABI marshalling. Numeric boxing is language policy.
unsafe extern "C" fn size_word(c:*mut c_void,r:Oop,a:*const Operand,n:usize)->Outcome {
    done((||unsafe {let s=state(c)?; if !borrowed_slice(a,n).ok_or_else(invalid)?.is_empty() {return Err(invalid());}
        Scope::run(s,&[r],|_|Ok(Operand::word(payload(s,r)?.count)))
    })())
}
unsafe extern "C" fn at_word(c:*mut c_void,r:Oop,a:*const Operand,n:usize)->Outcome {
    done((||unsafe {let s=state(c)?;let [index]=borrowed_slice(a,n).ok_or_else(invalid)? else {return Err(invalid());};let index=index.as_word().ok_or_else(invalid)?;
        Scope::run(s,&[r],|_| {let value=at(s,payload(s,r)?,index)?;Ok(if value.is_null() {s.nil()} else {Operand::object(value)})})
    })())
}
const METHODS: &[(&[u8], Method)] = &[
    (b"sizeWord", size_word),
    (b"atWord:", at_word),
    (b"size", size),
    (b"at:", at_method),
    (b"add:", add),
    (b"do:", do_each),
    (b"inject:into:", inject),
    (b"indexOf:", index_of),
    (b"copyFrom:to:", copy),
    (b"reversed", reversed),
    (b"sortBy:", sort_by),
    (b"mergeSortBy:", merge_sort_by),
    (b"printString", print),
    (b",", comma),
    (b"new", new),
    (b"clone", new),
];
