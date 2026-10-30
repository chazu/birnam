//! PEG owns pattern, grammar and CST data. Only Oops cross Id.
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
    rc::Rc,
    sync::atomic::{AtomicPtr, Ordering},
};
include!("matcher.rs");
fn abort(message: &str) -> ! {
    panic!("invalid private PEG graph: {message}")
}
static INSTANCE: AtomicPtr<State> = AtomicPtr::new(ptr::null_mut());
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
unsafe fn state<'a>(c: Oop) -> Result<&'a State, Oop> {
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
#[derive(Clone)]
enum Value {
    Pattern(PatternState),
    Grammar(Vec<(Oop, Oop)>, usize),
    Cst(Oop, usize, usize, std::rc::Rc<[Oop]>),
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
    pub(crate) byte_reader: abi::byte_transfer::Reader,
    id: Protocol,
    ns: Oop,
    gc: Oop,
    model: Oop,
    prototype: Oop,
    behaviors: [Oop; 3],
    values: RefCell<HashMap<usize, Value>>,
    permanent: Vec<Oop>,
    names: HashMap<&'static [u8], Oop>,
    scoped: RefCell<Vec<Oop>>,
    ready: Cell<bool>,
}
struct Scope<'a> {
    s: &'a State,
    start: usize,
}
impl Scope<'_> {
    fn keep(&self, v: Oop) -> Oop {
        self.s.scoped.borrow_mut().push(v);
        v
    }
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        self.s.scoped.borrow_mut().truncate(self.start);
    }
}
impl State {
    fn scope(&self) -> Scope<'_> {
        Scope {
            s: self,
            start: self.scoped.borrow().len(),
        }
    }
    unsafe fn send(&self, r: Oop, n: &[u8], a: &[Oop]) -> Result<Oop, Oop> {
        unsafe { self.id.send(r, *self.names.get(n).ok_or_else(invalid)?, a) }
    }
    unsafe fn named(&self, n: &[u8]) -> Result<Oop, Oop> {
        object(unsafe { self.send(self.ns, b"at:", &[*self.names.get(n).ok_or_else(invalid)?]) }?)
    }
    unsafe fn read_bytes(&self, v: Oop) -> Result<Vec<u8>, Oop> {
        unsafe {
            self.byte_reader.read(v)
        }
    }
    unsafe fn bytes(&self, v: Oop) -> Result<Vec<u8>, Oop> {
        unsafe { self.read_bytes(self.send(v, b"bytes", &[])?) }
    }
    unsafe fn number(&self, v: usize) -> Result<Oop, Oop> {
        unsafe {
            self.send(
                self.named(b"Integer")?,
                b"fromWord:",
                &[abi::size(v).ok_or_else(invalid)?],
            )
        }
    }
    unsafe fn word_bits(&self, v: Oop) -> Result<u64, Oop> {
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
            Ok(lo as u64 | ((hi as u64) << 32))
        }
    }
    unsafe fn index(&self, v: Oop) -> Result<usize, Oop> {
        unsafe {
            let word = self.send(self.named(b"Integer")?, b"wordOf:", &[v])?;
            let n = self.word_bits(word)?;
            if n > isize::MAX as u64 {
                Err(invalid())
            } else {
                Ok(n as usize)
            }
        }
    }
    unsafe fn byte(&self, v: Oop) -> Result<u8, Oop> {
        unsafe {
            let integer = self.send(self.named(b"Integer")?, b"wordOf:", &[v])?;
            if !integer.is_null() {
                return Ok(self.word_bits(integer)? as u8);
            }
            Ok(self.bytes(v)?.first().copied().unwrap_or(0))
        }
    }
    unsafe fn symbol(&self, v: Oop) -> Result<Oop, Oop> {
        unsafe {
            if self
                .send(self.id.root, b"isSymbol:", &[v])
                .ok()
                .and_then(abi::as_size)
                == Some(1)
            {
                return Ok(v);
            }
            let bytes = self.bytes(v)?;
            let values: Vec<_> = bytes
                .iter()
                .map(|&b| abi::size(b as usize).unwrap())
                .collect();
            let name = self.send(self.id.root, b"nameWithBytes", &values)?;
            self.id.send(name, self.id.intern, &[])
        }
    }

    unsafe fn allocate(&self, scope: &Scope<'_>, v: Value) -> Result<Oop, Oop> {
        unsafe {
            for r in v.refs() {
                scope.keep(r);
            }
            let behavior = self.behaviors[match v {
                Value::Pattern(_) => 0,
                Value::Grammar(..) => 1,
                Value::Cst(..) => 2,
            }];
            let o = scope.keep(self.send(
                self.model,
                b"allocate:bytes:",
                &[behavior, abi::size(0).unwrap()],
            )?);
            if self.send(self.id.root, b"allocationSizeOf:", &[o])? != abi::size(0).unwrap()
                || self.send(self.id.root, b"behaviorOf:", &[o])? != behavior
                || self.values.borrow().contains_key(&o.addr())
            {
                return Err(invalid());
            }
            self.values.borrow_mut().insert(o.addr(), v);
            Ok(o)
        }
    }
    fn value(&self, v: Oop) -> Result<Value, Oop> {
        self.values
            .borrow()
            .get(&v.addr())
            .cloned()
            .ok_or_else(invalid)
    }
    fn pattern(&self, v: Oop) -> Result<PatternState, Oop> {
        match self.value(v)? {
            Value::Pattern(p) => Ok(p),
            _ => Err(invalid()),
        }
    }
    unsafe fn error(&self, scope: &Scope<'_>, kind: &[u8], msg: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            let source = abi::byte_transfer::buffer(msg, self.named(b"Bytes")?,
                |r,n,a| self.send(r,n,a), |v| Ok(scope.keep(v)))?;
            let m = scope.keep(object(self.send(
                self.named(b"String")?,
                b"fromBytes:",
                &[source],
            )?)?);
            self.send(
                self.named(b"Error")?,
                b"kind:message:",
                &[self.names[kind], m],
            )
        }
    }
    unsafe fn cst(&self, scope: &Scope<'_>, m: &Matcher, index: usize) -> Result<Oop, Oop> {
        unsafe {
            let n = &m.nodes[index];
            let mut children = Vec::new();
            for &i in &n.children {
                children.push(self.cst(scope, m, i)?);
            }
            self.allocate(scope, Value::Cst(n.rule, n.start, n.end, children.into()))
        }
    }
    unsafe fn parse(
        &self,
        scope: &Scope<'_>,
        grammar: Oop,
        source: Vec<u8>,
        root: Oop,
    ) -> Result<Oop, Oop> {
        unsafe {
            let root = scope.keep(self.symbol(root)?);
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
                    scope.keep(*v);
                }
                if !p.rule_name.is_null() {
                    scope.keep(p.rule_name);
                }
            }
            for n in &m.nodes {
                scope.keep(n.rule);
            }
            if let Some(r) = result {
                let i = m.nodes.len();
                m.nodes.push(TempNode {
                    rule: root,
                    start: 0,
                    end: r.end,
                    children: r.children,
                });
                return Ok(self.cst(scope, &m, i)?);
            }
            for (name, kind, _prefix) in [
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
                    let mut message = _prefix.to_vec();
                    message.extend_from_slice(&self.read_bytes(name)?);
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
unsafe fn execute(s: &State, r: Oop, a: &[Oop], op: Op) -> Result<Oop, Oop> {
    unsafe {
        let scope = s.scope();
        if a.iter().any(|v| v.is_null()) {
            return Err(invalid());
        }
        s.send(s.gc, b"prepareAllocation", &[])?;
        let expected = match op {
            Op::New(_) | Op::Grammar | Op::Fail => 0,
            Op::Unary(_, true) | Op::Cst(0..=4) | Op::Cst(6) => 0,
            Op::Range | Op::FromTo | Op::Rule | Op::Parse => 2,
            _ => 1,
        };
        if a.len() != expected {
            return Err(invalid());
        }
        let mut p = match op {
            Op::New(k) | Op::Seq(k) | Op::Unary(k, _) => PatternState::new(k),
            _ => PatternState::new(Kind::Literal),
        };
        match op {
            Op::New(_) => {}
            Op::Grammar => {
                return Ok(s.allocate(&scope, Value::Grammar(Vec::new(), 0))?);
            }
            Op::Literal => p.literal = s.bytes(a[0])?,
            Op::RuleRef => {
                p.kind = Kind::RuleRef;
                p.rule_name = s.symbol(a[0])?
            }
            Op::Seq(_) => {
                let count = scope.keep(object(s.send(a[0], b"size", &[])?)?);
                let count = s.index(count)?;
                for i in 0..count {
                    let index = scope.keep(object(s.number(i)?)?);
                    let v = scope.keep(object(s.send(a[0], b"at:", &[index])?)?);
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
                p.ranges = s
                    .bytes(a[0])?
                    .into_iter()
                    .map(|b| Range { lo: b, hi: b })
                    .collect();
            }
            Op::Range | Op::FromTo => {
                let (lo, hi) = if matches!(op, Op::Range) {
                    (s.byte(a[0])?, s.byte(a[1])?)
                } else {
                    (s.index(a[0])? as u8, s.index(a[1])? as u8)
                };
                if matches!(op, Op::FromTo) {
                    p = s.pattern(r)?;
                } else {
                    p.kind = Kind::Charset;
                }
                p.ranges.push(Range {
                    lo: lo as u8,
                    hi: hi as u8,
                });
                if matches!(op, Op::FromTo) {
                    s.values.borrow_mut().insert(r as usize, Value::Pattern(p));
                    return Ok(r);
                }
            }
            Op::Add => {
                s.pattern(a[0])?;
                p = s.pattern(r)?;
                p.children.push(a[0]);
                s.values.borrow_mut().insert(r as usize, Value::Pattern(p));
                return Ok(r);
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
                return Ok(r);
            }
            Op::Parse => return s.parse(&scope, r, s.bytes(a[0])?, a[1]),
            Op::Fail => {
                let v = match s.values.borrow().get(&r.addr()) {
                    Some(Value::Grammar(_, position)) => *position,
                    _ => return Err(invalid()),
                };
                return s.number(v);
            }
            Op::Cst(op) => {
                let Value::Cst(rule, start, end, children) = s.value(r)? else {
                    return Err(invalid());
                };
                return match op {
                    0 => Ok(rule),
                    1 => s.send(s.named(b"String")?, b"fromBytes:", &[rule]),
                    2 => s.number(start),
                    3 => s.number(end),
                    4 => s.number(children.len()),
                    5 => Ok(children
                        .get(s.index(a[0])?)
                        .copied()
                        .unwrap_or(s.named(b"nil")?)),
                    6 => {
                        let mut v =
                            scope.keep(object(s.send(s.named(b"Vector")?, b"new", &[])?)?);
                        for &child in children.iter() {
                            v = scope.keep(object(s.send(v, b"add:", &[child])?)?);
                        }
                        Ok(v)
                    }
                    _ => Err(invalid()),
                };
            }
        }
        Ok(s.allocate(&scope, Value::Pattern(p))?)
    }
}

unsafe extern "C" fn references(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    done((|| unsafe {
        let s = state(c)?;
        let [o, v] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let refs = s
            .values
            .borrow()
            .get(&o.addr())
            .map(Value::refs)
            .unwrap_or_default();
        for r in refs {
            s.send(*v, b"visit:", &[r])?;
        }
        Ok(ptr::null_mut())
    })())
}
unsafe extern "C" fn roots(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    done((|| unsafe {
        let s = state(c)?;
        let [v] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let mut values = s.permanent.clone();
        values.extend(s.names.values().copied());
        values.extend(s.scoped.borrow().iter().copied());
        for r in values {
            s.send(*v, b"visit:", &[r])?;
        }
        Ok(ptr::null_mut())
    })())
}
unsafe extern "C" fn retire(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    done((|| unsafe {
        let s = state(c)?;
        let a = borrowed_slice(a, n).ok_or_else(invalid)?;
        let mut values = s.values.borrow_mut();
        for v in a {
            values.remove(&v.addr());
        }
        Ok(ptr::null_mut())
    })())
}
macro_rules! entry {
    ($name:ident,$op:expr) => {
        unsafe extern "C" fn $name(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
            done((|| unsafe {
                execute(state(c)?, r, borrowed_slice(a, n).ok_or_else(invalid)?, $op)
            })())
        }
    };
}
entry!(operation_0, Op::Literal);
entry!(operation_1, Op::New(Kind::Any));
entry!(operation_2, Op::New(Kind::Any));
entry!(operation_3, Op::New(Kind::Eof));
entry!(operation_4, Op::New(Kind::Charset));
entry!(operation_5, Op::New(Kind::Seq));
entry!(operation_6, Op::New(Kind::Choice));
entry!(operation_7, Op::Grammar);
entry!(operation_8, Op::Grammar);
entry!(operation_9, Op::RuleRef);
entry!(operation_10, Op::Seq(Kind::Seq));
entry!(operation_11, Op::Seq(Kind::Choice));
entry!(operation_12, Op::AnyOf);
entry!(operation_13, Op::Range);
entry!(operation_14, Op::Unary(Kind::Star, false));
entry!(operation_15, Op::Unary(Kind::Plus, false));
entry!(operation_16, Op::Unary(Kind::Opt, false));
entry!(operation_17, Op::Unary(Kind::And, false));
entry!(operation_18, Op::Unary(Kind::Not, false));
entry!(operation_19, Op::Unary(Kind::Star, true));
entry!(operation_20, Op::Unary(Kind::Plus, true));
entry!(operation_21, Op::Unary(Kind::Opt, true));
entry!(operation_22, Op::Unary(Kind::Not, true));
entry!(operation_23, Op::Unary(Kind::And, true));
entry!(operation_24, Op::Add);
entry!(operation_25, Op::FromTo);
entry!(operation_26, Op::Rule);
entry!(operation_27, Op::Parse);
entry!(operation_28, Op::Fail);
entry!(operation_29, Op::Cst(0));
entry!(operation_30, Op::Cst(1));
entry!(operation_31, Op::Cst(2));
entry!(operation_32, Op::Cst(3));
entry!(operation_33, Op::Cst(4));
entry!(operation_34, Op::Cst(5));
entry!(operation_35, Op::Cst(6));
macro_rules! literals{($($n:literal),*$(,)?)=>{const NAMES:&[&[u8]]=&[$($n),*];static LITERALS:&[Literal]=&[$(Literal::new($n)),*];};}
literals![
    b"Error",
    b"GC",
    b"Integer",
    b"ObjectModel",
    b"ObjectVt",
    b"Peg",
    b"PegCstVt",
    b"PegFacadeVt",
    b"PegGrammarVt",
    b"PegPatternVt",
    b"Roots",
    b"String",
    b"Vector",
    b"add:",
    b"addMethod",
    b"allocate:bytes:",
    b"allocationSizeOf:",
    b"and:",
    b"any",
    b"anyByte",
    b"anyOf:",
    b"sizeWord",
    b"atWord:",
    b"at:",
    b"at:put:",
    b"behaviorOf:",
    b"bytes",
    b"charset",
    b"childAt:",
    b"childCount",
    b"children",
    b"choice",
    b"choiceOf:",
    b"delegated",
    b"end",
    b"eof",
    b"from:to:",
    b"fromBytes:",
    b"fromWord:",
    b"furthestFail",
    b"grammar",
    b"high32",
    b"isSymbol:",
    b"kind:message:",
    b"leftRecursion",
    b"literal:",
    b"lookahead",
    b"low32",
    b"methodAt:",
    b"nameWithBytes",
    b"negate",
    b"new",
    b"newGrammar",
    b"nil",
    b"not:",
    b"opt",
    b"opt:",
    b"parse:from:",
    b"plus",
    b"plus:",
    b"prepareAllocation",
    b"range:to:",
    b"referencesOf:do:",
    b"register:",
    b"registerRetirement:",
    b"release:",
    b"retain:",
    b"retire:",
    b"rootsDo:",
    b"rule",
    b"rule:is:",
    b"ruleName",
    b"ruleRef:",
    b"seq",
    b"seqOf:",
    b"sequence",
    b"size",
    b"star",
    b"star:",
    b"start",
    b"syntaxError",
    b"trace:with:",
    b"undefinedRule",
    b"visit:",
    b"withContext:",
    b"wordOf:",
    b"copyByteValuesFrom:count:to:",
    b"appendByteValues",
    b"objectBehavior",
    b"allocate",
    b"releaseTokensProtecting",
    b"Bytes",
    b"fromByteValues",
    b"receiveByteValues",
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
        name: index(b"literal:"),
        entry: Some(operation_0),
    },
    MethodSpec {
        name: index(b"any"),
        entry: Some(operation_1),
    },
    MethodSpec {
        name: index(b"anyByte"),
        entry: Some(operation_2),
    },
    MethodSpec {
        name: index(b"eof"),
        entry: Some(operation_3),
    },
    MethodSpec {
        name: index(b"charset"),
        entry: Some(operation_4),
    },
    MethodSpec {
        name: index(b"seq"),
        entry: Some(operation_5),
    },
    MethodSpec {
        name: index(b"choice"),
        entry: Some(operation_6),
    },
    MethodSpec {
        name: index(b"grammar"),
        entry: Some(operation_7),
    },
    MethodSpec {
        name: index(b"newGrammar"),
        entry: Some(operation_8),
    },
    MethodSpec {
        name: index(b"ruleRef:"),
        entry: Some(operation_9),
    },
    MethodSpec {
        name: index(b"seqOf:"),
        entry: Some(operation_10),
    },
    MethodSpec {
        name: index(b"choiceOf:"),
        entry: Some(operation_11),
    },
    MethodSpec {
        name: index(b"anyOf:"),
        entry: Some(operation_12),
    },
    MethodSpec {
        name: index(b"range:to:"),
        entry: Some(operation_13),
    },
    MethodSpec {
        name: index(b"star:"),
        entry: Some(operation_14),
    },
    MethodSpec {
        name: index(b"plus:"),
        entry: Some(operation_15),
    },
    MethodSpec {
        name: index(b"opt:"),
        entry: Some(operation_16),
    },
    MethodSpec {
        name: index(b"and:"),
        entry: Some(operation_17),
    },
    MethodSpec {
        name: index(b"not:"),
        entry: Some(operation_18),
    },
    MethodSpec {
        name: index(b"star"),
        entry: Some(operation_19),
    },
    MethodSpec {
        name: index(b"plus"),
        entry: Some(operation_20),
    },
    MethodSpec {
        name: index(b"opt"),
        entry: Some(operation_21),
    },
    MethodSpec {
        name: index(b"negate"),
        entry: Some(operation_22),
    },
    MethodSpec {
        name: index(b"lookahead"),
        entry: Some(operation_23),
    },
    MethodSpec {
        name: index(b"add:"),
        entry: Some(operation_24),
    },
    MethodSpec {
        name: index(b"from:to:"),
        entry: Some(operation_25),
    },
    MethodSpec {
        name: index(b"rule:is:"),
        entry: Some(operation_26),
    },
    MethodSpec {
        name: index(b"parse:from:"),
        entry: Some(operation_27),
    },
    MethodSpec {
        name: index(b"furthestFail"),
        entry: Some(operation_28),
    },
    MethodSpec {
        name: index(b"rule"),
        entry: Some(operation_29),
    },
    MethodSpec {
        name: index(b"ruleName"),
        entry: Some(operation_30),
    },
    MethodSpec {
        name: index(b"start"),
        entry: Some(operation_31),
    },
    MethodSpec {
        name: index(b"end"),
        entry: Some(operation_32),
    },
    MethodSpec {
        name: index(b"childCount"),
        entry: Some(operation_33),
    },
    MethodSpec {
        name: index(b"childAt:"),
        entry: Some(operation_34),
    },
    MethodSpec {
        name: index(b"children"),
        entry: Some(operation_35),
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
        name: index(b"retire:"),
        entry: Some(retire),
    },

    MethodSpec { name: NAMES.len()-1, entry: Some(abi::byte_transfer::receive_entry) },
];
const BINDINGS: &[(usize, &[u8])] = &[
    (3, b"literal:"),
    (3, b"any"),
    (3, b"anyByte"),
    (3, b"eof"),
    (3, b"charset"),
    (3, b"seq"),
    (3, b"choice"),
    (3, b"grammar"),
    (3, b"newGrammar"),
    (3, b"ruleRef:"),
    (3, b"seqOf:"),
    (3, b"choiceOf:"),
    (3, b"anyOf:"),
    (3, b"range:to:"),
    (3, b"star:"),
    (3, b"plus:"),
    (3, b"opt:"),
    (3, b"and:"),
    (3, b"not:"),
    (0, b"star"),
    (0, b"plus"),
    (0, b"opt"),
    (0, b"negate"),
    (0, b"lookahead"),
    (0, b"add:"),
    (0, b"from:to:"),
    (1, b"rule:is:"),
    (1, b"parse:from:"),
    (1, b"furthestFail"),
    (2, b"rule"),
    (2, b"ruleName"),
    (2, b"start"),
    (2, b"end"),
    (2, b"childCount"),
    (2, b"childAt:"),
    (2, b"children"),
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
    let id = unsafe { *p };
    let Some([ns]) = (unsafe { borrowed_slice(a, count) }) else {
        return id.invalid;
    };
    let Some(symbols) = (unsafe { borrowed_slice(l, n) }) else {
        return id.invalid;
    };
    if n != NAMES.len() {
        return id.invalid;
    }
    let old = INSTANCE.load(Ordering::Acquire);
    if !old.is_null() {
        let s = unsafe { &*old };
        return if s.ready.get() && s.ns == *ns {
            done(unsafe { s.send(*ns, b"at:", &[s.names[b"Peg".as_slice()]]) })
        } else {
            id.invalid
        };
    }
    done((|| unsafe {
        let names: HashMap<_, _> = NAMES.iter().copied().zip(symbols.iter().copied()).collect();
        let send = |r, n: &[u8], a: &[Oop]| id.send(r, names[n], a);
        let named = |n: &[u8]| object(send(*ns, b"at:", &[names[n]])?);
        let existing = send(*ns, b"at:", &[names[b"Peg".as_slice()]])?;
        if !existing.is_null() {
            return Ok(existing);
        }
        let roots = named(b"Roots")?;
        let model = named(b"ObjectModel")?;
        let gc = named(b"GC")?;
        let parent = named(b"ObjectVt")?;
        let mut tokens = Vec::new();
        let result = (|| {
            let mut keep = |v| -> Result<Oop, Oop> {
                tokens.push(send(roots, b"retain:", &[v])?);
                Ok(v)
            };
            let vt = keep(send(parent, b"delegated", &[])?)?;
            let behaviors = [
                keep(send(parent, b"delegated", &[])?)?,
                keep(send(parent, b"delegated", &[])?)?,
                keep(send(parent, b"delegated", &[])?)?,
            ];
            let receiver = keep(send(
                model,
                b"allocate:bytes:",
                &[vt, abi::size(0).unwrap()],
            )?)?;
            let s = Box::into_raw(Box::new(State {
                byte_reader: abi::byte_transfer::Reader::new(id, image, roots, |n| names[n])?,
                id,
                ns: *ns,
                gc,
                model,
                prototype: receiver,
                behaviors,
                values: RefCell::new(HashMap::new()),
                permanent: vec![
                    *ns,
                    gc,
                    model,
                    receiver,
                    vt,
                    behaviors[0],
                    behaviors[1],
                    behaviors[2],
                ],
                names: names.clone(),
                scoped: RefCell::new(Vec::new()),
                ready: Cell::new(false),
            }));
            INSTANCE.store(s, Ordering::Release);
            let s = &*s;
            for spec in &METHODS[..METHODS.len()-1] {
                let name = NAMES[spec.name];
                let method = send(image, b"methodAt:", &[symbols[spec.name]])?;
                let method = send(method, b"withContext:", &[receiver])?;
                let target = BINDINGS
                    .iter()
                    .find(|(_, n)| *n == name)
                    .map_or(
                        vt,
                        |(target, _)| if *target == 3 { vt } else { behaviors[*target] },
                    );
                send(target, b"addMethod", &[symbols[spec.name], method])?;
            }
            send(roots, b"register:", &[receiver])?;
            send(gc, b"registerRetirement:", &[receiver])?;
            for b in behaviors {
                send(gc, b"trace:with:", &[b, receiver])?;
            }
            for (name, v) in [
                (b"Peg".as_slice(), receiver),
                (b"PegFacadeVt", vt),
                (b"PegPatternVt", behaviors[0]),
                (b"PegGrammarVt", behaviors[1]),
                (b"PegCstVt", behaviors[2]),
            ] {
                send(*ns, b"at:put:", &[names[name], v])?;
            }
            s.ready.set(true);
            Ok(receiver)
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
#[cfg(test)]
mod matching_tests;
