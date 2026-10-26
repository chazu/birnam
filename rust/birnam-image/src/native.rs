//! Image-v8 integration. Representations, code and facts cross owner messages.
#[path = "format.rs"]
mod format;
use super::{support, v8};
use birnam_abi::{
    self as abi, Oop, borrowed_slice,
    id_protocol::{Literal, MethodSpec, Module, Protocol},
};
use birnam_artifact::{
    code_binding_inventory as bindings, materialization_selection as selection, portable_inventory,
};
use core::{
    cell::{Cell, RefCell},
    ptr,
};
use std::{collections::HashMap, os::unix::ffi::OsStrExt, path::Path};
// Image admission refusal is local policy, not a second Id result format.
#[derive(Debug)]
enum Failure {
    Rejected,
    Native(Oop),
    Value(Oop),
}
impl From<Oop> for Failure {
    fn from(v: Oop) -> Self {
        Self::Native(v)
    }
}
fn bad() -> Failure {
    Failure::Rejected
}
fn object(v: Oop) -> Result<Oop, Failure> {
    if v.is_null() { Err(bad()) } else { Ok(v) }
}
fn optional(v: Oop) -> Option<Oop> {
    (!v.is_null()).then_some(v)
}
fn done(v: Result<Oop, Failure>) -> Oop {
    match v {
        Ok(v) | Err(Failure::Value(v)) => v,
        Err(Failure::Native(v)) => abi::fault_reference(v),
        Err(Failure::Rejected) => unsafe { (*INSTANCE.load(Ordering::Acquire)).id.invalid },
    }
}
use std::sync::atomic::{AtomicPtr, Ordering};
static INSTANCE: AtomicPtr<State> = AtomicPtr::new(ptr::null_mut());
unsafe fn state(c: Oop) -> Result<&'static State, Failure> {
    let p = INSTANCE.load(Ordering::Acquire);
    if p.is_null() {
        return Err(bad());
    }
    let s = unsafe { &*p };
    if s.ns != c || s.thread != std::thread::current().id() {
        return Err(bad());
    }
    Ok(s)
}
#[derive(Clone)]
struct Fact {
    values: [Oop; 3],
    tx: u64,
    op: u8,
    history: bool,
}
#[derive(Default)]
struct Report {
    saved: usize,
    status: &'static [u8],
    detail: Vec<u8>,
    code: usize,
}
struct State {
    id: Protocol,
    names: HashMap<&'static [u8], Oop>,
    scoped: RefCell<Vec<Oop>>,
    ns: Oop,
    model: Oop,
    receiver: Oop,
    permanent: Vec<Oop>,
    busy: Cell<bool>,
    ready: Cell<bool>,
    facts: RefCell<Vec<Fact>>,
    report: RefCell<Report>,
    composition: RefCell<Option<Vec<u8>>>,
    thread: std::thread::ThreadId,
}
struct Scope<'a> {
    s: &'a State,
    start: usize,
}
impl<'a> Scope<'a> {
    fn new(s: &'a State) -> Self {
        Self {
            s,
            start: s.scoped.borrow().len(),
        }
    }
    unsafe fn keep(&self, o: Oop) -> Result<Oop, Failure> {
        self.s.scoped.borrow_mut().push(o);
        Ok(o)
    }
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        self.s.scoped.borrow_mut().truncate(self.start);
    }
}
impl State {
    unsafe fn send(&self, r: Oop, n: &[u8], a: &[Oop]) -> Result<Oop, Failure> {
        unsafe { self.id.send(r, self.names[n], a).map_err(Failure::Native) }
    }
    unsafe fn intern(&self, n: &[u8]) -> Result<Oop, Failure> {
        unsafe {
            if let Some(v) = self.names.get(n) {
                return Ok(*v);
            }
            let args: Vec<_> = n.iter().map(|b| abi::size(*b as usize).unwrap()).collect();
            let name = self.send(self.id.root, b"nameWithBytes", &args)?;
            Ok(self.id.send(name, self.id.intern, &[])?)
        }
    }
    unsafe fn named(&self, n: &[u8]) -> Result<Oop, Failure> {
        unsafe { object(self.send(self.ns, b"at:", &[self.intern(n)?])?) }
    }
    unsafe fn bytes(&self, o: Oop) -> Result<Vec<u8>, Failure> {
        unsafe {
            let q = Scope::new(self);
            let v = q.keep(self.send(o, b"bytes", &[])?)?;
            Ok(self.id.read_bytes(
                v,
                self.names[b"sizeWord".as_slice()],
                self.names[b"atWord:".as_slice()],
            )?)
        }
    }
    unsafe fn symbol(&self, o: Oop) -> Result<Vec<u8>, Failure> {
        unsafe {
            let q = Scope::new(self);
            q.keep(o)?;
            Ok(self.id.read_bytes(
                o,
                self.names[b"sizeWord".as_slice()],
                self.names[b"atWord:".as_slice()],
            )?)
        }
    }
    unsafe fn byte_value(&self, b: &[u8]) -> Result<Oop, Failure> {
        unsafe {
            let args: Vec<_> = b.iter().map(|b| abi::size(*b as usize).unwrap()).collect();
            self.send(self.id.root, b"sequence", &args)
        }
    }
    unsafe fn text(&self, q: &Scope<'_>, family: &[u8], b: &[u8]) -> Result<Oop, Failure> {
        unsafe {
            let value = {
                let input = Scope::new(self);
                let v = input.keep(self.byte_value(b)?)?;
                self.send(self.named(family)?, b"fromBytes:", &[v])?
            };
            q.keep(value)
        }
    }
    unsafe fn word(&self, v: usize) -> Result<Oop, Failure> {
        unsafe {
            if let Some(v) = abi::size(v) {
                return Ok(v);
            }
            self.send(
                self.named(b"Word")?,
                b"fromUnsignedLow:high:",
                &[
                    abi::size(v & 0xffff_ffff).unwrap(),
                    abi::size(v >> 32).unwrap(),
                ],
            )
        }
    }
    unsafe fn bits(&self, v: Oop) -> Result<usize, Failure> {
        unsafe {
            let q = Scope::new(self);
            q.keep(v)?;
            if let Some(v) = abi::as_size(v) {
                return Ok(v);
            }
            let lo = abi::as_size(self.send(v, b"low32", &[])?).ok_or_else(bad)?;
            let hi = abi::as_size(self.send(v, b"high32", &[])?).ok_or_else(bad)?;
            Ok(lo | (hi << 32))
        }
    }
    unsafe fn integer(&self, q: &Scope<'_>, v: usize) -> Result<Oop, Failure> {
        unsafe {
            let v = q.keep(self.word(v)?)?;
            q.keep(self.send(self.named(b"Integer")?, b"fromWord:", &[v])?)
        }
    }
    unsafe fn number(&self, v: Oop) -> Result<usize, Failure> {
        unsafe {
            let q = Scope::new(self);
            let v = q.keep(self.send(self.named(b"Integer")?, b"wordOf:", &[v])?)?;
            self.bits(v)
        }
    }
    unsafe fn call(&self, q: &Scope<'_>, r: Oop, n: &[u8], a: &[Oop]) -> Result<Oop, Failure> {
        unsafe { q.keep(object(self.send(r, n, a)?)?) }
    }
    unsafe fn vector(&self, q: &Scope<'_>, values: &[Oop]) -> Result<Oop, Failure> {
        unsafe {
            let mut v = self.call(q, self.named(b"Vector")?, b"new", &[])?;
            for &x in values {
                v = self.call(q, v, b"add:", &[x])?;
            }
            Ok(v)
        }
    }
    unsafe fn sequence(&self, q: &Scope<'_>, v: Oop) -> Result<Vec<Oop>, Failure> {
        unsafe {
            let n = self.number(self.call(q, v, b"size", &[])?)?;
            if n > 1_048_576 {
                return Err(bad());
            }
            let mut out = Vec::new();
            for i in 0..n {
                out.push(self.call(q, v, b"at:", &[self.integer(q, i)?])?);
            }
            Ok(out)
        }
    }
    unsafe fn export(&self, o: Oop) -> Result<Option<Vec<u8>>, Failure> {
        unsafe {
            optional(self.send(self.ns, b"nameOf:", &[o])?)
                .map(|n| self.symbol(n))
                .transpose()
        }
    }
    unsafe fn behavior(&self, o: Oop) -> Result<Oop, Failure> {
        unsafe { object(self.send(self.id.root, b"behaviorOf:", &[o])?) }
    }
    // A vtable is itself the dictionary target; ordinary receivers use their
    // current vtable. The Id API receiver is distinct from the metatable.
    unsafe fn method_behavior(&self, target: Oop) -> Result<Oop, Failure> {
        unsafe {
            if abi::as_size(self.send(self.id.root, b"isBehavior:", &[target])?) == Some(1) {
                Ok(target)
            } else {
                self.behavior(target)
            }
        }
    }
    unsafe fn is_error(&self, o: Oop) -> Result<bool, Failure> {
        unsafe { Ok(optional(self.send(o, b"isError", &[])?) == Some(self.named(b"true")?)) }
    }
    fn reject(&self, detail: &[u8]) {
        *self.report.borrow_mut() = Report {
            status: b"rejected",
            detail: detail.to_vec(),
            ..Report::default()
        };
    }
}
// The old native shims cannot execute these message-based object files.
fn runtime_abi() -> u64 {
    support::wyhash(support::runtime_abi_hash(), b"Id-oop-image-v3")
}
fn composition_identity(decoded: &v8::DecodedImage<'_>) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    if let Some(code) = decoded.code {
        digest.update(b"birnam.image-composition.v1\0");
        digest.update(1u32.to_le_bytes());
        digest.update(&code[..8]); // validated code-record checksum
    } else {
        digest.update(b"birnam.image-rebuilt-composition.v1\0");
        for bytes in [decoded.programs, decoded.bindings, decoded.selection] {
            digest.update((bytes.len() as u64).to_le_bytes());
            digest.update(bytes);
        }
    }

    if let Some(providers) = decoded.providers {
        digest.update((providers.len() as u64).to_le_bytes());
        digest.update(providers);
    }
    digest
        .finalize()
        .iter()
        .flat_map(|b| {
            [
                b"0123456789abcdef"[(b >> 4) as usize],
                b"0123456789abcdef"[(b & 15) as usize],
            ]
        })
        .collect()
}
#[derive(Clone, Debug)]
enum Value {
    Nil,
    True,
    False,
    Null,
    Raw(u64),
    Symbol(Vec<u8>),
    Export(Vec<u8>),
    Heap(u32),
    Program(u32),
    Block(u32, bool),
    Integer(u64),
    Float(u64),
    Word(u64),
    Immediate(u64),
    String(Vec<u8>),
    Bytes(Vec<u8>),
}
impl Value {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::Nil => out.push(0),
            Self::True => out.push(1),
            Self::False => out.push(2),
            Self::Null => out.push(8),
            Self::Raw(n) | Self::Integer(n) | Self::Float(n) | Self::Word(n) => {
                out.push(match self {
                    Self::Raw(_) => 3,
                    Self::Integer(_) => 13,
                    Self::Float(_) => 14,
                    _ => 15,
                });
                out.extend_from_slice(&n.to_le_bytes());
            }
            Self::Immediate(n) => {
                out.push(19);
                out.extend_from_slice(&8u32.to_le_bytes());
                out.extend_from_slice(&n.to_le_bytes());
            }
            Self::Symbol(b) | Self::Export(b) | Self::String(b) | Self::Bytes(b) => {
                out.push(match self {
                    Self::Symbol(_) => 5,
                    Self::Export(_) => 9,
                    Self::String(_) => 16,
                    _ => 17,
                });
                out.extend_from_slice(&(b.len() as u32).to_le_bytes());
                out.extend_from_slice(b);
            }
            Self::Heap(n) | Self::Program(n) | Self::Block(n, _) => {
                out.push(match self {
                    Self::Heap(_) => 7,
                    Self::Program(_) => 11,
                    Self::Block(_, true) => 18,
                    _ => 12,
                });
                out.extend_from_slice(&n.to_le_bytes());
            }
        }
    }
    fn decode(bytes: &[u8], offset: &mut usize) -> Result<Self, Failure> {
        let start = *offset;
        let field = v8::parse_field(bytes, offset).map_err(|_| bad())?;
        Ok(match field.tag {
            0 => Self::Nil,
            1 => Self::True,
            2 => Self::False,
            8 => Self::Null,
            3 | 13..=15 => {
                let n = u64::from_le_bytes(
                    bytes
                        .get(start + 1..start + 9)
                        .ok_or_else(bad)?
                        .try_into()
                        .map_err(|_| bad())?,
                );
                match field.tag {
                    3 => Self::Raw(n),
                    13 => Self::Integer(n),
                    14 => Self::Float(n),
                    _ => Self::Word(n),
                }
            }
            19 => Self::Immediate(u64::from_le_bytes(
                field.data.ok_or_else(bad)?.try_into().map_err(|_| bad())?,
            )),
            5 => Self::Symbol(field.data.ok_or_else(bad)?.to_vec()),
            9 => Self::Export(field.data.ok_or_else(bad)?.to_vec()),
            16 => Self::String(field.data.ok_or_else(bad)?.to_vec()),
            17 => Self::Bytes(field.data.ok_or_else(bad)?.to_vec()),
            7 => Self::Heap(field.index.ok_or_else(bad)?),
            11 => Self::Program(field.index.ok_or_else(bad)?),
            12 | 18 => Self::Block(field.index.ok_or_else(bad)?, field.tag == 18),
            _ => return Err(bad()),
        })
    }
    fn binding(&self) -> Result<bindings::Field<'_>, Failure> {
        Ok(match self {
            Self::Nil => bindings::Field::Nil,
            Self::True => bindings::Field::True,
            Self::False => bindings::Field::False,
            Self::Symbol(b) => bindings::Field::Symbol(b),
            Self::Export(b) => bindings::Field::ExportRef(b),
            Self::Heap(i) => bindings::Field::HeapRef(*i),
            _ => return Err(bad()),
        })
    }
    fn from_binding(v: bindings::Field<'_>) -> Self {
        match v {
            bindings::Field::Nil => Self::Nil,
            bindings::Field::True => Self::True,
            bindings::Field::False => Self::False,
            bindings::Field::Symbol(b) => Self::Symbol(b.to_vec()),
            bindings::Field::ExportRef(b) => Self::Export(b.to_vec()),
            bindings::Field::HeapRef(n) => Self::Heap(n),
        }
    }
}
struct Node {
    locator: Value,
    fields: Vec<Value>,
    program: Option<Vec<u8>>,
}
struct Graph<'a, 'b> {
    s: &'a State,
    q: &'b Scope<'a>,
    objects: Vec<Oop>,
    indices: HashMap<usize, u32>,
    nodes: Vec<Node>,
    behaviors: Vec<super::behavior_links::Link>,
    fact_owner: Option<Oop>,
}
impl Graph<'_, '_> {
    unsafe fn reference(&mut self, o: Oop) -> Result<Value, Failure> {
        unsafe {
            if o.is_null() {
                return Ok(Value::Null);
            }
            if abi::as_size(o).is_some() {
                let index = if let Some(index) = self.indices.get(&o.addr()) {
                    *index
                } else {
                    let index = u32::try_from(self.objects.len()).map_err(|_| bad())?;
                    self.indices.insert(o.addr(), index);
                    self.objects.push(o);
                    index
                };
                return Ok(Value::Heap(index));
            }
            self.q.keep(o)?;
            for (n, v) in [
                (b"nil".as_slice(), Value::Nil),
                (b"true", Value::True),
                (b"false", Value::False),
            ] {
                if o == self.s.named(n)? {
                    return Ok(v);
                }
            }
            if self.s.behavior(o)? == self.s.named(b"SymbolVt")? {
                return Ok(Value::Symbol(self.s.symbol(o)?));
            }
            if let Some(n) = self.s.export(o)? {
                return Ok(Value::Export(n));
            }
            let i = if let Some(i) = self.indices.get(&o.addr()) {
                *i
            } else {
                if self.objects.len() >= 1_048_576 {
                    return Err(bad());
                }
                let i = self.objects.len() as u32;
                self.indices.insert(o.addr(), i);
                self.objects.push(o);
                i
            };
            Ok(Value::Heap(i))
        }
    }
    unsafe fn finish(&mut self) -> Result<Vec<u8>, Failure> {
        unsafe {
            let mut i = 0;
            while i < self.objects.len() {
                let o = self.objects[i];
                if let Some(bits) = abi::as_size(o) {
                    self.nodes.push(Node {
                        locator: Value::Immediate(bits as u64),
                        fields: Vec::new(),
                        program: None,
                    });
                    i += 1;
                    continue;
                }
                // Current owner policy selects an unpublished payload or refuses
                // persistence. Returning self preserves ordinary Error values too.
                let payload = self.s.call(self.q, o, b"imagePayload", &[])?;
                if payload == self.s.named(b"false")? {
                    return Err(bad());
                }
                if payload != o && self.s.is_error(payload)? {
                    return Err(Failure::Value(payload));
                }
                let first = self.q.keep(self.s.behavior(o)?)?;
                let mut vt = first;
                let mut visited = std::collections::HashSet::new();
                let name = loop {
                    if let Some(name) = self.s.export(vt)? {
                        break name;
                    }
                    if !visited.insert(vt) {
                        return Err(bad());
                    }
                    vt = self.q.keep(object(self.s.send(
                        self.s.id.root,
                        b"parentOf:",
                        &[vt],
                    )?)?)?;
                };
                // Projection changes plain object slots, never a native family's
                // representation. Their existing owner-specific codecs remain.
                if payload != o && name != b"ObjectVt" {
                    return Err(bad());
                }
                if name == b"BlockVt" {
                    // Execution reconstructs its intrinsic per-Method behavior.
                    // Additional user-defined Block behavior layers remain inadmissible.
                    if visited.len() > 1 {
                        return Err(bad());
                    }
                } else if !visited.is_empty() {
                    let owner = self.q.keep(object(self.s.send(
                        self.s.id.root,
                        b"ownerOf:",
                        &[first],
                    )?)?)?;
                    let Value::Heap(owner_index) = self.reference(owner)? else {
                        return Err(bad());
                    };
                    let parent = if owner == o {
                        let parent = self.q.keep(object(self.s.send(
                            self.s.id.root,
                            b"parentOf:",
                            &[first],
                        )?)?)?;
                        Some(if let Some(name) = self.s.export(parent)? {
                            super::behavior_links::Parent::Export(name)
                        } else {
                            let parent_owner = self.q.keep(object(self.s.send(
                                self.s.id.root,
                                b"ownerOf:",
                                &[parent],
                            )?)?)?;
                            let Value::Heap(index) = self.reference(parent_owner)? else {
                                return Err(bad());
                            };
                            super::behavior_links::Parent::Owner(index)
                        })
                    } else {
                        None
                    };
                    self.behaviors.push(super::behavior_links::Link {
                        object: i as u32,
                        owner: owner_index,
                        parent,
                    });
                }
                let mut node = Node {
                    locator: Value::Export(name.clone()),
                    fields: Vec::new(),
                    program: None,
                };
                match name.as_slice() {
                    b"IntegerVt" | b"FloatVt" | b"WordVt" => {
                        let family = match name.as_slice() {
                            b"IntegerVt" => b"Integer".as_slice(),
                            b"FloatVt" => b"Float",
                            _ => b"Word",
                        };
                        let selector = if family == b"Float" {
                            b"bitsOf:".as_slice()
                        } else {
                            b"wordOf:"
                        };
                        let bits = if family == b"Word" {
                            self.s.send(o, b"bits", &[])?
                        } else {
                            self.s.send(self.s.named(family)?, selector, &[o])?
                        };
                        let n = self.s.bits(bits)? as u64;
                        node.locator = match family {
                            b"Integer" => Value::Integer(n),
                            b"Float" => Value::Float(n),
                            _ => Value::Word(n),
                        };
                    }
                    b"StringVt" => node.locator = Value::String(self.s.bytes(o)?),
                    b"BytesVt" => node.locator = Value::Bytes(self.s.bytes(o)?),
                    b"CellVt" => {
                        for i in 0..3 {
                            let v = self.s.call(
                                self.q,
                                o,
                                b"imageFieldAt:",
                                &[self.s.integer(self.q, i)?],
                            )?;
                            if i == 0 && self.fact_owner != Some(v) {
                                return Err(bad());
                            }
                            node.fields.push(self.reference(v)?);
                        }
                    }
                    b"CodeObjectVt" => {
                        let bytes = self.s.call(
                            self.q,
                            self.s.named(b"CodeObject")?,
                            b"canonicalBytesOf:",
                            &[o],
                        )?;
                        node.program = Some(self.s.bytes(bytes)?);
                        node.fields.push(Value::Program(0));
                        let provenance = self.s.call(self.q, o, b"sourceProvenance", &[])?;
                        node.fields.push(self.reference(provenance)?);
                        let signature = self.s.call(self.q, o, b"nativeSignature", &[])?;
                        if signature != self.s.named(b"nil")? {
                            node.fields.push(Value::Bytes(self.s.bytes(signature)?));
                        }
                    }
                    // Execution's family name is BlockVt; source Blocks have private behaviors.
                    b"BlockVt" => {
                        let code = self.s.call(self.q, o, b"codeObject", &[])?;
                        let Value::Heap(index) = self.reference(code)? else {
                            return Err(bad());
                        };
                        let sync = self.s.call(self.q, o, b"isSynchronous", &[])?
                            == self.s.named(b"true")?;
                        node.locator = Value::Block(index, sync);
                        node.fields.push(Value::Heap(index));
                        let count =
                            self.s
                                .number(self.s.call(self.q, o, b"captureCount", &[])?)?;
                        if count > 1_048_576 {
                            return Err(bad());
                        }
                        for n in 0..count {
                            let v = self.s.call(
                                self.q,
                                o,
                                b"captureAt:",
                                &[self.s.integer(self.q, n)?],
                            )?;
                            node.fields.push(self.reference(v)?);
                        }
                    }
                    _ => {
                        let allowed = matches!(
                            name.as_slice(),
                            b"ObjectVt"
                                | b"SystemVt"
                                | b"VectorVt"
                                | b"VectorNodeVt"
                                | b"VectorNode"
                                | b"MapVt"
                                | b"MapNodeVt"
                                | b"MapNode"
                                | b"RangeVt"
                                | b"ErrorVt"
                        );
                        if !allowed {
                            return Err(bad());
                        }
                        let bytes =
                            abi::as_size(self.s.send(self.s.model, b"imageSizeOf:", &[payload])?)
                                .ok_or_else(bad)?;
                        // Prelude owns System as an empty Object. Native service state
                        // belongs to module contexts, never to a cloned receiver.
                        if (name == b"SystemVt" && bytes != 0)
                            || bytes % size_of::<usize>() != 0
                            || bytes > 64 << 20
                        {
                            return Err(bad());
                        }
                        for slot in 0..bytes / size_of::<usize>() {
                            let v = self.q.keep(self.s.send(
                                self.s.model,
                                b"imageSlotAt:in:",
                                &[self.s.word(slot)?, payload],
                            )?)?;
                            let kind = self.s.send(
                                self.s.model,
                                b"imageSlotKindAt:in:",
                                &[abi::size(slot).ok_or_else(bad)?, payload],
                            )?;
                            node.fields.push(if abi::as_size(kind) == Some(1) {
                                self.reference(v)?
                            } else {
                                Value::Raw(self.s.bits(v)? as u64)
                            });
                        }
                    }
                }
                self.nodes.push(node);
                i += 1;
            }
            let roots = self
                .nodes
                .iter()
                .filter_map(|n| n.program.as_deref())
                .collect::<Vec<_>>();
            let programs = portable_inventory::encode_closure(&roots).map_err(|_| bad())?;
            let inventory = portable_inventory::inspect(&programs).map_err(|_| bad())?;
            for node in &mut self.nodes {
                if let Some(ref p) = node.program {
                    let i = (0..inventory.program_count as usize)
                        .find(|&i| inventory.program(i) == Some(p.as_slice()))
                        .ok_or_else(bad)?;
                    node.fields[0] = Value::Program(i as u32);
                }
            }
            Ok(programs)
        }
    }
    fn encode(&self) -> Vec<u8> {
        let mut b = (self.nodes.len() as u32).to_le_bytes().to_vec();
        for node in &self.nodes {
            node.locator.encode(&mut b);
            b.extend_from_slice(&(node.fields.len() as u32).to_le_bytes());
            for field in &node.fields {
                field.encode(&mut b);
            }
        }
        b
    }
}
// Snapshot consistency is about owner data, not optional certification or
// method identities. Monotonic epochs may conservatively reject a no-op edit.
unsafe fn snapshot_stamp(
    s: &State,
    q: &Scope<'_>,
    store: Option<Oop>,
) -> Result<
    (
        Option<(Oop, usize)>,
        Oop,
        Oop,
        Oop,
        Oop,
        usize,
        usize,
        usize,
    ),
    Failure,
