//! Canonical S2 ArtifactSelection and GenerationSelection codecs.
//!
//! These codecs own only pointer-free selection provenance. They neither
//! choose policy nor publish code. Callers must compare an admitted artifact
//! selection with the normalized implementation inventory before activation.

use core::cmp::Ordering;
use sha2::{Digest, Sha256};

pub const ARTIFACT_SELECTION_DOMAIN: [u8; 32] = *b"birnam.artifact-selection.v1\0\0\0\0";
pub const GENERATION_SELECTION_DOMAIN: [u8; 32] = *b"birnam.generation-selection.v1\0\0";
pub const PORTABLE_IDENTITY_DOMAIN: &[u8] = b"birnam.portable-implementation.v1\0";
pub const SOURCE_MAP_IDENTITY_DOMAIN: &[u8] = b"birnam.source-map-identity.v1\0";

pub const MAX_CANONICAL_VALUE_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_FIXED_RECORDS: usize = 1_048_576;
pub const MAX_COVERED_REVISIONS: usize = 65_536;
pub const MAX_IMPLEMENTATION_ROWS: usize = 262_144;
pub const MAX_PATH_DEPTH: usize = 4_096;
pub const MAX_PATH_ORDINALS: usize = 4_194_304;

const PREFIX_BYTES: usize = 64;
const DIRECTORY_ENTRY_BYTES: usize = 32;
const SECTION_COUNT: usize = 3;
const DATA_OFFSET: usize = PREFIX_BYTES + SECTION_COUNT * DIRECTORY_ENTRY_BYTES;
const REVISION_BYTES: usize = 32;
const ROW_BYTES: usize = 52;
const ORDINAL_BYTES: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    TooLarge,
    Truncated,
    DomainMismatch,
    PrefixMismatch,
    RequiredFlags,
    DirectoryMismatch,
    NonzeroPadding,
    ArithmeticOverflow,
    LimitExceeded(Quantity),
    RevisionOrder,
    RowField,
    RowShape,
    RowOrder,
    PathStorage,
    ScopeMembership,
    ScopePartition,
    InventoryMismatch,
    DuplicateRequest,
    UnknownRequest,
    InvalidLoweringVersion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quantity {
    CanonicalValueBytes,
    FixedRecords,
    CoveredRevisions,
    ImplementationRows,
    PathDepth,
    PathOrdinals,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionKind {
    Artifact,
    Generation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum RootKind {
    NonMethodProgram = 1,
    MethodOperation = 2,
}

impl RootKind {
    fn decode(value: u32) -> Result<Self, Error> {
        match value {
            1 => Ok(Self::NonMethodProgram),
            2 => Ok(Self::MethodOperation),
            _ => Err(Error::RowField),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ImplementationKind {
    NamedAdapter = 1,
    BlockBody = 2,
}

impl ImplementationKind {
    fn decode(value: u32) -> Result<Self, Error> {
        match value {
            1 => Ok(Self::NamedAdapter),
            2 => Ok(Self::BlockBody),
            _ => Err(Error::RowField),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Disposition {
    Synchronous = 0,
    ResumableRequired = 1,
}

impl Disposition {
    fn decode(value: u32) -> Result<Self, Error> {
        match value {
            0 => Ok(Self::Synchronous),
            1 => Ok(Self::ResumableRequired),
            _ => Err(Error::RowField),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Path<'a> {
    bytes: &'a [u8],
}

impl<'a> Path<'a> {
    pub const fn len(self) -> usize {
        self.bytes.len() / ORDINAL_BYTES
    }

    pub const fn is_empty(self) -> bool {
        self.bytes.is_empty()
    }

    pub fn get(self, index: usize) -> Option<u32> {
        let start = index.checked_mul(ORDINAL_BYTES)?;
        read_u32(self.bytes, start)
    }

    pub fn iter(self) -> PathIter<'a> {
        PathIter {
            bytes: self.bytes,
            position: 0,
        }
    }
}

pub struct PathIter<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl Iterator for PathIter<'_> {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        let value = read_u32(self.bytes, self.position)?;
        self.position += ORDINAL_BYTES;
        Some(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionRow<'a> {
    pub source_revision: [u8; 32],
    pub root_kind: RootKind,
    pub root_index: u32,
    pub path: Path<'a>,
    pub implementation_kind: ImplementationKind,
    pub disposition: Disposition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionSpec<'a> {
    pub source_revision: [u8; 32],
    pub root_kind: RootKind,
    pub root_index: u32,
    pub path: &'a [u32],
    pub implementation_kind: ImplementationKind,
    pub disposition: Disposition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImplementationSpec<'a> {
    pub source_revision: [u8; 32],
    pub root_kind: RootKind,
    pub root_index: u32,
    pub path: &'a [u32],
    pub implementation_kind: ImplementationKind,
}

impl<'a> ImplementationSpec<'a> {
    fn with_disposition(self, disposition: Disposition) -> SelectionSpec<'a> {
        SelectionSpec {
            source_revision: self.source_revision,
            root_kind: self.root_kind,
            root_index: self.root_index,
            path: self.path,
            implementation_kind: self.implementation_kind,
            disposition,
        }
    }

    fn validate(self) -> Result<(), Error> {
        self.with_disposition(Disposition::Synchronous).validate()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionRequest<'a> {
    pub implementation: ImplementationSpec<'a>,
    pub disposition: Disposition,
}

impl SelectionSpec<'_> {
    fn validate(self) -> Result<(), Error> {
        if self.path.len() > MAX_PATH_DEPTH {
            return Err(Error::LimitExceeded(Quantity::PathDepth));
        }
        if self.root_kind == RootKind::NonMethodProgram && self.root_index != 0 {
            return Err(Error::RowField);
        }
        match self.implementation_kind {
            ImplementationKind::NamedAdapter
                if self.root_kind == RootKind::MethodOperation && self.path.is_empty() => {}
            ImplementationKind::BlockBody if !self.path.is_empty() => {}
            _ => return Err(Error::RowShape),
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection<'a> {
    kind: SelectionKind,
    bytes: &'a [u8],
    revisions: &'a [u8],
    rows: &'a [u8],
    ordinals: &'a [u8],
}

impl<'a> Selection<'a> {
    pub const fn kind(self) -> SelectionKind {
        self.kind
    }

    pub const fn canonical_bytes(self) -> &'a [u8] {
        self.bytes
    }

    pub const fn revision_count(self) -> usize {
        self.revisions.len() / REVISION_BYTES
    }

    pub const fn row_count(self) -> usize {
        self.rows.len() / ROW_BYTES
    }

    pub const fn path_ordinal_count(self) -> usize {
        self.ordinals.len() / ORDINAL_BYTES
    }

    pub fn revision(self, index: usize) -> Option<[u8; 32]> {
        let start = index.checked_mul(REVISION_BYTES)?;
        self.revisions
            .get(start..start + REVISION_BYTES)?
            .try_into()
            .ok()
    }

    pub fn rows(self) -> SelectionRows<'a> {
        SelectionRows {
            rows: self.rows,
            ordinals: self.ordinals,
            row_index: 0,
            path_cursor: 0,
        }
    }

    pub fn require_inventory(self, expected: &[SelectionSpec<'_>]) -> Result<Self, Error> {
        if expected.len() != self.row_count() {
            return Err(Error::InventoryMismatch);
        }
        for (actual, expected) in self.rows().zip(expected) {
            let actual = actual?;
            if actual.source_revision != expected.source_revision
                || actual.root_kind != expected.root_kind
                || actual.root_index != expected.root_index
                || actual.implementation_kind != expected.implementation_kind
                || actual.disposition != expected.disposition
                || !actual.path.iter().eq(expected.path.iter().copied())
            {
                return Err(Error::InventoryMismatch);
            }
        }
        Ok(self)
    }
}

pub struct SelectionRows<'a> {
    rows: &'a [u8],
    ordinals: &'a [u8],
    row_index: usize,
    path_cursor: usize,
}

impl<'a> Iterator for SelectionRows<'a> {
    type Item = Result<SelectionRow<'a>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.row_index >= self.rows.len() / ROW_BYTES {
            return None;
        }
        let base = self.row_index * ROW_BYTES;
        self.row_index += 1;
        let path_count = read_u32(self.rows, base + 0x28)? as usize;
        let path_bytes = path_count.checked_mul(ORDINAL_BYTES)?;
        let path_end = self.path_cursor.checked_add(path_bytes)?;
        let path = self.ordinals.get(self.path_cursor..path_end)?;
        self.path_cursor = path_end;
        Some(decode_row(&self.rows[base..base + ROW_BYTES], path))
    }
}

pub fn inspect_artifact_selection(bytes: &[u8]) -> Result<Selection<'_>, Error> {
    inspect_selection(bytes, SelectionKind::Artifact)
}

pub fn inspect_generation_selection(bytes: &[u8]) -> Result<Selection<'_>, Error> {
    inspect_selection(bytes, SelectionKind::Generation)
}

fn inspect_selection(bytes: &[u8], kind: SelectionKind) -> Result<Selection<'_>, Error> {
    if bytes.len() > MAX_CANONICAL_VALUE_BYTES {
        return Err(Error::LimitExceeded(Quantity::CanonicalValueBytes));
    }
    if bytes.len() < DATA_OFFSET {
        return Err(Error::Truncated);
    }
    let domain = match kind {
        SelectionKind::Artifact => &ARTIFACT_SELECTION_DOMAIN,
        SelectionKind::Generation => &GENERATION_SELECTION_DOMAIN,
    };
    if bytes.get(..32) != Some(domain) {
        return Err(Error::DomainMismatch);
    }
    if read_u32(bytes, 0x20) != Some(1)
        || read_u32(bytes, 0x24) != Some(PREFIX_BYTES as u32)
        || read_u64(bytes, 0x28) != Some(bytes.len() as u64)
        || read_u32(bytes, 0x34) != Some(SECTION_COUNT as u32)
        || read_u32(bytes, 0x38) != Some(DIRECTORY_ENTRY_BYTES as u32)
        || read_u32(bytes, 0x3c) != Some(0)
    {
        return Err(Error::PrefixMismatch);
    }
    if read_u32(bytes, 0x30) != Some(0) {
        return Err(Error::RequiredFlags);
    }

    let requirements = [(1u32, 32usize), (2, 52), (3, 4)];
    let limits = [
        (Quantity::CoveredRevisions, MAX_COVERED_REVISIONS),
        (Quantity::ImplementationRows, MAX_IMPLEMENTATION_ROWS),
        (Quantity::PathOrdinals, MAX_PATH_ORDINALS),
    ];
    let mut cursor = DATA_OFFSET;
    let mut fixed_records = 0usize;
    let mut sections = [(0usize, 0usize); SECTION_COUNT];
    for index in 0..SECTION_COUNT {
        let base = PREFIX_BYTES + index * DIRECTORY_ENTRY_BYTES;
        let (required_kind, required_stride) = requirements[index];
        let kind = read_u32(bytes, base).ok_or(Error::Truncated)?;
        let stride = read_u32(bytes, base + 4).ok_or(Error::Truncated)? as usize;
        let count = read_u32(bytes, base + 8).ok_or(Error::Truncated)? as usize;
        let reserved = read_u32(bytes, base + 12).ok_or(Error::Truncated)?;
        let offset = usize::try_from(read_u64(bytes, base + 16).ok_or(Error::Truncated)?)
            .map_err(|_| Error::ArithmeticOverflow)?;
        let length = usize::try_from(read_u64(bytes, base + 24).ok_or(Error::Truncated)?)
            .map_err(|_| Error::ArithmeticOverflow)?;
        if kind != required_kind || stride != required_stride || reserved != 0 {
            return Err(Error::DirectoryMismatch);
        }
        if count > limits[index].1 {
            return Err(Error::LimitExceeded(limits[index].0));
        }
        fixed_records = fixed_records
            .checked_add(count)
            .ok_or(Error::ArithmeticOverflow)?;
        if fixed_records > MAX_FIXED_RECORDS {
            return Err(Error::LimitExceeded(Quantity::FixedRecords));
        }
        let expected_length = count.checked_mul(stride).ok_or(Error::ArithmeticOverflow)?;
        let expected_offset = align_up_8(cursor)?;
        if length != expected_length || offset != expected_offset {
            return Err(Error::DirectoryMismatch);
        }
        if bytes
            .get(cursor..expected_offset)
            .ok_or(Error::Truncated)?
            .iter()
            .any(|byte| *byte != 0)
        {
            return Err(Error::NonzeroPadding);
        }
        let end = offset
            .checked_add(length)
            .ok_or(Error::ArithmeticOverflow)?;
        if end > bytes.len() {
            return Err(Error::Truncated);
        }
        sections[index] = (offset, length);
        cursor = end;
    }
    if cursor != bytes.len() {
        return Err(Error::DirectoryMismatch);
    }

    let revisions = &bytes[sections[0].0..sections[0].0 + sections[0].1];
    let rows = &bytes[sections[1].0..sections[1].0 + sections[1].1];
    let ordinals = &bytes[sections[2].0..sections[2].0 + sections[2].1];
    validate_revisions(revisions)?;
    validate_rows(revisions, rows, ordinals)?;
    Ok(Selection {
        kind,
        bytes,
        revisions,
        rows,
        ordinals,
    })
}

fn validate_revisions(revisions: &[u8]) -> Result<(), Error> {
    let mut previous: Option<&[u8]> = None;
    for revision in revisions.chunks_exact(REVISION_BYTES) {
        if previous.is_some_and(|value| value >= revision) {
            return Err(Error::RevisionOrder);
        }
        previous = Some(revision);
    }
    Ok(())
}

fn validate_rows(revisions: &[u8], rows: &[u8], ordinals: &[u8]) -> Result<(), Error> {
    let mut path_cursor = 0usize;
    let mut previous = None;
    for bytes in rows.chunks_exact(ROW_BYTES) {
        let path_count = read_u32(bytes, 0x28).ok_or(Error::Truncated)? as usize;
        if path_count > MAX_PATH_DEPTH {
            return Err(Error::LimitExceeded(Quantity::PathDepth));
        }
        let path_bytes = path_count
            .checked_mul(ORDINAL_BYTES)
            .ok_or(Error::ArithmeticOverflow)?;
        let path_end = path_cursor
            .checked_add(path_bytes)
            .ok_or(Error::ArithmeticOverflow)?;
        let path = ordinals
            .get(path_cursor..path_end)
            .ok_or(Error::PathStorage)?;
        path_cursor = path_end;
        let row = decode_row(bytes, path)?;
        if !revision_is_member(revisions, &row.source_revision) {
            return Err(Error::ScopeMembership);
        }
        if previous.is_some_and(|prior| compare_rows(prior, row) != Ordering::Less) {
            return Err(Error::RowOrder);
        }
        previous = Some(row);
    }
    if path_cursor != ordinals.len() {
        return Err(Error::PathStorage);
    }
    if revisions.is_empty() && (!rows.is_empty() || !ordinals.is_empty()) {
        return Err(Error::ScopeMembership);
    }
    Ok(())
}

fn decode_row<'a>(bytes: &[u8], path: &'a [u8]) -> Result<SelectionRow<'a>, Error> {
    let source_revision = bytes
        .get(..32)
        .ok_or(Error::Truncated)?
        .try_into()
        .map_err(|_| Error::Truncated)?;
    let root_kind = RootKind::decode(read_u32(bytes, 0x20).ok_or(Error::Truncated)?)?;
    let root_index = read_u32(bytes, 0x24).ok_or(Error::Truncated)?;
    let implementation_kind =
        ImplementationKind::decode(read_u32(bytes, 0x2c).ok_or(Error::Truncated)?)?;
    let disposition = Disposition::decode(read_u32(bytes, 0x30).ok_or(Error::Truncated)?)?;
    let row = SelectionRow {
        source_revision,
        root_kind,
        root_index,
        path: Path { bytes: path },
        implementation_kind,
        disposition,
    };
    if root_kind == RootKind::NonMethodProgram && root_index != 0 {
        return Err(Error::RowField);
    }
    match implementation_kind {
        ImplementationKind::NamedAdapter
            if root_kind == RootKind::MethodOperation && path.is_empty() => {}
        ImplementationKind::BlockBody if !path.is_empty() => {}
        _ => return Err(Error::RowShape),
    }
    Ok(row)
}

fn compare_rows(left: SelectionRow<'_>, right: SelectionRow<'_>) -> Ordering {
    left.source_revision
        .cmp(&right.source_revision)
        .then(left.root_kind.cmp(&right.root_kind))
        .then(left.root_index.cmp(&right.root_index))
        .then_with(|| compare_paths(left.path.iter(), right.path.iter()))
        .then(left.implementation_kind.cmp(&right.implementation_kind))
}

fn compare_specs(left: &SelectionSpec<'_>, right: &SelectionSpec<'_>) -> Ordering {
    left.source_revision
        .cmp(&right.source_revision)
        .then(left.root_kind.cmp(&right.root_kind))
        .then(left.root_index.cmp(&right.root_index))
        .then_with(|| left.path.cmp(right.path))
        .then(left.implementation_kind.cmp(&right.implementation_kind))
}

fn compare_implementations(
    left: &ImplementationSpec<'_>,
    right: &ImplementationSpec<'_>,
) -> Ordering {
    left.source_revision
        .cmp(&right.source_revision)
        .then(left.root_kind.cmp(&right.root_kind))
        .then(left.root_index.cmp(&right.root_index))
        .then_with(|| left.path.cmp(right.path))
        .then(left.implementation_kind.cmp(&right.implementation_kind))
}

fn compare_paths(
    mut left: impl Iterator<Item = u32>,
    mut right: impl Iterator<Item = u32>,
) -> Ordering {
    loop {
        match (left.next(), right.next()) {
            (Some(left), Some(right)) => match left.cmp(&right) {
                Ordering::Equal => {}
                ordering => return ordering,
            },
            (Some(_), None) => return Ordering::Greater,
            (None, Some(_)) => return Ordering::Less,
            (None, None) => return Ordering::Equal,
        }
    }
}

pub fn encode_artifact_selection(
    revisions: &[[u8; 32]],
    rows: &[SelectionSpec<'_>],
) -> Result<Vec<u8>, Error> {
    encode_selection(SelectionKind::Artifact, revisions, rows)
}

/// Materialize one scope-total ArtifactSelection from a normalized inventory
/// and an order-insensitive sparse policy request. Every omitted inventory row
/// receives the synchronous compatibility disposition.
pub fn materialize_artifact_selection(
    revisions: &[[u8; 32]],
    inventory: &[ImplementationSpec<'_>],
    requests: &[SelectionRequest<'_>],
) -> Result<Vec<u8>, Error> {
    validate_revision_specs(revisions)?;
    if inventory.len() > MAX_IMPLEMENTATION_ROWS {
        return Err(Error::LimitExceeded(Quantity::ImplementationRows));
    }
    let mut previous = None;
    for implementation in inventory {
        implementation.validate()?;
        if revisions
            .binary_search(&implementation.source_revision)
            .is_err()
        {
            return Err(Error::ScopeMembership);
        }
        if previous
            .is_some_and(|prior| compare_implementations(prior, implementation) != Ordering::Less)
        {
            return Err(Error::InventoryMismatch);
        }
        previous = Some(implementation);
    }

    let mut requests: Vec<_> = requests.iter().collect();
    for request in &requests {
        request.implementation.validate()?;
    }
    requests.sort_by(|left, right| {
        compare_implementations(&left.implementation, &right.implementation)
    });
    if requests.windows(2).any(|pair| {
        compare_implementations(&pair[0].implementation, &pair[1].implementation) == Ordering::Equal
    }) {
        return Err(Error::DuplicateRequest);
    }

    let mut rows = Vec::with_capacity(inventory.len());
    let mut request_index = 0usize;
    for implementation in inventory {
        while request_index < requests.len()
            && compare_implementations(&requests[request_index].implementation, implementation)
                == Ordering::Less
        {
            return Err(Error::UnknownRequest);
        }
        let disposition = if request_index < requests.len()
            && compare_implementations(&requests[request_index].implementation, implementation)
                == Ordering::Equal
        {
            let disposition = requests[request_index].disposition;
            request_index += 1;
            disposition
        } else {
            Disposition::Synchronous
        };
        rows.push(implementation.with_disposition(disposition));
    }
    if request_index != requests.len() {
        return Err(Error::UnknownRequest);
    }
    encode_artifact_selection(revisions, &rows)
}

pub fn encode_generation_selection(
    revisions: &[[u8; 32]],
    rows: &[SelectionSpec<'_>],
) -> Result<Vec<u8>, Error> {
    encode_selection(SelectionKind::Generation, revisions, rows)
}

fn encode_selection(
    kind: SelectionKind,
    revisions: &[[u8; 32]],
    rows: &[SelectionSpec<'_>],
) -> Result<Vec<u8>, Error> {
    validate_revision_specs(revisions)?;
    if rows.len() > MAX_IMPLEMENTATION_ROWS {
        return Err(Error::LimitExceeded(Quantity::ImplementationRows));
    }
    let mut path_count = 0usize;
    let mut previous = None;
    for row in rows {
        row.validate()?;
        if revisions.binary_search(&row.source_revision).is_err() {
            return Err(Error::ScopeMembership);
        }
        if previous.is_some_and(|prior| compare_specs(prior, row) != Ordering::Less) {
            return Err(Error::RowOrder);
        }
        path_count = path_count
            .checked_add(row.path.len())
            .ok_or(Error::ArithmeticOverflow)?;
        previous = Some(row);
    }
    if revisions.is_empty() && !rows.is_empty() {
        return Err(Error::ScopeMembership);
    }
    if path_count > MAX_PATH_ORDINALS {
        return Err(Error::LimitExceeded(Quantity::PathOrdinals));
    }
    let fixed_records = revisions
        .len()
        .checked_add(rows.len())
        .and_then(|value| value.checked_add(path_count))
        .ok_or(Error::ArithmeticOverflow)?;
    if fixed_records > MAX_FIXED_RECORDS {
        return Err(Error::LimitExceeded(Quantity::FixedRecords));
    }

    let revision_length = revisions
        .len()
        .checked_mul(REVISION_BYTES)
        .ok_or(Error::ArithmeticOverflow)?;
    let row_length = rows
        .len()
        .checked_mul(ROW_BYTES)
        .ok_or(Error::ArithmeticOverflow)?;
    let ordinal_length = path_count
        .checked_mul(ORDINAL_BYTES)
        .ok_or(Error::ArithmeticOverflow)?;
    let revision_offset = DATA_OFFSET;
    let row_offset = revision_offset
        .checked_add(revision_length)
        .ok_or(Error::ArithmeticOverflow)?;
    let row_end = row_offset
        .checked_add(row_length)
        .ok_or(Error::ArithmeticOverflow)?;
    let ordinal_offset = align_up_8(row_end)?;
    let total = ordinal_offset
        .checked_add(ordinal_length)
        .ok_or(Error::ArithmeticOverflow)?;
    if total > MAX_CANONICAL_VALUE_BYTES {
        return Err(Error::LimitExceeded(Quantity::CanonicalValueBytes));
    }

    let domain = match kind {
        SelectionKind::Artifact => ARTIFACT_SELECTION_DOMAIN,
        SelectionKind::Generation => GENERATION_SELECTION_DOMAIN,
    };
    let mut output = Vec::with_capacity(total);
    output.extend_from_slice(&domain);
    push_u32(&mut output, 1);
    push_u32(&mut output, PREFIX_BYTES as u32);
    push_u64(&mut output, total as u64);
    push_u32(&mut output, 0);
    push_u32(&mut output, SECTION_COUNT as u32);
    push_u32(&mut output, DIRECTORY_ENTRY_BYTES as u32);
    push_u32(&mut output, 0);
    push_directory(
        &mut output,
        1,
        REVISION_BYTES,
        revisions.len(),
        revision_offset,
        revision_length,
    )?;
    push_directory(
        &mut output,
        2,
        ROW_BYTES,
        rows.len(),
        row_offset,
        row_length,
    )?;
    push_directory(
        &mut output,
        3,
        ORDINAL_BYTES,
        path_count,
        ordinal_offset,
        ordinal_length,
    )?;
    for revision in revisions {
        output.extend_from_slice(revision);
    }
    for row in rows {
        output.extend_from_slice(&row.source_revision);
        push_u32(&mut output, row.root_kind as u32);
        push_u32(&mut output, row.root_index);
        push_u32(
            &mut output,
            u32::try_from(row.path.len()).map_err(|_| Error::ArithmeticOverflow)?,
        );
        push_u32(&mut output, row.implementation_kind as u32);
        push_u32(&mut output, row.disposition as u32);
    }
    output.resize(ordinal_offset, 0);
    for row in rows {
        for ordinal in row.path {
            push_u32(&mut output, *ordinal);
        }
    }
    debug_assert_eq!(output.len(), total);
    Ok(output)
}

pub fn compose_generation_selection(
    active_revisions: &[[u8; 32]],
    slices: &[Selection<'_>],
) -> Result<Vec<u8>, Error> {
    validate_revision_specs(active_revisions)?;
    let mut ownership = vec![0u8; active_revisions.len()];
    let mut rows = Vec::new();
    for selection in slices {
        if selection.kind != SelectionKind::Artifact {
            return Err(Error::DomainMismatch);
        }
        for index in 0..selection.revision_count() {
            let revision = selection.revision(index).ok_or(Error::Truncated)?;
            let owner = active_revisions
                .binary_search(&revision)
                .map_err(|_| Error::ScopePartition)?;
            ownership[owner] = ownership[owner]
                .checked_add(1)
                .ok_or(Error::ScopePartition)?;
        }
        for row in selection.rows() {
            let row = row?;
            let path: Vec<u32> = row.path.iter().collect();
            rows.push((row, path));
        }
    }
    if ownership.iter().any(|count| *count != 1) {
        return Err(Error::ScopePartition);
    }
    rows.sort_by(|left, right| compare_rows(left.0, right.0));
    if rows
        .windows(2)
        .any(|pair| compare_rows(pair[0].0, pair[1].0) != Ordering::Less)
    {
        return Err(Error::RowOrder);
    }
    let specs: Vec<_> = rows
        .iter()
        .map(|(row, path)| SelectionSpec {
            source_revision: row.source_revision,
            root_kind: row.root_kind,
            root_index: row.root_index,
            path,
            implementation_kind: row.implementation_kind,
            disposition: row.disposition,
        })
        .collect();
    encode_generation_selection(active_revisions, &specs)
}

/// Replace an exact revision-owned slice in an accepted GenerationSelection.
///
/// Candidate compilation uses this operation to project a successor without
/// re-querying policy: predecessor rows outside `removed_revisions` are copied
/// byte-semantically, while `replacement` supplies the complete new artifact
/// scope. The declared successor revision inventory must equal that exact
/// remove-and-insert result.
pub fn replace_generation_selection(
    predecessor: Selection<'_>,
    removed_revisions: &[[u8; 32]],
    successor_revisions: &[[u8; 32]],
    replacement: Selection<'_>,
) -> Result<Vec<u8>, Error> {
    if predecessor.kind != SelectionKind::Generation || replacement.kind != SelectionKind::Artifact
    {
        return Err(Error::DomainMismatch);
    }
    validate_revision_specs(removed_revisions)?;
    validate_revision_specs(successor_revisions)?;

    let predecessor_revisions = (0..predecessor.revision_count())
        .map(|index| predecessor.revision(index).ok_or(Error::Truncated))
        .collect::<Result<Vec<_>, _>>()?;
    if removed_revisions
        .iter()
        .any(|revision| predecessor_revisions.binary_search(revision).is_err())
    {
        return Err(Error::ScopePartition);
    }
    let replacement_revisions = (0..replacement.revision_count())
        .map(|index| replacement.revision(index).ok_or(Error::Truncated))
        .collect::<Result<Vec<_>, _>>()?;
    let mut projected_revisions = predecessor_revisions
        .iter()
        .copied()
        .filter(|revision| removed_revisions.binary_search(revision).is_err())
        .chain(replacement_revisions.iter().copied())
        .collect::<Vec<_>>();
    projected_revisions.sort_unstable();
    if projected_revisions
        .windows(2)
        .any(|pair| pair[0] == pair[1])
        || projected_revisions != successor_revisions
    {
        return Err(Error::ScopePartition);
    }

    let mut rows = Vec::new();
    for row in predecessor.rows() {
        let row = row?;
        if removed_revisions
            .binary_search(&row.source_revision)
            .is_err()
        {
            rows.push((row, row.path.iter().collect::<Vec<_>>()));
        }
    }
    for row in replacement.rows() {
        let row = row?;
        rows.push((row, row.path.iter().collect::<Vec<_>>()));
    }
    rows.sort_by(|left, right| compare_rows(left.0, right.0));
    if rows
        .windows(2)
        .any(|pair| compare_rows(pair[0].0, pair[1].0) != Ordering::Less)
    {
        return Err(Error::RowOrder);
    }
    let specs = rows
        .iter()
        .map(|(row, path)| SelectionSpec {
            source_revision: row.source_revision,
            root_kind: row.root_kind,
            root_index: row.root_index,
            path,
            implementation_kind: row.implementation_kind,
            disposition: row.disposition,
        })
        .collect::<Vec<_>>();
    encode_generation_selection(successor_revisions, &specs)
}

pub fn portable_implementation_identity(
    row: SelectionSpec<'_>,
    lowering_version: u32,
) -> Result<u64, Error> {
    if lowering_version == 0 {
        return Err(Error::InvalidLoweringVersion);
    }
    row.validate()?;
    let mut digest = Sha256::new();
    digest.update(PORTABLE_IDENTITY_DOMAIN);
    digest.update(row.source_revision);
    digest.update((row.root_kind as u32).to_le_bytes());
    digest.update(row.root_index.to_le_bytes());
    digest.update(
        u32::try_from(row.path.len())
            .map_err(|_| Error::ArithmeticOverflow)?
            .to_le_bytes(),
    );
    digest.update((row.implementation_kind as u32).to_le_bytes());
    for ordinal in row.path {
        digest.update(ordinal.to_le_bytes());
    }
    digest.update(lowering_version.to_le_bytes());
    Ok(u64::from_le_bytes(
        digest.finalize()[..8].try_into().unwrap(),
    ))
}

pub fn source_map_identity(row: SelectionSpec<'_>, lowering_version: u32) -> Result<u64, Error> {
    if lowering_version == 0 {
        return Err(Error::InvalidLoweringVersion);
    }
    row.validate()?;
    let mut digest = Sha256::new();
    digest.update(SOURCE_MAP_IDENTITY_DOMAIN);
    digest.update(row.source_revision);
    digest.update((row.root_kind as u32).to_le_bytes());
    digest.update(row.root_index.to_le_bytes());
    digest.update(
        u32::try_from(row.path.len())
            .map_err(|_| Error::ArithmeticOverflow)?
            .to_le_bytes(),
    );
    digest.update((row.implementation_kind as u32).to_le_bytes());
    for ordinal in row.path {
        digest.update(ordinal.to_le_bytes());
    }
    digest.update(lowering_version.to_le_bytes());
    Ok(u64::from_le_bytes(
        digest.finalize()[..8].try_into().unwrap(),
    ))
}

fn validate_revision_specs(revisions: &[[u8; 32]]) -> Result<(), Error> {
    if revisions.len() > MAX_COVERED_REVISIONS {
        return Err(Error::LimitExceeded(Quantity::CoveredRevisions));
    }
    if revisions.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(Error::RevisionOrder);
    }
    Ok(())
}

fn revision_is_member(revisions: &[u8], revision: &[u8; 32]) -> bool {
    let mut low = 0usize;
    let mut high = revisions.len() / REVISION_BYTES;
    while low < high {
        let middle = low + (high - low) / 2;
        let start = middle * REVISION_BYTES;
        match revisions[start..start + REVISION_BYTES].cmp(revision.as_slice()) {
            Ordering::Less => low = middle + 1,
            Ordering::Greater => high = middle,
            Ordering::Equal => return true,
        }
    }
    false
}

fn push_directory(
    output: &mut Vec<u8>,
    kind: u32,
    stride: usize,
    count: usize,
    offset: usize,
    length: usize,
) -> Result<(), Error> {
    push_u32(output, kind);
    push_u32(
        output,
        u32::try_from(stride).map_err(|_| Error::ArithmeticOverflow)?,
    );
    push_u32(
        output,
        u32::try_from(count).map_err(|_| Error::ArithmeticOverflow)?,
    );
    push_u32(output, 0);
    push_u64(
        output,
        u64::try_from(offset).map_err(|_| Error::ArithmeticOverflow)?,
    );
    push_u64(
        output,
        u64::try_from(length).map_err(|_| Error::ArithmeticOverflow)?,
    );
    Ok(())
}

use super::wire::{push_u32, push_u64, read_u32, read_u64};

fn align_up_8(value: usize) -> Result<usize, Error> {
    value
        .checked_add(7)
        .map(|value| value & !7)
        .ok_or(Error::ArithmeticOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMPTY: &[u8] =
        include_bytes!("../../../tests/fixtures/stackless-s2/artifact-selection-empty.bin");
    const SYNCHRONOUS: &[u8] =
        include_bytes!("../../../tests/fixtures/stackless-s2/artifact-selection-all-synchronous.bin");
    const MIXED: &[u8] =
        include_bytes!("../../../tests/fixtures/stackless-s2/artifact-selection-mixed.bin");
    const B_SYNCHRONOUS: &[u8] =
        include_bytes!("../../../tests/fixtures/stackless-s2/artifact-selection-b-synchronous.bin");
    const GENERATION_AB: &[u8] =
        include_bytes!("../../../tests/fixtures/stackless-s2/generation-selection-ab.bin");
    const BAD_SCOPE: &[u8] =
        include_bytes!("../../../tests/fixtures/stackless-s2/artifact-selection-bad-scope.bin");

    fn specs(selection: Selection<'_>) -> (Vec<[u8; 32]>, Vec<(SelectionRow<'_>, Vec<u32>)>) {
        let revisions = (0..selection.revision_count())
            .map(|index| selection.revision(index).unwrap())
            .collect();
        let rows = selection
            .rows()
            .map(|row| {
                let row = row.unwrap();
                (row, row.path.iter().collect())
            })
            .collect();
        (revisions, rows)
    }

    fn encode_again(selection: Selection<'_>) -> Vec<u8> {
        let (revisions, rows) = specs(selection);
        let specs: Vec<_> = rows
            .iter()
            .map(|(row, path)| SelectionSpec {
                source_revision: row.source_revision,
                root_kind: row.root_kind,
                root_index: row.root_index,
                path,
                implementation_kind: row.implementation_kind,
                disposition: row.disposition,
            })
            .collect();
        encode_artifact_selection(&revisions, &specs).unwrap()
    }

    #[test]
    fn golden_artifact_selections_parse_and_serialize_byte_identically() {
        let empty = inspect_artifact_selection(EMPTY).unwrap();
        assert_eq!(empty.revision_count(), 0);
        assert_eq!(empty.row_count(), 0);
        assert_eq!(encode_again(empty), EMPTY);

        let synchronous = inspect_artifact_selection(SYNCHRONOUS).unwrap();
        assert_eq!(synchronous.revision_count(), 1);
        assert_eq!(synchronous.row_count(), 2);
        assert_eq!(encode_again(synchronous), SYNCHRONOUS);

        let mixed = inspect_artifact_selection(MIXED).unwrap();
        let rows: Vec<_> = mixed.rows().map(Result::unwrap).collect();
        assert_eq!(rows[0].disposition, Disposition::Synchronous);
        assert_eq!(rows[1].disposition, Disposition::ResumableRequired);
        assert_eq!(rows[1].path.iter().collect::<Vec<_>>(), [2, 1]);
        assert_eq!(encode_again(mixed), MIXED);
    }

    #[test]
    fn sparse_requests_materialize_over_total_inventory_with_synchronous_default() {
        let selection = inspect_artifact_selection(SYNCHRONOUS).unwrap();
        let revision = selection.revision(0).unwrap();
        let block_path = [2, 1];
        let inventory = [
            ImplementationSpec {
                source_revision: revision,
                root_kind: RootKind::MethodOperation,
                root_index: 0,
                path: &[],
                implementation_kind: ImplementationKind::NamedAdapter,
            },
            ImplementationSpec {
                source_revision: revision,
                root_kind: RootKind::MethodOperation,
                root_index: 0,
                path: &block_path,
                implementation_kind: ImplementationKind::BlockBody,
            },
        ];
        assert_eq!(
            materialize_artifact_selection(&[revision], &inventory, &[]).unwrap(),
            SYNCHRONOUS
        );

        let block_request = SelectionRequest {
            implementation: inventory[1],
            disposition: Disposition::ResumableRequired,
        };
        assert_eq!(
            materialize_artifact_selection(&[revision], &inventory, &[block_request]).unwrap(),
            MIXED
        );
        let explicit_named_default = SelectionRequest {
            implementation: inventory[0],
            disposition: Disposition::Synchronous,
        };
        assert_eq!(
            materialize_artifact_selection(
                &[revision],
                &inventory,
                &[block_request, explicit_named_default],
            )
            .unwrap(),
            MIXED,
            "request iteration order has no authority"
        );
        assert_eq!(
            materialize_artifact_selection(
                &[revision],
                &inventory,
                &[block_request, block_request],
            ),
            Err(Error::DuplicateRequest)
        );

        let unknown_path = [9];
        let unknown = SelectionRequest {
            implementation: ImplementationSpec {
                source_revision: revision,
                root_kind: RootKind::MethodOperation,
                root_index: 0,
                path: &unknown_path,
                implementation_kind: ImplementationKind::BlockBody,
            },
            disposition: Disposition::ResumableRequired,
        };
        assert_eq!(
            materialize_artifact_selection(&[revision], &inventory, &[unknown]),
            Err(Error::UnknownRequest)
        );
    }

    #[test]
    fn generation_composition_is_partition_and_input_order_independent() {
        let expected = inspect_generation_selection(GENERATION_AB).unwrap();
        let revisions: Vec<_> = (0..expected.revision_count())
            .map(|index| expected.revision(index).unwrap())
            .collect();
        let a = inspect_artifact_selection(MIXED).unwrap();
        let b = inspect_artifact_selection(B_SYNCHRONOUS).unwrap();
        assert_eq!(
            compose_generation_selection(&revisions, &[a, b]).unwrap(),
            GENERATION_AB
        );
        assert_eq!(
            compose_generation_selection(&revisions, &[b, a]).unwrap(),
            GENERATION_AB
        );
        let empty = inspect_artifact_selection(EMPTY).unwrap();
        assert_eq!(
            compose_generation_selection(&revisions, &[empty, b, a]).unwrap(),
            GENERATION_AB
        );
    }

    #[test]
    fn generation_slice_replacement_copies_unowned_rows_and_rejects_stale_scope() {
        let predecessor = inspect_generation_selection(GENERATION_AB).unwrap();
        let revisions = (0..predecessor.revision_count())
            .map(|index| predecessor.revision(index).unwrap())
            .collect::<Vec<_>>();
        let replacement = inspect_artifact_selection(SYNCHRONOUS).unwrap();
        let other = inspect_artifact_selection(B_SYNCHRONOUS).unwrap();
        let expected = compose_generation_selection(&revisions, &[replacement, other]).unwrap();
        assert_eq!(
            replace_generation_selection(
                predecessor,
                &[replacement.revision(0).unwrap()],
                &revisions,
                replacement,
            )
            .unwrap(),
            expected
        );
        assert_eq!(
            replace_generation_selection(predecessor, &[[0xff; 32]], &revisions, replacement,),
            Err(Error::ScopePartition)
        );
    }

    #[test]
    fn malformed_scope_and_partition_disagreement_fail_closed() {
        assert_eq!(
            inspect_artifact_selection(BAD_SCOPE),
            Err(Error::ScopeMembership)
        );
        let generation = inspect_generation_selection(GENERATION_AB).unwrap();
        let revisions: Vec<_> = (0..generation.revision_count())
            .map(|index| generation.revision(index).unwrap())
            .collect();
        let a = inspect_artifact_selection(MIXED).unwrap();
        assert_eq!(
            compose_generation_selection(&revisions, &[a]),
            Err(Error::ScopePartition)
        );
        assert_eq!(
            compose_generation_selection(&revisions, &[a, a]),
            Err(Error::ScopePartition)
        );
    }

    #[test]
    fn portable_identity_matches_descriptor_fixture_authority() {
        let selection = inspect_artifact_selection(SYNCHRONOUS).unwrap();
        let rows: Vec<_> = selection.rows().map(Result::unwrap).collect();
        let named = SelectionSpec {
            source_revision: rows[0].source_revision,
            root_kind: rows[0].root_kind,
            root_index: rows[0].root_index,
            path: &[],
            implementation_kind: rows[0].implementation_kind,
            disposition: rows[0].disposition,
        };
        let block_path = [2, 1];
        let block = SelectionSpec {
            source_revision: rows[1].source_revision,
            root_kind: rows[1].root_kind,
            root_index: rows[1].root_index,
            path: &block_path,
            implementation_kind: rows[1].implementation_kind,
            disposition: rows[1].disposition,
        };
        assert_eq!(
            portable_implementation_identity(named, 1).unwrap(),
            0x9ede_5adb_05eb_098e
        );
        assert_eq!(
            portable_implementation_identity(block, 1).unwrap(),
            0xb570_02af_cfe2_9873
        );
        assert_eq!(
            portable_implementation_identity(named, 0),
            Err(Error::InvalidLoweringVersion)
        );
    }

    #[test]
    fn all_truncations_and_adversarial_bytes_are_rejected_without_panicking() {
        for length in 0..MIXED.len() {
            assert!(inspect_artifact_selection(&MIXED[..length]).is_err());
        }
        let mut bytes = [0u8; 401];
        for seed in 0u32..10_000 {
            let mut value = seed.wrapping_mul(0x9e37_79b9);
            for byte in &mut bytes {
                value ^= value << 13;
                value ^= value >> 17;
                value ^= value << 5;
                *byte = value as u8;
            }
            let _ = inspect_artifact_selection(&bytes);
            let _ = inspect_generation_selection(&bytes);
        }
    }
}
