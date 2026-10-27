//! Production Alien calls, buffers, callbacks and code reclamation through Oops.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use birnam_host_support::{Context, Scope};
struct H<'a> {
    host: &'a Context,
    q: Scope<'a>,
    hooks: oop::hooks::Hooks<'a>,
    plain_integer: Oop,
    collecting_integer: Oop,
}
impl H<'_> {
    fn named(&self, n: &[u8]) -> Oop {
        self.host.named(n).unwrap()
    }
    fn key(&self, n: &[u8]) -> Oop {
        self.hooks.symbol(n)
    }
    fn call(&self, r: Oop, n: &[u8], a: &[Oop]) -> Oop {
        self.q.call(r, n, a).unwrap()
    }
    fn string(&self, b: &[u8]) -> Oop {
        self.q.string(b).unwrap()
    }
    fn number(&self, o: Oop) -> usize {
        self.q
            .word_bits(self.call(self.named(b"Integer"), b"wordOf:", &[o]))
            .unwrap()
    }
    fn int(&self, n: usize) -> Oop {
        self.call(
            self.named(b"Integer"),
            b"fromWord:",
            &[self.q.word(n).unwrap()],
        )
    }
    fn bind(&self, n: &[u8], o: Oop) {
        self.call(self.host.namespace, b"at:put:", &[self.key(n), o]);
    }
    fn eval(&self, b: &[u8]) -> Oop {
        let result = self.call(self.named(b"Codegen"), b"eval:", &[self.string(b)]);
        self.q.check_result(result).unwrap();
        result
    }
    fn integer_factory(&self, m: Oop) {
        self.call(
            self.named(b"IntegerVt"),
            b"addMethod",
            &[self.key(b"fromWord:"), m],
        );
    }
    fn compiler_call(&self, n: &[u8], a: &[Oop]) -> Oop {
        // Collect during FFI marshalling, not each allocation in the authored compiler.
        self.integer_factory(self.plain_integer);
        let result = self.host.send(self.named(b"Alien"), n, a);
        if let Ok(value) = result {
            self.q.keep(value).unwrap();
        }
        self.integer_factory(self.collecting_integer);
        result.unwrap()
    }
    fn wrap(&self, n: &[u8], signature: &[u8], selector: &[u8], target: Oop) {
        let result = self.compiler_call(
            b"wrap:signature:as:on:",
            &[
                self.string(n),
                self.string(signature),
                self.key(selector),
                target,
            ],
        );
        self.q.check_result(result).unwrap();
        assert_eq!(result, target);
    }
}
#[test]
#[ignore = "requires production Oop modules with TCC metrics"]
fn alien_calls_callbacks_and_owned_memory() {
    let host = oop::bootstrap();
    let floor = host.scope();
    let named = |n: &[u8]| host.named(n).unwrap();
    let gc = named(b"GC");
    let tcc = named(b"TCC");
    floor
        .call(gc, b"precisePayloads:", &[size(1).unwrap()])
        .unwrap();
    let module = host.global(&floor, b"Module").unwrap();
    floor
        .call(
            module,
            b"libDir:",
            &[floor
                .string(host.resources.lib_dir.as_os_str().as_encoded_bytes())
                .unwrap()],
        )
        .unwrap();
    let store_path = floor
        .string(
            host.resources
                .lib_dir
                .join("libbirnam_store.dylib")
                .as_os_str()
                .as_encoded_bytes(),
        )
        .unwrap();
    floor
        .check_result(
            floor
                .call(named(b"Codegen"), b"load:", &[store_path])
                .unwrap(),
        )
        .unwrap();
    floor
        .call(module, b"markLoaded:", &[host.intern(b"store").unwrap()])
        .unwrap();
    for name in [
        b"buildGrammar".as_slice(),
        b"initializeGrammar",
        b"grammar",
        b"parse:",
        b"furthestFail",
    ] {
        floor
            .call(
                host.id.root,
                b"bind:to:",
                &[host.intern(name).unwrap(), named(b"Lang")],
            )
            .unwrap();
    }
    floor.call(gc, b"collectPrecisely", &[]).unwrap();
    floor.call(tcc, b"checkpoint", &[]).unwrap();
    let units = as_size(floor.call(tcc, b"liveNativeUnits", &[]).unwrap()).unwrap();
    let fixture_path =
        std::env::temp_dir().join(format!("birnam-alien-{}.dylib", std::process::id()));
    assert!(
        std::process::Command::new("clang")
            .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-dynamiclib"])
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/alien.c")
            )
            .arg("-o")
            .arg(&fixture_path)
            .status()
            .unwrap()
            .success()
    );
    {
        let hooks = oop::hooks::Hooks::new(&host);
        let original = hooks.capture(named(b"IntegerVt"), hooks.symbol(b"fromWord:"));
        let state = hooks.context(&[original, gc, size(0).unwrap()]);
        let collecting = hooks.method(b"collectEvery", state);
        let h = H {
            host: &host,
            q: host.scope(),
            hooks,
            plain_integer: original,
            collecting_integer: collecting,
        };
        h.integer_factory(collecting);
        let alien = named(b"Alien");
        let nil = named(b"nil");
        let integer = named(b"Integer");
        let fixture = h.string(fixture_path.as_os_str().as_encoded_bytes());
        assert_eq!(h.call(alien, b"dlopen:", &[fixture]), named(b"true"));
        h.wrap(b"strlen", b"l s", b"foreignLength", named(b"String"));
        assert_eq!(
            h.number(h.call(h.string(b"hello"), b"foreignLength", &[])),
            5
        );
        h.wrap(b"alien_add", b"l l l", b"foreignAdd:", integer);
        assert_eq!(
            h.number(h.call(h.int(17), b"foreignAdd:", &[h.int(25)])),
            42
        );
        h.wrap(b"alien_pair", b"{ll} l", b"foreignPair", integer);
        let pair = h.call(h.int(20), b"foreignPair", &[]);
        assert_eq!(h.number(h.call(pair, b"at:", &[h.int(1)])), 21);
        h.wrap(b"alien_sum", b"l {ll}", b"foreignSum", named(b"Vector"));
        assert_eq!(h.number(h.call(pair, b"foreignSum", &[])), 41);
        h.wrap(
            b"alien_unsigned",
            b"u u",
            b"foreignUnsigned",
            named(b"Word"),
        );
        let max = h.call(
            named(b"Word"),
            b"fromBits:",
            &[h.q.word(usize::MAX).unwrap()],
        );
        let max2 = h.call(max, b"foreignUnsigned", &[]);
        assert_eq!(
            h.q.word_bits(h.call(max2, b"bits", &[])).unwrap(),
            usize::MAX
        );
        h.wrap(
            b"alien_unsigned_pair",
            b"{u} u",
            b"foreignUnsignedPair",
            named(b"Word"),
        );
        let pair = h.call(max, b"foreignUnsignedPair", &[]);
        let value = h.call(pair, b"at:", &[h.int(0)]);
        assert_eq!(
            h.q.word_bits(h.call(value, b"bits", &[])).unwrap(),
            usize::MAX
        );
        h.wrap(b"alien_double", b"d d", b"foreignDouble", named(b"Float"));
        let input = h.call(
            named(b"Float"),
            b"fromBits:",
            &[h.q.word(1.25f64.to_bits() as usize).unwrap()],
        );
        let result = h.call(input, b"foreignDouble", &[]);
        assert_eq!(
            h.q.word_bits(h.call(named(b"Float"), b"bitsOf:", &[result]))
                .unwrap(),
            2.5f64.to_bits() as usize
        );
        let memory = h.call(alien, b"alloc:", &[h.int(32)]);
        assert_eq!(h.number(h.call(memory, b"peekIntAt:", &[h.int(1)])), 0);
        h.call(memory, b"pokeInt:at:", &[h.int(1234), h.int(1)]);
        assert_eq!(h.number(h.call(memory, b"peekIntAt:", &[h.int(1)])), 1234);
        assert_eq!(h.call(memory, b"free", &[]), nil);
        assert_eq!(h.call(memory, b"isNull", &[]), named(b"true"));
        assert_eq!(
            h.call(h.call(memory, b"free", &[]), b"kind", &[]),
            h.key(b"alreadyFreed")
        );
        h.wrap(b"alien_call", b"l l p", b"foreignCall:", integer);
        let block = h.eval(b"[:x | [GC collectPrecisely]; [x + 1]]");
        assert_eq!(h.number(h.call(block, b"value:", &[h.int(4)])), 5);
        let cb = h.compiler_call(b"callback:signature:", &[block, h.string(b"l l")]);
        assert_eq!(h.number(h.call(h.int(41), b"foreignCall:", &[cb])), 42);
        h.call(gc, b"collectPrecisely", &[]);
        assert_eq!(h.number(h.call(h.int(9), b"foreignCall:", &[cb])), 10);
        h.bind(b"AlienTestCallback", cb);
        let recursive=h.eval(b"[:x | [[x = 0] ifTrue: [| 10] ifFalse: [| [[[x - 1] foreignCall: AlienTestRecursive] + 1]]]]");
        let cb2 = h.compiler_call(b"callback:signature:", &[recursive, h.string(b"l l")]);
        h.bind(b"AlienTestRecursive", cb2);
        assert_eq!(h.number(h.call(h.int(3), b"foreignCall:", &[cb2])), 13);
        h.call(alien, b"releaseCallback:", &[cb2]);
        let releasing=h.eval(b"[:x | [Alien releaseCallback: AlienRelease]; [GC collectPrecisely]; [TCC checkpoint]; [x + 2]]");
        let cb3 = h.compiler_call(b"callback:signature:", &[releasing, h.string(b"l l")]);
        h.bind(b"AlienRelease", cb3);
        assert_eq!(h.number(h.call(h.int(40), b"foreignCall:", &[cb3])), 42);
        assert_eq!(h.call(cb3, b"isNull", &[]), named(b"true"));
        for n in [
            b"AlienRelease".as_slice(),
            b"AlienTestCallback",
            b"AlienTestRecursive",
        ] {
            h.bind(n, nil);
        }
        assert_eq!(h.call(alien, b"releaseCallback:", &[cb]), nil);
        assert_eq!(h.call(cb, b"isNull", &[]), named(b"true"));
        assert_eq!(
            h.call(h.call(alien, b"dlclose:", &[fixture]), b"kind", &[]),
            h.key(b"inUse")
        );
        for (target, selector) in [
            (named(b"String"), b"foreignLength".as_slice()),
            (integer, b"foreignAdd:"),
            (integer, b"foreignPair"),
            (named(b"Vector"), b"foreignSum"),
            (named(b"Word"), b"foreignUnsigned"),
            (named(b"Word"), b"foreignUnsignedPair"),
            (named(b"Float"), b"foreignDouble"),
            (integer, b"foreignCall:"),
        ] {
            let vt = h.call(host.id.root, b"behaviorOf:", &[target]);
            h.call(named(b"Store"), b"removeMethod:on:", &[h.key(selector), vt]);
        }
        assert!(
            h.hooks.count(state, 2) > 0,
            "FFI values must use the current Integer constructor"
        );
        h.integer_factory(original);
    }
    for _ in 0..2 {
        floor.call(gc, b"collectPrecisely", &[]).unwrap();
        floor.call(tcc, b"checkpoint", &[]).unwrap();
    }
    assert_eq!(
        as_size(floor.call(tcc, b"liveNativeUnits", &[]).unwrap()).unwrap(),
        units
    );
    let fixture = floor
        .string(fixture_path.as_os_str().as_encoded_bytes())
        .unwrap();
    assert_eq!(
        floor
            .call(named(b"Alien"), b"dlclose:", &[fixture])
            .unwrap(),
        named(b"true")
    );
    std::fs::remove_file(fixture_path).unwrap();
}
