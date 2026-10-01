//! Exact grammar comparison against the explicitly selected build-only seed.
mod support;
use birnam_abi::{
    Oop, borrowed_bytes,
    id_protocol::{Operand, Protocol},
};
use std::{os::unix::ffi::OsStrExt, path::PathBuf};
use support::*;

struct LangProof {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    integer: Oop,
}
impl LangProof {
    unsafe fn named(&self, name: &[u8]) -> Oop {
        unsafe {
            object(send(
                self.id,
                self.ns,
                b"at:",
                &[Operand::object(self.id.intern_name(name).unwrap())],
            ))
        }
    }
    unsafe fn call(&self, receiver: Oop, selector: &[u8], args: &[Oop]) -> Oop {
        unsafe {
            object(send(
                self.id,
                receiver,
                selector,
                &args
                    .iter()
                    .copied()
                    .map(Operand::object)
                    .collect::<Vec<_>>(),
            ))
        }
    }
    unsafe fn text(&self, value: Oop) -> Vec<u8> {
        unsafe {
            let bytes = send(self.id, value, b"bytes", &[]).as_bytes().unwrap();
            borrowed_bytes(bytes.pointer, bytes.length)
                .unwrap()
                .to_vec()
        }
    }
    unsafe fn number(&self, value: Oop) -> usize {
        unsafe {
            send(self.id, self.integer, b"wordOf:", &[Operand::object(value)])
                .as_word()
                .unwrap()
        }
    }
    unsafe fn string(&self, bytes: &[u8]) -> Oop {
        unsafe {
            object(send(
                self.id,
                self.named(b"String"),
                b"fromBytes:",
                &[Operand::bytes(bytes)],
            ))
        }
    }
    unsafe fn symbol(&self, value: Oop) -> Vec<u8> {
        unsafe {
            let bytes = send(
                self.id,
                self.id.receiver,
                b"symbolBytesOf:",
                &[Operand::object(value)],
            )
            .as_bytes()
            .unwrap();
            borrowed_bytes(bytes.pointer, bytes.length)
                .unwrap()
                .to_vec()
        }
    }
    unsafe fn tree(&self, node: Oop, rows: &mut Vec<(Vec<u8>, usize, usize, usize)>) {
        unsafe {
            let token = retain(self.id, self.roots, node);
            let rule = self.symbol(self.call(node, b"rule", &[]));
            let start = self.number(self.call(node, b"start", &[]));
            let end = self.number(self.call(node, b"end", &[]));
            let count = self.number(self.call(node, b"childCount", &[]));
            rows.push((rule, start, end, count));
            for index in 0..count {
                let index = object(send(
                    self.id,
                    self.integer,
                    b"fromWord:",
                    &[Operand::word(index)],
                ));
                let child = self.call(node, b"childAt:", &[index]);
                self.tree(child, rows);
            }
            release(self.id, self.roots, token);
        }
    }
    unsafe fn parse(&self, parser: Oop, source: Oop) -> Parse {
        unsafe {
            let value = self.call(parser, b"parse:", &[source]);
            let token = retain(self.id, self.roots, value);
            let result = if self.call(value, b"isError", &[]) == self.named(b"true") {
                Parse::Failure(
                    self.symbol(self.call(value, b"kind", &[])),
                    self.text(self.call(value, b"message", &[])),
                    self.number(self.call(parser, b"furthestFail", &[])),
                )
            } else {
                let mut rows = Vec::new();
                self.tree(value, &mut rows);
                Parse::Tree(rows)
            };
            release(self.id, self.roots, token);
            result
        }
    }
}
#[derive(Debug, PartialEq)]
enum Parse {
    Tree(Vec<(Vec<u8>, usize, usize, usize)>),
    Failure(Vec<u8>, Vec<u8>, usize),
}

#[test]
#[ignore = "requires the explicit grammar seed and production Lang artifacts"]
fn authored_lang_matches_seed_trees_spans_and_diagnostics() {
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
        let mut h = LangProof {
            id,
            ns,
            roots,
            integer: std::ptr::null_mut(),
        };
        h.integer = h.named(b"Integer");
        let seed = h.named(b"Lang");
        let seed_token = retain(id, roots, seed);
        let codegen = h.named(b"Codegen");
        for name in ["modules", "lang"] {
            let path = directory().join(format!("libbirnam_{name}.dylib"));
            let path = h.string(path.as_os_str().as_bytes());
            let token = retain(id, roots, path);
            let result = h.call(codegen, b"load:", &[path]);
            assert_ne!(result, h.named(b"nil"));
            assert_eq!(h.call(result, b"isError", &[]), h.named(b"false"));
            release(id, roots, token);
        }
        let authored = h.named(b"Lang");
        assert_ne!(authored, seed);
        let manifest = PathBuf::from(std::env::var_os("BIRNAM_LANG_CORPUS").unwrap());
        let paths = std::fs::read_to_string(manifest).unwrap();
        let mut count = 0;
        for path in paths.lines() {
            let bytes = std::fs::read(path).unwrap();
            let source = h.string(&bytes);
            let token = retain(id, roots, source);
            let expected = h.parse(seed, source);
            let actual = h.parse(authored, source);
            assert_eq!(actual, expected, "Lang grammar differs for {path}");
            release(id, roots, token);
            collect(id, h.named(b"GC"));
            count += 1;
        }
        let clone = h.call(authored, b"clone", &[]);
        let clone_token = retain(id, roots, clone);
        collect(id, h.named(b"GC"));
        assert_eq!(
            h.call(clone, b"grammar", &[]),
            h.call(authored, b"grammar", &[])
        );
        assert_eq!(h.call(authored, b"grammar", &[]), h.named(b"BirnamGrammar"));
        let source = h.string(b"[1 + 2]");
        let source_token = retain(id, roots, source);
        assert_eq!(h.parse(clone, source), h.parse(seed, source));
        release(id, roots, source_token);
        release(id, roots, clone_token);
        release(id, roots, seed_token);
        println!(
            "Lang: identical CSTs, spans and diagnostics for {count} fixtures and shipped sources; cloning and GC passed"
        );
    }
}
