//! Actual authored Globals initialization on a physically Store-absent floor.
#[path = "support/minimal.rs"]
#[allow(dead_code)]
mod minimal;
use birnam_abi::{Oop, size};
use birnam_host_support::Context;
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
};
struct H<'a> {
    host: &'a Context,
    tokens: RefCell<Vec<Oop>>,
}
impl H<'_> {
    fn named(&self, n: &[u8]) -> Oop {
        self.host.named(n).unwrap()
    }
    fn keep(&self, o: Oop) -> Oop {
        self.tokens
            .borrow_mut()
            .push(self.host.send(self.host.roots, b"retain:", &[o]).unwrap());
        o
    }
    fn key(&self, n: &[u8]) -> Oop {
        self.keep(self.host.intern(n).unwrap())
    }
    fn call(&self, r: Oop, n: &[u8], a: &[Oop]) -> Oop {
        self.keep(self.host.send(r, n, a).unwrap())
    }
    fn integer(&self, n: usize) -> Oop {
        self.call(self.named(b"Integer"), b"fromWord:", &[size(n).unwrap()])
    }
    fn number(&self, o: Oop) -> usize {
        self.host
            .scope()
            .word_bits(self.call(self.named(b"Integer"), b"wordOf:", &[o]))
            .unwrap()
    }
    fn text(&self, b: &[u8]) -> Oop {
        self.keep(self.host.scope().string(b).unwrap())
    }
    fn clear_from(&self, i: usize) {
        for token in self.tokens.borrow_mut().drain(i..).rev() {
            self.host
                .send(self.host.roots, b"release:", &[token])
                .unwrap();
        }
    }
    fn load(&self, name: &str) -> Oop {
        let path = self
            .host
            .resources
            .lib_dir
            .join(format!("libbirnam_{name}.dylib"));
        let value = self.call(
            self.named(b"Codegen"),
            b"load:",
            &[self.text(path.as_os_str().as_encoded_bytes())],
        );
        assert_ne!(value, self.named(b"nil"));
        value
    }
}
impl Drop for H<'_> {
    fn drop(&mut self) {
        self.clear_from(0);
    }
}
#[test]
#[ignore = "scripts/globals-without-store-test.py supplies the isolated artifact floor"]
fn authored_bindings_work_without_store_or_relational() {
    let directory = PathBuf::from(std::env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").unwrap());
    for name in ["store", "relational", "bootstrap", "generation", "lang"] {
        assert!(!directory.join(format!("libbirnam_{name}.dylib")).exists());
    }
    let host = minimal::native_floor();
    let h = H {
        host: &host,
        tokens: RefCell::new(vec![]),
    };
    let q = host.scope();
    let ns = host.namespace;
    let gc = h.named(b"GC");
    let nil = h.named(b"nil");
    let prototype = h.named(b"Object");
    assert!(h.call(ns, b"at:", &[h.key(b"Globals")]).is_null());
    assert_eq!(
        h.call(ns, b"resolveGlobal:", &[h.key(b"Object")]),
        prototype
    );
    assert_eq!(
        h.call(ns, b"resolveGlobal:", &[h.key(b"missingBeforeGlobals")]),
        nil
    );
    h.load("error");
    let globals = h.load("globalsOwner");
    assert_eq!(globals, h.named(b"Globals"));
    q.check_result(globals).unwrap();
    for name in [
        b"Store".as_slice(),
        b"db",
        b"Relational",
        b"Lang",
        b"Generation",
        b"Bootstrap",
    ] {
        assert!(h.call(ns, b"at:", &[h.key(name)]).is_null());
    }
    // Weak identity observations do not keep removed binding keys/values alive.
    let path = std::env::temp_dir().join(format!(
        "birnam-globals-presence-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/allocation-presence.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let module = host.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let monitor = q.call(prototype, b"clone", &[]).unwrap();
    let vt = q
        .call(host.id.root, b"ensurePrivateVtableFor:", &[monitor])
        .unwrap();
    for n in [b"allocation:bytes:identity:".as_slice(), b"present:"] {
        let k = q.keep(host.intern(n).unwrap()).unwrap();
        let m = q.call(module, b"methodAt:", &[k]).unwrap();
        q.call(vt, b"addMethod", &[k, m]).unwrap();
    }
    let identity = |o| q.call(host.id.root, b"identityOf:", &[o]).unwrap();
    let present = |id| q.call(monitor, b"present:", &[id]).unwrap() == size(1).unwrap();
    let name = h.key(b"owned");
    let foreign = h.key(b"foreign");
    assert_eq!(h.call(globals, b"includes:", &[name]), h.named(b"false"));
    h.call(globals, b"at:put:", &[name, nil]);
    assert_eq!(h.call(globals, b"includes:", &[name]), h.named(b"true"));
    let before = h.call(globals, b"bindingAt:", &[name]);
    assert_eq!(h.number(h.call(before, b"size", &[])), 1);
    let value = h.call(prototype, b"clone", &[]);
    h.call(globals, b"at:put:", &[name, value]);
    h.call(gc, b"collectPrecisely", &[]);
    assert_eq!(h.call(globals, b"at:", &[name]), value);
    assert_eq!(
        h.call(before, b"at:", &[h.integer(0)]),
        nil,
        "snapshot changed"
    );
    h.call(globals, b"at:put:", &[foreign, value]);
    h.call(globals, b"remove:", &[name]);
    assert_eq!(h.call(globals, b"includes:", &[name]), h.named(b"false"));
    assert_eq!(h.call(globals, b"at:", &[foreign]), value);
    let a = h.text(b"same");
    let b = h.text(b"same");
    assert_ne!(a, b);
    h.call(globals, b"at:put:", &[a, name]);
    h.call(globals, b"at:put:", &[b, foreign]);
    assert_eq!(h.call(globals, b"at:", &[a]), name);
    assert_eq!(h.call(globals, b"at:", &[b]), foreign);
    h.call(globals, b"remove:", &[a]);
    assert_eq!(h.call(globals, b"includes:", &[a]), h.named(b"false"));
    assert_eq!(h.call(globals, b"at:", &[b]), foreign);
    let snapshot = h.call(globals, b"bindingState", &[]);
    h.call(globals, b"remove:", &[b]);
    let bindings = h.call(snapshot, b"at:", &[h.integer(0)]);
    let binding_key = h.call(b, b"bindingKey", &[]);
    assert_eq!(h.call(bindings, b"at:", &[binding_key]), foreign);
    assert_eq!(h.number(h.call(snapshot, b"size", &[])), 4);
    let mark = h.tokens.borrow().len();
    let transient_key = h.call(prototype, b"clone", &[]);
    let transient = h.call(prototype, b"clone", &[]);
    let key_id = identity(transient_key);
    let value_id = identity(transient);
    h.call(globals, b"at:put:", &[transient_key, transient]);
    h.clear_from(mark);
    h.call(gc, b"collectPrecisely", &[]);
    assert!(present(key_id) && present(value_id));
    h.call(globals, b"remove:", &[transient_key]);
    h.clear_from(mark);
    h.call(gc, b"collectPrecisely", &[]);
    assert!(!present(key_id) && !present(value_id));
    h.call(globals, b"at:put:", &[a, name]);
    let owner = h.call(globals, b"bindingOwner", &[]);
    let state = h.call(globals, b"bindingState", &[]);
    assert_eq!(h.load("globalsOwner"), globals);
    assert_eq!(h.call(globals, b"bindingOwner", &[]), owner);
    assert_eq!(h.call(globals, b"bindingState", &[]), state);
    assert_eq!(h.call(globals, b"at:", &[a]), name);
    assert_eq!(h.call(globals, b"includes:", &[b]), h.named(b"false"));
    assert_eq!(h.call(globals, b"at:", &[foreign]), value);
    let optional = PathBuf::from(std::env::var_os("BIRNAM_OPTIONAL_MODULE_DIRECTORY").unwrap());
    let load_optional = |name: &str| {
        let value = host
            .load_native(&q, &optional.join(format!("libbirnam_{name}.dylib")), &[ns])
            .unwrap();
        q.check_result(value).unwrap();
    };
    for name in ["store", "relational"] {
        load_optional(name);
        assert_eq!(h.named(b"Globals"), globals);
        assert_eq!(h.call(globals, b"bindingOwner", &[]), owner);
        assert_eq!(h.call(globals, b"at:", &[foreign]), value);
    }
    let store = h.named(b"Store");
    let entity = h.key(b"__globals__");
    assert_eq!(
        h.call(store, b"hasValue:attr:", &[entity, foreign]),
        h.named(b"false")
    );
    h.call(store, b"assert:attr:value:", &[entity, foreign, nil]);
    assert_eq!(h.call(globals, b"at:", &[foreign]), value);
    let basis = h.call(store, b"basis", &[]);
    h.call(globals, b"at:put:", &[name, value]);
    assert_eq!(h.number(h.call(store, b"basis", &[])), h.number(basis));
    for name in ["store", "relational"] {
        load_optional(name);
    }
    assert_eq!(h.named(b"Store"), store);
    assert_eq!(
        h.call(store, b"hasValue:attr:", &[entity, foreign]),
        h.named(b"true")
    );
    assert_eq!(h.call(globals, b"bindingOwner", &[]), owner);
    assert_eq!(h.call(globals, b"at:", &[foreign]), value);
}
