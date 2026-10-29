//! Query values, matching and rule evaluation over a caller-owned snapshot.
//! This module owns no IdApi, Store, Oop root or callback state.

/// Maximum pull path depth shared by evaluation and the native record format.
pub const PULL_MAX_DEPTH: usize = 4;

/// A value classified by the IdApi adapter before entering the query kernel.
/// Identity values compare by pointer identity; scalar values compare by value,
/// matching the relational `valEq` law.
#[derive(Clone, Debug)]
pub enum Value {
    Identity(u64),
    Integer(i64),
    Float(f64),
    String(Vec<u8>),
}

impl Value {
    pub fn matches(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Identity(left), Self::Identity(right)) => left == right,
            (Self::Integer(left), Self::Integer(right)) => left == right,
            (Self::Float(left), Self::Float(right)) => left == right,
            (Self::String(left), Self::String(right)) => left == right,
            _ => false,
        }
    }
}

/// A copied triple view supplied by the Store callback seam.
#[derive(Clone, Debug)]
pub struct Triple {
    pub entity: Value,
    pub attribute: Value,
    pub value: Value,
}

/// A clause term.
#[derive(Clone, Debug)]
pub enum Term {
    Constant(Value),
    Variable(u32),
}

#[derive(Clone, Debug)]
pub struct Clause(pub [Term; 3]);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PullKind {
    Leaf,
    Reference,
}

/// Owned, authority-free pull specification used by the pure kernel. The FFI
/// adapter builds this tree from borrowed Oops and immediately copies every
/// attribute into a `Value`.
#[derive(Clone, Debug)]
pub struct PullSpec {
    pub attribute: Value,
    pub kind: PullKind,
    pub many: bool,
    pub children: Vec<PullSpec>,
}

#[derive(Clone, Debug)]
pub struct PullRecord {
    pub path: Vec<Value>,
    pub value: Value,
}

#[derive(Clone, Debug)]
pub struct GroupedAggregate {
    pub group: Value,
    pub aggregate: Option<Value>,
}

#[derive(Clone, Debug)]
pub enum RuleClause {
    Triple(Clause),
    Call {
        name: Value,
        args: Vec<Term>,
    },
    Predicate {
        left: Term,
        operation: CompareOp,
        right: Term,
    },
    Negated(Vec<RuleClause>),
    Disjunction(Vec<Vec<RuleClause>>),
}

#[derive(Clone, Debug)]
pub struct RuleDefinition {
    pub name: Value,
    pub params: Vec<u32>,
    pub body: Vec<RuleClause>,
}

#[derive(Clone, Debug)]
pub struct Binding {
    pub variable: u32,
    pub value: Value,
}

/// The bounded comparison vocabulary used by the predicate migration probe.
/// Ordering follows the Prelude numeric law: Integer/Float pairs promote to
/// Float for ordering, while equality remains type-strict.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum CompareOp {
    Less = 0,
    LessEqual = 1,
    Greater = 2,
    GreaterEqual = 3,
    Equal = 4,
    NotEqual = 5,
}

impl CompareOp {
    pub(super) fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::Less,
            1 => Self::LessEqual,
            2 => Self::Greater,
            3 => Self::GreaterEqual,
            4 => Self::Equal,
            5 => Self::NotEqual,
            _ => return None,
        })
    }
}

/// The bounded aggregate vocabulary used by the migration probe.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum AggregateOp {
    Count = 0,
    Sum = 1,
    Min = 2,
    Max = 3,
    Average = 4,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AggregateError {
    UnsupportedValue,
    Overflow,
}

impl AggregateOp {
    pub(super) fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::Count,
            1 => Self::Sum,
            2 => Self::Min,
            3 => Self::Max,
            4 => Self::Average,
            _ => return None,
        })
    }
}

impl Value {
    /// Apply one Prelude-shaped comparison without sending a borrowed Oop
    /// across the Rust boundary. Unknown/non-numeric ordering operands fail the
    /// row, matching the relational filter's error-to-no-row behavior.
    pub fn compares(&self, op: CompareOp, other: &Self) -> bool {
        match op {
            CompareOp::Equal => self.matches(other),
            CompareOp::NotEqual => !self.matches(other),
            CompareOp::Less
            | CompareOp::LessEqual
            | CompareOp::Greater
            | CompareOp::GreaterEqual => {
                let ordering = match (self, other) {
                    (Self::Integer(left), Self::Integer(right)) => left.partial_cmp(right),
                    (Self::Integer(left), Self::Float(right)) => (*left as f64).partial_cmp(right),
                    (Self::Float(left), Self::Integer(right)) => left.partial_cmp(&(*right as f64)),
                    (Self::Float(left), Self::Float(right)) => left.partial_cmp(right),
                    _ => None,
                };
                matches!(
                    (op, ordering),
                    (CompareOp::Less, Some(std::cmp::Ordering::Less))
                        | (CompareOp::LessEqual, Some(std::cmp::Ordering::Less))
                        | (CompareOp::LessEqual, Some(std::cmp::Ordering::Equal))
                        | (CompareOp::Greater, Some(std::cmp::Ordering::Greater))
                        | (CompareOp::GreaterEqual, Some(std::cmp::Ordering::Greater))
                        | (CompareOp::GreaterEqual, Some(std::cmp::Ordering::Equal))
                )
            }
        }
    }
}

fn lookup(solution: &[Binding], variable: u32) -> Option<&Value> {
    solution
        .iter()
        .find(|binding| binding.variable == variable)
        .map(|binding| &binding.value)
}

fn match_term(solution: &[Binding], fresh: &mut Vec<Binding>, term: &Term, value: &Value) -> bool {
    match term {
        Term::Constant(expected) => expected.matches(value),
        Term::Variable(variable) => {
            let bound = lookup(solution, *variable).or_else(|| lookup(fresh, *variable));
            match bound {
                Some(expected) => expected.matches(value),
                None => {
                    fresh.push(Binding {
                        variable: *variable,
                        value: value.clone(),
                    });
                    true
                }
            }
        }
    }
}

