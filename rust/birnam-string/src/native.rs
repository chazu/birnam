//! String owns its bytes. Id sees only object references and allocation facts.
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
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    String,
    Builder,
}
struct Buffer {
    bytes: Vec<u8>,
    kind: Kind,
}
struct Text {
    id: Protocol,
    namespace: Oop,
    roots: Oop,
    gc: Oop,
    image: Oop,
    behavior: Oop,
    builder_behavior: Oop,
    prototype: Oop,
    builder: Cell<Oop>,
    truth: Oop,
    falsity: Oop,
    nil: Oop,
    names: HashMap<&'static [u8], Oop>,
    buffers: RefCell<HashMap<usize, Buffer>>,
    scoped: RefCell<Vec<Oop>>,
    numeric: Cell<bool>,
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
static INSTANCE: AtomicPtr<Text> = AtomicPtr::new(ptr::null_mut());
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
unsafe fn instance<'a>() -> Result<&'a Text, Oop> {
    let p = INSTANCE.load(Ordering::Acquire);
    if p.is_null() {
        Err(invalid())
    } else {
        Ok(unsafe { &*p })
    }
}
unsafe fn state<'a>(c: Oop) -> Result<&'a Text, Oop> {
    let t = unsafe { instance()? };
    if c == t.prototype {
        Ok(t)
    } else {
        Err(invalid())
    }
}
impl Text {
    fn selector(&self, n: &[u8]) -> Result<Oop, Oop> {
        self.names.get(n).copied().ok_or_else(invalid)
    }
    unsafe fn send(&self, r: Oop, n: &[u8], a: &[Oop]) -> Result<Oop, Oop> {
        unsafe { self.id.send(r, self.selector(n)?, a) }
    }
    unsafe fn binding(&self, n: &[u8]) -> Result<Oop, Oop> {
        object(unsafe { self.send(self.namespace, b"at:", &[self.selector(n)?]) }?)
    }
    unsafe fn rooted(
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
    fn length(&self, v: Oop, kind: Kind) -> Result<usize, Oop> {
        self.buffers
            .borrow()
            .get(&v.addr())
            .filter(|b| b.kind == kind)
            .map(|b| b.bytes.len())
            .ok_or_else(invalid)
    }
    fn copy(&self, v: Oop, kind: Kind) -> Result<Vec<u8>, Oop> {
        self.buffers
            .borrow()
            .get(&v.addr())
            .filter(|b| b.kind == kind)
            .map(|b| b.bytes.clone())
            .ok_or_else(invalid)
    }
    unsafe fn charge(&self, n: usize) -> Result<(), Oop> {
        if n != 0 {
            unsafe {
                self.send(
                    self.gc,
                    b"chargeExternalAllocation:",
                    &[abi::size(n).ok_or_else(invalid)?],
                )
            }?;
        }
        Ok(())
    }
    unsafe fn make(&self, bytes: Vec<u8>, kind: Kind) -> Result<Oop, Oop> {
        unsafe {
            let behavior = if kind == Kind::String {
                self.behavior
            } else {
                self.builder_behavior
            };
            let v = self.send(behavior, b"allocate", &[abi::size(0).unwrap()])?;
            self.rooted(&[v], || {
                if self.send(self.id.root, b"allocationSizeOf:", &[v])? != abi::size(0).unwrap()
                    || self.send(self.id.root, b"behaviorOf:", &[v])? != behavior
                    || self.buffers.borrow().contains_key(&v.addr())
                {
                    return Err(invalid());
                }
                let capacity = bytes.capacity();
                self.buffers
                    .borrow_mut()
                    .insert(v.addr(), Buffer { bytes, kind });
                self.send(self.gc, b"finalize:with:", &[v, self.prototype])?;
                self.charge(capacity)?;
                Ok(v)
            })
        }
    }
    unsafe fn append(&self, r: Oop, bytes: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            self.length(r, Kind::Builder)?;
            self.send(self.gc, b"prepareAllocation", &[])?;
            let growth = {
                let mut buffers = self.buffers.borrow_mut();
                let b = buffers
                    .get_mut(&r.addr())
                    .filter(|b| b.kind == Kind::Builder)
                    .ok_or_else(invalid)?;
                let before = b.bytes.capacity();
                b.bytes.extend_from_slice(bytes);
                b.bytes.capacity() - before
            };
            self.charge(growth)?;
            Ok(r)
        }
    }
    unsafe fn string(&self, bytes: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            // Keep the established single-provider constructor protocol replaceable.
            let values: Vec<_> = bytes
                .iter()
                .map(|&b| abi::size(b as usize).unwrap())
                .collect();
            let source = self.send(self.id.root, b"sequence", &values)?;
            self.send(self.prototype, b"fromBytes:", &[source])
        }
    }
    unsafe fn from_provider(&self, provider: Oop) -> Result<Oop, Oop> {
        unsafe {
            let bytes = self.id.read_bytes(
                provider,
                self.selector(b"sizeWord")?,
                self.selector(b"atWord:")?,
            )?;
            self.make(bytes, Kind::String)
        }
    }
    unsafe fn printable(&self, value: Oop) -> Result<Vec<u8>, Oop> {
        unsafe {
            if let Ok(bytes) = self.copy(value, Kind::String) {
                return Ok(bytes);
            }
            let printed = self.send(value, b"printString", &[])?;
            let mut result = None;
            self.rooted(&[printed], || {
                if let Ok(bytes) = self.copy(printed, Kind::String) {
                    result = Some(bytes);
                    return Ok(ptr::null_mut());
                }
                let provider = self.send(printed, b"bytes", &[])?;
                let text = self.from_provider(provider)?;
                result = Some(self.copy(text, Kind::String)?);
                Ok(ptr::null_mut())
            })?;
            result.ok_or_else(invalid)
        }
    }
    unsafe fn method(&self, name: &[u8], context: Oop) -> Result<Oop, Oop> {
        unsafe {
            let m = self.send(self.image, b"methodAt:", &[self.selector(name)?])?;
            self.send(m, b"withContext:", &[context])
        }
    }
    unsafe fn word(&self, bits: u64) -> Result<Oop, Oop> {
        unsafe {
            if let Some(v) = abi::size(bits as usize) {
                return Ok(v);
            }
            self.send(
                self.binding(b"Word")?,
                b"fromUnsignedLow:high:",
                &[
                    abi::size(bits as u32 as usize).unwrap(),
                    abi::size((bits >> 32) as usize).unwrap(),
                ],
            )
        }
    }
    unsafe fn number(&self, v: Oop) -> Result<u64, Oop> {
        unsafe {
            if let Some(n) = abi::as_size(v) {
                return Ok(n as u64);
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
}
unsafe extern "C" fn roots(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    done((|| unsafe {
        let t = state(c)?;
        let [visitor] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let mut values = vec![
            t.namespace,
            t.roots,
            t.gc,
            t.image,
            t.behavior,
            t.builder_behavior,
            t.prototype,
            t.builder.get(),
            t.truth,
            t.falsity,
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
unsafe extern "C" fn references(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    done((|| unsafe {
        state(c)?;
        let [_, _] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        // Owned byte arrays have no managed references. Method contexts are traced by Method.
        Ok(ptr::null_mut())
    })())
}
unsafe extern "C" fn finalize(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    done((|| unsafe {
        let t = state(c)?;
        let [v] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        t.buffers
            .borrow_mut()
            .remove(&v.addr())
            .ok_or_else(invalid)?;
        Ok(ptr::null_mut())
    })())
}
unsafe fn run(t: &Text, r: Oop, a: &[Oop], op: usize) -> Result<Oop, Oop> {
    unsafe {
        let expected = match op {
            0 | 5 | 6 | 11 | 13 | 17 => Some(1),
            8 => Some(2),

            _ => Some(0),
        };
        if expected.is_some_and(|n| a.len() != n) {
            return Err(invalid());
        }
        match op {
            0 => t.from_provider(a[0]),
            1 => {
                t.length(r, Kind::String)?;
                t.send(t.binding(b"Bytes")?, b"fromBytes:", &[r])
            }
            2 => abi::size(t.length(r, Kind::String)?).ok_or_else(invalid),
            3 => t.make(t.copy(r, Kind::String)?, Kind::String),
            4 => {
                t.length(r, Kind::String)?;
                Ok(r)
            }
            5 => {
                let mut bytes = t.copy(r, Kind::String)?;
                bytes.extend_from_slice(&t.printable(a[0])?);
                t.string(&bytes)
            }
            6 => {
                let left = t.copy(r, Kind::String)?;
                Ok(
                    if t.copy(a[0], Kind::String).is_ok_and(|right| left == right) {
                        t.truth
                    } else {
                        t.falsity
                    },
                )
            }
            7 => t.string(crate::text::trimmed_bytes(&t.copy(r, Kind::String)?)),
            8 => {
                let bytes = t.copy(r, Kind::String)?;
                let (Ok(needle), Ok(replacement)) =
                    (t.copy(a[0], Kind::String), t.copy(a[1], Kind::String))
                else {
                    return Ok(t.nil);
                };
                t.string(&crate::text::replace(&bytes, &needle, &replacement))
            }
            9 => {
                let bytes = t.copy(r, Kind::String)?;
                let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
                let values: Vec<_> = bytes[..end]
                    .iter()
                    .map(|&b| abi::size(b as usize).unwrap())
                    .collect();
                let name = t.send(t.id.root, b"nameWithBytes", &values)?;
                t.id.send(name, t.id.intern, &[])
            }
            10 => {
                t.length(r, Kind::Builder)?;
                t.make(Vec::new(), Kind::Builder)
            }
            11 => {
                let bytes = t.printable(a[0])?;
                t.append(r, &bytes)
            }
            12 => t.string(&t.copy(r, Kind::Builder)?),
            13 => {
                let index = abi::as_size(a[0]).ok_or_else(invalid)?;
                let buffers = t.buffers.borrow();
                let b = buffers
                    .get(&r.addr())
                    .filter(|b| b.kind == Kind::String)
                    .ok_or_else(invalid)?;
                b.bytes
                    .get(index)
                    .and_then(|&v| abi::size(v as usize))
                    .ok_or_else(invalid)
            }
            16 => {
                let hash = if t.send(t.id.root, b"behaviorOf:", &[r])? != t.behavior {
                    mix(r.addr() as u64)
                } else {
                    t.copy(r, Kind::String)?
                        .iter()
                        .fold(2_166_136_261u32, |h, &b| {
                            (h ^ u32::from(b)).wrapping_mul(16_777_619)
                        }) as usize
                };
                Ok(abi::size(hash).unwrap())
            }
            17 => {
                let equal = r == a[0]
                    || (t.send(t.id.root, b"behaviorOf:", &[r])? == t.behavior
                        && t.send(t.id.root, b"behaviorOf:", &[a[0]])? == t.behavior
                        && t.copy(r, Kind::String)? == t.copy(a[0], Kind::String)?);
                Ok(if equal { t.truth } else { t.falsity })
            }
            _ => Err(invalid()),
        }
    }
}
fn mix(mut v: u64) -> usize {
    v = v.wrapping_add(0x9E3779B97F4A7C15);
    v = (v ^ (v >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    v = (v ^ (v >> 27)).wrapping_mul(0x94D049BB133111EB);
    (v ^ (v >> 31)) as u32 as usize
}
macro_rules! entry {
    ($name:ident,$op:expr) => {
        unsafe extern "C" fn $name(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
            done((|| unsafe {
                run(state(c)?, r, borrowed_slice(a, n).ok_or_else(invalid)?, $op)
            })())
        }
    };
}
entry!(from_bytes, 0);
entry!(bytes, 1);
entry!(length, 2);
entry!(clone_string, 3);
entry!(print, 4);
entry!(comma, 5);
entry!(equals, 6);
entry!(trimmed, 7);
entry!(replace, 8);
entry!(as_symbol, 9);
entry!(new_builder, 10);
entry!(add, 11);
entry!(as_string, 12);
entry!(collection_hash, 16);
entry!(collection_equal, 17);
entry!(byte_value, 13);
#[path = "numeric.rs"]
mod numeric;
#[path = "split.rs"]
mod split;

macro_rules! literals{($($n:literal),*$(,)?)=>{const NAMES:&[&[u8]]=&[$($n),*];static LITERALS:&[Literal]=&[$(Literal::new($n)),*];};}
literals![
    b",",
    b"=",
    b"Bytes",
    b"GC",
    b"ObjectVt",
    b"Roots",
    b"String",
    b"StringBuilder",
    b"StringBuilderVt",
    b"StringVt",
    b"Word",
    b"add",
    b"add:",
    b"addByte:",
    b"addMethod",
    b"allocate",
    b"allocationSizeOf:",
    b"appendByteValues",
    b"asBytes",
    b"asInteger",
    b"asNumber",
    b"asString",
    b"asSymbol",
    b"as_string",
    b"as_symbol",
    b"at:",
    b"at:put:",
    b"behaviorOf:",
    b"bitsOf:",
    b"bracketDepth",
    b"byteAt:",
    b"bytes",
    b"chargeExternalAllocation:",
    b"clone",
    b"clone_string",
    b"collectionEquals:",
    b"collectionHash",
    b"collection_equal",
    b"collection_hash",
    b"comma",
    b"copyFrom:to:",
    b"delegated",
    b"equals",
    b"false",
    b"finalize",
    b"finalize:",
    b"finalize:with:",
    b"fromBits:",
    b"fromByteValues",
    b"fromBytes:",
    b"fromUnsignedLow:high:",
    b"fromWord:",
    b"from_bytes",
    b"high32",
    b"indexOf:",
    b"installCollections:",
    b"installNumbers:",
    b"length",
    b"lookup",
    b"low32",
    b"methodAt:",
    b"nameWithBytes",
    b"new",
    b"new_builder",
    b"nil",
    b"numeric::add_byte",
    b"numeric::as_integer",
    b"numeric::as_number",
    b"numeric::at",
    b"numeric::builder_size",
    b"numeric::byte_at",
    b"numeric::copy",
    b"numeric::depth",
    b"numeric::index_of",
    b"numeric::install",
    b"numeric::size",
    b"ownerOf:put:",
    b"prepareAllocation",
    b"print",
    b"printString",
    b"references",
    b"referencesOf:do:",
    b"register:",
    b"release:",
    b"replace",
    b"replace:with:",
    b"retain:",
    b"roots",
    b"rootsDo:",
    b"sequence",
    b"size",
    b"sizeWord",
    b"atWord:",
    b"byte_value",
    b"split:",
    b"split::install",
    b"split::split",
    b"trace:with:",
    b"trimmed",
    b"true",
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
    MethodSpec {
        name: index(b"byte_value"),
        entry: Some(byte_value),
    },
    MethodSpec {
        name: index(b"numeric::install"),
        entry: Some(numeric::install),
    },
    MethodSpec {
        name: index(b"split::install"),
        entry: Some(split::install),
    },
    MethodSpec {
        name: index(b"from_bytes"),
        entry: Some(from_bytes),
    },
    MethodSpec {
        name: index(b"bytes"),
        entry: Some(bytes),
    },
    MethodSpec {
        name: index(b"length"),
        entry: Some(length),
    },
    MethodSpec {
        name: index(b"clone_string"),
        entry: Some(clone_string),
    },
    MethodSpec {
        name: index(b"print"),
        entry: Some(print),
    },
    MethodSpec {
        name: index(b"comma"),
        entry: Some(comma),
    },
    MethodSpec {
        name: index(b"equals"),
        entry: Some(equals),
    },
    MethodSpec {
        name: index(b"collection_equal"),
        entry: Some(collection_equal),
    },
    MethodSpec {
        name: index(b"collection_hash"),
        entry: Some(collection_hash),
    },
    MethodSpec {
        name: index(b"trimmed"),
        entry: Some(trimmed),
    },
    MethodSpec {
        name: index(b"replace"),
        entry: Some(replace),
    },
    MethodSpec {
        name: index(b"as_symbol"),
        entry: Some(as_symbol),
    },
    MethodSpec {
        name: index(b"roots"),
        entry: Some(roots),
    },
    MethodSpec {
        name: index(b"references"),
        entry: Some(references),
    },
    MethodSpec {
        name: index(b"finalize"),
        entry: Some(finalize),
    },
    MethodSpec {
        name: index(b"new_builder"),
        entry: Some(new_builder),
    },
    MethodSpec {
        name: index(b"add"),
        entry: Some(add),
    },
    MethodSpec {
        name: index(b"as_string"),
        entry: Some(as_string),
    },
    MethodSpec {
        name: index(b"numeric::size"),
        entry: Some(numeric::size),
    },
    MethodSpec {
        name: index(b"numeric::at"),
        entry: Some(numeric::at),
    },
    MethodSpec {
        name: index(b"numeric::byte_at"),
        entry: Some(numeric::byte_at),
    },
    MethodSpec {
        name: index(b"numeric::copy"),
        entry: Some(numeric::copy),
    },
    MethodSpec {
        name: index(b"numeric::index_of"),
        entry: Some(numeric::index_of),
    },
    MethodSpec {
        name: index(b"numeric::depth"),
        entry: Some(numeric::depth),
    },
    MethodSpec {
        name: index(b"numeric::as_integer"),
        entry: Some(numeric::as_integer),
    },
    MethodSpec {
        name: index(b"numeric::as_number"),
        entry: Some(numeric::as_number),
    },
    MethodSpec {
        name: index(b"numeric::builder_size"),
        entry: Some(numeric::builder_size),
    },
    MethodSpec {
        name: index(b"numeric::add_byte"),
        entry: Some(numeric::add_byte),
    },
    MethodSpec {
        name: index(b"split::split"),
        entry: Some(split::split),
    },
];
const BINDINGS: &[(bool, &[u8], &[u8])] = &[
    (false, b"sizeWord", b"length"),
    (false, b"atWord:", b"byte_value"),
    (false, b"installNumbers:", b"numeric::install"),
    (false, b"installCollections:", b"split::install"),
    (false, b"fromBytes:", b"from_bytes"),
    (false, b"asBytes", b"bytes"),
    (false, b"bytes", b"bytes"),
    (false, b"length", b"length"),
    (false, b"clone", b"clone_string"),
    (false, b"new", b"clone_string"),
    (false, b"printString", b"print"),
    (false, b",", b"comma"),
    (false, b"=", b"equals"),
    (false, b"collectionEquals:", b"collection_equal"),
    (false, b"collectionHash", b"collection_hash"),
    (false, b"trimmed", b"trimmed"),
    (false, b"replace:with:", b"replace"),
    (false, b"asSymbol", b"as_symbol"),
    (false, b"rootsDo:", b"roots"),
    (false, b"referencesOf:do:", b"references"),
    (false, b"finalize:", b"finalize"),
    (true, b"new", b"new_builder"),
    (true, b"clone", b"new_builder"),
    (true, b"add:", b"add"),
    (true, b"asString", b"as_string"),
    (true, b"printString", b"as_string"),
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
        let named = |n: &[u8]| object(send(*namespace, b"at:", &[names[n]])?);
        let roots = named(b"Roots")?;
        let gc = named(b"GC")?;
        let parent = named(b"ObjectVt")?;
        let truth = named(b"true")?;
        let falsity = named(b"false")?;
        let nil = named(b"nil")?;
        let mut tokens = Vec::new();
        let result = (|| {
            let mut keep = |v| -> Result<Oop, Oop> {
                tokens.push(send(roots, b"retain:", &[v])?);
                Ok(v)
            };
            let behavior = keep(send(parent, b"delegated", &[])?)?;
            let builder_behavior = keep(send(parent, b"delegated", &[])?)?;
            let prototype = keep(send(behavior, b"allocate", &[abi::size(0).unwrap()])?)?;
            if send(id.root, b"allocationSizeOf:", &[prototype])? != abi::size(0).unwrap()
                || send(id.root, b"behaviorOf:", &[prototype])? != behavior
            {
                return Err(invalid());
            }
            let t = Box::into_raw(Box::new(Text {
                id,
                namespace: *namespace,
                roots,
                gc,
                image,
                behavior,
                builder_behavior,
                prototype,
                builder: Cell::new(ptr::null_mut()),
                truth,
                falsity,
                nil,
                names: names.clone(),
                buffers: RefCell::new(HashMap::from([(
                    prototype.addr(),
                    Buffer {
                        bytes: Vec::new(),
                        kind: Kind::String,
                    },
                )])),
                scoped: RefCell::new(Vec::new()),
                numeric: Cell::new(false),
                ready: Cell::new(false),
            }));
            INSTANCE.store(t, Ordering::Release);
            let t = &*t;
            for &(builder, name, implementation) in BINDINGS {
                let method = t.method(implementation, prototype)?;
                t.send(
                    if builder { builder_behavior } else { behavior },
                    b"addMethod",
                    &[t.selector(name)?, method],
                )?;
            }
            t.send(roots, b"register:", &[prototype])?;
            for target in [behavior, builder_behavior] {
                t.send(gc, b"trace:with:", &[target, prototype])?;
                t.send(id.root, b"ownerOf:put:", &[target, prototype])?;
            }
            t.send(gc, b"finalize:with:", &[prototype, prototype])?;
            t.builder.set(t.make(Vec::new(), Kind::Builder)?);
            for (name, v) in [
                (b"String".as_slice(), prototype),
                (b"StringVt", behavior),
                (b"StringBuilder", t.builder.get()),
                (b"StringBuilderVt", builder_behavior),
            ] {
                t.send(*namespace, b"at:put:", &[t.selector(name)?, v])?;
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
