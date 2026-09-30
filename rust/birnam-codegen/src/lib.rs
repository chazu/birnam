//! Pure source lowering, portable semantics, native emission, and generation planning.
//!
//! Runtime orchestration and owner messages live in birnam-codegen-runtime.

use std::borrow::Cow;

mod c_string;
pub mod native;
pub mod literal;
pub mod closure_elision;
pub mod discarded_clone;
pub mod frame_region;
pub mod numeric_chain;
pub mod portable;
pub mod recovery_graph;
pub mod resumable;
pub mod resumable_descriptor;
pub mod small_callee;
pub mod tail_entry;
pub mod virtual_aggregate;
pub use birnam_artifact::stackless_selection as artifact_stackless_selection;

/// Semantic expressions lowered from CST before portable/native compilation.
#[derive(Debug, PartialEq)]
pub enum Expr<'a> {
    SmallInt(i64),
    /// A source-preserved integer literal. The parser's spelling is retained
    /// so generated C does not drift through a Rust integer formatter.
    SmallIntLiteral(&'a [u8]),
    /// A source-preserved floating-point literal (for example `3.5` or
    /// `-0.25`). Keeping the lexical bytes avoids changing C spelling through
    /// an intermediate Rust `f64` formatter.
    Float(&'a [u8]),
    String(&'a [u8]),
    /// A Birnam string literal including its source quotes and escapes. The
    /// known Birnam escapes are decoded before the C string is emitted; an
    /// unknown escape keeps its backslash, matching the native emitter.
    StringLiteral(&'a [u8]),
    /// Owned literal bytes, including the carrier for `System embedFile:`. These
    /// bytes are owned by the compiler rather than borrowed from the
    /// authoring source, so generated artifacts remain self-contained.
    StringOwned(Vec<u8>),
    Symbol(&'a [u8]),
    Global(&'a [u8]),
    Nil,
    True,
    False,
    Block(Box<Block<'a>>),
    Send {
        receiver: Box<Expr<'a>>,
        selector: Cow<'a, [u8]>,
        args: Vec<Expr<'a>>,
        /// Source span of the complete send. Artifact lowering uses this to
        /// retain the portable live-operation journal alongside precompiled
        /// method installation.
        source: &'a [u8],
    },
}

#[derive(Debug, PartialEq)]
pub struct Block<'a> {
    pub params: Vec<&'a [u8]>,
    pub body: Vec<Statement<'a>>,
}

#[derive(Debug, PartialEq)]
pub enum Statement<'a> {
    Let { name: &'a [u8], value: Expr<'a> },
    Expr(Expr<'a>),
}

#[derive(Debug, PartialEq)]
pub struct Program<'a> {
    pub body: Vec<Statement<'a>>,
}

const MAX_EXPR_DEPTH: usize = 4096;
const MAX_CST_WRAPPER_DEPTH: usize = 4096;

/// The small read-only surface needed by the emitter's CST adapter. The
/// production Rust PEG object wrapper can implement this without exposing its
/// allocation or IdApi details to this crate.
pub trait CstNode {
    fn rule(&self) -> &[u8];
    fn start(&self) -> usize;
    fn end(&self) -> usize;
    fn child_count(&self) -> usize;
    fn child(&self, index: usize) -> Option<&Self>;

    /// Optional compiler-owned bytes attached to a synthetic literal node.
    /// Ordinary parser nodes return `None`; the runtime adapter uses this to
    /// preserve embedded files without threading filesystem access into the
    /// pure lowering functions.
    fn embedded_bytes(&self) -> Option<&[u8]> {
        None
    }
}

/// An owned CST projection used at the runtime boundary. The core emitter does
/// not know about IdApi or Oop objects; the Rust PEG adapter materializes its
/// transient nodes into this byte-owned tree before lowering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedCst {
    rule: Vec<u8>,
    start: usize,
    end: usize,
    children: Vec<OwnedCst>,
    embedded: Option<Vec<u8>>,
}

impl OwnedCst {
    pub fn new(rule: &[u8], start: usize, end: usize, children: Vec<OwnedCst>) -> Self {
        Self {
            rule: rule.to_vec(),
            start,
            end,
            children,
            embedded: None,
        }
    }

    /// Transform children by value, retaining this node's span and bytes.
    /// Rewriters can consume a tree without cloning every descendant at each level.
    pub fn try_map_children(mut self, mut map: impl FnMut(Self) -> Option<Self>) -> Option<Self> {
        self.children = self.children.into_iter().map(&mut map).collect::<Option<_>>()?;
        Some(self)
    }

    /// Construct a synthetic string node whose bytes came from an embedded
    /// file. The source span is retained for diagnostics and deterministic
    /// traversal, while lowering reads the owned payload through
    /// `CstNode::embedded_bytes`.
    pub fn embedded_string(start: usize, end: usize, bytes: Vec<u8>) -> Self {
        Self {
            rule: b"embeddedString".to_vec(),
            start,
            end,
            children: Vec::new(),
            embedded: Some(bytes),
        }
    }
}

impl CstNode for OwnedCst {
    fn rule(&self) -> &[u8] {
        &self.rule
    }

    fn start(&self) -> usize {
        self.start
    }

    fn end(&self) -> usize {
        self.end
    }

    fn child_count(&self) -> usize {
        self.children.len()
    }

    fn child(&self, index: usize) -> Option<&Self> {
        self.children.get(index)
    }

    fn embedded_bytes(&self) -> Option<&[u8]> {
        self.embedded.as_deref()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum LowerError {
    DepthLimit,
    InvalidSpan,
    MissingChild,
    InvalidSymbol,
    InvalidShape,
    ThreadedSpecialForm,
    UnsupportedRule,
}

/// Lower the literal subset of a CST node into the pure emitter IR.
///
/// `expr`, `primary`, and `message` wrappers are transparent when they have a
/// single child, matching the native emitter's `skip` operation. Sends, blocks,
/// collections, and logic variables deliberately remain unsupported until the
/// corresponding CST-shape slices are frozen.
pub fn lower_literal<'a, N: CstNode>(node: &N, source: &'a [u8]) -> Result<Expr<'a>, LowerError> {
    let node = skip_wrappers(node)?;
    if node.rule() == b"embeddedString" {
        return node
            .embedded_bytes()
            .map(|bytes| Expr::StringOwned(bytes.to_vec()))
            .ok_or(LowerError::InvalidShape);
    }
    let span = span(node, source)?;
    match node.rule() {
        b"number" => Ok(Expr::SmallIntLiteral(span)),
        b"float" => Ok(Expr::Float(span)),
        b"string" => Ok(Expr::StringLiteral(span)),
        b"symbol" => span
            .strip_prefix(b"#")
            .map(Expr::Symbol)
            .ok_or(LowerError::InvalidSymbol),
        b"ident" => match span {
            b"nil" => Ok(Expr::Nil),
            b"true" => Ok(Expr::True),
            b"false" => Ok(Expr::False),
            _ => Ok(Expr::Global(span)),
        },
        _ => Err(LowerError::UnsupportedRule),
    }
}

fn skip_wrappers<N: CstNode>(node: &N) -> Result<&N, LowerError> {
    let mut current = node;
    for _ in 0..MAX_CST_WRAPPER_DEPTH {
        let rule = current.rule();
        if matches!(rule, b"expr" | b"primary" | b"message" | b"threadDispatch")
            && current.child_count() == 1
        {
            current = current.child(0).ok_or(LowerError::MissingChild)?;
        } else {
            return Ok(current);
        }
    }
    Err(LowerError::DepthLimit)
}

fn span<'a, N: CstNode>(node: &N, source: &'a [u8]) -> Result<&'a [u8], LowerError> {
    source
        .get(node.start()..node.end())
        .ok_or(LowerError::InvalidSpan)
}

/// Lower an expression and its send/message shape without touching the
/// runtime. The recursive depth bound is shared with emission so a malformed
/// or adversarial CST cannot force unbounded native recursion.
pub fn lower_expr<'a, N: CstNode>(node: &N, source: &'a [u8]) -> Result<Expr<'a>, LowerError> {
    lower_expr_at_depth(node, source, 0)
}

