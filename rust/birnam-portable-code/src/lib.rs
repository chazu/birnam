//! Reified PortableProgram objects. Every connection to another owner is an Id send.
use birnam_abi::{
    self as abi, Oop, borrowed_slice,
    id_protocol::{Literal, MethodSpec, Module, Protocol},
};
use birnam_artifact::portable_program::{self as portable, ValidatedProgram};
use core::{
    cell::{Cell, RefCell},
    ptr,
};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    rc::Rc,
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};
mod codec;
mod inventory;

#[derive(Clone, Copy)]
enum Failure {
    TypeMismatch,
    InvalidPortableProgram,
    PortableProgramLimit,
}
#[derive(Clone, Copy)]
enum Singleton {
    Nil,
    True,
    False,
}
enum NodeValue {
    Integer(i64),
    FloatBits([u8; 8]),
    String(Vec<u8>),
    Symbol(Vec<u8>),
    Singleton(Singleton),
    LexicalRead(u32),
    GlobalRead(Vec<u8>),
    Send {
        selector: Vec<u8>,
        operands: Vec<Oop>,
    },
    Block {
        program: Oop,
        captures: Vec<u32>,
    },
}
enum StatementValue {
    Expression(Oop),
    Let { name: Vec<u8>, expression: Oop },
}
#[derive(Clone)]
enum Value {
    Node(Rc<NodeValue>),
    Statement(Rc<StatementValue>),
    Program(Rc<ValidatedProgram>),
    Binding {
        program: Oop,
        index: u32,
    },
    Code {
        program: Oop,
        provenance: Oop,
        signature: Option<birnam_abi::native_signature::Signature>,
    },
}
impl Value {
    fn edges(&self) -> Vec<Oop> {
        match self {
            Self::Node(n) => match &**n {
                NodeValue::Send { operands, .. } => operands.clone(),
                NodeValue::Block { program, .. } => vec![*program],
                _ => vec![],
            },
            Self::Statement(s) => match &**s {
                StatementValue::Expression(e) | StatementValue::Let { expression: e, .. } => {
                    vec![*e]
                }
            },
            Self::Program(_) => vec![],
            Self::Binding { program, .. } => vec![*program],
            Self::Code {
                program,
                provenance,
                ..
            } => vec![*program, *provenance],
        }
    }
}
struct State {
    id: Protocol,
    names: HashMap<&'static [u8], Oop>,
    ns: Oop,
    roots: Oop,
    gc: Oop,
    nil: Oop,
    families: [(Oop, Oop); 5],
    // Native values belong to this module, never to mutable foreign payloads.
    // Snapshots release the map borrow before any reentrant send.
    values: RefCell<HashMap<usize, Value>>,
    thread: std::thread::ThreadId,
    ready: Cell<bool>,
}
struct Scope<'a> {
    state: &'a State,
    tokens: RefCell<Vec<usize>>,
}
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
fn optional(v: Oop) -> Option<Oop> {
    (!v.is_null()).then_some(v)
}
impl State {
    fn send(&self, receiver: Oop, name: &[u8], args: &[Oop]) -> Result<Oop, Oop> {
        unsafe { self.id.send(receiver, self.names[name], args) }
    }
    fn symbol(&self, name: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            if let Some(value) = self.names.get(name) {
                return Ok(*value);
            }
            let values: Vec<_> = name
                .iter()
                .map(|b| abi::size(*b as usize).unwrap())
                .collect();
            let value = self.send(self.id.root, b"nameWithBytes", &values)?;
            self.id.send(value, self.id.intern, &[])
        }
    }
    fn named(&self, name: &[u8]) -> Result<Oop, Oop> {
        self.send(self.ns, b"at:", &[self.names[name]])
    }
    fn behavior(&self, value: Oop) -> Result<Oop, Oop> {
        self.send(self.id.root, b"behaviorOf:", &[value])
    }
    fn node(&self, o: Oop) -> Option<Rc<NodeValue>> {
        match self.values.borrow().get(&o.addr())? {
            Value::Node(v) => Some(v.clone()),
            _ => None,
        }
    }
    fn statement(&self, o: Oop) -> Option<Rc<StatementValue>> {
        match self.values.borrow().get(&o.addr())? {
            Value::Statement(v) => Some(v.clone()),
            _ => None,
        }
    }
    fn program(&self, o: Oop) -> Option<Rc<ValidatedProgram>> {
        match self.values.borrow().get(&o.addr())? {
            Value::Program(v) => Some(v.clone()),
            _ => None,
        }
    }
    fn code(&self, o: Oop) -> Option<(Oop, Oop, Option<birnam_abi::native_signature::Signature>)> {
        match self.values.borrow().get(&o.addr())? {
            Value::Code {
                program,
                provenance,
                signature,
            } => Some((*program, *provenance, signature.clone())),
            _ => None,
        }
    }
    fn scoped(&self, f: impl FnOnce(&Scope<'_>) -> Result<Oop, Oop>) -> Result<Oop, Oop> {
        let scope = Scope {
            state: self,
            tokens: RefCell::new(Vec::new()),
        };
        let result = f(&scope);
        let protected = match result {
            Ok(v) | Err(v) => v,
        };
        let mut args = vec![protected];
        for token in scope.tokens.into_inner() {
            args.push(abi::size(token).ok_or_else(invalid)?);
        }
        let cleanup = self.send(self.roots, b"releaseTokensProtecting", &args);
        match (result, cleanup) {
            (Ok(_), Err(e)) => Err(e),
            (result, _) => result,
        }
    }
}
impl Scope<'_> {
    fn keep(&self, value: Oop) -> Result<Oop, Oop> {
        let token = abi::as_size(self.state.send(self.state.roots, b"retain:", &[value])?)
            .filter(|t| *t != 0)
            .ok_or_else(invalid)?;
        self.tokens.borrow_mut().push(token);
        Ok(value)
    }
    fn call(&self, r: Oop, name: &[u8], args: &[Oop]) -> Result<Oop, Oop> {
        self.keep(self.state.send(r, name, args)?)
    }
    fn word(&self, bits: u64) -> Result<Oop, Oop> {
        if let Some(v) = abi::size(bits as usize) {
            Ok(v)
        } else {
            self.call(
                self.state.named(b"Word")?,
                b"fromUnsignedLow:high:",
                &[
                    abi::size(bits as u32 as usize).unwrap(),
                    abi::size((bits >> 32) as usize).unwrap(),
                ],
            )
        }
    }
    fn word_bits(&self, value: Oop) -> Result<u64, Oop> {
        if let Some(v) = abi::as_size(value) {
            return Ok(v as u64);
        }
        let lo = abi::as_size(self.call(value, b"low32", &[])?)
            .filter(|n| *n <= u32::MAX as usize)
            .ok_or_else(invalid)?;
        let hi = abi::as_size(self.call(value, b"high32", &[])?)
            .filter(|n| *n <= u32::MAX as usize)
            .ok_or_else(invalid)?;
        Ok(lo as u64 | ((hi as u64) << 32))
    }
    fn number(&self, n: i64) -> Result<Oop, Oop> {
        let word = self.word(n as u64)?;
        self.call(self.state.named(b"Integer")?, b"fromWord:", &[word])
    }
    fn integer(&self, value: Oop) -> Result<Option<i64>, Oop> {
        let word = self.call(self.state.named(b"Integer")?, b"wordOf:", &[value])?;
        if word.is_null() || word == self.state.nil {
            Ok(None)
        } else {
            Ok(Some(self.word_bits(word)? as i64))
        }
    }
    fn blob(&self, bytes: &[u8], factory: &[u8]) -> Result<Oop, Oop> {
        let factory = self.keep(self.state.named(factory)?)?;
        let values: Vec<_> = bytes
            .iter()
            .map(|b| abi::size(*b as usize).unwrap())
            .collect();
        let sequence = self.call(self.state.id.root, b"sequence", &values)?;
        self.call(factory, b"fromBytes:", &[sequence])
    }
    fn string(&self, bytes: &[u8]) -> Result<Oop, Oop> {
        self.blob(bytes, b"String")
    }
    fn byte_values(&self, bytes: &[u8]) -> Result<Oop, Oop> {
        let values: Vec<_> = bytes
            .iter()
            .map(|b| abi::size(*b as usize).unwrap())
            .collect();
        self.call(self.state.id.root, b"sequence", &values)
    }
    fn read_bytes(&self, value: Oop) -> Result<Vec<u8>, Oop> {
        let value = self.call(value, b"bytes", &[])?;
        unsafe {
            self.state.id.read_bytes(
                value,
                self.state.names[b"sizeWord".as_slice()],
                self.state.names[b"atWord:".as_slice()],
            )
        }
    }
    fn bytes(&self, value: Oop, symbol: bool) -> Result<Option<Vec<u8>>, Oop> {
        let s = self.state;
        if s.behavior(value)? != s.named(if symbol { b"SymbolVt" } else { b"StringVt" })? {
            return Ok(None);
        }
        if symbol {
            unsafe {
                s.id.read_bytes(
                    value,
                    s.names[b"sizeWord".as_slice()],
                    s.names[b"atWord:".as_slice()],
                )
                .map(Some)
            }
        } else {
            self.read_bytes(value).map(Some)
        }
    }
    fn fail(&self, failure: Failure, message: &[u8]) -> Result<Oop, Oop> {
        let s = self.state;
        let factory = self.keep(object(s.named(b"Error")?)?)?;
        let kind = self.keep(s.symbol(match failure {
            Failure::TypeMismatch => b"typeMismatch",
            Failure::InvalidPortableProgram => b"invalidPortableProgram",
            Failure::PortableProgramLimit => b"portableProgramLimit",
        })?)?;
        let message = self.string(message)?;
        let error = self.call(factory, b"kind:message:", &[kind, message])?;
        if error == s.nil {
            return Err(invalid());
        }
        Ok(error)
    }
    fn make(&self, behavior: Oop, value: Value) -> Result<Oop, Oop> {
        let s = self.state;
        self.keep(behavior)?;
        for edge in value.edges() {
            self.keep(edge)?;
        }
        let extent = size_of::<usize>();
        let o = self.keep(object(s.send(
            behavior,
            b"allocate",
            &[abi::size(extent).ok_or_else(invalid)?],
        )?)?)?;
        if s.behavior(o)? != behavior
            || abi::as_size(s.send(s.id.root, b"allocationSizeOf:", &[o])?) != Some(extent)
            || s.values.borrow().contains_key(&o.addr())
        {
            return Err(invalid());
        }
        s.values.borrow_mut().insert(o.addr(), value);
        s.send(s.gc, b"finalize:with:", &[o, s.families[0].1])?;
        Ok(o)
    }
    fn node(&self, receiver: Oop, value: NodeValue) -> Result<Oop, Oop> {
        self.make(self.state.behavior(receiver)?, Value::Node(Rc::new(value)))
    }
    fn statement(&self, receiver: Oop, value: StatementValue) -> Result<Oop, Oop> {
        self.make(
            self.state.behavior(receiver)?,
            Value::Statement(Rc::new(value)),
        )
    }
    fn binding(&self, program: Oop, index: u32) -> Result<Oop, Oop> {
        let Some(vt) = optional(self.state.named(b"CodeLexicalBindingVt")?) else {
            return Ok(self.state.nil);
        };
        self.make(vt, Value::Binding { program, index })
    }
    fn code(&self, receiver: Oop, program: Oop, provenance: Oop) -> Result<Oop, Oop> {
        self.signed_code(receiver, program, provenance, None)
    }
    fn signed_code(
        &self,
        receiver: Oop,
        program: Oop,
        provenance: Oop,
        signature: Option<birnam_abi::native_signature::Signature>,
    ) -> Result<Oop, Oop> {
        if self.state.program(program).is_none() {
            return self.fail(
                Failure::TypeMismatch,
                b"CodeObject program must be a CodeProgram",
            );
        }
        if signature.as_ref().is_some_and(|sig| {
            self.state
                .program(program)
                .is_none_or(|p| p.view().parameter_count as usize != sig.arguments().len() + 1)
        }) {
            return self.fail(
                Failure::TypeMismatch,
                b"native signature arity disagrees with CodeObject",
            );
        }
        self.make(
            self.state.behavior(receiver)?,
            Value::Code {
                program,
                provenance,
                signature,
            },
        )
    }
    fn collection(&self, collection: Oop, limit: usize) -> Result<Result<Vec<Oop>, Failure>, Oop> {
        let count = self.call(collection, b"size", &[])?;
        let Some(count) = self.integer(count)?.and_then(|n| usize::try_from(n).ok()) else {
            return Ok(Err(Failure::TypeMismatch));
        };
        if count > limit {
            return Ok(Err(Failure::PortableProgramLimit));
        }
        let mut values = Vec::with_capacity(count);
        for index in 0..count {
            values.push(self.call(collection, b"at:", &[self.number(index as i64)?])?);
        }
        Ok(Ok(values))
    }
    fn names(&self, values: &[Oop]) -> Result<Result<Vec<Vec<u8>>, Failure>, Oop> {
        let mut names = Vec::with_capacity(values.len());
        for &value in values {
            let Some(bytes) = self.bytes(value, false)? else {
                return Ok(Err(Failure::TypeMismatch));
            };
            if bytes.is_empty() || bytes.contains(&0) {
                return Ok(Err(Failure::InvalidPortableProgram));
            }
            if bytes.len() > 1 << 20 {
                return Ok(Err(Failure::PortableProgramLimit));
            }
            names.push(bytes);
        }
        Ok(Ok(names))
    }
    fn index(&self, args: &[Oop]) -> Result<Option<usize>, Oop> {
        if let [index] = args {
            Ok(self.integer(*index)?.and_then(|n| usize::try_from(n).ok()))
        } else {
            Ok(None)
        }
    }
}