fn evaluate_clauses(facts: &[Triple], clauses: &[Clause]) -> Vec<Vec<Binding>> {
    let mut solutions: Vec<Vec<Binding>> = vec![Vec::new()];
    for Clause(terms) in clauses {
        let mut next = Vec::new();
        for solution in &solutions {
            for triple in facts {
                let values = [&triple.entity, &triple.attribute, &triple.value];
                let mut fresh = Vec::new();
                if terms
                    .iter()
                    .zip(values)
                    .all(|(term, value)| match_term(solution, &mut fresh, term, value))
                {
                    let mut extended = solution.clone();
                    extended.extend(fresh);
                    next.push(extended);
                }
            }
        }
        solutions = next;
        if solutions.is_empty() {
            break;
        }
    }
    solutions
}

fn project_solutions(solutions: Vec<Vec<Binding>>, result: u32) -> Vec<Value> {
    let mut result_values: Vec<Value> = Vec::new();
    for solution in solutions {
        let Some(value) = lookup(&solution, result) else {
            continue;
        };
        if !result_values.iter().any(|seen| seen.matches(value)) {
            result_values.push(value.clone());
        }
    }
    result_values
}

/// Evaluate a left-to-right conjunction and project one variable.
///
/// Fact order is preserved and projected values are set-like: the first
/// occurrence wins, matching the existing relational surface's deterministic
/// result order and distinct projection semantics. No query result borrows the
/// input snapshot.
pub fn project(facts: &[Triple], clauses: &[Clause], result: u32) -> Vec<Value> {
    project_solutions(evaluate_clauses(facts, clauses), result)
}

/// Evaluate one triple clause, filter its bound variable with a comparison,
/// and project another variable. This is deliberately narrower than the
/// canonical query protocol: the predicate is constant-rhs, runs after the
/// single binder, and does not reorder or bind solutions.
pub fn project_predicate(
    facts: &[Triple],
    clause: &Clause,
    predicate_variable: u32,
    op: CompareOp,
    rhs: &Value,
    result: u32,
) -> Vec<Value> {
    let solutions = evaluate_clauses(facts, std::slice::from_ref(clause));
    let filtered = solutions.into_iter().filter(|solution| {
        lookup(solution, predicate_variable).is_some_and(|left| left.compares(op, rhs))
    });
    project_solutions(filtered.collect(), result)
}

/// Evaluate two positive branches from the same empty solution set and union
/// their projected values. Branch order is preserved and projection remains
/// distinct under the existing value law. The bounded FFI probe uses one
/// triple clause per branch; the kernel keeps this helper independent of the
/// Store and IdApi.
pub fn project_disjunction(facts: &[Triple], branches: &[Clause], result: u32) -> Vec<Value> {
    let mut values: Vec<Value> = Vec::new();
    for branch in branches {
        for value in project(facts, std::slice::from_ref(branch), result) {
            if !values.iter().any(|seen| seen.matches(&value)) {
                values.push(value);
            }
        }
    }
    values
}

/// Compute the transitive closure of a bounded binary edge clause. The clause
/// must bind variables `0` and `2`; its middle term is the edge attribute. New
/// `(source, target)` pairs are appended in deterministic breadth-first
/// fixpoint order and compared by the existing value law. The result projects
/// either side for rows whose source equals `root`.
pub fn project_recursive(facts: &[Triple], edge: &Clause, root: &Value, result: u32) -> Vec<Value> {
    if result != 0 && result != 2 {
        return Vec::new();
    }
    let solutions = evaluate_clauses(facts, std::slice::from_ref(edge));
    let mut edges: Vec<(Value, Value)> = Vec::new();
    for solution in solutions {
        let (Some(source), Some(target)) = (lookup(&solution, 0), lookup(&solution, 2)) else {
            continue;
        };
        if !edges.iter().any(|(seen_source, seen_target)| {
            seen_source.matches(source) && seen_target.matches(target)
        }) {
            edges.push((source.clone(), target.clone()));
        }
    }

    let mut cursor = 0;
    while cursor < edges.len() {
        let (source, middle) = edges[cursor].clone();
        for (next_source, target) in edges.clone() {
            if !middle.matches(&next_source)
                || edges.iter().any(|(seen_source, seen_target)| {
                    seen_source.matches(&source) && seen_target.matches(&target)
                })
            {
                continue;
            }
            edges.push((source.clone(), target));
        }
        cursor += 1;
    }

    let mut values: Vec<Value> = Vec::new();
    for (source, target) in edges {
        if !source.matches(root) {
            continue;
        }
        let value = if result == 0 { source } else { target };
        if !values.iter().any(|seen| seen.matches(&value)) {
            values.push(value);
        }
    }
    values
}

/// Fold distinct projected values with the frozen aggregate law. Count and
/// empty sum return integer zero. Min/max/average return `None` on empty input
/// (the caller maps that to the explicit nil status) and reject nonnumeric
/// values rather than inventing a cross-floor coercion.
pub fn aggregate_values(
    values: &[Value],
    operation: AggregateOp,
) -> Result<Option<Value>, AggregateError> {
    match operation {
        AggregateOp::Count => Ok(Some(Value::Integer(values.len() as i64))),
        AggregateOp::Sum | AggregateOp::Average => {
            if values.is_empty() {
                return Ok(Some(Value::Integer(0)));
            }
            let has_float = values.iter().any(|value| matches!(value, Value::Float(_)));
            if has_float {
                let mut sum = 0.0;
                for value in values {
                    sum += match value {
                        Value::Integer(integer) => *integer as f64,
                        Value::Float(float) => *float,
                        _ => return Err(AggregateError::UnsupportedValue),
                    };
                }
                if operation == AggregateOp::Average {
                    Ok(Some(Value::Float(sum / values.len() as f64)))
                } else {
                    Ok(Some(Value::Float(sum)))
                }
            } else {
                let mut sum = 0i64;
                for value in values {
                    let Value::Integer(integer) = value else {
                        return Err(AggregateError::UnsupportedValue);
                    };
                    sum = sum.checked_add(*integer).ok_or(AggregateError::Overflow)?;
                }
                if operation == AggregateOp::Average {
                    Ok(Some(Value::Integer(sum / values.len() as i64)))
                } else {
                    Ok(Some(Value::Integer(sum)))
                }
            }
        }
        AggregateOp::Min | AggregateOp::Max => {
            let mut best: Option<&Value> = None;
            for value in values {
                if !matches!(value, Value::Integer(_) | Value::Float(_)) {
                    return Err(AggregateError::UnsupportedValue);
                }
                let replace = match best {
                    None => true,
                    Some(current) => {
                        let op = if operation == AggregateOp::Min {
                            CompareOp::Less
                        } else {
                            CompareOp::Greater
                        };
                        value.compares(op, current)
                    }
                };
                if replace {
                    best = Some(value);
                }
            }
            Ok(best.cloned())
        }
    }
}

