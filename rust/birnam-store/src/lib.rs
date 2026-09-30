//! Store-owned fact history, projection, retention and snapshot algorithms.
use birnam_abi::Oop;
use core::{mem::size_of, ptr};
const DEFAULT_COMPACTION_TRANSITIONS: u64 = 4096;
const DEFAULT_FACT_BYTES_LIMIT: usize = 64 * 1024 * 1024;
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum Op {
    Assert = 0,
    Retract = 1,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct Fact {
    e: birnam_abi::Oop,
    a: birnam_abi::Oop,
    v: birnam_abi::Oop,
    tx: u64,
    op: Op,
    historied: bool,
    mirror: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RetentionPolicy {
    CurrentOnly,
    KeepSince(u64),
    KeepLast(u64),
}

#[repr(C)]
struct StoreState {
    facts: Vec<Fact>,
    compensation_reservations: Vec<CompensationReservation>,
    raw: bool,
    tx: u64,
    oldest_retained_basis: u64,
    retention: RetentionPolicy,
    transitions_since_compaction: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CompensationReservation {
    owner: usize,
    facts: usize,
}

impl StoreState {
    fn new() -> Self {
        Self {
            facts: Vec::new(),
            compensation_reservations: Vec::new(),
            raw: true,
            tx: 0,
            oldest_retained_basis: 0,
            retention: RetentionPolicy::CurrentOnly,
            transitions_since_compaction: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct Runtime {
    cardinality: usize,
    one: usize,
    many: usize,
    history: usize,
    true_value: usize,
    compaction_transitions: u64,
    fact_bytes_limit: usize,
}
fn abort(message: &str) -> ! {
    eprintln!("birnam Rust Store: {message}");
    std::process::abort();
}

fn positive_config(name: &str, default: usize) -> usize {
    match std::env::var(name) {
        Ok(value) => value
            .parse::<usize>()
            .ok()
            .filter(|value| *value > 0)
            .unwrap_or_else(|| abort(&format!("{name} must be a positive integer"))),
        Err(std::env::VarError::NotPresent) => default,
        Err(std::env::VarError::NotUnicode(_)) => abort(&format!("{name} was not valid UTF-8")),
    }
}

fn as_oop(value: usize) -> birnam_abi::Oop {
    value as birnam_abi::Oop
}

fn same(a: birnam_abi::Oop, b: birnam_abi::Oop) -> bool {
    a == b
}

fn next_tx(state: &mut StoreState) -> u64 {
    let tx = state.tx;
    state.tx = state
        .tx
        .checked_add(1)
        .filter(|next| *next <= i64::MAX as u64)
        .unwrap_or_else(|| abort("Store durable basis exhausted SmallInt range"));
    state.transitions_since_compaction = state.transitions_since_compaction.saturating_add(1);
    tx
}

fn append_fact(
    state: &mut StoreState,
    e: birnam_abi::Oop,
    a: birnam_abi::Oop,
    v: birnam_abi::Oop,
    op: Op,
    historied: bool,
) {
    let tx = next_tx(state);
    state.facts.push(Fact {
        e,
        a,
        v,
        tx,
        op,
        historied,
        mirror: false,
    });
}

fn current_value_raw_as_of(
    state: &StoreState,
    e: birnam_abi::Oop,
    a: birnam_abi::Oop,
    basis: Option<u64>,
) -> birnam_abi::Oop {
    state
        .facts
        .iter()
        .rev()
        .find(|fact| {
            same(fact.e, e) && same(fact.a, a) && basis.is_none_or(|watermark| fact.tx < watermark)
        })
        .map_or(ptr::null_mut(), |fact| {
            if fact.op == Op::Assert {
                fact.v
            } else {
                ptr::null_mut()
            }
        })
}

fn current_value_raw(
    state: &StoreState,
    e: birnam_abi::Oop,
    a: birnam_abi::Oop,
) -> birnam_abi::Oop {
    current_value_raw_as_of(state, e, a, None)
}

fn count_values(state: &StoreState, e: birnam_abi::Oop, a: birnam_abi::Oop) -> usize {
    let mut seen = Vec::new();
    let mut count = 0;
    for fact in state.facts.iter().rev() {
        if !same(fact.e, e) || !same(fact.a, a) || seen.contains(&fact.v) {
            continue;
        }
        seen.push(fact.v);
        if fact.op == Op::Assert {
            count += 1;
        }
    }
    count
}

fn cardinality(runtime: &Runtime, state: &StoreState, attribute: birnam_abi::Oop) -> usize {
    let value = current_value_raw(state, attribute, as_oop(runtime.cardinality));
    if value.is_null() {
        runtime.one
    } else {
        value as usize
    }
}

fn history_enabled(runtime: &Runtime, state: &StoreState, attribute: birnam_abi::Oop) -> bool {
    current_value_raw(state, attribute, as_oop(runtime.history)) == as_oop(runtime.true_value)
}

fn is_schema_fact(runtime: &Runtime, attribute: birnam_abi::Oop) -> bool {
    attribute == as_oop(runtime.cardinality) || attribute == as_oop(runtime.history)
}

fn is_member(
    state: &StoreState,
    e: birnam_abi::Oop,
    a: birnam_abi::Oop,
    v: birnam_abi::Oop,
) -> bool {
    state
        .facts
        .iter()
        .rev()
        .find(|fact| same(fact.e, e) && same(fact.a, a) && same(fact.v, v))
        .is_some_and(|fact| fact.op == Op::Assert)
}

fn record_projection_one(
    runtime: &Runtime,
    state: &mut StoreState,
    e: birnam_abi::Oop,
    a: birnam_abi::Oop,
    v: birnam_abi::Oop,
    op: Op,
) -> Result<(), CompactError> {
    prepare_semantic_mutation(runtime, state, false)?;
    if let Some(index) = state
        .facts
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, fact)| (same(fact.e, e) && same(fact.a, a)).then_some(index))
    {
        if !state.facts[index].historied {
            let tx = next_tx(state);
            let fact = &mut state.facts[index];
            fact.v = v;
            fact.op = op;
            fact.tx = tx;
            return Ok(());
        }
    }
    prepare_semantic_mutation(runtime, state, true)?;
    append_fact(state, e, a, v, op, false);
    Ok(())
}

fn record_projection_many(
    runtime: &Runtime,
    state: &mut StoreState,
    e: birnam_abi::Oop,
    a: birnam_abi::Oop,
    v: birnam_abi::Oop,
    op: Op,
) -> Result<(), CompactError> {
    prepare_semantic_mutation(runtime, state, false)?;
    if let Some(index) = state
        .facts
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, fact)| {
            (same(fact.e, e) && same(fact.a, a) && same(fact.v, v)).then_some(index)
        })
    {
        if !state.facts[index].historied {
            let tx = next_tx(state);
            let fact = &mut state.facts[index];
            fact.op = op;
            fact.tx = tx;
            return Ok(());
        }
    }
    prepare_semantic_mutation(runtime, state, true)?;
    append_fact(state, e, a, v, op, false);
    Ok(())
}

fn remove_mirror_facts(
    state: &mut StoreState,
    e: birnam_abi::Oop,
    a: birnam_abi::Oop,
    v: birnam_abi::Oop,
) {
    state
        .facts
        .retain(|fact| !(same(fact.e, e) && same(fact.a, a) && same(fact.v, v)));
}

fn append_mirror_fact(
    state: &mut StoreState,
    e: birnam_abi::Oop,
    a: birnam_abi::Oop,
    v: birnam_abi::Oop,
) {
    let tx = next_tx(state);
    state.facts.push(Fact {
        e,
        a,
        v,
        tx,
        op: Op::Assert,
        historied: false,
        mirror: true,
    });
}

fn durable_image_fact(fact: &Fact, transient_attributes: &[birnam_abi::Oop]) -> bool {
    !fact.mirror && !transient_attributes.contains(&fact.a)
}

fn live_attributes(
    state: &StoreState,
    entity: birnam_abi::Oop,
    runtime: &Runtime,
) -> Vec<birnam_abi::Oop> {
    let mut attributes = Vec::new();
    for fact in &state.facts {
        if !same(fact.e, entity) || attributes.contains(&fact.a) {
            continue;
        }
        attributes.push(fact.a);
    }
    attributes.retain(|attribute| {
        if cardinality(runtime, state, *attribute) == runtime.many {
            count_values(state, entity, *attribute) != 0
        } else {
            !current_value_raw(state, entity, *attribute).is_null()
        }
    });
    attributes
}

fn relation_history_available(
    state: &StoreState,
    entity: birnam_abi::Oop,
    attribute: birnam_abi::Oop,
    basis: u64,
    history_attribute: birnam_abi::Oop,
    true_value: birnam_abi::Oop,
) -> bool {
    if basis < state.oldest_retained_basis {
        return false;
    }
    if basis >= state.tx {
        return true;
    }
    for (index, fact) in state.facts.iter().enumerate() {
        if fact.historied
            || !same(fact.e, entity)
            || !same(fact.a, attribute)
            || fact.tx < state.oldest_retained_basis
        {
            continue;
        }
        let recovery = state.facts.iter().skip(index + 1).find(|later| {
            (later.historied && same(later.e, entity) && same(later.a, attribute))
                || (later.historied
                    && same(later.e, attribute)
                    && same(later.a, history_attribute)
                    && same(later.v, true_value)
                    && later.op == Op::Assert)
        });
        if recovery.is_none_or(|later| basis <= later.tx) {
            return false;
        }
    }
    true
}

#[derive(Clone, Copy)]
struct Triple {
    e: birnam_abi::Oop,
    a: birnam_abi::Oop,
    v: birnam_abi::Oop,
}

fn visible(fact: &Fact, hi: Option<u64>) -> bool {
    hi.is_none_or(|watermark| fact.tx < watermark)
}

fn in_lower_bound(fact: &Fact, lo: Option<u64>) -> bool {
    lo.is_none_or(|watermark| fact.tx >= watermark)
}

#[cfg(test)]
fn cardinality_as_of(
    runtime: &Runtime,
    state: &StoreState,
    attribute: birnam_abi::Oop,
    hi: Option<u64>,
) -> birnam_abi::Oop {
    let value = current_value_raw_as_of(state, attribute, as_oop(runtime.cardinality), hi);
    if value.is_null() {
        as_oop(runtime.one)
    } else {
        value
    }
}

fn cardinality_as_of_values(
    state: &StoreState,
    attribute: birnam_abi::Oop,
    hi: Option<u64>,
    cardinality_attribute: birnam_abi::Oop,
    one: birnam_abi::Oop,
) -> birnam_abi::Oop {
    let value = current_value_raw_as_of(state, attribute, cardinality_attribute, hi);
    if value.is_null() { one } else { value }
}

fn same_stream(a: &Fact, b: &Fact, many: bool) -> bool {
    same(a.e, b.e) && same(a.a, b.a) && (!many || same(a.v, b.v))
}

fn latest_stream_index(state: &StoreState, candidate: usize, hi: Option<u64>, many: bool) -> bool {
    let fact = &state.facts[candidate];
    !state
        .facts
        .iter()
        .enumerate()
        .skip(candidate + 1)
        .any(|(_, later)| visible(later, hi) && same_stream(fact, later, many))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CompactError {
    Allocation,
}

fn requested_retention_floor(state: &StoreState) -> u64 {
    let requested = match state.retention {
        RetentionPolicy::CurrentOnly => state.tx,
        RetentionPolicy::KeepSince(basis) => basis.min(state.tx),
        RetentionPolicy::KeepLast(count) => state.tx.saturating_sub(count),
    };
    requested.max(state.oldest_retained_basis)
}

fn compact_state(
    state: &mut StoreState,
    cardinality_attribute: birnam_abi::Oop,
    history_attribute: birnam_abi::Oop,
    true_value: birnam_abi::Oop,
    one: birnam_abi::Oop,
    many_value: birnam_abi::Oop,
) -> Result<usize, CompactError> {
    let floor = requested_retention_floor(state);
    let mut keep = Vec::new();
    keep.try_reserve_exact(state.facts.len())
        .map_err(|_| CompactError::Allocation)?;
    keep.resize(state.facts.len(), false);

    // Retained history is prospective: only transitions committed while the
    // relation was historied belong to the exact suffix.
    for (index, fact) in state.facts.iter().enumerate() {
        if fact.mirror || (fact.historied && fact.tx >= floor) {
            keep[index] = true;
        }
    }

    // A history-disabled interval remains an explicit unavailable interval.
    // Preserve its final projection until a retained history-enable boundary
    // or later historied transition makes that projection an exact baseline.
    for (index, fact) in state.facts.iter().enumerate() {
        if fact.historied || fact.tx < floor {
            continue;
        }
        let has_retained_recovery = state.facts.iter().enumerate().any(|(later_index, later)| {
            later_index > index
                && keep[later_index]
                && (same(later.e, fact.e) && same(later.a, fact.a)
                    || (same(later.e, fact.a)
                        && same(later.a, history_attribute)
                        && same(later.v, true_value)
                        && later.op == Op::Assert))
        });
        if has_retained_recovery {
            keep[index] = true;
        }
    }

    // Seed the first retained basis with the state immediately before it.
    // An absent stream needs no tombstone because no older assertion survives.
    for (index, fact) in state.facts.iter().enumerate() {
        if fact.tx >= floor || fact.op != Op::Assert {
            continue;
        }
        let cardinality =
            cardinality_as_of_values(state, fact.a, Some(floor), cardinality_attribute, one);
        let is_many = cardinality == many_value;
        if latest_stream_index(state, index, Some(floor), is_many) {
            keep[index] = true;
        }
    }

    // Preserve the current projection. A tombstone is only necessary when it
    // shadows an assertion retained by the two passes above.
    for (index, fact) in state.facts.iter().enumerate() {
        let cardinality = cardinality_as_of_values(state, fact.a, None, cardinality_attribute, one);
        let is_many = cardinality == many_value;
        if !latest_stream_index(state, index, None, is_many) {
            continue;
        }
        if fact.op == Op::Assert
            || state.facts.iter().enumerate().any(|(prior_index, prior)| {
                prior_index < index
                    && keep[prior_index]
                    && prior.op == Op::Assert
                    && same_stream(fact, prior, is_many)
            })
        {
            keep[index] = true;
        }
    }

    let retained = keep.iter().filter(|selected| **selected).count();
    let mut replacement = Vec::new();
    replacement
        .try_reserve_exact(retained)
        .map_err(|_| CompactError::Allocation)?;
    replacement.extend(
        state
            .facts
            .iter()
            .zip(keep)
            .filter_map(|(fact, selected)| selected.then_some(*fact)),
    );

    let dropped = state.facts.len() - replacement.len();
    state.facts = replacement;
    state.oldest_retained_basis = floor;
    state.transitions_since_compaction = 0;
    Ok(dropped)
}

fn semantic_fact_count(state: &StoreState) -> usize {
    state.facts.iter().filter(|fact| !fact.mirror).count()
}

fn reserved_compensation_fact_count(state: &StoreState) -> usize {
    state
        .compensation_reservations
        .iter()
        .fold(0usize, |total, reservation| {
            total.saturating_add(reservation.facts)
        })
}

fn prepare_fact_capacity(
    runtime: &Runtime,
    state: &mut StoreState,
    additional_reserved: usize,
    pending_appends: usize,
) -> Result<(), CompactError> {
    let max_facts = runtime.fact_bytes_limit / size_of::<Fact>();
    let requested = semantic_fact_count(state)
        .saturating_add(reserved_compensation_fact_count(state))
        .saturating_add(additional_reserved)
        .saturating_add(pending_appends);
    let at_growth_gate = state.transitions_since_compaction >= runtime.compaction_transitions;
    if at_growth_gate || requested > max_facts {
        compact_state(
            state,
            as_oop(runtime.cardinality),
            as_oop(runtime.history),
            as_oop(runtime.true_value),
            as_oop(runtime.one),
            as_oop(runtime.many),
        )?;
    }
    let requested = semantic_fact_count(state)
        .saturating_add(reserved_compensation_fact_count(state))
        .saturating_add(additional_reserved)
        .saturating_add(pending_appends);
    (requested <= max_facts)
        .then_some(())
        .ok_or(CompactError::Allocation)
}

fn prepare_semantic_mutation(
    runtime: &Runtime,
    state: &mut StoreState,
    needs_append: bool,
) -> Result<(), CompactError> {
    prepare_fact_capacity(runtime, state, 0, usize::from(needs_append))
}

fn reserve_compensation_in(
    runtime: &Runtime,
    state: &mut StoreState,
    owner: usize,
    facts: usize,
) -> Result<(), CompactError> {
    if owner == 0 || facts == 0 {
        return Err(CompactError::Allocation);
    }
    // A reservation is made immediately before its candidate-owned write. In
    // addition to the compensation records, retain capacity for that forward
    // transition itself.
    prepare_fact_capacity(runtime, state, facts, 1)?;
    if let Some(reservation) = state
        .compensation_reservations
        .iter_mut()
        .find(|reservation| reservation.owner == owner)
    {
        let Some(total) = reservation.facts.checked_add(facts) else {
            return Err(CompactError::Allocation);
        };
        reservation.facts = total;
    } else {
        state
            .compensation_reservations
            .try_reserve(1)
            .map_err(|_| CompactError::Allocation)?;
        state
            .compensation_reservations
            .push(CompensationReservation { owner, facts });
    }
    Ok(())
}

fn consume_compensation_in(state: &mut StoreState, owner: usize, facts: usize) -> bool {
    if owner == 0 || facts == 0 {
        return false;
    }
    let Some(index) = state
        .compensation_reservations
        .iter()
        .position(|reservation| reservation.owner == owner)
    else {
        return false;
    };
    if state.compensation_reservations[index].facts < facts {
        return false;
    }
    state.compensation_reservations[index].facts -= facts;
    if state.compensation_reservations[index].facts == 0 {
        state.compensation_reservations.swap_remove(index);
    }
    true
}

fn collect_live_triples(
    runtime: &Runtime,
    state: &StoreState,
    lo: Option<u64>,
    hi: Option<u64>,
) -> Vec<Triple> {
    // All indices are invocation-local and gone before the first foreign send.
    // Physical order matters: non-historied facts can be updated in place.
    let mut by_pair = std::collections::HashMap::new();
    let mut groups: Vec<Vec<&Fact>> = Vec::new();
    let mut previous = None;
    for fact in &state.facts {
        if !visible(fact, hi) {
            continue;
        }
        let pair = (fact.e, fact.a);
        let index = match previous {
            Some((key, index)) if key == pair => index,
            _ => {
                let next = groups.len();
                let index = *by_pair.entry(pair).or_insert_with(|| {
                    groups.push(Vec::new());
                    next
                });
                previous = Some((pair, index));
                index
            }
        };
        groups[index].push(fact);
    }

    let mut triples = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for group in &groups {
        let last = group.last().expect("nonempty visible fact group");
        let many = by_pair
            .get(&(last.a, as_oop(runtime.cardinality)))
            .and_then(|index| groups[*index].last())
            .is_some_and(|schema| schema.op == Op::Assert && schema.v == as_oop(runtime.many));
        seen.clear();
        for fact in group.iter().rev() {
            if many && !seen.insert(fact.v) {
                continue;
            }
            if fact.op == Op::Assert && in_lower_bound(fact, lo) {
                triples.push(Triple {
                    e: fact.e,
                    a: fact.a,
                    v: fact.v,
                });
            }
            if !many {
                break;
            }
        }
    }
    triples
}

#[cfg(test)]
mod tests {
    use super::*;
    use birnam_abi::Oop;
    use core::ptr;

    fn oop(value: usize) -> Oop {
        value as Oop
    }

    const CARDINALITY: usize = 10;
    const HISTORY: usize = 11;
    const ONE: usize = 12;
    const MANY: usize = 13;
    const TRUE: usize = 14;

    fn compact(state: &mut StoreState) -> usize {
        compact_state(
            state,
            oop(CARDINALITY),
            oop(HISTORY),
            oop(TRUE),
            oop(ONE),
            oop(MANY),
        )
        .unwrap()
    }

    pub(super) fn test_runtime(compaction_transitions: u64, fact_bytes_limit: usize) -> Runtime {
        Runtime {
            cardinality: CARDINALITY,
            one: ONE,
            many: MANY,
            history: HISTORY,
            true_value: TRUE,
            compaction_transitions,
            fact_bytes_limit,
        }
    }

    #[test]
    #[ignore = "manual Store scaling investigation; no timing thresholds"]
    fn snapshot_scaling_investigation() {
        use std::hint::black_box;
        use std::time::Instant;
        let runtime = test_runtime(u64::MAX, usize::MAX);
        for count in [100, 1000, 5000] {
            let mut state = StoreState::new();
            for index in 0..count {
                append_fact(
                    &mut state,
                    oop(100 + index),
                    oop(20),
                    oop(30),
                    Op::Assert,
                    true,
                );
            }
            let mut samples = Vec::new();
            for _ in 0..5 {
                let start = Instant::now();
                let triples = super::collect_live_triples(&runtime, black_box(&state), None, None);
                samples.push(start.elapsed().as_nanos());
                assert_eq!(triples.len(), count);
                for (index, triple) in triples.iter().enumerate() {
                    assert_eq!(triple.e, oop(100 + index));
                }
                black_box(triples);
            }
            samples.sort_unstable();
            eprintln!(
                "snapshot distinct_pairs={count} median_ns={} legacy_pair_comparisons={} legacy_fact_search_visits={} legacy_schema_search_visits={}",
                samples[2],
                count * (count - 1) / 2,
                count * (count + 1) / 2,
                count * count
            );
        }
    }

    #[test]
    fn nonhistoried_projection_shadows_but_does_not_destroy_predecessor() {
        let mut state = StoreState::new();
        let runtime = test_runtime(u64::MAX, usize::MAX);
        append_fact(&mut state, oop(20), oop(21), oop(30), Op::Assert, true);
        record_projection_one(&runtime, &mut state, oop(20), oop(21), oop(31), Op::Assert).unwrap();
        record_projection_one(&runtime, &mut state, oop(20), oop(21), oop(32), Op::Assert).unwrap();

        assert_eq!(state.facts.len(), 2);
        assert!(state.facts[0].historied);
        assert!(!state.facts[1].historied);
        assert_eq!(state.facts[1].v, oop(32));
        assert_eq!(state.facts[1].tx, 2);
        assert_eq!(
            current_value_raw_as_of(&state, oop(20), oop(21), None),
            oop(32)
        );
    }

    #[test]
    fn current_only_compaction_keeps_only_current_asserted_projection() {
        let mut state = StoreState::new();
        append_fact(&mut state, oop(20), oop(21), oop(30), Op::Assert, true);
        append_fact(&mut state, oop(20), oop(21), oop(31), Op::Assert, true);
        append_fact(&mut state, oop(22), oop(21), oop(32), Op::Assert, true);
        append_fact(&mut state, oop(22), oop(21), oop(32), Op::Retract, true);

        assert_eq!(compact(&mut state), 3);
        assert_eq!(state.oldest_retained_basis, 4);
        assert_eq!(state.facts.len(), 1);
        assert_eq!(state.facts[0].v, oop(31));
        assert_eq!(
            current_value_raw_as_of(&state, oop(22), oop(21), None),
            ptr::null_mut()
        );
    }

    #[test]
    fn keep_since_preserves_baseline_and_exact_historied_suffix() {
        let mut state = StoreState::new();
        append_fact(&mut state, oop(20), oop(21), oop(30), Op::Assert, true);
        append_fact(&mut state, oop(20), oop(21), oop(31), Op::Assert, true);
        append_fact(&mut state, oop(20), oop(21), oop(31), Op::Retract, true);
        append_fact(&mut state, oop(20), oop(21), oop(32), Op::Assert, true);
        state.retention = RetentionPolicy::KeepSince(2);

        assert_eq!(compact(&mut state), 1);
        assert_eq!(state.oldest_retained_basis, 2);
        assert_eq!(
            state.facts.iter().map(|fact| fact.tx).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(
            current_value_raw_as_of(&state, oop(20), oop(21), Some(2)),
            oop(31)
        );
        assert_eq!(
            current_value_raw_as_of(&state, oop(20), oop(21), Some(3)),
            ptr::null_mut()
        );
        assert_eq!(
            current_value_raw_as_of(&state, oop(20), oop(21), Some(4)),
            oop(32)
        );
    }

    #[test]
    fn prospective_history_keeps_an_unavailable_gap_until_reenabled() {
        let mut state = StoreState::new();
        append_fact(&mut state, oop(20), oop(21), oop(30), Op::Assert, true);
        append_fact(&mut state, oop(20), oop(21), oop(31), Op::Assert, false);
        append_fact(
            &mut state,
            oop(21),
            oop(HISTORY),
            oop(TRUE),
            Op::Assert,
            true,
        );
        append_fact(&mut state, oop(20), oop(21), oop(32), Op::Assert, true);
        state.retention = RetentionPolicy::KeepSince(0);

        assert_eq!(compact(&mut state), 0);
        assert!(!relation_history_available(
            &state,
            oop(20),
            oop(21),
            2,
            oop(HISTORY),
            oop(TRUE)
        ));
        assert!(relation_history_available(
            &state,
            oop(20),
            oop(21),
            3,
            oop(HISTORY),
            oop(TRUE)
        ));
    }

    #[test]
    fn fact_carries_commit_time_history_status() {
        assert!(core::mem::size_of::<Fact>() >= core::mem::size_of::<[usize; 3]>() + 10);
    }

    #[test]
    fn method_dictionary_mirrors_are_explicitly_nondurable() {
        let semantic = Fact {
            e: oop(20),
            a: oop(21),
            v: oop(30),
            tx: 0,
            op: Op::Assert,
            historied: false,
            mirror: false,
        };
        let mirror = Fact {
            mirror: true,
            ..semantic
        };
        assert!(super::durable_image_fact(&semantic, &[]));
        assert!(!super::durable_image_fact(&mirror, &[]));
        assert!(!super::durable_image_fact(&semantic, &[oop(21)]));
    }

    #[test]
    fn growth_gate_keeps_current_only_history_at_a_plateau() {
        let mut state = StoreState::new();
        let runtime = test_runtime(8, core::mem::size_of::<Fact>() * 32);
        for value in 0..10_000usize {
            prepare_semantic_mutation(&runtime, &mut state, true).unwrap();
            append_fact(
                &mut state,
                oop(20),
                oop(21),
                oop(100 + value),
                Op::Assert,
                true,
            );
        }
        // One baseline plus at most one trigger window of transitions.
        assert!(
            state.facts.len() <= 9,
            "retained {} facts",
            state.facts.len()
        );
        assert!(state.oldest_retained_basis >= 9_992);
        assert_eq!(
            current_value_raw_as_of(&state, oop(20), oop(21), None),
            oop(10_099)
        );
    }

    #[test]
    fn fixed_retention_floor_rejects_before_crossing_fact_ceiling() {
        let mut state = StoreState::new();
        let runtime = test_runtime(u64::MAX, core::mem::size_of::<Fact>() * 4);
        state.retention = RetentionPolicy::KeepSince(0);
        for value in 0..4usize {
            prepare_semantic_mutation(&runtime, &mut state, true).unwrap();
            append_fact(
                &mut state,
                oop(20),
                oop(21),
                oop(100 + value),
                Op::Assert,
                true,
            );
        }
        let basis = state.tx;
        assert!(prepare_semantic_mutation(&runtime, &mut state, true).is_err());
        assert_eq!(state.tx, basis);
        assert_eq!(state.facts.len(), 4);
    }

    #[test]
    fn compensation_reservation_charges_rollback_and_forward_capacity() {
        let mut state = StoreState::new();
        let runtime = test_runtime(u64::MAX, core::mem::size_of::<Fact>() * 4);
        state.retention = RetentionPolicy::KeepSince(0);
        append_fact(&mut state, oop(20), oop(21), oop(30), Op::Assert, true);

        reserve_compensation_in(&runtime, &mut state, 7, 2).unwrap();
        assert_eq!(reserved_compensation_fact_count(&state), 2);
        prepare_semantic_mutation(&runtime, &mut state, true).unwrap();
        append_fact(&mut state, oop(20), oop(21), oop(31), Op::Assert, true);

        // Two retained facts plus two rollback slots exhaust the four-fact
        // ceiling. A second candidate-owned forward write cannot begin.
        assert!(reserve_compensation_in(&runtime, &mut state, 7, 2).is_err());
        assert_eq!(reserved_compensation_fact_count(&state), 2);

        assert!(consume_compensation_in(&mut state, 7, 1));
        prepare_semantic_mutation(&runtime, &mut state, true).unwrap();
        append_fact(&mut state, oop(20), oop(21), oop(30), Op::Assert, true);
        assert_eq!(reserved_compensation_fact_count(&state), 1);
    }
}

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod snapshot_tests;

mod native;
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