> {
    unsafe {
        let facts = store
            .map(|owner| {
                abi::as_size(s.send(owner, b"imageStateRevision", &[])?)
                    .map(|revision| (owner, revision))
                    .ok_or_else(bad)
            })
            .transpose()?;
        let globals = q.keep(s.named(b"Globals")?)?;
        let bindings = s.number(s.call(q, globals, b"bindingBasis", &[])?)?;
        let methods = abi::as_size(s.send(s.id.root, b"bindingEpoch", &[])?).ok_or_else(bad)?;
        let name = q.keep(s.intern(b"SourceRegistry")?)?;
        let sources = s.call(q, globals, b"at:", &[name])?;
        let revision = s.number(s.call(q, sources, b"revision", &[])?)?;
        let manager = s.call(q, globals, b"at:", &[q.keep(s.intern(b"Module")?)?])?;
        let registry = s.call(q, manager, b"registry", &[])?;
        Ok((
            facts, globals, sources, manager, registry, bindings, methods, revision,
        ))
    }
}
unsafe fn save(s: &State, q: &Scope<'_>, system: Oop, path: &Path) -> Result<Oop, Failure> {
    unsafe {
        let admitted = s.call(q, s.named(b"Generation")?, b"prepareImageSnapshot", &[])?;
        if admitted != s.named(b"true")? {
            s.reject(b"Image snapshot admission failed");
            return if s.is_error(admitted)? {
                Ok(admitted)
            } else {
                Err(bad())
            };
        }
        let selected = s.call(q, system, b"imageStore", &[])?;
        if s.is_error(selected)? {
            s.reject(b"Image fact participant selection failed");
            return Ok(selected);
        }
        let store = (selected != s.named(b"nil")?).then_some(selected);
        // Native Store identities are resident exports, not copied slot payloads.
        if store.is_some() && s.export(selected)?.is_none() {
            return Err(bad());
        }
        let stamp = snapshot_stamp(s, q, store)?;
        let manager = s.call(
            q,
            s.named(b"Globals")?,
            b"at:",
            &[q.keep(s.intern(b"Module")?)?],
        )?;
        let names = s.call(q, manager, b"imageProviders", &[])?;
        if s.is_error(names)? {
            return Ok(names);
        }
        let names = s.sequence(q, names)?;
        let mut providers = (u32::try_from(names.len()).map_err(|_| bad())?)
            .to_le_bytes()
            .to_vec();
        for name in names {
            Value::Symbol(s.symbol(name)?).encode(&mut providers);
        }
        s.facts.borrow_mut().clear();
        let metadata = if let Some(store) = store {
            let metadata = [
                b"basis".as_slice(),
                b"oldestRetainedBasis",
                b"retentionPolicyCode",
                b"retentionPolicyArgument",
            ]
            .map(|n| s.call(q, store, n, &[]).and_then(|o| s.number(o)));
            let [basis, oldest, policy, argument] = metadata;
            let metadata = (basis?, oldest?, policy?, argument?);
            s.send(store, b"imageFactsInto:", &[s.receiver])?;
            Some(metadata)
        } else {
            None
        };
        let facts = s.facts.borrow().clone();
        let codegen = q.keep(s.named(b"Codegen")?)?;
        let rows = s.sequence(q, s.call(q, codegen, b"portableImageBindings", &[])?)?;
        let mut graph = Graph {
            s,
            q,
            objects: Vec::new(),
            indices: HashMap::new(),
            nodes: Vec::new(),
            behaviors: Vec::new(),
            fact_owner: store,
        };
        let mut fact_values = Vec::new();
        for fact in &facts {
            let mut values = Vec::new();
            for &o in &fact.values {
                values.push(graph.reference(o)?);
            }
            fact_values.push(values);
        }
        let mut binding_values = Vec::new();
        let mut selectors = Vec::new();
        let mut named_styles = Vec::new();
        for row in rows {
            let values = s.sequence(q, row)?;
            if !matches!(values.len(), 4 | 5) {
                return Err(bad());
            }
            let sync = if values.len() == 5 {
                if values[4] == s.named(b"true")? {
                    true
                } else if values[4] == s.named(b"false")? {
                    false
                } else {
                    return Err(bad());
                }
            } else {
                false
            };
            named_styles.push(if sync {
                selection::Disposition::Synchronous
            } else {
                selection::Disposition::Resumable
            });
            selectors.push(s.symbol(values[1])?);
            binding_values.push([
                graph.reference(values[0])?,
                graph.reference(values[2])?,
                graph.reference(values[3])?,
            ]);
        }
        let sources = s.call(q, codegen, b"portableImageSources", &[])?;
        if s.is_error(sources)? {
            return Err(Failure::Value(sources));
        }
        // Explicit owner roots avoid rediscovery through half-restored Globals.
        let tag = q.keep(s.intern(b"imageOwnersV2")?)?;
        let name = q.keep(s.intern(b"AlienRecipes")?)?;
        let foreign = s.call(q, s.named(b"Globals")?, b"at:", &[name])?;
        let mut owners = s.call(q, s.named(b"Vector")?, b"new", &[])?;
        for value in [tag, sources, foreign, selected] {
            owners = s.call(q, owners, b"add:", &[value])?;
        }
        let mut sources_bytes = Vec::new();
        graph.reference(owners)?.encode(&mut sources_bytes);
        let programs = graph.finish()?;

        let specs = binding_values
            .iter()
            .zip(&selectors)
            .map(|(v, name)| {
                Ok(bindings::BindingSpec {
                    target: v[0].binding()?,
                    selector: name,
                    implementation: v[1].binding()?,
                    provenance: v[2].binding()?,
                })
            })
            .collect::<Result<Vec<_>, Failure>>()?;
        let bindings = bindings::encode(&specs).map_err(|_| bad())?;
        let count = portable_inventory::inspect(&programs)
            .map_err(|_| bad())?
            .program_count as usize;
        let selection = selection::encode(
            &programs,
            &bindings,
            &named_styles,
            &vec![selection::Disposition::Resumable; count],
        )
        .map_err(|_| bad())?;
        let heap = graph.encode();
        let behaviors = super::behavior_links::encode(&graph.behaviors);
        let facts_bytes = if let Some((basis, oldest, policy, argument)) = metadata {
            let retention = match policy {
                0 => super::retained_facts::RetentionPolicy::CurrentOnly,
                1 => super::retained_facts::RetentionPolicy::KeepSince(argument as u64),
                2 => super::retained_facts::RetentionPolicy::KeepLast(argument as u64),
                _ => return Err(bad()),
            };
            let mut writer =
                super::retained_facts::FactsWriter::new(basis as u64, oldest as u64, retention)
                    .map_err(|_| bad())?;
            for (fact, values) in facts.iter().zip(fact_values) {
                let mut encoded = Vec::new();
                for v in values {
                    v.encode(&mut encoded);
                }
                writer
                    .fact_encoded(fact.history, fact.op, fact.tx, &encoded)
                    .map_err(|_| bad())?;
            }
            Some(writer.finish())
        } else {
            None
        };
        let fact_sections = facts_bytes.as_deref().into_iter().collect::<Vec<_>>();
        // Structural checks precede the native compiler and any file publication.
        format::compose(
            s,
            q,
            v8::ImageSpec {
                sources: Some(&sources_bytes),
                behaviors: Some(&behaviors),
                providers: Some(&providers),
                architecture: support::IMAGE_ARCH_TAG,
                runtime_abi: runtime_abi(),
                programs: &programs,
                bindings: &bindings,
                selection: &selection,
                heaps: &[&heap],
                facts: &fact_sections,
                code: None,
            },
        )?;
        let p = s.text(q, b"Bytes", &programs)?;
        let b = s.text(q, b"Bytes", &bindings)?;
        let selected = s.text(q, b"Bytes", &selection)?;
        let compiled = s.call(
            q,
            codegen,
            b"compilePortableImageCode:bindings:selection:",
            &[p, b, selected],
        )?;
        if s.is_error(compiled)? {
            s.reject(b"Native image compilation failed");
            return Ok(compiled);
        }
        let code = s.bytes(compiled)?;
        let bytes = format::compose(
            s,
            q,
            v8::ImageSpec {
                sources: Some(&sources_bytes),
                behaviors: Some(&behaviors),
                providers: Some(&providers),
                architecture: support::IMAGE_ARCH_TAG,
                runtime_abi: runtime_abi(),
                programs: &programs,
                bindings: &bindings,
                selection: &selection,
                heaps: &[&heap],
                facts: &fact_sections,
                code: Some(&code),
            },
        )?;
        let admitted = s.call(q, s.named(b"Generation")?, b"prepareImageSnapshot", &[])?;
        if admitted != s.named(b"true")? {
            s.reject(b"Image snapshot admission changed during save");
            return if s.is_error(admitted)? {
                Ok(admitted)
            } else {
                Err(bad())
            };
        }
        if snapshot_stamp(s, q, store)? != stamp {
            s.reject(b"World changed during image snapshot");
            return Ok(s.named(b"nil")?);
        }
        support::publish_atomic(path, &bytes).map_err(|_| bad())?;
        *s.report.borrow_mut() = Report {
            status: b"saved",
            saved: facts.len(),
            code: count * 2,
            detail: Vec::new(),
        };
        Ok(s.named(b"true")?)
    }
}
fn heap_nodes(decoded: &v8::DecodedImage<'_>) -> Result<Vec<Node>, Failure> {
    let mut nodes = Vec::new();
    for heap in &decoded.heaps {
        let count = u32::from_le_bytes(
            heap.get(..4)
                .ok_or_else(bad)?
                .try_into()
                .map_err(|_| bad())?,
        );
        let mut offset = 4;
        for _ in 0..count {
            let locator = Value::decode(heap, &mut offset)?;
            let words = u32::from_le_bytes(
                heap.get(offset..offset + 4)
                    .ok_or_else(bad)?
                    .try_into()
                    .map_err(|_| bad())?,
            );
            offset += 4;
            let mut fields = Vec::new();
            for _ in 0..words {
                fields.push(Value::decode(heap, &mut offset)?);
            }
            nodes.push(Node {
                locator,
                fields,
                program: None,
            });
        }
        if offset != heap.len() {
            return Err(bad());
        }
    }
    Ok(nodes)
}
unsafe fn resolve(
    s: &State,
    q: &Scope<'_>,
    value: &Value,
    objects: &[Oop],
) -> Result<Oop, Failure> {
    unsafe {
        match value {
            Value::Nil => s.named(b"nil"),
            Value::True => s.named(b"true"),
            Value::False => s.named(b"false"),
            Value::Null => Ok(ptr::null_mut()),

            Value::Symbol(b) => q.keep(s.intern(b)?),
            Value::Export(b) => q.keep(s.named(b)?),
            Value::Heap(i) => objects
                .get(*i as usize)
                .copied()
                .filter(|o| !o.is_null())
                .ok_or_else(bad),
            _ => Err(bad()),
        }
    }
}
unsafe fn restore_heap(
    s: &State,
    q: &Scope<'_>,
    decoded: &v8::DecodedImage<'_>,
) -> Result<Vec<Oop>, Failure> {
    unsafe {
        let nodes = heap_nodes(decoded)?;
        let inventory = portable_inventory::inspect(decoded.programs).map_err(|_| bad())?;
        let mut objects = vec![ptr::null_mut(); nodes.len()];
        for (i, node) in nodes.iter().enumerate() {
            let v = match &node.locator {
                Value::Immediate(n) => usize::try_from(*n)
                    .ok()
                    .and_then(abi::size)
                    .ok_or_else(bad)?,
                Value::Integer(n) | Value::Float(n) | Value::Word(n) => {
                    let family = match node.locator {
                        Value::Integer(_) => b"Integer".as_slice(),
                        Value::Float(_) => b"Float",
                        _ => b"Word",
                    };
                    let selector = if family == b"Integer" {
                        b"fromWord:".as_slice()
                    } else {
                        b"fromBits:"
                    };
                    object(s.send(s.named(family)?, selector, &[s.word(*n as usize)?])?)?
                }
                Value::String(b) => s.text(q, b"String", b)?,
                Value::Bytes(b) => s.text(q, b"Bytes", b)?,
                Value::Export(n) if n == b"CellVt" => {
                    if node.fields.len() != 3 {
                        return Err(bad());
                    }
                    let Value::Export(_) = &node.fields[0] else {
                        return Err(bad());
                    };
                    let store = resolve(s, q, &node.fields[0], &[])?;
                    let nil = s.named(b"nil")?;
                    s.call(q, store, b"cellFor:attr:", &[nil, nil])?
                }
                Value::Export(n) if n != b"CodeObjectVt" => {
                    if !matches!(
                        n.as_slice(),
                        b"ObjectVt"
                            | b"SystemVt"
                            | b"VectorVt"
                            | b"VectorNode"
                            | b"VectorNodeVt"
                            | b"MapVt"
                            | b"MapNode"
                            | b"MapNodeVt"
                            | b"RangeVt"
                            | b"ErrorVt"
                    ) {
                        return Err(bad());
                    }
                    if n == b"SystemVt" && !node.fields.is_empty() {
                        return Err(bad());
                    }
                    let vt = q.keep(s.named(n)?)?;
                    object(
                        s.send(
                            s.model,
                            b"allocate:bytes:",
                            &[
                                vt,
                                s.word(
                                    node.fields
                                        .len()
                                        .checked_mul(size_of::<usize>())
                                        .ok_or_else(bad)?,
                                )?,
                            ],
                        )?,
                    )?
                }
                _ => continue,
            };
            objects[i] = q.keep(v)?;
            if matches!(&node.locator,Value::Export(n) if n==b"ObjectVt") && !node.fields.is_empty()
            {
                let vt = object(s.send(s.id.root, b"ensurePrivateVtableFor:", &[v])?)?;
                q.keep(vt)?;
                let kinds = node
                    .fields
                    .iter()
                    .map(|f| u8::from(!matches!(f, Value::Raw(_))))
                    .collect::<Vec<_>>();
                s.send(
                    s.model,
                    b"setLayoutOf:to:rest:",
                    &[vt, q.keep(s.byte_value(&kinds)?)?, s.word(0)?],
                )?;
            }
        }
        // Immutable provenance can itself reference a CodeObject or Block.
        // Construct that dependency graph before wiring mutable slots/captures.
        let mut remaining = objects.iter().filter(|o| o.is_null()).count();
        while remaining != 0 {
            let before = remaining;
            for (i, node) in nodes.iter().enumerate() {
                if !objects[i].is_null()
                    || !matches!(&node.locator,Value::Export(n) if n==b"CodeObjectVt")
                {
                    continue;
                }
                let (index, provenance, signature) = match node.fields.as_slice() {
                    [Value::Program(index), provenance] => (*index, provenance, None),
                    [Value::Program(index), provenance, Value::Bytes(signature)] => {
                        (*index, provenance, Some(signature))
                    }
                    _ => return Err(bad()),
                };
                if let Value::Heap(index) = provenance {
                    if objects.get(*index as usize).ok_or_else(bad)?.is_null() {
                        continue;
                    }
                }
                let bytes = s.text(
                    q,
                    b"Bytes",
                    inventory.program(index as usize).ok_or_else(bad)?,
                )?;
                let code = s.call(q, s.named(b"CodeObject")?, b"fromCanonicalBytes:", &[bytes])?;
                if s.is_error(code)? {
                    return Err(bad());
                }
                let provenance = resolve(s, q, provenance, &objects)?;
                objects[i] = if provenance == s.named(b"nil")? {
                    code
                } else {
                    let program = s.call(q, code, b"program", &[])?;
                    s.call(
                        q,
                        s.named(b"CodeObject")?,
                        b"fromProgram:provenance:",
                        &[program, provenance],
                    )?
                };
                if let Some(signature) = signature {
                    let signature = s.text(q, b"Bytes", signature)?;
                    objects[i] = s.call(q, objects[i], b"withNativeSignature:", &[signature])?;
                    if s.is_error(objects[i])? {
                        return Err(bad());
                    }
                }
                remaining -= 1;
            }
            // Allocate every Block before wiring captures, including cycles through slots.
            for (i, node) in nodes.iter().enumerate() {
                if !objects[i].is_null() {
                    continue;
                }
                if let Value::Block(index, sync) = node.locator {
                    let code = *objects.get(index as usize).ok_or_else(bad)?;
                    if code.is_null() {
                        continue;
                    }
                    let Some(Value::Program(program)) = nodes[index as usize].fields.first() else {
                        return Err(bad());
                    };
                    let method = s.call(
                        q,
                        s.named(b"Codegen")?,
                        b"portableImageBlockMethod:synchronous:",
                        &[
                            s.integer(q, *program as usize)?,
                            s.named(if sync { b"true" } else { b"false" })?,
                        ],
                    )?;
                    objects[i] = s.call(
                        q,
                        s.named(b"Block")?,
                        b"imageShellFromMethod:codeObject:",
                        &[method, code],
                    )?;
                    remaining -= 1;
                }
            }
            if before == remaining {
                return Err(bad());
            }
        }
        for (i, node) in nodes.iter().enumerate() {
            match &node.locator {
                Value::Block(_, _) => {
                    let mut captures = Vec::new();
                    for value in &node.fields[1..] {
                        captures.push(resolve(s, q, value, &objects)?);
                    }
                    let captures = s.vector(q, &captures)?;
                    s.call(
                        q,
                        s.named(b"Block")?,
                        b"restoreImageCaptures:of:",
                        &[captures, objects[i]],
                    )?;
                }
                Value::Export(n) if n == b"CellVt" => {
                    let mut fields = Vec::new();
                    for field in &node.fields {
                        fields.push(resolve(s, q, field, &objects)?);
                    }
                    s.call(q, objects[i], b"restoreImageStore:entity:attr:", &fields)?;
                }
                Value::Export(n) if n != b"CodeObjectVt" => {
                    for (slot, field) in node.fields.iter().enumerate() {
                        let value = if let Value::Raw(n) = field {
                            s.word(*n as usize)?
                        } else {
                            let v = resolve(s, q, field, &objects)?;
                            if v.is_null() { ptr::null_mut() } else { v }
                        };
                        s.send(
                            s.model,
                            b"restoreImageSlotAt:in:put:",
                            &[s.word(slot)?, objects[i], value],
                        )?;
                    }
                }
                _ => (),
            }
        }
        if let Some(bytes) = decoded.behaviors {
            let links = super::behavior_links::decode(bytes).map_err(|_| bad())?;
            let mut private = HashMap::new();
            // Allocate every owner behavior before wiring inheritance or sharing.
            for link in links.iter().filter(|link| link.object == link.owner) {
                let owner = *objects.get(link.owner as usize).ok_or_else(bad)?;
                let behavior = q.keep(object(s.send(
                    s.id.root,
                    b"ensurePrivateVtableFor:",
                    &[owner],
                )?)?)?;
                if matches!(&nodes[link.owner as usize].locator, Value::Export(name) if name == b"ObjectVt")
                {
                    let kinds = nodes[link.owner as usize]
                        .fields
                        .iter()
                        .map(|field| u8::from(!matches!(field, Value::Raw(_))))
                        .collect::<Vec<_>>();
                    s.send(
                        s.model,
                        b"setLayoutOf:to:rest:",
                        &[behavior, q.keep(s.byte_value(&kinds)?)?, s.word(0)?],
                    )?;
                }
                private.insert(link.owner, behavior);
            }
            for link in &links {
                let behavior = *private.get(&link.owner).ok_or_else(bad)?;
                if let Some(parent) = &link.parent {
                    let parent = match parent {
                        super::behavior_links::Parent::Export(name) => q.keep(s.named(name)?)?,
                        super::behavior_links::Parent::Owner(index) => {
                            *private.get(index).ok_or_else(bad)?
                        }
                    };
                    s.send(s.id.root, b"parentOf:put:", &[behavior, parent])?;
                } else {
                    s.send(
                        s.id.root,
                        b"reassociateBehaviorOf:with:",
                        &[objects[link.object as usize], behavior],
                    )?;
                }
            }
        }
        Ok(objects)
    }
}
unsafe fn restore_facts(
    s: &State,
    q: &Scope<'_>,
    store: Oop,
    decoded: &v8::DecodedImage<'_>,
    objects: &[Oop],
) -> Result<Oop, Failure> {
    unsafe {
        // All fact decoding and fallible construction happen in a separate Store.
        let staged = s.call(q, store, b"new", &[])?;
        s.call(q, staged, b"imageLoadBegin", &[])?;
        if decoded.facts.len() > 1 {
            return Err(bad());
        }
        for bytes in &decoded.facts {
            let u64_at = |i| -> Result<usize, Failure> {
                usize::try_from(u64::from_le_bytes(
                    bytes
                        .get(i..i + 8)
                        .ok_or_else(bad)?
                        .try_into()
                        .map_err(|_| bad())?,
                ))
                .map_err(|_| bad())
            };
            let (basis, oldest, argument) = (u64_at(0)?, u64_at(8)?, u64_at(24)?);
            let policy = *bytes.get(16).ok_or_else(bad)?;
            let count = u32::from_le_bytes(
                bytes
                    .get(32..36)
                    .ok_or_else(bad)?
                    .try_into()
                    .map_err(|_| bad())?,
            );
            let mut offset = 36;
            for _ in 0..count {
                let history = bytes[offset] != 0;
                let op = bytes[offset + 1] as usize;
                let tx = u64_at(offset + 2)?;
                offset += 10;
                let mut fields = Vec::new();
                for _ in 0..3 {
                    fields.push(resolve(s, q, &Value::decode(bytes, &mut offset)?, objects)?);
                }
                fields.extend([
                    s.integer(q, tx)?,
                    s.integer(q, op)?,
                    s.named(if history { b"true" } else { b"false" })?,
                ]);
                let result = s.call(
                    q,
                    staged,
                    b"imageAddFact:attr:value:tx:op:historied:",
                    &fields,
                )?;
                if result != staged {
                    return Err(bad());
                }
            }
            if offset != bytes.len() {
                return Err(bad());
            }
            let result = s.call(
                q,
                staged,
                b"imageLoadEnd:raw:oldest:policy:argument:",
                &[
                    s.integer(q, basis)?,
                    s.named(b"false")?,
                    s.integer(q, oldest)?,
                    s.integer(q, policy as usize)?,
                    s.integer(q, argument)?,
                ],
            )?;
            if result != staged {
                return Err(bad());
            }
        }
        Ok(staged)
    }
}
unsafe fn load(
    s: &State,
    q: &Scope<'_>,
    bytes: Vec<u8>,
    rebuild: bool,
    mapped_only: bool,
    startup: bool,
) -> Result<Oop, Failure> {
    unsafe {
        let mut owned = format::inspect(s, &bytes)?;
        // Validation returned independent snapshots; the file buffer is dead.
        drop(bytes);
        // Activation follows declared metadata, never ambient Store discovery.
        let providers = owned.providers.as_deref().ok_or_else(bad)?;
        let count = u32::from_le_bytes(
            providers
                .get(..4)
                .ok_or_else(bad)?
                .try_into()
                .map_err(|_| bad())?,
        );
        let mut offset = 4;
        let mut names = Vec::new();
        for _ in 0..count {
            let Value::Symbol(name) = Value::decode(providers, &mut offset)? else {
                return Err(bad());
            };
            names.push(q.keep(s.intern(&name)?)?);
        }
        if offset != providers.len() {
            return Err(bad());
        }
        let manager = s.call(
            q,
            s.named(b"Globals")?,
            b"at:",
            &[q.keep(s.intern(b"Module")?)?],
        )?;
        let names = s.vector(q, &names)?;
        let ready = s.call(q, manager, b"loadImageProviders:", &[names])?;
        if s.is_error(ready)? {
            return Err(Failure::Value(ready));
        }
        if ready != manager {
            return Err(bad());
        }
        let composition = composition_identity(&owned.view());
        let native_code = owned.code.take();
        let decoded = owned.view();
        // Current images identify their ordinary source owner explicitly.
        // Older source-less images need rebuilding, not a hidden native fallback.
        if decoded.sources.is_none() {
            return Err(bad());
        }
        if mapped_only && native_code.is_none() {
            return Err(bad());
        }
        let codegen = q.keep(s.named(b"Codegen")?)?;
        let yes = q.keep(s.named(b"true")?)?;
        let p = s.text(q, b"Bytes", decoded.programs)?;
        let b = s.text(q, b"Bytes", decoded.bindings)?;
        let selection = s.text(q, b"Bytes", decoded.selection)?;
        let mapped = !rebuild && native_code.is_some();
        let loaded_result = q.keep(s.named(if mapped { b"true" } else { b"false" })?)?;
        let started = if mapped {
            let input = Scope::new(s);
            let code = s.text(
                &input,
                b"Bytes",
                native_code
                    .as_deref()
                    .ok_or_else(bad)?
                    .get(72..)
                    .ok_or_else(bad)?,
            )?;
            // The current factory has consumed our borrow. Its owned carrier
            // is pinned independently; do not retain the Rust copy as well.
            drop(native_code);
            s.call(
                q,
                codegen,
                b"beginPortableImageMapped:programs:bindings:selection:",
                &[code, p, b, selection],
            )?
        } else {
            drop(native_code);
            s.call(
                q,
                codegen,
                b"beginPortableImageRebuild:bindings:selection:",
                &[p, b, selection],
            )?
        };
        if started != yes {
            return Err(bad());
        }
        struct Stage<'a> {
            s: &'a State,
            codegen: Oop,
        }
        impl Drop for Stage<'_> {
            fn drop(&mut self) {
                unsafe {
                    self.s
                        .send(self.codegen, b"cancelPortableImageUnit", &[])
                        .expect("cancel Image staging");
                }
            }
        }
        let _stage = Stage { s, codegen };
        let objects = restore_heap(s, q, &decoded)?;
        let (foreign_owner, fact_owner) = {
            let roots = decoded.sources.ok_or_else(bad)?;
            let mut offset = 0;
            let roots = resolve(s, q, &Value::decode(roots, &mut offset)?, &objects)?;
            let fields = s.sequence(q, roots)?;
            let [tag, sources, foreign, store] = fields.as_slice() else {
                return Err(bad());
            };
            if *tag != s.intern(b"imageOwnersV2")? {
                return Err(bad());
            }
            if s.call(q, codegen, b"preparePortableImageSources:", &[*sources])? != yes {
                return Err(bad());
            }
            (*foreign, (*store != s.named(b"nil")?).then_some(*store))
        };
        if decoded.facts.len() != usize::from(fact_owner.is_some()) {
            return Err(bad());
        }
        // Admission of malformed input is separate from ordinary Cell dispatch.
        for node in heap_nodes(&decoded)? {
            if matches!(&node.locator, Value::Export(name) if name == b"CellVt") {
                if fact_owner != Some(resolve(s, q, &node.fields[0], &objects)?) {
                    return Err(bad());
                }
            }
        }
        let staged = fact_owner
            .map(|store| restore_facts(s, q, store, &decoded, &objects))
            .transpose()?;
        let inventory = bindings::inspect(decoded.bindings).map_err(|_| bad())?;
        let mut publication = Vec::new();
        for i in 0..inventory.binding_count as usize {
            let binding = inventory.binding_spec(i).ok_or_else(bad)?;
            let target = resolve(s, q, &Value::from_binding(binding.target), &objects)?;
            let implementation =
                resolve(s, q, &Value::from_binding(binding.implementation), &objects)?;
            let provenance = resolve(s, q, &Value::from_binding(binding.provenance), &objects)?;
            let selector = q.keep(s.intern(binding.selector)?)?;
            let index = s.integer(q, i)?;
            if s.call(
                q,
                codegen,
                b"preparePortableImageBinding:implementation:",
                &[index, implementation],
            )? != yes
            {
                return Err(bad());
            }
            let behavior = s.method_behavior(target)?;
            q.keep(behavior)?;
            publication.push((index, target, selector, provenance, behavior));
        }
        // A restored snapshot must also remove later source-defined bindings on
        // resident exports. Old heap objects held by callers keep their methods.
        // Only resident binding keys are needed here. Query their actual owners;
        // constructing a full code/provenance report creates avoidable heap work.
        let rows = s.sequence(q, s.call(q, codegen, b"portableImageBindings", &[])?)?;
        let mut removals = Vec::new();
        for row in rows {
            let row = s.sequence(q, row)?;
            if row.len() < 4 {
                return Err(bad());
            }
            let target = row[0];
            let selector = row[1];
            if s.export(target)?.is_none()
                || publication
                    .iter()
                    .any(|&(_, t, n, _, _)| t == target && n == selector)
            {
                continue;
            }
            let behavior = q.keep(s.method_behavior(target)?)?;
            removals.push((
                ptr::null_mut(),
                target,
                selector,
                s.named(b"nil")?,
                behavior,
            ));
        }
        removals.extend(publication);
        let publication = removals;
        if s.call(q, codegen, b"finishPortableImageUnit", &[])? != yes {
            return Err(bad());
        }
        // The admitted heap locator names the representation family. Restored
        // private/shared behavior layers do not change a Map's hash table.
        let maps = heap_nodes(&decoded)?
            .iter()
            .zip(&objects)
            .filter_map(|(node, &o)| {
                matches!(&node.locator, Value::Export(name) if name == b"MapVt").then_some(o)
            })
            .collect::<Vec<_>>();
        // A replaced begin operation can invalidate capabilities before returning
        // failure. Include that call in the partial-application boundary itself.
        // Earlier staging failures leave the old capabilities in service.
        let result = (|| -> Result<(), Failure> {
            let admitted = s.call(q, codegen, b"beginPortableImageApplication", &[])?;
            if admitted != yes {
                return Err(if s.is_error(admitted)? {
                    Failure::Value(admitted)
                } else {
                    bad()
                });
            }
            if let (Some(store), Some(staged)) = (fact_owner, staged) {
                if s.call(q, store, b"swapImageStateWith:", &[staged])? != store {
                    return Err(bad());
                }
            }
            for &(index, target, selector, provenance, behavior) in &publication {
                let published = (|| {
                    if index.is_null() {
                        let removed = s.send(behavior, b"removeMethod:", &[selector])?;
                        if let Some(value) = optional(removed) {
                            if s.is_error(value)? {
                                return Err(Failure::Value(removed));
                            }
                        }
                        Ok(target)
                    } else {
                        s.call(
                            q,
                            codegen,
                            b"installPortableImageBinding:on:as:source:",
                            &[index, target, selector, provenance],
                        )
                    }
                })()
                .and_then(|value| {
                    if value == target {
                        Ok(value)
                    } else if s.is_error(value)? {
                        Err(Failure::Value(value))
                    } else {
                        Err(bad())
                    }
                });
                if published? != target {
                    return Err(bad());
                }
            }
            // Only captured-receiver preparation runs before hashes are repaired.
            // Keep newly installed Methods as temporary image-operation roots;
            // the authored index stores identity stamps, never these Methods.
            let mut pending = Vec::new();
            if foreign_owner != s.named(b"nil")? {
                let rows = s.call(q, foreign_owner, b"prepareImageRecipes", &[])?;
                if s.is_error(rows)? {
                    return Err(Failure::Value(rows));
                }
                for recipe in s.sequence(q, rows)? {
                    let method = s.call(q, s.named(b"Alien")?, b"installRecipe:", &[recipe])?;
                    if s.is_error(method)? {
                        return Err(Failure::Value(method));
                    }
                    pending.push((recipe, method));
                }
            }
            for &o in &maps {
                s.call(q, s.named(b"Map")?, b"rehashImage:", &[o])?;
            }
            if foreign_owner != s.named(b"nil")? {
                let mut rows = s.call(q, s.named(b"Vector")?, b"new", &[])?;
                for (recipe, method) in pending {
                    let mut row = s.call(q, s.named(b"Vector")?, b"new", &[])?;
                    row = s.call(q, row, b"add:", &[recipe])?;
                    row = s.call(q, row, b"add:", &[method])?;
                    rows = s.call(q, rows, b"add:", &[row])?;
                }
                let indexed = s.call(q, foreign_owner, b"indexRestoredRecipes:", &[rows])?;
                if indexed != foreign_owner {
                    return Err(if s.is_error(indexed)? {
                        Failure::Value(indexed)
                    } else {
                        bad()
                    });
                }
            }
            // Globals and private Methods now refer to the restored ordinary
            // source owner. Validate it; never install a second native registry.
            if decoded.sources.is_some() {
                let checked = s.call(q, codegen, b"finishPortableImageSources", &[])?;
                if checked != yes {
                    return Err(if s.is_error(checked)? {
                        Failure::Value(checked)
                    } else {
                        bad()
                    });
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            if let Failure::Native(value) | Failure::Value(value) = error {
                q.keep(value)?;
            }
            // Publication may already have committed, including before a failing
            // callback. Never restore old capabilities over the resulting world.
            // Staging cleanup still releases temporary roots; entered/captured
            // code remains owned by Execution. No user callback is replayed.
            *s.composition.borrow_mut() = None;
            *s.report.borrow_mut() = Report {
                status: if startup {
                    b"startupFailed"
                } else {
                    b"partialLoad"
                },
                detail: format!(
                    "image load failed (failure={:?}); applied changes retained; {}",
                    error,
                    if startup {
                        "discard the startup world"
                    } else {
                        "world may be partially loaded; no rollback was attempted"
                    }
                )
                .into_bytes(),
                ..Report::default()
            };
            return Err(error);
        }
        *s.composition.borrow_mut() = Some(composition);
        Ok(loaded_result)
    }
}
#[derive(Clone, Copy)]
enum Op {
    StartMapped,
    Start,
    StartBytes,
    LoadMapped,
    Save,
    Load,
    Rebuild,
    LoadBytes,
    Report,
    Status,
    Detail,
    Saved,
    Zero,
    Code,
    Identity,
}
unsafe fn operation(c: Oop, r: Oop, a: *const Oop, n: usize, op: Op) -> Oop {
    done((|| unsafe {
        let s = state(c)?;
        let args = borrowed_slice(a, n).ok_or_else(bad)?;
        let q = Scope::new(s);
        q.keep(r)?;
        for &a in args {
            q.keep(object(a)?)?;
        }
        let expected = match op {
            Op::Save | Op::Load | Op::LoadMapped | Op::StartMapped => 1,
            Op::Rebuild | Op::LoadBytes | Op::Start | Op::StartBytes => 2,
            _ => 0,
        };
        if n != expected {
            return Err(bad());
        }
        match op {
            Op::Report => return Ok(s.receiver),
            Op::Status => {
                let status = s.report.borrow().status;
                return Ok(s.intern(if status.is_empty() { b"none" } else { status })?);
            }
            Op::Detail => {
                let text = s.report.borrow().detail.clone();
                return s.text(&q, b"String", &text);
            }
            Op::Saved | Op::Zero | Op::Code => {
                let n = {
                    let report = s.report.borrow();
                    match op {
                        Op::Saved => report.saved,
                        Op::Code => report.code,
                        _ => 0,
                    }
                };
                return s.integer(&q, n);
            }
            Op::Identity => {
                let identity = s.composition.borrow().clone();
                return match identity {
                    Some(bytes) => s.text(&q, b"String", &bytes),
                    None => Ok(s.named(b"nil")?),
                };
            }
            _ => (),
        }
        if s.busy.replace(true) {
            return Err(bad());
        }
        struct Busy<'a>(&'a State);
        impl Drop for Busy<'_> {
            fn drop(&mut self) {
                self.0.facts.borrow_mut().clear();
                self.0.busy.set(false);
            }
        }
        let _busy = Busy(s);
        *s.report.borrow_mut() = Report::default();
        let result = (|| -> Result<Oop, Failure> {
            let bytes = s.bytes(object(args[0])?)?;
            if matches!(op, Op::Save) {
                if s.behavior(object(args[0])?)? != s.named(b"StringVt")? {
                    return Err(bad());
                }
                return save(s, &q, r, Path::new(std::ffi::OsStr::from_bytes(&bytes)));
            }
            let bytes = if matches!(op, Op::LoadBytes | Op::StartBytes) {
                bytes
            } else {
                if s.behavior(object(args[0])?)? != s.named(b"StringVt")? {
                    return Err(bad());
                }
                let path = Path::new(std::ffi::OsStr::from_bytes(&bytes));
                let metadata = std::fs::metadata(path).map_err(|_| bad())?;
                if !metadata.is_file() || metadata.len() > 1 << 30 {
                    return Err(bad());
                }
                std::fs::read(path).map_err(|_| bad())?
            };
            let rebuild = n == 2 && object(args[1])? == s.named(b"true")?;
            load(
                s,
                &q,
                bytes,
                rebuild,
                matches!(op, Op::LoadMapped | Op::StartMapped),
                matches!(op, Op::Start | Op::StartMapped | Op::StartBytes),
            )
        })();
        if result.is_err() && !matches!(s.report.borrow().status, b"partialLoad" | b"startupFailed")
        {
            s.reject(b"Image admission, owner operation or publication failed");
        }
        match result {
            Err(Failure::Rejected) => Ok(s.named(b"nil")?),
            result => result,
        }
    })())
}
unsafe extern "C" fn collect_fact(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    done((|| unsafe {
        let s = state(c)?;
        if s.thread != std::thread::current().id() || !s.busy.get() {
            return Err(bad());
        }
        let Some([e, attr, v, tx, op, h]) = borrowed_slice(a, n) else {
            return Err(bad());
        };
        let values = [object(*e)?, object(*attr)?, object(*v)?];
        // Install references before Integer conversion can collect or reenter.
        let index = s.facts.borrow().len();
        s.facts.borrow_mut().push(Fact {
            values,
            tx: 0,
            op: 0,
            history: false,
        });
        let tx = s.number(object(*tx)?)?;
        let op = s.number(object(*op)?)?;
        if op > 1 {
            return Err(bad());
        }
        let history = object(*h)? == s.named(b"true")?;
        s.facts.borrow_mut()[index] = Fact {
            values,
            tx: tx as u64,
            op: op as u8,
            history,
        };
        Ok(ptr::null_mut())
    })())
}
unsafe extern "C" fn enumerate_roots(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    done((|| unsafe {
        let s = state(c)?;
        let Some([visitor]) = borrowed_slice(a, n) else {
            return Err(bad());
        };
        let visitor = object(*visitor)?;
        let mut values = s.permanent.clone();
        values.extend(s.names.values().copied());
        values.extend(s.scoped.borrow().iter().copied());
        values.extend(s.facts.borrow().iter().flat_map(|f| f.values));
        for value in values {
            s.send(visitor, b"visit:", &[value])?;
        }
        Ok(ptr::null_mut())
    })())
}
unsafe extern "C" fn op_save(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::Save) }
}
unsafe extern "C" fn op_start(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::Start) }
}
unsafe extern "C" fn op_startmapped(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::StartMapped) }
}
unsafe extern "C" fn op_startbytes(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::StartBytes) }
}
unsafe extern "C" fn op_load(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::Load) }
}
unsafe extern "C" fn op_rebuild(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::Rebuild) }
}
unsafe extern "C" fn op_loadmapped(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::LoadMapped) }
}
unsafe extern "C" fn op_loadbytes(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::LoadBytes) }
}
unsafe extern "C" fn op_report(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::Report) }
}
unsafe extern "C" fn op_identity(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::Identity) }
}
unsafe extern "C" fn op_status(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::Status) }
}
unsafe extern "C" fn op_detail(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::Detail) }
}
unsafe extern "C" fn op_saved(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::Saved) }
}
unsafe extern "C" fn op_zero(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::Zero) }
}
unsafe extern "C" fn op_code(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { operation(c, r, a, n, Op::Code) }
}
unsafe extern "C" fn initialize(
    p: *const Protocol,
    image: Oop,
    l: *const Oop,
    n: usize,
    a: *const Oop,
    count: usize,
) -> Oop {
    let result = (|| -> Result<Oop, Failure> {
        unsafe {
            let id = *p;
            let [ns] = borrowed_slice(a, count).ok_or_else(bad)? else {
                return Err(bad());
            };
            let symbols = borrowed_slice(l, n)
                .filter(|v| v.len() == NAMES.len())
                .ok_or_else(bad)?;
            let old = INSTANCE.load(Ordering::Acquire);
            if !old.is_null() {
                let s = &*old;
                return if s.ready.get() && s.ns == *ns && s.id.root == id.root {
                    Ok(s.receiver)
                } else {
                    Err(bad())
                };
            }
            let names: HashMap<_, _> = NAMES.iter().copied().zip(symbols.iter().copied()).collect();
            let send = |r, n: &[u8], a: &[Oop]| id.send(r, names[n], a);
            let named = |n: &[u8]| send(*ns, b"at:", &[names[n]]);
            let roots = named(b"Roots")?;
            let model = named(b"ObjectModel")?;
            let vt = send(named(b"ObjectVt")?, b"delegated", &[])?;
            let token = send(roots, b"retain:", &[vt])?;
            let receiver = send(vt, b"allocate", &[abi::size(0).unwrap()])?;
            let token2 = send(roots, b"retain:", &[receiver])?;
            let system = named(b"System")?;
            let system_vt = send(id.root, b"behaviorOf:", &[system])?;
            let s = Box::leak(Box::new(State {
                id,
                names,
                scoped: RefCell::new(Vec::new()),
                ns: *ns,
                model,
                receiver,
                permanent: vec![*ns, roots, model, receiver, vt, system, system_vt],
                busy: Cell::new(false),
                ready: Cell::new(false),
                facts: RefCell::new(Vec::new()),
                report: RefCell::new(Report::default()),
                composition: RefCell::new(None),
                thread: std::thread::current().id(),
            }));
            INSTANCE.store(s, Ordering::Release);
            for (index, (is_system, name)) in INSTALL.iter().enumerate() {
                let method = s.send(image, b"newMethodAt:", &[abi::size(index).unwrap()])?;
                let method = s.send(method, b"withContext:", &[*ns])?;
                s.send(
                    if *is_system { system_vt } else { vt },
                    b"addMethod",
                    &[s.intern(name)?, method],
                )?;
            }
            s.send(roots, b"register:", &[receiver])?;
            for (name, value) in [
                (b"Image".as_slice(), receiver),
                (b"ImageSaveReport", receiver),
                (b"ImageSaveReportVt", vt),
            ] {
                s.send(*ns, b"at:put:", &[s.intern(name)?, value])?;
            }
            s.send(roots, b"release:", &[token])?;
            s.send(roots, b"release:", &[token2])?;
            s.ready.set(true);
            Ok(receiver)
        }
    })();
    match result {
        Err(Failure::Rejected) => unsafe { (*p).invalid },
        v => done(v),
    }
}
static INSTALL: &[(bool, &[u8])] = &[
    (true, b"saveImage:"),
    (true, b"startImage:rebuild:"),
    (true, b"startImageMapped:"),
    (true, b"startImageBytes:rebuild:"),
    (true, b"loadImage:"),
    (true, b"loadImage:rebuild:"),
    (true, b"loadImageMapped:"),
    (true, b"loadImageBytes:rebuild:"),
    (true, b"imageSaveReport"),
    (true, b"imageCompositionIdentity"),
    (false, b"status"),
    (false, b"detail"),
    (false, b"savedFacts"),
    (false, b"recoveredFacts"),
    (false, b"rejectedFacts"),
    (false, b"recoveredSlots"),
    (false, b"rejectedSlots"),
    (false, b"codeUnits"),
    (false, b"rootsDo:"),
    (false, b"e:a:v:tx:op:historied:"),
];
macro_rules! literals { ($($v:literal),*$(,)?) => {
 const NAMES:&[&[u8]]=&[$($v),*];
 static LITERALS:&[Literal]=&[$(Literal::new($v)),*];
}; }
literals![
    b"nameWithBytes",
    b"at:",
    b"bytes",
    b"sizeWord",
    b"atWord:",
    b"symbolBytesOf:",
    b"sequence",
    b"fromBytes:",
    b"Word",
    b"fromUnsignedLow:high:",
    b"low32",
    b"high32",
    b"Integer",
    b"fromWord:",
    b"wordOf:",
    b"Vector",
    b"new",
    b"add:",
    b"size",
    b"nameOf:",
    b"behaviorOf:",
    b"isError",
    b"true",
    b"rejected",
    b"Id-oop-image-v3",
    b"0123456789abcdef",
    b"nil",
    b"false",
    b"SymbolVt",
    b"imagePayload",
    b"parentOf:",
    b"ObjectVt",
    b"BlockVt",
    b"ownerOf:",
    b"IntegerVt",
    b"FloatVt",
    b"WordVt",
    b"Float",
    b"bitsOf:",
    b"bits",
    b"StringVt",
    b"BytesVt",
    b"CellVt",
    b"imageFieldAt:",
    b"CodeObjectVt",
    b"CodeObject",
    b"canonicalBytesOf:",
    b"sourceProvenance",
    b"nativeSignature",
    b"codeObject",
    b"isSynchronous",
    b"captureCount",
    b"captureAt:",
    b"SystemVt",
    b"VectorVt",
    b"VectorNodeVt",
    b"VectorNode",
    b"MapVt",
    b"MapNodeVt",
    b"MapNode",
    b"RangeVt",
    b"ErrorVt",
    b"imageSizeOf:",
    b"imageSlotAt:in:",
    b"imageSlotKindAt:in:",
    b"imageStateRevision",
    b"Globals",
    b"bindingBasis",
    b"bindingEpoch",
    b"SourceRegistry",
    b"revision",
    b"Module",
    b"registry",
    b"Generation",
    b"prepareImageSnapshot",
    b"imageStore",
    b"imageProviders",
    b"basis",
    b"oldestRetainedBasis",
    b"retentionPolicyCode",
    b"retentionPolicyArgument",
    b"imageFactsInto:",
    b"Codegen",
    b"portableImageBindings",
    b"portableImageSources",
    b"imageOwnersV2",
    b"AlienRecipes",
    b"Bytes",
    b"compilePortableImageCode:bindings:selection:",
    b"saved",
    b"fromBits:",
    b"String",
    b"cellFor:attr:",
    b"allocate:bytes:",
    b"ensurePrivateVtableFor:",
    b"setLayoutOf:to:rest:",
    b"fromCanonicalBytes:",
    b"program",
    b"fromProgram:provenance:",
    b"withNativeSignature:",
    b"portableImageBlockMethod:synchronous:",
    b"Block",
    b"imageShellFromMethod:codeObject:",
    b"restoreImageCaptures:of:",
    b"restoreImageStore:entity:attr:",
    b"restoreImageSlotAt:in:put:",
    b"parentOf:put:",
    b"reassociateBehaviorOf:with:",
    b"imageLoadBegin",
    b"imageAddFact:attr:value:tx:op:historied:",
    b"imageLoadEnd:raw:oldest:policy:argument:",
    b"loadImageProviders:",
    b"beginPortableImageMapped:programs:bindings:selection:",
    b"beginPortableImageRebuild:bindings:selection:",
    b"cancelPortableImageUnit",
    b"preparePortableImageSources:",
    b"preparePortableImageBinding:implementation:",
    b"behaviorCount",
    b"behaviorAt:",
    b"selectorAt:in:",
    b"bindingOf:in:",
    b"Execution",
    b"implementationOfEntry:",
    b"finishPortableImageUnit",
    b"beginPortableImageApplication",
    b"swapImageStateWith:",
    b"removeMethod:",
    b"installPortableImageBinding:on:as:source:",
    b"prepareImageRecipes",
    b"Alien",
    b"installRecipe:",
    b"Map",
    b"rehashImage:",
    b"indexRestoredRecipes:",
    b"finishPortableImageSources",
    b"startupFailed",
    b"partialLoad",
    b"none",
    b"visit:",
    b"Roots",
    b"ObjectModel",
    b"delegated",
    b"retain:",
    b"allocate",
    b"System",
    b"newMethodAt:",
    b"withContext:",
    b"addMethod",
    b"register:",
    b"Image",
    b"ImageSaveReport",
    b"ImageSaveReportVt",
    b"at:put:",
    b"release:",
    b"saveImage:",
    b"startImage:rebuild:",
    b"startImageMapped:",
    b"startImageBytes:rebuild:",
    b"loadImage:",
    b"loadImage:rebuild:",
    b"loadImageMapped:",
    b"loadImageBytes:rebuild:",
    b"imageSaveReport",
    b"imageCompositionIdentity",
    b"status",
    b"detail",
    b"savedFacts",
    b"recoveredFacts",
    b"rejectedFacts",
    b"recoveredSlots",
    b"rejectedSlots",
    b"codeUnits",
    b"rootsDo:",
    b"e:a:v:tx:op:historied:",
    b"kind",
    b"imageRejected",
    b"ImageFormat",
    b"inspect:architecture:abi:",
    b"compose:",
    b"isBehavior:",
];
static METHODS: &[MethodSpec] = &[
    MethodSpec {
        name: 154,
        entry: Some(op_save),
    },
    MethodSpec {
        name: 155,
        entry: Some(op_start),
    },
    MethodSpec {
        name: 156,
        entry: Some(op_startmapped),
    },
    MethodSpec {
        name: 157,
        entry: Some(op_startbytes),
    },
    MethodSpec {
        name: 158,
        entry: Some(op_load),
    },
    MethodSpec {
        name: 159,
        entry: Some(op_rebuild),
    },
    MethodSpec {
        name: 160,
        entry: Some(op_loadmapped),
    },
    MethodSpec {
        name: 161,
        entry: Some(op_loadbytes),
    },
    MethodSpec {
        name: 162,
        entry: Some(op_report),
    },
    MethodSpec {
        name: 163,
        entry: Some(op_identity),
    },
    MethodSpec {
        name: 164,
        entry: Some(op_status),
    },
    MethodSpec {
        name: 165,
        entry: Some(op_detail),
    },
    MethodSpec {
        name: 166,
        entry: Some(op_saved),
    },
    MethodSpec {
        name: 167,
        entry: Some(op_zero),
    },
    MethodSpec {
        name: 168,
        entry: Some(op_zero),
    },
    MethodSpec {
        name: 169,
        entry: Some(op_zero),
    },
    MethodSpec {
        name: 170,
        entry: Some(op_zero),
    },
    MethodSpec {
        name: 171,
        entry: Some(op_code),
    },
    MethodSpec {
        name: 172,
        entry: Some(enumerate_roots),
    },
    MethodSpec {
        name: 173,
        entry: Some(collect_fact),
    },
];
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
#[unsafe(no_mangle)]
pub extern "C" fn birnam_image_rust_identity() -> *const core::ffi::c_char {
    c"rust-image-v8".as_ptr()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
