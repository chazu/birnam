//! Relational evaluation over caller-supplied Store snapshots.
//! Pure evaluation is separate from native representation and Store callbacks.

pub mod oop;
mod evaluator;
pub use evaluator::{
    PULL_MAX_DEPTH,
    AggregateError,
    AggregateOp,
    Binding,
    Clause,
    CompareOp,
    GroupedAggregate,
    PullKind,
    PullRecord,
    PullSpec,
    RuleCache,
    RuleClause,
    RuleDefinition,
    Term,
    Triple,
    Value,
    aggregate_values,
    group_aggregate,
    group_rule_body,
    materialize_rules,
    project,
    project_disjunction,
    project_negated,
    project_predicate,
    project_recursive,
    project_rule_body,
    pull,
    query_rule,
};

mod adapter;
pub use adapter::{
    ClassifiedTriple,
    ClassifiedValue,
    GroupedPattern,
    GroupedRecord,
    ProjectPattern,
    PullSpecNode,
    PulledRecord,
    RuleBodyNode,
    RuleClauseNode,
    RuleDefinitionNode,
    RuleTermNode,
    RuleTreeDefinitionNode,
    birnam_native_field_projection_v1,
    birnam_relational_rust_aggregate_live,
    birnam_relational_rust_classify,
    birnam_relational_rust_collect_live,
    birnam_relational_rust_collect_window,
    birnam_relational_rust_group_aggregate_live,
    birnam_relational_rust_identity,
    birnam_relational_rust_project_disjunction_live,
    birnam_relational_rust_project_join_live,
    birnam_relational_rust_project_live,
    birnam_relational_rust_project_negated_live,
    birnam_relational_rust_project_predicate_live,
    birnam_relational_rust_project_recursive_live,
    birnam_relational_rust_project_rule_live,
    birnam_relational_rust_project_rule_store_live,
    birnam_relational_rust_project_rule_tree_live,
    birnam_relational_rust_project_window_live,
    birnam_relational_rust_pull_live,
    birnam_symbol_borrow_v1,
    module_init,
};

birnam_abi::integer_dependency!();

birnam_abi::float_dependency!();

birnam_abi::string_dependency!();
