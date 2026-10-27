//! Declared Method lifetime, explicit invocation and current custom lookup.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use birnam_host_support::{Context, Scope};
use std::{path::Path, ptr};

struct H<'a> {
    host: &'a Context,
    floor: Scope<'a>,
    fixture: Oop,
    monitor: Oop,
    gc: Oop,
}
impl<'a> H<'a> {
    fn key(&self, n: &[u8]) -> Oop {
        self.host.intern(n).unwrap()
    }
    fn named(&self, n: &[u8]) -> Oop {
        self.host.named(n).unwrap()
    }
    // The caller chooses the lifetime of returned objects explicitly.
    fn call(&self, r: Oop, n: &[u8], a: &[Oop]) -> Oop {
        self.host.send(r, n, a).unwrap()
    }
    fn method(&self, q: &Scope<'_>, n: &[u8], c: Oop) -> Oop {
        let template = q.call(self.fixture, b"methodAt:", &[self.key(n)]).unwrap();
        q.call(template, b"withContext:", &[c]).unwrap()
    }
    fn context(&self, q: &Scope<'_>, a: &[Oop]) -> Oop {
        q.call(self.host.id.root, b"sequence", a).unwrap()
    }
    fn object(&self, q: &Scope<'_>) -> Oop {
        q.call(self.named(b"Object"), b"clone", &[]).unwrap()
    }
    fn vt(&self, q: &Scope<'_>, v: Oop) -> Oop {
        q.call(self.host.id.root, b"ensurePrivateVtableFor:", &[v])
            .unwrap()
    }
    fn bind(&self, vt: Oop, n: &[u8], m: Oop) {
        self.call(vt, b"addMethod", &[self.key(n), m]);
    }
    fn remove(&self, vt: Oop, n: &[u8]) {
        self.call(vt, b"removeMethod:", &[self.key(n)]);
    }
    fn put(&self, c: Oop, i: usize, v: Oop) {
        self.call(c, b"at:put:", &[size(i).unwrap(), v]);
    }
    fn field(&self, c: Oop, i: usize) -> Oop {
        self.call(c, b"at:", &[size(i).unwrap()])
    }
    fn identity(&self, v: Oop) -> Oop {
        self.call(self.host.id.root, b"identityOf:", &[v])
    }
    fn present(&self, identity: Oop) -> bool {
        as_size(self.call(self.monitor, b"present", &[identity])).unwrap() == 1
    }
    fn collect(&self) {
        self.call(self.gc, b"collectPrecisely", &[]);
    }
}

