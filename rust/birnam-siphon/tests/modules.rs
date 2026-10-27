//! Native initialization, Namespace, Roots and Boolean policy through Oop messages.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, id_protocol::Protocol, size};
use std::{path::Path, ptr};
// Only the rejection path runs on this thread; the creating thread waits.
struct ForeignThreadProbe {
    id: Protocol,
    selector: Oop,
}
unsafe impl Send for ForeignThreadProbe {}
impl ForeignThreadProbe {
    fn invoke(self) -> usize {
        unsafe { self.id.send(self.id.root, self.selector, &[]) }
            .unwrap_err()
            .addr()
    }
}
#[test]
#[ignore = "requires production Oop modules"]
fn bootstrap_prefix_uses_live_messages_and_retains_nested_results() {
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let key = |n: &[u8]| hooks.symbol(n);
    let call = |r, n: &[u8], a: &[Oop]| h.send(r, n, a).unwrap();
    let vt = |v| call(h.id.root, b"behaviorOf:", &[v]);
    let set = |v, n: &[u8], m| {
        call(v, b"addMethod", &[key(n), m]);
    };
    let count = key(b"allocationCount");
    let foreign = ForeignThreadProbe {
        id: h.id,
        selector: count,
    };
    assert_eq!(
        std::thread::spawn(move || foreign.invoke()).join().unwrap(),
        birnam_abi::object_reference(h.id.invalid).addr()
    );
    assert!(as_size(unsafe { h.id.send(h.id.root, count, &[]) }.unwrap()).is_some());
    let load = |n: &str, a: &[Oop]| {
        h.load_native(
            &q,
            &h.resources.lib_dir.join(format!("libbirnam_{n}.dylib")),
            a,
        )
        .unwrap()
    };
    assert_eq!(load("root_registry", &[]), h.roots);
    assert_eq!(load("namespace", &[h.roots]), h.namespace);
    let name = key(b"sample");
    let prototype = h.named(b"Object").unwrap();
    let value = q.call(prototype, b"clone", &[]).unwrap();
    assert!(call(h.namespace, b"at:", &[name]).is_null());
    assert_eq!(call(h.namespace, b"at:put:", &[name, value]), value);
    assert_eq!(call(h.namespace, b"at:", &[name]), value);
    assert_eq!(call(h.namespace, b"nameOf:", &[value]), name);
    assert!(call(h.namespace, b"nameAt:", &[size(usize::MAX >> 1).unwrap()]).is_null());
    assert!(h.send(h.namespace, b"at:", &[value]).is_err());
    let path = std::env::temp_dir().join(format!("birnam-modules-{}.dylib", std::process::id()));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/modules.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = h.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let bootstrap = h.named(b"Bootstrap").unwrap();
    let context = hooks.context(&[h.roots, bootstrap, q.string(b"nested").unwrap()]);
    let method = |n: &[u8], c| {
        q.call(
            q.call(fixture, b"methodAt:", &[key(n)]).unwrap(),
            b"withContext:",
            &[c],
        )
        .unwrap()
    };
    let root_vt = vt(h.id.root);
    let original = call(root_vt, b"lookup", &[key(b"isSymbol:")]);
    set(root_vt, b"moduleOriginalSymbol:", original);
    let symbol_calls = hooks.context(&[key(b"moduleOriginalSymbol:"), size(0).unwrap()]);
    set(root_vt, b"isSymbol:", method(b"forward", symbol_calls));
    let at = key(b"at:");
    hooks.put(symbol_calls, 1, size(0).unwrap());
    assert_eq!(
        unsafe { h.id.send(h.namespace, at, &[name]) }.unwrap(),
        value
    );
    assert_eq!(hooks.count(symbol_calls, 1), 1);
    set(root_vt, b"isSymbol:", original);
    call(root_vt, b"removeMethod:", &[key(b"moduleOriginalSymbol:")]);
    let visitor = q.call(prototype, b"clone", &[]).unwrap();
    let visitor_vt = q
        .call(h.id.root, b"ensurePrivateVtableFor:", &[visitor])
        .unwrap();
    for n in [
        b"visit:".as_slice(),
        b"binding:value:",
        b"configureVisit",
        b"reset",
        b"containsBinding",
        b"bindingKeyCount",
        b"rootCountFor:",
        b"retainedToken",
        b"configureLoad",
        b"dispose",
    ] {
        set(visitor_vt, n, method(n, context));
    }
    call(h.namespace, b"bindingsDo:", &[visitor]);
    assert_eq!(
        as_size(call(visitor, b"containsBinding", &[name, value])),
        Some(1)
    );
    let missing = key(b"inventoryAbsent");
    let bound_nil = key(b"inventoryNil");
    let nil = h.named(b"nil").unwrap();
    call(h.namespace, b"at:put:", &[missing, ptr::null_mut()]);
    call(h.namespace, b"at:put:", &[bound_nil, nil]);
    call(visitor, b"reset", &[]);
    call(h.namespace, b"liveBindingsDo:", &[visitor]);
    assert_eq!(
        as_size(call(visitor, b"containsBinding", &[name, value])),
        Some(1)
    );
    assert_eq!(
        as_size(call(visitor, b"bindingKeyCount", &[missing])),
        Some(0)
    );
    assert_eq!(
        as_size(call(visitor, b"containsBinding", &[bound_nil, nil])),
        Some(1)
    );
    let failure = hooks.context(&[hooks.fault(79), size(0).unwrap()]);
    set(
        visitor_vt,
        b"binding:value:",
        hooks.method(b"deny", failure),
    );
    assert!(h.send(h.namespace, b"liveBindingsDo:", &[visitor]).is_err());
    set(
        visitor_vt,
        b"binding:value:",
        method(b"binding:value:", context),
    );
    call(visitor, b"reset", &[]);
    call(h.namespace, b"liveBindingsDo:", &[visitor]);
    assert_eq!(
        as_size(call(visitor, b"containsBinding", &[name, value])),
        Some(1)
    );
    assert!(h.send(h.namespace, b"bindingsDo:", &[]).is_err());
    let ns_vt = vt(h.namespace);
    let original = call(ns_vt, b"lookup", &[key(b"rootsDo:")]);
    set(ns_vt, b"moduleOriginalRoots:", original);
    let roots_calls = hooks.context(&[key(b"moduleOriginalRoots:"), size(0).unwrap()]);
    set(ns_vt, b"rootsDo:", method(b"forward", roots_calls));
    let config = key(b"configureVisit");
    let roots_do = key(b"rootsDo:");
    let reset = key(b"reset");
    let root_count = key(b"rootCountFor:");
    let retained = key(b"retainedToken");
    let release = key(b"release:");
    hooks.put(roots_calls, 1, size(0).unwrap());
    let extra = h.send(prototype, b"clone", &[]).unwrap();
    unsafe { h.id.send(visitor, config, &[extra, size(1).unwrap()]) }.unwrap();
    unsafe { h.id.send(h.roots, roots_do, &[visitor]) }.unwrap();
    let observed = |v| as_size(unsafe { h.id.send(visitor, root_count, &[v]) }.unwrap()).unwrap();
    assert_eq!(hooks.count(roots_calls, 1), 1);
    for v in [h.namespace, name, value] {
        assert!(observed(v) > 0);
    }
    assert_eq!(observed(extra), 0);
    unsafe { h.id.send(visitor, reset, &[]) }.unwrap();
    unsafe { h.id.send(h.roots, roots_do, &[visitor]) }.unwrap();
    assert!(observed(extra) > 0);
    let token = unsafe { h.id.send(visitor, retained, &[]) }.unwrap();
    assert_eq!(
        as_size(unsafe { h.id.send(h.roots, release, &[token]) }.unwrap()),
        Some(1)
    );
    assert_eq!(
        as_size(unsafe { h.id.send(h.roots, release, &[token]) }.unwrap()),
        Some(0)
    );
    set(ns_vt, b"rootsDo:", original);
    call(ns_vt, b"removeMethod:", &[key(b"moduleOriginalRoots:")]);
    let truth = load("boolean", &[h.namespace]);
    let falsity = h.named(b"false").unwrap();
    assert_eq!(h.named(b"Boolean").unwrap(), truth);
    assert_eq!(h.named(b"true").unwrap(), truth);
    assert_eq!(h.named(b"False").unwrap(), falsity);
    assert_ne!(truth, falsity);
    assert_ne!(truth, nil);
    assert!(load("nil", &[h.namespace]).is_null());
    for (v, opposite) in [(truth, falsity), (falsity, truth)] {
        assert_eq!(call(v, b"not", &[]), opposite);
        for other in [truth, falsity, nil] {
            assert_eq!(
                call(v, b"==", &[other]),
                if v == other { truth } else { falsity }
            );
            assert_eq!(
                call(v, b"&", &[other]),
                if v == truth { other } else { falsity }
            );
            assert_eq!(
                call(v, b"|", &[other]),
                if v == truth { truth } else { other }
            );
        }
    }
    for v in [truth, falsity, nil, prototype] {
        assert_eq!(
            call(v, b"isNil", &[]),
            if v == nil { truth } else { falsity }
        );
        assert_eq!(
            call(v, b"notNil", &[]),
            if v == nil { falsity } else { truth }
        );
    }
    let branch = q.call(prototype, b"clone", &[]).unwrap();
    let bvt = q
        .call(h.id.root, b"ensurePrivateVtableFor:", &[branch])
        .unwrap();
    let branch_state = hooks.context(&[nil, size(0).unwrap()]);
    set(bvt, b"value", hooks.method(b"fixedCount", branch_state));
    let unused = q.call(prototype, b"clone", &[]).unwrap();
    let uvt = q
        .call(h.id.root, b"ensurePrivateVtableFor:", &[unused])
        .unwrap();
    let untouched = hooks.context(&[hooks.fault(80), size(0).unwrap()]);
    set(uvt, b"value", hooks.method(b"deny", untouched));
    for (receiver, n, args, answer, called) in [
        (
            truth,
            b"ifTrue:ifFalse:".as_slice(),
            vec![branch, unused],
            nil,
            true,
        ),
        (falsity, b"ifTrue:ifFalse:", vec![unused, branch], nil, true),
        (truth, b"ifTrue:", vec![branch], nil, true),
        (falsity, b"ifTrue:", vec![unused], nil, false),
        (truth, b"ifFalse:", vec![unused], nil, false),
        (falsity, b"ifFalse:", vec![branch], nil, true),
        (truth, b"and:", vec![branch], nil, true),
        (falsity, b"and:", vec![unused], falsity, false),
        (truth, b"or:", vec![unused], truth, false),
        (falsity, b"or:", vec![branch], nil, true),
    ] {
        let before = hooks.count(branch_state, 1);
        assert_eq!(call(receiver, n, &args), answer);
        assert_eq!(hooks.count(branch_state, 1) - before, usize::from(called));
    }
    assert_eq!(hooks.count(untouched, 1), 0);
    let replaced = hooks.context(&[truth, size(0).unwrap()]);
    set(bvt, b"value", hooks.method(b"fixedCount", replaced));
    assert_eq!(call(falsity, b"or:", &[branch]), truth);
    assert_eq!(hooks.count(replaced, 1), 1);
    assert!(h.send(truth, b"ifTrue:", &[]).is_err());
    assert_eq!(call(truth, b"&", &[size(0).unwrap()]), size(0).unwrap());
    let bool_vt = vt(truth);
    let original = call(bool_vt, b"lookup", &[key(b"not")]);
    set(bool_vt, b"moduleOriginalNot", original);
    let bool_calls = hooks.context(&[key(b"moduleOriginalNot"), size(0).unwrap()]);
    set(bool_vt, b"not", method(b"forward", bool_calls));
    assert_eq!(load("boolean", &[h.namespace]), truth);
    assert_eq!(call(truth, b"not", &[]), falsity);
    assert_eq!(hooks.count(bool_calls, 1), 1);
    set(bool_vt, b"not", original);
    call(bool_vt, b"removeMethod:", &[key(b"moduleOriginalNot")]);
    assert_eq!(
        load(
            "bootstrap",
            &[q.string(h.resources.lib_dir.as_os_str().as_encoded_bytes())
                .unwrap()]
        ),
        h.namespace
    );
    let ns_path = q
        .string(
            h.resources
                .lib_dir
                .join("libbirnam_namespace.dylib")
                .as_os_str()
                .as_encoded_bytes(),
        )
        .unwrap();
    for _ in 0..2 {
        assert_eq!(
            call(bootstrap, b"loadNative:", &[ns_path, h.roots]),
            h.namespace
        );
    }
    for expected in [1, 1, 0] {
        assert_eq!(
            as_size(call(bootstrap, b"release:", &[h.namespace])),
            Some(expected)
        );
    }
    assert!(
        h.send(
            bootstrap,
            b"loadNative:",
            &[q.string(b"/no/such/birnam/module").unwrap(), h.namespace]
        )
        .is_err()
    );
    assert_eq!(
        as_size(call(bootstrap, b"release:", &[h.namespace])),
        Some(0)
    );
    let name_vt = call(h.id.root, b"nameBehavior", &[]);
    let loader = call(name_vt, b"lookup", &[key(b"loadNative")]);
    let nested_loader = method(b"reentrantLoad", context);
    let outer_path = q.string(b"outer").unwrap();
    let inputs = h.scope();
    let outer = inputs.call(prototype, b"clone", &[]).unwrap();
    let nested = inputs.call(prototype, b"clone", &[]).unwrap();
    call(visitor, b"configureLoad", &[nested]);
    set(name_vt, b"loadNative", nested_loader);
    assert_eq!(call(bootstrap, b"loadNative:", &[outer_path, outer]), outer);
    set(name_vt, b"loadNative", loader);
    drop(inputs);
    unsafe {
        h.id.send(visitor, config, &[ptr::null_mut(), size(0).unwrap()])
    }
    .unwrap();
    unsafe { h.id.send(h.roots, roots_do, &[visitor]) }.unwrap();
    for v in [outer, nested] {
        assert_eq!(observed(v), 1, "only Bootstrap result retention remains");
        assert_eq!(
            as_size(unsafe { h.id.send(bootstrap, release, &[v]) }.unwrap()),
            Some(1)
        );
    }
    unsafe { h.id.send(visitor, reset, &[]) }.unwrap();
    unsafe { h.id.send(h.roots, roots_do, &[visitor]) }.unwrap();
    assert_eq!(observed(outer), 0);
    assert_eq!(observed(nested), 0);
    call(visitor, b"dispose", &[]);
}
