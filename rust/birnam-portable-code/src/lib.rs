//! Reified PortableProgram objects. Every connection to another owner is an Id send.
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as native, Method, NativeEntry, Operand, Outcome, Protocol},
};
use birnam_artifact::portable_program::{self as portable, ValidatedProgram};
use core::{cell::RefCell, ffi::c_void, ptr};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    rc::Rc,
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};
mod codec;

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
    Binding { program: Oop, index: u32 },
    Code { program: Oop, provenance: Oop },
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
            } => vec![*program, *provenance],
        }
    }
}
struct State {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    gc: Oop,
    nil: Oop,
    string: Oop,
    string_vt: Oop,
    integer: Oop,
    symbol_vt: Oop,
    families: [(Oop, Oop); 5],
    // Native values belong to this module, never to mutable foreign payloads.
    // Snapshots release the map borrow before any reentrant send.
    values: RefCell<HashMap<usize, Value>>,
    thread: std::thread::ThreadId,
}
struct Scope<'a> {
    state: &'a State,
    tokens: RefCell<Vec<usize>>,
}
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
fn object(v: Operand) -> Result<Oop, Outcome> {
    v.as_object().filter(|v| !v.is_null()).ok_or_else(invalid)
}
impl State {
    fn send(&self, receiver: Oop, name: &[u8], args: &[Operand]) -> Result<Operand, Outcome> {
        unsafe { self.id.send_named(receiver, name, args) }
    }
    fn symbol(&self, name: &[u8]) -> Result<Oop, Outcome> {
        unsafe { self.id.intern_name(name) }
    }
    fn named(&self, name: &[u8]) -> Result<Operand, Outcome> {
        self.send(self.ns, b"at:", &[Operand::object(self.symbol(name)?)])
    }
    fn behavior(&self, o: Oop) -> Result<Oop, Outcome> {
        object(self.send(self.id.receiver, b"behaviorOf:", &[Operand::object(o)])?)
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
    fn code(&self, o: Oop) -> Option<(Oop, Oop)> {
        match self.values.borrow().get(&o.addr())? {
            Value::Code {
                program,
                provenance,
            } => Some((*program, *provenance)),
            _ => None,
        }
    }
    fn scoped<T>(&self, f: impl FnOnce(&Scope<'_>) -> Result<T, Outcome>) -> Result<T, Outcome> {
        let scope = Scope {
            state: self,
            tokens: RefCell::new(Vec::new()),
        };
        let result = f(&scope);
        let mut release_error = None;
        for token in scope.tokens.into_inner() {
            match self.send(self.roots, b"release:", &[Operand::word(token)]) {
                Ok(v) if v.as_word() == Some(1) => {}
                Ok(_) => {
                    release_error.get_or_insert_with(invalid);
                }
                Err(e) => {
                    release_error.get_or_insert(e);
                }
            }
        }
        match (result, release_error) {
            (Ok(_), Some(e)) => Err(e),
            (r, _) => r,
        }
    }
}
impl Scope<'_> {
    fn keep(&self, o: Oop) -> Result<Oop, Outcome> {
        let token = self
            .state
            .send(self.state.roots, b"retain:", &[Operand::object(o)])?
            .as_word()
            .filter(|v| *v != 0)
            .ok_or_else(invalid)?;
        self.tokens.borrow_mut().push(token);
        Ok(o)
    }
    fn call(&self, r: Oop, name: &[u8], args: &[Oop]) -> Result<Oop, Outcome> {
        let args = args
            .iter()
            .copied()
            .map(Operand::object)
            .collect::<Vec<_>>();
        self.keep(object(self.state.send(r, name, &args)?)?)
    }
    fn number(&self, n: i64) -> Result<Oop, Outcome> {
        self.keep(object(self.state.send(
            self.state.integer,
            b"fromWord:",
            &[Operand::word(n as usize)],
        )?)?)
    }
    fn integer(&self, o: Oop) -> Result<Option<i64>, Outcome> {
        Ok(self
            .state
            .send(self.state.integer, b"wordOf:", &[Operand::object(o)])?
            .as_word()
            .map(|n| n as i64))
    }
    fn string(&self, bytes: &[u8]) -> Result<Oop, Outcome> {
        self.keep(object(self.state.send(
            self.state.string,
            b"fromBytes:",
            &[Operand::bytes(bytes)],
        )?)?)
    }
    fn bytes(&self, o: Oop, symbol: bool) -> Result<Option<Vec<u8>>, Outcome> {
        let s = self.state;
        if s.behavior(o)? != if symbol { s.symbol_vt } else { s.string_vt } {
            return Ok(None);
        }
        let result = if symbol {
            s.send(s.id.receiver, b"symbolBytesOf:", &[Operand::object(o)])?
        } else {
            s.send(o, b"bytes", &[])?
        };
        let bytes = result.as_bytes().ok_or_else(invalid)?;
        // Copy the foreign borrow before any send, including retain:.
        Ok(Some(
            unsafe { borrowed_bytes(bytes.pointer, bytes.length) }
                .ok_or_else(invalid)?
                .to_vec(),
        ))
    }
    fn fail(&self, failure: Failure, message: &[u8]) -> Result<Oop, Outcome> {
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
    fn make(&self, behavior: Oop, value: Value) -> Result<Oop, Outcome> {
        let s = self.state;
        self.keep(behavior)?;
        for edge in value.edges() {
            self.keep(edge)?;
        }
        let extent = size_of::<usize>();
        let o = self.keep(object(s.send(
            behavior,
            b"allocate",
            &[Operand::word(extent)],
        )?)?)?;
        if s.behavior(o)? != behavior
            || s.send(s.id.receiver, b"allocationSizeOf:", &[Operand::object(o)])?
                .as_word()
                != Some(extent)
            || s.values.borrow().contains_key(&o.addr())
        {
            return Err(invalid());
        }
        s.values.borrow_mut().insert(o.addr(), value);
        s.send(
            s.gc,
            b"finalize:with:",
            &[Operand::object(o), Operand::object(s.families[0].1)],
        )?;
        Ok(o)
    }
    fn node(&self, receiver: Oop, value: NodeValue) -> Result<Oop, Outcome> {
        self.make(self.state.behavior(receiver)?, Value::Node(Rc::new(value)))
    }
    fn statement(&self, receiver: Oop, value: StatementValue) -> Result<Oop, Outcome> {
        self.make(
            self.state.behavior(receiver)?,
            Value::Statement(Rc::new(value)),
        )
    }
    fn binding(&self, program: Oop, index: u32) -> Result<Oop, Outcome> {
        let Some(vt) = self.state.named(b"CodeLexicalBindingVt")?.as_object() else {
            return Ok(self.state.nil);
        };
        self.make(vt, Value::Binding { program, index })
    }
    fn code(&self, receiver: Oop, program: Oop, provenance: Oop) -> Result<Oop, Outcome> {
        if self.state.program(program).is_none() {
            return self.fail(
                Failure::TypeMismatch,
                b"CodeObject program must be a CodeProgram",
            );
        }
        self.make(
            self.state.behavior(receiver)?,
            Value::Code {
                program,
                provenance,
            },
        )
    }
    fn collection(
        &self,
        collection: Oop,
        limit: usize,
    ) -> Result<Result<Vec<Oop>, Failure>, Outcome> {
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
    fn names(&self, values: &[Oop]) -> Result<Result<Vec<Vec<u8>>, Failure>, Outcome> {
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
    fn index(&self, args: &[Oop]) -> Result<Option<usize>, Outcome> {
        if let [index] = args {
            Ok(self.integer(*index)?.and_then(|n| usize::try_from(n).ok()))
        } else {
            Ok(None)
        }
    }
}

fn node_method(q: &Scope<'_>, r: Oop, a: &[Oop], op: &[u8]) -> Result<Oop, Outcome> {
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
fn statement_method(q: &Scope<'_>, r: Oop, a: &[Oop], op: &[u8]) -> Result<Oop, Outcome> {
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
fn decode_node_tree(
    q: &Scope<'_>,
    program: &portable::Program<'_>,
    root: u32,
) -> Result<Oop, Outcome> {
    let s = q.state;
    let root = root as usize;
    let start = codec::decoded_subtree_start(program, root).ok_or_else(invalid)?;
    let Some(vt) = s.named(b"CodeNodeVt")?.as_object() else {
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
                let Some(program_vt) = s.named(b"CodeProgramVt")?.as_object() else {
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
fn program_method(q: &Scope<'_>, r: Oop, a: &[Oop], op: &[u8]) -> Result<Oop, Outcome> {
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
fn binding_method(q: &Scope<'_>, r: Oop, a: &[Oop], op: &[u8]) -> Result<Oop, Outcome> {
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
fn code_method(q: &Scope<'_>, r: Oop, a: &[Oop], op: &[u8]) -> Result<Oop, Outcome> {
    let s = q.state;
    match op {
        b"canonicalBytesOf:" => {
            let [value] = a else {
                return Ok(s.nil);
            };
            let Some((program, _)) = s.code(*value) else {
                return Ok(s.nil);
            };
            let bytes = s.program(program).ok_or_else(invalid)?;
            let factory = q.keep(object(s.named(b"Bytes")?)?)?;
            return q.keep(object(s.send(
                factory,
                b"fromBytes:",
                &[Operand::bytes(bytes.as_ref())],
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
            let raw = s.send(*value, b"bytes", &[])?;
            let raw = raw.as_bytes().ok_or_else(invalid)?;
            let bytes = unsafe { borrowed_bytes(raw.pointer, raw.length) }
                .ok_or_else(invalid)?
                .to_vec();
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
        b"withProgram:" => {
            let [p] = a else {
                return q.fail(Failure::TypeMismatch, b"withProgram: needs one CodeProgram");
            };
            let Some((_, provenance)) = s.code(r) else {
                return Ok(s.nil);
            };
            return q.code(r, *p, provenance);
        }
        _ => {}
    }
    if !a.is_empty() {
        return Ok(s.nil);
    }
    let Some((program, provenance)) = s.code(r) else {
        return Ok(s.nil);
    };
    match op {
        b"format" => return s.symbol(b"portableProgramV1"),
        b"program" => return Ok(program),
        b"sourceProvenance" => return Ok(provenance),
        b"withoutSourceProvenance" => return q.code(r, program, s.nil),
        _ => {}
    }
    let bytes = s.program(program).ok_or_else(invalid)?;
    let inspected = bytes.view();
    match op {
        b"arity" => q.number(i64::from(inspected.parameter_count)),
        b"digest" => q.string(&Sha256::digest(bytes.as_ref())),
        b"captureSchema" => {
            let Some(factory) = s.named(b"Vector")?.as_object() else {
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

unsafe fn state<'a>(context: *mut c_void) -> Result<&'a State, Outcome> {
    let s = unsafe { &*context.cast::<State>() };
    if s.thread != std::thread::current().id() {
        return Err(invalid());
    }
    Ok(s)
}
unsafe extern "C" fn roots(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    (|| unsafe {
        let s = state(context)?;
        let Some([visitor]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let visitor = object(*visitor)?;
        let mut roots = vec![
            s.ns,
            s.roots,
            s.gc,
            s.nil,
            s.string,
            s.string_vt,
            s.integer,
            s.symbol_vt,
        ];
        for (vt, prototype) in s.families {
            roots.extend([vt, prototype]);
        }
        for root in roots {
            s.send(visitor, b"visit:", &[Operand::object(root)])?;
        }
        Ok(Operand::unit())
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
unsafe extern "C" fn references(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
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
            s.send(visitor, b"visit:", &[Operand::object(edge)])?;
        }
        Ok(Operand::unit())
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
unsafe extern "C" fn finalize(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
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
        Ok(Operand::unit())
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
macro_rules! entry {
    ($name:ident, $family:ident, $selector:literal) => {
        unsafe extern "C" fn $name(
            context: *mut c_void,
            r: Oop,
            args: *const Operand,
            count: usize,
        ) -> Outcome {
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
                    Ok(Operand::object(q.keep(result)?))
                })
            })()
            .map_or_else(|e| e, Outcome::loaded)
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
entry!(code_object_with_program, code_method, b"withProgram:");
entry!(
    code_object_without_source_provenance,
    code_method,
    b"withoutSourceProvenance"
);
entry!(clone_4, code_method, b"clone");

const FAMILIES: &[(&[u8], &[u8], &[(&[u8], Method)])] = &[
    (
        b"CodeNode",
        b"CodeNodeVt",
        &[
            (b"integer:", node_integer_new),
            (b"floatBits:", node_float_bits_new),
            (b"string:", node_string_new),
            (b"symbol:", node_symbol_new),
            (b"nilLiteral", node_nil_new),
            (b"trueLiteral", node_true_new),
            (b"falseLiteral", node_false_new),
            (b"lexicalReadAt:", node_lexical_read_new),
            (b"globalRead:", node_global_read_new),
            (b"send:operands:", node_send_new),
            (b"block:captureBindings:", node_block_new),
            (b"kind", node_kind),
            (b"integer", node_integer),
            (b"floatBits", node_float_bits),
            (b"string", node_string),
            (b"symbol", node_symbol),
            (b"bindingIndex", node_binding_index),
            (b"global", node_global),
            (b"selector", node_selector),
            (b"operandCount", node_operand_count),
            (b"operandAt:", node_operand_at),
            (b"program", node_program),
            (b"captureCount", node_capture_count),
            (b"captureBindingAt:", node_capture_binding_at),
            (b"clone", clone_0),
        ],
    ),
    (
        b"CodeStatement",
        b"CodeStatementVt",
        &[
            (b"expression:", statement_expression_new),
            (b"letNamed:expression:", statement_let_new),
            (b"kind", statement_kind),
            (b"expression", statement_expression),
            (b"bindingName", statement_binding_name),
            (b"clone", clone_1),
        ],
    ),
    (
        b"CodeProgram",
        b"CodeProgramVt",
        &[
            (b"parameters:captures:statements:", program_new),
            (b"format", program_format),
            (b"arity", program_arity),
            (b"parameterCount", program_parameter_count),
            (b"captureCount", program_capture_count),
            (b"letCount", program_let_count),
            (b"bindingCount", program_binding_count),
            (b"statementCount", program_statement_count),
            (b"statementAt:", program_statement_at),
            (b"bindingAt:", program_any_binding_at),
            (b"parameterAt:", program_parameter_at),
            (b"captureAt:", program_capture_at),
            (b"letAt:", program_let_at),
            (b"clone", clone_2),
        ],
    ),
    (
        b"CodeLexicalBinding",
        b"CodeLexicalBindingVt",
        &[
            (b"kind", binding_kind),
            (b"index", binding_index),
            (b"ordinal", binding_ordinal),
            (b"name", binding_name),
            (b"definingStatementIndex", binding_defining_statement),
            (b"clone", clone_3),
        ],
    ),
    (
        b"CodeObject",
        b"CodeObjectVt",
        &[
            (b"canonicalBytesOf:", canonical_bytes),
            (b"fromCanonicalBytes:", admit_bytes),
            (b"fromProgram:", code_object_from_program),
            (
                b"fromProgram:provenance:",
                code_object_from_program_provenance,
            ),
            (b"format", code_object_format),
            (b"arity", code_object_arity),
            (b"captureSchema", code_object_capture_schema),
            (b"program", code_object_program),
            (b"digest", code_object_digest),
            (b"sourceProvenance", code_object_source_provenance),
            (b"withProgram:", code_object_with_program),
            (
                b"withoutSourceProvenance",
                code_object_without_source_provenance,
            ),
            (b"clone", clone_4),
        ],
    ),
];
static INSTANCE: AtomicPtr<State> = AtomicPtr::new(ptr::null_mut());
static BUSY: AtomicBool = AtomicBool::new(false);
unsafe fn construct(id: Protocol, ns: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let named = |name: &[u8]| {
            object(id.send_named(ns, b"at:", &[Operand::object(id.intern_name(name)?)])?)
        };
        let roots = named(b"Roots")?;
        let mut tokens = Vec::new();
        let result = (|| {
            let mut keep = |o| -> Result<Oop, Outcome> {
                tokens.push(
                    id.send_named(roots, b"retain:", &[Operand::object(o)])?
                        .as_word()
                        .filter(|t| *t != 0)
                        .ok_or_else(invalid)?,
                );
                Ok(o)
            };
            keep(ns)?;
            keep(roots)?;
            let gc = keep(named(b"GC")?)?;
            let nil = keep(named(b"nil")?)?;
            let string = keep(named(b"String")?)?;
            let string_vt = keep(named(b"StringVt")?)?;
            let integer = keep(named(b"Integer")?)?;
            let symbol_vt = keep(named(b"SymbolVt")?)?;
            let parent = keep(named(b"ObjectVt")?)?;
            keep(named(b"Error")?)?;
            let mut families = [(ptr::null_mut(), ptr::null_mut()); 5];
            for family in &mut families {
                let vt = keep(object(id.send_named(parent, b"delegated", &[])?)?)?;
                let prototype = keep(object(id.send_named(
                    vt,
                    b"allocate",
                    &[Operand::word(0)],
                )?)?)?;
                *family = (vt, prototype);
            }
            let pointer = Box::into_raw(Box::new(State {
                id,
                ns,
                roots,
                gc,
                nil,
                string,
                string_vt,
                integer,
                symbol_vt,
                families,
                values: RefCell::new(HashMap::new()),
                thread: std::thread::current().id(),
            }));
            // Published native entries keep this context valid even if a later send fails.
            for (name, method) in [
                (b"rootsDo:".as_slice(), self::roots as Method),
                (b"referencesOf:do:", references as Method),
                (b"finalize:", finalize as Method),
            ] {
                id.send_named(
                    families[0].0,
                    b"addMethod",
                    &[
                        Operand::object(id.intern_name(name)?),
                        Operand::entry(NativeEntry::new(method, pointer.cast())),
                    ],
                )?;
            }
            for ((name, vt_name, methods), (vt, prototype)) in FAMILIES.iter().zip(families) {
                for &(selector, method) in *methods {
                    id.send_named(
                        vt,
                        b"addMethod",
                        &[
                            Operand::object(id.intern_name(selector)?),
                            Operand::entry(NativeEntry::new(method, pointer.cast())),
                        ],
                    )?;
                }
                id.send_named(
                    id.receiver,
                    b"ownerOf:put:",
                    &[Operand::object(vt), Operand::object(prototype)],
                )?;
                id.send_named(
                    gc,
                    b"trace:with:",
                    &[Operand::object(vt), Operand::object(families[0].1)],
                )?;
                for (name, value) in [(*name, prototype), (*vt_name, vt)] {
                    id.send_named(
                        ns,
                        b"at:put:",
                        &[
                            Operand::object(id.intern_name(name)?),
                            Operand::object(value),
                        ],
                    )?;
                }
            }
            id.send_named(roots, b"register:", &[Operand::object(families[0].1)])?;
            INSTANCE.store(pointer, Ordering::Release);
            Ok(Operand::unit())
        })();
        for token in tokens {
            id.send_named(roots, b"release:", &[Operand::word(token)])?;
        }
        result
    }
}
unsafe extern "C" fn initialize(
    id: *const Protocol,
    args: *const Operand,
    count: usize,
) -> Outcome {
    (|| unsafe {
        if !native::valid_protocol(id) {
            return Err(invalid());
        }
        let Some([ns]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let ns = object(*ns)?;
        let existing = INSTANCE.load(Ordering::Acquire);
        if !existing.is_null() {
            let s = &*existing;
            return if s.id.same_contract(&*id)
                && s.ns == ns
                && s.thread == std::thread::current().id()
            {
                Ok(Operand::unit())
            } else {
                Err(invalid())
            };
        }
        if BUSY.swap(true, Ordering::AcqRel) {
            return Err(invalid());
        }
        let result = construct(*id, ns);
        BUSY.store(false, Ordering::Release);
        result
    })()
    .map_or_else(|e| e, Outcome::loaded)
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
#[unsafe(no_mangle)]
pub extern "C" fn birnam_portable_code_rust_identity() -> *const core::ffi::c_char {
    c"rust-portable-code-k1".as_ptr()
}
