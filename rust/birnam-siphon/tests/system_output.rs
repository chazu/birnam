//! Real stdout bytes and live printing dependencies over production Bootstrap.
mod support;
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
};
use core::{cell::Cell, ffi::c_void, ptr};
use std::{io::Write, os::unix::ffi::OsStrExt, process::Command};
use support::*;

unsafe fn text(id: Protocol, o: Oop) -> Vec<u8> {
    let b = unsafe { send(id, o, b"bytes", &[]) }.as_bytes().unwrap();
    unsafe { borrowed_bytes(b.pointer, b.length) }
        .unwrap()
        .to_vec()
}
unsafe fn save(id: Protocol, vt: Oop, name: &[u8], alias: &[u8]) -> NativeEntry {
    unsafe {
        let entry = send(
            id,
            id.receiver,
            b"bindingOf:in:",
            &[
                Operand::object(id.intern_name(name).unwrap()),
                Operand::object(vt),
            ],
        )
        .as_entry()
        .unwrap();
        send(
            id,
            vt,
            b"addMethod",
            &[
                Operand::object(id.intern_name(alias).unwrap()),
                Operand::entry(entry),
            ],
        );
        entry
    }
}
unsafe fn restore(id: Protocol, vt: Oop, name: &[u8], alias: &[u8], original: NativeEntry) {
    unsafe {
        send(
            id,
            vt,
            b"addMethod",
            &[
                Operand::object(id.intern_name(name).unwrap()),
                Operand::entry(original),
            ],
        );
        send(
            id,
            vt,
            b"removeMethod:",
            &[Operand::object(id.intern_name(alias).unwrap())],
        );
    }
}
struct Forward {
    id: Protocol,
    gc: Oop,
    alias: Oop,
    calls: Cell<usize>,
    failure: Cell<bool>,
}
unsafe extern "C" fn collecting(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Forward>();
        p.calls.set(p.calls.get() + 1);
        collect(p.id, p.gc);
        if p.failure.get() {
            return Outcome::failed(native::INVALID_REQUEST, 941);
        }
        p.id.send(r, p.alias, borrowed_slice(a, n).unwrap())
            .map_or_else(|e| e, Outcome::loaded)
    }
}
unsafe extern "C" fn answer(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::object(c))
}
unsafe extern "C" fn foreign_bytes(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::bytes(b"not a String"))
}
struct Reentrant {
    id: Protocol,
    gc: Oop,
    system: Oop,
    string: Oop,
    prefix: Oop,
    output_receiver: Oop,
}
unsafe extern "C" fn render(c: *mut c_void, r: Oop, _: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Reentrant>();
        assert_eq!(n, 0);
        collect(p.id, p.gc);
        assert!(present(p.id, r), "output retains its printable input");
        assert!(
            present(p.id, p.output_receiver),
            "output retains its receiver"
        );
        send(p.id, p.system, b"print:", &[Operand::object(p.prefix)]);
        // Deliberately no fixture root: output must retain this new String before
        // the collecting bytes method runs.
        Outcome::loaded(send(
            p.id,
            p.string,
            b"fromBytes:",
            &[Operand::bytes(b"outer")],
        ))
    }
}

