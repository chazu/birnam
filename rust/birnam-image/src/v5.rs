//! Borrowed image-v5 codec for the frozen stackless projection.

use birnam_artifact::resumable_manifest::{
    MAX_MANIFEST_BYTES, Manifest, inspect as inspect_manifest,
};
use birnam_artifact::stackless_selection::ImplementationKind;
use sha2::{Digest, Sha256};

pub const MAGIC: &[u8; 8] = b"BIRNAMIM";
pub const FORMAT_VERSION: u32 = 5;
pub const ENDIAN_TAG: u32 = 0x0102_0304;
pub const HEADER_LEN: usize = 28;
pub const RECORD_HEADER_LEN: usize = 12;
pub const AUTHORITY_BYTES: usize = 64;
pub const CODE_PREFIX_BYTES: usize = 8 + AUTHORITY_BYTES;
pub const RESUMABLES_PREFIX_BYTES: usize = 104;
pub const MIN_MANIFEST_BYTES: usize = 224;
pub const MAX_RESUMABLE_REFERENCES: usize = 1_048_576;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Truncated,
    Magic,
    Version,
    Endian,
    Architecture,
    RuntimeAbi,
    RecordKind,
    RecordLength,
    RecordOrder,
    MissingAuthority,
    DuplicateAuthority,
    MissingResumables,
    DuplicateResumables,
    OrphanResumables,
    MissingCode,
    DuplicateCode,
    AuthoritySpelling,
    AuthorityMismatch,
    ManifestLength,
    ManifestHash,
    Manifest,
    CodeChecksum,
    FieldTag,
    ExportName,
    ManifestIndex,
    NamedRowReference,
    Layout,
    ReferenceLimit,
    Arithmetic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub format_version: u32,
    pub architecture: u32,
    pub runtime_abi: u64,
}