pub(super) fn pull_values(facts: &[Triple], entity: &Value, attribute: &Value) -> Vec<Value> {
    let mut values: Vec<Value> = Vec::new();
    for triple in facts {
        if !triple.entity.matches(entity) || !triple.attribute.matches(attribute) {
            continue;
        }
        if !values.iter().any(|seen| seen.matches(&triple.value)) {
            values.push(triple.value.clone());
        }
    }
    values
}

fn pull_into(
    facts: &[Triple],
    entity: &Value,
    specs: &[PullSpec],
    path: &mut Vec<Value>,
    output: &mut Vec<PullRecord>,
) {
    for spec in specs {
        let values = pull_values(facts, entity, &spec.attribute);
        if values.is_empty() {
            continue;
        }
        path.push(spec.attribute.clone());
        match spec.kind {
            PullKind::Leaf => {
                let selected = if spec.many {
                    values.into_iter().collect::<Vec<_>>()
                } else {
                    values.into_iter().take(1).collect::<Vec<_>>()
                };
                for value in selected {
                    output.push(PullRecord {
                        path: path.clone(),
                        value,
                    });
                }
            }
            PullKind::Reference => {
                let selected = if spec.many {
                    values.into_iter().collect::<Vec<_>>()
                } else {
                    values.into_iter().take(1).collect::<Vec<_>>()
                };
                for reference in selected {
                    // The FFI parser bounds this tree to PULL_MAX_DEPTH. The
                    // guard also makes the pure helper total if called directly
                    // by a future Rust consumer with a malformed tree.
                    if path.len() >= PULL_MAX_DEPTH {
                        continue;
                    }
                    pull_into(facts, &reference, &spec.children, path, output);
                }
            }
        }
        path.pop();
    }
}

/// Flatten a nested pull specification over a copied Store snapshot. This is
/// the authority-free kernel seam for the later canonical `pull:spec:` method:
/// facts remain Store-owned, cardinality is supplied by the adapter, and the
/// result owns only copied values and attribute paths.
pub fn pull(facts: &[Triple], root: &Value, specs: &[PullSpec]) -> Vec<PullRecord> {
    let mut output = Vec::new();
    let mut path = Vec::new();
    pull_into(facts, root, specs, &mut path, &mut output);
    output
}

/// Group the solutions of two positive clauses by one bound variable and fold
/// distinct values of another. This is the bounded kernel seam for the later
/// canonical `groupBy:aggregate:of:where:` method: group order follows first
/// appearance, values use the same value law, and facts remain Store-owned.
pub fn group_aggregate(
    facts: &[Triple],
    clauses: &[Clause],
    group_variable: u32,
    value_variable: u32,
    operation: AggregateOp,
) -> Result<Vec<GroupedAggregate>, AggregateError> {
    let solutions = evaluate_clauses(facts, clauses);
    let mut groups: Vec<(Value, Vec<Value>)> = Vec::new();
    for solution in solutions {
        let (Some(group), Some(value)) = (
            lookup(&solution, group_variable),
            lookup(&solution, value_variable),
        ) else {
            continue;
        };
        let Some(existing) = groups.iter_mut().find(|(seen, _)| seen.matches(group)) else {
            groups.push((group.clone(), vec![value.clone()]));
            continue;
        };
        if !existing.1.iter().any(|seen| seen.matches(value)) {
            existing.1.push(value.clone());
        }
    }
    groups
        .into_iter()
        .map(|(group, values)| {
            Ok(GroupedAggregate {
                group,
                aggregate: aggregate_values(&values, operation)?,
            })
        })
        .collect()
}

/// Group a query body that may include rule calls, predicates, negation, and
/// disjunction. Definitions are rebuilt from the same authoritative snapshot;
/// the grouped result owns copied values and no cache survives the call.
pub fn group_rule_body(
    facts: &[Triple],
    definitions: &[RuleDefinition],
    body: &[RuleClause],
    group_variable: u32,
    value_variable: u32,
    operation: AggregateOp,
) -> Result<Vec<GroupedAggregate>, AggregateError> {
    let mut cache = RuleCache::default();
    cache.rebuild(facts, definitions);
    let solutions = evaluate_rule_body(facts, &cache.relations, body);
    let mut groups: Vec<(Value, Vec<Value>)> = Vec::new();
    for solution in solutions {
        let (Some(group), Some(value)) = (
            lookup(&solution, group_variable),
            lookup(&solution, value_variable),
        ) else {
            continue;
        };
        let Some(existing) = groups.iter_mut().find(|(seen, _)| seen.matches(group)) else {
            groups.push((group.clone(), vec![value.clone()]));
            continue;
        };
        if !existing.1.iter().any(|seen| seen.matches(value)) {
            existing.1.push(value.clone());
        }
    }
    groups
        .into_iter()
        .map(|(group, values)| {
            Ok(GroupedAggregate {
                group,
                aggregate: aggregate_values(&values, operation)?,
            })
        })
        .collect()
}

