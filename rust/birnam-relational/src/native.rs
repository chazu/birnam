//! Relational's language adapter. All dependency operations use current Id sends.
use crate::evaluator::*;
use birnam_abi::{
    self as abi, Oop, borrowed_slice,
    id_protocol::{Literal, Method, MethodSpec, Module, Protocol},
};
use core::ptr;
use core::{
    cell::{Cell, RefCell},
    mem::size_of,
};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicPtr, Ordering},
};

static INSTANCE: AtomicPtr<State> = AtomicPtr::new(ptr::null_mut());
static API: AtomicPtr<Protocol> = AtomicPtr::new(ptr::null_mut());
fn invalid() -> Oop {
    let p = API.load(Ordering::Acquire);
    if p.is_null() {
        std::process::abort();
    }
    unsafe { abi::object_reference((*p).invalid) }
}
fn object(v: Oop) -> core::result::Result<Oop, Oop> {
    if v.is_null() { Err(invalid()) } else { Ok(v) }
}
#[derive(Debug)]
enum Error {
    Shape,
    Send(Oop),
}
impl From<Oop> for Error {
    fn from(v: Oop) -> Self {
        Self::Send(v)
    }
}
type Result<T> = core::result::Result<T, Error>;
struct Collector {
    scope: usize,
    facts: Vec<Triple>,
    pairs: Vec<(Oop, Oop)>,
    rules: bool,
    failed: bool,
}
struct State {
    byte_reader: abi::byte_transfer::Reader,
    id: Protocol,
    namespace: Oop,
    roots: Oop,
    gc: Oop,
    receiver: Cell<Oop>,
    logic_vt: Oop,
    factory_vt: Oop,
    entity_vt: Oop,
    collector_vt: Oop,
    names: HashMap<&'static [u8], Oop>,
    values: HashMap<&'static [u8], Oop>,
    logicvars: RefCell<HashMap<Oop, Oop>>,
    collectors: RefCell<HashMap<Oop, Collector>>,
    active: RefCell<Vec<Vec<Oop>>>,
    ready: Cell<bool>,
    thread: std::thread::ThreadId,
}
struct Scope<'a> {
    s: &'a State,
    index: usize,
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        self.s
            .collectors
            .borrow_mut()
            .retain(|_, c| c.scope != self.index);
        let mut active = self.s.active.borrow_mut();
        assert_eq!(active.len(), self.index + 1);
        active.pop();
    }
}
impl<'a> Scope<'a> {
    fn new(s: &'a State, values: Vec<Oop>) -> Self {
        let mut a = s.active.borrow_mut();
        let index = a.len();
        a.push(values);
        Self { s, index }
    }
    fn keep(&self, o: Oop) -> Oop {
        self.s.active.borrow_mut()[self.index].push(o);
        o
    }
    fn value(&self, n: &'static [u8]) -> Oop {
        self.s.values[n]
    }
    fn name(&self, n: &'static [u8]) -> Oop {
        self.s.names[n]
    }
    unsafe fn send(&self, r: Oop, n: &'static [u8], a: &[Oop]) -> Result<Oop> {
        unsafe { Ok(self.keep(self.s.id.send(r, self.name(n), a)?)) }
    }
    unsafe fn call(&self, r: Oop, n: &'static [u8], a: &[Oop]) -> Result<Oop> {
        unsafe {
            let v = self.send(r, n, a)?;
            Ok(if v.is_null() { self.value(b"nil") } else { v })
        }
    }
    unsafe fn import(&self, name: &'static [u8]) -> Result<Oop> {
        unsafe {
            Ok(object(self.send(
                self.s.namespace,
                b"at:",
                &[self.name(name)],
            )?)?)
        }
    }
    unsafe fn word_bits(&self, bits: Oop) -> Result<u64> {
        unsafe {
            if let Some(n) = abi::as_size(bits) {
                return Ok(n as u64);
            }
            object(bits)?;
            let low = abi::as_size(self.send(bits, b"low32", &[])?)
                .filter(|n| *n <= u32::MAX as usize)
                .ok_or_else(invalid)?;
            let high = abi::as_size(self.send(bits, b"high32", &[])?)
                .filter(|n| *n <= u32::MAX as usize)
                .ok_or_else(invalid)?;
            Ok((low as u64) | ((high as u64) << 32))
        }
    }
    unsafe fn word(&self, bits: u64) -> Result<Oop> {
        unsafe {
            if let Some(o) = abi::size(bits as usize) {
                return Ok(o);
            }
            self.send(
                self.import(b"Word")?,
                b"fromUnsignedLow:high:",
                &[
                    abi::size(bits as u32 as usize).unwrap(),
                    abi::size((bits >> 32) as usize).unwrap(),
                ],
            )
        }
    }
    unsafe fn number(&self, v: Oop) -> Result<Option<i64>> {
        unsafe {
            let bits = self.send(self.value(b"Integer"), b"wordOf:", &[v])?;
            if bits.is_null() {
                Ok(None)
            } else {
                Ok(Some(self.word_bits(bits)? as i64))
            }
        }
    }
    unsafe fn integer(&self, v: i64) -> Result<Oop> {
        unsafe {
            self.send(
                self.value(b"Integer"),
                b"fromWord:",
                &[self.word(v as u64)?],
            )
        }
    }
    unsafe fn behavior(&self, v: Oop) -> Result<Oop> {
        unsafe { self.call(self.s.id.root, b"behaviorOf:", &[v]) }
    }
    unsafe fn bytes(&self, value: Oop) -> Result<Vec<u8>> {
        unsafe {
            Ok(self.s.byte_reader.read(value)?)
        }
    }
    unsafe fn text(&self, bytes: &[u8]) -> Result<Oop> {
        unsafe {
            let values: Vec<_> = bytes
                .iter()
                .map(|b| abi::size(*b as usize).unwrap())
                .collect();
            let data = self.send(self.s.id.root, b"sequence", &values)?;
            self.send(self.value(b"String"), b"fromBytes:", &[data])
        }
    }
    unsafe fn classify(&self, v: Oop) -> Result<Value> {
        unsafe {
            if let Some(n) = self.number(v)? {
                return Ok(Value::Integer(n));
            }
            let bits = self.send(self.value(b"Float"), b"bitsOf:", &[v])?;
            if !bits.is_null() {
                return Ok(Value::Float(f64::from_bits(self.word_bits(bits)?)));
            }
            if self.behavior(v)? == self.value(b"StringVt") {
                let bytes = self.send(v, b"bytes", &[])?;
                return Ok(Value::String(self.bytes(bytes)?));
            }
            Ok(Value::Identity(v.addr() as u64))
        }
    }
    unsafe fn materialize(&self, v: Value) -> Result<Oop> {
        unsafe {
            match v {
                Value::Identity(v) => Ok(self.keep(v as usize as Oop)),
                Value::Integer(v) => self.integer(v),
                Value::Float(v) => self.send(
                    self.value(b"Float"),
                    b"fromBits:",
                    &[self.word(v.to_bits())?],
                ),
                Value::String(v) => self.text(&v),
            }
        }
    }
    unsafe fn allocate(&self, vt: Oop, bytes: usize) -> Result<Oop> {
        unsafe {
            let o =
                object(self.send(vt, b"allocate", &[abi::size(bytes).ok_or_else(invalid)?])?)?;
            if self.behavior(o)? != vt
                || abi::as_size(self.send(self.s.id.root, b"allocationSizeOf:", &[o])?)
                    != Some(bytes)
            {
                return Err(invalid().into());
            }
            Ok(o)
        }
    }
    unsafe fn vector(&self, values: impl IntoIterator<Item = Value>) -> Result<Oop> {
        let mut v = unsafe { self.call(self.import(b"Vector")?, b"new", &[]) }?;
        for value in values {
            let o = unsafe { self.materialize(value) }?;
            v = unsafe { self.call(v, b"add:", &[o]) }?;
        }
        Ok(v)
    }
    unsafe fn elements(&self, v: Oop) -> Result<Vec<Oop>> {
        let size = unsafe { self.call(v, b"size", &[]) }?;
        let size = unsafe { self.number(size) }?.ok_or(Error::Shape)?;
        if !(0..=512).contains(&size) {
            return Err(Error::Shape);
        }
        let mut out = Vec::with_capacity(size as usize);
        for i in 0..size {
            let i = unsafe { self.integer(i) }?;
            out.push(unsafe { self.call(v, b"at:", &[i]) }?);
        }
        Ok(out)
    }
    unsafe fn snapshot(
        &self,
        store: Oop,
        mode: usize,
        lower: i64,
        upper: i64,
        rules: bool,
    ) -> Result<(Vec<Triple>, Vec<(Oop, Oop)>)> {
        let callback = unsafe { self.allocate(self.s.collector_vt, 0) }?;
        self.s.collectors.borrow_mut().insert(
            callback,
            Collector {
                scope: self.index,
                facts: vec![],
                pairs: vec![],
                rules,
                failed: false,
            },
        );
        let lo = unsafe { self.integer(lower) }?;
        let hi = unsafe { self.integer(upper) }?;
        let (name, args) = match mode {
            0 => (b"liveTriplesDo:".as_slice(), vec![callback]),
            1 => (b"liveTriplesAsOf:do:".as_slice(), vec![hi, callback]),
            2 => (b"liveTriplesSince:do:".as_slice(), vec![lo, callback]),
            3 => (
                b"liveTriplesBetween:and:do:".as_slice(),
                vec![lo, hi, callback],
            ),
            _ => return Err(Error::Shape),
        };
        let sent = unsafe { self.call(store, name, &args) };
        let c = self
            .s
            .collectors
            .borrow_mut()
            .remove(&callback)
            .ok_or_else(invalid)?;
        sent?;
        if c.failed {
            return Err(Error::Shape);
        }
        Ok((c.facts, c.pairs))
    }
}
unsafe fn state(c: Oop) -> core::result::Result<&'static State, Oop> {
    unsafe {
        let p = INSTANCE.load(Ordering::Acquire);
        if p.is_null() {
            return Err(invalid());
        }
        let s = &*p;
        if s.namespace != c || s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        Ok(s)
    }
}
fn answer(s: &State, r: Result<Oop>) -> Oop {
    match r {
        Ok(o) => o,
        Err(Error::Shape) => s.values[b"nil".as_slice()],
        Err(Error::Send(e)) => abi::fault_reference(e),
    }
}
unsafe extern "C" fn collect(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    let s = match unsafe { state(c) } {
        Ok(s) => s,
        Err(e) => return abi::fault_reference(e),
    };
    let result = (|| unsafe {
        let args = borrowed_slice(a, n).ok_or_else(invalid)?;
        let [e, attr, v] = args else {
            if let Some(c) = s.collectors.borrow_mut().get_mut(&r) {
                c.failed = true;
            }
            return Err(invalid().into());
        };
        let values = [object(*e)?, object(*attr)?, object(*v)?];
        let scope = Scope::new(s, values.to_vec());
        let (index, rules) = {
            let cs = s.collectors.borrow();
            let c = cs.get(&r).ok_or_else(invalid)?;
            if c.failed || c.facts.len() >= 1_000_000 {
                drop(cs);
                s.collectors.borrow_mut().get_mut(&r).unwrap().failed = true;
                return Err(Error::Shape);
            }
            (c.scope, c.rules)
        };
        // Snapshot identities and rule vectors outlive destructive Store callbacks.
        s.active.borrow_mut()[index].extend(values);
        let triple = Triple {
            entity: scope.classify(values[0])?,
            attribute: scope.classify(values[1])?,
            value: scope.classify(values[2])?,
        };
        let mut cs = s.collectors.borrow_mut();
        let c = cs.get_mut(&r).ok_or_else(invalid)?;
        c.facts.push(triple);
        if rules && values[1] == scope.name(b"__rule-name__") {
            c.pairs.push((values[0], values[2]));
        }
        Ok(scope.value(b"nil"))
    })();
    if result.is_err() {
        if let Some(c) = s.collectors.borrow_mut().get_mut(&r) {
            c.failed = true;
        }
    }
    answer(s, result)
}

