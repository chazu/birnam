//! Initial generation metadata through current parsing, value and GC owners.
#[path = "support/generation.rs"]
#[allow(dead_code)]
mod support;
use birnam_abi::size;
use birnam_artifact_codec::stackless_selection::{Disposition, inspect_generation_selection};
#[test]
#[ignore = "requires production Oop modules"]
fn initial_projection_is_canonical_inert_rooted_and_rejects_mixed_sources() {
    let host = support::bootstrap();
    let h = support::H::new(&host);
    let cg = h.named(b"Codegen");
    let core = h.call(cg, b"initialGenerationProjection", &[]);
    let core_bytes = h.bytes(h.at(core, 3));
    let parsed = inspect_generation_selection(&core_bytes).unwrap();
    let revisions = parsed.revision_count();
    let rows = parsed.row_count();
    assert!(revisions >= 2);
    let a=h.text(b"[Module named: #a requires: #[]]; [Reflect defineMethod: #generationAnswer on: Integer as: [:self | 42]]");
    let b = h.text(b"[Module named: #b requires: #[#a]]; let projected := [| 99]");
    for (name, source) in [(b"b".as_slice(), b), (b"a".as_slice(), a)] {
        assert_eq!(
            h.call(cg, b"recordModuleSource:named:", &[source, h.key(name)]),
            source
        );
    }
    let projection = h.call(cg, b"initialGenerationProjection", &[]);
    assert_eq!(h.at(projection, 0), h.key(b"generationProjection"));
    let map = h.at(projection, 1);
    let count = h.number(h.call(map, b"size", &[]));
    let row = |name: &[u8]| {
        (0..count)
            .map(|i| h.at(map, i))
            .find(|r| h.at(*r, 0) == h.key(name))
            .unwrap()
    };
    let ar = row(b"a");
    let br = row(b"b");
    assert_eq!(h.at(h.at(ar, 1), 3), a);
    assert_eq!(h.at(h.at(br, 1), 3), b);
    assert_eq!(h.at(ar, 2), h.named(b"nil"));
    let digest = h.bytes(h.at(projection, 2));
    assert_eq!(digest.len(), 64);
    let selection = h.bytes(h.at(projection, 3));
    let parsed = inspect_generation_selection(&selection).unwrap();
    assert_eq!(parsed.revision_count(), revisions + 2);
    assert_eq!(parsed.row_count(), rows + 3);
    assert!(
        parsed
            .rows()
            .all(|r| r.unwrap().disposition == Disposition::ResumableRequired)
    );
    assert!(
        h.binding(h.named(b"IntegerVt"), b"generationAnswer")
            .is_null(),
        "projection must not publish its methods"
    );
    let repeat = h.call(cg, b"initialGenerationProjection", &[]);
    assert_eq!(h.bytes(h.at(repeat, 2)), digest);
    assert_eq!(h.bytes(h.at(repeat, 3)), selection);
    let extra = h.text(b"[Module named: #c requires: #[]]");
    let vt = h.vt(h.named(b"String"));
    let original = h.binding(vt, b"fromBytes:");
    let alias = h.key(b"projectionOriginalFromBytes:");
    h.bind(vt, b"projectionOriginalFromBytes:", original);
    let state = h.context(&[
        h.gc,
        alias,
        cg,
        extra,
        h.key(b"c"),
        size(0).unwrap(),
        size(0).unwrap(),
    ]);
    h.bind(vt, b"fromBytes:", h.method(b"constructing", state));
    let collected = h.call(cg, b"initialGenerationProjection", &[]);
    assert_eq!(h.bytes(h.at(collected, 2)), digest);
    assert!(h.count(state, 6) > 0);
    h.put(state, 5, size(1).unwrap());
    let changed = h.call(cg, b"initialGenerationProjection", &[]);
    assert_eq!(h.call(changed, b"isError", &[]), h.named(b"true"));
    h.bind(vt, b"fromBytes:", original);
    h.call(vt, b"removeMethod:", &[alias]);
    let fresh = h.call(cg, b"initialGenerationProjection", &[]);
    assert_ne!(h.bytes(h.at(fresh, 2)), digest);
    assert_eq!(
        inspect_generation_selection(&h.bytes(h.at(fresh, 3)))
            .unwrap()
            .revision_count(),
        revisions + 3
    );
    assert_eq!(h.call(cg, b"moduleSourceFor:", &[h.key(b"c")]), extra);
}