fn lower_expr_at_depth<'a, N: CstNode>(
    node: &N,
    source: &'a [u8],
    depth: usize,
) -> Result<Expr<'a>, LowerError> {
    if depth >= MAX_EXPR_DEPTH {
        return Err(LowerError::DepthLimit);
    }
    let node = skip_wrappers(node)?;
    match node.rule() {
        b"number" | b"float" | b"string" | b"symbol" | b"ident" | b"embeddedString" => {
            lower_literal(node, source)
        }
        b"send" => lower_send(node, source, depth + 1),
        b"vectorLit" => lower_vector(node, source, depth + 1),
        b"mapLit" => lower_map(node, source, depth + 1),
        b"logicVar" => lower_logic_var(node, source),
        b"block" => lower_block(node, source, depth + 1).map(|block| Expr::Block(Box::new(block))),
        _ => Err(LowerError::UnsupportedRule),
    }
}

fn lower_block<'a, N: CstNode>(
    node: &N,
    source: &'a [u8],
    depth: usize,
) -> Result<Block<'a>, LowerError> {
    let mut params = Vec::new();
    let mut body = Vec::new();
    for index in 0..node.child_count() {
        let child = node.child(index).ok_or(LowerError::MissingChild)?;
        match child.rule() {
            b"ident" => params.push(span(child, source)?),
            b"statements" => {
                if !body.is_empty() {
                    return Err(LowerError::InvalidShape);
                }
                body = lower_statements(child, source, depth)?;
            }
            _ => return Err(LowerError::InvalidShape),
        }
    }
    Ok(Block { params, body })
}

