//! Matched native/authored Id arithmetic; startup and policy admission are untimed.
mod support;
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use std::{collections::BTreeMap, ffi::c_void, os::unix::ffi::OsStrExt, time::Instant};
use support::*;
struct Seed {
    id: Protocol,
    loader: NativeEntry,
    entries: Vec<(Oop, NativeEntry)>,
    receiver: Option<Oop>,
}
unsafe extern "C" fn capture_seed(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    let seed = unsafe { &mut *c.cast::<Seed>() };
    let args = unsafe { borrowed_slice(a, n) }.unwrap();
    let bytes = args[0].as_bytes().unwrap();
    let is_integer = unsafe { borrowed_bytes(bytes.pointer, bytes.length) }
        .unwrap()
        .ends_with(b"libbirnam_integer.dylib");
    let result = unsafe { (seed.loader.method.unwrap())(seed.loader.context, r, a, n) };
    if result.status == 0 && is_integer {
        let integer = result.value.as_object().unwrap();
        seed.receiver = Some(integer);
        for name in [b"+".as_slice(), b"-", b"*", b"/", b"mod:"] {
            let selector = unsafe { seed.id.intern_name(name) }.unwrap();
            let entry = unsafe {
                send(
                    seed.id,
                    seed.id.receiver,
                    b"bind:to:",
                    &[Operand::object(selector), Operand::object(integer)],
                )
            }
            .as_entry()
            .unwrap();
            seed.entries.push((selector, entry));
        }
    }
    result
}

