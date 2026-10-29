//! Direct Method/dictionary operations without Store, Globals or Generation.
#[path = "support/minimal.rs"]
#[allow(dead_code)]
mod minimal;
use birnam_abi::{as_size, size};
use minimal::H;
use std::path::{Path, PathBuf};
#[test]
#[ignore = "scripts/minimal-mutation-test.py supplies an isolated floor and exported programs"]
fn direct_mutation_needs_no_store_generation_or_publication_owner() {
    let case = match std::env::var("BIRNAM_MUTATION_CASE") {
        Ok(case) => case,
        Err(_) => {
            let mut passed = true;
            for case in ["construction", "mutation"] {
                let status = std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--ignored",
                        "--exact",
                        "direct_mutation_needs_no_store_generation_or_publication_owner",
                        "--nocapture",
                    ])
                    .env("BIRNAM_MUTATION_CASE", case)
                    .status()
                    .unwrap();
                passed &= status.success();
            }
            assert!(passed, "all isolated mutation contracts must pass");
            return;
        }
    };

    let directory = PathBuf::from(std::env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").unwrap());
    let programs = PathBuf::from(std::env::var_os("BIRNAM_MUTATION_PROGRAMS").unwrap());
    for module in ["store", "relational", "generation", "bootstrap", "lang"] {
        assert!(!directory.join(format!("libbirnam_{module}.dylib")).exists());
    }
    let host = minimal::mutation_floor();
    let h = H::new(&host);
    let q = host.scope();
    let absent = || {
        for n in [
            b"Store".as_slice(),
            b"db",
            b"Relational",
            b"Generation",
            b"MethodPublication",
            b"Lang",
            b"Globals",
            b"Bootstrap",
        ] {
            assert!(
                h.call(host.namespace, b"at:", &[h.key(n)]).is_null(),
                "unexpected owner {}",
                String::from_utf8_lossy(n)
            );
        }
    };
    absent();
    assert!(h
        .call(
            host.id.root,
            b"bind:to:",
            &[h.key(b"methodChangesTo:"), host.id.root]
        )
        .is_null());
    let prototype = h.named(b"Object");
    let gc = h.named(b"GC");
    let nil = h.named(b"nil");
    let path = std::env::temp_dir().join(format!(
        "birnam-minimal-mutation-{}.dylib",
        std::process::id()
    ));
    assert!(std::process::Command::new("clang")
        .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/oop-host/minimal-mutation.c")
        )
        .arg("-o")
        .arg(&path)
        .status()
        .unwrap()
        .success());
    let fixture = host.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let method = |n: &[u8], context| {
        q.call(
            q.call(fixture, b"methodAt:", &[h.key(n)]).unwrap(),
            b"withContext:",
            &[context],
        )
        .unwrap()
    };
    let monitor = q.call(prototype, b"clone", &[]).unwrap();
    let mv = q
        .call(host.id.root, b"ensurePrivateVtableFor:", &[monitor])
        .unwrap();
    for n in [b"allocation:bytes:identity:".as_slice(), b"present:"] {
        q.call(
            mv,
            b"addMethod",
            &[h.key(n), method(n, std::ptr::null_mut())],
        )
        .unwrap();
    }
    let identity = |o| q.call(host.id.root, b"identityOf:", &[o]).unwrap();
    let present = |id| q.call(monitor, b"present:", &[id]).unwrap() == size(1).unwrap();
    let program = |index| {
        let bytes = std::fs::read(programs.join(format!("{index}.bin"))).unwrap();
        let bytes = h.call(
            h.named(b"Bytes"),
            b"fromBytes:",
            &[q.byte_values(&bytes).unwrap()],
        );
        let code = h.call(h.named(b"CodeObject"), b"fromCanonicalBytes:", &[bytes]);
        let captures = h.call(h.named(b"Vector"), b"new", &[]);
        h.call(
            h.named(b"Codegen"),
            b"materializeResumable:captures:",
            &[code, captures],
        )
    };
    let bind = |arity, body| {
        h.call(
            h.named(b"Reflect"),
            b"methodForArity:implementation:",
            &[h.integer(arity), body],
        )
    };
    let epoch = || as_size(h.call(host.id.root, b"bindingEpoch", &[])).unwrap();
    let direct = |vt, selector| {
        h.call(
            h.named(b"Execution"),
            b"directMethodFor:in:",
            &[selector, vt],
        )
    };
    let selector = h.key(b"answer");
    let one = program(0);
    let two = program(1);
    let before = epoch();
    let first = bind(0, one);
    let second = bind(0, two);
    assert_ne!(first, second);
    if case == "construction" {
        assert_eq!(
            epoch(),
            before,
            "binding construction must not edit a live dictionary"
        );
        return;
    }
    assert_eq!(case, "mutation");
    let parent = h.call(prototype, b"clone", &[]);
    let parent_vt = h.call(host.id.root, b"ensurePrivateVtableFor:", &[parent]);
    assert_eq!(
        h.call(parent_vt, b"addMethod:as:", &[first, selector]),
        first
    );
    let child = h.call(parent, b"clone", &[]);
    let child_vt = h.call(host.id.root, b"ensurePrivateVtableFor:", &[child]);
    assert_eq!(h.number(h.call(child, b"answer", &[])), 11);
    assert_eq!(direct(child_vt, selector), nil);
    h.call(child_vt, b"addMethod:as:", &[second, selector]);
    for _ in 0..32 {
        assert_eq!(h.number(h.call(child, b"answer", &[])), 22);
    }
    assert_eq!(h.number(h.call(parent, b"answer", &[])), 11);
    let captured = h.call(child, b"methodFor:", &[selector]);
    let before = epoch();
    assert_eq!(h.call(child_vt, b"removeMethod:", &[selector]), second);
    assert!(epoch() > before);
    assert_eq!(h.number(h.call(child, b"answer", &[])), 11);
    let args = h.call(h.named(b"Vector"), b"new", &[]);
    assert_eq!(
        h.number(h.call(captured, b"invokeOn:arguments:", &[child, args])),
        22
    );
    let before = epoch();
    assert!(h.call(child_vt, b"removeMethod:", &[selector]).is_null());
    assert!(epoch() >= before);
    h.call(child_vt, b"addMethod:as:", &[second, selector]);
    let before = epoch();
    h.call(child_vt, b"addMethod:as:", &[first, selector]);
    h.call(child_vt, b"addMethod:as:", &[second, selector]);
    assert!(
        epoch() > before,
        "the existing epoch observes remove/replacement cycles"
    );
    let iv = h.call(host.id.root, b"ensurePrivateVtableFor:", &[one]);
    let value = h.integer(33);
    h.call(
        iv,
        b"addMethod",
        &[h.key(b"value:"), method(b"fixed", value)],
    );
    assert_eq!(
        h.number(h.call(first, b"invokeOn:arguments:", &[child, args])),
        11
    );
    h.call(iv, b"removeMethod:", &[h.key(b"value:")]);
    assert_eq!(
        h.number(h.call(first, b"invokeOn:arguments:", &[child, args])),
        11
    );
    let mark = h.mark();
    let body = program(2);
    let entered = bind(0, body);
    let entered_id = identity(entered);
    let body_id = identity(body);
    h.call(child_vt, b"addMethod:as:", &[entered, selector]);
    h.clear_from(mark);
    let state = q
        .call(
            host.id.root,
            b"sequence",
            &[
                gc,
                child_vt,
                selector,
                second,
                entered_id,
                body_id,
                size(0).unwrap(),
                monitor,
            ],
        )
        .unwrap();
    h.call(
        child_vt,
        b"addMethod",
        &[h.key(b"replaceNow"), method(b"switch", state)],
    );
    assert_eq!(h.number(h.call(child, b"answer", &[])), 11);
    assert_eq!(
        as_size(q.call(state, b"at:", &[size(6).unwrap()]).unwrap()),
        Some(1)
    );
    assert_eq!(h.number(h.call(child, b"answer", &[])), 22);
    h.collect();
    assert!(
        !present(entered_id) && !present(body_id),
        "replaced Method and body must collect after the entered call ends"
    );
    let body = program(3);
    let implementation = bind(1, body);
    let argument = h.call(prototype, b"clone", &[]);
    let echo = h.key(b"echo:");
    h.call(child_vt, b"addMethod:as:", &[implementation, echo]);
    assert_eq!(h.call(child, b"echo:", &[argument]), argument);
    let reflect = h.named(b"Reflect");
    let ordinary = h.key(b"ordinaryFacade");
    assert_eq!(
        h.call(reflect, b"defineMethod:on:as:", &[ordinary, child, one]),
        child
    );
    assert_eq!(h.number(h.call(child, b"ordinaryFacade", &[])), 11);
    assert_eq!(
        h.call(reflect, b"removeMethod:on:", &[ordinary, child]),
        child
    );
    assert_eq!(direct(child_vt, ordinary), nil);
    let native = h.key(b"nativeOrdinary");
    h.call(child_vt, b"addMethod", &[native, method(b"fixed", value)]);
    assert_eq!(
        h.call(reflect, b"removeMethod:on:", &[native, child]),
        child
    );
    assert_eq!(direct(child_vt, native), nil);
    absent();
    h.clear_from(0);
}
