pub(crate) use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as native, Method, NativeEntry, Operand, Outcome, Protocol},
};
pub(crate) use core::{cell::RefCell, ffi::c_void};
use std::collections::HashMap;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Bytes,
    TaggedView,
}
pub(crate) struct Buffer {
    pub(crate) bytes: Vec<u8>,
    pub(crate) kind: Kind,
}
pub(crate) struct Bytes {
    pub(crate) id: Protocol,
    pub(crate) namespace: Oop,
    roots: Oop,
    gc: Oop,
    behavior: Oop,
    view_behavior: Oop,
    prototype: Oop,
    pub(crate) nil: Oop,
    pub(crate) buffers: RefCell<HashMap<usize, Buffer>>,
    pub(crate) views: RefCell<HashMap<usize, crate::tagged_view::View>>,
    names: Vec<Oop>,
    thread: std::thread::ThreadId,
}
pub(crate) fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
pub(crate) fn object(v: Operand) -> Result<Oop, Outcome> {
    v.as_object().filter(|v| !v.is_null()).ok_or_else(invalid)
}
pub(crate) fn done(v: Result<Operand, Outcome>) -> Outcome {
    v.map_or_else(|e| e, Outcome::loaded)
}
pub(crate) unsafe fn state<'a>(c: *mut c_void) -> Result<&'a Bytes, Outcome> {
    let t = unsafe { &*c.cast::<Bytes>() };
    if t.thread != std::thread::current().id() {
        return Err(invalid());
    }
    Ok(t)
}
impl Bytes {
    pub(crate) unsafe fn binding(&self, name: &[u8]) -> Result<Oop, Outcome> {
        unsafe {
            object(self.id.send_named(
                self.namespace,
                b"at:",
                &[Operand::object(self.id.intern_name(name)?)],
            )?)
        }
    }
    pub(crate) unsafe fn rooted(
        &self,
        values: &[Oop],
        f: impl FnOnce() -> Result<Operand, Outcome>,
    ) -> Result<Operand, Outcome> {
        let mut tokens = Vec::new();
        let result = (|| unsafe {
            for &v in values {
                tokens.push(
                    self.id
                        .send_named(self.roots, b"retain:", &[Operand::object(v)])?
                        .as_word()
                        .filter(|v| *v != 0)
                        .ok_or_else(invalid)?,
                );
            }
            f()
        })();
        for token in tokens {
            if unsafe {
                self.id
                    .send_named(self.roots, b"release:", &[Operand::word(token)])
            }?
            .as_word()
                != Some(1)
            {
                return Err(invalid());
            }
        }
        result
    }
    pub(crate) fn length(&self, value: Oop, kind: Kind) -> Result<usize, Outcome> {
        self.buffers
            .borrow()
            .get(&value.addr())
            .filter(|b| b.kind == kind)
            .map(|b| b.bytes.len())
            .ok_or_else(invalid)
    }
    pub(crate) fn copy(&self, value: Oop, kind: Kind) -> Result<Vec<u8>, Outcome> {
        self.buffers
            .borrow()
            .get(&value.addr())
            .filter(|b| b.kind == kind)
            .map(|b| b.bytes.clone())
            .ok_or_else(invalid)
    }
    pub(crate) unsafe fn input_bytes(&self, value: Oop) -> Result<Vec<u8>, Outcome> {
        // Foreign providers are opaque. Copy their borrow before another send.
        let raw = unsafe { self.id.send_named(value, b"bytes", &[]) }?
            .as_bytes()
            .ok_or_else(invalid)?;
        Ok(unsafe { borrowed_bytes(raw.pointer, raw.length) }
            .ok_or_else(invalid)?
            .to_vec())
    }
    pub(crate) unsafe fn make(&self, bytes: Vec<u8>, kind: Kind) -> Result<Operand, Outcome> {
        unsafe {
            let behavior = if kind == Kind::Bytes {
                self.behavior
            } else {
                self.view_behavior
            };
            let value = object(self.id.send_named(
                behavior,
                b"allocate",
                &[Operand::word(2 * size_of::<usize>())],
            )?)?;
            self.rooted(&[value], || {
                if self
                    .id
                    .send_named(
                        self.id.receiver,
                        b"allocationSizeOf:",
                        &[Operand::object(value)],
                    )?
                    .as_word()
                    != Some(2 * size_of::<usize>())
                    || self
                        .id
                        .send_named(self.id.receiver, b"behaviorOf:", &[Operand::object(value)])?
                        .as_object()
                        != Some(behavior)
                    || self.buffers.borrow().contains_key(&value.addr())
                {
                    return Err(invalid());
                }
                let b = Buffer { bytes, kind };
                mirror(value, &b);
                self.buffers.borrow_mut().insert(value.addr(), b);
                self.id.send_named(
                    self.gc,
                    b"finalize:with:",
                    &[Operand::object(value), Operand::object(self.prototype)],
                )?;
                Ok(Operand::object(value))
            })
        }
    }
}
unsafe fn mirror(value: Oop, buffer: &Buffer) {
    unsafe {
        value.cast::<usize>().write(buffer.bytes.as_ptr().addr());
        value.cast::<usize>().add(1).write(buffer.bytes.len());
    }
}
unsafe extern "C" fn roots(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let t = state(c)?;
        let [visitor] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let visitor = object(*visitor)?;
        for value in [
            t.namespace,
            t.roots,
            t.gc,
            t.behavior,
            t.view_behavior,
            t.prototype,
            t.nil,
        ]
        .into_iter()
        .chain(t.names.iter().copied())
        {
            t.id.send_named(visitor, b"visit:", &[Operand::object(value)])?;
        }
        Ok(Operand::unit())
    })())
}
unsafe extern "C" fn references(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let t = state(c)?;
        let [value, visitor] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let (value, visitor) = (object(*value)?, object(*visitor)?);
        t.buffers.borrow().get(&value.addr()).ok_or_else(invalid)?;
        if t.id
            .send_named(t.gc, b"precisePayloads", &[])?
            .as_word()
            .ok_or_else(invalid)?
            == 0
        {
            for i in 0..2 {
                let edge = t.id.send_named(
                    t.id.receiver,
                    b"allocationAt:",
                    &[Operand::word(value.cast::<usize>().add(i).read())],
                )?;
                if edge.as_object().is_some() {
                    t.id.send_named(visitor, b"visit:", &[edge])?;
                }
            }
        }
        Ok(Operand::unit())
    })())
}
unsafe extern "C" fn finalize(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let t = state(c)?;
        let [value] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let value = object(*value)?;
        // Detached allocation: no Id queries or outgoing callbacks here.
        t.views.borrow_mut().remove(&value.addr());
        t.buffers
            .borrow_mut()
            .remove(&value.addr())
            .ok_or_else(invalid)?;
        Ok(Operand::unit())
    })())
}
unsafe fn run(t: &Bytes, r: Oop, a: &[Operand], op: usize) -> Result<Operand, Outcome> {
    unsafe {
        if op == 0 || op == 7 {
            let [input] = a else {
                return Err(invalid());
            };
            let raw = input.as_bytes().ok_or_else(invalid)?;
            let copied = borrowed_bytes(raw.pointer, raw.length)
                .ok_or_else(invalid)?
                .to_vec();
            return t.rooted(&[r], || {
                if op == 0 {
                    return t.make(copied, Kind::Bytes);
                }
                t.id.send_named(t.gc, b"prepareAllocation", &[])?;
                let mut buffers = t.buffers.borrow_mut();
                let b = buffers
                    .get_mut(&r.addr())
                    .filter(|b| b.kind == Kind::Bytes)
                    .ok_or_else(invalid)?;
                b.bytes.extend_from_slice(&copied);
                mirror(r, b);
                Ok(Operand::object(r))
            });
        }
        if matches!(op, 1..=3) {
            if !a.is_empty() {
                return Err(invalid());
            }
            if op == 1 || op == 2 {
                let buffers = t.buffers.borrow();
                let b = buffers
                    .get(&r.addr())
                    .filter(|b| b.kind == Kind::Bytes)
                    .ok_or_else(invalid)?;
                return Ok(if op == 1 {
                    Operand::bytes(&b.bytes)
                } else {
                    Operand::word(b.bytes.len())
                });
            }
            return t.rooted(&[r], || {
                let copied = t.copy(r, Kind::Bytes)?;
                t.make(copied, Kind::Bytes)
            });
        }
        // Raw Word indices and byte values avoid depending on language Integer.
        let count = if op == 6 { 2 } else { 1 };
        if a.len() != count {
            return Err(invalid());
        }
        let first = a[0].as_word().ok_or_else(invalid)?;
        if op == 5 {
            return Ok(t
                .buffers
                .borrow()
                .get(&r.addr())
                .filter(|b| b.kind == Kind::Bytes)
                .ok_or_else(invalid)?
                .bytes
                .get(first)
                .copied()
                .map(|v| Operand::word(v as usize))
                .unwrap_or_else(Operand::unit));
        }
        if op == 6 {
            let end = a[1].as_word().ok_or_else(invalid)?;
            let copied = t.copy(r, Kind::Bytes)?.get(first..end).map(<[u8]>::to_vec);
            return t.rooted(&[r], || {
                copied.map_or(Ok(Operand::unit()), |b| t.make(b, Kind::Bytes))
            });
        }
        Err(invalid())
    }
}
macro_rules! primitive {
    ($name:ident,$op:expr) => {
        unsafe extern "C" fn $name(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
            done((|| unsafe {
                run(state(c)?, r, borrowed_slice(a, n).ok_or_else(invalid)?, $op)
            })())
        }
    };
}
primitive!(from_bytes, 0);
primitive!(bytes, 1);
primitive!(length, 2);
primitive!(clone, 3);
primitive!(at_word, 5);
primitive!(slice_words, 6);
primitive!(append_bytes, 7);
const METHODS: &[(&[u8], Method)] = &[
    (b"fromBytes:", from_bytes),
    (b"bytes", bytes),
    (b"length", length),
    (b"clone", clone),
    (b"new", clone),
    (b"byteAtWord:", at_word),
    (b"sliceFromWord:toWord:", slice_words),
    (b"appendBytes:", append_bytes),
    (b"size", super::numeric::bytes_size),
    (b"at:", super::numeric::bytes_at),
    (b"copyFrom:to:", super::numeric::binary_slice),
    (b"readU32At:", super::numeric::read_u32),
    (b"readU64At:", super::numeric::read_u64),
    (b"sha256", super::numeric::sha256),
    (b"wyhash64", super::numeric::wyhash64),
    (b"readTaggedFieldAt:schema:", super::numeric::tagged_field),
    (
        b"readTaggedRowsWithSchema:limit:",
        super::numeric::tagged_rows,
    ),
    (b"withSymbolName:", super::numeric::with_symbol_name),
    (
        b"readTaggedTableWithSchema:limit:",
        super::numeric::tagged_table,
    ),
    (b"emptyTaggedRows", super::numeric::empty_rows),
    (
        b"readTaggedRecordsAt:count:prefixBytes:fields:schema:",
        super::numeric::tagged_records,
    ),
];
const VIEW_METHODS: &[(&[u8], Method)] = &[
    (b"size", crate::tagged_view::size),
    (b"at:", crate::tagged_view::at),
    (b"sourceIndex", crate::tagged_rows::source_index),
    (b"concat:", crate::tagged_rows::concat),
    (b"copyFrom:to:", crate::tagged_rows::slice),
    (b"whereLocatorTags:", crate::tagged_rows::where_tags),
    (b"whereLocatorData:", crate::tagged_rows::where_data),
    (b"withoutLocatorData:", crate::tagged_rows::without_data),
    (
        b"firstLocatorOutside:",
        crate::tagged_rows::first_locator_outside,
    ),
    (
        b"firstFieldsOutside:",
        crate::tagged_rows::first_fields_outside,
    ),
    (b"firstFieldCountNot:", crate::tagged_rows::first_count_not),
    (
        b"firstIndexForTags:atLeast:",
        crate::tagged_rows::first_index_outside,
    ),
    (b"indexedReferencesForTags:", crate::tagged_rows::indices),
    (
        b"firstFieldOffsetOutside:",
        crate::tagged_rows::first_field_offset,
    ),
    (
        b"firstReferenceOffsetForTags:atLeast:",
        crate::tagged_rows::first_reference_offset,
    ),
    (
        b"fieldReferencesWithOffsetsForTags:",
        crate::tagged_rows::field_references,
    ),
    (
        b"firstPrefixOutside:bytes:",
        crate::tagged_rows::first_prefix,
    ),
];
unsafe fn construct(id: Protocol, namespace: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let named = |name: &[u8]| {
            object(id.send_named(namespace, b"at:", &[Operand::object(id.intern_name(name)?)])?)
        };
        let roots_object = named(b"Roots")?;
        let gc = named(b"GC")?;
        let parent = named(b"ObjectVt")?;
        let mut tokens = Vec::new();
        let mut keep = |v: Oop| -> Result<Oop, Outcome> {
            tokens.push(
                id.send_named(roots_object, b"retain:", &[Operand::object(v)])?
                    .as_word()
                    .ok_or_else(invalid)?,
            );
            Ok(v)
        };
        let behavior = keep(object(id.send_named(parent, b"delegated", &[])?)?)?;
        let view_behavior = keep(object(id.send_named(parent, b"delegated", &[])?)?)?;
        let mut names = Vec::new();
        for name in METHODS.iter().chain(VIEW_METHODS).map(|(n, _)| *n).chain([
            b"rootsDo:".as_slice(),
            b"referencesOf:do:",
            b"finalize:",
            b"trace:with:",
            b"finalize:with:",
            b"register:",
            b"ownerOf:put:",
            b"at:put:",
            b"Bytes",
            b"BytesVt",
        ]) {
            names.push(keep(id.intern_name(name)?)?);
        }
        let prototype = keep(object(id.send_named(
            behavior,
            b"allocate",
            &[Operand::word(2 * size_of::<usize>())],
        )?)?)?;
        let pointer = Box::into_raw(Box::new(Bytes {
            id,
            namespace,
            roots: roots_object,
            gc,
            behavior,
            view_behavior,
            prototype,
            nil: named(b"nil")?,
            buffers: RefCell::new(HashMap::new()),
            views: RefCell::new(HashMap::new()),
            names,
            thread: std::thread::current().id(),
        }));
        let t = &*pointer;
        let b = Buffer {
            bytes: Vec::new(),
            kind: Kind::Bytes,
        };
        mirror(prototype, &b);
        t.buffers.borrow_mut().insert(prototype.addr(), b);
        for (target, methods) in [(behavior, METHODS), (view_behavior, VIEW_METHODS)] {
            for (name, method) in methods.iter().copied().chain([
                (b"rootsDo:".as_slice(), self::roots as Method),
                (b"referencesOf:do:", references),
                (b"finalize:", finalize),
            ]) {
                id.send_named(
                    target,
                    b"addMethod",
                    &[
                        Operand::object(id.intern_name(name)?),
                        Operand::entry(NativeEntry::new(method, pointer.cast())),
                    ],
                )?;
            }
            id.send_named(
                gc,
                b"trace:with:",
                &[Operand::object(target), Operand::object(prototype)],
            )?;
            id.send_named(
                id.receiver,
                b"ownerOf:put:",
                &[Operand::object(target), Operand::object(prototype)],
            )?;
        }
        id.send_named(roots_object, b"register:", &[Operand::object(prototype)])?;
        id.send_named(
            gc,
            b"finalize:with:",
            &[Operand::object(prototype), Operand::object(prototype)],
        )?;
        for (name, value) in [(b"Bytes".as_slice(), prototype), (b"BytesVt", behavior)] {
            id.send_named(
                namespace,
                b"at:put:",
                &[
                    Operand::object(id.intern_name(name)?),
                    Operand::object(value),
                ],
            )?;
        }
        for token in tokens {
            id.send_named(roots_object, b"release:", &[Operand::word(token)])?;
        }
        Ok(Operand::object(prototype))
    }
}
unsafe extern "C" fn initialize(id: *const Protocol, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        if !native::valid_protocol(id) {
            return Err(invalid());
        }
        let [namespace] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let namespace = object(*namespace)?;
        let id = *id;
        let current = id.send_named(
            namespace,
            b"at:",
            &[Operand::object(id.intern_name(b"Bytes")?)],
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
