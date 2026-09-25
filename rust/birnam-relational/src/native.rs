//! Relational's language adapter. All dependency operations use current Id sends.
use crate::evaluator::*;
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as native, Method, NativeEntry, Operand, Outcome, Protocol},
};
use core::{
    cell::{Cell, RefCell},
    ffi::c_void,
    mem::size_of,
};
use std::collections::HashMap;

fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
fn object(v: Operand) -> core::result::Result<Oop, Outcome> {
    v.as_object().filter(|o| !o.is_null()).ok_or_else(invalid)
}
#[derive(Debug)]
enum Error {
    Shape,
    Send(Outcome),
}
impl From<Outcome> for Error {
    fn from(v: Outcome) -> Self {
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
    unsafe fn send(&self, r: Oop, n: &'static [u8], a: &[Operand]) -> Result<Operand> {
        let v = unsafe { self.s.id.send(r, self.name(n), a) }?;
        if let Some(o) = v.as_object() {
            self.keep(o);
        }
        Ok(v)
    }
    unsafe fn call(&self, r: Oop, n: &'static [u8], a: &[Oop]) -> Result<Oop> {
        let args = a.iter().map(|o| Operand::object(*o)).collect::<Vec<_>>();
        let v = unsafe { self.send(r, n, &args) }?;
        if v.kind() == native::UNIT {
            return Ok(self.value(b"nil"));
        }
        Ok(object(v)?)
    }
    unsafe fn import(&self, name: &'static [u8]) -> Result<Oop> {
        let value = unsafe {
            self.send(
                self.s.namespace,
                b"at:",
                &[Operand::object(self.name(name))],
            )
        }?;
        value.as_object().ok_or(Error::Shape)
    }
    unsafe fn number(&self, v: Oop) -> Result<Option<i64>> {
        let v = unsafe { self.send(self.value(b"Integer"), b"wordOf:", &[Operand::object(v)]) }?;
        if v.kind() == native::UNIT {
            return Ok(None);
        }
        Ok(Some(v.as_word().ok_or_else(invalid)? as i64))
    }
    unsafe fn integer(&self, v: i64) -> Result<Oop> {
        Ok(object(unsafe {
            self.send(
                self.value(b"Integer"),
                b"fromWord:",
                &[Operand::word(v as usize)],
            )
        }?)?)
    }
    unsafe fn behavior(&self, v: Oop) -> Result<Oop> {
        unsafe { self.call(self.s.id.receiver, b"behaviorOf:", &[v]) }
    }
    unsafe fn classify(&self, v: Oop) -> Result<Value> {
        if let Some(n) = unsafe { self.number(v) }? {
            return Ok(Value::Integer(n));
        }
        let bits = unsafe { self.send(self.value(b"Float"), b"bitsOf:", &[Operand::object(v)]) }?;
        if let Some(bits) = bits.as_word() {
            return Ok(Value::Float(f64::from_bits(bits as u64)));
        }
        if bits.kind() != native::UNIT {
            return Err(invalid().into());
        }
        if unsafe { self.behavior(v) }? == self.value(b"StringVt") {
            let v = unsafe { self.send(v, b"bytes", &[]) }?;
            let raw = v.as_bytes().ok_or_else(invalid)?;
            return Ok(Value::String(
                unsafe { borrowed_bytes(raw.pointer, raw.length) }
                    .ok_or_else(invalid)?
                    .to_vec(),
            ));
        }
        Ok(Value::Identity(v as usize as u64))
    }
    unsafe fn materialize(&self, v: Value) -> Result<Oop> {
        match v {
            Value::Identity(v) => Ok(self.keep(v as usize as Oop)),
            Value::Integer(v) => unsafe { self.integer(v) },
            Value::Float(v) => Ok(object(unsafe {
                self.send(
                    self.value(b"Float"),
                    b"fromBits:",
                    &[Operand::word(v.to_bits() as usize)],
                )
            }?)?),
            Value::String(v) => Ok(object(unsafe {
                self.send(self.value(b"String"), b"fromBytes:", &[Operand::bytes(&v)])
            }?)?),
        }
    }
    unsafe fn allocate(&self, vt: Oop, bytes: usize) -> Result<Oop> {
        let o = object(unsafe { self.send(vt, b"allocate", &[Operand::word(bytes)]) }?)?;
        if unsafe { self.behavior(o) }? != vt
            || unsafe {
                self.send(
                    self.s.id.receiver,
                    b"allocationSizeOf:",
                    &[Operand::object(o)],
                )
            }?
            .as_word()
                != Some(bytes)
        {
            return Err(invalid().into());
        }
        Ok(o)
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
unsafe fn state<'a>(c: *mut c_void) -> core::result::Result<&'a State, Outcome> {
    let s = unsafe { &*c.cast::<State>() };
    if s.thread != std::thread::current().id() {
        return Err(invalid());
    }
    Ok(s)
}
fn answer(s: &State, r: Result<Oop>) -> Outcome {
    match r {
        Ok(o) => Outcome::loaded(Operand::object(o)),
        Err(Error::Shape) => Outcome::loaded(Operand::object(s.values[b"nil".as_slice()])),
        Err(Error::Send(e)) => e,
    }
}
unsafe extern "C" fn collect(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    let s = match unsafe { state(c) } {
        Ok(s) => s,
        Err(e) => return e,
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
unsafe fn invoke(c: *mut c_void, r: Oop, a: *const Operand, n: usize, op: usize) -> Outcome {
    let s = match unsafe { state(c) } {
        Ok(s) => s,
        Err(e) => return e,
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
                        || q.send(s.id.receiver, b"allocationSizeOf:", &[Operand::object(r)])?
                            .as_word()
                            != Some(size_of::<Oop>())
                    {
                        return Err(Error::Shape);
                    }
                    if op == 1 {
                        return Ok(r.cast::<Oop>().read());
                    }
                    let name = q.call(r, b"name", &[])?;
                    let raw = q.send(s.id.receiver, b"symbolBytesOf:", &[Operand::object(name)])?;
                    let raw = raw.as_bytes().ok_or_else(invalid)?;
                    let mut text = vec![b'?'];
                    text.extend_from_slice(
                        borrowed_bytes(raw.pointer, raw.length).ok_or_else(invalid)?,
                    );
                    Ok(object(q.send(
                        q.value(b"String"),
                        b"fromBytes:",
                        &[Operand::bytes(&text)],
                    )?)?)
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
                        || q.send(s.id.receiver, b"allocationSizeOf:", &[Operand::object(r)])?
                            .as_word()
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
        unsafe extern "C" fn $name(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
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
unsafe extern "C" fn roots(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
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
            s.id.send(v, s.names[b"visit:".as_slice()], &[Operand::object(edge)])?;
        }
        Ok(Operand::unit())
    })();
    result.map_or_else(|e| e, Outcome::loaded)
}
unsafe extern "C" fn references(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    let result = (|| unsafe {
        let s = state(c)?;
        let Some([o, v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let o = object(*o)?;
        let v = object(*v)?;
        let owned =
            s.id.send_named(s.id.receiver, b"behaviorOf:", &[Operand::object(o)])?
                .as_object()
                == Some(s.logic_vt)
                && s.id
                    .send_named(s.id.receiver, b"allocationSizeOf:", &[Operand::object(o)])?
                    .as_word()
                    == Some(size_of::<Oop>());
        if owned {
            s.id.send(
                v,
                s.names[b"visit:".as_slice()],
                &[Operand::object(o.cast::<Oop>().read())],
            )?;
        }
        Ok(Operand::unit())
    })();
    result.map_or_else(|e| e, Outcome::loaded)
}
unsafe fn construct(id: Protocol, ns: Oop) -> core::result::Result<Operand, Outcome> {
    unsafe {
        let lookup =
            |n: &[u8]| object(id.send_named(ns, b"at:", &[Operand::object(id.intern_name(n)?)])?);
        let roots = lookup(b"Roots")?;
        let store = lookup(b"Store")?;
        // Like the other modules, publication marks a completed construction.
        let existing =
            id.send_named(ns, b"at:", &[Operand::object(id.intern_name(b"LogicVar")?)])?;
        if existing.kind() != native::UNIT {
            return Ok(Operand::object(store));
        }
        let mut tokens = vec![];
        let mut keep = |o: Oop| -> core::result::Result<Oop, Outcome> {
            tokens.push(
                id.send_named(roots, b"retain:", &[Operand::object(o)])?
                    .as_word()
                    .filter(|n| *n != 0)
                    .ok_or_else(invalid)?,
            );
            Ok(o)
        };
        let result = (|| {
            let mut names = HashMap::new();
            for &n in NAMES {
                names.insert(n, keep(id.intern_name(n)?)?);
            }
            let mut values = HashMap::new();
            for n in [
                b"Integer".as_slice(),
                b"Float",
                b"String",
                b"StringVt",
                b"nil",
                b"Store",
                b"StoreVt",
            ] {
                values.insert(n, lookup(n)?);
            }
            let parent = lookup(b"ObjectVt")?;
            let mut vt = || keep(object(id.send_named(parent, b"delegated", &[])?)?);
            let logic_vt = vt()?;
            let factory_vt = vt()?;
            let entity_vt = vt()?;
            let collector_vt = vt()?;
            let state = Box::into_raw(Box::new(State {
                id,
                namespace: ns,
                roots,
                gc: lookup(b"GC")?,
                receiver: Cell::new(core::ptr::null_mut()),
                logic_vt,
                factory_vt,
                entity_vt,
                collector_vt,
                names,
                values,
                logicvars: RefCell::new(HashMap::new()),
                collectors: RefCell::new(HashMap::new()),
                active: RefCell::new(vec![]),
                thread: std::thread::current().id(),
            }));
            let s = &*state;
            let logic_methods = [
                (b"name".as_slice(), name as Method),
                (b"printString".as_slice(), print as Method),
                (b"clone".as_slice(), clone_logic as Method),
                (b"new".as_slice(), clone_logic as Method),
            ];
            let factory_methods = [
                (b"named:".as_slice(), named as Method),
                (b"rootsDo:".as_slice(), self::roots as Method),
                (b"referencesOf:do:".as_slice(), references as Method),
            ];
            let collector_methods = [(b"value:value:value:".as_slice(), collect as Method)];
            for (vt, methods) in [
                (logic_vt, logic_methods.as_slice()),
                (factory_vt, factory_methods.as_slice()),
                (collector_vt, collector_methods.as_slice()),
                (s.values[b"StoreVt".as_slice()], STORE_METHODS),
            ] {
                for &(name, method) in methods {
                    id.send(
                        vt,
                        s.names[b"addMethod".as_slice()],
                        &[
                            Operand::object(s.names[name]),
                            Operand::entry(NativeEntry::new(method, state.cast())),
                        ],
                    )?;
                }
            }
            let receiver = keep(object(id.send_named(
                factory_vt,
                b"allocate",
                &[Operand::word(0)],
            )?)?)?;
            s.receiver.set(receiver);
            id.send_named(roots, b"register:", &[Operand::object(receiver)])?;
            id.send_named(
                s.gc,
                b"trace:with:",
                &[Operand::object(logic_vt), Operand::object(receiver)],
            )?;
            for vt in [logic_vt, factory_vt, entity_vt, collector_vt] {
                id.send_named(
                    id.receiver,
                    b"ownerOf:put:",
                    &[Operand::object(vt), Operand::object(receiver)],
                )?;
            }
            for (name, o) in [
                (b"LogicVarVt".as_slice(), logic_vt),
                (b"LogicVarFactoryVt", factory_vt),
                (b"EntityVt", entity_vt),
                (b"RelCollectorVt", collector_vt),
                (b"LogicVar", receiver),
                (b"db", store),
            ] {
                id.send_named(
                    ns,
                    b"at:put:",
                    &[Operand::object(s.names[name]), Operand::object(o)],
                )?;
            }
            Ok(Operand::object(store))
        })();
        for t in tokens {
            id.send_named(roots, b"release:", &[Operand::word(t)])?;
        }
        result
    }
}
unsafe extern "C" fn initialize(id: *const Protocol, a: *const Operand, n: usize) -> Outcome {
    if !unsafe { native::valid_protocol(id) } {
        return invalid();
    }
    let result = (|| unsafe {
        let Some([ns]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        construct(*id, object(*ns)?)
    })();
    result.map_or_else(|e| e, Outcome::loaded)
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module {
    version: 1,
    byte_size: size_of::<native::Module>() as u32,
    initialize: Some(initialize),
};

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
    b"symbolBytesOf:",
    b"trace:with:",
    b"value:value:value:",
    b"visit:",
    b"wordOf:",
    b"~=",
];
