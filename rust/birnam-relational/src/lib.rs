//! Authority-free relational query kernel.
//!
//! This first R4 slice evaluates a left-to-right conjunction over a borrowed
//! Store snapshot. It owns neither facts nor IdApi objects: the caller supplies
//! the current (or historical) live triples, and receives projected bindings.
//! The dynamic module/Store callback wrapper is deliberately a later slice.

/// A copied triple view supplied by the Store callback seam.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Triple {
    pub entity: u64,
    pub attribute: u64,
    pub value: u64,
}

/// A clause term. Values are opaque Oop identities in this kernel; value-aware
/// boxed scalar matching is added at the IdApi boundary where type information
/// is available.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Term {
    Constant(u64),
    Variable(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clause(pub [Term; 3]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    pub variable: u32,
    pub value: u64,
}

fn lookup(solution: &[Binding], variable: u32) -> Option<u64> {
    solution
        .iter()
        .find(|binding| binding.variable == variable)
        .map(|binding| binding.value)
}

fn match_term(solution: &[Binding], fresh: &mut Vec<Binding>, term: Term, value: u64) -> bool {
    match term {
        Term::Constant(expected) => expected == value,
        Term::Variable(variable) => {
            let bound = lookup(solution, variable).or_else(|| lookup(fresh, variable));
            match bound {
                Some(expected) => expected == value,
                None => {
                    fresh.push(Binding { variable, value });
                    true
                }
            }
        }
    }
}

/// Evaluate a left-to-right conjunction and project one variable.
///
/// Fact order is preserved and projected values are set-like: the first
/// occurrence wins, matching the existing relational surface's deterministic
/// result order and distinct projection semantics. No query result borrows the
/// input snapshot.
pub fn project(facts: &[Triple], clauses: &[Clause], result: u32) -> Vec<u64> {
    let mut solutions: Vec<Vec<Binding>> = vec![Vec::new()];
    for Clause(terms) in clauses {
        let mut next = Vec::new();
        for solution in &solutions {
            for triple in facts {
                let values = [triple.entity, triple.attribute, triple.value];
                let mut fresh = Vec::new();
                if terms
                    .iter()
                    .zip(values)
                    .all(|(term, value)| match_term(solution, &mut fresh, *term, value))
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

    let mut result_values = Vec::new();
    for solution in solutions {
        let Some(value) = lookup(&solution, result) else {
            continue;
        };
        if !result_values.contains(&value) {
            result_values.push(value);
        }
    }
    result_values
}

#[cfg(test)]
mod tests {
    use super::{Clause, Term, Triple, project};

    const fn fact(entity: u64, attribute: u64, value: u64) -> Triple {
        Triple {
            entity,
            attribute,
            value,
        }
    }

    #[test]
    fn single_clause_projects_distinct_values_in_store_order() {
        let facts = [fact(1, 10, 100), fact(2, 10, 100), fact(3, 10, 200)];
        let clauses = [Clause([
            Term::Variable(0),
            Term::Constant(10),
            Term::Variable(1),
        ])];

        assert_eq!(project(&facts, &clauses, 1), vec![100, 200]);
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
            Clause([Term::Variable(0), Term::Constant(10), Term::Variable(1)]),
            Clause([Term::Variable(0), Term::Constant(20), Term::Variable(2)]),
        ];

        assert_eq!(project(&facts, &clauses, 2), vec![300]);
    }

    #[test]
    fn repeated_variable_within_clause_must_agree() {
        let facts = [fact(1, 10, 2), fact(2, 10, 3)];
        let clauses = [Clause([
            Term::Variable(0),
            Term::Constant(10),
            Term::Variable(0),
        ])];

        assert!(project(&facts, &clauses, 0).is_empty());
    }

    #[test]
    fn unbound_result_is_not_invented() {
        let facts = [fact(1, 10, 2)];
        let clauses = [Clause([
            Term::Constant(1),
            Term::Constant(10),
            Term::Constant(2),
        ])];

        assert!(project(&facts, &clauses, 99).is_empty());
    }
}
