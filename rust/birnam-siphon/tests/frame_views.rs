//! Live frame inspection through the production Oop facade.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::size;

#[test]
#[ignore = "requires production Oop modules"]
fn frame_views_follow_live_frames_and_reject_stale_handles() {
    let h = oop::bootstrap();
    let retained = h.scope();
    let gc = h.named(b"GC").unwrap();
    h.send(gc, b"precisePayloads:", &[size(1).unwrap()])
        .unwrap();
    let (machine, view, initial_pc, identity) = {
        let q = h.scope();
        let source = q.string(b"[| let a := [Object clone]; [a == a]]").unwrap();
        let block = q
            .call(h.named(b"Codegen").unwrap(), b"eval:", &[source])
            .unwrap();
        q.check_result(block).unwrap();
        let machine = q.call(h.named(b"Machine").unwrap(), b"new", &[]).unwrap();
        assert_eq!(
            q.call(machine, b"currentFrameView", &[]).unwrap(),
            h.named(b"nil").unwrap()
        );
        let budget = q
            .call(
                h.named(b"Integer").unwrap(),
                b"fromWord:",
                &[size(2).unwrap()],
            )
            .unwrap();
        assert_eq!(
            q.call(machine, b"runBlock:budget:", &[block, budget])
                .unwrap(),
            machine
        );
        let view = q.call(machine, b"currentFrameView", &[]).unwrap();
        q.check_result(view).unwrap();
        assert_ne!(view, h.named(b"nil").unwrap());
        let pc = q
            .word_bits(
                q.call(
                    h.named(b"Integer").unwrap(),
                    b"wordOf:",
                    &[q.call(view, b"programCounter", &[]).unwrap()],
                )
                .unwrap(),
            )
            .unwrap();
        let identity = q
            .word_bits(q.call(view, b"implementationIdentity", &[]).unwrap())
            .unwrap();
        assert_eq!(
            q.word_bits(q.call(view, b"methodIdentity", &[]).unwrap())
                .unwrap(),
            identity
        );
        assert_eq!(
            q.word_bits(
                q.call(
                    h.named(b"Integer").unwrap(),
                    b"wordOf:",
                    &[q.call(view, b"sourceMapLocation", &[]).unwrap()]
                )
                .unwrap()
            )
            .unwrap(),
            pc
        );
        for name in [b"generationIdentity".as_slice(), b"sourceMapIdentity"] {
            q.word_bits(q.call(view, name, &[]).unwrap()).unwrap();
        }
        let count = q
            .word_bits(
                q.call(
                    h.named(b"Integer").unwrap(),
                    b"wordOf:",
                    &[q.call(view, b"slotCount", &[]).unwrap()],
                )
                .unwrap(),
            )
            .unwrap();
        assert!(count > 0);
        // Future sends observe replacement; a captured getter still reads the
        // entered implementation without a second lookup on that Method.
        let getter = q
            .call(view, b"methodFor:", &[h.intern(b"programCounter").unwrap()])
            .unwrap();
        let slots = q
            .call(view, b"methodFor:", &[h.intern(b"slotCount").unwrap()])
            .unwrap();
        let vt = q
            .call(h.id.root, b"ensurePrivateVtableFor:", &[view])
            .unwrap();
        q.call(
            vt,
            b"addMethod",
            &[h.intern(b"programCounter").unwrap(), slots],
        )
        .unwrap();
        let changed = q.call(view, b"programCounter", &[]).unwrap();
        assert_eq!(
            q.word_bits(
                q.call(h.named(b"Integer").unwrap(), b"wordOf:", &[changed])
                    .unwrap()
            )
            .unwrap(),
            count
        );
        let captured = q.call(getter, b"invokeOn:", &[view]).unwrap();
        assert_eq!(
            q.word_bits(
                q.call(h.named(b"Integer").unwrap(), b"wordOf:", &[captured])
                    .unwrap()
            )
            .unwrap(),
            pc
        );
        // A captured getter cannot interpret an unrelated object's payload.
        let foreign = q.call(h.named(b"Object").unwrap(), b"clone", &[]).unwrap();
        let error = q.call(getter, b"invokeOn:", &[foreign]).unwrap();
        assert_eq!(
            q.call(error, b"kind", &[]).unwrap(),
            h.intern(b"staleFrameView").unwrap()
        );
        q.call(
            vt,
            b"addMethod",
            &[h.intern(b"programCounter").unwrap(), getter],
        )
        .unwrap();
        for i in 0..count {
            assert_eq!(
                q.call(view, b"slotKindAt:", &[size(i).unwrap()]).unwrap(),
                h.intern(b"oop").unwrap()
            );
            q.call(view, b"oopAt:", &[size(i).unwrap()]).unwrap();
        }
        for name in [b"oopAt:".as_slice(), b"slotKindAt:"] {
            for index in [size(count).unwrap(), h.named(b"nil").unwrap()] {
                let error = q.call(view, name, &[index]).unwrap();
                assert_eq!(
                    q.call(error, b"kind", &[]).unwrap(),
                    h.intern(b"invalidFrameSlot").unwrap()
                );
            }
        }
        assert!(q.call(view, b"clone", &[]).is_err());
        // Retain only the view: its owner must report the Machine edge to GC.
        retained.keep(view).unwrap();
        (machine, view, pc, identity)
    };
    h.send(gc, b"collectPrecisely", &[]).unwrap();
    let q = h.scope();
    let prototype = h.named(b"FrameView").unwrap();
    let error = q.call(prototype, b"programCounter", &[]).unwrap();
    assert_eq!(
        q.call(error, b"kind", &[]).unwrap(),
        h.intern(b"staleFrameView").unwrap()
    );
    assert_eq!(
        q.word_bits(q.call(view, b"implementationIdentity", &[]).unwrap())
            .unwrap(),
        identity
    );
    assert_eq!(
        q.word_bits(
            q.call(
                h.named(b"Integer").unwrap(),
                b"wordOf:",
                &[q.call(view, b"programCounter", &[]).unwrap()]
            )
            .unwrap()
        )
        .unwrap(),
        initial_pc
    );
    let budget = q
        .call(
            h.named(b"Integer").unwrap(),
            b"fromWord:",
            &[size(1000).unwrap()],
        )
        .unwrap();
    assert_eq!(
        q.call(machine, b"continueWithBudget:", &[budget]).unwrap(),
        h.named(b"true").unwrap()
    );
    for name in [
        b"programCounter".as_slice(),
        b"slotCount",
        b"implementationIdentity",
    ] {
        let error = q.call(view, name, &[]).unwrap();
        assert_eq!(
            q.call(error, b"kind", &[]).unwrap(),
            h.intern(b"staleFrameView").unwrap()
        );
    }
    assert_eq!(
        q.call(machine, b"currentFrameView", &[]).unwrap(),
        h.named(b"nil").unwrap()
    );
    q.call(machine, b"destroy", &[]).unwrap();
    let error = q.call(view, b"oopAt:", &[size(0).unwrap()]).unwrap();
    assert_eq!(
        q.call(error, b"kind", &[]).unwrap(),
        h.intern(b"staleFrameView").unwrap()
    );
}
