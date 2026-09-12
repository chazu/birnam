//! Private native candidates derived from existing portable staging inputs.
//!
//! Ordinary artifacts register their unchanged baseline entry. Candidate
//! code and all its callbacks are owned inside this runtime implementation.

use super::{FrameDescriptor, Method, Oop, Runtime, with_active};
use birnam_artifact::{
    materialization_selection as selection, portable_inventory,
    portable_resumable_manifest as manifest,
};
use birnam_codegen::{resumable, resumable_c, resumable_descriptor};
use birnam_native_compiler::Unit;
use core::ffi::{c_char, c_void};
use std::collections::BTreeMap;
use std::ffi::{CStr, CString};
use std::rc::Rc;
use std::sync::{Arc, Weak};

const HOT_ENTRIES: usize = 64;
const MAX_RECORDS: usize = 256;
const MAX_CANDIDATE_BYTES: usize = 256 << 10;

#[derive(Default)]
pub(super) struct Registry {
    staged: Vec<Option<Rc<Prepared>>>,
    records: BTreeMap<usize, Record>,
}

struct Record {
    descriptor: Weak<FrameDescriptor>,
    pending: Option<Rc<Prepared>>,
    candidate: Option<Rc<Candidate>>,
    visits: usize,
    disabled: bool,
    leaf: Option<usize>,
    leaf_miss_epoch: Option<u64>,
}

struct Prepared {
    descriptor: Vec<u8>,
    source: CString,
    pcs: usize,
    leaf: Option<usize>,
}

struct Candidate {
    entries: Vec<Method>,
    _unit: Unit,
}

pub(super) struct Selected {
    pub(super) method: Method,
    _candidate: Rc<Candidate>,
}

