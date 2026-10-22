//! Nonescaping Word regions and adjacent observation boundaries.
//! Run against each release prefix via BIRNAM_TEST_MODULE_DIRECTORY.
#[path = "../tests/support/mod.rs"] mod support;
#[path = "micro_support/clock.rs"] mod clock;
use birnam_abi::id_protocol::Operand;
use std::{os::unix::ffi::OsStrExt, time::Instant};
use support::*;
fn main() {
    unsafe {
        let iterations: usize = std::env::args().nth(1).unwrap_or_else(|| "10000".into()).parse().unwrap();
        let id = id();
        let ns = load(id, "bootstrap", &[Operand::bytes(directory().as_os_str().as_bytes())]);
        let named = |name: &[u8]| object(send(id, ns, b"at:", &[Operand::object(id.intern_name(name).unwrap())]));
        let roots = named(b"Roots"); let gc = named(b"GC");
        let keep = |v| { retain(id, roots, v); v };
        let text = |s: &str| keep(object(send(id, named(b"String"), b"fromBytes:", &[Operand::bytes(s.as_bytes())])));
        let a = keep(object(send(id, named(b"Word"), b"fromBits:", &[Operand::word(10)])));
        let b = keep(object(send(id, named(b"Word"), b"fromBits:", &[Operand::word(6)])));
        let selector = id.intern_name(b"compute:with:").unwrap();
        let args = [Operand::object(a), Operand::object(b)];
        for (name, body, expected) in [
            ("shared-region", "let x := [a bitXor: b]; let sum := [x wrappingAdd: a]; let masked := [sum bitAnd: x]; [self finish: [masked wrappingAdd: a]]", 14),
            ("single-use-region", "let x := [a bitXor: b]; let sum := [x wrappingAdd: a]; let masked := [sum bitAnd: b]; [self finish: [masked wrappingAdd: a]]", 16),
            ("escaping-local", "let x := [a bitXor: b]; let sum := [x wrappingAdd: a]; let masked := [sum bitAnd: x]; let result := [masked wrappingAdd: a]; [self finish: x]; result", 14),
            ("observation-barrier", "let x := [a bitXor: b]; [self finish: x]; let sum := [x wrappingAdd: a]; let masked := [sum bitAnd: x]; [self finish: [masked wrappingAdd: a]]", 14),
        ] {
            let source = format!(r#"let NumericProbe := [Object clone]; [Reflect defineMethod: #finish: on: NumericProbe as: [Execution block: [:self :value | value] withNativeSignature: "oO"]]; [Reflect defineMethod: #compute:with: on: NumericProbe as: [Execution block: [:self :a :b | {body}] withNativeSignature: "oOO"]]; NumericProbe"#);
            let probe = keep(object(send(id, named(b"Codegen"), b"eval:", &[Operand::object(text(&source))])));
            assert_eq!(send(id, probe, b"isError", &[]).as_object(), Some(named(b"false")));
            let run = || id.send(probe, selector, &args).unwrap();
            let check = |r| assert_eq!(send(id, object(r), b"bits", &[]).as_word(), Some(expected));
            for _ in 0..10 { check(run()); }
            send(id, gc, b"automaticCollection:", &[Operand::word(0)]);
            let mut samples = Vec::new();
            for _ in 0..5 {
                collect(id, gc);
                let before = send(id, id.receiver, b"allocationCount", &[]).as_word().unwrap();
                let start = Instant::now(); let cpu = clock::cpu_ns();
                let mut answer = Operand::unit();
                for _ in 0..iterations { answer = run(); std::hint::black_box(answer); }
                let cpu = clock::cpu_ns() - cpu; let wall = start.elapsed().as_nanos();
                let allocations = send(id, id.receiver, b"allocationCount", &[]).as_word().unwrap() - before;
                check(answer);
                samples.push(serde_json::json!({"cpu_ns":cpu,"wall_ns":wall,"allocations":allocations}));
            }
            println!("{}", serde_json::json!({"case":name,"source":source,"iterations":iterations,"samples":samples}));
            send(id, gc, b"automaticCollection:", &[Operand::word(1)]);
        }
    }
}
