//! Real Storage snapshots, Trace reachability and live reference-owner selection.
#[path = "support/storage_trace.rs"]
#[allow(dead_code)]
mod support;
use birnam_abi::{as_size, size};

#[test]
#[ignore = "requires production Oop Storage and authored Trace"]
fn live_diagnostics_preserve_graph_coverage_lifetimes_and_failure_cleanup() {
    let host = support::bootstrap();
    let h = support::H::new(&host);
    h.call(h.gc, b"automaticCollection:", &[size(0).unwrap()]);
    let original = h.project_storage();
    let owner = h.object();
    h.bind(
        h.private(owner),
        b"referencesOf:do:",
        h.method(b"referencesOf:do:"),
    );
    let pair = h.call(h.named(b"ObjectVt"), b"delegated", &[]);
    h.call(h.gc, b"trace:with:", &[pair, owner]);
    let setup = host.scope();
    let nodes: Vec<_> = (0..4)
        .map(|_| setup.call(pair, b"allocate", &[size(0).unwrap()]).unwrap())
        .collect();
    let raw = setup
        .call(
            h.named(b"Integer"),
            b"fromWord:",
            &[setup.word(nodes[2].addr()).unwrap()],
        )
        .unwrap();
    let x_pin = host.scope();
    x_pin.keep(nodes[0]).unwrap();
    h.q.keep(raw).unwrap();
    h.call(
        h.probe,
        b"configure",
        &[nodes[0], nodes[1], nodes[2], nodes[3], raw],
    );
    let ids: Vec<_> = nodes.iter().copied().map(|o| h.identity(o)).collect();
    drop(setup);
    h.flags(false, false, false, true, false);
    for mask in [0, 2] {
        assert_eq!(h.detail(h.report(mask).unwrap_err()), 3);
        assert_eq!(h.stat(0), 0);
        assert_eq!(h.stat(1), 0);
    }
    h.flags(true, false, false, true, false);
    assert_eq!(as_size(h.report(1).unwrap()), Some(73));
    assert_eq!(h.stat(1), 1);
    assert_eq!(h.stat(3), 1);
    assert_eq!(h.stat(0), 5);
    for (i, reached) in [true, true, false, false, true].into_iter().enumerate() {
        assert_eq!(h.row(i).1, reached, "node {i}");
    }
    let identity = h.row(2).0;
    h.flags(true, true, false, true, false);
    assert!(h.report(1).is_err());
    h.flags(true, false, false, true, false);
    assert!(host.send(h.field(8), b"visit:", &[nodes[0]]).is_err());
    h.flags(true, false, true, true, false);
    assert_eq!(h.detail(h.report(1).unwrap_err()), 991);
    h.flags(true, false, false, true, false);
    assert_eq!(as_size(h.report(1).unwrap()), Some(73));
    assert_eq!(h.row(2).0, identity);
    let empty = h.object();
    h.bind(h.private(empty), b"referencesOf:do:", h.method(b"empty"));
    h.call(h.gc, b"trace:with:", &[pair, empty]);
    h.report(1).unwrap();
    assert!(h.row(0).1);
    assert!(!h.row(1).1);
    h.call(h.gc, b"trace:with:", &[pair, owner]);
    h.report(1).unwrap();
    assert!(h.row(1).1);
    assert!(h.stat(2) >= 8);
    drop(x_pin);
    h.report(1).unwrap();
    assert!(!h.row(0).1 && !h.row(1).1);
    assert!(
        h.present(ids[2]) && h.present(ids[3]),
        "observation protection must retain unreachable endpoints"
    );
    h.flags(false, false, false, true, false);
    h.put(8, std::ptr::null_mut());
    h.collect();
    for id in ids {
        assert!(
            !h.present(id),
            "ordinary GC resumes after observation releases its roots"
        );
    }
    let unknown_vt = h.call(h.named(b"ObjectVt"), b"delegated", &[]);
    h.call(
        host.id.root,
        b"parentOf:put:",
        &[unknown_vt, std::ptr::null_mut()],
    );
    let unknown = h.call(unknown_vt, b"allocate", &[size(8).unwrap()]);
    h.call(h.probe, b"configure", &[unknown]);
    h.flags(false, false, false, false, false);
    assert_eq!(h.detail(h.report(1).unwrap_err()), 3);
    assert_eq!(as_size(h.report(3).unwrap()), Some(73));
    assert_eq!(h.stat(3), 3);
    assert!(h.row(0).1);
    h.bind(h.vt(h.storage), b"observe:", original);
    h.call(
        h.vt(h.storage),
        b"removeMethod:",
        &[h.key(b"originalObserve:")],
    );
}
