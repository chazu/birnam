//! Differential semantics and thread-local allocation evidence for matching.
use super::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct CountingAllocator;

thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}

fn record_allocation() {
    let _ = ALLOCATIONS.try_with(|count| {
        if let Some(value) = count.get() {
            count.set(Some(value + 1));
        }
    });
}

// SAFETY: every allocation operation is forwarded unchanged to System. Only
// the calling test thread's allocation count is observed; no Oops are retained.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record_allocation();
        unsafe { System.realloc(pointer, layout, size) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn allocations<T>(run: impl FnOnce() -> T) -> (T, usize) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            ALLOCATIONS.with(|count| count.set(None));
        }
    }
    ALLOCATIONS.with(|count| {
        assert!(count.get().is_none());
        count.set(Some(0));
    });
    let reset = Reset;
    let result = run();
    let count = ALLOCATIONS.with(|count| count.get().unwrap());
    drop(reset);
    (result, count)
}

// Frozen pre-change matcher, deliberately retaining eager copies as an oracle.
fn reference(solution: &[Binding], terms: &[Term], values: &[Value]) -> Option<Vec<Binding>> {
    if terms.len() != values.len() {
        return None;
    }
    let mut extended = solution.to_vec();
    for (term, value) in terms.iter().zip(values) {
        match term {
            Term::Constant(expected) if !expected.matches(value) => return None,
            Term::Constant(_) => {}
            Term::Variable(variable) => {
                if let Some(existing) = lookup(&extended, *variable) {
                    if !existing.matches(value) {
                        return None;
                    }
                } else {
                    extended.push(Binding {
                        variable: *variable,
                        value: value.clone(),
                    });
                }
            }
        }
    }
    Some(extended)
}