#[cfg(test)]
thread_local! {
    static HITS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

impl Registry {
    pub(super) fn stage(
        &mut self,
        programs: &[u8],
        bindings: &[u8],
        selected: &[u8],
        manifest_bytes: &[u8],
        authority: [u8; 32],
    ) {
        self.staged.clear();
        let Ok(inventory) = portable_inventory::inspect(programs) else {
            return;
        };
        let Ok(selected) = selection::inspect(selected, programs, bindings) else {
            return;
        };
        let Ok(manifest) = manifest::inspect(manifest_bytes, authority, &selected) else {
            return;
        };
        // Optimization is optional and bounded, even for a much larger valid
        // unit. Registration still accepts every ordinary manifest row.
        for index in 0..manifest.entry_count().min(MAX_RECORDS) {
            let prepared = (|| {
                let entry = manifest.entry(index)?;
                if entry.lowering_version != resumable_descriptor::LOWERING_VERSION_V1 {
                    return None;
                }
                let row = selected.row(entry.selection_row_index as usize)?;
                if row.kind != selection::SubjectKind::BlockBody {
                    return None;
                }
                Prepared::new(inventory.program(row.subject_index as usize)?, row)
            })();
            self.staged.push(prepared.map(Rc::new));
        }
    }

    pub(super) fn end_stage(&mut self) {
        self.staged.clear();
    }

    pub(super) fn register(
        &mut self,
        index: usize,
        bytes: &[u8],
        descriptor: &Arc<FrameDescriptor>,
    ) {
        let Some(prepared) = self.staged.get(index).and_then(Option::as_ref).cloned() else {
            return;
        };
        if prepared.descriptor != bytes || prepared.pcs != descriptor.program_counter_count() {
            return;
        }
        self.prune();
        if self.records.len() >= MAX_RECORDS {
            self.records.pop_first();
        }
        self.records.insert(
            Arc::as_ptr(descriptor) as usize,
            Record {
                descriptor: Arc::downgrade(descriptor),
                leaf: prepared.leaf,
                leaf_miss_epoch: None,
                pending: Some(prepared),
                candidate: None,
                visits: 0,
                disabled: false,
            },
        );
    }

    pub(super) fn prune(&mut self) {
        self.records
            .retain(|_, record| record.descriptor.strong_count() != 0);
    }
}

impl Prepared {
    fn new(bytes: &[u8], row: selection::Row<'_>) -> Option<Self> {
        let program = resumable::lower_portable_block(bytes).ok()?;
        if program.blocks.is_empty() || program.blocks.len() > 4096 {
            return None;
        }
        for block in &program.blocks {
            for operation in &block.operations {
                match operation.kind {
                    resumable::OperationKind::EntryReceiver
                    | resumable::OperationKind::EntryArgument { .. }
                    | resumable::OperationKind::EntryCapture { .. }
                    | resumable::OperationKind::Bind { .. }
                    | resumable::OperationKind::Literal(resumable::Literal::SmallInt(_))
                    | resumable::OperationKind::Literal(resumable::Literal::Symbol(_))
                    | resumable::OperationKind::Literal(resumable::Literal::Nil)
                    | resumable::OperationKind::Literal(resumable::Literal::True)
                    | resumable::OperationKind::Literal(resumable::Literal::False) => {}
                    _ => return None,
                }
            }
        }
        let layout = resumable::color_slots(&program).ok()?;
        let descriptor =
            resumable_descriptor::compile_portable_descriptor_v2(&program, &layout, row, 1).ok()?;
        if program.blocks.len() == 1 {
            if !program.blocks[0].operations.iter().all(|op| {
                matches!(
                    op.kind,
                    resumable::OperationKind::EntryReceiver
                        | resumable::OperationKind::EntryArgument { .. }
                        | resumable::OperationKind::EntryCapture { .. }
                )
            }) {
                return None;
            }
            let resumable::Terminator::Return(value) = program.blocks[0].terminator else {
                return None;
            };
            let slot = *layout.value_slots.get(value.0 as usize)? as usize;
            return Some(Self {
                descriptor,
                source: CString::new("").ok()?,
                pcs: 1,
                leaf: Some(slot),
            });
        }
        let mut source = include_bytes!("native_tier/shims.c").to_vec();
        for pc in 0..program.blocks.len() {
            source.extend(
                resumable_c::emit_pc_specialized_step(pc as u32, &program, &layout, pc as u32)
                    .ok()?,
            );
            if source.len() > MAX_CANDIDATE_BYTES {
                return None;
            }
        }
        source.extend_from_slice(b"void* candidate_entry(unsigned long pc){switch(pc){\n");
        for pc in 0..program.blocks.len() {
            source
                .extend(format!("case {pc}:return (void*)__birnam_resumable_step_{pc};\n").bytes());
        }
        source.extend_from_slice(b"default:return 0;}}\n");
        Some(Self {
            descriptor,
            source: CString::new(source).ok()?,
            pcs: program.blocks.len(),
            leaf: None,
        })
    }

    fn compile(&self) -> Option<Candidate> {
        let bindings = [
            (c"birnam_send", shim_send as *const c_void),
            (c"birnam_intern", shim_intern as *const c_void),
            (c"birnam_mkSmallInt", shim_integer as *const c_void),
            (c"birnam_nil", shim_nil as *const c_void),
            (c"birnam_true", shim_true as *const c_void),
            (c"birnam_false", shim_false as *const c_void),
        ];
        // SAFETY: source comes only from the canonical emitter, whose helper
        // declarations match these process-lifetime runtime functions.
        let unit = unsafe { Unit::compile(&self.source, &bindings) }?;
        let getter: unsafe extern "C" fn(usize) -> *mut c_void =
            unsafe { core::mem::transmute(unit.symbol(c"candidate_entry")?.as_ptr()) };
        let mut entries = Vec::with_capacity(self.pcs);
        for pc in 0..self.pcs {
            let address = unsafe { getter(pc) };
            if address.is_null() {
                return None;
            }
            entries.push(unsafe { core::mem::transmute::<*mut c_void, Method>(address) });
        }
        Some(Candidate {
            entries,
            _unit: unit,
        })
    }
}

pub(super) fn select(
    runtime: &Runtime,
    machine: Oop,
    descriptor_key: usize,
    pc: usize,
) -> Option<Selected> {
    let mut registry = runtime.native_tier.borrow_mut();
    let record = registry.records.get_mut(&descriptor_key)?;
    let descriptor = record.descriptor.upgrade()?;
    if Arc::as_ptr(&descriptor) as usize != descriptor_key
        || record.disabled
        || record.leaf.is_some()
    {
        return None;
    }
    let epoch = unsafe { (*runtime.state.get()).vtables.mutation_epoch() };
    // The slot guard revalidates canonical Machine/lookup bindings after
    // dictionary mutation, and checks receiver/registration state every time.
    // Ordinary targets remain late-bound sends, so unrelated epoch changes do
    // not invalidate this descriptor's PC-specialized body.
    if epoch == u64::MAX || !runtime.guarded_slot_access_eligible(machine) {
        record.disabled |= record.candidate.is_some();
        // A selected activation owns its own Rc, including during nested
        // entry. Withdrawing admission never frees executing native code.
        record.candidate.take();
        return None;
    }
    record.visits = record.visits.saturating_add(1);
    if record.visits < HOT_ENTRIES {
        return None;
    }
    if record.candidate.is_none() {
        let prepared = record.pending.take()?;
        record.candidate = prepared.compile().map(Rc::new);
    }
    let candidate = record.candidate.as_ref()?.clone();
    #[cfg(test)]
    HITS.set(HITS.get() + 1);
    Some(Selected {
        method: *candidate.entries.get(pc)?,
        _candidate: candidate,
    })
}

unsafe extern "C" fn shim_send(receiver: Oop, selector: Oop, args: *mut Oop, count: i64) -> Oop {
    with_active(|runtime| {
        #[cfg(test)]
        tests::on_send(runtime, receiver);
        unsafe { runtime.send_abi(receiver, selector, args, count as usize) }
    })
}
unsafe extern "C" fn shim_intern(name: *const c_char) -> Oop {
    with_active(|runtime| {
        #[cfg(test)]
        tests::on_intern(runtime);
        runtime
            .intern(unsafe { CStr::from_ptr(name) }.to_bytes())
            .expect("emitted selector")
    })
}
unsafe extern "C" fn shim_integer(value: i64) -> Oop {
    with_active(|runtime| runtime.make_integer(value))
}
unsafe extern "C" fn shim_nil() -> Oop {
    with_active(|runtime| runtime.canonical().nil)
}
unsafe extern "C" fn shim_true() -> Oop {
    with_active(|runtime| runtime.canonical().true_object)
}
unsafe extern "C" fn shim_false() -> Oop {
    with_active(|runtime| runtime.canonical().false_object)
}

#[cfg(test)]
mod tests;

/// Native projection inline over compiler-owned entry locations. No language
/// observation occurs between successful guard and Call result publication.
fn leaf_control_guard(runtime: &Runtime, machine: Oop) -> bool {
    if !runtime.guarded_slot_access_eligible(machine) {
        return false;
    }
    let state = unsafe { &*runtime.state.get() };
    let Some(objects) = state.execution.objects() else {
        return false;
    };
    let method = crate::raw::MethodAddress::from_method(crate::execution::machine_return_at);
    if state.execution.has_resumable_thunk(method)
        || state
            .vtables
            .direct_lookup(objects.machine_vtable, objects.return_at)
            != Some(method)
    {
        return false;
    }
    true
}

fn leaf_allocation_guard(runtime: &Runtime, descriptor: &FrameDescriptor) -> bool {
    let state = unsafe { &*runtime.state.get() };
    // Preserve the baseline's housekeeping boxes and its collection boundary.
    // If any of those allocations could collect, create the callee first.
    let bytes = crate::heap::Heap::allocation_total_size(core::mem::size_of::<i64>())
        .unwrap()
        .saturating_mul(1 + descriptor.driver_argument_count());
    !state
        .metadata
        .automatic_collection_due(state.heap.bytes_since_collection().saturating_add(bytes))
}

#[cfg(test)]
thread_local! {
    pub(super) static INLINE_HITS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    pub(super) static INLINE_RECOVERIES: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

pub(super) fn inline_call(
    runtime: &Runtime,
    machine: Oop,
    capability: crate::execution::MachineCapability,
    entry: crate::execution::ResumableEntryCapability,
    result_slot: usize,
    descriptor: &Arc<FrameDescriptor>,
    initial: &[(usize, Oop)],
) -> Result<bool, crate::execution::ExecutionError> {
    let epoch = unsafe { (*runtime.state.get()).vtables.mutation_epoch() };
    let leaf = {
        let mut registry = runtime.native_tier.borrow_mut();
        let Some(record) = registry
            .records
            .get_mut(&(Arc::as_ptr(descriptor) as usize))
        else {
            return Ok(false);
        };
        let Some(slot) = record.leaf else {
            return Ok(false);
        };
        if record.disabled
            || record.leaf_miss_epoch == Some(epoch)
            || !record
                .descriptor
                .upgrade()
                .is_some_and(|owned| Arc::ptr_eq(&owned, descriptor))
        {
            return Ok(false);
        }
        record.visits = record.visits.saturating_add(1);
        if record.visits < HOT_ENTRIES {
            return Ok(false);
        }
        slot
    };
    let nil = runtime.canonical().nil;
    let owner = super::current_thread_token();
    let valid_return = unsafe {
        let state = &*runtime.state.get();
        state
            .execution
            .inline_return_storage_valid(&state.heap, owner, capability)
    };
    let controls = leaf_control_guard(runtime, machine);
    if !valid_return || !controls || !leaf_allocation_guard(runtime, descriptor) {
        if !controls {
            // Recover this activation once; use ordinary entry thereafter until
            // dictionary mutation gives this optional optimization another chance.
            if let Some(record) = runtime
                .native_tier
                .borrow_mut()
                .records
                .get_mut(&(Arc::as_ptr(descriptor) as usize))
            {
                record.leaf_miss_epoch = Some(epoch);
            }
        }
        // The acquisition still owns the exact originally resolved generation.
        // Nil-fill non-entry locations, including live root slots, explicitly.
        let mut slots = vec![nil; descriptor.slot_count()];
        for &(slot, value) in initial {
            slots[slot] = value;
        }
        runtime.with_recovery_request(capability, entry, result_slot, || {
            runtime.recover_slots(capability, 0, &slots)
        })?;
        #[cfg(test)]
        INLINE_RECOVERIES.set(INLINE_RECOVERIES.get() + 1);
        return Ok(true);
    }
    // The guarded region cannot callback or collect. Keep the same allocation
    // count/values as baseline PC-token and return-index preparation.
    if descriptor.driver_argument_count() == 1 {
        runtime.make_integer(0);
    }
    runtime.make_integer(leaf as i64);
    let value = initial
        .iter()
        .find_map(|(slot, value)| (*slot == leaf).then_some(*value))
        .unwrap_or(nil);
    let result = unsafe {
        let state = &mut *runtime.state.get();
        state.execution.commit_inlined_return(
            &mut state.heap,
            owner,
            capability,
            result_slot,
            leaf,
            nil,
            value,
        )
    };
    unsafe {
        (*runtime.state.get())
            .execution
            .release_acquired_entry(entry)?;
    }
    result?;
    #[cfg(test)]
    INLINE_HITS.set(INLINE_HITS.get() + 1);
    Ok(true)
}

#[cfg(test)]
mod leaf_tests;
