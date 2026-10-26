//! Borrowed, bounded admission for the canonical FrameDescriptor v2 bytes.
//!
//! Runtime code may copy the admitted execution sections into immutable owned
//! metadata. No decoded view contains a pointer supplied by native code.

use crate::stackless_selection::{
    ImplementationKind, SelectionSpec, portable_implementation_identity, source_map_identity,
};
use crate::{
    materialization_selection::{Disposition as PortableDisposition, Row, SubjectKind},
    portable_resumable_manifest,
};

pub const DOMAIN: [u8; 32] = *b"birnam.frame-descriptor.v2\0\0\0\0\0\0";
pub const MAX_DESCRIPTOR_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_FRAME_SLOTS: usize = 65_536;
pub const MAX_PROGRAM_COUNTERS: usize = 65_536;
pub const MAX_CALL_SITES: usize = 65_536;
pub const MAX_EDGES: usize = 262_144;
pub const MAX_COPY_PAIRS: usize = 262_144;
pub const MAX_COPY_PAIRS_PER_EDGE: usize = 65_536;
pub const MAX_LOGICAL_VALUES: usize = 262_144;
pub const MAX_SOURCE_RECORDS: usize = 262_144;
pub const MAX_BITMAP_WORDS: usize = 1_048_576;
pub const MAX_SELECTOR_BYTES: usize = 1024 * 1024;

const ABSENT: u32 = u32::MAX;
const PREFIX_BYTES: usize = 64;
const DIRECTORY_STRIDE: usize = 32;
const SECTION_COUNT: usize = 15;
const DATA_OFFSET: usize = PREFIX_BYTES + SECTION_COUNT * DIRECTORY_STRIDE;
const STRIDES: [usize; SECTION_COUNT] = [64, 4, 32, 8, 48, 32, 8, 8, 48, 24, 32, 32, 16, 16, 1];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    TooLarge,
    Truncated,
    Domain,
    Prefix,
    RequiredFlags,
    Directory,
    Padding,
    Arithmetic,
    Limit,
    Identity,
    Metadata,
    Entry,
    ProgramCounters,
    Roots,
    Calls,
    Edges,
    CopyPairs,
    ClearMaps,
    SourceMap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminatorKind {
    Call,
    TailCall,
    Return,
    Continue,
    Abort,
}

