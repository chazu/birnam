//! Relational evaluation over caller-supplied Store snapshots.
//! The query kernel is unchanged; the production adapter uses Id messages.

mod evaluator;
pub use evaluator::{
    AggregateError, AggregateOp, Binding, Clause, CompareOp, GroupedAggregate, PULL_MAX_DEPTH,
    PullKind, PullRecord, PullSpec, RuleCache, RuleClause, RuleDefinition, Term, Triple, Value,
    aggregate_values, group_aggregate, group_rule_body, materialize_rules, project,
    project_disjunction, project_negated, project_predicate, project_recursive, project_rule_body,
    pull, query_rule,
};

mod native;

#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> core::ffi::c_int {
    1
}
