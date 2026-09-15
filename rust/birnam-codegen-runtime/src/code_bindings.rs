//! CodeBinding admission, publication, retirement and rooted semantic/native owners.
use super::*;

static CODE_BINDINGS: Mutex<Vec<CodeBindingEntry>> = Mutex::new(Vec::new());

const MAX_CURRENT_CODE_BINDINGS: usize = 262_144;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CodeBindingLifecycle {
    Staging,
    Current,
    Retiring,
}

/// Process-local carrier for the current semantic method association and its
/// physical NamedAdapter. Durable image-v8 meaning excludes `unit`,
/// `generation`, `method`, and `lifecycle`; those fields only keep native code
/// attributed until resolve-and-pin quiescence permits retirement.
struct CodeBindingEntry {
    target: usize,
    vtable: usize,
    selector: usize,
    implementation: usize,
    provenance: usize,
    unit: usize,
    generation: u64,
    method: usize,
    authority: [u8; 32],
    generated_bytes: usize,
    lifecycle: CodeBindingLifecycle,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn stage_code_binding(
    target: Oop,
    vtable: Oop,
    selector: Oop,
    implementation: Oop,
    provenance: Oop,
    unit: Oop,
    generation: u64,
    authority: [u8; 32],
    generated_bytes: usize,
) -> bool {
    let Ok(mut bindings) = CODE_BINDINGS.lock() else {
        return false;
    };
    let current_count = bindings
        .iter()
        .filter(|binding| binding.lifecycle == CodeBindingLifecycle::Current)
        .count();
    let replacing = bindings.iter().any(|binding| {
        binding.lifecycle == CodeBindingLifecycle::Current
            && binding.target == target as usize
            && binding.selector == selector as usize
    });
    if (current_count >= MAX_CURRENT_CODE_BINDINGS && !replacing)
        || bindings.iter().any(|binding| {
            binding.lifecycle == CodeBindingLifecycle::Staging
                && binding.target == target as usize
                && binding.selector == selector as usize
        })
    {
        return false;
    }
    bindings.push(CodeBindingEntry {
        target: target as usize,
        vtable: vtable as usize,
        selector: selector as usize,
        implementation: implementation as usize,
        provenance: provenance as usize,
        unit: unit as usize,
        generation,
        method: 0,
        authority,
        generated_bytes,
        lifecycle: CodeBindingLifecycle::Staging,
    });
    true
}

pub(super) fn set_staged_code_binding_method(unit: Oop, method: usize) -> bool {
    let Ok(mut bindings) = CODE_BINDINGS.lock() else {
        return false;
    };
    let Some(binding) = bindings.iter_mut().find(|binding| {
        binding.unit == unit as usize && binding.lifecycle == CodeBindingLifecycle::Staging
    }) else {
        return false;
    };
    if method == 0 {
        return false;
    }
    binding.method = method;
    true
}

pub(super) fn remove_staged_code_binding(unit: Oop) {
    if let Ok(mut bindings) = CODE_BINDINGS.lock() {
        bindings.retain(|binding| {
            binding.unit != unit as usize || binding.lifecycle != CodeBindingLifecycle::Staging
        });
    }
}

pub(super) fn retire_staged_code_binding(unit: Oop) -> Option<u64> {
    let mut bindings = CODE_BINDINGS.lock().ok()?;
    let binding = bindings.iter_mut().find(|binding| {
        binding.unit == unit as usize && binding.lifecycle == CodeBindingLifecycle::Staging
    })?;
    binding.lifecycle = CodeBindingLifecycle::Retiring;
    Some(binding.generation)
}

pub(super) fn publish_staged_code_binding(unit: Oop) -> Option<Vec<u64>> {
    let mut bindings = CODE_BINDINGS.lock().ok()?;
    let staged_index = bindings.iter().position(|binding| {
        binding.unit == unit as usize
            && binding.lifecycle == CodeBindingLifecycle::Staging
            && binding.method != 0
    })?;
    let target = bindings[staged_index].target;
    let selector = bindings[staged_index].selector;
    let mut predecessors = Vec::new();
    for (index, binding) in bindings.iter_mut().enumerate() {
        if index != staged_index
            && binding.lifecycle == CodeBindingLifecycle::Current
            && binding.target == target
            && binding.selector == selector
        {
            binding.lifecycle = CodeBindingLifecycle::Retiring;
            predecessors.push(binding.generation);
        }
    }
    bindings[staged_index].lifecycle = CodeBindingLifecycle::Current;
    Some(predecessors)
}

pub(super) fn withdraw_current_code_binding(target: Oop, selector: Oop) -> Vec<u64> {
    let Ok(mut bindings) = CODE_BINDINGS.lock() else {
        return Vec::new();
    };
    let mut generations = Vec::new();
    for binding in bindings.iter_mut() {
        if binding.lifecycle == CodeBindingLifecycle::Current
            && binding.target == target as usize
            && binding.selector == selector as usize
        {
            binding.lifecycle = CodeBindingLifecycle::Retiring;
            generations.push(binding.generation);
        }
    }
    generations
}

pub(super) fn update_current_code_binding_provenance(target: Oop, selector: Oop, provenance: Oop) -> bool {
    let Ok(mut bindings) = CODE_BINDINGS.lock() else {
        return false;
    };
    let Some(binding) = bindings.iter_mut().find(|binding| {
        binding.lifecycle == CodeBindingLifecycle::Current
            && binding.target == target as usize
            && binding.selector == selector as usize
    }) else {
        return false;
    };
    binding.provenance = provenance as usize;
    true
}

pub(super) fn current_code_binding_implementation(vtable: Oop, selector: Oop) -> Option<Oop> {
    CODE_BINDINGS.lock().ok().and_then(|bindings| {
        bindings
            .iter()
            .find(|binding| {
                binding.lifecycle == CodeBindingLifecycle::Current
                    && binding.vtable == vtable as usize
                    && binding.selector == selector as usize
            })
            .map(|binding| binding.implementation as Oop)
    })
}

pub(super) unsafe fn request_code_binding_retirements(api: &IdApi, generations: &[u64]) {
    for generation in generations.iter().copied() {
        let _ = unsafe { retire_runtime_generation_v1(api, generation) };
    }
}

pub(super) unsafe fn reap_code_bindings(api: &IdApi) -> usize {
    let generations = CODE_BINDINGS
        .lock()
        .ok()
        .map(|bindings| {
            bindings
                .iter()
                .filter(|binding| binding.lifecycle == CodeBindingLifecycle::Retiring)
                .map(|binding| binding.generation)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut retired = Vec::new();
    for generation in generations {
        if unsafe { retire_runtime_generation_v1(api, generation) } == RuntimeRetirement::Retired {
            retired.push(generation);
        }
    }
    if retired.is_empty() {
        return 0;
    }
    let removed = CODE_BINDINGS
        .lock()
        .ok()
        .map(|mut bindings| {
            let before = bindings.len();
            bindings.retain(|binding| {
                binding.lifecycle != CodeBindingLifecycle::Retiring
                    || !retired.contains(&binding.generation)
            });
            before - bindings.len()
        })
        .unwrap_or(0);
    // Removed rows release their disposable Unit Oops. Collection runs only
    // after Runtime has forgotten every native address in those generations.
    let _ = unsafe { (api.gcCollect)() };
    removed
}

pub(super) unsafe extern "C" fn rust_codegen_current_code_binding_count(
    _self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen currentCodeBindingCount expects no arguments");
    }
    let value = CODE_BINDINGS
        .lock()
        .map(|bindings| {
            bindings
                .iter()
                .filter(|binding| {
                    binding.lifecycle == CodeBindingLifecycle::Current
                        && binding.authority != [0; 32]
                })
                .count()
        })
        .unwrap_or(0);
    let api = unsafe { api(runtime()) };
    unsafe { (api.mkSmallInt)(value as i64) }
}

pub(super) unsafe extern "C" fn rust_codegen_retiring_code_binding_count(
    _self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen retiringCodeBindingCount expects no arguments");
    }
    let value = CODE_BINDINGS
        .lock()
        .map(|bindings| {
            bindings
                .iter()
                .filter(|binding| binding.lifecycle == CodeBindingLifecycle::Retiring)
                .count()
        })
        .unwrap_or(0);
    let api = unsafe { api(runtime()) };
    unsafe { (api.mkSmallInt)(value as i64) }
}

pub(super) unsafe extern "C" fn rust_codegen_code_binding_generated_bytes(
    _self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen codeBindingGeneratedBytes expects no arguments");
    }
    let value = CODE_BINDINGS
        .lock()
        .ok()
        .and_then(|bindings| {
            let mut generations = BTreeMap::<u64, usize>::new();
            for binding in bindings.iter() {
                generations
                    .entry(binding.generation)
                    .and_modify(|bytes| *bytes = (*bytes).max(binding.generated_bytes))
                    .or_insert(binding.generated_bytes);
            }
            generations
                .into_values()
                .try_fold(0usize, usize::checked_add)
        })
        .unwrap_or(usize::MAX);
    let api = unsafe { api(runtime()) };
    i64::try_from(value)
        .ok()
        .map(|value| unsafe { (api.mkSmallInt)(value) })
        .unwrap_or(api.nilObj)
}

