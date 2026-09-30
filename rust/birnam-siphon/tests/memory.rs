//! Real production collection through Siphon -> Id -> Bootstrap, with small
//! native representation owners used only to observe lifecycle effects.
mod support;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
};
use core::{
    cell::{Cell, RefCell},
    ffi::c_void,
    ptr,
};
use std::{collections::HashMap, os::unix::ffi::OsStrExt};
use support::*;

struct Owner {
    id: Protocol,
    gc: Oop,
    behavior: Oop,
    edges: RefCell<HashMap<usize, Vec<Oop>>>,
    visits: RefCell<Vec<usize>>,
    finalized: RefCell<Vec<(usize, usize)>>,
    failure: Cell<bool>,
    ignored_failure: Cell<bool>,
    phase_checks: Cell<usize>,
    first: Cell<Oop>,
    later: Cell<Oop>,
    replacement: Cell<Oop>,
    switch_behavior: Cell<Oop>,
    retired_symbol: Cell<Oop>,
    retired_behavior: Cell<Oop>,
    tag: usize,
}
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
fn done(result: Result<(), Outcome>) -> Outcome {
    result.map_or_else(|e| e, |_| Outcome::loaded(Operand::unit()))
}
impl Owner {
    unsafe fn checks(&self) -> Result<(), Outcome> {
        unsafe {
            if self.id.send_named(self.gc, b"collect", &[])?.as_word() != Some(0) {
                return Err(invalid());
            }
            if self
                .id
                .send_named(self.behavior, b"allocate", &[Operand::word(0)])
                .is_ok()
            {
                return Err(invalid());
            }
            if self.id.send_named(self.behavior, b"delegated", &[]).is_ok() {
                return Err(invalid());
            }
            if self
                .id
                .intern_name(b"a-name-that-must-not-be-created-during-gc")
                .is_ok()
            {
                return Err(invalid());
            }
            // Interning an existing selector is a lookup, not allocation.
            self.id.intern_name(b"rootsDo:")?;
            self.phase_checks.set(self.phase_checks.get() + 1);
            Ok(())
        }
    }
}
unsafe extern "C" fn trace(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let state = &*context.cast::<Owner>();
        let Some([value, visitor]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let value = value.as_object().ok_or_else(invalid)?;
        let visitor = visitor.as_object().ok_or_else(invalid)?;
        state.visits.borrow_mut().push(value.addr());
        state.checks()?;
        if state.failure.get() {
            return Err(invalid());
        }
        if state.ignored_failure.get() {
            // Failure is sticky even when a trusted provider accidentally ignores it.
            let _ = state.id.send_named(visitor, b"visit:", &[Operand::word(7)]);
            return Ok(());
        }
        if value == state.first.get() && !state.switch_behavior.get().is_null() {
            state.id.send_named(
                state.gc,
                b"trace:with:",
                &[
                    Operand::object(state.switch_behavior.get()),
                    Operand::object(state.replacement.get()),
                ],
            )?;
        }
        let edges = state
            .edges
            .borrow()
            .get(&value.addr())
            .cloned()
            .unwrap_or_default();
        for edge in edges {
            state
                .id
                .send_named(visitor, b"visit:", &[Operand::object(edge)])?;
        }
        Ok(())
    })())
}
unsafe extern "C" fn finalize(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let state = &*context.cast::<Owner>();
        let Some([value]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let value = value.as_object().ok_or_else(invalid)?;
        state.checks()?;
        // Detached from Id, but our own payload remains readable until return.
        if state
            .id
            .send_named(
                state.id.receiver,
                b"allocationAt:",
                &[Operand::word(value.addr())],
            )?
            .kind()
            != native::UNIT
        {
            return Err(invalid());
        }
        if value.cast::<usize>().read() != 0xcafe {
            return Err(invalid());
        }
        state.finalized.borrow_mut().push((value.addr(), state.tag));
        if value == state.first.get() && !state.later.get().is_null() {
            if state
                .id
                .send_named(
                    state.id.receiver,
                    b"allocationAt:",
                    &[Operand::word(state.later.get().addr())],
                )?
                .as_object()
                != Some(state.later.get())
            {
                return Err(invalid());
            }
            state.id.send_named(
                state.gc,
                b"finalize:with:",
                &[
                    Operand::object(state.later.get()),
                    Operand::object(state.replacement.get()),
                ],
            )?;
            if state
                .id
                .send_named(
                    state.id.receiver,
                    b"symbolNamed:",
                    &[Operand::bytes(b"weak-gc-symbol")],
                )?
                .kind()
                != native::UNIT
            {
                return Err(invalid());
            }
            if state
                .id
                .send_named(
                    state.id.receiver,
                    b"isBehavior:",
                    &[Operand::object(state.retired_behavior.get())],
                )?
                .as_word()
                != Some(0)
            {
                return Err(invalid());
            }
        }
        Ok(())
    })())
}
struct Counted {
    original: NativeEntry,
    calls: Cell<usize>,
}
unsafe extern "C" fn counted(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let state = unsafe { &*context.cast::<Counted>() };
    state.calls.set(state.calls.get() + 1);
    unsafe { (state.original.method.unwrap())(state.original.context, receiver, args, count) }
}
struct VisitorDispatch {
    id: Protocol,
    behavior: Oop,
    first: Cell<usize>,
    later: Cell<usize>,
}
unsafe extern "C" fn next_visit(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<VisitorDispatch>();
        p.later.set(p.later.get() + 1);
        p.id.send_named(r, b"savedGCVisit:", borrowed_slice(a, n).unwrap())
            .map_or_else(|e| e, Outcome::loaded)
    }
}
unsafe extern "C" fn first_visit(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<VisitorDispatch>();
        p.first.set(p.first.get() + 1);
        install(p.id, p.behavior, b"visit:", next_visit, c);
        p.id.send_named(r, b"savedGCVisit:", borrowed_slice(a, n).unwrap())
            .map_or_else(|e| e, Outcome::loaded)
    }
}
unsafe extern "C" fn refuse_visit(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 917)
}
unsafe fn visitor_dispatch(id: Protocol, gc: Oop, nodes: Oop) {
    unsafe {
        let behavior = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(gc)],
        ));
        let selector = id.intern_name(b"visit:").unwrap();
        let original = send(id, behavior, b"lookup", &[Operand::object(selector)])
            .as_entry()
            .unwrap();
        install(
            id,
            behavior,
            b"savedGCVisit:",
            original.method.unwrap(),
            original.context,
        );
        let mut probe = VisitorDispatch {
            id,
            behavior,
            first: Cell::new(0),
            later: Cell::new(0),
        };
        install(
            id,
            behavior,
            b"visit:",
            first_visit,
            ptr::from_mut(&mut probe).cast(),
        );
        collect(id, gc);
        assert_eq!(
            probe.first.get(),
            1,
            "lookup occurs for every reported edge"
        );
        assert!(probe.later.get() > 1);
        install(id, behavior, b"visit:", refuse_visit, ptr::null_mut());
        let dead = allocate(id, nodes, 0);
        assert_eq!(
            id.send_named(gc, b"collectPrecisely", &[])
                .unwrap_err()
                .detail,
            917
        );
        assert!(present(id, dead), "a failed visitor send prevents sweeping");
        install(
            id,
            behavior,
            b"visit:",
            original.method.unwrap(),
            original.context,
        );
        assert!(
            id.send_named(gc, b"visit:", &[Operand::object(gc)])
                .is_err(),
            "visitor inactive outside marking"
        );
        assert!(
            id.send_named(
                gc,
                b"allocation:bytes:identity:",
                &[Operand::object(gc), Operand::word(0), Operand::word(1)]
            )
            .is_err()
        );
        assert!(
            id.send_named(gc, b"rootsDo:", &[Operand::entry(original)])
                .is_err(),
            "old native callback operands are rejected"
        );
        send(
            id,
            behavior,
            b"removeMethod:",
            &[Operand::object(id.intern_name(b"savedGCVisit:").unwrap())],
        );
        collect(id, gc);
        assert!(!present(id, dead));
    }
}
// Run in a child: retirement failure deliberately leaves this collector terminal.
unsafe fn release_failure(id: Protocol, gc: Oop, roots: Oop) {
    unsafe {
        let vt = behavior(id);
        retain(id, roots, vt);
        let selector = id.intern_name(b"releaseDetachedAllocation:").unwrap();
        let original = send(id, id.receiver, b"lookup", &[Operand::object(selector)])
            .as_entry()
            .unwrap();
        install(
            id,
            id.receiver,
            b"savedRelease:",
            original.method.unwrap(),
            original.context,
        );
        let at = id.intern_name(b"allocationAt:").unwrap();
        let collect_selector = id.intern_name(b"collectPrecisely").unwrap();
        let saved_release = id.intern_name(b"savedRelease:").unwrap();
        collect(id, gc);
        let dead = allocate(id, vt, size_of::<usize>());
        dead.cast::<usize>().write(0xabc123);
        let identity = send(id, id.receiver, b"identityOf:", &[Operand::object(dead)]);
        install(
            id,
            id.receiver,
            b"releaseDetachedAllocation:",
            refuse_visit,
            ptr::null_mut(),
        );
        assert_eq!(
            id.send_named(gc, b"collectPrecisely", &[])
                .unwrap_err()
                .detail,
            917
        );
        assert_eq!(
            id.send(id.receiver, at, &[Operand::word(dead.addr())])
                .unwrap()
                .kind(),
            native::UNIT
        );
        assert_eq!(
            dead.cast::<usize>().read(),
            0xabc123,
            "Id keeps detached storage on failed release send"
        );
        assert!(
            id.send(gc, collect_selector, &[]).is_err(),
            "retirement failure is terminal"
        );
        id.send(id.receiver, saved_release, &[identity]).unwrap();
        assert!(
            id.send(id.receiver, saved_release, &[identity]).is_err(),
            "release handles are one-use"
        );
    }
}
unsafe fn owner(
    id: Protocol,
    gc: Oop,
    roots: Oop,
    node_behavior: Oop,
    tag: usize,
) -> (Oop, &'static Owner) {
    unsafe {
        let state = Box::leak(Box::new(Owner {
            id,
            gc,
            behavior: node_behavior,
            edges: RefCell::new(HashMap::new()),
            visits: RefCell::new(Vec::new()),
            finalized: RefCell::new(Vec::new()),
            failure: Cell::new(false),
            ignored_failure: Cell::new(false),
            phase_checks: Cell::new(0),
            first: Cell::new(ptr::null_mut()),
            later: Cell::new(ptr::null_mut()),
            replacement: Cell::new(ptr::null_mut()),
            switch_behavior: Cell::new(ptr::null_mut()),
            retired_symbol: Cell::new(ptr::null_mut()),
            retired_behavior: Cell::new(ptr::null_mut()),
            tag,
        }));
        let behavior = behavior(id);
        install(
            id,
            behavior,
            b"referencesOf:do:",
            trace,
            ptr::from_mut(state).cast(),
        );
        install(
            id,
            behavior,
            b"finalize:",
            finalize,
            ptr::from_mut(state).cast(),
        );
        let object = allocate(id, behavior, 0);
        retain(id, roots, object);
        (object, state)
    }
}
#[test]
#[ignore = "scripts/native-message-test.sh builds the production modules"]
fn bootstrap_memory_preserves_tracing_finalization_and_live_dispatch() {
    unsafe {
        let id = id();
        let namespace = load(
            id,
            "bootstrap",
            &[Operand::bytes(directory().as_os_str().as_bytes())],
        );
        let named = |name: &[u8]| {
            object(send(
                id,
                namespace,
                b"at:",
                &[Operand::object(id.intern_name(name).unwrap())],
            ))
        };
        let roots = named(b"Roots");
        let gc = named(b"GC");
        if std::env::var_os("BIRNAM_GC_RELEASE_FAILURE_CHILD").is_some() {
            release_failure(id, gc, roots);
            return;
        }
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--include-ignored",
                "--exact",
                "bootstrap_memory_preserves_tracing_finalization_and_live_dispatch",
            ])
            .env("BIRNAM_GC_RELEASE_FAILURE_CHILD", "1")
            .output()
            .unwrap();
        assert!(
            child.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr)
        );
        assert_eq!(load(id, "collect", &[Operand::object(roots)]), gc);
        let nodes = behavior(id);
        retain(id, roots, nodes);
        visitor_dispatch(id, gc, nodes);
        let (owner_a, a) = owner(id, gc, roots, nodes, 1);
        let (owner_b, b) = owner(id, gc, roots, nodes, 2);
        send(
            id,
            gc,
            b"trace:with:",
            &[Operand::object(nodes), Operand::object(owner_a)],
        );
        let parent = allocate(id, nodes, 8);
        let child = allocate(id, nodes, 8);
        a.edges
            .borrow_mut()
            .insert(parent.addr(), vec![child, child]); // duplicate admission
        a.edges.borrow_mut().insert(child.addr(), vec![parent]); // cycle
        let token = retain(id, roots, parent);
        collect(id, gc);
        assert!(present(id, parent) && present(id, child));
        assert_eq!(
            a.visits
                .borrow()
                .iter()
                .filter(|&&v| v == parent.addr())
                .count(),
            1
        );
        assert_eq!(
            a.visits
                .borrow()
                .iter()
                .filter(|&&v| v == child.addr())
                .count(),
            1
        );
        // Replacement of an Id dependency is observed on the next collection.
        let selector = id.intern_name(b"referencesOf:do:").unwrap();
        let original = send(id, id.receiver, b"lookup", &[Operand::object(selector)])
            .as_entry()
            .unwrap();
        let counter = Box::leak(Box::new(Counted {
            original,
            calls: Cell::new(0),
        }));
        install(
            id,
            id.receiver,
            b"referencesOf:do:",
            counted,
            ptr::from_mut(counter).cast(),
        );
        collect(id, gc);
        assert!(counter.calls.get() > 0);
        install(
            id,
            id.receiver,
            b"referencesOf:do:",
            original.method.unwrap(),
            original.context,
        );
        // Trace errors, including ignored visitor failure, cannot sweep anything.
        let dead = allocate(id, nodes, 8);
        a.failure.set(true);
        assert!(id.send_named(gc, b"collectPrecisely", &[]).is_err());
        assert!(present(id, dead));
        a.failure.set(false);
        a.ignored_failure.set(true);
        assert!(id.send_named(gc, b"collectPrecisely", &[]).is_err());
        assert!(present(id, dead));
        a.ignored_failure.set(false);
        collect(id, gc);
        assert!(!present(id, dead));
        // Parent emits left then right: right runs first and replaces left's hook.
        let left_behavior = behavior(id);
        retain(id, roots, left_behavior);
        send(
            id,
            gc,
            b"trace:with:",
            &[Operand::object(left_behavior), Operand::object(owner_a)],
        );
        let left = allocate(id, left_behavior, 8);
        let right = allocate(id, nodes, 8);
        a.edges
            .borrow_mut()
            .insert(parent.addr(), vec![left, right]);
        a.first.set(right);
        a.switch_behavior.set(left_behavior);
        a.replacement.set(owner_b);
        a.visits.borrow_mut().clear();
        b.visits.borrow_mut().clear();
        collect(id, gc);
        assert!(a.visits.borrow().contains(&right.addr()));
        assert!(!a.visits.borrow().contains(&left.addr()));
        assert!(b.visits.borrow().contains(&left.addr()));
        a.switch_behavior.set(ptr::null_mut());
        // Dead snapshot creation order, live finalizer selection and retirement.
        let first = allocate(id, nodes, 8);
        first.cast::<usize>().write(0xcafe);
        let later = allocate(id, nodes, 8);
        later.cast::<usize>().write(0xcafe);
        let symbol = id.intern_name(b"weak-gc-symbol").unwrap();
        let dead_behavior = behavior(id);
        send(
            id,
            gc,
            b"trace:with:",
            &[Operand::object(dead_behavior), Operand::object(owner_a)],
        );
        // An opaque weak binding must disappear before the Symbol address is reused.
        send(
            id,
            id.receiver,
            b"addWeakBinding:to:as:",
            &[
                Operand::object(symbol),
                Operand::word(nodes.addr()),
                Operand::object(owner_a),
            ],
        );
        a.first.set(first);
        a.later.set(later);
        a.replacement.set(owner_b);
        a.retired_symbol.set(symbol);
        a.retired_behavior.set(dead_behavior);
        for value in [first, later] {
            send(
                id,
                gc,
                b"finalize:with:",
                &[Operand::object(value), Operand::object(owner_a)],
            );
        }
        collect(id, gc);
        assert_eq!(&*a.finalized.borrow(), &[(first.addr(), 1)]);
        assert_eq!(&*b.finalized.borrow(), &[(later.addr(), 2)]);
        assert!(
            !present(id, first)
                && !present(id, later)
                && !present(id, symbol)
                && !present(id, dead_behavior)
        );
        assert_eq!(
            send(
                id,
                id.receiver,
                b"bindingOf:in:",
                &[Operand::object(symbol), Operand::object(nodes)]
            )
            .kind(),
            native::UNIT
        );
        a.first.set(ptr::null_mut());
        a.later.set(ptr::null_mut());
        // No old finalizer/hook may apply to a reused address or new identity.
        for _ in 0..64 {
            allocate(id, nodes, 8);
        }
        collect(id, gc);
        assert_eq!(a.finalized.borrow().len(), 1);
        assert_eq!(b.finalized.borrow().len(), 1);
        // Default collection scans native roots; automatic budget collection uses it.
        let native_only = allocate(id, nodes, 8);
        send(id, gc, b"collect", &[]);
        assert!(present(id, core::hint::black_box(native_only)));
        let before = a.phase_checks.get();
        send(id, gc, b"budget:", &[Operand::word(1)]);
        let _one = allocate(id, nodes, 8);
        let _two = allocate(id, nodes, 8);
        assert!(a.phase_checks.get() > before);
        assert!(present(id, core::hint::black_box(native_only)));
        send(id, gc, b"budget:", &[Operand::word(1 << 20)]);
        // Drop the actual root: the old graph no longer retains its objects.
        release(id, roots, token);
        collect(id, gc);
        assert!(!present(id, parent) && !present(id, left) && !present(id, right));
        assert!(a.phase_checks.get() > 0 && b.phase_checks.get() > 0);
        // Churn formerly measured through Codegen's aggregate counters belongs
        // to Id's symbol owner. Root each batch, then reclaim its entire live set.
        collect(id, gc);
        let metrics = || {
            (
                send(id, id.receiver, b"symbolCount", &[])
                    .as_word()
                    .unwrap(),
                send(id, id.receiver, b"symbolNameBytes", &[])
                    .as_word()
                    .unwrap(),
            )
        };
        let baseline = metrics();
        for batch in 0..3 {
            let mut temporary = Vec::new();
            for index in 0..5000 {
                let symbol = id
                    .intern_name(format!("churn-{batch}-{index}").as_bytes())
                    .unwrap();
                temporary.push(retain(id, roots, symbol));
            }
            collect(id, gc);
            let live = metrics();
            assert!(live.0 >= baseline.0 + 5000 && live.1 > baseline.1);
            for root in temporary {
                release(id, roots, root);
            }
            collect(id, gc);
            assert_eq!(metrics(), baseline, "symbol batch {batch} leaked names");
        }

    }
}
