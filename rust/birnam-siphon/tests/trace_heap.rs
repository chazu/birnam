//! Unfiltered physical-heap reporting with automatic collection left enabled.
#[path = "support/storage_trace.rs"]
#[allow(dead_code)]
mod support;
use birnam_abi::as_size;

#[test]
#[ignore = "requires production Oop Storage and authored Trace"]
fn complete_physical_inventory_survives_automatic_and_completion_collection() {
    let host = support::bootstrap();
    let h = support::H::new(&host);
    let vt = h.vt(h.gc);
    let selector = h.key(b"automaticCollection:");
    let original = h.call(vt, b"lookup", &[selector]);
    h.bind(vt, b"automaticCollection:", h.method(b"setting"));
    h.flags(true, false, false, false, true);
    h.collect();
    assert_eq!(as_size(h.report(usize::MAX).unwrap()), Some(73));
    assert_eq!(h.stat(1), 1);
    assert_eq!(h.stat(3) & 1, 1);
    assert!(h.stat(0) > 1000);
    assert_eq!(h.stat(4), 0);
    eprintln!("unfiltered Trace reported {} allocations", h.stat(0));
    h.bind(vt, b"automaticCollection:", original);
}