impl TerminatorKind {
    fn decode(value: u32) -> Result<Self, Error> {
        match value {
            1 => Ok(Self::Call),
            2 => Ok(Self::TailCall),
            3 => Ok(Self::Return),
            4 => Ok(Self::Continue),
            5 => Ok(Self::Abort),
            _ => Err(Error::ProgramCounters),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallKind {
    Call,
    TailCall,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeKind {
    Continue,
    Call,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Metadata {
    pub portable_implementation_identity: u64,
    pub source_map_identity: u64,
    pub lowering_version: u32,
    pub implementation_kind: ImplementationKind,
    pub slot_count: u32,
    pub program_counter_count: u32,
    pub driver_argument_count: u32,
    pub immediate_program_counter: bool,
    pub entry_receiver_slot: u32,
    pub entry_argument_start: u32,
    pub entry_argument_count: u32,
    pub implementation_slot: Option<u32>,
    pub call_window_start: Option<u32>,
    pub call_window_argument_capacity: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProgramCounterRecord {
    pub kind: TerminatorKind,
    pub payload_index: u32,
    pub source_span_index: u32,
    pub first_value_location: u32,
    pub value_location_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallSite<'a> {
    pub kind: CallKind,
    pub source_pc: u32,
    pub receiver_slot: u32,
    pub selector_slot: u32,
    pub argument_start: u32,
    pub argument_count: u32,
    pub result_slot: Option<u32>,
    pub continuation_pc: Option<u32>,
    pub edge_index: Option<u32>,
    pub source_span_index: u32,
    pub selector: &'a [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    pub kind: EdgeKind,
    pub source_pc: u32,
    pub target_pc: u32,
    pub call_site_index: Option<u32>,
    pub first_copy_pair: u32,
    pub copy_pair_count: u32,
    pub result_initialization_slot: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CopyPair {
    pub source_slot: u32,
    pub destination_slot: u32,
}

/// Canonical section payloads for one FrameDescriptor v2 value.
///
/// Fixed-width records are already encoded little-endian. `materialize`
/// owns canonical framing, placement, zero padding, limit enforcement, and a
/// complete semantic admission pass against the stable implementation key.
#[derive(Clone, Copy)]
pub struct Sections<'a> {
    pub metadata: &'a [u8],
    pub entry_capture_slots: &'a [u8],
    pub program_counters: &'a [u8],
    pub root_bitmap_words: &'a [u8],
    pub call_sites: &'a [u8],
    pub edges: &'a [u8],
    pub copy_pairs: &'a [u8],
    pub clear_bitmap_words: &'a [u8],
    pub source_files: &'a [u8],
    pub source_spans: &'a [u8],
    pub lexical_bindings: &'a [u8],
    pub logical_values: &'a [u8],
    pub logical_uses: &'a [u8],
    pub pc_value_locations: &'a [u8],
    pub source_bytes: &'a [u8],
}

impl<'a> Sections<'a> {
    fn as_array(self) -> [&'a [u8]; SECTION_COUNT] {
        [
            self.metadata,
            self.entry_capture_slots,
            self.program_counters,
            self.root_bitmap_words,
            self.call_sites,
            self.edges,
            self.copy_pairs,
            self.clear_bitmap_words,
            self.source_files,
            self.source_spans,
            self.lexical_bindings,
            self.logical_values,
            self.logical_uses,
            self.pc_value_locations,
            self.source_bytes,
        ]
    }
}

#[derive(Clone, Copy)]
struct Section<'a> {
    bytes: &'a [u8],
    count: usize,
}

#[derive(Clone)]
pub struct FrameDescriptor<'a> {
    bytes: &'a [u8],
    metadata: Metadata,
    sections: [Section<'a>; SECTION_COUNT],
    bitmap_words_per_pc: usize,
    selector_offsets: Vec<(usize, usize)>,
}

/// Serialize one canonical FrameDescriptor v2 and reject any section set that
/// would not be admitted byte-for-byte by the production decoder.
pub fn materialize(authority: SelectionSpec<'_>, sections: Sections<'_>) -> Result<Vec<u8>, Error> {
    let output = materialize_sections(sections)?;
    inspect(&output, authority)?;
    Ok(output)
}

/// Serialize the portable admission profile of FrameDescriptor layout v2.
pub fn materialize_portable(row: Row<'_>, sections: Sections<'_>) -> Result<Vec<u8>, Error> {
    let output = materialize_sections(sections)?;
    inspect_portable(&output, row)?;
    Ok(output)
}

fn materialize_sections(sections: Sections<'_>) -> Result<Vec<u8>, Error> {
    let sections = sections.as_array();
    let mut cursor = DATA_OFFSET;
    let mut fixed_records = 0usize;
    let mut entries = [(0usize, 0usize); SECTION_COUNT];
    for (index, (section, stride)) in sections.iter().zip(STRIDES).enumerate() {
        if section.len() % stride != 0 {
            return Err(Error::Directory);
        }
        let count = section.len() / stride;
        enforce_section_limit(index, count)?;
        if index < 14 {
            fixed_records = fixed_records.checked_add(count).ok_or(Error::Arithmetic)?;
            if fixed_records > 1_048_576 {
                return Err(Error::Limit);
            }
        }
        let offset = align8(cursor)?;
        entries[index] = (offset, count);
        cursor = offset.checked_add(section.len()).ok_or(Error::Arithmetic)?;
        if cursor > MAX_DESCRIPTOR_BYTES {
            return Err(Error::TooLarge);
        }
    }
    let total = cursor;
    let total_u64 = u64::try_from(total).map_err(|_| Error::Arithmetic)?;
    let mut output = Vec::with_capacity(total);
    output.extend_from_slice(&DOMAIN);
    push_u32(&mut output, 2);
    push_u32(&mut output, PREFIX_BYTES as u32);
    push_u64(&mut output, total_u64);
    push_u32(&mut output, 0);
    push_u32(&mut output, SECTION_COUNT as u32);
    push_u32(&mut output, DIRECTORY_STRIDE as u32);
    push_u32(&mut output, 0);
    for (index, ((offset, count), stride)) in entries.iter().zip(STRIDES).enumerate() {
        push_u32(&mut output, index as u32 + 1);
        push_u32(&mut output, stride as u32);
        push_u32(
            &mut output,
            u32::try_from(*count).map_err(|_| Error::Arithmetic)?,
        );
        push_u32(&mut output, 0);
        push_u64(
            &mut output,
            u64::try_from(*offset).map_err(|_| Error::Arithmetic)?,
        );
        push_u64(
            &mut output,
            u64::try_from(sections[index].len()).map_err(|_| Error::Arithmetic)?,
        );
    }
    for (index, section) in sections.iter().enumerate() {
        output.resize(entries[index].0, 0);
        output.extend_from_slice(section);
    }
    if output.len() != total {
        return Err(Error::Arithmetic);
    }
    Ok(output)
}

impl<'a> FrameDescriptor<'a> {
    pub const fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    pub const fn metadata(&self) -> Metadata {
        self.metadata
    }

    pub const fn capture_count(&self) -> usize {
        self.sections[1].count
    }

    pub fn capture_slot(&self, index: usize) -> Option<u32> {
        read_u32(self.sections[1].bytes, index.checked_mul(4)?)
    }

    pub const fn program_counter_count(&self) -> usize {
        self.sections[2].count
    }

    pub fn program_counter(&self, index: usize) -> Option<ProgramCounterRecord> {
        let base = index.checked_mul(32)?;
        let bytes = self.sections[2].bytes.get(base..base + 32)?;
        Some(ProgramCounterRecord {
            kind: TerminatorKind::decode(read_u32(bytes, 0)?).ok()?,
            payload_index: read_u32(bytes, 8)?,
            source_span_index: read_u32(bytes, 12)?,
            first_value_location: read_u32(bytes, 16)?,
            value_location_count: read_u32(bytes, 20)?,
        })
    }

    pub fn root_word(&self, pc: usize, word: usize) -> Option<u64> {
        if word >= self.bitmap_words_per_pc {
            return None;
        }
        let index = pc
            .checked_mul(self.bitmap_words_per_pc)?
            .checked_add(word)?;
        read_u64(self.sections[3].bytes, index.checked_mul(8)?)
    }

    pub const fn call_site_count(&self) -> usize {
        self.sections[4].count
    }

    pub fn call_site(&self, index: usize) -> Option<CallSite<'a>> {
        let base = index.checked_mul(48)?;
        let bytes = self.sections[4].bytes.get(base..base + 48)?;
        let (selector_start, selector_length) = *self.selector_offsets.get(index)?;
        let kind = match read_u32(bytes, 0)? {
            1 => CallKind::Call,
            2 => CallKind::TailCall,
            _ => return None,
        };
        Some(CallSite {
            kind,
            source_pc: read_u32(bytes, 8)?,
            receiver_slot: read_u32(bytes, 12)?,
            selector_slot: read_u32(bytes, 16)?,
            argument_start: read_u32(bytes, 20)?,
            argument_count: read_u32(bytes, 24)?,
            result_slot: optional(read_u32(bytes, 28)?),
            continuation_pc: optional(read_u32(bytes, 32)?),
            edge_index: optional(read_u32(bytes, 36)?),
            source_span_index: read_u32(bytes, 40)?,
            selector: self.sections[14]
                .bytes
                .get(selector_start..selector_start + selector_length)?,
        })
    }

    pub const fn edge_count(&self) -> usize {
        self.sections[5].count
    }

    pub fn edge(&self, index: usize) -> Option<Edge> {
        let base = index.checked_mul(32)?;
        let bytes = self.sections[5].bytes.get(base..base + 32)?;
        Some(Edge {
            kind: match read_u32(bytes, 0)? {
                1 => EdgeKind::Continue,
                2 => EdgeKind::Call,
                _ => return None,
            },
            source_pc: read_u32(bytes, 8)?,
            target_pc: read_u32(bytes, 12)?,
            call_site_index: optional(read_u32(bytes, 16)?),
            first_copy_pair: read_u32(bytes, 20)?,
            copy_pair_count: read_u32(bytes, 24)?,
            result_initialization_slot: optional(read_u32(bytes, 28)?),
        })
    }

    pub fn copy_pair(&self, index: usize) -> Option<CopyPair> {
        let base = index.checked_mul(8)?;
        let bytes = self.sections[6].bytes.get(base..base + 8)?;
        Some(CopyPair {
            source_slot: read_u32(bytes, 0)?,
            destination_slot: read_u32(bytes, 4)?,
        })
    }

    pub fn clear_word(&self, edge: usize, word: usize) -> Option<u64> {
        if word >= self.bitmap_words_per_pc {
            return None;
        }
        let index = edge
            .checked_mul(self.bitmap_words_per_pc)?
            .checked_add(word)?;
        read_u64(self.sections[7].bytes, index.checked_mul(8)?)
    }
}

pub fn inspect<'a>(
    bytes: &'a [u8],
    authority: SelectionSpec<'_>,
) -> Result<FrameDescriptor<'a>, Error> {
    let descriptor = inspect_structure(bytes)?;
    validate_identity(descriptor.metadata, authority)?;
    validate_source_map(
        descriptor.metadata,
        authority.source_revision,
        &descriptor.sections,
        &descriptor.selector_offsets,
    )?;
    Ok(descriptor)
}

/// Admit the source-independent portable profile of FrameDescriptor layout
/// v2 against one already-admitted 60L materialization row.
pub fn inspect_portable<'a>(bytes: &'a [u8], row: Row<'_>) -> Result<FrameDescriptor<'a>, Error> {
    if row.disposition != PortableDisposition::Resumable {
        return Err(Error::Identity);
    }
    let descriptor = inspect_structure(bytes)?;
    let expected_kind = match row.kind {
        SubjectKind::NamedAdapter => ImplementationKind::NamedAdapter,
        SubjectKind::BlockBody => ImplementationKind::BlockBody,
    };
    let metadata = descriptor.metadata;
    if metadata.lowering_version == 0
        || metadata.implementation_kind != expected_kind
        || metadata.portable_implementation_identity
            != portable_resumable_manifest::portable_implementation_identity(
                row,
                metadata.lowering_version,
            )
            .map_err(|_| Error::Identity)?
        || metadata.source_map_identity != 0
    {
        return Err(Error::Identity);
    }
    validate_source_map(
        metadata,
        [0; 32],
        &descriptor.sections,
        &descriptor.selector_offsets,
    )?;
    if descriptor.sections[8].count != 1
        || descriptor.sections[9].count != 1
        || descriptor.sections[8].bytes.iter().any(|byte| *byte != 0)
        || descriptor.sections[9].bytes.iter().any(|byte| *byte != 0)
    {
        return Err(Error::SourceMap);
    }
    Ok(descriptor)
}

fn inspect_structure<'a>(bytes: &'a [u8]) -> Result<FrameDescriptor<'a>, Error> {
    if bytes.len() > MAX_DESCRIPTOR_BYTES {
        return Err(Error::TooLarge);
    }
    if bytes.len() < DATA_OFFSET {
        return Err(Error::Truncated);
    }
    if bytes.get(..32) != Some(&DOMAIN) {
        return Err(Error::Domain);
    }
    if read_u32(bytes, 0x20) != Some(2)
        || read_u32(bytes, 0x24) != Some(PREFIX_BYTES as u32)
        || read_u64(bytes, 0x28) != Some(bytes.len() as u64)
        || read_u32(bytes, 0x34) != Some(SECTION_COUNT as u32)
        || read_u32(bytes, 0x38) != Some(DIRECTORY_STRIDE as u32)
        || read_u32(bytes, 0x3c) != Some(0)
    {
        return Err(Error::Prefix);
    }
    if read_u32(bytes, 0x30) != Some(0) {
        return Err(Error::RequiredFlags);
    }
    let empty = Section {
        bytes: &[],
        count: 0,
    };
    let mut sections = [empty; SECTION_COUNT];
    let mut cursor = DATA_OFFSET;
    let mut fixed_records = 0usize;
    for (index, stride) in STRIDES.iter().copied().enumerate() {
        let base = PREFIX_BYTES + index * DIRECTORY_STRIDE;
        let kind = read_u32(bytes, base).ok_or(Error::Truncated)?;
        let actual_stride = read_u32(bytes, base + 4).ok_or(Error::Truncated)? as usize;
        let count = read_u32(bytes, base + 8).ok_or(Error::Truncated)? as usize;
        let reserved = read_u32(bytes, base + 12).ok_or(Error::Truncated)?;
        let offset = usize::try_from(read_u64(bytes, base + 16).ok_or(Error::Truncated)?)
            .map_err(|_| Error::Arithmetic)?;
        let length = usize::try_from(read_u64(bytes, base + 24).ok_or(Error::Truncated)?)
            .map_err(|_| Error::Arithmetic)?;
        if kind != index as u32 + 1 || actual_stride != stride || reserved != 0 {
            return Err(Error::Directory);
        }
        enforce_section_limit(index, count)?;
        if index < 14 {
            fixed_records = fixed_records.checked_add(count).ok_or(Error::Arithmetic)?;
            if fixed_records > 1_048_576 {
                return Err(Error::Limit);
            }
        }
        let expected_length = count.checked_mul(stride).ok_or(Error::Arithmetic)?;
        let expected_offset = align8(cursor)?;
        if length != expected_length || offset != expected_offset {
            return Err(Error::Directory);
        }
        if bytes
            .get(cursor..offset)
            .ok_or(Error::Truncated)?
            .iter()
            .any(|byte| *byte != 0)
        {
            return Err(Error::Padding);
        }
        let end = offset.checked_add(length).ok_or(Error::Arithmetic)?;
        sections[index] = Section {
            bytes: bytes.get(offset..end).ok_or(Error::Truncated)?,
            count,
        };
        cursor = end;
    }
    if cursor != bytes.len() || sections[0].count != 1 {
        return Err(Error::Directory);
    }
    let metadata = decode_metadata(sections[0].bytes)?;
    let words = (metadata.slot_count as usize)
        .checked_add(63)
        .ok_or(Error::Arithmetic)?
        / 64;
    validate_entry(metadata, sections[1])?;
    validate_roots(metadata, sections[3], words)?;
    validate_entry_roots(metadata, sections[1], sections[3], words)?;
    let selector_offsets = validate_control_flow(metadata, &sections, words)?;
    Ok(FrameDescriptor {
        bytes,
        metadata,
        sections,
        bitmap_words_per_pc: words,
        selector_offsets,
    })
}