fn lower_statements<'a, N: CstNode>(
    node: &N,
    source: &'a [u8],
    depth: usize,
) -> Result<Vec<Statement<'a>>, LowerError> {
    let mut statements = Vec::with_capacity(node.child_count());
    for index in 0..node.child_count() {
        let statement = node.child(index).ok_or(LowerError::MissingChild)?;
        if statement.rule() != b"statement" || statement.child_count() != 1 {
            return Err(LowerError::InvalidShape);
        }
        let inner = statement.child(0).ok_or(LowerError::MissingChild)?;
        match inner.rule() {
            b"letBinding" => {
                if inner.child_count() != 2 {
                    return Err(LowerError::InvalidShape);
                }
                let name = span(inner.child(0).ok_or(LowerError::MissingChild)?, source)?;
                let value = lower_expr_at_depth(
                    inner.child(1).ok_or(LowerError::MissingChild)?,
                    source,
                    depth,
                )?;
                statements.push(Statement::Let { name, value });
            }
            b"expr" => statements.push(Statement::Expr(lower_expr_at_depth(inner, source, depth)?)),
            _ => return Err(LowerError::InvalidShape),
        }
    }
    Ok(statements)
}

/// Lower the accepted `program` root into an ordered, runtime-free statement
/// list. An empty program is a valid program whose eventual value is nil.
pub fn lower_program<'a, N: CstNode>(
    node: &N,
    source: &'a [u8],
) -> Result<Program<'a>, LowerError> {
    let node = skip_wrappers(node)?;
    if node.rule() != b"program" {
        return Err(LowerError::UnsupportedRule);
    }
    if node.child_count() == 0 {
        return Ok(Program { body: Vec::new() });
    }
    if node.child_count() != 1 {
        return Err(LowerError::InvalidShape);
    }
    let statements = node.child(0).ok_or(LowerError::MissingChild)?;
    if statements.rule() != b"statements" {
        return Err(LowerError::InvalidShape);
    }
    Ok(Program {
        body: lower_statements(statements, source, 1)?,
    })
}

