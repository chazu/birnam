use crate::symbol::*;
use crate::abi::Oop;
#[cfg(test)]
mod tests {
    use crate::heap::{Heap, Permanence};

    use super::*;
    use crate::behaviors::Behaviors;
    use std::ffi::CStr;

    fn symbol_harness() -> (Heap, Oop) {
        let mut heap = Heap::new();
        let mut vtables = Behaviors::new();
        let meta = vtables.create(&mut heap, core::ptr::null_mut(), core::ptr::null_mut());
        assert!(heap.set_vtable(meta, meta));
        let symbol_vtable = vtables.create(&mut heap, core::ptr::null_mut(), meta);
        (heap, symbol_vtable)
    }

    #[test]
    fn repeated_equal_bytes_return_one_live_collectable_identity() {
        let (mut heap, symbol_vtable) = symbol_harness();
        let mut symbols = crate::id_registries::Symbols::Local(crate::owned::Owned::new(Symbols::new()));
        let first = heap.intern_symbol(&mut symbols, symbol_vtable, b"lookup").unwrap();
        let second = heap.intern_symbol(&mut symbols, symbol_vtable, b"lookup").unwrap();
        assert_eq!(first, second);
        assert_eq!(symbols.count(), 1);
        assert_eq!(symbols.at(0), first);
        assert!(symbols.at(1).is_null());
        assert_eq!(heap.permanence_of(first), Some(Permanence::Collectable));
        assert_eq!(Behaviors::checked_vtable_of(&heap, first), symbol_vtable);
    }

    #[test]
    fn arbitrary_non_nul_bytes_never_require_utf8() {
        let (mut heap, symbol_vtable) = symbol_harness();
        let mut symbols = crate::id_registries::Symbols::Local(crate::owned::Owned::new(Symbols::new()));
        let cases: [&[u8]; 6] = [
            b"",
            b"ascii",
            b"doesNotUnderstand:",
            b"at:put:",
            b"!@#$%^&*",
            &[0xff, 0xfe, b':'],
        ];
        for bytes in cases {
            let symbol = heap.intern_symbol(&mut symbols, symbol_vtable, bytes).unwrap();
            assert_eq!(symbols.bytes_of(symbol), Some(bytes));
            // CStr validates only the terminating NUL; no UTF-8 conversion is
            // attempted for the non-UTF-8 selector.
            let c_name = symbols.c_name_of(symbol);
            // SAFETY: each record retains a stable NUL-terminated boxed name.
            assert_eq!(unsafe { CStr::from_ptr(c_name) }.to_bytes(), bytes);
        }
        assert_eq!(symbols.count(), cases.len());
        assert_eq!(
            heap.intern_symbol(&mut symbols, symbol_vtable, b"bad\0name"),
            Err(crate::id_registries::EmbeddedNul)
        );
    }

    #[test]
    fn enumeration_order_and_rooted_c_name_pointers_survive_unrelated_growth() {
        let (mut heap, symbol_vtable) = symbol_harness();
        let mut symbols = crate::id_registries::Symbols::Local(crate::owned::Owned::new(Symbols::new()));
        let anchor = heap.intern_symbol(&mut symbols, symbol_vtable, b"anchor:")
            .unwrap();
        let anchor_pointer = symbols.c_name_of(anchor);

        let mut expected = vec![anchor];
        let count = if cfg!(miri) { 128 } else { 4096 };
        for index in 0..count {
            let name = format!("symbol-{index}");
            let symbol = heap.intern_symbol(&mut symbols, symbol_vtable, name.as_bytes())
                .unwrap();
            expected.push(symbol);
            let _ = heap.allocate(index as usize & 31, Permanence::Collectable);
        }

        assert_eq!(symbols.count(), expected.len());
        for (index, expected_symbol) in expected.into_iter().enumerate() {
            assert_eq!(symbols.at(index), expected_symbol);
        }
        assert_eq!(symbols.c_name_of(anchor), anchor_pointer);
        // SAFETY: unrelated Vec/Heap growth cannot move the boxed name bytes.
        assert_eq!(
            unsafe { CStr::from_ptr(anchor_pointer) }.to_bytes(),
            b"anchor:"
        );
    }

