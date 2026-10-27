//! Copied values and worker replies use current owners, including collecting replacements.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, size};
use birnam_host_support::{Context, Scope};

struct H<'a> {
    host: &'a Context,
    q: Scope<'a>,
    hooks: oop::hooks::Hooks<'a>,
}
impl H<'_> {
    fn named(&self, n: &[u8]) -> Oop {
        self.host.named(n).unwrap()
    }
    fn key(&self, n: &[u8]) -> Oop {
        self.hooks.symbol(n)
    }
    fn call(&self, r: Oop, n: &[u8], args: &[Oop]) -> Oop {
        self.q.call(r, n, args).unwrap()
    }
    fn string(&self, b: &[u8]) -> Oop {
        self.q.string(b).unwrap()
    }
    fn bytes(&self, o: Oop) -> Vec<u8> {
        self.q.read_bytes(self.call(o, b"bytes", &[])).unwrap()
    }
    fn int(&self, n: usize) -> Oop {
        self.call(self.named(b"Integer"), b"fromWord:", &[size(n).unwrap()])
    }
    fn number(&self, o: Oop) -> usize {
        self.q
            .word_bits(self.call(self.named(b"Integer"), b"wordOf:", &[o]))
            .unwrap()
    }
    fn eval(&self, b: &[u8]) -> Oop {
        let result = self.call(self.named(b"Codegen"), b"eval:", &[self.string(b)]);
        self.q.check_result(result).unwrap();
        result
    }
    fn bind(&self, vt: Oop, selector: &[u8], method: Oop) {
        self.call(vt, b"addMethod", &[self.key(selector), method]);
    }
}

#[test]
#[ignore = "requires production Oop modules"]
fn isolate_value_conversion_uses_owner_messages() {
    let host = oop::bootstrap();
    {
        let h = H {
            host: &host,
            q: host.scope(),
            hooks: oop::hooks::Hooks::new(&host),
        };
        let gc = h.named(b"GC");
        h.call(gc, b"precisePayloads:", &[size(1).unwrap()]);
        let iv = h.named(b"IntegerVt");
        let original = h.hooks.capture(iv, h.key(b"fromWord:"));
        let forwarding = h.hooks.context(&[original, gc, size(0).unwrap()]);
        h.bind(
            iv,
            b"fromWord:",
            h.hooks.method(b"collectEvery", forwarding),
        );
        let isolate = h.named(b"Isolate");
        let value = h.eval(b"#[nil true false 42 1.25 \"binary\" #symbol #[7 8]]");
        let encoded = h.call(isolate, b"encode:", &[value]);
        let bytes = h.bytes(encoded);
        assert_eq!(&bytes[..5], &[7, 0, 0, 0, 8]);
        let restored = h.call(isolate, b"decode:", &[encoded]);
        assert_eq!(h.number(h.call(restored, b"size", &[])), 8);
        h.call(gc, b"collectPrecisely", &[]);
        assert_eq!(h.bytes(h.call(restored, b"at:", &[h.int(5)])), b"binary");
        assert_eq!(h.bytes(h.call(isolate, b"encode:", &[restored])), bytes);
        assert!(
            h.hooks.count(forwarding, 2) > 0,
            "current Integer construction must be entered"
        );
        let map = h.call(h.named(b"Map"), b"new", &[]);
        let map = h.call(map, b"at:put:", &[h.string(b"key"), value]);
        let encoded_map = h.call(isolate, b"encode:", &[map]);
        let map2 = h.call(isolate, b"decode:", &[encoded_map]);
        assert_eq!(
            h.bytes(h.call(isolate, b"encode:", &[map2])),
            h.bytes(encoded_map)
        );
        let block = h.eval(b"[| 1]");
        let error = h.call(isolate, b"encode:", &[block]);
        h.call(gc, b"collectPrecisely", &[]);
        assert_eq!(h.call(error, b"isError", &[]), h.named(b"true"));
        assert_eq!(h.call(error, b"class", &[]), h.key(b"encode"));
        assert_eq!(h.call(error, b"kind", &[]), h.key(b"nonTransferable"));
        assert_eq!(h.bytes(h.call(error, b"message", &[])), b"NonTransferable");
        let malformed = h.call(
            h.named(b"Bytes"),
            b"fromBytes:",
            &[h.q.byte_values(&[7, 255, 255, 255, 255]).unwrap()],
        );
        let error = h.call(isolate, b"decode:", &[malformed]);
        assert_eq!(
            h.bytes(h.call(error, b"message", &[])),
            b"ContainerTooLarge"
        );
        let cycle = h.eval(b"#[1]");
        let vt = h.named(b"VectorVt");
        let old_at = h.hooks.capture(vt, h.key(b"at:"));
        let cyclic = h
            .hooks
            .context(&[h.hooks.method(b"fixed", cycle), gc, size(0).unwrap()]);
        h.bind(vt, b"at:", h.hooks.method(b"collectEvery", cyclic));
        let error = h.call(isolate, b"encode:", &[cycle]);
        h.bind(vt, b"at:", old_at);
        assert!(h.hooks.count(cyclic, 2) > 0);
        assert_eq!(h.bytes(h.call(error, b"message", &[])), b"CyclicValue");

        // A native owner failure keeps its identity instead of becoming a codec Error.
        let failure = unsafe {
            host.id.send(
                h.named(b"Execution"),
                h.key(b"failNative:detail:"),
                &[size(1).unwrap(), size(813).unwrap()],
            )
        }
        .unwrap_err();
        h.q.keep(failure).unwrap();
        let failing = h
            .hooks
            .method(b"deny", h.hooks.context(&[failure, size(0).unwrap()]));
        h.bind(iv, b"fromWord:", failing);
        let result = unsafe { host.id.send(isolate, h.key(b"decode:"), &[encoded]) };
        h.bind(iv, b"fromWord:", original);
        assert_eq!(result.unwrap_err(), failure);
        reply_construction_failure(&h, failing, failure);
    }
    host.send(host.named(b"GC").unwrap(), b"collectPrecisely", &[])
        .unwrap();
}

