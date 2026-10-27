//! Bootstrap loop semantics and replacement by the existing authored controls.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use birnam_host_support::{Context, Scope};
use std::path::Path;
struct H<'a> {
    host: &'a Context,
    q: Scope<'a>,
    fixture: Oop,
    gc: Oop,
    state: Oop,
    probe: Oop,
}
impl H<'_> {
    fn named(&self, n: &[u8]) -> Oop {
        self.host.named(n).unwrap()
    }
    fn key(&self, n: &[u8]) -> Oop {
        self.q.keep(self.host.intern(n).unwrap()).unwrap()
    }
    fn call(&self, r: Oop, n: &[u8], a: &[Oop]) -> Oop {
        self.q.call(r, n, a).unwrap()
    }
    fn string(&self, b: &[u8]) -> Oop {
        self.q.string(b).unwrap()
    }
    fn eval(&self, b: &[u8]) -> Oop {
        self.call(self.named(b"Codegen"), b"eval:", &[self.string(b)])
    }
    fn number(&self, v: Oop) -> usize {
        self.q
            .word_bits(self.call(self.named(b"Integer"), b"wordOf:", &[v]))
            .unwrap()
    }
    fn bind(&self, n: &[u8], v: Oop) {
        self.call(self.host.namespace, b"at:put:", &[self.key(n), v]);
    }
    fn vt(&self, v: Oop) -> Oop {
        self.call(self.host.id.root, b"behaviorOf:", &[v])
    }
    fn private(&self, v: Oop) -> Oop {
        self.call(self.host.id.root, b"ensurePrivateVtableFor:", &[v])
    }
    fn object(&self) -> Oop {
        self.call(self.named(b"Object"), b"clone", &[])
    }
    fn context(&self, a: &[Oop]) -> Oop {
        self.call(self.host.id.root, b"sequence", a)
    }
    fn native(&self, n: &[u8], c: Oop) -> Oop {
        self.call(
            self.call(self.fixture, b"methodAt:", &[self.key(n)]),
            b"withContext:",
            &[c],
        )
    }
    fn method(&self, vt: Oop, n: &[u8]) -> Oop {
        self.call(vt, b"lookup", &[self.key(n)])
    }
    fn set(&self, vt: Oop, n: &[u8], m: Oop) {
        self.call(vt, b"addMethod", &[self.key(n), m]);
    }
    fn field(&self, c: Oop, i: usize) -> Oop {
        self.call(c, b"at:", &[size(i).unwrap()])
    }
    fn put(&self, c: Oop, i: usize, v: Oop) {
        self.call(c, b"at:put:", &[size(i).unwrap(), v]);
    }
    fn word(&self, i: usize, v: usize) {
        self.put(self.state, i, size(v).unwrap());
    }
    fn count(&self, c: Oop, i: usize) -> usize {
        as_size(self.field(c, i)).unwrap()
    }
    fn error(&self, n: &[u8]) -> Oop {
        self.call(self.named(b"Error"), b"kind:", &[self.key(n)])
    }
    fn collect(&self) {
        self.call(self.gc, b"collectPrecisely", &[]);
    }
    fn identity(&self, v: Oop) -> Oop {
        self.call(self.host.id.root, b"identityOf:", &[v])
    }
    fn present(&self, v: Oop) -> bool {
        as_size(self.call(self.probe, b"present:", &[v])).unwrap() == 1
    }
    fn override_value(&self, condition: bool, value: Option<Oop>) {
        let i = if condition { 8 } else { 10 };
        self.word(i, usize::from(value.is_some()));
        self.put(self.state, i + 1, value.unwrap_or(self.named(b"nil")));
    }
}
#[test]
#[ignore = "requires production Oop modules"]
fn bootstrap_loops_use_live_sends_and_preserve_control_boundaries() {
    let host = oop::bootstrap();
    let q = host.scope();
    let gc = host.named(b"GC").unwrap();
    q.call(gc, b"precisePayloads:", &[size(1).unwrap()])
        .unwrap();
    let path = std::env::temp_dir().join(format!("birnam-block-loop-{}.dylib", std::process::id()));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/block-loop.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = host.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let named = |n: &[u8]| host.named(n).unwrap();
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
    let probe = q.call(named(b"Object"), b"clone", &[]).unwrap();
    let zero = size(0).unwrap();
    let state = q
        .call(
            host.id.root,
            b"sequence",
            &[
                gc,
                named(b"true"),
                named(b"false"),
                named(b"nil"),
                zero,
                zero,
                size(10000).unwrap(),
                zero,
                zero,
                named(b"nil"),
                zero,
                named(b"nil"),
                zero,
                zero,
                failure(881, 17),
                failure(882, 19),
                named(b"Object"),
                zero,
                probe,
                host.roots,
            ],
        )
        .unwrap();
    let h = H {
        host: &host,
        q,
        fixture,
        gc,
        state,
        probe,
    };
    let pvt = h.private(probe);
    h.set(pvt, b"present:", h.native(b"present:", state));
    h.set(
        pvt,
        b"allocation:bytes:identity:",
        h.native(b"census", state),
    );
    let bvt = h.vt(h.named(b"Block"));
    assert_eq!(h.call(host.id.root, b"ownerOf:", &[bvt]), h.named(b"Block"));
    let while_true = h.method(bvt, b"whileTrue:");
    let while_false = h.method(bvt, b"whileFalse:");
    let cond = h.object();
    let action = h.object();
    let cvt = h.private(cond);
    let avt = h.private(action);
    h.set(cvt, b"value", h.native(b"condition", state));
    h.set(avt, b"value", h.native(b"body", state));
    h.set(cvt, b"loop:", while_true);
    let nil = h.named(b"nil");
    let truth = h.named(b"true");
    assert_eq!(h.call(cond, b"loop:", &[action]), nil);
    assert_eq!((h.count(state, 5), h.count(state, 4)), (10001, 10000));
    assert_eq!(h.call(cond, b"loop:", &[]), nil);
    assert_eq!(h.call(cond, b"loop:", &[action, action]), nil);
    assert_eq!(h.call(cond, b"loop:", &[nil]), nil);
    h.word(7, 1);
    h.word(4, 0);
    h.word(5, 0);
    h.word(6, 3);
    h.set(cvt, b"loop:", while_false);
    assert_eq!(h.call(cond, b"loop:", &[action]), nil);
    assert_eq!((h.count(state, 5), h.count(state, 4)), (4, 3));
    h.word(7, 0);
    h.set(cvt, b"loop:", while_true);
    h.word(13, 1);
    let error = h.error(b"testLoop");
    h.override_value(true, Some(error));
    h.word(4, 0);
    assert_eq!(h.call(cond, b"loop:", &[action]), error);
    assert_eq!(h.count(state, 4), 0);
    h.override_value(true, Some(truth));
    h.override_value(false, Some(error));
    assert_eq!(h.call(cond, b"loop:", &[action]), error);
    assert_eq!(h.count(state, 4), 1);
    h.override_value(true, Some(nil));
    let error = h.call(cond, b"loop:", &[action]);
    assert_eq!(h.call(error, b"kind", &[]), h.key(b"mustBeBoolean"));
    assert_eq!(
        h.q.string_bytes(h.call(error, b"message", &[])).unwrap(),
        b"while loop condition must answer true or false"
    );
    h.override_value(true, Some(truth));
    h.override_value(false, None);
    for which in [1, 2] {
        h.word(12, which);
        let failure = unsafe { host.id.send(cond, h.key(b"loop:"), &[action]) }.unwrap_err();
        assert_eq!(failure, h.field(state, 13 + which));
    }
    h.bind(b"LoopFailureCondition", cond);
    h.word(12, 1);
    let ordinary = h.eval(b"[LoopFailureCondition value]");
    assert_eq!(h.call(ordinary, b"kind", &[]), h.key(b"nativeFailure"));
    h.bind(b"LoopFailureCondition", nil);
    h.word(12, 0);
    h.override_value(true, None);
    h.word(4, 0);
    h.word(6, 3);
    assert_eq!(h.call(cond, b"loop:", &[action]), nil);
    let temporary = host.scope();
    let transient_cond = temporary.call(cvt, b"allocate", &[zero]).unwrap();
    let transient_body = temporary.call(avt, b"allocate", &[zero]).unwrap();
    let ids = [transient_cond, transient_body].map(|v| h.identity(v));
    let selector = h.key(b"loop:");
    h.word(4, 0);
    drop(temporary);
    assert_eq!(
        unsafe { host.id.send(transient_cond, selector, &[transient_body]) }.unwrap(),
        nil
    );
    h.collect();
    for id in ids {
        assert!(!h.present(id));
    }
    transient_results(&h, cond, action, avt);
    compiled(&h);
    replacements(&h, cond, action, cvt, avt);
    control(&h);
}
fn compiled(h: &H<'_>) {
    let target = h.object();
    let vt = h.private(target);
    let reply = h.native(
        b"reply",
        h.context(&[h.gc, std::ptr::null_mut(), size(0).unwrap()]),
    );
    h.set(vt, b"value", reply);
    h.bind(b"LoopAbsent", target);
    assert!(
        unsafe { h.host.id.send(target, h.key(b"value"), &[]) }
            .unwrap()
            .is_null()
    );
    assert!(
        unsafe { h.host.id.send(reply, h.key(b"invokeOn:"), &[target]) }
            .unwrap()
            .is_null()
    );
    assert_eq!(h.eval(b"[LoopAbsent value]"), h.named(b"nil"));
    assert_eq!(
        h.eval(b"[[| let result := [LoopAbsent value]; [result == nil]] value]"),
        h.named(b"true")
    );
    h.bind(b"LoopAbsent", h.named(b"nil"));
    let value=h.eval(b"let n := [Cell with: 0]; [n set: 0]; [[| [[n value] < 100]] whileTrue: [| [n set: [[n value] + 1]]]]; [n value]");
    assert_eq!(h.number(value), 100);
    let value=h.eval(b"let m := [Cell with: 0]; [m set: 0]; [[| [[m value] >= 7]] whileFalse: [| [System gc]; [m set: [[m value] + 1]]]]; [m value]");
    assert_eq!(h.number(value), 7);
    for (source, kind) in [
        (
            b"[[| 42] whileTrue: [| nil]]".as_slice(),
            b"mustBeBoolean".as_slice(),
        ),
        (
            b"[[| true] whileTrue: [| [Error kind: #bodyError]]]",
            b"bodyError",
        ),
    ] {
        let e = h.eval(source);
        assert_eq!(h.call(e, b"kind", &[]), h.key(kind));
    }
}
fn transient_results(h: &H<'_>, cond: Oop, action: Oop, avt: Oop) {
    let original = h.method(avt, b"value");
    h.set(avt, b"value", h.native(b"fresh", h.state));
    let vt = h.vt(h.host.namespace);
    let prior = h.method(vt, b"valueAt:");
    let alias = h.key(b"loopOriginalValueAt:");
    h.set(vt, b"loopOriginalValueAt:", prior);
    let state = h.context(&[h.gc, alias, h.key(b"ErrorVt"), size(0).unwrap()]);
    h.set(vt, b"valueAt:", h.native(b"namespaceAt", state));
    h.word(4, 0);
    h.word(6, 3);
    assert_eq!(h.call(cond, b"loop:", &[action]), h.named(b"nil"));
    assert_eq!(h.count(h.state, 4), 3);
    assert!(h.count(state, 3) > 0);
    let previous = h.field(h.state, 17);
    h.collect();
    assert!(!h.present(previous));
    h.set(vt, b"valueAt:", prior);
    h.call(vt, b"removeMethod:", &[alias]);
    h.set(avt, b"value", original);
}
fn replacements(h: &H<'_>, cond: Oop, action: Oop, cvt: Oop, avt: Oop) {
    let original_condition = h.method(cvt, b"value");
    let original_body = h.method(avt, b"value");
    let nil = h.named(b"nil");
    let truth = h.named(b"true");
    let stop = h.context(&[h.gc, h.named(b"false"), size(0).unwrap()]);
    let mutation = h.context(&[h.gc, cvt, h.native(b"reply", stop), nil, size(0).unwrap()]);
    h.set(avt, b"value", h.native(b"mutate", mutation));
    h.word(4, 0);
    h.word(6, 1);
    assert_eq!(h.call(cond, b"loop:", &[action]), nil);
    assert_eq!((h.count(mutation, 4), h.count(stop, 2)), (1, 1));
    h.set(cvt, b"value", original_condition);
    h.set(avt, b"value", original_body);
    let error = h.error(b"replacedBody");
    let answer = h.context(&[h.gc, error, size(0).unwrap()]);
    let mutation = h.context(&[
        h.gc,
        avt,
        h.native(b"reply", answer),
        truth,
        size(0).unwrap(),
    ]);
    h.set(cvt, b"value", h.native(b"mutate", mutation));
    assert_eq!(h.call(cond, b"loop:", &[action]), error);
    assert_eq!((h.count(mutation, 4), h.count(answer, 2)), (1, 1));
    h.set(cvt, b"value", original_condition);
    h.set(avt, b"value", original_body);
    let spoof = h.object();
    let spoof_vt = h.private(spoof);
    let checked = h.context(&[h.gc, truth, size(0).unwrap()]);
    h.set(spoof_vt, b"isError", h.native(b"reply", checked));
    h.override_value(true, Some(spoof));
    let result = h.call(cond, b"loop:", &[action]);
    assert_eq!(h.call(result, b"kind", &[]), h.key(b"mustBeBoolean"));
    h.override_value(true, None);
    h.override_value(false, Some(spoof));
    h.word(4, 0);
    assert_eq!(h.call(cond, b"loop:", &[action]), nil);
    assert_eq!(h.count(checked, 2), 0);
    h.override_value(false, None);
    h.override_value(true, Some(nil));
    let factory = h.named(b"Error");
    h.bind(b"Error", nil);
    assert_eq!(h.call(cond, b"loop:", &[action]), nil);
    h.bind(b"Error", factory);
    let fvt = h.vt(factory);
    let original = h.method(fvt, b"kind:message:");
    let value = h.context(&[h.gc, error, size(0).unwrap()]);
    h.set(fvt, b"kind:message:", h.native(b"reply", value));
    assert_eq!(h.call(cond, b"loop:", &[action]), error);
    h.put(value, 1, std::ptr::null_mut());
    assert_eq!(h.call(cond, b"loop:", &[action]), nil);
    h.set(fvt, b"kind:message:", original);
    h.override_value(true, None);
}
fn control(h: &H<'_>) {
    let machine = h.call(h.named(b"Machine"), b"new", &[]);
    h.bind(b"LoopMachine", machine);
    let blocked = h.eval(b"[| [[| true] whileTrue: [| [LoopMachine park]]]]");
    let failure = unsafe { h.host.id.send(machine, h.key(b"runBlock:"), &[blocked]) }.unwrap_err();
    assert_eq!(
        h.q.symbol_text(h.call(failure, b"faultKind", &[])).unwrap(),
        "invalidRequest"
    );
    assert_eq!(h.call(machine, b"executionState", &[]), h.key(b"failed"));
    h.call(machine, b"destroy", &[]);
    let old_condition = h.eval(b"[| [LoopMachine park]; false]");
    h.bind(b"OldLoopCondition", old_condition);
    let authored = include_str!("../../../modules/stackless-control.birnam");
    let start = authored.find("// Boolean is").unwrap();
    let end = authored.find("// Integer iteration").unwrap();
    let result = h.eval(authored[start..end].as_bytes());
    h.q.check_result(result).unwrap();
    let vt = h.vt(h.named(b"Block"));
    let installed = h.method(vt, b"whileTrue:");
    h.eval(b"[Module load: #blockLoop]");
    assert_eq!(h.method(vt, b"whileTrue:"), installed);
    let machine = h.call(h.named(b"Machine"), b"new", &[]);
    h.bind(b"LoopMachine", machine);
    let caller = h.eval(b"[| [OldLoopCondition whileTrue: [| nil]]]");
    assert_eq!(h.call(machine, b"runBlock:", &[caller]), machine);
    h.collect();
    assert_eq!(
        h.call(machine, b"resumeWith:", &[h.named(b"nil")]),
        h.named(b"nil")
    );
    let machine = h.call(h.named(b"Machine"), b"new", &[]);
    h.bind(b"LoopMachine", machine);
    let block=h.eval(b"[| let state := [Cell with: 0]; [state set: 0]; [[| [[state value] < 2]] whileTrue: [| [LoopMachine park]; [state set: [[state value] + 1]]]]; [state value]]");
    assert_eq!(h.call(machine, b"runBlock:", &[block]), machine);
    h.collect();
    assert_eq!(h.call(machine, b"resumeWith:", &[h.named(b"nil")]), machine);
    h.collect();
    assert_eq!(
        h.number(h.call(machine, b"resumeWith:", &[h.named(b"nil")])),
        2
    );
}