impl Header {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() < HEADER_LEN {
            return Err(Error::Truncated);
        }
        if bytes.get(..8) != Some(MAGIC) {
            return Err(Error::Magic);
        }
        let format_version = u32_at(bytes, 8)?;
        if !matches!(format_version, FORMAT_VERSION | super::v6::FORMAT_VERSION) {
            return Err(Error::Version);
        }
        if u32_at(bytes, 12)? != ENDIAN_TAG {
            return Err(Error::Endian);
        }
        Ok(Self {
            format_version,
            architecture: u32_at(bytes, 16)?,
            runtime_abi: u64_at(bytes, 20)?,
        })
    }

    pub fn require(self, architecture: u32, runtime_abi: u64) -> Result<Self, Error> {
        if self.architecture != architecture {
            return Err(Error::Architecture);
        }
        if self.runtime_abi != runtime_abi {
            return Err(Error::RuntimeAbi);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum RecordKind {
    Facts = 2,
    Heap = 3,
    Code = 4,
    Authority = 5,
    Resumables = 6,
}

impl RecordKind {
    fn parse(value: u32) -> Result<Self, Error> {
        match value {
            2 => Ok(Self::Facts),
            3 => Ok(Self::Heap),
            4 => Ok(Self::Code),
            5 => Ok(Self::Authority),
            6 => Ok(Self::Resumables),
            _ => Err(Error::RecordKind),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Record<'a> {
    pub kind: RecordKind,
    pub payload: &'a [u8],
}

#[derive(Debug, Clone, Copy)]
pub struct Records<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Records<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub fn next_record(&mut self) -> Result<Option<Record<'a>>, Error> {
        if self.offset == self.bytes.len() {
            return Ok(None);
        }
        let header_end = self
            .offset
            .checked_add(RECORD_HEADER_LEN)
            .ok_or(Error::Arithmetic)?;
        let header = self
            .bytes
            .get(self.offset..header_end)
            .ok_or(Error::Truncated)?;
        let kind = RecordKind::parse(u32_at(header, 0)?)?;
        let length = usize::try_from(u64_at(header, 4)?).map_err(|_| Error::Arithmetic)?;
        let payload_end = header_end.checked_add(length).ok_or(Error::Arithmetic)?;
        let payload = self
            .bytes
            .get(header_end..payload_end)
            .ok_or(Error::Truncated)?;
        self.offset = payload_end;
        Ok(Some(Record { kind, payload }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Authority<'a> {
    pub spelling: &'a [u8; AUTHORITY_BYTES],
    pub raw: [u8; 32],
}

impl<'a> Authority<'a> {
    pub fn parse(payload: &'a [u8]) -> Result<Self, Error> {
        let spelling: &[u8; AUTHORITY_BYTES] =
            payload.try_into().map_err(|_| Error::AuthoritySpelling)?;
        let raw = decode_lower_hex(spelling)?;
        Ok(Self { spelling, raw })
    }
}

#[derive(Clone)]
pub struct Resumables<'a> {
    pub authority_spelling: &'a [u8; AUTHORITY_BYTES],
    pub manifest_sha256: [u8; 32],
    pub manifest_bytes: &'a [u8],
    pub manifest: Manifest<'a>,
}

impl<'a> Resumables<'a> {
    pub fn parse(payload: &'a [u8], authority: Authority<'a>) -> Result<Self, Error> {
        if payload.len() < RESUMABLES_PREFIX_BYTES {
            return Err(Error::ManifestLength);
        }
        let authority_spelling: &[u8; AUTHORITY_BYTES] = payload[..AUTHORITY_BYTES]
            .try_into()
            .map_err(|_| Error::AuthoritySpelling)?;
        if authority_spelling != authority.spelling {
            return Err(Error::AuthorityMismatch);
        }
        let manifest_sha256: [u8; 32] = payload[0x40..0x60]
            .try_into()
            .map_err(|_| Error::ManifestHash)?;
        let length = usize::try_from(u64_at(payload, 0x60)?).map_err(|_| Error::Arithmetic)?;
        if !(MIN_MANIFEST_BYTES..=MAX_MANIFEST_BYTES).contains(&length)
            || RESUMABLES_PREFIX_BYTES
                .checked_add(length)
                .ok_or(Error::Arithmetic)?
                != payload.len()
        {
            return Err(Error::ManifestLength);
        }
        let manifest_bytes = &payload[RESUMABLES_PREFIX_BYTES..];
        let actual_hash: [u8; 32] = Sha256::digest(manifest_bytes).into();
        if actual_hash != manifest_sha256 {
            return Err(Error::ManifestHash);
        }
        let manifest =
            inspect_manifest(manifest_bytes, authority.raw).map_err(|_| Error::Manifest)?;
        Ok(Self {
            authority_spelling,
            manifest_sha256,
            manifest_bytes,
            manifest,
        })
    }

    pub fn has_block_entries(&self) -> bool {
        (0..self.manifest.entry_count()).any(|index| {
            self.manifest
                .entry(index)
                .is_some_and(|entry| entry.implementation_kind == ImplementationKind::BlockBody)
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Code<'a> {
    pub checksum: u64,
    pub generation_digest: &'a [u8; AUTHORITY_BYTES],
    pub unit: &'a [u8],
}

impl<'a> Code<'a> {
    pub fn parse(payload: &'a [u8]) -> Result<Self, Error> {
        if payload.len() < CODE_PREFIX_BYTES {
            return Err(Error::RecordLength);
        }
        Ok(Self {
            checksum: u64_at(payload, 0)?,
            generation_digest: payload[8..CODE_PREFIX_BYTES]
                .try_into()
                .map_err(|_| Error::RecordLength)?,
            unit: &payload[CODE_PREFIX_BYTES..],
        })
    }

    pub fn checksum_matches(self) -> bool {
        super::codec::wyhash(0, self.unit) == self.checksum
    }
}

#[derive(Clone)]
pub struct Image<'a> {
    pub header: Header,
    pub authority: Authority<'a>,
    resumables: Option<Resumables<'a>>,
    code: Option<Code<'a>>,
    records: Records<'a>,
}

impl<'a> Image<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, Error> {
        let header = Header::parse(bytes)?;
        let record_bytes = bytes.get(HEADER_LEN..).ok_or(Error::Truncated)?;
        let mut records = Records::new(record_bytes);
        let first = records.next_record()?.ok_or(Error::MissingAuthority)?;
        if first.kind != RecordKind::Authority {
            return Err(Error::MissingAuthority);
        }
        let authority = Authority::parse(first.payload)?;
        let mut resumables = None;
        let mut code = None;
        let mut seen_heap = false;
        let mut seen_facts = false;
        let mut record_index = 1usize;
        while let Some(record) = records.next_record()? {
            if code.is_some() {
                return Err(Error::RecordOrder);
            }
            match record.kind {
                RecordKind::Authority => return Err(Error::DuplicateAuthority),
                RecordKind::Resumables => {
                    if resumables.is_some() {
                        return Err(Error::DuplicateResumables);
                    }
                    if record_index != 1 || seen_heap || seen_facts {
                        return Err(Error::RecordOrder);
                    }
                    resumables = Some(Resumables::parse(record.payload, authority)?);
                }
                RecordKind::Heap => {
                    if seen_facts {
                        return Err(Error::RecordOrder);
                    }
                    seen_heap = true;
                }
                RecordKind::Facts => seen_facts = true,
                RecordKind::Code => {
                    let Some(_) = resumables else {
                        return Err(Error::MissingResumables);
                    };
                    let value = Code::parse(record.payload)?;
                    if value.generation_digest != authority.spelling {
                        return Err(Error::AuthorityMismatch);
                    }
                    if !value.checksum_matches() {
                        return Err(Error::CodeChecksum);
                    }
                    if code.replace(value).is_some() {
                        return Err(Error::DuplicateCode);
                    }
                }
            }
            record_index += 1;
        }
        if code.is_none() && resumables.is_some() {
            return Err(Error::OrphanResumables);
        }
        Ok(Self {
            header,
            authority,
            resumables,
            code,
            records: Records::new(record_bytes),
        })
    }

    pub fn preflight(bytes: &'a [u8], architecture: u32, runtime_abi: u64) -> Result<Self, Error> {
        let image = Self::parse(bytes)?;
        image.header.require(architecture, runtime_abi)?;
        image.validate_stackless_references()?;
        Ok(image)
    }

    pub fn preflight_with_layout<F>(
        bytes: &'a [u8],
        architecture: u32,
        runtime_abi: u64,
        mut layout_for: F,
    ) -> Result<Self, Error>
    where
        F: FnMut(&[u8], u32) -> Option<Vec<SlotKind>>,
    {
        let image = Self::parse(bytes)?;
        image.header.require(architecture, runtime_abi)?;
        image.validate_references_with_layout(&mut layout_for)?;
        Ok(image)
    }

    pub fn resumables(&self) -> Option<&Resumables<'a>> {
        self.resumables.as_ref()
    }

    pub fn code(&self) -> Option<Code<'a>> {
        self.code
    }

    pub fn records(&self) -> Records<'a> {
        self.records
    }

    pub fn validate_stackless_references(&self) -> Result<ReferenceSummary, Error> {
        self.validate_references_with_layout(&mut |_, _| None)
    }

    pub fn validate_references_with_layout<F>(
        &self,
        layout_for: &mut F,
    ) -> Result<ReferenceSummary, Error>
    where
        F: FnMut(&[u8], u32) -> Option<Vec<SlotKind>>,
    {
        let mut summary = ReferenceSummary::default();
        let mut records = self.records();
        while let Some(record) = records.next_record()? {
            match record.kind {
                RecordKind::Facts => {
                    if self.header.format_version == super::v6::FORMAT_VERSION {
                        let mut facts = super::v6::Facts::parse(record.payload)?;
                        while let Some(fact) = facts.next_fact()? {
                            for field in [fact.entity, fact.attribute, fact.value] {
                                self.validate_field(field, &mut summary)?;
                            }
                        }
                    } else {
                        let mut facts = Facts::parse(record.payload)?;
                        while let Some(fact) = facts.next_fact()? {
                            for field in [fact.entity, fact.attribute, fact.value] {
                                self.validate_field(field, &mut summary)?;
                            }
                        }
                    }
                }
                RecordKind::Heap => {
                    let mut heap = Heap::parse(record.payload)?;
                    while let Some(header) = heap.peek_header()? {
                        match header.vtable {
                            VtableLocator::Export(name) => {
                                let layout =
                                    layout_for(name, header.word_count).ok_or(Error::Layout)?;
                                let object =
                                    heap.next_object(&layout)?.ok_or(Error::RecordLength)?;
                                self.validate_object_fields(
                                    object.encoded_slots,
                                    &layout,
                                    &mut summary,
                                )?;
                            }
                            VtableLocator::ResumableBlockVtableRef(index) => {
                                self.validate_index(index, &mut summary)?;
                                let object = heap.next_resumable_object()?;
                                let object = object.ok_or(Error::RecordLength)?;
                                let mut fields = Fields::new(object.encoded_slots);
                                while let Some(field) = fields.next_field()? {
                                    self.validate_field(field, &mut summary)?;
                                }
                            }
                        }
                    }
                }
                RecordKind::Authority | RecordKind::Resumables | RecordKind::Code => {}
            }
        }
        Ok(summary)
    }

    fn validate_object_fields(
        &self,
        encoded: &[u8],
        layout: &[SlotKind],
        summary: &mut ReferenceSummary,
    ) -> Result<(), Error> {
        let mut offset = 0usize;
        for kind in layout {
            match kind {
                SlotKind::Oop => {
                    let mut fields = Fields::new(encoded.get(offset..).ok_or(Error::RecordLength)?);
                    let field = fields.next_field()?.ok_or(Error::RecordLength)?;
                    self.validate_field(field, summary)?;
                    offset = offset
                        .checked_add(fields.consumed())
                        .ok_or(Error::Arithmetic)?;
                }
                SlotKind::RawWord => {
                    offset = offset.checked_add(8).ok_or(Error::Arithmetic)?;
                    if offset > encoded.len() {
                        return Err(Error::RecordLength);
                    }
                }
                SlotKind::Code => {}
            }
        }
        if offset != encoded.len() {
            return Err(Error::RecordLength);
        }
        Ok(())
    }

    fn validate_field(
        &self,
        field: Field<'_>,
        summary: &mut ReferenceSummary,
    ) -> Result<(), Error> {
        if let Field::ResumableBlockVtableRef(index) = field {
            self.validate_index(index, summary)?;
        }
        Ok(())
    }

    fn validate_index(&self, index: u32, summary: &mut ReferenceSummary) -> Result<(), Error> {
        let resumables = self.resumables.as_ref().ok_or(Error::MissingResumables)?;
        let entry = resumables
            .manifest
            .entry(index as usize)
            .ok_or(Error::ManifestIndex)?;
        if entry.implementation_kind != ImplementationKind::BlockBody {
            return Err(Error::NamedRowReference);
        }
        summary.occurrences = summary
            .occurrences
            .checked_add(1)
            .ok_or(Error::ReferenceLimit)?;
        if summary.occurrences > MAX_RESUMABLE_REFERENCES {
            return Err(Error::ReferenceLimit);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReferenceSummary {
    pub occurrences: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FieldTag {
    Nil = 0,
    True = 1,
    False = 2,
    Integer = 3,
    FloatBits = 4,
    Symbol = 5,
    String = 6,
    HeapRef = 7,
    NullOop = 8,
    ExportRef = 9,
    ResumableBlockVtableRef = 10,
}

impl FieldTag {
    fn parse(value: u8) -> Result<Self, Error> {
        match value {
            0 => Ok(Self::Nil),
            1 => Ok(Self::True),
            2 => Ok(Self::False),
            3 => Ok(Self::Integer),
            4 => Ok(Self::FloatBits),
            5 => Ok(Self::Symbol),
            6 => Ok(Self::String),
            7 => Ok(Self::HeapRef),
            8 => Ok(Self::NullOop),
            9 => Ok(Self::ExportRef),
            10 => Ok(Self::ResumableBlockVtableRef),
            _ => Err(Error::FieldTag),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field<'a> {
    Nil,
    True,
    False,
    Integer(i64),
    FloatBits(u64),
    Symbol(&'a [u8]),
    String(&'a [u8]),
    HeapRef(u32),
    NullOop,
    ExportRef(&'a [u8]),
    ResumableBlockVtableRef(u32),
}

#[derive(Debug, Clone, Copy)]
pub struct Fields<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Fields<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub fn next_field(&mut self) -> Result<Option<Field<'a>>, Error> {
        if self.offset == self.bytes.len() {
            return Ok(None);
        }
        let tag = FieldTag::parse(self.take_byte()?)?;
        let field = match tag {
            FieldTag::Nil => Field::Nil,
            FieldTag::True => Field::True,
            FieldTag::False => Field::False,
            FieldTag::Integer => Field::Integer(i64::from_le_bytes(self.take_fixed()?)),
            FieldTag::FloatBits => Field::FloatBits(u64::from_le_bytes(self.take_fixed()?)),
            FieldTag::Symbol => Field::Symbol(self.take_bytes()?),
            FieldTag::String => Field::String(self.take_bytes()?),
            FieldTag::HeapRef => Field::HeapRef(u32::from_le_bytes(self.take_fixed()?)),
            FieldTag::NullOop => Field::NullOop,
            FieldTag::ExportRef => Field::ExportRef(self.take_bytes()?),
            FieldTag::ResumableBlockVtableRef => {
                Field::ResumableBlockVtableRef(u32::from_le_bytes(self.take_fixed()?))
            }
        };
        Ok(Some(field))
    }

    pub fn consumed(&self) -> usize {
        self.offset
    }

    fn exhausted(&self) -> bool {
        self.offset == self.bytes.len()
    }

    fn take_byte(&mut self) -> Result<u8, Error> {
        let value = *self.bytes.get(self.offset).ok_or(Error::RecordLength)?;
        self.offset += 1;
        Ok(value)
    }

    fn take_fixed<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        let end = self.offset.checked_add(N).ok_or(Error::Arithmetic)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(Error::RecordLength)?;
        self.offset = end;
        Ok(value.try_into().expect("fixed-width field"))
    }

    fn take_bytes(&mut self) -> Result<&'a [u8], Error> {
        let length = u32::from_le_bytes(self.take_fixed()?) as usize;
        let end = self.offset.checked_add(length).ok_or(Error::Arithmetic)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(Error::RecordLength)?;
        self.offset = end;
        Ok(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fact<'a> {
    pub operation: u8,
    pub transaction: u64,
    pub entity: Field<'a>,
    pub attribute: Field<'a>,
    pub value: Field<'a>,
}

#[derive(Debug, Clone, Copy)]
pub struct Facts<'a> {
    basis: u64,
    remaining: u32,
    fields: Fields<'a>,
}

impl<'a> Facts<'a> {
    pub fn parse(payload: &'a [u8]) -> Result<Self, Error> {
        if payload.len() < 12 {
            return Err(Error::RecordLength);
        }
        Ok(Self {
            basis: u64_at(payload, 0)?,
            remaining: u32_at(payload, 8)?,
            fields: Fields::new(&payload[12..]),
        })
    }

    pub fn basis(&self) -> u64 {
        self.basis
    }

    pub fn next_fact(&mut self) -> Result<Option<Fact<'a>>, Error> {
        if self.remaining == 0 {
            return if self.fields.exhausted() {
                Ok(None)
            } else {
                Err(Error::RecordLength)
            };
        }
        let operation = self.fields.take_byte()?;
        let transaction = u64::from_le_bytes(self.fields.take_fixed()?);
        let entity = self.fields.next_field()?.ok_or(Error::RecordLength)?;
        let attribute = self.fields.next_field()?.ok_or(Error::RecordLength)?;
        let value = self.fields.next_field()?.ok_or(Error::RecordLength)?;
        self.remaining -= 1;
        Ok(Some(Fact {
            operation,
            transaction,
            entity,
            attribute,
            value,
        }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VtableLocator<'a> {
    Export(&'a [u8]),
    ResumableBlockVtableRef(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeapHeader<'a> {
    pub vtable: VtableLocator<'a>,
    pub word_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeapObject<'a> {
    pub header: HeapHeader<'a>,
    pub encoded_slots: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    Oop,
    RawWord,
    Code,
}

#[derive(Debug, Clone, Copy)]
pub struct Heap<'a> {
    bytes: &'a [u8],
    offset: usize,
    remaining: u32,
}

impl<'a> Heap<'a> {
    pub fn parse(payload: &'a [u8]) -> Result<Self, Error> {
        if payload.len() < 4 {
            return Err(Error::RecordLength);
        }
        Ok(Self {
            bytes: payload,
            offset: 4,
            remaining: u32_at(payload, 0)?,
        })
    }

    pub fn peek_header(&self) -> Result<Option<HeapHeader<'a>>, Error> {
        self.header_at()
            .map(|value| value.map(|(header, _)| header))
    }

    pub fn next_object(&mut self, layout: &[SlotKind]) -> Result<Option<HeapObject<'a>>, Error> {
        let Some((header, body_start)) = self.header_at()? else {
            return Ok(None);
        };
        if layout.len() != header.word_count as usize {
            return Err(Error::Layout);
        }
        let mut cursor = body_start;
        for kind in layout {
            match kind {
                SlotKind::Oop => cursor = consume_field(self.bytes, cursor)?,
                SlotKind::RawWord => {
                    cursor = cursor.checked_add(8).ok_or(Error::Arithmetic)?;
                    if cursor > self.bytes.len() {
                        return Err(Error::RecordLength);
                    }
                }
                SlotKind::Code => {}
            }
        }
        self.finish_object(header, body_start, cursor)
    }

    pub fn next_resumable_object(&mut self) -> Result<Option<HeapObject<'a>>, Error> {
        let Some((header, body_start)) = self.header_at()? else {
            return Ok(None);
        };
        if !matches!(header.vtable, VtableLocator::ResumableBlockVtableRef(_)) {
            return Err(Error::Layout);
        }
        let mut cursor = body_start;
        for _ in 0..header.word_count {
            cursor = consume_field(self.bytes, cursor)?;
        }
        self.finish_object(header, body_start, cursor)
    }

    fn finish_object(
        &mut self,
        header: HeapHeader<'a>,
        body_start: usize,
        body_end: usize,
    ) -> Result<Option<HeapObject<'a>>, Error> {
        let encoded_slots = self
            .bytes
            .get(body_start..body_end)
            .ok_or(Error::RecordLength)?;
        self.offset = body_end;
        self.remaining -= 1;
        Ok(Some(HeapObject {
            header,
            encoded_slots,
        }))
    }

    fn header_at(&self) -> Result<Option<(HeapHeader<'a>, usize)>, Error> {
        if self.remaining == 0 {
            return if self.offset == self.bytes.len() {
                Ok(None)
            } else {
                Err(Error::RecordLength)
            };
        }
        let tag = *self.bytes.get(self.offset).ok_or(Error::RecordLength)?;
        let mut cursor = self.offset + 1;
        let vtable = match FieldTag::parse(tag)? {
            FieldTag::ExportRef => {
                let length = u32_at(self.bytes, cursor)? as usize;
                cursor = cursor.checked_add(4).ok_or(Error::Arithmetic)?;
                let end = cursor.checked_add(length).ok_or(Error::Arithmetic)?;
                let name = self.bytes.get(cursor..end).ok_or(Error::RecordLength)?;
                if name.is_empty() || name.contains(&0) {
                    return Err(Error::ExportName);
                }
                cursor = end;
                VtableLocator::Export(name)
            }
            FieldTag::ResumableBlockVtableRef => {
                let index = u32_at(self.bytes, cursor)?;
                cursor = cursor.checked_add(4).ok_or(Error::Arithmetic)?;
                VtableLocator::ResumableBlockVtableRef(index)
            }
            _ => return Err(Error::FieldTag),
        };
        let word_count = u32_at(self.bytes, cursor)?;
        cursor = cursor.checked_add(4).ok_or(Error::Arithmetic)?;
        Ok(Some((HeapHeader { vtable, word_count }, cursor)))
    }
}

fn consume_field(bytes: &[u8], offset: usize) -> Result<usize, Error> {
    let mut fields = Fields::new(bytes.get(offset..).ok_or(Error::RecordLength)?);
    fields.next_field()?.ok_or(Error::RecordLength)?;
    offset
        .checked_add(fields.consumed())
        .ok_or(Error::Arithmetic)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeError {
    MissingAuthority,
    DuplicateAuthority,
    MissingResumables,
    DuplicateResumables,
    OrphanResumables,
    DuplicateCode,
    AuthorityMismatch,
    RecordOrder,
    PayloadTooLarge,
    Manifest,
}

pub struct ImageWriter {
    bytes: Vec<u8>,
    authority: Option<[u8; AUTHORITY_BYTES]>,
    resumables_written: bool,
    seen_heap: bool,
    seen_facts: bool,
    code_written: bool,
}

impl ImageWriter {
    pub fn new(architecture: u32, runtime_abi: u64) -> Self {
        Self::new_version(architecture, runtime_abi, FORMAT_VERSION)
    }

    pub(crate) fn new_version(architecture: u32, runtime_abi: u64, version: u32) -> Self {
        let mut bytes = Vec::with_capacity(HEADER_LEN);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&version.to_le_bytes());
        bytes.extend_from_slice(&ENDIAN_TAG.to_le_bytes());
        bytes.extend_from_slice(&architecture.to_le_bytes());
        bytes.extend_from_slice(&runtime_abi.to_le_bytes());
        Self {
            bytes,
            authority: None,
            resumables_written: false,
            seen_heap: false,
            seen_facts: false,
            code_written: false,
        }
    }

    pub fn authority(&mut self, spelling: &[u8; AUTHORITY_BYTES]) -> Result<(), EncodeError> {
        if self.authority.is_some() {
            return Err(EncodeError::DuplicateAuthority);
        }
        decode_lower_hex(spelling).map_err(|_| EncodeError::AuthorityMismatch)?;
        self.push_record(RecordKind::Authority, spelling)?;
        self.authority = Some(*spelling);
        Ok(())
    }

    pub fn resumables(&mut self, manifest: &[u8]) -> Result<(), EncodeError> {
        let authority = self.authority.ok_or(EncodeError::MissingAuthority)?;
        if self.resumables_written {
            return Err(EncodeError::DuplicateResumables);
        }
        if self.seen_heap || self.seen_facts || self.code_written {
            return Err(EncodeError::RecordOrder);
        }
        let raw = decode_lower_hex(&authority).map_err(|_| EncodeError::AuthorityMismatch)?;
        inspect_manifest(manifest, raw).map_err(|_| EncodeError::Manifest)?;
        let length = u64::try_from(manifest.len()).map_err(|_| EncodeError::PayloadTooLarge)?;
        let payload_length = RESUMABLES_PREFIX_BYTES
            .checked_add(manifest.len())
            .ok_or(EncodeError::PayloadTooLarge)?;
        let mut payload = Vec::with_capacity(payload_length);
        payload.extend_from_slice(&authority);
        payload.extend_from_slice(&Sha256::digest(manifest));
        payload.extend_from_slice(&length.to_le_bytes());
        payload.extend_from_slice(manifest);
        self.push_record(RecordKind::Resumables, &payload)?;
        self.resumables_written = true;
        Ok(())
    }

    pub fn heap(&mut self, payload: &[u8]) -> Result<(), EncodeError> {
        if self.authority.is_none() || self.seen_facts || self.code_written {
            return Err(EncodeError::RecordOrder);
        }
        self.push_record(RecordKind::Heap, payload)?;
        self.seen_heap = true;
        Ok(())
    }

    pub fn facts(&mut self, payload: &[u8]) -> Result<(), EncodeError> {
        if self.authority.is_none() || self.code_written {
            return Err(EncodeError::RecordOrder);
        }
        self.push_record(RecordKind::Facts, payload)?;
        self.seen_facts = true;
        Ok(())
    }

    pub fn code(
        &mut self,
        checksum: u64,
        generation_digest: &[u8; AUTHORITY_BYTES],
        unit: &[u8],
    ) -> Result<(), EncodeError> {
        let authority = self.authority.ok_or(EncodeError::MissingAuthority)?;
        if !self.resumables_written {
            return Err(EncodeError::MissingResumables);
        }
        if self.code_written {
            return Err(EncodeError::DuplicateCode);
        }
        if authority != *generation_digest {
            return Err(EncodeError::AuthorityMismatch);
        }
        let mut payload = Vec::with_capacity(CODE_PREFIX_BYTES + unit.len());
        payload.extend_from_slice(&checksum.to_le_bytes());
        payload.extend_from_slice(generation_digest);
        payload.extend_from_slice(unit);
        self.push_record(RecordKind::Code, &payload)?;
        self.code_written = true;
        Ok(())
    }

    pub fn finish(self) -> Result<Vec<u8>, EncodeError> {
        if self.authority.is_none() {
            return Err(EncodeError::MissingAuthority);
        }
        if self.resumables_written != self.code_written {
            return if self.resumables_written {
                Err(EncodeError::OrphanResumables)
            } else {
                Err(EncodeError::MissingResumables)
            };
        }
        Ok(self.bytes)
    }

    fn push_record(&mut self, kind: RecordKind, payload: &[u8]) -> Result<(), EncodeError> {
        let length = u64::try_from(payload.len()).map_err(|_| EncodeError::PayloadTooLarge)?;
        self.bytes.extend_from_slice(&(kind as u32).to_le_bytes());
        self.bytes.extend_from_slice(&length.to_le_bytes());
        self.bytes.extend_from_slice(payload);
        Ok(())
    }
}

/// Compose one complete code-bearing image from already-preflighted payloads.
///
/// This owns only v5 framing and the authority/manifest/code relationships.
/// Callers remain responsible for constructing the generation-owned manifest,
/// validating heap layouts, and deciding whether the bytes may be published.
pub fn compose_complete_image(
    architecture: u32,
    runtime_abi: u64,
    facts: &[u8],
    heap: &[u8],
    authority: &[u8; AUTHORITY_BYTES],
    manifest: &[u8],
    code_checksum: u64,
    code_unit: &[u8],
) -> Result<Vec<u8>, EncodeError> {
    compose_complete_image_version(
        architecture,
        runtime_abi,
        FORMAT_VERSION,
        facts,
        heap,
        authority,
        manifest,
        code_checksum,
        code_unit,
    )
}

pub(crate) fn compose_complete_image_version(
    architecture: u32,
    runtime_abi: u64,
    format_version: u32,
    facts: &[u8],
    heap: &[u8],
    authority: &[u8; AUTHORITY_BYTES],
    manifest: &[u8],
    code_checksum: u64,
    code_unit: &[u8],
) -> Result<Vec<u8>, EncodeError> {
    let mut writer = ImageWriter::new_version(architecture, runtime_abi, format_version);
    writer.authority(authority)?;
    writer.resumables(manifest)?;
    if !heap.is_empty() {
        writer.heap(heap)?;
    }
    if !facts.is_empty() {
        writer.facts(facts)?;
    }
    writer.code(code_checksum, authority, code_unit)?;
    writer.finish()
}

pub fn encode_export_locator(output: &mut Vec<u8>, name: &[u8]) -> Result<(), EncodeError> {
    if name.is_empty() || name.contains(&0) {
        return Err(EncodeError::RecordOrder);
    }
    let length = u32::try_from(name.len()).map_err(|_| EncodeError::PayloadTooLarge)?;
    output.push(FieldTag::ExportRef as u8);
    output.extend_from_slice(&length.to_le_bytes());
    output.extend_from_slice(name);
    Ok(())
}

pub fn encode_resumable_ref(output: &mut Vec<u8>, index: u32) {
    output.push(FieldTag::ResumableBlockVtableRef as u8);
    output.extend_from_slice(&index.to_le_bytes());
}

fn decode_lower_hex(spelling: &[u8; AUTHORITY_BYTES]) -> Result<[u8; 32], Error> {
    fn nibble(value: u8) -> Option<u8> {
        match value {
            b'0'..=b'9' => Some(value - b'0'),
            b'a'..=b'f' => Some(value - b'a' + 10),
            _ => None,
        }
    }
    let mut raw = [0; 32];
    for (index, pair) in spelling.chunks_exact(2).enumerate() {
        raw[index] = nibble(pair[0])
            .ok_or(Error::AuthoritySpelling)?
            .checked_shl(4)
            .ok_or(Error::AuthoritySpelling)?
            | nibble(pair[1]).ok_or(Error::AuthoritySpelling)?;
    }
    Ok(raw)
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    let end = offset.checked_add(4).ok_or(Error::Arithmetic)?;
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..end)
            .ok_or(Error::Truncated)?
            .try_into()
            .expect("four-byte slice"),
    ))
}

fn u64_at(bytes: &[u8], offset: usize) -> Result<u64, Error> {
    let end = offset.checked_add(8).ok_or(Error::Arithmetic)?;
    Ok(u64::from_le_bytes(
        bytes
            .get(offset..end)
            .ok_or(Error::Truncated)?
            .try_into()
            .expect("eight-byte slice"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../abi/fixtures/stackless-s2")
                .join(name),
        )
        .unwrap()
    }

    #[test]
    fn frozen_images_admit_with_exact_stackless_relationships() {
        let empty = fixture("image-v5-empty-manifest.bin");
        let image = Image::preflight(&empty, 1, 0x6c3b_67f0_ef16_14b3).unwrap();
        assert_eq!(image.resumables().unwrap().manifest.entry_count(), 0);
        assert!(!image.resumables().unwrap().has_block_entries());
        assert!(image.code().is_some());

        let trace = fixture("image-v5-construction-trace.bin");
        let image = Image::preflight(&trace, 1, 0x6c3b_67f0_ef16_14b3).unwrap();
        assert_eq!(image.resumables().unwrap().manifest.entry_count(), 2);
        assert!(image.resumables().unwrap().has_block_entries());
        assert_eq!(
            image.validate_stackless_references().unwrap(),
            ReferenceSummary { occurrences: 4 }
        );

        let no_code = fixture("image-v5-no-code.bin");
        let image = Image::preflight(&no_code, 1, 0x6c3b_67f0_ef16_14b3).unwrap();
        assert!(image.resumables().is_none());
        assert!(image.code().is_none());
    }

    #[test]
    fn frozen_invalid_images_fail_before_materialization() {
        for name in [
            "image-v5-out-of-range-index.bin",
            "image-v5-named-row-index.bin",
            "image-v5-authority-mismatch.bin",
            "image-v5-manifest-hash-mismatch.bin",
        ] {
            let bytes = fixture(name);
            assert!(
                Image::preflight(&bytes, 1, 0x6c3b_67f0_ef16_14b3).is_err(),
                "{name}"
            );
        }
    }

    #[test]
    fn writer_reproduces_all_frozen_valid_images() {
        for name in [
            "image-v5-empty-manifest.bin",
            "image-v5-construction-trace.bin",
            "image-v5-no-code.bin",
        ] {
            let expected = fixture(name);
            let image = Image::parse(&expected).unwrap();
            let mut writer = ImageWriter::new(image.header.architecture, image.header.runtime_abi);
            writer.authority(image.authority.spelling).unwrap();
            if let Some(resumables) = image.resumables() {
                writer.resumables(resumables.manifest_bytes).unwrap();
            }
            let mut records = image.records();
            while let Some(record) = records.next_record().unwrap() {
                match record.kind {
                    RecordKind::Heap => writer.heap(record.payload).unwrap(),
                    RecordKind::Facts => writer.facts(record.payload).unwrap(),
                    RecordKind::Code => {
                        let code = Code::parse(record.payload).unwrap();
                        writer
                            .code(code.checksum, code.generation_digest, code.unit)
                            .unwrap();
                    }
                    RecordKind::Authority | RecordKind::Resumables => {}
                }
            }
            assert_eq!(writer.finish().unwrap(), expected, "{name}");
        }
    }

    #[test]
    fn complete_composer_reproduces_the_frozen_empty_manifest_image() {
        let bytes = fixture("image-v5-empty-manifest.bin");
        let image = Image::parse(&bytes).unwrap();
        let mut heap = None;
        let mut facts = None;
        let mut records = image.records();
        while let Some(record) = records.next_record().unwrap() {
            match record.kind {
                RecordKind::Heap => heap = Some(record.payload),
                RecordKind::Facts => facts = Some(record.payload),
                RecordKind::Authority | RecordKind::Resumables | RecordKind::Code => {}
            }
        }
        let resumables = image.resumables().unwrap();
        let code = image.code().unwrap();
        assert_eq!(
            compose_complete_image(
                image.header.architecture,
                image.header.runtime_abi,
                facts.unwrap_or(&[]),
                heap.unwrap_or(&[]),
                image.authority.spelling,
                resumables.manifest_bytes,
                code.checksum,
                code.unit,
            )
            .unwrap(),
            bytes
        );
    }

    #[test]
    fn every_non_image_prefix_of_the_trace_fails() {
        let bytes = fixture("image-v5-construction-trace.bin");
        for length in 0..bytes.len() {
            if length == HEADER_LEN + RECORD_HEADER_LEN + AUTHORITY_BYTES {
                assert!(
                    Image::parse(&bytes[..length]).is_ok(),
                    "the Authority-only prefix is independently a canonical no-Code image"
                );
            } else {
                assert!(Image::parse(&bytes[..length]).is_err(), "length {length}");
            }
        }
    }

    #[test]
    fn ordinary_heap_preflight_requires_and_uses_the_exported_layout() {
        let authority_image = fixture("image-v5-no-code.bin");
        let authority = Image::parse(&authority_image).unwrap().authority;
        let mut heap = Vec::new();
        heap.extend_from_slice(&1u32.to_le_bytes());
        encode_export_locator(&mut heap, b"PointVt").unwrap();
        heap.extend_from_slice(&2u32.to_le_bytes());
        heap.push(FieldTag::ResumableBlockVtableRef as u8);
        heap.extend_from_slice(&0u32.to_le_bytes());
        heap.extend_from_slice(&19u64.to_le_bytes());

        let mut writer = ImageWriter::new(1, 0x6c3b_67f0_ef16_14b3);
        writer.authority(authority.spelling).unwrap();
        writer.heap(&heap).unwrap();
        let bytes = writer.finish().unwrap();
        assert!(matches!(
            Image::preflight(&bytes, 1, 0x6c3b_67f0_ef16_14b3),
            Err(Error::Layout)
        ));
        assert!(
            matches!(
                Image::preflight_with_layout(&bytes, 1, 0x6c3b_67f0_ef16_14b3, |name, words| (name
                    == b"PointVt"
                    && words == 2)
                    .then_some(vec![SlotKind::Oop, SlotKind::RawWord]),),
                Err(Error::MissingResumables)
            ),
            "tag-10 in a no-Code ordinary object is rejected after layout-guided decoding"
        );

        heap.truncate(heap.len() - 13);
        heap.push(FieldTag::Nil as u8);
        heap.extend_from_slice(&19u64.to_le_bytes());
        let mut writer = ImageWriter::new(1, 0x6c3b_67f0_ef16_14b3);
        writer.authority(authority.spelling).unwrap();
        writer.heap(&heap).unwrap();
        let bytes = writer.finish().unwrap();
        let image =
            Image::preflight_with_layout(&bytes, 1, 0x6c3b_67f0_ef16_14b3, |name, words| {
                (name == b"PointVt" && words == 2).then_some(vec![SlotKind::Oop, SlotKind::RawWord])
            })
            .unwrap();
        assert_eq!(
            image
                .validate_references_with_layout(&mut |_, _| Some(vec![
                    SlotKind::Oop,
                    SlotKind::RawWord
                ]))
                .unwrap(),
            ReferenceSummary { occurrences: 0 }
        );
    }
}
