//! PEG's existing matcher over production Id-owned pattern, grammar and CST objects.
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as native, Method, NativeEntry, Operand, Outcome, Protocol},
};
use core::{cell::RefCell, ffi::c_void, ptr};
use std::{collections::HashMap, rc::Rc};
include!("matcher.rs");
fn abort(message: &str) -> ! {
    panic!("invalid private PEG graph: {message}")
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
#[derive(Clone)]
enum Value {
    Pattern(PatternState),
    Grammar(Vec<(Oop, Oop)>, usize),
    Cst(Oop, usize, usize, Vec<Oop>),
}
impl Value {
    fn refs(&self) -> Vec<Oop> {
        match self {
            Self::Pattern(p) => p
                .children
                .iter()
                .copied()
                .chain([p.rule_name])
                .filter(|v| !v.is_null())
                .collect(),
            Self::Grammar(r, _) => r.iter().flat_map(|&(n, p)| [n, p]).collect(),
            Self::Cst(r, _, _, c) => c.iter().copied().chain([*r]).collect(),
        }
    }
}
struct State {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    gc: Oop,
    model: Oop,
    receiver: Oop,
    behaviors: [Oop; 3],
    values: RefCell<HashMap<usize, Value>>,
    permanent: Vec<Oop>,
    thread: std::thread::ThreadId,
}
struct Scope<'a> {
    s: &'a State,
    tokens: RefCell<Vec<usize>>,
}
impl Scope<'_> {
    unsafe fn keep(&self, v: Oop) -> Result<Oop, Outcome> {
        let t = unsafe {
            self.s
                .id
                .send_named(self.s.roots, b"retain:", &[Operand::object(v)])
        }?
        .as_word()
        .ok_or_else(invalid)?;
        self.tokens.borrow_mut().push(t);
        Ok(v)
    }
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        for t in self.tokens.get_mut().drain(..) {
            unsafe {
                self.s
                    .id
                    .send_named(self.s.roots, b"release:", &[Operand::word(t)])
                    .expect("release PEG root");
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
    unsafe fn bytes(&self, v: Oop) -> Result<Vec<u8>, Outcome> {
        unsafe {
            let b = self
                .send(v, b"bytes", &[])?
                .as_bytes()
                .ok_or_else(invalid)?;
            Ok(borrowed_bytes(b.pointer, b.length)
                .ok_or_else(invalid)?
                .to_vec())
        }
    }
    unsafe fn number(&self, v: usize) -> Result<Operand, Outcome> {
        unsafe { self.send(self.named(b"Integer")?, b"fromWord:", &[Operand::word(v)]) }
    }
    unsafe fn index(&self, v: Operand) -> Result<usize, Outcome> {
        unsafe { self.send(self.named(b"Integer")?, b"wordOf:", &[v]) }?
            .as_word()
            .filter(|v| *v <= isize::MAX as usize)
            .ok_or_else(invalid)
    }
    unsafe fn byte(&self, v: Oop) -> Result<u8, Outcome> {
        unsafe {
            let integer = self.send(self.named(b"Integer")?, b"wordOf:", &[Operand::object(v)])?;
            if let Some(word) = integer.as_word() {
                return Ok(word as u8);
            }
            Ok(self.bytes(v)?.first().copied().unwrap_or(0))
        }
    }
    unsafe fn symbol(&self, v: Oop) -> Result<Oop, Outcome> {
        unsafe {
            match self.send(self.id.receiver, b"symbolBytesOf:", &[Operand::object(v)]) {
                Ok(b) if b.as_bytes().is_some() => Ok(v),
                _ => self.id.intern_name(&self.bytes(v)?),
            }
        }
    }
    unsafe fn allocate(&self, scope: &Scope<'_>, v: Value) -> Result<Oop, Outcome> {
        unsafe {
            let b = self.behaviors[match v {
                Value::Pattern(_) => 0,
                Value::Grammar(..) => 1,
                Value::Cst(..) => 2,
            }];
            let o = scope.keep(object(self.send(
                self.model,
                b"allocate:bytes:",
                &[Operand::object(b), Operand::word(0)],
            )?)?)?;
            self.values.borrow_mut().insert(o as usize, v);
            Ok(o)
        }
    }
    fn value(&self, v: Oop) -> Result<Value, Outcome> {
        self.values
            .borrow()
            .get(&(v as usize))
            .cloned()
            .ok_or_else(invalid)
    }
    fn pattern(&self, v: Oop) -> Result<PatternState, Outcome> {
        match self.value(v)? {
            Value::Pattern(p) => Ok(p),
            _ => Err(invalid()),
        }
    }
    unsafe fn error(&self, scope: &Scope<'_>, kind: &[u8], msg: &[u8]) -> Result<Operand, Outcome> {
        unsafe {
            let m = scope.keep(object(self.send(
                self.named(b"String")?,
                b"fromBytes:",
                &[Operand::bytes(msg)],
            )?)?)?;
            self.send(
                self.named(b"Error")?,
                b"kind:message:",
                &[
                    Operand::object(self.id.intern_name(kind)?),
                    Operand::object(m),
                ],
            )
        }
    }
    unsafe fn cst(&self, scope: &Scope<'_>, m: &Matcher, index: usize) -> Result<Oop, Outcome> {
        unsafe {
            let n = &m.nodes[index];
            let mut children = Vec::new();
            for &i in &n.children {
                children.push(self.cst(scope, m, i)?);
            }
            self.allocate(scope, Value::Cst(n.rule, n.start, n.end, children))
        }
    }
    unsafe fn parse(
        &self,
        scope: &Scope<'_>,
        grammar: Oop,
        source: Oop,
        root: Oop,
    ) -> Result<Operand, Outcome> {
        unsafe {
            let source = self.bytes(source)?;
            let root = scope.keep(self.symbol(root)?)?;
            let Value::Grammar(rules, _) = self.value(grammar)? else {
                return Err(invalid());
            };
            let mut m = Matcher {
                source,
                rules: index_rules(rules),
                patterns: KeyMap::default(),
                nodes: Vec::new(),
                memo: KeyMap::default(),
                furthest: 0,
                undefined_rule: ptr::null_mut(),
                left_recursive_rule: ptr::null_mut(),
            };
            let result = if let Some(p) = named_rule(&m.rules, root) {
                match_pattern(&mut m, p, 0, &|p| self.pattern(p).ok())
            } else {
                m.undefined_rule = root;
                None
            };
            if let Some(Value::Grammar(_, furthest)) =
                self.values.borrow_mut().get_mut(&(grammar as usize))
            {
                *furthest = m.furthest;
            }
            // Matcher snapshots can outlive reentrant grammar replacement during CST allocation.
            for p in m.patterns.values() {
                for v in &p.children {
                    scope.keep(*v)?;
                }
                if !p.rule_name.is_null() {
                    scope.keep(p.rule_name)?;
                }
            }
            for n in &m.nodes {
                scope.keep(n.rule)?;
            }
            if let Some(r) = result {
                let i = m.nodes.len();
                m.nodes.push(TempNode {
                    rule: root,
                    start: 0,
                    end: r.end,
                    children: r.children,
                });
                return Ok(Operand::object(self.cst(scope, &m, i)?));
            }
            for (name, kind, prefix) in [
                (
                    m.undefined_rule,
                    b"undefinedRule".as_slice(),
                    b"undefined rule: ".as_slice(),
                ),
                (
                    m.left_recursive_rule,
                    b"leftRecursion",
                    b"left-recursive rule: ",
                ),
            ] {
                if !name.is_null() {
                    let b = self
                        .send(
                            self.id.receiver,
                            b"symbolBytesOf:",
                            &[Operand::object(name)],
                        )?
                        .as_bytes()
                        .ok_or_else(invalid)?;
                    let mut message = prefix.to_vec();
                    message.extend_from_slice(
                        borrowed_bytes(b.pointer, b.length).ok_or_else(invalid)?,
                    );
                    return self.error(scope, kind, &message);
                }
            }
            let (line, col) = syntax_position(&m.source, m.furthest);
            self.error(
                scope,
                b"syntaxError",
                format!("syntax error at line {line}, col {col}").as_bytes(),
            )
        }
    }
}
#[derive(Clone, Copy)]
enum Op {
    New(Kind),
    Grammar,
    Literal,
    RuleRef,
    Seq(Kind),
    Unary(Kind, bool),
    AnyOf,
    Range,
    Add,
    FromTo,
    Rule,
    Parse,
    Fail,
    Cst(u8),
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
            return Err(invalid());
        }
        let a = borrowed_slice(a, n).ok_or_else(invalid)?;
        let scope = Scope {
            s,
            tokens: RefCell::new(Vec::new()),
        };
        scope.keep(r)?;
        let a = a
            .iter()
            .map(|v| object(*v))
            .collect::<Result<Vec<_>, _>>()?;
        for &v in &a {
            scope.keep(v)?;
        }
        s.send(s.gc, b"prepareAllocation", &[])?;
        let expected = match e.op {
            Op::New(_) | Op::Grammar | Op::Fail => 0,
            Op::Unary(_, true) | Op::Cst(0..=4) | Op::Cst(6) => 0,
            Op::Range | Op::FromTo | Op::Rule | Op::Parse => 2,
            _ => 1,
        };
        if a.len() != expected {
            return Err(invalid());
        }
        let mut p = match e.op {
            Op::New(k) | Op::Seq(k) | Op::Unary(k, _) => PatternState::new(k),
            _ => PatternState::new(Kind::Literal),
        };
        match e.op {
            Op::New(_) => {}
            Op::Grammar => {
                return Ok(Operand::object(
                    s.allocate(&scope, Value::Grammar(Vec::new(), 0))?,
                ));
            }
            Op::Literal => p.literal = s.bytes(a[0])?,
            Op::RuleRef => {
                p.kind = Kind::RuleRef;
                p.rule_name = s.symbol(a[0])?
            }
            Op::Seq(_) => {
                let count = scope.keep(object(s.send(a[0], b"size", &[])?)?)?;
                let count = s.index(Operand::object(count))?;
                for i in 0..count {
                    let index = scope.keep(object(s.number(i)?)?)?;
                    let v =
                        scope.keep(object(s.send(a[0], b"at:", &[Operand::object(index)])?)?)?;
                    s.pattern(v)?;
                    p.children.push(v);
                }
            }
            Op::Unary(_, own) => {
                let v = if own { r } else { a[0] };
                s.pattern(v)?;
                p.children.push(v)
            }
            Op::AnyOf => {
                p.kind = Kind::Charset;
                for b in s.bytes(a[0])? {
                    p.ranges.push(Range { lo: b, hi: b });
                }
            }
            Op::Range | Op::FromTo => {
                let (lo, hi) = if matches!(e.op, Op::Range) {
                    (s.byte(a[0])?, s.byte(a[1])?)
                } else {
                    (
                        s.index(Operand::object(a[0]))? as u8,
                        s.index(Operand::object(a[1]))? as u8,
                    )
                };
                if matches!(e.op, Op::FromTo) {
                    p = s.pattern(r)?;
                } else {
                    p.kind = Kind::Charset;
                }
                p.ranges.push(Range {
                    lo: lo as u8,
                    hi: hi as u8,
                });
                if matches!(e.op, Op::FromTo) {
                    s.values.borrow_mut().insert(r as usize, Value::Pattern(p));
                    return Ok(Operand::object(r));
                }
            }
            Op::Add => {
                s.pattern(a[0])?;
                p = s.pattern(r)?;
                p.children.push(a[0]);
                s.values.borrow_mut().insert(r as usize, Value::Pattern(p));
                return Ok(Operand::object(r));
            }
            Op::Rule => {
                let name = s.symbol(a[0])?;
                s.pattern(a[1])?;
                let mut values = s.values.borrow_mut();
                let Some(Value::Grammar(rules, _)) = values.get_mut(&(r as usize)) else {
                    return Err(invalid());
                };
                if let Some(row) = rules.iter_mut().find(|row| row.0 == name) {
                    row.1 = a[1]
                } else {
                    rules.push((name, a[1]));
                }
                return Ok(Operand::object(r));
            }
            Op::Parse => return s.parse(&scope, r, a[0], a[1]),
            Op::Fail => {
                let Value::Grammar(_, v) = s.value(r)? else {
                    return Err(invalid());
                };
                return s.number(v);
            }
            Op::Cst(op) => {
                let Value::Cst(rule, start, end, children) = s.value(r)? else {
                    return Err(invalid());
                };
                return match op {
                    0 => Ok(Operand::object(rule)),
                    1 => {
                        let b = s
                            .send(s.id.receiver, b"symbolBytesOf:", &[Operand::object(rule)])?
                            .as_bytes()
                            .ok_or_else(invalid)?;
                        let bytes = borrowed_bytes(b.pointer, b.length)
                            .ok_or_else(invalid)?
                            .to_vec();
                        s.send(
                            s.named(b"String")?,
                            b"fromBytes:",
                            &[Operand::bytes(&bytes)],
                        )
                    }
                    2 => s.number(start),
                    3 => s.number(end),
                    4 => s.number(children.len()),
                    5 => Ok(Operand::object(
                        children
                            .get(s.index(Operand::object(a[0]))?)
                            .copied()
                            .unwrap_or(s.named(b"nil")?),
                    )),
                    6 => {
                        let mut v =
                            scope.keep(object(s.send(s.named(b"Vector")?, b"new", &[])?)?)?;
                        for child in children {
                            v = scope.keep(object(s.send(
                                v,
                                b"add:",
                                &[Operand::object(child)],
                            )?)?)?;
                        }
                        Ok(Operand::object(v))
                    }
                    _ => Err(invalid()),
                };
            }
        }
        Ok(Operand::object(s.allocate(&scope, Value::Pattern(p))?))
    })())
}
unsafe extern "C" fn references(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = &*c.cast::<State>();
        let Some([o, v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let entry = v.as_entry().ok_or_else(invalid)?;
        let refs = s
            .values
            .borrow()
            .get(&(object(*o)? as usize))
            .map(Value::refs)
            .unwrap_or_default();
        for r in refs {
            entry.method.ok_or_else(invalid)?(entry.context, s.receiver, &Operand::object(r), 1)
                .into_result()?;
        }
        Ok(Operand::unit())
    })())
}
unsafe extern "C" fn roots(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = &*c.cast::<State>();
        let Some([v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let v = v.as_entry().ok_or_else(invalid)?;
        for &o in &s.permanent {
            v.method.ok_or_else(invalid)?(v.context, s.receiver, &Operand::object(o), 1)
                .into_result()?;
        }
        Ok(Operand::unit())
    })())
}
unsafe extern "C" fn retire(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = &*c.cast::<State>();
        let Some([v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let b = v.as_bytes().ok_or_else(invalid)?;
        let bytes = borrowed_bytes(b.pointer, b.length).ok_or_else(invalid)?;
        if bytes.len() % size_of::<usize>() != 0 {
            return Err(invalid());
        }
        let mut values = s.values.borrow_mut();
        for b in bytes.chunks_exact(size_of::<usize>()) {
            values.remove(&usize::from_ne_bytes(b.try_into().unwrap()));
        }
        Ok(Operand::unit())
    })())
}
unsafe fn construct(id: Protocol, ns: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let named =
            |n: &[u8]| object(id.send_named(ns, b"at:", &[Operand::object(id.intern_name(n)?)])?);
        let existing = id.send_named(ns, b"at:", &[Operand::object(id.intern_name(b"Peg")?)])?;
        if existing.kind() == native::OBJECT {
            return Ok(existing);
        }
        let root = named(b"Roots")?;
        let model = named(b"ObjectModel")?;
        let gc = named(b"GC")?;
        let mut tokens = Vec::new();
        let mut make = || -> Result<Oop, Outcome> {
            let v = object(id.send_named(named(b"ObjectVt")?, b"delegated", &[])?)?;
            tokens.push(
                id.send_named(root, b"retain:", &[Operand::object(v)])?
                    .as_word()
                    .ok_or_else(invalid)?,
            );
            Ok(v)
        };
        let vt = make()?;
        let behaviors = [make()?, make()?, make()?];
        let receiver = object(id.send_named(
            model,
            b"allocate:bytes:",
            &[Operand::object(vt), Operand::word(0)],
        )?)?;
        tokens.push(
            id.send_named(root, b"retain:", &[Operand::object(receiver)])?
                .as_word()
                .ok_or_else(invalid)?,
        );
        let s = Box::into_raw(Box::new(State {
            id,
            ns,
            roots: root,
            gc,
            model,
            receiver,
            behaviors,
            values: RefCell::new(HashMap::new()),
            permanent: vec![
                ns,
                root,
                gc,
                model,
                receiver,
                vt,
                behaviors[0],
                behaviors[1],
                behaviors[2],
            ],
            thread: std::thread::current().id(),
        }));
        for (n, f) in [
            (b"rootsDo:".as_slice(), roots as Method),
            (b"referencesOf:do:", references),
            (b"retire:", retire),
        ] {
            id.send_named(
                vt,
                b"addMethod",
                &[
                    Operand::object(id.intern_name(n)?),
                    Operand::entry(NativeEntry::new(f, s.cast())),
                ],
            )?;
        }
        let methods: &[(usize, &[u8], Op)] = &[
            (3, b"literal:", Op::Literal),
            (3, b"any", Op::New(Kind::Any)),
            (3, b"anyByte", Op::New(Kind::Any)),
            (3, b"eof", Op::New(Kind::Eof)),
            (3, b"charset", Op::New(Kind::Charset)),
            (3, b"seq", Op::New(Kind::Seq)),
            (3, b"choice", Op::New(Kind::Choice)),
            (3, b"grammar", Op::Grammar),
            (3, b"newGrammar", Op::Grammar),
            (3, b"ruleRef:", Op::RuleRef),
            (3, b"seqOf:", Op::Seq(Kind::Seq)),
            (3, b"choiceOf:", Op::Seq(Kind::Choice)),
            (3, b"anyOf:", Op::AnyOf),
            (3, b"range:to:", Op::Range),
            (3, b"star:", Op::Unary(Kind::Star, false)),
            (3, b"plus:", Op::Unary(Kind::Plus, false)),
            (3, b"opt:", Op::Unary(Kind::Opt, false)),
            (3, b"and:", Op::Unary(Kind::And, false)),
            (3, b"not:", Op::Unary(Kind::Not, false)),
            (0, b"star", Op::Unary(Kind::Star, true)),
            (0, b"plus", Op::Unary(Kind::Plus, true)),
            (0, b"opt", Op::Unary(Kind::Opt, true)),
            (0, b"negate", Op::Unary(Kind::Not, true)),
            (0, b"lookahead", Op::Unary(Kind::And, true)),
            (0, b"add:", Op::Add),
            (0, b"from:to:", Op::FromTo),
            (1, b"rule:is:", Op::Rule),
            (1, b"parse:from:", Op::Parse),
            (1, b"furthestFail", Op::Fail),
            (2, b"rule", Op::Cst(0)),
            (2, b"ruleName", Op::Cst(1)),
            (2, b"start", Op::Cst(2)),
            (2, b"end", Op::Cst(3)),
            (2, b"childCount", Op::Cst(4)),
            (2, b"childAt:", Op::Cst(5)),
            (2, b"children", Op::Cst(6)),
        ];
        for &(target, n, op) in methods {
            let c = Box::into_raw(Box::new(Entry { state: s, op }));
            id.send_named(
                if target == 3 { vt } else { behaviors[target] },
                b"addMethod",
                &[
                    Operand::object(id.intern_name(n)?),
                    Operand::entry(NativeEntry::new(operation, c.cast())),
                ],
            )?;
        }
        id.send_named(root, b"register:", &[Operand::object(receiver)])?;
        id.send_named(gc, b"registerRetirement:", &[Operand::object(receiver)])?;
        for b in behaviors {
            id.send_named(
                gc,
                b"trace:with:",
                &[Operand::object(b), Operand::object(receiver)],
            )?;
        }
        for (n, v) in [
            (b"Peg".as_slice(), receiver),
            (b"PegFacadeVt", vt),
            (b"PegPatternVt", behaviors[0]),
            (b"PegGrammarVt", behaviors[1]),
            (b"PegCstVt", behaviors[2]),
        ] {
            id.send_named(
                ns,
                b"at:put:",
                &[Operand::object(id.intern_name(n)?), Operand::object(v)],
            )?;
        }
        for t in tokens {
            id.send_named(root, b"release:", &[Operand::word(t)])?;
        }
        Ok(Operand::object(receiver))
    }
}
unsafe extern "C" fn initialize(id: *const Protocol, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        if !native::valid_protocol(id) {
            return Err(invalid());
        }
        let Some([ns]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        construct(*id, object(*ns)?)
    })())
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module {
    version: native::VERSION,
    byte_size: size_of::<native::Module>() as u32,
    initialize: Some(initialize),
};
#[cfg(test)]
mod matching_tests;