// Validate borrowed candidates before constructing an owned result. Triple
// callers pass a stack array of references; rule calls borrow their stored row.
// No language callbacks occur here, and rejected candidates allocate nothing.
fn extend_rule_solution<V: std::borrow::Borrow<Value>>(
    solution: &[Binding],
    terms: &[Term],
    values: &[V],
) -> Option<Vec<Binding>> {
    if terms.len() != values.len() {
        return None;
    }
    for (index, (term, value)) in terms.iter().zip(values).enumerate() {
        let value = value.borrow();
        match term {
            Term::Constant(expected) if !expected.matches(value) => return None,
            Term::Constant(_) => {}
            Term::Variable(variable) => {
                let existing = lookup(solution, *variable).or_else(|| {
                    let earlier = terms[..index].iter().position(
                        |earlier| matches!(earlier, Term::Variable(name) if name == variable),
                    )?;
                    Some(values[earlier].borrow())
                });
                if let Some(existing) = existing {
                    if !existing.matches(value) {
                        return None;
                    }
                }
            }
        }
    }
    let mut extended = solution.to_vec();
    for (term, value) in terms.iter().zip(values) {
        if let Term::Variable(variable) = term {
            if lookup(&extended, *variable).is_none() {
                extended.push(Binding {
                    variable: *variable,
                    value: value.borrow().clone(),
                });
            }
        }
    }
    Some(extended)
}

#[derive(Clone, Debug)]
struct DerivedRelation {
    name: Value,
    rows: Vec<Vec<Value>>,
}

fn relation_rows<'a>(relations: &'a [DerivedRelation], name: &Value) -> &'a [Vec<Value>] {
    relations
        .iter()
        .find(|relation| relation.name.matches(name))
        .map_or(&[], |relation| relation.rows.as_slice())
}

fn term_value(solution: &[Binding], term: &Term) -> Option<Value> {
    match term {
        Term::Constant(value) => Some(value.clone()),
        Term::Variable(variable) => lookup(solution, *variable).cloned(),
    }
}

fn add_clause_variable(variables: &mut Vec<u32>, term: &Term) {
    let Term::Variable(variable) = term else {
        return;
    };
    if !variables.contains(variable) {
        variables.push(*variable);
    }
}

fn clause_variables(clause: &RuleClause, variables: &mut Vec<u32>) {
    match clause {
        RuleClause::Triple(Clause(terms)) => {
            for term in terms {
                add_clause_variable(variables, term);
            }
        }
        RuleClause::Call { args, .. } => {
            for term in args {
                add_clause_variable(variables, term);
            }
        }
        RuleClause::Predicate { left, right, .. } => {
            add_clause_variable(variables, left);
            add_clause_variable(variables, right);
        }
        RuleClause::Negated(inner) => {
            for clause in inner {
                clause_variables(clause, variables);
            }
        }
        RuleClause::Disjunction(branches) => {
            for branch in branches {
                for clause in branch {
                    clause_variables(clause, variables);
                }
            }
        }
    }
}

fn clause_is_binder(clause: &RuleClause) -> bool {
    matches!(
        clause,
        RuleClause::Triple(_) | RuleClause::Call { .. } | RuleClause::Disjunction(_)
    )
}

/// Pick binders before filters while retaining source order among clauses that
/// are ready. This is the native relational planner: a
/// predicate or negation written before its operands are bound must not drop
/// every row merely because it appeared first in the Vector.
fn plan_rule_body(body: &[RuleClause]) -> Vec<usize> {
    let mut order = Vec::with_capacity(body.len());
    let mut used = vec![false; body.len()];
    let mut bound = Vec::new();
    for _ in 0..body.len() {
        let mut chosen = None;
        for (index, clause) in body.iter().enumerate() {
            if used[index] {
                continue;
            }
            let ready = clause_is_binder(clause) || {
                let mut variables = Vec::new();
                clause_variables(clause, &mut variables);
                variables.iter().all(|variable| bound.contains(variable))
            };
            if ready {
                chosen = Some(index);
                break;
            }
        }
        let index = chosen.or_else(|| used.iter().position(|is_used| !is_used));
        let Some(index) = index else {
            break;
        };
        used[index] = true;
        order.push(index);
        if clause_is_binder(&body[index]) {
            clause_variables(&body[index], &mut bound);
        }
    }
    order
}

fn evaluate_rule_body_from_solutions(
    facts: &[Triple],
    relations: &[DerivedRelation],
    body: &[RuleClause],
    mut solutions: Vec<Vec<Binding>>,
) -> Vec<Vec<Binding>> {
    for clause_index in plan_rule_body(body) {
        let clause = &body[clause_index];
        let mut next = Vec::new();
        for solution in &solutions {
            match clause {
                RuleClause::Triple(Clause(terms)) => {
                    for fact in facts {
                        let values = [&fact.entity, &fact.attribute, &fact.value];
                        if let Some(extended) = extend_rule_solution(solution, terms, &values) {
                            next.push(extended);
                        }
                    }
                }
                RuleClause::Call { name, args } => {
                    for row in relation_rows(relations, name) {
                        if let Some(extended) = extend_rule_solution(solution, args, row) {
                            next.push(extended);
                        }
                    }
                }
                RuleClause::Predicate {
                    left,
                    operation,
                    right,
                } => {
                    let (Some(left), Some(right)) =
                        (term_value(solution, left), term_value(solution, right))
                    else {
                        continue;
                    };
                    if left.compares(*operation, &right) {
                        next.push(solution.clone());
                    }
                }
                RuleClause::Negated(inner) => {
                    let nested = evaluate_rule_body_from_solutions(
                        facts,
                        relations,
                        inner,
                        vec![solution.clone()],
                    );
                    if nested.is_empty() {
                        next.push(solution.clone());
                    }
                }
                RuleClause::Disjunction(branches) => {
                    for branch in branches {
                        let nested = evaluate_rule_body_from_solutions(
                            facts,
                            relations,
                            branch,
                            vec![solution.clone()],
                        );
                        next.extend(nested);
                    }
                }
            }
        }
        solutions = next;
        if solutions.is_empty() {
            break;
        }
    }
    solutions
}

