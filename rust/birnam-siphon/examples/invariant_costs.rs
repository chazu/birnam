//! Focused operation benchmarks. Setup and explicit collection are outside timing.
//! Run with BIRNAM_TEST_MODULE_DIRECTORY pointing at a release native composition.
#![allow(dead_code)]
#[path = "../tests/support/minimal.rs"]
mod minimal;
use birnam_abi::{Oop, as_size};
use std::{hint::black_box, time::Instant};
fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "strings".into());
    let h = minimal::native_floor();
    let q = h.scope();
    let gc = h.named(b"GC").unwrap();
    let size = h.intern(b"sizeWord").unwrap();
    if mode == "strings" {
        let builder = h.named(b"StringBuilder").unwrap();
        let new = h.intern(b"new").unwrap();
        let add = h.intern(b"add:").unwrap();
        let as_string = h.intern(b"asString").unwrap();
        for n in [16usize, 8192, 1 << 20] {
            let bytes: Vec<_> = (0..n).map(|i| (i % 251) as u8).collect();
            let text = q.string(&bytes).unwrap();
            let b = q.keep(unsafe { h.id.send(builder, new, &[]) }.unwrap()).unwrap();
            unsafe { h.id.send(b, add, &[text]) }.unwrap();
            for sample in 0..7 {
                for operation in ["host_string", "builder_string"] {
                    q.call(gc, b"collectPrecisely", &[]).unwrap();
                    let calls = if n < 8192 { 500 } else if n < 1 << 20 { 100 } else { 8 };
                    let start = Instant::now();
                    for _ in 0..calls {
                        let scope = h.scope();
                        let result: Oop = if operation == "host_string" {
                            scope.string(black_box(&bytes)).unwrap()
                        } else {
                            unsafe { h.id.send(b, as_string, &[]) }.unwrap()
                        };
                        assert_eq!(as_size(unsafe { h.id.send(result, size, &[]) }.unwrap()), Some(n));
                        black_box(result);
                    }
                    let ns = start.elapsed().as_nanos() as f64 / calls as f64;
                    println!("{{\"operation\":\"{operation}\",\"bytes\":{n},\"sample\":{sample},\"calls\":{calls},\"ns_per_call\":{ns}}}");
                }
            }
            assert_eq!(q.read_bytes(unsafe { h.id.send(b, as_string, &[]) }.unwrap()).unwrap(), bytes);
        }
    } else { panic!("unknown benchmark {mode}"); }
}