struct Census {
    id: Protocol,
    families: Vec<(Oop, &'static str)>,
    counts: BTreeMap<&'static str, usize>,
}
unsafe extern "C" fn allocation(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    let census = unsafe { &mut *c.cast::<Census>() };
    let values = unsafe { borrowed_slice(a, n) }.unwrap();
    let behavior = unsafe { send(census.id, census.id.receiver, b"behaviorOf:", &[values[0]]) }
        .as_object()
        .unwrap();
    let mut current = behavior;
    let mut name = if behavior == census.id.receiver {
        "behavior"
    } else {
        "other"
    };
    for _ in 0..64 {
        if let Some((_, family)) = census.families.iter().find(|(vt, _)| *vt == current) {
            name = family;
            break;
        }
        let Some(parent) = (unsafe {
            send(
                census.id,
                census.id.receiver,
                b"parentOf:",
                &[Operand::object(current)],
            )
        })
        .as_object() else {
            break;
        };
        current = parent;
    }
    *census.counts.entry(name).or_default() += 1;
    Outcome::loaded(Operand::unit())
}
unsafe fn census(id: Protocol, families: &[(Oop, &'static str)]) -> BTreeMap<&'static str, usize> {
    let mut c = Census {
        id,
        families: families.to_vec(),
        counts: BTreeMap::new(),
    };
    unsafe {
        inventory(id, allocation, (&raw mut c).cast());
    }
    c.counts
}

#[test]
#[ignore = "production prefix; BIRNAM_INTEGER_BENCH_MODE=native|authored"]
fn integer_arithmetic_benchmark() {
    unsafe {
        let startup = Instant::now();
        let id = id();
        let mode = std::env::var("BIRNAM_INTEGER_BENCH_MODE").unwrap_or("authored".into());
        let loader = send(id, id.receiver, b"lookup", &[Operand::object(id.load)])
            .as_entry()
            .unwrap();
        let mut seed = Seed {
            id,
            loader,
            entries: vec![],
            receiver: None,
        };
        if mode == "native" {
            send(
                id,
                id.receiver,
                b"addMethod",
                &[
                    Operand::object(id.load),
                    Operand::entry(NativeEntry::new(capture_seed, (&raw mut seed).cast())),
                ],
            );
        }
        let ns = load(
            id,
            "bootstrap",
            &[Operand::bytes(directory().as_os_str().as_bytes())],
        );
        let named = |name: &[u8]| {
            object(send(
                id,
                ns,
                b"at:",
                &[Operand::object(id.intern_name(name).unwrap())],
            ))
        };
        let roots = named(b"Roots");
        let gc = named(b"GC");
        let integer = named(b"Integer");
        if mode == "native" {
            send(
                id,
                id.receiver,
                b"addMethod",
                &[Operand::object(id.load), Operand::entry(loader)],
            );
            assert_eq!(seed.receiver, Some(integer));
            assert_eq!(seed.entries.len(), 5);
            let vt = named(b"IntegerVt");
            for (selector, entry) in &seed.entries {
                send(
                    id,
                    vt,
                    b"addMethod",
                    &[Operand::object(*selector), Operand::entry(*entry)],
                );
            }
        }
        let startup_ms = startup.elapsed().as_secs_f64() * 1000.0;
        let admission = Instant::now();
        if mode == "authored" {
            let path = object(send(
                id,
                named(b"String"),
                b"fromBytes:",
                &[Operand::bytes(
                    directory()
                        .join("libbirnam_integerValues.dylib")
                        .as_os_str()
                        .as_bytes(),
                )],
            ));
            let token = retain(id, roots, path);
            let result = object(send(
                id,
                named(b"Codegen"),
                b"load:",
                &[Operand::object(path)],
            ));
            assert_eq!(object(send(id, result, b"isError", &[])), named(b"false"));
            release(id, roots, token);
        } else {
            assert_eq!(mode, "native");
        }
        println!(
            "BENCH_START mode={mode} startup_ms={startup_ms:.3} admission_ms={:.3}",
            admission.elapsed().as_secs_f64() * 1000.0
        );
        let left = object(send(id, integer, b"fromWord:", &[Operand::word(12345)]));
        let right = object(send(id, integer, b"fromWord:", &[Operand::word(37)]));
        let lt = retain(id, roots, left);
        let rt = retain(id, roots, right);
        let families = [
            (named(b"IntegerVt"), "integer"),
            (named(b"WordVt"), "word"),
            (named(b"VectorVt"), "vector"),
            (named(b"BlockVt"), "block"),
            (named(b"MethodVt"), "method"),
        ];
        let count = std::env::var("BIRNAM_INTEGER_BENCH_COUNT")
            .ok()
            .map(|v| v.parse::<usize>().unwrap())
            .unwrap_or(500);
        for (selector, expected) in [
            (b"+".as_slice(), 12382),
            (b"-", 12308),
            (b"*", 456765),
            (b"/", 333),
            (b"mod:", 24),
        ] {
            if std::env::var("BIRNAM_INTEGER_BENCH_OP").is_ok_and(|v| v.as_bytes() != selector) {
                continue;
            }
            let symbol = id.intern_name(selector).unwrap();
            send(id, gc, b"budget:", &[Operand::word(1 << 20)]);
            let entry = send(
                id,
                id.receiver,
                b"bind:to:",
                &[Operand::object(symbol), Operand::object(left)],
            )
            .as_entry()
            .unwrap();
            assert_eq!(
                send(
                    id,
                    named(b"Execution"),
                    b"implementationOfEntry:",
                    &[Operand::entry(entry)]
                )
                .as_object()
                .is_some(),
                mode == "authored"
            );
            for _ in 0..20 {
                id.send(left, symbol, &[Operand::object(right)]).unwrap();
            }
            for (gc_mode, budget, n, trials) in [
                ("off", usize::MAX, count, 3),
                ("normal", 1 << 20, count, 3),
                ("stress", 1, 2, 1),
            ] {
                if std::env::var("BIRNAM_INTEGER_BENCH_GC").is_ok_and(|v| v != gc_mode) {
                    continue;
                }
                let trials = std::env::var("BIRNAM_INTEGER_BENCH_TRIALS")
                    .ok()
                    .map(|v| v.parse::<usize>().unwrap())
                    .unwrap_or(trials);
                for trial in 0..trials {
                    collect(id, gc);
                    send(id, gc, b"budget:", &[Operand::word(budget)]);
                    let before = if gc_mode == "off" {
                        Some(census(id, &families))
                    } else {
                        None
                    };
                    let bytes_before = send(id, gc, b"allocationBytes", &[]).as_word().unwrap();
                    let start = Instant::now();
                    for _ in 0..n {
                        let result = id.send(left, symbol, &[Operand::object(right)]).unwrap();
                        assert_eq!(
                            send(id, object(result), b"word", &[]).as_word(),
                            Some(expected)
                        );
                    }
                    let nanos = start.elapsed().as_nanos();
                    let bytes_after = send(id, gc, b"allocationBytes", &[]).as_word().unwrap();
                    let mut delta = BTreeMap::new();
                    if let Some(before) = before {
                        let after = census(id, &families);
                        for (name, value) in after {
                            delta.insert(name, value - before.get(name).copied().unwrap_or(0));
                        }
                    }
                    println!(
                        "BENCH mode={mode} op={} gc={gc_mode} trial={trial} count={n} ns_per_op={:.1} bytes_per_op={} allocations={delta:?}",
                        String::from_utf8_lossy(selector),
                        nanos as f64 / n as f64,
                        if gc_mode == "off" {
                            (bytes_after - bytes_before) / n
                        } else {
                            0
                        }
                    );
                }
            }
        }
        release(id, roots, rt);
        release(id, roots, lt);
    }
}