fn evaluate_rule_body(
    facts: &[Triple],
    relations: &[DerivedRelation],
    body: &[RuleClause],
) -> Vec<Vec<Binding>> {
    evaluate_rule_body_from_solutions(facts, relations, body, vec![Vec::new()])
}

fn relation_contains(rows: &[Vec<Value>], candidate: &[Value]) -> bool {
    rows.iter().any(|row| {
        row.len() == candidate.len()
            && row
                .iter()
                .zip(candidate)
                .all(|(left, right)| left.matches(right))
    })
}

/// A query-scoped derived-relation cache. It is deliberately separate from
/// Store: callers rebuild it from an authoritative snapshot and definitions,
/// then discard it when the snapshot changes. This mirrors the rule-cache
/// lifecycle without allowing Rust to retain a second fact authority.
#[derive(Clone, Debug, Default)]
pub struct RuleCache {
    relations: Vec<DerivedRelation>,
}

impl RuleCache {
    /// Replace every cached relation and materialize the supplied definitions
    /// to their finite least fixpoint in definition order.
    pub fn rebuild(&mut self, facts: &[Triple], definitions: &[RuleDefinition]) {
        self.relations.clear();
        for definition in definitions {
            if !self
                .relations
                .iter()
                .any(|relation| relation.name.matches(&definition.name))
            {
                self.relations.push(DerivedRelation {
                    name: definition.name.clone(),
                    rows: Vec::new(),
                });
            }
        }

        self.materialize(facts, definitions);
    }