fn decode_metadata(bytes: &[u8]) -> Result<Metadata, Error> {
    if bytes.len() != 64 || !matches!(read_u32(bytes, 0x3c), Some(0 | 1)) {
        return Err(Error::Metadata);
    }
    let implementation_kind = match read_u32(bytes, 0x14) {
        Some(1) => ImplementationKind::NamedAdapter,
        Some(2) => ImplementationKind::BlockBody,
        _ => return Err(Error::Metadata),
    };
    Ok(Metadata {
        portable_implementation_identity: read_u64(bytes, 0).ok_or(Error::Metadata)?,
        source_map_identity: read_u64(bytes, 8).ok_or(Error::Metadata)?,
        lowering_version: read_u32(bytes, 0x10).ok_or(Error::Metadata)?,
        implementation_kind,
        slot_count: read_u32(bytes, 0x18).ok_or(Error::Metadata)?,
        program_counter_count: read_u32(bytes, 0x1c).ok_or(Error::Metadata)?,
        driver_argument_count: read_u32(bytes, 0x20).ok_or(Error::Metadata)?,
        immediate_program_counter: read_u32(bytes, 0x3c) == Some(1),
        entry_receiver_slot: read_u32(bytes, 0x24).ok_or(Error::Metadata)?,
        entry_argument_start: read_u32(bytes, 0x28).ok_or(Error::Metadata)?,
        entry_argument_count: read_u32(bytes, 0x2c).ok_or(Error::Metadata)?,
        implementation_slot: optional(read_u32(bytes, 0x30).ok_or(Error::Metadata)?),
        call_window_start: optional(read_u32(bytes, 0x34).ok_or(Error::Metadata)?),
        call_window_argument_capacity: read_u32(bytes, 0x38).ok_or(Error::Metadata)?,
    })
}

