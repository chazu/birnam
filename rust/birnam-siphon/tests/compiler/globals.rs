//! Generated global accesses preserve current Globals and explicit Store history.
use super::*;
use std::{path::Path, ptr};
pub(super) fn check(h: &H<'_>) {
    h.reclaimed();
    let q = h.host.scope();
    for name in ["store", "relational"] {
        let owner = h
            .host
            .load_native(
                &q,
                &h.host
                    .resources
                    .lib_dir
                    .join(format!("native/libbirnam_{name}.dylib")),
                &[h.host.namespace],
            )
            .unwrap();
        q.check_result(owner).unwrap();
    }
    let library = std::env::temp_dir().join(format!(
        "birnam-compiler-globals-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/compiler-globals.c")
            )
            .arg("-o")
            .arg(&library)
            .status()
            .unwrap()
            .success()
    );
    let fixture = h.host.load_native(&q, &library, &[]).unwrap();
    std::fs::remove_file(library).unwrap();
    let globals = h.named(b"Globals");
    let store = h.named(b"Store");
    let nil = h.named(b"nil");
    let vt = h.vt(globals);
    let read = |n: &[u8]| h.call(globals, b"at:", &[h.key(n)]);
    let run = |b| h.call(b, b"value", &[]);
    let number = |v| h.call(h.named(b"Integer"), b"fromWord:", &[size(v).unwrap()]);
    let save = |n: &[u8], alias: &[u8]| {
        let m = h.call(vt, b"lookup", &[h.key(n)]);
        h.call(vt, b"addMethod", &[h.key(alias), m]);
        m
    };
    let restore = |n: &[u8], alias: &[u8], m| {
        h.call(vt, b"addMethod", &[h.key(n), m]);
        h.call(vt, b"removeMethod:", &[h.key(alias)]);
    };
    let v = h.eval(b"let sourceGlobal := 41; sourceGlobal");
    assert_eq!(h.integer(v), 41);
    assert_eq!(read(b"sourceGlobal"), v);
    assert!(
        h.call(h.host.namespace, b"at:", &[h.key(b"sourceGlobal")])
            .is_null()
    );
    let reader = h.block(h.compile(b"sourceGlobal"));
    assert_eq!(h.integer(run(reader)), 41);
    let value = number(42);
    h.publish(b"sourceGlobal", value);
    assert_eq!(run(reader), value);
    let fallback = number(99);
    h.bind(b"sourceGlobal", fallback);
    assert_eq!(run(reader), value);
    h.publish(b"sourceGlobal", nil);
    assert_eq!(run(reader), fallback);
    assert_eq!(
        h.call(globals, b"includes:", &[h.key(b"sourceGlobal")]),
        h.named(b"true")
    );
    assert_eq!(h.eval(b"missingGlobalForCompilerTest"), nil);
    assert_eq!(h.eval(b"let sourceNil := nil; sourceNil"), nil);
    assert_eq!(
        h.call(globals, b"includes:", &[h.key(b"sourceNil")]),
        h.named(b"true")
    );
    assert_eq!(h.integer(h.eval(b"let lexicalGlobal := 10; [[:arg | let lexicalGlobal := arg; let onlyLocal := 7; [lexicalGlobal + onlyLocal]] value: 35]")),42);
    assert_eq!(h.integer(read(b"lexicalGlobal")), 10);
    for n in [b"onlyLocal".as_slice(), b"arg"] {
        assert_eq!(
            h.call(globals, b"includes:", &[h.key(n)]),
            h.named(b"false")
        );
    }
    h.eval(b"let historiedSourceGlobal := 11; [db assert: #historiedSourceGlobal attr: #history value: true]; [db assert: #__globals__ attr: #historiedSourceGlobal value: historiedSourceGlobal]");
    let basis = h.call(store, b"basis", &[]);
    h.eval(b"let historiedSourceGlobal := 22");
    assert_eq!(
        h.integer(h.eval(b"[db currentValue: #__globals__ attr: #historiedSourceGlobal]")),
        11
    );
    h.eval(b"[db assert: #__globals__ attr: #historiedSourceGlobal value: 99]");
    assert_eq!(
        h.integer(h.call(
            store,
            b"valueOf:attr:asOf:",
            &[
                h.key(b"__globals__"),
                h.key(b"historiedSourceGlobal"),
                basis
            ]
        )),
        11
    );
    assert_eq!(h.integer(read(b"historiedSourceGlobal")), 22);
    assert_eq!(
        h.integer(h.eval(
            b"[[db findAll: ?v where: #[ #[#__globals__ #historiedSourceGlobal ?v] ]] at: 0]"
        )),
        99
    );
    let original = save(b"at:", b"savedCompilerGlobalAt:");
    let context = h.hooks.context(&[
        h.named(b"GC"),
        h.key(b"savedCompilerGlobalAt:"),
        h.key(b"sourceGlobal"),
        value,
        size(0).unwrap(),
        ptr::null_mut(),
    ]);
    let replacement = q
        .call(
            q.call(fixture, b"methodAt:", &[h.key(b"replacing")])
                .unwrap(),
            b"withContext:",
            &[context],
        )
        .unwrap();
    h.call(vt, b"addMethod", &[h.key(b"at:"), replacement]);
    assert_eq!(run(reader), value);
    assert_eq!(h.hooks.count(context, 4), 1);
    let fault = h.hooks.fault(917);
    h.hooks.put(context, 5, fault);
    assert_eq!(
        unsafe { h.host.id.send(reader, h.key(b"value"), &[]) }.unwrap_err(),
        fault
    );
    restore(b"at:", b"savedCompilerGlobalAt:", original);
    let error = h.call(
        h.named(b"Error"),
        b"kind:message:",
        &[
            h.key(b"historyResourceLimit"),
            h.string(b"refused global write"),
        ],
    );
    let setter = save(b"at:put:", b"savedCompilerGlobalAt:put:");
    for (i, v) in [
        (1, h.key(b"savedCompilerGlobalAt:put:")),
        (2, h.key(b"refusedGlobal")),
        (3, error),
        (4, size(0).unwrap()),
        (5, ptr::null_mut()),
    ] {
        h.hooks.put(context, i, v);
    }
    h.call(vt, b"addMethod", &[h.key(b"at:put:"), replacement]);
    let writer=h.block(h.compile(b"let precedingGlobal := 7; let refusedGlobal := [Object clone]; [Globals at: #afterRefusedGlobal put: true]; 99"));
    assert_eq!(run(writer), error);
    assert_eq!(h.hooks.count(context, 4), 1);
    assert_eq!(h.integer(read(b"precedingGlobal")), 7);
    assert_eq!(read(b"refusedGlobal"), nil);
    assert_eq!(read(b"afterRefusedGlobal"), nil);
    h.hooks.put(context, 3, nil);
    assert_eq!(run(writer), nil);
    assert_eq!(read(b"afterRefusedGlobal"), nil);
    h.hooks.put(context, 5, fault);
    assert_eq!(
        unsafe { h.host.id.send(writer, h.key(b"value"), &[]) }.unwrap_err(),
        fault
    );
    restore(b"at:put:", b"savedCompilerGlobalAt:put:", setter);
    h.hooks.put(context, 2, h.key(b"unusedSetterTarget"));
    h.hooks.put(context, 5, ptr::null_mut());
    let setter = save(b"at:put:", b"savedCompilerGlobalAt:put:");
    h.call(vt, b"addMethod", &[h.key(b"at:put:"), replacement]);
    h.eval(b"let retainedGlobalBlock := [| 73]");
    restore(b"at:put:", b"savedCompilerGlobalAt:put:", setter);
    h.clear();
    h.collect();
    assert!(h.units() > 0);
    assert_eq!(h.integer(h.eval(b"[retainedGlobalBlock value]")), 73);
    h.publish(b"retainedGlobalBlock", nil);
    h.call(store, b"compact", &[]);
    drop(q);
    h.reclaimed();
}