pub(super) unsafe extern "C" fn rust_codegen_reap_code_bindings(
    _self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen reapCodeBindings expects no arguments");
    }
    let api = unsafe { api(runtime()) };
    let removed = unsafe { reap_code_bindings(api) };
    unsafe { (api.mkSmallInt)(removed as i64) }
}

/// Return a point-in-time semantic snapshot of the current CodeBindings.
///
/// This is a private K5 image-writer seam, not public compilation policy. Each
/// row is `[target, selector, implementation, provenance]`; publication
/// generations, Methods, units, and other process-local carrier state are
/// deliberately absent. Automatic collection is disabled while copied Oops are
/// between the rooted registry and the returned rooted Vector graph.
pub(super) unsafe extern "C" fn rust_codegen_portable_image_bindings(
    _self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen portableImageBindings expects no arguments");
    }
    let api = unsafe { api(runtime()) };
    unsafe { (api.gcSetAuto)(0) };
    struct AutoGc<'a>(&'a IdApi);
    impl Drop for AutoGc<'_> {
        fn drop(&mut self) {
            unsafe { (self.0.gcSetAuto)(1) };
        }
    }
    let _auto_gc = AutoGc(api);
    let rows = CODE_BINDINGS
        .lock()
        .ok()
        .map(|bindings| {
            bindings
                .iter()
                .filter(|binding| binding.lifecycle == CodeBindingLifecycle::Current)
                .map(|binding| {
                    [
                        binding.target as Oop,
                        binding.selector as Oop,
                        binding.implementation as Oop,
                        binding.provenance as Oop,
                    ]
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let vector = unsafe { (api.import)(VECTOR_NAME.as_ptr().cast()) };
    if vector.is_null() || vector == api.nilObj {
        return api.nilObj;
    }
    let mut encoded = Vec::with_capacity(rows.len());
    for row in rows {
        let row = unsafe { make_vector(api, vector, &row) };
        if row.is_null() || row == api.nilObj {
            return api.nilObj;
        }
        encoded.push(row);
    }
    unsafe { make_vector(api, vector, &encoded) }
}

pub(super) fn reserve_image_binding(target: Oop, selector: Oop) -> bool {
        let Ok(mut bindings) = CODE_BINDINGS.lock() else {
            return false;
        };
        let current_count = bindings
            .iter()
            .filter(|binding| binding.lifecycle == CodeBindingLifecycle::Current)
            .count();
        let replacing = bindings.iter().any(|binding| {
            binding.lifecycle == CodeBindingLifecycle::Current
                && binding.target == target as usize
                && binding.selector == selector as usize
        });
        // Image recovery runs on the Runtime owner thread, so no competing
        // publisher can consume this reservation between the two lock scopes.
        // Keep every fallible registry operation ahead of Store publication.
        if (current_count >= MAX_CURRENT_CODE_BINDINGS && !replacing)
            || bindings.try_reserve(1).is_err()
        {
            return false;
        }
    true
}

#[allow(clippy::too_many_arguments)]
pub(super) fn publish_image_binding(
    target: Oop, vtable: Oop, selector: Oop, implementation: Oop, provenance: Oop,
    owner: usize, generation: u64, method: usize, authority: [u8; 32], generated_bytes: usize,
) -> Vec<u64> {
        let Ok(mut bindings) = CODE_BINDINGS.lock() else {
            abort("portable image binding registry became unavailable after Store publication");
        };
        let mut predecessors = Vec::new();
        for binding in bindings.iter_mut() {
            if binding.lifecycle == CodeBindingLifecycle::Current
                && binding.target == target as usize
                && binding.selector == selector as usize
            {
                binding.lifecycle = CodeBindingLifecycle::Retiring;
                predecessors.push(binding.generation);
            }
        }
        bindings.push(CodeBindingEntry {
            target: target as usize,
            vtable: vtable as usize,
            selector: selector as usize,
            implementation: implementation as usize,
            provenance: provenance as usize,
            unit: owner,
            generation,
            method,
            authority,
            generated_bytes,
            lifecycle: CodeBindingLifecycle::Current,
        });
        predecessors
}

pub(super) unsafe fn mark_roots(mark: MarkFn) {
    if let Ok(bindings) = CODE_BINDINGS.lock() {
        for binding in bindings.iter() {
            // Current semantic bindings retain target, selector,
            // implementation, provenance, and native owner. Retiring rows
            // retain the same graph only until Runtime generation quiescence;
            // staging rows make publication allocation-safe.
            for value in [
                binding.target,
                binding.vtable,
                binding.selector,
                binding.implementation,
                binding.provenance,
                binding.unit,
            ] {
                if value != 0 {
                    unsafe { mark(value as Oop) };
                }
            }
        }
    }
}
