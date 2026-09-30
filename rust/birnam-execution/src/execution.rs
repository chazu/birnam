//! Retained Machine/frame engine, now owned by the production Execution instance.
//!
//! This module deliberately contains no dispatch or code-loader policy. It
//! owns Machine capabilities, validated frame descriptors, and retained frame
//! arenas. `machine` connects these algorithms to Id-owned objects and messages.
#![allow(dead_code)] // Preserve private recovery/optimizer consumers for batch 2.

use birnam_artifact_codec::frame_descriptor as canonical_descriptor;
use birnam_artifact_codec::materialization_selection::Row as PortableRow;
use birnam_artifact_codec::stackless_selection::SelectionSpec;
use core::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;

use crate::entry::EntryKey as MethodAddress;
use crate::objects::Objects as Heap;
use birnam_abi::{Oop, id_protocol::NativeEntry as Method};
use std::collections::HashMap;

#[path = "execution/boundary.rs"]
pub(crate) mod boundary;
#[cfg(test)]
#[path = "execution/inline_recovery_experiment.rs"]
mod inline_recovery_experiment;
#[path = "execution/loop_iteration.rs"]
pub(crate) mod loop_iteration;
#[path = "execution/recovery.rs"]
pub(crate) mod recovery;
mod scheduling;
#[path = "execution/tail_entry.rs"]
pub(crate) mod tail_entry;

#[cfg(test)]
#[path = "execution/probe_descriptor.rs"]
mod probe_descriptor;
#[cfg(test)]
pub(crate) use probe_descriptor::{probe_continue, probe_spec};

pub(crate) const FRAME_LAYOUT_VERSION_V2: usize = 2;
pub(crate) const REGISTRATION_SCHEMA_VERSION_V1: usize = 1;
const MACHINE_CAPABILITY_KIND_V1: usize = 0x4d41_4348_0000_0001;
const CONTINUATION_CAPABILITY_KIND_V1: usize = 0x434f_4e54_0000_0001;

#[derive(Clone, Copy)]
pub(crate) struct ExecutionObjects {
    pub(crate) machine_vtable: Oop,
    pub(crate) machine_prototype: Oop,
    pub(crate) frame_view_vtable: Oop,
    pub(crate) frame_view_prototype: Oop,
    pub(crate) continuation_vtable: Oop,
    pub(crate) continuation_prototype: Oop,
    pub(crate) continue_action_vtable: Oop,
    pub(crate) continue_action_prototype: Oop,
    pub(crate) call_action_vtable: Oop,
    pub(crate) call_action_prototype: Oop,
    pub(crate) tail_call_action_vtable: Oop,
    pub(crate) tail_call_action_prototype: Oop,
    pub(crate) return_action_vtable: Oop,
    pub(crate) return_action_prototype: Oop,
    pub(crate) suspend_action_vtable: Oop,
    pub(crate) suspend_action_prototype: Oop,
    pub(crate) capture_action_vtable: Oop,
    pub(crate) capture_action_prototype: Oop,
    pub(crate) restore_action_vtable: Oop,
    pub(crate) restore_action_prototype: Oop,
    pub(crate) boundary_call_action_vtable: Oop,
    pub(crate) boundary_call_action_prototype: Oop,
    pub(crate) exit_boundary_action_vtable: Oop,
    pub(crate) exit_boundary_action_prototype: Oop,
    pub(crate) boundary_call_kind: Oop,
    pub(crate) exit_boundary_kind: Oop,
    pub(crate) abort_action_vtable: Oop,
    pub(crate) abort_action_prototype: Oop,
    pub(crate) continue_kind: Oop,
    pub(crate) call_kind: Oop,
    pub(crate) tail_call_kind: Oop,
    pub(crate) return_kind: Oop,
    pub(crate) suspend_kind: Oop,
    pub(crate) capture_kind: Oop,
    pub(crate) restore_kind: Oop,
    pub(crate) abort_kind: Oop,
    pub(crate) oop_slot_kind: Oop,
    pub(crate) stale_frame_view_kind: Oop,
    pub(crate) invalid_frame_slot_kind: Oop,
    pub(crate) oop_at: Oop,
    pub(crate) oop_at_put: Oop,
    pub(crate) continue_at: Oop,
    pub(crate) call_site: Oop,
    pub(crate) tail_call_site: Oop,
    pub(crate) return_at: Oop,
}

