//! Native diagnostic callbacks and explicit test roots, all through Id messages.
#[path = "oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use birnam_host_support::{Context, Scope};
use std::path::Path;
pub fn bootstrap() -> Context {
    oop::bootstrap()
}
pub struct H<'a> {
    pub host: &'a Context,
    pub q: Scope<'a>,
    pub context: Oop,
    pub fixture: Oop,
    pub probe: Oop,
    pub gc: Oop,
    pub storage: Oop,
    pub trace: Oop,
}
impl<'a> H<'a> {
    pub fn new(host: &'a Context) -> Self {
        let q = host.scope();
        let gc = host.named(b"GC").unwrap();
        q.call(gc, b"precisePayloads:", &[size(1).unwrap()])
            .unwrap();
        let path =
            std::env::temp_dir().join(format!("birnam-storage-trace-{}.dylib", std::process::id()));
        assert!(
            std::process::Command::new("clang")
                .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
                .arg(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../tests/fixtures/oop-host/storage-trace.c")
                )
                .arg("-o")
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let fixture = host.load_native(&q, &path, &[]).unwrap();
        std::fs::remove_file(path).unwrap();
        let storage = host.named(b"Storage").unwrap();
        let trace = host.named(b"Trace").unwrap();
        let failure = unsafe {
            host.id.send(
                host.named(b"Execution").unwrap(),
                host.intern(b"failNative:detail:").unwrap(),
                &[size(1).unwrap(), size(991).unwrap()],
            )
        }
        .unwrap_err();
        q.keep(failure).unwrap();
        let context = q
            .call(
                host.id.root,
                b"sequence",
                &[
                    host.roots,
                    gc,
                    storage,
                    trace,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    host.intern(b"originalObserve:").unwrap(),
                    failure,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                ],
            )
            .unwrap();
        let probe = q
            .call(host.named(b"Object").unwrap(), b"clone", &[])
            .unwrap();
        let h = Self {
            host,
            q,
            context,
            fixture,
            probe,
            gc,
            storage,
            trace,
        };
        let vt = h.private(probe);
        for name in [
            b"configure".as_slice(),
            b"flags",
            b"stats",
            b"row:field:",
            b"runReport:",
            b"collectNow",
            b"present:",
        ] {
            h.bind(vt, name, h.method(name));
        }
        h.bind(vt, b"allocation:bytes:identity:", h.method(b"census"));
        let reporter = h.object();
        let vt = h.private(reporter);
        for name in [
            b"reached:identity:".as_slice(),
            b"notReached:identity:",
            b"complete:",
        ] {
            h.bind(vt, name, h.method(name));
        }
        h.put(4, reporter);
        h
    }
    pub fn named(&self, n: &[u8]) -> Oop {
        self.host.named(n).unwrap()
    }
    pub fn key(&self, n: &[u8]) -> Oop {
        self.q.keep(self.host.intern(n).unwrap()).unwrap()
    }
    pub fn call(&self, r: Oop, n: &[u8], a: &[Oop]) -> Oop {
        self.q.call(r, n, a).unwrap()
    }
    pub fn object(&self) -> Oop {
        self.call(self.named(b"Object"), b"clone", &[])
    }
    pub fn private(&self, o: Oop) -> Oop {
        self.call(self.host.id.root, b"ensurePrivateVtableFor:", &[o])
    }
    pub fn vt(&self, o: Oop) -> Oop {
        self.call(self.host.id.root, b"behaviorOf:", &[o])
    }
    pub fn method(&self, n: &[u8]) -> Oop {
        self.call(
            self.call(self.fixture, b"methodAt:", &[self.key(n)]),
            b"withContext:",
            &[self.context],
        )
    }
    pub fn bind(&self, vt: Oop, n: &[u8], method: Oop) {
        self.call(vt, b"addMethod", &[self.key(n), method]);
    }
    pub fn put(&self, i: usize, v: Oop) {
        self.call(self.context, b"at:put:", &[size(i).unwrap(), v]);
    }
    pub fn field(&self, i: usize) -> Oop {
        self.call(self.context, b"at:", &[size(i).unwrap()])
    }
    pub fn flags(&self, collect: bool, bad: bool, fail: bool, sweep: bool, full: bool) {
        self.call(
            self.probe,
            b"flags",
            &[collect, bad, fail, sweep, full].map(|v| size(usize::from(v)).unwrap()),
        );
    }
    pub fn stat(&self, i: usize) -> usize {
        as_size(self.call(self.probe, b"stats", &[size(i).unwrap()])).unwrap()
    }
    pub fn row(&self, i: usize) -> (usize, bool) {
        let read = |field| {
            as_size(self.call(
                self.probe,
                b"row:field:",
                &[size(i).unwrap(), size(field).unwrap()],
            ))
            .unwrap()
        };
        (read(0), read(1) != 0)
    }
    pub fn report(&self, mask: usize) -> Result<Oop, Oop> {
        let mask = self.q.word(mask).unwrap();
        unsafe {
            self.host
                .id
                .send(self.probe, self.key(b"runReport:"), &[mask])
        }
    }
    pub fn detail(&self, error: Oop) -> usize {
        self.q.keep(error).unwrap();
        let detail = self.call(error, b"detail", &[]);
        as_size(detail)
            .unwrap_or_else(|| as_size(self.call(detail, b"at:", &[size(1).unwrap()])).unwrap())
    }
    pub fn collect(&self) {
        self.call(self.probe, b"collectNow", &[]);
    }
    pub fn identity(&self, value: Oop) -> usize {
        as_size(self.call(self.host.id.root, b"identityOf:", &[value])).unwrap()
    }
    pub fn present(&self, identity: usize) -> bool {
        as_size(self.call(self.probe, b"present:", &[size(identity).unwrap()])).unwrap() == 1
    }
    pub fn project_storage(&self) -> Oop {
        let proxy = self.object();
        let vt = self.private(proxy);
        for name in [
            b"allocation:bytes:identity:".as_slice(),
            b"root:",
            b"edgeFrom:to:",
        ] {
            self.bind(vt, name, self.method(name));
        }
        self.bind(vt, b"complete:", self.method(b"proxyComplete"));
        self.put(5, proxy);
        let vt = self.vt(self.storage);
        let original = self.call(vt, b"lookup", &[self.key(b"observe:")]);
        self.bind(vt, b"originalObserve:", original);
        self.bind(vt, b"observe:", self.method(b"observed"));
        original
    }
}