unsafe fn parse_store_term(
    q: &Scope<'_>,
    term: Oop,
    logicvar_vt: Oop,
    _name_selector: Oop,
    variables: &mut Vec<Oop>,
) -> Result<Term> {
    if term.is_null() {
        return Err(Error::Shape);
    }
    if unsafe { q.behavior(term)? } == logicvar_vt {
        let name = unsafe { q.call(term, b"name", &[])? };
        if name.is_null() {
            return Err(Error::Shape);
        }
        let variable = match variables.iter().position(|existing| *existing == name) {
            Some(index) => index as u32,
            None => {
                variables.push(name);
                (variables.len() - 1) as u32
            }
        };
        return Ok(Term::Variable(variable));
    }
    // SAFETY: non-logic terms are copied by value under the frozen classifier.
    unsafe { q.classify(term) }.map(Term::Constant)
}

unsafe fn parse_store_rule_body(
    q: &Scope<'_>,
    body: Oop,
    logicvar_vt: Oop,
    name_selector: Oop,
    variables: &mut Vec<Oop>,
) -> Result<Vec<RuleClause>> {
    let clauses = unsafe { q.elements(body)? };
    let mut parsed = Vec::with_capacity(clauses.len());
    for clause in clauses {
        parsed.push(unsafe {
            parse_store_rule_clause(q, clause, logicvar_vt, name_selector, variables)?
        });
    }
    Ok(parsed)
}

