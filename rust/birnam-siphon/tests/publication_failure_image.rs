//! Characterize a known adapter failure-channel defect after image restoration.
//! Passing this experiment confirms the gap; it does not qualify publication migration.
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
    unsafe fn keep(&self, o: Oop) -> Oop {
        self.tokens
            .borrow_mut()
            .push(unsafe { retain(self.id, self.roots, o) });
        o
    }
    unsafe fn key(&self, n: &[u8]) -> Oop {
        unsafe { self.keep(self.id.intern_name(n).unwrap()) }
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
}

impl H {
    unsafe fn text(&self, bytes: &[u8]) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                self.named(b"String"),
                b"fromBytes:",
                &[Operand::bytes(bytes)],
            )))
        }
    }
    unsafe fn global(&self, name: &[u8]) -> Oop {
        unsafe { self.call(self.named(b"Globals"), b"at:", &[self.key(name)]) }
    }
}

#[test]
#[ignore = "production modules"]
fn characterizes_adapter_failure_channel_change_across_images() {
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
        let dir = std::env::temp_dir().join(format!("birnam-failure-image-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let artifact = dir.join("policy.dylib");
        let image = dir.join("policy.image");
        let output = h.text(artifact.as_os_str().as_bytes());
        let image_path = h.text(image.as_os_str().as_bytes());
        let text = h.text(include_bytes!(
            "../../../tests/fixtures/publication-failure-image.birnam"
        ));
        let compiled = h.call(
            h.named(b"Codegen"),
            b"compile:output:from:synchronousBodies:",
            &[text, output, output, h.named(b"true")],
        );
        assert_ne!(compiled, h.named(b"nil"));
        assert_eq!(h.call(compiled, b"isError", &[]), h.named(b"false"));
        h.call(h.named(b"Codegen"), b"load:", &[output]);
        for mode in ["fresh", "mapped", "rebuilt"] {
            if mode == "fresh" {
                assert_eq!(
                    h.call(h.named(b"System"), b"saveImage:", &[image_path]),
                    h.named(b"true")
                );
                std::fs::remove_file(&artifact).unwrap();
            } else {
                let loaded = h.call(
                    h.named(b"System"),
                    b"loadImage:rebuild:",
                    &[
                        image_path,
                        h.named(if mode == "rebuilt" { b"true" } else { b"false" }),
                    ],
                );
                // The result reports mapped (true) versus rebuilt (false), not success.
                assert_eq!(
                    loaded,
                    h.named(if mode == "rebuilt" { b"false" } else { b"true" })
                );
            }
            collect(id, h.named(b"GC"));
            let owner = h.global(b"PublicationFailureImageOwner");
            let result = id.send_named(owner, b"publish", &[]);
            if mode == "fresh" {
                let e = result.unwrap_err();
                assert_eq!((e.status, e.detail), (1, 0));
                eprintln!(
                    "image failure boundary {mode}: native {} {}",
                    e.status, e.detail
                );
            } else {
                // Known defect: the image bundle creates a resumable named
                // adapter even for this explicitly synchronous native policy.
                // Change this expectation to exact native (1,0) when fixed.
                let value = h.keep(object(result.expect("current image path converts failure")));
                assert_eq!(h.call(value, b"isError", &[]), h.named(b"true"));
                assert_eq!(h.call(value, b"kind", &[]), h.key(b"nativeInvalidRequest"));
                eprintln!(
                    "image failure boundary {mode}: Error(nativeInvalidRequest), native channel lost"
                );
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
