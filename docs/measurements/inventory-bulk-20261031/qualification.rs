use birnam_abi::{self as abi, Oop};
use birnam_codegen::{CstNode, OwnedCst};
use birnam_host_support::{Context, Resources, Scope};
use std::{os::unix::ffi::OsStrExt, path::PathBuf};
fn context(prefix: PathBuf) -> Context {
    let lib = prefix.join("lib");
    let id = birnam_siphon::load(
        lib.join("libbirnam_id.dylib").as_os_str().as_bytes(),
        &[
            lib.join("libbirnam_bootstrap.dylib").as_os_str().as_bytes(),
            lib.as_os_str().as_bytes(),
            b"nameWithBytes",
            b"sequence",
        ],
    )
    .unwrap();
    unsafe {
        let constructor = id.send(id.literal(2).unwrap(), id.intern, &[]).unwrap();
        let seq = id.send(id.literal(3).unwrap(), id.intern, &[]).unwrap();
        let args = id.send(id.root, seq, &[id.literal(1).unwrap()]).unwrap();
        let ns = id.send(id.literal(0).unwrap(), id.load, &[args]).unwrap();
        Context::from_namespace(
            id,
            ns,
            Resources {
                prefix: prefix.clone(),
                lib_dir: lib.clone(),
                id_dylib: lib.join("libbirnam_id.dylib"),
                rust_workspace_dir: prefix.join("share/birnam/src"),
                guide_dir: prefix.join("share/birnam/docs/guide"),
            },
            constructor,
        )
        .unwrap()
    }
}
fn number(q: &Scope<'_>, h: &Context, o: Oop) -> usize {
    q.word_bits(
        q.call(h.named(b"Integer").unwrap(), b"wordOf:", &[o])
            .unwrap(),
    )
    .unwrap()
}
fn at(q: &Scope<'_>, h: &Context, o: Oop, i: usize) -> Oop {
    let n = q
        .call(
            h.named(b"Integer").unwrap(),
            b"fromWord:",
            &[abi::size(i).unwrap()],
        )
        .unwrap();
    q.call(o, b"at:", &[n]).unwrap()
}
fn project(q: &Scope<'_>, h: &Context, n: Oop) -> OwnedCst {
    let rule = q
        .string_bytes(q.call(n, b"ruleName", &[]).unwrap())
        .unwrap();
    let start = number(q, h, q.call(n, b"start", &[]).unwrap());
    let end = number(q, h, q.call(n, b"end", &[]).unwrap());
    let count = number(q, h, q.call(n, b"childCount", &[]).unwrap());
    let children = (0..count)
        .map(|i| {
            let index = q
                .call(
                    h.named(b"Integer").unwrap(),
                    b"fromWord:",
                    &[abi::size(i).unwrap()],
                )
                .unwrap();
            let child = q.call(n, b"childAt:", &[index]).unwrap();
            project(q, h, child)
        })
        .collect();
    OwnedCst::new(&rule, start, end, children)
}
fn rows(q: &Scope<'_>, h: &Context, v: Oop) -> Vec<Vec<u8>> {
    (0..number(q, h, q.call(v, b"size", &[]).unwrap()))
        .map(|i| q.string_bytes(at(q, h, v, i)).unwrap())
        .collect()
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let h = context(PathBuf::from(&args[1]));
    if std::env::var_os("QUAL_BOOTSTRAP_ONLY").is_some() {
        println!("BOOTSTRAP_COMPLETED");
        return;
    }
    let q = h.scope();
    let codegen = h.named(b"Codegen").unwrap();
    let lang = h.named(b"Lang").unwrap();
    let word = h.named(b"Word").unwrap();
    let mode = std::env::var("QUAL_MODE").unwrap_or("compileSynchronous:".into());
    let source = q.string(&std::fs::read(&args[2]).unwrap()).unwrap();
    let method = q.call(codegen, mode.as_bytes(), &[source]).unwrap();
    q.check_result(method).unwrap();
    let block = q
        .call(
            h.named(b"Block").unwrap(),
            b"fromMethod:captureOperands:",
            &[method],
        )
        .unwrap();
    assert_eq!(q.call(block, b"value", &[]).unwrap(), codegen);
    for name in [
        b"inventoryZero".as_slice(),
        b"inventoryOne",
        b"inventoryLimit",
    ] {
        assert_eq!(h.global(&q, name).unwrap(), h.named(b"nil").unwrap());
    }
    println!("INITIALIZER_BINDINGS_LOCAL_OK");
    let source=br#"[Reflect defineMethod: #outer on: Object as: [:self | [Reflect defineMethod: #inner on: Object as: [:self | 1]]]]"#;
    let text = q.string(source).unwrap();
    let tree = q.call(lang, b"parse:", &[text]).unwrap();
    q.check_result(tree).unwrap();
    let expected = birnam_codegen::source_revision::method_sources(&project(&q, &h, tree), source);
    assert_eq!(expected.len(), 2);
    let inventory = |node| {
        let local = h.scope();
        let got = local
            .call(codegen, b"methodSourceTextsIn:source:", &[node, text])
            .unwrap();
        local.check_result(got).unwrap();
        rows(&local, &h, got)
    };
    assert_eq!(inventory(tree), expected);
    fn first_send(q: &Scope<'_>, h: &Context, n: Oop) -> Option<Oop> {
        let rule = q
            .string_bytes(q.call(n, b"ruleName", &[]).unwrap())
            .unwrap();
        if rule == b"send" {
            return Some(n);
        }
        let count = number(q, h, q.call(n, b"childCount", &[]).unwrap());
        for i in 0..count {
            let index = q
                .call(
                    h.named(b"Integer").unwrap(),
                    b"fromWord:",
                    &[abi::size(i).unwrap()],
                )
                .unwrap();
            let child = q.call(n, b"childAt:", &[index]).unwrap();
            if let Some(found) = first_send(q, h, child) {
                return Some(found);
            }
        }
        None
    }
    let declaration = first_send(&q, &h, tree).unwrap();
    let empty = q.call(h.named(b"Vector").unwrap(), b"new", &[]).unwrap();
    for (depth, count) in [(0, 2), (4095, 1), (4096, 0), (4097, 0)] {
        let depth = q
            .call(word, b"fromBits:", &[abi::size(depth).unwrap()])
            .unwrap();
        let found = q
            .call(
                codegen,
                b"inventoryCollect:source:depth:into:",
                &[declaration, text, depth, empty],
            )
            .unwrap();
        q.check_result(found).unwrap();
        let found = rows(&q, &h, found);
        assert_eq!(found, expected[..count]);
    }
    println!("DEPTH_BOUNDARY_OK");
    let factory_source = q
        .string(
            br#"[:node |
   let children := [[[Vector new] add: node] add: node];
   let proxy := [Object clone];
   [Reflect defineMethod: #ruleName on: proxy as: [:self | "container"]];
   [Reflect defineMethod: #start on: proxy as: [:self | 0]];
   [Reflect defineMethod: #end on: proxy as: [:self | 0]];
   [Reflect defineMethod: #childCount on: proxy as: [:self | [children size]]];
   [Reflect defineMethod: #childAt: on: proxy as: [:self :index | [children at: index]]];
   [Reflect defineMethod: #children on: proxy as: [:self | children]];
   [Reflect defineMethod: #useChildren: on: proxy as: [:self :next |
     [Reflect defineMethod: #children on: self as: [:self | next]]; self
   ]];
   proxy
 ]"#,
        )
        .unwrap();
    let factory = q.call(codegen, b"eval:", &[factory_source]).unwrap();
    q.check_result(factory).unwrap();
    let proxy = q.call(factory, b"value:", &[tree]).unwrap();
    q.check_result(proxy).unwrap();
    assert_ne!(
        q.call(h.id.root, b"behaviorOf:", &[proxy]).unwrap(),
        q.call(h.id.root, b"behaviorOf:", &[tree]).unwrap()
    );
    let projected = project(&q, &h, proxy);
    let proxy_expected = birnam_codegen::source_revision::method_sources(&projected, source);
    assert_eq!(
        proxy_expected,
        [expected.clone(), expected.clone()].concat()
    );
    assert_eq!(inventory(proxy), proxy_expected);
    q.call(h.named(b"GC").unwrap(), b"collectPrecisely", &[])
        .unwrap();
    assert_eq!(inventory(proxy), proxy_expected);
    println!("SUBSTITUTE_CST_AND_GC_OK");
    q.call(proxy, b"useChildren:", &[empty]).unwrap();
    assert!(inventory(proxy).is_empty());
    assert_eq!(
        birnam_codegen::source_revision::method_sources(&project(&q, &h, proxy), source),
        proxy_expected
    );
    println!("LIVE_BULK_REPLACEMENT_DIFFERS_FROM_INDEXED_VIEW");

    if let Ok(depth) = std::env::var("QUAL_CHAIN_DEPTH") {
        let depth: usize = depth.parse().unwrap();
        let code = q
            .string(
                br#"[:child |
   let node := [Object clone];
   let children := [[Vector new] add: child];
   [Reflect defineMethod: #ruleName on: node as: [:self | "branch"]];
   [Reflect defineMethod: #children on: node as: [:self | children]];
   node
  ]"#,
            )
            .unwrap();
        let factory = q.call(codegen, b"eval:", &[code]).unwrap();
        q.check_result(factory).unwrap();
        let mut node = declaration;
        for _ in 0..depth {
            node = q.call(factory, b"value:", &[node]).unwrap();
            q.check_result(node).unwrap();
        }
        eprintln!("CHAIN_READY depth={depth}");
        let found = inventory(node);
        let count = if depth >= 4096 {
            0
        } else if depth == 4095 {
            1
        } else {
            2
        };
        assert_eq!(found, expected[..count]);
        println!("REAL_CHAIN_OK depth={depth}");
    }
    std::fs::write(&args[3],serde_json::to_vec_pretty(&serde_json::json!({"mode":mode,"depth_boundary":true,"substitute_cst":true,"precise_gc":true,"bulk_replacement_observed":true,"inconsistent_indexed_view_is_not_equivalent":true})).unwrap()).unwrap();
}