unsafe fn parse_store_rule_clause(
    q: &Scope<'_>,
    clause: Oop,
    logicvar_vt: Oop,
    name_selector: Oop,
    variables: &mut Vec<Oop>,
) -> Result<RuleClause> {
    let terms = unsafe { q.elements(clause)? };
    let rule_symbol = q.name(b"rule");
    let not_symbol = q.name(b"not");
    let or_symbol = q.name(b"or");
    if terms.len() == 3
        && terms[0] != rule_symbol
        && terms[0] != not_symbol
        && terms[0] != or_symbol
    {
        let left = unsafe { parse_store_term(q, terms[0], logicvar_vt, name_selector, variables)? };
        let middle =
            unsafe { parse_store_term(q, terms[1], logicvar_vt, name_selector, variables)? };
        let right =
            unsafe { parse_store_term(q, terms[2], logicvar_vt, name_selector, variables)? };
        if let Ok(operation) = parse_rule_compare_op(q, terms[1]) {
            return Ok(RuleClause::Predicate {
                left,
                operation,
                right,
            });
        }
        return Ok(RuleClause::Triple(Clause([left, middle, right])));
    }
    if terms.len() >= 2 && terms[0] == rule_symbol {
        let name = unsafe { q.classify(terms[1])? };
        let mut args = Vec::with_capacity(terms.len() - 2);
        for term in &terms[2..] {
            args.push(unsafe {
                parse_store_term(q, *term, logicvar_vt, name_selector, variables)?
            });
        }
        return Ok(RuleClause::Call { name, args });
    }
    if terms.first().is_some_and(|term| *term == not_symbol) {
        let mut inner = Vec::with_capacity(terms.len().saturating_sub(1));
        for term in &terms[1..] {
            inner.push(unsafe {
                parse_store_rule_clause(q, *term, logicvar_vt, name_selector, variables)?
            });
        }
        return Ok(RuleClause::Negated(inner));
    }
    if terms.first().is_some_and(|term| *term == or_symbol) {
        let mut branches = Vec::with_capacity(terms.len().saturating_sub(1));
        for term in &terms[1..] {
            branches.push(vec![unsafe {
                parse_store_rule_clause(q, *term, logicvar_vt, name_selector, variables)?
            }]);
        }
        return Ok(RuleClause::Disjunction(branches));
    }
    Err(Error::Shape)
}

