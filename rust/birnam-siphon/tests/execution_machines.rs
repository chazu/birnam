//! Production Machine scheduling, control and lifetime through Oop messages.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use birnam_artifact_codec::stackless_selection::{
    self as selection, Disposition, ImplementationKind, RootKind, SelectionSpec,
};
use birnam_host_support::{Context, Scope};
use std::{cell::RefCell, path::Path, ptr};

struct H<'a> {
    host: &'a Context,
    _floor: Scope<'a>,
    fixture: Oop,
    monitor: Oop,
    gc: Oop,
    retain: Oop,
    release: Oop,
    tokens: RefCell<Vec<Oop>>,
}
impl<'a> H<'a> {
    fn new(host: &'a Context) -> Self {
        let q = host.scope();
        let gc = host.named(b"GC").unwrap();
        q.call(gc, b"precisePayloads:", &[size(1).unwrap()])
            .unwrap();
        let path = std::env::temp_dir().join(format!(
            "birnam-execution-machines-{}.dylib",
            std::process::id()
        ));
        assert!(
            std::process::Command::new("clang")
                .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
                .arg(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../tests/fixtures/oop-host/execution-machines.c")
                )
                .arg("-o")
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let fixture = host.load_native(&q, &path, &[]).unwrap();
        std::fs::remove_file(path).unwrap();
        let monitor = q
            .call(host.named(b"Object").unwrap(), b"clone", &[])
            .unwrap();
        let vt = q
            .call(host.id.root, b"ensurePrivateVtableFor:", &[monitor])
            .unwrap();
        let retain = q.keep(host.intern(b"retain:").unwrap()).unwrap();
        let release = q.keep(host.intern(b"release:").unwrap()).unwrap();
        let h = Self {
            host,
            _floor: q,
            fixture,
            monitor,
            gc,
            retain,
            release,
            tokens: RefCell::new(vec![]),
        };
        for n in [b"present".as_slice(), b"allocation:bytes:identity:"] {
            h.bind(vt, n, h.native(n, ptr::null_mut()));
        }
        h
    }
    fn named(&self, n: &[u8]) -> Oop {
        self.host.named(n).unwrap()
    }
    fn keep(&self, v: Oop) -> Oop {
        self.tokens
            .borrow_mut()
            .push(unsafe { self.host.id.send(self.host.roots, self.retain, &[v]) }.unwrap());
        v
    }
    fn key(&self, n: &[u8]) -> Oop {
        self.keep(self.host.intern(n).unwrap())
    }
    fn call(&self, r: Oop, n: &[u8], a: &[Oop]) -> Oop {
        self.keep(self.host.send(r, n, a).unwrap())
    }
    fn context(&self, a: &[Oop]) -> Oop {
        self.call(self.host.id.root, b"sequence", a)
    }
    fn native(&self, n: &[u8], c: Oop) -> Oop {
        let method = self.call(self.fixture, b"methodAt:", &[self.key(n)]);
        self.call(method, b"withContext:", &[c])
    }
    fn object(&self) -> Oop {
        self.call(self.named(b"Object"), b"clone", &[])
    }
    fn vt(&self, v: Oop) -> Oop {
        self.call(self.host.id.root, b"behaviorOf:", &[v])
    }
    fn private_vt(&self, v: Oop) -> Oop {
        self.call(self.host.id.root, b"ensurePrivateVtableFor:", &[v])
    }
    fn bind(&self, vt: Oop, n: &[u8], m: Oop) {
        self.call(vt, b"addMethod", &[self.key(n), m]);
    }
    fn remove(&self, vt: Oop, n: &[u8]) {
        self.call(vt, b"removeMethod:", &[self.key(n)]);
    }
    fn put(&self, c: Oop, i: usize, v: Oop) {
        self.call(c, b"at:put:", &[size(i).unwrap(), v]);
    }
    fn count(&self, c: Oop, i: usize) -> usize {
        as_size(self.call(c, b"at:", &[size(i).unwrap()])).unwrap()
    }
    fn integer(&self, i: usize) -> Oop {
        self.call(self.named(b"Integer"), b"fromWord:", &[size(i).unwrap()])
    }
    fn number(&self, v: Oop) -> usize {
        self.host
            .scope()
            .word_bits(self.call(self.named(b"Integer"), b"wordOf:", &[v]))
            .unwrap()
    }
    fn vector(&self, values: &[Oop]) -> Oop {
        let mut v = self.call(self.named(b"Vector"), b"new", &[]);
        for &value in values {
            v = self.call(v, b"add:", &[value]);
        }
        v
    }
    fn method(&self, fixture: &str, mode: usize, owner: Oop) -> Oop {
        let bytes = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../tests/fixtures/stackless-s2/frame-descriptor-{fixture}.bin"
        )))
        .unwrap();
        let directory = 64 + 8 * 32;
        let offset =
            u64::from_le_bytes(bytes[directory + 16..directory + 24].try_into().unwrap()) as usize;
        let revision = bytes[offset..offset + 32].try_into().unwrap();
        let spec = SelectionSpec {
            source_revision: revision,
            root_kind: RootKind::MethodOperation,
            root_index: 0,
            path: if fixture == "source-map-block" {
                &[2, 1]
            } else {
                &[]
            },
            implementation_kind: if fixture == "source-map-block" {
                ImplementationKind::BlockBody
            } else {
                ImplementationKind::NamedAdapter
            },
            disposition: Disposition::ResumableRequired,
        };
        let selected = selection::encode_artifact_selection(&[revision], &[spec]).unwrap();
        let context = self.context(&[self.host.namespace, size(mode).unwrap()]);
        let step = self.native(b"step", context);
        let q = self.host.scope();
        let description = q.string(&bytes).unwrap();
        let selected = q.string(&selected).unwrap();
        self.call(
            self.named(b"Execution"),
            b"fromStep:descriptor:selection:owner:",
            &[step, description, selected, owner],
        )
    }
    fn machine(&self) -> Oop {
        self.call(self.named(b"Machine"), b"new", &[])
    }
    fn block(&self, method: Oop, captures: &[Oop]) -> Oop {
        self.call(
            self.named(b"Block"),
            b"fromMethod:captures:",
            &[method, self.vector(captures)],
        )
    }
    fn invoke(&self, method: Oop, receiver: Oop) -> Oop {
        self.call(method, b"invokeOn:", &[receiver])
    }
    fn collect(&self) {
        self.call(self.gc, b"collectPrecisely", &[]);
    }
    fn identity(&self, v: Oop) -> Oop {
        self.call(self.host.id.root, b"identityOf:", &[v])
    }
    fn present(&self, id: Oop) -> bool {
        as_size(self.call(self.monitor, b"present", &[id])).unwrap() == 1
    }
    fn clear(&self) {
        for token in self.tokens.take().into_iter().rev() {
            unsafe { self.host.id.send(self.host.roots, self.release, &[token]) }.unwrap();
        }
    }
}
impl Drop for H<'_> {
    fn drop(&mut self) {
        self.clear();
    }
}