#[test]
#[ignore = "requires production Oop modules"]
fn methods_retain_bindings_active_calls_arguments_and_exact_lookup_contexts() {
    let host = oop::bootstrap();
    let floor = host.scope();
    let gc = host.named(b"GC").unwrap();
    floor
        .call(gc, b"precisePayloads:", &[size(1).unwrap()])
        .unwrap();
    let path = std::env::temp_dir().join(format!(
        "birnam-execution-methods-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/execution-methods.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = host.load_native(&floor, &path, &[]).unwrap();
    let monitor = floor
        .call(host.named(b"Object").unwrap(), b"clone", &[])
        .unwrap();
    let h = H {
        host: &host,
        floor,
        fixture,
        monitor,
        gc,
    };
    let monitor_vt = h.vt(&h.floor, monitor);
    for n in [
        b"present".as_slice(),
        b"allocation:bytes:identity:",
        b"argumentIdentities",
    ] {
        h.bind(monitor_vt, n, h.method(&h.floor, n, ptr::null_mut()));
    }
    assert_eq!(
        host.load_native(
            &h.floor,
            &host.resources.lib_dir.join("libbirnam_execution.dylib"),
            &[host.namespace]
        )
        .unwrap(),
        h.named(b"Execution")
    );
    let instance = h.object(&h.floor);
    let vt = h.vt(&h.floor, instance);
    let selector = h.key(b"ownedProbe");
    let replacement = h.method(&h.floor, b"self", ptr::null_mut());
    let finalized = h.context(&h.floor, &[size(0).unwrap(), size(0).unwrap()]);
    let observer = h.object(&h.floor);
    h.bind(
        h.vt(&h.floor, observer),
        b"finalize:",
        h.method(&h.floor, b"finalize:", finalized),
    );
    let (owner, method, state, owner_id, method_id) = {
        let q = host.scope();
        let owner = h.object(&q);
        let identity = h.identity(owner);
        h.put(finalized, 1, identity);
        h.call(gc, b"finalize:with:", &[owner, observer]);
        let state = h.context(
            &q,
            &[
                owner,
                vt,
                selector,
                replacement,
                size(0).unwrap(),
                gc,
                size(0).unwrap(),
                finalized,
            ],
        );
        let method = h.method(&q, b"answer", state);
        h.bind(vt, b"ownedProbe", method);
        (owner, method, state, identity, h.identity(method))
    };
    h.collect();
    assert!(
        h.present(owner_id) && h.present(method_id),
        "binding retains Method and context owner"
    );
    assert_eq!(h.call(instance, b"ownedProbe", &[]), owner);
    h.put(state, 4, size(1).unwrap());
    assert_eq!(
        h.call(instance, b"ownedProbe", &[]),
        owner,
        "entered Method retains its context after replacement and GC"
    );
    // No collection occurs between return and reinstalling the still-live Method.
    h.bind(vt, b"ownedProbe", method);
    let captures = host.scope();
    let captured = captures.call(instance, b"methodFor:", &[selector]).unwrap();
    let captured_id = h.identity(captured);
    assert_eq!(h.call(captured, b"clone", &[]), captured);
    assert!(host.send(captured, b"clone", &[instance]).is_err());
    h.put(state, 4, size(1).unwrap());
    assert_eq!(h.call(instance, b"ownedProbe", &[]), owner);
    assert_eq!(h.call(instance, b"ownedProbe", &[]), instance);
    assert_eq!(h.call(captured, b"invokeOn:", &[instance]), owner);

    let fault = oop::hooks::Hooks::new(&host);
    let read_failure = fault.fault(91);
    {
        let q = host.scope();
        let sequence = h.object(&q);
        let seq_vt = h.vt(&q, sequence);
        let seq_state = h.context(&q, &[gc, h.named(b"Integer"), monitor, ptr::null_mut()]);
        for n in [b"sizeWord".as_slice(), b"atWord:"] {
            h.bind(seq_vt, n, h.method(&q, n, seq_state));
        }
        let last_id;
        {
            let result_scope = host.scope();
            let result = result_scope
                .call(captured, b"invokeOn:arguments:", &[instance, sequence])
                .unwrap();
            assert_eq!(as_size(h.call(result, b"word", &[])), Some(102));
            let ids = q.call(monitor, b"argumentIdentities", &[]).unwrap();
            last_id = h.field(ids, 2);
            h.collect();
            for i in 0..2 {
                assert!(!h.present(h.field(ids, i)), "completed argument leaked");
            }
            assert!(h.present(last_id));
        }
        h.collect();
        assert!(!h.present(last_id));
        h.put(seq_state, 3, read_failure);
        assert_eq!(
            unsafe {
                host.id.send(
                    captured,
                    h.key(b"invokeOn:arguments:"),
                    &[instance, sequence],
                )
            }
            .unwrap_err(),
            read_failure
        );
        let ids = q.call(monitor, b"argumentIdentities", &[]).unwrap();
        assert_eq!(as_size(h.call(ids, b"sizeWord", &[])), Some(2));
        h.collect();
        for i in 0..2 {
            assert!(!h.present(h.field(ids, i)), "failed argument read leaked");
        }
    }
    {
        let q = host.scope();
        let other_owner = h.object(&q);
        let other_state = h.context(
            &q,
            &[
                other_owner,
                vt,
                selector,
                replacement,
                size(0).unwrap(),
                gc,
                size(0).unwrap(),
                finalized,
            ],
        );
        let other = h.method(&q, b"answer", other_state);
        let nested = h.object(&q);
        h.bind(h.vt(&q, nested), b"ownedProbe", other);
        let custom = h.object(&q);
        let custom_vt = h.vt(&q, custom);
        let old_meta = q.call(host.id.root, b"behaviorOf:", &[custom_vt]).unwrap();
        let meta = q.call(old_meta, b"delegated", &[]).unwrap();
        let lookup_state = h.context(
            &q,
            &[
                captured,
                nested,
                selector,
                size(0).unwrap(),
                gc,
                h.named(b"true"),
            ],
        );
        h.bind(meta, b"lookup", h.method(&q, b"choose", lookup_state));
        h.call(
            host.id.root,
            b"reassociateBehaviorOf:with:",
            &[custom_vt, meta],
        );
        assert_eq!(h.call(custom, b"ownedProbe", &[]), owner);
        assert_eq!(
            as_size(h.field(other_state, 6)),
            Some(0),
            "nested lookup must not enter or replace the selected context"
        );
        h.put(lookup_state, 3, size(1).unwrap());
        assert!(h.call(custom_vt, b"lookup", &[selector]).is_null());
        h.put(lookup_state, 3, size(2).unwrap());
        assert!(
            host.send(custom, b"ownedProbe", &[]).is_err(),
            "non-Method selection is rejected"
        );
        h.call(
            host.id.root,
            b"reassociateBehaviorOf:with:",
            &[custom_vt, old_meta],
        );
    }
    drop(captures);
    h.remove(vt, b"ownedProbe");
    h.collect();
    assert!(!h.present(owner_id));
    assert!(!h.present(method_id));
    assert!(!h.present(captured_id));
    assert_eq!(as_size(h.field(finalized, 0)), Some(1));

    // Real Bootstrap loading retains its result independently of the binding.
    let path_string = h.floor.string(path.as_os_str().as_encoded_bytes()).unwrap();
    for cycle in [false, true] {
        let (owner_id, method_id, behavior_id) = {
            let q = host.scope();
            let owner = h.object(&q);
            let behavior = if cycle { h.vt(&q, owner) } else { vt };
            let method = q
                .call(h.named(b"Bootstrap"), b"loadNative:", &[path_string, owner])
                .unwrap();
            h.bind(behavior, b"ownedProbe", method);
            assert_eq!(
                as_size(h.call(h.named(b"Bootstrap"), b"release:", &[method])),
                Some(1)
            );
            (h.identity(owner), h.identity(method), h.identity(behavior))
        };
        h.collect();
        if !cycle {
            assert!(h.present(owner_id) && h.present(method_id));
            let returned = h.call(instance, b"ownedProbe", &[]);
            assert_eq!(h.identity(returned), owner_id);
            h.remove(vt, b"ownedProbe");
            h.collect();
        } else {
            assert!(
                !h.present(behavior_id),
                "unreachable binding cycle retains its vtable"
            );
        }
        assert!(!h.present(owner_id) && !h.present(method_id));
    }
    // Generic captured invocation preserves raw Oop results; explicit native
    // adapters and Machine execution own conversion to language Errors.
    {
        let q = host.scope();
        let other_failure = fault.fault(18);
        let mode = h.context(&q, &[size(0).unwrap(), read_failure, other_failure]);
        let method = h.method(&q, b"result", mode);
        assert!(h.call(method, b"invokeOn:", &[instance]).is_null());
        h.put(mode, 0, size(1).unwrap());
        assert_eq!(
            h.call(method, b"invokeOn:", &[instance]),
            size(123).unwrap()
        );
        for (i, expected) in [(2, read_failure), (3, other_failure)] {
            h.put(mode, 0, size(i).unwrap());
            assert_eq!(
                unsafe { host.id.send(method, h.key(b"invokeOn:"), &[instance]) }.unwrap_err(),
                expected
            );
        }
        for compile in [b"compile:".as_slice(), b"compileSynchronous:"] {
            let source = q.string(b"42").unwrap();
            let compiled = q.call(h.named(b"Codegen"), compile, &[source]).unwrap();
            q.check_result(compiled).unwrap();
            assert_eq!(h.call(compiled, b"clone", &[]), compiled);
        }
        // Capturing native lookup preserves its Method-or-absence result.
        let target = h.object(&q);
        let behavior = h.vt(&q, target);
        let old_meta = q.call(host.id.root, b"behaviorOf:", &[behavior]).unwrap();
        let raw = q
            .call(old_meta, b"methodFor:", &[h.key(b"lookup")])
            .unwrap();
        assert_ne!(raw, h.named(b"nil"));
        let meta = q.call(old_meta, b"delegated", &[]).unwrap();
        h.bind(meta, b"lookup", raw);
        h.bind(behavior, b"rawLookupProbe", replacement);
        let args = h.context(&q, &[h.key(b"rawLookupProbe")]);
        assert_eq!(
            q.call(raw, b"invokeOn:arguments:", &[behavior, args])
                .unwrap(),
            replacement,
            "captured lookup before reassociation"
        );
        h.call(
            host.id.root,
            b"reassociateBehaviorOf:with:",
            &[behavior, meta],
        );
        assert_eq!(
            h.call(behavior, b"lookup", &[h.key(b"rawLookupProbe")]),
            replacement
        );
        let reflected = q
            .call(raw, b"invokeOn:arguments:", &[behavior, args])
            .unwrap();
        assert_eq!(h.call(reflected, b"invokeOn:", &[instance]), instance);
    }
    h.collect();
    std::fs::remove_file(path).unwrap();
}
