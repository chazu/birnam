//! Object-level Method binding construction from ordinary compiled templates.
mod support;
use birnam_abi::{
    Oop,
    id_protocol::{Operand, Protocol},
};
use std::{cell::RefCell, os::unix::ffi::OsStrExt};
use support::*;
struct H {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    tokens: RefCell<Vec<usize>>,
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
    unsafe fn keep(&self, v: Oop) -> Oop {
        self.tokens
            .borrow_mut()
            .push(unsafe { retain(self.id, self.roots, v) });
        v
    }
    unsafe fn call(&self, r: Oop, n: &[u8], args: &[Oop]) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                r,
                n,
                &args
                    .iter()
                    .copied()
                    .map(Operand::object)
                    .collect::<Vec<_>>(),
            )))
        }
    }
    unsafe fn integer(&self, n: usize) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                self.named(b"Integer"),
                b"fromWord:",
                &[Operand::word(n)],
            )))
        }
    }
    unsafe fn number(&self, n: Oop) -> usize {
        unsafe {
            send(
                self.id,
                self.named(b"Integer"),
                b"wordOf:",
                &[Operand::object(n)],
            )
            .as_word()
            .unwrap()
        }
    }
    unsafe fn text(&self, b: &[u8]) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                self.named(b"String"),
                b"fromBytes:",
                &[Operand::bytes(b)],
            )))
        }
    }
    unsafe fn eval(&self, b: &[u8]) -> Oop {
        unsafe {
            let result = self.call(self.named(b"Codegen"), b"eval:", &[self.text(b)]);
            assert_eq!(self.call(result, b"isError", &[]), self.named(b"false"));
            result
        }
    }
    unsafe fn clear(&self, mark: usize) {
        for t in self.tokens.borrow_mut().drain(mark..).rev() {
            unsafe { release(self.id, self.roots, t) }
        }
    }
}

#[test]
#[ignore = "production modules"]
fn generated_templates_bind_through_source_and_artifact_messages() {
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
        let h = H {
            id,
            ns,
            roots,
            tokens: RefCell::new(vec![]),
        };
        let source = include_bytes!("../../../tests/fixtures/method-binding.birnam");
        let dir =
            std::env::temp_dir().join(format!("birnam-method-binding-{}", std::process::id()));
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
            collect(id, h.named(b"GC"));
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
                    id.intern_name(tag).unwrap()
                );
            }
            eprintln!("Method withImplementation: generated template from {mode}: passed");
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
            collect(id, h.named(b"GC"));
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
                    id.intern_name(tag).unwrap()
                );
            }
            // A restored Method must also provide a usable owned code template.
            let value = h.eval(
                br#"
              let target := [MethodBindingTargets at: 0];
              let template := [target methodFor: #answer:];
              let bound := [template withImplementation: [:self :argument | argument]];
              [System gc];
              [bound invokeOn: target arguments: #[#restoredBinding]]
            "#,
            );
            assert_eq!(value, id.intern_name(b"restoredBinding").unwrap());
            eprintln!("Method withImplementation: source-free image rebuild={rebuild}: passed");
        }
        // Unsupported templates fail at the explicit owner boundary, not by
        // creating a seemingly valid Method with a stale implementation anchor.
        for template in [
            h.named(b"Method"),
            h.eval(b"[Method methodFor: #fromNative:owner:]"),
        ] {
            let error = id
                .send_named(
                    template,
                    b"withImplementation:",
                    &[Operand::object(h.named(b"nil"))],
                )
                .unwrap_err();
            assert_eq!(error.status, 1);
        }
        // Construction policy is an ordinary replaceable send. Replacing it on
        // this template affects the next binding, with no guard or code cache.
        let result = h.eval(
            br#"
          let target := [MethodBindingTargets at: 0];
          let template := [target methodFor: #answer:];
          let constructor := [template methodFor: #withImplementation:];
          let calls := [Cell with: 0];
          [Reflect defineMethod: #withImplementation: on: template as: [:self :body |
            [calls inc]; [System gc];
            [constructor invokeOn: self arguments: #[body]]
          ]];
          let bound := [template withImplementation: [:self :argument | argument]];
          #[[bound invokeOn: target arguments: #[#currentConstructor]] [calls value]]
        "#,
        );
        assert_eq!(
            h.call(result, b"at:", &[h.integer(0)]),
            id.intern_name(b"currentConstructor").unwrap()
        );
        assert_eq!(h.number(h.call(result, b"at:", &[h.integer(1)])), 1);
        h.eval(b"[Globals remove: #MethodBindingTargets]");
        h.clear(0);
        collect(id, h.named(b"GC"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
