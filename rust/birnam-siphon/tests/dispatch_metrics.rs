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
        let roots = load(id, "root_registry", &[]);
        let b = behavior(id);
        let receiver = allocate(id, b, 0);
        let context = Box::into_raw(Box::new(NestedRoot { id, roots }));
        install(id, b, b"nestedRoot", nested_root, context.cast());
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
        }
    }
}
