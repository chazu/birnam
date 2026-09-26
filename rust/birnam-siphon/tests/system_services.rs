//! Isolated real stdin, environment and file operations through production Id sends.
#[path = "system_services/file_io.rs"]
mod file_io;
mod support;
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
};
use core::{cell::Cell, ffi::c_void, ptr};
use std::{
    ffi::OsString,
    fs,
    os::unix::ffi::{OsStrExt, OsStringExt},
    process::{Command, Stdio},
};
use support::*;
struct H {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    tokens: Vec<usize>,
}
impl H {
    unsafe fn named(&self, n: &[u8]) -> Oop {
        unsafe {
            object(send(
                self.id,
                self.ns,
                b"at:",
                &[Operand::object(self.id.intern_name(n).unwrap())],
            ))
        }
    }
    unsafe fn keep(&mut self, o: Oop) -> Oop {
        self.tokens.push(unsafe { retain(self.id, self.roots, o) });
        o
    }
    unsafe fn string(&mut self, b: &[u8]) -> Oop {
        let o = unsafe {
            object(send(
                self.id,
                self.named(b"String"),
                b"fromBytes:",
                &[Operand::bytes(b)],
            ))
        };
        unsafe { self.keep(o) }
    }
    unsafe fn call(&mut self, receiver: Oop, name: &[u8], a: &[Oop]) -> Oop {
        let args: Vec<_> = a.iter().copied().map(Operand::object).collect();
        let o = unsafe { object(send(self.id, receiver, name, &args)) };
        unsafe { self.keep(o) }
    }
    unsafe fn system(&mut self, name: &[u8], a: &[Oop]) -> Oop {
        unsafe { self.call(self.named(b"System"), name, a) }
    }
    unsafe fn eval(&mut self, source: &[u8]) -> Oop {
        unsafe {
            let s = self.string(source);
            self.call(self.named(b"Codegen"), b"eval:", &[s])
        }
    }
    unsafe fn error(&mut self, value: Oop, message: &[u8]) {
        unsafe {
            assert_eq!(
                self.call(value, b"kind", &[]),
                self.id.intern_name(b"ioError").unwrap()
            );
            let m = self.call(value, b"message", &[]);
            assert_eq!(text(self.id, m), message);
        }
    }
    unsafe fn bind(&self, n: &[u8], value: Operand) {
        unsafe {
            send(
                self.id,
                self.ns,
                b"at:put:",
                &[Operand::object(self.id.intern_name(n).unwrap()), value],
            );
        }
    }
}
unsafe fn text(id: Protocol, o: Oop) -> Vec<u8> {
    let b = unsafe { send(id, o, b"bytes", &[]) }.as_bytes().unwrap();
    unsafe { borrowed_bytes(b.pointer, b.length) }
        .unwrap()
        .to_vec()
}
unsafe fn save(h: &H, vt: Oop, name: &[u8], alias: &[u8]) -> NativeEntry {
    unsafe {
        let entry = send(
            h.id,
            h.id.receiver,
            b"bindingOf:in:",
            &[
                Operand::object(h.id.intern_name(name).unwrap()),
                Operand::object(vt),
            ],
        )
        .as_entry()
        .unwrap();
        send(
            h.id,
            vt,
            b"addMethod",
            &[
                Operand::object(h.id.intern_name(alias).unwrap()),
                Operand::entry(entry),
            ],
        );
        entry
    }
}
unsafe fn restore(h: &H, vt: Oop, name: &[u8], alias: &[u8], entry: NativeEntry) {
    unsafe {
        send(
            h.id,
            vt,
            b"addMethod",
            &[
                Operand::object(h.id.intern_name(name).unwrap()),
                Operand::entry(entry),
            ],
        );
        send(
            h.id,
            vt,
            b"removeMethod:",
            &[Operand::object(h.id.intern_name(alias).unwrap())],
        );
    }
}
struct Forward {
    id: Protocol,
    gc: Oop,
    alias: Oop,
    calls: Cell<usize>,
    failure: Cell<bool>,
    ns: Oop,
    publish_error: Cell<Oop>,
}
unsafe extern "C" fn forwarding(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Forward>();
        collect(p.id, p.gc);
        p.calls.set(p.calls.get() + 1);
        if p.failure.get() {
            return Outcome::failed(native::INVALID_REQUEST, 951);
        }
        if !p.publish_error.get().is_null() {
            send(
                p.id,
                p.ns,
                b"at:put:",
                &[
                    Operand::object(p.id.intern_name(b"Error").unwrap()),
                    Operand::object(p.publish_error.get()),
                ],
            );
        }
        p.id.send(r, p.alias, borrowed_slice(a, n).unwrap())
            .map_or_else(|e| e, Outcome::loaded)
    }
}
unsafe extern "C" fn answer(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::object(c))
}