fn validate_identity(metadata: Metadata, authority: SelectionSpec<'_>) -> Result<(), Error> {
    if metadata.lowering_version == 0
        || metadata.implementation_kind != authority.implementation_kind
        || portable_implementation_identity(authority, metadata.lowering_version)
            .map_err(|_| Error::Identity)?
            != metadata.portable_implementation_identity
        || source_map_identity(authority, metadata.lowering_version).map_err(|_| Error::Identity)?
            != metadata.source_map_identity
    {
        return Err(Error::Identity);
    }
    Ok(())
}

fn validate_entry(metadata: Metadata, captures: Section<'_>) -> Result<(), Error> {
    let slots = metadata.slot_count;
    if slots == 0 || slots as usize > MAX_FRAME_SLOTS {
        return Err(Error::Limit);
    }
    if metadata.program_counter_count == 0
        || metadata.program_counter_count as usize > MAX_PROGRAM_COUNTERS
        || metadata.driver_argument_count > 1
        || (metadata.immediate_program_counter
            && (metadata.driver_argument_count != 1 || metadata.lowering_version < 2))
        || (metadata.driver_argument_count == 0 && metadata.program_counter_count != 1)
    {
        return Err(Error::Metadata);
    }
    let argument_end = metadata
        .entry_argument_start
        .checked_add(metadata.entry_argument_count)
        .ok_or(Error::Entry)?;
    if metadata.entry_receiver_slot >= slots || argument_end > slots {
        return Err(Error::Entry);
    }
    let mut occupied =
        Vec::with_capacity(captures.count + metadata.entry_argument_count as usize + 2);
    occupied.push(metadata.entry_receiver_slot);
    occupied.extend(metadata.entry_argument_start..argument_end);
    match metadata.implementation_kind {
        ImplementationKind::NamedAdapter => {
            if captures.count != 0 {
                return Err(Error::Entry);
            }
            occupied.push(metadata.implementation_slot.ok_or(Error::Entry)?);
        }
        ImplementationKind::BlockBody => {
            if metadata.implementation_slot.is_some() {
                return Err(Error::Entry);
            }
            for index in 0..captures.count {
                occupied.push(read_u32(captures.bytes, index * 4).ok_or(Error::Entry)?);
            }
        }
    }
    occupied.sort_unstable();
    if occupied.last().is_some_and(|slot| *slot >= slots)
        || occupied.windows(2).any(|pair| pair[0] == pair[1])
    {
        return Err(Error::Entry);
    }
    Ok(())
}

fn validate_roots(metadata: Metadata, roots: Section<'_>, words: usize) -> Result<(), Error> {
    let expected = (metadata.program_counter_count as usize)
        .checked_mul(words)
        .ok_or(Error::Arithmetic)?;
    if roots.count != expected || expected > MAX_BITMAP_WORDS {
        return Err(Error::Roots);
    }
    let remainder = metadata.slot_count as usize % 64;
    if remainder != 0 {
        let allowed = (1u64 << remainder) - 1;
        for pc in 0..metadata.program_counter_count as usize {
            let last = read_u64(roots.bytes, (pc * words + words - 1) * 8).ok_or(Error::Roots)?;
            if last & !allowed != 0 {
                return Err(Error::Roots);
            }
        }
    }
    Ok(())
}

fn validate_entry_roots(
    metadata: Metadata,
    captures: Section<'_>,
    roots: Section<'_>,
    words: usize,
) -> Result<(), Error> {
    let argument_end = metadata.entry_argument_start + metadata.entry_argument_count;
    let mut required = vec![metadata.entry_receiver_slot];
    required.extend(metadata.entry_argument_start..argument_end);
    if let Some(implementation) = metadata.implementation_slot {
        required.push(implementation);
    }
    for index in 0..captures.count {
        required.push(read_u32(captures.bytes, index * 4).ok_or(Error::Entry)?);
    }
    if required
        .into_iter()
        .any(|slot| !rooted(roots.bytes, words, 0, slot))
    {
        return Err(Error::Entry);
    }
    Ok(())
}

