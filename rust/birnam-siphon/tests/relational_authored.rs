//! The optional authored positive-query slice uses live Oop equality and callbacks.
#[path = "support/oop.rs"]
mod oop;
#[path = "support/generation.rs"]
#[allow(dead_code)]
mod support;
use birnam_abi::{size, Oop};
use support::H;
fn elements(h: &H<'_>, value: Oop) -> Vec<Oop> {
    (0..h.number(h.call(value, b"size", &[])))
        .map(|i| h.at(value, i))
        .collect()
}
fn query(h: &H<'_>, owner: Oop, store: Oop, result: Oop, clauses: &[&[Oop]]) -> Oop {
    let facade = h.call(owner, b"on:", &[store]);
    let rows = clauses.iter().map(|row| h.vector(row)).collect::<Vec<_>>();
    let answer = h.call(facade, b"findAll:where:", &[result, h.vector(&rows)]);
    h.q.check_result(answer).unwrap();
    answer
}
#[test]
#[ignore = "requires production Oop modules"]
fn authored_queries_preserve_order_and_use_live_world_equality() {
    let host = oop::bootstrap();
    let h = H::with_fixture(&host, "relational");
    h.modules(&["store", "relational", "stdlib"]);
    let source = h.text(include_bytes!("../../../modules/relational-query.birnam"));
    let path = std::env::temp_dir().join(format!(
        "birnam-relational-oop-{}.dylib",
        std::process::id()
    ));
    let filename = h.text(path.as_os_str().as_encoded_bytes());
    let compiled = h.call(
        h.named(b"Codegen"),
        b"compile:output:from:synchronousBodies:",
        &[source, filename, filename, h.named(b"true")],
    );
    h.q.check_result(compiled).unwrap();
    let loaded = h.call(h.named(b"Codegen"), b"load:", &[filename]);
    h.q.check_result(loaded).unwrap();
    std::fs::remove_file(path).unwrap();
    let owner = h.named(b"RelationalQuery");
    let store = h.named(b"Store");
    let nil = h.named(b"nil");
    let variable = |name: &[u8]| h.call(h.named(b"LogicVar"), b"named:", &[h.key(name)]);
    let (x, y, z) = (variable(b"x"), variable(b"y"), variable(b"z"));
    let attr = h.key(b"edge");
    let nodes = [h.key(b"a"), h.key(b"b")];
    for bits in 0..16 {
        let data = h.call(store, b"new", &[]);
        for i in 0..2 {
            for j in 0..2 {
                if bits & (1 << (2 * i + j)) != 0 {
                    h.call(data, b"assert:attr:value:", &[nodes[i], attr, nodes[j]]);
                }
            }
        }
        for (result, clauses) in [
            (y, vec![vec![x, attr, y]]),
            (x, vec![vec![x, attr, x]]),
            (z, vec![vec![x, attr, y], vec![y, attr, z]]),
            (y, vec![vec![nodes[0], attr, y]]),
        ] {
            let rows = clauses.iter().map(Vec::as_slice).collect::<Vec<_>>();
            let native = h.call(
                data,
                b"findAll:where:",
                &[
                    result,
                    h.vector(&clauses.iter().map(|r| h.vector(r)).collect::<Vec<_>>()),
                ],
            );
            assert_eq!(
                elements(&h, query(&h, owner, data, result, &rows)),
                elements(&h, native),
                "graph={bits}"
            );
        }
    }
    let data = h.call(store, b"new", &[]);
    for (i, value) in [nodes[0], nodes[1], nodes[1]].into_iter().enumerate() {
        h.call(data, b"assert:attr:value:", &[h.integer(i), attr, value]);
    }
    let native = h.call(
        data,
        b"findAll:where:",
        &[x, h.vector(&[h.vector(&[x, attr, y])])],
    );
    let order = elements(&h, native)
        .into_iter()
        .map(|v| h.number(v))
        .collect::<Vec<_>>();
    let pattern = h.call(h.named(b"Object"), b"clone", &[]);
    let seen = h.eval(b"[Cell with: #[]]");
    let changing_equality=h.eval(b"[:pattern :seen | [Reflect defineMethod: #= on: pattern as: [:self :other | [seen set: [[seen value] add: #[self other]]]; [Reflect defineMethod: #= on: self as: [:self :other | [seen set: [[seen value] add: #[self other]]]; [System gc]; true]]; false]]]");
    h.call(changing_equality, b"value:value:", &[pattern, seen]);
    assert_eq!(
        elements(&h, query(&h, owner, data, x, &[&[x, attr, pattern]]))
            .into_iter()
            .map(|v| h.number(v))
            .collect::<Vec<_>>(),
        order[1..]
    );
    let pairs = elements(&h, h.call(seen, b"value", &[]));
    assert_eq!(pairs.len(), 3);
    assert!(pairs.iter().all(|&row| h.at(row, 0) == pattern));
    let facade = h.call(owner, b"on:", &[data]);
    let rows = h.vector(&[h.vector(&[x, attr, pattern])]);
    let counter = h.eval(b"[Cell with: 0]");
    let factory=h.eval(b"[:pattern :count :facade :x :y :attr | [Reflect defineMethod: #= on: pattern as: [:self :other | [count inc]; [[[count value] = 1] ifTrue: [| [facade findAll: y where: #[#[x attr y]]]]]; [System gc]; true]]]");
    h.call(
        factory,
        b"value:value:value:value:value:value:",
        &[pattern, counter, facade, x, y, attr],
    );
    query(&h, owner, data, x, &[&[x, attr, pattern]]);
    assert_eq!(h.number(h.call(counter, b"value", &[])), 3);

    let hooks = oop::hooks::Hooks::new(&host);
    let failure = hooks.fault(719);
    let error = h.call(h.named(b"Error"), b"kind:", &[h.key(b"queryProbe")]);
    for (native, value) in [(true, failure), (false, error)] {
        let state = hooks.context(&[value, size(0).unwrap()]);
        h.bind(
            h.vt(pattern),
            b"=",
            hooks.method(if native { b"deny" } else { b"fixedCount" }, state),
        );
        let result = unsafe { host.id.send(facade, h.key(b"findAll:where:"), &[x, rows]) };
        if native {
            assert_eq!(result.unwrap_err(), failure);
        } else {
            assert_eq!(result.unwrap(), error);
        }
        assert_eq!(
            hooks.count(state, 1),
            1,
            "failure must stop later comparisons"
        );
    }
    let dedup = h.call(store, b"new", &[]);
    let second = h.call(h.named(b"Object"), b"clone", &[]);
    let third = h.call(h.named(b"Object"), b"clone", &[]);
    for (i, value) in [pattern, second, third].into_iter().enumerate() {
        h.call(dedup, b"assert:attr:value:", &[h.integer(i), attr, value]);
    }
    h.call(seen, b"set:", &[h.vector(&[])]);
    h.call(changing_equality, b"value:value:", &[pattern, seen]);
    assert_eq!(
        elements(&h, query(&h, owner, dedup, y, &[&[x, attr, y]])),
        vec![pattern, second]
    );
    let pairs = elements(&h, h.call(seen, b"value", &[]));
    assert_eq!(pairs.len(), 2);
    assert_eq!((h.at(pairs[0], 0), h.at(pairs[0], 1)), (pattern, second));
    assert_eq!((h.at(pairs[1], 0), h.at(pairs[1], 1)), (pattern, third));
    let repeated = h.call(store, b"new", &[]);
    for i in 0..3 {
        h.call(
            repeated,
            b"assert:attr:value:",
            &[h.integer(i), attr, pattern],
        );
    }
    let state = hooks.context(&[h.named(b"false"), size(0).unwrap()]);
    h.bind(h.vt(pattern), b"=", hooks.method(b"fixedCount", state));
    assert_eq!(
        elements(&h, query(&h, owner, repeated, y, &[&[x, attr, y]])),
        vec![pattern; 3]
    );
    assert_eq!(
        hooks.count(state, 1),
        3,
        "identity must not replace live value equality"
    );

    let provider = h.call(h.named(b"Object"), b"clone", &[]);
    let vt = h.call(host.id.root, b"ensurePrivateVtableFor:", &[provider]);
    let emission = h.context(&[
        h.gc,
        h.named(b"EntityVt"),
        host.roots,
        h.named(b"Integer"),
        attr,
        nil,
        size(0).unwrap(),
        nil,
        size(0).unwrap(),
        failure,
    ]);
    let monitor = h.call(h.named(b"Object"), b"clone", &[]);
    let mv = h.call(host.id.root, b"ensurePrivateVtableFor:", &[monitor]);
    for name in [b"present".as_slice(), b"allocation:bytes:identity:"] {
        h.bind(mv, name, h.method(name, monitor));
    }
    for mode in 0..3 {
        h.put(emission, 8, size(mode).unwrap());
        h.bind(
            vt,
            b"liveTriplesDo:",
            h.method(if mode == 0 { b"emit" } else { b"emitFailure" }, emission),
        );
        {
            let local = host.scope();
            let facade = local.call(owner, b"on:", &[provider]).unwrap();
            let clauses = h.vector(&[h.vector(&[x, attr, y])]);
            let result = unsafe {
                host.id
                    .send(facade, h.key(b"findAll:where:"), &[x, clauses])
            };
            if mode == 0 {
                let result = local.keep(result.unwrap()).unwrap();
                let entity = local.vector_at(result, 0).unwrap();
                assert_eq!(
                    local.call(host.id.root, b"identityOf:", &[entity]).unwrap(),
                    h.call(emission, b"at:", &[size(6).unwrap()])
                );
                local.call(h.gc, b"collectPrecisely", &[]).unwrap();
            } else {
                let error = h.keep(result.unwrap_err());
                assert_eq!(h.call(error, b"faultKind", &[]), h.key(b"nativeFailure"));
                let detail = h.call(error, b"detail", &[]);
                let expected = if mode == 1 {
                    h.call(failure, b"detail", &[])
                } else {
                    h.context(&[size(1).unwrap(), size(0).unwrap()])
                };
                for index in 0..2 {
                    assert_eq!(
                        h.call(detail, b"at:", &[size(index).unwrap()]),
                        h.call(expected, b"at:", &[size(index).unwrap()])
                    );
                }
            }
        }
        let sink = h.call(emission, b"at:", &[size(5).unwrap()]);
        assert!(unsafe {
            host.id
                .send(sink, h.key(b"value:value:value:"), &[nil, nil, nil])
        }
        .is_err());
        h.call(h.gc, b"collectPrecisely", &[]);
        let identity = h.call(emission, b"at:", &[size(6).unwrap()]);
        assert_eq!(
            h.call(monitor, b"present", &[identity]),
            size(0).unwrap(),
            "expired collector must release its snapshot"
        );
    }
    for malformed in [
        h.vector(&[h.vector(&[x, attr])]),
        h.vector(&[h.vector(&[h.key(b"rule"), attr, x])]),
        h.vector(&[h.vector(&[x, h.key(b"="), y])]),
    ] {
        assert_eq!(h.call(facade, b"findAll:where:", &[x, malformed]), nil);
    }
    assert_eq!(
        h.call(facade, b"findAll:where:", &[nodes[0], h.vector(&[])]),
        nil
    );
}
