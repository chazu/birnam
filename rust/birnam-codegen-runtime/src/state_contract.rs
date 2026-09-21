//! State and root-callback contracts, isolated from process singleton reuse.
use super::*;
static MARKED: Mutex<Vec<(usize, bool)>> = Mutex::new(Vec::new());

unsafe extern "C" fn record_mark(value: Oop) {
    let sources_held = matches!(
        MODULE_SOURCES.try_lock(),
        Err(std::sync::TryLockError::WouldBlock)
    );
    MARKED.lock().unwrap().push((value as usize, sources_held));
}
fn marked() -> Vec<(usize, bool)> {
    MARKED.lock().unwrap().clear();
    unsafe { mark_live_units(record_mark) };
    MARKED.lock().unwrap().clone()
}
fn cache(index: usize, ready: bool) {
    assert!(stage_materialization(
        MaterializationDisposition::Synchronous,
        [index as u8; 32],
        vec![index as u8],
        (1000 + index) as Oop,
        1
    ));
    assert!(set_staged_materialization_vtable(
        (1000 + index) as Oop,
        (2000 + index) as Oop
    ));
    if ready {
        assert!(commit_staged_materialization((1000 + index) as Oop));
    }
}

#[test]
#[ignore = "requires a fresh process for runtime singleton state"]
fn cache_eviction_preserves_staging_and_failure_side_effects() {
    for index in 0..64 {
        cache(index, true);
    }
    assert!(
        cached_materialization(MaterializationDisposition::Synchronous, [0; 32], &[0]).is_some()
    );
    cache(64, false);
    assert!(
        cached_materialization(MaterializationDisposition::Synchronous, [1; 32], &[1]).is_none()
    );
    assert!(
        cached_materialization(MaterializationDisposition::Synchronous, [0; 32], &[0]).is_some()
    );
    assert!(cached_materialization(MaterializationDisposition::Resumable, [0; 32], &[0]).is_none());
    assert!(
        cached_materialization(MaterializationDisposition::Synchronous, [0; 32], &[99]).is_none()
    );
    // Over-budget admission first evicts ready leases, then refuses while the
    // staging lease is still rooted. This is deliberately not transactional.
    assert!(!stage_materialization(
        MaterializationDisposition::Synchronous,
        [99; 32],
        vec![],
        1099 as Oop,
        MAX_MATERIALIZATION_C_SOURCE_BYTES + 1
    ));
    assert_eq!(marked(), vec![(2064, false), (1064, false)]);
    assert!(commit_staged_materialization(1064 as Oop));
    assert!(
        cached_materialization(MaterializationDisposition::Synchronous, [64; 32], &[64]).is_some()
    );
}

#[test]
#[ignore = "requires a fresh process for runtime singleton state"]
fn binding_publication_retains_both_generations() {
    let stage = |unit, generation| {
        stage_code_binding(
            20 as Oop,
            21 as Oop,
            22 as Oop,
            23 as Oop,
            24 as Oop,
            unit as Oop,
            generation,
            [0; 32],
            0,
        )
    };
    assert!(stage(25, 125));
    assert!(!stage(26, 126));
    assert_eq!(publish_staged_code_binding(25 as Oop), None);
    assert!(set_staged_code_binding_method(25 as Oop, 1));
    assert_eq!(publish_staged_code_binding(25 as Oop), Some(vec![]));
    assert!(stage(26, 126));
    assert!(set_staged_code_binding_method(26 as Oop, 2));
    assert_eq!(publish_staged_code_binding(26 as Oop), Some(vec![125]));
    assert_eq!(
        withdraw_current_code_binding(20 as Oop, 22 as Oop),
        vec![126]
    );
    assert_eq!(withdraw_current_code_binding(20 as Oop, 22 as Oop), vec![]);
    remove_staged_code_binding(25 as Oop);
    assert_eq!(
        marked(),
        [20, 21, 22, 23, 24, 25, 20, 21, 22, 23, 24, 26]
            .map(|p| (p, false))
            .to_vec()
    );
}

#[test]
#[ignore = "requires a fresh process for root order and poison behavior"]
fn root_order_and_source_lock_scope() {
    EVALUATOR_ROOTS.lock().unwrap().push(1);
    cache(0, false);
    assert!(stage_code_binding(
        20 as Oop, 21 as Oop, 22 as Oop, 23 as Oop, 24 as Oop, 25 as Oop, 125, [0; 32], 0
    ));
    MODULE_SOURCES.lock().unwrap().push(ModuleSourceEntry {
        name: 40,
        source: 41,
        artifact_selection: None,
    });
    CANDIDATE_ROOTS.lock().unwrap().push(CandidateRoot {
        generation: 1,
        index: 0,
        value: 60,
    });
    VTABLE_CACHE_ROOTS.lock().unwrap().insert((1, 0), 70);
    ACTIVE_GENERATION.store(80, Ordering::Release);
    let mut expected = [1, 2000, 1000, 20, 21, 22, 23, 24, 25]
        .map(|p| (p, false))
        .to_vec();
    expected.extend([40, 41, 60, 70, 80].map(|p| (p, true)));
    assert_eq!(marked(), expected);
    let _ = std::thread::spawn(|| {
        let _guard = MODULE_SOURCES.lock().unwrap();
        panic!("intentional source-lock poison");
    })
    .join();
    expected.truncate(9);
    assert_eq!(
        marked(),
        expected,
        "poison stops the whole root callback before later registries"
    );
}