unsafe fn parse_store_rule_definitions(
    q: &Scope<'_>,
    pairs: &[(Oop, Oop)],
) -> Result<Vec<RuleDefinition>> {
    let logicvar_vt = unsafe { q.import(b"LogicVarVt")? };
    if logicvar_vt.is_null() {
        return Err(Error::Shape);
    }
    let name_selector = q.name(b"name");
    let mut definitions = Vec::with_capacity(pairs.len());
    for (pair, name_oop) in pairs {
        let pair_values = unsafe { q.elements(*pair)? };
        if pair_values.len() != 2 {
            return Err(Error::Shape);
        }
        let name = unsafe { q.classify(*name_oop)? };
        let params = unsafe { q.elements(pair_values[0])? };
        let mut variables = Vec::with_capacity(params.len());
        let mut parameter_ids = Vec::with_capacity(params.len());
        for parameter in params {
            let Term::Variable(variable) = (unsafe {
                parse_store_term(q, parameter, logicvar_vt, name_selector, &mut variables)?
            }) else {
                return Err(Error::Shape);
            };
            parameter_ids.push(variable);
        }
        let body = unsafe {
            parse_store_rule_body(
                q,
                pair_values[1],
                logicvar_vt,
                name_selector,
                &mut variables,
            )?
        };
        definitions.push(RuleDefinition {
            name,
            params: parameter_ids,
            body,
        });
    }
    Ok(definitions)
}

