//! Isolate the ordinary boxed-slot adapter from authored loop/frame overhead.
//! BIRNAM_TEST_MODULE_DIRECTORY=PREFIX/lib cargo run --release -p birnam-siphon
//!   --example slot_cost -- 100000
//! macOS clock 16 is owner-thread CPU time. Setup and warmup are outside timers.
#[path = "../tests/support/mod.rs"]
mod support;
use birnam_abi::id_protocol::Operand;
use std::os::unix::ffi::OsStrExt;
use support::*;
unsafe extern "C" { fn clock_gettime_nsec_np(clock: i32) -> u64; }
fn main() {
    let count: usize = std::env::args().nth(1).unwrap_or_else(|| "100000".into()).parse().unwrap();
    assert!(count > 0);
    unsafe {
        let id = id();
        let ns = load(id, "bootstrap", &[Operand::bytes(directory().as_os_str().as_bytes())]);
        let named = |name: &[u8]| object(send(id, ns, b"at:", &[Operand::object(id.intern_name(name).unwrap())]));
        let roots = named(b"Roots"); let model = named(b"ObjectModel");
        let behavior = object(send(id, named(b"ObjectVt"), b"delegated", &[]));
        let mut tokens = vec![retain(id, roots, behavior)];
        send(id, model, b"setLayoutOf:to:rest:", &[Operand::object(behavior), Operand::bytes(&[1]), Operand::word(0)]);
        let target = object(send(id, model, b"allocate:bytes:", &[Operand::object(behavior), Operand::word(size_of::<usize>())]));
        tokens.push(retain(id, roots, target));
        let value = object(send(id, named(b"Object"), b"clone", &[]));
        tokens.push(retain(id, roots, value));
        let index = send(id, named(b"Integer"), b"fromWord:", &[Operand::word(0)]);
        tokens.push(retain(id, roots, object(index)));
        let read = [index, Operand::object(target)];
        let write = [index, Operand::object(target), Operand::object(value)];
        send(id, model, b"slot:in:put:", &write);
        for (name, args, expected) in [(b"slot:in:".as_slice(), read.as_slice(), value),
            (b"slot:in:put:".as_slice(), write.as_slice(), target)] {
            let selector = id.intern_name(name).unwrap();
            tokens.push(retain(id, roots, selector));
            for _ in 0..1000 { assert_eq!(id.send(model, selector, args).unwrap().as_object(), Some(expected)); }
            let before = clock_gettime_nsec_np(16);
            for _ in 0..count { assert_eq!(id.send(model, selector, args).unwrap().as_object(), Some(expected)); }
            let elapsed = clock_gettime_nsec_np(16) - before;
            println!("{{\"case\":\"{}\",\"iterations\":{},\"cpu_ns\":{}}}",
                std::str::from_utf8(name).unwrap(), count, elapsed);
        }
        for token in tokens { release(id, roots, token); }
    }
}
