//! Optimized test-only benchmark for exact-address membership predicates.

use core::ffi::c_void;
use std::hint::black_box;
use std::time::Instant;

use crate::abi::Oop;
use crate::heap::{Heap, Permanence};
use crate::raw::MethodAddress;
use crate::vtable::Vtables;

pub(crate) const CASE_COUNT: usize = 10;
pub(crate) const POPULATION_COUNT: usize = 3;
const PROBE_COUNT: usize = 1024;
const ITERATIONS: usize = 1 << 19;
const REPETITIONS: usize = 5;
const POPULATIONS: [usize; POPULATION_COUNT] = [1024, 16_384, 131_072];

#[derive(Clone, Copy)]
#[repr(C)]
pub(crate) struct BenchSample {
    pub(crate) live_objects: usize,
    pub(crate) vtables: usize,
    pub(crate) raw_ns_per_call: f64,
    pub(crate) loop_ns_per_call: f64,
    pub(crate) ns_per_call: f64,
}

impl BenchSample {
    const ZERO: Self = Self {
        live_objects: 0,
        vtables: 0,
        raw_ns_per_call: 0.0,
        loop_ns_per_call: 0.0,
        ns_per_call: 0.0,
    };
}

#[repr(C)]
pub(crate) struct BenchReport {
    pub(crate) schema: usize,
    pub(crate) iterations: usize,
    pub(crate) repetitions: usize,
    pub(crate) case_count: usize,
    pub(crate) population_count: usize,
    pub(crate) samples: [[BenchSample; POPULATION_COUNT]; CASE_COUNT],
    pub(crate) scaling_exponents: [f64; CASE_COUNT],
}

#[derive(Clone, Copy)]
#[repr(usize)]
enum Case {
    ObjectMembership = 0,
    VtableOf = 1,
    VtableOfMethod = 2,
    VtableOfForeign = 3,
    VtableOwner = 4,
    VtableOwnerMethod = 5,
    VtableOwnerForeign = 6,
    IsSmallInt = 7,
    IsFloat = 8,
    IsWord = 9,
}

const CASES: [Case; CASE_COUNT] = [
    Case::ObjectMembership,
    Case::VtableOf,
    Case::VtableOfMethod,
    Case::VtableOfForeign,
    Case::VtableOwner,
    Case::VtableOwnerMethod,
    Case::VtableOwnerForeign,
    Case::IsSmallInt,
    Case::IsFloat,
    Case::IsWord,
];

struct Probes {
    objects: [Oop; PROBE_COUNT],
    vtables: [Oop; PROBE_COUNT],
    foreign: [Oop; PROBE_COUNT],
    integers: [Oop; PROBE_COUNT],
    floats: [Oop; PROBE_COUNT],
    words: [Oop; PROBE_COUNT],
    smallint_vt: Oop,
    float_vt: Oop,
    word_vt: Oop,
    method: Oop,
}

unsafe extern "C" fn foreign_method(_: Oop, _: *const Oop, _: usize) -> Oop {
    core::ptr::null_mut()
}

fn run_operation(case: Case, heap: &Heap, vtables: &Vtables, probes: &Probes) -> usize {
    let mut result = 0usize;
    for iteration in 0..ITERATIONS {
        let probe = iteration & (PROBE_COUNT - 1);
        result = result.wrapping_add(match case {
            Case::ObjectMembership => usize::from(heap.contains(black_box(probes.objects[probe]))),
            Case::VtableOf => {
                Vtables::checked_vtable_of(heap, black_box(probes.objects[probe])).addr()
            }
            Case::VtableOfMethod => {
                Vtables::checked_vtable_of(heap, black_box(probes.method)).addr()
            }
            Case::VtableOfForeign => {
                Vtables::checked_vtable_of(heap, black_box(probes.foreign[probe])).addr()
            }
            Case::VtableOwner => vtables.owner_of(black_box(probes.vtables[probe])).addr(),
            Case::VtableOwnerMethod => vtables.owner_of(black_box(probes.method)).addr(),
            Case::VtableOwnerForeign => vtables.owner_of(black_box(probes.foreign[probe])).addr(),
            Case::IsSmallInt => usize::from(
                Vtables::checked_vtable_of(heap, black_box(probes.integers[probe]))
                    == probes.smallint_vt,
            ),
            Case::IsFloat => usize::from(
                Vtables::checked_vtable_of(heap, black_box(probes.floats[probe]))
                    == probes.float_vt,
            ),
            Case::IsWord => usize::from(
                Vtables::checked_vtable_of(heap, black_box(probes.words[probe])) == probes.word_vt,
            ),
        });
    }
    black_box(result)
}

fn run_loop(probes: &Probes) -> usize {
    let mut result = 0usize;
    for iteration in 0..ITERATIONS {
        result =
            result.wrapping_add(black_box(probes.objects[iteration & (PROBE_COUNT - 1)]).addr());
    }
    black_box(result)
}

