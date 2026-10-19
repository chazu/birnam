//! Image-v8 integration. Representations, code and facts cross owner messages.
#[path = "format.rs"]
mod format;
use super::{support, v8};
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as abi, Method, NativeEntry, Operand, Outcome, Protocol},
};
use birnam_artifact::{
    code_binding_inventory as bindings, materialization_selection as selection, portable_inventory,
};
use core::{
    cell::{Cell, RefCell},
    ffi::c_void,
    ptr,
};
use std::{collections::HashMap, os::unix::ffi::OsStrExt, path::Path};
// Distinguish our validation refusal (the public Image API answers nil) from
// an owner's native failure, which must propagate unchanged.
const IMAGE_REJECTED: usize = 0x494d4147;
fn bad() -> Outcome {
    Outcome::failed(abi::INVALID_REQUEST, IMAGE_REJECTED)
}
fn object(v: Operand) -> Result<Oop, Outcome> {
    v.as_object().filter(|o| !o.is_null()).ok_or_else(bad)
}
fn done(v: Result<Operand, Outcome>) -> Outcome {
    v.map_or_else(|e| e, Outcome::loaded)
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
    ns: Oop,
    roots: Oop,
    model: Oop,
    receiver: Oop,
    permanent: Vec<Oop>,
    busy: Cell<bool>,
    facts: RefCell<Vec<Fact>>,
    report: RefCell<Report>,
    composition: RefCell<Option<Vec<u8>>>,
    thread: std::thread::ThreadId,
}
struct Scope<'a> {
    s: &'a State,
    tokens: RefCell<Vec<usize>>,
}
impl Scope<'_> {
    unsafe fn keep(&self, o: Oop) -> Result<Oop, Outcome> {
        unsafe {
            let t = self
                .s
                .send(self.s.roots, b"retain:", &[Operand::object(o)])?
                .as_word()
                .ok_or_else(bad)?;
            self.tokens.borrow_mut().push(t);
            Ok(o)
        }
    }
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        for t in self.tokens.get_mut().drain(..) {
            unsafe {
                self.s
                    .send(self.s.roots, b"release:", &[Operand::word(t)])
                    .expect("release Image root");
            }
        }
    }
}
impl State {
    unsafe fn send(&self, r: Oop, n: &[u8], a: &[Operand]) -> Result<Operand, Outcome> {
        unsafe { self.id.send_named(r, n, a) }
    }
    unsafe fn named(&self, n: &[u8]) -> Result<Oop, Outcome> {
        unsafe { object(self.send(self.ns, b"at:", &[Operand::object(self.id.intern_name(n)?)])?) }
    }
    unsafe fn bytes(&self, o: Oop) -> Result<Vec<u8>, Outcome> {
        unsafe {
            let v = self.send(o, b"bytes", &[])?.as_bytes().ok_or_else(bad)?;
            Ok(borrowed_bytes(v.pointer, v.length)
                .ok_or_else(bad)?
                .to_vec())
        }
    }
    unsafe fn symbol(&self, o: Oop) -> Result<Vec<u8>, Outcome> {
        unsafe {
            let v = self
                .send(self.id.receiver, b"symbolBytesOf:", &[Operand::object(o)])?
                .as_bytes()
                .ok_or_else(bad)?;
            Ok(borrowed_bytes(v.pointer, v.length)
                .ok_or_else(bad)?
                .to_vec())
        }
    }
    unsafe fn text(&self, q: &Scope<'_>, family: &[u8], b: &[u8]) -> Result<Oop, Outcome> {
        unsafe {
            q.keep(object(self.send(
                self.named(family)?,
                b"fromBytes:",
                &[Operand::bytes(b)],
            )?)?)
        }
    }
    unsafe fn integer(&self, q: &Scope<'_>, v: usize) -> Result<Oop, Outcome> {
        unsafe {
            q.keep(object(self.send(
                self.named(b"Integer")?,
                b"fromWord:",
                &[Operand::word(v)],
            )?)?)
        }
    }
    unsafe fn number(&self, v: Oop) -> Result<usize, Outcome> {
        unsafe {
            self.send(self.named(b"Integer")?, b"wordOf:", &[Operand::object(v)])?
                .as_word()
                .ok_or_else(bad)
        }
    }
    unsafe fn call(&self, q: &Scope<'_>, r: Oop, n: &[u8], a: &[Oop]) -> Result<Oop, Outcome> {
        unsafe {
            q.keep(object(self.send(
                r,
                n,
                &a.iter().copied().map(Operand::object).collect::<Vec<_>>(),
            )?)?)
        }
    }
    unsafe fn vector(&self, q: &Scope<'_>, values: &[Oop]) -> Result<Oop, Outcome> {
        unsafe {
            let mut v = self.call(q, self.named(b"Vector")?, b"new", &[])?;
            for &x in values {
                v = self.call(q, v, b"add:", &[x])?;
            }
            Ok(v)
        }
    }
    unsafe fn sequence(&self, q: &Scope<'_>, v: Oop) -> Result<Vec<Oop>, Outcome> {
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
    unsafe fn export(&self, o: Oop) -> Result<Option<Vec<u8>>, Outcome> {
        unsafe {
            self.send(self.ns, b"nameOf:", &[Operand::object(o)])?
                .as_object()
                .map(|n| self.symbol(n))
                .transpose()
        }
    }
    unsafe fn behavior(&self, o: Oop) -> Result<Oop, Outcome> {
        unsafe { object(self.send(self.id.receiver, b"behaviorOf:", &[Operand::object(o)])?) }
    }
    // Match ObjectModel ensurePrivateVtableFor: without mutating dictionaries.
    // A behavior is already its own method target; its behavior is the Id.
    unsafe fn method_behavior(&self, target: Oop) -> Result<Oop, Outcome> {
        let behavior = unsafe { self.behavior(target) }?;
        Ok(if behavior == self.id.receiver {
            target
        } else {
            behavior
        })
    }
    unsafe fn is_error(&self, o: Oop) -> Result<bool, Outcome> {
        unsafe { Ok(self.send(o, b"isError", &[])?.as_object() == Some(self.named(b"true")?)) }
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
    support::wyhash(support::runtime_abi_hash(), b"Id-native-message-image-v1")
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
    fn decode(bytes: &[u8], offset: &mut usize) -> Result<Self, Outcome> {
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
    fn binding(&self) -> Result<bindings::Field<'_>, Outcome> {
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
}
impl Graph<'_, '_> {
    unsafe fn reference(&mut self, o: Oop) -> Result<Value, Outcome> {
        unsafe {
            if o.is_null() {
                return Ok(Value::Null);
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
    unsafe fn finish(&mut self) -> Result<Vec<u8>, Outcome> {
        unsafe {
            let mut i = 0;
            while i < self.objects.len() {
                let o = self.objects[i];
                // Current owner policy selects an unpublished payload or refuses
                // persistence. Returning self preserves ordinary Error values too.
                let payload = self.s.call(self.q, o, b"imagePayload", &[])?;
                if payload == self.s.named(b"false")? { return Err(bad()); }
                if payload != o && self.s.is_error(payload)? {
                    return Err(Outcome::loaded(Operand::object(payload)));
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
                        self.s.id.receiver,
                        b"parentOf:",
                        &[Operand::object(vt)],
                    )?)?)?;
                };
                // Projection changes plain object slots, never a native family's
                // representation. Their existing owner-specific codecs remain.
                if payload != o && name != b"ObjectVt" { return Err(bad()); }
                if name == b"BlockVt" {
                    // Execution reconstructs its intrinsic per-Method behavior.
                    // Additional user-defined Block behavior layers remain inadmissible.
                    if visited.len() > 1 {
                        return Err(bad());
                    }
                } else if !visited.is_empty() {
                    let owner = self.q.keep(object(self.s.send(
                        self.s.id.receiver,
                        b"ownerOf:",
                        &[Operand::object(first)],
                    )?)?)?;
                    let Value::Heap(owner_index) = self.reference(owner)? else {
                        return Err(bad());
                    };
                    let parent = if owner == o {
                        let parent = self.q.keep(object(self.s.send(
                            self.s.id.receiver,
                            b"parentOf:",
                            &[Operand::object(first)],
                        )?)?)?;
                        Some(if let Some(name) = self.s.export(parent)? {
                            super::behavior_links::Parent::Export(name)
                        } else {
                            let parent_owner = self.q.keep(object(self.s.send(
                                self.s.id.receiver,
                                b"ownerOf:",
                                &[Operand::object(parent)],
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
                            self.s
                                .send(self.s.named(family)?, selector, &[Operand::object(o)])?
                        };
                        let n = bits.as_word().ok_or_else(bad)? as u64;
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
                        let signature=self.s.call(self.q,o,b"nativeSignature",&[])?;
                        if signature!=self.s.named(b"nil")? {node.fields.push(Value::Bytes(self.s.bytes(signature)?));}
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
                        let bytes = self
                            .s
                            .send(self.s.model, b"imageSizeOf:", &[Operand::object(payload)])?
                            .as_word()
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
                            let v = self.s.send(
                                self.s.model,
                                b"imageSlotAt:in:",
                                &[Operand::word(slot), Operand::object(payload)],
                            )?;
                            node.fields.push(match v.kind() {
                                abi::UNIT => Value::Null,
                                abi::WORD => Value::Raw(v.as_word().ok_or_else(bad)? as u64),
                                abi::OBJECT => self.reference(object(v)?)?,
                                _ => return Err(bad()),
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
unsafe fn snapshot_stamp(s: &State, q: &Scope<'_>) -> Result<(Oop, Oop, Oop, usize, usize, usize, usize), Outcome> {
    unsafe {
        let store = q.keep(s.named(b"Store")?)?;
        let globals = q.keep(s.named(b"Globals")?)?;
        let facts = s.number(s.call(q, store, b"basis", &[])?)?;
        let bindings = s.number(s.call(q, globals, b"bindingBasis", &[])?)?;
        let methods = s.send(s.id.receiver, b"bindingEpoch", &[])?.as_word().ok_or_else(bad)?;
        let name = q.keep(s.id.intern_name(b"SourceRegistry")?)?;
        let sources = s.call(q, globals, b"at:", &[name])?;
        let revision = s.number(s.call(q, sources, b"revision", &[])?)?;
        Ok((store, globals, sources, facts, bindings, methods, revision))
    }
}
unsafe fn save(s: &State, q: &Scope<'_>, path: &Path) -> Result<Operand, Outcome> {
    unsafe {
        let admitted = s.call(q, s.named(b"Generation")?, b"prepareImageSnapshot", &[])?;
        if admitted != s.named(b"true")? {
            s.reject(b"Image snapshot admission failed");
            return if s.is_error(admitted)? {
                Ok(Operand::object(admitted))
            } else {
                Err(bad())
            };
        }
        let stamp = snapshot_stamp(s, q)?;
        s.facts.borrow_mut().clear();
        let store = q.keep(s.named(b"Store")?)?;
        let metadata = [
            b"basis".as_slice(),
            b"oldestRetainedBasis",
            b"retentionPolicyCode",
            b"retentionPolicyArgument",
        ]
        .map(|n| s.call(q, store, n, &[]).and_then(|o| s.number(o)));
        let [basis, oldest, policy, argument] = metadata;
        let (basis, oldest, policy, argument) = (basis?, oldest?, policy?, argument?);
        s.send(store, b"imageFactsInto:", &[Operand::object(s.receiver)])?;
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
        for row in rows {
            let values = s.sequence(q, row)?;
            if values.len() != 4 {
                return Err(bad());
            }
            selectors.push(s.symbol(values[1])?);
            binding_values.push([
                graph.reference(values[0])?,
                graph.reference(values[2])?,
                graph.reference(values[3])?,
            ]);
        }
        let sources = s.call(q, codegen, b"portableImageSources", &[])?;
        if s.is_error(sources)? { return Err(Outcome::loaded(Operand::object(sources))); }
        let mut sources_bytes = Vec::new();
        graph.reference(sources)?.encode(&mut sources_bytes);
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
            .collect::<Result<Vec<_>, Outcome>>()?;
        let bindings = bindings::encode(&specs).map_err(|_| bad())?;
        let count = portable_inventory::inspect(&programs)
            .map_err(|_| bad())?
            .program_count as usize;
        let selection = selection::encode(
            &programs,
            &bindings,
            &vec![selection::Disposition::Resumable; specs.len()],
            &vec![selection::Disposition::Resumable; count],
        )
        .map_err(|_| bad())?;
        let heap = graph.encode();
        let behaviors = super::behavior_links::encode(&graph.behaviors);
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
        let facts_bytes = writer.finish();
        // Structural checks precede the native compiler and any file publication.
        format::compose(
            s,
            q,
            v8::ImageSpec {
                sources: Some(&sources_bytes),
                behaviors: Some(&behaviors),
                architecture: support::IMAGE_ARCH_TAG,
                runtime_abi: runtime_abi(),
                programs: &programs,
                bindings: &bindings,
                selection: &selection,
                heaps: &[&heap],
                facts: &[&facts_bytes],
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
            return Ok(Operand::object(compiled));
        }
        let code = s.bytes(compiled)?;
        let bytes = format::compose(
            s,
            q,
            v8::ImageSpec {
                sources: Some(&sources_bytes),
                behaviors: Some(&behaviors),
                architecture: support::IMAGE_ARCH_TAG,
                runtime_abi: runtime_abi(),
                programs: &programs,
                bindings: &bindings,
                selection: &selection,
                heaps: &[&heap],
                facts: &[&facts_bytes],
                code: Some(&code),
            },
        )?;
        let admitted = s.call(q, s.named(b"Generation")?, b"prepareImageSnapshot", &[])?;
        if admitted != s.named(b"true")? {
            s.reject(b"Image snapshot admission changed during save");
            return if s.is_error(admitted)? {
                Ok(Operand::object(admitted))
            } else {
                Err(bad())
            };
        }
        if snapshot_stamp(s, q)? != stamp {
            s.reject(b"World changed during image snapshot");
            return Ok(Operand::object(s.named(b"nil")?));
        }
        support::publish_atomic(path, &bytes).map_err(|_| bad())?;
        *s.report.borrow_mut() = Report {
            status: b"saved",
            saved: facts.len(),
            code: count * 2 + specs.len(),
            detail: Vec::new(),
        };
        Ok(Operand::object(s.named(b"true")?))
    }
}
fn heap_nodes(decoded: &v8::DecodedImage<'_>) -> Result<Vec<Node>, Outcome> {
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
) -> Result<Oop, Outcome> {
    unsafe {
        match value {
            Value::Nil => s.named(b"nil"),
            Value::True => s.named(b"true"),
            Value::False => s.named(b"false"),
            Value::Null => Ok(ptr::null_mut()),
            Value::Symbol(b) => q.keep(s.id.intern_name(b)?),
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
) -> Result<Vec<Oop>, Outcome> {
    unsafe {
        let nodes = heap_nodes(decoded)?;
        let inventory = portable_inventory::inspect(decoded.programs).map_err(|_| bad())?;
        let mut objects = vec![ptr::null_mut(); nodes.len()];
        for (i, node) in nodes.iter().enumerate() {
            let v = match &node.locator {
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
                    object(s.send(s.named(family)?, selector, &[Operand::word(*n as usize)])?)?
                }
                Value::String(b) => s.text(q, b"String", b)?,
                Value::Bytes(b) => s.text(q, b"Bytes", b)?,
                Value::Export(n) if n == b"CellVt" => {
                    if node.fields.len() != 3 {
                        return Err(bad());
                    }
                    let nil = s.named(b"nil")?;
                    s.call(q, s.named(b"Store")?, b"cellFor:attr:", &[nil, nil])?
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
                                Operand::object(vt),
                                Operand::word(
                                    node.fields
                                        .len()
                                        .checked_mul(size_of::<usize>())
                                        .ok_or_else(bad)?,
                                ),
                            ],
                        )?,
                    )?
                }
                _ => continue,
            };
            objects[i] = q.keep(v)?;
            if matches!(&node.locator,Value::Export(n) if n==b"ObjectVt") && !node.fields.is_empty()
            {
                let vt = object(s.send(
                    s.id.receiver,
                    b"ensurePrivateVtableFor:",
                    &[Operand::object(v)],
                )?)?;
                q.keep(vt)?;
                let kinds = node
                    .fields
                    .iter()
                    .map(|f| u8::from(!matches!(f, Value::Raw(_))))
                    .collect::<Vec<_>>();
                s.send(
                    s.model,
                    b"setLayoutOf:to:rest:",
                    &[
                        Operand::object(vt),
                        Operand::bytes(&kinds),
                        Operand::word(0),
                    ],
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
                let (index,provenance,signature)=match node.fields.as_slice() {
                    [Value::Program(index),provenance]=>(*index,provenance,None),
                    [Value::Program(index),provenance,Value::Bytes(signature)]=>(*index,provenance,Some(signature)),
                    _=>return Err(bad()),
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
                if let Some(signature)=signature {
                    let signature=s.text(q,b"Bytes",signature)?;
                    objects[i]=s.call(q,objects[i],b"withNativeSignature:",&[signature])?;
                    if s.is_error(objects[i])? {return Err(bad());}
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
                            Operand::word(*n as usize)
                        } else {
                            let v = resolve(s, q, field, &objects)?;
                            if v.is_null() {
                                Operand::unit()
                            } else {
                                Operand::object(v)
                            }
                        };
                        s.send(
                            s.model,
                            b"restoreImageSlotAt:in:put:",
                            &[Operand::word(slot), Operand::object(objects[i]), value],
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
                    s.id.receiver,
                    b"ensurePrivateVtableFor:",
                    &[Operand::object(owner)],
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
                        &[
                            Operand::object(behavior),
                            Operand::bytes(&kinds),
                            Operand::word(0),
                        ],
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
                    s.send(
                        s.id.receiver,
                        b"parentOf:put:",
                        &[Operand::object(behavior), Operand::object(parent)],
                    )?;
                } else {
                    s.send(
                        s.id.receiver,
                        b"reassociateBehaviorOf:with:",
                        &[
                            Operand::object(objects[link.object as usize]),
                            Operand::object(behavior),
                        ],
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
    decoded: &v8::DecodedImage<'_>,
    objects: &[Oop],
) -> Result<Oop, Outcome> {
    unsafe {
        // All fact decoding and fallible construction happen in a separate Store.
        let staged = s.call(q, s.named(b"Store")?, b"new", &[])?;
        s.call(q, staged, b"imageLoadBegin", &[])?;
        if decoded.facts.len() > 1 {
            return Err(bad());
        }
        for bytes in &decoded.facts {
            let u64_at = |i| -> Result<usize, Outcome> {
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
// Alien wrappers are ordinary method publications in the Image transaction.
// Validate recipe shape before applying the image. No prior bindings are retained:
// live loading leaves applied changes in place on failure, just like other sends.
unsafe fn foreign_publication(
    s: &State,
    q: &Scope<'_>,
    staged: Oop,
) -> Result<Vec<(Oop, Oop)>, Outcome> {
    unsafe {
        let entity = q.keep(s.id.intern_name(b"__aliens__")?)?;
        let count = s.call(q, staged, b"attrCount:", &[entity])?;
        let count = s
            .send(s.named(b"Integer")?, b"wordOf:", &[Operand::object(count)])?
            .as_word()
            .ok_or_else(bad)?;
        let mut publication = Vec::new();
        for i in 0..count {
            let index = s.integer(q, i)?;
            let key = s.call(q, staged, b"attrAt:index:", &[entity, index])?;
            let recipe = s.call(q, staged, b"currentValue:attr:", &[entity, key])?;
            let fields = s.sequence(q, recipe)?;
            let [_, _, _, target, _] = fields.as_slice() else {
                return Err(bad());
            };
            publication.push((recipe, *target));
        }
        Ok(publication)
    }
}

unsafe fn load(
    s: &State,
    q: &Scope<'_>,
    bytes: Vec<u8>,
    rebuild: bool,
    mapped_only: bool,
    startup: bool,
) -> Result<Operand, Outcome> {
    unsafe {
        let mut owned = format::inspect(s, &bytes)?;
        // Validation returned independent snapshots; the file buffer is dead.
        drop(bytes);
        let composition = composition_identity(&owned.view());
        let native_code = owned.code.take();
        let decoded = owned.view();
        // Current images identify their ordinary source owner explicitly.
        // Older source-less images need rebuilding, not a hidden native fallback.
        if decoded.sources.is_none() { return Err(bad()); }
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
            let input = Scope {
                s,
                tokens: RefCell::new(Vec::new()),
            };
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
        if let Some(sources) = decoded.sources {
            let mut offset = 0;
            let sources = resolve(s, q, &Value::decode(sources, &mut offset)?, &objects)?;
            if s.call(q, codegen, b"preparePortableImageSources:", &[sources])? != yes {
                return Err(bad());
            }
        }
        let staged = restore_facts(s, q, &decoded, &objects)?;
        let foreign = foreign_publication(s, q, staged)?;
        let inventory = bindings::inspect(decoded.bindings).map_err(|_| bad())?;
        let mut publication = Vec::new();
        for i in 0..inventory.binding_count as usize {
            let binding = inventory.binding_spec(i).ok_or_else(bad)?;
            let target = resolve(s, q, &Value::from_binding(binding.target), &objects)?;
            let implementation =
                resolve(s, q, &Value::from_binding(binding.implementation), &objects)?;
            let provenance = resolve(s, q, &Value::from_binding(binding.provenance), &objects)?;
            let selector = q.keep(s.id.intern_name(binding.selector)?)?;
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
        let current = s.sequence(q, s.call(q, codegen, b"portableImageBindings", &[])?)?;
        let mut removals = Vec::new();
        for row in current {
            let fields = s.sequence(q, row)?;
            let [target, selector, _, _] = fields.as_slice() else {
                return Err(bad());
            };
            if s.export(*target)?.is_none()
                || publication
                    .iter()
                    .any(|&(_, t, n, _, _)| t == *target && n == *selector)
            {
                continue;
            }
            let behavior = q.keep(s.method_behavior(*target)?)?;
            removals.push((
                ptr::null_mut(),
                *target,
                *selector,
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
        let store = q.keep(s.named(b"Store")?)?;
        // A replaced begin operation can invalidate capabilities before returning
        // failure. Include that call in the partial-application boundary itself.
        // Earlier staging failures leave the old capabilities in service.
        let result = (|| -> Result<(), Outcome> {
            let admitted = s.call(q, codegen, b"beginPortableImageApplication", &[])?;
            if admitted != yes {
                return Err(if s.is_error(admitted)? { Outcome::loaded(Operand::object(admitted)) } else { bad() });
            }
            if s.call(q, store, b"swapImageStateWith:", &[staged])? != store {
                return Err(bad());
            }
            for &(index, target, selector, provenance, behavior) in &publication {
                let published = (|| {
                    if index.is_null() {
                        let removed = s.send(behavior, b"removeMethod:", &[Operand::object(selector)])?;
                        if let Some(value) = removed.as_object() {
                            if s.is_error(value)? { return Err(Outcome::loaded(removed)); }
                        }
                        Ok(target)
                    } else {
                        s.call(q, codegen, b"installPortableImageBinding:on:as:source:",
                            &[index, target, selector, provenance])
                    }
                })().and_then(|value| {
                    if value == target { Ok(value) }
                    else if s.is_error(value)? { Err(Outcome::loaded(Operand::object(value))) }
                    else { Err(bad()) }
                });
                if published? != target { return Err(bad()); }
            }
            for &(recipe, target) in &foreign {
                let restored = s.call(q, s.named(b"Alien")?, b"restoreWrapper:", &[recipe])?;
                if restored != target {
                    return Err(if s.is_error(restored)? {
                        Outcome::loaded(Operand::object(restored))
                    } else {
                        bad()
                    });
                }
            }
            for &o in &maps {
                s.call(q, s.named(b"Map")?, b"rehashImage:", &[o])?;
            }
            // Globals and private Methods now refer to the restored ordinary
            // source owner. Validate it; never install a second native registry.
            if decoded.sources.is_some() {
                let checked = s.call(q, codegen, b"finishPortableImageSources", &[])?;
                if checked != yes {
                    return Err(if s.is_error(checked)? { Outcome::loaded(Operand::object(checked)) } else { bad() });
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            if let Some(value) = error.value.as_object() {
                q.keep(value)?;
            }
            // Publication may already have committed, including before a failing
            // callback. Never restore old capabilities over the resulting world.
            // Staging cleanup still releases temporary roots; entered/captured
            // code remains owned by Execution. No user callback is replayed.
            *s.composition.borrow_mut() = None;
            *s.report.borrow_mut() = Report {
                status: if startup { b"startupFailed" } else { b"partialLoad" },
                detail: format!("image load failed (native status={} detail={}); applied changes retained; {}",
                    error.status, error.detail,
                    if startup { "discard the startup world" }
                    else { "world may be partially loaded; no rollback was attempted" }).into_bytes(),
                ..Report::default()
            };
            return Err(error);
        }
        *s.composition.borrow_mut() = Some(composition);
        Ok(Operand::object(loaded_result))
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
struct Entry {
    state: *const State,
    op: Op,
}
unsafe extern "C" fn operation(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let e = &*c.cast::<Entry>();
        let s = &*e.state;
        if s.thread != std::thread::current().id() {
            return Err(bad());
        }
        let args = borrowed_slice(a, n).ok_or_else(bad)?;
        let q = Scope {
            s,
            tokens: RefCell::new(Vec::new()),
        };
        q.keep(r)?;
        for &a in args {
            q.keep(object(a)?)?;
        }
        let expected = match e.op {
            Op::Save | Op::Load | Op::LoadMapped | Op::StartMapped => 1,
            Op::Rebuild | Op::LoadBytes | Op::Start | Op::StartBytes => 2,
            _ => 0,
        };
        if n != expected {
            return Err(bad());
        }
        match e.op {
            Op::Report => return Ok(Operand::object(s.receiver)),
            Op::Status => {
                let status = s.report.borrow().status;
                return Ok(Operand::object(s.id.intern_name(if status.is_empty() {
                    b"none"
                } else {
                    status
                })?));
            }
            Op::Detail => {
                let text = s.report.borrow().detail.clone();
                return s.text(&q, b"String", &text).map(Operand::object);
            }
            Op::Saved | Op::Zero | Op::Code => {
                let n = {
                    let report = s.report.borrow();
                    match e.op {
                        Op::Saved => report.saved,
                        Op::Code => report.code,
                        _ => 0,
                    }
                };
                return s.integer(&q, n).map(Operand::object);
            }
            Op::Identity => {
                let identity = s.composition.borrow().clone();
                return match identity {
                    Some(bytes) => s.text(&q, b"String", &bytes).map(Operand::object),
                    None => Ok(Operand::object(s.named(b"nil")?)),
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
        let result = (|| -> Result<Operand, Outcome> {
            let bytes = s.bytes(object(args[0])?)?;
            if matches!(e.op, Op::Save) {
                if s.behavior(object(args[0])?)? != s.named(b"StringVt")? {
                    return Err(bad());
                }
                return save(s, &q, Path::new(std::ffi::OsStr::from_bytes(&bytes)));
            }
            let bytes = if matches!(e.op, Op::LoadBytes | Op::StartBytes) {
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
            load(s, &q, bytes, rebuild, matches!(e.op, Op::LoadMapped | Op::StartMapped),
                matches!(e.op, Op::Start | Op::StartMapped | Op::StartBytes))
        })();
        if result.is_err() && !matches!(s.report.borrow().status, b"partialLoad" | b"startupFailed") {
            s.reject(b"Image admission, owner operation or publication failed");
        }
        match result {
            Err(error)
                if error.status == abi::INVALID_REQUEST && error.detail == IMAGE_REJECTED =>
            {
                Ok(Operand::object(s.named(b"nil")?))
            }
            result => result,
        }
    })())
}
unsafe extern "C" fn collect_fact(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = &*c.cast::<State>();
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
        Ok(Operand::unit())
    })())
}
unsafe extern "C" fn enumerate_roots(
    c: *mut c_void,
    _: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    done((|| unsafe {
        let s = &*c.cast::<State>();
        let Some([visitor]) = borrowed_slice(a, n) else {
            return Err(bad());
        };
        let visitor = object(*visitor)?;
        let mut values = s.permanent.clone();
        values.extend(s.facts.borrow().iter().flat_map(|f| f.values));
        for value in values {
            s.send(visitor, b"visit:", &[Operand::object(value)])?;
        }
        Ok(Operand::unit())
    })())
}
unsafe fn construct(id: Protocol, ns: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let named =
            |n: &[u8]| object(id.send_named(ns, b"at:", &[Operand::object(id.intern_name(n)?)])?);
        let existing = id.send_named(ns, b"at:", &[Operand::object(id.intern_name(b"Image")?)])?;
        if existing.as_object().is_some() {
            return Ok(existing);
        }
        let roots = named(b"Roots")?;
        let model = named(b"ObjectModel")?;
        let mut tokens = Vec::new();
        let mut keep = |o: Oop| -> Result<Oop, Outcome> {
            tokens.push(
                id.send_named(roots, b"retain:", &[Operand::object(o)])?
                    .as_word()
                    .ok_or_else(bad)?,
            );
            Ok(o)
        };
        let vt = keep(object(id.send_named(
            named(b"ObjectVt")?,
            b"delegated",
            &[],
        )?)?)?;
        let receiver = keep(object(id.send_named(
            model,
            b"allocate:bytes:",
            &[Operand::object(vt), Operand::word(0)],
        )?)?)?;
        let system = named(b"System")?;
        let system_vt =
            object(id.send_named(id.receiver, b"behaviorOf:", &[Operand::object(system)])?)?;
        let state = Box::into_raw(Box::new(State {
            id,
            ns,
            roots,
            model,
            receiver,
            permanent: vec![ns, roots, model, receiver, vt, system, system_vt],
            busy: Cell::new(false),
            facts: RefCell::new(Vec::new()),
            report: RefCell::new(Report::default()),
            composition: RefCell::new(None),
            thread: std::thread::current().id(),
        }));
        for (target, name, op) in [
            (system_vt, b"saveImage:".as_slice(), Op::Save),
            (system_vt, b"startImage:rebuild:", Op::Start),
            (system_vt, b"startImageMapped:", Op::StartMapped),
            (system_vt, b"startImageBytes:rebuild:", Op::StartBytes),
            (system_vt, b"loadImage:", Op::Load),
            (system_vt, b"loadImage:rebuild:", Op::Rebuild),
            (system_vt, b"loadImageMapped:", Op::LoadMapped),
            (system_vt, b"loadImageBytes:rebuild:", Op::LoadBytes),
            (system_vt, b"imageSaveReport", Op::Report),
            (system_vt, b"imageCompositionIdentity", Op::Identity),
            (vt, b"status", Op::Status),
            (vt, b"detail", Op::Detail),
            (vt, b"savedFacts", Op::Saved),
            (vt, b"recoveredFacts", Op::Zero),
            (vt, b"rejectedFacts", Op::Zero),
            (vt, b"recoveredSlots", Op::Zero),
            (vt, b"rejectedSlots", Op::Zero),
            (vt, b"codeUnits", Op::Code),
        ] {
            let e = Box::into_raw(Box::new(Entry { state, op }));
            id.send_named(
                target,
                b"addMethod",
                &[
                    Operand::object(id.intern_name(name)?),
                    Operand::entry(NativeEntry::new(operation, e.cast())),
                ],
            )?;
        }
        for (name, f) in [
            (b"rootsDo:".as_slice(), enumerate_roots as Method),
            (b"e:a:v:tx:op:historied:", collect_fact),
        ] {
            id.send_named(
                vt,
                b"addMethod",
                &[
                    Operand::object(id.intern_name(name)?),
                    Operand::entry(NativeEntry::new(f, state.cast())),
                ],
            )?;
        }
        id.send_named(roots, b"register:", &[Operand::object(receiver)])?;
        for (name, v) in [
            (b"Image".as_slice(), receiver),
            (b"ImageSaveReport", receiver),
            (b"ImageSaveReportVt", vt),
        ] {
            id.send_named(
                ns,
                b"at:put:",
                &[Operand::object(id.intern_name(name)?), Operand::object(v)],
            )?;
        }
        for token in tokens {
            id.send_named(roots, b"release:", &[Operand::word(token)])?;
        }
        Ok(Operand::object(receiver))
    }
}
unsafe extern "C" fn initialize(id: *const Protocol, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        if !abi::valid_protocol(id) {
            return Err(bad());
        }
        let Some([ns]) = borrowed_slice(a, n) else {
            return Err(bad());
        };
        construct(*id, object(*ns)?)
    })())
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: abi::Module = abi::Module {
    version: abi::VERSION,
    byte_size: size_of::<abi::Module>() as u32,
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
