use super::*;

#[test]
#[ignore = "manual before/after snapshot size matrix"]
fn snapshot_matrix() {
    use std::hint::black_box;
    use std::time::Instant;
    let runtime = super::tests::test_runtime(u64::MAX, usize::MAX);
    for shape in ["distinct", "many", "history"] {
        for n in [10, 100, 1000, 5000] {
            let mut state = StoreState::new();
            if shape == "many" {
                append_fact(
                    &mut state,
                    200 as birnam_abi::Oop,
                    as_oop(runtime.cardinality),
                    as_oop(runtime.many),
                    Op::Assert,
                    true,
                );
            }
            for index in 0..n {
                let entity = if shape == "distinct" {
                    1000 + index
                } else {
                    100
                };
                append_fact(
                    &mut state,
                    entity as birnam_abi::Oop,
                    200 as birnam_abi::Oop,
                    (300 + index) as birnam_abi::Oop,
                    Op::Assert,
                    true,
                );
            }
            for optimized in [false, true] {
                let mut samples = Vec::new();
                for _ in 0..5 {
                    let start = Instant::now();
                    let result = if optimized {
                        collect_live_triples(&runtime, black_box(&state), None, None)
                    } else {
                        reference(&runtime, black_box(&state), None, None)
                    };
                    samples.push(start.elapsed().as_nanos());
                    black_box(result);
                }
                samples.sort_unstable();
                eprintln!(
                    "shape={shape} n={n} optimized={optimized} median_ns={}",
                    samples[2]
                );
            }
        }
    }
}

fn reference(
    runtime: &Runtime,
    state: &StoreState,
    lo: Option<u64>,
    hi: Option<u64>,
) -> Vec<Triple> {
    let mut pairs: Vec<(birnam_abi::Oop, birnam_abi::Oop)> = Vec::new();
    for fact in &state.facts {
        if !visible(fact, hi)
            || pairs
                .iter()
                .any(|(entity, attribute)| same(*entity, fact.e) && same(*attribute, fact.a))
        {
            continue;
        }
        pairs.push((fact.e, fact.a));
    }

    let mut triples = Vec::new();
    for (entity, attribute) in pairs {
        let cardinality = cardinality_as_of(runtime, state, attribute, hi);
        if cardinality == as_oop(runtime.many) {
            let mut seen = Vec::new();
            for fact in state.facts.iter().rev() {
                if !same(fact.e, entity) || !same(fact.a, attribute) || !visible(fact, hi) {
                    continue;
                }
                if seen.contains(&fact.v) {
                    continue;
                }
                seen.push(fact.v);
                if fact.op == Op::Assert && in_lower_bound(fact, lo) {
                    triples.push(Triple {
                        e: entity,
                        a: attribute,
                        v: fact.v,
                    });
                }
            }
        } else {
            for fact in state.facts.iter().rev() {
                if same(fact.e, entity) && same(fact.a, attribute) && visible(fact, hi) {
                    if fact.op == Op::Assert && in_lower_bound(fact, lo) {
                        triples.push(Triple {
                            e: entity,
                            a: attribute,
                            v: fact.v,
                        });
                    }
                    break;
                }
            }
        }
    }
    triples
}
fn tuples(triples: Vec<Triple>) -> Vec<(usize, usize, usize)> {
    triples
        .into_iter()
        .map(|t| (t.e as usize, t.a as usize, t.v as usize))
        .collect()
}

#[test]
fn indexed_snapshots_match_scan_before_and_after_compaction() {
    let runtime = super::tests::test_runtime(u64::MAX, usize::MAX);
    for seed in 1..65u64 {
        let mut random = seed;
        let mut state = StoreState::new();
        for _ in 0..80 {
            random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
            let e = (100 + (random >> 8) % 7) as birnam_abi::Oop;
            let a = (200 + (random >> 16) % 3) as birnam_abi::Oop;
            let v = (300 + (random >> 24) % 4) as birnam_abi::Oop;
            let op = if random & 1 == 0 {
                Op::Assert
            } else {
                Op::Retract
            };
            if random % 5 == 0 {
                append_fact(
                    &mut state,
                    a,
                    as_oop(runtime.cardinality),
                    as_oop(if random & 2 == 0 {
                        runtime.many
                    } else {
                        runtime.one
                    }),
                    op,
                    true,
                );
            } else {
                append_fact(&mut state, e, a, v, op, random & 4 == 0);
            }
            // Deliberately make transaction and vector order disagree, like
            // current-only in-place updates. Compare even unusual histories.
            if random % 7 == 0 && state.facts.len() > 2 {
                let tx = next_tx(&mut state);
                state.facts[0].tx = tx;
                state.facts[0].v = v;
            }
        }
        for compacted in [false, true] {
            if compacted {
                state.retention = RetentionPolicy::KeepLast(20);
                compact_state(
                    &mut state,
                    as_oop(runtime.cardinality),
                    as_oop(runtime.history),
                    as_oop(runtime.true_value),
                    as_oop(runtime.one),
                    as_oop(runtime.many),
                )
                .unwrap();
            }
            for lo in [None, Some(0), Some(1), Some(20), Some(60), Some(state.tx)] {
                for hi in [None, Some(0), Some(1), Some(20), Some(60), Some(state.tx)] {
                    assert_eq!(
                        tuples(collect_live_triples(&runtime, &state, lo, hi)),
                        tuples(reference(&runtime, &state, lo, hi)),
                        "seed={seed} compacted={compacted} lo={lo:?} hi={hi:?}"
                    );
                }
            }
        }
    }
}