#[test]
#[ignore = "requires native prefix"]
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
        assert_eq!(&output.stdout[start..finish], b"caf\xc3\xa9 output\nanObject\n#a b#caf\xc3\xa9\n\xff\0\n\nfirst\nsecond\nraw\nnested:outer\nchanged print\nchanged println\n");
        return;
    }
    unsafe {
        let id = id();
        let ns = load(
            id,
            "bootstrap",
            &[Operand::bytes(directory().as_os_str().as_bytes())],
        );
        let named = |n: &[u8]| {
            object(send(
                id,
                ns,
                b"at:",
                &[Operand::object(id.intern_name(n).unwrap())],
            ))
        };
        let roots = named(b"Roots");
        let gc = named(b"GC");
        let system = named(b"System");
        let svt = named(b"StringVt");
        let string = named(b"String");
        let nil = named(b"nil");
        let mut tokens = Vec::new();
        let mut keep = |o| {
            tokens.push(retain(id, roots, o));
            o
        };
        let make = |b: &[u8]| object(send(id, string, b"fromBytes:", &[Operand::bytes(b)]));
        let eval = |b: &[u8]| {
            let source = make(b);
            let token = retain(id, roots, source);
            let result = object(send(
                id,
                named(b"Codegen"),
                b"eval:",
                &[Operand::object(source)],
            ));
            release(id, roots, token);
            result
        };
        assert_eq!(load(id, "prelude", &[Operand::object(ns)]), system);
        let system_vt = named(b"SystemVt");
        assert_eq!(
            object(send(
                id,
                id.receiver,
                b"behaviorOf:",
                &[Operand::object(system)]
            )),
            system_vt
        );
        assert_eq!(
            object(send(
                id,
                id.receiver,
                b"ownerOf:",
                &[Operand::object(system_vt)]
            )),
            system
        );
        // Default methods must not suppress the already installed value printers.
        for (value, expected) in [
            (nil, b"nil".as_slice()),
            (named(b"true"), b"true"),
            (named(b"false"), b"false"),
        ] {
            assert_eq!(
                text(id, object(send(id, value, b"asString", &[]))),
                expected
            );
        }
        let ordinary = keep(object(send(id, named(b"Object"), b"clone", &[])));
        let a = keep(object(send(id, ordinary, b"printString", &[])));
        let b = keep(object(send(id, ordinary, b"printString", &[])));
        assert_ne!(a, b);
        assert_eq!(text(id, a), b"anObject");
        let symbol = keep(id.intern_name("a b#café".as_bytes()).unwrap());
        assert_eq!(
            object(send(
                id,
                id.receiver,
                b"behaviorOf:",
                &[Operand::object(symbol)]
            )),
            named(b"SymbolVt")
        );
        assert_eq!(
            text(id, object(send(id, symbol, b"asString", &[]))),
            "#a b#café".as_bytes()
        );
        for (receiver, selector) in [
            (ordinary, b"printString".as_slice()),
            (ordinary, b"asString"),
            (symbol, b"printString"),
        ] {
            assert_eq!(
                object(send(id, receiver, selector, &[Operand::object(nil)])),
                nil
            );
        }
        assert_eq!(object(send(id, system, b"print:", &[])), nil);
        assert_eq!(
            object(send(
                id,
                system,
                b"println:",
                &[Operand::object(nil), Operand::object(nil)]
            )),
            nil
        );
        assert!(
            id.send_named(system, b"print:", &[Operand::word(1)])
                .is_err()
        );
        // Existing methods and owner dependencies survive replacement and reload.
        let factory = save(id, svt, b"fromBytes:", b"savedOutputFromBytes:");
        let mut forwarding = Forward {
            id,
            gc,
            alias: keep(id.intern_name(b"savedOutputFromBytes:").unwrap()),
            calls: Cell::new(0),
            failure: Cell::new(false),
        };
        install(
            id,
            svt,
            b"fromBytes:",
            collecting,
            ptr::from_mut(&mut forwarding).cast(),
        );
        let transient = id.intern_name(b"unrooted printing Symbol").unwrap();
        let rendered = keep(object(send(id, transient, b"asString", &[])));
        assert_eq!(text(id, rendered), b"#unrooted printing Symbol");
        assert_eq!(forwarding.calls.get(), 1);
        forwarding.failure.set(true);
        let failure = id.send_named(ordinary, b"printString", &[]).unwrap_err();
        assert_eq!(
            (failure.status, failure.detail),
            (native::INVALID_REQUEST, 941)
        );
        restore(id, svt, b"fromBytes:", b"savedOutputFromBytes:", factory);

        std::io::stdout()
            .write_all(b"BEGIN-BIRNAM-OUTPUT\n")
            .unwrap();
        assert_eq!(
            eval("[System print: \"café\"]; [System println: \" output\"]".as_bytes()),
            system
        );
        assert_eq!(
            object(send(id, system, b"println:", &[Operand::object(ordinary)])),
            system
        );
        assert_eq!(
            object(send(id, system, b"println:", &[Operand::object(symbol)])),
            system
        );
        let alias = keep(object(send(id, system, b"clone", &[])));
        // Give this fixture its own original output bindings so it can report
        // results after the source test replaces System's inherited methods.
        let alias_vt = keep(object(send(
            id,
            id.receiver,
            b"ensurePrivateVtableFor:",
            &[Operand::object(alias)],
        )));
        for name in [b"print:".as_slice(), b"println:"] {
            let selector = id.intern_name(name).unwrap();
            let entry = send(id, system_vt, b"lookup", &[Operand::object(selector)])
                .as_entry()
                .unwrap();
            send(
                id,
                alias_vt,
                b"addMethod",
                &[Operand::object(selector), Operand::entry(entry)],
            );
        }
        let binary = keep(make(&[0xff, 0, b'\n']));
        assert_eq!(
            object(send(id, alias, b"print:", &[Operand::object(binary)])),
            alias
        );
        let empty = keep(make(b""));
        assert_eq!(
            object(send(id, alias, b"println:", &[Operand::object(empty)])),
            alias
        );
        let ordinary_vt = keep(object(send(
            id,
            id.receiver,
            b"ensurePrivateVtableFor:",
            &[Operand::object(ordinary)],
        )));
        for bytes in [b"first".as_slice(), b"second"] {
            let result = keep(make(bytes));
            install(id, ordinary_vt, b"printString", answer, result);
            let path = directory().join("libbirnam_printing.dylib");
            let path = keep(make(path.as_os_str().as_bytes()));
            let module = keep(eval(b"Module"));
            let loaded = keep(object(send(id, module, b"load:path:", &[Operand::object(id.intern_name(b"printing").unwrap()), Operand::object(path)])));
            assert_ne!(loaded, nil);
            assert_eq!(object(send(id, loaded, b"isError", &[])), named(b"false"));
            assert_eq!(
                text(id, object(send(id, ordinary, b"asString", &[]))),
                bytes
            );
            send(id, system, b"println:", &[Operand::object(ordinary)]);
        }
        // Public module reload must not reset a replacement on the shared root.
        let root_vt = named(b"ObjectVt");
        let old = save(id, root_vt, b"printString", b"savedRootPrinter");
        let replacement = keep(make(b"root printer kept"));
        install(id, root_vt, b"printString", answer, replacement);
        let module = keep(eval(b"Module"));
        let path = keep(make(directory().join("libbirnam_printing.dylib").as_os_str().as_bytes()));
        let loaded = keep(object(send(id, module, b"load:path:", &[Operand::object(id.intern_name(b"printing").unwrap()), Operand::object(path)])));
        assert_eq!(object(send(id, loaded, b"isError", &[])), named(b"false"));
        assert_eq!(object(send(id, named(b"Object"), b"printString", &[])), replacement);
        restore(id, root_vt, b"printString", b"savedRootPrinter", old);
        // Non-String renderings are refused even if they implement bytes.
        install(id, ordinary_vt, b"printString", answer, ordinary);
        install(id, ordinary_vt, b"bytes", foreign_bytes, ptr::null_mut());
        assert_eq!(
            object(send(id, system, b"println:", &[Operand::object(ordinary)])),
            nil
        );
        let original_print = save(id, svt, b"printString", b"savedOutputPrintString");
        install(id, svt, b"printString", answer, ordinary);
        let raw = keep(make(b"raw"));
        assert_eq!(
            object(send(id, system, b"println:", &[Operand::object(raw)])),
            system
        );
        restore(
            id,
            svt,
            b"printString",
            b"savedOutputPrintString",
            original_print,
        );

        let original_bytes = save(id, svt, b"bytes", b"savedOutputBytes");
        forwarding.alias = keep(id.intern_name(b"savedOutputBytes").unwrap());
        forwarding.calls.set(0);
        forwarding.failure.set(false);
        install(
            id,
            svt,
            b"bytes",
            collecting,
            ptr::from_mut(&mut forwarding).cast(),
        );
        let prefix = keep(make(b"nested:"));
        let transient_receiver = object(send(id, system, b"clone", &[]));
        let transient_value = object(send(id, named(b"Object"), b"clone", &[]));
        let transient_vt = object(send(
            id,
            id.receiver,
            b"ensurePrivateVtableFor:",
            &[Operand::object(transient_value)],
        ));
        let mut reentrant = Reentrant {
            id,
            gc,
            system,
            string,
            prefix,
            output_receiver: transient_receiver,
        };
        install(
            id,
            transient_vt,
            b"printString",
            render,
            ptr::from_mut(&mut reentrant).cast(),
        );
        assert_eq!(
            object(send(
                id,
                transient_receiver,
                b"println:",
                &[Operand::object(transient_value)]
            )),
            transient_receiver
        );
        assert_eq!(forwarding.calls.get(), 2);
        forwarding.failure.set(true);
        let failure = id
            .send_named(system, b"print:", &[Operand::object(raw)])
            .unwrap_err();
        assert_eq!(
            (failure.status, failure.detail),
            (native::INVALID_REQUEST, 941)
        );
        restore(id, svt, b"bytes", b"savedOutputBytes", original_bytes);
        // Source-installed overrides retain their normal behavior after module reload.
        eval(b"[Reflect defineMethod: #print: on: System as: [:self :value | \"changed print\"]]");
        eval(b"[Reflect defineMethod: #println: on: System as: [:self :value | \"changed println\"]]");
        let path = directory().join("libbirnam_system_output.dylib");
        assert_eq!(
            send(
                id,
                id.receiver,
                b"loadNative:",
                &[
                    Operand::bytes(path.as_os_str().as_bytes()),
                    Operand::object(ns)
                ]
            )
            .kind(),
            native::UNIT
        );
        // The fixture's explicitly installed output methods remain available.
        for name in [b"print:".as_slice(), b"println:"] {
            let result = keep(object(send(id, system, name, &[Operand::object(raw)])));
            send(id, alias, b"println:", &[Operand::object(result)]);
        }
        // Reload returns the constructed System even if the Namespace name changes.
        send(
            id,
            ns,
            b"at:put:",
            &[
                Operand::object(id.intern_name(b"System").unwrap()),
                Operand::object(nil),
            ],
        );
        collect(id, gc);
        assert_eq!(load(id, "prelude", &[Operand::object(ns)]), system);
        send(
            id,
            ns,
            b"at:put:",
            &[
                Operand::object(id.intern_name(b"System").unwrap()),
                Operand::object(system),
            ],
        );
        for token in tokens {
            release(id, roots, token);
        }
        collect(id, gc);
        std::io::stdout().write_all(b"END-BIRNAM-OUTPUT\n").unwrap();
    }
}