fn node_method(q: &Scope<'_>, r: Oop, a: &[Oop], op: &[u8]) -> Result<Oop, Oop> {
    use Failure::*;
    use NodeValue::*;
    let s = q.state;
    match op {
        b"integer:" | b"lexicalReadAt:" => {
            let lexical = op == b"lexicalReadAt:";
            let [value] = a else {
                return q.fail(
                    TypeMismatch,
                    if lexical {
                        b"lexicalReadAt: needs one Integer"
                    } else {
                        b"integer: needs one Integer"
                    },
                );
            };
            let Some(value) = q.integer(*value)? else {
                return q.fail(
                    TypeMismatch,
                    if lexical {
                        b"lexicalReadAt: needs an Integer"
                    } else {
                        b"integer: needs an Integer"
                    },
                );
            };
            let value = if lexical {
                let Ok(index) = u32::try_from(value) else {
                    return q.fail(
                        InvalidPortableProgram,
                        b"lexical binding index is outside u32",
                    );
                };
                LexicalRead(index)
            } else {
                Integer(value)
            };
            return q.node(r, value);
        }
        b"string:" | b"floatBits:" | b"symbol:" | b"globalRead:" => {
            let [value] = a else {
                return q.fail(
                    TypeMismatch,
                    match op {
                        b"string:" => b"string: needs one String",
                        b"floatBits:" => b"floatBits: needs one eight-byte String",
                        b"symbol:" => b"symbol: needs one Symbol",
                        _ => b"globalRead: needs one Symbol",
                    },
                );
            };
            let symbol = matches!(op, b"symbol:" | b"globalRead:");
            let Some(bytes) = q.bytes(*value, symbol)? else {
                return q.fail(
                    TypeMismatch,
                    match op {
                        b"string:" => b"string: needs a String",
                        b"floatBits:" => b"floatBits: needs a String",
                        b"symbol:" => b"symbol: needs a Symbol",
                        _ => b"globalRead: needs a Symbol",
                    },
                );
            };
            let value = match op {
                b"floatBits:" => {
                    let Ok(bits) = <[u8; 8]>::try_from(bytes.as_slice()) else {
                        return q.fail(
                            InvalidPortableProgram,
                            b"floatBits: needs exactly eight bytes",
                        );
                    };
                    FloatBits(bits)
                }
                b"string:" => {
                    if bytes.len() > 16 << 20 {
                        return q.fail(
                            PortableProgramLimit,
                            b"String literal exceeds the PortableProgram limit",
                        );
                    }
                    String(bytes)
                }
                _ => {
                    let global = op == b"globalRead:";
                    if bytes.is_empty() {
                        return q.fail(
                            InvalidPortableProgram,
                            if global {
                                b"global name must not be empty"
                            } else {
                                b"Symbol literal name must not be empty"
                            },
                        );
                    }
                    if bytes.len() > 1 << 20 {
                        return q.fail(
                            PortableProgramLimit,
                            if global {
                                b"global name exceeds the PortableProgram limit"
                            } else {
                                b"Symbol literal exceeds the PortableProgram limit"
                            },
                        );
                    }
                    if global {
                        GlobalRead(bytes)
                    } else {
                        Symbol(bytes)
                    }
                }
            };
            return q.node(r, value);
        }
        b"nilLiteral" | b"trueLiteral" | b"falseLiteral" => {
            if !a.is_empty() {
                return q.fail(TypeMismatch, b"literal constructor takes no arguments");
            }
            return q.node(
                r,
                Singleton(match op {
                    b"nilLiteral" => self::Singleton::Nil,
                    b"trueLiteral" => self::Singleton::True,
                    _ => self::Singleton::False,
                }),
            );
        }
        b"send:operands:" => {
            let [selector, collection] = a else {
                return q.fail(
                    TypeMismatch,
                    b"send:operands: needs a Symbol and collection",
                );
            };
            let Some(selector) = q.bytes(*selector, true)? else {
                return q.fail(TypeMismatch, b"send selector must be a Symbol");
            };
            let Ok(arity) = portable::selector_arity(&selector) else {
                return q.fail(
                    InvalidPortableProgram,
                    b"send selector has invalid arity spelling",
                );
            };
            let operands = match q.collection(*collection, 4096)? {
                Ok(v) => v,
                Err(TypeMismatch) => {
                    return q.fail(TypeMismatch, b"send operands must be a finite collection");
                }
                Err(_) => {
                    return q.fail(
                        PortableProgramLimit,
                        b"send operand count exceeds the PortableProgram limit",
                    );
                }
            };
            if operands.len() != arity as usize + 1 {
                return q.fail(
                    InvalidPortableProgram,
                    b"send operand count does not match selector arity",
                );
            }
            if operands.iter().any(|o| s.node(*o).is_none()) {
                return q.fail(TypeMismatch, b"send operands must contain only CodeNodes");
            }
            return q.node(r, Send { selector, operands });
        }
        b"block:captureBindings:" => {
            let [program, collection] = a else {
                return q.fail(
                    TypeMismatch,
                    b"block:captureBindings: needs a CodeProgram and collection",
                );
            };
            let Some(bytes) = s.program(*program) else {
                return q.fail(TypeMismatch, b"block program must be a CodeProgram");
            };
            let values = match q.collection(*collection, 4096)? {
                Ok(v) => v,
                Err(TypeMismatch) => {
                    return q.fail(
                        TypeMismatch,
                        b"capture bindings must be a finite collection",
                    );
                }
                Err(_) => {
                    return q.fail(
                        PortableProgramLimit,
                        b"capture binding count exceeds the PortableProgram limit",
                    );
                }
            };
            if values.len() != bytes.view().capture_count as usize {
                return q.fail(
                    InvalidPortableProgram,
                    b"capture binding count does not match nested program",
                );
            }
            let mut captures = Vec::with_capacity(values.len());
            for value in values {
                let Some(index) = q.integer(value)? else {
                    return q.fail(TypeMismatch, b"capture binding indices must be Integers");
                };
                let Ok(index) = u32::try_from(index) else {
                    return q.fail(
                        InvalidPortableProgram,
                        b"capture binding index is outside u32",
                    );
                };
                captures.push(index);
            }
            return q.node(
                r,
                Block {
                    program: *program,
                    captures,
                },
            );
        }
        _ => {}
    }
    let Some(value) = s.node(r) else {
        return Ok(s.nil);
    };
    if matches!(op, b"operandAt:" | b"captureBindingAt:") {
        let Some(index) = q.index(a)? else {
            return Ok(s.nil);
        };
        return match (&*value, op) {
            (Send { operands, .. }, b"operandAt:") => {
                Ok(operands.get(index).copied().unwrap_or(s.nil))
            }
            (Block { captures, .. }, b"captureBindingAt:") => match captures.get(index) {
                Some(n) => q.number(i64::from(*n)),
                None => Ok(s.nil),
            },
            _ => Ok(s.nil),
        };
    }
    if !a.is_empty() {
        return Ok(s.nil);
    }
    match (op, &*value) {
        (b"kind", v) => s.symbol(match v {
            Integer(_) => b"integer",
            FloatBits(_) => b"float",
            String(_) => b"string",
            Symbol(_) => b"symbol",
            Singleton(self::Singleton::Nil) => b"nil",
            Singleton(self::Singleton::True) => b"true",
            Singleton(self::Singleton::False) => b"false",
            LexicalRead(_) => b"lexicalRead",
            GlobalRead(_) => b"globalRead",
            Send { .. } => b"send",
            Block { .. } => b"block",
        }),
        (b"integer", Integer(n)) => q.number(*n),
        (b"floatBits", FloatBits(b)) => q.string(b),
        (b"string", String(b)) => q.string(b),
        (b"symbol", Symbol(b))
        | (b"global", GlobalRead(b))
        | (b"selector", Send { selector: b, .. }) => s.symbol(b),
        (b"bindingIndex", LexicalRead(n)) => q.number(i64::from(*n)),
        (b"program", Block { program, .. }) => Ok(*program),
        (b"operandCount", v) => q.number(if let Send { operands, .. } = v {
            operands.len() as i64
        } else {
            0
        }),
        (b"captureCount", v) => q.number(if let Block { captures, .. } = v {
            captures.len() as i64
        } else {
            0
        }),
        _ => Ok(s.nil),
    }
}
fn statement_method(q: &Scope<'_>, r: Oop, a: &[Oop], op: &[u8]) -> Result<Oop, Oop> {
    use Failure::*;
    let s = q.state;
    match op {
        b"expression:" => {
            let [e] = a else {
                return Ok(s.nil);
            };
            if s.node(*e).is_none() {
                return q.fail(TypeMismatch, b"expression: needs a CodeNode");
            }
            return q.statement(r, StatementValue::Expression(*e));
        }
        b"letNamed:expression:" => {
            let [name, expression] = a else {
                return q.fail(
                    TypeMismatch,
                    b"letNamed:expression: needs a String and CodeNode",
                );
            };
            let Some(name) = q.bytes(*name, false)? else {
                return q.fail(TypeMismatch, b"Let binding name must be a String");
            };
            if name.is_empty() || name.contains(&0) {
                return q.fail(
                    InvalidPortableProgram,
                    b"Let binding name must be nonempty and NUL-free",
                );
            }
            if name.len() > 1 << 20 {
                return q.fail(
                    PortableProgramLimit,
                    b"Let binding name exceeds the PortableProgram limit",
                );
            }
            if s.node(*expression).is_none() {
                return q.fail(TypeMismatch, b"Let expression must be a CodeNode");
            }
            return q.statement(
                r,
                StatementValue::Let {
                    name,
                    expression: *expression,
                },
            );
        }
        _ => {}
    }
    if !a.is_empty() {
        return Ok(s.nil);
    }
    let Some(value) = s.statement(r) else {
        return Ok(s.nil);
    };
    match (op, &*value) {
        (b"kind", StatementValue::Expression(_)) => s.symbol(b"expression"),
        (b"kind", StatementValue::Let { .. }) => s.symbol(b"let"),
        (
            b"expression",
            StatementValue::Expression(e) | StatementValue::Let { expression: e, .. },
        ) => Ok(*e),
        (b"bindingName", StatementValue::Let { name, .. }) => q.string(name),
        _ => Ok(s.nil),
    }
}
fn decode_node_tree(q: &Scope<'_>, program: &portable::Program<'_>, root: u32) -> Result<Oop, Oop> {
    let s = q.state;
    let root = root as usize;
    let start = codec::decoded_subtree_start(program, root).ok_or_else(invalid)?;
    let Some(vt) = optional(s.named(b"CodeNodeVt")?) else {
        return Ok(s.nil);
    };
    q.keep(vt)?;
    let mut stack = Vec::new();
    for index in start..=root {
        let node = program.node(index).ok_or_else(invalid)?;
        let value = match node.kind {
            1 => NodeValue::Integer(node.payload as i64),
            2 => NodeValue::FloatBits(node.payload.to_le_bytes()),
            3 => NodeValue::String(node.data.to_vec()),
            4 => NodeValue::Symbol(node.data.to_vec()),
            5 => NodeValue::Singleton(Singleton::Nil),
            6 => NodeValue::Singleton(Singleton::True),
            7 => NodeValue::Singleton(Singleton::False),
            8 => NodeValue::LexicalRead(node.auxiliary_0),
            9 => NodeValue::GlobalRead(node.data.to_vec()),
            10 => {
                let count = node.operand_count as usize;
                if stack.len() < count {
                    return Err(invalid());
                }
                let operands = stack.split_off(stack.len() - count);
                NodeValue::Send {
                    selector: node.data.to_vec(),
                    operands,
                }
            }
            11 => {
                let Some(program_vt) = optional(s.named(b"CodeProgramVt")?) else {
                    return Ok(s.nil);
                };
                let bytes = ValidatedProgram::copy_from_slice(node.data).map_err(|_| invalid())?;
                let nested = q.make(program_vt, Value::Program(Rc::new(bytes)))?;
                let captures = (0..node.operand_count as usize)
                    .map(|operand| program.node_operand(index, operand))
                    .collect::<Option<Vec<_>>>()
                    .ok_or_else(invalid)?;
                NodeValue::Block {
                    program: nested,
                    captures,
                }
            }
            _ => return Err(invalid()),
        };
        stack.push(q.make(vt, Value::Node(Rc::new(value)))?);
    }
    if stack.len() != 1 {
        return Err(invalid());
    }
    Ok(stack[0])
}
fn program_method(q: &Scope<'_>, r: Oop, a: &[Oop], op: &[u8]) -> Result<Oop, Oop> {
    use Failure::*;
    let s = q.state;
    if op == b"parameters:captures:statements:" {
        let [parameters, captures, statements] = a else {
            return q.fail(
                TypeMismatch,
                b"CodeProgram construction needs parameter, capture, and statement collections",
            );
        };
        let mut collections = Vec::new();
        for (value, limit, mismatch, overflow) in [
            (
                *parameters,
                4096,
                b"parameters must be a finite collection".as_slice(),
                b"parameter count exceeds the PortableProgram limit".as_slice(),
            ),
            (
                *captures,
                4096,
                b"captures must be a finite collection",
                b"capture count exceeds the PortableProgram limit",
            ),
            (
                *statements,
                262_144,
                b"statements must be a finite collection",
                b"statement count exceeds the PortableProgram limit",
            ),
        ] {
            match q.collection(value, limit)? {
                Ok(v) => collections.push(v),
                Err(TypeMismatch) => return q.fail(TypeMismatch, mismatch),
                Err(_) => return q.fail(PortableProgramLimit, overflow),
            }
        }
        let parameters = match q.names(&collections[0])? {
            Ok(v) => v,
            Err(f) => return q.fail(f, b"CodeProgram semantic validation failed"),
        };
        let captures = match q.names(&collections[1])? {
            Ok(v) => v,
            Err(f) => return q.fail(f, b"CodeProgram semantic validation failed"),
        };
        let bytes = match codec::encode_program(s, &parameters, &captures, &collections[2]) {
            Ok(v) => v,
            Err(f) => return q.fail(f, b"CodeProgram semantic validation failed"),
        };
        let bytes = ValidatedProgram::new(bytes).map_err(|_| invalid())?;
        return q.make(s.behavior(r)?, Value::Program(Rc::new(bytes)));
    }
    let Some(bytes) = s.program(r) else {
        return Ok(s.nil);
    };
    let program = bytes.view();
    let lets = program.binding_count - program.parameter_count - program.capture_count;
    if matches!(
        op,
        b"statementAt:" | b"bindingAt:" | b"parameterAt:" | b"captureAt:" | b"letAt:"
    ) {
        let Some(index) = q.index(a)? else {
            return Ok(s.nil);
        };
        if op == b"statementAt:" {
            let Some(statement) = program.statement(index) else {
                return Ok(s.nil);
            };
            let expression = decode_node_tree(q, &program, statement.root_node)?;
            if expression == s.nil {
                return Ok(s.nil);
            }
            let value = match statement.kind {
                portable::StatementKind::Expression => StatementValue::Expression(expression),
                portable::StatementKind::Let => StatementValue::Let {
                    name: program
                        .binding(statement.binding_index.ok_or_else(invalid)? as usize)
                        .ok_or_else(invalid)?
                        .name
                        .to_vec(),
                    expression,
                },
            };
            let vt = object(s.named(b"CodeStatementVt")?)?;
            return q.make(vt, Value::Statement(Rc::new(value)));
        }
        let global = match op {
            b"bindingAt:" if index < program.binding_count as usize => index,
            b"parameterAt:" if index < program.parameter_count as usize => index,
            b"captureAt:" if index < program.capture_count as usize => {
                program.parameter_count as usize + index
            }
            b"letAt:" if index < lets as usize => {
                (program.parameter_count + program.capture_count) as usize + index
            }
            _ => return Ok(s.nil),
        };
        return q.binding(r, global as u32);
    }
    if !a.is_empty() {
        return Ok(s.nil);
    }
    if op == b"format" {
        return s.symbol(b"portableProgramV1");
    }
    q.number(i64::from(match op {
        b"arity" | b"parameterCount" => program.parameter_count,
        b"captureCount" => program.capture_count,
        b"letCount" => lets,
        b"bindingCount" => program.binding_count,
        b"statementCount" => program.statement_count,
        _ => return Ok(s.nil),
    }))
}
fn binding_method(q: &Scope<'_>, r: Oop, a: &[Oop], op: &[u8]) -> Result<Oop, Oop> {
    let s = q.state;
    if !a.is_empty() {
        return Ok(s.nil);
    }
    let value = s.values.borrow().get(&r.addr()).cloned();
    let Some(Value::Binding { program, index }) = value else {
        return Ok(s.nil);
    };
    if op == b"index" {
        return q.number(i64::from(index));
    }
    let Some(bytes) = s.program(program) else {
        return Ok(s.nil);
    };
    let Some(binding) = bytes.view().binding(index as usize) else {
        return Ok(s.nil);
    };
    match op {
        b"kind" => s.symbol(match binding.kind {
            portable::BindingKind::Parameter => b"parameter",
            portable::BindingKind::Capture => b"capture",
            portable::BindingKind::Let => b"let",
        }),
        b"ordinal" => q.number(i64::from(binding.ordinal)),
        b"name" => q.string(binding.name),
        b"definingStatementIndex" => match binding.defining_statement {
            Some(n) => q.number(i64::from(n)),
            None => Ok(s.nil),
        },
        _ => Ok(s.nil),
    }
}
fn code_method(q: &Scope<'_>, r: Oop, a: &[Oop], op: &[u8]) -> Result<Oop, Oop> {
    let s = q.state;
    match op {
        b"inspectInventory:bindings:selection:"
        | b"inventory:matchesRoots:"
        | b"bindings:referencesBelow:" => return inventory::operation(q, a, op),
        b"canonicalBytesOf:" => {
            let [value] = a else {
                return Ok(s.nil);
            };
            let Some((program, _, _)) = s.code(*value) else {
                return Ok(s.nil);
            };
            let bytes = s.program(program).ok_or_else(invalid)?;
            let factory = q.keep(object(s.named(b"Bytes")?)?)?;
            return q.keep(object(s.send(
                factory,
                b"fromBytes:",
                &[q.byte_values(bytes.as_ref())?],
            )?)?);
        }
        b"fromCanonicalBytes:" => {
            let [value] = a else {
                return q.fail(
                    Failure::TypeMismatch,
                    b"fromCanonicalBytes: needs one Bytes",
                );
            };
            if s.behavior(*value)? != object(s.named(b"BytesVt")?)? {
                return q.fail(Failure::TypeMismatch, b"fromCanonicalBytes: needs a Bytes");
            }
            let bytes = q.read_bytes(*value)?;
            let bytes = match ValidatedProgram::new(bytes) {
                Ok(v) => v,
                Err(e) => {
                    return q.fail(
                        match e {
                            portable::Error::Quantity => Failure::PortableProgramLimit,
                            _ => Failure::InvalidPortableProgram,
                        },
                        b"CodeObject canonical program validation failed",
                    );
                }
            };
            let program = q.make(
                object(s.named(b"CodeProgramVt")?)?,
                Value::Program(Rc::new(bytes)),
            )?;
            return q.code(r, program, s.nil);
        }
        b"fromProgram:" => {
            let [p] = a else {
                return q.fail(Failure::TypeMismatch, b"fromProgram: needs one CodeProgram");
            };
            return q.code(r, *p, s.nil);
        }
        b"fromProgram:provenance:" => {
            let [p, provenance] = a else {
                return q.fail(
                    Failure::TypeMismatch,
                    b"fromProgram:provenance: needs two arguments",
                );
            };
            return q.code(r, *p, *provenance);
        }
        b"nativeSignature:matchesProgram:" => {
            let [signature, program] = a else {
                return object(s.named(b"false")?);
            };
            let copy = |value| -> Result<Vec<u8>, Oop> { q.read_bytes(value) };
            let signature = copy(*signature)?;
            let program = copy(*program)?;
            let valid = birnam_abi::native_signature::Signature::parse(&signature)
                .zip(ValidatedProgram::new(program).ok())
                .is_some_and(|(signature, program)| {
                    signature.arguments().len() + 1 == program.view().parameter_count as usize
                });
            return object(s.named(if valid { b"true" } else { b"false" })?);
        }
        b"isNativeSignature:" => {
            let [input] = a else {
                return object(s.named(b"false")?);
            };
            let copied = q.read_bytes(*input)?;
            return Ok(
                if birnam_abi::native_signature::Signature::parse(&copied).is_some() {
                    object(s.named(b"true")?)?
                } else {
                    object(s.named(b"false")?)?
                },
            );
        }
        b"withNativeSignature:" => {
            let [input] = a else {
                return Ok(s.nil);
            };
            let Some((program, provenance, _)) = s.code(r) else {
                return Ok(s.nil);
            };
            let signature = if *input == s.nil {
                None
            } else {
                let copied = q.read_bytes(*input)?;
                let Some(signature) = birnam_abi::native_signature::Signature::parse(&copied)
                else {
                    return q.fail(Failure::TypeMismatch, b"invalid native signature");
                };
                Some(signature)
            };
            return q.signed_code(r, program, provenance, signature);
        }
        b"withProgram:" => {
            let [p] = a else {
                return q.fail(Failure::TypeMismatch, b"withProgram: needs one CodeProgram");
            };
            let Some((_, provenance, signature)) = s.code(r) else {
                return Ok(s.nil);
            };
            return q.signed_code(r, *p, provenance, signature);
        }
        _ => {}
    }
    if !a.is_empty() {
        return Ok(s.nil);
    }
    let Some((program, provenance, signature)) = s.code(r) else {
        return Ok(s.nil);
    };
    match op {
        b"format" => return s.symbol(b"portableProgramV1"),
        b"program" => return Ok(program),
        b"sourceProvenance" => return Ok(provenance),
        b"nativeSignature" => {
            return match signature {
                None => Ok(s.nil),
                Some(signature) => {
                    let factory = q.keep(object(s.named(b"Bytes")?)?)?;
                    q.keep(object(s.send(
                        factory,
                        b"fromBytes:",
                        &[q.byte_values(signature.bytes())?],
                    )?)?)
                }
            };
        }
        b"withoutSourceProvenance" => return q.signed_code(r, program, s.nil, signature),
        _ => {}
    }
    let bytes = s.program(program).ok_or_else(invalid)?;
    let inspected = bytes.view();
    match op {
        b"arity" => q.number(i64::from(inspected.parameter_count)),
        b"digest" => q.string(&Sha256::digest(bytes.as_ref())),
        b"captureSchema" => {
            let Some(factory) = optional(s.named(b"Vector")?) else {
                return Ok(s.nil);
            };
            q.keep(factory)?;
            let mut vector = q.call(factory, b"new", &[])?;
            if vector == s.nil {
                return Ok(s.nil);
            }
            for ordinal in 0..inspected.capture_count {
                let binding = q.binding(program, inspected.parameter_count + ordinal)?;
                if binding == s.nil {
                    return Ok(s.nil);
                }
                vector = q.call(vector, b"add:", &[binding])?;
                if vector == s.nil {
                    return Ok(s.nil);
                }
            }
            Ok(vector)
        }
        _ => Ok(s.nil),
    }
}