fn elapsed_ns(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

fn median(mut values: [u64; REPETITIONS]) -> u64 {
    values.sort_unstable();
    values[REPETITIONS / 2]
}

fn measure(case: Case, heap: &Heap, vtables: &Vtables, probes: &Probes) -> BenchSample {
    let mut raw = [0u64; REPETITIONS];
    let mut loop_only = [0u64; REPETITIONS];
    let mut net = [0u64; REPETITIONS];

    let _ = run_operation(case, heap, vtables, probes);
    for repetition in 0..REPETITIONS {
        let loop_start = Instant::now();
        let _ = run_loop(probes);
        loop_only[repetition] = elapsed_ns(loop_start);

        let raw_start = Instant::now();
        let _ = run_operation(case, heap, vtables, probes);
        raw[repetition] = elapsed_ns(raw_start);
        net[repetition] = raw[repetition].saturating_sub(loop_only[repetition]);
    }

    let divisor = ITERATIONS as f64;
    BenchSample {
        live_objects: heap.allocation_count(),
        vtables: vtables.count(),
        raw_ns_per_call: median(raw) as f64 / divisor,
        loop_ns_per_call: median(loop_only) as f64 / divisor,
        ns_per_call: median(net) as f64 / divisor,
    }
}

fn scaling_exponent(first: BenchSample, last: BenchSample) -> f64 {
    if first.ns_per_call <= 0.0 || last.ns_per_call <= 0.0 {
        return 0.0;
    }
    let population_ratio = last.live_objects as f64 / first.live_objects as f64;
    (last.ns_per_call / first.ns_per_call).ln() / population_ratio.ln()
}

pub(crate) fn run() -> BenchReport {
    let mut heap = Heap::new();
    let mut vtables = Vtables::new();
    let meta = vtables.create(&mut heap, core::ptr::null_mut(), core::ptr::null_mut());
    boundary_set_vtable(&mut heap, meta, meta);
    let root = vtables.create(&mut heap, core::ptr::null_mut(), meta);
    let smallint_vt = vtables.create(&mut heap, root, meta);
    let float_vt = vtables.create(&mut heap, root, meta);
    let word_vt = vtables.create(&mut heap, root, meta);

    let mut foreign_words = [0usize; PROBE_COUNT * 2];
    let mut probes = Probes {
        objects: [core::ptr::null_mut(); PROBE_COUNT],
        vtables: [core::ptr::null_mut(); PROBE_COUNT],
        foreign: [core::ptr::null_mut(); PROBE_COUNT],
        integers: [core::ptr::null_mut(); PROBE_COUNT],
        floats: [core::ptr::null_mut(); PROBE_COUNT],
        words: [core::ptr::null_mut(); PROBE_COUNT],
        smallint_vt,
        float_vt,
        word_vt,
        method: MethodAddress::from_method(foreign_method).as_oop(),
    };
    for index in 0..PROBE_COUNT {
        let vtable = vtables.create(&mut heap, root, meta);
        let object = heap.allocate(size_of::<usize>(), Permanence::Collectable);
        boundary_set_vtable(&mut heap, object, vtable);
        vtables.set_owner(vtable, object);
        probes.objects[index] = object;
        probes.vtables[index] = vtable;
        probes.foreign[index] = core::ptr::from_mut(&mut foreign_words[index * 2]).cast::<c_void>();
        for (target, family) in [
            (&mut probes.integers[index], smallint_vt),
            (&mut probes.floats[index], float_vt),
            (&mut probes.words[index], word_vt),
        ] {
            let scalar = heap.allocate(size_of::<u64>(), Permanence::Collectable);
            boundary_set_vtable(&mut heap, scalar, family);
            *target = scalar;
        }
    }

    let mut report = BenchReport {
        schema: 2,
        iterations: ITERATIONS,
        repetitions: REPETITIONS,
        case_count: CASE_COUNT,
        population_count: POPULATION_COUNT,
        samples: [[BenchSample::ZERO; POPULATION_COUNT]; CASE_COUNT],
        scaling_exponents: [0.0; CASE_COUNT],
    };
    let mut ring = 0usize;
    for (population_index, population) in POPULATIONS.into_iter().enumerate() {
        while vtables.count() < population {
            let vtable = vtables.create(&mut heap, root, meta);
            let object = heap.allocate(size_of::<usize>(), Permanence::Collectable);
            boundary_set_vtable(&mut heap, object, vtable);
            vtables.set_owner(vtable, object);
            probes.objects[ring] = object;
            probes.vtables[ring] = vtable;
            ring = (ring + 1) & (PROBE_COUNT - 1);
        }
        for case in CASES {
            report.samples[case as usize][population_index] =
                measure(case, &heap, &vtables, &probes);
        }
    }
    for case in CASES {
        report.scaling_exponents[case as usize] = scaling_exponent(
            report.samples[case as usize][0],
            report.samples[case as usize][POPULATION_COUNT - 1],
        );
    }
    report
}

fn boundary_set_vtable(heap: &mut Heap, object: Oop, vtable: Oop) {
    crate::boundary::require_invariant(
        heap.set_vtable(object, vtable),
        "benchmark object was not registered",
    );
}
