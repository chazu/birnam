//! Private stackless-execution substrate owned by one [`Runtime`](crate::runtime::Runtime).
//!
//! This module deliberately contains no dispatch or code-loader policy. It
//! owns the S1 Machine capability registry, validated frame descriptors, and
//! retained frame arenas. Later S1 slices connect this substrate to canonical
//! Birnam Machine and StepAction objects without creating a second runtime.
#![allow(dead_code)] // S1 test artifacts exercise carriers not emitted until S2/S3.

use core::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;

use crate::abi::{MarkFn, Method, Oop};
use crate::heap::Heap;
use crate::raw::{AddressMap, MethodAddress};

pub(crate) const FRAME_LAYOUT_VERSION_V1: usize = 1;
pub(crate) const REGISTRATION_SCHEMA_VERSION_V1: usize = 1;
const MACHINE_CAPABILITY_KIND_V1: usize = 0x4d41_4348_0000_0001;

#[derive(Clone, Copy)]
pub(crate) struct ExecutionObjects {
    pub(crate) machine_vtable: Oop,
    pub(crate) machine_prototype: Oop,
    pub(crate) frame_view_vtable: Oop,
    pub(crate) frame_view_prototype: Oop,
    pub(crate) continue_action_vtable: Oop,
    pub(crate) continue_action_prototype: Oop,
    pub(crate) call_action_vtable: Oop,
    pub(crate) call_action_prototype: Oop,
    pub(crate) tail_call_action_vtable: Oop,
    pub(crate) tail_call_action_prototype: Oop,
    pub(crate) return_action_vtable: Oop,
    pub(crate) return_action_prototype: Oop,
    pub(crate) continue_kind: Oop,
    pub(crate) call_kind: Oop,
    pub(crate) tail_call_kind: Oop,
    pub(crate) return_kind: Oop,
    pub(crate) oop_slot_kind: Oop,
    pub(crate) raw_slot_kind: Oop,
    pub(crate) stale_frame_view_kind: Oop,
    pub(crate) invalid_frame_slot_kind: Oop,
    pub(crate) oop_at: Oop,
    pub(crate) oop_at_put: Oop,
    pub(crate) continue_at: Oop,
    pub(crate) call_site: Oop,
    pub(crate) tail_call_site: Oop,
    pub(crate) return_at: Oop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SlotKind {
    Oop,
    Raw,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CallSiteKind {
    Call,
    TailCall,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DescriptorIdentity {
    pub(crate) implementation: u64,
    pub(crate) generation: u64,
    pub(crate) source_map: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CallSiteDescriptor {
    pub(crate) kind: CallSiteKind,
    pub(crate) receiver_slot: usize,
    pub(crate) selector_slot: usize,
    pub(crate) argument_start: usize,
    pub(crate) argument_count: usize,
    pub(crate) result_slot: Option<usize>,
    pub(crate) continuation_pc: Option<usize>,
}

pub(crate) struct FrameDescriptorSpec {
    pub(crate) identity: DescriptorIdentity,
    pub(crate) layout_version: usize,
    pub(crate) program_counter_count: usize,
    pub(crate) driver_argument_count: usize,
    pub(crate) slot_kinds: Vec<SlotKind>,
    pub(crate) call_sites: Vec<CallSiteDescriptor>,
    pub(crate) step_method: Method,
}

pub(crate) struct FrameDescriptor {
    identity: DescriptorIdentity,
    program_counter_count: usize,
    driver_argument_count: usize,
    slot_kinds: Box<[SlotKind]>,
    call_sites: Box<[CallSiteDescriptor]>,
    step_method: Method,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DescriptorError {
    UnsupportedLayoutVersion,
    EmptyProgramCounterRange,
    InvalidDriverArgumentCount,
    ZeroArgumentDescriptorHasMultipleEntries,
    InvalidReceiverSlot,
    InvalidSelectorSlot,
    InvalidArgumentRange,
    InvalidArgumentSlotKind,
    InvalidResultSlot,
    ResultOverlapsArguments,
    MissingContinuation,
    UnexpectedContinuation,
    InvalidContinuation,
}

impl FrameDescriptor {
    pub(crate) fn validate(spec: FrameDescriptorSpec) -> Result<Arc<Self>, DescriptorError> {
        if spec.layout_version != FRAME_LAYOUT_VERSION_V1 {
            return Err(DescriptorError::UnsupportedLayoutVersion);
        }
        if spec.program_counter_count == 0 {
            return Err(DescriptorError::EmptyProgramCounterRange);
        }
        if spec.driver_argument_count > 1 {
            return Err(DescriptorError::InvalidDriverArgumentCount);
        }
        if spec.driver_argument_count == 0 && spec.program_counter_count != 1 {
            return Err(DescriptorError::ZeroArgumentDescriptorHasMultipleEntries);
        }

        for call_site in &spec.call_sites {
            require_oop_slot(&spec.slot_kinds, call_site.receiver_slot)
                .map_err(|_| DescriptorError::InvalidReceiverSlot)?;
            require_oop_slot(&spec.slot_kinds, call_site.selector_slot)
                .map_err(|_| DescriptorError::InvalidSelectorSlot)?;

            let argument_end = call_site
                .argument_start
                .checked_add(call_site.argument_count)
                .ok_or(DescriptorError::InvalidArgumentRange)?;
            if argument_end > spec.slot_kinds.len() {
                return Err(DescriptorError::InvalidArgumentRange);
            }
            if spec.slot_kinds[call_site.argument_start..argument_end]
                .iter()
                .any(|kind| *kind != SlotKind::Oop)
            {
                return Err(DescriptorError::InvalidArgumentSlotKind);
            }

            match call_site.kind {
                CallSiteKind::Call => {
                    let result_slot = call_site
                        .result_slot
                        .ok_or(DescriptorError::InvalidResultSlot)?;
                    require_oop_slot(&spec.slot_kinds, result_slot)
                        .map_err(|_| DescriptorError::InvalidResultSlot)?;
                    if (call_site.argument_start..argument_end).contains(&result_slot) {
                        return Err(DescriptorError::ResultOverlapsArguments);
                    }
                    let continuation = call_site
                        .continuation_pc
                        .ok_or(DescriptorError::MissingContinuation)?;
                    if continuation >= spec.program_counter_count {
                        return Err(DescriptorError::InvalidContinuation);
                    }
                }
                CallSiteKind::TailCall => {
                    if call_site.result_slot.is_some() {
                        return Err(DescriptorError::InvalidResultSlot);
                    }
                    if call_site.continuation_pc.is_some() {
                        return Err(DescriptorError::UnexpectedContinuation);
                    }
                }
            }
        }

        Ok(Arc::new(Self {
            identity: spec.identity,
            program_counter_count: spec.program_counter_count,
            driver_argument_count: spec.driver_argument_count,
            slot_kinds: spec.slot_kinds.into_boxed_slice(),
            call_sites: spec.call_sites.into_boxed_slice(),
            step_method: spec.step_method,
        }))
    }

    pub(crate) fn identity(&self) -> DescriptorIdentity {
        self.identity
    }

    pub(crate) fn program_counter_count(&self) -> usize {
        self.program_counter_count
    }

    pub(crate) fn driver_argument_count(&self) -> usize {
        self.driver_argument_count
    }

    pub(crate) fn slot_count(&self) -> usize {
        self.slot_kinds.len()
    }

    pub(crate) fn slot_kind(&self, index: usize) -> Option<SlotKind> {
        self.slot_kinds.get(index).copied()
    }

    pub(crate) fn call_site(&self, index: usize) -> Option<&CallSiteDescriptor> {
        self.call_sites.get(index)
    }

    fn validate_send_operands(
        &self,
        receiver_slot: usize,
        selector_slot: usize,
        argument_start: usize,
        argument_count: usize,
        result_slot: Option<usize>,
    ) -> Result<(), FrameError> {
        require_oop_slot(&self.slot_kinds, receiver_slot).map_err(|_| FrameError::InvalidSlot)?;
        require_oop_slot(&self.slot_kinds, selector_slot).map_err(|_| FrameError::InvalidSlot)?;
        let argument_end = argument_start
            .checked_add(argument_count)
            .ok_or(FrameError::InvalidSlot)?;
        if argument_end > self.slot_kinds.len()
            || self.slot_kinds[argument_start..argument_end]
                .iter()
                .any(|kind| *kind != SlotKind::Oop)
        {
            return Err(FrameError::InvalidSlot);
        }
        if let Some(result_slot) = result_slot {
            require_oop_slot(&self.slot_kinds, result_slot).map_err(|_| FrameError::InvalidSlot)?;
            if (argument_start..argument_end).contains(&result_slot) {
                return Err(FrameError::InvalidSlot);
            }
        }
        Ok(())
    }

    fn validate_entry_binding(
        &self,
        receiver_slot: usize,
        argument_start: usize,
        declared_arity: usize,
    ) -> Result<(), DescriptorError> {
        require_oop_slot(&self.slot_kinds, receiver_slot)
            .map_err(|_| DescriptorError::InvalidReceiverSlot)?;
        let argument_end = argument_start
            .checked_add(declared_arity)
            .ok_or(DescriptorError::InvalidArgumentRange)?;
        if argument_end > self.slot_kinds.len() {
            return Err(DescriptorError::InvalidArgumentRange);
        }
        if self.slot_kinds[argument_start..argument_end]
            .iter()
            .any(|kind| *kind != SlotKind::Oop)
        {
            return Err(DescriptorError::InvalidArgumentSlotKind);
        }
        Ok(())
    }

    pub(crate) fn step_method(&self) -> Method {
        self.step_method
    }

    fn accepts_program_counter(&self, pc: usize) -> bool {
        pc < self.program_counter_count
    }
}

fn require_oop_slot(slot_kinds: &[SlotKind], index: usize) -> Result<(), ()> {
    match slot_kinds.get(index) {
        Some(SlotKind::Oop) => Ok(()),
        Some(SlotKind::Raw) | None => Err(()),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FrameHandle {
    arena: u64,
    index: usize,
    generation: u64,
}

impl FrameHandle {
    fn from_words(arena: usize, index: usize, generation: usize) -> Self {
        Self {
            arena: arena as u64,
            index,
            generation: generation as u64,
        }
    }

    pub(crate) fn words(self) -> Result<[usize; 3], FrameViewError> {
        Ok([
            usize::try_from(self.arena).map_err(|_| FrameViewError::InvalidView)?,
            self.index,
            usize::try_from(self.generation).map_err(|_| FrameViewError::InvalidView)?,
        ])
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FrameError {
    InvalidProgramCounter,
    InvalidSlot,
    WrongSlotKind,
    StaleHandle,
    GenerationExhausted,
    ArenaDestroyed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FrameViewError {
    InvalidView,
    StaleFrameView,
    InvalidFrameSlot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FrameViewSnapshot {
    pub(crate) identity: DescriptorIdentity,
    pub(crate) program_counter: usize,
    pub(crate) slot_count: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ArenaDiagnostics {
    pub(crate) live_frames: usize,
    pub(crate) high_water_live_frames: usize,
    pub(crate) reserved_bytes: usize,
    pub(crate) current_synchronous_boundary_depth: usize,
    pub(crate) high_water_synchronous_boundary_depth: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RuntimeExecutionDiagnostics {
    pub(crate) live_machines: usize,
    pub(crate) live_frames: usize,
    pub(crate) reserved_arena_bytes: usize,
}

struct ArenaSlot {
    generation: u64,
    live: bool,
    descriptor: Option<Arc<FrameDescriptor>>,
    program_counter: usize,
    return_destination: Option<ReturnDestination>,
    resumable_entry: Option<ResumableEntryCapability>,
    words: Vec<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ReturnDestination {
    pub(crate) result_slot: usize,
}

impl ArenaSlot {
    fn new() -> Self {
        Self {
            generation: 1,
            live: false,
            descriptor: None,
            program_counter: 0,
            return_destination: None,
            resumable_entry: None,
            words: Vec::new(),
        }
    }
}

pub(crate) struct FrameArena {
    identity: u64,
    slots: Vec<ArenaSlot>,
    stack: Vec<usize>,
    high_water_live_frames: usize,
    destroyed: bool,
}

impl FrameArena {
    fn new(identity: u64) -> Self {
        Self {
            identity,
            slots: Vec::new(),
            stack: Vec::new(),
            high_water_live_frames: 0,
            destroyed: false,
        }
    }

    pub(crate) fn push(
        &mut self,
        descriptor: Arc<FrameDescriptor>,
        program_counter: usize,
        nil: Oop,
        initial_oops: &[(usize, Oop)],
    ) -> Result<FrameHandle, FrameError> {
        self.push_with_return(descriptor, program_counter, nil, initial_oops, None)
    }

    pub(crate) fn push_with_return(
        &mut self,
        descriptor: Arc<FrameDescriptor>,
        program_counter: usize,
        nil: Oop,
        initial_oops: &[(usize, Oop)],
        return_destination: Option<ReturnDestination>,
    ) -> Result<FrameHandle, FrameError> {
        self.push_with_return_and_entry(
            descriptor,
            program_counter,
            nil,
            initial_oops,
            return_destination,
            None,
        )
    }

    pub(crate) fn push_with_return_and_entry(
        &mut self,
        descriptor: Arc<FrameDescriptor>,
        program_counter: usize,
        nil: Oop,
        initial_oops: &[(usize, Oop)],
        return_destination: Option<ReturnDestination>,
        resumable_entry: Option<ResumableEntryCapability>,
    ) -> Result<FrameHandle, FrameError> {
        self.validate_push(&descriptor, program_counter, initial_oops)?;
        self.push_validated(
            descriptor,
            program_counter,
            nil,
            initial_oops,
            return_destination,
            resumable_entry,
        )
    }

    fn validate_push(
        &self,
        descriptor: &FrameDescriptor,
        program_counter: usize,
        initial_oops: &[(usize, Oop)],
    ) -> Result<(), FrameError> {
        if self.destroyed {
            return Err(FrameError::ArenaDestroyed);
        }
        if !descriptor.accepts_program_counter(program_counter) {
            return Err(FrameError::InvalidProgramCounter);
        }
        for (slot, _) in initial_oops {
            match descriptor.slot_kind(*slot) {
                None => return Err(FrameError::InvalidSlot),
                Some(SlotKind::Raw) => return Err(FrameError::WrongSlotKind),
                Some(SlotKind::Oop) => {}
            }
        }
        Ok(())
    }

    fn push_validated(
        &mut self,
        descriptor: Arc<FrameDescriptor>,
        program_counter: usize,
        nil: Oop,
        initial_oops: &[(usize, Oop)],
        return_destination: Option<ReturnDestination>,
        resumable_entry: Option<ResumableEntryCapability>,
    ) -> Result<FrameHandle, FrameError> {
        let index = self.stack.len();
        if index == self.slots.len() {
            self.slots.push(ArenaSlot::new());
        }
        let slot = &mut self.slots[index];
        debug_assert!(!slot.live);
        slot.words.resize(descriptor.slot_count(), 0);
        slot.words[..descriptor.slot_count()].fill(0);
        for (index, kind) in descriptor.slot_kinds.iter().copied().enumerate() {
            if kind == SlotKind::Oop {
                slot.words[index] = nil.addr();
            }
        }
        for (index, value) in initial_oops {
            slot.words[*index] = value.addr();
        }
        slot.words.truncate(descriptor.slot_count());
        slot.descriptor = Some(descriptor);
        slot.program_counter = program_counter;
        slot.return_destination = return_destination;
        slot.resumable_entry = resumable_entry;
        slot.live = true;
        self.stack.push(index);
        self.high_water_live_frames = self.high_water_live_frames.max(self.stack.len());

        Ok(FrameHandle {
            arena: self.identity,
            index,
            generation: slot.generation,
        })
    }

    pub(crate) fn pop(&mut self) -> Result<FrameHandle, FrameError> {
        if self.destroyed {
            return Err(FrameError::ArenaDestroyed);
        }
        let index = *self.stack.last().ok_or(FrameError::StaleHandle)?;
        let slot = &mut self.slots[index];
        let next_generation = slot
            .generation
            .checked_add(1)
            .ok_or(FrameError::GenerationExhausted)?;
        let old = FrameHandle {
            arena: self.identity,
            index,
            generation: slot.generation,
        };
        self.stack.pop();
        slot.live = false;
        slot.descriptor = None;
        slot.program_counter = 0;
        slot.return_destination = None;
        slot.resumable_entry = None;
        slot.generation = next_generation;
        Ok(old)
    }

    pub(crate) fn return_destination(
        &self,
        handle: FrameHandle,
    ) -> Result<Option<ReturnDestination>, FrameError> {
        Ok(self.require_live(handle)?.return_destination)
    }

    pub(crate) fn resumable_entry(
        &self,
        handle: FrameHandle,
    ) -> Result<Option<ResumableEntryCapability>, FrameError> {
        Ok(self.require_live(handle)?.resumable_entry)
    }

    pub(crate) fn replace_current(
        &mut self,
        descriptor: Arc<FrameDescriptor>,
        program_counter: usize,
        nil: Oop,
        initial_oops: &[(usize, Oop)],
    ) -> Result<FrameHandle, FrameError> {
        self.replace_current_with_entry(descriptor, program_counter, nil, initial_oops, None)
    }

    pub(crate) fn replace_current_with_entry(
        &mut self,
        descriptor: Arc<FrameDescriptor>,
        program_counter: usize,
        nil: Oop,
        initial_oops: &[(usize, Oop)],
        resumable_entry: Option<ResumableEntryCapability>,
    ) -> Result<FrameHandle, FrameError> {
        self.validate_push(&descriptor, program_counter, initial_oops)?;
        let current = self.current_handle().ok_or(FrameError::StaleHandle)?;
        let return_destination = self.return_destination(current)?;
        self.pop()?;
        self.push_validated(
            descriptor,
            program_counter,
            nil,
            initial_oops,
            return_destination,
            resumable_entry,
        )
    }

    pub(crate) fn current_handle(&self) -> Option<FrameHandle> {
        let index = *self.stack.last()?;
        let slot = &self.slots[index];
        Some(FrameHandle {
            arena: self.identity,
            index,
            generation: slot.generation,
        })
    }

    pub(crate) fn descriptor(&self, handle: FrameHandle) -> Result<&FrameDescriptor, FrameError> {
        let slot = self.require_live(handle)?;
        Ok(slot
            .descriptor
            .as_deref()
            .expect("a live arena slot always has a descriptor"))
    }

    pub(crate) fn descriptor_arc(
        &self,
        handle: FrameHandle,
    ) -> Result<Arc<FrameDescriptor>, FrameError> {
        Ok(self
            .require_live(handle)?
            .descriptor
            .as_ref()
            .expect("a live arena slot always has a descriptor")
            .clone())
    }

    pub(crate) fn program_counter(&self, handle: FrameHandle) -> Result<usize, FrameError> {
        Ok(self.require_live(handle)?.program_counter)
    }

    pub(crate) fn set_program_counter(
        &mut self,
        handle: FrameHandle,
        program_counter: usize,
    ) -> Result<(), FrameError> {
        let slot = self.require_live(handle)?;
        let descriptor = slot
            .descriptor
            .as_deref()
            .expect("a live arena slot always has a descriptor");
        if !descriptor.accepts_program_counter(program_counter) {
            return Err(FrameError::InvalidProgramCounter);
        }
        self.require_live_mut(handle)?.program_counter = program_counter;
        Ok(())
    }

    pub(crate) fn read_oop(&self, handle: FrameHandle, index: usize) -> Result<Oop, FrameError> {
        let slot = self.require_live(handle)?;
        match slot
            .descriptor
            .as_deref()
            .expect("a live arena slot always has a descriptor")
            .slot_kind(index)
        {
            None => return Err(FrameError::InvalidSlot),
            Some(SlotKind::Raw) => return Err(FrameError::WrongSlotKind),
            Some(SlotKind::Oop) => {}
        }
        Ok(core::ptr::with_exposed_provenance_mut(slot.words[index]))
    }

    pub(crate) fn write_oop(
        &mut self,
        handle: FrameHandle,
        index: usize,
        value: Oop,
    ) -> Result<(), FrameError> {
        let slot = self.require_live(handle)?;
        match slot
            .descriptor
            .as_deref()
            .expect("a live arena slot always has a descriptor")
            .slot_kind(index)
        {
            None => return Err(FrameError::InvalidSlot),
            Some(SlotKind::Raw) => return Err(FrameError::WrongSlotKind),
            Some(SlotKind::Oop) => {}
        }
        self.require_live_mut(handle)?.words[index] = value.addr();
        Ok(())
    }

    pub(crate) fn trace_roots(&self, mut mark: impl FnMut(Oop)) {
        if self.destroyed {
            return;
        }
        for index in &self.stack {
            let slot = &self.slots[*index];
            let descriptor = slot
                .descriptor
                .as_deref()
                .expect("a live arena slot always has a descriptor");
            for (word, kind) in slot.words.iter().zip(descriptor.slot_kinds.iter()) {
                if *kind == SlotKind::Oop {
                    mark(core::ptr::with_exposed_provenance_mut(*word));
                }
            }
        }
    }

    pub(crate) fn diagnostics(&self) -> ArenaDiagnostics {
        if self.destroyed {
            return ArenaDiagnostics {
                high_water_live_frames: self.high_water_live_frames,
                ..ArenaDiagnostics::default()
            };
        }
        let payload_bytes = self.slots.iter().fold(0usize, |total, slot| {
            total.saturating_add(slot.words.capacity().saturating_mul(size_of::<usize>()))
        });
        let bookkeeping_bytes = self
            .slots
            .capacity()
            .saturating_mul(size_of::<ArenaSlot>())
            .saturating_add(self.stack.capacity().saturating_mul(size_of::<usize>()));
        ArenaDiagnostics {
            live_frames: self.stack.len(),
            high_water_live_frames: self.high_water_live_frames,
            reserved_bytes: payload_bytes.saturating_add(bookkeeping_bytes),
            ..ArenaDiagnostics::default()
        }
    }

    pub(crate) fn destroy(&mut self) -> Vec<ResumableEntryCapability> {
        let mut entry_pins = Vec::new();
        for slot in &mut self.slots {
            if slot.live
                && let Some(entry) = slot.resumable_entry
            {
                entry_pins.push(entry);
            }
            slot.live = false;
            slot.descriptor = None;
            slot.resumable_entry = None;
            slot.words.clear();
        }
        self.stack.clear();
        self.slots.clear();
        self.stack.shrink_to_fit();
        self.slots.shrink_to_fit();
        self.destroyed = true;
        entry_pins
    }

    fn require_live(&self, handle: FrameHandle) -> Result<&ArenaSlot, FrameError> {
        if self.destroyed {
            return Err(FrameError::ArenaDestroyed);
        }
        let slot = self
            .slots
            .get(handle.index)
            .ok_or(FrameError::StaleHandle)?;
        if handle.arena != self.identity || !slot.live || handle.generation != slot.generation {
            return Err(FrameError::StaleHandle);
        }
        Ok(slot)
    }

    fn require_live_mut(&mut self, handle: FrameHandle) -> Result<&mut ArenaSlot, FrameError> {
        if self.destroyed {
            return Err(FrameError::ArenaDestroyed);
        }
        let slot = self
            .slots
            .get_mut(handle.index)
            .ok_or(FrameError::StaleHandle)?;
        if handle.arena != self.identity || !slot.live || handle.generation != slot.generation {
            return Err(FrameError::StaleHandle);
        }
        Ok(slot)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MachineLifecycle {
    New,
    Running,
    Completed,
    Suspended,
    Failed,
    Destroyed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MachineCapability {
    pub(crate) index: usize,
    pub(crate) generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ResumableEntryCapability {
    index: usize,
    generation: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct RegistrationGenerationLease {
    generation_identity: u64,
    token: Arc<()>,
}

impl RegistrationGenerationLease {
    pub(crate) fn new(generation_identity: u64) -> Self {
        Self {
            generation_identity,
            token: Arc::new(()),
        }
    }

    #[cfg(test)]
    pub(crate) fn holder_count(&self) -> usize {
        Arc::strong_count(&self.token)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GenerationLeaseCapability {
    index: usize,
    generation: u64,
}

pub(crate) struct SemanticRegistrationV1 {
    pub(crate) schema_version: usize,
    pub(crate) byte_size: usize,
    pub(crate) thunk: Method,
    pub(crate) descriptor: Arc<FrameDescriptor>,
    pub(crate) declared_arity: usize,
    pub(crate) receiver_slot: usize,
    pub(crate) argument_start: usize,
    pub(crate) implementation_identity: u64,
    pub(crate) code_generation_identity: u64,
    pub(crate) generation_lease: RegistrationGenerationLease,
}

impl SemanticRegistrationV1 {
    pub(crate) fn new(
        thunk: Method,
        descriptor: Arc<FrameDescriptor>,
        declared_arity: usize,
        receiver_slot: usize,
        argument_start: usize,
    ) -> Self {
        let identity = descriptor.identity();
        Self {
            schema_version: REGISTRATION_SCHEMA_VERSION_V1,
            byte_size: size_of::<Self>(),
            thunk,
            descriptor,
            declared_arity,
            receiver_slot,
            argument_start,
            implementation_identity: identity.implementation,
            code_generation_identity: identity.generation,
            generation_lease: RegistrationGenerationLease::new(identity.generation),
        }
    }

    pub(crate) fn with_generation_lease(
        mut self,
        generation_lease: RegistrationGenerationLease,
    ) -> Self {
        self.generation_lease = generation_lease;
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RegistrationError {
    UnsupportedSchema,
    RecordTooSmall,
    InvalidDescriptor,
    InvalidIdentity,
    StepMethodInstalledAsThunk,
    DuplicateThunk,
    StaleCapability,
    GenerationExhausted,
    EntryUnpublished,
    EntryRetired,
    BindingReferenceOverflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MachineError {
    WrongOwner,
    StaleCapability,
    InvalidLifecycle,
    DriveEpochExhausted,
    RegistryGenerationExhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExecutionError {
    Machine(MachineError),
    Frame(FrameError),
    InvalidAction,
    StaleAction,
    ActionWrongMachine,
    MissingImplementation,
    Registration(RegistrationError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DriverFailureKind {
    InvalidMachineOwner,
    MachineAlreadyRunning,
    InvalidStepAction,
    StaleStepAction,
    StepActionWrongMachine,
    InvalidFrameSlot,
    MissingImplementation,
    CodeGenerationUnavailable,
    ResumableEntryUnregistered,
    ResumableEntryRetired,
    ResumableEntryStaleGeneration,
}

impl DriverFailureKind {
    pub(crate) fn symbol(self) -> &'static str {
        match self {
            Self::InvalidMachineOwner => "invalidMachineOwner",
            Self::MachineAlreadyRunning => "machineAlreadyRunning",
            Self::InvalidStepAction => "invalidStepAction",
            Self::StaleStepAction => "staleStepAction",
            Self::StepActionWrongMachine => "stepActionWrongMachine",
            Self::InvalidFrameSlot => "invalidFrameSlot",
            Self::MissingImplementation => "missingImplementation",
            Self::CodeGenerationUnavailable => "codeGenerationUnavailable",
            Self::ResumableEntryUnregistered => "resumableEntryUnregistered",
            Self::ResumableEntryRetired => "resumableEntryRetired",
            Self::ResumableEntryStaleGeneration => "resumableEntryStaleGeneration",
        }
    }
}

impl ExecutionError {
    pub(crate) fn failure_kind(self) -> DriverFailureKind {
        match self {
            Self::Machine(MachineError::WrongOwner) => DriverFailureKind::InvalidMachineOwner,
            Self::Machine(MachineError::InvalidLifecycle) => {
                DriverFailureKind::MachineAlreadyRunning
            }
            Self::Machine(
                MachineError::StaleCapability
                | MachineError::DriveEpochExhausted
                | MachineError::RegistryGenerationExhausted,
            ) => DriverFailureKind::CodeGenerationUnavailable,
            Self::Frame(FrameError::InvalidSlot | FrameError::WrongSlotKind) => {
                DriverFailureKind::InvalidFrameSlot
            }
            Self::Frame(
                FrameError::StaleHandle
                | FrameError::ArenaDestroyed
                | FrameError::GenerationExhausted
                | FrameError::InvalidProgramCounter,
            )
            | Self::InvalidAction => DriverFailureKind::InvalidStepAction,
            Self::StaleAction => DriverFailureKind::StaleStepAction,
            Self::ActionWrongMachine => DriverFailureKind::StepActionWrongMachine,
            Self::MissingImplementation => DriverFailureKind::MissingImplementation,
            Self::Registration(RegistrationError::EntryRetired) => {
                DriverFailureKind::ResumableEntryRetired
            }
            Self::Registration(RegistrationError::EntryUnpublished) => {
                DriverFailureKind::ResumableEntryUnregistered
            }
            Self::Registration(RegistrationError::StaleCapability) => {
                DriverFailureKind::ResumableEntryStaleGeneration
            }
            Self::Registration(
                RegistrationError::UnsupportedSchema
                | RegistrationError::RecordTooSmall
                | RegistrationError::InvalidDescriptor
                | RegistrationError::InvalidIdentity
                | RegistrationError::StepMethodInstalledAsThunk
                | RegistrationError::DuplicateThunk
                | RegistrationError::GenerationExhausted
                | RegistrationError::BindingReferenceOverflow,
            ) => DriverFailureKind::CodeGenerationUnavailable,
        }
    }
}

impl From<MachineError> for ExecutionError {
    fn from(error: MachineError) -> Self {
        Self::Machine(error)
    }
}

impl From<FrameError> for ExecutionError {
    fn from(error: FrameError) -> Self {
        Self::Frame(error)
    }
}

impl From<RegistrationError> for ExecutionError {
    fn from(error: RegistrationError) -> Self {
        Self::Registration(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DecodedAction {
    Continue,
    Call {
        receiver_slot: usize,
        selector_slot: usize,
        argument_start: usize,
        argument_count: usize,
        result_slot: usize,
    },
    TailCall {
        receiver_slot: usize,
        selector_slot: usize,
        argument_start: usize,
        argument_count: usize,
    },
    Return {
        value_slot: usize,
    },
}

pub(crate) enum StacklessTarget {
    Registered {
        entry: ResumableEntryCapability,
    },
    Resumable {
        descriptor: Arc<FrameDescriptor>,
        initial_oops: Vec<(usize, Oop)>,
        entry: Option<ResumableEntryCapability>,
    },
    Synchronous(Method),
    Unbound,
}

pub(crate) struct MaterializedEntry {
    pub(crate) descriptor: Arc<FrameDescriptor>,
    pub(crate) initial_oops: Vec<(usize, Oop)>,
}

pub(crate) struct SendOperands {
    pub(crate) receiver: Oop,
    pub(crate) selector: Oop,
    pub(crate) arguments: Vec<Oop>,
}

pub(crate) struct MachineState {
    owner: u64,
    lifecycle: MachineLifecycle,
    drive_depth: usize,
    drive_epoch: u64,
    drive_boundaries: Vec<BoundaryDiagnostics>,
    last_drive_boundaries: BoundaryDiagnostics,
    arena: FrameArena,
    machine: Oop,
    continue_action: Oop,
    call_action: Oop,
    tail_call_action: Oop,
    return_action: Oop,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct BoundaryDiagnostics {
    current: usize,
    high_water: usize,
}

impl MachineState {
    fn new(
        owner: u64,
        arena_identity: u64,
        machine: Oop,
        continue_action: Oop,
        call_action: Oop,
        tail_call_action: Oop,
        return_action: Oop,
    ) -> Self {
        Self {
            owner,
            lifecycle: MachineLifecycle::New,
            drive_depth: 0,
            drive_epoch: 0,
            drive_boundaries: Vec::new(),
            last_drive_boundaries: BoundaryDiagnostics::default(),
            arena: FrameArena::new(arena_identity),
            machine,
            continue_action,
            call_action,
            tail_call_action,
            return_action,
        }
    }

    pub(crate) fn lifecycle(&self) -> MachineLifecycle {
        self.lifecycle
    }

    pub(crate) fn drive_epoch(&self) -> u64 {
        self.drive_epoch
    }

    pub(crate) fn arena(&self) -> &FrameArena {
        &self.arena
    }

    pub(crate) fn arena_mut(&mut self) -> &mut FrameArena {
        &mut self.arena
    }

    #[cfg(test)]
    pub(crate) fn scratch_actions(&self) -> (Oop, Oop, Oop, Oop) {
        (
            self.continue_action,
            self.call_action,
            self.tail_call_action,
            self.return_action,
        )
    }

    pub(crate) fn begin_initial_drive(&mut self, owner: u64) -> Result<u64, MachineError> {
        self.require_owner(owner)?;
        if self.lifecycle != MachineLifecycle::New || self.drive_depth != 0 {
            return Err(MachineError::InvalidLifecycle);
        }
        self.advance_drive_epoch()?;
        self.drive_depth = 1;
        self.drive_boundaries.push(BoundaryDiagnostics::default());
        self.lifecycle = MachineLifecycle::Running;
        Ok(self.drive_epoch)
    }

    pub(crate) fn begin_nested_drive(&mut self, owner: u64) -> Result<u64, MachineError> {
        self.require_driving(owner)?;
        self.advance_drive_epoch()?;
        self.drive_depth = self
            .drive_depth
            .checked_add(1)
            .ok_or(MachineError::DriveEpochExhausted)?;
        self.drive_boundaries.push(BoundaryDiagnostics::default());
        Ok(self.drive_epoch)
    }

    pub(crate) fn complete_initial_drive(&mut self, owner: u64) -> Result<(), MachineError> {
        self.require_owner(owner)?;
        if self.lifecycle != MachineLifecycle::Running
            || self.drive_depth != 1
            || self.arena.current_handle().is_some()
        {
            return Err(MachineError::InvalidLifecycle);
        }
        self.finish_drive_epoch()?;
        self.drive_depth = 0;
        self.lifecycle = MachineLifecycle::Completed;
        Ok(())
    }

    pub(crate) fn complete_nested_drive(&mut self, owner: u64) -> Result<(), MachineError> {
        self.require_owner(owner)?;
        if self.lifecycle != MachineLifecycle::Running || self.drive_depth <= 1 {
            return Err(MachineError::InvalidLifecycle);
        }
        self.finish_drive_epoch()?;
        self.drive_depth -= 1;
        Ok(())
    }

    pub(crate) fn complete_barrier_drive(&mut self, owner: u64) -> Result<(), MachineError> {
        if self.drive_depth == 1 {
            self.complete_initial_drive(owner)
        } else if self.arena.current_handle().is_some() {
            self.complete_nested_drive(owner)
        } else {
            Err(MachineError::InvalidLifecycle)
        }
    }

    pub(crate) fn enter_synchronous_boundary(&mut self, owner: u64) -> Result<(), MachineError> {
        self.require_driving(owner)?;
        let diagnostics = self
            .drive_boundaries
            .last_mut()
            .ok_or(MachineError::InvalidLifecycle)?;
        diagnostics.current = diagnostics
            .current
            .checked_add(1)
            .ok_or(MachineError::DriveEpochExhausted)?;
        diagnostics.high_water = diagnostics.high_water.max(diagnostics.current);
        Ok(())
    }

    pub(crate) fn leave_synchronous_boundary(&mut self, owner: u64) -> Result<(), MachineError> {
        self.require_driving(owner)?;
        let diagnostics = self
            .drive_boundaries
            .last_mut()
            .ok_or(MachineError::InvalidLifecycle)?;
        diagnostics.current = diagnostics
            .current
            .checked_sub(1)
            .ok_or(MachineError::InvalidLifecycle)?;
        Ok(())
    }

    fn finish_drive_epoch(&mut self) -> Result<(), MachineError> {
        let diagnostics = self
            .drive_boundaries
            .pop()
            .ok_or(MachineError::InvalidLifecycle)?;
        if diagnostics.current != 0 {
            return Err(MachineError::InvalidLifecycle);
        }
        self.last_drive_boundaries = diagnostics;
        self.advance_drive_epoch()
    }

    fn advance_drive_epoch(&mut self) -> Result<(), MachineError> {
        self.drive_epoch = self
            .drive_epoch
            .checked_add(1)
            .ok_or(MachineError::DriveEpochExhausted)?;
        Ok(())
    }

    pub(crate) fn fail(&mut self, owner: u64) -> Result<(), MachineError> {
        self.require_owner(owner)?;
        if matches!(
            self.lifecycle,
            MachineLifecycle::Completed | MachineLifecycle::Destroyed
        ) {
            return Err(MachineError::InvalidLifecycle);
        }
        for _ in 0..self.drive_depth {
            self.advance_drive_epoch()?;
        }
        self.drive_depth = 0;
        self.drive_boundaries.clear();
        self.lifecycle = MachineLifecycle::Failed;
        Ok(())
    }

    fn require_owner(&self, owner: u64) -> Result<(), MachineError> {
        if self.owner == owner {
            Ok(())
        } else {
            Err(MachineError::WrongOwner)
        }
    }

    fn require_driving(&self, owner: u64) -> Result<(), MachineError> {
        self.require_owner(owner)?;
        if self.lifecycle == MachineLifecycle::Running && self.drive_depth != 0 {
            Ok(())
        } else {
            Err(MachineError::InvalidLifecycle)
        }
    }

    fn destroy(&mut self) -> Vec<ResumableEntryCapability> {
        let entry_pins = self.arena.destroy();
        self.drive_depth = 0;
        self.drive_boundaries.clear();
        self.lifecycle = MachineLifecycle::Destroyed;
        entry_pins
    }

    fn trace_roots(&self, mark: MarkFn) {
        if !self.continue_action.is_null() {
            // SAFETY: the GC supplies a synchronous callback for this hook.
            unsafe { mark(self.continue_action) };
        }
        if !self.return_action.is_null() {
            // SAFETY: same callback lifetime as above.
            unsafe { mark(self.return_action) };
        }
        if !self.call_action.is_null() {
            // SAFETY: same callback lifetime as above.
            unsafe { mark(self.call_action) };
        }
        if !self.tail_call_action.is_null() {
            // SAFETY: same callback lifetime as above.
            unsafe { mark(self.tail_call_action) };
        }
        self.arena.trace_roots(|root| {
            // SAFETY: FrameArena never retains the callback or root.
            unsafe { mark(root) }
        });
    }
}

struct MachineSlot {
    generation: u64,
    state: Option<MachineState>,
}

struct ResumableEntry {
    thunk: MethodAddress,
    descriptor: Arc<FrameDescriptor>,
    declared_arity: usize,
    receiver_slot: usize,
    argument_start: usize,
    implementation_identity: u64,
    code_generation_identity: u64,
    lifecycle: ResumableEntryLifecycle,
    binding_references: usize,
    active_frame_pins: usize,
    acquired_entries: usize,
    explicit_generation_leases: usize,
    registration_generation_lease: Option<RegistrationGenerationLease>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResumableEntryLifecycle {
    Unpublished,
    Live,
    Retiring,
    Retired,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RegistrationDiagnostics {
    pub(crate) registered_entries: usize,
    pub(crate) live_entries: usize,
    pub(crate) retired_entries: usize,
    pub(crate) direct_binding_references: usize,
    pub(crate) active_frame_pins: usize,
    pub(crate) acquired_entries: usize,
    pub(crate) explicit_generation_leases: usize,
    pub(crate) registration_generation_leases: usize,
}

struct ResumableEntrySlot {
    generation: u64,
    entry: Option<ResumableEntry>,
}

struct GenerationLeaseRecord {
    entry: ResumableEntryCapability,
    _generation: RegistrationGenerationLease,
}

struct GenerationLeaseSlot {
    generation: u64,
    record: Option<GenerationLeaseRecord>,
}

pub(crate) struct ExecutionState {
    owner: u64,
    next_arena_identity: u64,
    machines: Vec<MachineSlot>,
    free_machine_slots: Vec<usize>,
    resumable_entries: Vec<ResumableEntrySlot>,
    free_resumable_entry_slots: Vec<usize>,
    resumable_by_thunk: AddressMap<ResumableEntryCapability>,
    generation_leases: Vec<GenerationLeaseSlot>,
    free_generation_lease_slots: Vec<usize>,
    dispatch_read_depth: usize,
    objects: Option<ExecutionObjects>,
    scoped_host_roots: Vec<Oop>,
    _owner_thread_only: PhantomData<Rc<()>>,
}

impl ExecutionState {
    pub(crate) fn new(owner: u64) -> Self {
        Self {
            owner,
            next_arena_identity: 1,
            machines: Vec::new(),
            free_machine_slots: Vec::new(),
            resumable_entries: Vec::new(),
            free_resumable_entry_slots: Vec::new(),
            resumable_by_thunk: AddressMap::default(),
            generation_leases: Vec::new(),
            free_generation_lease_slots: Vec::new(),
            dispatch_read_depth: 0,
            objects: None,
            scoped_host_roots: Vec::new(),
            _owner_thread_only: PhantomData,
        }
    }

    pub(crate) fn create_machine(&mut self, owner: u64) -> Result<MachineCapability, MachineError> {
        self.create_machine_with_objects(
            owner,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        )
    }

    pub(crate) fn install_objects(&mut self, objects: ExecutionObjects) {
        assert!(self.objects.is_none(), "execution objects installed twice");
        self.objects = Some(objects);
    }

    pub(crate) fn objects(&self) -> Option<ExecutionObjects> {
        self.objects
    }

    pub(crate) fn register_resumable(
        &mut self,
        owner: u64,
        carrier: SemanticRegistrationV1,
    ) -> Result<ResumableEntryCapability, RegistrationError> {
        self.require_owner(owner)
            .map_err(|_| RegistrationError::StaleCapability)?;
        if carrier.schema_version != REGISTRATION_SCHEMA_VERSION_V1 {
            return Err(RegistrationError::UnsupportedSchema);
        }
        if carrier.byte_size < size_of::<SemanticRegistrationV1>() {
            return Err(RegistrationError::RecordTooSmall);
        }
        carrier
            .descriptor
            .validate_entry_binding(
                carrier.receiver_slot,
                carrier.argument_start,
                carrier.declared_arity,
            )
            .map_err(|_| RegistrationError::InvalidDescriptor)?;
        let identity = carrier.descriptor.identity();
        if identity.implementation != carrier.implementation_identity
            || identity.generation != carrier.code_generation_identity
            || carrier.generation_lease.generation_identity != carrier.code_generation_identity
        {
            return Err(RegistrationError::InvalidIdentity);
        }
        let thunk = MethodAddress::from_method(carrier.thunk);
        if thunk == MethodAddress::from_method(carrier.descriptor.step_method()) {
            return Err(RegistrationError::StepMethodInstalledAsThunk);
        }
        let thunk_key = thunk.as_oop().addr();
        if self.resumable_by_thunk.contains_key(&thunk_key) {
            return Err(RegistrationError::DuplicateThunk);
        }
        let index = if let Some(index) = self.free_resumable_entry_slots.pop() {
            index
        } else {
            self.resumable_entries.push(ResumableEntrySlot {
                generation: 1,
                entry: None,
            });
            self.resumable_entries.len() - 1
        };
        let slot = &mut self.resumable_entries[index];
        debug_assert!(slot.entry.is_none());
        let capability = ResumableEntryCapability {
            index,
            generation: slot.generation,
        };
        slot.entry = Some(ResumableEntry {
            thunk,
            descriptor: carrier.descriptor,
            declared_arity: carrier.declared_arity,
            receiver_slot: carrier.receiver_slot,
            argument_start: carrier.argument_start,
            implementation_identity: carrier.implementation_identity,
            code_generation_identity: carrier.code_generation_identity,
            lifecycle: ResumableEntryLifecycle::Unpublished,
            binding_references: 0,
            active_frame_pins: 0,
            acquired_entries: 0,
            explicit_generation_leases: 0,
            registration_generation_lease: Some(carrier.generation_lease),
        });
        self.resumable_by_thunk.insert(thunk_key, capability);
        Ok(capability)
    }

    pub(crate) fn unregister_unpublished_resumable(
        &mut self,
        owner: u64,
        capability: ResumableEntryCapability,
    ) -> Result<(), RegistrationError> {
        self.require_owner(owner)
            .map_err(|_| RegistrationError::StaleCapability)?;
        let slot = self
            .resumable_entries
            .get_mut(capability.index)
            .ok_or(RegistrationError::StaleCapability)?;
        if slot.generation != capability.generation {
            return Err(RegistrationError::StaleCapability);
        }
        let next_generation = slot
            .generation
            .checked_add(1)
            .ok_or(RegistrationError::GenerationExhausted)?;
        let entry = slot
            .entry
            .as_ref()
            .ok_or(RegistrationError::StaleCapability)?;
        if entry.lifecycle != ResumableEntryLifecycle::Unpublished
            || entry.binding_references != 0
            || entry.active_frame_pins != 0
            || entry.acquired_entries != 0
            || entry.explicit_generation_leases != 0
        {
            return Err(RegistrationError::EntryRetired);
        }
        let entry = slot
            .entry
            .take()
            .expect("validated unpublished entry remains installed");
        self.resumable_by_thunk.remove(&entry.thunk.as_oop().addr());
        slot.generation = next_generation;
        self.free_resumable_entry_slots.push(capability.index);
        Ok(())
    }

    pub(crate) fn classify_resumable(
        &mut self,
        method: MethodAddress,
    ) -> Result<Option<StacklessTarget>, RegistrationError> {
        let Some(capability) = self
            .resumable_by_thunk
            .get(&method.as_oop().addr())
            .copied()
        else {
            return Ok(None);
        };
        let slot = self
            .resumable_entries
            .get_mut(capability.index)
            .ok_or(RegistrationError::StaleCapability)?;
        if slot.generation != capability.generation {
            return Err(RegistrationError::StaleCapability);
        }
        let entry = slot
            .entry
            .as_mut()
            .ok_or(RegistrationError::StaleCapability)?;
        match entry.lifecycle {
            ResumableEntryLifecycle::Live => {}
            ResumableEntryLifecycle::Retiring
                if self.dispatch_read_depth != 0 || entry.explicit_generation_leases != 0 => {}
            ResumableEntryLifecycle::Retiring => {
                return Err(RegistrationError::EntryRetired);
            }
            ResumableEntryLifecycle::Unpublished => {
                return Err(RegistrationError::EntryUnpublished);
            }
            ResumableEntryLifecycle::Retired => {
                return Err(RegistrationError::EntryRetired);
            }
        }
        debug_assert_eq!(entry.thunk, method);
        debug_assert_eq!(
            entry.implementation_identity,
            entry.descriptor.identity().implementation
        );
        entry.acquired_entries = entry
            .acquired_entries
            .checked_add(1)
            .ok_or(RegistrationError::BindingReferenceOverflow)?;
        debug_assert_eq!(
            entry.code_generation_identity,
            entry.descriptor.identity().generation
        );
        Ok(Some(StacklessTarget::Registered { entry: capability }))
    }

    pub(crate) fn materialize_acquired_entry(
        &self,
        capability: ResumableEntryCapability,
        receiver: Oop,
        arguments: &[Oop],
        nil: Oop,
    ) -> Result<MaterializedEntry, RegistrationError> {
        let slot = self
            .resumable_entries
            .get(capability.index)
            .ok_or(RegistrationError::StaleCapability)?;
        if slot.generation != capability.generation {
            return Err(RegistrationError::StaleCapability);
        }
        let entry = slot
            .entry
            .as_ref()
            .ok_or(RegistrationError::StaleCapability)?;
        if entry.acquired_entries == 0 {
            return Err(RegistrationError::StaleCapability);
        }
        let mut initial_oops = Vec::with_capacity(1 + entry.declared_arity);
        initial_oops.push((entry.receiver_slot, receiver));
        for offset in 0..entry.declared_arity {
            initial_oops.push((
                entry.argument_start + offset,
                arguments.get(offset).copied().unwrap_or(nil),
            ));
        }
        Ok(MaterializedEntry {
            descriptor: entry.descriptor.clone(),
            initial_oops,
        })
    }

    pub(crate) fn transfer_acquired_entry_to_frame(
        &mut self,
        capability: ResumableEntryCapability,
    ) -> Result<(), RegistrationError> {
        let slot = self
            .resumable_entries
            .get_mut(capability.index)
            .ok_or(RegistrationError::StaleCapability)?;
        if slot.generation != capability.generation {
            return Err(RegistrationError::StaleCapability);
        }
        let entry = slot
            .entry
            .as_mut()
            .ok_or(RegistrationError::StaleCapability)?;
        entry.acquired_entries = entry
            .acquired_entries
            .checked_sub(1)
            .ok_or(RegistrationError::StaleCapability)?;
        entry.active_frame_pins = entry
            .active_frame_pins
            .checked_add(1)
            .ok_or(RegistrationError::BindingReferenceOverflow)?;
        Ok(())
    }

    pub(crate) fn release_acquired_entry(
        &mut self,
        capability: ResumableEntryCapability,
    ) -> Result<(), RegistrationError> {
        let slot = self
            .resumable_entries
            .get_mut(capability.index)
            .ok_or(RegistrationError::StaleCapability)?;
        if slot.generation != capability.generation {
            return Err(RegistrationError::StaleCapability);
        }
        let entry = slot
            .entry
            .as_mut()
            .ok_or(RegistrationError::StaleCapability)?;
        entry.acquired_entries = entry
            .acquired_entries
            .checked_sub(1)
            .ok_or(RegistrationError::StaleCapability)?;
        self.finish_entry_retirement(capability);
        Ok(())
    }

    pub(crate) fn acquire_explicit_generation_lease(
        &mut self,
        method: MethodAddress,
    ) -> Result<GenerationLeaseCapability, RegistrationError> {
        let entry_capability = self
            .resumable_by_thunk
            .get(&method.as_oop().addr())
            .copied()
            .ok_or(RegistrationError::EntryRetired)?;
        let generation = {
            let slot = self
                .resumable_entries
                .get(entry_capability.index)
                .ok_or(RegistrationError::StaleCapability)?;
            if slot.generation != entry_capability.generation {
                return Err(RegistrationError::StaleCapability);
            }
            let entry = slot
                .entry
                .as_ref()
                .ok_or(RegistrationError::StaleCapability)?;
            if entry.lifecycle != ResumableEntryLifecycle::Live {
                return Err(RegistrationError::EntryRetired);
            }
            if entry.explicit_generation_leases == usize::MAX {
                return Err(RegistrationError::BindingReferenceOverflow);
            }
            entry
                .registration_generation_lease
                .as_ref()
                .cloned()
                .ok_or(RegistrationError::StaleCapability)?
        };
        let index = if let Some(index) = self.free_generation_lease_slots.pop() {
            index
        } else {
            self.generation_leases.push(GenerationLeaseSlot {
                generation: 1,
                record: None,
            });
            self.generation_leases.len() - 1
        };
        let lease_slot = &mut self.generation_leases[index];
        debug_assert!(lease_slot.record.is_none());
        let capability = GenerationLeaseCapability {
            index,
            generation: lease_slot.generation,
        };
        lease_slot.record = Some(GenerationLeaseRecord {
            entry: entry_capability,
            _generation: generation,
        });
        self.resumable_entries[entry_capability.index]
            .entry
            .as_mut()
            .expect("validated entry remains installed")
            .explicit_generation_leases += 1;
        Ok(capability)
    }

    pub(crate) fn release_explicit_generation_lease(
        &mut self,
        capability: GenerationLeaseCapability,
    ) -> Result<(), RegistrationError> {
        let slot = self
            .generation_leases
            .get(capability.index)
            .ok_or(RegistrationError::StaleCapability)?;
        if slot.generation != capability.generation {
            return Err(RegistrationError::StaleCapability);
        }
        let record = slot
            .record
            .as_ref()
            .ok_or(RegistrationError::StaleCapability)?;
        let entry_slot = self
            .resumable_entries
            .get(record.entry.index)
            .ok_or(RegistrationError::StaleCapability)?;
        if entry_slot.generation != record.entry.generation
            || entry_slot
                .entry
                .as_ref()
                .is_none_or(|entry| entry.explicit_generation_leases == 0)
        {
            return Err(RegistrationError::StaleCapability);
        }
        let next_generation = slot
            .generation
            .checked_add(1)
            .ok_or(RegistrationError::GenerationExhausted)?;
        let entry_capability = record.entry;

        let slot = &mut self.generation_leases[capability.index];
        let _record = slot
            .record
            .take()
            .expect("validated generation lease remains installed");
        slot.generation = next_generation;
        self.free_generation_lease_slots.push(capability.index);
        self.resumable_entries[entry_capability.index]
            .entry
            .as_mut()
            .expect("validated entry remains installed")
            .explicit_generation_leases -= 1;
        self.finish_entry_retirement(entry_capability);
        Ok(())
    }

    pub(crate) fn release_entry_frame_pin(
        &mut self,
        capability: ResumableEntryCapability,
    ) -> Result<(), RegistrationError> {
        let slot = self
            .resumable_entries
            .get_mut(capability.index)
            .ok_or(RegistrationError::StaleCapability)?;
        if slot.generation != capability.generation {
            return Err(RegistrationError::StaleCapability);
        }
        let entry = slot
            .entry
            .as_mut()
            .ok_or(RegistrationError::StaleCapability)?;
        entry.active_frame_pins = entry
            .active_frame_pins
            .checked_sub(1)
            .ok_or(RegistrationError::StaleCapability)?;
        self.finish_entry_retirement(capability);
        Ok(())
    }

    pub(crate) fn note_direct_binding_added(
        &mut self,
        method: MethodAddress,
    ) -> Result<(), RegistrationError> {
        let Some(capability) = self
            .resumable_by_thunk
            .get(&method.as_oop().addr())
            .copied()
        else {
            return Ok(());
        };
        let slot = self
            .resumable_entries
            .get_mut(capability.index)
            .ok_or(RegistrationError::StaleCapability)?;
        if slot.generation != capability.generation {
            return Err(RegistrationError::StaleCapability);
        }
        let entry = slot
            .entry
            .as_mut()
            .ok_or(RegistrationError::StaleCapability)?;
        if matches!(
            entry.lifecycle,
            ResumableEntryLifecycle::Retiring | ResumableEntryLifecycle::Retired
        ) {
            return Err(RegistrationError::EntryRetired);
        }
        entry.binding_references = entry
            .binding_references
            .checked_add(1)
            .ok_or(RegistrationError::BindingReferenceOverflow)?;
        entry.lifecycle = ResumableEntryLifecycle::Live;
        Ok(())
    }

    pub(crate) fn note_direct_binding_removed(
        &mut self,
        method: MethodAddress,
    ) -> Result<(), RegistrationError> {
        let Some(capability) = self
            .resumable_by_thunk
            .get(&method.as_oop().addr())
            .copied()
        else {
            return Ok(());
        };
        let slot = self
            .resumable_entries
            .get_mut(capability.index)
            .ok_or(RegistrationError::StaleCapability)?;
        if slot.generation != capability.generation {
            return Err(RegistrationError::StaleCapability);
        }
        let entry = slot
            .entry
            .as_mut()
            .ok_or(RegistrationError::StaleCapability)?;
        entry.binding_references = entry
            .binding_references
            .checked_sub(1)
            .ok_or(RegistrationError::EntryUnpublished)?;
        if entry.binding_references == 0 {
            entry.lifecycle = if entry.acquired_entries != 0
                || entry.active_frame_pins != 0
                || entry.explicit_generation_leases != 0
                || self.dispatch_read_depth != 0
            {
                ResumableEntryLifecycle::Retiring
            } else {
                entry.registration_generation_lease = None;
                ResumableEntryLifecycle::Retired
            };
        }
        Ok(())
    }

    pub(crate) fn begin_dispatch_read(&mut self) -> Result<(), RegistrationError> {
        self.dispatch_read_depth = self
            .dispatch_read_depth
            .checked_add(1)
            .ok_or(RegistrationError::BindingReferenceOverflow)?;
        Ok(())
    }

    pub(crate) fn end_dispatch_read(&mut self) -> Result<(), RegistrationError> {
        self.dispatch_read_depth = self
            .dispatch_read_depth
            .checked_sub(1)
            .ok_or(RegistrationError::StaleCapability)?;
        if self.dispatch_read_depth == 0 {
            for slot in &mut self.resumable_entries {
                let Some(entry) = slot.entry.as_mut() else {
                    continue;
                };
                if entry.lifecycle == ResumableEntryLifecycle::Retiring
                    && entry.binding_references == 0
                    && entry.acquired_entries == 0
                    && entry.active_frame_pins == 0
                    && entry.explicit_generation_leases == 0
                {
                    entry.lifecycle = ResumableEntryLifecycle::Retired;
                    entry.registration_generation_lease = None;
                }
            }
        }
        Ok(())
    }

    fn finish_entry_retirement(&mut self, capability: ResumableEntryCapability) {
        if self.dispatch_read_depth != 0 {
            return;
        }
        let Some(slot) = self.resumable_entries.get_mut(capability.index) else {
            return;
        };
        if slot.generation != capability.generation {
            return;
        }
        let Some(entry) = slot.entry.as_mut() else {
            return;
        };
        if entry.lifecycle == ResumableEntryLifecycle::Retiring
            && entry.binding_references == 0
            && entry.acquired_entries == 0
            && entry.active_frame_pins == 0
            && entry.explicit_generation_leases == 0
        {
            entry.lifecycle = ResumableEntryLifecycle::Retired;
            entry.registration_generation_lease = None;
        }
    }

    pub(crate) fn registration_diagnostics(&self) -> RegistrationDiagnostics {
        self.resumable_entries
            .iter()
            .filter_map(|slot| slot.entry.as_ref())
            .fold(RegistrationDiagnostics::default(), |mut total, entry| {
                total.registered_entries = total.registered_entries.saturating_add(1);
                total.direct_binding_references = total
                    .direct_binding_references
                    .saturating_add(entry.binding_references);
                total.active_frame_pins = total
                    .active_frame_pins
                    .saturating_add(entry.active_frame_pins);
                total.acquired_entries = total
                    .acquired_entries
                    .saturating_add(entry.acquired_entries);
                total.explicit_generation_leases = total
                    .explicit_generation_leases
                    .saturating_add(entry.explicit_generation_leases);
                total.registration_generation_leases = total
                    .registration_generation_leases
                    .saturating_add(usize::from(entry.registration_generation_lease.is_some()));
                match entry.lifecycle {
                    ResumableEntryLifecycle::Live => {
                        total.live_entries = total.live_entries.saturating_add(1)
                    }
                    ResumableEntryLifecycle::Retiring => {
                        total.live_entries = total.live_entries.saturating_add(1)
                    }
                    ResumableEntryLifecycle::Retired => {
                        total.retired_entries = total.retired_entries.saturating_add(1)
                    }
                    ResumableEntryLifecycle::Unpublished => {}
                }
                total
            })
    }

    pub(crate) fn push_scoped_host_root(&mut self, machine: Oop) {
        self.scoped_host_roots.push(machine);
    }

    pub(crate) fn pop_scoped_host_root(&mut self, machine: Oop) {
        let popped = self
            .scoped_host_roots
            .pop()
            .expect("stackless drive root stack underflow");
        assert_eq!(popped, machine, "stackless drive roots must unwind LIFO");
    }

    pub(crate) fn trace_scoped_host_roots(&self, mark: MarkFn) {
        for root in &self.scoped_host_roots {
            // SAFETY: the registered root provider receives a synchronous mark
            // callback and does not retain it beyond this enumeration.
            unsafe { mark(*root) };
        }
    }

    pub(crate) fn create_machine_with_objects(
        &mut self,
        owner: u64,
        machine: Oop,
        continue_action: Oop,
        call_action: Oop,
        tail_call_action: Oop,
        return_action: Oop,
    ) -> Result<MachineCapability, MachineError> {
        self.require_owner(owner)?;
        let arena_identity = self.next_arena_identity;
        self.next_arena_identity = self
            .next_arena_identity
            .checked_add(1)
            .ok_or(MachineError::RegistryGenerationExhausted)?;

        let index = if let Some(index) = self.free_machine_slots.pop() {
            index
        } else {
            self.machines.push(MachineSlot {
                generation: 1,
                state: None,
            });
            self.machines.len() - 1
        };
        let slot = &mut self.machines[index];
        debug_assert!(slot.state.is_none());
        slot.state = Some(MachineState::new(
            owner,
            arena_identity,
            machine,
            continue_action,
            call_action,
            tail_call_action,
            return_action,
        ));
        Ok(MachineCapability {
            index,
            generation: slot.generation,
        })
    }

    pub(crate) fn machine(
        &self,
        owner: u64,
        capability: MachineCapability,
    ) -> Result<&MachineState, MachineError> {
        self.require_owner(owner)?;
        let slot = self
            .machines
            .get(capability.index)
            .ok_or(MachineError::StaleCapability)?;
        if slot.generation != capability.generation {
            return Err(MachineError::StaleCapability);
        }
        slot.state.as_ref().ok_or(MachineError::StaleCapability)
    }

    pub(crate) fn machine_object(
        &self,
        owner: u64,
        capability: MachineCapability,
    ) -> Result<Oop, MachineError> {
        Ok(self.machine(owner, capability)?.machine)
    }

    pub(crate) fn machine_mut(
        &mut self,
        owner: u64,
        capability: MachineCapability,
    ) -> Result<&mut MachineState, MachineError> {
        self.require_owner(owner)?;
        let slot = self
            .machines
            .get_mut(capability.index)
            .ok_or(MachineError::StaleCapability)?;
        if slot.generation != capability.generation {
            return Err(MachineError::StaleCapability);
        }
        slot.state.as_mut().ok_or(MachineError::StaleCapability)
    }

    pub(crate) fn destroy_machine(
        &mut self,
        owner: u64,
        capability: MachineCapability,
    ) -> Result<(), MachineError> {
        self.require_owner(owner)?;
        let slot = self
            .machines
            .get_mut(capability.index)
            .ok_or(MachineError::StaleCapability)?;
        if slot.generation != capability.generation {
            return Err(MachineError::StaleCapability);
        }
        let next_generation = slot
            .generation
            .checked_add(1)
            .ok_or(MachineError::RegistryGenerationExhausted)?;
        slot.state
            .as_ref()
            .ok_or(MachineError::StaleCapability)?
            .require_owner(owner)?;
        let entry_pins = {
            let mut machine = slot
                .state
                .take()
                .expect("a validated live machine slot retains its state");
            let entry_pins = machine.destroy();
            slot.generation = next_generation;
            self.free_machine_slots.push(capability.index);
            entry_pins
        };
        for entry in entry_pins {
            self.release_entry_frame_pin(entry)
                .map_err(|_| MachineError::StaleCapability)?;
        }
        Ok(())
    }

    pub(crate) fn destroy_machine_explicit(
        &mut self,
        owner: u64,
        capability: MachineCapability,
    ) -> Result<(), MachineError> {
        let lifecycle = self.machine(owner, capability)?.lifecycle;
        if !matches!(
            lifecycle,
            MachineLifecycle::Suspended | MachineLifecycle::Completed | MachineLifecycle::Failed
        ) {
            return Err(MachineError::InvalidLifecycle);
        }
        self.destroy_machine(owner, capability)
    }

    pub(crate) fn capability_for_object(
        &self,
        heap: &Heap,
        object: Oop,
    ) -> Result<MachineCapability, MachineError> {
        let _objects = self.objects.ok_or(MachineError::StaleCapability)?;
        if heap.vtable_of(object).is_none()
            || heap.read_payload_word(object, 0) != Some(MACHINE_CAPABILITY_KIND_V1)
        {
            return Err(MachineError::StaleCapability);
        }
        let index = heap
            .read_payload_word(object, 1)
            .ok_or(MachineError::StaleCapability)?;
        let generation = heap
            .read_payload_word(object, 2)
            .and_then(|value| u64::try_from(value).ok())
            .ok_or(MachineError::StaleCapability)?;
        let capability = MachineCapability { index, generation };
        let machine = self.machine(self.owner, capability)?;
        if machine.machine != object {
            return Err(MachineError::StaleCapability);
        }
        Ok(capability)
    }

    pub(crate) fn initialize_machine_payload(
        &self,
        heap: &mut Heap,
        object: Oop,
        capability: MachineCapability,
    ) -> bool {
        heap.write_payload_word(object, 0, MACHINE_CAPABILITY_KIND_V1)
            && heap.write_payload_word(object, 1, capability.index)
            && usize::try_from(capability.generation)
                .ok()
                .is_some_and(|generation| heap.write_payload_word(object, 2, generation))
    }

    pub(crate) fn trace_object_roots(&self, heap: &Heap, object: Oop, vtable: Oop, mark: MarkFn) {
        let _ = vtable;
        let Ok(capability) = self.capability_for_object(heap, object) else {
            return;
        };
        if let Ok(machine) = self.machine(self.owner, capability) {
            machine.trace_roots(mark);
        }
    }

    /// Invalidate one Machine after the heap detached its still-live block.
    ///
    /// # Safety
    ///
    /// `object` must be the payload start of a detached Machine allocation with
    /// at least three initialized pointer-sized payload words. The allocation
    /// remains live until this synchronous finalizer returns.
    pub(crate) unsafe fn finalize_detached_machine(&mut self, object: Oop) {
        if object.is_null() {
            return;
        }
        // SAFETY: forwarded finalizer contract supplies three live raw words.
        let words = object.cast::<usize>();
        let kind = unsafe { words.read() };
        if kind != MACHINE_CAPABILITY_KIND_V1 {
            return;
        }
        // SAFETY: words 1 and 2 are within the same Machine payload.
        let index = unsafe { words.add(1).read() };
        // SAFETY: same detached payload contract.
        let generation_word = unsafe { words.add(2).read() };
        let Ok(generation) = u64::try_from(generation_word) else {
            return;
        };
        let capability = MachineCapability { index, generation };
        if self
            .machine(self.owner, capability)
            .is_ok_and(|machine| machine.machine == object)
        {
            let _ = self.destroy_machine(self.owner, capability);
        }
    }

    pub(crate) fn read_current_oop(
        &self,
        owner: u64,
        capability: MachineCapability,
        index: usize,
    ) -> Result<Oop, ExecutionError> {
        let machine = self.machine(owner, capability)?;
        machine.require_driving(owner)?;
        let frame = machine
            .arena
            .current_handle()
            .ok_or(FrameError::StaleHandle)?;
        Ok(machine.arena.read_oop(frame, index)?)
    }

    pub(crate) fn machine_diagnostics(
        &self,
        owner: u64,
        capability: MachineCapability,
    ) -> Result<ArenaDiagnostics, MachineError> {
        let machine = self.machine(owner, capability)?;
        let mut diagnostics = machine.arena.diagnostics();
        let boundaries = machine
            .drive_boundaries
            .last()
            .copied()
            .unwrap_or(machine.last_drive_boundaries);
        diagnostics.current_synchronous_boundary_depth = boundaries.current;
        diagnostics.high_water_synchronous_boundary_depth = boundaries.high_water;
        Ok(diagnostics)
    }

    pub(crate) fn runtime_diagnostics(&self) -> RuntimeExecutionDiagnostics {
        self.machines
            .iter()
            .filter_map(|slot| slot.state.as_ref())
            .fold(
                RuntimeExecutionDiagnostics::default(),
                |mut total, machine| {
                    let arena = machine.arena.diagnostics();
                    total.live_machines = total.live_machines.saturating_add(1);
                    total.live_frames = total.live_frames.saturating_add(arena.live_frames);
                    total.reserved_arena_bytes = total
                        .reserved_arena_bytes
                        .saturating_add(arena.reserved_bytes);
                    total
                },
            )
    }

    pub(crate) fn write_current_oop(
        &mut self,
        owner: u64,
        capability: MachineCapability,
        index: usize,
        value: Oop,
    ) -> Result<Oop, ExecutionError> {
        let machine = self.machine_mut(owner, capability)?;
        machine.require_driving(owner)?;
        let frame = machine
            .arena
            .current_handle()
            .ok_or(FrameError::StaleHandle)?;
        machine.arena.write_oop(frame, index, value)?;
        Ok(value)
    }

    pub(crate) fn copy_send_operands(
        &self,
        owner: u64,
        capability: MachineCapability,
        receiver_slot: usize,
        selector_slot: usize,
        argument_start: usize,
        argument_count: usize,
    ) -> Result<SendOperands, ExecutionError> {
        let machine = self.machine(owner, capability)?;
        machine.require_driving(owner)?;
        let frame = machine
            .arena
            .current_handle()
            .ok_or(FrameError::StaleHandle)?;
        machine.arena.descriptor(frame)?.validate_send_operands(
            receiver_slot,
            selector_slot,
            argument_start,
            argument_count,
            None,
        )?;
        let receiver = machine.arena.read_oop(frame, receiver_slot)?;
        let selector = machine.arena.read_oop(frame, selector_slot)?;
        let mut arguments = Vec::with_capacity(argument_count);
        for index in argument_start..argument_start + argument_count {
            arguments.push(machine.arena.read_oop(frame, index)?);
        }
        Ok(SendOperands {
            receiver,
            selector,
            arguments,
        })
    }

    pub(crate) fn publish_continue(
        &mut self,
        heap: &mut Heap,
        owner: u64,
        capability: MachineCapability,
        next_pc: usize,
    ) -> Result<Oop, ExecutionError> {
        let objects = self.objects.ok_or(ExecutionError::InvalidAction)?;
        let machine = self.machine_mut(owner, capability)?;
        machine.require_driving(owner)?;
        let frame = machine
            .arena
            .current_handle()
            .ok_or(FrameError::StaleHandle)?;
        if next_pc >= machine.arena.descriptor(frame)?.program_counter_count() {
            return Err(FrameError::InvalidProgramCounter.into());
        }
        let generation = usize::try_from(capability.generation)
            .map_err(|_| MachineError::RegistryGenerationExhausted)?;
        let drive_epoch =
            usize::try_from(machine.drive_epoch).map_err(|_| MachineError::DriveEpochExhausted)?;
        initialize_action(
            heap,
            machine.continue_action,
            objects.continue_action_vtable,
            &[1, capability.index, generation, drive_epoch],
        )?;
        machine.arena.set_program_counter(frame, next_pc)?;
        Ok(machine.continue_action)
    }

    pub(crate) fn publish_return(
        &mut self,
        heap: &mut Heap,
        owner: u64,
        capability: MachineCapability,
        value_slot: usize,
    ) -> Result<Oop, ExecutionError> {
        let objects = self.objects.ok_or(ExecutionError::InvalidAction)?;
        let machine = self.machine_mut(owner, capability)?;
        machine.require_driving(owner)?;
        let frame = machine
            .arena
            .current_handle()
            .ok_or(FrameError::StaleHandle)?;
        let _ = machine.arena.read_oop(frame, value_slot)?;
        let generation = usize::try_from(capability.generation)
            .map_err(|_| MachineError::RegistryGenerationExhausted)?;
        let drive_epoch =
            usize::try_from(machine.drive_epoch).map_err(|_| MachineError::DriveEpochExhausted)?;
        initialize_action(
            heap,
            machine.return_action,
            objects.return_action_vtable,
            &[1, capability.index, generation, drive_epoch, value_slot],
        )?;
        Ok(machine.return_action)
    }

    pub(crate) fn publish_call_site(
        &mut self,
        heap: &mut Heap,
        owner: u64,
        capability: MachineCapability,
        call_site_index: usize,
        expected_kind: CallSiteKind,
    ) -> Result<Oop, ExecutionError> {
        let objects = self.objects.ok_or(ExecutionError::InvalidAction)?;
        let machine = self.machine_mut(owner, capability)?;
        machine.require_driving(owner)?;
        let frame = machine
            .arena
            .current_handle()
            .ok_or(FrameError::StaleHandle)?;
        let call_site = *machine
            .arena
            .descriptor(frame)?
            .call_site(call_site_index)
            .ok_or(FrameError::InvalidSlot)?;
        if call_site.kind != expected_kind {
            return Err(FrameError::InvalidSlot.into());
        }
        let generation = usize::try_from(capability.generation)
            .map_err(|_| MachineError::RegistryGenerationExhausted)?;
        let drive_epoch =
            usize::try_from(machine.drive_epoch).map_err(|_| MachineError::DriveEpochExhausted)?;
        match expected_kind {
            CallSiteKind::Call => {
                let result_slot = call_site.result_slot.ok_or(FrameError::InvalidSlot)?;
                let continuation_pc = call_site
                    .continuation_pc
                    .ok_or(FrameError::InvalidProgramCounter)?;
                // Saving the continuation precedes action publication so a
                // valid CallAction always names a waiting caller.
                machine.arena.set_program_counter(frame, continuation_pc)?;
                initialize_action(
                    heap,
                    machine.call_action,
                    objects.call_action_vtable,
                    &[
                        1,
                        capability.index,
                        generation,
                        drive_epoch,
                        call_site.receiver_slot,
                        call_site.selector_slot,
                        call_site.argument_start,
                        call_site.argument_count,
                        result_slot,
                    ],
                )?;
                Ok(machine.call_action)
            }
            CallSiteKind::TailCall => {
                initialize_action(
                    heap,
                    machine.tail_call_action,
                    objects.tail_call_action_vtable,
                    &[
                        1,
                        capability.index,
                        generation,
                        drive_epoch,
                        call_site.receiver_slot,
                        call_site.selector_slot,
                        call_site.argument_start,
                        call_site.argument_count,
                    ],
                )?;
                Ok(machine.tail_call_action)
            }
        }
    }

    pub(crate) fn decode_action(
        &self,
        heap: &Heap,
        owner: u64,
        capability: MachineCapability,
        action: Oop,
    ) -> Result<DecodedAction, ExecutionError> {
        let objects = self.objects.ok_or(ExecutionError::InvalidAction)?;
        let machine = self.machine(owner, capability)?;
        machine.require_driving(owner)?;
        let vtable = heap
            .vtable_of(action)
            .ok_or(ExecutionError::InvalidAction)?;
        let word_count = if vtable == objects.continue_action_vtable {
            4
        } else if vtable == objects.call_action_vtable {
            9
        } else if vtable == objects.tail_call_action_vtable {
            8
        } else if vtable == objects.return_action_vtable {
            5
        } else {
            return Err(ExecutionError::InvalidAction);
        };
        if heap.payload_size(action) != Some(word_count * size_of::<usize>())
            || heap.read_payload_word(action, 0) != Some(1)
        {
            return Err(ExecutionError::InvalidAction);
        }
        let generation = usize::try_from(capability.generation)
            .map_err(|_| MachineError::RegistryGenerationExhausted)?;
        let drive_epoch =
            usize::try_from(machine.drive_epoch).map_err(|_| MachineError::DriveEpochExhausted)?;
        if heap.read_payload_word(action, 1) != Some(capability.index)
            || heap.read_payload_word(action, 2) != Some(generation)
        {
            return Err(ExecutionError::ActionWrongMachine);
        }
        if heap.read_payload_word(action, 3) != Some(drive_epoch) {
            return Err(ExecutionError::StaleAction);
        }
        if vtable == objects.continue_action_vtable {
            Ok(DecodedAction::Continue)
        } else if vtable == objects.return_action_vtable {
            let value_slot = heap
                .read_payload_word(action, 4)
                .ok_or(ExecutionError::InvalidAction)?;
            let frame = machine
                .arena
                .current_handle()
                .ok_or(FrameError::StaleHandle)?;
            let _ = machine.arena.read_oop(frame, value_slot)?;
            Ok(DecodedAction::Return { value_slot })
        } else {
            let receiver_slot = heap
                .read_payload_word(action, 4)
                .ok_or(ExecutionError::InvalidAction)?;
            let selector_slot = heap
                .read_payload_word(action, 5)
                .ok_or(ExecutionError::InvalidAction)?;
            let argument_start = heap
                .read_payload_word(action, 6)
                .ok_or(ExecutionError::InvalidAction)?;
            let argument_count = heap
                .read_payload_word(action, 7)
                .ok_or(ExecutionError::InvalidAction)?;
            let result_slot = if vtable == objects.call_action_vtable {
                Some(
                    heap.read_payload_word(action, 8)
                        .ok_or(ExecutionError::InvalidAction)?,
                )
            } else {
                None
            };
            let frame = machine
                .arena
                .current_handle()
                .ok_or(FrameError::StaleHandle)?;
            machine.arena.descriptor(frame)?.validate_send_operands(
                receiver_slot,
                selector_slot,
                argument_start,
                argument_count,
                result_slot,
            )?;
            if let Some(result_slot) = result_slot {
                Ok(DecodedAction::Call {
                    receiver_slot,
                    selector_slot,
                    argument_start,
                    argument_count,
                    result_slot,
                })
            } else {
                Ok(DecodedAction::TailCall {
                    receiver_slot,
                    selector_slot,
                    argument_start,
                    argument_count,
                })
            }
        }
    }

    pub(crate) fn action_kind(&self, heap: &Heap, action: Oop) -> Option<Oop> {
        let objects = self.objects?;
        match heap.vtable_of(action)? {
            vtable if vtable == objects.continue_action_vtable => Some(objects.continue_kind),
            vtable if vtable == objects.call_action_vtable => Some(objects.call_kind),
            vtable if vtable == objects.tail_call_action_vtable => Some(objects.tail_call_kind),
            vtable if vtable == objects.return_action_vtable => Some(objects.return_kind),
            _ => None,
        }
    }

    pub(crate) fn action_word(&self, heap: &Heap, action: Oop, index: usize) -> Option<usize> {
        let objects = self.objects?;
        let expected_words = match heap.vtable_of(action)? {
            vtable if vtable == objects.continue_action_vtable => 4,
            vtable if vtable == objects.call_action_vtable => 9,
            vtable if vtable == objects.tail_call_action_vtable => 8,
            vtable if vtable == objects.return_action_vtable => 5,
            _ => return None,
        };
        if heap.payload_size(action)? != expected_words * size_of::<usize>()
            || index >= expected_words
        {
            return None;
        }
        heap.read_payload_word(action, index)
    }

    pub(crate) fn current_frame_view_handle(
        &self,
        owner: u64,
        capability: MachineCapability,
    ) -> Result<Option<FrameHandle>, FrameViewError> {
        let machine = self
            .machine(owner, capability)
            .map_err(|_| FrameViewError::StaleFrameView)?;
        Ok(machine.arena.current_handle())
    }

    pub(crate) fn initialize_frame_view(
        &self,
        heap: &mut Heap,
        view: Oop,
        machine: Oop,
        handle: FrameHandle,
    ) -> Result<(), FrameViewError> {
        let objects = self.objects.ok_or(FrameViewError::InvalidView)?;
        if heap.vtable_of(view) != Some(objects.frame_view_vtable)
            || heap.payload_size(view) != Some(4 * size_of::<usize>())
        {
            return Err(FrameViewError::InvalidView);
        }
        let [arena, index, generation] = handle.words()?;
        if heap.write_payload_word(view, 0, machine.addr())
            && heap.write_payload_word(view, 1, arena)
            && heap.write_payload_word(view, 2, index)
            && heap.write_payload_word(view, 3, generation)
        {
            Ok(())
        } else {
            Err(FrameViewError::InvalidView)
        }
    }

    pub(crate) fn frame_view_snapshot(
        &self,
        heap: &Heap,
        owner: u64,
        view: Oop,
    ) -> Result<FrameViewSnapshot, FrameViewError> {
        let (machine, handle) = self.frame_for_view(heap, owner, view)?;
        let descriptor = machine
            .arena
            .descriptor(handle)
            .map_err(|_| FrameViewError::StaleFrameView)?;
        Ok(FrameViewSnapshot {
            identity: descriptor.identity(),
            program_counter: machine
                .arena
                .program_counter(handle)
                .map_err(|_| FrameViewError::StaleFrameView)?,
            slot_count: descriptor.slot_count(),
        })
    }

    pub(crate) fn frame_view_slot_kind(
        &self,
        heap: &Heap,
        owner: u64,
        view: Oop,
        index: usize,
    ) -> Result<SlotKind, FrameViewError> {
        let (machine, handle) = self.frame_for_view(heap, owner, view)?;
        machine
            .arena
            .descriptor(handle)
            .map_err(|_| FrameViewError::StaleFrameView)?
            .slot_kind(index)
            .ok_or(FrameViewError::InvalidFrameSlot)
    }

    pub(crate) fn frame_view_oop(
        &self,
        heap: &Heap,
        owner: u64,
        view: Oop,
        index: usize,
    ) -> Result<Oop, FrameViewError> {
        let (machine, handle) = self.frame_for_view(heap, owner, view)?;
        machine
            .arena
            .read_oop(handle, index)
            .map_err(|error| match error {
                FrameError::InvalidSlot | FrameError::WrongSlotKind => {
                    FrameViewError::InvalidFrameSlot
                }
                FrameError::StaleHandle
                | FrameError::ArenaDestroyed
                | FrameError::GenerationExhausted
                | FrameError::InvalidProgramCounter => FrameViewError::StaleFrameView,
            })
    }

    fn frame_for_view<'a>(
        &'a self,
        heap: &Heap,
        owner: u64,
        view: Oop,
    ) -> Result<(&'a MachineState, FrameHandle), FrameViewError> {
        self.require_owner(owner)
            .map_err(|_| FrameViewError::StaleFrameView)?;
        let objects = self.objects.ok_or(FrameViewError::InvalidView)?;
        if heap.vtable_of(view) != Some(objects.frame_view_vtable)
            || heap.payload_size(view) != Some(4 * size_of::<usize>())
        {
            return Err(FrameViewError::InvalidView);
        }
        let machine_object = heap
            .read_payload_word(view, 0)
            .map(core::ptr::with_exposed_provenance_mut)
            .ok_or(FrameViewError::InvalidView)?;
        let capability = self
            .capability_for_object(heap, machine_object)
            .map_err(|_| FrameViewError::StaleFrameView)?;
        let handle = FrameHandle::from_words(
            heap.read_payload_word(view, 1)
                .ok_or(FrameViewError::InvalidView)?,
            heap.read_payload_word(view, 2)
                .ok_or(FrameViewError::InvalidView)?,
            heap.read_payload_word(view, 3)
                .ok_or(FrameViewError::InvalidView)?,
        );
        let machine = self
            .machine(owner, capability)
            .map_err(|_| FrameViewError::StaleFrameView)?;
        machine
            .arena
            .require_live(handle)
            .map_err(|_| FrameViewError::StaleFrameView)?;
        Ok((machine, handle))
    }

    fn require_owner(&self, owner: u64) -> Result<(), MachineError> {
        if self.owner == owner {
            Ok(())
        } else {
            Err(MachineError::WrongOwner)
        }
    }
}

fn initialize_action(
    heap: &mut Heap,
    action: Oop,
    expected_vtable: Oop,
    words: &[usize],
) -> Result<(), ExecutionError> {
    if heap.vtable_of(action) != Some(expected_vtable)
        || heap.payload_size(action) != Some(size_of_val(words))
    {
        return Err(ExecutionError::InvalidAction);
    }
    for (index, word) in words.iter().copied().enumerate() {
        if !heap.write_payload_word(action, index, word) {
            return Err(ExecutionError::InvalidAction);
        }
    }
    Ok(())
}

fn nonnegative_index(runtime: &crate::runtime::Runtime, operand: Oop) -> Option<usize> {
    if !runtime.is_integer(operand) {
        return None;
    }
    usize::try_from(runtime.integer_value(operand)).ok()
}

pub(crate) unsafe extern "C" fn machine_new(_: Oop, _: *const Oop, count: usize) -> Oop {
    crate::runtime::with_active(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        runtime
            .create_stackless_machine()
            .unwrap_or_else(|_| runtime.canonical().nil)
    })
}

pub(crate) unsafe extern "C" fn machine_clone_disabled(_: Oop, _: *const Oop, _: usize) -> Oop {
    crate::runtime::with_active(|runtime| runtime.canonical().nil)
}

pub(crate) unsafe extern "C" fn machine_oop_at(
    receiver: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    crate::runtime::with_active(|runtime| {
        if count != 1 || args.is_null() {
            return runtime.canonical().nil;
        }
        // SAFETY: the Method ABI promises one live argument for count == 1.
        let operand = unsafe { args.read() };
        let Some(index) = nonnegative_index(runtime, operand) else {
            return runtime.canonical().nil;
        };
        runtime
            .stackless_oop_at(receiver, index)
            .unwrap_or_else(|_| runtime.canonical().nil)
    })
}

pub(crate) unsafe extern "C" fn machine_oop_at_put(
    receiver: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    crate::runtime::with_active(|runtime| {
        if count != 2 || args.is_null() {
            return runtime.canonical().nil;
        }
        // SAFETY: the Method ABI promises two live arguments for count == 2.
        let index_operand = unsafe { args.read() };
        // SAFETY: the second argument is within the same live buffer.
        let value = unsafe { args.add(1).read() };
        let Some(index) = nonnegative_index(runtime, index_operand) else {
            return runtime.canonical().nil;
        };
        runtime
            .stackless_oop_at_put(receiver, index, value)
            .unwrap_or_else(|_| runtime.canonical().nil)
    })
}

pub(crate) unsafe extern "C" fn machine_continue_at(
    receiver: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    crate::runtime::with_active(|runtime| {
        if count != 1 || args.is_null() {
            return runtime.canonical().nil;
        }
        // SAFETY: the Method ABI promises one live argument for count == 1.
        let operand = unsafe { args.read() };
        let Some(program_counter) = nonnegative_index(runtime, operand) else {
            return runtime.canonical().nil;
        };
        runtime
            .stackless_continue_at(receiver, program_counter)
            .unwrap_or_else(|_| runtime.canonical().nil)
    })
}

fn machine_call_site_method(
    receiver: Oop,
    args: *const Oop,
    count: usize,
    kind: CallSiteKind,
) -> Oop {
    crate::runtime::with_active(|runtime| {
        if count != 1 || args.is_null() {
            return runtime.canonical().nil;
        }
        // SAFETY: the Method ABI promises one live argument for count == 1.
        let operand = unsafe { args.read() };
        let Some(call_site_index) = nonnegative_index(runtime, operand) else {
            return runtime.canonical().nil;
        };
        runtime
            .stackless_call_site(receiver, call_site_index, kind)
            .unwrap_or_else(|_| runtime.canonical().nil)
    })
}

pub(crate) unsafe extern "C" fn machine_call_site(
    receiver: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    machine_call_site_method(receiver, args, count, CallSiteKind::Call)
}

pub(crate) unsafe extern "C" fn machine_tail_call_site(
    receiver: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    machine_call_site_method(receiver, args, count, CallSiteKind::TailCall)
}

pub(crate) unsafe extern "C" fn machine_return_at(
    receiver: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    crate::runtime::with_active(|runtime| {
        if count != 1 || args.is_null() {
            return runtime.canonical().nil;
        }
        // SAFETY: the Method ABI promises one live argument for count == 1.
        let operand = unsafe { args.read() };
        let Some(value_slot) = nonnegative_index(runtime, operand) else {
            return runtime.canonical().nil;
        };
        runtime
            .stackless_return_at(receiver, value_slot)
            .unwrap_or_else(|_| runtime.canonical().nil)
    })
}

pub(crate) unsafe extern "C" fn machine_current_frame_view(
    receiver: Oop,
    _: *const Oop,
    count: usize,
) -> Oop {
    crate::runtime::with_active(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        runtime
            .stackless_current_frame_view(receiver)
            .unwrap_or_else(|error| runtime.stackless_frame_view_error(error))
    })
}

fn frame_view_identity_method(view: Oop, count: usize, field: usize) -> Oop {
    crate::runtime::with_active(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        runtime
            .stackless_frame_view_identity_word(view, field)
            .unwrap_or_else(|error| runtime.stackless_frame_view_error(error))
    })
}

fn frame_view_index_method(view: Oop, count: usize, field: usize) -> Oop {
    crate::runtime::with_active(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        runtime
            .stackless_frame_view_index_word(view, field)
            .unwrap_or_else(|error| runtime.stackless_frame_view_error(error))
    })
}

pub(crate) unsafe extern "C" fn frame_view_implementation_identity(
    view: Oop,
    _: *const Oop,
    count: usize,
) -> Oop {
    frame_view_identity_method(view, count, 0)
}

pub(crate) unsafe extern "C" fn frame_view_generation_identity(
    view: Oop,
    _: *const Oop,
    count: usize,
) -> Oop {
    frame_view_identity_method(view, count, 1)
}

pub(crate) unsafe extern "C" fn frame_view_method_identity(
    view: Oop,
    _: *const Oop,
    count: usize,
) -> Oop {
    frame_view_identity_method(view, count, 2)
}

pub(crate) unsafe extern "C" fn frame_view_source_map_identity(
    view: Oop,
    _: *const Oop,
    count: usize,
) -> Oop {
    frame_view_identity_method(view, count, 3)
}

pub(crate) unsafe extern "C" fn frame_view_source_map_location(
    view: Oop,
    _: *const Oop,
    count: usize,
) -> Oop {
    frame_view_index_method(view, count, 0)
}

pub(crate) unsafe extern "C" fn frame_view_program_counter(
    view: Oop,
    _: *const Oop,
    count: usize,
) -> Oop {
    frame_view_index_method(view, count, 1)
}

pub(crate) unsafe extern "C" fn frame_view_slot_count(
    view: Oop,
    _: *const Oop,
    count: usize,
) -> Oop {
    frame_view_index_method(view, count, 2)
}

pub(crate) unsafe extern "C" fn frame_view_slot_kind_at(
    view: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    crate::runtime::with_active(|runtime| {
        if count != 1 || args.is_null() {
            return runtime.canonical().nil;
        }
        // SAFETY: the Method ABI promises one live argument for count == 1.
        let operand = unsafe { args.read() };
        let Some(index) = nonnegative_index(runtime, operand) else {
            return runtime.stackless_frame_view_error(FrameViewError::InvalidFrameSlot);
        };
        runtime
            .stackless_frame_view_slot_kind(view, index)
            .unwrap_or_else(|error| runtime.stackless_frame_view_error(error))
    })
}

pub(crate) unsafe extern "C" fn frame_view_oop_at(
    view: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    crate::runtime::with_active(|runtime| {
        if count != 1 || args.is_null() {
            return runtime.canonical().nil;
        }
        // SAFETY: the Method ABI promises one live argument for count == 1.
        let operand = unsafe { args.read() };
        let Some(index) = nonnegative_index(runtime, operand) else {
            return runtime.stackless_frame_view_error(FrameViewError::InvalidFrameSlot);
        };
        runtime
            .stackless_frame_view_oop(view, index)
            .unwrap_or_else(|error| runtime.stackless_frame_view_error(error))
    })
}

pub(crate) unsafe extern "C" fn machine_trace_hook(object: Oop, mark: MarkFn) {
    crate::runtime::with_active(|runtime| runtime.trace_stackless_machine(object, mark));
}

pub(crate) unsafe extern "C" fn execution_root_provider(mark: MarkFn) {
    crate::runtime::with_active(|runtime| runtime.trace_stackless_scoped_roots(mark));
}

pub(crate) unsafe extern "C" fn machine_finalizer(object: Oop) {
    crate::runtime::with_active(|runtime| {
        // SAFETY: the heap invokes this finalizer while the detached Machine
        // payload is still live and reclaims it only after this call returns.
        unsafe { runtime.finalize_stackless_machine(object) }
    });
}

pub(crate) unsafe extern "C" fn action_kind(action: Oop, _: *const Oop, count: usize) -> Oop {
    crate::runtime::with_active(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        runtime
            .stackless_action_kind(action)
            .unwrap_or_else(|| runtime.canonical().nil)
    })
}

fn action_word_method(action: Oop, count: usize, index: usize) -> Oop {
    crate::runtime::with_active(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        runtime.stackless_action_integer_word(action, index)
    })
}

fn machine_diagnostic_method(machine: Oop, count: usize, index: usize) -> Oop {
    crate::runtime::with_active(|runtime| {
        if count != 0 {
            return runtime.canonical().nil;
        }
        runtime.stackless_machine_diagnostic_word(machine, index)
    })
}

pub(crate) unsafe extern "C" fn machine_live_frame_count(
    machine: Oop,
    _: *const Oop,
    count: usize,
) -> Oop {
    machine_diagnostic_method(machine, count, 0)
}

pub(crate) unsafe extern "C" fn machine_high_water_live_frame_count(
    machine: Oop,
    _: *const Oop,
    count: usize,
) -> Oop {
    machine_diagnostic_method(machine, count, 1)
}

pub(crate) unsafe extern "C" fn machine_reserved_arena_bytes(
    machine: Oop,
    _: *const Oop,
    count: usize,
) -> Oop {
    machine_diagnostic_method(machine, count, 2)
}

pub(crate) unsafe extern "C" fn machine_current_synchronous_boundary_depth(
    machine: Oop,
    _: *const Oop,
    count: usize,
) -> Oop {
    machine_diagnostic_method(machine, count, 3)
}

pub(crate) unsafe extern "C" fn machine_high_water_synchronous_boundary_depth(
    machine: Oop,
    _: *const Oop,
    count: usize,
) -> Oop {
    machine_diagnostic_method(machine, count, 4)
}

pub(crate) unsafe extern "C" fn action_schema_version(
    action: Oop,
    _: *const Oop,
    count: usize,
) -> Oop {
    action_word_method(action, count, 0)
}

pub(crate) unsafe extern "C" fn action_machine_index(
    action: Oop,
    _: *const Oop,
    count: usize,
) -> Oop {
    action_word_method(action, count, 1)
}

pub(crate) unsafe extern "C" fn action_machine_generation(
    action: Oop,
    _: *const Oop,
    count: usize,
) -> Oop {
    action_word_method(action, count, 2)
}

pub(crate) unsafe extern "C" fn action_drive_epoch(
    action: Oop,
    _: *const Oop,
    count: usize,
) -> Oop {
    action_word_method(action, count, 3)
}

pub(crate) unsafe extern "C" fn return_action_value_slot(
    action: Oop,
    _: *const Oop,
    count: usize,
) -> Oop {
    action_word_method(action, count, 4)
}

pub(crate) unsafe extern "C" fn action_receiver_slot(
    action: Oop,
    _: *const Oop,
    count: usize,
) -> Oop {
    action_word_method(action, count, 4)
}

pub(crate) unsafe extern "C" fn action_selector_slot(
    action: Oop,
    _: *const Oop,
    count: usize,
) -> Oop {
    action_word_method(action, count, 5)
}

pub(crate) unsafe extern "C" fn action_argument_start(
    action: Oop,
    _: *const Oop,
    count: usize,
) -> Oop {
    action_word_method(action, count, 6)
}

pub(crate) unsafe extern "C" fn action_argument_count(
    action: Oop,
    _: *const Oop,
    count: usize,
) -> Oop {
    action_word_method(action, count, 7)
}

pub(crate) unsafe extern "C" fn call_action_result_slot(
    action: Oop,
    _: *const Oop,
    count: usize,
) -> Oop {
    action_word_method(action, count, 8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ffi::c_void;

    unsafe extern "C" fn inert_step(_: Oop, _: *const Oop, _: usize) -> Oop {
        core::ptr::null_mut()
    }

    fn oop(value: usize) -> Oop {
        core::ptr::with_exposed_provenance_mut(value)
    }

    fn descriptor(slot_kinds: Vec<SlotKind>, pc_count: usize) -> Arc<FrameDescriptor> {
        FrameDescriptor::validate(FrameDescriptorSpec {
            identity: DescriptorIdentity {
                implementation: 11,
                generation: 7,
                source_map: 3,
            },
            layout_version: FRAME_LAYOUT_VERSION_V1,
            program_counter_count: pc_count,
            driver_argument_count: usize::from(pc_count > 1),
            slot_kinds,
            call_sites: Vec::new(),
            step_method: inert_step,
        })
        .expect("valid test descriptor")
    }

    #[test]
    fn resolve_and_classify_fast_path_has_no_allocation_lock_atomic_or_refcount_surface() {
        fn function_body<'a>(source: &'a str, start: &str, next: &str) -> &'a str {
            let start = source
                .find(start)
                .expect("audited function remains present");
            let tail = &source[start..];
            let end = tail
                .find(next)
                .expect("audited function retains its structural successor");
            &tail[..end]
        }

        let execution_source = include_str!("execution.rs");
        let classification = function_body(
            execution_source,
            "    pub(crate) fn classify_resumable(",
            "    pub(crate) fn materialize_acquired_entry(",
        );
        let read_entry = function_body(
            execution_source,
            "    pub(crate) fn begin_dispatch_read(",
            "    pub(crate) fn end_dispatch_read(",
        );
        let runtime_source = include_str!("runtime.rs");
        let resolution = function_body(
            runtime_source,
            "    fn resolve_stackless_target(",
            "    fn materialize_registered_target(",
        );

        for (name, body) in [
            ("dispatch read entry", read_entry),
            ("registry classification", classification),
            ("resolve-and-pin", resolution),
        ] {
            for forbidden in [
                "Vec", "Arc", "Rc", "Box", ".clone(", "Mutex", "Atomic", ".lock(", "fetch_",
            ] {
                assert!(
                    !body.contains(forbidden),
                    "{name} introduced forbidden fast-path surface {forbidden}"
                );
            }
        }
    }

    #[test]
    fn descriptor_rejects_bad_arity_ranges_kinds_and_continuations() {
        let base = || FrameDescriptorSpec {
            identity: DescriptorIdentity {
                implementation: 1,
                generation: 2,
                source_map: 3,
            },
            layout_version: FRAME_LAYOUT_VERSION_V1,
            program_counter_count: 2,
            driver_argument_count: 1,
            slot_kinds: vec![SlotKind::Oop, SlotKind::Oop, SlotKind::Raw, SlotKind::Oop],
            call_sites: Vec::new(),
            step_method: inert_step,
        };

        let mut invalid = base();
        invalid.driver_argument_count = 2;
        assert!(matches!(
            FrameDescriptor::validate(invalid),
            Err(DescriptorError::InvalidDriverArgumentCount)
        ));

        let mut invalid = base();
        invalid.call_sites.push(CallSiteDescriptor {
            kind: CallSiteKind::Call,
            receiver_slot: 0,
            selector_slot: 1,
            argument_start: 1,
            argument_count: 2,
            result_slot: Some(3),
            continuation_pc: Some(1),
        });
        assert!(matches!(
            FrameDescriptor::validate(invalid),
            Err(DescriptorError::InvalidArgumentSlotKind)
        ));

        let mut invalid = base();
        invalid.call_sites.push(CallSiteDescriptor {
            kind: CallSiteKind::Call,
            receiver_slot: 0,
            selector_slot: 1,
            argument_start: 3,
            argument_count: 1,
            result_slot: Some(3),
            continuation_pc: Some(1),
        });
        assert!(matches!(
            FrameDescriptor::validate(invalid),
            Err(DescriptorError::ResultOverlapsArguments)
        ));

        let mut invalid = base();
        invalid.call_sites.push(CallSiteDescriptor {
            kind: CallSiteKind::Call,
            receiver_slot: 0,
            selector_slot: 1,
            argument_start: 3,
            argument_count: 0,
            result_slot: Some(3),
            continuation_pc: Some(2),
        });
        assert!(matches!(
            FrameDescriptor::validate(invalid),
            Err(DescriptorError::InvalidContinuation)
        ));

        let mut invalid = base();
        invalid.program_counter_count = 2;
        invalid.driver_argument_count = 0;
        assert!(matches!(
            FrameDescriptor::validate(invalid),
            Err(DescriptorError::ZeroArgumentDescriptorHasMultipleEntries)
        ));
    }

    #[test]
    fn frame_push_initializes_oop_roots_and_pop_invalidates_reused_handles() {
        let nil = oop(0x1000);
        let first_root = oop(0x2000);
        let second_root = oop(0x3000);
        let descriptor = descriptor(vec![SlotKind::Oop, SlotKind::Raw, SlotKind::Oop], 2);
        let mut arena = FrameArena::new(19);

        let first = arena
            .push(descriptor.clone(), 0, nil, &[(0, first_root)])
            .expect("first frame push");
        assert_eq!(arena.read_oop(first, 0), Ok(first_root));
        assert_eq!(arena.read_oop(first, 2), Ok(nil));
        assert_eq!(arena.read_oop(first, 1), Err(FrameError::WrongSlotKind));
        let mut roots = Vec::new();
        arena.trace_roots(|root| roots.push(root.addr()));
        assert_eq!(roots, vec![first_root.addr(), nil.addr()]);

        let reserved = arena.diagnostics().reserved_bytes;
        assert!(reserved > 0);
        assert_eq!(arena.pop(), Ok(first));
        assert_eq!(arena.read_oop(first, 0), Err(FrameError::StaleHandle));
        roots.clear();
        arena.trace_roots(|root| roots.push(root.addr()));
        assert!(roots.is_empty(), "retained free capacity is not traced");
        assert_eq!(arena.diagnostics().live_frames, 0);
        assert!(arena.diagnostics().reserved_bytes >= reserved);

        let second = arena
            .push(descriptor, 1, nil, &[(2, second_root)])
            .expect("reused frame push");
        assert_eq!(second.index, first.index);
        assert_ne!(second.generation, first.generation);
        assert_eq!(arena.read_oop(first, 2), Err(FrameError::StaleHandle));
        assert_eq!(arena.read_oop(second, 0), Ok(nil));
        assert_eq!(arena.read_oop(second, 2), Ok(second_root));
        assert_eq!(arena.diagnostics().high_water_live_frames, 1);

        assert!(arena.destroy().is_empty());
        assert_eq!(arena.read_oop(second, 2), Err(FrameError::ArenaDestroyed));
        assert_eq!(arena.diagnostics().reserved_bytes, 0);
    }

    #[test]
    fn checked_frame_operations_validate_before_mutation() {
        let nil = oop(0x1000);
        let original = oop(0x2000);
        let replacement = oop(0x3000);
        let descriptor = descriptor(vec![SlotKind::Oop, SlotKind::Raw], 2);
        let mut arena = FrameArena::new(23);
        let frame = arena
            .push(descriptor, 0, nil, &[(0, original)])
            .expect("frame push");

        assert_eq!(
            arena.write_oop(frame, 1, replacement),
            Err(FrameError::WrongSlotKind)
        );
        assert_eq!(arena.read_oop(frame, 0), Ok(original));
        assert_eq!(
            arena.set_program_counter(frame, 2),
            Err(FrameError::InvalidProgramCounter)
        );
        assert_eq!(arena.program_counter(frame), Ok(0));

        arena.write_oop(frame, 0, replacement).unwrap();
        arena.set_program_counter(frame, 1).unwrap();
        assert_eq!(arena.read_oop(frame, 0), Ok(replacement));
        assert_eq!(arena.program_counter(frame), Ok(1));
    }

    #[test]
    fn machine_registry_enforces_owner_lifecycle_and_generational_reuse() {
        let owner = 71;
        let mut execution = ExecutionState::new(owner);
        assert_eq!(
            execution.create_machine(owner + 1),
            Err(MachineError::WrongOwner)
        );

        let first = execution.create_machine(owner).expect("first machine");
        assert_eq!(
            execution
                .machine_mut(owner, first)
                .expect("live machine")
                .begin_initial_drive(owner),
            Ok(1)
        );
        assert_eq!(
            execution.machine(owner + 1, first).map(|_| ()),
            Err(MachineError::WrongOwner),
            "a competing host owner cannot enter a Running Machine"
        );
        let machine = execution
            .machine_mut(owner, first)
            .expect("owner retains live machine");
        assert_eq!(
            machine.begin_initial_drive(owner),
            Err(MachineError::InvalidLifecycle)
        );
        assert_eq!(
            machine.complete_initial_drive(owner),
            Ok(()),
            "an empty initial drive can complete"
        );
        assert_eq!(machine.lifecycle(), MachineLifecycle::Completed);

        execution
            .destroy_machine(owner, first)
            .expect("destroy first machine");
        assert!(matches!(
            execution.machine(owner, first),
            Err(MachineError::StaleCapability)
        ));

        let second = execution
            .create_machine(owner)
            .expect("reused machine slot");
        assert_eq!(second.index, first.index);
        assert_ne!(second.generation, first.generation);
        assert_eq!(
            execution.machine(owner + 1, second).map(|_| ()),
            Err(MachineError::WrongOwner)
        );
    }

    #[test]
    fn oop_test_values_are_pointer_sized() {
        let _: *mut c_void = oop(0x1000);
    }
}
