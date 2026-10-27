//! Isolated stdin, environment and file operations through production Oop messages.
#[path = "system_services/file_io.rs"]
mod file_io;
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, size};
use birnam_host_support::{Context, Scope};
use std::{
    ffi::OsString,
    fs,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::Path,
    process::{Command, Stdio},
    ptr,
};
struct H<'a> {
    host: &'a Context,
    q: Scope<'a>,
    hooks: oop::hooks::Hooks<'a>,
    fixture: Oop,
}
impl H<'_> {
    fn named(&self, n: &[u8]) -> Oop {
        self.host.named(n).unwrap()
    }
    fn key(&self, n: &[u8]) -> Oop {
        self.hooks.symbol(n)
    }
    fn string(&self, b: &[u8]) -> Oop {
        self.q.string(b).unwrap()
    }
    fn call(&self, r: Oop, n: &[u8], a: &[Oop]) -> Oop {
        self.q.call(r, n, a).unwrap()
    }
    fn system(&self, n: &[u8], a: &[Oop]) -> Oop {
        self.call(self.named(b"System"), n, a)
    }
    fn eval(&self, source: &[u8]) -> Oop {
        self.call(self.named(b"Codegen"), b"eval:", &[self.string(source)])
    }
    fn text(&self, v: Oop) -> Vec<u8> {
        self.q.read_bytes(self.call(v, b"bytes", &[])).unwrap()
    }
    fn error(&self, value: Oop, message: &[u8]) {
        assert_eq!(self.call(value, b"kind", &[]), self.key(b"ioError"));
        assert_eq!(self.text(self.call(value, b"message", &[])), message);
    }
    fn bind(&self, n: &[u8], v: Oop) {
        self.call(self.host.namespace, b"at:put:", &[self.key(n), v]);
    }
    fn method(&self, n: &[u8], c: Oop) -> Oop {
        self.call(
            self.call(self.fixture, b"methodAt:", &[self.key(n)]),
            b"withContext:",
            &[c],
        )
    }
    fn install(&self, vt: Oop, n: &[u8], m: Oop) {
        self.call(vt, b"addMethod", &[self.key(n), m]);
    }
    fn save(&self, vt: Oop, n: &[u8], alias: &[u8]) -> Oop {
        let m = self.hooks.capture(vt, self.key(n));
        self.install(vt, alias, m);
        m
    }
    fn restore(&self, vt: Oop, n: &[u8], alias: &[u8], m: Oop) {
        self.install(vt, n, m);
        self.call(vt, b"removeMethod:", &[self.key(alias)]);
    }
    fn forward(&self, alias: &[u8]) -> Oop {
        self.hooks.context(&[
            self.named(b"GC"),
            self.key(alias),
            size(0).unwrap(),
            ptr::null_mut(),
            self.host.namespace,
            ptr::null_mut(),
        ])
    }
    fn fail(&self, r: Oop, n: &[u8], a: &[Oop]) -> Oop {
        unsafe { self.host.id.send(r, self.key(n), a) }.unwrap_err()
    }
    fn load(&self, n: &str) -> Oop {
        self.host
            .load_native(
                &self.q,
                &self
                    .host
                    .resources
                    .lib_dir
                    .join(format!("libbirnam_{n}.dylib")),
                &[self.host.namespace],
            )
            .unwrap()
    }
}
#[test]
#[ignore = "requires production Oop modules"]
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
    let host = oop::bootstrap();
    let q = host.scope();
    let path = std::env::temp_dir().join(format!(
        "birnam-system-services-{}.dylib",
        std::process::id()
    ));
    assert!(
        Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/system-services.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = host.load_native(&q, &path, &[]).unwrap();
    fs::remove_file(path).unwrap();
    let h = H {
        host: &host,
        q,
        hooks: oop::hooks::Hooks::new(&host),
        fixture,
    };
    environment(&h);
    input(&h, mode == "bad-input");
    if mode == "input" {
        file_io::check(&h);
    }
}
fn environment(h: &H<'_>) {
    let nil = h.named(b"nil");
    assert_eq!(h.system(b"getEnv:", &[]), nil);
    assert_eq!(h.system(b"getEnv:", &[nil]), nil);
    for (name, expected) in [
        (b"BIRNAM_SERVICES_VALUE".as_slice(), "café".as_bytes()),
        (b"BIRNAM_SERVICES_EMPTY", b""),
    ] {
        let name = h.string(name);
        assert_eq!(h.text(h.system(b"getEnv:", &[name])), expected);
    }
    assert_eq!(
        h.text(h.eval(b"[System getEnv: \"BIRNAM_SERVICES_VALUE\"]")),
        "café".as_bytes()
    );
    for bytes in [b"BIRNAM_SERVICES_MISSING".as_slice(), b"", b"bad\0name"] {
        assert_eq!(h.system(b"getEnv:", &[h.string(bytes)]), nil);
    }
    let invalid = h.string(&[0xff]);
    let binary = h.string(b"BIRNAM_SERVICES_BINARY");
    for name in [invalid, binary] {
        h.error(h.system(b"getEnv:", &[name]), b"could not read environment");
    }
    let name = h.string(b"BIRNAM_SERVICES_VALUE");
    let inherited = h.string(b"BIRNAM_SERVICES_VALUE");
    h.call(h.host.id.root, b"ensurePrivateVtableFor:", &[inherited]);
    assert_eq!(
        h.system(b"getEnv:", &[inherited]),
        nil,
        "exact String recognition"
    );
    let svt = h.named(b"StringVt");
    let original = h.save(svt, b"bytes", b"servicesBytes");
    let p = h.forward(b"servicesBytes");
    h.install(svt, b"bytes", h.method(b"forward", p));
    let value = h.system(b"getEnv:", &[name]);
    assert_eq!(h.hooks.count(p, 2), 1);
    assert_eq!(h.text(value), "café".as_bytes());
    let failure = h.hooks.fault(951);
    h.hooks.put(p, 3, failure);
    assert_eq!(h.fail(h.named(b"System"), b"getEnv:", &[name]), failure);
    h.restore(svt, b"bytes", b"servicesBytes", original);
    // Resolve Error after message construction, including publication by that callback.
    let error = h.named(b"Error");
    let original = h.save(svt, b"fromBytes:", b"servicesFromBytes:");
    h.hooks.put(p, 1, h.key(b"servicesFromBytes:"));
    h.hooks.put(p, 2, size(0).unwrap());
    h.hooks.put(p, 3, ptr::null_mut());
    h.install(svt, b"fromBytes:", h.method(b"forward", p));
    h.bind(b"Error", ptr::null_mut());
    assert_eq!(h.system(b"getEnv:", &[invalid]), nil);
    assert_eq!(
        h.hooks.count(p, 2),
        1,
        "message still constructed without Error factory"
    );
    h.hooks.put(p, 5, error);
    h.error(
        h.system(b"getEnv:", &[invalid]),
        b"could not read environment",
    );
    h.hooks.put(p, 5, ptr::null_mut());
    assert_eq!(h.text(h.system(b"getEnv:", &[name])), "café".as_bytes());
    h.restore(svt, b"fromBytes:", b"servicesFromBytes:", original);
    let evt = h.named(b"ErrorFactoryVt");
    let original = h.save(evt, b"kind:message:", b"servicesError:message:");
    h.install(evt, b"kind:message:", h.hooks.method(b"fixed", nil));
    assert_eq!(h.system(b"getEnv:", &[binary]), nil);
    h.restore(evt, b"kind:message:", b"servicesError:message:", original);
    let vt = h.named(b"SystemVt");
    let original = h.save(vt, b"getEnv:", b"servicesGetEnv:");
    h.install(vt, b"getEnv:", h.hooks.method(b"fixed", name));
    assert!(h.load("environment").is_null());
    assert_eq!(h.system(b"getEnv:", &[binary]), name);
    h.restore(vt, b"getEnv:", b"servicesGetEnv:", original);
}
fn input(h: &H<'_>, bad: bool) {
    let nil = h.named(b"nil");
    assert_eq!(h.system(b"readLine", &[nil]), nil);
    let svt = h.named(b"StringVt");
    let original = h.save(svt, b"fromBytes:", b"servicesInputString:");
    let p = h.forward(b"servicesInputString:");
    h.install(svt, b"fromBytes:", h.method(b"forward", p));
    if bad {
        h.error(h.system(b"readLine", &[]), b"stdin read failed");
        let error = h.named(b"Error");
        h.bind(b"Error", ptr::null_mut());
        assert_eq!(h.system(b"readLine", &[]), nil);
        h.bind(b"Error", error);
    } else {
        for expected in [
            vec![],
            b"A\r".to_vec(),
            vec![0, 0xff],
            vec![b'x'; 20_000],
            b"last".to_vec(),
        ] {
            assert_eq!(h.text(h.system(b"readLine", &[])), expected);
        }
        assert_eq!(h.system(b"readLine", &[]), nil);
        assert_eq!(h.hooks.count(p, 2), 5);
    }
    h.restore(svt, b"fromBytes:", b"servicesInputString:", original);
    let vt = h.named(b"SystemVt");
    let original = h.save(vt, b"readLine", b"servicesReadLine");
    h.install(vt, b"readLine", h.hooks.method(b"fixed", h.named(b"true")));
    assert!(h.load("system_input").is_null());
    assert_eq!(h.system(b"readLine", &[]), h.named(b"true"));
    h.restore(vt, b"readLine", b"servicesReadLine", original);
}
