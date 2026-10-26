//! Canonical FrameDescriptor-v2 section construction from colored logical IR.
//!
//! This production constructor covers named adapters and BlockBody
//! implementations, including canonical capture entry locations.

use std::collections::BTreeSet;

use birnam_artifact::frame_descriptor::{self, Sections};
use birnam_artifact::materialization_selection::{Row, SubjectKind};
use birnam_artifact::portable_resumable_manifest;
use birnam_artifact::stackless_selection::{
    ImplementationKind, SelectionSpec, portable_implementation_identity, source_map_identity,
};

use crate::resumable::{
    LogicalProgram, OperationKind, PhysicalLayout, ProgramCounter, Terminator, ValueId,
};

const ABSENT: u32 = u32::MAX;
pub const LOWERING_VERSION_V1: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidProgram,
    InvalidSource,
    UnsupportedImplementationKind,
    QuantityOverflow,
    Descriptor(frame_descriptor::Error),
}

impl From<frame_descriptor::Error> for Error {
    fn from(value: frame_descriptor::Error) -> Self {
        Self::Descriptor(value)
    }
}

#[derive(Clone, Copy)]
struct ValueRecord {
    kind: u32,
    binding: u32,
}

#[derive(Clone, Copy)]
enum DescriptorProfile<'a> {
    Legacy(SelectionSpec<'a>),
    Portable(Row<'a>),
}

/// Construct and semantically admit canonical descriptor-v2 bytes.
///
/// The source map currently uses one exact whole-file span for definitions and
/// uses. That span is conservative and reproducible; finer spans can be added
/// without changing execution semantics under a successor lowering version.
pub fn compile_descriptor_v2(
    program: &LogicalProgram<'_>,
    layout: &PhysicalLayout,
    authority: SelectionSpec<'_>,
    diagnostic_path: &[u8],
    source: &[u8],
    driver_argument_count: u32,
) -> Result<Vec<u8>, Error> {
    compile_descriptor_profile_v2(
        program,
        layout,
        DescriptorProfile::Legacy(authority),
        diagnostic_path,
        source,
        driver_argument_count,
        false,
    )
}

/// Native generated steps use immediate Oop counters and indices. The general
/// constructor retains the Integer counter contract for authored steps.
pub fn compile_oop_descriptor_v2(
    program: &LogicalProgram<'_>,
    layout: &PhysicalLayout,
    authority: SelectionSpec<'_>,
    diagnostic_path: &[u8],
    source: &[u8],
) -> Result<Vec<u8>, Error> {
    compile_descriptor_profile_v2(
        program,
        layout,
        DescriptorProfile::Legacy(authority),
        diagnostic_path,
        source,
        1,
        true,
    )
}

/// Construct the source-independent portable admission profile of the same
/// FrameDescriptor-v2 execution layout.
pub fn compile_portable_descriptor_v2(
    program: &LogicalProgram<'_>,
    layout: &PhysicalLayout,
    row: Row<'_>,
    driver_argument_count: u32,
) -> Result<Vec<u8>, Error> {
    compile_descriptor_profile_v2(
        program,
        layout,
        DescriptorProfile::Portable(row),
        b"",
        b"",
        driver_argument_count,
        false,
    )
}

fn compile_descriptor_profile_v2(
    program: &LogicalProgram<'_>,
    layout: &PhysicalLayout,
    profile: DescriptorProfile<'_>,
    diagnostic_path: &[u8],
    source: &[u8],
    driver_argument_count: u32,
    immediate_program_counter: bool,
) -> Result<Vec<u8>, Error> {
    if program.blocks.is_empty()
        || program.blocks.len() != layout.pc_value_locations.len()
        || program.value_count as usize != layout.value_slots.len()
        || layout.entry_argument_count != program.argument_count
        || driver_argument_count > 1
        || (driver_argument_count == 0 && program.blocks.len() != 1)
    {
        return Err(Error::InvalidProgram);
    }
    let lowering_version = if immediate_program_counter {
        2
    } else {
        LOWERING_VERSION_V1
    };
    let (implementation_kind, portable, source_map, source_digest) = match profile {
        DescriptorProfile::Legacy(authority) => (
            authority.implementation_kind,
            portable_implementation_identity(authority, lowering_version)
                .map_err(|_| Error::InvalidProgram)?,
            source_map_identity(authority, lowering_version).map_err(|_| Error::InvalidProgram)?,
            authority.source_revision,
        ),
        DescriptorProfile::Portable(row) => (
            match row.kind {
                SubjectKind::NamedAdapter => ImplementationKind::NamedAdapter,
                SubjectKind::BlockBody => ImplementationKind::BlockBody,
            },
            portable_resumable_manifest::portable_implementation_identity(row, lowering_version)
                .map_err(|_| Error::InvalidProgram)?,
            0,
            [0; 32],
        ),
    };
    match implementation_kind {
        ImplementationKind::NamedAdapter if layout.implementation_slot.is_some() => {}
        ImplementationKind::BlockBody if layout.implementation_slot.is_none() => {}
        _ => return Err(Error::InvalidProgram),
    }
    validate_colored_program(program, layout)?;

    let mut metadata = Vec::with_capacity(64);
    push_u64(&mut metadata, portable);
    push_u64(&mut metadata, source_map);
    push_u32(&mut metadata, lowering_version);
    push_u32(&mut metadata, implementation_kind as u32);
    push_u32(&mut metadata, layout.slot_count);
    push_u32(
        &mut metadata,
        u32::try_from(program.blocks.len()).map_err(|_| Error::QuantityOverflow)?,
    );
    push_u32(&mut metadata, driver_argument_count);
    push_u32(&mut metadata, layout.entry_receiver_slot);
    push_u32(&mut metadata, layout.entry_argument_start);
    push_u32(&mut metadata, layout.entry_argument_count);
    push_u32(&mut metadata, layout.implementation_slot.unwrap_or(ABSENT));
    push_u32(&mut metadata, layout.call_window_start.unwrap_or(ABSENT));
    push_u32(&mut metadata, layout.call_window_argument_capacity);
    push_u32(&mut metadata, u32::from(immediate_program_counter));

    let edge_index = |source: ProgramCounter, target: ProgramCounter| {
        layout
            .edges
            .iter()
            .position(|edge| edge.source == source && edge.target == target)
            .and_then(|index| u32::try_from(index).ok())
    };
    let mut calls = Vec::new();
    let mut selectors = Vec::new();
    let mut call_index_by_pc = vec![None; program.blocks.len()];
    for block in &program.blocks {
        let (kind, call_site, receiver, selector, arguments, result, continuation, edge) =
            match &block.terminator {
                Terminator::Call {
                    call_site,
                    receiver,
                    selector,
                    arguments,
                    result,
                    successor,
                    ..
                } => (
                    1,
                    *call_site,
                    *receiver,
                    *selector,
                    arguments.as_slice(),
                    Some(*result),
                    Some(*successor),
                    edge_index(block.pc, *successor),
                ),
                Terminator::TailCall {
                    call_site,
                    receiver,
                    selector,
                    arguments,
                    ..
                } => (
                    2,
                    *call_site,
                    *receiver,
                    *selector,
                    arguments.as_slice(),
                    None,
                    None,
                    None,
                ),
                _ => continue,
            };
        let window = layout.call_window_start.ok_or(Error::InvalidProgram)?;
        let call_index = u32::try_from(calls.len() / 48).map_err(|_| Error::QuantityOverflow)?;
        if call_site.0 != call_index {
            return Err(Error::InvalidProgram);
        }
        call_index_by_pc[block.pc.0 as usize] = Some(call_index);
        push_u32(&mut calls, kind);
        push_u32(&mut calls, 0);
        push_u32(&mut calls, block.pc.0);
        push_u32(&mut calls, window);
        push_u32(
            &mut calls,
            window.checked_add(1).ok_or(Error::QuantityOverflow)?,
        );
        push_u32(
            &mut calls,
            window.checked_add(2).ok_or(Error::QuantityOverflow)?,
        );
        push_u32(
            &mut calls,
            u32::try_from(arguments.len()).map_err(|_| Error::QuantityOverflow)?,
        );
        push_u32(
            &mut calls,
            result
                .map(|value| layout.value_slots[value.0 as usize])
                .unwrap_or(ABSENT),
        );
        push_u32(&mut calls, continuation.map(|pc| pc.0).unwrap_or(ABSENT));
        push_u32(&mut calls, edge.unwrap_or(ABSENT));
        push_u32(&mut calls, 0);
        push_u32(
            &mut calls,
            u32::try_from(selector.len()).map_err(|_| Error::QuantityOverflow)?,
        );
        selectors.extend_from_slice(selector);

        // Values are staged by generated code, but the descriptor's window is
        // deliberately independent of their colored source locations.
        let _ = receiver;
    }
    if calls.len() / 48 != program.call_site_count as usize {
        return Err(Error::InvalidProgram);
    }

    let mut edges = Vec::new();
    let mut copies = Vec::new();
    let mut clears = Vec::new();
    for edge in &layout.edges {
        let source_block = program
            .blocks
            .get(edge.source.0 as usize)
            .ok_or(Error::InvalidProgram)?;
        let call_site = match source_block.terminator {
            Terminator::Call { .. } => call_index_by_pc[edge.source.0 as usize],
            Terminator::Continue(_) => None,
            _ => return Err(Error::InvalidProgram),
        };
        let kind = if call_site.is_some() { 2 } else { 1 };
        let first_copy = u32::try_from(copies.len() / 8).map_err(|_| Error::QuantityOverflow)?;
        for copy in &edge.copies {
            push_u32(&mut copies, copy.source);
            push_u32(&mut copies, copy.destination);
        }
        push_u32(&mut edges, kind);
        push_u32(&mut edges, 0);
        push_u32(&mut edges, edge.source.0);
        push_u32(&mut edges, edge.target.0);
        push_u32(&mut edges, call_site.unwrap_or(ABSENT));
        push_u32(&mut edges, first_copy);
        push_u32(
            &mut edges,
            u32::try_from(edge.copies.len()).map_err(|_| Error::QuantityOverflow)?,
        );
        push_u32(
            &mut edges,
            edge.result_initialization_slot.unwrap_or(ABSENT),
        );
        for word in &edge.clear_bitmap_words {
            push_u64(&mut clears, *word);
        }
    }

    let mut locations = Vec::new();
    let mut location_starts = Vec::with_capacity(program.blocks.len());
    for (pc, values) in layout.pc_value_locations.iter().enumerate() {
        let first = u32::try_from(locations.len() / 16).map_err(|_| Error::QuantityOverflow)?;
        for (value, slot) in values {
            push_u32(
                &mut locations,
                u32::try_from(pc).map_err(|_| Error::QuantityOverflow)?,
            );
            push_u32(&mut locations, value.0);
            push_u32(&mut locations, 1);
            push_u32(&mut locations, *slot);
        }
        location_starts.push((
            first,
            u32::try_from(values.len()).map_err(|_| Error::QuantityOverflow)?,
        ));
    }
    let mut pc_records = Vec::new();
    for block in &program.blocks {
        let (kind, payload) = match block.terminator {
            Terminator::Call { .. } => (
                1,
                call_index_by_pc[block.pc.0 as usize].ok_or(Error::InvalidProgram)?,
            ),
            Terminator::TailCall { .. } => (
                2,
                call_index_by_pc[block.pc.0 as usize].ok_or(Error::InvalidProgram)?,
            ),
            Terminator::Return(value) => (3, layout.value_slots[value.0 as usize]),
            Terminator::Continue(target) => (
                4,
                edge_index(block.pc, target).ok_or(Error::InvalidProgram)?,
            ),
        };
        let (first, count) = location_starts[block.pc.0 as usize];
        push_u32(&mut pc_records, kind);
        push_u32(&mut pc_records, 0);
        push_u32(&mut pc_records, payload);
        push_u32(&mut pc_records, 0);
        push_u32(&mut pc_records, first);
        push_u32(&mut pc_records, count);
        push_u32(&mut pc_records, 0);
        push_u32(&mut pc_records, 0);
    }

    let mut bindings = Vec::new();
    let mut binding_names = Vec::new();
    let mut binding_indices = vec![None; program.binding_count as usize];
    append_binding(&mut bindings, &mut binding_names, 0, 1, b"");
    for block in &program.blocks {
        for operation in &block.operations {
            let (binding, kind, name) = match operation.kind {
                OperationKind::EntryArgument { binding, name, .. } => (binding, 2, name),
                OperationKind::EntryCapture { binding, name, .. } => (binding, 4, name),
                OperationKind::Bind { binding, name, .. } => (binding, 3, name),
                _ => continue,
            };
            let index = u32::try_from(bindings.len() / 32).map_err(|_| Error::QuantityOverflow)?;
            let target = binding_indices
                .get_mut(binding.0 as usize)
                .ok_or(Error::InvalidProgram)?;
            if target.replace(index).is_some() {
                return Err(Error::InvalidProgram);
            }
            append_binding(
                &mut bindings,
                &mut binding_names,
                u64::from(binding.0) + 1,
                kind,
                name,
            );
        }
    }
    if binding_indices.iter().any(Option::is_none) {
        return Err(Error::InvalidProgram);
    }

    let mut value_records = vec![None; program.value_count as usize];
    for block in &program.blocks {
        for operation in &block.operations {
            let record = match operation.kind {
                OperationKind::EntryReceiver => ValueRecord {
                    kind: 1,
                    binding: 0,
                },
                OperationKind::EntryImplementation => ValueRecord {
                    kind: 3,
                    binding: 0,
                },
                OperationKind::EntryArgument { binding, .. } => ValueRecord {
                    kind: 2,
                    binding: binding_indices[binding.0 as usize].unwrap(),
                },
                OperationKind::EntryCapture { binding, .. } => ValueRecord {
                    kind: 3,
                    binding: binding_indices[binding.0 as usize].unwrap(),
                },
                OperationKind::Bind { binding, .. } => ValueRecord {
                    kind: 5,
                    binding: binding_indices[binding.0 as usize].unwrap(),
                },
                OperationKind::BlockLiteral { .. } | OperationKind::PortableBlockLiteral { .. } => {
                    ValueRecord {
                        kind: 8,
                        binding: ABSENT,
                    }
                }
                OperationKind::Literal(_) => ValueRecord {
                    kind: 9,
                    binding: ABSENT,
                },
                OperationKind::GlobalLookup(_) => ValueRecord {
                    kind: 6,
                    binding: ABSENT,
                },
            };
            set_value_record(&mut value_records, operation.result, record)?;
        }
        if let Terminator::Call { result, .. } = block.terminator {
            set_value_record(
                &mut value_records,
                result,
                ValueRecord {
                    kind: 7,
                    binding: ABSENT,
                },
            )?;
        }
    }
    for record in &mut value_records {
        if record.is_none() {
            *record = Some(ValueRecord {
                kind: 7,
                binding: ABSENT,
            });
        }
    }

    let mut uses_by_value = vec![BTreeSet::new(); program.value_count as usize];
    for block in &program.blocks {
        for operation in &block.operations {
            if let OperationKind::Bind { source, .. } = operation.kind {
                uses_by_value[source.0 as usize].insert((block.pc.0, 0u32, 1u32));
            }
        }
        match &block.terminator {
            Terminator::Call {
                receiver,
                arguments,
                ..
            }
            | Terminator::TailCall {
                receiver,
                arguments,
                ..
            } => {
                uses_by_value[receiver.0 as usize].insert((block.pc.0, 0, 2));
                for argument in arguments {
                    uses_by_value[argument.0 as usize].insert((block.pc.0, 0, 2));
                }
            }
            Terminator::Return(value) => {
                uses_by_value[value.0 as usize].insert((block.pc.0, 0, 3));
            }
            Terminator::Continue(_) => {}
        }
    }
    let mut logical_values = Vec::new();
    let mut logical_uses = Vec::new();
    for (value_id, record) in value_records.into_iter().enumerate() {
        let record = record.ok_or(Error::InvalidProgram)?;
        let first_use =
            u32::try_from(logical_uses.len() / 16).map_err(|_| Error::QuantityOverflow)?;
        for (pc, span, kind) in &uses_by_value[value_id] {
            push_u32(
                &mut logical_uses,
                u32::try_from(value_id).map_err(|_| Error::QuantityOverflow)?,
            );
            push_u32(&mut logical_uses, *pc);
            push_u32(&mut logical_uses, *span);
            push_u32(&mut logical_uses, *kind);
        }
        push_u32(
            &mut logical_values,
            u32::try_from(value_id).map_err(|_| Error::QuantityOverflow)?,
        );
        push_u32(&mut logical_values, record.kind);
        push_u32(&mut logical_values, record.binding);
        push_u32(&mut logical_values, 0);
        push_u32(&mut logical_values, first_use);
        push_u32(
            &mut logical_values,
            u32::try_from(uses_by_value[value_id].len()).map_err(|_| Error::QuantityOverflow)?,
        );
        push_u32(&mut logical_values, 1);
        push_u32(&mut logical_values, 0);
    }

    let mut roots = Vec::with_capacity(layout.root_bitmap_words.len() * 8);
    for word in &layout.root_bitmap_words {
        push_u64(&mut roots, *word);
    }
    let mut source_files = Vec::with_capacity(48);
    source_files.extend_from_slice(&source_digest);
    push_u64(
        &mut source_files,
        u64::try_from(source.len()).map_err(|_| Error::QuantityOverflow)?,
    );
    push_u32(
        &mut source_files,
        u32::try_from(diagnostic_path.len()).map_err(|_| Error::QuantityOverflow)?,
    );
    push_u32(&mut source_files, 0);
    let mut source_spans = Vec::with_capacity(24);
    push_u32(&mut source_spans, 0);
    push_u32(&mut source_spans, 0);
    push_u64(&mut source_spans, 0);
    push_u64(
        &mut source_spans,
        u64::try_from(source.len()).map_err(|_| Error::QuantityOverflow)?,
    );
    let mut source_bytes = selectors;
    source_bytes.extend_from_slice(diagnostic_path);
    source_bytes.extend_from_slice(&binding_names);

    let mut capture_slots = Vec::with_capacity(layout.entry_capture_slots.len() * 4);
    for slot in &layout.entry_capture_slots {
        push_u32(&mut capture_slots, *slot);
    }

    let sections = Sections {
        metadata: &metadata,
        entry_capture_slots: &capture_slots,
        program_counters: &pc_records,
        root_bitmap_words: &roots,
        call_sites: &calls,
        edges: &edges,
        copy_pairs: &copies,
        clear_bitmap_words: &clears,
        source_files: &source_files,
        source_spans: &source_spans,
        lexical_bindings: &bindings,
        logical_values: &logical_values,
        logical_uses: &logical_uses,
        pc_value_locations: &locations,
        source_bytes: &source_bytes,
    };
    match profile {
        DescriptorProfile::Legacy(authority) => frame_descriptor::materialize(authority, sections),
        DescriptorProfile::Portable(row) => frame_descriptor::materialize_portable(row, sections),
    }
    .map_err(Error::from)
}

fn validate_colored_program(
    program: &LogicalProgram<'_>,
    layout: &PhysicalLayout,
) -> Result<(), Error> {
    let root_word_count = (layout.root_words_per_pc as usize)
        .checked_mul(program.blocks.len())
        .ok_or(Error::QuantityOverflow)?;
    let entry_argument_end = layout
        .entry_argument_start
        .checked_add(layout.entry_argument_count)
        .ok_or(Error::QuantityOverflow)?;
    let mut entry_slots = BTreeSet::new();
    entry_slots.insert(layout.entry_receiver_slot);
    entry_slots.extend(layout.entry_argument_start..entry_argument_end);
    if let Some(slot) = layout.implementation_slot {
        entry_slots.insert(slot);
    }
    let entry_slots_before_captures = entry_slots.len();
    entry_slots.extend(layout.entry_capture_slots.iter().copied());
    if layout.slot_count == 0
        || layout.root_words_per_pc
            != layout
                .slot_count
                .checked_add(63)
                .ok_or(Error::QuantityOverflow)?
                / 64
        || layout.root_bitmap_words.len() != root_word_count
        || layout
            .value_slots
            .iter()
            .any(|slot| *slot >= layout.slot_count)
        || layout.entry_receiver_slot >= layout.slot_count
        || entry_argument_end > layout.slot_count
        || layout.entry_capture_slots.len() != program.capture_count as usize
        || layout
            .entry_capture_slots
            .iter()
            .any(|slot| *slot >= layout.slot_count)
        || entry_slots_before_captures
            != 1 + layout.entry_argument_count as usize
                + usize::from(layout.implementation_slot.is_some())
        || entry_slots.len() != entry_slots_before_captures + layout.entry_capture_slots.len()
    {
        return Err(Error::InvalidProgram);
    }
    if let Some(window) = layout.call_window_start {
        let end = window
            .checked_add(2)
            .and_then(|slot| slot.checked_add(layout.call_window_argument_capacity))
            .ok_or(Error::QuantityOverflow)?;
        if end > layout.slot_count {
            return Err(Error::InvalidProgram);
        }
    } else if layout.call_window_argument_capacity != 0 {
        return Err(Error::InvalidProgram);
    }
    for (index, block) in program.blocks.iter().enumerate() {
        if block.pc.0 as usize != index {
            return Err(Error::InvalidProgram);
        }
        for operation in &block.operations {
            if operation.result.0 >= program.value_count {
                return Err(Error::InvalidProgram);
            }
            match operation.kind {
                OperationKind::EntryArgument { index, binding, .. } => {
                    if index >= program.argument_count || binding.0 >= program.binding_count {
                        return Err(Error::InvalidProgram);
                    }
                }
                OperationKind::EntryCapture { index, binding, .. } => {
                    if index >= program.capture_count || binding.0 >= program.binding_count {
                        return Err(Error::InvalidProgram);
                    }
                }
                OperationKind::EntryImplementation => {
                    if layout.implementation_slot.is_none() {
                        return Err(Error::InvalidProgram);
                    }
                }
                OperationKind::Bind {
                    binding, source, ..
                } => {
                    if binding.0 >= program.binding_count || source.0 >= program.value_count {
                        return Err(Error::InvalidProgram);
                    }
                }
                _ => {}
            }
        }
        let valid_value = |value: ValueId| value.0 < program.value_count;
        match &block.terminator {
            Terminator::Call {
                receiver,
                arguments,
                result,
                successor,
                ..
            } => {
                if !valid_value(*receiver)
                    || !valid_value(*result)
                    || arguments.iter().any(|value| !valid_value(*value))
                    || successor.0 as usize >= program.blocks.len()
                {
                    return Err(Error::InvalidProgram);
                }
            }
            Terminator::TailCall {
                receiver,
                arguments,
                ..
            } => {
                if !valid_value(*receiver) || arguments.iter().any(|value| !valid_value(*value)) {
                    return Err(Error::InvalidProgram);
                }
            }
            Terminator::Return(value) => {
                if !valid_value(*value) {
                    return Err(Error::InvalidProgram);
                }
            }
            Terminator::Continue(target) => {
                if target.0 as usize >= program.blocks.len() {
                    return Err(Error::InvalidProgram);
                }
            }
        }
    }
    for (pc, locations) in layout.pc_value_locations.iter().enumerate() {
        let mut previous = None;
        for (value, slot) in locations {
            if value.0 >= program.value_count
                || *slot >= layout.slot_count
                || layout.value_slots[value.0 as usize] != *slot
                || previous.is_some_and(|prior| prior >= value.0)
                || pc >= program.blocks.len()
            {
                return Err(Error::InvalidProgram);
            }
            previous = Some(value.0);
        }
    }
    for edge in &layout.edges {
        if edge.source.0 as usize >= program.blocks.len()
            || edge.target.0 as usize >= program.blocks.len()
            || edge.clear_bitmap_words.len() != layout.root_words_per_pc as usize
            || edge
                .result_initialization_slot
                .is_some_and(|slot| slot >= layout.slot_count)
            || edge.copies.iter().any(|copy| {
                copy.source >= layout.slot_count || copy.destination >= layout.slot_count
            })
        {
            return Err(Error::InvalidProgram);
        }
    }
    Ok(())
}

/// Compatibility name for the capture-free BlockBody constructor.
pub fn compile_block_descriptor_v2(
    program: &LogicalProgram<'_>,
    layout: &PhysicalLayout,
    authority: SelectionSpec<'_>,
    diagnostic_path: &[u8],
    source: &[u8],
    driver_argument_count: u32,
) -> Result<Vec<u8>, Error> {
    if authority.implementation_kind != ImplementationKind::BlockBody {
        return Err(Error::UnsupportedImplementationKind);
    }
    compile_descriptor_v2(
        program,
        layout,
        authority,
        diagnostic_path,
        source,
        driver_argument_count,
    )
}

fn append_binding(
    records: &mut Vec<u8>,
    names: &mut Vec<u8>,
    identity: u64,
    kind: u32,
    name: &[u8],
) {
    push_u64(records, identity);
    push_u32(records, kind);
    push_u32(records, name.len() as u32);
    push_u32(records, 0);
    push_u32(records, 0);
    push_u32(records, ABSENT);
    push_u32(records, 0);
    names.extend_from_slice(name);
}

fn set_value_record(
    records: &mut [Option<ValueRecord>],
    value: ValueId,
    record: ValueRecord,
) -> Result<(), Error> {
    let target = records
        .get_mut(value.0 as usize)
        .ok_or(Error::InvalidProgram)?;
    if target.replace(record).is_some() {
        return Err(Error::InvalidProgram);
    }
    Ok(())
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use birnam_artifact::frame_descriptor::inspect;
    use birnam_artifact::stackless_selection::{Disposition, RootKind};

    use super::*;
    use crate::resumable::{color_slots, lower_block};
    use crate::{Block, Expr, Statement};

    fn authority<'a>(path: &'a [u32]) -> SelectionSpec<'a> {
        SelectionSpec {
            source_revision: [7; 32],
            root_kind: RootKind::MethodOperation,
            root_index: 3,
            path,
            implementation_kind: ImplementationKind::BlockBody,
            disposition: Disposition::ResumableRequired,
        }
    }

    fn send<'a>(receiver: Expr<'a>, selector: &'a [u8]) -> Expr<'a> {
        Expr::Send {
            receiver: Box::new(receiver),
            selector: Cow::Borrowed(selector),
            args: Vec::new(),
            source: selector,
        }
    }

    #[test]
    fn generated_counter_mode_is_explicit_and_legacy_steps_keep_integers() {
        let block = Block {
            params: vec![b"value"],
            body: vec![Statement::Expr(Expr::Global(b"value"))],
        };
        let program = lower_block(&block).unwrap();
        let layout = color_slots(&program).unwrap();
        let authority = authority(&[0]);
        let old =
            compile_descriptor_v2(&program, &layout, authority, b"test", b"source", 1).unwrap();
        let new =
            compile_oop_descriptor_v2(&program, &layout, authority, b"test", b"source").unwrap();
        let old = inspect(&old, authority).unwrap();
        let admitted = inspect(&new, authority).unwrap();
        assert!(!old.metadata().immediate_program_counter);
        assert!(admitted.metadata().immediate_program_counter);
        assert_eq!(admitted.metadata().lowering_version, 2);
        assert_ne!(
            old.metadata().portable_implementation_identity,
            admitted.metadata().portable_implementation_identity
        );
        // Metadata begins after the fixed directory. Unknown counter flags and
        // immediate counters without a driver argument are not admitted.
        let metadata = u64::from_le_bytes(new[80..88].try_into().unwrap()) as usize;
        let mut bad = new.clone();
        bad[metadata + 0x3c..metadata + 0x40].copy_from_slice(&2u32.to_le_bytes());
        assert!(inspect(&bad, authority).is_err());
        let mut bad = new;
        bad[metadata + 0x20..metadata + 0x24].copy_from_slice(&0u32.to_le_bytes());
        assert!(inspect(&bad, authority).is_err());
    }

    #[test]
    fn capture_free_block_descriptor_is_deterministic_and_admitted() {
        let block = Block {
            params: vec![b"value"],
            body: vec![Statement::Expr(Expr::Global(b"value"))],
        };
        let program = lower_block(&block).unwrap();
        let layout = color_slots(&program).unwrap();
        let first = compile_block_descriptor_v2(
            &program,
            &layout,
            authority(&[0]),
            b"unit.birnam",
            b"[:value | value]",
            0,
        )
        .unwrap();
        let second = compile_block_descriptor_v2(
            &program,
            &layout,
            authority(&[0]),
            b"unit.birnam",
            b"[:value | value]",
            0,
        )
        .unwrap();
        assert_eq!(first, second);
        let descriptor = inspect(&first, authority(&[0])).unwrap();
        assert_eq!(descriptor.metadata().slot_count, layout.slot_count);
        assert_eq!(descriptor.metadata().entry_argument_count, 1);
        assert_eq!(descriptor.call_site_count(), 0);
    }

    #[test]
    fn call_descriptor_agrees_with_coloring_edges_and_staging() {
        let block = Block {
            params: vec![b"worker"],
            body: vec![
                Statement::Expr(send(Expr::Global(b"worker"), b"run")),
                Statement::Expr(Expr::Global(b"worker")),
            ],
        };
        let program = lower_block(&block).unwrap();
        let layout = color_slots(&program).unwrap();
        let bytes = compile_block_descriptor_v2(
            &program,
            &layout,
            authority(&[1]),
            b"unit.birnam",
            b"[:worker | worker run. worker]",
            1,
        )
        .unwrap();
        let descriptor = inspect(&bytes, authority(&[1])).unwrap();
        assert_eq!(descriptor.call_site_count(), 1);
        assert_eq!(descriptor.edge_count(), 1);
        assert_eq!(
            descriptor.metadata().call_window_start,
            layout.call_window_start
        );
        assert_eq!(
            descriptor.edge(0).unwrap().result_initialization_slot,
            layout.edges[0].result_initialization_slot
        );
    }
}
