//! Test-only declared callbacks. Captured Methods and state remain Oops.
use birnam_abi::{self as abi, Oop};
use birnam_host_support::{Context, Scope};
use std::path::Path;
pub struct Hooks<'a> {
    host: &'a Context,
    scope: Scope<'a>,
    module: Oop,
}
impl<'a> Hooks<'a> {
    pub fn new(host: &'a Context) -> Self {
        let scope = host.scope();
        let path =
            std::env::temp_dir().join(format!("birnam-host-hooks-{}.dylib", std::process::id()));
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/oop-host/host-hooks.c");
        assert!(
            std::process::Command::new("clang")
                .args(["-dynamiclib", "-O2"])
                .arg(source)
                .arg("-o")
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let module = host.load_native(&scope, &path, &[]).unwrap();
        std::fs::remove_file(path).unwrap();
        Self {
            host,
            scope,
            module,
        }
    }
    pub fn symbol(&self, name: &[u8]) -> Oop {
        self.scope.keep(self.host.intern(name).unwrap()).unwrap()
    }
    pub fn context(&self, values: &[Oop]) -> Oop {
        self.scope
            .call(self.host.id.root, b"sequence", values)
            .unwrap()
    }
    pub fn method(&self, name: &[u8], context: Oop) -> Oop {
        let method = self
            .scope
            .call(self.module, b"methodAt:", &[self.symbol(name)])
            .unwrap();
        self.scope
            .call(method, b"withContext:", &[context])
            .unwrap()
    }
    pub fn field(&self, context: Oop, index: usize) -> Oop {
        self.scope
            .call(context, b"at:", &[abi::size(index).unwrap()])
            .unwrap()
    }
    pub fn count(&self, context: Oop, index: usize) -> usize {
        abi::as_size(self.field(context, index)).unwrap()
    }
    pub fn put(&self, context: Oop, index: usize, value: Oop) {
        self.scope
            .call(context, b"at:put:", &[abi::size(index).unwrap(), value])
            .unwrap();
    }
    pub fn capture(&self, behavior: Oop, selector: Oop) -> Oop {
        self.scope
            .call(
                self.host.named(b"Execution").unwrap(),
                b"directMethodFor:in:",
                &[selector, behavior],
            )
            .unwrap()
    }
    pub fn fault(&self, detail: usize) -> Oop {
        let kind = self.symbol(b"nativeFailure");
        let details = self.context(&[abi::size(6).unwrap(), abi::size(detail).unwrap()]);
        let result = unsafe {
            self.host.id.send(
                self.host.id.root,
                self.symbol(b"fault:detail:"),
                &[kind, details],
            )
        }
        .unwrap_err();
        self.scope.keep(result).unwrap()
    }
}
