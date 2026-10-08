//! Generated-call hints never replace the first root or survive call exit.
mod support;
use birnam_abi::{Oop, borrowed_slice, id_protocol::{NativeEntry, Operand, Outcome, Protocol}};
use std::{cell::{Cell, RefCell}, ffi::c_void, os::unix::ffi::OsStrExt};
use support::*;
struct Hook {
    id: Protocol, gc: Oop, parent: Oop, marker: Oop,
    original: NativeEntry, created: RefCell<Vec<Oop>>, retained: Cell<usize>,
    marker_retained: Cell<usize>, fail_retain: Cell<bool>, zero_token: Cell<bool>, checkpoints: Cell<usize>,
}
unsafe extern "C" fn fresh(c: *mut c_void, _: Oop, _: *const Operand, n: usize) -> Outcome {
    assert_eq!(n, 0);
    let h = unsafe { &*c.cast::<Hook>() };
    let value = unsafe { allocate(h.id, h.parent, 0) };
    h.created.borrow_mut().push(value);
    Outcome::loaded(Operand::object(value))
}
unsafe extern "C" fn echo(_: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    let [value] = (unsafe { borrowed_slice(a, n) }).unwrap() else { panic!("echo") };
    Outcome::loaded(*value)
}
unsafe extern "C" fn checkpoint(c: *mut c_void, _: Oop, _: *const Operand, n: usize) -> Outcome {
    assert_eq!(n, 0);
    let h = unsafe { &*c.cast::<Hook>() };
    unsafe { collect(h.id, h.gc); }
    for &value in h.created.borrow().iter() {
        assert!(unsafe { present(h.id, value) }, "local lost through duplicate reuse or hint eviction");
    }
    assert!(unsafe { present(h.id, h.marker) }, "immutable capture cover lost");
    h.checkpoints.set(h.checkpoints.get() + 1);
    Outcome::loaded(Operand::object(h.marker))
}
unsafe extern "C" fn retain_hook(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    let h = unsafe { &*c.cast::<Hook>() };
    let value = object((unsafe { borrowed_slice(a, n) }).unwrap()[0]);
    if h.created.borrow().contains(&value) {
        h.retained.set(h.retained.get() + 1);
        if h.fail_retain.get() { return Outcome::failed(814, 23); }
        if h.zero_token.get() { return Outcome::loaded(Operand::word(0)); }
    }
    if value == h.marker { h.marker_retained.set(h.marker_retained.get() + 1); }
    unsafe { (h.original.method.unwrap())(h.original.context, r, a, n) }
}
#[test]
#[ignore = "requires rebuilt synchronous compiler and production owners"]
fn local_and_capture_aliases_reuse_only_live_call_protection() {
    unsafe {
        let id = id();
        let ns = load(id, "bootstrap", &[Operand::bytes(directory().as_os_str().as_bytes())]);
        let named = |n: &[u8]| object(send(id, ns, b"at:", &[Operand::object(id.intern_name(n).unwrap())]));
        let roots = named(b"Roots"); let gc = named(b"GC");
        let _floor = CompiledFloor::pin(id, ns, roots);
        let parent = behavior(id);
        let hook_object = allocate(id, parent, 0); let ht = retain(id, roots, hook_object);
        let marker = allocate(id, parent, 0); let mt = retain(id, roots, marker);
        let original = send(id, id.receiver, b"bind:to:", &[Operand::object(id.intern_name(b"retain:").unwrap()), Operand::object(roots)]).as_entry().unwrap();
        let mut hook = Hook {id, gc, parent, marker, original, created: RefCell::new(Vec::new()), retained: Cell::new(0), marker_retained: Cell::new(0), fail_retain: Cell::new(false), zero_token: Cell::new(false), checkpoints: Cell::new(0)};
        let context = (&raw mut hook).cast();
        install(id, parent, b"fresh", fresh, context);
        install(id, parent, b"echo:", echo, context);
        install(id, parent, b"checkpoint", checkpoint, context);
        let globals = named(b"Globals");
        for (n, value) in [(b"ReuseHook".as_slice(), hook_object), (b"ReuseMarker", marker)] {
            send(id, globals, b"at:put:", &[Operand::object(id.intern_name(n).unwrap()), Operand::object(value)]);
        }
        let mut source = String::from("let hook := [Globals at: #ReuseHook]; let marker := [Globals at: #ReuseMarker]; let probe := [Object clone];\n");
        source.push_str("[Reflect defineMethod: #run on: probe as: [Execution block: [:self | let a := [hook fresh]; [hook echo: a]; [hook echo: a]; [hook echo: marker]; [hook echo: marker]; [hook checkpoint]; #done] withNativeSignature: \"o\"]];\n");
        source.push_str("[Reflect defineMethod: #evict on: probe as: [Execution block: [:self |\n");
        for i in 0..40 { source.push_str(&format!("let a{i} := [hook fresh];\n")); }
        source.push_str("[hook echo: a0]; [hook echo: a39]; [hook checkpoint]; #done] withNativeSignature: \"o\"]]; [Globals at: #ReuseProbe put: probe]; true");
        let text = object(send(id, named(b"String"), b"fromBytes:", &[Operand::bytes(source.as_bytes())])); let tt = retain(id, roots, text);
        let path = std::env::temp_dir().join(format!("birnam-root-reuse-{}.dylib", std::process::id()));
        let output = object(send(id, named(b"String"), b"fromBytes:", &[Operand::bytes(path.as_os_str().as_bytes())])); let ot = retain(id, roots, output);
        let answer = object(send(id, named(b"Codegen"), b"compile:output:from:synchronousBodies:", &[Operand::object(text), Operand::object(output), Operand::object(output), Operand::object(named(b"true"))]));
        assert_eq!(object(send(id, answer, b"isError", &[])), named(b"false"));
        send(id, named(b"Codegen"), b"load:", &[Operand::object(output)]);
        let probe = object(send(id, globals, b"at:", &[Operand::object(id.intern_name(b"ReuseProbe").unwrap())])); let pt = retain(id, roots, probe);
        send(id, globals, b"at:put:", &[Operand::object(id.intern_name(b"ReuseMarker").unwrap()), Operand::object(named(b"nil"))]);
        release(id, roots, mt);
        let rb = object(send(id, id.receiver, b"behaviorOf:", &[Operand::object(roots)]));
        install(id, rb, b"retain:", retain_hook, context);
        let done = id.intern_name(b"done").unwrap();
        let dt = retain(id, roots, done);
        for _ in 0..2 {
            hook.created.borrow_mut().clear(); hook.retained.set(0); hook.marker_retained.set(0);
            assert_eq!(object(send(id, probe, b"run", &[])), done);
            assert_eq!(hook.retained.get(), 1, "fresh local needs exactly its first registration");
            assert_eq!(hook.marker_retained.get(), 0, "capture alias remains covered by live implementation Block");
            collect(id, gc);
            assert!(hook.created.borrow().iter().all(|&v| !present(id, v)), "private roots must drain at call exit");
        }
        hook.created.borrow_mut().clear(); hook.retained.set(0);
        assert_eq!(object(send(id, probe, b"evict", &[])), done);
        assert!(hook.retained.get() >= 40, "eviction must not skip first acquisitions");
        collect(id, gc);
        assert!(hook.created.borrow().iter().all(|&v| !present(id, v)));
        hook.created.borrow_mut().clear(); hook.fail_retain.set(true);
        let before = hook.checkpoints.get();
        let error = id.send_named(probe, b"run", &[]).unwrap_err();
        assert_eq!((error.status, error.detail), (814, 23));
        assert_eq!(hook.checkpoints.get(), before, "failed acquisition prevents later body sends");
        hook.fail_retain.set(false); hook.created.borrow_mut().clear();
        hook.zero_token.set(true);
        let error = id.send_named(probe, b"run", &[]).unwrap_err();
        assert_eq!(error.status, 1, "zero is not an acquired protection token");
        assert_eq!(hook.checkpoints.get(), before);
        hook.zero_token.set(false); hook.created.borrow_mut().clear();
        assert_eq!(object(send(id, probe, b"run", &[])), done);
        install(id, rb, b"retain:", original.method.unwrap(), original.context);
        for token in [dt, pt, ot, tt, ht] { release(id, roots, token); }
        std::fs::remove_file(path).unwrap();
    }
}
