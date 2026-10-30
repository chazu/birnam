//! Persistent queue data operations keep snapshots, use node messages and release jobs.
#[allow(dead_code)]
#[path = "support/minimal.rs"]
mod minimal;
#[allow(dead_code)]
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{as_size, size};
#[test]
#[ignore = "requires native modules"]
fn fifo_data_preserves_snapshots_and_current_node_access() {
    let h = minimal::native_floor();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let factory = h.named(b"Queue").unwrap();
    let empty = q.call(factory, b"new", &[]).unwrap();
    let first = q.call(empty, b"add:", &[size(11).unwrap()]).unwrap();
    let second = q.call(first, b"add:", &[size(22).unwrap()]).unwrap();
    let model = h.named(b"ObjectModel").unwrap();
    let back = q
        .call(
            model,
            b"slot:in:",
            &[
                q.call(
                    h.named(b"Integer").unwrap(),
                    b"fromWord:",
                    &[size(1).unwrap()],
                )
                .unwrap(),
                second,
            ],
        )
        .unwrap();
    let vt = q
        .call(h.id.root, b"ensurePrivateVtableFor:", &[back])
        .unwrap();
    let at = hooks.symbol(b"at:");
    let original = q.call(vt, b"lookup", &[at]).unwrap();
    let denied = hooks.context(&[hooks.fault(71), size(0).unwrap()]);
    q.call(vt, b"addMethod", &[at, hooks.method(b"deny", denied)])
        .unwrap();
    assert!(q.call(second, b"takeFirst", &[]).is_err());
    q.call(vt, b"addMethod", &[at, original]).unwrap();
    assert_eq!(hooks.count(denied, 1), 1);
    let gc = h.named(b"GC").unwrap();
    let mvt = q.call(h.id.root, b"behaviorOf:", &[model]).unwrap();
    let allocate = hooks.symbol(b"allocate:bytes:");
    let saved = q.call(mvt, b"lookup", &[allocate]).unwrap();
    let collecting = hooks.context(&[saved, gc, size(0).unwrap()]);
    q.call(
        mvt,
        b"addMethod",
        &[allocate, hooks.method(b"collectOnce", collecting)],
    )
    .unwrap();
    let pair = q.call(second, b"takeFirst", &[]).unwrap();
    q.call(mvt, b"addMethod", &[allocate, saved]).unwrap();
    assert_eq!(
        q.call(pair, b"atWord:", &[size(0).unwrap()]).unwrap(),
        size(11).unwrap()
    );
    let rest = q.call(pair, b"atWord:", &[size(1).unwrap()]).unwrap();
    let vector = q.call(rest, b"asVector", &[]).unwrap();
    assert_eq!(
        q.call(vector, b"atWord:", &[size(0).unwrap()]).unwrap(),
        size(22).unwrap()
    );
    assert_eq!(
        as_size(
            q.call(q.call(second, b"asVector", &[]).unwrap(), b"sizeWord", &[])
                .unwrap()
        ),
        Some(2)
    );
    // Only the remaining queue retains the jobs after the temporary result closes.
    let holder = q.call(h.id.root, b"sequence", &[empty]).unwrap();
    let object = h.named(b"Object").unwrap();
    let add = h.intern(b"add:").unwrap();
    let clone = h.intern(b"clone").unwrap();
    let put = h.intern(b"at:put:").unwrap();
    let job = unsafe { h.id.send(object, clone, &[]) }.unwrap();
    let queue = unsafe { h.id.send(empty, add, &[job]) }.unwrap();
    unsafe { h.id.send(holder, put, &[size(0).unwrap(), queue]) }.unwrap();
    {
        let scope = h.scope();
        let result = scope.call(queue, b"takeFirst", &[]).unwrap();
        let rest = scope.call(result, b"atWord:", &[size(1).unwrap()]).unwrap();
        scope
            .call(holder, b"at:put:", &[size(0).unwrap(), rest])
            .unwrap();
    }
    q.call(gc, b"collectPrecisely", &[]).unwrap();
    assert!(
        unsafe {
            h.id.send(h.id.root, h.intern(b"behaviorOf:").unwrap(), &[job])
        }
        .is_err()
    );
}
