//! Authored task scheduling through Oop messages and actual native barriers.
use birnam_abi::{self as abi, Oop};
use std::{cell::RefCell, env, os::unix::ffi::OsStrExt, path::PathBuf};

#[test]
#[ignore = "scripts/host-cli-test.sh --group tasks supplies installed release modules"]
fn authored_task_scheduling_preserves_native_boundaries() {
    let directory = PathBuf::from(env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").unwrap());
    let bootstrap = directory.join("libbirnam_bootstrap.dylib");
    let fixture =
        env::temp_dir().join(format!("birnam-task-observer-{}.dylib", std::process::id()));
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/oop-continuation/native.c");
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2"])
            .arg(source)
            .arg("-o")
            .arg(&fixture)
            .status()
            .unwrap()
            .success()
    );
    let names: &[&[u8]] = &[
        b"sequence",
        b"at:",
        b"at:put:",
        b"Roots",
        b"retain:",
        b"release:",
        b"String",
        b"fromBytes:",
        b"Codegen",
        b"evalPersisting:",
        b"isError",
        b"false",
        b"Integer",
        b"wordOf:",
        b"ensurePrivateVtableFor:",
        b"methodAt:",
        b"withContext:",
        b"addMethod",
        b"observe:",
        b"failTask:",
        b"ensureTask:body:",
        b"mustNotRecord",
        b"Globals",
        b"NativeTaskObserver",
        b"GC",
        b"precisePayloads:",
    ];
    let mut seeds = vec![
        bootstrap.as_os_str().as_bytes(),
        directory.as_os_str().as_bytes(),
        fixture.as_os_str().as_bytes(),
    ];
    seeds.extend_from_slice(names);
    let id = birnam_siphon::load(
        directory.join("libbirnam_id.dylib").as_os_str().as_bytes(),
        &seeds,
    )
    .unwrap();
    unsafe {
        let symbol = |name: &[u8]| {
            let index = names.iter().position(|n| *n == name).unwrap();
            id.send(id.literal(index + 3).unwrap(), id.intern, &[])
                .unwrap()
        };
        let send = |receiver, name: &[u8], args: &[Oop]| {
            id.send(receiver, symbol(name), args)
                .unwrap_or_else(|e| panic!("{}: {e:?}", String::from_utf8_lossy(name)))
        };
        let args = send(id.root, b"sequence", &[id.literal(1).unwrap()]);
        let ns = id.send(id.literal(0).unwrap(), id.load, &[args]).unwrap();
        let named = |name: &[u8]| send(ns, b"at:", &[symbol(name)]);
        let roots = named(b"Roots");
        let tokens = RefCell::new(Vec::new());
        let keep = |value| {
            tokens.borrow_mut().push(send(roots, b"retain:", &[value]));
            value
        };
        for name in names {
            keep(symbol(name));
        }
        send(named(b"GC"), b"precisePayloads:", &[abi::size(1).unwrap()]);
        let text = |bytes: &[u8]| {
            let data: Vec<_> = bytes
                .iter()
                .map(|b| abi::size(*b as usize).unwrap())
                .collect();
            let data = send(id.root, b"sequence", &data);
            keep(send(named(b"String"), b"fromBytes:", &[data]))
        };
        let eval = |source: &[u8]| {
            let result = keep(send(named(b"Codegen"), b"evalPersisting:", &[text(source)]));
            assert_eq!(
                send(result, b"isError", &[]),
                named(b"false"),
                "{}",
                String::from_utf8_lossy(source)
            );
            result
        };
        eval(include_bytes!("../../../modules/modules.birnam"));
        eval(include_bytes!("../../../modules/stdlib.birnam"));
        eval(include_bytes!("../../../modules/stackless-control.birnam"));
        let args = send(id.root, b"sequence", &[]);
        let image = keep(id.send(id.literal(2).unwrap(), id.load, &[args]).unwrap());
        std::fs::remove_file(&fixture).unwrap();
        let observer = eval(b"[Object clone]");
        let context = keep(send(
            id.root,
            b"sequence",
            &[eval(b"[| nil]"), symbol(b"mustNotRecord")],
        ));
        let vt = keep(send(id.root, b"ensurePrivateVtableFor:", &[observer]));
        for name in [b"observe:".as_slice(), b"failTask:", b"ensureTask:body:"] {
            let method = keep(send(image, b"methodAt:", &[symbol(name)]));
            let method = keep(send(method, b"withContext:", &[context]));
            send(vt, b"addMethod", &[symbol(name), method]);
        }
        send(
            named(b"Globals"),
            b"at:put:",
            &[symbol(b"NativeTaskObserver"), observer],
        );
        eval(include_bytes!("../../../scripts/task-control-smoke.birnam"));
        eval(b"[TaskCheck run]");
        let count = |source: &[u8]| {
            abi::as_size(send(named(b"Integer"), b"wordOf:", &[eval(source)])).unwrap()
        };
        assert_eq!(count(b"[TaskCheckFailures value]"), 0);
        assert_eq!(count(b"[TaskChecks value]"), 177);
        for token in tokens.into_inner().into_iter().rev() {
            send(roots, b"release:", &[token]);
        }
    }
}