#[test]
#[ignore = "requires native prefix"]
fn system_services_use_messages() {
    let Some(mode) = std::env::var_os("BIRNAM_SERVICES_CHILD") else {
        let root =
            std::env::temp_dir().join(format!("birnam-system-services-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let mut input = b"\nA\r\n\0\xff\n".to_vec();
        input.extend(vec![b'x'; 20_000]);
        input.extend_from_slice(b"\nlast");
        let input_path = root.join("stdin");
        fs::write(&input_path, input).unwrap();
        for mode in ["input", "bad-input"] {
            let stdin = fs::File::open(if mode == "input" { &input_path } else { &root }).unwrap();
            let output = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--ignored",
                    "--exact",
                    "system_services_use_messages",
                    "--nocapture",
                ])
                .env("BIRNAM_SERVICES_CHILD", mode)
                .env("BIRNAM_SERVICES_ROOT", &root)
                .env("BIRNAM_SERVICES_VALUE", "café")
                .env("BIRNAM_SERVICES_EMPTY", "")
                .env("BIRNAM_SERVICES_BINARY", OsString::from_vec(vec![0xff]))
                .env_remove("BIRNAM_SERVICES_MISSING")
                .stdin(Stdio::from(stdin))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{mode}: stdout={}\nstderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        fs::remove_dir_all(root).unwrap();
        return;
    };
    unsafe {
        let id = id();
        let ns = load(
            id,
            "bootstrap",
            &[Operand::bytes(directory().as_os_str().as_bytes())],
        );
        let roots = object(send(
            id,
            ns,
            b"at:",
            &[Operand::object(id.intern_name(b"Roots").unwrap())],
        ));
        let mut h = H {
            id,
            ns,
            roots,
            tokens: vec![],
        };
        environment(&mut h);
        input(&mut h, mode == "bad-input");
        if mode == "input" {
            file_io::check(&mut h);
        }
        for token in h.tokens {
            release(id, roots, token);
        }
    }
}
unsafe fn environment(h: &mut H) {
    unsafe {
        let id = h.id;
        let nil = h.named(b"nil");
        assert_eq!(h.system(b"getEnv:", &[]), nil);
        assert_eq!(h.system(b"getEnv:", &[nil]), nil);
        for (name, expected) in [
            (b"BIRNAM_SERVICES_VALUE".as_slice(), "café".as_bytes()),
            (b"BIRNAM_SERVICES_EMPTY", b""),
        ] {
            let name = h.string(name);
            let value = h.system(b"getEnv:", &[name]);
            assert_eq!(text(id, value), expected);
        }
        let source = h.eval(b"[System getEnv: \"BIRNAM_SERVICES_VALUE\"]");
        assert_eq!(text(id, source), "café".as_bytes());
        for bytes in [b"BIRNAM_SERVICES_MISSING".as_slice(), b"", b"bad\0name"] {
            let name = h.string(bytes);
            assert_eq!(h.system(b"getEnv:", &[name]), nil);
        }
        let invalid = h.string(&[0xff]);
        let binary = h.string(b"BIRNAM_SERVICES_BINARY");
        for name in [invalid, binary] {
            let error = h.system(b"getEnv:", &[name]);
            h.error(error, b"could not read environment");
        }
        let name = h.string(b"BIRNAM_SERVICES_VALUE");
        let inherited = h.string(b"BIRNAM_SERVICES_VALUE");
        send(
            id,
            id.receiver,
            b"ensurePrivateVtableFor:",
            &[Operand::object(inherited)],
        );
        assert_eq!(
            h.system(b"getEnv:", &[inherited]),
            nil,
            "exact String recognition"
        );
        let svt = h.named(b"StringVt");
        let original = save(h, svt, b"bytes", b"servicesBytes");
        let mut p = Forward {
            id,
            gc: h.named(b"GC"),
            ns: h.ns,
            alias: h.keep(id.intern_name(b"servicesBytes").unwrap()),
            calls: Cell::new(0),
            failure: Cell::new(false),
            publish_error: Cell::new(ptr::null_mut()),
        };
        install(id, svt, b"bytes", forwarding, ptr::from_mut(&mut p).cast());
        let value = h.system(b"getEnv:", &[name]);
        assert_eq!(p.calls.get(), 1);
        assert_eq!(text(id, value), "café".as_bytes());
        p.failure.set(true);
        let e = id
            .send_named(h.named(b"System"), b"getEnv:", &[Operand::object(name)])
            .unwrap_err();
        assert_eq!((e.status, e.detail), (native::INVALID_REQUEST, 951));
        restore(h, svt, b"bytes", b"servicesBytes", original);
        // Preserve late Error lookup, including factory publication by message construction.
        let error = h.named(b"Error");
        let original = save(h, svt, b"fromBytes:", b"servicesFromBytes:");
        p.alias = h.keep(id.intern_name(b"servicesFromBytes:").unwrap());
        p.calls.set(0);
        p.failure.set(false);
        install(
            id,
            svt,
            b"fromBytes:",
            forwarding,
            ptr::from_mut(&mut p).cast(),
        );
        h.bind(b"Error", Operand::unit());
        assert_eq!(h.system(b"getEnv:", &[invalid]), nil);
        assert_eq!(
            p.calls.get(),
            1,
            "message still constructed without Error factory"
        );
        p.publish_error.set(error);
        let e = h.system(b"getEnv:", &[invalid]);
        h.error(e, b"could not read environment");
        p.publish_error.set(ptr::null_mut());
        let value = h.system(b"getEnv:", &[name]);
        assert_eq!(text(id, value), "café".as_bytes());
        restore(h, svt, b"fromBytes:", b"servicesFromBytes:", original);
        let evt = h.named(b"ErrorFactoryVt");
        let original = save(h, evt, b"kind:message:", b"servicesError:message:");
        install(id, evt, b"kind:message:", answer, nil);
        assert_eq!(h.system(b"getEnv:", &[binary]), nil);
        restore(
            h,
            evt,
            b"kind:message:",
            b"servicesError:message:",
            original,
        );
        let vt = h.named(b"SystemVt");
        let original = save(h, vt, b"getEnv:", b"servicesGetEnv:");
        install(id, vt, b"getEnv:", answer, name);
        let path = directory().join("libbirnam_environment.dylib");
        assert_eq!(
            send(
                id,
                id.receiver,
                b"loadNative:",
                &[
                    Operand::bytes(path.as_os_str().as_bytes()),
                    Operand::object(h.ns)
                ]
            )
            .kind(),
            native::UNIT
        );
        assert_eq!(h.system(b"getEnv:", &[binary]), name);
        restore(h, vt, b"getEnv:", b"servicesGetEnv:", original);
    }
}
unsafe fn input(h: &mut H, bad: bool) {
    unsafe {
        let id = h.id;
        let nil = h.named(b"nil");
        assert_eq!(h.system(b"readLine", &[nil]), nil);
        let svt = h.named(b"StringVt");
        let original = save(h, svt, b"fromBytes:", b"servicesInputString:");
        let mut p = Forward {
            id,
            gc: h.named(b"GC"),
            ns: h.ns,
            alias: h.keep(id.intern_name(b"servicesInputString:").unwrap()),
            calls: Cell::new(0),
            failure: Cell::new(false),
            publish_error: Cell::new(ptr::null_mut()),
        };
        install(
            id,
            svt,
            b"fromBytes:",
            forwarding,
            ptr::from_mut(&mut p).cast(),
        );
        if bad {
            let e = h.system(b"readLine", &[]);
            h.error(e, b"stdin read failed");
            let error = h.named(b"Error");
            h.bind(b"Error", Operand::unit());
            assert_eq!(h.system(b"readLine", &[]), nil);
            h.bind(b"Error", Operand::object(error));
        } else {
            for expected in [
                vec![],
                b"A\r".to_vec(),
                vec![0, 0xff],
                vec![b'x'; 20_000],
                b"last".to_vec(),
            ] {
                let line = h.system(b"readLine", &[]);
                assert_eq!(text(id, line), expected);
            }
            assert_eq!(h.system(b"readLine", &[]), nil);
            assert_eq!(p.calls.get(), 5);
        }
        restore(h, svt, b"fromBytes:", b"servicesInputString:", original);
        let vt = h.named(b"SystemVt");
        let original = save(h, vt, b"readLine", b"servicesReadLine");
        install(id, vt, b"readLine", answer, h.named(b"true"));
        let path = directory().join("libbirnam_system_input.dylib");
        assert_eq!(
            send(
                id,
                id.receiver,
                b"loadNative:",
                &[
                    Operand::bytes(path.as_os_str().as_bytes()),
                    Operand::object(h.ns)
                ]
            )
            .kind(),
            native::UNIT
        );
        assert_eq!(h.system(b"readLine", &[]), h.named(b"true"));
        restore(h, vt, b"readLine", b"servicesReadLine", original);
    }
}