#[test]
fn matching_preserves_eager_reference_semantics() {
    let values = [
        Value::Identity(1),
        Value::Integer(1),
        Value::Float(1.0),
        Value::Float(0.0),
        Value::Float(-0.0),
        Value::Float(f64::NAN),
        Value::String(b"same".to_vec()),
        Value::String(b"other".to_vec()),
    ];
    let term_options = [
        Term::Variable(0),
        Term::Variable(1),
        Term::Variable(2),
        Term::Constant(Value::String(b"same".to_vec())),
    ];
    for initial in [
        vec![],
        vec![Binding {
            variable: 0,
            value: values[6].clone(),
        }],
        vec![Binding {
            variable: 1,
            value: Value::Float(f64::NAN),
        }],
    ] {
        for a in &term_options {
            for b in &term_options {
                for c in &term_options {
                    let terms = [a.clone(), b.clone(), c.clone()];
                    for x in &values {
                        for y in &values {
                            for z in &values {
                                let candidate = [x.clone(), y.clone(), z.clone()];
                                let expected = reference(&initial, &terms, &candidate);
                                let actual = extend_rule_solution(&initial, &terms, &candidate);
                                let refs = [&candidate[0], &candidate[1], &candidate[2]];
                                let borrowed = extend_rule_solution(&initial, &terms, &refs);
                                // Debug preserves NaN, signed zero, binding order and variants;
                                // Value::matches intentionally does not equate NaN with itself.
                                assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
                                assert_eq!(format!("{borrowed:?}"), format!("{expected:?}"));
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(extend_rule_solution(&[], &[Term::Variable(0)], &[] as &[Value]).is_none());
    for constant in &values {
        for value in &values {
            let terms = [Term::Constant(constant.clone())];
            let candidate = [value.clone()];
            assert_eq!(
                extend_rule_solution(&[], &terms, &candidate).is_some(),
                reference(&[], &terms, &candidate).is_some()
            );
        }
    }
}

#[test]
fn rejected_query_candidates_allocate_nothing() {
    let initial = vec![Binding {
        variable: 0,
        value: Value::String(vec![b'x'; 256]),
    }];
    let terms = [
        Term::Variable(1),
        Term::Constant(Value::Identity(99)),
        Term::Variable(2),
    ];
    let facts: Vec<_> = (0..1000)
        .map(|i| Triple {
            entity: Value::Identity(i),
            attribute: Value::Identity(1),
            value: Value::String(vec![b'y'; 256]),
        })
        .collect();
    let (_, before) = allocations(|| {
        for fact in &facts {
            let owned = vec![
                fact.entity.clone(),
                fact.attribute.clone(),
                fact.value.clone(),
            ];
            assert!(reference(&initial, &terms, &owned).is_none());
        }
    });
    let body = [RuleClause::Triple(Clause(terms))];
    let solutions = vec![initial];
    let (_, fixed) = allocations(|| plan_rule_body(&body));
    let (result, after) =
        allocations(|| evaluate_rule_body_from_solutions(&facts, &[], &body, solutions));
    assert!(result.is_empty());
    eprintln!(
        "1000 rejected string triples: eager candidate allocations={before}, current total={after}, fixed planner allocations={fixed}"
    );
    assert!(before >= facts.len());
    assert_eq!(
        after, fixed,
        "candidate matching must not allocate for rejected rows"
    );
}

#[test]
fn borrowed_matching_handles_wide_rows_and_owns_results() {
    let mut values: Vec<Value> = (0..64).map(|i| Value::String(vec![i; 16])).collect();
    let mut terms: Vec<Term> = (0..64).map(Term::Variable).collect();
    terms.push(Term::Variable(7));
    values.push(values[7].clone());
    let expected = reference(&[], &terms, &values);
    let borrowed: Vec<&Value> = values.iter().collect();
    let actual = extend_rule_solution(&[], &terms, &borrowed);
    assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
    drop(borrowed);
    values[0] = Value::Identity(999);
    assert!(
        actual.unwrap()[0]
            .value
            .matches(&Value::String(vec![0; 16]))
    );
    values[64] = Value::Identity(999);
    let (rejected, count) = allocations(|| extend_rule_solution(&[], &terms, &values));
    assert!(rejected.is_none());
    assert_eq!(count, 0);
    assert!(
        extend_rule_solution(&[], &[], &[] as &[Value])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn successful_triple_matching_removes_only_temporary_copies() {
    let terms = [Term::Variable(0), Term::Variable(1), Term::Variable(2)];
    let values = [
        Value::Identity(1),
        Value::Identity(2),
        Value::String(vec![b'x'; 256]),
    ];
    let (expected, before) = allocations(|| {
        let owned = values.to_vec();
        reference(&[], &terms, &owned)
    });
    let borrowed = [&values[0], &values[1], &values[2]];
    let (actual, after) = allocations(|| extend_rule_solution(&[], &terms, &borrowed));
    assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
    eprintln!("successful string triple: eager allocations={before}, borrowed={after}");
    assert_eq!(
        before,
        after + 2,
        "remove candidate Vec and its String clone"
    );
}

/// Development-only pure-kernel comparison, not host-gated release evidence.
/// Run with --release --ignored --nocapture; alternate order to reduce bias.
#[test]
#[ignore = "manual kernel timing; no wall-time assertions"]
fn candidate_matching_development_comparison() {
    use std::hint::black_box;
    use std::time::Instant;
    for accepts in [false, true] {
        for string_value in [false, true] {
            let terms = [
                Term::Variable(0),
                Term::Constant(Value::Identity(2)),
                Term::Variable(1),
            ];
            let candidate = [
                Value::Identity(1),
                Value::Identity(if accepts { 2 } else { 3 }),
                if string_value {
                    Value::String(vec![b'x'; 256])
                } else {
                    Value::Integer(3)
                },
            ];
            for round in 0..6 {
                for optimized in if round % 2 == 0 {
                    [false, true]
                } else {
                    [true, false]
                } {
                    let start = Instant::now();
                    for _ in 0..100_000 {
                        let result = if optimized {
                            let refs = [&candidate[0], &candidate[1], &candidate[2]];
                            extend_rule_solution(
                                black_box(&[]),
                                black_box(&terms),
                                black_box(&refs),
                            )
                        } else {
                            let owned = black_box(&candidate).to_vec();
                            reference(black_box(&[]), black_box(&terms), black_box(&owned))
                        };
                        black_box(result);
                    }
                    eprintln!(
                        "accepts={accepts} string={string_value} round={round} optimized={optimized} ns={}",
                        start.elapsed().as_nanos()
                    );
                }
            }
        }
    }
}
