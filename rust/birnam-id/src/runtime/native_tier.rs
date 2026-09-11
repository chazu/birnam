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
    epoch: Option<u64>,
    disabled: bool,
}

struct Prepared {
    descriptor: Vec<u8>,
    source: CString,
    pcs: usize,
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
                pending: Some(prepared),
                candidate: None,
                visits: 0,
                epoch: None,
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
        if program.blocks.len() < 2 || program.blocks.len() > 4096 {
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
    if Arc::as_ptr(&descriptor) as usize != descriptor_key || record.disabled {
        return None;
    }
    let epoch = unsafe { (*runtime.state.get()).vtables.mutation_epoch() };
    if epoch == u64::MAX
        || record.epoch.is_some_and(|previous| previous != epoch)
        || !runtime.guarded_slot_access_eligible(machine)
    {
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
        record.epoch = Some(epoch);
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
