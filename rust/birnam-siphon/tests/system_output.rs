//! Real stdout bytes and live printing dependencies over production Bootstrap.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, size};
use std::{io::Write, path::Path, process::Command, ptr};
#[test]
#[ignore = "requires production Oop modules"]
fn system_construction_printing_and_stdout_use_messages() {
    if std::env::var_os("BIRNAM_OUTPUT_CHILD").is_none() {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "system_construction_printing_and_stdout_use_messages",
                "--nocapture",
            ])
            .env("BIRNAM_OUTPUT_CHILD", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "child stdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let begin = b"BEGIN-BIRNAM-OUTPUT\n";
        let end = b"END-BIRNAM-OUTPUT\n";
        let start = output
            .stdout
            .windows(begin.len())
            .position(|x| x == begin)
            .unwrap()
            + begin.len();
        let finish = output
            .stdout
            .windows(end.len())
            .position(|x| x == end)
            .unwrap();
        assert_eq!(&output.stdout[start..finish], b"0x1\ncaf\xc3\xa9 output\nanObject\n#a b#caf\xc3\xa9\n\xff\0\n\nfirst\nsecond\nraw\nnested:outer\nchanged print\nchanged println\n");
        return;
    }
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let named = |name: &[u8]| h.named(name).unwrap();
    let key = |name: &[u8]| hooks.symbol(name);
    let call = |r, name: &[u8], args: &[Oop]| q.call(r, name, args).unwrap();
    let text = |v| q.read_bytes(call(v, b"bytes", &[])).unwrap();
    let make = |bytes: &[u8]| q.string(bytes).unwrap();
    let eval = |source: &[u8]| call(named(b"Codegen"), b"eval:", &[make(source)]);
    let gc = named(b"GC");
    let system = named(b"System");
    let string = named(b"String");
    let svt = named(b"StringVt");
    let nil = named(b"nil");
    let load = |name: &str| {
        h.load_native(
            &q,
            &h.resources.lib_dir.join(format!("libbirnam_{name}.dylib")),
            &[h.namespace],
        )
        .unwrap()
    };
    assert_eq!(load("prelude"), system);
    let system_vt = named(b"SystemVt");
    assert_eq!(call(h.id.root, b"behaviorOf:", &[system]), system_vt);
    assert_eq!(call(h.id.root, b"ownerOf:", &[system_vt]), system);
    for (v, expected) in [
        (nil, b"nil".as_slice()),
        (named(b"true"), b"true"),
        (named(b"false"), b"false"),
    ] {
        assert_eq!(text(call(v, b"asString", &[])), expected);
    }
    let ordinary = call(named(b"Object"), b"clone", &[]);
    let a = call(ordinary, b"printString", &[]);
    let b = call(ordinary, b"printString", &[]);
    assert_ne!(a, b);
    assert_eq!(text(a), b"anObject");
    let symbol = key("a b#café".as_bytes());
    assert_eq!(
        call(h.id.root, b"behaviorOf:", &[symbol]),
        named(b"SymbolVt")
    );
    assert_eq!(text(call(symbol, b"asString", &[])), "#a b#café".as_bytes());
    for (r, s) in [
        (ordinary, b"printString".as_slice()),
        (ordinary, b"asString"),
        (symbol, b"printString"),
    ] {
        assert_eq!(call(r, s, &[nil]), nil);
    }
    assert_eq!(call(system, b"print:", &[]), nil);
    assert_eq!(call(system, b"println:", &[nil, nil]), nil);
    assert!(h.send(system, b"print:", &[ptr::null_mut()]).is_err());

    let path =
        std::env::temp_dir().join(format!("birnam-system-output-{}.dylib", std::process::id()));
    assert!(
        Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/system-output.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = h.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let method = |name: &[u8], context| {
        call(
            call(fixture, b"methodAt:", &[key(name)]),
            b"withContext:",
            &[context],
        )
    };
    let install = |vt, name: &[u8], m| {
        call(vt, b"addMethod", &[key(name), m]);
    };
    let save = |vt, name: &[u8], alias: &[u8]| {
        let m = hooks.capture(vt, key(name));
        install(vt, alias, m);
        m
    };
    let restore = |vt, name: &[u8], alias: &[u8], m| {
        install(vt, name, m);
        call(vt, b"removeMethod:", &[key(alias)]);
    };
    let factory = save(svt, b"fromBytes:", b"savedOutputFromBytes:");
    let fault = hooks.fault(941);
    let forwarding = hooks.context(&[
        gc,
        key(b"savedOutputFromBytes:"),
        size(0).unwrap(),
        ptr::null_mut(),
    ]);
    install(svt, b"fromBytes:", method(b"forward", forwarding));
    let transient = h.intern(b"unrooted printing Symbol").unwrap();
    assert_eq!(
        text(call(transient, b"asString", &[])),
        b"#unrooted printing Symbol"
    );
    assert_eq!(hooks.count(forwarding, 2), 1);
    hooks.put(forwarding, 3, fault);
    assert_eq!(
        unsafe { h.id.send(ordinary, key(b"printString"), &[]) }.unwrap_err(),
        fault
    );
    restore(svt, b"fromBytes:", b"savedOutputFromBytes:", factory);

    std::io::stdout()
        .write_all(b"BEGIN-BIRNAM-OUTPUT\n")
        .unwrap();
    // Supported immediate Words participate in ordinary object printing.
    assert_eq!(call(system, b"println:", &[size(1).unwrap()]), system);
    assert_eq!(
        eval("[System print: \"café\"]; [System println: \" output\"]".as_bytes()),
        system
    );
    assert_eq!(call(system, b"println:", &[ordinary]), system);
    assert_eq!(call(system, b"println:", &[symbol]), system);
    let alias = call(system, b"clone", &[]);
    let alias_vt = call(h.id.root, b"ensurePrivateVtableFor:", &[alias]);
    // Preserve explicit output bindings while the source test replaces System's methods.
    for name in [b"print:".as_slice(), b"println:"] {
        install(alias_vt, name, call(system_vt, b"lookup", &[key(name)]));
    }
    let binary = make(&[0xff, 0, b'\n']);
    assert_eq!(call(alias, b"print:", &[binary]), alias);
    assert_eq!(call(alias, b"println:", &[make(b"")]), alias);
    let ordinary_vt = call(h.id.root, b"ensurePrivateVtableFor:", &[ordinary]);
    let module = h.global(&q, b"Module").unwrap();
    let printing_path = make(
        h.resources
            .lib_dir
            .join("libbirnam_printing.dylib")
            .as_os_str()
            .as_encoded_bytes(),
    );
    for bytes in [b"first".as_slice(), b"second"] {
        install(
            ordinary_vt,
            b"printString",
            hooks.method(b"fixed", make(bytes)),
        );
        let loaded = call(module, b"load:path:", &[key(b"printing"), printing_path]);
        assert_ne!(loaded, nil);
        assert_eq!(call(loaded, b"isError", &[]), named(b"false"));
        assert_eq!(text(call(ordinary, b"asString", &[])), bytes);
        call(system, b"println:", &[ordinary]);
    }
    let root_vt = named(b"ObjectVt");
    let old = save(root_vt, b"printString", b"savedRootPrinter");
    let replacement = make(b"root printer kept");
    install(root_vt, b"printString", hooks.method(b"fixed", replacement));
    let loaded = call(module, b"load:path:", &[key(b"printing"), printing_path]);
    assert_eq!(call(loaded, b"isError", &[]), named(b"false"));
    assert_eq!(call(named(b"Object"), b"printString", &[]), replacement);
    restore(root_vt, b"printString", b"savedRootPrinter", old);
    // A byte-providing object is not thereby a String rendering.
    install(
        ordinary_vt,
        b"printString",
        hooks.method(b"fixed", ordinary),
    );
    let foreign = call(
        named(b"Bytes"),
        b"fromBytes:",
        &[q.byte_values(b"not a String").unwrap()],
    );
    install(ordinary_vt, b"bytes", hooks.method(b"fixed", foreign));
    assert_eq!(call(system, b"println:", &[ordinary]), nil);
    let original_print = save(svt, b"printString", b"savedOutputPrintString");
    install(svt, b"printString", hooks.method(b"fixed", ordinary));
    let raw = make(b"raw");
    assert_eq!(call(system, b"println:", &[raw]), system);
    restore(
        svt,
        b"printString",
        b"savedOutputPrintString",
        original_print,
    );

    let original_bytes = save(svt, b"bytes", b"savedOutputBytes");
    hooks.put(forwarding, 1, key(b"savedOutputBytes"));
    hooks.put(forwarding, 2, size(0).unwrap());
    hooks.put(forwarding, 3, ptr::null_mut());
    install(svt, b"bytes", method(b"forward", forwarding));
    let prefix = make(b"nested:");
    let probe = call(named(b"Object"), b"clone", &[]);
    let probe_vt = call(h.id.root, b"ensurePrivateVtableFor:", &[probe]);
    for name in [b"watch".as_slice(), b"mask", b"allocation:bytes:identity:"] {
        install(probe_vt, name, method(name, probe));
    }
    let context = hooks.context(&[
        gc,
        system,
        string,
        prefix,
        q.byte_values(b"outer").unwrap(),
        probe,
    ]);
    let renderer = method(b"render", context);
    let println = key(b"println:");
    let (transient_receiver, transient_value) = {
        let setup = h.scope();
        let receiver = setup.call(system, b"clone", &[]).unwrap();
        let value = setup.call(named(b"Object"), b"clone", &[]).unwrap();
        let vt = setup
            .call(h.id.root, b"ensurePrivateVtableFor:", &[value])
            .unwrap();
        setup
            .call(vt, b"addMethod", &[key(b"printString"), renderer])
            .unwrap();
        h.send(probe, b"watch", &[receiver, value]).unwrap();
        (receiver, value)
    };
    assert_eq!(
        unsafe { h.id.send(transient_receiver, println, &[transient_value]) }.unwrap(),
        transient_receiver
    );
    assert_eq!(hooks.count(forwarding, 2), 2);
    hooks.put(forwarding, 3, fault);
    assert_eq!(
        unsafe { h.id.send(system, key(b"print:"), &[raw]) }.unwrap_err(),
        fault
    );
    restore(svt, b"bytes", b"savedOutputBytes", original_bytes);
    eval(b"[Reflect defineMethod: #print: on: System as: [:self :value | \"changed print\"]]");
    eval(b"[Reflect defineMethod: #println: on: System as: [:self :value | \"changed println\"]]");
    assert!(load("system_output").is_null());
    for name in [b"print:".as_slice(), b"println:"] {
        let result = call(system, name, &[raw]);
        call(alias, b"println:", &[result]);
    }
    call(h.namespace, b"at:put:", &[key(b"System"), nil]);
    call(gc, b"collectPrecisely", &[]);
    assert_eq!(load("prelude"), system);
    call(h.namespace, b"at:put:", &[key(b"System"), system]);
    drop(hooks);
    drop(q);
    h.send(gc, b"collectPrecisely", &[]).unwrap();
    std::io::stdout().write_all(b"END-BIRNAM-OUTPUT\n").unwrap();
}
