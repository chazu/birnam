//! Exact grammar comparison against the explicitly selected build-only seed.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, size};
use birnam_host_support::{Context, Scope};
use std::{os::unix::ffi::OsStrExt, path::PathBuf};

fn number(q: &Scope<'_>, value: Oop) -> usize {
    let bits = q
        .call(q.host.named(b"Integer").unwrap(), b"wordOf:", &[value])
        .unwrap();
    q.word_bits(bits).unwrap()
}

fn tree(h: &Context, node: Oop, rows: &mut Vec<(Vec<u8>, usize, usize, usize)>) {
    let q = h.scope();
    q.keep(node).unwrap();
    let rule = q.call(node, b"rule", &[]).unwrap();
    let rule = q.read_bytes(rule).unwrap();
    let start = number(&q, q.call(node, b"start", &[]).unwrap());
    let end = number(&q, q.call(node, b"end", &[]).unwrap());
    let count = number(&q, q.call(node, b"childCount", &[]).unwrap());
    rows.push((rule, start, end, count));
    for index in 0..count {
        let index = q
            .call(
                h.named(b"Integer").unwrap(),
                b"fromWord:",
                &[size(index).unwrap()],
            )
            .unwrap();
        let child = q.call(node, b"childAt:", &[index]).unwrap();
        tree(h, child, rows);
    }
}

#[derive(Debug, PartialEq)]
enum Parse {
    Tree(Vec<(Vec<u8>, usize, usize, usize)>),
    Failure(Vec<u8>, Vec<u8>, usize),
}

fn parse(h: &Context, parser: Oop, source: Oop) -> Parse {
    let q = h.scope();
    let value = q.call(parser, b"parse:", &[source]).unwrap();
    if q.call(value, b"isError", &[]).unwrap() == h.named(b"true").unwrap() {
        let kind = q.call(value, b"kind", &[]).unwrap();
        let message = q.call(value, b"message", &[]).unwrap();
        let bytes = q.call(message, b"bytes", &[]).unwrap();
        Parse::Failure(
            q.read_bytes(kind).unwrap(),
            q.read_bytes(bytes).unwrap(),
            number(&q, q.call(parser, b"furthestFail", &[]).unwrap()),
        )
    } else {
        let mut rows = Vec::new();
        tree(h, value, &mut rows);
        Parse::Tree(rows)
    }
}

#[test]
#[ignore = "requires the explicit grammar seed and production Lang artifacts"]
fn authored_lang_matches_seed_trees_spans_and_diagnostics() {
    let h = oop::bootstrap();
    let held = h.scope();
    let seed = held.keep(h.named(b"Lang").unwrap()).unwrap();
    let codegen = h.named(b"Codegen").unwrap();
    for name in ["modules", "lang"] {
        let q = h.scope();
        let path = h.resources.lib_dir.join(format!("libbirnam_{name}.dylib"));
        let path = q.string(path.as_os_str().as_bytes()).unwrap();
        let result = q.call(codegen, b"load:", &[path]).unwrap();
        assert_ne!(result, h.named(b"nil").unwrap());
        q.check_result(result).unwrap();
    }
    let authored = held.keep(h.named(b"Lang").unwrap()).unwrap();
    assert_ne!(authored, seed);
    let manifest = PathBuf::from(std::env::var_os("BIRNAM_LANG_CORPUS").unwrap());
    let paths = std::fs::read_to_string(manifest).unwrap();
    let gc = h.named(b"GC").unwrap();
    h.send(gc, b"precisePayloads:", &[size(1).unwrap()])
        .unwrap();
    let mut count = 0;
    for path in paths.lines() {
        {
            let q = h.scope();
            let source = q.string(&std::fs::read(path).unwrap()).unwrap();
            let expected = parse(&h, seed, source);
            let actual = parse(&h, authored, source);
            assert_eq!(actual, expected, "Lang grammar differs for {path}");
        }
        h.send(gc, b"collectPrecisely", &[]).unwrap();
        count += 1;
    }
    let clone = held.call(authored, b"clone", &[]).unwrap();
    h.send(gc, b"collectPrecisely", &[]).unwrap();
    assert_eq!(
        held.call(clone, b"grammar", &[]).unwrap(),
        held.call(authored, b"grammar", &[]).unwrap()
    );
    assert_eq!(
        held.call(authored, b"grammar", &[]).unwrap(),
        h.named(b"BirnamGrammar").unwrap()
    );
    let source = held.string(b"[1 + 2]").unwrap();
    assert_eq!(parse(&h, clone, source), parse(&h, seed, source));
    println!(
        "Lang: identical CSTs, spans and diagnostics for {count} fixtures and shipped sources; cloning and GC passed"
    );
}
