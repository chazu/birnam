//! Bounded materialization leases, admission, eviction and their root segment.
//! Live Blocks retain their own Unit carriers after a cache lease is removed.
use super::*;

static MATERIALIZATIONS: Mutex<Vec<MaterializationEntry>> = Mutex::new(Vec::new());

static NEXT_MATERIALIZATION_AGE: AtomicU64 = AtomicU64::new(1);

const MAX_MATERIALIZATION_CACHE_ENTRIES: usize = 64;

const MAX_MATERIALIZATION_CACHE_PROGRAM_BYTES: usize = 64 << 20;

pub(super) const MAX_MATERIALIZATION_C_SOURCE_BYTES: usize = 128 << 20;

struct MaterializationEntry {
    disposition: MaterializationDisposition,
    digest: [u8; 32],
    program: Vec<u8>,
    vtable: usize,
    unit: usize,
    generated_bytes: usize,
    age: u64,
    ready: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MaterializationDisposition {
    Synchronous,
    Resumable,
}

fn next_materialization_age() -> u64 {
    NEXT_MATERIALIZATION_AGE.fetch_add(1, Ordering::AcqRel)
}

pub(super) fn cached_materialization(
    disposition: MaterializationDisposition,
    digest: [u8; 32],
    program: &[u8],
) -> Option<(Oop, Oop)> {
    let mut entries = MATERIALIZATIONS.lock().ok()?;
    let entry = entries.iter_mut().find(|entry| {
        entry.ready
            && entry.disposition == disposition
            && entry.digest == digest
            && entry.program == program
    })?;
    entry.age = next_materialization_age();
    Some((entry.vtable as Oop, entry.unit as Oop))
}

pub(super) fn stage_materialization(
    disposition: MaterializationDisposition,
    digest: [u8; 32],
    program: Vec<u8>,
    unit: Oop,
    generated_bytes: usize,
) -> bool {
    let Ok(mut entries) = MATERIALIZATIONS.lock() else {
        return false;
    };
    while entries.len() >= MAX_MATERIALIZATION_CACHE_ENTRIES
        || entries
            .iter()
            .try_fold(0usize, |total, entry| {
                total.checked_add(entry.program.len())
            })
            .and_then(|total| total.checked_add(program.len()))
            .is_none_or(|total| total > MAX_MATERIALIZATION_CACHE_PROGRAM_BYTES)
        || entries
            .iter()
            .try_fold(0usize, |total, entry| {
                total.checked_add(entry.generated_bytes)
            })
            .and_then(|total| total.checked_add(generated_bytes))
            .is_none_or(|total| total > MAX_MATERIALIZATION_C_SOURCE_BYTES)
    {
        let Some((index, _)) = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.ready)
            .min_by_key(|(_, entry)| entry.age)
        else {
            return false;
        };
        entries.remove(index);
    }
    entries.push(MaterializationEntry {
        disposition,
        digest,
        program,
        vtable: 0,
        unit: unit as usize,
        generated_bytes,
        age: next_materialization_age(),
        ready: false,
    });
    true
}

pub(super) fn set_staged_materialization_vtable(unit: Oop, vtable: Oop) -> bool {
    let Ok(mut entries) = MATERIALIZATIONS.lock() else {
        return false;
    };
    let Some(entry) = entries
        .iter_mut()
        .find(|entry| !entry.ready && entry.unit == unit as usize)
    else {
        return false;
    };
    entry.vtable = vtable as usize;
    true
}

pub(super) fn commit_staged_materialization(unit: Oop) -> bool {
    let Ok(mut entries) = MATERIALIZATIONS.lock() else {
        return false;
    };
    let Some(entry) = entries
        .iter_mut()
        .find(|entry| !entry.ready && entry.unit == unit as usize && entry.vtable != 0)
    else {
        return false;
    };
    entry.ready = true;
    entry.age = next_materialization_age();
    true
}

pub(super) fn remove_materialization(unit: Oop) {
    let removed = if let Ok(mut entries) = MATERIALIZATIONS.lock() {
        let before = entries.len();
        entries.retain(|entry| entry.unit != unit as usize);
        before != entries.len()
    } else {
        false
    };
    if removed {
        let _ = take_vtable_cache_roots(jit_code_generation_identity(unit));
    }
}

pub(super) unsafe extern "C" fn rust_codegen_materialization_count(
    _self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen materializationCount expects no arguments");
    }
    let value = MATERIALIZATIONS
        .lock()
        .map(|entries| entries.iter().filter(|entry| entry.ready).count())
        .unwrap_or(0);
    let _api = unsafe { api(runtime()) };
    unsafe { (birnam_abi::integer::make)(value as i64) }
}

pub(super) unsafe extern "C" fn rust_codegen_evict_materializations(
    _self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen evictMaterializations expects no arguments");
    }
    let (removed, generations) = MATERIALIZATIONS
        .lock()
        .map(|mut entries| {
            let before = entries.len();
            let mut generations = Vec::new();
            entries.retain(|entry| {
                if entry.ready {
                    generations.push((
                        entry.disposition,
                        jit_code_generation_identity(entry.unit as Oop),
                    ));
                    false
                } else {
                    true
                }
            });
            (before - entries.len(), generations)
        })
        .unwrap_or_default();
    // Repository eviction drops its own Unit lease. Materialized JIT Units
    // trace their vtable/Code caches for the lifetime of real Block owners;
    // those values retain identity and edits until the whole cycle dies.
    // The separate artifact cache cleanup below does not clear Unit caches.
    for (disposition, generation) in generations {
        let _ = take_vtable_cache_roots(generation);
        if disposition == MaterializationDisposition::Resumable {
            let api = unsafe { api(runtime()) };
            // A live Block makes this first attempt reject after preserving
            // the generation for a later GC safe-point retry. A quiescent unit
            // retires immediately. Either result leaves no repository root.
            let _ = unsafe { retire_runtime_generation_v1(api, generation) };
        }
    }
    let _api = unsafe { api(runtime()) };
    unsafe { (birnam_abi::integer::make)(removed as i64) }
}

pub(super) unsafe fn mark_roots(mark: MarkFn) {
    if let Ok(materializations) = MATERIALIZATIONS.lock() {
        for entry in materializations.iter() {
            // SAFETY: bounded ready entries form an explicit cache lease;
            // staging entries protect their unit and vtable across
            // allocation-capable setup. After eviction, live Block vtables
            // retain the same unit through their two-edge private carrier.
            unsafe {
                if entry.vtable != 0 {
                    mark(entry.vtable as Oop);
                }
                if entry.unit != 0 {
                    mark(entry.unit as Oop);
                }
            }
        }
    }
}
