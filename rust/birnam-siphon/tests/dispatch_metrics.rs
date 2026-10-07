//! Metrics are an owner-native optional facility, not a dispatch replacement.
mod support;
use birnam_abi::borrowed_bytes;
use birnam_abi::id_protocol::{Operand, Outcome, Protocol};
use core::ffi::c_void;
use support::*;

struct NestedRoot {
    id: Protocol,
    roots: birnam_abi::Oop,
}
unsafe extern "C" fn nested_root(
    context: *mut c_void,
    receiver: birnam_abi::Oop,
    _: *const Operand,
    _: usize,
) -> Outcome {
    let c = unsafe { &*context.cast::<NestedRoot>() };
    unsafe {
        let token = retain(c.id, c.roots, receiver);
        release(c.id, c.roots, token);
    }
    Outcome::loaded(Operand::unit())
}
#[test]
#[ignore = "requires Id built with dispatch-metrics"]
fn census_stops_and_restarts_without_changing_behavior_epoch() {
    unsafe {
        let id = id();
        let begin = id.intern_name(b"dispatchCountsBegin").unwrap();
        let end = id.intern_name(b"dispatchCountsEnd").unwrap();
        let epoch = id.intern_name(b"behaviorEpoch").unwrap();
        let summary = id.intern_name(b"dispatchRootSummary").unwrap();
        let edges = id.intern_name(b"dispatchEdges").unwrap();
        let regions = id.intern_name(b"dispatchRegions").unwrap();
        let subtrees = id.intern_name(b"dispatchSubtrees").unwrap();
        let trace = id.intern_name(b"dispatchTrace").unwrap();
        let roots = load(id, "root_registry", &[]);
        let b = behavior(id);
        let receiver = allocate(id, b, 0);
        let context = Box::into_raw(Box::new(NestedRoot { id, roots }));
        install(id, b, b"nestedRoot", nested_root, context.cast());
        // Deliberately use an ABI spelling on a test object. Region names are
        // ancestry labels, never a claim about the implementation's owner.
        install(id, b, b"wordResultOf:selector:", nested_root, context.cast());
        let region = id.intern_name(b"wordResultOf:selector:").unwrap();
        let nested = id.intern_name(b"nestedRoot").unwrap();
        let keep = id.intern_name(b"retain:").unwrap();
        let drop = id.intern_name(b"release:").unwrap();
        let before = id.send(id.receiver, epoch, &[]).unwrap().as_word().unwrap();
        for _ in 0..2 {
            id.send(id.receiver, begin, &[]).unwrap();
            assert_eq!(
                id.send(id.receiver, epoch, &[]).unwrap().as_word(),
                Some(before)
            );
            let value = id.send(id.receiver, end, &[]).unwrap();
            let raw = value.as_bytes().unwrap();
            let snapshot = borrowed_bytes(raw.pointer, raw.length).unwrap().to_vec();
            let line = format!(
                "1\t{}\n",
                b"behaviorEpoch"
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            );
            assert!(std::str::from_utf8(&snapshot).unwrap().contains(&line));
            let edge_value = id
                .send(id.receiver, edges, &[])
                .unwrap()
                .as_bytes()
                .unwrap();
            let edge_snapshot = borrowed_bytes(edge_value.pointer, edge_value.length)
                .unwrap()
                .to_vec();
            let epoch_hex = b"behaviorEpoch"
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            let lines: Vec<_> = std::str::from_utf8(&edge_snapshot)
                .unwrap()
                .lines()
                .collect();
            let epoch_edge = lines
                .iter()
                .find(|line| line.ends_with(&epoch_hex))
                .unwrap();
            assert!(
                epoch_edge.starts_with("1\t0\t-\t"),
                "reset must clear edges and preserve ingress nesting"
            );

            assert_eq!(
                id.send(id.receiver, epoch, &[]).unwrap().as_word(),
                Some(before)
            );

            id.send(id.receiver, begin, &[]).unwrap();
            let first = id
                .send(roots, keep, &[Operand::object(receiver)])
                .unwrap()
                .as_word()
                .unwrap();
            let second = id
                .send(roots, keep, &[Operand::object(receiver)])
                .unwrap()
                .as_word()
                .unwrap();
            id.send(receiver, nested, &[]).unwrap();
            id.send(roots, drop, &[Operand::word(second)]).unwrap();
            id.send(roots, drop, &[Operand::word(first)]).unwrap();
            let third = id
                .send(roots, keep, &[Operand::object(receiver)])
                .unwrap()
                .as_word()
                .unwrap();
            id.send(roots, drop, &[Operand::word(third)]).unwrap();
            id.send(id.receiver, end, &[]).unwrap();
            let value = id.send(id.receiver, summary, &[]).unwrap();
            let raw = value.as_bytes().unwrap();
            let summary: serde_json::Value =
                serde_json::from_slice(borrowed_bytes(raw.pointer, raw.length).unwrap()).unwrap();
            assert_eq!(summary["retains"], 4);
            assert_eq!(summary["distinct_object_addresses"], 1);
            assert_eq!(summary["same_message_activation_live_duplicates"], 1);
            assert_eq!(summary["other_activation_live_duplicates"], 1);
            assert_eq!(summary["max_live_tokens_observed"], 3);
            assert_eq!(summary["remaining_tokens_observed"], 0);
            id.send(id.receiver, begin, &[]).unwrap();
            id.send(receiver, region, &[]).unwrap();
            id.send(id.receiver, epoch, &[]).unwrap();
            id.send(id.receiver, end, &[]).unwrap();
            let raw = id.send(id.receiver, regions, &[]).unwrap().as_bytes().unwrap();
            let copy = borrowed_bytes(raw.pointer, raw.length).unwrap().to_vec();
            let hex = |name: &[u8]| name.iter().map(|b|format!("{b:02x}")).collect::<String>();
            let label = hex(b"wordResultOf:selector:");
            let rows: Vec<Vec<&str>> = std::str::from_utf8(&copy).unwrap().lines().map(|l|l.split('\t').collect()).collect();
            for name in [b"wordResultOf:selector:".as_slice(), b"retain:", b"release:"] {
                assert!(rows.iter().any(|r|r[1]==label && r[2]==hex(name)), "nested sends keep their nearest region");
            }
            assert!(rows.iter().any(|r|r[1]=="-" && r[2]==hex(b"behaviorEpoch")), "outside work stays separate");
            let raw = id.send(id.receiver, subtrees, &[]).unwrap().as_bytes().unwrap();
            let copy = borrowed_bytes(raw.pointer, raw.length).unwrap().to_vec();
            let root = std::str::from_utf8(&copy).unwrap().lines().find(|line| {
                let parts: Vec<_> = line.split('\t').collect();
                parts[2]==label && parts[3]=="-" && parts[4]==label
            }).unwrap();
            let parts: Vec<_> = root.split('\t').collect();
            assert_eq!(parts[0], "1");
            assert!(parts[1].parse::<usize>().unwrap()>=3, "inclusive counts include nested roots and interning");
            let raw = id.send(id.receiver, trace, &[]).unwrap().as_bytes().unwrap();
            let copy = borrowed_bytes(raw.pointer, raw.length).unwrap().to_vec();
            let records: Vec<Vec<&str>> = std::str::from_utf8(&copy).unwrap().lines().map(|l|l.split('\t').collect()).collect();
            assert!(records.iter().all(|r|r.len()==10));
            assert!(records.iter().enumerate().all(|(i,r)|r[0].parse::<usize>().unwrap()==i+1 && r[1].parse::<usize>().unwrap()<i+1));
            let root = records.iter().find(|r|r[4]==label).unwrap();
            assert_eq!(root[1], "0");
            assert!(root[8].parse::<usize>().unwrap()>=3);
            assert_eq!(root[9], "0");
            assert!(records.iter().any(|r|r[1]==root[0] && r[4]==hex(b"retain:")), "trace preserves activation ancestry");
            assert_eq!(id.send(id.receiver, epoch, &[]).unwrap().as_word(), Some(before));
        }
    }
}
