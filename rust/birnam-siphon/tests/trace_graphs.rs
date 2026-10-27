//! Authored Trace against independent graph closure and adversarial Oop providers.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use birnam_host_support::{Context, Scope};
use std::path::Path;
struct H<'a> {
    host: &'a Context,
    q: Scope<'a>,
    fixture: Oop,
    state: Oop,
    probe: Oop,
    trace: Oop,
    reporter: Oop,
    report_selector: Oop,
}
impl H<'_> {
    fn key(&self, n: &[u8]) -> Oop {
        self.host.intern(n).unwrap()
    }
    fn call(&self, r: Oop, n: &[u8], a: &[Oop]) -> Oop {
        self.q.call(r, n, a).unwrap()
    }
    fn raw(&self, r: Oop, n: &[u8], a: &[Oop]) -> Oop {
        self.host.send(r, n, a).unwrap()
    }
    fn method(&self, n: &[u8]) -> Oop {
        self.call(
            self.call(self.fixture, b"methodAt:", &[self.key(n)]),
            b"withContext:",
            &[self.state],
        )
    }
    fn vt(&self, o: Oop) -> Oop {
        self.call(self.host.id.root, b"behaviorOf:", &[o])
    }
    fn bind(&self, vt: Oop, n: &[u8], m: Oop) {
        self.call(vt, b"addMethod", &[self.key(n), m]);
    }
    fn config(
        &self,
        edges: usize,
        roots: usize,
        anomaly: usize,
        mode: usize,
        result: usize,
        cleanup: usize,
    ) {
        self.raw(
            self.probe,
            b"configure",
            &[edges, roots, anomaly, mode, result, cleanup].map(|v| size(v).unwrap()),
        );
    }
    fn stat(&self, i: usize) -> usize {
        as_size(self.raw(self.probe, b"stats", &[size(i).unwrap()])).unwrap()
    }
    fn field(&self, i: usize) -> Oop {
        self.call(self.state, b"at:", &[size(i).unwrap()])
    }
    fn put(&self, i: usize, v: Oop) {
        self.call(self.state, b"at:put:", &[size(i).unwrap(), v]);
    }
    fn report(&self, mask: Oop) -> Result<Oop, Oop> {
        unsafe {
            self.host
                .id
                .send(self.trace, self.report_selector, &[self.reporter, mask])
        }
    }
    fn rows(&self) -> Vec<(usize, usize, bool)> {
        let q = self.host.scope();
        (0..self.stat(0))
            .map(|i| {
                let read = |f| {
                    q.call(
                        self.probe,
                        b"row:field:",
                        &[size(i).unwrap(), size(f).unwrap()],
                    )
                    .unwrap()
                };
                (
                    as_size(read(0)).unwrap(),
                    q.word_bits(read(1)).unwrap(),
                    as_size(read(2)).unwrap() != 0,
                )
            })
            .collect()
    }
    fn error(&self, value: Oop) -> (usize, usize) {
        let q = self.host.scope();
        q.keep(value).unwrap();
        assert_eq!(
            q.call(value, b"faultKind", &[]).unwrap(),
            self.key(b"nativeFailure")
        );
        let details = q.call(value, b"detail", &[]).unwrap();
        (
            as_size(q.call(details, b"at:", &[size(0).unwrap()]).unwrap()).unwrap(),
            q.word_bits(q.call(details, b"at:", &[size(1).unwrap()]).unwrap())
                .unwrap(),
        )
    }
}
#[test]
#[ignore = "requires production Oop modules and authored Trace"]
fn authored_graphs_preserve_identity_validation_failure_and_native_results() {
    let host = oop::bootstrap();
    let q = host.scope();
    let named = |n: &[u8]| host.named(n).unwrap();
    let gc = named(b"GC");
    q.call(gc, b"automaticCollection:", &[size(0).unwrap()])
        .unwrap();
    q.call(gc, b"precisePayloads:", &[size(1).unwrap()])
        .unwrap();
    let path =
        std::env::temp_dir().join(format!("birnam-trace-graphs-{}.dylib", std::process::id()));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/trace-graphs.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = host.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let nodes: Vec<_> = (0..4).map(|_| q.string(b"same").unwrap()).collect();
    for i in 0..4 {
        for j in 0..i {
            assert_ne!(nodes[i], nodes[j]);
        }
    }
    let ids = [
        size(1).unwrap(),
        q.word(1usize << 63).unwrap(),
        q.word(usize::MAX).unwrap(),
    ];
    let failure = |status, detail| {
        let e = unsafe {
            host.id.send(
                named(b"Execution"),
                host.intern(b"failNative:detail:").unwrap(),
                &[size(status).unwrap(), size(detail).unwrap()],
            )
        }
        .unwrap_err();
        q.keep(e).unwrap()
    };
    let reporter = q.call(named(b"Object"), b"clone", &[]).unwrap();
    let trace = named(b"Trace");
    let nil = named(b"nil");
    let state = q
        .call(
            host.id.root,
            b"sequence",
            &[
                gc,
                trace,
                named(b"Word"),
                named(b"String"),
                named(b"Bytes"),
                q.call(host.id.root, b"sequence", &nodes).unwrap(),
                q.call(host.id.root, b"sequence", &ids).unwrap(),
                reporter,
                std::ptr::null_mut(),
                size(0).unwrap(),
                std::ptr::null_mut(),
                failure(921, 73),
                failure(923, 75),
                failure(924, 76),
                nil,
                host.intern(b"originalEnd:").unwrap(),
                size(1).unwrap(),
                host.roots,
            ],
        )
        .unwrap();
    let probe = q.call(named(b"Object"), b"clone", &[]).unwrap();
    let report_selector = host.intern(b"report:acceptingGaps:").unwrap();
    let h = H {
        host: &host,
        q,
        fixture,
        state,
        probe,
        trace,
        reporter,
        report_selector,
    };
    let pvt = h.call(host.id.root, b"ensurePrivateVtableFor:", &[probe]);
    for n in [b"configure".as_slice(), b"stats", b"row:field:"] {
        h.bind(pvt, n, h.method(n));
    }
    let rvt = h.call(host.id.root, b"ensurePrivateVtableFor:", &[reporter]);
    for n in [
        b"reached:identity:".as_slice(),
        b"notReached:identity:",
        b"complete:",
        b"echoBytes:",
    ] {
        let m = h.method(n);
        h.bind(rvt, n, m);
        if n == b"complete:" {
            h.put(10, m);
        }
    }
    let storage = named(b"Storage");
    let svt = h.vt(storage);
    let original_observe = h.call(svt, b"lookup", &[h.key(b"observe:")]);
    h.bind(svt, b"observe:", h.method(b"observe:"));
    let started = std::time::Instant::now();
    for edges in 0..512usize {
        for roots in 0..8usize {
            h.config(edges, roots, 0, 0, 0, 0);
            let mut closure = roots;
            loop {
                let before = closure;
                for i in 0..3 {
                    for j in 0..3 {
                        if closure & (1 << i) != 0 && edges & (1 << (i * 3 + j)) != 0 {
                            closure |= 1 << j;
                        }
                    }
                }
                if closure == before {
                    break;
                }
            }
            let local = host.scope();
            let answer = h.report(size(1).unwrap()).unwrap();
            assert_eq!(
                local.word_bits(answer).unwrap(),
                usize::MAX,
                "edges={edges} roots={roots}"
            );
            assert_eq!(
                h.rows(),
                vec![
                    (0, 1, closure & 1 != 0),
                    (1, 1usize << 63, closure & 2 != 0),
                    (2, usize::MAX, closure & 4 != 0)
                ]
            );
            if roots == 7 {
                h.raw(gc, b"collectPrecisely", &[]);
            }
        }
        if edges % 64 == 63 {
            eprintln!("Trace graph oracle: {} / 4096 cases", (edges + 1) * 8);
        }
    }
    eprintln!(
        "Trace graph oracle: 4096 cases passed in {:?}",
        started.elapsed()
    );
    for anomaly in 1..=5 {
        h.config(0, 0, anomaly, 0, 0, 0);
        assert_eq!(h.error(h.report(size(1).unwrap()).unwrap_err()).0, 1);
        assert_eq!(h.stat(0), 0);
    }
    h.config(0, 0, 6, 0, 0, 0);
    h.report(size(1).unwrap()).unwrap();
    assert_eq!(
        h.rows().iter().map(|r| r.2).collect::<Vec<_>>(),
        vec![true, true, false]
    );
    assert!(host.send(h.field(8), b"root:", &[nodes[0]]).is_err());
    for mode in [1, 2, 3] {
        h.config(0, 0, 0, mode, if mode == 3 { 5 } else { 0 }, 0);
        let e = h.error(h.report(size(1).unwrap()).unwrap_err());
        if mode == 3 {
            assert_eq!(e, (921, 73));
        }
        h.config(0, 0, 0, 0, 0, 0);
        h.report(size(1).unwrap()).unwrap();
    }
    h.put(16, ids[1]);
    h.config(0, 0, 0, 0, 0, 0);
    assert_eq!(h.error(h.report(size(1).unwrap()).unwrap_err()), (1, 3));
    assert_eq!(h.stat(0), 0);
    h.report(ids[2]).unwrap();
    h.put(16, size(1).unwrap());
    let tvt = h.vt(trace);
    let original_end = h.call(tvt, b"lookup", &[h.key(b"end:")]);
    h.bind(tvt, b"originalEnd:", original_end);
    h.bind(tvt, b"end:", h.method(b"cleanup"));
    for mode in [0, 1, 2, 3, 4, 6, 7, 8, 9, 10] {
        h.config(0, 0, 0, 0, mode, 1);
        let result = h.report(size(1).unwrap());
        let local = host.scope();
        match mode {
            0 => assert_eq!(local.word_bits(result.unwrap()).unwrap(), usize::MAX),
            1 | 8 => assert!(result.unwrap().is_null()),
            2 | 3 => {
                let result = local.keep(result.unwrap()).unwrap();
                assert_eq!(
                    as_size(local.call(host.id.root, b"identityOf:", &[result]).unwrap()),
                    Some(h.stat(3))
                );
                if mode == 2 {
                    assert_eq!(local.string_bytes(result).unwrap(), b"fresh completion");
                } else {
                    assert_eq!(
                        local.read_bytes(result).unwrap(),
                        b"native completion bytes"
                    );
                }
            }
            4 => assert_eq!(h.error(result.unwrap_err()), (923, 75)),
            6 => assert_eq!(result.unwrap(), h.field(10)),
            7 => assert_eq!(result.unwrap(), host.id.root),
            9 => assert_eq!(result.unwrap(), nil),
            10 => assert_eq!(result.unwrap(), h.field(12)),
            _ => unreachable!(),
        }
        assert_eq!(
            h.stat(2),
            b'!' as usize,
            "cleanup must overwrite the native source bytes"
        );
    }
    h.config(0, 0, 0, 0, 0, 2);
    assert_eq!(h.error(h.report(size(1).unwrap()).unwrap_err()), (924, 76));
    h.config(0, 0, 0, 0, 4, 2);
    assert_eq!(h.error(h.report(size(1).unwrap()).unwrap_err()), (923, 75));
    h.config(0, 0, 7, 0, 0, 1);
    h.report(size(1).unwrap()).unwrap();
    assert_eq!(h.stat(0), 0);
    assert_eq!(h.stat(1), 1);
    h.bind(tvt, b"end:", original_end);
    h.call(tvt, b"removeMethod:", &[h.key(b"originalEnd:")]);
    h.bind(svt, b"observe:", original_observe);
    h.call(
        host.namespace,
        b"at:put:",
        &[h.key(b"TraceEchoTarget"), reporter],
    );
    let source=h.q.string(br#"
        let traceRequestFixture := [Object clone];
        [Reflect defineMethod: #value on: traceRequestFixture as: [Execution block: [:self | self] withNativeSignature: "m"]];
        [Reflect defineMethod: #nativeReceiver on: traceRequestFixture as: [:self | TraceEchoTarget]];
        [Reflect defineMethod: #nativeSelector on: traceRequestFixture as: [:self | #echoBytes:]];
        [Reflect defineMethod: #nativeArguments on: traceRequestFixture as: [:self | #[["borrowed argument survives" asBytes]]]];
        [Reflect defineMethod: #nativeSignature on: traceRequestFixture as: [:self | "vb"]];
        [Reflect defineMethod: #nativeCleanup on: traceRequestFixture as: [:self | [| [System gc]]]];
        traceRequestFixture
    "#).unwrap();
    let request = h.call(named(b"Codegen"), b"eval:", &[source]);
    h.q.check_result(request).unwrap();
    let result = h.call(request, b"value", &[]);
    assert_eq!(
        h.q.read_bytes(result).unwrap(),
        b"borrowed argument survives"
    );
    assert_eq!(
        as_size(h.call(host.id.root, b"identityOf:", &[result])),
        Some(h.stat(3))
    );
    h.call(
        host.namespace,
        b"at:put:",
        &[h.key(b"TraceEchoTarget"), nil],
    );
}