fn control_replacements(h: &H<'_>) {
    let machine = h.machine();
    let vt = h.vt(machine);
    for name in [b"park".as_slice(), b"protectedBlock:"] {
        let key = h.key(name);
        let original = h.call(vt, b"lookup", &[key]);
        let captured = h.call(machine, b"methodFor:", &[key]);
        assert_ne!(captured, h.named(b"nil"));
        let replacement = h.native(b"self", ptr::null_mut());
        h.bind(vt, name, replacement);
        for _ in 0..2 {
            let q = h.host.scope();
            h.host
                .load_native(
                    &q,
                    &h.host.resources.lib_dir.join("libbirnam_execution.dylib"),
                    &[h.host.namespace],
                )
                .unwrap();
            let other = h.machine();
            h.collect();
            assert_eq!(h.call(vt, b"lookup", &[key]), replacement);
            let args = if name == b"park" {
                vec![]
            } else {
                vec![h.named(b"nil")]
            };
            assert_eq!(h.call(other, name, &args), other);
            assert_eq!(h.number(h.call(other, b"liveFrameCount", &[])), 0);
        }
        h.bind(vt, name, original);
    }
}
#[test]
#[ignore = "requires production Oop modules"]
fn production_machine_calls_blocks_budgets_waits_and_lifetimes() {
    let host = oop::bootstrap();
    let h = H::new(&host);
    control_replacements(&h);
    let nil = h.named(b"nil");
    let owner = h.object();
    let simple = h.method("minimal-return", 1, owner);
    assert_eq!(h.invoke(simple, nil), owner);
    let capture = h.object();
    let bm = h.method("source-map-block", 6, owner);
    let block = h.block(bm, &[capture]);
    assert_eq!(h.call(block, b"value", &[]), capture);
    let machine = h.machine();
    assert_eq!(h.call(machine, b"runBlock:", &[block]), capture);
    assert_eq!(h.number(h.call(machine, b"liveFrameCount", &[])), 0);

    // Collection during a native call preserves copied operands; later calls
    // observe replacement, including tail calls.
    let receiver = h.object();
    let vt = h.private_vt(receiver);
    let selector = h.key(b"machineAnswer:");
    let argument = h.object();
    let callback = h.context(&[h.gc, size(0).unwrap()]);
    h.bind(vt, b"machineAnswer:", h.native(b"callback", callback));
    let operands = h.vector(&[receiver, selector, argument]);
    let caller = h.method("call", 4, operands);
    assert_eq!(h.invoke(caller, nil), argument);
    assert_eq!(h.count(callback, 1), 1);
    assert_eq!(
        h.call(
            simple,
            b"invokeOn:arguments:",
            &[receiver, h.vector(&[argument])]
        ),
        owner
    );
    h.bind(vt, b"machineAnswer:", simple);
    assert_eq!(h.invoke(caller, nil), owner);
    assert_eq!(h.invoke(h.method("tail-call", 7, operands), nil), owner);
    let candidate_calls = h.context(&[h.gc, size(0).unwrap()]);
    let candidate = h.call(
        simple,
        b"withInline:at:owner:",
        &[h.native(b"callback", candidate_calls), h.integer(1), owner],
    );
    h.bind(vt, b"machineAnswer:", candidate);
    assert_eq!(h.invoke(caller, nil), owner);
    assert_eq!(
        h.count(candidate_calls, 1),
        0,
        "mismatched arity uses ordinary binding before entering a candidate"
    );
    // Missing ordinary Method arguments bind nil on direct and pending calls.
    let q = host.scope();
    let code = q.string(b"[:a :b | b]").unwrap();
    let body = h.call(h.named(b"Codegen"), b"eval:", &[code]);
    q.check_result(body).unwrap();
    let missing = h.call(body, b"methodFor:", &[h.key(b"value:value:")]);
    assert_eq!(
        h.call(
            missing,
            b"invokeOn:arguments:",
            &[body, h.vector(&[argument])]
        ),
        nil
    );
    h.bind(vt, b"machineAnswer:", missing);
    assert_eq!(h.invoke(caller, nil), nil);
    assert_eq!(h.invoke(h.method("tail-call", 7, operands), nil), nil);
    h.bind(vt, b"machineAnswer:", simple);
    drop(q);

    let branch = h.method("branch-join", 5, owner);
    let branch_block = h.block(branch, &[]);
    let a = h.machine();
    let b = h.machine();
    for m in [a, b] {
        assert_eq!(
            h.call(m, b"runBlock:budget:", &[branch_block, h.integer(2)]),
            m
        );
    }
    let view = h.call(a, b"currentFrameView", &[]);
    assert_eq!(h.number(h.call(view, b"programCounter", &[])), 1);
    h.collect();
    assert_eq!(h.call(a, b"continueWithBudget:", &[h.integer(1)]), a);
    assert_eq!(h.number(h.call(view, b"programCounter", &[])), 3);
    assert_eq!(h.call(b, b"continueWithBudget:", &[h.integer(20)]), owner);
    assert_eq!(h.call(a, b"continueWithBudget:", &[h.integer(20)]), owner);
    let stale = h.call(view, b"programCounter", &[]);
    assert_eq!(h.call(stale, b"kind", &[]), h.key(b"staleFrameView"));
    assert_eq!(h.call(a, b"currentFrameView", &[]), nil);

    let wait = h.method("call", 2, owner);
    let wait_block = h.block(wait, &[]);
    let waiting = h.machine();
    assert_eq!(h.call(waiting, b"runBlock:", &[wait_block]), waiting);
    assert!(
        host.send(waiting, b"continueWithBudget:", &[h.integer(1)])
            .is_err()
    );
    h.collect();
    assert_eq!(h.call(waiting, b"resumeWith:", &[argument]), argument);
    assert!(host.send(waiting, b"resumeWith:", &[argument]).is_err());

    let protected = h.machine();
    let protected_owner = h.vector(&[protected, h.key(b"protectedBlock:"), branch_block]);
    let protected_method = h.method("call", 4, protected_owner);
    let protected_block = h.block(protected_method, &[]);
    assert_eq!(
        h.call(
            protected,
            b"runBlock:budget:",
            &[protected_block, h.integer(3)]
        ),
        protected
    );
    h.call(protected, b"exitRunnableBoundary:", &[argument]);
    assert_eq!(
        h.call(protected, b"boundaryDeliveryPending", &[]),
        h.named(b"true")
    );
    assert_eq!(
        h.call(protected, b"continueWithBudget:", &[h.integer(20)]),
        argument
    );

    let native = h.machine();
    let mvt = h.vt(native);
    let boundary = h.context(&[h.gc, size(0).unwrap(), size(0).unwrap()]);
    h.bind(mvt, b"value", h.native(b"boundary", boundary));
    assert_eq!(h.call(native, b"runBlock:", &[native]), h.named(b"false"));
    h.put(boundary, 1, size(1).unwrap());
    let blocked = h.machine();
    assert!(host.send(blocked, b"runBlock:", &[blocked]).is_err());
    assert_eq!(h.count(boundary, 2), 1);
    h.call(blocked, b"destroy", &[]);
    h.remove(mvt, b"value");

    let tracer = h.object();
    let tvt = h.private_vt(tracer);
    let during = h.context(&[h.named(b"Machine"), size(0).unwrap()]);
    h.bind(tvt, b"rootsDo:", h.native(b"rootsDo:", during));
    h.call(host.roots, b"register:", &[tracer]);
    h.collect();
    assert_eq!(h.count(during, 1), 1);
    h.call(host.roots, b"unregister:", &[tracer]);

    let capturer = h.method("call", 3, owner);
    let capturing_block = h.block(capturer, &[]);
    let source = h.machine();
    let continuation = h.call(source, b"runBlock:", &[capturing_block]);
    h.collect();
    let destination = h.machine();
    assert!(
        host.send(
            destination,
            b"restoreContinuation:with:",
            &[continuation, argument]
        )
        .is_err()
    );
    assert_eq!(
        h.call(
            source,
            b"restoreContinuation:with:",
            &[continuation, argument]
        ),
        argument
    );
    assert!(
        host.send(
            source,
            b"restoreContinuation:with:",
            &[continuation, argument]
        )
        .is_err()
    );
    let clone = h.call(block, b"clone", &[]);
    assert_ne!(clone, block);
    assert_eq!(h.call(clone, b"value", &[]), capture);

    // A paused Machine is the sole remaining owner after binding replacement.
    let retained_owner = h.object();
    let retained = h.method("branch-join", 5, retained_owner);
    let ordinary = h.object();
    let ovt = h.private_vt(ordinary);
    h.bind(ovt, b"value", retained);
    let only = h.machine();
    let pinned = host.scope();
    pinned.keep(only).unwrap();
    let owner_id = h.identity(retained_owner);
    let method_id = h.identity(retained);
    assert_eq!(
        h.call(only, b"runBlock:budget:", &[ordinary, h.integer(2)]),
        only
    );
    h.bind(ovt, b"value", simple);
    h.clear();
    h.collect();
    assert!(h.present(owner_id) && h.present(method_id));
    assert_eq!(
        h.call(only, b"continueWithBudget:", &[h.integer(10)]),
        retained_owner
    );
    // Release the returned value before checking that completed frames release it.
    h.clear();
    h.collect();
    assert!(!h.present(owner_id) && !h.present(method_id));
    drop(pinned);
    h.clear();
    h.collect();

    // A detached Continuation alone retains the source Machine and its frames.
    let captured_owner = h.object();
    let capturing = h.method("call", 3, captured_owner);
    let block = h.block(capturing, &[]);
    let source = h.machine();
    let continuation = h.call(source, b"runBlock:", &[block]);
    let owner_id = h.identity(captured_owner);
    let source_id = h.identity(source);
    let pin = host.scope();
    pin.keep(continuation).unwrap();
    h.clear();
    h.collect();
    assert!(h.present(owner_id) && h.present(source_id));
    let supplied = h.named(b"true");
    assert_eq!(
        h.call(
            source,
            b"restoreContinuation:with:",
            &[continuation, supplied]
        ),
        supplied
    );
    h.clear();
    h.collect();
    assert!(!h.present(owner_id));
    drop(pin);
    h.clear();
    h.collect();
}