impl ExecutionObjects {
    pub(crate) fn roots(self) -> Vec<Oop> {
        vec![
            self.machine_vtable,
            self.machine_prototype,
            self.frame_view_vtable,
            self.frame_view_prototype,
            self.continuation_vtable,
            self.continuation_prototype,
            self.continue_action_vtable,
            self.continue_action_prototype,
            self.call_action_vtable,
            self.call_action_prototype,
            self.tail_call_action_vtable,
            self.tail_call_action_prototype,
            self.return_action_vtable,
            self.return_action_prototype,
            self.suspend_action_vtable,
            self.suspend_action_prototype,
            self.capture_action_vtable,
            self.capture_action_prototype,
            self.restore_action_vtable,
            self.restore_action_prototype,
            self.boundary_call_action_vtable,
            self.boundary_call_action_prototype,
            self.exit_boundary_action_vtable,
            self.exit_boundary_action_prototype,
            self.boundary_call_kind,
            self.exit_boundary_kind,
            self.abort_action_vtable,
            self.abort_action_prototype,
            self.continue_kind,
            self.call_kind,
            self.tail_call_kind,
            self.return_kind,
            self.suspend_kind,
            self.capture_kind,
            self.restore_kind,
            self.abort_kind,
            self.oop_slot_kind,
            self.stale_frame_view_kind,
            self.invalid_frame_slot_kind,
            self.oop_at,
            self.oop_at_put,
            self.continue_at,
            self.call_site,
            self.tail_call_site,
            self.return_at,
        ]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SlotKind {
    Oop,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EdgeTransitionKind {
    Continue,
    Call,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SlotCopy {
    pub(crate) source: usize,
    pub(crate) destination: usize,
}

pub(crate) struct FrameEdgeSpec {
    pub(crate) kind: EdgeTransitionKind,
    pub(crate) source_pc: usize,
    pub(crate) target_pc: usize,
    pub(crate) call_site_index: Option<usize>,
    pub(crate) result_slot: Option<usize>,
    pub(crate) copies: Vec<SlotCopy>,
    pub(crate) clear_bitmap_words: Vec<u64>,
}

pub(crate) struct FrameDescriptorV2Spec {
    pub(crate) identity: DescriptorIdentity,
    pub(crate) program_counter_count: usize,
    pub(crate) driver_argument_count: usize,
    pub(crate) slot_count: usize,
    pub(crate) entry_receiver_slot: usize,
    pub(crate) entry_argument_start: usize,
    pub(crate) entry_argument_count: usize,
    pub(crate) entry_capture_slots: Vec<usize>,
    pub(crate) implementation_slot: Option<usize>,
    pub(crate) root_bitmap_words: Vec<u64>,
    pub(crate) call_sites: Vec<CallSiteDescriptor>,
    pub(crate) edges: Vec<FrameEdgeSpec>,
    pub(crate) step_method: Method,
}

struct FrameEdge {
    kind: EdgeTransitionKind,
    source_pc: usize,
    target_pc: usize,
    call_site_index: Option<usize>,
    result_slot: Option<usize>,
    copies: Box<[SlotCopy]>,
    clear_bitmap_words: Box<[u64]>,
}

pub(crate) struct FrameDescriptor {
    pub(crate) executable_owner: core::cell::Cell<Oop>,
    identity: DescriptorIdentity,
    layout_version: usize,
    program_counter_count: usize,
    driver_argument_count: usize,
    slot_kinds: Box<[SlotKind]>,
    call_sites: Box<[CallSiteDescriptor]>,
    root_bitmap_words: Box<[u64]>,
    root_words_per_pc: usize,
    edges: Box<[FrameEdge]>,
    max_edge_copy_count: usize,
    v2_entry_binding: Option<V2EntryBinding>,
    entry_capture_slots: Box<[usize]>,
    tail_entry_plan: std::sync::OnceLock<Option<Box<crate::entry_plan::Plan>>>,
    step_method: Method,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct V2EntryBinding {
    pub(crate) receiver_slot: usize,
    pub(crate) argument_start: usize,
    pub(crate) argument_count: usize,
    pub(crate) implementation_slot: Option<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DescriptorError {
    InvalidCanonicalDescriptor,
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
    InvalidRootBitmap,
    InvalidEdge,
    InvalidCopyPlan,
    InvalidClearBitmap,
}

impl FrameDescriptor {
    #[cfg(test)]
    pub(crate) fn owned_byte_size(&self) -> usize {
        core::mem::size_of::<Self>()
            + core::mem::size_of_val(self.slot_kinds.as_ref())
            + core::mem::size_of_val(self.call_sites.as_ref())
            + core::mem::size_of_val(self.root_bitmap_words.as_ref())
            + core::mem::size_of_val(self.edges.as_ref())
            + self
                .edges
                .iter()
                .map(|edge| {
                    core::mem::size_of_val(edge.copies.as_ref())
                        + core::mem::size_of_val(edge.clear_bitmap_words.as_ref())
                })
                .sum::<usize>()
            + core::mem::size_of_val(self.entry_capture_slots.as_ref())
            + self
                .tail_entry_plan
                .get()
                .and_then(Option::as_ref)
                .map_or(0, |plan| plan.owned_bytes())
    }

    /// Admit canonical descriptor-v2 bytes, copy only Runtime execution
    /// metadata into owned storage, and retain no artifact pointer.
    pub(crate) fn admit_v2(
        bytes: &[u8],
        authority: SelectionSpec<'_>,
        code_generation_identity: u64,
        step_method: Method,
    ) -> Result<Arc<Self>, DescriptorError> {
        let descriptor = canonical_descriptor::inspect(bytes, authority)
            .map_err(|_| DescriptorError::InvalidCanonicalDescriptor)?;
        Self::admit_canonical_v2(descriptor, code_generation_identity, step_method)
    }

    /// Admit the source-independent portable profile of descriptor-v2 bytes.
    pub(crate) fn admit_portable_v2(
        bytes: &[u8],
        row: PortableRow<'_>,
        code_generation_identity: u64,
        step_method: Method,
    ) -> Result<Arc<Self>, DescriptorError> {
        let descriptor = canonical_descriptor::inspect_portable(bytes, row)
            .map_err(|_| DescriptorError::InvalidCanonicalDescriptor)?;
        Self::admit_canonical_v2(descriptor, code_generation_identity, step_method)
    }

    fn admit_canonical_v2(
        descriptor: canonical_descriptor::FrameDescriptor<'_>,
        code_generation_identity: u64,
        step_method: Method,
    ) -> Result<Arc<Self>, DescriptorError> {
        let metadata = descriptor.metadata();
        let words_per_pc = (metadata.slot_count as usize)
            .checked_add(63)
            .ok_or(DescriptorError::InvalidCanonicalDescriptor)?
            / 64;
        let mut root_bitmap_words =
            Vec::with_capacity(metadata.program_counter_count as usize * words_per_pc);
        for pc in 0..metadata.program_counter_count as usize {
            for word in 0..words_per_pc {
                root_bitmap_words.push(
                    descriptor
                        .root_word(pc, word)
                        .ok_or(DescriptorError::InvalidCanonicalDescriptor)?,
                );
            }
        }
        let mut call_sites = Vec::with_capacity(descriptor.call_site_count());
        for index in 0..descriptor.call_site_count() {
            let call = descriptor
                .call_site(index)
                .ok_or(DescriptorError::InvalidCanonicalDescriptor)?;
            call_sites.push(CallSiteDescriptor {
                kind: match call.kind {
                    canonical_descriptor::CallKind::Call => CallSiteKind::Call,
                    canonical_descriptor::CallKind::TailCall => CallSiteKind::TailCall,
                },
                receiver_slot: call.receiver_slot as usize,
                selector_slot: call.selector_slot as usize,
                argument_start: call.argument_start as usize,
                argument_count: call.argument_count as usize,
                result_slot: call.result_slot.map(|slot| slot as usize),
                continuation_pc: call.continuation_pc.map(|pc| pc as usize),
            });
        }
        let mut entry_capture_slots = Vec::with_capacity(descriptor.capture_count());
        for index in 0..descriptor.capture_count() {
            entry_capture_slots.push(
                descriptor
                    .capture_slot(index)
                    .ok_or(DescriptorError::InvalidCanonicalDescriptor)? as usize,
            );
        }
        let mut edges = Vec::with_capacity(descriptor.edge_count());
        for index in 0..descriptor.edge_count() {
            let edge = descriptor
                .edge(index)
                .ok_or(DescriptorError::InvalidCanonicalDescriptor)?;
            let first_copy = edge.first_copy_pair as usize;
            let copy_end = first_copy
                .checked_add(edge.copy_pair_count as usize)
                .ok_or(DescriptorError::InvalidCanonicalDescriptor)?;
            let mut copies = Vec::with_capacity(edge.copy_pair_count as usize);
            for copy_index in first_copy..copy_end {
                let copy = descriptor
                    .copy_pair(copy_index)
                    .ok_or(DescriptorError::InvalidCanonicalDescriptor)?;
                copies.push(SlotCopy {
                    source: copy.source_slot as usize,
                    destination: copy.destination_slot as usize,
                });
            }
            let mut clear_bitmap_words = Vec::with_capacity(words_per_pc);
            for word in 0..words_per_pc {
                clear_bitmap_words.push(
                    descriptor
                        .clear_word(index, word)
                        .ok_or(DescriptorError::InvalidCanonicalDescriptor)?,
                );
            }
            edges.push(FrameEdgeSpec {
                kind: match edge.kind {
                    canonical_descriptor::EdgeKind::Continue => EdgeTransitionKind::Continue,
                    canonical_descriptor::EdgeKind::Call => EdgeTransitionKind::Call,
                },
                source_pc: edge.source_pc as usize,
                target_pc: edge.target_pc as usize,
                call_site_index: edge.call_site_index.map(|call| call as usize),
                result_slot: edge.result_initialization_slot.map(|slot| slot as usize),
                copies,
                clear_bitmap_words,
            });
        }
        Self::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: metadata.portable_implementation_identity,
                generation: code_generation_identity,
                source_map: metadata.source_map_identity,
            },
            program_counter_count: metadata.program_counter_count as usize,
            driver_argument_count: metadata.driver_argument_count as usize,
            slot_count: metadata.slot_count as usize,
            entry_receiver_slot: metadata.entry_receiver_slot as usize,
            entry_argument_start: metadata.entry_argument_start as usize,
            entry_argument_count: metadata.entry_argument_count as usize,
            entry_capture_slots,
            implementation_slot: metadata.implementation_slot.map(|slot| slot as usize),
            root_bitmap_words,
            call_sites,
            edges,
            step_method,
        })
    }

    pub(crate) fn validate_v2(spec: FrameDescriptorV2Spec) -> Result<Arc<Self>, DescriptorError> {
        if spec.program_counter_count == 0 {
            return Err(DescriptorError::EmptyProgramCounterRange);
        }
        if spec.driver_argument_count > 1 {
            return Err(DescriptorError::InvalidDriverArgumentCount);
        }
        if spec.driver_argument_count == 0 && spec.program_counter_count != 1 {
            return Err(DescriptorError::ZeroArgumentDescriptorHasMultipleEntries);
        }
        if spec.slot_count == 0 {
            return Err(DescriptorError::InvalidRootBitmap);
        }
        let root_words_per_pc = spec
            .slot_count
            .checked_add(63)
            .ok_or(DescriptorError::InvalidRootBitmap)?
            / 64;
        let root_word_count = spec
            .program_counter_count
            .checked_mul(root_words_per_pc)
            .ok_or(DescriptorError::InvalidRootBitmap)?;
        if spec.root_bitmap_words.len() != root_word_count {
            return Err(DescriptorError::InvalidRootBitmap);
        }
        let remainder = spec.slot_count % 64;
        if remainder != 0 {
            let allowed = (1u64 << remainder) - 1;
            if (0..spec.program_counter_count)
                .any(|pc| spec.root_bitmap_words[(pc + 1) * root_words_per_pc - 1] & !allowed != 0)
            {
                return Err(DescriptorError::InvalidRootBitmap);
            }
        }

        let slot_kinds = vec![SlotKind::Oop; spec.slot_count];
        require_oop_slot(&slot_kinds, spec.entry_receiver_slot)
            .map_err(|_| DescriptorError::InvalidReceiverSlot)?;
        let entry_argument_end = spec
            .entry_argument_start
            .checked_add(spec.entry_argument_count)
            .ok_or(DescriptorError::InvalidArgumentRange)?;
        if entry_argument_end > spec.slot_count
            || spec.entry_argument_count == 0 && spec.entry_argument_start > spec.slot_count
            || spec
                .implementation_slot
                .is_some_and(|slot| slot >= spec.slot_count)
        {
            return Err(DescriptorError::InvalidArgumentRange);
        }
        let mut entry_slots = vec![spec.entry_receiver_slot];
        entry_slots.extend(spec.entry_argument_start..entry_argument_end);
        if let Some(slot) = spec.implementation_slot {
            entry_slots.push(slot);
        }
        entry_slots.extend(spec.entry_capture_slots.iter().copied());
        entry_slots.sort_unstable();
        if entry_slots
            .last()
            .is_some_and(|slot| *slot >= spec.slot_count)
            || entry_slots.windows(2).any(|slots| slots[0] == slots[1])
        {
            return Err(DescriptorError::InvalidArgumentRange);
        }
        for call_site in &spec.call_sites {
            validate_call_site(call_site, &slot_kinds, spec.program_counter_count)?;
        }

        let mut edges = Vec::with_capacity(spec.edges.len());
        let mut previous = None;
        let mut max_edge_copy_count = 0;
        for edge in spec.edges {
            if edge.source_pc >= spec.program_counter_count
                || edge.target_pc >= spec.program_counter_count
                || previous.is_some_and(|order| order >= (edge.source_pc, edge.target_pc))
                || edge.clear_bitmap_words.len() != root_words_per_pc
            {
                return Err(DescriptorError::InvalidEdge);
            }
            match edge.kind {
                EdgeTransitionKind::Continue => {
                    if edge.call_site_index.is_some() || edge.result_slot.is_some() {
                        return Err(DescriptorError::InvalidEdge);
                    }
                }
                EdgeTransitionKind::Call => {
                    let call_site = edge
                        .call_site_index
                        .and_then(|index| spec.call_sites.get(index))
                        .ok_or(DescriptorError::InvalidEdge)?;
                    if call_site.kind != CallSiteKind::Call
                        || call_site.continuation_pc != Some(edge.target_pc)
                        || call_site.result_slot != edge.result_slot
                    {
                        return Err(DescriptorError::InvalidEdge);
                    }
                }
            }
            if remainder != 0
                && edge.clear_bitmap_words[root_words_per_pc - 1] & !((1u64 << remainder) - 1) != 0
            {
                return Err(DescriptorError::InvalidClearBitmap);
            }
            let mut previous_copy = None;
            let mut destinations = Vec::with_capacity(edge.copies.len());
            for copy in &edge.copies {
                if copy.source >= spec.slot_count
                    || copy.destination >= spec.slot_count
                    || copy.source == copy.destination
                    || previous_copy.is_some_and(|order| order >= (copy.destination, copy.source))
                    || destinations.contains(&copy.destination)
                {
                    return Err(DescriptorError::InvalidCopyPlan);
                }
                if bitmap_contains(&edge.clear_bitmap_words, copy.destination) {
                    return Err(DescriptorError::InvalidClearBitmap);
                }
                previous_copy = Some((copy.destination, copy.source));
                destinations.push(copy.destination);
            }
            if edge.result_slot.is_some_and(|slot| {
                slot >= spec.slot_count || bitmap_contains(&edge.clear_bitmap_words, slot)
            }) {
                return Err(DescriptorError::InvalidClearBitmap);
            }
            for slot in 0..spec.slot_count {
                let target_rooted = descriptor_bitmap_contains(
                    &spec.root_bitmap_words,
                    root_words_per_pc,
                    edge.target_pc,
                    slot,
                );
                let predecessor_rooted = descriptor_bitmap_contains(
                    &spec.root_bitmap_words,
                    root_words_per_pc,
                    edge.source_pc,
                    slot,
                );
                if target_rooted
                    && !predecessor_rooted
                    && !destinations.contains(&slot)
                    && edge.result_slot != Some(slot)
                    && !bitmap_contains(&edge.clear_bitmap_words, slot)
                {
                    return Err(DescriptorError::InvalidEdge);
                }
            }
            max_edge_copy_count = max_edge_copy_count.max(edge.copies.len());
            previous = Some((edge.source_pc, edge.target_pc));
            edges.push(FrameEdge {
                kind: edge.kind,
                source_pc: edge.source_pc,
                target_pc: edge.target_pc,
                call_site_index: edge.call_site_index,
                result_slot: edge.result_slot,
                copies: edge.copies.into_boxed_slice(),
                clear_bitmap_words: edge.clear_bitmap_words.into_boxed_slice(),
            });
        }

        Ok(Arc::new(Self {
            identity: spec.identity,
            layout_version: FRAME_LAYOUT_VERSION_V2,
            program_counter_count: spec.program_counter_count,
            driver_argument_count: spec.driver_argument_count,
            slot_kinds: slot_kinds.into_boxed_slice(),
            call_sites: spec.call_sites.into_boxed_slice(),
            root_bitmap_words: spec.root_bitmap_words.into_boxed_slice(),
            root_words_per_pc,
            edges: edges.into_boxed_slice(),
            max_edge_copy_count,
            v2_entry_binding: Some(V2EntryBinding {
                receiver_slot: spec.entry_receiver_slot,
                argument_start: spec.entry_argument_start,
                argument_count: spec.entry_argument_count,
                implementation_slot: spec.implementation_slot,
            }),
            entry_capture_slots: spec.entry_capture_slots.into_boxed_slice(),
            tail_entry_plan: std::sync::OnceLock::new(),
            step_method: spec.step_method,
            executable_owner: core::cell::Cell::new(core::ptr::null_mut()),
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

    fn traces_slot(&self, root_state_pc: usize, index: usize) -> bool {
        match self.layout_version {
            FRAME_LAYOUT_VERSION_V2 => descriptor_bitmap_contains(
                &self.root_bitmap_words,
                self.root_words_per_pc,
                root_state_pc,
                index,
            ),
            _ => false,
        }
    }

    fn edge(&self, source_pc: usize, target_pc: usize) -> Option<(usize, &FrameEdge)> {
        self.edges
            .binary_search_by_key(&(source_pc, target_pc), |edge| {
                (edge.source_pc, edge.target_pc)
            })
            .ok()
            .map(|index| (index, &self.edges[index]))
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

    pub(crate) fn v2_entry_binding(&self) -> Option<V2EntryBinding> {
        self.v2_entry_binding
    }

    pub(crate) fn entry_capture_slots(&self) -> &[usize] {
        &self.entry_capture_slots
    }

    fn accepts_program_counter(&self, pc: usize) -> bool {
        pc < self.program_counter_count
    }
}

fn require_oop_slot(slot_kinds: &[SlotKind], index: usize) -> Result<(), ()> {
    match slot_kinds.get(index) {
        Some(SlotKind::Oop) => Ok(()),
        None => Err(()),
    }
}

fn validate_call_site(
    call_site: &CallSiteDescriptor,
    slot_kinds: &[SlotKind],
    program_counter_count: usize,
) -> Result<(), DescriptorError> {
    require_oop_slot(slot_kinds, call_site.receiver_slot)
        .map_err(|_| DescriptorError::InvalidReceiverSlot)?;
    require_oop_slot(slot_kinds, call_site.selector_slot)
        .map_err(|_| DescriptorError::InvalidSelectorSlot)?;
    let argument_end = call_site
        .argument_start
        .checked_add(call_site.argument_count)
        .ok_or(DescriptorError::InvalidArgumentRange)?;
    if argument_end > slot_kinds.len() {
        return Err(DescriptorError::InvalidArgumentRange);
    }
    if slot_kinds[call_site.argument_start..argument_end]
        .iter()
        .any(|kind| *kind != SlotKind::Oop)
    {
        return Err(DescriptorError::InvalidArgumentSlotKind);
    }
    match call_site.kind {
        CallSiteKind::Call => {
            let result = call_site
                .result_slot
                .ok_or(DescriptorError::InvalidResultSlot)?;
            require_oop_slot(slot_kinds, result).map_err(|_| DescriptorError::InvalidResultSlot)?;
            if (call_site.argument_start..argument_end).contains(&result) {
                return Err(DescriptorError::ResultOverlapsArguments);
            }
            let continuation = call_site
                .continuation_pc
                .ok_or(DescriptorError::MissingContinuation)?;
            if continuation >= program_counter_count {
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
    Ok(())
}

fn bitmap_contains(words: &[u64], slot: usize) -> bool {
    words
        .get(slot / 64)
        .is_some_and(|word| word & (1u64 << (slot % 64)) != 0)
}

fn descriptor_bitmap_contains(words: &[u64], words_per_pc: usize, pc: usize, slot: usize) -> bool {
    pc.checked_mul(words_per_pc)
        .and_then(|base| base.checked_add(slot / 64))
        .and_then(|index| words.get(index))
        .is_some_and(|word| word & (1u64 << (slot % 64)) != 0)
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
    self_tail_entry: bool,
    live: bool,
    descriptor: Option<Arc<FrameDescriptor>>,
    program_counter: usize,
    root_state_program_counter: usize,
    pending_edge: Option<usize>,
    control: boundary::FrameControl,
    return_destination: Option<ReturnDestination>,
    resumable_entry: Option<ResumableEntryCapability>,
    words: Vec<usize>,
    transfer_scratch: Vec<usize>,
}

struct DetachedFrame {
    source_handle: FrameHandle,
    descriptor: Arc<FrameDescriptor>,
    program_counter: usize,
    root_state_program_counter: usize,
    pending_edge: Option<usize>,
    control: boundary::FrameControl,
    return_destination: Option<ReturnDestination>,
    resumable_entry: Option<ResumableEntryCapability>,
    words: Vec<usize>,
    transfer_scratch: Vec<usize>,
}

impl DetachedFrame {
    fn trace_roots(&self, mut mark: impl FnMut(Oop)) {
        if !self.descriptor.executable_owner.get().is_null() {
            mark(self.descriptor.executable_owner.get());
        }
        for (index, word) in self.words.iter().enumerate() {
            if self
                .descriptor
                .traces_slot(self.root_state_program_counter, index)
            {
                mark(core::ptr::with_exposed_provenance_mut(*word));
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ReturnDestination {
    pub(crate) result_slot: usize,
}

impl ArenaSlot {
    fn new() -> Self {
        Self {
            generation: 1,
            self_tail_entry: false,
            live: false,
            descriptor: None,
            program_counter: 0,
            root_state_program_counter: 0,
            pending_edge: None,
            control: boundary::FrameControl::default(),
            return_destination: None,
            resumable_entry: None,
            words: Vec::new(),
            transfer_scratch: Vec::new(),
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
        slot.root_state_program_counter = program_counter;
        slot.pending_edge = None;
        slot.control = boundary::FrameControl::default();
        let scratch_count = slot
            .descriptor
            .as_deref()
            .expect("a live arena slot always has a descriptor")
            .max_edge_copy_count;
        slot.transfer_scratch.resize(scratch_count, 0);
        slot.return_destination = return_destination;
        slot.resumable_entry = resumable_entry;
        slot.self_tail_entry = false;
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
        slot.root_state_program_counter = 0;
        slot.pending_edge = None;
        slot.control = boundary::FrameControl::default();
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
        let control = self.require_live(current)?.control;
        if control.boundary_result.is_some() {
            return Err(FrameError::InvalidProgramCounter);
        }
        let self_tail = Arc::ptr_eq(
            self.require_live(current)?.descriptor.as_ref().unwrap(),
            &descriptor,
        );
        self.pop()?;
        let next = self.push_validated(
            descriptor,
            program_counter,
            nil,
            initial_oops,
            return_destination,
            resumable_entry,
        )?;
        self.slots[next.index].self_tail_entry = self_tail;
        self.slots[next.index].control = control;
        Ok(next)
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

    fn live_depth(&self) -> usize {
        self.stack.len()
    }

    fn detach_from(&mut self, boundary_depth: usize) -> Result<Vec<DetachedFrame>, FrameError> {
        if self.destroyed || boundary_depth > self.stack.len() {
            return Err(FrameError::StaleHandle);
        }
        let mut detached = Vec::with_capacity(self.stack.len() - boundary_depth);
        while self.stack.len() > boundary_depth {
            let index = *self.stack.last().ok_or(FrameError::StaleHandle)?;
            let slot = &mut self.slots[index];
            let next_generation = slot
                .generation
                .checked_add(1)
                .ok_or(FrameError::GenerationExhausted)?;
            detached.push(DetachedFrame {
                source_handle: FrameHandle {
                    arena: self.identity,
                    index,
                    generation: slot.generation,
                },
                descriptor: slot
                    .descriptor
                    .take()
                    .expect("a live arena slot always has a descriptor"),
                program_counter: slot.program_counter,
                root_state_program_counter: slot.root_state_program_counter,
                pending_edge: slot.pending_edge.take(),
                control: core::mem::take(&mut slot.control),
                return_destination: slot.return_destination.take(),
                resumable_entry: slot.resumable_entry.take(),
                words: core::mem::take(&mut slot.words),
                transfer_scratch: core::mem::take(&mut slot.transfer_scratch),
            });
            self.stack.pop();
            slot.live = false;
            slot.program_counter = 0;
            slot.root_state_program_counter = 0;
            slot.generation = next_generation;
        }
        detached.reverse();
        Ok(detached)
    }

    fn install_segment(
        &mut self,
        boundary_depth: usize,
        segment: Vec<DetachedFrame>,
    ) -> Result<(), FrameError> {
        if self.destroyed || self.stack.len() != boundary_depth || segment.is_empty() {
            return Err(FrameError::StaleHandle);
        }
        for frame in &segment {
            if frame.source_handle.arena != self.identity
                || !frame
                    .descriptor
                    .accepts_program_counter(frame.program_counter)
            {
                return Err(FrameError::StaleHandle);
            }
        }
        for frame in segment {
            let index = self.stack.len();
            if index == self.slots.len() {
                self.slots.push(ArenaSlot::new());
            }
            let slot = &mut self.slots[index];
            debug_assert!(!slot.live);
            slot.descriptor = Some(frame.descriptor);
            slot.program_counter = frame.program_counter;
            slot.root_state_program_counter = frame.root_state_program_counter;
            slot.pending_edge = frame.pending_edge;
            slot.control = frame.control;
            slot.return_destination = frame.return_destination;
            slot.resumable_entry = frame.resumable_entry;
            slot.words = frame.words;
            slot.transfer_scratch = frame.transfer_scratch;
            slot.self_tail_entry = false;
            slot.live = true;
            self.stack.push(index);
        }
        self.high_water_live_frames = self.high_water_live_frames.max(self.stack.len());
        Ok(())
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

    fn begin_continue_transition(
        &mut self,
        handle: FrameHandle,
        target_pc: usize,
    ) -> Result<(), FrameError> {
        self.begin_v2_transition(handle, EdgeTransitionKind::Continue, target_pc, None)
    }

    /// A suspended/captured invocation is a Call edge whose answer arrives on
    /// resume. Keep predecessor roots and copies pending until that answer.
    fn begin_control_transition(
        &mut self,
        handle: FrameHandle,
        result_slot: usize,
        target_pc: usize,
    ) -> Result<(), FrameError> {
        let slot = self.require_live(handle)?;
        if slot.pending_edge.is_some() {
            self.validate_pending_call(handle, result_slot)?;
            return if slot.program_counter == target_pc {
                Ok(())
            } else {
                Err(FrameError::InvalidProgramCounter)
            };
        }
        let descriptor = slot.descriptor.as_deref().ok_or(FrameError::StaleHandle)?;
        let (_, edge) = descriptor
            .edge(slot.program_counter, target_pc)
            .ok_or(FrameError::InvalidProgramCounter)?;
        if edge.kind != EdgeTransitionKind::Call || edge.result_slot != Some(result_slot) {
            return Err(FrameError::InvalidSlot);
        }
        self.begin_v2_transition(
            handle,
            EdgeTransitionKind::Call,
            target_pc,
            edge.call_site_index,
        )
    }

    fn validate_pending_call(
        &self,
        handle: FrameHandle,
        result_slot: usize,
    ) -> Result<(), FrameError> {
        let slot = self.require_live(handle)?;
        let descriptor = slot.descriptor.as_deref().ok_or(FrameError::StaleHandle)?;
        let edge = slot
            .pending_edge
            .and_then(|index| descriptor.edges.get(index))
            .ok_or(FrameError::InvalidProgramCounter)?;
        if edge.kind != EdgeTransitionKind::Call || edge.result_slot != Some(result_slot) {
            return Err(FrameError::InvalidSlot);
        }
        if edge.source_pc != slot.root_state_program_counter
            || edge.target_pc != slot.program_counter
            || slot.transfer_scratch.len() < edge.copies.len()
        {
            return Err(FrameError::InvalidProgramCounter);
        }
        Ok(())
    }

    fn begin_call_transition(
        &mut self,
        handle: FrameHandle,
        target_pc: usize,
        call_site_index: usize,
    ) -> Result<(), FrameError> {
        self.begin_v2_transition(
            handle,
            EdgeTransitionKind::Call,
            target_pc,
            Some(call_site_index),
        )
    }

    fn commit_continue_transition(
        &mut self,
        handle: FrameHandle,
        nil: Oop,
    ) -> Result<(), FrameError> {
        self.commit_v2_transition(handle, nil, None)
    }

    pub(crate) fn commit_call_transition(
        &mut self,
        handle: FrameHandle,
        result_slot: usize,
        nil: Oop,
        result: Oop,
    ) -> Result<(), FrameError> {
        self.validate_pending_call(handle, result_slot)?;
        let deliveries = self.completed_boundary_deliveries(handle, result_slot)?;
        self.commit_v2_transition(handle, nil, Some(result))?;
        let slot = self.require_live_mut(handle)?;
        slot.control.boundary_result = None;
        slot.control.deliveries = deliveries;
        Ok(())
    }

    pub(crate) fn begin_v2_transition(
        &mut self,
        handle: FrameHandle,
        kind: EdgeTransitionKind,
        target_pc: usize,
        call_site_index: Option<usize>,
    ) -> Result<(), FrameError> {
        let slot = self.require_live(handle)?;
        let descriptor = slot
            .descriptor
            .as_deref()
            .expect("a live arena slot always has a descriptor");
        if descriptor.layout_version != FRAME_LAYOUT_VERSION_V2 || slot.pending_edge.is_some() {
            return Err(FrameError::InvalidProgramCounter);
        }
        let (edge_index, edge) = descriptor
            .edge(slot.program_counter, target_pc)
            .ok_or(FrameError::InvalidProgramCounter)?;
        if edge.kind != kind || edge.call_site_index != call_site_index {
            return Err(FrameError::InvalidProgramCounter);
        }
        let slot = self.require_live_mut(handle)?;
        slot.program_counter = target_pc;
        slot.pending_edge = Some(edge_index);
        Ok(())
    }

    pub(crate) fn commit_v2_transition(
        &mut self,
        handle: FrameHandle,
        nil: Oop,
        call_result: Option<Oop>,
    ) -> Result<(), FrameError> {
        let slot = self.require_live_mut(handle)?;
        let descriptor = slot
            .descriptor
            .as_deref()
            .expect("a live arena slot always has a descriptor");
        let edge_index = slot.pending_edge.ok_or(FrameError::InvalidProgramCounter)?;
        let edge = descriptor
            .edges
            .get(edge_index)
            .ok_or(FrameError::InvalidProgramCounter)?;
        match (edge.kind, edge.result_slot, call_result) {
            (EdgeTransitionKind::Continue, None, None) => {}
            (EdgeTransitionKind::Call, Some(_), Some(_)) => {}
            _ => return Err(FrameError::InvalidSlot),
        }
        if edge.source_pc != slot.root_state_program_counter
            || edge.target_pc != slot.program_counter
            || slot.transfer_scratch.len() < edge.copies.len()
        {
            return Err(FrameError::InvalidProgramCounter);
        }
        for (scratch, copy) in slot.transfer_scratch.iter_mut().zip(edge.copies.iter()) {
            *scratch = slot.words[copy.source];
        }
        for (scratch, copy) in slot.transfer_scratch.iter().zip(edge.copies.iter()) {
            slot.words[copy.destination] = *scratch;
        }
        if let (Some(result_slot), Some(result)) = (edge.result_slot, call_result) {
            slot.words[result_slot] = result.addr();
        }
        for (word_index, clear_word) in edge.clear_bitmap_words.iter().copied().enumerate() {
            let base = word_index * 64;
            let mut bits = clear_word;
            while bits != 0 {
                let bit = bits.trailing_zeros() as usize;
                slot.words[base + bit] = nil.addr();
                bits &= bits - 1;
            }
        }
        slot.root_state_program_counter = edge.target_pc;
        slot.pending_edge = None;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn root_state_program_counter(
        &self,
        handle: FrameHandle,
    ) -> Result<usize, FrameError> {
        Ok(self.require_live(handle)?.root_state_program_counter)
    }

    /// Private no-callback kernel boundary. Validate the complete Frame shape
    /// before handing the existing storage to an owned compiler-produced kernel.
    pub(crate) fn with_native_words(
        &mut self,
        handle: FrameHandle,
        descriptor_key: usize,
        pc: usize,
        slot_count: usize,
        body: impl FnOnce(&mut [usize]),
    ) -> Result<(), FrameError> {
        let slot = self.require_live_mut(handle)?;
        let descriptor = slot.descriptor.as_deref().ok_or(FrameError::StaleHandle)?;
        if descriptor as *const FrameDescriptor as usize != descriptor_key
            || slot.program_counter != pc
            || slot.root_state_program_counter != pc
            || slot.pending_edge.is_some()
            || slot.words.len() != slot_count
            || (0..slot_count).any(|i| descriptor.slot_kind(i) != Some(SlotKind::Oop))
        {
            return Err(FrameError::InvalidSlot);
        }
        body(&mut slot.words);
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
            if !descriptor.executable_owner.get().is_null() {
                mark(descriptor.executable_owner.get());
            }
            for (index, word) in slot.words.iter().enumerate() {
                if descriptor.traces_slot(slot.root_state_program_counter, index) {
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
            total
                .saturating_add(slot.words.capacity().saturating_mul(size_of::<usize>()))
                .saturating_add(
                    slot.transfer_scratch
                        .capacity()
                        .saturating_mul(size_of::<usize>()),
                )
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
            slot.transfer_scratch.clear();
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
    Runnable,
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
pub(crate) struct ContinuationCapability {
    index: usize,
    generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DelimiterHandle {
    pub(crate) index: usize,
    pub(crate) generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DelimiterKind {
    Host,
    EntryBarrier,
}

struct DelimiterRecord {
    generation: u64,
    live: bool,
    kind: DelimiterKind,
    frame_boundary: usize,
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

impl GenerationLeaseCapability {
    pub(crate) fn into_raw_parts(self) -> (usize, u64) {
        (self.index, self.generation)
    }

    pub(crate) fn from_raw_parts(index: usize, generation: u64) -> Self {
        Self { index, generation }
    }
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
    pub(crate) implementation_anchor: Option<Oop>,
    pub(crate) receiver_capture_offset_words: usize,
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
            implementation_anchor: None,
            receiver_capture_offset_words: 0,
        }
    }

    pub(crate) fn with_generation_lease(
        mut self,
        generation_lease: RegistrationGenerationLease,
    ) -> Self {
        self.generation_lease = generation_lease;
        self
    }

    pub(crate) fn with_implementation_anchor(mut self, implementation: Oop) -> Self {
        self.implementation_anchor = Some(implementation);
        self
    }

    pub(crate) fn with_receiver_capture_offset_words(mut self, offset: usize) -> Self {
        self.receiver_capture_offset_words = offset;
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
    MachineNotSuspended,
    ContinuationConsumed,
    ContinuationBarrier,
    ContinuationWrongMachine,
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
    MachineNotSuspended,
    ContinuationConsumed,
    ContinuationBarrier,
    ContinuationWrongMachine,
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
    MachineNotSuspended,
    ContinuationConsumed,
    ContinuationBarrier,
    ContinuationWrongMachine,
    ContinuationNotImageable,
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
            Self::MachineNotSuspended => "machineNotSuspended",
            Self::ContinuationConsumed => "continuationConsumed",
            Self::ContinuationBarrier => "continuationBarrier",
            Self::ContinuationWrongMachine => "continuationWrongMachine",
            Self::ContinuationNotImageable => "continuationNotImageable",
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
            Self::Machine(MachineError::MachineNotSuspended) | Self::MachineNotSuspended => {
                DriverFailureKind::MachineNotSuspended
            }
            Self::Machine(MachineError::ContinuationConsumed) | Self::ContinuationConsumed => {
                DriverFailureKind::ContinuationConsumed
            }
            Self::Machine(MachineError::ContinuationBarrier) | Self::ContinuationBarrier => {
                DriverFailureKind::ContinuationBarrier
            }
            Self::Machine(MachineError::ContinuationWrongMachine)
            | Self::ContinuationWrongMachine => DriverFailureKind::ContinuationWrongMachine,
            Self::Frame(FrameError::InvalidSlot) => DriverFailureKind::InvalidFrameSlot,
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
    BoundaryCall {
        receiver_slot: usize,
        selector_slot: usize,
        argument_start: usize,
        argument_count: usize,
        result_slot: usize,
    },
    ExitBoundary {
        value_slot: usize,
    },
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
    Suspend {
        result_slot: usize,
        delimiter: DelimiterHandle,
    },
    Capture {
        delimiter: DelimiterHandle,
        result_slot: usize,
    },
    Restore {
        continuation_slot: usize,
        supplied_value_slot: usize,
    },
    Abort {
        delimiter: Option<DelimiterHandle>,
        reason_slot: usize,
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
    Synchronous(birnam_abi::id_protocol::NativeEntry),
    NativeFailure(birnam_abi::id_protocol::Outcome),
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
    suspend_action: Oop,
    capture_action: Oop,
    restore_action: Oop,
    abort_action: Oop,
    pub(crate) boundary_call_action: Oop,
    pub(crate) exit_boundary_action: Oop,
    delimiters: Vec<DelimiterRecord>,
    delimiter_stack: Vec<usize>,
    suspended_result_slot: Option<usize>,
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
        suspend_action: Oop,
        capture_action: Oop,
        restore_action: Oop,
        abort_action: Oop,
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
            suspend_action,
            capture_action,
            restore_action,
            abort_action,
            boundary_call_action: core::ptr::null_mut(),
            exit_boundary_action: core::ptr::null_mut(),
            delimiters: Vec::new(),
            delimiter_stack: Vec::new(),
            suspended_result_slot: None,
        }
    }

    pub(crate) fn lifecycle(&self) -> MachineLifecycle {
        self.lifecycle
    }

    /// Query from a StepMethod, outside any native callback activation. This
    /// checks execution eligibility only; cancellation policy stays in Birnam.
    pub(crate) fn control_eligible(&self) -> bool {
        self.lifecycle == MachineLifecycle::Running
            && self.drive_depth == 1
            && self.drive_boundaries.last().is_some_and(|b| b.current == 0)
            && self
                .delimiter_stack
                .iter()
                .all(|index| self.delimiters[*index].kind != DelimiterKind::EntryBarrier)
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

    #[cfg(test)]
    pub(crate) fn s3_scratch_actions(&self) -> [Oop; 4] {
        [
            self.suspend_action,
            self.capture_action,
            self.restore_action,
            self.abort_action,
        ]
    }

    pub(crate) fn begin_initial_drive(&mut self, owner: u64) -> Result<u64, MachineError> {
        self.require_owner(owner)?;
        if self.lifecycle != MachineLifecycle::New || self.drive_depth != 0 {
            return Err(MachineError::InvalidLifecycle);
        }
        self.advance_drive_epoch()?;
        self.drive_depth = 1;
        self.drive_boundaries.push(BoundaryDiagnostics::default());
        self.push_delimiter(DelimiterKind::Host)?;
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
        self.push_delimiter(DelimiterKind::EntryBarrier)?;
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
        self.pop_delimiter()?;
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
        self.pop_delimiter()?;
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

    fn push_delimiter(&mut self, kind: DelimiterKind) -> Result<DelimiterHandle, MachineError> {
        let index = self.delimiters.len();
        self.delimiters.push(DelimiterRecord {
            generation: 1,
            live: true,
            kind,
            frame_boundary: self.arena.live_depth(),
        });
        self.delimiter_stack.push(index);
        Ok(DelimiterHandle {
            index,
            generation: 1,
        })
    }

    fn pop_delimiter(&mut self) -> Result<(), MachineError> {
        let index = self
            .delimiter_stack
            .pop()
            .ok_or(MachineError::InvalidLifecycle)?;
        let delimiter = self
            .delimiters
            .get_mut(index)
            .ok_or(MachineError::InvalidLifecycle)?;
        delimiter.live = false;
        delimiter.generation = delimiter
            .generation
            .checked_add(1)
            .ok_or(MachineError::RegistryGenerationExhausted)?;
        Ok(())
    }

    pub(crate) fn current_delimiter(&self) -> Result<DelimiterHandle, MachineError> {
        let index = *self
            .delimiter_stack
            .last()
            .ok_or(MachineError::InvalidLifecycle)?;
        let record = self
            .delimiters
            .get(index)
            .filter(|record| record.live)
            .ok_or(MachineError::InvalidLifecycle)?;
        Ok(DelimiterHandle {
            index,
            generation: record.generation,
        })
    }

    fn validate_delimiter(
        &self,
        delimiter: DelimiterHandle,
    ) -> Result<&DelimiterRecord, MachineError> {
        let record = self
            .delimiters
            .get(delimiter.index)
            .filter(|record| record.live && record.generation == delimiter.generation)
            .ok_or(MachineError::ContinuationBarrier)?;
        let position = self
            .delimiter_stack
            .iter()
            .position(|index| *index == delimiter.index)
            .ok_or(MachineError::ContinuationBarrier)?;
        if self.delimiter_stack[position..]
            .iter()
            .any(|index| self.delimiters[*index].kind == DelimiterKind::EntryBarrier)
        {
            return Err(MachineError::ContinuationBarrier);
        }
        Ok(record)
    }

    fn suspend(
        &mut self,
        owner: u64,
        delimiter: DelimiterHandle,
        result_slot: usize,
    ) -> Result<(), MachineError> {
        self.require_driving(owner)?;
        let _ = self.validate_delimiter(delimiter)?;
        if self.drive_depth != 1 {
            return Err(MachineError::ContinuationBarrier);
        }
        self.finish_drive_epoch()?;
        self.drive_depth = 0;
        self.lifecycle = MachineLifecycle::Suspended;
        self.suspended_result_slot = Some(result_slot);
        Ok(())
    }

    fn capture(
        &mut self,
        owner: u64,
        delimiter: DelimiterHandle,
        result_slot: usize,
    ) -> Result<(usize, Vec<DetachedFrame>), ExecutionError> {
        self.require_driving(owner)?;
        let boundary_depth = self.validate_delimiter(delimiter)?.frame_boundary;
        if self.drive_depth != 1 {
            return Err(ExecutionError::ContinuationBarrier);
        }
        let frame = self.arena.current_handle().ok_or(FrameError::StaleHandle)?;
        self.arena.validate_pending_call(frame, result_slot)?;
        self.drive_epoch
            .checked_add(1)
            .ok_or(MachineError::DriveEpochExhausted)?;
        let segment = self.arena.detach_from(boundary_depth)?;
        if segment.is_empty() {
            return Err(ExecutionError::ContinuationBarrier);
        }
        self.finish_drive_epoch()?;
        self.drive_depth = 0;
        self.lifecycle = MachineLifecycle::Suspended;
        self.suspended_result_slot = None;
        Ok((boundary_depth, segment))
    }

    fn begin_continuation_restore(&mut self, owner: u64) -> Result<(), MachineError> {
        self.require_owner(owner)?;
        if self.lifecycle != MachineLifecycle::Suspended || self.drive_depth != 0 {
            return Err(MachineError::MachineNotSuspended);
        }
        self.advance_drive_epoch()?;
        self.drive_depth = 1;
        self.drive_boundaries.push(BoundaryDiagnostics::default());
        self.lifecycle = MachineLifecycle::Running;
        Ok(())
    }

    fn resume_suspended(&mut self, owner: u64, value: Oop, nil: Oop) -> Result<(), ExecutionError> {
        self.require_owner(owner)?;
        if self.lifecycle != MachineLifecycle::Suspended || self.drive_depth != 0 {
            return Err(ExecutionError::MachineNotSuspended);
        }
        let result_slot = self
            .suspended_result_slot
            .ok_or(ExecutionError::MachineNotSuspended)?;
        let frame = self.arena.current_handle().ok_or(FrameError::StaleHandle)?;
        self.arena.validate_pending_call(frame, result_slot)?;
        self.drive_epoch
            .checked_add(1)
            .ok_or(MachineError::DriveEpochExhausted)?;
        self.arena
            .commit_call_transition(frame, result_slot, nil, value)?;
        self.suspended_result_slot = None;
        self.advance_drive_epoch()?;
        self.drive_depth = 1;
        self.drive_boundaries.push(BoundaryDiagnostics::default());
        self.lifecycle = MachineLifecycle::Running;
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
        self.delimiter_stack.clear();
        self.suspended_result_slot = None;
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
        self.delimiter_stack.clear();
        self.suspended_result_slot = None;
        self.lifecycle = MachineLifecycle::Destroyed;
        entry_pins
    }

    fn trace_roots(&self, mut mark: impl FnMut(Oop)) {
        if !self.continue_action.is_null() {
            mark(self.continue_action);
        }
        if !self.return_action.is_null() {
            mark(self.return_action);
        }
        if !self.call_action.is_null() {
            mark(self.call_action);
        }
        if !self.tail_call_action.is_null() {
            mark(self.tail_call_action);
        }
        for action in [
            self.boundary_call_action,
            self.exit_boundary_action,
            self.suspend_action,
            self.capture_action,
            self.restore_action,
            self.abort_action,
        ] {
            if !action.is_null() {
                mark(action);
            }
        }
        self.arena.trace_roots(mark);
    }
}

struct MachineSlot {
    generation: u64,
    state: Option<MachineState>,
}

struct ContinuationState {
    owner: u64,
    machine: MachineCapability,
    delimiter: DelimiterHandle,
    boundary_depth: usize,
    result_slot: usize,
    segment: Option<Vec<DetachedFrame>>,
    continuation: Oop,
}

impl ContinuationState {
    fn trace_roots(&self, machine: Oop, mut mark: impl FnMut(Oop)) {
        if self.segment.is_some() && !machine.is_null() {
            mark(machine);
        }
        if let Some(segment) = &self.segment {
            for frame in segment {
                frame.trace_roots(&mut mark);
            }
        }
    }
}

struct ContinuationSlot {
    generation: u64,
    state: Option<ContinuationState>,
}

struct ResumableEntry {
    thunk: MethodAddress,
    descriptor: Arc<FrameDescriptor>,
    declared_arity: usize,
    receiver_slot: usize,
    argument_start: usize,
    implementation_identity: u64,
    implementation_anchor: Option<Oop>,
    receiver_capture_offset_words: usize,
    code_generation_identity: u64,
    lifecycle: ResumableEntryLifecycle,
    binding_references: usize,
    active_frame_pins: usize,
    continuation_frame_pins: usize,
    acquired_entries: usize,
    explicit_generation_leases: usize,
    registration_generation_lease: Option<RegistrationGenerationLease>,
    unpublish_after_collection: bool,
}

impl ResumableEntry {
    fn finish_retirement(&mut self) {
        if self.unpublish_after_collection {
            // A collected Block vtable can be reconstructed by another live
            // factory in this admitted generation. It has no current dispatch
            // authority until an actual new binding publishes it again.
            debug_assert!(self.implementation_anchor.is_none());
            self.lifecycle = ResumableEntryLifecycle::Unpublished;
        } else {
            self.lifecycle = ResumableEntryLifecycle::Retired;
            self.registration_generation_lease = None;
            self.implementation_anchor = None;
        }
    }
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
    pub(crate) continuation_frame_pins: usize,
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
    continuations: Vec<ContinuationSlot>,
    free_continuation_slots: Vec<usize>,
    resumable_entries: Vec<ResumableEntrySlot>,
    free_resumable_entry_slots: Vec<usize>,
    resumable_by_thunk: HashMap<(usize, usize), ResumableEntryCapability>,
    generation_leases: Vec<GenerationLeaseSlot>,
    free_generation_lease_slots: Vec<usize>,
    managed_turn_depth: usize,
    objects: Option<ExecutionObjects>,
    scoped_host_roots: Vec<Oop>,
    _owner_thread_only: PhantomData<Rc<()>>,
}

impl ExecutionState {
    /// A language Method owns the executable independent of dictionary count.
    /// Bindings retain that Method through GC; frames and continuations trace it.
    pub(crate) fn publish_owned(
        &mut self,
        capability: ResumableEntryCapability,
    ) -> Result<GenerationLeaseCapability, RegistrationError> {
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
        if entry.lifecycle != ResumableEntryLifecycle::Unpublished {
            return Err(RegistrationError::EntryRetired);
        }
        entry.lifecycle = ResumableEntryLifecycle::Live;
        let thunk = entry.thunk;
        self.acquire_explicit_generation_lease(thunk)
    }
    pub(crate) fn retire_owned(
        &mut self,
        capability: ResumableEntryCapability,
        lease: GenerationLeaseCapability,
    ) -> Result<(), RegistrationError> {
        self.release_explicit_generation_lease(lease)?;
        let slot = self
            .resumable_entries
            .get_mut(capability.index)
            .ok_or(RegistrationError::StaleCapability)?;
        if slot.generation != capability.generation {
            return Err(RegistrationError::StaleCapability);
        }
        slot.entry
            .as_mut()
            .ok_or(RegistrationError::StaleCapability)?
            .lifecycle = ResumableEntryLifecycle::Retiring;
        self.finish_entry_retirement(capability);
        Ok(())
    }
    pub(crate) fn new(owner: u64) -> Self {
        Self {
            owner,
            next_arena_identity: 1,
            machines: Vec::new(),
            free_machine_slots: Vec::new(),
            continuations: Vec::new(),
            free_continuation_slots: Vec::new(),
            resumable_entries: Vec::new(),
            free_resumable_entry_slots: Vec::new(),
            resumable_by_thunk: HashMap::new(),
            generation_leases: Vec::new(),
            free_generation_lease_slots: Vec::new(),
            managed_turn_depth: 0,
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
        if carrier
            .descriptor
            .v2_entry_binding()
            .is_some_and(|binding| binding.implementation_slot.is_some())
            != carrier.implementation_anchor.is_some()
            || carrier.receiver_capture_offset_words > 1
            || carrier.implementation_anchor.is_some() && carrier.receiver_capture_offset_words != 0
        {
            return Err(RegistrationError::InvalidDescriptor);
        }
        let thunk = MethodAddress::from_method(carrier.thunk);
        if thunk == MethodAddress::from_method(carrier.descriptor.step_method()) {
            return Err(RegistrationError::StepMethodInstalledAsThunk);
        }
        let thunk_key = thunk.key();
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
            implementation_anchor: carrier.implementation_anchor,
            receiver_capture_offset_words: carrier.receiver_capture_offset_words,
            code_generation_identity: carrier.code_generation_identity,
            lifecycle: ResumableEntryLifecycle::Unpublished,
            binding_references: 0,
            active_frame_pins: 0,
            continuation_frame_pins: 0,
            acquired_entries: 0,
            explicit_generation_leases: 0,
            registration_generation_lease: Some(carrier.generation_lease),
            unpublish_after_collection: false,
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
            || entry.continuation_frame_pins != 0
            || entry.acquired_entries != 0
            || entry.explicit_generation_leases != 0
        {
            return Err(RegistrationError::EntryRetired);
        }
        let entry = slot
            .entry
            .take()
            .expect("validated unpublished entry remains installed");
        self.resumable_by_thunk.remove(&entry.thunk.key());
        slot.generation = next_generation;
        self.free_resumable_entry_slots.push(capability.index);
        Ok(())
    }

    /// Remove every quiescent entry owned by one code generation. This is the
    /// final Runtime-side release: direct bindings must already be gone and no
    /// managed turn, Frame, Continuation, acquisition, or explicit lease may remain.
    pub(crate) fn unregister_code_generation(
        &mut self,
        code_generation_identity: u64,
    ) -> Result<bool, RegistrationError> {
        if code_generation_identity == 0 || self.managed_turn_depth != 0 {
            return Ok(false);
        }
        let indices = self
            .resumable_entries
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| {
                slot.entry
                    .as_ref()
                    .is_some_and(|entry| entry.code_generation_identity == code_generation_identity)
                    .then_some(index)
            })
            .collect::<Vec<_>>();
        if indices.is_empty() {
            return Ok(false);
        }
        for index in &indices {
            let slot = &self.resumable_entries[*index];
            let entry = slot
                .entry
                .as_ref()
                .ok_or(RegistrationError::StaleCapability)?;
            if entry.binding_references != 0
                || entry.active_frame_pins != 0
                || entry.continuation_frame_pins != 0
                || entry.acquired_entries != 0
                || entry.explicit_generation_leases != 0
                || !matches!(
                    entry.lifecycle,
                    ResumableEntryLifecycle::Unpublished | ResumableEntryLifecycle::Retired
                )
                || slot.generation == u64::MAX
            {
                return Ok(false);
            }
        }
        for index in indices {
            let slot = &mut self.resumable_entries[index];
            let entry = slot
                .entry
                .take()
                .expect("validated generation entry remains installed");
            self.resumable_by_thunk.remove(&entry.thunk.key());
            slot.generation += 1;
            self.free_resumable_entry_slots.push(index);
        }
        Ok(true)
    }

    /// Non-effectful guard query. Presence includes unpublished and retiring
    /// entries: their normal classification may fail or require a lease.
    pub(crate) fn has_resumable_thunk(&self, method: MethodAddress) -> bool {
        self.resumable_by_thunk.contains_key(&method.key())
    }

    pub(crate) fn classify_resumable(
        &mut self,
        method: MethodAddress,
    ) -> Result<Option<StacklessTarget>, RegistrationError> {
        let Some(capability) = self.resumable_by_thunk.get(&method.key()).copied() else {
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
                if self.managed_turn_depth != 0 || entry.explicit_generation_leases != 0 => {}
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
        captures: &[Oop],
        nil: Oop,
    ) -> Result<MaterializedEntry, RegistrationError> {
        let entry = self.acquired_entry_record(capability)?;
        if captures.len() != entry.descriptor.entry_capture_slots().len() {
            return Err(RegistrationError::InvalidDescriptor);
        }
        let mut initial_oops = Vec::with_capacity(
            1 + entry.declared_arity
                + captures.len()
                + usize::from(entry.implementation_anchor.is_some()),
        );
        initial_oops.push((entry.receiver_slot, receiver));
        for offset in 0..entry.declared_arity {
            initial_oops.push((
                entry.argument_start + offset,
                arguments.get(offset).copied().unwrap_or(nil),
            ));
        }
        if let (Some(binding), Some(implementation)) = (
            entry.descriptor.v2_entry_binding(),
            entry.implementation_anchor,
        ) {
            initial_oops.push((
                binding
                    .implementation_slot
                    .expect("anchor agreement checked at registration"),
                implementation,
            ));
        }
        initial_oops.extend(
            entry
                .descriptor
                .entry_capture_slots()
                .iter()
                .copied()
                .zip(captures.iter().copied()),
        );
        Ok(MaterializedEntry {
            descriptor: entry.descriptor.clone(),
            initial_oops,
        })
    }

    /// Inspect immutable metadata under an existing acquisition. The caller
    /// must retain that acquisition while using the descriptor for execution.
    pub(crate) fn acquired_entry_descriptor(
        &self,
        capability: ResumableEntryCapability,
    ) -> Result<Arc<FrameDescriptor>, RegistrationError> {
        Ok(self.acquired_entry_record(capability)?.descriptor.clone())
    }

    fn acquired_entry_record(
        &self,
        capability: ResumableEntryCapability,
    ) -> Result<&ResumableEntry, RegistrationError> {
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
        Ok(entry)
    }

    pub(crate) fn acquired_entry_capture_layout(
        &self,
        capability: ResumableEntryCapability,
    ) -> Result<(usize, usize), RegistrationError> {
        let entry = self.acquired_entry_record(capability)?;
        Ok((
            entry.receiver_capture_offset_words,
            entry.descriptor.entry_capture_slots().len(),
        ))
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
        self.try_acquire_explicit_generation_lease(method)?
            .ok_or(RegistrationError::EntryRetired)
    }

    pub(crate) fn try_acquire_explicit_generation_lease(
        &mut self,
        method: MethodAddress,
    ) -> Result<Option<GenerationLeaseCapability>, RegistrationError> {
        let Some(entry_capability) = self.resumable_by_thunk.get(&method.key()).copied() else {
            return Ok(None);
        };
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
            if entry.lifecycle != ResumableEntryLifecycle::Live
                && !(entry.lifecycle == ResumableEntryLifecycle::Retiring
                    && self.managed_turn_depth != 0)
            {
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
        Ok(Some(capability))
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

    fn transfer_frame_pin_to_continuation(
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
        entry.continuation_frame_pins = entry
            .continuation_frame_pins
            .checked_add(1)
            .ok_or(RegistrationError::BindingReferenceOverflow)?;
        Ok(())
    }

    fn transfer_continuation_pin_to_frame(
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
        entry.continuation_frame_pins = entry
            .continuation_frame_pins
            .checked_sub(1)
            .ok_or(RegistrationError::StaleCapability)?;
        entry.active_frame_pins = entry
            .active_frame_pins
            .checked_add(1)
            .ok_or(RegistrationError::BindingReferenceOverflow)?;
        Ok(())
    }

    fn release_entry_continuation_pin(
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
        entry.continuation_frame_pins = entry
            .continuation_frame_pins
            .checked_sub(1)
            .ok_or(RegistrationError::StaleCapability)?;
        self.finish_entry_retirement(capability);
        Ok(())
    }

    pub(crate) fn note_direct_binding_added(
        &mut self,
        method: MethodAddress,
    ) -> Result<(), RegistrationError> {
        let Some(capability) = self.resumable_by_thunk.get(&method.key()).copied() else {
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
        if entry.lifecycle == ResumableEntryLifecycle::Retired {
            return Err(RegistrationError::EntryRetired);
        }
        entry.binding_references = entry
            .binding_references
            .checked_add(1)
            .ok_or(RegistrationError::BindingReferenceOverflow)?;
        entry.lifecycle = ResumableEntryLifecycle::Live;
        entry.unpublish_after_collection = false;
        Ok(())
    }

    pub(crate) fn note_direct_binding_removed(
        &mut self,
        method: MethodAddress,
    ) -> Result<(), RegistrationError> {
        self.remove_direct_binding(method, false)
    }

    /// The Runtime has identified a collected vtable's method as a BlockBody
    /// in an admitted manifest. This is not explicit MOP method removal.
    pub(crate) fn note_collected_block_binding_removed(
        &mut self,
        method: MethodAddress,
    ) -> Result<(), RegistrationError> {
        self.remove_direct_binding(method, true)
    }

    fn remove_direct_binding(
        &mut self,
        method: MethodAddress,
        collected_block: bool,
    ) -> Result<(), RegistrationError> {
        let Some(capability) = self.resumable_by_thunk.get(&method.key()).copied() else {
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
        if collected_block && entry.implementation_anchor.is_some() {
            return Err(RegistrationError::InvalidDescriptor);
        }
        entry.binding_references = entry
            .binding_references
            .checked_sub(1)
            .ok_or(RegistrationError::EntryUnpublished)?;
        if entry.binding_references == 0 {
            entry.unpublish_after_collection = collected_block;
            if entry.acquired_entries != 0
                || entry.active_frame_pins != 0
                || entry.continuation_frame_pins != 0
                || entry.explicit_generation_leases != 0
                || self.managed_turn_depth != 0
            {
                entry.lifecycle = ResumableEntryLifecycle::Retiring;
            } else {
                entry.finish_retirement();
            }
        }
        Ok(())
    }

    /// Remove one direct binding inside a staged unit transaction without
    /// retiring the referenced entry. The transaction will either restore the
    /// binding or explicitly settle retirement after commit.
    pub(crate) fn note_staged_direct_binding_removed(
        &mut self,
        method: MethodAddress,
    ) -> Result<(), RegistrationError> {
        let Some(capability) = self.resumable_by_thunk.get(&method.key()).copied() else {
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
            .checked_sub(1)
            .ok_or(RegistrationError::EntryUnpublished)?;
        Ok(())
    }

    /// Restore a staged unit's own zero-binding entry to its pre-publication
    /// state so rollback can unregister it, or commit can retain it as an
    /// eagerly registered but not-yet-instantiated block entry.
    pub(crate) fn settle_staged_registration(
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
        if entry.binding_references == 0 {
            if entry.active_frame_pins != 0
                || entry.continuation_frame_pins != 0
                || entry.acquired_entries != 0
                || entry.explicit_generation_leases != 0
            {
                return Err(RegistrationError::EntryRetired);
            }
            entry.lifecycle = ResumableEntryLifecycle::Unpublished;
        } else {
            entry.lifecycle = ResumableEntryLifecycle::Live;
        }
        Ok(())
    }

    /// Complete retirement deferred while a staged transaction could still
    /// restore the previous binding.
    pub(crate) fn commit_staged_binding_removal(
        &mut self,
        method: MethodAddress,
    ) -> Result<(), RegistrationError> {
        let Some(capability) = self.resumable_by_thunk.get(&method.key()).copied() else {
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
        if entry.binding_references != 0 {
            return Ok(());
        }
        entry.unpublish_after_collection = false;
        entry.lifecycle = if entry.acquired_entries != 0
            || entry.active_frame_pins != 0
            || entry.continuation_frame_pins != 0
            || entry.explicit_generation_leases != 0
            || self.managed_turn_depth != 0
        {
            ResumableEntryLifecycle::Retiring
        } else {
            entry.registration_generation_lease = None;
            entry.implementation_anchor = None;
            ResumableEntryLifecycle::Retired
        };
        Ok(())
    }

    pub(crate) fn begin_managed_turn(&mut self) -> Result<(), RegistrationError> {
        self.managed_turn_depth = self
            .managed_turn_depth
            .checked_add(1)
            .ok_or(RegistrationError::BindingReferenceOverflow)?;
        Ok(())
    }

    pub(crate) fn end_managed_turn(&mut self) -> Result<(), RegistrationError> {
        self.managed_turn_depth = self
            .managed_turn_depth
            .checked_sub(1)
            .ok_or(RegistrationError::StaleCapability)?;
        if self.managed_turn_depth == 0 {
            for slot in &mut self.resumable_entries {
                let Some(entry) = slot.entry.as_mut() else {
                    continue;
                };
                if entry.lifecycle == ResumableEntryLifecycle::Retiring
                    && entry.binding_references == 0
                    && entry.acquired_entries == 0
                    && entry.active_frame_pins == 0
                    && entry.continuation_frame_pins == 0
                    && entry.explicit_generation_leases == 0
                {
                    entry.finish_retirement();
                }
            }
        }
        Ok(())
    }

    fn finish_entry_retirement(&mut self, capability: ResumableEntryCapability) {
        if self.managed_turn_depth != 0 {
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
            && entry.continuation_frame_pins == 0
            && entry.explicit_generation_leases == 0
        {
            entry.finish_retirement();
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
                total.continuation_frame_pins = total
                    .continuation_frame_pins
                    .saturating_add(entry.continuation_frame_pins);
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

    pub(crate) fn trace_scoped_host_roots(&self, mut mark: impl FnMut(Oop)) {
        for root in &self.scoped_host_roots {
            mark(*root);
        }
    }

    pub(crate) fn can_suppress_registration_roots(&self, code_generation_identity: u64) -> bool {
        if code_generation_identity == 0 || self.managed_turn_depth != 0 {
            return false;
        }
        let mut found = false;
        for entry in self
            .resumable_entries
            .iter()
            .filter_map(|slot| slot.entry.as_ref())
            .filter(|entry| entry.code_generation_identity == code_generation_identity)
        {
            found = true;
            if entry.active_frame_pins != 0
                || entry.continuation_frame_pins != 0
                || entry.acquired_entries != 0
                || entry.explicit_generation_leases != 0
            {
                return false;
            }
        }
        found
    }

    pub(crate) fn managed_turn_depth(&self) -> usize {
        self.managed_turn_depth
    }

    pub(crate) fn has_managed_turn(&self) -> bool {
        self.managed_turn_depth != 0
    }

    pub(crate) fn has_code_generation(&self, code_generation_identity: u64) -> bool {
        self.resumable_entries.iter().any(|slot| {
            slot.entry
                .as_ref()
                .is_some_and(|entry| entry.code_generation_identity == code_generation_identity)
        })
    }

    pub(crate) fn registration_roots_excluding_generation(
        &self,
        code_generation_identity: Option<u64>,
    ) -> Vec<Oop> {
        self.resumable_entries
            .iter()
            .filter_map(|slot| slot.entry.as_ref())
            .filter(|entry| Some(entry.code_generation_identity) != code_generation_identity)
            .filter_map(|entry| entry.implementation_anchor)
            .collect()
    }

    pub(crate) fn create_machine_with_objects(
        &mut self,
        owner: u64,
        machine: Oop,
        continue_action: Oop,
        call_action: Oop,
        tail_call_action: Oop,
        return_action: Oop,
        suspend_action: Oop,
        capture_action: Oop,
        restore_action: Oop,
        abort_action: Oop,
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
            suspend_action,
            capture_action,
            restore_action,
            abort_action,
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

    pub(crate) fn capture_continuation(
        &mut self,
        heap: &mut Heap,
        owner: u64,
        machine: MachineCapability,
        delimiter: DelimiterHandle,
        result_slot: usize,
        continuation: Oop,
    ) -> Result<ContinuationCapability, ExecutionError> {
        self.require_owner(owner)?;
        let objects = self.objects.ok_or(ExecutionError::InvalidAction)?;
        if heap.vtable_of(continuation) != Some(objects.continuation_vtable)
            || heap.payload_size(continuation) != Some(3 * size_of::<usize>())
        {
            return Err(ExecutionError::InvalidAction);
        }
        let (boundary_depth, segment) =
            self.machine_mut(owner, machine)?
                .capture(owner, delimiter, result_slot)?;
        let entry_pins: Vec<_> = segment
            .iter()
            .filter_map(|frame| frame.resumable_entry)
            .collect();
        for entry in &entry_pins {
            self.transfer_frame_pin_to_continuation(*entry)?;
        }

        let index = if let Some(index) = self.free_continuation_slots.pop() {
            index
        } else {
            self.continuations.push(ContinuationSlot {
                generation: 1,
                state: None,
            });
            self.continuations.len() - 1
        };
        let slot = &mut self.continuations[index];
        debug_assert!(slot.state.is_none());
        let capability = ContinuationCapability {
            index,
            generation: slot.generation,
        };
        slot.state = Some(ContinuationState {
            owner,
            machine,
            delimiter,
            boundary_depth,
            result_slot,
            segment: Some(segment),
            continuation,
        });
        if !heap.write_payload_word(continuation, 0, CONTINUATION_CAPABILITY_KIND_V1)
            || !heap.write_payload_word(continuation, 1, capability.index)
            || !usize::try_from(capability.generation)
                .ok()
                .is_some_and(|generation| heap.write_payload_word(continuation, 2, generation))
        {
            return Err(ExecutionError::InvalidAction);
        }
        Ok(capability)
    }

    pub(crate) fn continuation_capability_for_object(
        &self,
        heap: &Heap,
        object: Oop,
    ) -> Result<ContinuationCapability, ExecutionError> {
        let objects = self.objects.ok_or(ExecutionError::InvalidAction)?;
        if heap.vtable_of(object) != Some(objects.continuation_vtable)
            || heap.payload_size(object) != Some(3 * size_of::<usize>())
            || heap.read_payload_word(object, 0) != Some(CONTINUATION_CAPABILITY_KIND_V1)
        {
            return Err(ExecutionError::ContinuationConsumed);
        }
        let capability = ContinuationCapability {
            index: heap
                .read_payload_word(object, 1)
                .ok_or(ExecutionError::ContinuationConsumed)?,
            generation: heap
                .read_payload_word(object, 2)
                .and_then(|word| u64::try_from(word).ok())
                .ok_or(ExecutionError::ContinuationConsumed)?,
        };
        let slot = self
            .continuations
            .get(capability.index)
            .ok_or(ExecutionError::ContinuationConsumed)?;
        if slot.generation != capability.generation
            || slot
                .state
                .as_ref()
                .is_none_or(|state| state.continuation != object)
        {
            return Err(ExecutionError::ContinuationConsumed);
        }
        Ok(capability)
    }

    pub(crate) fn restore_continuation(
        &mut self,
        owner: u64,
        machine: MachineCapability,
        continuation: ContinuationCapability,
        supplied_value: Oop,
        nil: Oop,
    ) -> Result<(), ExecutionError> {
        self.require_owner(owner)?;
        let (continuation_machine, delimiter, boundary_depth, result_slot, entries) = {
            let slot = self
                .continuations
                .get(continuation.index)
                .ok_or(ExecutionError::ContinuationConsumed)?;
            if slot.generation != continuation.generation {
                return Err(ExecutionError::ContinuationConsumed);
            }
            let state = slot
                .state
                .as_ref()
                .ok_or(ExecutionError::ContinuationConsumed)?;
            let segment = state
                .segment
                .as_ref()
                .ok_or(ExecutionError::ContinuationConsumed)?;
            (
                state.machine,
                state.delimiter,
                state.boundary_depth,
                state.result_slot,
                segment
                    .iter()
                    .filter_map(|frame| frame.resumable_entry)
                    .collect::<Vec<_>>(),
            )
        };
        if continuation_machine != machine {
            return Err(ExecutionError::ContinuationWrongMachine);
        }
        {
            let machine_state = self.machine(owner, machine)?;
            if machine_state.lifecycle != MachineLifecycle::Suspended
                || machine_state.drive_depth != 0
            {
                return Err(ExecutionError::MachineNotSuspended);
            }
            machine_state
                .drive_epoch
                .checked_add(1)
                .ok_or(MachineError::DriveEpochExhausted)?;
            let record = machine_state.validate_delimiter(delimiter)?;
            if record.frame_boundary != boundary_depth
                || machine_state.arena.live_depth() != boundary_depth
            {
                return Err(ExecutionError::ContinuationBarrier);
            }
        }
        let segment = self.continuations[continuation.index]
            .state
            .as_mut()
            .and_then(|state| state.segment.take())
            .ok_or(ExecutionError::ContinuationConsumed)?;
        {
            let machine_state = self.machine_mut(owner, machine)?;
            machine_state
                .arena
                .install_segment(boundary_depth, segment)?;
            let frame = machine_state
                .arena
                .current_handle()
                .ok_or(FrameError::StaleHandle)?;
            machine_state
                .arena
                .commit_call_transition(frame, result_slot, nil, supplied_value)?;
            machine_state.begin_continuation_restore(owner)?;
        }
        for entry in entries {
            self.transfer_continuation_pin_to_frame(entry)?;
        }
        Ok(())
    }

    pub(crate) fn resume_suspended_machine(
        &mut self,
        owner: u64,
        machine: MachineCapability,
        supplied_value: Oop,
        nil: Oop,
    ) -> Result<(), ExecutionError> {
        self.machine_mut(owner, machine)?
            .resume_suspended(owner, supplied_value, nil)
    }

    pub(crate) fn trace_continuation_roots(&self, heap: &Heap, object: Oop, mark: impl FnMut(Oop)) {
        let Ok(capability) = self.continuation_capability_for_object(heap, object) else {
            return;
        };
        let Some(state) = self.continuations[capability.index].state.as_ref() else {
            return;
        };
        let machine = self
            .machine(self.owner, state.machine)
            .map(|machine| machine.machine)
            .unwrap_or(core::ptr::null_mut());
        state.trace_roots(machine, mark);
    }

    pub(crate) unsafe fn finalize_detached_continuation(&mut self, object: Oop) {
        if object.is_null() {
            return;
        }
        let words = object.cast::<usize>();
        // SAFETY: finalizer contract supplies three initialized payload words.
        if unsafe { words.read() } != CONTINUATION_CAPABILITY_KIND_V1 {
            return;
        }
        // SAFETY: words 1 and 2 are in the same detached allocation.
        let index = unsafe { words.add(1).read() };
        let Ok(generation) = u64::try_from(unsafe { words.add(2).read() }) else {
            return;
        };
        let Some(slot) = self.continuations.get_mut(index) else {
            return;
        };
        if slot.generation != generation {
            return;
        }
        let Some(mut continuation) = slot.state.take() else {
            return;
        };
        if continuation.continuation != object {
            slot.state = Some(continuation);
            return;
        }
        let next_generation = match slot.generation.checked_add(1) {
            Some(generation) => generation,
            None => return,
        };
        slot.generation = next_generation;
        self.free_continuation_slots.push(index);
        if let Some(segment) = continuation.segment.take() {
            for entry in segment
                .into_iter()
                .filter_map(|frame| frame.resumable_entry)
            {
                let _ = self.release_entry_continuation_pin(entry);
            }
        }
    }

    pub(crate) fn destroy_machine_explicit(
        &mut self,
        owner: u64,
        capability: MachineCapability,
    ) -> Result<(), MachineError> {
        let lifecycle = self.machine(owner, capability)?.lifecycle;
        if !matches!(
            lifecycle,
            MachineLifecycle::Runnable
                | MachineLifecycle::Suspended
                | MachineLifecycle::Completed
                | MachineLifecycle::Failed
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

    pub(crate) fn trace_object_roots(
        &self,
        heap: &Heap,
        object: Oop,
        vtable: Oop,
        mark: impl FnMut(Oop),
    ) {
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

    pub(crate) fn commit_current_continue(
        &mut self,
        owner: u64,
        capability: MachineCapability,
        nil: Oop,
    ) -> Result<(), ExecutionError> {
        let machine = self.machine_mut(owner, capability)?;
        machine.require_driving(owner)?;
        let frame = machine
            .arena
            .current_handle()
            .ok_or(FrameError::StaleHandle)?;
        machine.arena.commit_continue_transition(frame, nil)?;
        Ok(())
    }

    pub(crate) fn commit_current_call_result(
        &mut self,
        owner: u64,
        capability: MachineCapability,
        result_slot: usize,
        nil: Oop,
        result: Oop,
    ) -> Result<(), ExecutionError> {
        let machine = self.machine_mut(owner, capability)?;
        machine.require_driving(owner)?;
        let frame = machine
            .arena
            .current_handle()
            .ok_or(FrameError::StaleHandle)?;
        machine
            .arena
            .commit_call_transition(frame, result_slot, nil, result)?;
        Ok(())
    }

    pub(crate) fn current_delimiter(
        &self,
        owner: u64,
        capability: MachineCapability,
    ) -> Result<DelimiterHandle, MachineError> {
        self.machine(owner, capability)?.current_delimiter()
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
        machine.arena.begin_continue_transition(frame, next_pc)?;
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

    pub(crate) fn publish_suspend(
        &mut self,
        heap: &mut Heap,
        owner: u64,
        capability: MachineCapability,
        result_slot: usize,
        delimiter: DelimiterHandle,
        next_pc: usize,
    ) -> Result<Oop, ExecutionError> {
        let objects = self.objects.ok_or(ExecutionError::InvalidAction)?;
        let machine = self.machine_mut(owner, capability)?;
        machine.require_driving(owner)?;
        let frame = machine
            .arena
            .current_handle()
            .ok_or(FrameError::StaleHandle)?;
        let _ = machine.arena.read_oop(frame, result_slot)?;
        let _ = machine.validate_delimiter(delimiter)?;
        let generation = usize::try_from(capability.generation)
            .map_err(|_| MachineError::RegistryGenerationExhausted)?;
        let delimiter_generation = usize::try_from(delimiter.generation)
            .map_err(|_| MachineError::RegistryGenerationExhausted)?;
        let drive_epoch =
            usize::try_from(machine.drive_epoch).map_err(|_| MachineError::DriveEpochExhausted)?;
        validate_action_storage(
            heap,
            machine.suspend_action,
            objects.suspend_action_vtable,
            7,
        )?;
        machine
            .arena
            .begin_control_transition(frame, result_slot, next_pc)?;
        initialize_action(
            heap,
            machine.suspend_action,
            objects.suspend_action_vtable,
            &[
                1,
                capability.index,
                generation,
                drive_epoch,
                result_slot,
                delimiter.index,
                delimiter_generation,
            ],
        )?;
        Ok(machine.suspend_action)
    }

    pub(crate) fn publish_capture(
        &mut self,
        heap: &mut Heap,
        owner: u64,
        capability: MachineCapability,
        delimiter: DelimiterHandle,
        result_slot: usize,
        next_pc: usize,
    ) -> Result<Oop, ExecutionError> {
        let objects = self.objects.ok_or(ExecutionError::InvalidAction)?;
        let machine = self.machine_mut(owner, capability)?;
        machine.require_driving(owner)?;
        let frame = machine
            .arena
            .current_handle()
            .ok_or(FrameError::StaleHandle)?;
        let _ = machine.arena.read_oop(frame, result_slot)?;
        let _ = machine.validate_delimiter(delimiter)?;
        let generation = usize::try_from(capability.generation)
            .map_err(|_| MachineError::RegistryGenerationExhausted)?;
        let delimiter_generation = usize::try_from(delimiter.generation)
            .map_err(|_| MachineError::RegistryGenerationExhausted)?;
        let drive_epoch =
            usize::try_from(machine.drive_epoch).map_err(|_| MachineError::DriveEpochExhausted)?;
        validate_action_storage(
            heap,
            machine.capture_action,
            objects.capture_action_vtable,
            7,
        )?;
        machine
            .arena
            .begin_control_transition(frame, result_slot, next_pc)?;
        initialize_action(
            heap,
            machine.capture_action,
            objects.capture_action_vtable,
            &[
                1,
                capability.index,
                generation,
                drive_epoch,
                delimiter.index,
                delimiter_generation,
                result_slot,
            ],
        )?;
        Ok(machine.capture_action)
    }

    pub(crate) fn publish_restore(
        &mut self,
        heap: &mut Heap,
        owner: u64,
        capability: MachineCapability,
        continuation_slot: usize,
        supplied_value_slot: usize,
    ) -> Result<Oop, ExecutionError> {
        let objects = self.objects.ok_or(ExecutionError::InvalidAction)?;
        let machine = self.machine_mut(owner, capability)?;
        machine.require_driving(owner)?;
        let frame = machine
            .arena
            .current_handle()
            .ok_or(FrameError::StaleHandle)?;
        let _ = machine.arena.read_oop(frame, continuation_slot)?;
        let _ = machine.arena.read_oop(frame, supplied_value_slot)?;
        let generation = usize::try_from(capability.generation)
            .map_err(|_| MachineError::RegistryGenerationExhausted)?;
        let drive_epoch =
            usize::try_from(machine.drive_epoch).map_err(|_| MachineError::DriveEpochExhausted)?;
        initialize_action(
            heap,
            machine.restore_action,
            objects.restore_action_vtable,
            &[
                1,
                capability.index,
                generation,
                drive_epoch,
                continuation_slot,
                supplied_value_slot,
            ],
        )?;
        Ok(machine.restore_action)
    }

    pub(crate) fn publish_abort(
        &mut self,
        heap: &mut Heap,
        owner: u64,
        capability: MachineCapability,
        delimiter: Option<DelimiterHandle>,
        reason_slot: usize,
    ) -> Result<Oop, ExecutionError> {
        let objects = self.objects.ok_or(ExecutionError::InvalidAction)?;
        let machine = self.machine_mut(owner, capability)?;
        machine.require_driving(owner)?;
        let frame = machine
            .arena
            .current_handle()
            .ok_or(FrameError::StaleHandle)?;
        let _ = machine.arena.read_oop(frame, reason_slot)?;
        if let Some(delimiter) = delimiter {
            let _ = machine.validate_delimiter(delimiter)?;
        }
        let generation = usize::try_from(capability.generation)
            .map_err(|_| MachineError::RegistryGenerationExhausted)?;
        let drive_epoch =
            usize::try_from(machine.drive_epoch).map_err(|_| MachineError::DriveEpochExhausted)?;
        let (delimiter_index, delimiter_generation) = match delimiter {
            Some(delimiter) => (
                delimiter.index,
                usize::try_from(delimiter.generation)
                    .map_err(|_| MachineError::RegistryGenerationExhausted)?,
            ),
            None => (usize::MAX, 0),
        };
        initialize_action(
            heap,
            machine.abort_action,
            objects.abort_action_vtable,
            &[
                1,
                capability.index,
                generation,
                drive_epoch,
                delimiter_index,
                delimiter_generation,
                reason_slot,
            ],
        )?;
        Ok(machine.abort_action)
    }

    pub(crate) fn apply_suspend(
        &mut self,
        owner: u64,
        capability: MachineCapability,
        result_slot: usize,
        delimiter: DelimiterHandle,
    ) -> Result<(), ExecutionError> {
        let machine = self.machine_mut(owner, capability)?;
        let frame = machine
            .arena
            .current_handle()
            .ok_or(FrameError::StaleHandle)?;
        machine.arena.validate_pending_call(frame, result_slot)?;
        machine.suspend(owner, delimiter, result_slot)?;
        Ok(())
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
                machine
                    .arena
                    .begin_call_transition(frame, continuation_pc, call_site_index)?;
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
        } else if vtable == objects.call_action_vtable
            || vtable == objects.boundary_call_action_vtable
        {
            9
        } else if vtable == objects.tail_call_action_vtable {
            8
        } else if vtable == objects.return_action_vtable
            || vtable == objects.exit_boundary_action_vtable
        {
            5
        } else if vtable == objects.suspend_action_vtable
            || vtable == objects.capture_action_vtable
            || vtable == objects.abort_action_vtable
        {
            7
        } else if vtable == objects.restore_action_vtable {
            6
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
        } else if vtable == objects.return_action_vtable
            || vtable == objects.exit_boundary_action_vtable
        {
            let value_slot = heap
                .read_payload_word(action, 4)
                .ok_or(ExecutionError::InvalidAction)?;
            let frame = machine
                .arena
                .current_handle()
                .ok_or(FrameError::StaleHandle)?;
            let _ = machine.arena.read_oop(frame, value_slot)?;
            if vtable == objects.exit_boundary_action_vtable {
                self.preflight_boundary_exit(owner, capability)?;
                Ok(DecodedAction::ExitBoundary { value_slot })
            } else {
                Ok(DecodedAction::Return { value_slot })
            }
        } else if vtable == objects.suspend_action_vtable {
            let result_slot = heap
                .read_payload_word(action, 4)
                .ok_or(ExecutionError::InvalidAction)?;
            let delimiter = DelimiterHandle {
                index: heap
                    .read_payload_word(action, 5)
                    .ok_or(ExecutionError::InvalidAction)?,
                generation: heap
                    .read_payload_word(action, 6)
                    .and_then(|word| u64::try_from(word).ok())
                    .ok_or(ExecutionError::InvalidAction)?,
            };
            let frame = machine
                .arena
                .current_handle()
                .ok_or(FrameError::StaleHandle)?;
            let _ = machine.arena.read_oop(frame, result_slot)?;
            machine.arena.validate_pending_call(frame, result_slot)?;
            let _ = machine.validate_delimiter(delimiter)?;
            Ok(DecodedAction::Suspend {
                result_slot,
                delimiter,
            })
        } else if vtable == objects.capture_action_vtable {
            let delimiter = DelimiterHandle {
                index: heap
                    .read_payload_word(action, 4)
                    .ok_or(ExecutionError::InvalidAction)?,
                generation: heap
                    .read_payload_word(action, 5)
                    .and_then(|word| u64::try_from(word).ok())
                    .ok_or(ExecutionError::InvalidAction)?,
            };
            let result_slot = heap
                .read_payload_word(action, 6)
                .ok_or(ExecutionError::InvalidAction)?;
            let frame = machine
                .arena
                .current_handle()
                .ok_or(FrameError::StaleHandle)?;
            let _ = machine.arena.read_oop(frame, result_slot)?;
            machine.arena.validate_pending_call(frame, result_slot)?;
            let _ = machine.validate_delimiter(delimiter)?;
            Ok(DecodedAction::Capture {
                delimiter,
                result_slot,
            })
        } else if vtable == objects.restore_action_vtable {
            let continuation_slot = heap
                .read_payload_word(action, 4)
                .ok_or(ExecutionError::InvalidAction)?;
            let supplied_value_slot = heap
                .read_payload_word(action, 5)
                .ok_or(ExecutionError::InvalidAction)?;
            let frame = machine
                .arena
                .current_handle()
                .ok_or(FrameError::StaleHandle)?;
            let _ = machine.arena.read_oop(frame, continuation_slot)?;
            let _ = machine.arena.read_oop(frame, supplied_value_slot)?;
            Ok(DecodedAction::Restore {
                continuation_slot,
                supplied_value_slot,
            })
        } else if vtable == objects.abort_action_vtable {
            let delimiter_index = heap
                .read_payload_word(action, 4)
                .ok_or(ExecutionError::InvalidAction)?;
            let delimiter_generation = heap
                .read_payload_word(action, 5)
                .ok_or(ExecutionError::InvalidAction)?;
            let delimiter = if delimiter_index == usize::MAX && delimiter_generation == 0 {
                None
            } else {
                Some(DelimiterHandle {
                    index: delimiter_index,
                    generation: u64::try_from(delimiter_generation)
                        .map_err(|_| ExecutionError::InvalidAction)?,
                })
            };
            if let Some(delimiter) = delimiter {
                let _ = machine.validate_delimiter(delimiter)?;
            }
            let reason_slot = heap
                .read_payload_word(action, 6)
                .ok_or(ExecutionError::InvalidAction)?;
            let frame = machine
                .arena
                .current_handle()
                .ok_or(FrameError::StaleHandle)?;
            let _ = machine.arena.read_oop(frame, reason_slot)?;
            Ok(DecodedAction::Abort {
                delimiter,
                reason_slot,
            })
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
            let result_slot = if vtable == objects.call_action_vtable
                || vtable == objects.boundary_call_action_vtable
            {
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
            if vtable == objects.boundary_call_action_vtable {
                let result_slot = result_slot.ok_or(ExecutionError::InvalidAction)?;
                machine.arena.validate_boundary_call(
                    frame,
                    receiver_slot,
                    selector_slot,
                    argument_start,
                    argument_count,
                    result_slot,
                )?;
                Ok(DecodedAction::BoundaryCall {
                    receiver_slot,
                    selector_slot,
                    argument_start,
                    argument_count,
                    result_slot,
                })
            } else if let Some(result_slot) = result_slot {
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
            vtable if vtable == objects.suspend_action_vtable => Some(objects.suspend_kind),
            vtable if vtable == objects.capture_action_vtable => Some(objects.capture_kind),
            vtable if vtable == objects.restore_action_vtable => Some(objects.restore_kind),
            vtable if vtable == objects.abort_action_vtable => Some(objects.abort_kind),
            vtable if vtable == objects.boundary_call_action_vtable => {
                Some(objects.boundary_call_kind)
            }
            vtable if vtable == objects.exit_boundary_action_vtable => {
                Some(objects.exit_boundary_kind)
            }
            _ => None,
        }
    }

    /// Test-floor snapshot of the original eight action buffers. Only Machine
    /// identity is normalized, after validating the published owner fields.
    #[cfg(test)]
    pub(crate) fn snapshot_action_words(
        &self,
        heap: &Heap,
        owner: u64,
        capability: MachineCapability,
    ) -> Vec<Vec<usize>> {
        let machine = self.machine(owner, capability).unwrap();
        let objects = self.objects.unwrap();
        [
            (machine.continue_action, objects.continue_action_vtable, 4),
            (machine.call_action, objects.call_action_vtable, 9),
            (machine.tail_call_action, objects.tail_call_action_vtable, 8),
            (machine.return_action, objects.return_action_vtable, 5),
            (machine.suspend_action, objects.suspend_action_vtable, 7),
            (machine.capture_action, objects.capture_action_vtable, 7),
            (machine.restore_action, objects.restore_action_vtable, 6),
            (machine.abort_action, objects.abort_action_vtable, 7),
        ]
        .into_iter()
        .map(|(action, vtable, count)| {
            assert_eq!(heap.vtable_of(action), Some(vtable));
            assert_eq!(heap.payload_size(action), Some(count * size_of::<usize>()));
            let mut words: Vec<_> = (0..count)
                .map(|index| self.action_word(heap, action, index).unwrap())
                .collect();
            if words[0] == 1 {
                assert_eq!(words[1], capability.index);
                assert_eq!(words[2], capability.generation as usize);
                words[1] = 0;
                words[2] = 0;
            }
            words
        })
        .collect()
    }

    pub(crate) fn action_word(&self, heap: &Heap, action: Oop, index: usize) -> Option<usize> {
        let objects = self.objects?;
        let expected_words = match heap.vtable_of(action)? {
            vtable if vtable == objects.continue_action_vtable => 4,
            vtable if vtable == objects.call_action_vtable => 9,
            vtable if vtable == objects.tail_call_action_vtable => 8,
            vtable if vtable == objects.return_action_vtable => 5,
            vtable if vtable == objects.suspend_action_vtable => 7,
            vtable if vtable == objects.capture_action_vtable => 7,
            vtable if vtable == objects.restore_action_vtable => 6,
            vtable if vtable == objects.abort_action_vtable => 7,
            vtable if vtable == objects.boundary_call_action_vtable => 9,
            vtable if vtable == objects.exit_boundary_action_vtable => 5,
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
                FrameError::InvalidSlot => FrameViewError::InvalidFrameSlot,
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

fn validate_action_storage(
    heap: &Heap,
    action: Oop,
    expected_vtable: Oop,
    word_count: usize,
) -> Result<(), ExecutionError> {
    if heap.vtable_of(action) != Some(expected_vtable)
        || heap.payload_size(action) != Some(word_count * size_of::<usize>())
    {
        return Err(ExecutionError::InvalidAction);
    }
    Ok(())
}

fn initialize_action(
    heap: &mut Heap,
    action: Oop,
    expected_vtable: Oop,
    words: &[usize],
) -> Result<(), ExecutionError> {
    validate_action_storage(heap, action, expected_vtable, words.len())?;
    for (index, word) in words.iter().copied().enumerate() {
        if !heap.write_payload_word(action, index, word) {
            return Err(ExecutionError::InvalidAction);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use birnam_artifact_codec::stackless_selection::{
        Disposition, ImplementationKind as CanonicalImplementationKind, RootKind,
    };
    use core::ffi::c_void;

    const INERT_STEP: Method = Method::new(inert_step_entry, core::ptr::null_mut());
    unsafe extern "C" fn inert_step_entry(
        _: *mut core::ffi::c_void,
        _: Oop,
        _: *const birnam_abi::id_protocol::Operand,
        _: usize,
    ) -> birnam_abi::id_protocol::Outcome {
        birnam_abi::id_protocol::Outcome::loaded(birnam_abi::id_protocol::Operand::unit())
    }

    const INERT_THUNK: Method = Method::new(inert_thunk_entry, core::ptr::null_mut());
    unsafe extern "C" fn inert_thunk_entry(
        _: *mut core::ffi::c_void,
        _: Oop,
        _: *const birnam_abi::id_protocol::Operand,
        _: usize,
    ) -> birnam_abi::id_protocol::Outcome {
        // Registration requires distinct entry and step addresses, also in release.
        std::hint::black_box(60_001usize);
        birnam_abi::id_protocol::Outcome::loaded(birnam_abi::id_protocol::Operand::unit())
    }

    fn oop(value: usize) -> Oop {
        core::ptr::with_exposed_provenance_mut(value)
    }

    fn descriptor(slot_kinds: Vec<SlotKind>, pc_count: usize) -> Arc<FrameDescriptor> {
        let slots = slot_kinds.len().max(1);
        FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 11,
                generation: 7,
                source_map: 3,
            },
            entry_argument_count: usize::from(slots > 1),
            edges: if pc_count > 1 {
                vec![probe_continue(0, 1, slots)]
            } else {
                vec![]
            },
            step_method: INERT_STEP,
            ..probe_spec(slots, pc_count)
        })
        .expect("valid V2 probe descriptor")
    }

    fn descriptor_v2(
        slot_count: usize,
        roots: Vec<u64>,
        call_sites: Vec<CallSiteDescriptor>,
        edges: Vec<FrameEdgeSpec>,
    ) -> Arc<FrameDescriptor> {
        FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 17,
                generation: 9,
                source_map: 4,
            },
            program_counter_count: 2,
            driver_argument_count: 1,
            slot_count,
            entry_receiver_slot: 0,
            entry_argument_start: 1,
            entry_argument_count: 0,
            entry_capture_slots: Vec::new(),
            implementation_slot: None,
            root_bitmap_words: roots,
            call_sites,
            edges,
            step_method: INERT_STEP,
        })
        .expect("valid v2 test descriptor")
    }

    #[test]
    fn canonical_v2_bytes_admit_into_owned_runtime_metadata() {
        let cases: [(&str, CanonicalImplementationKind, &[u32]); 9] = [
            (
                "frame-descriptor-minimal-return.bin",
                CanonicalImplementationKind::NamedAdapter,
                &[],
            ),
            (
                "frame-descriptor-source-map-block.bin",
                CanonicalImplementationKind::BlockBody,
                &[2, 1],
            ),
            (
                "frame-descriptor-call.bin",
                CanonicalImplementationKind::NamedAdapter,
                &[],
            ),
            (
                "frame-descriptor-tail-call.bin",
                CanonicalImplementationKind::NamedAdapter,
                &[],
            ),
            (
                "frame-descriptor-branch-join.bin",
                CanonicalImplementationKind::NamedAdapter,
                &[],
            ),
            (
                "frame-descriptor-cyclic-copy.bin",
                CanonicalImplementationKind::NamedAdapter,
                &[],
            ),
            (
                "frame-descriptor-colored-reuse.bin",
                CanonicalImplementationKind::NamedAdapter,
                &[],
            ),
            (
                "frame-descriptor-construction-trace-named.bin",
                CanonicalImplementationKind::NamedAdapter,
                &[],
            ),
            (
                "frame-descriptor-construction-trace-block.bin",
                CanonicalImplementationKind::BlockBody,
                &[0],
            ),
        ];
        for (name, implementation_kind, path) in cases {
            let bytes = std::fs::read(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/stackless-s2")
                    .join(name),
            )
            .unwrap();
            let source_file_directory = 64 + 8 * 32;
            let source_file_offset = u64::from_le_bytes(
                bytes[source_file_directory + 16..source_file_directory + 24]
                    .try_into()
                    .unwrap(),
            ) as usize;
            let source_revision = bytes[source_file_offset..source_file_offset + 32]
                .try_into()
                .unwrap();
            let descriptor = FrameDescriptor::admit_v2(
                &bytes,
                SelectionSpec {
                    source_revision,
                    root_kind: RootKind::MethodOperation,
                    root_index: 0,
                    path,
                    implementation_kind,
                    disposition: Disposition::ResumableRequired,
                },
                91,
                INERT_STEP,
            )
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
            assert_eq!(descriptor.identity().generation, 91);
            assert_eq!(descriptor.layout_version, FRAME_LAYOUT_VERSION_V2);
        }
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
            "    pub(crate) fn begin_managed_turn(",
            "    pub(crate) fn end_managed_turn(",
        );
        let runtime_source = include_str!("machine.rs");
        let resolution = function_body(
            runtime_source,
            "unsafe fn lookup_target(",
            "unsafe fn loop_drive(",
        );

        for (name, body) in [
            ("managed turn entry", read_entry),
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
        let base = || FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 1,
                generation: 2,
                source_map: 3,
            },
            step_method: INERT_STEP,
            ..probe_spec(4, 2)
        };

        let mut invalid = base();
        invalid.driver_argument_count = 2;
        assert!(matches!(
            FrameDescriptor::validate_v2(invalid),
            Err(DescriptorError::InvalidDriverArgumentCount)
        ));

        let mut invalid = base();
        invalid.call_sites.push(CallSiteDescriptor {
            kind: CallSiteKind::Call,
            receiver_slot: 0,
            selector_slot: 1,
            argument_start: 1,
            argument_count: 4,
            result_slot: Some(3),
            continuation_pc: Some(1),
        });
        assert!(matches!(
            FrameDescriptor::validate_v2(invalid),
            Err(DescriptorError::InvalidArgumentRange)
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
            FrameDescriptor::validate_v2(invalid),
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
            FrameDescriptor::validate_v2(invalid),
            Err(DescriptorError::InvalidContinuation)
        ));

        let mut invalid = base();
        invalid.program_counter_count = 2;
        invalid.driver_argument_count = 0;
        assert!(matches!(
            FrameDescriptor::validate_v2(invalid),
            Err(DescriptorError::ZeroArgumentDescriptorHasMultipleEntries)
        ));
    }

    #[test]
    fn frame_push_initializes_oop_roots_and_pop_invalidates_reused_handles() {
        let nil = oop(0x1000);
        let first_root = oop(0x2000);
        let second_root = oop(0x3000);
        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            root_bitmap_words: vec![0b101, 0b101],
            ..probe_spec(3, 2)
        })
        .unwrap();
        let mut arena = FrameArena::new(19);

        let first = arena
            .push(descriptor.clone(), 0, nil, &[(0, first_root)])
            .expect("first frame push");
        assert_eq!(arena.read_oop(first, 0), Ok(first_root));
        assert_eq!(arena.read_oop(first, 2), Ok(nil));
        assert_eq!(arena.read_oop(first, 3), Err(FrameError::InvalidSlot));
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
        let descriptor = descriptor(vec![SlotKind::Oop, SlotKind::Oop], 2);
        let mut arena = FrameArena::new(23);
        let frame = arena
            .push(descriptor, 0, nil, &[(0, original)])
            .expect("frame push");

        assert_eq!(
            arena.write_oop(frame, 2, replacement),
            Err(FrameError::InvalidSlot)
        );
        assert_eq!(arena.read_oop(frame, 0), Ok(original));
        assert_eq!(
            arena.begin_continue_transition(frame, 2),
            Err(FrameError::InvalidProgramCounter)
        );
        assert_eq!(arena.program_counter(frame), Ok(0));

        arena.write_oop(frame, 0, replacement).unwrap();
        arena.begin_continue_transition(frame, 1).unwrap();
        arena.commit_continue_transition(frame, nil).unwrap();
        assert_eq!(arena.read_oop(frame, 0), Ok(replacement));
        assert_eq!(arena.program_counter(frame), Ok(1));
    }

    #[test]
    fn v2_transition_keeps_predecessor_roots_until_copy_clear_commit() {
        let nil = oop(0x1000);
        let receiver = oop(0x2000);
        let argument = oop(0x3000);
        let carried = oop(0x4000);
        let stale = oop(0x5000);
        let descriptor = descriptor_v2(
            4,
            vec![0b0111, 0b1011],
            Vec::new(),
            vec![FrameEdgeSpec {
                kind: EdgeTransitionKind::Continue,
                source_pc: 0,
                target_pc: 1,
                call_site_index: None,
                result_slot: None,
                copies: vec![SlotCopy {
                    source: 2,
                    destination: 3,
                }],
                clear_bitmap_words: vec![0b0100],
            }],
        );
        let mut arena = FrameArena::new(29);
        let frame = arena
            .push(
                descriptor,
                0,
                nil,
                &[(0, receiver), (1, argument), (2, carried), (3, stale)],
            )
            .unwrap();

        arena.begin_continue_transition(frame, 1).unwrap();
        assert_eq!(arena.program_counter(frame), Ok(1));
        assert_eq!(arena.root_state_program_counter(frame), Ok(0));
        let mut roots = Vec::new();
        arena.trace_roots(|root| roots.push(root.addr()));
        assert_eq!(roots, [receiver.addr(), argument.addr(), carried.addr()]);

        arena.commit_continue_transition(frame, nil).unwrap();
        assert_eq!(arena.root_state_program_counter(frame), Ok(1));
        assert_eq!(arena.read_oop(frame, 2), Ok(nil));
        assert_eq!(arena.read_oop(frame, 3), Ok(carried));
        roots.clear();
        arena.trace_roots(|root| roots.push(root.addr()));
        assert_eq!(roots, [receiver.addr(), argument.addr(), carried.addr()]);
    }

    #[test]
    fn v2_parallel_copy_cycles_and_call_results_use_preallocated_scratch() {
        let nil = oop(0x1000);
        let left = oop(0x2000);
        let right = oop(0x3000);
        let cyclic = descriptor_v2(
            4,
            vec![0b1111, 0b1111],
            Vec::new(),
            vec![FrameEdgeSpec {
                kind: EdgeTransitionKind::Continue,
                source_pc: 0,
                target_pc: 1,
                call_site_index: None,
                result_slot: None,
                copies: vec![
                    SlotCopy {
                        source: 3,
                        destination: 2,
                    },
                    SlotCopy {
                        source: 2,
                        destination: 3,
                    },
                ],
                clear_bitmap_words: vec![0],
            }],
        );
        let mut arena = FrameArena::new(31);
        let frame = arena
            .push(cyclic, 0, nil, &[(2, left), (3, right)])
            .unwrap();
        let reserved = arena.diagnostics().reserved_bytes;
        arena
            .begin_v2_transition(frame, EdgeTransitionKind::Continue, 1, None)
            .unwrap();
        arena.commit_v2_transition(frame, nil, None).unwrap();
        assert_eq!(arena.read_oop(frame, 2), Ok(right));
        assert_eq!(arena.read_oop(frame, 3), Ok(left));
        assert_eq!(arena.diagnostics().reserved_bytes, reserved);

        let result = oop(0x6000);
        let call_site = CallSiteDescriptor {
            kind: CallSiteKind::Call,
            receiver_slot: 0,
            selector_slot: 1,
            argument_start: 2,
            argument_count: 1,
            result_slot: Some(3),
            continuation_pc: Some(1),
        };
        let call = descriptor_v2(
            4,
            vec![0b0111, 0b1000],
            vec![call_site],
            vec![FrameEdgeSpec {
                kind: EdgeTransitionKind::Call,
                source_pc: 0,
                target_pc: 1,
                call_site_index: Some(0),
                result_slot: Some(3),
                copies: Vec::new(),
                clear_bitmap_words: vec![0b0111],
            }],
        );
        let mut call_arena = FrameArena::new(37);
        let call_frame = call_arena.push(call, 0, nil, &[]).unwrap();
        call_arena.begin_call_transition(call_frame, 1, 0).unwrap();
        assert_eq!(
            call_arena.commit_v2_transition(call_frame, nil, None),
            Err(FrameError::InvalidSlot)
        );
        call_arena
            .commit_call_transition(call_frame, 3, nil, result)
            .unwrap();
        assert_eq!(call_arena.read_oop(call_frame, 3), Ok(result));
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
    fn owned_publication_keeps_exact_contexts_without_synthetic_bindings() {
        let mut engine = ExecutionState::new(1);
        let mut registrations = Vec::new();
        for context in [1usize, 2] {
            let thunk = Method::new(
                INERT_THUNK.method.unwrap(),
                core::ptr::with_exposed_provenance_mut(context),
            );
            let cap = engine
                .register_resumable(
                    1,
                    SemanticRegistrationV1::new(thunk, descriptor(vec![SlotKind::Oop], 1), 0, 0, 1),
                )
                .unwrap();
            let lease = engine.publish_owned(cap).unwrap();
            registrations.push((thunk, cap, lease));
        }
        assert_eq!(
            engine.registration_diagnostics().direct_binding_references,
            0
        );
        assert_eq!(
            engine.registration_diagnostics().explicit_generation_leases,
            2
        );
        for (thunk, cap, _) in &registrations {
            let Some(StacklessTarget::Registered { entry }) = engine
                .classify_resumable(MethodAddress::from_method(*thunk))
                .unwrap()
            else {
                panic!("owned registration must acquire");
            };
            assert_eq!(entry, *cap);
            engine.release_acquired_entry(entry).unwrap();
        }
        engine.begin_managed_turn().unwrap();
        for (_, cap, lease) in registrations {
            engine.retire_owned(cap, lease).unwrap();
        }
        assert!(!engine.unregister_code_generation(7).unwrap());
        engine.end_managed_turn().unwrap();
        assert!(engine.unregister_code_generation(7).unwrap());
        assert_eq!(engine.registration_diagnostics().registered_entries, 0);
        assert_eq!(
            engine.registration_diagnostics().explicit_generation_leases,
            0
        );
    }

    #[test]
    fn acquired_descriptor_matches_materialization_and_requires_a_live_acquisition() {
        let owner = 74;
        let mut execution = ExecutionState::new(owner);
        let descriptor = descriptor(vec![SlotKind::Oop, SlotKind::Oop], 1);
        let capability = execution
            .register_resumable(
                owner,
                SemanticRegistrationV1::new(INERT_THUNK, descriptor.clone(), 1, 0, 1),
            )
            .unwrap();
        assert!(matches!(
            execution.acquired_entry_descriptor(capability),
            Err(RegistrationError::StaleCapability)
        ));
        let method = MethodAddress::from_method(INERT_THUNK);
        execution.note_direct_binding_added(method).unwrap();
        let Some(StacklessTarget::Registered { entry }) =
            execution.classify_resumable(method).unwrap()
        else {
            panic!("published entry must acquire");
        };
        let admitted = execution.acquired_entry_descriptor(entry).unwrap();
        let receiver = oop(0x1000);
        let nil = core::ptr::null_mut();
        let ordinary = execution
            .materialize_acquired_entry(entry, receiver, &[], &[], nil)
            .unwrap();
        assert!(Arc::ptr_eq(&admitted, &ordinary.descriptor));
        assert!(Arc::ptr_eq(&admitted, &descriptor));
        assert_eq!(ordinary.initial_oops, vec![(0, receiver), (1, nil)]);
        assert!(matches!(
            execution.acquired_entry_descriptor(ResumableEntryCapability {
                generation: entry.generation + 1,
                ..entry
            }),
            Err(RegistrationError::StaleCapability)
        ));
        execution.note_direct_binding_removed(method).unwrap();
        assert!(Arc::ptr_eq(
            &admitted,
            &execution.acquired_entry_descriptor(entry).unwrap()
        ));
        execution.release_acquired_entry(entry).unwrap();
        assert!(matches!(
            execution.acquired_entry_descriptor(entry),
            Err(RegistrationError::StaleCapability)
        ));
        assert!(matches!(
            execution.materialize_acquired_entry(entry, receiver, &[], &[], nil),
            Err(RegistrationError::StaleCapability)
        ));
        assert_eq!(execution.registration_diagnostics().acquired_entries, 0);
    }

    #[test]
    fn collected_block_bindings_republish_only_after_holders_release() {
        for holder in 0..6 {
            let owner = 74;
            let mut execution = ExecutionState::new(owner);
            let descriptor = descriptor(vec![SlotKind::Oop], 1);
            let generation = descriptor.identity().generation;
            let capability = execution
                .register_resumable(
                    owner,
                    SemanticRegistrationV1::new(INERT_THUNK, descriptor, 0, 0, 1),
                )
                .unwrap();
            let method = MethodAddress::from_method(INERT_THUNK);
            execution.note_direct_binding_added(method).unwrap();
            let mut lease = None;
            match holder {
                1 => execution.begin_managed_turn().unwrap(),
                2 | 4 | 5 => {
                    assert!(matches!(
                        execution.classify_resumable(method),
                        Ok(Some(StacklessTarget::Registered { .. }))
                    ));
                    if holder >= 4 {
                        execution
                            .transfer_acquired_entry_to_frame(capability)
                            .unwrap();
                    }
                    if holder == 5 {
                        execution
                            .transfer_frame_pin_to_continuation(capability)
                            .unwrap();
                    }
                }
                3 => lease = Some(execution.acquire_explicit_generation_lease(method).unwrap()),
                _ => (),
            }
            execution
                .note_collected_block_binding_removed(method)
                .unwrap();
            if holder != 0 {
                assert!(!execution.unregister_code_generation(generation).unwrap());
            }
            match holder {
                1 => execution.end_managed_turn().unwrap(),
                2 => execution.release_acquired_entry(capability).unwrap(),
                3 => execution
                    .release_explicit_generation_lease(lease.unwrap())
                    .unwrap(),
                4 => execution.release_entry_frame_pin(capability).unwrap(),
                5 => execution
                    .release_entry_continuation_pin(capability)
                    .unwrap(),
                _ => (),
            }
            assert!(matches!(
                execution.classify_resumable(method),
                Err(RegistrationError::EntryUnpublished)
            ));
            assert_eq!(execution.registration_diagnostics().retired_entries, 0);
            execution.note_direct_binding_added(method).unwrap();
            assert!(matches!(
                execution.classify_resumable(method),
                Ok(Some(StacklessTarget::Registered { .. }))
            ));
            execution.release_acquired_entry(capability).unwrap();
            // A subsequent explicit method removal remains terminal. A prior
            // collection must not leave permission to revive a removed method.
            execution.note_direct_binding_removed(method).unwrap();
            assert!(matches!(
                execution.classify_resumable(method),
                Err(RegistrationError::EntryRetired)
            ));
            assert_eq!(
                execution.note_direct_binding_added(method),
                Err(RegistrationError::EntryRetired)
            );
            assert!(execution.unregister_code_generation(generation).unwrap());
            assert!(!execution.has_resumable_thunk(method));
        }
    }

    #[test]
    fn collected_unpublished_block_generation_can_retire_without_republication() {
        let owner = 74;
        let mut execution = ExecutionState::new(owner);
        let descriptor = descriptor(vec![SlotKind::Oop], 1);
        let generation = descriptor.identity().generation;
        execution
            .register_resumable(
                owner,
                SemanticRegistrationV1::new(INERT_THUNK, descriptor, 0, 0, 1),
            )
            .unwrap();
        let method = MethodAddress::from_method(INERT_THUNK);
        execution.note_direct_binding_added(method).unwrap();
        execution
            .note_collected_block_binding_removed(method)
            .unwrap();
        assert!(execution.unregister_code_generation(generation).unwrap());
        assert!(!execution.has_code_generation(generation));
    }

    #[test]
    fn rollback_binding_revives_a_generation_lease_pinned_retiring_entry() {
        let owner = 73;
        let mut execution = ExecutionState::new(owner);
        let carrier =
            SemanticRegistrationV1::new(INERT_THUNK, descriptor(vec![SlotKind::Oop], 1), 0, 0, 1);
        execution
            .register_resumable(owner, carrier)
            .expect("registration");
        let method = MethodAddress::from_method(INERT_THUNK);
        execution
            .note_direct_binding_added(method)
            .expect("initial publication");
        let lease = execution
            .acquire_explicit_generation_lease(method)
            .expect("rollback generation lease");
        execution
            .note_staged_direct_binding_removed(method)
            .expect("candidate overwrite");
        execution
            .commit_staged_binding_removal(method)
            .expect("committed overwrite");

        execution
            .note_direct_binding_added(method)
            .expect("rollback restores the pinned predecessor binding");
        assert!(matches!(
            execution.classify_resumable(method),
            Ok(Some(StacklessTarget::Registered { .. }))
        ));
        execution
            .release_explicit_generation_lease(lease)
            .expect("restored live binding outlives the rollback lease");
    }

    #[test]
    fn oop_test_values_are_pointer_sized() {
        let _: *mut c_void = oop(0x1000);
    }
}