fn validate_control_flow(
    metadata: Metadata,
    sections: &[Section<'_>; SECTION_COUNT],
    words: usize,
) -> Result<Vec<(usize, usize)>, Error> {
    let pcs = sections[2];
    let calls = sections[4];
    let edges = sections[5];
    let copies = sections[6];
    let clears = sections[7];
    if pcs.count != metadata.program_counter_count as usize
        || clears.count != edges.count.checked_mul(words).ok_or(Error::Arithmetic)?
    {
        return Err(Error::ProgramCounters);
    }

    let mut location_cursor = 0usize;
    let mut decoded_pcs = Vec::with_capacity(pcs.count);
    for pc in 0..pcs.count {
        let record = record(pcs, pc, 32).ok_or(Error::ProgramCounters)?;
        if read_u32(record, 4) != Some(0)
            || read_u32(record, 24) != Some(0)
            || read_u32(record, 28) != Some(0)
        {
            return Err(Error::ProgramCounters);
        }
        let kind = TerminatorKind::decode(read_u32(record, 0).ok_or(Error::ProgramCounters)?)?;
        let payload = read_u32(record, 8).ok_or(Error::ProgramCounters)?;
        let span = read_u32(record, 12).ok_or(Error::ProgramCounters)? as usize;
        let first = read_u32(record, 16).ok_or(Error::ProgramCounters)? as usize;
        let count = read_u32(record, 20).ok_or(Error::ProgramCounters)? as usize;
        if span >= sections[9].count
            || first != location_cursor
            || first.checked_add(count).ok_or(Error::Arithmetic)? > sections[13].count
        {
            return Err(Error::ProgramCounters);
        }
        location_cursor += count;
        decoded_pcs.push((kind, payload));
    }
    if location_cursor != sections[13].count {
        return Err(Error::ProgramCounters);
    }

    let mut selector_cursor = 0usize;
    let mut selector_offsets = Vec::with_capacity(calls.count);
    let mut call_owners = vec![false; pcs.count];
    let mut previous_pc = None;
    let mut decoded_calls = Vec::with_capacity(calls.count);
    let mut maximum_arity = 0;
    for index in 0..calls.count {
        let call = record(calls, index, 48).ok_or(Error::Calls)?;
        let kind = match read_u32(call, 0) {
            Some(1) => CallKind::Call,
            Some(2) => CallKind::TailCall,
            _ => return Err(Error::Calls),
        };
        let source_pc = read_u32(call, 8).ok_or(Error::Calls)?;
        let receiver = read_u32(call, 12).ok_or(Error::Calls)?;
        let selector_slot = read_u32(call, 16).ok_or(Error::Calls)?;
        let argument_start = read_u32(call, 20).ok_or(Error::Calls)?;
        let arity = read_u32(call, 24).ok_or(Error::Calls)?;
        let result = read_u32(call, 28).ok_or(Error::Calls)?;
        let continuation = read_u32(call, 32).ok_or(Error::Calls)?;
        let edge = read_u32(call, 36).ok_or(Error::Calls)?;
        let span = read_u32(call, 40).ok_or(Error::Calls)?;
        let selector_length = read_u32(call, 44).ok_or(Error::Calls)? as usize;
        let selector_end = selector_cursor
            .checked_add(selector_length)
            .ok_or(Error::Arithmetic)?;
        let selector = sections[14]
            .bytes
            .get(selector_cursor..selector_end)
            .ok_or(Error::Calls)?;
        let window = metadata.call_window_start.ok_or(Error::Calls)?;
        if read_u32(call, 4) != Some(0)
            || source_pc as usize >= pcs.count
            || previous_pc.is_some_and(|previous| previous >= source_pc)
            || span as usize >= sections[9].count
            || selector.is_empty()
            || selector.len() > MAX_SELECTOR_BYTES
            || selector_arity(selector) != arity
            || receiver != window
            || selector_slot != window.checked_add(1).ok_or(Error::Calls)?
            || argument_start != window.checked_add(2).ok_or(Error::Calls)?
            || argument_start.checked_add(arity).ok_or(Error::Calls)? > metadata.slot_count
            || read_u32(record(pcs, source_pc as usize, 32).ok_or(Error::Calls)?, 12) != Some(span)
            || !rooted(sections[3].bytes, words, source_pc as usize, receiver)
            || !rooted(sections[3].bytes, words, source_pc as usize, selector_slot)
            || (argument_start..argument_start + arity)
                .any(|slot| !rooted(sections[3].bytes, words, source_pc as usize, slot))
        {
            return Err(Error::Calls);
        }
        match kind {
            CallKind::Call => {
                if result >= metadata.slot_count
                    || (argument_start..argument_start + arity).contains(&result)
                    || continuation as usize >= pcs.count
                    || edge as usize >= edges.count
                {
                    return Err(Error::Calls);
                }
            }
            CallKind::TailCall => {
                if result != ABSENT || continuation != ABSENT || edge != ABSENT {
                    return Err(Error::Calls);
                }
            }
        }
        if !matches!(
            (decoded_pcs[source_pc as usize].0, kind),
            (TerminatorKind::Call, CallKind::Call) | (TerminatorKind::TailCall, CallKind::TailCall)
        ) || decoded_pcs[source_pc as usize].1 != index as u32
        {
            return Err(Error::Calls);
        }
        selector_offsets.push((selector_cursor, selector_length));
        if std::mem::replace(&mut call_owners[source_pc as usize], true) {
            return Err(Error::Calls);
        }
        selector_cursor = selector_end;
        previous_pc = Some(source_pc);
        maximum_arity = maximum_arity.max(arity);
        decoded_calls.push((kind, result, continuation, edge));
    }
    if calls.count == 0 {
        if metadata.call_window_start.is_some() || metadata.call_window_argument_capacity != 0 {
            return Err(Error::Calls);
        }
    } else {
        let window_end = metadata
            .call_window_start
            .and_then(|window| window.checked_add(2))
            .and_then(|start| start.checked_add(maximum_arity))
            .ok_or(Error::Calls)?;
        if maximum_arity != metadata.call_window_argument_capacity
            || window_end > metadata.slot_count
        {
            return Err(Error::Calls);
        }
    }
    for (pc, (kind, payload)) in decoded_pcs.iter().copied().enumerate() {
        match kind {
            TerminatorKind::Call | TerminatorKind::TailCall if !call_owners[pc] => {
                return Err(Error::Calls);
            }
            TerminatorKind::Return | TerminatorKind::Abort
                if payload >= metadata.slot_count
                    || !rooted(sections[3].bytes, words, pc, payload) =>
            {
                return Err(Error::ProgramCounters);
            }
            _ => {}
        }
    }

    let mut decoded_edges = Vec::with_capacity(edges.count);
    let mut previous_order = None;
    let mut copy_cursor = 0usize;
    for index in 0..edges.count {
        let edge = record(edges, index, 32).ok_or(Error::Edges)?;
        let kind = read_u32(edge, 0).ok_or(Error::Edges)?;
        let source = read_u32(edge, 8).ok_or(Error::Edges)?;
        let target = read_u32(edge, 12).ok_or(Error::Edges)?;
        let call = read_u32(edge, 16).ok_or(Error::Edges)?;
        let first_copy = read_u32(edge, 20).ok_or(Error::Edges)? as usize;
        let copy_count = read_u32(edge, 24).ok_or(Error::Edges)? as usize;
        let result = read_u32(edge, 28).ok_or(Error::Edges)?;
        if read_u32(edge, 4) != Some(0)
            || !matches!(kind, 1 | 2)
            || source as usize >= pcs.count
            || target as usize >= pcs.count
            || previous_order.is_some_and(|previous| previous >= (source, target))
            || first_copy != copy_cursor
            || copy_count > MAX_COPY_PAIRS_PER_EDGE
            || first_copy
                .checked_add(copy_count)
                .ok_or(Error::Arithmetic)?
                > copies.count
        {
            return Err(Error::Edges);
        }
        match kind {
            1 if call == ABSENT && result == ABSENT => {}
            2 if (call as usize) < calls.count && result < metadata.slot_count => {}
            _ => return Err(Error::Edges),
        }
        copy_cursor += copy_count;
        previous_order = Some((source, target));
        decoded_edges.push((kind, source, target, call, result));
    }
    if copy_cursor != copies.count {
        return Err(Error::CopyPairs);
    }

    for (pc, &(kind, payload)) in decoded_pcs.iter().enumerate() {
        let owned: Vec<_> = decoded_edges
            .iter()
            .enumerate()
            .filter(|(_, edge)| edge.1 as usize == pc)
            .collect();
        match kind {
            TerminatorKind::Continue => {
                if owned.is_empty()
                    || payload as usize != owned[0].0
                    || owned.iter().any(|(_, edge)| edge.0 != 1)
                {
                    return Err(Error::Edges);
                }
            }
            TerminatorKind::Call => {
                if owned.len() != 1 {
                    return Err(Error::Edges);
                }
                let (edge_index, edge) = owned[0];
                let call = decoded_calls.get(payload as usize).ok_or(Error::Edges)?;
                if edge.0 != 2
                    || edge.3 != payload
                    || call.3 as usize != edge_index
                    || call.2 != edge.2
                    || call.1 != edge.4
                {
                    return Err(Error::Edges);
                }
            }
            _ if !owned.is_empty() => return Err(Error::Edges),
            _ => {}
        }
    }

    for edge_index in 0..edges.count {
        let edge = record(edges, edge_index, 32).ok_or(Error::Edges)?;
        let first = read_u32(edge, 20).ok_or(Error::Edges)? as usize;
        let count = read_u32(edge, 24).ok_or(Error::Edges)? as usize;
        let result = read_u32(edge, 28).ok_or(Error::Edges)?;
        let mut previous = None;
        let mut destinations = Vec::with_capacity(count);
        for copy_index in first..first + count {
            let pair = record(copies, copy_index, 8).ok_or(Error::CopyPairs)?;
            let source = read_u32(pair, 0).ok_or(Error::CopyPairs)?;
            let destination = read_u32(pair, 4).ok_or(Error::CopyPairs)?;
            if source >= metadata.slot_count
                || destination >= metadata.slot_count
                || source == destination
                || previous.is_some_and(|order| order >= (destination, source))
                || destinations.contains(&destination)
            {
                return Err(Error::CopyPairs);
            }
            previous = Some((destination, source));
            destinations.push(destination);
        }
        for word in 0..words {
            let clear =
                read_u64(clears.bytes, (edge_index * words + word) * 8).ok_or(Error::ClearMaps)?;
            if word + 1 == words && metadata.slot_count % 64 != 0 {
                let allowed = (1u64 << (metadata.slot_count % 64)) - 1;
                if clear & !allowed != 0 {
                    return Err(Error::ClearMaps);
                }
            }
            if destinations
                .iter()
                .any(|slot| *slot as usize / 64 == word && clear & (1 << (*slot % 64)) != 0)
                || (result != ABSENT
                    && result as usize / 64 == word
                    && clear & (1 << (result % 64)) != 0)
            {
                return Err(Error::ClearMaps);
            }
        }
    }
    Ok(selector_offsets)
}

fn selector_arity(selector: &[u8]) -> u32 {
    let colons = selector.iter().filter(|byte| **byte == b':').count();
    if colons != 0 {
        colons as u32
    } else if selector
        .first()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
    {
        0
    } else {
        1
    }
}

fn record(section: Section<'_>, index: usize, stride: usize) -> Option<&[u8]> {
    let start = index.checked_mul(stride)?;
    section.bytes.get(start..start + stride)
}

fn rooted(root_words: &[u8], words_per_pc: usize, pc: usize, slot: u32) -> bool {
    pc.checked_mul(words_per_pc)
        .and_then(|base| base.checked_add(slot as usize / 64))
        .and_then(|word| read_u64(root_words, word * 8))
        .is_some_and(|word| word & (1 << (slot % 64)) != 0)
}

fn validate_source_map(
    metadata: Metadata,
    source_revision: [u8; 32],
    sections: &[Section<'_>; SECTION_COUNT],
    selector_offsets: &[(usize, usize)],
) -> Result<(), Error> {
    let files = sections[8];
    let spans = sections[9];
    let bindings = sections[10];
    let values = sections[11];
    let uses = sections[12];
    let locations = sections[13];
    let source_bytes = sections[14].bytes;
    if files.count == 0 {
        return Err(Error::SourceMap);
    }
    let mut byte_cursor = selector_offsets
        .last()
        .map(|(start, length)| start + length)
        .unwrap_or(0);
    let mut file_lengths = Vec::with_capacity(files.count);
    let mut previous_path: Option<&[u8]> = None;
    for index in 0..files.count {
        let file = record(files, index, 48).ok_or(Error::SourceMap)?;
        let path_length = read_u32(file, 40).ok_or(Error::SourceMap)? as usize;
        let path_end = byte_cursor
            .checked_add(path_length)
            .ok_or(Error::Arithmetic)?;
        let path = source_bytes
            .get(byte_cursor..path_end)
            .ok_or(Error::SourceMap)?;
        if file.get(..32) != Some(&source_revision)
            || read_u32(file, 44) != Some(0)
            || previous_path.is_some_and(|previous| previous >= path)
        {
            return Err(Error::SourceMap);
        }
        file_lengths.push(read_u64(file, 32).ok_or(Error::SourceMap)?);
        previous_path = Some(path);
        byte_cursor = path_end;
    }

    let mut previous_span = None;
    for index in 0..spans.count {
        let span = record(spans, index, 24).ok_or(Error::SourceMap)?;
        let file = read_u32(span, 0).ok_or(Error::SourceMap)? as usize;
        let start = read_u64(span, 8).ok_or(Error::SourceMap)?;
        let end = read_u64(span, 16).ok_or(Error::SourceMap)?;
        if read_u32(span, 4) != Some(0)
            || file >= files.count
            || start > end
            || end > file_lengths[file]
            || previous_span.is_some_and(|previous| previous >= (file, start, end))
        {
            return Err(Error::SourceMap);
        }
        previous_span = Some((file, start, end));
    }

    let mut binding_identities = Vec::with_capacity(bindings.count);
    for index in 0..bindings.count {
        let binding = record(bindings, index, 32).ok_or(Error::SourceMap)?;
        let identity = read_u64(binding, 0).ok_or(Error::SourceMap)?;
        let kind = read_u32(binding, 8).ok_or(Error::SourceMap)?;
        let name_length = read_u32(binding, 12).ok_or(Error::SourceMap)? as usize;
        let declaration = read_u32(binding, 16).ok_or(Error::SourceMap)?;
        let scope = read_u32(binding, 20).ok_or(Error::SourceMap)?;
        let parent = read_u32(binding, 24).ok_or(Error::SourceMap)?;
        let name_end = byte_cursor
            .checked_add(name_length)
            .ok_or(Error::Arithmetic)?;
        if !matches!(kind, 1..=5)
            || declaration as usize >= spans.count
            || scope as usize >= spans.count
            || (parent != ABSENT && parent as usize >= index)
            || read_u32(binding, 28) != Some(0)
            || binding_identities.contains(&identity)
            || source_bytes.get(byte_cursor..name_end).is_none()
        {
            return Err(Error::SourceMap);
        }
        binding_identities.push(identity);
        byte_cursor = name_end;
    }
    if byte_cursor != source_bytes.len() {
        return Err(Error::SourceMap);
    }

    let mut use_cursor = 0usize;
    for index in 0..values.count {
        let value = record(values, index, 32).ok_or(Error::SourceMap)?;
        let binding = read_u32(value, 8).ok_or(Error::SourceMap)?;
        let definition = read_u32(value, 12).ok_or(Error::SourceMap)?;
        let first = read_u32(value, 16).ok_or(Error::SourceMap)? as usize;
        let count = read_u32(value, 20).ok_or(Error::SourceMap)? as usize;
        let flags = read_u32(value, 24).ok_or(Error::SourceMap)?;
        if read_u32(value, 0) != Some(index as u32)
            || !matches!(read_u32(value, 4), Some(1..=9))
            || (binding != ABSENT && binding as usize >= bindings.count)
            || (definition != ABSENT && definition as usize >= spans.count)
            || first != use_cursor
            || first.checked_add(count).ok_or(Error::Arithmetic)? > uses.count
            || flags & !1 != 0
            || read_u32(value, 28) != Some(0)
        {
            return Err(Error::SourceMap);
        }
        let mut previous = None;
        for use_index in first..first + count {
            let logical_use = record(uses, use_index, 16).ok_or(Error::SourceMap)?;
            let tuple = (
                read_u32(logical_use, 4).ok_or(Error::SourceMap)?,
                read_u32(logical_use, 8).ok_or(Error::SourceMap)?,
                read_u32(logical_use, 12).ok_or(Error::SourceMap)?,
            );
            if read_u32(logical_use, 0) != Some(index as u32)
                || tuple.0 >= metadata.program_counter_count
                || tuple.1 as usize >= spans.count
                || !matches!(tuple.2, 1..=5)
                || previous.is_some_and(|prior| prior >= tuple)
            {
                return Err(Error::SourceMap);
            }
            previous = Some(tuple);
        }
        use_cursor += count;
    }
    if use_cursor != uses.count {
        return Err(Error::SourceMap);
    }

    let words = (metadata.slot_count as usize + 63) / 64;
    let roots = sections[3].bytes;
    let mut previous_location = None;
    let mut slots_by_pc = vec![Vec::new(); metadata.program_counter_count as usize];
    for index in 0..locations.count {
        let location_record = record(locations, index, 16).ok_or(Error::SourceMap)?;
        let pc = read_u32(location_record, 0).ok_or(Error::SourceMap)?;
        let value = read_u32(location_record, 4).ok_or(Error::SourceMap)?;
        let kind = read_u32(location_record, 8).ok_or(Error::SourceMap)?;
        let location = read_u32(location_record, 12).ok_or(Error::SourceMap)?;
        if pc >= metadata.program_counter_count
            || value as usize >= values.count
            || !matches!(kind, 1 | 2)
            || previous_location.is_some_and(|previous| previous >= (pc, value))
        {
            return Err(Error::SourceMap);
        }
        if kind == 1 {
            let root_word = (pc as usize)
                .checked_mul(words)
                .and_then(|base| base.checked_add(location as usize / 64))
                .and_then(|word| read_u64(roots, word * 8))
                .ok_or(Error::SourceMap)?;
            if location >= metadata.slot_count
                || root_word & (1 << (location % 64)) == 0
                || slots_by_pc[pc as usize].contains(&location)
            {
                return Err(Error::SourceMap);
            }
            slots_by_pc[pc as usize].push(location);
        } else if location != ABSENT {
            return Err(Error::SourceMap);
        }
        previous_location = Some((pc, value));
    }
    Ok(())
}

fn enforce_section_limit(index: usize, count: usize) -> Result<(), Error> {
    let limit = match index {
        0 => 1,
        1 => MAX_FRAME_SLOTS,
        2 => MAX_PROGRAM_COUNTERS,
        3 | 7 => MAX_BITMAP_WORDS,
        4 => MAX_CALL_SITES,
        5 => MAX_EDGES,
        6 => MAX_COPY_PAIRS,
        8..=10 | 12..=13 => MAX_SOURCE_RECORDS,
        11 => MAX_LOGICAL_VALUES,
        14 => MAX_DESCRIPTOR_BYTES,
        _ => unreachable!(),
    };
    (count <= limit).then_some(()).ok_or(Error::Limit)
}

fn optional(value: u32) -> Option<u32> {
    (value != ABSENT).then_some(value)
}

fn align8(value: usize) -> Result<usize, Error> {
    value
        .checked_add(7)
        .map(|value| value & !7)
        .ok_or(Error::Arithmetic)
}

use super::wire::{push_u32, push_u64, read_u32, read_u64};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stackless_selection::{Disposition, RootKind};

    fn authority(revision: [u8; 32], kind: ImplementationKind, path: &[u32]) -> SelectionSpec<'_> {
        SelectionSpec {
            source_revision: revision,
            root_kind: RootKind::MethodOperation,
            root_index: 0,
            path,
            implementation_kind: kind,
            disposition: Disposition::ResumableRequired,
        }
    }

    fn bytes(name: &str) -> Vec<u8> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/stackless-s2")
                .join(name),
        )
        .unwrap()
    }

    fn revision(bytes: &[u8]) -> [u8; 32] {
        let directory = PREFIX_BYTES + 8 * DIRECTORY_STRIDE;
        let offset = read_u64(bytes, directory + 16).unwrap() as usize;
        bytes[offset..offset + 32].try_into().unwrap()
    }

    #[test]
    fn all_frozen_execution_shapes_admit() {
        let cases: [(&str, ImplementationKind, &[u32]); 9] = [
            (
                "frame-descriptor-minimal-return.bin",
                ImplementationKind::NamedAdapter,
                &[],
            ),
            (
                "frame-descriptor-source-map-block.bin",
                ImplementationKind::BlockBody,
                &[2, 1],
            ),
            (
                "frame-descriptor-call.bin",
                ImplementationKind::NamedAdapter,
                &[],
            ),
            (
                "frame-descriptor-tail-call.bin",
                ImplementationKind::NamedAdapter,
                &[],
            ),
            (
                "frame-descriptor-branch-join.bin",
                ImplementationKind::NamedAdapter,
                &[],
            ),
            (
                "frame-descriptor-cyclic-copy.bin",
                ImplementationKind::NamedAdapter,
                &[],
            ),
            (
                "frame-descriptor-colored-reuse.bin",
                ImplementationKind::NamedAdapter,
                &[],
            ),
            (
                "frame-descriptor-construction-trace-named.bin",
                ImplementationKind::NamedAdapter,
                &[],
            ),
            (
                "frame-descriptor-construction-trace-block.bin",
                ImplementationKind::BlockBody,
                &[0],
            ),
        ];
        for (name, kind, path) in cases {
            let bytes = bytes(name);
            let descriptor = inspect(&bytes, authority(revision(&bytes), kind, path))
                .unwrap_or_else(|error| panic!("{name}: {error:?}"));
            assert_eq!(descriptor.bytes(), bytes);
            assert_eq!(
                descriptor.program_counter_count(),
                descriptor.metadata().program_counter_count as usize
            );
            let sections = Sections {
                metadata: descriptor.sections[0].bytes,
                entry_capture_slots: descriptor.sections[1].bytes,
                program_counters: descriptor.sections[2].bytes,
                root_bitmap_words: descriptor.sections[3].bytes,
                call_sites: descriptor.sections[4].bytes,
                edges: descriptor.sections[5].bytes,
                copy_pairs: descriptor.sections[6].bytes,
                clear_bitmap_words: descriptor.sections[7].bytes,
                source_files: descriptor.sections[8].bytes,
                source_spans: descriptor.sections[9].bytes,
                lexical_bindings: descriptor.sections[10].bytes,
                logical_values: descriptor.sections[11].bytes,
                logical_uses: descriptor.sections[12].bytes,
                pc_value_locations: descriptor.sections[13].bytes,
                source_bytes: descriptor.sections[14].bytes,
            };
            assert_eq!(
                materialize(authority(revision(&bytes), kind, path), sections).unwrap(),
                bytes,
                "{name}"
            );
        }
    }

    #[test]
    fn call_and_parallel_copy_records_are_borrowed() {
        let call_bytes = bytes("frame-descriptor-call.bin");
        let call = inspect(
            &call_bytes,
            authority(revision(&call_bytes), ImplementationKind::NamedAdapter, &[]),
        )
        .unwrap();
        assert_eq!(call.call_site(0).unwrap().kind, CallKind::Call);
        assert_eq!(call.call_site(0).unwrap().selector, b"\xff");
        assert_eq!(call.edge(0).unwrap().kind, EdgeKind::Call);

        let cycle_bytes = bytes("frame-descriptor-cyclic-copy.bin");
        let cycle = inspect(
            &cycle_bytes,
            authority(
                revision(&cycle_bytes),
                ImplementationKind::NamedAdapter,
                &[],
            ),
        )
        .unwrap();
        assert_eq!(
            cycle.copy_pair(0),
            Some(CopyPair {
                source_slot: 3,
                destination_slot: 2
            })
        );
        assert_eq!(
            cycle.copy_pair(1),
            Some(CopyPair {
                source_slot: 2,
                destination_slot: 3
            })
        );
    }

    #[test]
    fn all_truncations_and_execution_mutations_fail_closed() {
        let bytes = bytes("frame-descriptor-call.bin");
        let revision = revision(&bytes);
        for length in 0..bytes.len() {
            assert!(
                inspect(
                    &bytes[..length],
                    authority(revision, ImplementationKind::NamedAdapter, &[])
                )
                .is_err(),
                "truncation {length}"
            );
        }
        for offset in [0usize, 0x20, 0x204, 0x2d0] {
            let mut mutated = bytes.clone();
            mutated[offset] ^= 1;
            assert!(
                inspect(
                    &mutated,
                    authority(revision, ImplementationKind::NamedAdapter, &[])
                )
                .is_err(),
                "mutation {offset:#x}"
            );
        }
    }

    #[test]
    fn adversarial_inputs_do_not_panic() {
        let revision = [0; 32];
        for length in 0..10_000 {
            let bytes: Vec<_> = (0..length)
                .map(|index| (index as u8).wrapping_mul(37).wrapping_add(length as u8))
                .collect();
            assert!(
                std::panic::catch_unwind(|| inspect(
                    &bytes,
                    authority(revision, ImplementationKind::NamedAdapter, &[])
                ))
                .is_ok(),
                "length {length}"
            );
        }
    }
}