fn reply_construction_failure(h: &H<'_>, failing: Oop, failure: Oop) {
    let peer = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/oop-host/isolate_reply.py");
    let previous = std::env::var_os("BIRNAM_WORKER_EXECUTABLE");
    // This binary has one test. Only worker creation reads this override.
    unsafe { std::env::set_var("BIRNAM_WORKER_EXECUTABLE", &peer) };
    let started = h.host.send(
        h.named(b"Isolate"),
        b"start:selector:image:",
        &[
            h.string(b"Peer"),
            h.string(b"echo:"),
            h.string(peer.as_os_str().as_encoded_bytes()),
        ],
    );
    unsafe {
        match previous {
            Some(value) => std::env::set_var("BIRNAM_WORKER_EXECUTABLE", value),
            None => std::env::remove_var("BIRNAM_WORKER_EXECUTABLE"),
        }
    }
    let handle = h.q.keep(started.unwrap()).unwrap();
    assert_eq!(h.call(handle, b"state", &[]), h.key(b"ready"));
    let request = h.call(handle, b"submit:timeout:", &[h.named(b"nil"), h.int(5000)]);
    let vt = h.named(b"VectorVt");
    let original = h.hooks.capture(vt, h.key(b"new"));
    h.bind(vt, b"new", failing);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let result = loop {
        match unsafe { h.host.id.send(handle, h.key(b"pollRequest:"), &[request]) } {
            Err(error) => break error,
            Ok(value) => assert_eq!(value, h.named(b"nil")),
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    h.bind(vt, b"new", original);
    assert_eq!(result, failure);
    assert_eq!(h.call(handle, b"state", &[]), h.key(b"exited"));
}
