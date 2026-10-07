pub(crate) use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as native, Method, NativeEntry, Operand, Outcome, Protocol},
};
pub(crate) use core::{
    cell::{Cell, RefCell},
    ffi::c_void,
    ptr,
};
use std::collections::HashMap;

// Text owns only its Bytes reference and kind. Mutable payload mirrors never
// authorize access to byte storage; that owner is reached through Id messages.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    String,
    Builder,
}
struct Buffer {
    storage: Oop,
    kind: Kind,
}
pub(crate) struct Text {
    pub(crate) id: Protocol,
    roots: Oop,
    pub(crate) namespace: Oop,
    gc: Oop,
    behavior: Oop,
    builder_behavior: Oop,
    prototype: Cell<Oop>,
    builder: Cell<Oop>,
    truth: Oop,
    falsity: Oop,
    pub(crate) nil: Oop,
    names: HashMap<&'static [u8], Oop>,
    buffers: RefCell<HashMap<usize, Buffer>>,
    numeric: Cell<bool>,
    thread: std::thread::ThreadId,
}
pub(crate) fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
pub(crate) fn object(v: Operand) -> Result<Oop, Outcome> {
    v.as_object().filter(|p| !p.is_null()).ok_or_else(invalid)
}
pub(crate) fn done(v: Result<Operand, Outcome>) -> Outcome {
    v.map_or_else(|e| e, Outcome::loaded)
}
pub(crate) unsafe fn state<'a>(context: *mut c_void) -> Result<&'a Text, Outcome> {
    let t = unsafe { &*context.cast::<Text>() };
    if t.thread != std::thread::current().id() {
        return Err(invalid());
    }
    Ok(t)
}
impl Text {
    unsafe fn send(
        &self,
        receiver: Oop,
        name: &'static [u8],
        args: &[Operand],
    ) -> Result<Operand, Outcome> {
        unsafe { self.id.send(receiver, self.names[name], args) }
    }
    pub(crate) unsafe fn rooted(
        &self,
        values: &[Oop],
        f: impl FnOnce() -> Result<Operand, Outcome>,
    ) -> Result<Operand, Outcome> {
        let mut tokens = Vec::new();
        let result = (|| unsafe {
            for value in values {
                tokens.push(
                    self.send(self.roots, b"retain:", &[Operand::object(*value)])?
                        .as_word()
                        .filter(|n| *n != 0)
                        .ok_or_else(invalid)?,
                );
            }
            f()
        })();
        for token in tokens {
            if unsafe { self.send(self.roots, b"release:", &[Operand::word(token)]) }?.as_word()
                != Some(1)
            {
                return Err(invalid());
            }
        }
        result
    }
    fn storage(&self, value: Oop, kind: Kind) -> Result<Oop, Outcome> {
        self.buffers.borrow().get(&value.addr()).filter(|b| b.kind == kind).map(|b| b.storage).ok_or_else(invalid)
    }
    unsafe fn length(&self, value: Oop, kind: Kind) -> Result<usize, Outcome> {
        unsafe { self.id.send_named(self.storage(value, kind)?, b"length", &[]) }?.as_word().ok_or_else(invalid)
    }
    pub(crate) unsafe fn copy(&self, value: Oop, kind: Kind) -> Result<Vec<u8>, Outcome> {
        let raw = unsafe { self.id.send_named(self.storage(value, kind)?, b"bytes", &[]) }?.as_bytes().ok_or_else(invalid)?;
        Ok(unsafe { borrowed_bytes(raw.pointer, raw.length) }.ok_or_else(invalid)?.to_vec())
    }
    unsafe fn byte_factory(&self) -> Result<Oop, Outcome> {
        unsafe { object(self.id.send_named(self.namespace, b"at:", &[Operand::object(self.names[b"Bytes".as_slice()])])?) }
    }
    pub(crate) unsafe fn make(&self, bytes: Vec<u8>, kind: Kind) -> Result<Operand, Outcome> {
        unsafe {
            let storage = object(self.id.send_named(self.byte_factory()?, b"fromBytes:", &[Operand::bytes(&bytes)])?)?;
            self.rooted(&[storage], || {
                let behavior = if kind == Kind::String { self.behavior } else { self.builder_behavior };
                let extent = if kind == Kind::Builder { 3 } else { 2 } * size_of::<usize>();
                let value = object(self.send(behavior, b"allocate", &[Operand::word(extent)])?)?;
                self.rooted(&[value], || {
                    if self.send(self.id.receiver,b"allocationSizeOf:",&[Operand::object(value)])?.as_word() != Some(extent)
                        || self.send(self.id.receiver,b"behaviorOf:",&[Operand::object(value)])?.as_object() != Some(behavior)
                        || self.buffers.borrow().contains_key(&value.addr()) { return Err(invalid()); }
                    let b = Buffer { storage, kind }; mirror(value, &b);
                    self.buffers.borrow_mut().insert(value.addr(),b);
                    self.send(self.gc,b"finalize:with:",&[Operand::object(value),Operand::object(self.prototype.get())])?;
                    Ok(Operand::object(value))
                })
            })
        }
    }
    unsafe fn string(&self, bytes: &[u8]) -> Result<Operand, Outcome> {
        unsafe {
            self.send(
                self.prototype.get(),
                b"fromBytes:",
                &[Operand::bytes(bytes)],
            )
        }
    }
    unsafe fn printable(&self, value: Oop) -> Result<Vec<u8>, Outcome> {
        unsafe {
            if let Ok(bytes) = self.copy(value, Kind::String) {
                return Ok(bytes);
            }
            let printed = object(self.send(value, b"printString", &[])?)?;
            let mut bytes = None;
            self.rooted(&[printed], || {
                // A replacement can return another text implementation. Ask it for
                // bytes and copy the borrowed result before any further send.
                let result = self.send(printed, b"bytes", &[])?;
                let raw = result.as_bytes().ok_or_else(invalid)?;
                bytes = Some(
                    borrowed_bytes(raw.pointer, raw.length)
                        .ok_or_else(invalid)?
                        .to_vec(),
                );
                Ok(Operand::unit())
            })?;
            bytes.ok_or_else(invalid)
        }
    }
}
unsafe fn mirror(value: Oop, b: &Buffer) {
    // This payload belongs to String. The Bytes payload is never inspected.
    unsafe { value.cast::<Oop>().write(b.storage); }
}
unsafe extern "C" fn roots(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let t = state(context)?;
        let Some([visitor]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let visitor = object(*visitor)?;
        let mut values = vec![
            t.namespace, t.roots,
            t.gc,
            t.behavior,
            t.builder_behavior,
            t.prototype.get(),
            t.builder.get(),
            t.truth,
            t.falsity,
            t.nil,
        ];
        values.extend(t.names.values().copied());
        for value in values.into_iter().filter(|v| !v.is_null()) {
            t.id.send_named(visitor, b"visit:", &[Operand::object(value)])?;
        }
        // Wrapper keys are weak; referencesOf:do: traces each owned Bytes reference.
        Ok(Operand::unit())
    })())
}
unsafe extern "C" fn references(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let t = state(context)?;
        let Some([value, visitor]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let value = object(*value)?;
        let visitor = object(*visitor)?;
        let precise =
            t.id.send_named(t.gc, b"precisePayloads", &[])?
                .as_word()
                .ok_or_else(invalid)?
                != 0;
        let (storage, words) = {
            let buffers = t.buffers.borrow(); let b = buffers.get(&value.addr()).ok_or_else(invalid)?;
            (b.storage, if b.kind == Kind::Builder { 3 } else { 2 })
        };
        t.id.send_named(visitor,b"visit:",&[Operand::object(storage)])?;
        if !precise {
            // Scan only this owner's payload mirror, never the String's bytes or Rust containers.
            for i in 0..words {
                let bits = value.cast::<usize>().add(i).read();
                let edge =
                    t.id.send_named(t.id.receiver, b"allocationAt:", &[Operand::word(bits)])?;
                if edge.as_object().is_some() {
                    t.id.send_named(visitor, b"visit:", &[edge])?;
                }
            }
        }
        Ok(Operand::unit())
    })())
}
unsafe extern "C" fn finalize(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let t = state(context)?;
        let Some([value]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        // GC has detached the object; no Id query or send is valid for it now.
        let value = object(*value)?;
        t.buffers
            .borrow_mut()
            .remove(&value.addr())
            .ok_or_else(invalid)?;
        Ok(Operand::unit())
    })())
}
unsafe fn execute(
    t: &Text,
    receiver: Oop,
    args: &[Operand],
    operation: usize,
) -> Result<Operand, Outcome> {
    unsafe {
        match operation {
            0 => {
                // fromBytes: copies native input before retaining/allocating sends.
                let [input] = args else {
                    return Err(invalid());
                };
                let raw = input.as_bytes().ok_or_else(invalid)?;
                let bytes = borrowed_bytes(raw.pointer, raw.length)
                    .ok_or_else(invalid)?
                    .to_vec();
                t.rooted(&[receiver], || t.make(bytes, Kind::String))
            }
            1 | 2 => {
                if !args.is_empty() {
                    return Err(invalid());
                }
                let storage = t.storage(receiver,Kind::String)?;
                t.id.send_named(storage,if operation==1 { b"bytes" } else { b"length" },&[])
            }
            _ => {
                let expected = match operation {
                    5 | 6 | 11 => 1,
                    8 => 2,
                    _ => 0,
                };
                if args.len() != expected {
                    return Err(invalid());
                }
                let mut objects = vec![receiver];
                for arg in args {
                    objects.push(object(*arg)?);
                }
                t.rooted(&objects, || {
                    match operation {
                        3 => t.make(t.copy(receiver, Kind::String)?, Kind::String), // independent byte buffer
                        4 => {
                            t.length(receiver, Kind::String)?;
                            Ok(Operand::object(receiver))
                        }
                        5 => {
                            let mut bytes = t.copy(receiver, Kind::String)?;
                            bytes.extend_from_slice(&t.printable(objects[1])?);
                            t.string(&bytes)
                        }
                        6 => {
                            let left = t.copy(receiver, Kind::String)?;
                            let equal = t
                                .copy(objects[1], Kind::String)
                                .is_ok_and(|right| left == right);
                            Ok(Operand::object(if equal { t.truth } else { t.falsity }))
                        }
                        7 => t.string(crate::text::trimmed_bytes(&t.copy(receiver, Kind::String)?)),
                        8 => {
                            let bytes = t.copy(receiver, Kind::String)?;
                            let (Ok(needle), Ok(replacement)) = (
                                t.copy(objects[1], Kind::String),
                                t.copy(objects[2], Kind::String),
                            ) else {
                                return Ok(Operand::object(t.nil));
                            };
                            t.string(&crate::text::replace(&bytes, &needle, &replacement))
                        }
                        9 => {
                            let bytes = t.copy(receiver, Kind::String)?;
                            let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
                            t.id.send(t.id.receiver, t.id.intern, &[Operand::bytes(&bytes[..end])])
                        }
                        10 => {
                            t.length(receiver, Kind::Builder)?;
                            t.make(Vec::new(), Kind::Builder)
                        } // existing builder new/clone empties
                        11 => {
                            t.length(receiver, Kind::Builder)?;
                            let bytes = t.printable(objects[1])?;
                            t.id.send_named(t.storage(receiver,Kind::Builder)?,b"appendBytes:",&[Operand::bytes(&bytes)])?;
                            Ok(Operand::object(receiver))
                        }
                        12 => t.string(&t.copy(receiver, Kind::Builder)?),
                        _ => Err(invalid()),
                    }
                })
            }
        }
    }
}
macro_rules! entry {
    ($name:ident, $op:expr) => {
        unsafe extern "C" fn $name(
            context: *mut c_void,
            receiver: Oop,
            args: *const Operand,
            count: usize,
        ) -> Outcome {
            done((|| unsafe {
                execute(
                    state(context)?,
                    receiver,
                    borrowed_slice(args, count).ok_or_else(invalid)?,
                    $op,
                )
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
// Delegate the byte borrow to its owner only after proving the ordinary
// String accessor. No callback occurs after receiving that borrow.
unsafe extern "C" fn try_primitive_bytes(
    c: *mut c_void,
    r: Oop,
    _: *const Operand,
    n: usize,
) -> Outcome {
    done((|| unsafe {
        let t = state(c)?;
        if n != 0 {
            return Err(invalid());
        }
        let Ok(storage) = t.storage(r, Kind::String) else {
            return Ok(Operand::unit());
        };
        if t.id
            .send_named(t.id.receiver, b"usesDefaultLookup:", &[Operand::object(r)])?
            .as_word()
            != Some(1)
            || t.id
                .send_named(
                    t.id.receiver,
                    b"bind:to:",
                    &[
                        Operand::object(t.id.intern_name(b"bytes")?),
                        Operand::object(r),
                    ],
                )?
                .as_entry()
                != Some(NativeEntry::new(bytes, c))
        {
            return Ok(Operand::unit());
        }
        match t.id.send_named(storage, b"tryPrimitiveBytes", &[]) {
            Err(e) if e.status == native::UNBOUND => Ok(Operand::unit()),
            answer => answer,
        }
    })())
}

const NAMES: &[&[u8]] = &[
    b"collectionEquals:",
    b"collectionHash",
    b"installNumbers:",
    b"installCollections:",
    b"retain:",
    b"release:",
    b"allocate",
    b"delegated",
    b"allocationSizeOf:",
    b"behaviorOf:",
    b"fromBytes:",
    b"bytes",
    b"tryPrimitiveBytes",
    b"length",
    b"printString",
    b"clone",
    b"new",
    b",",
    b"=",
    b"trimmed",
    b"replace:with:",
    b"asSymbol",
    b"add:",
    b"asString",
    b"rootsDo:",
    b"referencesOf:do:",
    b"finalize:",
    b"finalize:with:",
    b"trace:with:",
    b"prepareAllocation",
    b"register:",
    b"ownerOf:put:",
    b"addMethod",
    b"at:put:",
    b"String",
    b"StringVt",
    b"asBytes",
    b"Bytes",
    b"BytesVt",
    b"StringBuilder",
    b"StringBuilderVt",
];
unsafe fn construct(id: Protocol, namespace: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let named = |name: &[u8]| {
            object(id.send_named(namespace, b"at:", &[Operand::object(id.intern_name(name)?)])?)
        };
        let roots = named(b"Roots")?;
        let gc = named(b"GC")?;
        let truth = named(b"true")?;
        let falsity = named(b"false")?;
        let nil = named(b"nil")?;
        let parent = named(b"ObjectVt")?;
        let retain = id.intern_name(b"retain:")?;
        let release = id.intern_name(b"release:")?;
        let mut tokens = Vec::new();
        let mut keep = |v: Oop| -> Result<Oop, Outcome> {
            tokens.push(
                id.send(roots, retain, &[Operand::object(v)])?
                    .as_word()
                    .filter(|n| *n != 0)
                    .ok_or_else(invalid)?,
            );
            Ok(v)
        };
        let mut names = HashMap::new();
        for name in NAMES {
            names.insert(*name, keep(id.intern_name(name)?)?);
        }
        let behavior = keep(object(id.send_named(parent, b"delegated", &[])?)?)?;
        let builder_behavior = keep(object(id.send_named(parent, b"delegated", &[])?)?)?;
        let pointer = Box::into_raw(Box::new(Text {
            id,
            roots,
            namespace,
            gc,
            behavior,
            builder_behavior,
            truth,
            falsity,
            nil,
            names,
            prototype: Cell::new(ptr::null_mut()),
            builder: Cell::new(ptr::null_mut()),
            buffers: RefCell::new(HashMap::new()),
            numeric: Cell::new(false),
            thread: std::thread::current().id(),
        }));
        let t = &*pointer;
        for (target, name, method) in [
            (
                behavior,
                b"installNumbers:".as_slice(),
                numeric::install as Method,
            ),
            (behavior, b"installCollections:", split::install as Method),
            (behavior, b"fromBytes:", from_bytes as Method),
            (behavior, b"asBytes", binary::from_string),
            (behavior, b"bytes", bytes),
            (behavior, b"tryPrimitiveBytes", try_primitive_bytes),
            (behavior, b"length", length),
            (behavior, b"clone", clone_string),
            (behavior, b"new", clone_string),
            (behavior, b"printString", print),
            (behavior, b",", comma),
            (behavior, b"=", equals),
            (behavior, b"collectionEquals:", collection_equal),
            (behavior, b"collectionHash", collection_hash),
            (behavior, b"trimmed", trimmed),
            (behavior, b"replace:with:", replace),
            (behavior, b"asSymbol", as_symbol),
            (behavior, b"rootsDo:", self::roots),
            (behavior, b"referencesOf:do:", references),
            (behavior, b"finalize:", finalize),
            (builder_behavior, b"new", new_builder),
            (builder_behavior, b"clone", new_builder),
            (builder_behavior, b"add:", add),
            (builder_behavior, b"asString", as_string),
            (builder_behavior, b"printString", as_string),
        ] {
            t.send(
                target,
                b"addMethod",
                &[
                    Operand::object(t.names[name]),
                    Operand::entry(NativeEntry::new(method, pointer.cast())),
                ],
            )?;
        }
        // Complete the Bytes backing before allocating the first String payload.
        let storage = keep(object(t.id.send_named(t.byte_factory()?,b"fromBytes:",&[Operand::bytes(&[])])?)?)?;
        // No allocating send between allocating this first payload and installing
        // its owner. All selectors above were interned and rooted already.
        let prototype = keep(object(t.send(
            behavior,
            b"allocate",
            &[Operand::word(2 * size_of::<usize>())],
        )?)?)?;
        if t.send(
            id.receiver,
            b"allocationSizeOf:",
            &[Operand::object(prototype)],
        )?
        .as_word()
            != Some(2 * size_of::<usize>())
            || t.send(id.receiver, b"behaviorOf:", &[Operand::object(prototype)])?
                .as_object()
                != Some(behavior)
        {
            return Err(invalid());
        }
        t.prototype.set(prototype);
        let b = Buffer { storage,kind:Kind::String };
        mirror(prototype,&b); t.buffers.borrow_mut().insert(prototype.addr(),b);
        t.send(
            gc,
            b"trace:with:",
            &[Operand::object(behavior), Operand::object(prototype)],
        )?;
        t.send(
            gc,
            b"trace:with:",
            &[
                Operand::object(builder_behavior),
                Operand::object(prototype),
            ],
        )?;
        t.send(
            gc,
            b"finalize:with:",
            &[Operand::object(prototype), Operand::object(prototype)],
        )?;
        t.send(roots, b"register:", &[Operand::object(prototype)])?;
        let builder = object(t.make(Vec::new(), Kind::Builder)?)?;
        t.builder.set(builder);
        for (target, owner) in [
            (behavior, prototype),
            (builder_behavior, builder),
        ] {
            t.send(
                id.receiver,
                b"ownerOf:put:",
                &[Operand::object(target), Operand::object(owner)],
            )?;
        }
        for (name, value) in [
            (b"String".as_slice(), prototype),
            (b"StringVt", behavior),
            (b"StringBuilder", builder),
            (b"StringBuilderVt", builder_behavior),
        ] {
            t.send(
                namespace,
                b"at:put:",
                &[Operand::object(t.names[name]), Operand::object(value)],
            )?;
        }
        for token in tokens {
            id.send(roots, release, &[Operand::word(token)])?;
        }
        Ok(Operand::object(prototype))
    }
}
unsafe extern "C" fn initialize(
    id: *const Protocol,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        if !native::valid_protocol(id) {
            return Err(invalid());
        }
        let Some([namespace]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let namespace = object(*namespace)?;
        let id = *id;
        let current = id.send_named(
            namespace,
            b"at:",
            &[Operand::object(id.intern_name(b"String")?)],
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

#[path = "numeric.rs"]
mod numeric;

unsafe extern "C" fn collection_equal(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let t = state(c)?;
        let Some([other]) = borrowed_slice(a, count) else {
            return Err(invalid());
        };
        let other = object(*other)?;
        t.rooted(&[r, other], || {
            let mut equal = r == other;
            if !equal
                && t.send(t.id.receiver, b"behaviorOf:", &[Operand::object(r)])?
                    .as_object()
                    == Some(t.behavior)
                && t.send(t.id.receiver, b"behaviorOf:", &[Operand::object(other)])?
                    .as_object()
                    == Some(t.behavior)
            {
                equal = t.copy(r, Kind::String)? == t.copy(other, Kind::String)?;
            }
            Ok(Operand::object(if equal { t.truth } else { t.falsity }))
        })
    })())
}
unsafe extern "C" fn collection_hash(
    c: *mut c_void,
    r: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let t = state(c)?;
        if count != 0 {
            return Err(invalid());
        }
        t.rooted(&[r], || {
            if t.send(t.id.receiver, b"behaviorOf:", &[Operand::object(r)])?
                .as_object()
                != Some(t.behavior)
            {
                return Ok(Operand::word(mix(r.addr() as u64)));
            }
            let mut hash = 2_166_136_261u32;
            for byte in t.copy(r, Kind::String)? {
                hash = (hash ^ u32::from(byte)).wrapping_mul(16_777_619);
            }
            Ok(Operand::word(hash as usize))
        })
    })())
}
fn mix(mut value: u64) -> usize {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (value ^ (value >> 31)) as u32 as usize
}

#[path = "split.rs"]
mod split;

#[path = "binary.rs"]
mod binary;