unsafe fn state<'a>(context: Oop) -> Result<&'a State, Oop> {
    unsafe {
        let p = INSTANCE.load(Ordering::Acquire);
        if p.is_null() {
            return Err(invalid());
        }
        let s = &*p;
        if s.ns != context || s.thread != std::thread::current().id() {
            Err(invalid())
        } else {
            Ok(s)
        }
    }
}
unsafe extern "C" fn roots(_: Oop, context: Oop, _: Oop, args: *const Oop, count: usize) -> Oop {
    (|| unsafe {
        let s = state(context)?;
        let Some([visitor]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let visitor = object(*visitor)?;
        let mut roots = vec![s.ns, s.roots, s.gc, s.nil];
        for (vt, prototype) in s.families {
            roots.extend([vt, prototype]);
        }
        for root in roots {
            s.send(visitor, b"visit:", &[root])?;
        }
        Ok(ptr::null_mut())
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn references(
    _: Oop,
    context: Oop,
    _: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    (|| unsafe {
        let s = state(context)?;
        let Some([value, visitor]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let value = object(*value)?;
        let visitor = object(*visitor)?;
        let edges = s
            .values
            .borrow()
            .get(&value.addr())
            .map(Value::edges)
            .unwrap_or_default();
        for edge in edges {
            s.send(visitor, b"visit:", &[edge])?;
        }
        Ok(ptr::null_mut())
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn finalize(_: Oop, context: Oop, _: Oop, args: *const Oop, count: usize) -> Oop {
    (|| unsafe {
        let s = state(context)?;
        let Some([value]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        // GC detached this identity; do not send to or inspect its allocation.
        s.values
            .borrow_mut()
            .remove(&object(*value)?.addr())
            .ok_or_else(invalid)?;
        Ok(ptr::null_mut())
    })()
    .unwrap_or_else(abi::fault_reference)
}
// Native shape queries read admitted immutable state without exporting bytes,
// allocating language Objects, or crossing a dependency callback. Callers keep
// the CodeObject live under the ordinary Id ABI lifetime contract.
unsafe fn code_count(context: Oop, args: *const Oop, count: usize, parameters: bool) -> Oop {
    (|| unsafe {
        let s = state(context)?;
        let [code] = borrowed_slice(args, count).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let (program, _, _) = s.code(object(*code)?).ok_or_else(invalid)?;
        let program = s.program(program).ok_or_else(invalid)?;
        let view = program.view();
        Ok(abi::size(if parameters {
            view.parameter_count
        } else {
            view.capture_count
        } as usize)
        .ok_or_else(invalid)?)
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn parameter_count_of(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { code_count(c, a, n, true) }
}
unsafe extern "C" fn capture_count_of(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { code_count(c, a, n, false) }
}

macro_rules! entry {
    ($name:ident, $family:ident, $selector:literal) => {
        unsafe extern "C" fn $name(
            _: Oop,
            context: Oop,
            r: Oop,
            args: *const Oop,
            count: usize,
        ) -> Oop {
            (|| unsafe {
                let s = state(context)?;
                let args = borrowed_slice(args, count).ok_or_else(invalid)?;
                let args = args
                    .iter()
                    .copied()
                    .map(object)
                    .collect::<Result<Vec<_>, _>>()?;
                s.scoped(|q| {
                    q.keep(r)?;
                    for &a in &args {
                        q.keep(a)?;
                    }
                    let result = if $selector.as_slice() == b"clone" {
                        if args.is_empty() { r } else { s.nil }
                    } else {
                        $family(q, r, &args, $selector)?
                    };
                    Ok(q.keep(result)?)
                })
            })()
            .unwrap_or_else(abi::fault_reference)
        }
    };
}

entry!(node_integer_new, node_method, b"integer:");
entry!(node_float_bits_new, node_method, b"floatBits:");
entry!(node_string_new, node_method, b"string:");
entry!(node_symbol_new, node_method, b"symbol:");
entry!(node_nil_new, node_method, b"nilLiteral");
entry!(node_true_new, node_method, b"trueLiteral");
entry!(node_false_new, node_method, b"falseLiteral");
entry!(node_lexical_read_new, node_method, b"lexicalReadAt:");
entry!(node_global_read_new, node_method, b"globalRead:");
entry!(node_send_new, node_method, b"send:operands:");
entry!(node_block_new, node_method, b"block:captureBindings:");
entry!(node_kind, node_method, b"kind");
entry!(node_integer, node_method, b"integer");
entry!(node_float_bits, node_method, b"floatBits");
entry!(node_string, node_method, b"string");
entry!(node_symbol, node_method, b"symbol");
entry!(node_binding_index, node_method, b"bindingIndex");
entry!(node_global, node_method, b"global");
entry!(node_selector, node_method, b"selector");
entry!(node_operand_count, node_method, b"operandCount");
entry!(node_operand_at, node_method, b"operandAt:");
entry!(node_program, node_method, b"program");
entry!(node_capture_count, node_method, b"captureCount");
entry!(node_capture_binding_at, node_method, b"captureBindingAt:");
entry!(clone_0, node_method, b"clone");
entry!(statement_expression_new, statement_method, b"expression:");
entry!(statement_let_new, statement_method, b"letNamed:expression:");
entry!(statement_kind, statement_method, b"kind");
entry!(statement_expression, statement_method, b"expression");
entry!(statement_binding_name, statement_method, b"bindingName");
entry!(clone_1, statement_method, b"clone");
entry!(
    program_new,
    program_method,
    b"parameters:captures:statements:"
);
entry!(program_format, program_method, b"format");
entry!(program_arity, program_method, b"arity");
entry!(program_parameter_count, program_method, b"parameterCount");
entry!(program_capture_count, program_method, b"captureCount");
entry!(program_let_count, program_method, b"letCount");
entry!(program_binding_count, program_method, b"bindingCount");
entry!(program_statement_count, program_method, b"statementCount");
entry!(program_statement_at, program_method, b"statementAt:");
entry!(program_any_binding_at, program_method, b"bindingAt:");
entry!(program_parameter_at, program_method, b"parameterAt:");
entry!(program_capture_at, program_method, b"captureAt:");
entry!(program_let_at, program_method, b"letAt:");
entry!(clone_2, program_method, b"clone");
entry!(binding_kind, binding_method, b"kind");
entry!(binding_index, binding_method, b"index");
entry!(binding_ordinal, binding_method, b"ordinal");
entry!(binding_name, binding_method, b"name");
entry!(
    binding_defining_statement,
    binding_method,
    b"definingStatementIndex"
);
entry!(clone_3, binding_method, b"clone");
entry!(canonical_bytes, code_method, b"canonicalBytesOf:");
entry!(
    inventory_inspect,
    code_method,
    b"inspectInventory:bindings:selection:"
);
entry!(inventory_closure, code_method, b"inventory:matchesRoots:");
entry!(
    inventory_references,
    code_method,
    b"bindings:referencesBelow:"
);
entry!(admit_bytes, code_method, b"fromCanonicalBytes:");
entry!(code_object_from_program, code_method, b"fromProgram:");
entry!(
    code_object_from_program_provenance,
    code_method,
    b"fromProgram:provenance:"
);
entry!(code_object_format, code_method, b"format");
entry!(code_object_arity, code_method, b"arity");
entry!(code_object_capture_schema, code_method, b"captureSchema");
entry!(code_object_program, code_method, b"program");
entry!(code_object_digest, code_method, b"digest");
entry!(
    code_object_source_provenance,
    code_method,
    b"sourceProvenance"
);
entry!(
    code_object_signature_matches,
    code_method,
    b"nativeSignature:matchesProgram:"
);
entry!(
    code_object_is_native_signature,
    code_method,
    b"isNativeSignature:"
);
entry!(
    code_object_native_signature,
    code_method,
    b"nativeSignature"
);
entry!(
    code_object_with_native_signature,
    code_method,
    b"withNativeSignature:"
);
entry!(code_object_with_program, code_method, b"withProgram:");
entry!(
    code_object_without_source_provenance,
    code_method,
    b"withoutSourceProvenance"
);
entry!(clone_4, code_method, b"clone");

const FAMILIES: &[(&[u8], &[u8], &[(&[u8], usize)])] = &[
    (
        b"CodeNode",
        b"CodeNodeVt",
        &[
            (b"integer:", 3),
            (b"floatBits:", 4),
            (b"string:", 5),
            (b"symbol:", 6),
            (b"nilLiteral", 7),
            (b"trueLiteral", 8),
            (b"falseLiteral", 9),
            (b"lexicalReadAt:", 10),
            (b"globalRead:", 11),
            (b"send:operands:", 12),
            (b"block:captureBindings:", 13),
            (b"kind", 14),
            (b"integer", 15),
            (b"floatBits", 16),
            (b"string", 17),
            (b"symbol", 18),
            (b"bindingIndex", 19),
            (b"global", 20),
            (b"selector", 21),
            (b"operandCount", 22),
            (b"operandAt:", 23),
            (b"program", 24),
            (b"captureCount", 25),
            (b"captureBindingAt:", 26),
            (b"clone", 27),
        ],
    ),
    (
        b"CodeStatement",
        b"CodeStatementVt",
        &[
            (b"expression:", 28),
            (b"letNamed:expression:", 29),
            (b"kind", 30),
            (b"expression", 31),
            (b"bindingName", 32),
            (b"clone", 33),
        ],
    ),
    (
        b"CodeProgram",
        b"CodeProgramVt",
        &[
            (b"parameters:captures:statements:", 34),
            (b"format", 35),
            (b"arity", 36),
            (b"parameterCount", 37),
            (b"captureCount", 38),
            (b"letCount", 39),
            (b"bindingCount", 40),
            (b"statementCount", 41),
            (b"statementAt:", 42),
            (b"bindingAt:", 43),
            (b"parameterAt:", 44),
            (b"captureAt:", 45),
            (b"letAt:", 46),
            (b"clone", 47),
        ],
    ),
    (
        b"CodeLexicalBinding",
        b"CodeLexicalBindingVt",
        &[
            (b"kind", 48),
            (b"index", 49),
            (b"ordinal", 50),
            (b"name", 51),
            (b"definingStatementIndex", 52),
            (b"clone", 53),
        ],
    ),
    (
        b"CodeObject",
        b"CodeObjectVt",
        &[
            (b"canonicalBytesOf:", 54),
            (b"parameterCountOf:", 55),
            (b"captureCountOf:", 56),
            (b"inspectInventory:bindings:selection:", 57),
            (b"inventory:matchesRoots:", 58),
            (b"bindings:referencesBelow:", 59),
            (b"fromCanonicalBytes:", 60),
            (b"fromProgram:", 61),
            (b"fromProgram:provenance:", 62),
            (b"format", 63),
            (b"arity", 64),
            (b"captureSchema", 65),
            (b"program", 66),
            (b"digest", 67),
            (b"sourceProvenance", 68),
            (b"nativeSignature:matchesProgram:", 69),
            (b"isNativeSignature:", 70),
            (b"nativeSignature", 71),
            (b"withNativeSignature:", 72),
            (b"withProgram:", 73),
            (b"withoutSourceProvenance", 74),
            (b"clone", 75),
        ],
    ),
];
static INSTANCE: AtomicPtr<State> = AtomicPtr::new(ptr::null_mut());
static BUSY: AtomicBool = AtomicBool::new(false);
struct Constructing;
impl Drop for Constructing {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::Release);
    }
}

const NAMES: &[&[u8]] = &[
    b"releaseTokensProtecting",
    b"nameWithBytes",
    b"at:",
    b"behaviorOf:",
    b"release:",
    b"retain:",
    b"Word",
    b"fromUnsignedLow:high:",
    b"low32",
    b"high32",
    b"Integer",
    b"fromWord:",
    b"wordOf:",
    b"sequence",
    b"fromBytes:",
    b"String",
    b"bytes",
    b"sizeWord",
    b"atWord:",
    b"SymbolVt",
    b"StringVt",
    b"Error",
    b"typeMismatch",
    b"invalidPortableProgram",
    b"portableProgramLimit",
    b"kind:message:",
    b"allocate",
    b"allocationSizeOf:",
    b"finalize:with:",
    b"CodeLexicalBindingVt",
    b"size",
    b"integer:",
    b"lexicalReadAt:",
    b"string:",
    b"floatBits:",
    b"symbol:",
    b"globalRead:",
    b"nilLiteral",
    b"trueLiteral",
    b"falseLiteral",
    b"send:operands:",
    b"block:captureBindings:",
    b"operandAt:",
    b"captureBindingAt:",
    b"kind",
    b"integer",
    b"float",
    b"string",
    b"symbol",
    b"nil",
    b"true",
    b"false",
    b"lexicalRead",
    b"globalRead",
    b"send",
    b"block",
    b"floatBits",
    b"global",
    b"selector",
    b"bindingIndex",
    b"program",
    b"operandCount",
    b"captureCount",
    b"expression:",
    b"letNamed:expression:",
    b"expression",
    b"let",
    b"bindingName",
    b"CodeNodeVt",
    b"CodeProgramVt",
    b"parameters:captures:statements:",
    b"statementAt:",
    b"bindingAt:",
    b"parameterAt:",
    b"captureAt:",
    b"letAt:",
    b"CodeStatementVt",
    b"format",
    b"portableProgramV1",
    b"arity",
    b"parameterCount",
    b"letCount",
    b"bindingCount",
    b"statementCount",
    b"index",
    b"parameter",
    b"capture",
    b"ordinal",
    b"name",
    b"definingStatementIndex",
    b"inspectInventory:bindings:selection:",
    b"inventory:matchesRoots:",
    b"bindings:referencesBelow:",
    b"canonicalBytesOf:",
    b"Bytes",
    b"fromCanonicalBytes:",
    b"BytesVt",
    b"fromProgram:",
    b"fromProgram:provenance:",
    b"nativeSignature:matchesProgram:",
    b"isNativeSignature:",
    b"withNativeSignature:",
    b"withProgram:",
    b"sourceProvenance",
    b"nativeSignature",
    b"withoutSourceProvenance",
    b"digest",
    b"captureSchema",
    b"Vector",
    b"new",
    b"add:",
    b"visit:",
    b"clone",
    b"CodeNode",
    b"CodeStatement",
    b"CodeProgram",
    b"CodeLexicalBinding",
    b"CodeObject",
    b"CodeObjectVt",
    b"parameterCountOf:",
    b"captureCountOf:",
    b"value",
    b"at:put:",
    b"methodAt:",
    b"withContext:",
    b"ObjectModel",
    b"ownerOf:put:",
    b"trace:with:",
    b"register:",
    b"Roots",
    b"GC",
    b"ObjectVt",
    b"delegated",
    b"addMethod",
    b"rootsDo:",
    b"referencesOf:do:",
    b"finalize:",
    b"newMethodAt:",
];
static LITERALS: &[Literal] = &[
    Literal::new(b"releaseTokensProtecting"),
    Literal::new(b"nameWithBytes"),
    Literal::new(b"at:"),
    Literal::new(b"behaviorOf:"),
    Literal::new(b"release:"),
    Literal::new(b"retain:"),
    Literal::new(b"Word"),
    Literal::new(b"fromUnsignedLow:high:"),
    Literal::new(b"low32"),
    Literal::new(b"high32"),
    Literal::new(b"Integer"),
    Literal::new(b"fromWord:"),
    Literal::new(b"wordOf:"),
    Literal::new(b"sequence"),
    Literal::new(b"fromBytes:"),
    Literal::new(b"String"),
    Literal::new(b"bytes"),
    Literal::new(b"sizeWord"),
    Literal::new(b"atWord:"),
    Literal::new(b"SymbolVt"),
    Literal::new(b"StringVt"),
    Literal::new(b"Error"),
    Literal::new(b"typeMismatch"),
    Literal::new(b"invalidPortableProgram"),
    Literal::new(b"portableProgramLimit"),
    Literal::new(b"kind:message:"),
    Literal::new(b"allocate"),
    Literal::new(b"allocationSizeOf:"),
    Literal::new(b"finalize:with:"),
    Literal::new(b"CodeLexicalBindingVt"),
    Literal::new(b"size"),
    Literal::new(b"integer:"),
    Literal::new(b"lexicalReadAt:"),
    Literal::new(b"string:"),
    Literal::new(b"floatBits:"),
    Literal::new(b"symbol:"),
    Literal::new(b"globalRead:"),
    Literal::new(b"nilLiteral"),
    Literal::new(b"trueLiteral"),
    Literal::new(b"falseLiteral"),
    Literal::new(b"send:operands:"),
    Literal::new(b"block:captureBindings:"),
    Literal::new(b"operandAt:"),
    Literal::new(b"captureBindingAt:"),
    Literal::new(b"kind"),
    Literal::new(b"integer"),
    Literal::new(b"float"),
    Literal::new(b"string"),
    Literal::new(b"symbol"),
    Literal::new(b"nil"),
    Literal::new(b"true"),
    Literal::new(b"false"),
    Literal::new(b"lexicalRead"),
    Literal::new(b"globalRead"),
    Literal::new(b"send"),
    Literal::new(b"block"),
    Literal::new(b"floatBits"),
    Literal::new(b"global"),
    Literal::new(b"selector"),
    Literal::new(b"bindingIndex"),
    Literal::new(b"program"),
    Literal::new(b"operandCount"),
    Literal::new(b"captureCount"),
    Literal::new(b"expression:"),
    Literal::new(b"letNamed:expression:"),
    Literal::new(b"expression"),
    Literal::new(b"let"),
    Literal::new(b"bindingName"),
    Literal::new(b"CodeNodeVt"),
    Literal::new(b"CodeProgramVt"),
    Literal::new(b"parameters:captures:statements:"),
    Literal::new(b"statementAt:"),
    Literal::new(b"bindingAt:"),
    Literal::new(b"parameterAt:"),
    Literal::new(b"captureAt:"),
    Literal::new(b"letAt:"),
    Literal::new(b"CodeStatementVt"),
    Literal::new(b"format"),
    Literal::new(b"portableProgramV1"),
    Literal::new(b"arity"),
    Literal::new(b"parameterCount"),
    Literal::new(b"letCount"),
    Literal::new(b"bindingCount"),
    Literal::new(b"statementCount"),
    Literal::new(b"index"),
    Literal::new(b"parameter"),
    Literal::new(b"capture"),
    Literal::new(b"ordinal"),
    Literal::new(b"name"),
    Literal::new(b"definingStatementIndex"),
    Literal::new(b"inspectInventory:bindings:selection:"),
    Literal::new(b"inventory:matchesRoots:"),
    Literal::new(b"bindings:referencesBelow:"),
    Literal::new(b"canonicalBytesOf:"),
    Literal::new(b"Bytes"),
    Literal::new(b"fromCanonicalBytes:"),
    Literal::new(b"BytesVt"),
    Literal::new(b"fromProgram:"),
    Literal::new(b"fromProgram:provenance:"),
    Literal::new(b"nativeSignature:matchesProgram:"),
    Literal::new(b"isNativeSignature:"),
    Literal::new(b"withNativeSignature:"),
    Literal::new(b"withProgram:"),
    Literal::new(b"sourceProvenance"),
    Literal::new(b"nativeSignature"),
    Literal::new(b"withoutSourceProvenance"),
    Literal::new(b"digest"),
    Literal::new(b"captureSchema"),
    Literal::new(b"Vector"),
    Literal::new(b"new"),
    Literal::new(b"add:"),
    Literal::new(b"visit:"),
    Literal::new(b"clone"),
    Literal::new(b"CodeNode"),
    Literal::new(b"CodeStatement"),
    Literal::new(b"CodeProgram"),
    Literal::new(b"CodeLexicalBinding"),
    Literal::new(b"CodeObject"),
    Literal::new(b"CodeObjectVt"),
    Literal::new(b"parameterCountOf:"),
    Literal::new(b"captureCountOf:"),
    Literal::new(b"value"),
    Literal::new(b"at:put:"),
    Literal::new(b"methodAt:"),
    Literal::new(b"withContext:"),
    Literal::new(b"ObjectModel"),
    Literal::new(b"ownerOf:put:"),
    Literal::new(b"trace:with:"),
    Literal::new(b"register:"),
    Literal::new(b"Roots"),
    Literal::new(b"GC"),
    Literal::new(b"ObjectVt"),
    Literal::new(b"delegated"),
    Literal::new(b"addMethod"),
    Literal::new(b"rootsDo:"),
    Literal::new(b"referencesOf:do:"),
    Literal::new(b"finalize:"),
    Literal::new(b"newMethodAt:"),
];
static METHODS: &[MethodSpec] = &[
    MethodSpec {
        name: 134,
        entry: Some(roots),
    },
    MethodSpec {
        name: 135,
        entry: Some(references),
    },
    MethodSpec {
        name: 136,
        entry: Some(finalize),
    },
    MethodSpec {
        name: 31,
        entry: Some(node_integer_new),
    },
    MethodSpec {
        name: 34,
        entry: Some(node_float_bits_new),
    },
    MethodSpec {
        name: 33,
        entry: Some(node_string_new),
    },
    MethodSpec {
        name: 35,
        entry: Some(node_symbol_new),
    },
    MethodSpec {
        name: 37,
        entry: Some(node_nil_new),
    },
    MethodSpec {
        name: 38,
        entry: Some(node_true_new),
    },
    MethodSpec {
        name: 39,
        entry: Some(node_false_new),
    },
    MethodSpec {
        name: 32,
        entry: Some(node_lexical_read_new),
    },
    MethodSpec {
        name: 36,
        entry: Some(node_global_read_new),
    },
    MethodSpec {
        name: 40,
        entry: Some(node_send_new),
    },
    MethodSpec {
        name: 41,
        entry: Some(node_block_new),
    },
    MethodSpec {
        name: 44,
        entry: Some(node_kind),
    },
    MethodSpec {
        name: 45,
        entry: Some(node_integer),
    },
    MethodSpec {
        name: 56,
        entry: Some(node_float_bits),
    },
    MethodSpec {
        name: 47,
        entry: Some(node_string),
    },
    MethodSpec {
        name: 48,
        entry: Some(node_symbol),
    },
    MethodSpec {
        name: 59,
        entry: Some(node_binding_index),
    },
    MethodSpec {
        name: 57,
        entry: Some(node_global),
    },
    MethodSpec {
        name: 58,
        entry: Some(node_selector),
    },
    MethodSpec {
        name: 61,
        entry: Some(node_operand_count),
    },
    MethodSpec {
        name: 42,
        entry: Some(node_operand_at),
    },
    MethodSpec {
        name: 60,
        entry: Some(node_program),
    },
    MethodSpec {
        name: 62,
        entry: Some(node_capture_count),
    },
    MethodSpec {
        name: 43,
        entry: Some(node_capture_binding_at),
    },
    MethodSpec {
        name: 112,
        entry: Some(clone_0),
    },
    MethodSpec {
        name: 63,
        entry: Some(statement_expression_new),
    },
    MethodSpec {
        name: 64,
        entry: Some(statement_let_new),
    },
    MethodSpec {
        name: 44,
        entry: Some(statement_kind),
    },
    MethodSpec {
        name: 65,
        entry: Some(statement_expression),
    },
    MethodSpec {
        name: 67,
        entry: Some(statement_binding_name),
    },
    MethodSpec {
        name: 112,
        entry: Some(clone_1),
    },
    MethodSpec {
        name: 70,
        entry: Some(program_new),
    },
    MethodSpec {
        name: 77,
        entry: Some(program_format),
    },
    MethodSpec {
        name: 79,
        entry: Some(program_arity),
    },
    MethodSpec {
        name: 80,
        entry: Some(program_parameter_count),
    },
    MethodSpec {
        name: 62,
        entry: Some(program_capture_count),
    },
    MethodSpec {
        name: 81,
        entry: Some(program_let_count),
    },
    MethodSpec {
        name: 82,
        entry: Some(program_binding_count),
    },
    MethodSpec {
        name: 83,
        entry: Some(program_statement_count),
    },
    MethodSpec {
        name: 71,
        entry: Some(program_statement_at),
    },
    MethodSpec {
        name: 72,
        entry: Some(program_any_binding_at),
    },
    MethodSpec {
        name: 73,
        entry: Some(program_parameter_at),
    },
    MethodSpec {
        name: 74,
        entry: Some(program_capture_at),
    },
    MethodSpec {
        name: 75,
        entry: Some(program_let_at),
    },
    MethodSpec {
        name: 112,
        entry: Some(clone_2),
    },
    MethodSpec {
        name: 44,
        entry: Some(binding_kind),
    },
    MethodSpec {
        name: 84,
        entry: Some(binding_index),
    },
    MethodSpec {
        name: 87,
        entry: Some(binding_ordinal),
    },
    MethodSpec {
        name: 88,
        entry: Some(binding_name),
    },
    MethodSpec {
        name: 89,
        entry: Some(binding_defining_statement),
    },
    MethodSpec {
        name: 112,
        entry: Some(clone_3),
    },
    MethodSpec {
        name: 93,
        entry: Some(canonical_bytes),
    },
    MethodSpec {
        name: 119,
        entry: Some(parameter_count_of),
    },
    MethodSpec {
        name: 120,
        entry: Some(capture_count_of),
    },
    MethodSpec {
        name: 90,
        entry: Some(inventory_inspect),
    },
    MethodSpec {
        name: 91,
        entry: Some(inventory_closure),
    },
    MethodSpec {
        name: 92,
        entry: Some(inventory_references),
    },
    MethodSpec {
        name: 95,
        entry: Some(admit_bytes),
    },
    MethodSpec {
        name: 97,
        entry: Some(code_object_from_program),
    },
    MethodSpec {
        name: 98,
        entry: Some(code_object_from_program_provenance),
    },
    MethodSpec {
        name: 77,
        entry: Some(code_object_format),
    },
    MethodSpec {
        name: 79,
        entry: Some(code_object_arity),
    },
    MethodSpec {
        name: 107,
        entry: Some(code_object_capture_schema),
    },
    MethodSpec {
        name: 60,
        entry: Some(code_object_program),
    },
    MethodSpec {
        name: 106,
        entry: Some(code_object_digest),
    },
    MethodSpec {
        name: 103,
        entry: Some(code_object_source_provenance),
    },
    MethodSpec {
        name: 99,
        entry: Some(code_object_signature_matches),
    },
    MethodSpec {
        name: 100,
        entry: Some(code_object_is_native_signature),
    },
    MethodSpec {
        name: 104,
        entry: Some(code_object_native_signature),
    },
    MethodSpec {
        name: 101,
        entry: Some(code_object_with_native_signature),
    },
    MethodSpec {
        name: 102,
        entry: Some(code_object_with_program),
    },
    MethodSpec {
        name: 105,
        entry: Some(code_object_without_source_provenance),
    },
    MethodSpec {
        name: 112,
        entry: Some(clone_4),
    },
];
unsafe extern "C" fn initialize(
    p: *const Protocol,
    image: Oop,
    l: *const Oop,
    n: usize,
    a: *const Oop,
    count: usize,
) -> Oop {
    API.store(p.cast_mut(), Ordering::Release);
    (|| unsafe {
        let id = *p;
        let [ns] = borrowed_slice(a, count).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let symbols = borrowed_slice(l, n)
            .filter(|v| v.len() == NAMES.len())
            .ok_or_else(invalid)?;
        let old = INSTANCE.load(Ordering::Acquire);
        if !old.is_null() {
            let s = &*old;
            return if s.ready.get() && s.ns == *ns && s.id.root == id.root {
                Ok(s.families[0].1)
            } else {
                Err(invalid())
            };
        }
        if BUSY.swap(true, Ordering::AcqRel) {
            return Err(invalid());
        }
        let _busy = Constructing;
        let names: HashMap<_, _> = NAMES.iter().copied().zip(symbols.iter().copied()).collect();
        let send = |r, name: &[u8], args: &[Oop]| id.send(r, names[name], args);
        let named = |name: &[u8]| send(*ns, b"at:", &[names[name]]);
        let roots = named(b"Roots")?;
        let gc = named(b"GC")?;
        let nil = named(b"nil")?;
        let parent = named(b"ObjectVt")?;
        let mut tokens = Vec::new();
        let mut keep = |v| {
            tokens.push(send(roots, b"retain:", &[v])?);
            Ok::<Oop, Oop>(v)
        };
        let mut families = [(ptr::null_mut(), ptr::null_mut()); 5];
        for f in &mut families {
            let vt = keep(send(parent, b"delegated", &[])?)?;
            *f = (vt, keep(send(vt, b"allocate", &[abi::size(0).unwrap()])?)?);
        }
        let s = Box::leak(Box::new(State {
            id,
            names,
            ns: *ns,
            roots,
            gc,
            nil,
            families,
            values: RefCell::new(HashMap::new()),
            thread: std::thread::current().id(),
            ready: Cell::new(false),
        }));
        INSTANCE.store(s, Ordering::Release);
        let bind = |vt, selector: &[u8], index: usize| -> Result<(), Oop> {
            let method = s.send(
                image,
                b"newMethodAt:",
                &[abi::size(index).ok_or_else(invalid)?],
            )?;
            let method = s.send(method, b"withContext:", &[*ns])?;
            s.send(vt, b"addMethod", &[s.symbol(selector)?, method])?;
            Ok(())
        };
        for (i, name) in [b"rootsDo:".as_slice(), b"referencesOf:do:", b"finalize:"]
            .iter()
            .enumerate()
        {
            bind(families[0].0, name, i)?;
        }
        s.send(roots, b"register:", &[families[0].1])?;
        for ((name, vt_name, methods), (vt, prototype)) in FAMILIES.iter().zip(families) {
            for &(selector, index) in *methods {
                bind(vt, selector, index)?;
            }
            s.send(id.root, b"ownerOf:put:", &[vt, prototype])?;
            s.send(gc, b"trace:with:", &[vt, families[0].1])?;
            s.send(*ns, b"at:put:", &[s.symbol(name)?, prototype])?;
            s.send(*ns, b"at:put:", &[s.symbol(vt_name)?, vt])?;
        }
        for token in tokens {
            s.send(roots, b"release:", &[token])?;
        }
        s.ready.set(true);
        Ok(families[0].1)
    })()
    .unwrap_or_else(abi::fault_reference)
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
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_portable_code_rust_identity() -> *const core::ffi::c_char {
    c"rust-portable-code-k1".as_ptr()
}