    #[test]
    fn sweep_removes_dead_identity_and_reinterning_creates_a_fresh_oop() {
        use crate::gc::trace_conservative_for_test;

        let (mut heap, symbol_vtable) = symbol_harness();
        let mut symbols = crate::id_registries::Symbols::Local(crate::owned::Owned::new(Symbols::new()));
        let rooted = heap.intern_symbol(&mut symbols, symbol_vtable, b"rooted").unwrap();
        let first_dead = heap.intern_symbol(&mut symbols, symbol_vtable, b"ephemeral")
            .unwrap();
        assert_eq!(symbols.owned_name_bytes(), b"rooted\0ephemeral\0".len());

        heap.clear_marks();
        trace_conservative_for_test(&mut heap, &[rooted]);
        assert_eq!(symbols.remove_objects(&heap.dead_collectable_addresses()), 1);
        assert_eq!(symbols.count(), 1);
        assert_eq!(symbols.owned_name_bytes(), b"rooted\0".len());
        assert_eq!(symbols.at(0), rooted);
        assert_eq!(symbols.bytes_of(first_dead), None);
        assert!(symbols.c_name_of(first_dead).is_null());

        let second = heap.intern_symbol(&mut symbols, symbol_vtable, b"ephemeral")
            .unwrap();
        assert_ne!(second, first_dead);
        assert_eq!(symbols.bytes_of(second), Some(&b"ephemeral"[..]));
    }

    #[test]
    fn colliding_names_survive_middle_compaction_and_all_dead_cycles() {
        use crate::gc::trace_conservative_for_test;

        let (mut heap, vt) = symbol_harness();
        let mut symbols = crate::id_registries::Symbols::Local(crate::owned::Owned::new(Symbols::new()));
        let first = heap.intern_symbol_hashed(&mut symbols, vt, b"first", 7);
        let middle = heap.intern_symbol_hashed(&mut symbols, vt, b"middle", 7);
        let last = heap.intern_symbol_hashed(&mut symbols, vt, b"last", 7);
        let middle_name = symbols.c_name_of(middle);
        assert_eq!(heap.intern_symbol_hashed(&mut symbols, vt, b"first", 7), first);
        assert_eq!(heap.intern_symbol_hashed(&mut symbols, vt, b"middle", 7), middle);
        assert_eq!(heap.intern_symbol_hashed(&mut symbols, vt, b"last", 7), last);
        assert_eq!(symbols.local().find_hashed_for_test(b"absent", 7), None);

        heap.clear_marks();
        trace_conservative_for_test(&mut heap, &[middle, last]);
        assert_eq!(symbols.remove_objects(&heap.dead_collectable_addresses()), 1);
        assert_eq!(symbols.at(0), middle);
        assert_eq!(symbols.at(1), last);
        assert_eq!(symbols.c_name_of(middle), middle_name);
        assert_eq!(symbols.local().find_hashed_for_test(b"first", 7), None);
        assert_eq!(symbols.local().find_hashed_for_test(b"middle", 7), Some(middle));
        assert_eq!(symbols.local().find_hashed_for_test(b"last", 7), Some(last));
        assert_eq!(symbols.owned_name_bytes(), b"middle\0last\0".len());

        let grown = heap.intern_symbol_hashed(&mut symbols, vt, b"grown", 7);
        assert_eq!(symbols.at(2), grown);
        assert_eq!(symbols.local().find_hashed_for_test(b"middle", 7), Some(middle));
        heap.clear_marks();
        assert_eq!(symbols.remove_objects(&heap.dead_collectable_addresses()), 3);
        assert_eq!(symbols.owned_name_bytes(), 0);
        assert_eq!(symbols.local().find_hashed_for_test(b"middle", 7), None);
        assert!(symbols.bytes_of(middle).is_none());
        let recreated = heap.intern_symbol_hashed(&mut symbols, vt, b"middle", 7);
        assert_ne!(recreated, middle);
        assert_eq!(symbols.at(0), recreated);
        assert_eq!(symbols.local().find_hashed_for_test(b"middle", 7), Some(recreated));
    }