fn lower_vector<'a, N: CstNode>(
    node: &N,
    source: &'a [u8],
    depth: usize,
) -> Result<Expr<'a>, LowerError> {
    let mut result = Expr::Send {
        receiver: Box::new(Expr::Global(b"Vector")),
        selector: Cow::Borrowed(b"new"),
        args: Vec::new(),
        source: span(node, source)?,
    };
    for index in 0..node.child_count() {
        let element = lower_expr_at_depth(
            node.child(index).ok_or(LowerError::MissingChild)?,
            source,
            depth,
        )?;
        result = Expr::Send {
            receiver: Box::new(result),
            selector: Cow::Borrowed(b"add:"),
            args: vec![element],
            source: span(node, source)?,
        };
    }
    Ok(result)
}

fn lower_map<'a, N: CstNode>(
    node: &N,
    source: &'a [u8],
    depth: usize,
) -> Result<Expr<'a>, LowerError> {
    let mut result = Expr::Send {
        receiver: Box::new(Expr::Global(b"Map")),
        selector: Cow::Borrowed(b"new"),
        args: Vec::new(),
        source: span(node, source)?,
    };
    for index in 0..node.child_count() {
        let entry = node.child(index).ok_or(LowerError::MissingChild)?;
        if entry.rule() != b"mapEntry" || entry.child_count() != 2 {
            return Err(LowerError::InvalidShape);
        }
        let key = lower_expr_at_depth(
            entry.child(0).ok_or(LowerError::MissingChild)?,
            source,
            depth,
        )?;
        let value = lower_expr_at_depth(
            entry.child(1).ok_or(LowerError::MissingChild)?,
            source,
            depth,
        )?;
        result = Expr::Send {
            receiver: Box::new(result),
            selector: Cow::Borrowed(b"at:put:"),
            args: vec![key, value],
            source: span(node, source)?,
        };
    }
    Ok(result)
}

fn lower_logic_var<'a, N: CstNode>(node: &N, source: &'a [u8]) -> Result<Expr<'a>, LowerError> {
    if node.child_count() != 1 {
        return Err(LowerError::InvalidShape);
    }
    let name = span(node.child(0).ok_or(LowerError::MissingChild)?, source)?;
    Ok(Expr::Send {
        receiver: Box::new(Expr::Global(b"LogicVar")),
        selector: Cow::Borrowed(b"named:"),
        args: vec![Expr::Symbol(name)],
        source: span(node, source)?,
    })
}

fn lower_send<'a, N: CstNode>(
    node: &N,
    source: &'a [u8],
    depth: usize,
) -> Result<Expr<'a>, LowerError> {
    if node.child_count() != 2 {
        return Err(LowerError::InvalidShape);
    }
    let receiver = lower_expr_at_depth(
        node.child(0).ok_or(LowerError::MissingChild)?,
        source,
        depth,
    )?;
    let send_source = span(node, source)?;
    let message = skip_wrappers(node.child(1).ok_or(LowerError::MissingChild)?)?;
    if message.rule() == b"threadedMsg" {
        if threaded_special_form(
            node.child(0).ok_or(LowerError::MissingChild)?,
            message,
            source,
        )? {
            return Err(LowerError::ThreadedSpecialForm);
        }
        let mut accumulated = receiver;
        for index in 0..message.child_count() {
            let dispatch = message.child(index).ok_or(LowerError::MissingChild)?;
            accumulated = lower_message(accumulated, dispatch, source, depth, send_source)?;
        }
        return Ok(accumulated);
    }
    if message.rule() == b"message" && message.child_count() > 1 {
        let mut accumulated = receiver;
        for index in 0..message.child_count() {
            let part = message.child(index).ok_or(LowerError::MissingChild)?;
            accumulated = lower_message(accumulated, part, source, depth, send_source)?;
        }
        return Ok(accumulated);
    }
    lower_message(receiver, message, source, depth, send_source)
}