    fn materialize(&mut self, facts: &[Triple], definitions: &[RuleDefinition]) {
        const MAX_ITERATIONS: usize = 1_000;
        for _ in 0..MAX_ITERATIONS {
            let mut changed = false;
            for definition in definitions {
                let candidates = evaluate_rule_body(facts, &self.relations, &definition.body);
                let mut rows = Vec::new();
                for solution in candidates {
                    let Some(tuple) = definition
                        .params
                        .iter()
                        .map(|variable| lookup(&solution, *variable).cloned())
                        .collect::<Option<Vec<_>>>()
                    else {
                        continue;
                    };
                    if !relation_contains(&rows, &tuple) {
                        rows.push(tuple);
                    }
                }
                let relation = self
                    .relations
                    .iter_mut()
                    .find(|relation| relation.name.matches(&definition.name))
                    .expect("rule relation was seeded above");
                for row in rows {
                    if !relation_contains(&relation.rows, &row) {
                        relation.rows.push(row);
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
    }

    /// Query one cached relation with a positional call pattern and project
    /// one bound variable. Results are distinct and retain relation order.
    pub fn query(&self, name: &Value, args: &[Term], result_variable: u32) -> Vec<Value> {
        let mut values: Vec<Value> = Vec::new();
        for row in relation_rows(&self.relations, name) {
            let Some(solution) = extend_rule_solution(&[], args, row) else {
                continue;
            };
            let Some(value) = lookup(&solution, result_variable) else {
                continue;
            };
            if !values.iter().any(|seen| seen.matches(value)) {
                values.push(value.clone());
            }
        }
        values
    }

    fn rows(&self) -> Vec<Vec<Value>> {
        self.relations
            .iter()
            .flat_map(|relation| relation.rows.iter().cloned())
            .collect()
    }
}

/// Materialize all rule definitions to a finite least fixpoint. Definitions
/// with the same name form a union; body calls read the current partial
/// relations, so mutual recursion converges without host recursion or a
/// second Store authority.
pub fn materialize_rules(facts: &[Triple], definitions: &[RuleDefinition]) -> Vec<Vec<Value>> {
    let mut cache = RuleCache::default();
    cache.rebuild(facts, definitions);
    cache.rows()
}

/// Query one materialized rule relation with a positional call pattern and
/// project one bound variable. The result is distinct and deterministic.
pub fn query_rule(
    facts: &[Triple],
    definitions: &[RuleDefinition],
    name: &Value,
    args: &[Term],
    result_variable: u32,
) -> Vec<Value> {
    let mut cache = RuleCache::default();
    cache.rebuild(facts, definitions);
    cache.query(name, args, result_variable)
}

/// Evaluate a canonical query body against one authoritative Store snapshot
/// and the definitions reified from that same snapshot. The rule cache is
/// rebuilt for this call and discarded afterwards; it is an execution aid, not
/// a second fact authority. This is the pure seam used by the later
/// `findAll:where:` adapter.
pub fn project_rule_body(
    facts: &[Triple],
    definitions: &[RuleDefinition],
    body: &[RuleClause],
    result_variable: u32,
) -> Vec<Value> {
    let mut cache = RuleCache::default();
    cache.rebuild(facts, definitions);
    let solutions = evaluate_rule_body(facts, &cache.relations, body);
    project_solutions(solutions, result_variable)
}

fn clause_matches_solution(facts: &[Triple], solution: &[Binding], clause: &Clause) -> bool {
    let Clause(terms) = clause;
    facts.iter().any(|triple| {
        let values = [&triple.entity, &triple.attribute, &triple.value];
        let mut fresh = Vec::new();
        terms
            .iter()
            .zip(values)
            .all(|(term, value)| match_term(solution, &mut fresh, term, value))
    })
}

/// Evaluate one positive binder, then apply one negation-as-failure clause.
/// Variables already present in the positive solution are shared by position;
/// variables introduced only by the negative clause are scoped to its existence
/// check and never escape into the projected result.
pub fn project_negated(
    facts: &[Triple],
    positive: &Clause,
    negative: &Clause,
    result: u32,
) -> Vec<Value> {
    let solutions = evaluate_clauses(facts, std::slice::from_ref(positive));
    let kept = solutions
        .into_iter()
        .filter(|solution| !clause_matches_solution(facts, solution, negative));
    project_solutions(kept.collect(), result)
}

#[cfg(test)]
#[path = "query_matching_tests.rs"]
mod query_matching_tests;

#[cfg(test)]
mod tests {
    use super::{
        AggregateOp, Clause, CompareOp, PullKind, PullSpec, RuleCache, RuleClause, RuleDefinition,
        Term, Triple, Value, aggregate_values, group_aggregate, group_rule_body, project,
        project_disjunction, project_negated, project_predicate, project_recursive,
        project_rule_body, pull, query_rule,
    };

    fn id(value: u64) -> Value {
        Value::Identity(value)
    }

    fn integer(value: i64) -> Value {
        Value::Integer(value)
    }

    fn float(value: f64) -> Value {
        Value::Float(value)
    }

    fn fact(entity: u64, attribute: u64, value: u64) -> Triple {
        Triple {
            entity: id(entity),
            attribute: id(attribute),
            value: id(value),
        }
    }

    #[test]
    fn single_clause_projects_distinct_values_in_store_order() {
        let facts = [fact(1, 10, 100), fact(2, 10, 100), fact(3, 10, 200)];
        let clauses = [Clause([
            Term::Variable(0),
            Term::Constant(id(10)),
            Term::Variable(1),
        ])];

        let values = project(&facts, &clauses, 1);
        assert_eq!(values.len(), 2);
        assert!(values[0].matches(&id(100)));
        assert!(values[1].matches(&id(200)));
    }

    #[test]
    fn conjunction_joins_on_shared_variable() {
        let facts = [
            fact(1, 10, 100),
            fact(2, 10, 200),
            fact(1, 20, 300),
            fact(3, 20, 400),
        ];
        let clauses = [
            Clause([Term::Variable(0), Term::Constant(id(10)), Term::Variable(1)]),
            Clause([Term::Variable(0), Term::Constant(id(20)), Term::Variable(2)]),
        ];

        let values = project(&facts, &clauses, 2);
        assert_eq!(values.len(), 1);
        assert!(values[0].matches(&id(300)));
    }

    #[test]
    fn repeated_variable_within_clause_must_agree() {
        let facts = [fact(1, 10, 2), fact(2, 10, 3)];
        let clauses = [Clause([
            Term::Variable(0),
            Term::Constant(id(10)),
            Term::Variable(0),
        ])];

        assert!(project(&facts, &clauses, 0).is_empty());
    }

    #[test]
    fn unbound_result_is_not_invented() {
        let facts = [fact(1, 10, 2)];
        let clauses = [Clause([
            Term::Constant(id(1)),
            Term::Constant(id(10)),
            Term::Constant(id(2)),
        ])];

        assert!(project(&facts, &clauses, 99).is_empty());
    }

    #[test]
    fn scalar_constants_match_by_value_and_deduplicate() {
        let facts = [
            Triple {
                entity: id(1),
                attribute: id(10),
                value: integer(7),
            },
            Triple {
                entity: id(2),
                attribute: id(10),
                value: integer(7),
            },
        ];
        let clauses = [Clause([
            Term::Variable(0),
            Term::Constant(id(10)),
            Term::Constant(integer(7)),
        ])];

        let values = project(&facts, &clauses, 0);
        assert_eq!(values.len(), 2);
        assert!(values[0].matches(&id(1)));
        assert!(values[1].matches(&id(2)));
    }

    #[test]
    fn scalar_projection_deduplicates_equal_values() {
        let facts = [
            Triple {
                entity: id(1),
                attribute: id(10),
                value: integer(7),
            },
            Triple {
                entity: id(2),
                attribute: id(10),
                value: integer(7),
            },
        ];
        let clauses = [Clause([
            Term::Variable(0),
            Term::Constant(id(10)),
            Term::Variable(1),
        ])];

        let values = project(&facts, &clauses, 1);
        assert_eq!(values.len(), 1);
        assert!(values[0].matches(&integer(7)));
    }

    #[test]
    fn predicate_filters_after_binding_with_numeric_promotion_and_strict_equality() {
        let facts = [
            Triple {
                entity: id(1),
                attribute: id(10),
                value: integer(18),
            },
            Triple {
                entity: id(2),
                attribute: id(10),
                value: integer(30),
            },
            Triple {
                entity: id(3),
                attribute: id(10),
                value: float(30.0),
            },
        ];
        let clause = Clause([Term::Variable(0), Term::Constant(id(10)), Term::Variable(1)]);

        let over_eighteen =
            project_predicate(&facts, &clause, 1, CompareOp::Greater, &integer(18), 0);
        assert_eq!(over_eighteen.len(), 2);
        assert!(over_eighteen[0].matches(&id(2)));
        assert!(over_eighteen[1].matches(&id(3)));

        let exact_integer =
            project_predicate(&facts, &clause, 1, CompareOp::Equal, &integer(30), 0);
        assert_eq!(exact_integer.len(), 1);
        assert!(exact_integer[0].matches(&id(2)));

        let not_integer =
            project_predicate(&facts, &clause, 1, CompareOp::NotEqual, &integer(30), 0);
        assert_eq!(not_integer.len(), 2);
        assert!(not_integer[0].matches(&id(1)));
        assert!(not_integer[1].matches(&id(3)));
    }

    #[test]
    fn negation_excludes_matching_rows_without_binding_negative_variables() {
        let facts = [
            fact(1, 10, 100),
            fact(1, 20, 200),
            fact(2, 10, 100),
            fact(2, 20, 300),
            fact(3, 10, 100),
        ];
        let positive = Clause([
            Term::Variable(0),
            Term::Constant(id(10)),
            Term::Constant(id(100)),
        ]);
        let forbidden = Clause([
            Term::Variable(0),
            Term::Constant(id(20)),
            Term::Constant(id(200)),
        ]);

        let values = project_negated(&facts, &positive, &forbidden, 0);
        assert_eq!(values.len(), 2);
        assert!(values[0].matches(&id(2)));
        assert!(values[1].matches(&id(3)));
    }

    #[test]
    fn disjunction_unions_branches_in_branch_order_and_deduplicates_projection() {
        let facts = [
            fact(1, 10, 100),
            fact(2, 10, 100),
            fact(2, 20, 200),
            fact(3, 20, 200),
        ];
        let branches = [
            Clause([Term::Variable(0), Term::Constant(id(10)), Term::Variable(1)]),
            Clause([Term::Variable(0), Term::Constant(id(20)), Term::Variable(1)]),
        ];

        let values = project_disjunction(&facts, &branches, 0);
        assert_eq!(values.len(), 3);
        assert!(values[0].matches(&id(1)));
        assert!(values[1].matches(&id(2)));
        assert!(values[2].matches(&id(3)));
    }

    #[test]
    fn recursive_edge_projection_reaches_a_finite_fixpoint_in_order() {
        let facts = [fact(1, 10, 2), fact(2, 10, 3), fact(3, 10, 4)];
        let edge = Clause([Term::Variable(0), Term::Constant(id(10)), Term::Variable(2)]);

        let values = project_recursive(&facts, &edge, &id(1), 2);
        assert_eq!(values.len(), 3);
        assert!(values[0].matches(&id(2)));
        assert!(values[1].matches(&id(3)));
        assert!(values[2].matches(&id(4)));
    }

    #[test]
    fn aggregates_preserve_distinct_numeric_and_empty_laws() {
        let values = [integer(18), integer(30), integer(40)];
        assert!(
            aggregate_values(&values, AggregateOp::Count)
                .unwrap()
                .unwrap()
                .matches(&integer(3))
        );
        assert!(
            aggregate_values(&values, AggregateOp::Sum)
                .unwrap()
                .unwrap()
                .matches(&integer(88))
        );
        assert!(
            aggregate_values(&values, AggregateOp::Min)
                .unwrap()
                .unwrap()
                .matches(&integer(18))
        );
        assert!(
            aggregate_values(&values, AggregateOp::Max)
                .unwrap()
                .unwrap()
                .matches(&integer(40))
        );
        assert!(
            aggregate_values(&values, AggregateOp::Average)
                .unwrap()
                .unwrap()
                .matches(&integer(29))
        );
        assert!(
            aggregate_values(&[], AggregateOp::Sum)
                .unwrap()
                .unwrap()
                .matches(&integer(0))
        );
        assert!(aggregate_values(&[], AggregateOp::Min).unwrap().is_none());
    }

    #[test]
    fn pull_flattens_many_references_and_preserves_spec_order() {
        let facts = [
            Triple {
                entity: id(1),
                attribute: id(10),
                value: id(2),
            },
            Triple {
                entity: id(1),
                attribute: id(10),
                value: id(3),
            },
            Triple {
                entity: id(2),
                attribute: id(20),
                value: Value::String(b"Bob".to_vec()),
            },
            Triple {
                entity: id(3),
                attribute: id(20),
                value: Value::String(b"Carol".to_vec()),
            },
        ];
        let specs = [PullSpec {
            attribute: id(10),
            kind: PullKind::Reference,
            many: true,
            children: vec![PullSpec {
                attribute: id(20),
                kind: PullKind::Leaf,
                many: false,
                children: Vec::new(),
            }],
        }];

        let records = pull(&facts, &id(1), &specs);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].path.len(), 2);
        assert!(records[0].path[0].matches(&id(10)));
        assert!(records[0].path[1].matches(&id(20)));
        assert!(records[0].value.matches(&Value::String(b"Bob".to_vec())));
        assert!(records[1].value.matches(&Value::String(b"Carol".to_vec())));
    }

    #[test]
    fn grouped_aggregate_preserves_group_order_and_distinct_values() {
        let facts = [
            Triple {
                entity: id(1),
                attribute: id(10),
                value: id(100),
            },
            Triple {
                entity: id(1),
                attribute: id(20),
                value: integer(20),
            },
            Triple {
                entity: id(2),
                attribute: id(10),
                value: id(100),
            },
            Triple {
                entity: id(2),
                attribute: id(20),
                value: integer(40),
            },
            Triple {
                entity: id(3),
                attribute: id(10),
                value: id(200),
            },
            Triple {
                entity: id(3),
                attribute: id(20),
                value: integer(30),
            },
            Triple {
                entity: id(4),
                attribute: id(10),
                value: id(200),
            },
            Triple {
                entity: id(4),
                attribute: id(20),
                value: integer(30),
            },
        ];
        let clauses = [
            Clause([Term::Variable(0), Term::Constant(id(10)), Term::Variable(1)]),
            Clause([Term::Variable(0), Term::Constant(id(20)), Term::Variable(2)]),
        ];

        let sums = group_aggregate(&facts, &clauses, 1, 2, AggregateOp::Sum).unwrap();
        assert_eq!(sums.len(), 2);
        assert!(sums[0].group.matches(&id(100)));
        assert!(sums[0].aggregate.as_ref().unwrap().matches(&integer(60)));
        assert!(sums[1].group.matches(&id(200)));
        assert!(sums[1].aggregate.as_ref().unwrap().matches(&integer(30)));

        let counts = group_aggregate(&facts, &clauses, 1, 0, AggregateOp::Count).unwrap();
        assert!(counts[0].aggregate.as_ref().unwrap().matches(&integer(2)));
        assert!(counts[1].aggregate.as_ref().unwrap().matches(&integer(2)));

        let body = vec![
            RuleClause::Triple(clauses[0].clone()),
            RuleClause::Triple(clauses[1].clone()),
        ];
        let grouped = group_rule_body(&facts, &[], &body, 1, 2, AggregateOp::Sum).unwrap();
        assert_eq!(grouped.len(), 2);
        assert!(grouped[0].aggregate.as_ref().unwrap().matches(&integer(60)));
        assert!(grouped[1].aggregate.as_ref().unwrap().matches(&integer(30)));
    }

    #[test]
    fn recursive_rules_reach_a_least_fixpoint_in_definition_order() {
        let facts = [fact(1, 10, 2), fact(2, 10, 3), fact(3, 10, 4)];
        let ancestor = id(99);
        let definitions = [
            RuleDefinition {
                name: ancestor.clone(),
                params: vec![0, 1],
                body: vec![RuleClause::Triple(Clause([
                    Term::Variable(0),
                    Term::Constant(id(10)),
                    Term::Variable(1),
                ]))],
            },
            RuleDefinition {
                name: ancestor.clone(),
                params: vec![0, 1],
                body: vec![
                    RuleClause::Triple(Clause([
                        Term::Variable(0),
                        Term::Constant(id(10)),
                        Term::Variable(2),
                    ])),
                    RuleClause::Call {
                        name: ancestor.clone(),
                        args: vec![Term::Variable(2), Term::Variable(1)],
                    },
                ],
            },
        ];
        let values = query_rule(
            &facts,
            &definitions,
            &ancestor,
            &[Term::Constant(id(1)), Term::Variable(1)],
            1,
        );
        assert_eq!(values.len(), 3);
        assert!(values[0].matches(&id(2)));
        assert!(values[1].matches(&id(3)));
        assert!(values[2].matches(&id(4)));
    }

    #[test]
    fn mutually_recursive_rules_converge_without_host_recursion() {
        let zero = id(1);
        let one = id(2);
        let two = id(3);
        let three = id(4);
        let four = id(5);
        let facts = [
            fact(1, 20, 30),
            fact(1, 30, 2),
            fact(2, 30, 3),
            fact(3, 30, 4),
            fact(4, 30, 5),
        ];
        let even = id(100);
        let odd = id(101);
        let even_defs = [
            RuleDefinition {
                name: even.clone(),
                params: vec![0],
                body: vec![RuleClause::Triple(Clause([
                    Term::Variable(0),
                    Term::Constant(id(20)),
                    Term::Constant(id(30)),
                ]))],
            },
            RuleDefinition {
                name: even.clone(),
                params: vec![0],
                body: vec![
                    RuleClause::Triple(Clause([
                        Term::Variable(1),
                        Term::Constant(id(30)),
                        Term::Variable(0),
                    ])),
                    RuleClause::Call {
                        name: odd.clone(),
                        args: vec![Term::Variable(1)],
                    },
                ],
            },
        ];
        let odd_def = RuleDefinition {
            name: odd.clone(),
            params: vec![0],
            body: vec![
                RuleClause::Triple(Clause([
                    Term::Variable(1),
                    Term::Constant(id(30)),
                    Term::Variable(0),
                ])),
                RuleClause::Call {
                    name: even.clone(),
                    args: vec![Term::Variable(1)],
                },
            ],
        };
        let mut definitions = even_defs.to_vec();
        definitions.push(odd_def);
        let values = query_rule(&facts, &definitions, &even, &[Term::Variable(0)], 0);
        assert_eq!(values.len(), 3);
        assert!(values[0].matches(&zero));
        assert!(values[1].matches(&two));
        assert!(values[2].matches(&four));
        let _ = one;
        let _ = three;
    }

    #[test]
    fn rule_cache_rebuild_discards_stale_snapshot_rows() {
        let ancestor = id(100);
        let parent = id(10);
        let definitions = [RuleDefinition {
            name: ancestor.clone(),
            params: vec![0, 1],
            body: vec![RuleClause::Triple(Clause([
                Term::Variable(0),
                Term::Constant(parent.clone()),
                Term::Variable(1),
            ]))],
        }];
        let first = [fact(1, 10, 2)];
        let second = [fact(1, 10, 3)];
        let mut cache = RuleCache::default();

        cache.rebuild(&first, &definitions);
        let first_values = cache.query(&ancestor, &[Term::Constant(id(1)), Term::Variable(1)], 1);
        assert_eq!(first_values.len(), 1);
        assert!(first_values[0].matches(&id(2)));

        cache.rebuild(&second, &definitions);
        let second_values = cache.query(&ancestor, &[Term::Constant(id(1)), Term::Variable(1)], 1);
        assert_eq!(second_values.len(), 1);
        assert!(second_values[0].matches(&id(3)));
    }

    #[test]
    fn rule_body_preserves_predicate_negation_and_disjunction_order() {
        let eligible = id(100);
        let score = id(10);
        let blocked = id(11);
        let class = id(12);
        let yes = id(20);
        let alpha = id(21);
        let beta = id(22);
        let facts = [
            Triple {
                entity: id(1),
                attribute: score.clone(),
                value: Value::Integer(20),
            },
            Triple {
                entity: id(2),
                attribute: score.clone(),
                value: Value::Integer(5),
            },
            Triple {
                entity: id(3),
                attribute: score.clone(),
                value: Value::Integer(20),
            },
            Triple {
                entity: id(4),
                attribute: score.clone(),
                value: Value::Integer(20),
            },
            fact(3, 11, 20),
            fact(1, 12, 21),
            fact(4, 12, 22),
        ];
        let definitions = [RuleDefinition {
            name: eligible.clone(),
            params: vec![0],
            body: vec![
                RuleClause::Predicate {
                    left: Term::Variable(1),
                    operation: CompareOp::Greater,
                    right: Term::Constant(Value::Integer(10)),
                },
                RuleClause::Triple(Clause([
                    Term::Variable(0),
                    Term::Constant(score),
                    Term::Variable(1),
                ])),
                RuleClause::Negated(vec![RuleClause::Triple(Clause([
                    Term::Variable(0),
                    Term::Constant(blocked),
                    Term::Constant(yes),
                ]))]),
                RuleClause::Disjunction(vec![
                    vec![RuleClause::Triple(Clause([
                        Term::Variable(0),
                        Term::Constant(class.clone()),
                        Term::Constant(alpha),
                    ]))],
                    vec![RuleClause::Triple(Clause([
                        Term::Variable(0),
                        Term::Constant(class),
                        Term::Constant(beta),
                    ]))],
                ]),
            ],
        }];
        let values = query_rule(&facts, &definitions, &eligible, &[Term::Variable(0)], 0);
        assert_eq!(values.len(), 2);
        assert!(values[0].matches(&id(1)));
        assert!(values[1].matches(&id(4)));

        let body_values = project_rule_body(&facts, &definitions, &definitions[0].body, 0);
        assert_eq!(body_values.len(), 2);
        assert!(body_values[0].matches(&id(1)));
        assert!(body_values[1].matches(&id(4)));
    }
}
