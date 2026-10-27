//! A recovered frame retains its baseline, not the finished candidate recipe.
use super::*;

fn inline(h: &H<'_>, baseline: Oop, recipe: Oop) -> (Oop, Oop) {
    let context = h.context(&[recipe, h.gc, size(0).unwrap()]);
    let step = h.native(b"recovery", context);
    (
        h.call(
            baseline,
            b"withInline:at:owner:",
            &[step, h.integer(1), context],
        ),
        context,
    )
}

pub(super) fn check(h: &H<'_>) {
    h.clear();
    h.collect();
    annotation_aliases(h);
    for abandon in [false, true] {
        let owner = h.object();
        let baseline = h.method("minimal-return", 1, owner);
        let result = h.named(b"true");
        let recipe = h.vector(&[h.named(b"nil"), result]);
        let (candidate, context) = inline(h, baseline, recipe);
        let receiver = h.object();
        let vt = h.private_vt(receiver);
        h.bind(vt, b"value", candidate);
        let replacement = h.method("minimal-return", 1, h.named(b"nil"));
        let machine = h.machine();
        let pin = h.host.scope();
        pin.keep(machine).unwrap();
        let owner_id = h.identity(owner);
        let baseline_id = h.identity(baseline);
        let context_id = h.identity(context);
        let candidate_id = h.identity(candidate);
        assert_eq!(
            h.call(machine, b"runBlock:budget:", &[receiver, h.integer(1)]),
            machine
        );
        assert_eq!(h.number(h.call(machine, b"liveFrameCount", &[])), 2);
        assert_eq!(h.count(context, 2), 1);
        h.bind(vt, b"value", replacement);
        h.clear();
        h.collect();
        assert!(
            h.present(owner_id),
            "recovered frame must retain its code owner"
        );
        assert!(
            h.present(baseline_id),
            "recovered frame must retain its baseline Method"
        );
        assert!(
            !h.present(context_id),
            "completed recovery must release its candidate context"
        );
        assert!(
            !h.present(candidate_id),
            "completed recovery must release its candidate Method"
        );
        if abandon {
            h.call(machine, b"destroy", &[]);
        } else {
            assert_eq!(
                h.call(machine, b"continueWithBudget:", &[h.integer(10)]),
                result
            );
        }
        h.clear();
        h.collect();
        assert!(!h.present(owner_id) && !h.present(baseline_id));
        drop(pin);
        h.clear();
        h.collect();
    }
    let owner = h.object();
    let baseline = h.method("minimal-return", 1, owner);
    let (candidate, context) = inline(h, baseline, h.vector(&[]));
    let block = h.block(candidate, &[]);
    let machine = h.machine();
    let ids = [owner, baseline, candidate, context].map(|v| h.identity(v));
    // Machine owns native failure conversion at its pending call.
    let result = h.call(machine, b"runBlock:", &[block]);
    assert_eq!(h.call(result, b"kind", &[]), h.key(b"nativeInvalidRequest"));
    h.call(machine, b"destroy", &[]);
    h.clear();
    h.collect();
    for id in ids {
        assert!(
            !h.present(id),
            "failed recovery leaked an acquired entry or recipe"
        );
    }
}

fn annotation_aliases(h: &H<'_>) {
    let owner = h.object();
    let baseline = h.method("minimal-return", 1, owner);
    let (first, first_context) =
        inline(h, baseline, h.vector(&[h.named(b"nil"), h.named(b"true")]));
    let (second, _) = inline(h, first, h.vector(&[h.named(b"nil"), h.named(b"false")]));
    let first_ids = [first, first_context].map(|v| h.identity(v));
    let baseline_ids = [baseline, owner].map(|v| h.identity(v));
    let pin = h.host.scope();
    pin.keep(second).unwrap();
    h.clear();
    h.collect();
    for id in first_ids {
        assert!(
            !h.present(id),
            "reannotation must not retain the superseded recipe"
        );
    }
    for id in baseline_ids {
        assert!(h.present(id));
    }
    let block = h.block(second, &[]);
    let machine = h.machine();
    assert_eq!(h.call(machine, b"runBlock:", &[block]), h.named(b"false"));

    // PC candidates belong to the baseline registration, including installation
    // and withdrawal through an inline annotation of that registration.
    let optimized = h.object();
    let step = h.native(b"candidate", h.context(&[optimized, h.named(b"Integer")]));
    h.call(second, b"optimizeWith:", &[h.vector(&[step])]);
    for _ in 0..63 {
        assert_eq!(h.invoke(baseline, h.named(b"nil")), owner);
    }
    assert_eq!(h.invoke(baseline, h.named(b"nil")), optimized);
    h.call(second, b"optimizeWith:", &[h.vector(&[])]);
    assert_eq!(h.invoke(baseline, h.named(b"nil")), owner);
    drop(pin);
    h.clear();
    h.collect();
    for id in baseline_ids {
        assert!(!h.present(id));
    }
}