    #[test]
    fn indexed_lookup_is_runtime_local_and_rebuilds_after_sweep() {
        use crate::gc::trace_conservative_for_test;

        let (mut heap, vt) = symbol_harness();
        let (mut other_heap, other_vt) = symbol_harness();
        let mut symbols = crate::id_registries::Symbols::Local(crate::owned::Owned::new(Symbols::new()));
        let mut other = crate::id_registries::Symbols::Local(crate::owned::Owned::new(Symbols::new()));
        let dead = heap.intern_symbol(&mut symbols, vt, b"dead").unwrap();
        let live = heap.intern_symbol(&mut symbols, vt, b"live").unwrap();
        let independent = other_heap.intern_symbol(&mut other, other_vt, b"live").unwrap();
        assert_ne!(live, independent);
        assert!(symbols.contains_bytes(b"dead"));
        assert!(!symbols.contains_bytes(b"live\0"));
        heap.clear_marks();
        trace_conservative_for_test(&mut heap, &[live]);
        assert_eq!(symbols.remove_objects(&heap.dead_collectable_addresses()), 1);
        assert!(!symbols.contains_bytes(b"dead"));
        assert!(symbols.contains_bytes(b"live"));
        assert_eq!(heap.intern_symbol(&mut symbols, vt, b"live").unwrap(), live);
        assert_eq!(
            other_heap.intern_symbol(&mut other, other_vt, b"live").unwrap(),
            independent
        );
        assert!(symbols.bytes_of(dead).is_none());
        let count = heap.allocation_count();
        for _ in 0..128 {
            assert_eq!(heap.intern_symbol(&mut symbols, vt, b"live").unwrap(), live);
        }
        assert_eq!(heap.allocation_count(), count);
    }

    #[test]
    #[ignore = "diagnostic benchmark; run with --release --ignored --nocapture"]
    fn symbol_lookup_scaling_benchmark() {
        use std::hint::black_box;
        use std::time::Instant;

        for population in [1000, 10_000, 100_000] {
            let (mut heap, vt) = symbol_harness();
            let mut symbols = crate::id_registries::Symbols::Local(crate::owned::Owned::new(Symbols::new()));
            for index in 0..population {
                heap.intern_symbol(&mut symbols, vt, format!("name-{index:06}").as_bytes())
                    .unwrap();
            }
            let last = format!("name-{:06}", population - 1);
            for (kind, bytes) in [("hit", last.as_bytes()), ("miss", &b"name-absent"[..])] {
                for strategy in ["indexed", "linear"] {
                    let mut samples = [0.0f64; 3];
                    for sample in &mut samples {
                        let started = Instant::now();
                        for _ in 0..10_000 {
                            let result = if strategy == "indexed" {
                                symbols.contains_bytes(black_box(bytes))
                            } else {
                                symbols.local().contains_bytes_linearly_for_test(black_box(bytes))
                            };
                            black_box(result);
                        }
                        *sample = started.elapsed().as_nanos() as f64 / 10_000.0;
                    }
                    samples.sort_by(f64::total_cmp);
                    println!(
                        "symbols={population} kind={kind} strategy={strategy} median_ns={:.2}",
                        samples[1]
                    );
                }
            }
        }
    }

    #[test]
    fn foreign_carriers_are_not_symbols() {
        let (mut heap, symbol_vtable) = symbol_harness();
        let mut symbols = crate::id_registries::Symbols::Local(crate::owned::Owned::new(Symbols::new()));
        let _ = heap.intern_symbol(&mut symbols, symbol_vtable, b"real").unwrap();
        let mut foreign = 0usize;
        let foreign: Oop = core::ptr::from_mut(&mut foreign).cast();
        assert_eq!(symbols.bytes_of(core::ptr::null_mut()), None);
        assert_eq!(symbols.bytes_of(foreign), None);
        assert!(symbols.c_name_of(foreign).is_null());
    }
}
