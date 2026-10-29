//! Oop arithmetic timings; setup, selector interning and explicit GC are untimed.
#[path = "support/minimal.rs"]
#[allow(dead_code)]
mod minimal;
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::size;
use std::time::Instant;
#[test]
#[ignore = "production prefix; BIRNAM_INTEGER_BENCH_MODE=native|authored"]
fn integer_arithmetic_benchmark() {
    let mode = std::env::var("BIRNAM_INTEGER_BENCH_MODE").unwrap_or("authored".into());
    let start = Instant::now();
    let h = match mode.as_str() {
        "native" => minimal::native_floor(),
        "authored" => oop::bootstrap(),
        _ => panic!("unknown arithmetic mode"),
    };
    let q = h.scope();
    let integer = h.named(b"Integer").unwrap();
    let gc = h.named(b"GC").unwrap();
    let left = q
        .call(integer, b"fromWord:", &[size(12345).unwrap()])
        .unwrap();
    let right = q.call(integer, b"fromWord:", &[size(37).unwrap()]).unwrap();
    println!(
        "BENCH_START mode={mode} startup_ms={:.3} abi=oop",
        start.elapsed().as_secs_f64() * 1000.0
    );
    let count = std::env::var("BIRNAM_INTEGER_BENCH_COUNT")
        .ok()
        .map(|s| s.parse::<usize>().unwrap())
        .unwrap_or(500);
    assert!(count > 0);
    for (name, expected) in [
        (b"+".as_slice(), 12382),
        (b"-", 12308),
        (b"*", 456765),
        (b"/", 333),
        (b"mod:", 24),
    ] {
        if std::env::var("BIRNAM_INTEGER_BENCH_OP").is_ok_and(|v| v.as_bytes() != name) {
            continue;
        }
        let selector = q.keep(h.intern(name).unwrap()).unwrap();
        let word = q.keep(h.intern(b"word").unwrap()).unwrap();
        for (gc_mode, budget, n) in [
            ("off", usize::MAX >> 1, count),
            ("normal", 1 << 20, count),
            ("stress", 1, 2),
        ] {
            if std::env::var("BIRNAM_INTEGER_BENCH_GC").is_ok_and(|v| v != gc_mode) {
                continue;
            }
            q.call(gc, b"collectPrecisely", &[]).unwrap();
            q.call(gc, b"budget:", &[size(budget).unwrap()]).unwrap();
            let trials = std::env::var("BIRNAM_INTEGER_BENCH_TRIALS")
                .ok()
                .map(|s| s.parse::<usize>().unwrap())
                .unwrap_or(3);
            for trial in 0..trials {
                let started = Instant::now();
                for _ in 0..n {
                    let local = h.scope();
                    let value = local
                        .keep(unsafe { h.id.send(left, selector, &[right]) }.unwrap())
                        .unwrap();
                    let result = local
                        .keep(unsafe { h.id.send(value, word, &[]) }.unwrap())
                        .unwrap();
                    assert_eq!(local.word_bits(result).unwrap(), expected);
                }
                println!(
                    "BENCH mode={mode} op={} gc={gc_mode} trial={trial} count={n} ns_per_op={:.1}",
                    String::from_utf8_lossy(name),
                    started.elapsed().as_nanos() as f64 / n as f64
                );
            }
        }
    }
}
