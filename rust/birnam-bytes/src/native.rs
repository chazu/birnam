//! Byte buffers and decoded views own their storage; only Oops cross Id.
pub(crate) use birnam_abi::{
    self as abi, Oop, borrowed_slice,
    id_protocol::{Literal, MethodSpec, Module, Protocol},
};
pub(crate) use core::cell::RefCell;
use core::{cell::Cell, ptr};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicPtr, Ordering},
};
pub(crate) struct Buffer {
    pub(crate) bytes: Vec<u8>,
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
    names: HashMap<&'static [u8], Oop>,
    scoped: RefCell<Vec<Oop>>,
    ready: Cell<bool>,
}
struct Scope<'a> {
    values: &'a RefCell<Vec<Oop>>,
    start: usize,
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        self.values.borrow_mut().truncate(self.start);
    }
}
static INSTANCE: AtomicPtr<Bytes> = AtomicPtr::new(ptr::null_mut());
static API: AtomicPtr<Protocol> = AtomicPtr::new(ptr::null_mut());
pub(crate) fn invalid() -> Oop {
    let p = API.load(Ordering::Acquire);
    if p.is_null() {
        std::process::abort();
    }
    unsafe { abi::object_reference((*p).invalid) }
}
pub(crate) fn object(v: Oop) -> Result<Oop, Oop> {
    if v.is_null() { Err(invalid()) } else { Ok(v) }
}
pub(crate) fn done(v: Result<Oop, Oop>) -> Oop {
    v.unwrap_or_else(abi::fault_reference)
}
pub(crate) unsafe fn state<'a>(c: Oop) -> Result<&'a Bytes, Oop> {
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
impl Bytes {
    pub(crate) fn selector(&self, name: &[u8]) -> Result<Oop, Oop> {
        self.names.get(name).copied().ok_or_else(invalid)
    }
    pub(crate) unsafe fn send(&self, r: Oop, name: &[u8], a: &[Oop]) -> Result<Oop, Oop> {
        unsafe { self.id.send(r, self.selector(name)?, a) }
    }
    pub(crate) unsafe fn binding(&self, name: &[u8]) -> Result<Oop, Oop> {
        object(unsafe { self.send(self.namespace, b"at:", &[self.selector(name)?]) }?)
    }
    pub(crate) unsafe fn rooted(
        &self,
        values: &[Oop],
        f: impl FnOnce() -> Result<Oop, Oop>,
    ) -> Result<Oop, Oop> {
        let start = self.scoped.borrow().len();
        self.scoped.borrow_mut().extend_from_slice(values);
        let _scope = Scope {
            values: &self.scoped,
            start,
        };
        f()
    }
    pub(crate) fn length(&self, value: Oop) -> Result<usize, Oop> {
        self.buffers
            .borrow()
            .get(&value.addr())
            .map(|b| b.bytes.len())
            .ok_or_else(invalid)
    }
    pub(crate) fn copy(&self, value: Oop) -> Result<Vec<u8>, Oop> {
        self.buffers
            .borrow()
            .get(&value.addr())
            .map(|b| b.bytes.clone())
            .ok_or_else(invalid)
    }
    pub(crate) unsafe fn input_bytes(&self, value: Oop) -> Result<Vec<u8>, Oop> {
        let buffer = unsafe { self.send(value, b"bytes", &[]) }?;
        self.copy(buffer)
    }
    pub(crate) unsafe fn number(&self, value: Oop) -> Result<usize, Oop> {
        if let Some(n) = abi::as_size(value) {
            return Ok(n);
        }
        unsafe {
            let lo = abi::as_size(self.send(value, b"low32", &[])?)
                .filter(|v| *v <= u32::MAX as usize)
                .ok_or_else(invalid)?;
            let hi = abi::as_size(self.send(value, b"high32", &[])?)
                .filter(|v| *v <= u32::MAX as usize)
                .ok_or_else(invalid)?;
            Ok(lo | (hi << 32))
        }
    }
    pub(crate) unsafe fn word(&self, bits: u64) -> Result<Oop, Oop> {
        if let Some(v) = abi::size(bits as usize) {
            return Ok(v);
        }
        unsafe {
            self.send(
                self.binding(b"Word")?,
                b"fromUnsignedLow:high:",
                &[
                    abi::size((bits & u32::MAX as u64) as usize).unwrap(),
                    abi::size((bits >> 32) as usize).unwrap(),
                ],
            )
        }
    }
    unsafe fn charge(&self, bytes: usize) -> Result<(), Oop> {
        if bytes != 0 {
            unsafe {
                self.send(
                    self.gc,
                    b"chargeExternalAllocation:",
                    &[abi::size(bytes).ok_or_else(invalid)?],
                )
            }?;
        }
        Ok(())
    }
    unsafe fn allocate_handle(&self, behavior: Oop) -> Result<Oop, Oop> {
        let value = unsafe { self.send(behavior, b"allocate", &[abi::size(0).unwrap()]) }?;
        unsafe {
            self.rooted(&[value], || {
                if self.send(self.id.root, b"allocationSizeOf:", &[value])? != abi::size(0).unwrap()
                    || self.send(self.id.root, b"behaviorOf:", &[value])? != behavior
                    || self.buffers.borrow().contains_key(&value.addr())
                    || self.views.borrow().contains_key(&value.addr())
                {
                    return Err(invalid());
                }
                Ok(value)
            })
        }
    }
    pub(crate) unsafe fn make(&self, bytes: Vec<u8>) -> Result<Oop, Oop> {
        unsafe {
            let value = self.allocate_handle(self.behavior)?;
            self.rooted(&[value], || {
                let capacity = bytes.capacity();
                self.buffers
                    .borrow_mut()
                    .insert(value.addr(), Buffer { bytes });
                self.send(self.gc, b"finalize:with:", &[value, self.prototype])?;
                self.charge(capacity)?;
                Ok(value)
            })
        }
    }
    pub(crate) unsafe fn view(&self, view: crate::tagged_view::View) -> Result<Oop, Oop> {
        unsafe {
            let value = self.allocate_handle(self.view_behavior)?;
            self.rooted(&[value], || {
                self.views.borrow_mut().insert(value.addr(), view);
                self.send(self.gc, b"finalize:with:", &[value, self.prototype])?;
                Ok(value)
            })
        }
    }
    unsafe fn append(&self, r: Oop, bytes: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            self.send(self.gc, b"prepareAllocation", &[])?;
            let growth = {
                let mut buffers = self.buffers.borrow_mut();
                let b = buffers.get_mut(&r.addr()).ok_or_else(invalid)?;
                let before = b.bytes.capacity();
                b.bytes.extend_from_slice(bytes);
                b.bytes.capacity() - before
            };
            self.charge(growth)?;
            Ok(r)
        }
    }
}
unsafe extern "C" fn roots(_id: Oop, c: Oop, _r: Oop, a: *const Oop, n: usize) -> Oop {
    done((|| unsafe {
        let t = state(c)?;
        let [visitor] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let mut values = vec![
            t.namespace,
            t.roots,
            t.gc,
            t.behavior,
            t.view_behavior,
            t.prototype,
            t.nil,
        ];
        values.extend(t.names.values().copied());
        values.extend(t.scoped.borrow().iter().copied());
        for v in values {
            t.send(*visitor, b"visit:", &[v])?;
        }
        Ok(ptr::null_mut())
    })())
}
unsafe extern "C" fn references(_id: Oop, c: Oop, _r: Oop, a: *const Oop, n: usize) -> Oop {
    done((|| unsafe {
        state(c)?;
        let [_, _] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        // Buffers contain bytes; decoded views contain only native offsets and Arc
        // ownership. Neither format contains managed object references.
        Ok(ptr::null_mut())
    })())
}
unsafe extern "C" fn finalize(_id: Oop, c: Oop, _r: Oop, a: *const Oop, n: usize) -> Oop {
    done((|| unsafe {
        let t = state(c)?;
        let [v] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let bytes = t.buffers.borrow_mut().remove(&v.addr());
        let view = t.views.borrow_mut().remove(&v.addr());
        if bytes.is_none() && view.is_none() {
            return Err(invalid());
        }
        Ok(ptr::null_mut())
    })())
}
unsafe fn run(t: &Bytes, r: Oop, a: &[Oop], op: usize) -> Result<Oop, Oop> {
    unsafe {
        match op {
            0 => {
                let [source] = a else {
                    return Err(invalid());
                };
                let value = t.make(Vec::new())?;
                t.rooted(&[value], || {
                    t.send(*source, b"appendTo:", &[value])?;
                    Ok(value)
                })
            }
            1 => {
                if !a.is_empty() {
                    return Err(invalid());
                }
                t.length(r)?;
                Ok(r)
            }
            2 => {
                if !a.is_empty() {
                    return Err(invalid());
                }
                Ok(abi::size(t.length(r)?).ok_or_else(invalid)?)
            }
            3 => {
                if !a.is_empty() {
                    return Err(invalid());
                }
                t.make(t.copy(r)?)
            }
            5 => {
                let [index] = a else {
                    return Err(invalid());
                };
                let i = t.number(*index)?;
                Ok(t.buffers
                    .borrow()
                    .get(&r.addr())
                    .ok_or_else(invalid)?
                    .bytes
                    .get(i)
                    .copied()
                    .and_then(|b| abi::size(b as usize))
                    .unwrap_or(ptr::null_mut()))
            }
            6 => {
                let [from, to] = a else {
                    return Err(invalid());
                };
                let (from, to) = (t.number(*from)?, t.number(*to)?);
                let bytes = t.copy(r)?.get(from..to).map(<[u8]>::to_vec);
                bytes.map_or(Ok(ptr::null_mut()), |bytes| t.make(bytes))
            }
            7 => {
                let [source] = a else {
                    return Err(invalid());
                };
                if t.buffers.borrow().contains_key(&source.addr()) {
                    let bytes = t.copy(*source)?;
                    t.append(r, &bytes)
                } else {
                    t.send(*source, b"appendTo:", &[r])
                }
            }
            8 | 9 => {
                let bytes = a
                    .iter()
                    .map(|&v| {
                        abi::as_size(v)
                            .and_then(|v| u8::try_from(v).ok())
                            .ok_or_else(invalid)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if op == 8 {
                    t.make(bytes)
                } else {
                    t.append(r, &bytes)
                }
            }
            10 => {
                let [target] = a else {
                    return Err(invalid());
                };
                t.length(r)?;
                t.send(*target, b"appendBytes:", &[r])
            }
            11 => {
                let [callable, receiver, extra @ ..] = a else {
                    return Err(invalid());
                };
                let bytes = t.copy(r)?;
                let mut args = Vec::with_capacity(1 + bytes.len() + extra.len());
                args.push(*receiver);
                args.extend(bytes.iter().map(|&b| abi::size(b as usize).unwrap()));
                args.extend_from_slice(extra);
                t.send(*callable, b"apply", &args)
            }
            _ => Err(invalid()),
        }
    }
}
macro_rules! primitive {
    ($name:ident,$op:expr) => {
        unsafe extern "C" fn $name(_id: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
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
primitive!(from_values, 8);
primitive!(append_values, 9);
primitive!(append_to, 10);
primitive!(apply_bytes, 11);

macro_rules! literals{($($n:literal),*$(,)?)=>{const NAMES:&[&[u8]]=&[$($n),*];static LITERALS:&[Literal]=&[$(Literal::new($n)),*];};}
literals![
    b"allocationSizeOf:",
    b"behaviorOf:",
    b"Bytes",
    b"BytesVt",
    b"GC",
    b"Integer",
    b"ObjectVt",
    b"Roots",
    b"Vector",
    b"Word",
    b"add:",
    b"addMethod",
    b"allocate",
    b"appendByteValues",
    b"appendBytes:",
    b"appendTo:",
    b"apply",
    b"apply:to:with:",
    b"at:",
    b"at:put:",
    b"byteAtWord:",
    b"bytes",
    b"bytes:appendByteValues",
    b"bytes:appendBytes:",
    b"bytes:appendTo:",
    b"bytes:apply:to:with:",
    b"bytes:at:",
    b"bytes:byteAtWord:",
    b"bytes:bytes",
    b"bytes:clone",
    b"bytes:copyFrom:to:",
    b"bytes:emptyTaggedRows",
    b"bytes:fromByteValues",
    b"bytes:fromBytes:",
    b"bytes:length",
    b"bytes:new",
    b"bytes:readTaggedFieldAt:schema:",
    b"bytes:readTaggedRecordsAt:count:prefixBytes:fields:schema:",
    b"bytes:readTaggedRowsWithSchema:limit:",
    b"bytes:readTaggedTableWithSchema:limit:",
    b"bytes:readU32At:",
    b"bytes:readU64At:",
    b"bytes:sha256",
    b"bytes:size",
    b"bytes:sliceFromWord:toWord:",
    b"bytes:withSymbolName:",
    b"bytes:wyhash64",
    b"chargeExternalAllocation:",
    b"clone",
    b"concat:",
    b"copyFrom:to:",
    b"delegated",
    b"emptyTaggedRows",
    b"fieldReferencesWithOffsetsForTags:",
    b"finalize:",
    b"finalize:with:",
    b"firstFieldCountNot:",
    b"firstFieldOffsetOutside:",
    b"firstFieldsOutside:",
    b"firstIndexForTags:atLeast:",
    b"firstLocatorOutside:",
    b"firstPrefixOutside:bytes:",
    b"firstReferenceOffsetForTags:atLeast:",
    b"fromByteValues",
    b"fromBytes:",
    b"fromUnsignedLow:high:",
    b"fromWord:",
    b"high32",
    b"indexedReferencesForTags:",
    b"isSymbol:",
    b"length",
    b"low32",
    b"methodAt:",
    b"new",
    b"nil",
    b"ownerOf:put:",
    b"prepareAllocation",
    b"readTaggedFieldAt:schema:",
    b"readTaggedRecordsAt:count:prefixBytes:fields:schema:",
    b"readTaggedRowsWithSchema:limit:",
    b"readTaggedTableWithSchema:limit:",
    b"readU32At:",
    b"readU64At:",
    b"referencesOf:do:",
    b"register:",
    b"release:",
    b"retain:",
    b"rootsDo:",
    b"sha256",
    b"size",
    b"sliceFromWord:toWord:",
    b"sourceIndex",
    b"trace:with:",
    b"view:at:",
    b"view:concat:",
    b"view:copyFrom:to:",
    b"view:fieldReferencesWithOffsetsForTags:",
    b"view:firstFieldCountNot:",
    b"view:firstFieldOffsetOutside:",
    b"view:firstFieldsOutside:",
    b"view:firstIndexForTags:atLeast:",
    b"view:firstLocatorOutside:",
    b"view:firstPrefixOutside:bytes:",
    b"view:firstReferenceOffsetForTags:atLeast:",
    b"view:indexedReferencesForTags:",
    b"view:size",
    b"view:sourceIndex",
    b"view:whereLocatorData:",
    b"view:whereLocatorTags:",
    b"view:withoutLocatorData:",
    b"visit:",
    b"whereLocatorData:",
    b"whereLocatorTags:",
    b"withContext:",
    b"withSymbolName:",
    b"withoutLocatorData:",
    b"wordOf:",
    b"wyhash64"
];
const fn index(name: &[u8]) -> usize {
    let mut i = 0;
    while i < NAMES.len() {
        if NAMES[i].len() == name.len() {
            let mut j = 0;
            while j < name.len() && NAMES[i][j] == name[j] {
                j += 1;
            }
            if j == name.len() {
                return i;
            }
        }
        i += 1;
    }
    panic!("missing literal")
}
static METHODS: &[MethodSpec] = &[
    MethodSpec {
        name: index(b"bytes:fromBytes:"),
        entry: Some(from_bytes),
    },
    MethodSpec {
        name: index(b"bytes:bytes"),
        entry: Some(bytes),
    },
    MethodSpec {
        name: index(b"bytes:length"),
        entry: Some(length),
    },
    MethodSpec {
        name: index(b"bytes:clone"),
        entry: Some(clone),
    },
    MethodSpec {
        name: index(b"bytes:new"),
        entry: Some(clone),
    },
    MethodSpec {
        name: index(b"bytes:byteAtWord:"),
        entry: Some(at_word),
    },
    MethodSpec {
        name: index(b"bytes:sliceFromWord:toWord:"),
        entry: Some(slice_words),
    },
    MethodSpec {
        name: index(b"bytes:appendBytes:"),
        entry: Some(append_bytes),
    },
    MethodSpec {
        name: index(b"bytes:size"),
        entry: Some(crate::numeric::bytes_size),
    },
    MethodSpec {
        name: index(b"bytes:at:"),
        entry: Some(crate::numeric::bytes_at),
    },
    MethodSpec {
        name: index(b"bytes:copyFrom:to:"),
        entry: Some(crate::numeric::binary_slice),
    },
    MethodSpec {
        name: index(b"bytes:readU32At:"),
        entry: Some(crate::numeric::read_u32),
    },
    MethodSpec {
        name: index(b"bytes:readU64At:"),
        entry: Some(crate::numeric::read_u64),
    },
    MethodSpec {
        name: index(b"bytes:sha256"),
        entry: Some(crate::numeric::sha256),
    },
    MethodSpec {
        name: index(b"bytes:wyhash64"),
        entry: Some(crate::numeric::wyhash64),
    },
    MethodSpec {
        name: index(b"bytes:readTaggedFieldAt:schema:"),
        entry: Some(crate::numeric::tagged_field),
    },
    MethodSpec {
        name: index(b"bytes:readTaggedRowsWithSchema:limit:"),
        entry: Some(crate::numeric::tagged_rows),
    },
    MethodSpec {
        name: index(b"bytes:withSymbolName:"),
        entry: Some(crate::numeric::with_symbol_name),
    },
    MethodSpec {
        name: index(b"bytes:readTaggedTableWithSchema:limit:"),
        entry: Some(crate::numeric::tagged_table),
    },
    MethodSpec {
        name: index(b"bytes:emptyTaggedRows"),
        entry: Some(crate::numeric::empty_rows),
    },
    MethodSpec {
        name: index(b"bytes:readTaggedRecordsAt:count:prefixBytes:fields:schema:"),
        entry: Some(crate::numeric::tagged_records),
    },
    MethodSpec {
        name: index(b"bytes:fromByteValues"),
        entry: Some(from_values),
    },
    MethodSpec {
        name: index(b"bytes:appendByteValues"),
        entry: Some(append_values),
    },
    MethodSpec {
        name: index(b"bytes:appendTo:"),
        entry: Some(append_to),
    },
    MethodSpec {
        name: index(b"bytes:apply:to:with:"),
        entry: Some(apply_bytes),
    },
    MethodSpec {
        name: index(b"view:size"),
        entry: Some(crate::tagged_view::size),
    },
    MethodSpec {
        name: index(b"view:at:"),
        entry: Some(crate::tagged_view::at),
    },
    MethodSpec {
        name: index(b"view:sourceIndex"),
        entry: Some(crate::tagged_rows::source_index),
    },
    MethodSpec {
        name: index(b"view:concat:"),
        entry: Some(crate::tagged_rows::concat),
    },
    MethodSpec {
        name: index(b"view:copyFrom:to:"),
        entry: Some(crate::tagged_rows::slice),
    },
    MethodSpec {
        name: index(b"view:whereLocatorTags:"),
        entry: Some(crate::tagged_rows::where_tags),
    },
    MethodSpec {
        name: index(b"view:whereLocatorData:"),
        entry: Some(crate::tagged_rows::where_data),
    },
    MethodSpec {
        name: index(b"view:withoutLocatorData:"),
        entry: Some(crate::tagged_rows::without_data),
    },
    MethodSpec {
        name: index(b"view:firstLocatorOutside:"),
        entry: Some(crate::tagged_rows::first_locator_outside),
    },
    MethodSpec {
        name: index(b"view:firstFieldsOutside:"),
        entry: Some(crate::tagged_rows::first_fields_outside),
    },
    MethodSpec {
        name: index(b"view:firstFieldCountNot:"),
        entry: Some(crate::tagged_rows::first_count_not),
    },
    MethodSpec {
        name: index(b"view:firstIndexForTags:atLeast:"),
        entry: Some(crate::tagged_rows::first_index_outside),
    },
    MethodSpec {
        name: index(b"view:indexedReferencesForTags:"),
        entry: Some(crate::tagged_rows::indices),
    },
    MethodSpec {
        name: index(b"view:firstFieldOffsetOutside:"),
        entry: Some(crate::tagged_rows::first_field_offset),
    },
    MethodSpec {
        name: index(b"view:firstReferenceOffsetForTags:atLeast:"),
        entry: Some(crate::tagged_rows::first_reference_offset),
    },
    MethodSpec {
        name: index(b"view:fieldReferencesWithOffsetsForTags:"),
        entry: Some(crate::tagged_rows::field_references),
    },
    MethodSpec {
        name: index(b"view:firstPrefixOutside:bytes:"),
        entry: Some(crate::tagged_rows::first_prefix),
    },
    MethodSpec {
        name: index(b"rootsDo:"),
        entry: Some(roots),
    },
    MethodSpec {
        name: index(b"referencesOf:do:"),
        entry: Some(references),
    },
    MethodSpec {
        name: index(b"finalize:"),
        entry: Some(finalize),
    },
];
const BINDINGS: &[(bool, &[u8], &[u8])] = &[
    (false, b"fromBytes:", b"bytes:fromBytes:"),
    (false, b"bytes", b"bytes:bytes"),
    (false, b"length", b"bytes:length"),
    (false, b"clone", b"bytes:clone"),
    (false, b"new", b"bytes:new"),
    (false, b"byteAtWord:", b"bytes:byteAtWord:"),
    (
        false,
        b"sliceFromWord:toWord:",
        b"bytes:sliceFromWord:toWord:",
    ),
    (false, b"appendBytes:", b"bytes:appendBytes:"),
    (false, b"size", b"bytes:size"),
    (false, b"at:", b"bytes:at:"),
    (false, b"copyFrom:to:", b"bytes:copyFrom:to:"),
    (false, b"readU32At:", b"bytes:readU32At:"),
    (false, b"readU64At:", b"bytes:readU64At:"),
    (false, b"sha256", b"bytes:sha256"),
    (false, b"wyhash64", b"bytes:wyhash64"),
    (
        false,
        b"readTaggedFieldAt:schema:",
        b"bytes:readTaggedFieldAt:schema:",
    ),
    (
        false,
        b"readTaggedRowsWithSchema:limit:",
        b"bytes:readTaggedRowsWithSchema:limit:",
    ),
    (false, b"withSymbolName:", b"bytes:withSymbolName:"),
    (
        false,
        b"readTaggedTableWithSchema:limit:",
        b"bytes:readTaggedTableWithSchema:limit:",
    ),
    (false, b"emptyTaggedRows", b"bytes:emptyTaggedRows"),
    (
        false,
        b"readTaggedRecordsAt:count:prefixBytes:fields:schema:",
        b"bytes:readTaggedRecordsAt:count:prefixBytes:fields:schema:",
    ),
    (false, b"fromByteValues", b"bytes:fromByteValues"),
    (false, b"appendByteValues", b"bytes:appendByteValues"),
    (false, b"appendTo:", b"bytes:appendTo:"),
    (false, b"apply:to:with:", b"bytes:apply:to:with:"),
    (true, b"size", b"view:size"),
    (true, b"at:", b"view:at:"),
    (true, b"sourceIndex", b"view:sourceIndex"),
    (true, b"concat:", b"view:concat:"),
    (true, b"copyFrom:to:", b"view:copyFrom:to:"),
    (true, b"whereLocatorTags:", b"view:whereLocatorTags:"),
    (true, b"whereLocatorData:", b"view:whereLocatorData:"),
    (true, b"withoutLocatorData:", b"view:withoutLocatorData:"),
    (true, b"firstLocatorOutside:", b"view:firstLocatorOutside:"),
    (true, b"firstFieldsOutside:", b"view:firstFieldsOutside:"),
    (true, b"firstFieldCountNot:", b"view:firstFieldCountNot:"),
    (
        true,
        b"firstIndexForTags:atLeast:",
        b"view:firstIndexForTags:atLeast:",
    ),
    (
        true,
        b"indexedReferencesForTags:",
        b"view:indexedReferencesForTags:",
    ),
    (
        true,
        b"firstFieldOffsetOutside:",
        b"view:firstFieldOffsetOutside:",
    ),
    (
        true,
        b"firstReferenceOffsetForTags:atLeast:",
        b"view:firstReferenceOffsetForTags:atLeast:",
    ),
    (
        true,
        b"fieldReferencesWithOffsetsForTags:",
        b"view:fieldReferencesWithOffsetsForTags:",
    ),
    (
        true,
        b"firstPrefixOutside:bytes:",
        b"view:firstPrefixOutside:bytes:",
    ),
    (false, b"rootsDo:", b"rootsDo:"),
    (false, b"referencesOf:do:", b"referencesOf:do:"),
    (false, b"finalize:", b"finalize:"),
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
        let t = unsafe { &*old };
        return if t.ready.get() && t.namespace == *namespace {
            t.prototype
        } else {
            id.invalid
        };
    }
    done((|| unsafe {
        let names: HashMap<_, _> = NAMES.iter().copied().zip(symbols.iter().copied()).collect();
        let send = |r, n: &[u8], a: &[Oop]| id.send(r, names[n], a);
        let named = |name: &[u8]| object(send(*namespace, b"at:", &[names[name]])?);
        let roots = named(b"Roots")?;
        let gc = named(b"GC")?;
        let parent = named(b"ObjectVt")?;
        let nil = named(b"nil")?;
        let mut tokens = Vec::new();
        let result = (|| {
            let mut keep = |v| -> Result<Oop, Oop> {
                tokens.push(send(roots, b"retain:", &[v])?);
                Ok(v)
            };
            let behavior = keep(send(parent, b"delegated", &[])?)?;
            let view_behavior = keep(send(parent, b"delegated", &[])?)?;
            let prototype = keep(send(behavior, b"allocate", &[abi::size(0).unwrap()])?)?;
            let t = Box::into_raw(Box::new(Bytes {
                id,
                namespace: *namespace,
                roots,
                gc,
                behavior,
                view_behavior,
                prototype,
                nil,
                names: names.clone(),
                buffers: RefCell::new(HashMap::from([(
                    prototype.addr(),
                    Buffer { bytes: Vec::new() },
                )])),
                views: RefCell::new(HashMap::new()),
                scoped: RefCell::new(Vec::new()),
                ready: Cell::new(false),
            }));
            INSTANCE.store(t, Ordering::Release);
            let t = &*t;
            for &(view, name, implementation) in BINDINGS {
                let method = t.send(image, b"methodAt:", &[t.selector(implementation)?])?;
                let method = t.send(method, b"withContext:", &[prototype])?;
                let target = if view { view_behavior } else { behavior };
                t.send(target, b"addMethod", &[t.selector(name)?, method])?;
                if matches!(name, b"rootsDo:" | b"referencesOf:do:" | b"finalize:") {
                    t.send(view_behavior, b"addMethod", &[t.selector(name)?, method])?;
                }
            }
            t.send(roots, b"register:", &[prototype])?;
            for target in [behavior, view_behavior] {
                t.send(gc, b"trace:with:", &[target, prototype])?;
                t.send(id.root, b"ownerOf:put:", &[target, prototype])?;
            }
            t.send(gc, b"finalize:with:", &[prototype, prototype])?;
            for (name, value) in [(b"Bytes".as_slice(), prototype), (b"BytesVt", behavior)] {
                t.send(*namespace, b"at:put:", &[t.selector(name)?, value])?;
            }
            t.ready.set(true);
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