fn parse_rule_compare_op(q: &Scope<'_>, name: Oop) -> Result<CompareOp> {
    for (names, op) in [
        (&[b"lt".as_slice(), b"<"][..], 0),
        (&[b"le".as_slice(), b"<="][..], 1),
        (&[b"gt".as_slice(), b">"][..], 2),
        (&[b"ge".as_slice(), b">="][..], 3),
        (&[b"eq".as_slice(), b"="][..], 4),
        (&[b"ne".as_slice(), b"~=", b"!="][..], 5),
    ] {
        if names.iter().any(|n| q.name(n) == name) {
            return Ok(CompareOp::from_u8(op).unwrap());
        }
    }
    Err(Error::Shape)
}
fn aggregate_kind(q: &Scope<'_>, o: Oop) -> Result<AggregateOp> {
    for (name, op) in [
        (b"count".as_slice(), 0),
        (b"sum", 1),
        (b"min", 2),
        (b"max", 3),
        (b"avg", 4),
    ] {
        if o == q.name(name) {
            return Ok(AggregateOp::from_u8(op).unwrap());
        }
    }
    Err(Error::Shape)
}
unsafe fn find(q: &Scope<'_>, store: Oop, args: &[Oop], mode: usize) -> Result<Oop> {
    let count = match mode {
        0 => 2,
        1 | 2 => 3,
        3 => 4,
        _ => return Err(Error::Shape),
    };
    if args.len() != count {
        return Err(Error::Shape);
    }
    let logicvar_vt = unsafe { q.import(b"LogicVarVt")? };
    let mut vars = Vec::new();
    let Term::Variable(result) =
        unsafe { parse_store_term(q, args[0], logicvar_vt, q.name(b"name"), &mut vars) }?
    else {
        return Err(Error::Shape);
    };
    let body =
        unsafe { parse_store_rule_body(q, args[1], logicvar_vt, q.name(b"name"), &mut vars) }?;
    let tx = |v| {
        unsafe { q.number(v) }?
            .filter(|v| *v >= 0)
            .ok_or(Error::Shape)
    };
    let (lower, upper) = match mode {
        0 => (0, 0),
        1 => (0, tx(args[2])?),
        2 => (tx(args[2])?, 0),
        3 => (tx(args[2])?, tx(args[3])?),
        _ => unreachable!(),
    };
    if mode == 3 && lower > upper {
        return Err(Error::Shape);
    }
    let (facts, pairs) = unsafe { q.snapshot(store, mode, lower, upper, true) }?;
    let definitions = unsafe { parse_store_rule_definitions(q, &pairs) }?;
    unsafe { q.vector(project_rule_body(&facts, &definitions, &body, result)) }
}
unsafe fn aggregate(q: &Scope<'_>, store: Oop, args: &[Oop], kind: usize) -> Result<Oop> {
    let values = unsafe { find(q, store, args, 0) }?;
    if values == q.value(b"nil") {
        return Ok(values);
    }
    let size = unsafe { q.call(values, b"size", &[]) }?;
    let n = unsafe { q.number(size) }?
        .filter(|n| *n >= 0)
        .ok_or(Error::Shape)?;
    if kind == 0 {
        return Ok(size);
    }
    if n == 0 {
        return if kind == 1 {
            unsafe { q.integer(0) }
        } else {
            Ok(q.value(b"nil"))
        };
    }
    let mut copied = Vec::with_capacity(n as usize);
    for i in 0..n {
        let i = unsafe { q.integer(i) }?;
        let o = unsafe { q.call(values, b"at:", &[i]) }?;
        copied.push(unsafe { q.classify(o) }?);
    }
    let op = match kind {
        1 => AggregateOp::Sum,
        2 => AggregateOp::Min,
        3 => AggregateOp::Max,
        4 => AggregateOp::Average,
        _ => return Err(Error::Shape),
    };
    match aggregate_values(&copied, op) {
        Ok(Some(v)) => unsafe { q.materialize(v) },
        _ => Ok(q.value(b"nil")),
    }
}
unsafe fn group(q: &Scope<'_>, store: Oop, args: &[Oop]) -> Result<Oop> {
    let [key, op, value, body] = args else {
        return Err(Error::Shape);
    };
    let op = aggregate_kind(q, *op)?;
    let logicvar_vt = unsafe { q.import(b"LogicVarVt")? };
    let mut vars = vec![];
    let Term::Variable(key) =
        unsafe { parse_store_term(q, *key, logicvar_vt, q.name(b"name"), &mut vars) }?
    else {
        return Err(Error::Shape);
    };
    let Term::Variable(value) =
        unsafe { parse_store_term(q, *value, logicvar_vt, q.name(b"name"), &mut vars) }?
    else {
        return Err(Error::Shape);
    };
    let body = unsafe { parse_store_rule_body(q, *body, logicvar_vt, q.name(b"name"), &mut vars) }?;
    let (facts, pairs) = unsafe { q.snapshot(store, 0, 0, 0, true) }?;
    let definitions = unsafe { parse_store_rule_definitions(q, &pairs) }?;
    let groups =
        group_rule_body(&facts, &definitions, &body, key, value, op).map_err(|_| Error::Shape)?;
    let mut out = unsafe { q.call(q.import(b"Map")?, b"new", &[]) }?;
    for group in groups {
        let key = unsafe { q.materialize(group.group) }?;
        let value = match group.aggregate {
            Some(v) => unsafe { q.materialize(v) }?,
            None => q.value(b"nil"),
        };
        out = unsafe { q.call(out, b"at:put:", &[key, value]) }?;
    }
    Ok(out)
}
unsafe fn pull_specs(q: &Scope<'_>, store: Oop, spec: Oop, depth: usize) -> Result<Vec<PullSpec>> {
    if depth >= PULL_MAX_DEPTH {
        return Err(Error::Shape);
    }
    let elements = unsafe { q.elements(spec) }?;
    let vector_vt = unsafe { q.import(b"VectorVt")? };
    let mut specs = Vec::with_capacity(elements.len());
    for element in elements {
        let (attribute, kind, children) = if unsafe { q.behavior(element) }? == vector_vt {
            let pair = unsafe { q.elements(element) }?;
            let [attribute, child] = pair.as_slice() else {
                return Err(Error::Shape);
            };
            (
                unsafe { q.classify(*attribute) }?,
                PullKind::Reference,
                unsafe { pull_specs(q, store, *child, depth + 1) }?,
            )
        } else {
            (unsafe { q.classify(element) }?, PullKind::Leaf, vec![])
        };
        let arg = unsafe { q.materialize(attribute.clone()) }?;
        let cardinality = unsafe { q.call(store, b"cardinality:", &[arg]) }?;
        specs.push(PullSpec {
            attribute,
            kind,
            many: cardinality == q.name(b"many"),
            children,
        });
    }
    Ok(specs)
}
unsafe fn pull_entity(
    q: &Scope<'_>,
    facts: &[Triple],
    entity: &Value,
    specs: &[PullSpec],
) -> Result<Oop> {
    let mut out = unsafe { q.call(q.import(b"Map")?, b"new", &[]) }?;
    for spec in specs {
        let values = pull_values(facts, entity, &spec.attribute);
        if values.is_empty() {
            continue;
        }
        let result = match spec.kind {
            PullKind::Leaf => {
                if spec.many {
                    unsafe { q.vector(values) }?
                } else {
                    unsafe { q.materialize(values[0].clone()) }?
                }
            }
            PullKind::Reference => {
                if spec.many {
                    let mut v = unsafe { q.call(q.import(b"Vector")?, b"new", &[]) }?;
                    for value in values {
                        let child = unsafe { pull_entity(q, facts, &value, &spec.children) }?;
                        v = unsafe { q.call(v, b"add:", &[child]) }?;
                    }
                    v
                } else {
                    unsafe { pull_entity(q, facts, &values[0], &spec.children) }?
                }
            }
        };
        let attribute = unsafe { q.materialize(spec.attribute.clone()) }?;
        out = unsafe { q.call(out, b"at:put:", &[attribute, result]) }?;
    }
    Ok(out)
}
unsafe fn invoke(c: Oop, r: Oop, a: *const Oop, n: usize, op: usize) -> Oop {
    let s = match unsafe { state(c) } {
        Ok(s) => s,
        Err(e) => return abi::fault_reference(e),
    };
    answer(
        s,
        (|| unsafe {
            let a = borrowed_slice(a, n).ok_or_else(invalid)?;
            let args = a
                .iter()
                .copied()
                .map(object)
                .collect::<core::result::Result<Vec<_>, _>>()?;
            let q = Scope::new(s, std::iter::once(r).chain(args.iter().copied()).collect());
            match op {
                0 => {
                    // LogicVar named: retains canonical identity for each name.
                    let [name] = args.as_slice() else {
                        return Err(Error::Shape);
                    };
                    let existing = s.logicvars.borrow().get(name).copied();
                    if let Some(o) = existing {
                        return Ok(o);
                    }
                    let o = q.allocate(s.logic_vt, size_of::<Oop>())?;
                    // Own payload only; GC reports this edge through referencesOf:do:.
                    o.cast::<Oop>().write(*name);
                    let mut vars = s.logicvars.borrow_mut();
                    Ok(*vars.entry(*name).or_insert(o))
                }
                1 | 2 => {
                    if !args.is_empty() {
                        return Err(Error::Shape);
                    }
                    if q.behavior(r)? != s.logic_vt
                        || abi::as_size(q.send(s.id.root, b"allocationSizeOf:", &[r])?)
                            != Some(size_of::<Oop>())
                    {
                        return Err(Error::Shape);
                    }
                    if op == 1 {
                        return Ok(r.cast::<Oop>().read());
                    }
                    let name = q.call(r, b"name", &[])?;
                    if abi::as_size(q.send(s.id.root, b"isSymbol:", &[name])?) != Some(1) {
                        return Err(invalid().into());
                    }
                    let mut text = vec![b'?'];
                    text.extend_from_slice(&q.bytes(name)?);
                    q.text(&text)
                }
                3 => q.allocate(s.entity_vt, 0), // entity historically ignores operands.
                4..=7 => find(&q, r, &args, op - 4),
                8..=12 => aggregate(&q, r, &args, op - 8),
                13 => group(&q, r, &args),
                14 => {
                    let [entity, spec] = args.as_slice() else {
                        return Err(Error::Shape);
                    };
                    let entity = q.classify(*entity)?;
                    let specs = pull_specs(&q, r, *spec, 0)?;
                    let (facts, _) = q.snapshot(r, 0, 0, 0, false)?;
                    pull_entity(&q, &facts, &entity, &specs)
                }
                15 => {
                    let [name, params, body] = args.as_slice() else {
                        return Err(Error::Shape);
                    };
                    let mut pair = q.call(q.import(b"Vector")?, b"new", &[])?;
                    pair = q.call(pair, b"add:", &[*params])?;
                    pair = q.call(pair, b"add:", &[*body])?;
                    q.call(
                        r,
                        b"assert:attr:value:",
                        &[pair, q.name(b"__rule-name__"), *name],
                    )?;
                    Ok(r)
                }
                16 => {
                    if args.is_empty() {
                        Ok(r)
                    } else {
                        Err(Error::Shape)
                    }
                }
                17 => {
                    if !args.is_empty()
                        || q.behavior(r)? != s.logic_vt
                        || abi::as_size(q.send(s.id.root, b"allocationSizeOf:", &[r])?)
                            != Some(size_of::<Oop>())
                    {
                        return Err(Error::Shape);
                    }
                    let name = q.keep(r.cast::<Oop>().read());
                    let copy = q.allocate(s.logic_vt, size_of::<Oop>())?;
                    copy.cast::<Oop>().write(name);
                    Ok(copy)
                }
                _ => Err(invalid().into()),
            }
        })(),
    )
}
macro_rules! entry {
    ($name:ident,$op:expr) => {
        unsafe extern "C" fn $name(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
            unsafe { invoke(c, r, a, n, $op) }
        }
    };
}
entry!(named, 0);
entry!(name, 1);
entry!(print, 2);
entry!(entity, 3);
entry!(find_all, 4);
entry!(as_of, 5);
entry!(since, 6);
entry!(between, 7);
entry!(count, 8);
entry!(sum, 9);
entry!(min, 10);
entry!(max, 11);
entry!(avg, 12);
entry!(clone_logic, 17);
entry!(group_by, 13);
entry!(pull_method, 14);
entry!(define_rule, 15);
entry!(rebuild, 16);
const STORE_METHODS: &[(&[u8], Method)] = &[
    (b"entity", entity),
    (b"findAll:where:", find_all),
    (b"findAll:where:asOf:", as_of),
    (b"findAll:where:since:", since),
    (b"findAll:where:between:and:", between),
    (b"count:where:", count),
    (b"sum:where:", sum),
    (b"min:where:", min),
    (b"max:where:", max),
    (b"avg:where:", avg),
    (b"groupBy:aggregate:of:where:", group_by),
    (b"pull:spec:", pull_method),
    (b"defineRule:params:body:", define_rule),
    (b"rebuildRules", rebuild),
];
unsafe extern "C" fn roots(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    let result = (|| unsafe {
        let s = state(c)?;
        let Some([v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let v = object(*v)?;
        let mut edges = vec![
            s.namespace,
            s.roots,
            s.gc,
            s.receiver.get(),
            s.logic_vt,
            s.factory_vt,
            s.entity_vt,
            s.collector_vt,
        ];
        edges.extend(s.names.values().copied());
        edges.extend(s.values.values().copied());
        edges.extend(s.logicvars.borrow().iter().flat_map(|(a, b)| [*a, *b]));
        edges.extend(s.active.borrow().iter().flatten().copied());
        for edge in edges {
            s.id.send(v, s.names[b"visit:".as_slice()], &[edge])?;
        }
        Ok(ptr::null_mut())
    })();
    result.unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn references(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    let result = (|| unsafe {
        let s = state(c)?;
        let Some([o, v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let o = object(*o)?;
        let v = object(*v)?;
        let owned =
            s.id.send(s.id.root, s.names[b"behaviorOf:".as_slice()], &[o])? == s.logic_vt
                && abi::as_size(s.id.send(
                    s.id.root,
                    s.names[b"allocationSizeOf:".as_slice()],
                    &[o],
                )?) == Some(size_of::<Oop>());
        if owned {
            s.id.send(v, s.names[b"visit:".as_slice()], &[o.cast::<Oop>().read()])?;
        }
        Ok(ptr::null_mut())
    })();
    result.unwrap_or_else(abi::fault_reference)
}
const NAMES: &[&[u8]] = &[
    b"!=",
    b"<",
    b"<=",
    b"=",
    b">",
    b">=",
    b"EntityVt",
    b"Float",
    b"GC",
    b"Integer",
    b"LogicVar",
    b"LogicVarFactoryVt",
    b"LogicVarVt",
    b"Map",
    b"ObjectVt",
    b"RelCollectorVt",
    b"Roots",
    b"Store",
    b"StoreVt",
    b"String",
    b"StringVt",
    b"Vector",
    b"VectorVt",
    b"__rule-name__",
    b"add:",
    b"addMethod",
    b"allocate",
    b"allocationSizeOf:",
    b"assert:attr:value:",
    b"at:",
    b"at:put:",
    b"avg",
    b"avg:where:",
    b"behaviorOf:",
    b"bitsOf:",
    b"bytes",
    b"cardinality:",
    b"clone",
    b"count",
    b"count:where:",
    b"db",
    b"defineRule:params:body:",
    b"delegated",
    b"entity",
    b"eq",
    b"findAll:where:",
    b"findAll:where:asOf:",
    b"findAll:where:between:and:",
    b"findAll:where:since:",
    b"fromBits:",
    b"fromBytes:",
    b"fromWord:",
    b"ge",
    b"groupBy:aggregate:of:where:",
    b"gt",
    b"le",
    b"liveTriplesAsOf:do:",
    b"liveTriplesBetween:and:do:",
    b"liveTriplesDo:",
    b"liveTriplesSince:do:",
    b"lt",
    b"many",
    b"max",
    b"max:where:",
    b"min",
    b"min:where:",
    b"name",
    b"named:",
    b"ne",
    b"new",
    b"nil",
    b"not",
    b"or",
    b"ownerOf:put:",
    b"printString",
    b"pull:spec:",
    b"rebuildRules",
    b"referencesOf:do:",
    b"register:",
    b"release:",
    b"retain:",
    b"rootsDo:",
    b"rule",
    b"size",
    b"sum",
    b"sum:where:",
    b"isSymbol:",
    b"trace:with:",
    b"value:value:value:",
    b"visit:",
    b"wordOf:",
    b"~=",
    b"Word",
    b"low32",
    b"high32",
    b"fromUnsignedLow:high:",
    b"sequence",
    b"sizeWord",
    b"atWord:",
    b"methodAt:",
    b"withContext:",
    b"copyByteValuesFrom:count:to:",
    b"appendByteValues",
    b"objectBehavior",
    b"releaseTokensProtecting",
    b"receiveByteValues",
];

const LOGIC_METHODS: &[(&[u8], Method)] = &[
    (b"name", name),
    (b"printString", print),
    (b"clone", clone_logic),
    (b"new", clone_logic),
];
const FACTORY_METHODS: &[(&[u8], Method)] = &[
    (b"named:", named),
    (b"rootsDo:", roots),
    (b"referencesOf:do:", references),
];
const COLLECTOR_METHODS: &[(&[u8], Method)] = &[(b"value:value:value:", collect)];
const fn index(name: &[u8]) -> usize {
    let mut i = 0;
    while i < NAMES.len() {
        let n = NAMES[i];
        if n.len() == name.len() {
            let mut j = 0;
            while j < n.len() && n[j] == name[j] {
                j += 1;
            }
            if j == n.len() {
                return i;
            }
        }
        i += 1;
    }
    panic!("undeclared Relational name")
}
const LITERALS: [Literal; NAMES.len()] = {
    let mut a = [const { Literal::new(b"") }; NAMES.len()];
    let mut i = 0;
    while i < NAMES.len() {
        a[i] = Literal::new(NAMES[i]);
        i += 1;
    }
    a
};
const METHOD_COUNT: usize =
    STORE_METHODS.len() + LOGIC_METHODS.len() + FACTORY_METHODS.len() + COLLECTOR_METHODS.len() + 1;
const METHODS: [MethodSpec; METHOD_COUNT] = {
    let mut a = [const {
        MethodSpec {
            name: 0,
            entry: None,
        }
    }; METHOD_COUNT];
    let tables = [
        STORE_METHODS,
        LOGIC_METHODS,
        FACTORY_METHODS,
        COLLECTOR_METHODS,
    ];
    let mut i = 0;
    let mut t = 0;
    while t < tables.len() {
        let mut j = 0;
        while j < tables[t].len() {
            let (n, f) = tables[t][j];
            a[i] = MethodSpec {
                name: index(n),
                entry: Some(f),
            };
            i += 1;
            j += 1;
        }
        t += 1;
    }
    a[i] = MethodSpec { name: NAMES.len()-1, entry: Some(abi::byte_transfer::receive_entry) };
    a
};
unsafe extern "C" fn initialize(
    api: *const Protocol,
    image: Oop,
    l: *const Oop,
    count: usize,
    a: *const Oop,
    n: usize,
) -> Oop {
    if api.is_null() {
        std::process::abort();
    }
    API.store(api.cast_mut(), Ordering::Release);
    (|| unsafe {
        let id = *api;
        let [ns] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        object(*ns)?;
        let old = INSTANCE.load(Ordering::Acquire);
        if !old.is_null() {
            let s = state(*ns)?;
            return if s.ready.get() && s.id.root == id.root {
                Ok(s.values[b"Store".as_slice()])
            } else {
                Err(invalid())
            };
        }
        let literals = borrowed_slice(l, count)
            .filter(|v| v.len() == NAMES.len())
            .ok_or_else(invalid)?;
        let names: HashMap<_, _> = NAMES
            .iter()
            .copied()
            .zip(literals.iter().copied())
            .collect();
        let send = |r, name: &[u8], args: &[Oop]| id.send(r, names[name], args);
        let lookup = |name: &[u8]| object(send(*ns, b"at:", &[names[name]])?);
        let roots = lookup(b"Roots")?;
        let store = lookup(b"Store")?;
        let mut tokens = Vec::new();
        let mut keep = |o: Oop| -> core::result::Result<Oop, Oop> {
            tokens.push(send(roots, b"retain:", &[o])?);
            Ok(o)
        };
        let result = (|| {
            for &name in names.values() {
                keep(name)?;
            }
            let mut values = HashMap::new();
            for name in [
                b"Integer".as_slice(),
                b"Float",
                b"String",
                b"StringVt",
                b"nil",
                b"Store",
                b"StoreVt",
            ] {
                values.insert(name, lookup(name)?);
            }
            let parent = lookup(b"ObjectVt")?;
            let mut vt = || keep(send(parent, b"delegated", &[])?);
            let logic_vt = vt()?;
            let factory_vt = vt()?;
            let entity_vt = vt()?;
            let collector_vt = vt()?;
            let state = Box::into_raw(Box::new(State {
                byte_reader: abi::byte_transfer::Reader::new(id, image, roots, |n| names[n])?,
                id,
                namespace: *ns,
                roots,
                gc: lookup(b"GC")?,
                receiver: Cell::new(ptr::null_mut()),
                logic_vt,
                factory_vt,
                entity_vt,
                collector_vt,
                names: names.clone(),
                values,
                logicvars: RefCell::new(HashMap::new()),
                collectors: RefCell::new(HashMap::new()),
                active: RefCell::new(Vec::new()),
                ready: Cell::new(false),
                thread: std::thread::current().id(),
            }));
            INSTANCE.store(state, Ordering::Release);
            let s = &*state;
            for (vt, methods) in [
                (logic_vt, LOGIC_METHODS),
                (factory_vt, FACTORY_METHODS),
                (collector_vt, COLLECTOR_METHODS),
                (s.values[b"StoreVt".as_slice()], STORE_METHODS),
            ] {
                for &(name, _) in methods {
                    let method = keep(send(image, b"methodAt:", &[names[name]])?)?;
                    let method = keep(send(method, b"withContext:", &[*ns])?)?;
                    send(vt, b"addMethod", &[names[name], method])?;
                }
            }
            let receiver = keep(send(factory_vt, b"allocate", &[abi::size(0).unwrap()])?)?;
            s.receiver.set(receiver);
            send(roots, b"register:", &[receiver])?;
            send(s.gc, b"trace:with:", &[logic_vt, receiver])?;
            for vt in [logic_vt, factory_vt, entity_vt, collector_vt] {
                send(id.root, b"ownerOf:put:", &[vt, receiver])?;
            }
            for (name, value) in [
                (b"LogicVarVt".as_slice(), logic_vt),
                (b"LogicVarFactoryVt", factory_vt),
                (b"EntityVt", entity_vt),
                (b"RelCollectorVt", collector_vt),
                (b"LogicVar", receiver),
                (b"db", store),
            ] {
                send(*ns, b"at:put:", &[names[name], value])?;
            }
            s.ready.set(true);
            Ok(store)
        })();
        for token in tokens {
            send(roots, b"release:", &[token])?;
        }
        result
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
