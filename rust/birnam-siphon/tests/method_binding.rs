//! Object-level Method construction from compiled bodies, source and artifacts.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::Oop;
use birnam_host_support::{Context, Scope};
use std::os::unix::ffi::OsStrExt;
struct H<'a> {
    host: &'a Context,
    q: Scope<'a>,
}
impl H<'_> {
    fn named(&self, n: &[u8]) -> Oop {
        self.host.named(n).unwrap()
    }
    fn call(&self, r: Oop, n: &[u8], a: &[Oop]) -> Oop {
        self.q.call(r, n, a).unwrap()
    }
    fn integer(&self, n: usize) -> Oop {
        self.call(
            self.named(b"Integer"),
            b"fromWord:",
            &[self.q.word(n).unwrap()],
        )
    }
    fn number(&self, n: Oop) -> usize {
        self.q.word_bits(self.call(n, b"word", &[])).unwrap()
    }
    fn text(&self, b: &[u8]) -> Oop {
        self.q.string(b).unwrap()
    }
    fn eval(&self, b: &[u8]) -> Oop {
        let v = self.call(self.named(b"Codegen"), b"eval:", &[self.text(b)]);
        self.q.check_result(v).unwrap();
        v
    }
}
#[test]
#[ignore = "requires production Oop modules"]
fn compiled_bodies_bind_through_source_and_artifact_messages() {
    let host = oop::bootstrap();
    let h = H {
        host: &host,
        q: host.scope(),
    };
    let source = include_bytes!("../../../tests/fixtures/method-binding.birnam");
    let dir = std::env::temp_dir().join(format!("birnam-method-binding-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let artifact = dir.join("bindings.dylib");
    let path = h.text(artifact.as_os_str().as_bytes());
    let compiled = h.call(
        h.named(b"Codegen"),
        b"compile:output:from:",
        &[h.text(source), path, path],
    );
    assert_eq!(h.call(compiled, b"isError", &[]), h.named(b"false"));
    for mode in ["source", "artifact"] {
        let result = if mode == "source" {
            h.eval(source)
        } else {
            h.call(h.named(b"Codegen"), b"load:", &[path])
        };
        assert_eq!(result, h.named(b"true"));
        h.call(h.named(b"GC"), b"collectPrecisely", &[]);
        let targets = h.eval(b"MethodBindingTargets");
        let argument = h.call(h.named(b"Object"), b"clone", &[]);
        for (i, tag) in [b"first".as_slice(), b"second".as_slice()]
            .into_iter()
            .enumerate()
        {
            let target = h.call(targets, b"at:", &[h.integer(i)]);
            let value = h.call(target, b"answer:", &[argument]);
            assert_eq!(h.call(value, b"at:", &[h.integer(0)]), target);
            assert_eq!(h.call(value, b"at:", &[h.integer(1)]), argument);
            assert_eq!(
                h.call(value, b"at:", &[h.integer(2)]),
                h.host.intern(tag).unwrap()
            );
        }
        eprintln!("Direct Method construction from {mode}: passed");
    }
    // Installed source-backed implementations survive both restore paths.
    // The image reconstructs logical bindings; it need not preserve process
    // addresses or today's physical code-sharing arrangement.
    let image_path = h.text(dir.join("bindings.image").as_os_str().as_bytes());
    assert_eq!(
        h.call(h.named(b"System"), b"saveImage:", &[image_path]),
        h.named(b"true")
    );
    std::fs::remove_file(&artifact).unwrap();
    for rebuild in [false, true] {
        assert_eq!(
            h.call(
                h.named(b"System"),
                b"loadImage:rebuild:",
                &[
                    image_path,
                    h.named(if rebuild { b"true" } else { b"false" })
                ]
            ),
            h.named(if rebuild { b"false" } else { b"true" })
        );
        h.call(h.named(b"GC"), b"collectPrecisely", &[]);
        let targets = h.eval(b"MethodBindingTargets");
        for (i, tag) in [b"first".as_slice(), b"second".as_slice()]
            .into_iter()
            .enumerate()
        {
            let target = h.call(targets, b"at:", &[h.integer(i)]);
            let value = h.call(target, b"answer:", &[h.named(b"true")]);
            assert_eq!(h.call(value, b"at:", &[h.integer(0)]), target);
            assert_eq!(h.call(value, b"at:", &[h.integer(1)]), h.named(b"true"));
            assert_eq!(
                h.call(value, b"at:", &[h.integer(2)]),
                h.host.intern(tag).unwrap()
            );
        }
        // Ordinary construction remains available after image restoration.
        let value = h.eval(
            br#"
              let target := [MethodBindingTargets at: 0];
              let bound := [Reflect methodForArity: 1 implementation: [:self :argument | argument]];
              [System gc];
              [bound invokeOn: target arguments: #[#restoredBinding]]
            "#,
        );
        assert_eq!(value, h.host.intern(b"restoredBinding").unwrap());
        eprintln!("Direct Method source-free image rebuild={rebuild}: passed");
    }
    // Neither a raw native declaration nor an ordinary compiled root is a
    // body binder. The removed forwarding operation must remain unavailable.
    let native = h.call(
        host.id.root,
        b"bind:to:",
        &[host.intern(b"allocationCount").unwrap(), host.id.root],
    );
    let compiled = h.call(
        h.named(b"Codegen"),
        b"compileSynchronous:",
        &[h.text(b"42")],
    );
    for template in [native, compiled] {
        let error = h.call(template, b"withImplementation:", &[h.named(b"nil")]);
        assert_eq!(h.call(error, b"isError", &[]), h.named(b"true"));
        assert_eq!(
            h.call(error, b"kind", &[]),
            host.intern(b"doesNotUnderstand").unwrap()
        );
    }
    // The public constructor remains replaceable through ordinary lookup.
    let result = h.eval(
        br#"
      let target := [MethodBindingTargets at: 0];
      let constructor := [Reflect methodFor: #methodForArity:implementation:];
      let calls := [Cell with: 0];
      [Reflect defineMethod: #methodForArity:implementation: on: Reflect as: [:self :arity :body |
        [calls inc]; [System gc]; [constructor invokeOn: self arguments: #[arity body]]
      ]];
      let bound := [Reflect methodForArity: 1 implementation: [:self :argument | argument]];
      [[Vtable behaviorOf: Reflect] addMethod: constructor as: #methodForArity:implementation:];
      #[[bound invokeOn: target arguments: #[#currentConstructor]] [calls value]]
    "#,
    );
    assert_eq!(
        h.call(result, b"at:", &[h.integer(0)]),
        h.host.intern(b"currentConstructor").unwrap()
    );
    assert_eq!(h.number(h.call(result, b"at:", &[h.integer(1)])), 1);
    h.eval(b"[Globals remove: #MethodBindingTargets]");
    let gc = h.named(b"GC");
    drop(h);
    host.send(gc, b"collectPrecisely", &[]).unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}