fn threaded_special_form<N: CstNode>(
    receiver: &N,
    message: &N,
    source: &[u8],
) -> Result<bool, LowerError> {
    let receiver = skip_wrappers(receiver)?;
    if receiver.rule() != b"ident" {
        return Ok(false);
    }
    let receiver = span(receiver, source)?;
    let first = message.child(0).ok_or(LowerError::MissingChild)?;
    let first = skip_wrappers(first)?;
    if first.rule() != b"keywordMsg" || first.child_count() == 0 || first.child_count() % 2 != 0 {
        return Ok(false);
    }
    let mut selector = Vec::new();
    for index in (0..first.child_count()).step_by(2) {
        selector.extend_from_slice(span(
            first.child(index).ok_or(LowerError::MissingChild)?,
            source,
        )?);
    }
    Ok(matches!(
        (receiver, selector.as_slice()),
        (b"System", b"embedFile:")
            | (b"Module", b"named:requires:")
            | (b"Alien", b"dlopen:")
            | (b"Reflect", b"defineMethod:on:as:")
            | (b"Reflect", b"defineMethod:on:as:doc:")
            | (b"Reflect", b"removeMethod:on:")
    ))
}

fn lower_message<'a, N: CstNode>(
    receiver: Expr<'a>,
    message: &N,
    source: &'a [u8],
    depth: usize,
    send_source: &'a [u8],
) -> Result<Expr<'a>, LowerError> {
    let message = skip_wrappers(message)?;
    let (selector, args) = match message.rule() {
        b"binaryMsg" => {
            if message.child_count() != 2 {
                return Err(LowerError::InvalidShape);
            }
            let selector = Cow::Borrowed(span(
                message.child(0).ok_or(LowerError::MissingChild)?,
                source,
            )?);
            let arg = lower_expr_at_depth(
                message.child(1).ok_or(LowerError::MissingChild)?,
                source,
                depth,
            )?;
            (selector, vec![arg])
        }
        b"unaryMsg" => {
            let selector = Cow::Borrowed(span(message, source)?);
            (selector, Vec::new())
        }
        b"keywordMsg" => {
            if message.child_count() == 0 || message.child_count() % 2 != 0 {
                return Err(LowerError::InvalidShape);
            }
            let mut selector = Vec::new();
            let mut args = Vec::with_capacity(message.child_count() / 2);
            let mut index = 0;
            while index < message.child_count() {
                selector.extend_from_slice(span(
                    message.child(index).ok_or(LowerError::MissingChild)?,
                    source,
                )?);
                args.push(lower_expr_at_depth(
                    message.child(index + 1).ok_or(LowerError::MissingChild)?,
                    source,
                    depth,
                )?);
                index += 2;
            }
            (Cow::Owned(selector), args)
        }
        _ => return Err(LowerError::UnsupportedRule),
    };
    Ok(Expr::Send {
        receiver: Box::new(receiver),
        selector,
        args,
        source: send_source,
    })
}

fn collect_reference(references: &mut Vec<Vec<u8>>, name: &[u8]) {
    if !references
        .iter()
        .any(|reference| reference.as_slice() == name)
    {
        references.push(name.to_vec());
    }
}

fn collect_expr_refs(expr: &Expr<'_>, references: &mut Vec<Vec<u8>>) {
    match expr {
        Expr::Global(name) => collect_reference(references, name),
        Expr::Send { receiver, args, .. } => {
            collect_expr_refs(receiver, references);
            for arg in args {
                collect_expr_refs(arg, references);
            }
        }
        Expr::Block(block) => {
            for statement in &block.body {
                collect_statement_refs(statement, references);
            }
        }
        _ => {}
    }
}

fn collect_statement_refs(statement: &Statement<'_>, references: &mut Vec<Vec<u8>>) {
    match statement {
        Statement::Let { value, .. } => collect_expr_refs(value, references),
        Statement::Expr(expression) => collect_expr_refs(expression, references),
    }
}

fn block_has_let(block: &Block<'_>, name: &[u8]) -> bool {
    block.body.iter().any(
        |statement| matches!(statement, Statement::Let { name: let_name, .. } if *let_name == name),
    )
}

#[cfg(test)]
mod lowering_tests;

pub mod artifact;
pub mod source_revision;
pub mod generation;
pub mod candidate;
pub use birnam_artifact::candidate as candidate_envelope;

pub mod image;
