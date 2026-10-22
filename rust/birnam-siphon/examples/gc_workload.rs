//! Paired collector workloads; setup and garbage creation are outside timers.
//! Use a release prefix through BIRNAM_TEST_MODULE_DIRECTORY. No policy changes.
#[path = "../tests/support/mod.rs"]
mod support;
#[path = "micro_support/clock.rs"]
mod clock;
use birnam_abi::id_protocol::Operand;
use std::{os::unix::ffi::OsStrExt, time::Instant};
use support::*;

fn main() {
    unsafe {
        let count: usize = std::env::args().nth(1).unwrap_or_else(|| "20000".into()).parse().unwrap();
        let id = id();
        let ns = load(id, "bootstrap", &[Operand::bytes(directory().as_os_str().as_bytes())]);
        let named = |name: &[u8]| object(send(id, ns, b"at:", &[Operand::object(id.intern_name(name).unwrap())]));
        let roots = named(b"Roots");
        let gc = named(b"GC");
        let behavior = named(b"ObjectVt");
        let allocate = id.intern_name(b"allocate").unwrap();
        let retain = id.intern_name(b"retain:").unwrap();
        let release = id.intern_name(b"release:").unwrap();
        let collect = id.intern_name(b"collectPrecisely").unwrap();
        send(id, gc, b"automaticCollection:", &[Operand::word(0)]);
        let mut tokens = Vec::new();
        for _ in 0..count {
            let value = id.send(behavior, allocate, &[Operand::word(0)]).unwrap();
            tokens.push(id.send(roots, retain, &[value]).unwrap());
        }
        id.send(gc, collect, &[]).unwrap();
        for phase in ["live", "garbage"] {
            let mut rows = Vec::new();
            for _ in 0..5 {
                if phase == "garbage" {
                    for _ in 0..count {
                        id.send(behavior, allocate, &[Operand::word(0)]).unwrap();
                    }
                }
                let wall = Instant::now();
                let cpu = clock::cpu_ns();
                let reclaimed = id.send(gc, collect, &[]).unwrap().as_word().unwrap();
                let used = clock::cpu_ns() - cpu;
                let elapsed = wall.elapsed().as_nanos();
                assert_eq!(reclaimed, if phase == "garbage" { count } else { 0 });
                rows.push(serde_json::json!({"cpu_ns":used,"wall_ns":elapsed,"reclaimed":reclaimed}));
            }
            println!("{}", serde_json::json!({"case":phase,"live_objects":count,"samples":rows}));
        }
        for token in tokens { id.send(roots, release, &[token]).unwrap(); }
    }
}
