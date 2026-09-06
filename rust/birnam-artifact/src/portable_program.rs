//! Canonical pointer-free PortableProgram v1 bytes.
//!
//! This module owns no `IdApi`, Oop, compiler IR, or native materialization.
//! It encodes semantic program values and returns borrowed, validated views.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramSpec<'a> {
    pub parameters: &'a [&'a [u8]],
    pub captures: &'a [&'a [u8]],
    pub lets: &'a [LetSpec<'a>],
    pub statements: &'a [StatementSpec],
    pub nodes: &'a [NodeSpec<'a>],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LetSpec<'a> {
    pub name: &'a [u8],
    pub defining_statement: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatementSpec {
    Expression { root_node: u32 },
    Let { root_node: u32, let_index: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeSpec<'a> {
    Integer(i64),
    FloatBits(u64),
    String(&'a [u8]),
    Symbol(&'a [u8]),
    Nil,
    True,
    False,
    LexicalRead(u32),
    GlobalRead(&'a [u8]),
    Send {
        selector: &'a [u8],
        operands: &'a [u32],
    },
    BlockLiteral {
        program: &'a [u8],
        captures: &'a [u32],
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Node<'a> {
    pub kind: u32,
    pub operand_count: u32,
    pub payload: u64,
    pub auxiliary_0: u32,
    pub auxiliary_1: u32,
    pub data: &'a [u8],
}

/// Semantic discriminator for one already-validated PortableProgram node.
///
/// Consumers must use this enum rather than repeating the frozen wire tag
/// numbers. Node payload, data, and operands retain their zero-copy borrowed
/// representation on [`Node`] and [`Program`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Integer,
    FloatBits,
    String,
    Symbol,
    Nil,
    True,
    False,
    LexicalRead,
    GlobalRead,
    Send,
    BlockLiteral,
}

impl Node<'_> {
    pub fn semantic_kind(&self) -> Option<NodeKind> {
        Some(match self.kind {
            1 => NodeKind::Integer,
            2 => NodeKind::FloatBits,
            3 => NodeKind::String,
            4 => NodeKind::Symbol,
            5 => NodeKind::Nil,
            6 => NodeKind::True,
            7 => NodeKind::False,
            8 => NodeKind::LexicalRead,
            9 => NodeKind::GlobalRead,
            10 => NodeKind::Send,
            11 => NodeKind::BlockLiteral,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingKind {
    Parameter,
    Capture,
    Let,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding<'a> {
    pub kind: BindingKind,
    pub ordinal: u32,
    pub name: &'a [u8],
    pub defining_statement: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatementKind {
    Expression,
    Let,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Statement {
    pub kind: StatementKind,
    pub root_node: u32,
    pub binding_index: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Program<'a> {
    pub parameter_count: u32,
    pub capture_count: u32,
    pub binding_count: u32,
    pub statement_count: u32,
    pub node_count: u32,
    pub operand_count: u32,
    bindings: &'a [u8],
    statements: &'a [u8],
    nodes: &'a [u8],
    operands: &'a [u8],
    bytes: &'a [u8],
}

/// Immutable canonical bytes with their already-validated root section layout.
///
/// Offsets, rather than borrowed pointers, make this owner freely movable.
/// Every view borrows these exact bytes; callers cannot mutate the backing
/// storage or reuse the layout with another program. Nested programs are fully
/// validated on admission, just as they are by `inspect`.
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedProgram {
    bytes: Vec<u8>,
    layout: ProgramLayout,
}

#[derive(Debug, PartialEq, Eq)]
struct ProgramLayout {
    counts: [u32; 6],
    sections: [core::ops::Range<usize>; 5],
}

impl ProgramLayout {
    fn inspect(bytes: &[u8]) -> Result<Self, Error> {
        let program = inspect(bytes)?;
        let counts = [
            program.parameter_count,
            program.capture_count,
            program.binding_count,
            program.statement_count,
            program.node_count,
            program.operand_count,
        ];
        let sections = [
            program.bindings,
            program.statements,
            program.nodes,
            program.operands,
            program.bytes,
        ]
        .map(|section| {
            // inspect returns subslices of this same allocation, including
            // empty sections. Only integer offsets survive the borrow.
            let start = section.as_ptr().addr() - bytes.as_ptr().addr();
            start..start + section.len()
        });
        Ok(Self { counts, sections })
    }
}

impl ValidatedProgram {
    pub fn new(bytes: Vec<u8>) -> Result<Self, Error> {
        let layout = ProgramLayout::inspect(&bytes)?;
        Ok(Self { bytes, layout })
    }

    /// Validate foreign bytes before allocating their immutable owned copy.
    pub fn copy_from_slice(bytes: &[u8]) -> Result<Self, Error> {
        let layout = ProgramLayout::inspect(bytes)?;
        Ok(Self {
            bytes: bytes.to_vec(),
            layout,
        })
    }

    /// Reconstitute a checked view without validation or scratch allocation.
    pub fn view(&self) -> Program<'_> {
        Program {
            parameter_count: self.layout.counts[0],
            capture_count: self.layout.counts[1],
            binding_count: self.layout.counts[2],
            statement_count: self.layout.counts[3],
            node_count: self.layout.counts[4],
            operand_count: self.layout.counts[5],
            bindings: &self.bytes[self.layout.sections[0].clone()],
            statements: &self.bytes[self.layout.sections[1].clone()],
            nodes: &self.bytes[self.layout.sections[2].clone()],
            operands: &self.bytes[self.layout.sections[3].clone()],
            bytes: &self.bytes[self.layout.sections[4].clone()],
        }
    }
}

impl core::ops::Deref for ValidatedProgram {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.bytes
    }
}

impl AsRef<[u8]> for ValidatedProgram {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl<'a> Program<'a> {
    pub fn binding(&self, index: usize) -> Option<Binding<'a>> {
        if index >= self.binding_count as usize {
            return None;
        }
        let base = index.checked_mul(24)?;
        let kind = match read_u32(self.bindings, base)? {
            1 => BindingKind::Parameter,
            2 => BindingKind::Capture,
            3 => BindingKind::Let,
            _ => return None,
        };
        let defining_statement = match read_u32(self.bindings, base.checked_add(12)?)? {
            u32::MAX => None,
            value => Some(value),
        };
        Some(Binding {
            kind,
            ordinal: read_u32(self.bindings, base.checked_add(4)?)?,
            name: self.binding_name(index)?,
            defining_statement,
        })
    }

    pub fn statement(&self, index: usize) -> Option<Statement> {
        if index >= self.statement_count as usize {
            return None;
        }
        let base = index.checked_mul(16)?;
        let kind = match read_u32(self.statements, base)? {
            1 => StatementKind::Expression,
            2 => StatementKind::Let,
            _ => return None,
        };
        let binding_index = match read_u32(self.statements, base.checked_add(8)?)? {
            u32::MAX => None,
            value => Some(value),
        };
        Some(Statement {
            kind,
            root_node: read_u32(self.statements, base.checked_add(4)?)?,
            binding_index,
        })
    }

    pub fn binding_name(&self, index: usize) -> Option<&'a [u8]> {
        if index >= self.binding_count as usize {
            return None;
        }
        let mut start = 0usize;
        for current in 0..=index {
            let base = current.checked_mul(24)?;
            let length = read_u32(self.bindings, base.checked_add(8)?)? as usize;
            let end = start.checked_add(length)?;
            let name = self.bytes.get(start..end)?;
            if current == index {
                return Some(name);
            }
            start = end;
        }
        None
    }

    pub fn node(&self, index: usize) -> Option<Node<'a>> {
        if index >= self.node_count as usize {
            return None;
        }
        let base = index.checked_mul(40)?;
        let mut data_start = self.binding_names_len()?;
        for current in 0..index {
            data_start = data_start
                .checked_add(
                    read_u32(self.nodes, current.checked_mul(40)?.checked_add(12)?)? as usize,
                )?;
        }
        let data_end =
            data_start.checked_add(read_u32(self.nodes, base.checked_add(12)?)? as usize)?;
        Some(Node {
            kind: read_u32(self.nodes, base)?,
            operand_count: read_u32(self.nodes, base.checked_add(8)?)?,
            payload: read_u64(self.nodes, base.checked_add(16)?)?,
            auxiliary_0: read_u32(self.nodes, base.checked_add(24)?)?,
            auxiliary_1: read_u32(self.nodes, base.checked_add(28)?)?,
            data: self.bytes.get(data_start..data_end)?,
        })
    }

    pub fn node_operand(&self, node_index: usize, operand_index: usize) -> Option<u32> {
        if node_index >= self.node_count as usize {
            return None;
        }
        let mut operand_start = 0usize;
        for current in 0..node_index {
            operand_start = operand_start
                .checked_add(
                    read_u32(self.nodes, current.checked_mul(40)?.checked_add(8)?)? as usize,
                )?;
        }
        let base = node_index.checked_mul(40)?;
        let count = read_u32(self.nodes, base.checked_add(8)?)? as usize;
        if operand_index >= count {
            return None;
        }
        read_u32(
            self.operands,
            operand_start.checked_add(operand_index)?.checked_mul(4)?,
        )
    }

    fn binding_names_len(&self) -> Option<usize> {
        let mut total = 0usize;
        for index in 0..self.binding_count as usize {
            total = total
                .checked_add(
                    read_u32(self.bindings, index.checked_mul(24)?.checked_add(8)?)? as usize,
                )?;
        }
        Some(total)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Format,
    Name,
    Quantity,
}

const DOMAIN: &[u8] = b"birnam.portable-program.v1\0";
const PREFIX_BYTES: usize = 64;
const DIRECTORY_BYTES: usize = 32;
const SECTION_COUNT: usize = 6;
const METADATA_BYTES: usize = 32;
const EMPTY_BYTES: usize = PREFIX_BYTES + SECTION_COUNT * DIRECTORY_BYTES + METADATA_BYTES;

pub fn encode(spec: ProgramSpec<'_>) -> Result<Vec<u8>, Error> {
    if spec.statements.len() > 262_144 || spec.nodes.len() > 1_048_576 {
        return Err(Error::Quantity);
    }
    for (statement_index, statement) in spec.statements.iter().enumerate() {
        let root_node = match statement {
            StatementSpec::Expression { root_node } => *root_node,
            StatementSpec::Let {
                root_node,
                let_index,
            } => {
                let binding = spec.lets.get(*let_index as usize).ok_or(Error::Format)?;
                if binding.defining_statement as usize != statement_index {
                    return Err(Error::Format);
                }
                *root_node
            }
        };
        if root_node as usize >= spec.nodes.len() {
            return Err(Error::Format);
        }
    }
    if spec.parameters.len() > 4096 || spec.captures.len() > 4096 {
        return Err(Error::Quantity);
    }
    let binding_count = spec
        .parameters
        .len()
        .checked_add(spec.captures.len())
        .and_then(|count| count.checked_add(spec.lets.len()))
        .ok_or(Error::Quantity)?;
    if binding_count > 262_144 {
        return Err(Error::Quantity);
    }
    let mut bytes_count = 0usize;
    for name in spec
        .parameters
        .iter()
        .chain(spec.captures.iter())
        .chain(spec.lets.iter().map(|binding| &binding.name))
    {
        if name.is_empty() || name.contains(&0) || name.len() > 1 << 20 {
            return Err(Error::Name);
        }
        bytes_count = bytes_count.checked_add(name.len()).ok_or(Error::Quantity)?;
    }
    let mut previous_defining_statement = None;
    for (let_index, binding) in spec.lets.iter().enumerate() {
        let statement = spec
            .statements
            .get(binding.defining_statement as usize)
            .ok_or(Error::Format)?;
        if previous_defining_statement
            .is_some_and(|previous| binding.defining_statement <= previous)
            || !matches!(
                statement,
                StatementSpec::Let { let_index: actual, .. }
                    if *actual == u32::try_from(let_index).map_err(|_| Error::Quantity)?
            )
        {
            return Err(Error::Format);
        }
        previous_defining_statement = Some(binding.defining_statement);
    }
    let mut operand_count = 0usize;
    for (index, node) in spec.nodes.iter().enumerate() {
        match node {
            NodeSpec::Integer(_) | NodeSpec::FloatBits(_) => {}
            NodeSpec::String(data) => {
                if data.len() > 16 << 20 {
                    return Err(Error::Quantity);
                }
                bytes_count = bytes_count.checked_add(data.len()).ok_or(Error::Quantity)?;
            }
            NodeSpec::Symbol(name) | NodeSpec::GlobalRead(name) => {
                validate_name(name)?;
                bytes_count = bytes_count.checked_add(name.len()).ok_or(Error::Quantity)?;
            }
            NodeSpec::Nil | NodeSpec::True | NodeSpec::False => {}
            NodeSpec::LexicalRead(binding) => {
                if *binding as usize >= binding_count {
                    return Err(Error::Format);
                }
            }
            NodeSpec::Send { selector, operands } => {
                let arity = selector_arity(selector)?;
                if operands.len() != arity as usize + 1
                    || operands.len() > 4096
                    || operands.iter().any(|operand| *operand as usize >= index)
                {
                    return Err(Error::Format);
                }
                operand_count = operand_count
                    .checked_add(operands.len())
                    .ok_or(Error::Quantity)?;
                bytes_count = bytes_count
                    .checked_add(selector.len())
                    .ok_or(Error::Quantity)?;
            }
            NodeSpec::BlockLiteral { program, captures } => {
                let nested = inspect(program)?;
                if captures.len() != nested.capture_count as usize
                    || captures
                        .iter()
                        .any(|binding| *binding as usize >= binding_count)
                    || captures.len() > 4096
                {
                    return Err(Error::Format);
                }
                operand_count = operand_count
                    .checked_add(captures.len())
                    .ok_or(Error::Quantity)?;
                bytes_count = bytes_count
                    .checked_add(program.len())
                    .ok_or(Error::Quantity)?;
            }
        }
    }
    validate_spec_postorder(
        spec.statements,
        spec.nodes,
        spec.parameters.len() + spec.captures.len(),
    )?;

    let bindings_offset = EMPTY_BYTES;
    let bindings_bytes = binding_count.checked_mul(24).ok_or(Error::Quantity)?;
    let statements_offset = bindings_offset
        .checked_add(bindings_bytes)
        .ok_or(Error::Quantity)?;
    let statement_count = u32::try_from(spec.statements.len()).map_err(|_| Error::Quantity)?;
    let statements_bytes = spec
        .statements
        .len()
        .checked_mul(16)
        .ok_or(Error::Quantity)?;
    let nodes_offset = statements_offset
        .checked_add(statements_bytes)
        .ok_or(Error::Quantity)?;
    let node_count = u32::try_from(spec.nodes.len()).map_err(|_| Error::Quantity)?;
    let nodes_bytes = spec.nodes.len().checked_mul(40).ok_or(Error::Quantity)?;
    let operands_offset = nodes_offset
        .checked_add(nodes_bytes)
        .ok_or(Error::Quantity)?;
    let operands_bytes = operand_count.checked_mul(4).ok_or(Error::Quantity)?;
    let bytes_offset = align8(
        operands_offset
            .checked_add(operands_bytes)
            .ok_or(Error::Quantity)?,
    )
    .ok_or(Error::Quantity)?;
    let total_bytes = bytes_offset
        .checked_add(bytes_count)
        .ok_or(Error::Quantity)?;
    if total_bytes > 64 << 20 || bytes_count > 32 << 20 {
        return Err(Error::Quantity);
    }
    let parameter_count = u32::try_from(spec.parameters.len()).map_err(|_| Error::Quantity)?;
    let capture_count = u32::try_from(spec.captures.len()).map_err(|_| Error::Quantity)?;
    let binding_count = u32::try_from(binding_count).map_err(|_| Error::Quantity)?;
    let operand_count = u32::try_from(operand_count).map_err(|_| Error::Quantity)?;
    let bytes_count = u32::try_from(bytes_count).map_err(|_| Error::Quantity)?;

    let mut output = Vec::with_capacity(total_bytes);
    output.extend_from_slice(DOMAIN);
    output.resize(32, 0);
    push_u32(&mut output, 1);
    push_u32(&mut output, PREFIX_BYTES as u32);
    push_u64(
        &mut output,
        u64::try_from(total_bytes).map_err(|_| Error::Quantity)?,
    );
    push_u32(&mut output, 0);
    push_u32(&mut output, SECTION_COUNT as u32);
    push_u32(&mut output, DIRECTORY_BYTES as u32);
    push_u32(&mut output, 0);

    for (kind, stride, count, offset) in [
        (1, 32, 1, 0x100),
        (2, 24, binding_count, bindings_offset as u64),
        (3, 16, statement_count, statements_offset as u64),
        (4, 40, node_count, nodes_offset as u64),
        (5, 4, operand_count, operands_offset as u64),
        (6, 1, bytes_count, bytes_offset as u64),
    ] {
        push_u32(&mut output, kind);
        push_u32(&mut output, stride);
        push_u32(&mut output, count);
        push_u32(&mut output, 0);
        push_u64(&mut output, offset);
        push_u64(&mut output, u64::from(stride) * u64::from(count));
    }

    push_u32(&mut output, 1);
    push_u32(&mut output, parameter_count);
    push_u32(&mut output, capture_count);
    push_u32(&mut output, binding_count);
    push_u32(&mut output, statement_count);
    push_u32(&mut output, node_count);
    push_u32(&mut output, operand_count);
    push_u32(&mut output, 0);

    for (kind, names) in [(1u32, spec.parameters), (2u32, spec.captures)] {
        for (ordinal, name) in names.iter().enumerate() {
            push_u32(&mut output, kind);
            push_u32(
                &mut output,
                u32::try_from(ordinal).map_err(|_| Error::Quantity)?,
            );
            push_u32(
                &mut output,
                u32::try_from(name.len()).map_err(|_| Error::Quantity)?,
            );
            push_u32(&mut output, u32::MAX);
            push_u64(&mut output, 0);
        }
    }
    for (ordinal, binding) in spec.lets.iter().enumerate() {
        push_u32(&mut output, 3);
        push_u32(
            &mut output,
            u32::try_from(ordinal).map_err(|_| Error::Quantity)?,
        );
        push_u32(
            &mut output,
            u32::try_from(binding.name.len()).map_err(|_| Error::Quantity)?,
        );
        push_u32(&mut output, binding.defining_statement);
        push_u64(&mut output, 0);
    }
    for statement in spec.statements {
        let (kind, root_node, binding_index) = match statement {
            StatementSpec::Expression { root_node } => (1, *root_node, u32::MAX),
            StatementSpec::Let {
                root_node,
                let_index,
            } => (
                2,
                *root_node,
                parameter_count
                    .checked_add(capture_count)
                    .and_then(|prefix| prefix.checked_add(*let_index))
                    .ok_or(Error::Quantity)?,
            ),
        };
        push_u32(&mut output, kind);
        push_u32(&mut output, root_node);
        push_u32(&mut output, binding_index);
        push_u32(&mut output, 0);
    }
    for node in spec.nodes {
        let (kind, node_operand_count, data_len, payload, auxiliary_0) = match node {
            NodeSpec::Integer(value) => (1, 0, 0, *value as u64, 0),
            NodeSpec::FloatBits(bits) => (2, 0, 0, *bits, 0),
            NodeSpec::String(data) => (
                3,
                0,
                u32::try_from(data.len()).map_err(|_| Error::Quantity)?,
                0,
                0,
            ),
            NodeSpec::Symbol(name) => (
                4,
                0,
                u32::try_from(name.len()).map_err(|_| Error::Quantity)?,
                0,
                0,
            ),
            NodeSpec::Nil => (5, 0, 0, 0, 0),
            NodeSpec::True => (6, 0, 0, 0, 0),
            NodeSpec::False => (7, 0, 0, 0, 0),
            NodeSpec::LexicalRead(binding) => (8, 0, 0, 0, *binding),
            NodeSpec::GlobalRead(name) => (
                9,
                0,
                u32::try_from(name.len()).map_err(|_| Error::Quantity)?,
                0,
                0,
            ),
            NodeSpec::Send { selector, operands } => (
                10,
                u32::try_from(operands.len()).map_err(|_| Error::Quantity)?,
                u32::try_from(selector.len()).map_err(|_| Error::Quantity)?,
                0,
                selector_arity(selector)?,
            ),
            NodeSpec::BlockLiteral { program, captures } => {
                let nested = inspect(program)?;
                (
                    11,
                    u32::try_from(captures.len()).map_err(|_| Error::Quantity)?,
                    u32::try_from(program.len()).map_err(|_| Error::Quantity)?,
                    0,
                    nested.parameter_count,
                )
            }
        };
        push_u32(&mut output, kind);
        push_u32(&mut output, 0);
        push_u32(&mut output, node_operand_count);
        push_u32(&mut output, data_len);
        push_u64(&mut output, payload);
        push_u32(&mut output, auxiliary_0);
        let auxiliary_1 = match node {
            NodeSpec::BlockLiteral { program, .. } => inspect(program)?.capture_count,
            _ => 0,
        };
        push_u32(&mut output, auxiliary_1);
        push_u64(&mut output, 0);
    }
    for node in spec.nodes {
        if let NodeSpec::Send { operands, .. }
        | NodeSpec::BlockLiteral {
            captures: operands, ..
        } = node
        {
            for operand in *operands {
                push_u32(&mut output, *operand);
            }
        }
    }
    output.resize(bytes_offset, 0);
    for name in spec
        .parameters
        .iter()
        .chain(spec.captures.iter())
        .chain(spec.lets.iter().map(|binding| &binding.name))
    {
        output.extend_from_slice(name);
    }
    for node in spec.nodes {
        match node {
            NodeSpec::String(data) | NodeSpec::Symbol(data) | NodeSpec::GlobalRead(data) => {
                output.extend_from_slice(data)
            }
            NodeSpec::Send { selector, .. } => output.extend_from_slice(selector),
            NodeSpec::BlockLiteral { program, .. } => output.extend_from_slice(program),
            _ => {}
        }
    }
    debug_assert_eq!(output.len(), total_bytes);
    Ok(output)
}

pub fn selector_arity(selector: &[u8]) -> Result<u32, Error> {
    validate_name(selector)?;
    let colons = selector.iter().filter(|byte| **byte == b':').count();
    if colons != 0 {
        return u32::try_from(colons).map_err(|_| Error::Quantity);
    }
    Ok(
        if selector[0].is_ascii_alphabetic() || selector[0] == b'_' {
            0
        } else {
            1
        },
    )
}

fn validate_name(name: &[u8]) -> Result<(), Error> {
    if name.is_empty() || name.contains(&0) || name.len() > 1 << 20 {
        return Err(Error::Name);
    }
    Ok(())
}

fn validate_spec_postorder(
    statements: &[StatementSpec],
    nodes: &[NodeSpec<'_>],
    parameter_capture_count: usize,
) -> Result<(), Error> {
    let mut expected = 0usize;
    let mut completed_lets = 0usize;
    let mut stack = Vec::new();
    for statement in statements {
        let root_node = match statement {
            StatementSpec::Expression { root_node } | StatementSpec::Let { root_node, .. } => {
                *root_node
            }
        };
        stack.push((root_node, false, 1usize));
        while let Some((node_index, expanded, depth)) = stack.pop() {
            if depth > 4096 {
                return Err(Error::Quantity);
            }
            let index = node_index as usize;
            let node = nodes.get(index).ok_or(Error::Format)?;
            if let NodeSpec::LexicalRead(binding) = node
                && *binding as usize >= parameter_capture_count + completed_lets
            {
                return Err(Error::Format);
            }
            if let NodeSpec::BlockLiteral { captures, .. } = node
                && captures
                    .iter()
                    .any(|binding| *binding as usize >= parameter_capture_count + completed_lets)
            {
                return Err(Error::Format);
            }
            if expanded {
                if index != expected {
                    return Err(Error::Format);
                }
                expected = expected.checked_add(1).ok_or(Error::Quantity)?;
                continue;
            }
            stack.push((node_index, true, depth));
            if let NodeSpec::Send { operands, .. } = node {
                for operand in operands.iter().rev() {
                    stack.push((*operand, false, depth + 1));
                }
            }
        }
        if matches!(statement, StatementSpec::Let { .. }) {
            completed_lets = completed_lets.checked_add(1).ok_or(Error::Quantity)?;
        }
    }
    if expected != nodes.len() {
        return Err(Error::Format);
    }
    Ok(())
}

pub fn inspect(bytes: &[u8]) -> Result<Program<'_>, Error> {
    #[cfg(test)]
    tests::INSPECTION_COUNT.with(|count| count.set(count.get() + 1));
    let mut aggregate_bytes = 0usize;
    let mut root = None;
    let mut stack = vec![(bytes, 1usize)];
    while let Some((current, depth)) = stack.pop() {
        if depth > 4096 {
            return Err(Error::Quantity);
        }
        aggregate_bytes = aggregate_bytes
            .checked_add(current.len())
            .ok_or(Error::Quantity)?;
        if aggregate_bytes > 64 << 20 {
            return Err(Error::Quantity);
        }
        let (program, nested_programs) = inspect_one(current)?;
        if root.is_none() {
            root = Some(program);
        }
        for nested in nested_programs.into_iter().rev() {
            stack.push((nested, depth + 1));
        }
    }
    root.ok_or(Error::Format)
}

fn inspect_one(bytes: &[u8]) -> Result<(Program<'_>, Vec<&[u8]>), Error> {
    if bytes.len() < EMPTY_BYTES
        || bytes.get(..DOMAIN.len()) != Some(DOMAIN)
        || bytes
            .get(DOMAIN.len()..32)
            .is_none_or(|padding| padding.iter().any(|byte| *byte != 0))
        || read_u32(bytes, 0x20) != Some(1)
        || read_u32(bytes, 0x24) != Some(PREFIX_BYTES as u32)
        || read_u64(bytes, 0x28) != u64::try_from(bytes.len()).ok()
        || read_u32(bytes, 0x30) != Some(0)
        || read_u32(bytes, 0x34) != Some(SECTION_COUNT as u32)
        || read_u32(bytes, 0x38) != Some(DIRECTORY_BYTES as u32)
        || read_u32(bytes, 0x3c) != Some(0)
    {
        return Err(Error::Format);
    }
    let expected_strides = [32u32, 24, 16, 40, 4, 1];
    let mut sections = [(0usize, 0usize); SECTION_COUNT];
    let mut cursor = 0x100usize;
    for (index, stride) in expected_strides.into_iter().enumerate() {
        let base = PREFIX_BYTES + index * DIRECTORY_BYTES;
        let count = read_u32(bytes, base + 8).ok_or(Error::Format)?;
        let length = usize::try_from(count)
            .ok()
            .and_then(|value| value.checked_mul(stride as usize))
            .ok_or(Error::Format)?;
        let aligned_cursor = align8(cursor).ok_or(Error::Format)?;
        if bytes
            .get(cursor..aligned_cursor)
            .is_none_or(|padding| padding.iter().any(|byte| *byte != 0))
        {
            return Err(Error::Format);
        }
        cursor = aligned_cursor;
        if read_u32(bytes, base) != u32::try_from(index + 1).ok()
            || read_u32(bytes, base + 4) != Some(stride)
            || read_u32(bytes, base + 12) != Some(0)
            || read_u64(bytes, base + 16) != u64::try_from(cursor).ok()
            || read_u64(bytes, base + 24) != u64::try_from(length).ok()
        {
            return Err(Error::Format);
        }
        let end = cursor.checked_add(length).ok_or(Error::Format)?;
        if end > bytes.len() {
            return Err(Error::Format);
        }
        sections[index] = (cursor, end);
        cursor = end;
    }
    if cursor != bytes.len() {
        return Err(Error::Format);
    }
    let metadata = bytes
        .get(sections[0].0..sections[0].1)
        .filter(|value| value.len() == 32)
        .ok_or(Error::Format)?;
    let parameter_count = read_u32(metadata, 4).ok_or(Error::Format)?;
    let capture_count = read_u32(metadata, 8).ok_or(Error::Format)?;
    let binding_count = read_u32(metadata, 12).ok_or(Error::Format)?;
    let statement_count = read_u32(metadata, 16).ok_or(Error::Format)?;
    let node_count = read_u32(metadata, 20).ok_or(Error::Format)?;
    let operand_count = read_u32(metadata, 24).ok_or(Error::Format)?;
    if read_u32(metadata, 0) != Some(1)
        || read_u32(metadata, 28) != Some(0)
        || read_u32(bytes, PREFIX_BYTES + DIRECTORY_BYTES + 8) != Some(binding_count)
        || read_u32(bytes, PREFIX_BYTES + 2 * DIRECTORY_BYTES + 8) != Some(statement_count)
        || read_u32(bytes, PREFIX_BYTES + 3 * DIRECTORY_BYTES + 8) != Some(node_count)
        || read_u32(bytes, PREFIX_BYTES + 4 * DIRECTORY_BYTES + 8) != Some(operand_count)
        || read_u32(bytes, PREFIX_BYTES + 5 * DIRECTORY_BYTES + 8)
            .is_none_or(|bytes_count| bytes_count > 32 << 20)
        || parameter_count
            .checked_add(capture_count)
            .is_none_or(|prefix| prefix > binding_count)
        || parameter_count > 4096
        || capture_count > 4096
        || binding_count > 262_144
        || statement_count > 262_144
        || node_count > 1_048_576
    {
        return Err(Error::Format);
    }
    let bindings = bytes
        .get(sections[1].0..sections[1].1)
        .ok_or(Error::Format)?;
    let names = bytes
        .get(sections[5].0..sections[5].1)
        .ok_or(Error::Format)?;
    let mut names_cursor = 0usize;
    let parameter_capture_count = parameter_count
        .checked_add(capture_count)
        .ok_or(Error::Format)?;
    let let_count = binding_count - parameter_capture_count;
    let mut previous_defining_statement = None;
    for index in 0..binding_count as usize {
        let base = index.checked_mul(24).ok_or(Error::Format)?;
        let expected_kind = if index < parameter_count as usize {
            1
        } else if index < parameter_capture_count as usize {
            2
        } else {
            3
        };
        let expected_ordinal = match expected_kind {
            1 => index,
            2 => index - parameter_count as usize,
            _ => index - parameter_capture_count as usize,
        };
        let defining_statement = read_u32(bindings, base + 12).ok_or(Error::Format)?;
        let name_length = read_u32(bindings, base + 8).ok_or(Error::Format)? as usize;
        let name_end = names_cursor.checked_add(name_length).ok_or(Error::Format)?;
        let name = names.get(names_cursor..name_end).ok_or(Error::Format)?;
        if read_u32(bindings, base) != Some(expected_kind)
            || read_u32(bindings, base + 4) != u32::try_from(expected_ordinal).ok()
            || read_u64(bindings, base + 16) != Some(0)
            || name.is_empty()
            || name.contains(&0)
            || (expected_kind != 3 && defining_statement != u32::MAX)
            || (expected_kind == 3
                && (defining_statement >= statement_count
                    || previous_defining_statement
                        .is_some_and(|previous| defining_statement <= previous)))
        {
            return Err(Error::Format);
        }
        if expected_kind == 3 {
            previous_defining_statement = Some(defining_statement);
        }
        names_cursor = name_end;
    }
    let statements = bytes
        .get(sections[2].0..sections[2].1)
        .ok_or(Error::Format)?;
    let mut next_let = 0u32;
    for index in 0..statement_count as usize {
        let base = index.checked_mul(16).ok_or(Error::Format)?;
        let kind = read_u32(statements, base).ok_or(Error::Format)?;
        let binding_index = read_u32(statements, base + 8).ok_or(Error::Format)?;
        let expected_binding = parameter_capture_count
            .checked_add(next_let)
            .ok_or(Error::Format)?;
        if !matches!(kind, 1 | 2)
            || read_u32(statements, base + 4).is_none_or(|root| root >= node_count)
            || (kind == 1 && binding_index != u32::MAX)
            || (kind == 2
                && (binding_index != expected_binding
                    || read_u32(bindings, binding_index as usize * 24 + 12)
                        != u32::try_from(index).ok()))
            || read_u32(statements, base + 12) != Some(0)
        {
            return Err(Error::Format);
        }
        if kind == 2 {
            next_let = next_let.checked_add(1).ok_or(Error::Format)?;
        }
    }
    if next_let != let_count {
        return Err(Error::Format);
    }
    let nodes = bytes
        .get(sections[3].0..sections[3].1)
        .ok_or(Error::Format)?;
    let operands = bytes
        .get(sections[4].0..sections[4].1)
        .ok_or(Error::Format)?;
    let mut operand_starts = Vec::with_capacity(node_count as usize);
    let mut nested_programs = Vec::new();
    let mut operand_cursor = 0usize;
    let mut data_cursor = names_cursor;
    for index in 0..node_count as usize {
        let base = index.checked_mul(40).ok_or(Error::Format)?;
        let kind = read_u32(nodes, base).ok_or(Error::Format)?;
        let node_operand_count = read_u32(nodes, base + 8).ok_or(Error::Format)? as usize;
        let data_length = read_u32(nodes, base + 12).ok_or(Error::Format)? as usize;
        let payload = read_u64(nodes, base + 16).ok_or(Error::Format)?;
        let auxiliary_0 = read_u32(nodes, base + 24).ok_or(Error::Format)?;
        let auxiliary_1 = read_u32(nodes, base + 28).ok_or(Error::Format)?;
        if read_u32(nodes, base + 4) != Some(0) || read_u64(nodes, base + 32) != Some(0) {
            return Err(Error::Format);
        }
        operand_starts.push(operand_cursor);
        let operand_end = operand_cursor
            .checked_add(node_operand_count)
            .ok_or(Error::Format)?;
        if operand_end > operand_count as usize {
            return Err(Error::Format);
        }
        let data_end = data_cursor.checked_add(data_length).ok_or(Error::Format)?;
        let data = names.get(data_cursor..data_end).ok_or(Error::Format)?;
        match kind {
            1 | 2 => {
                if node_operand_count != 0
                    || data_length != 0
                    || auxiliary_0 != 0
                    || auxiliary_1 != 0
                {
                    return Err(Error::Format);
                }
            }
            3 => {
                if node_operand_count != 0
                    || data_length > 16 << 20
                    || payload != 0
                    || auxiliary_0 != 0
                    || auxiliary_1 != 0
                {
                    return Err(Error::Format);
                }
            }
            4 | 9 => {
                if node_operand_count != 0
                    || validate_name(data).is_err()
                    || payload != 0
                    || auxiliary_0 != 0
                    || auxiliary_1 != 0
                {
                    return Err(Error::Format);
                }
            }
            5..=7 => {
                if node_operand_count != 0
                    || data_length != 0
                    || payload != 0
                    || auxiliary_0 != 0
                    || auxiliary_1 != 0
                {
                    return Err(Error::Format);
                }
            }
            8 => {
                if node_operand_count != 0
                    || data_length != 0
                    || payload != 0
                    || auxiliary_0 >= binding_count
                    || auxiliary_1 != 0
                {
                    return Err(Error::Format);
                }
            }
            10 => {
                let arity = selector_arity(data).map_err(|_| Error::Format)?;
                if payload != 0
                    || auxiliary_0 != arity
                    || auxiliary_1 != 0
                    || node_operand_count != arity as usize + 1
                    || node_operand_count > 4096
                {
                    return Err(Error::Format);
                }
                for operand_index in operand_cursor..operand_end {
                    let child = read_u32(operands, operand_index * 4).ok_or(Error::Format)?;
                    if child as usize >= index {
                        return Err(Error::Format);
                    }
                }
            }
            11 => {
                let (nested_parameter_count, nested_capture_count) = peek_metadata(data)?;
                if payload != 0
                    || auxiliary_0 != nested_parameter_count
                    || auxiliary_1 != nested_capture_count
                    || node_operand_count != nested_capture_count as usize
                    || node_operand_count > 4096
                {
                    return Err(Error::Format);
                }
                for operand_index in operand_cursor..operand_end {
                    let binding = read_u32(operands, operand_index * 4).ok_or(Error::Format)?;
                    if binding >= binding_count {
                        return Err(Error::Format);
                    }
                }
                nested_programs.push(data);
            }
            _ => return Err(Error::Format),
        }
        operand_cursor = operand_end;
        data_cursor = data_end;
    }
    if operand_cursor != operand_count as usize || data_cursor != names.len() {
        return Err(Error::Format);
    }
    validate_encoded_postorder(
        statements,
        nodes,
        operands,
        &operand_starts,
        parameter_capture_count,
    )?;
    Ok((
        Program {
            parameter_count,
            capture_count,
            binding_count,
            statement_count,
            node_count,
            operand_count,
            bindings,
            statements,
            nodes,
            operands,
            bytes: names,
        },
        nested_programs,
    ))
}

fn peek_metadata(bytes: &[u8]) -> Result<(u32, u32), Error> {
    if bytes.len() < EMPTY_BYTES || read_u32(bytes, 0x100) != Some(1) {
        return Err(Error::Format);
    }
    Ok((
        read_u32(bytes, 0x104).ok_or(Error::Format)?,
        read_u32(bytes, 0x108).ok_or(Error::Format)?,
    ))
}

fn validate_encoded_postorder(
    statements: &[u8],
    nodes: &[u8],
    operands: &[u8],
    operand_starts: &[usize],
    parameter_capture_count: u32,
) -> Result<(), Error> {
    let statement_count = statements.len() / 16;
    let node_count = nodes.len() / 40;
    let mut expected = 0usize;
    let mut completed_lets = 0u32;
    let mut stack = Vec::new();
    for statement_index in 0..statement_count {
        let root = read_u32(statements, statement_index * 16 + 4).ok_or(Error::Format)?;
        stack.push((root, false, 1usize));
        while let Some((node_index, expanded, depth)) = stack.pop() {
            if depth > 4096 {
                return Err(Error::Format);
            }
            let index = node_index as usize;
            if index >= node_count {
                return Err(Error::Format);
            }
            let base = index.checked_mul(40).ok_or(Error::Format)?;
            if read_u32(nodes, base) == Some(8)
                && read_u32(nodes, base + 24).is_none_or(|binding| {
                    parameter_capture_count
                        .checked_add(completed_lets)
                        .is_none_or(|in_scope| binding >= in_scope)
                })
            {
                return Err(Error::Format);
            }
            if read_u32(nodes, base) == Some(11) {
                let count = read_u32(nodes, base + 8).ok_or(Error::Format)? as usize;
                let start = *operand_starts.get(index).ok_or(Error::Format)?;
                let in_scope = parameter_capture_count
                    .checked_add(completed_lets)
                    .ok_or(Error::Format)?;
                for operand_index in start..start + count {
                    if read_u32(operands, operand_index * 4)
                        .is_none_or(|binding| binding >= in_scope)
                    {
                        return Err(Error::Format);
                    }
                }
            }
            if expanded {
                if index != expected {
                    return Err(Error::Format);
                }
                expected = expected.checked_add(1).ok_or(Error::Format)?;
                continue;
            }
            stack.push((node_index, true, depth));
            if read_u32(nodes, base) == Some(10) {
                let count = read_u32(nodes, base + 8).ok_or(Error::Format)? as usize;
                let start = *operand_starts.get(index).ok_or(Error::Format)?;
                for operand_index in (start..start + count).rev() {
                    let child = read_u32(operands, operand_index * 4).ok_or(Error::Format)?;
                    stack.push((child, false, depth + 1));
                }
            }
        }
        if read_u32(statements, statement_index * 16) == Some(2) {
            completed_lets = completed_lets.checked_add(1).ok_or(Error::Format)?;
        }
    }
    if expected != node_count {
        return Err(Error::Format);
    }
    Ok(())
}

use super::wire::{push_u32, push_u64, read_u32_checked as read_u32, read_u64_checked as read_u64};

fn align8(value: usize) -> Option<usize> {
    value.checked_add(7).map(|aligned| aligned & !7)
}

#[cfg(test)]
mod tests {
    thread_local! {
        pub(super) static INSPECTION_COUNT: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    }

    #[test]
    fn owned_program_views_preserve_frozen_meaning_after_moves_and_source_changes() {
        let fixtures: &[&[u8]] = &[
            include_bytes!("../../../abi/fixtures/portable-code-v1/program-empty.bin"),
            include_bytes!("../../../abi/fixtures/portable-code-v1/program-send.bin"),
            include_bytes!("../../../abi/fixtures/portable-code-v1/program-nested-captures.bin"),
            include_bytes!("../../../abi/fixtures/portable-code-v1/program-numeric-boundaries.bin"),
            include_bytes!("../../../abi/fixtures/portable-code-v1/program-explicit-captures.bin"),
        ];
        let mut owners = Vec::new();
        for fixture in fixtures {
            let mut source = fixture.to_vec();
            owners.push(super::ValidatedProgram::copy_from_slice(&source).unwrap());
            source.fill(0);
            source.clear();
        }
        // Moving/reallocating owners cannot invalidate their section offsets.
        owners.reserve(100);
        for (owner, fixture) in owners.iter().zip(fixtures) {
            let expected = super::inspect(fixture).unwrap();
            let before = INSPECTION_COUNT.with(|count| count.get());
            for _ in 0..100 {
                assert_eq!(owner.view(), expected);
                assert_eq!(owner.as_ref(), *fixture);
            }
            assert_eq!(INSPECTION_COUNT.with(|count| count.get()), before);
        }
    }

    #[test]
    fn owned_program_admission_preserves_validation_errors() {
        let fixture =
            include_bytes!("../../../abi/fixtures/portable-code-v1/program-nested-captures.bin");
        for length in 0..fixture.len() {
            let bytes = &fixture[..length];
            assert_eq!(
                super::ValidatedProgram::copy_from_slice(bytes).unwrap_err(),
                super::inspect(bytes).unwrap_err(),
            );
        }
        // Exercise both foreign-copy and transferred-Vec constructors on
        // corruption throughout the root and nested program, not just headers.
        for index in 0..fixture.len() {
            let mut bytes = fixture.to_vec();
            bytes[index] ^= 0x80;
            match super::inspect(&bytes) {
                Ok(expected) => {
                    assert_eq!(
                        super::ValidatedProgram::copy_from_slice(&bytes)
                            .unwrap()
                            .view(),
                        expected
                    );
                    assert_eq!(
                        super::ValidatedProgram::new(bytes.clone()).unwrap().view(),
                        expected
                    );
                }
                Err(expected) => {
                    assert_eq!(
                        super::ValidatedProgram::copy_from_slice(&bytes).unwrap_err(),
                        expected
                    );
                    assert_eq!(super::ValidatedProgram::new(bytes).unwrap_err(), expected);
                }
            }
        }
    }
    use sha2::{Digest, Sha256};

    use super::{ProgramSpec, encode, inspect};

    #[test]
    fn empty_program_has_the_frozen_288_byte_encoding() {
        let bytes = encode(ProgramSpec {
            parameters: &[],
            captures: &[],
            lets: &[],
            statements: &[],
            nodes: &[],
        })
        .unwrap();

        assert_eq!(bytes.len(), 288);
        assert_eq!(
            <[u8; 32]>::from(Sha256::digest(&bytes)),
            [
                0x7b, 0xe9, 0x19, 0x40, 0x96, 0xc7, 0x26, 0x73, 0x76, 0xd4, 0xf7, 0x00, 0x67, 0x72,
                0x5f, 0x20, 0x23, 0x0a, 0x45, 0x68, 0xc3, 0xcb, 0x8c, 0xae, 0x4b, 0x6f, 0x48, 0x1d,
                0x99, 0xb8, 0xc8, 0xfb,
            ]
        );

        let program = inspect(&bytes).unwrap();
        assert_eq!(program.parameter_count, 0);
        assert_eq!(program.capture_count, 0);
        assert_eq!(program.binding_count, 0);
        assert_eq!(program.statement_count, 0);
        assert_eq!(program.node_count, 0);
        assert_eq!(program.operand_count, 0);
    }

    #[test]
    fn explicit_unread_capture_schema_is_preserved_in_order() {
        let bytes = encode(ProgramSpec {
            parameters: &[b"x"],
            captures: &[b"held", b"held"],
            lets: &[],
            statements: &[],
            nodes: &[],
        })
        .unwrap();

        assert_eq!(bytes.len(), 369);
        assert_eq!(
            <[u8; 32]>::from(Sha256::digest(&bytes)),
            [
                0xd6, 0xf1, 0xeb, 0x00, 0x18, 0xb9, 0xe3, 0xe7, 0x4f, 0xf5, 0xf6, 0x81, 0x93, 0xcb,
                0x33, 0x3e, 0xe7, 0xa2, 0x91, 0x70, 0x8b, 0xac, 0x6b, 0xd0, 0xfd, 0x8a, 0x52, 0xeb,
                0x6d, 0x0a, 0xf9, 0x70,
            ]
        );

        let program = inspect(&bytes).unwrap();
        assert_eq!(program.parameter_count, 1);
        assert_eq!(program.capture_count, 2);
        assert_eq!(program.binding_count, 3);
        assert_eq!(program.binding_name(0), Some(b"x".as_slice()));
        assert_eq!(program.binding_name(1), Some(b"held".as_slice()));
        assert_eq!(program.binding_name(2), Some(b"held".as_slice()));
    }

    #[test]
    fn signalling_nan_literal_preserves_its_exact_bits() {
        let bytes = encode(ProgramSpec {
            parameters: &[],
            captures: &[],
            lets: &[],
            statements: &[super::StatementSpec::Expression { root_node: 0 }],
            nodes: &[super::NodeSpec::FloatBits(0x7ff0_0000_0000_0042)],
        })
        .unwrap();

        assert_eq!(bytes.len(), 344);
        assert_eq!(
            <[u8; 32]>::from(Sha256::digest(&bytes)),
            [
                0x95, 0x46, 0xc7, 0xf0, 0xf1, 0x82, 0xd8, 0x86, 0xa3, 0xc7, 0x5e, 0x55, 0x18, 0xf4,
                0x3d, 0x39, 0x6d, 0xe9, 0x89, 0x7d, 0x3c, 0xa1, 0x49, 0x5d, 0x94, 0x33, 0xac, 0xd2,
                0x6b, 0xba, 0xe7, 0xad,
            ]
        );
        let program = inspect(&bytes).unwrap();
        assert_eq!(program.statement_count, 1);
        assert_eq!(program.node_count, 1);
        let node = program.node(0).unwrap();
        assert_eq!(node.kind, 2);
        assert_eq!(node.payload, 0x7ff0_0000_0000_0042);
    }

    #[test]
    fn send_uses_left_to_right_postorder_operands_and_derived_arity() {
        let bytes = encode(ProgramSpec {
            parameters: &[],
            captures: &[],
            lets: &[],
            statements: &[super::StatementSpec::Expression { root_node: 2 }],
            nodes: &[
                super::NodeSpec::Integer(41),
                super::NodeSpec::Integer(1),
                super::NodeSpec::Send {
                    selector: b"+",
                    operands: &[0, 1],
                },
            ],
        })
        .unwrap();

        assert_eq!(bytes.len(), 433);
        assert_eq!(
            <[u8; 32]>::from(Sha256::digest(&bytes)),
            [
                0xee, 0x65, 0x2c, 0x1f, 0x94, 0x92, 0x75, 0xfb, 0xe5, 0xb9, 0x96, 0x85, 0xae, 0x95,
                0x73, 0x99, 0xcd, 0x24, 0x2d, 0x6a, 0x5d, 0xb1, 0xe4, 0x44, 0x8c, 0x77, 0x97, 0xfd,
                0x45, 0xb4, 0x93, 0x45,
            ]
        );
        let program = inspect(&bytes).unwrap();
        assert_eq!(program.statement_count, 1);
        assert_eq!(program.node_count, 3);
        assert_eq!(program.operand_count, 2);
        let node = program.node(2).unwrap();
        assert_eq!(node.kind, 10);
        assert_eq!(node.auxiliary_0, 1);
        assert_eq!(node.data, b"+");
    }

    #[test]
    fn let_binding_enters_scope_after_its_defining_statement() {
        let bytes = encode(ProgramSpec {
            parameters: &[b"x"],
            captures: &[b"held"],
            lets: &[super::LetSpec {
                name: b"y",
                defining_statement: 0,
            }],
            statements: &[
                super::StatementSpec::Let {
                    root_node: 0,
                    let_index: 0,
                },
                super::StatementSpec::Expression { root_node: 1 },
            ],
            nodes: &[super::NodeSpec::Integer(7), super::NodeSpec::LexicalRead(2)],
        })
        .unwrap();

        assert_eq!(bytes.len(), 478);
        assert_eq!(
            <[u8; 32]>::from(Sha256::digest(&bytes)),
            [
                0x17, 0x15, 0xf0, 0x85, 0xa5, 0x8a, 0xdb, 0x4a, 0x76, 0xa1, 0x59, 0x9f, 0x7e, 0x29,
                0xd3, 0x27, 0x30, 0xd2, 0x51, 0x6d, 0x2e, 0x46, 0x14, 0x70, 0x90, 0x7d, 0xb1, 0x23,
                0xa2, 0x2d, 0xa9, 0xfa,
            ]
        );
        let program = inspect(&bytes).unwrap();
        assert_eq!(program.binding_count, 3);
        assert_eq!(program.binding_name(2), Some(b"y".as_slice()));
        assert_eq!(program.node(1).unwrap().auxiliary_0, 2);
    }

    #[test]
    fn auxiliary_values_preserve_opaque_and_empty_bytes() {
        let bytes = encode(ProgramSpec {
            parameters: &[],
            captures: &[],
            lets: &[],
            statements: &[
                super::StatementSpec::Expression { root_node: 0 },
                super::StatementSpec::Expression { root_node: 1 },
                super::StatementSpec::Expression { root_node: 2 },
                super::StatementSpec::Expression { root_node: 3 },
                super::StatementSpec::Expression { root_node: 4 },
                super::StatementSpec::Expression { root_node: 5 },
                super::StatementSpec::Expression { root_node: 6 },
            ],
            nodes: &[
                super::NodeSpec::String(b""),
                super::NodeSpec::String(b"\x00\xff"),
                super::NodeSpec::Symbol(b"\xffsym"),
                super::NodeSpec::Nil,
                super::NodeSpec::True,
                super::NodeSpec::False,
                super::NodeSpec::GlobalRead(b"\xfeG"),
            ],
        })
        .unwrap();

        assert_eq!(bytes.len(), 688);
        assert_eq!(
            <[u8; 32]>::from(Sha256::digest(&bytes)),
            [
                0x44, 0x74, 0x0a, 0xce, 0x20, 0xf4, 0xeb, 0x49, 0xe9, 0x7e, 0x36, 0x30, 0x4d, 0x19,
                0x3b, 0xf9, 0xdc, 0x51, 0x56, 0xd3, 0x68, 0xa5, 0x68, 0x78, 0x3e, 0x97, 0x5b, 0x66,
                0x12, 0xb9, 0x72, 0xaf,
            ]
        );
        let program = inspect(&bytes).unwrap();
        assert_eq!(program.node(0).unwrap().data, b"");
        assert_eq!(program.node(1).unwrap().data, b"\x00\xff");
        assert_eq!(program.node(2).unwrap().data, b"\xffsym");
        assert_eq!(program.node(6).unwrap().data, b"\xfeG");
    }

    #[test]
    fn block_literal_preserves_repeated_and_unread_capture_schema() {
        let nested = encode(ProgramSpec {
            parameters: &[],
            captures: &[b"a", b"b"],
            lets: &[],
            statements: &[],
            nodes: &[],
        })
        .unwrap();
        assert_eq!(nested.len(), 338);
        assert_eq!(
            <[u8; 32]>::from(Sha256::digest(&nested)),
            [
                0x8b, 0xf1, 0x48, 0xcf, 0xf2, 0x8f, 0x52, 0xe4, 0x99, 0xbb, 0x5d, 0xca, 0xe3, 0x58,
                0xcd, 0x4a, 0x02, 0xc0, 0xae, 0x8e, 0x04, 0x24, 0x03, 0xfa, 0x87, 0x68, 0xc0, 0x33,
                0x05, 0x37, 0xdd, 0x8e,
            ]
        );

        let bytes = encode(ProgramSpec {
            parameters: &[b"p"],
            captures: &[],
            lets: &[],
            statements: &[super::StatementSpec::Expression { root_node: 0 }],
            nodes: &[super::NodeSpec::BlockLiteral {
                program: &nested,
                captures: &[0, 0],
            }],
        })
        .unwrap();

        assert_eq!(bytes.len(), 715);
        assert_eq!(
            <[u8; 32]>::from(Sha256::digest(&bytes)),
            [
                0xa9, 0xed, 0x63, 0x53, 0x21, 0xc0, 0x4d, 0xb3, 0x7a, 0xc6, 0x80, 0xca, 0xd5, 0x5b,
                0xed, 0x8c, 0x5e, 0x06, 0x66, 0x5d, 0x4c, 0x3f, 0x89, 0xdd, 0x5b, 0x9f, 0x29, 0x7c,
                0x59, 0xf4, 0x52, 0x80,
            ]
        );
        let program = inspect(&bytes).unwrap();
        let block = program.node(0).unwrap();
        assert_eq!(block.kind, 11);
        assert_eq!(block.auxiliary_0, 0);
        assert_eq!(block.auxiliary_1, 2);
        assert_eq!(program.node_operand(0, 0), Some(0));
        assert_eq!(program.node_operand(0, 1), Some(0));
        assert_eq!(inspect(block.data).unwrap().capture_count, 2);
    }

    #[test]
    fn non_utf8_duplicate_names_and_nonempty_nested_block_match_independent_bytes() {
        let nested = encode(ProgramSpec {
            parameters: &[],
            captures: &[b"\xffdup"],
            lets: &[],
            statements: &[super::StatementSpec::Expression { root_node: 2 }],
            nodes: &[
                super::NodeSpec::LexicalRead(0),
                super::NodeSpec::Integer(1),
                super::NodeSpec::Send {
                    selector: b"\xff",
                    operands: &[0, 1],
                },
            ],
        })
        .unwrap();
        let bytes = encode(ProgramSpec {
            parameters: &[b"\xffdup"],
            captures: &[b"\xffdup"],
            lets: &[],
            statements: &[super::StatementSpec::Expression { root_node: 0 }],
            nodes: &[super::NodeSpec::BlockLiteral {
                program: &nested,
                captures: &[0],
            }],
        })
        .unwrap();
        assert_eq!(
            bytes,
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "program-nonutf8-names-block.bin"
            ))
        );
        let program = inspect(&bytes).unwrap();
        assert_eq!(program.binding_name(0), Some(b"\xffdup".as_slice()));
        assert_eq!(program.binding_name(1), Some(b"\xffdup".as_slice()));
        let nested = inspect(program.node(0).unwrap().data).unwrap();
        assert_eq!(nested.node(2).unwrap().data, b"\xff");
    }

    #[test]
    fn production_encoder_matches_the_independent_fixture_encoder() {
        let nested_one = encode(ProgramSpec {
            parameters: &[],
            captures: &[b"a"],
            lets: &[],
            statements: &[],
            nodes: &[],
        })
        .unwrap();
        let padding = encode(ProgramSpec {
            parameters: &[b"p"],
            captures: &[],
            lets: &[],
            statements: &[super::StatementSpec::Expression { root_node: 0 }],
            nodes: &[super::NodeSpec::BlockLiteral {
                program: &nested_one,
                captures: &[0],
            }],
        })
        .unwrap();
        assert_eq!(
            padding,
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "program-padding.bin"
            ))
        );

        let future_let = encode(ProgramSpec {
            parameters: &[b"p"],
            captures: &[],
            lets: &[super::LetSpec {
                name: b"y",
                defining_statement: 1,
            }],
            statements: &[
                super::StatementSpec::Expression { root_node: 0 },
                super::StatementSpec::Let {
                    root_node: 1,
                    let_index: 0,
                },
            ],
            nodes: &[super::NodeSpec::LexicalRead(0), super::NodeSpec::Integer(1)],
        })
        .unwrap();
        assert_eq!(
            future_let,
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "program-future-let.bin"
            ))
        );
    }

    #[test]
    fn encoder_rejects_future_let_reads_and_noncanonical_trees() {
        let future_read = encode(ProgramSpec {
            parameters: &[b"p"],
            captures: &[],
            lets: &[super::LetSpec {
                name: b"y",
                defining_statement: 1,
            }],
            statements: &[
                super::StatementSpec::Expression { root_node: 0 },
                super::StatementSpec::Let {
                    root_node: 1,
                    let_index: 0,
                },
            ],
            nodes: &[super::NodeSpec::LexicalRead(1), super::NodeSpec::Integer(1)],
        });
        assert_eq!(future_read, Err(super::Error::Format));

        let shared_child = encode(ProgramSpec {
            parameters: &[],
            captures: &[],
            lets: &[],
            statements: &[super::StatementSpec::Expression { root_node: 2 }],
            nodes: &[
                super::NodeSpec::Integer(1),
                super::NodeSpec::Integer(2),
                super::NodeSpec::Send {
                    selector: b"+",
                    operands: &[0, 0],
                },
            ],
        });
        assert_eq!(shared_child, Err(super::Error::Format));
    }

    #[test]
    fn numeric_boundaries_and_every_selector_arity_match_independent_bytes() {
        let numeric_statements: Vec<_> = (0..8)
            .map(|root_node| super::StatementSpec::Expression { root_node })
            .collect();
        let numeric = encode(ProgramSpec {
            parameters: &[],
            captures: &[],
            lets: &[],
            statements: &numeric_statements,
            nodes: &[
                super::NodeSpec::Integer(i64::MIN),
                super::NodeSpec::Integer(i64::MAX),
                super::NodeSpec::FloatBits(0x0000_0000_0000_0000),
                super::NodeSpec::FloatBits(0x8000_0000_0000_0000),
                super::NodeSpec::FloatBits(0x7ff0_0000_0000_0000),
                super::NodeSpec::FloatBits(0xfff0_0000_0000_0000),
                super::NodeSpec::FloatBits(0x7ff8_0000_0000_0001),
                super::NodeSpec::FloatBits(0x7ff8_0000_0000_0042),
            ],
        })
        .unwrap();
        assert_eq!(
            numeric,
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "program-numeric-boundaries.bin"
            ))
        );

        let sends = encode(ProgramSpec {
            parameters: &[],
            captures: &[],
            lets: &[],
            statements: &[
                super::StatementSpec::Expression { root_node: 1 },
                super::StatementSpec::Expression { root_node: 4 },
                super::StatementSpec::Expression { root_node: 8 },
                super::StatementSpec::Expression { root_node: 13 },
            ],
            nodes: &[
                super::NodeSpec::Integer(0),
                super::NodeSpec::Send {
                    selector: b"run",
                    operands: &[0],
                },
                super::NodeSpec::Integer(1),
                super::NodeSpec::Integer(2),
                super::NodeSpec::Send {
                    selector: b"+",
                    operands: &[2, 3],
                },
                super::NodeSpec::Integer(3),
                super::NodeSpec::Integer(4),
                super::NodeSpec::Integer(5),
                super::NodeSpec::Send {
                    selector: b"at:put:",
                    operands: &[5, 6, 7],
                },
                super::NodeSpec::Integer(6),
                super::NodeSpec::Integer(7),
                super::NodeSpec::Integer(8),
                super::NodeSpec::Integer(9),
                super::NodeSpec::Send {
                    selector: b"if:then:else:",
                    operands: &[9, 10, 11, 12],
                },
            ],
        })
        .unwrap();
        assert_eq!(
            sends,
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "program-send-arities.bin"
            ))
        );
    }

    #[test]
    fn every_stored_program_truncation_rejects_and_single_bit_mutations_do_not_panic() {
        let fixtures: &[&[u8]] = &[
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "program-auxiliary-values.bin"
            )),
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "program-empty.bin"
            )),
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "program-explicit-captures.bin"
            )),
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "program-future-let.bin"
            )),
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "program-let-scope.bin"
            )),
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "program-nested-captures.bin"
            )),
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "program-nested-inner.bin"
            )),
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "program-nonutf8-names-block.bin"
            )),
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "program-numeric-boundaries.bin"
            )),
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "program-padding.bin"
            )),
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "program-send-arities.bin"
            )),
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "program-send.bin"
            )),
            include_bytes!(concat!(
                "../../../abi/fixtures/portable-code-v1/",
                "program-signalling-nan.bin"
            )),
        ];
        for fixture in fixtures {
            for end in 0..fixture.len() {
                assert!(
                    inspect(&fixture[..end]).is_err(),
                    "accepted truncation {end} of {}",
                    fixture.len()
                );
            }
            for offset in 0..fixture.len() {
                let mut mutated = fixture.to_vec();
                mutated[offset] ^= 1;
                assert!(std::panic::catch_unwind(|| inspect(&mutated)).is_ok());
            }
        }
    }

    #[test]
    fn generated_maximum_length_vectors_match_the_frozen_manifest() {
        let one_mib = 1 << 20;
        let sixteen_mib = 16 << 20;
        {
            let parameter = vec![b'n'; one_mib];
            let string = vec![0; sixteen_mib];
            let symbol = vec![b's'; one_mib];
            let global = vec![b'g'; one_mib];
            let selector = vec![b'+'; one_mib];
            let bytes = encode(ProgramSpec {
                parameters: &[&parameter],
                captures: &[],
                lets: &[],
                statements: &[
                    super::StatementSpec::Expression { root_node: 0 },
                    super::StatementSpec::Expression { root_node: 1 },
                    super::StatementSpec::Expression { root_node: 2 },
                    super::StatementSpec::Expression { root_node: 5 },
                ],
                nodes: &[
                    super::NodeSpec::String(&string),
                    super::NodeSpec::Symbol(&symbol),
                    super::NodeSpec::GlobalRead(&global),
                    super::NodeSpec::Integer(0),
                    super::NodeSpec::Integer(1),
                    super::NodeSpec::Send {
                        selector: &selector,
                        operands: &[3, 4],
                    },
                ],
            })
            .unwrap();
            assert_eq!(bytes.len(), 20_972_144);
            assert_eq!(
                digest_hex(&bytes),
                "cf19e42057b5f41c4df5aff50eeb10dead2f67a1d4362bd2a4b2715a5c076c2f"
            );
            assert!(inspect(&bytes).is_ok());
        }
        {
            let first = vec![b'a'; sixteen_mib];
            let second = vec![b'b'; sixteen_mib];
            let bytes = encode(ProgramSpec {
                parameters: &[],
                captures: &[],
                lets: &[],
                statements: &[
                    super::StatementSpec::Expression { root_node: 0 },
                    super::StatementSpec::Expression { root_node: 1 },
                ],
                nodes: &[
                    super::NodeSpec::String(&first),
                    super::NodeSpec::String(&second),
                ],
            })
            .unwrap();
            assert_eq!(bytes.len(), 33_554_832);
            assert_eq!(
                digest_hex(&bytes),
                "625d4b6a280a4fbd4843f33fc8327435c4b303c9d22c075daba4c1c7b5e37d25"
            );
            assert!(inspect(&bytes).is_ok());
        }

        let oversized_name = vec![b'n'; one_mib + 1];
        assert_eq!(
            encode(ProgramSpec {
                parameters: &[&oversized_name],
                captures: &[],
                lets: &[],
                statements: &[],
                nodes: &[],
            }),
            Err(super::Error::Name)
        );
        let oversized_string = vec![0; sixteen_mib + 1];
        assert_eq!(
            encode(ProgramSpec {
                parameters: &[],
                captures: &[],
                lets: &[],
                statements: &[super::StatementSpec::Expression { root_node: 0 }],
                nodes: &[super::NodeSpec::String(&oversized_string)],
            }),
            Err(super::Error::Quantity)
        );
    }

    #[test]
    fn bounded_generated_trees_round_trip_and_depth_limit_is_exact() {
        for seed in 0..256u64 {
            let sends = (seed as usize % 32) + 1;
            let mut operands = Vec::with_capacity(sends);
            let mut root = 0u32;
            for step in 0..sends {
                let leaf = 2 * step as u32 + 1;
                let send = leaf + 1;
                operands.push([root, leaf]);
                root = send;
            }
            let mut nodes = Vec::with_capacity(2 * sends + 1);
            nodes.push(generated_leaf(seed, 0));
            for (step, owned) in operands.iter().enumerate() {
                nodes.push(generated_leaf(seed, step + 1));
                nodes.push(super::NodeSpec::Send {
                    selector: b"+",
                    operands: owned,
                });
            }
            let bytes = encode(ProgramSpec {
                parameters: &[b"p"],
                captures: &[],
                lets: &[],
                statements: &[super::StatementSpec::Expression { root_node: root }],
                nodes: &nodes,
            })
            .unwrap();
            let admitted = inspect(&bytes).unwrap();
            assert_eq!(admitted.node_count as usize, nodes.len());
            assert_eq!(admitted.statement_count, 1);
            let offset = (seed as usize * 2_654_435_761usize) % bytes.len();
            let mut mutated = bytes;
            mutated[offset] ^= 1;
            assert!(std::panic::catch_unwind(|| inspect(&mutated)).is_ok());
        }

        assert!(encode_chain(4095).is_ok());
        assert_eq!(encode_chain(4096), Err(super::Error::Quantity));
    }

    fn generated_leaf(seed: u64, ordinal: usize) -> super::NodeSpec<'static> {
        match (seed as usize + ordinal) % 9 {
            0 => super::NodeSpec::Integer(seed as i64 ^ ordinal as i64),
            1 => super::NodeSpec::FloatBits(seed.rotate_left((ordinal % 64) as u32)),
            2 => super::NodeSpec::String(b"\0generated"),
            3 => super::NodeSpec::Symbol(b"generated"),
            4 => super::NodeSpec::Nil,
            5 => super::NodeSpec::True,
            6 => super::NodeSpec::False,
            7 => super::NodeSpec::LexicalRead(0),
            _ => super::NodeSpec::GlobalRead(b"Generated"),
        }
    }

    fn encode_chain(sends: usize) -> Result<Vec<u8>, super::Error> {
        let mut operands = Vec::with_capacity(sends);
        let mut root = 0u32;
        for step in 0..sends {
            let leaf = 2 * step as u32 + 1;
            let send = leaf + 1;
            operands.push([root, leaf]);
            root = send;
        }
        let mut nodes = Vec::with_capacity(2 * sends + 1);
        nodes.push(super::NodeSpec::Nil);
        for owned in &operands {
            nodes.push(super::NodeSpec::Nil);
            nodes.push(super::NodeSpec::Send {
                selector: b"+",
                operands: owned,
            });
        }
        encode(ProgramSpec {
            parameters: &[],
            captures: &[],
            lets: &[],
            statements: &[super::StatementSpec::Expression { root_node: root }],
            nodes: &nodes,
        })
    }

    fn digest_hex(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
    #[test]
    #[ignore = "diagnostic scaling probe; host-gated timing acceptance is separate"]
    fn validated_program_access_probe() {
        use std::hint::black_box;
        use std::time::Instant;
        let bytes =
            include_bytes!("../../../abi/fixtures/portable-code-v1/program-nested-captures.bin");
        let owner = super::ValidatedProgram::copy_from_slice(bytes).unwrap();
        for cached in [false, true] {
            let started = Instant::now();
            for _ in 0..100_000 {
                let view = if cached {
                    black_box(&owner).view()
                } else {
                    super::inspect(black_box(bytes)).unwrap()
                };
                black_box(view);
            }
            eprintln!(
                "program bytes={} cached={cached} ns={}",
                bytes.len(),
                started.elapsed().as_nanos() / 100_000
            );
        }
    }
}
