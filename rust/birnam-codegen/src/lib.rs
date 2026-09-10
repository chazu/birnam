//! Pure deterministic C-emission primitives for the N6.2 Codegen port.
//!
//! This crate deliberately has no IdApi, loader, allocator, or runtime
//! dependency. It owns byte-buffer construction and a pure CST-to-expression
//! adapter; runtime orchestration arrives in later N6.2/N6.3 slices.

use std::borrow::Cow;

mod c_string;
pub mod portable;
pub mod portable_c;
pub mod resumable;
pub mod resumable_block;
pub mod resumable_c;
pub mod resumable_descriptor;
pub mod resumable_named;
pub mod resumable_registration_c;
pub mod resumable_resolver_c;
pub use birnam_artifact::stackless_selection as artifact_stackless_selection;

/// A bounded-independent byte emitter. Callers retain ownership of the returned
/// translation-unit bytes and may compare them byte-for-byte with the reference.
#[derive(Debug, Default)]
pub struct Emitter {
    bytes: Vec<u8>,
    function_defs: Vec<u8>,
    bound_names: Vec<Vec<u8>>,
    baked_globals: Vec<Vec<u8>>,
    next_block: usize,
    next_send: usize,
    artifact_mode: bool,
    artifact_method_counter: usize,
    artifact_operation_counter: u32,
    named_resumables: Vec<NamedResumableEmission>,
    block_resumables: Vec<BlockResumableEmission>,
    block_keys: Vec<(usize, resumable::ImplementationKey)>,
    artifact_operation_root: bool,
}

/// One precompiled named-adapter fragment selected for an exact portable
/// method-operation position in an artifact translation unit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedResumableEmission {
    pub operation_index: u32,
    pub symbol_index: u32,
    pub c_source: Vec<u8>,
}

/// One precompiled block-body fragment selected by its complete stable key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockResumableEmission {
    pub key: resumable::ImplementationKey,
    pub symbol_index: u32,
    pub manifest_entry_index: u32,
    pub selector: Vec<u8>,
    pub captures: Vec<Vec<u8>>,
    pub c_source: Vec<u8>,
}

#[derive(Clone, Copy)]
enum BlockCaptureValue<'a> {
    SourceBinding(&'a [u8]),
    PortableBinding(u32),
}

/// The pure expression subset consumed by the first Rust emission seam.
///
/// This is intentionally not a parser CST or a runtime Oop graph. A later
/// adapter will lower the Rust PEG CST into this representation after the
/// generated-byte contract is frozen.
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
    /// Bytes resolved from a compiler `System embedFile:` intrinsic. These
    /// bytes are owned by the CST projection rather than borrowed from the
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

#[derive(Debug, PartialEq, Eq)]
pub enum EmitError {
    DepthLimit,
    Unsupported,
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

fn collect_portable_node_sources<'a>(expression: &'a Expr<'a>, output: &mut Vec<&'a Expr<'a>>) {
    match expression {
        Expr::Block(_) => output.push(expression),
        Expr::Send { receiver, args, .. } => {
            collect_portable_node_sources(receiver, output);
            for argument in args {
                collect_portable_node_sources(argument, output);
            }
            output.push(expression);
        }
        _ => output.push(expression),
    }
}

fn sourced_reflect_selector<'a>(
    expression: &'a Expr<'a>,
    portable_selector: &[u8],
) -> Option<(&'static [u8], &'a [u8])> {
    let Expr::Send {
        receiver,
        selector,
        args,
        source,
    } = expression
    else {
        return None;
    };
    if !matches!(receiver.as_ref(), Expr::Global(name) if *name == b"Reflect")
        || selector.as_ref() != portable_selector
    {
        return None;
    }
    match (portable_selector, args.len()) {
        (b"defineMethod:on:as:", 3) => Some((b"defineMethod:on:as:source:", source)),
        (b"defineMethod:on:as:doc:", 4) => Some((b"defineMethod:on:as:doc:source:", source)),
        _ => None,
    }
}

fn selector_arity(selector: &[u8]) -> usize {
    let colons = selector.iter().filter(|byte| **byte == b':').count();
    if colons != 0 {
        return colons;
    }
    match selector.first() {
        None => 0,
        Some(byte) if byte.is_ascii_alphabetic() || *byte == b'_' => 0,
        Some(_) => 1,
    }
}

const C_RUNTIME_PRELUDE: &[u8] = br#"extern void* birnam_mkSmallInt(long v);
#define MKV(x) (birnam_mkSmallInt((long)(x)))
extern void* birnam_send(void* rcv, void* sel, void** args, long n);
extern void* birnam_intern(const char* name);
extern long birnam_smallIntValue(void* value);
extern void* birnam_mkString(const char* p, long n);
extern void* birnam_mkFloat(double v);
extern void* birnam_nil(void);
extern void* birnam_true(void);
extern void* birnam_false(void);
extern void* birnam_global(const char* name);
extern void* birnam_global_b(void* sym);
extern void* birnam_global_set(const char* name, void* value);
extern void* birnam_ensurePrivateVtable(void* obj);
extern void* birnam_install_method(void* vt, void* sel, void* method, const char* src, long n);
extern void* birnam_remove_method(void* target, void* sel, const char* src, long n);
extern void* birnam_candidate_root_get(unsigned long long salt, long index);
extern void* birnam_candidate_root_set(unsigned long long salt, long index, void* value);
extern void* birnam_vtable_cache_get(unsigned long long generation, unsigned long long index);
extern void* birnam_vtable_cache_set(unsigned long long generation, unsigned long long index, void* value);
extern void* birnam_portable_code_admit_v1(const unsigned char* bytes, unsigned long length);
extern void* birnam_portable_block_new_v1(void* vt, void* code_object, void** captures, unsigned long capture_count);
extern void* birnam_portable_materialized_block_new_v2(void* vt, void* code_object, void** captures, unsigned long capture_count, void* materialization_owner);
extern long birnam_is_error(void* value);
unsigned long long __birnam_code_generation_identity;
extern long birnam_candidate_preparing(void);
extern void* birnam_proto(void* parent);
extern void* birnam_blockVt(void);
extern void birnam_method(void* vt, void* selector, void* method);
extern void birnam_layoutAllOops(void* vt);
extern void* birnam_alloc(void* vt, long bytes);
"#;

/// Deterministic translation-unit assembly for the Rust emitter seam. Runtime
/// shims are declarations only; this type never links or resolves them.
#[derive(Debug, Default)]
pub struct TranslationUnit {
    artifact: bool,
    baked_globals: Vec<Vec<u8>>,
    function_defs: Vec<u8>,
    body: Vec<u8>,
}

impl TranslationUnit {
    pub fn new(artifact: bool) -> Self {
        Self {
            artifact,
            ..Self::default()
        }
    }

    pub fn add_baked_global(&mut self, name: &[u8]) {
        self.baked_globals.push(name.to_vec());
    }

    pub fn push_function(&mut self, definition: &[u8]) {
        self.function_defs.extend_from_slice(definition);
    }

    pub fn push_body(&mut self, body: &[u8]) {
        self.body.extend_from_slice(body);
    }

    pub fn finish(self) -> Vec<u8> {
        let mut output = Vec::with_capacity(
            C_RUNTIME_PRELUDE.len()
                + self.function_defs.len()
                + self.body.len()
                + self.baked_globals.len() * 32
                + if self.artifact { 64 } else { 0 },
        );
        output.extend_from_slice(C_RUNTIME_PRELUDE);
        if resumable_c::guarded_slots_selected() {
            output.extend_from_slice(resumable_c::GUARDED_SLOT_PRELUDE);
        }
        if resumable_c::slot_box_elision_selected() {
            output.extend_from_slice(resumable_c::ELIDED_SLOT_PRELUDE);
        }
        for (index, _name) in self.baked_globals.iter().enumerate() {
            if self.artifact {
                output.extend_from_slice(b"static void* __g");
                output.extend_from_slice(index.to_string().as_bytes());
                output.extend_from_slice(b";\n");
            } else {
                output.extend_from_slice(b"extern char __g");
                output.extend_from_slice(index.to_string().as_bytes());
                output.extend_from_slice(b"[];\n");
            }
        }
        output.extend_from_slice(&self.function_defs);
        output.extend_from_slice(
            b"void* __ev(void* self, void** args, long n){ (void)self;(void)args;(void)n;\n",
        );
        output.extend_from_slice(&self.body);
        output.extend_from_slice(b"}\n");
        if self.artifact {
            output.extend_from_slice(b"void __birnam_unit_init(void){\n");
            for (index, name) in self.baked_globals.iter().enumerate() {
                output.extend_from_slice(b"  __g");
                output.extend_from_slice(index.to_string().as_bytes());
                output.extend_from_slice(b" = birnam_intern(\"");
                let mut escaped = Emitter::new();
                escaped.push_c_string_body(name);
                output.extend_from_slice(&escaped.finish());
                output.extend_from_slice(b"\");\n");
            }
            output.extend_from_slice(b"}\n");
        }
        output
    }
}

impl Emitter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct an emitter for a candidate artifact. Method definitions are
    /// lowered to precompiled trampolines and rooted block captures; the
    /// runtime candidate wrapper later rewrites the frozen shim names to
    /// the selected Rust-owned namespace when required.
    pub fn new_candidate_artifact(_salt: u64) -> Self {
        Self {
            artifact_mode: true,
            ..Self::default()
        }
    }

    /// Construct an artifact emitter with exact operation-indexed named
    /// resumable fragments. Selection admission and manifest composition own
    /// the fragment set; this emitter preserves their publication ordering.
    pub fn new_resumable_candidate_artifact(
        salt: u64,
        named_resumables: Vec<NamedResumableEmission>,
    ) -> Result<Self, EmitError> {
        Self::new_selected_resumable_candidate_artifact(salt, named_resumables, Vec::new())
    }

    /// Construct a candidate emitter with exact named and block-body
    /// resumable fragments. Every fragment is emitted once, while block
    /// registrations run eagerly at activation before literal resolution.
    pub fn new_selected_resumable_candidate_artifact(
        _salt: u64,
        mut named_resumables: Vec<NamedResumableEmission>,
        mut block_resumables: Vec<BlockResumableEmission>,
    ) -> Result<Self, EmitError> {
        named_resumables.sort_by_key(|entry| entry.operation_index);
        if named_resumables
            .windows(2)
            .any(|entries| entries[0].operation_index == entries[1].operation_index)
            || block_resumables.iter().any(|entry| {
                entry.key.implementation_kind != resumable::InventoryImplementationKind::BlockBody
                    || entry.key.path.is_empty()
            })
        {
            return Err(EmitError::Unsupported);
        }
        block_resumables.sort_by(|left, right| left.key.cmp(&right.key));
        if block_resumables
            .windows(2)
            .any(|entries| entries[0].key == entries[1].key)
            || named_resumables.iter().enumerate().any(|(index, named)| {
                named_resumables[..index]
                    .iter()
                    .any(|prior| prior.symbol_index == named.symbol_index)
                    || block_resumables
                        .iter()
                        .any(|block| block.symbol_index == named.symbol_index)
            })
            || block_resumables.iter().enumerate().any(|(index, block)| {
                block_resumables[..index]
                    .iter()
                    .any(|prior| prior.symbol_index == block.symbol_index)
            })
        {
            return Err(EmitError::Unsupported);
        }
        let mut emitter = Self {
            artifact_mode: true,
            named_resumables,
            block_resumables,
            ..Self::default()
        };
        if !emitter.named_resumables.is_empty() || !emitter.block_resumables.is_empty() {
            emitter
                .function_defs
                .extend_from_slice(resumable_registration_c::registration_prelude_v2());
            if !emitter.block_resumables.is_empty() {
                emitter.function_defs.extend_from_slice(
                    b"void* __birnam_resolve_block_vtable_v1(unsigned int manifest_entry_index);\n",
                );
            }
            for entry in &emitter.named_resumables {
                emitter.function_defs.extend_from_slice(&entry.c_source);
            }
            for entry in &emitter.block_resumables {
                emitter.function_defs.extend_from_slice(&entry.c_source);
            }
            if !emitter.block_resumables.is_empty() {
                let mut specs = emitter
                    .block_resumables
                    .iter()
                    .map(|entry| resumable_resolver_c::BlockVtableSpec {
                        manifest_entry_index: entry.manifest_entry_index,
                        symbol_index: entry.symbol_index,
                        selector: &entry.selector,
                    })
                    .collect::<Vec<_>>();
                // A monolithic image unit orders implementation keys by their
                // rebased source positions, while its manifest is ordered by
                // the complete selection key (including source revision).
                // The resolver ABI is indexed by the latter.
                specs.sort_by_key(|entry| entry.manifest_entry_index);
                let resolver = resumable_resolver_c::emit_block_vtable_resolver_v1(&specs)
                    .map_err(|_| EmitError::Unsupported)?;
                emitter.function_defs.extend_from_slice(&resolver);
            }
        }
        Ok(emitter)
    }

    /// Construct an emitter for the requested translation-unit mode. Global
    /// references use the frozen baked-symbol ABI in both JIT and artifact
    /// output; only the declaration spelling differs by mode.
    pub fn new_for_artifact(artifact: bool) -> Self {
        Self {
            artifact_mode: artifact,
            ..Self::default()
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(capacity),
            ..Self::default()
        }
    }

    pub fn push(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    pub fn push_c_string_body(&mut self, bytes: &[u8]) {
        c_string::push_c_string_body(&mut self.bytes, bytes);
    }

    pub fn emit_small_int(&mut self, value: i64) {
        self.bytes.extend_from_slice(b"MKV(");
        self.bytes.extend_from_slice(value.to_string().as_bytes());
        self.bytes.extend_from_slice(b"L)");
    }

    pub fn emit_small_int_literal(&mut self, literal: &[u8]) {
        self.bytes.extend_from_slice(b"MKV(");
        self.bytes.extend_from_slice(literal);
        self.bytes.extend_from_slice(b"L)");
    }

    pub fn emit_string(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(b"birnam_mkString(\"");
        self.push_c_string_body(bytes);
        self.bytes.extend_from_slice(b"\", ");
        self.bytes
            .extend_from_slice(bytes.len().to_string().as_bytes());
        self.bytes.extend_from_slice(b"L)");
    }

    pub fn emit_float(&mut self, literal: &[u8]) {
        self.bytes.extend_from_slice(b"birnam_mkFloat(");
        self.bytes.extend_from_slice(literal);
        self.bytes.extend_from_slice(b")");
    }

    pub fn emit_float_bits(&mut self, bits: u64) {
        self.bytes
            .extend_from_slice(b"({union{unsigned long long u;double d;}__birnam_float_bits={0x");
        self.bytes
            .extend_from_slice(format!("{bits:016x}").as_bytes());
        self.bytes
            .extend_from_slice(b"ULL};birnam_mkFloat(__birnam_float_bits.d);})");
    }

    pub fn emit_string_literal(&mut self, source: &[u8]) {
        let inner = if source.len() >= 2 {
            &source[1..source.len() - 1]
        } else {
            source
        };
        let mut decoded = Vec::with_capacity(inner.len());
        let mut index = 0;
        while index < inner.len() {
            if inner[index] == b'\\' && index + 1 < inner.len() {
                let escape = match inner[index + 1] {
                    b'"' => Some(b'"'),
                    b'\\' => Some(b'\\'),
                    b'n' => Some(b'\n'),
                    b't' => Some(b'\t'),
                    b'r' => Some(b'\r'),
                    b'0' => Some(0),
                    _ => None,
                };
                if let Some(byte) = escape {
                    decoded.push(byte);
                    index += 2;
                    continue;
                }
            }
            decoded.push(inner[index]);
            index += 1;
        }
        self.emit_string(&decoded);
    }

    pub fn emit_nil(&mut self) {
        self.bytes.extend_from_slice(b"birnam_nil()");
    }

    pub fn emit_true(&mut self) {
        self.bytes.extend_from_slice(b"birnam_true()");
    }

    pub fn emit_false(&mut self) {
        self.bytes.extend_from_slice(b"birnam_false()");
    }

    pub fn emit_symbol(&mut self, name: &[u8]) {
        self.bytes.extend_from_slice(b"birnam_intern(\"");
        self.push_c_string_body(name);
        self.bytes.extend_from_slice(b"\")");
    }

    pub fn emit_global(&mut self, name: &[u8]) {
        self.bytes.extend_from_slice(b"birnam_global(\"");
        self.push_c_string_body(name);
        self.bytes.extend_from_slice(b"\")");
    }

    fn emit_reference(&mut self, name: &[u8]) {
        if self
            .bound_names
            .iter()
            .any(|bound| bound.as_slice() == name)
        {
            self.bytes.extend_from_slice(b"b_");
            self.bytes.extend_from_slice(name);
        } else {
            let index = self.bake_global(name);
            self.bytes.extend_from_slice(b"birnam_global_b(");
            if self.artifact_mode {
                self.bytes.extend_from_slice(b"__g");
            } else {
                self.bytes.extend_from_slice(b"(void*)__g");
            }
            self.bytes.extend_from_slice(index.to_string().as_bytes());
            self.bytes.extend_from_slice(b")");
        }
    }

    fn bake_global(&mut self, name: &[u8]) -> usize {
        if let Some(index) = self
            .baked_globals
            .iter()
            .position(|baked| baked.as_slice() == name)
        {
            return index;
        }
        let index = self.baked_globals.len();
        self.baked_globals.push(name.to_vec());
        index
    }

    pub fn emit_send(&mut self, receiver: &[u8], selector: &[u8], args: &[&[u8]]) {
        self.bytes.extend_from_slice(b"birnam_send(");
        self.push(receiver);
        self.bytes.extend_from_slice(b", ");
        self.emit_symbol(selector);
        if args.is_empty() {
            self.bytes.extend_from_slice(b", 0, 0L)");
            return;
        }
        self.bytes.extend_from_slice(b", (void*[]){");
        for (index, arg) in args.iter().enumerate() {
            if index != 0 {
                self.bytes.extend_from_slice(b", ");
            }
            self.push(arg);
        }
        self.bytes.extend_from_slice(b"}, ");
        self.bytes
            .extend_from_slice(args.len().to_string().as_bytes());
        self.bytes.extend_from_slice(b"L)");
    }

    pub fn emit_expr(&mut self, expr: &Expr<'_>) -> Result<(), EmitError> {
        self.emit_root_expr(expr, 0, true)
    }

    /// Emit a program body transactionally. The caller owns the surrounding
    /// translation-unit wrapper; this method only appends the ordered body and
    /// any nested block definitions. Runtime error propagation and persistent
    /// top-level global writes remain N6.3 orchestration concerns.
    pub fn emit_program(&mut self, program: &Program<'_>) -> Result<(), EmitError> {
        self.emit_program_mode(program, false, None)
    }

    /// Emit a top-level program whose `let` bindings update the shared global
    /// environment after their local value is computed. Nested block locals
    /// remain lexical and never persist.
    pub fn emit_program_persisting(&mut self, program: &Program<'_>) -> Result<(), EmitError> {
        self.emit_program_mode(program, true, None)
    }

    /// Emit the declarative part of a candidate generation projection. Method
    /// operations retain their ordinary candidate treatment; only the named
    /// top-level statements are additionally evaluated and published as
    /// Globals. The caller must derive the indices from
    /// `resumable::projected_block_bindings`, not from source offsets.
    pub fn emit_projected_candidate_program(
        &mut self,
        program: &Program<'_>,
        projected_statement_indices: &[usize],
    ) -> Result<(), EmitError> {
        if projected_statement_indices
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
            || projected_statement_indices
                .iter()
                .any(|index| *index >= program.body.len())
        {
            return Err(EmitError::Unsupported);
        }
        self.emit_program_mode(program, true, Some(projected_statement_indices))
    }

    /// Emit the executable part of an image generation. Method operations must
    /// run to rebuild process-local dispatch, and projection-safe capture-free
    /// block bindings must recreate their process-local Globals roots. Every
    /// other initializer is suppressed so loading and resaving an image is
    /// state-idempotent.
    pub fn emit_image_recovery_program(&mut self, program: &Program<'_>) -> Result<(), EmitError> {
        let mut projected = resumable::projected_block_bindings(program)
            .into_iter()
            .filter(|binding| binding.captures.is_empty())
            .map(|binding| binding.statement_index)
            .collect::<Vec<_>>();
        projected.sort_unstable();
        projected.dedup();
        self.emit_program_mode(program, true, Some(&projected))
    }

    fn emit_program_mode(
        &mut self,
        program: &Program<'_>,
        persist_top_level: bool,
        projected_statement_indices: Option<&[usize]>,
    ) -> Result<(), EmitError> {
        let saved_bytes = std::mem::take(&mut self.bytes);
        let saved_bound_names = std::mem::take(&mut self.bound_names);
        let saved_block_keys = std::mem::take(&mut self.block_keys);
        let saved_function_defs_len = self.function_defs.len();
        self.bytes = Vec::new();
        self.bound_names = Vec::new();
        self.block_keys = resumable::implementation_inventory(program)
            .into_iter()
            .filter_map(|key| {
                resumable::resolve_block_implementation(program, &key)
                    .map(|resolved| (resolved.block as *const Block<'_> as usize, key))
            })
            .collect();
        if self.artifact_mode && !self.block_resumables.is_empty() {
            self.bytes
                .extend_from_slice(b"if(!birnam_candidate_preparing()){");
            for entry in &self.block_resumables {
                self.bytes
                    .extend_from_slice(b"if(__birnam_resumable_prepare_");
                self.bytes
                    .extend_from_slice(entry.symbol_index.to_string().as_bytes());
                self.bytes
                    .extend_from_slice(b"((void*)0)!=0U)return birnam_nil();");
            }
            self.bytes.extend_from_slice(b"}\n");
        }
        let selected_indices = projected_statement_indices.map(|projected| {
            program
                .body
                .iter()
                .enumerate()
                .filter_map(|(index, statement)| {
                    let expression = match statement {
                        Statement::Let { value, .. } | Statement::Expr(value) => value,
                    };
                    (resumable::method_operation(expression).is_some()
                        || projected.binary_search(&index).is_ok())
                    .then_some(index)
                })
                .collect::<Vec<_>>()
        });
        let body_result = if let Some(indices) = selected_indices.as_deref() {
            self.emit_block_body_indices(&program.body, persist_top_level, true, indices)
        } else {
            self.emit_block_body(&program.body, persist_top_level, true)
        };
        let body = std::mem::take(&mut self.bytes);
        self.bytes = saved_bytes;
        self.bound_names = saved_bound_names;
        self.block_keys = saved_block_keys;
        if let Err(error) = body_result {
            self.function_defs.truncate(saved_function_defs_len);
            return Err(error);
        }
        self.bytes.extend_from_slice(&body);
        Ok(())
    }

    fn emit_expr_at_depth(&mut self, expr: &Expr<'_>, depth: usize) -> Result<(), EmitError> {
        if depth >= MAX_EXPR_DEPTH {
            return Err(EmitError::DepthLimit);
        }
        // Stable MethodOperation indices describe direct program statements.
        // Consume this permission before descending so a reflective send in a
        // method body, block, let value, receiver, or argument remains an
        // ordinary runtime send and cannot shift later top-level selections.
        let artifact_operation_root = std::mem::take(&mut self.artifact_operation_root);
        match expr {
            Expr::SmallInt(value) => self.emit_small_int(*value),
            Expr::SmallIntLiteral(literal) => self.emit_small_int_literal(literal),
            Expr::Float(literal) => self.emit_float(literal),
            Expr::String(bytes) => self.emit_string(bytes),
            Expr::StringLiteral(source) => self.emit_string_literal(source),
            Expr::StringOwned(bytes) => self.emit_string(bytes),
            Expr::Symbol(name) => self.emit_symbol(name),
            Expr::Global(name) => self.emit_reference(name),
            Expr::Nil => self.emit_nil(),
            Expr::True => self.emit_true(),
            Expr::False => self.emit_false(),
            Expr::Block(block) => self.emit_block(block)?,
            Expr::Send {
                receiver,
                selector,
                args,
                source,
            } => {
                if self.artifact_mode
                    && artifact_operation_root
                    && matches!(receiver.as_ref(), Expr::Global(name) if *name == b"Reflect")
                    && ((selector.as_ref() == b"defineMethod:on:as:" && args.len() == 3)
                        || (selector.as_ref() == b"defineMethod:on:as:doc:" && args.len() == 4))
                {
                    return self.emit_artifact_method_install(args, source);
                }
                if matches!(receiver.as_ref(), Expr::Global(name) if *name == b"Reflect")
                    && ((selector.as_ref() == b"defineMethod:on:as:" && args.len() == 3)
                        || (selector.as_ref() == b"defineMethod:on:as:doc:" && args.len() == 4))
                {
                    return self
                        .emit_reflect_method_sourced(receiver, selector, args, source, depth);
                }
                if self.artifact_mode
                    && artifact_operation_root
                    && matches!(receiver.as_ref(), Expr::Global(name) if *name == b"Reflect")
                    && selector.as_ref() == b"removeMethod:on:"
                    && args.len() == 2
                {
                    return self.emit_artifact_method_remove(args, source);
                }
                self.emit_sequenced_send(receiver, selector, args, None, depth)?;
            }
        }
        Ok(())
    }

    fn emit_root_expr(
        &mut self,
        expr: &Expr<'_>,
        depth: usize,
        artifact_operation_root: bool,
    ) -> Result<(), EmitError> {
        let saved = std::mem::replace(&mut self.artifact_operation_root, artifact_operation_root);
        let result = self.emit_expr_at_depth(expr, depth);
        self.artifact_operation_root = saved;
        result
    }

    fn emit_reflect_method_sourced(
        &mut self,
        receiver: &Expr<'_>,
        selector: &[u8],
        args: &[Expr<'_>],
        source: &[u8],
        depth: usize,
    ) -> Result<(), EmitError> {
        let sourced_selector = if selector == b"defineMethod:on:as:doc:" {
            b"defineMethod:on:as:doc:source:".as_slice()
        } else {
            b"defineMethod:on:as:source:".as_slice()
        };
        self.emit_sequenced_send(receiver, sourced_selector, args, Some(source), depth)
    }

    fn emit_sequenced_send(
        &mut self,
        receiver: &Expr<'_>,
        selector: &[u8],
        args: &[Expr<'_>],
        extra_source: Option<&[u8]>,
        depth: usize,
    ) -> Result<(), EmitError> {
        let send = self.next_send;
        self.next_send += 1;
        self.bytes.extend_from_slice(b"({void* __birnam_send_");
        self.bytes.extend_from_slice(send.to_string().as_bytes());
        self.bytes.extend_from_slice(b"_receiver=");
        self.emit_expr_at_depth(receiver, depth + 1)?;
        self.bytes.extend_from_slice(b";");

        for (index, argument) in args.iter().enumerate() {
            self.bytes.extend_from_slice(b"void* __birnam_send_");
            self.bytes.extend_from_slice(send.to_string().as_bytes());
            self.bytes.extend_from_slice(b"_arg_");
            self.bytes.extend_from_slice(index.to_string().as_bytes());
            self.bytes.extend_from_slice(b"=");
            self.emit_expr_at_depth(argument, depth + 1)?;
            self.bytes.extend_from_slice(b";");
        }
        if let Some(source) = extra_source {
            self.bytes.extend_from_slice(b"void* __birnam_send_");
            self.bytes.extend_from_slice(send.to_string().as_bytes());
            self.bytes.extend_from_slice(b"_arg_");
            self.bytes
                .extend_from_slice(args.len().to_string().as_bytes());
            self.bytes.extend_from_slice(b"=");
            self.emit_string(source);
            self.bytes.extend_from_slice(b";");
        }

        let argument_count = args.len() + usize::from(extra_source.is_some());
        if argument_count != 0 {
            self.bytes.extend_from_slice(b"void* __birnam_send_");
            self.bytes.extend_from_slice(send.to_string().as_bytes());
            self.bytes.extend_from_slice(b"_args[]={");
            for index in 0..argument_count {
                if index != 0 {
                    self.bytes.extend_from_slice(b",");
                }
                self.bytes.extend_from_slice(b"__birnam_send_");
                self.bytes.extend_from_slice(send.to_string().as_bytes());
                self.bytes.extend_from_slice(b"_arg_");
                self.bytes.extend_from_slice(index.to_string().as_bytes());
            }
            self.bytes.extend_from_slice(b"};");
        }

        self.bytes.extend_from_slice(b"birnam_send(__birnam_send_");
        self.bytes.extend_from_slice(send.to_string().as_bytes());
        self.bytes.extend_from_slice(b"_receiver,birnam_intern(\"");
        self.push_c_string_body(selector);
        if argument_count == 0 {
            self.bytes.extend_from_slice(b"\"),0,0L);")
        } else {
            self.bytes.extend_from_slice(b"\"),__birnam_send_");
            self.bytes.extend_from_slice(send.to_string().as_bytes());
            self.bytes.extend_from_slice(b"_args,");
            self.bytes
                .extend_from_slice(argument_count.to_string().as_bytes());
            self.bytes.extend_from_slice(b"L);");
        }
        self.bytes.extend_from_slice(b"})");
        Ok(())
    }

    fn emit_artifact_method_install(
        &mut self,
        args: &[Expr<'_>],
        source: &[u8],
    ) -> Result<(), EmitError> {
        let (selector, block) = match (&args[0], &args[2]) {
            (Expr::Symbol(selector), Expr::Block(block)) => (*selector, block),
            _ => return Err(EmitError::Unsupported),
        };
        let operation_index = self.artifact_operation_counter;
        self.artifact_operation_counter = self
            .artifact_operation_counter
            .checked_add(1)
            .ok_or(EmitError::Unsupported)?;
        let selected = self
            .named_resumables
            .iter()
            .find(|entry| entry.operation_index == operation_index)
            .cloned();
        let arity = selector_arity(selector);
        let method_number = self.artifact_method_counter;
        self.artifact_method_counter += 1;

        // Emit the block expression into a temporary buffer. Its function
        // definition is appended to `function_defs`, while the expression
        // itself becomes the candidate-root initializer below.
        let saved_bytes = std::mem::take(&mut self.bytes);
        self.bytes = Vec::new();
        self.emit_block(block)?;
        let block_expression = std::mem::take(&mut self.bytes);
        self.bytes = saved_bytes;

        if selected.is_none() {
            self.function_defs.extend_from_slice(b"void* __amtramp");
            self.function_defs
                .extend_from_slice(method_number.to_string().as_bytes());
            self.function_defs
                .extend_from_slice(b"(void* self, void** args, long n){ (void)n; void* a[");
            self.function_defs
                .extend_from_slice((arity + 1).to_string().as_bytes());
            self.function_defs
                .extend_from_slice(b"]; a[0]=self; long i; for(i=0;i<");
            self.function_defs
                .extend_from_slice(arity.to_string().as_bytes());
            self.function_defs.extend_from_slice(
                b";i++) a[i+1]=args[i]; return birnam_send(birnam_candidate_root_get(",
            );
            self.function_defs
                .extend_from_slice(b"__birnam_code_generation_identity,");
            self.function_defs
                .extend_from_slice(method_number.to_string().as_bytes());
            self.function_defs
                .extend_from_slice(b"L), birnam_intern(\"");
            for _ in 0..arity + 1 {
                self.function_defs.extend_from_slice(b"value:");
            }
            self.function_defs.extend_from_slice(b"\"), a, ");
            self.function_defs
                .extend_from_slice((arity + 1).to_string().as_bytes());
            self.function_defs.extend_from_slice(b"); }\n");
        }

        self.bytes
            .extend_from_slice(b"({ void* _vt = birnam_ensurePrivateVtable(");
        self.emit_expr_at_depth(&args[1], 1)?;
        self.bytes
            .extend_from_slice(b"); void* _blk = birnam_candidate_root_get(");
        self.bytes
            .extend_from_slice(b"__birnam_code_generation_identity,");
        self.bytes
            .extend_from_slice(method_number.to_string().as_bytes());
        self.bytes
            .extend_from_slice(b"L); if(_blk == birnam_nil()){ _blk = birnam_candidate_root_set(");
        self.bytes
            .extend_from_slice(b"__birnam_code_generation_identity,");
        self.bytes
            .extend_from_slice(method_number.to_string().as_bytes());
        self.bytes.extend_from_slice(b"L, ");
        self.bytes.extend_from_slice(&block_expression);
        self.bytes.extend_from_slice(b"); }");
        if let Some(selected) = selected.as_ref() {
            self.bytes.extend_from_slice(
                b" if(!birnam_candidate_preparing()){if(__birnam_resumable_prepare_",
            );
            self.bytes
                .extend_from_slice(selected.symbol_index.to_string().as_bytes());
            self.bytes.extend_from_slice(
                b"(_blk)!=0U)return birnam_nil(); void* _installed=birnam_install_method(_vt, birnam_intern(\"",
            );
        } else {
            self.bytes
                .extend_from_slice(b" birnam_install_method(_vt, birnam_intern(\"");
        }
        self.push_c_string_body(selector);
        if let Some(selected) = selected.as_ref() {
            self.bytes
                .extend_from_slice(b"\"), (void*)&__birnam_resumable_thunk_");
            self.bytes
                .extend_from_slice(selected.symbol_index.to_string().as_bytes());
        } else {
            self.bytes.extend_from_slice(b"\"), (void*)&__amtramp");
            self.bytes
                .extend_from_slice(method_number.to_string().as_bytes());
        }
        self.bytes.extend_from_slice(b", \"");
        self.push_c_string_body(source);
        self.bytes.extend_from_slice(b"\", ");
        self.bytes
            .extend_from_slice(source.len().to_string().as_bytes());
        if let Some(selected) = selected {
            self.bytes.extend_from_slice(
                b"L); if(!_installed||_installed==birnam_nil()){if(__birnam_resumable_abort_",
            );
            self.bytes
                .extend_from_slice(selected.symbol_index.to_string().as_bytes());
            self.bytes
                .extend_from_slice(b"()!=0U)return birnam_nil();return birnam_nil();}} _vt; })");
        } else {
            self.bytes.extend_from_slice(b"L); _vt; })");
        }
        Ok(())
    }

    fn emit_artifact_method_remove(
        &mut self,
        args: &[Expr<'_>],
        source: &[u8],
    ) -> Result<(), EmitError> {
        self.artifact_operation_counter = self
            .artifact_operation_counter
            .checked_add(1)
            .ok_or(EmitError::Unsupported)?;
        let selector = match &args[0] {
            Expr::Symbol(selector) => *selector,
            _ => return Err(EmitError::Unsupported),
        };
        self.bytes.extend_from_slice(b"birnam_remove_method(");
        self.emit_expr_at_depth(&args[1], 1)?;
        self.bytes.extend_from_slice(b", birnam_intern(\"");
        self.push_c_string_body(selector);
        self.bytes.extend_from_slice(b"\"), \"");
        self.push_c_string_body(source);
        self.bytes.extend_from_slice(b"\", ");
        self.bytes
            .extend_from_slice(source.len().to_string().as_bytes());
        self.bytes.extend_from_slice(b"L)");
        Ok(())
    }

    fn emit_block(&mut self, block: &Block<'_>) -> Result<(), EmitError> {
        let captures = resumable::lexical_capture_names(block, &self.bound_names);
        let portable_program =
            portable::encode_block(block, &self.bound_names).map_err(|error| {
                if error == portable::Error::DepthLimit {
                    EmitError::DepthLimit
                } else {
                    EmitError::Unsupported
                }
            })?;
        let capture_values = captures
            .iter()
            .map(|name| BlockCaptureValue::SourceBinding(name.as_slice()))
            .collect::<Vec<_>>();
        self.emit_portable_block(block, &portable_program, &capture_values)
    }

    fn emit_portable_block<'a>(
        &mut self,
        source_block: &'a Block<'a>,
        portable_program: &[u8],
        capture_values: &[BlockCaptureValue<'_>],
    ) -> Result<(), EmitError> {
        let program = birnam_artifact::portable_program::inspect(portable_program)
            .map_err(|_| EmitError::Unsupported)?;
        if capture_values.len() != program.capture_count as usize {
            return Err(EmitError::Unsupported);
        }
        let capture_names = (0..program.capture_count)
            .map(|index| {
                program
                    .binding((program.parameter_count + index) as usize)
                    .map(|binding| binding.name.to_vec())
                    .ok_or(EmitError::Unsupported)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let key = self
            .block_keys
            .iter()
            .find(|(pointer, _)| *pointer == source_block as *const Block<'_> as usize)
            .map(|(_, key)| key);
        let selected = key.and_then(|key| {
            self.block_resumables
                .iter()
                .find(|entry| entry.key == *key)
                .cloned()
        });
        if selected
            .as_ref()
            .is_some_and(|entry| entry.captures != capture_names)
        {
            return Err(EmitError::Unsupported);
        }

        let number = self.next_block;
        self.next_block += 1;
        let number_u64 = u64::try_from(number).map_err(|_| EmitError::Unsupported)?;
        if number_u64 >= 1_u64 << 61 {
            return Err(EmitError::Unsupported);
        }
        let code_object_cache_key = (1_u64 << 61) | number_u64;
        let saved_function_defs_len = self.function_defs.len();
        let saved_bytes = std::mem::take(&mut self.bytes);
        self.bytes = Vec::new();
        let body_result = self.emit_portable_block_body(source_block, portable_program);
        let body = std::mem::take(&mut self.bytes);
        self.bytes = saved_bytes;
        if let Err(error) = body_result {
            self.function_defs.truncate(saved_function_defs_len);
            return Err(error);
        }

        self.function_defs.extend_from_slice(b"void* __blk");
        self.function_defs
            .extend_from_slice(number.to_string().as_bytes());
        self.function_defs.extend_from_slice(
            b"(void* self, void** args, long n){ (void)self;(void)args;(void)n;\n",
        );
        self.function_defs.extend_from_slice(&body);
        self.function_defs.extend_from_slice(b"}\n");

        self.bytes.extend_from_slice(b"({ ");
        if let Some(selected) = selected.as_ref() {
            self.bytes.extend_from_slice(b"void* __vt");
            self.bytes.extend_from_slice(number.to_string().as_bytes());
            self.bytes
                .extend_from_slice(b"=0;if(!birnam_candidate_preparing()){__vt");
            self.bytes.extend_from_slice(number.to_string().as_bytes());
            self.bytes
                .extend_from_slice(b"=__birnam_resolve_block_vtable_v1(");
            self.bytes
                .extend_from_slice(selected.manifest_entry_index.to_string().as_bytes());
            self.bytes.extend_from_slice(b"U);if(!__vt");
            self.bytes.extend_from_slice(number.to_string().as_bytes());
            self.bytes.extend_from_slice(b")return birnam_nil();}");
        } else {
            self.bytes.extend_from_slice(b"void* __vt");
            self.bytes.extend_from_slice(number.to_string().as_bytes());
            self.bytes
                .extend_from_slice(b"=birnam_vtable_cache_get(__birnam_code_generation_identity,");
            self.bytes.extend_from_slice(number.to_string().as_bytes());
            self.bytes.extend_from_slice(b"ULL);if(!__vt");
            self.bytes.extend_from_slice(number.to_string().as_bytes());
            self.bytes.extend_from_slice(b"){ __vt");
            self.bytes.extend_from_slice(number.to_string().as_bytes());
            self.bytes
                .extend_from_slice(b"=birnam_vtable_cache_set(__birnam_code_generation_identity,");
            self.bytes.extend_from_slice(number.to_string().as_bytes());
            self.bytes
                .extend_from_slice(b"ULL,birnam_proto(birnam_blockVt()));birnam_method(__vt");
            self.bytes.extend_from_slice(number.to_string().as_bytes());
            self.bytes.extend_from_slice(b", birnam_intern(\"");
            if program.parameter_count == 0 {
                self.bytes.extend_from_slice(b"value");
            } else {
                for _ in 0..program.parameter_count {
                    self.bytes.extend_from_slice(b"value:");
                }
            }
            self.bytes.extend_from_slice(b"\"), (void*)&__blk");
            self.bytes.extend_from_slice(number.to_string().as_bytes());
            self.bytes
                .extend_from_slice(b"); birnam_layoutAllOops(__vt");
            self.bytes.extend_from_slice(number.to_string().as_bytes());
            self.bytes.extend_from_slice(b"); }");
        }
        self.bytes.extend_from_slice(b" void* __o");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes.extend_from_slice(b"=birnam_nil();if(__vt");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes
            .extend_from_slice(b"){static const unsigned char __pp");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes.extend_from_slice(b"[]={");
        for (index, byte) in portable_program.iter().enumerate() {
            if index != 0 {
                self.bytes.extend_from_slice(b",");
            }
            self.bytes.extend_from_slice(byte.to_string().as_bytes());
        }
        self.bytes.extend_from_slice(b"};void* __co");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes
            .extend_from_slice(b"=birnam_vtable_cache_get(__birnam_code_generation_identity,");
        self.bytes
            .extend_from_slice(code_object_cache_key.to_string().as_bytes());
        self.bytes.extend_from_slice(b"ULL);if(!__co");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes.extend_from_slice(b"){__co");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes
            .extend_from_slice(b"=birnam_portable_code_admit_v1(__pp");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes.extend_from_slice(b",sizeof(__pp");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes.extend_from_slice(b"));if(!__co");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes
            .extend_from_slice(b")return birnam_nil();if(birnam_is_error(__co");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes.extend_from_slice(b"))return __co");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes.extend_from_slice(b";__co");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes
            .extend_from_slice(b"=birnam_vtable_cache_set(__birnam_code_generation_identity,");
        self.bytes
            .extend_from_slice(code_object_cache_key.to_string().as_bytes());
        self.bytes.extend_from_slice(b"ULL,__co");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes.extend_from_slice(b");if(!__co");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes.extend_from_slice(b")return birnam_nil();}");
        if !capture_values.is_empty() {
            self.bytes.extend_from_slice(b"void* __captures");
            self.bytes.extend_from_slice(number.to_string().as_bytes());
            self.bytes.extend_from_slice(b"[");
            self.bytes
                .extend_from_slice(capture_values.len().to_string().as_bytes());
            self.bytes.extend_from_slice(b"];");
            for (index, value) in capture_values.iter().enumerate() {
                self.bytes.extend_from_slice(b"__captures");
                self.bytes.extend_from_slice(number.to_string().as_bytes());
                self.bytes.extend_from_slice(b"[");
                self.bytes.extend_from_slice(index.to_string().as_bytes());
                self.bytes.extend_from_slice(b"]=");
                self.emit_block_capture_value(*value);
                self.bytes.extend_from_slice(b";");
            }
        }
        self.bytes.extend_from_slice(b"__o");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes
            .extend_from_slice(b"=birnam_portable_block_new_v1(__vt");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes.extend_from_slice(b",__co");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes.extend_from_slice(b",");
        if capture_values.is_empty() {
            self.bytes.extend_from_slice(b"(void**)0");
        } else {
            self.bytes.extend_from_slice(b"__captures");
            self.bytes.extend_from_slice(number.to_string().as_bytes());
        }
        self.bytes.extend_from_slice(b",");
        self.bytes
            .extend_from_slice(capture_values.len().to_string().as_bytes());
        self.bytes.extend_from_slice(b"UL);if(!__o");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes
            .extend_from_slice(b")return birnam_nil();if(birnam_is_error(__o");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes.extend_from_slice(b"))return __o");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes.extend_from_slice(b";}");
        self.bytes.extend_from_slice(b"__o");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes.extend_from_slice(b"; })");
        Ok(())
    }

    fn emit_block_capture_value(&mut self, value: BlockCaptureValue<'_>) {
        match value {
            BlockCaptureValue::SourceBinding(name) => {
                self.bytes.extend_from_slice(b"b_");
                self.bytes.extend_from_slice(name);
            }
            BlockCaptureValue::PortableBinding(index) => {
                self.bytes.extend_from_slice(b"__b");
                self.bytes.extend_from_slice(index.to_string().as_bytes());
            }
        }
    }

    fn emit_portable_block_body<'a>(
        &mut self,
        source_block: &'a Block<'a>,
        portable_program: &[u8],
    ) -> Result<(), EmitError> {
        use birnam_artifact::portable_program::{NodeKind, StatementKind};

        let program = birnam_artifact::portable_program::inspect(portable_program)
            .map_err(|_| EmitError::Unsupported)?;
        for index in 0..program.parameter_count {
            self.bytes.extend_from_slice(b"void* __b");
            self.bytes.extend_from_slice(index.to_string().as_bytes());
            self.bytes.extend_from_slice(b"=args[");
            self.bytes.extend_from_slice(index.to_string().as_bytes());
            self.bytes.extend_from_slice(b"];\n");
        }
        for capture in 0..program.capture_count {
            let binding = program
                .parameter_count
                .checked_add(capture)
                .ok_or(EmitError::Unsupported)?;
            self.bytes.extend_from_slice(b"void* __b");
            self.bytes.extend_from_slice(binding.to_string().as_bytes());
            self.bytes.extend_from_slice(b"=((void**)self)[");
            self.bytes
                .extend_from_slice((capture + 1).to_string().as_bytes());
            self.bytes.extend_from_slice(b"];\n");
        }

        let mut source_nodes = Vec::new();
        for statement in &source_block.body {
            let expression = match statement {
                Statement::Let { value, .. } | Statement::Expr(value) => value,
            };
            collect_portable_node_sources(expression, &mut source_nodes);
        }
        if source_nodes.len() != program.node_count as usize {
            return Err(EmitError::Unsupported);
        }

        let mut next_node = 0u32;
        let mut result_node = None;
        for statement_index in 0..program.statement_count as usize {
            let statement = program
                .statement(statement_index)
                .ok_or(EmitError::Unsupported)?;
            while next_node <= statement.root_node {
                let node = program
                    .node(next_node as usize)
                    .ok_or(EmitError::Unsupported)?;
                self.bytes.extend_from_slice(b"void* __n");
                self.bytes
                    .extend_from_slice(next_node.to_string().as_bytes());
                self.bytes.extend_from_slice(b"=");
                match node.semantic_kind().ok_or(EmitError::Unsupported)? {
                    NodeKind::Integer => self.emit_small_int(node.payload as i64),
                    NodeKind::FloatBits => self.emit_float_bits(node.payload),
                    NodeKind::String => self.emit_string(node.data),
                    NodeKind::Symbol => self.emit_symbol(node.data),
                    NodeKind::Nil => self.emit_nil(),
                    NodeKind::True => self.emit_true(),
                    NodeKind::False => self.emit_false(),
                    NodeKind::LexicalRead => {
                        self.bytes.extend_from_slice(b"__b");
                        self.bytes
                            .extend_from_slice(node.auxiliary_0.to_string().as_bytes());
                    }
                    NodeKind::GlobalRead => self.emit_global(node.data),
                    NodeKind::Send => {
                        let source = *source_nodes
                            .get(next_node as usize)
                            .ok_or(EmitError::Unsupported)?;
                        let receiver = program
                            .node_operand(next_node as usize, 0)
                            .ok_or(EmitError::Unsupported)?;
                        self.bytes.extend_from_slice(b"birnam_send(__n");
                        self.bytes
                            .extend_from_slice(receiver.to_string().as_bytes());
                        self.bytes.extend_from_slice(b",birnam_intern(\"");
                        let sourced = sourced_reflect_selector(source, node.data);
                        self.push_c_string_body(
                            sourced.map(|(selector, _)| selector).unwrap_or(node.data),
                        );
                        self.bytes.extend_from_slice(b"\"),");
                        let extra_source = sourced.map(|(_, source)| source);
                        if node.operand_count == 1 && extra_source.is_none() {
                            self.bytes.extend_from_slice(b"(void**)0");
                        } else {
                            self.bytes.extend_from_slice(b"(void*[]){");
                            for operand_index in 1..node.operand_count as usize {
                                if operand_index != 1 {
                                    self.bytes.extend_from_slice(b",");
                                }
                                let operand = program
                                    .node_operand(next_node as usize, operand_index)
                                    .ok_or(EmitError::Unsupported)?;
                                self.bytes.extend_from_slice(b"__n");
                                self.bytes.extend_from_slice(operand.to_string().as_bytes());
                            }
                            if let Some(source) = extra_source {
                                if node.operand_count != 1 {
                                    self.bytes.extend_from_slice(b",");
                                }
                                self.emit_string(source);
                            }
                            self.bytes.extend_from_slice(b"}");
                        }
                        self.bytes.extend_from_slice(b",");
                        self.bytes.extend_from_slice(
                            node.operand_count
                                .checked_sub(1)
                                .and_then(|count| {
                                    count.checked_add(u32::from(extra_source.is_some()))
                                })
                                .ok_or(EmitError::Unsupported)?
                                .to_string()
                                .as_bytes(),
                        );
                        self.bytes.extend_from_slice(b"L)");
                    }
                    NodeKind::BlockLiteral => {
                        let source = match source_nodes.get(next_node as usize) {
                            Some(Expr::Block(block)) => block.as_ref(),
                            _ => return Err(EmitError::Unsupported),
                        };
                        let captures = (0..node.operand_count as usize)
                            .map(|operand_index| {
                                program
                                    .node_operand(next_node as usize, operand_index)
                                    .map(BlockCaptureValue::PortableBinding)
                                    .ok_or(EmitError::Unsupported)
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        self.emit_portable_block(source, node.data, &captures)?;
                    }
                }
                self.bytes.extend_from_slice(b";\n");
                next_node = next_node.checked_add(1).ok_or(EmitError::Unsupported)?;
            }
            if statement.kind == StatementKind::Let {
                let binding = statement.binding_index.ok_or(EmitError::Unsupported)?;
                self.bytes.extend_from_slice(b"void* __b");
                self.bytes.extend_from_slice(binding.to_string().as_bytes());
                self.bytes.extend_from_slice(b"=__n");
                self.bytes
                    .extend_from_slice(statement.root_node.to_string().as_bytes());
                self.bytes.extend_from_slice(b";\n");
            }
            result_node = Some(statement.root_node);
        }
        if next_node != program.node_count {
            return Err(EmitError::Unsupported);
        }
        self.bytes.extend_from_slice(b"return ");
        if let Some(result) = result_node {
            self.bytes.extend_from_slice(b"__n");
            self.bytes.extend_from_slice(result.to_string().as_bytes());
        } else {
            self.emit_nil();
        }
        self.bytes.extend_from_slice(b";\n");
        Ok(())
    }

    fn emit_block_body(
        &mut self,
        body: &[Statement<'_>],
        persist_top_level: bool,
        artifact_operations: bool,
    ) -> Result<(), EmitError> {
        let indices = (0..body.len()).collect::<Vec<_>>();
        self.emit_block_body_indices(body, persist_top_level, artifact_operations, &indices)
    }

    fn emit_block_body_indices(
        &mut self,
        body: &[Statement<'_>],
        persist_top_level: bool,
        artifact_operations: bool,
        indices: &[usize],
    ) -> Result<(), EmitError> {
        if indices.is_empty() {
            self.bytes.extend_from_slice(b"return birnam_nil();\n");
            return Ok(());
        }
        for (position, index) in indices.iter().copied().enumerate() {
            let statement = body.get(index).ok_or(EmitError::Unsupported)?;
            let last = position + 1 == indices.len();
            match statement {
                Statement::Let { name, value } => {
                    self.bytes.extend_from_slice(b"void* b_");
                    self.bytes.extend_from_slice(name);
                    self.bytes.extend_from_slice(b" = ");
                    self.emit_root_expr(value, 1, false)?;
                    self.bytes.extend_from_slice(b";\n");
                    if persist_top_level {
                        self.bytes
                            .extend_from_slice(b"void* __birnam_global_set_result_");
                        self.bytes.extend_from_slice(index.to_string().as_bytes());
                        self.bytes.extend_from_slice(b" = birnam_global_set(\"");
                        self.push_c_string_body(name);
                        self.bytes.extend_from_slice(b"\", b_");
                        self.bytes.extend_from_slice(name);
                        self.bytes
                            .extend_from_slice(b");\nif(__birnam_global_set_result_");
                        self.bytes.extend_from_slice(index.to_string().as_bytes());
                        self.bytes.extend_from_slice(b" != b_");
                        self.bytes.extend_from_slice(name);
                        self.bytes
                            .extend_from_slice(b") return __birnam_global_set_result_");
                        self.bytes.extend_from_slice(index.to_string().as_bytes());
                        self.bytes.extend_from_slice(b";\n");
                    }
                    self.bound_names.push(name.to_vec());
                    if last {
                        self.bytes.extend_from_slice(b"return b_");
                        self.bytes.extend_from_slice(name);
                        self.bytes.extend_from_slice(b";\n");
                    }
                }
                Statement::Expr(expression) => {
                    if last {
                        self.bytes.extend_from_slice(b"return ");
                        self.emit_root_expr(expression, 1, artifact_operations)?;
                        self.bytes.extend_from_slice(b";\n");
                    } else {
                        self.bytes.extend_from_slice(b"(void)(");
                        self.emit_root_expr(expression, 1, artifact_operations)?;
                        self.bytes.extend_from_slice(b");\n");
                    }
                }
            }
        }
        Ok(())
    }

    pub fn finish(self) -> Vec<u8> {
        let (function_defs, bytes) = self.finish_parts();
        let mut output = function_defs;
        output.extend_from_slice(&bytes);
        output
    }

    pub fn finish_parts(self) -> (Vec<u8>, Vec<u8>) {
        (self.function_defs, self.bytes)
    }

    pub fn baked_globals(&self) -> &[Vec<u8>] {
        &self.baked_globals
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::fs;
    use std::process::Command;

    use birnam_artifact::stackless_selection::{
        Disposition, ImplementationKind, RootKind, SelectionSpec,
    };

    use super::{
        Block, BlockResumableEmission, CstNode, EmitError, Emitter, Expr, LowerError,
        NamedResumableEmission, OwnedCst, Program, Statement, TranslationUnit, lower_expr,
        lower_literal, lower_program, selector_arity,
    };

    use crate::resumable::{ImplementationKey, InventoryImplementationKind, InventoryRootKind};

    struct TestNode {
        rule: &'static [u8],
        start: usize,
        end: usize,
        children: Vec<TestNode>,
    }

    impl CstNode for TestNode {
        fn rule(&self) -> &[u8] {
            self.rule
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
    }

    fn node(rule: &'static [u8], start: usize, end: usize, children: Vec<TestNode>) -> TestNode {
        TestNode {
            rule,
            start,
            end,
            children,
        }
    }

    #[test]
    fn escapes_like_reference_codegen() {
        let mut emitter = Emitter::new();
        emitter.emit_string(b"a\n\0\xff\\\"");
        assert_eq!(
            emitter.finish(),
            br#"birnam_mkString("a\n\000\377\\\"", 6L)"#
        );
    }

    #[test]
    fn emits_literals_globals_and_sends_without_intermediate_strings() {
        let mut emitter = Emitter::with_capacity(128);
        emitter.emit_small_int(42);
        emitter.push(b"; ");
        emitter.emit_global(b"Integer");
        emitter.push(b"; ");
        emitter.emit_send(b"MKV(1L)", b"+", &[b"MKV(2L)"]);
        assert_eq!(
            String::from_utf8(emitter.finish()).unwrap(),
            "MKV(42L); birnam_global(\"Integer\"); birnam_send(MKV(1L), birnam_intern(\"+\"), (void*[]){MKV(2L)}, 1L)"
        );
    }

    #[test]
    fn emits_pure_expression_ir_and_refuses_unbounded_depth() {
        let expression = Expr::Send {
            receiver: Box::new(Expr::Global(b"Integer")),
            selector: Cow::Borrowed(b"at:"),
            args: vec![Expr::String(b"answer")],
            source: b"",
        };
        let mut emitter = Emitter::new();
        emitter.emit_expr(&expression).unwrap();
        assert_eq!(
            String::from_utf8(emitter.finish()).unwrap(),
            "({void* __birnam_send_0_receiver=birnam_global_b((void*)__g0);void* __birnam_send_0_arg_0=birnam_mkString(\"answer\", 6L);void* __birnam_send_0_args[]={__birnam_send_0_arg_0};birnam_send(__birnam_send_0_receiver,birnam_intern(\"at:\"),__birnam_send_0_args,1L);})"
        );

        let result = std::thread::Builder::new()
            .stack_size(16 << 20)
            .spawn(|| {
                let mut deep = Expr::SmallInt(0);
                for _ in 0..4096 {
                    deep = Expr::Send {
                        receiver: Box::new(deep),
                        selector: Cow::Borrowed(b"value"),
                        args: Vec::new(),
                        source: b"",
                    };
                }
                let mut emitter = Emitter::new();
                let result = emitter.emit_expr(&deep);
                // Keep the adversarial fixture from recursively dropping on
                // the worker stack after the depth guard has been exercised.
                std::mem::forget(deep);
                result
            })
            .expect("depth-limit worker starts")
            .join()
            .expect("depth-limit worker exits");
        assert_eq!(result, Err(EmitError::DepthLimit));
    }

    #[test]
    fn lowers_candidate_method_install_to_rooted_trampoline() {
        let expression = Expr::Send {
            receiver: Box::new(Expr::Global(b"Reflect")),
            selector: Cow::Borrowed(b"defineMethod:on:as:"),
            args: vec![
                Expr::Symbol(b"answer"),
                Expr::Global(b"Integer"),
                Expr::Block(Box::new(Block {
                    params: vec![b"self"],
                    body: vec![Statement::Expr(Expr::SmallInt(71))],
                })),
            ],
            source: b"[Reflect defineMethod: #answer on: Integer as: [:self | 71]]",
        };
        let mut emitter = Emitter::new_candidate_artifact(7);
        emitter.emit_expr(&expression).unwrap();
        let output = String::from_utf8(emitter.finish()).unwrap();
        assert!(output.contains("__amtramp0"));
        assert!(output.contains("birnam_candidate_root_get(__birnam_code_generation_identity,0L)"));
        assert!(output.contains("birnam_install_method"));
        assert!(output.contains("birnam_intern(\"answer\")"));
    }

    #[test]
    fn projected_candidate_emits_only_methods_and_named_safe_initializers() {
        let side_effect = Expr::Send {
            receiver: Box::new(Expr::Global(b"System")),
            selector: Cow::Borrowed(b"println:"),
            args: vec![Expr::String(b"must not run")],
            source: b"[System println: \"must not run\"]",
        };
        let projected = Statement::Let {
            name: b"projectedClosure",
            value: Expr::Block(Box::new(Block {
                params: Vec::new(),
                body: vec![Statement::Expr(Expr::SmallInt(99))],
            })),
        };
        let method = Expr::Send {
            receiver: Box::new(Expr::Global(b"Reflect")),
            selector: Cow::Borrowed(b"defineMethod:on:as:"),
            args: vec![
                Expr::Symbol(b"answer"),
                Expr::Global(b"Integer"),
                Expr::Block(Box::new(Block {
                    params: vec![b"self"],
                    body: vec![Statement::Expr(Expr::SmallInt(71))],
                })),
            ],
            source: b"[Reflect defineMethod: #answer on: Integer as: [:self | 71]]",
        };
        let program = Program {
            body: vec![
                Statement::Expr(side_effect),
                projected,
                Statement::Expr(method),
            ],
        };
        let mut emitter = Emitter::new_candidate_artifact(7);
        emitter
            .emit_projected_candidate_program(&program, &[1])
            .unwrap();
        let output = String::from_utf8(emitter.finish()).unwrap();
        assert!(output.contains("birnam_global_set(\"projectedClosure\""));
        assert!(output.contains("birnam_install_method"));
        assert!(!output.contains("must not run"));
        assert!(!output.contains("println:"));
    }

    #[test]
    fn image_recovery_emits_methods_and_projected_roots_without_effectful_initializers() {
        let initializer = Expr::Send {
            receiver: Box::new(Expr::Global(b"System")),
            selector: Cow::Borrowed(b"println:"),
            args: vec![Expr::String(b"must not run")],
            source: b"[System println: \"must not run\"]",
        };
        let method = Expr::Send {
            receiver: Box::new(Expr::Global(b"Reflect")),
            selector: Cow::Borrowed(b"defineMethod:on:as:"),
            args: vec![
                Expr::Symbol(b"answer"),
                Expr::Global(b"Integer"),
                Expr::Block(Box::new(Block {
                    params: vec![b"self"],
                    body: vec![Statement::Expr(Expr::SmallInt(71))],
                })),
            ],
            source: b"[Reflect defineMethod: #answer on: Integer as: [:self | 71]]",
        };
        let projected = Statement::Let {
            name: b"recoveredClosure",
            value: Expr::Block(Box::new(Block {
                params: vec![],
                body: vec![Statement::Expr(Expr::SmallInt(73))],
            })),
        };
        let program = Program {
            body: vec![
                Statement::Expr(initializer),
                projected,
                Statement::Expr(method),
            ],
        };
        let mut emitter = Emitter::new_candidate_artifact(7);
        emitter.emit_image_recovery_program(&program).unwrap();
        let output = String::from_utf8(emitter.finish()).unwrap();
        assert!(output.contains("birnam_global_set(\"recoveredClosure\""));
        assert!(output.contains("birnam_install_method"));
        assert!(!output.contains("must not run"));
        assert!(!output.contains("println:"));
    }

    #[test]
    fn selected_named_adapter_prepares_before_publishing_its_unique_thunk() {
        let expression = Expr::Send {
            receiver: Box::new(Expr::Global(b"Reflect")),
            selector: Cow::Borrowed(b"defineMethod:on:as:"),
            args: vec![
                Expr::Symbol(b"answer"),
                Expr::Global(b"Integer"),
                Expr::Block(Box::new(Block {
                    params: vec![b"self"],
                    body: vec![Statement::Expr(Expr::SmallInt(71))],
                })),
            ],
            source: b"[Reflect defineMethod: #answer on: Integer as: [:self | 71]]",
        };
        let authority = SelectionSpec {
            source_revision: [0x31; 32],
            root_kind: RootKind::MethodOperation,
            root_index: 0,
            path: &[],
            implementation_kind: ImplementationKind::NamedAdapter,
            disposition: Disposition::ResumableRequired,
        };
        let compiled = crate::resumable_named::compile_named_adapter_v1(
            b"answer",
            authority,
            b"module.birnam",
            b"[Reflect defineMethod: #answer on: Integer as: [:self | 71]]",
            7,
            0,
        )
        .unwrap();
        let mut emitter = Emitter::new_resumable_candidate_artifact(
            11,
            vec![NamedResumableEmission {
                operation_index: 0,
                symbol_index: 7,
                c_source: compiled.c_source,
            }],
        )
        .unwrap();
        emitter
            .emit_program(&Program {
                body: vec![Statement::Expr(expression)],
            })
            .unwrap();
        let baked_globals = emitter.baked_globals().to_vec();
        let (function_defs, body) = emitter.finish_parts();
        let mut unit = TranslationUnit::new(true);
        for name in baked_globals {
            unit.add_baked_global(&name);
        }
        unit.push_function(&function_defs);
        unit.push_body(&body);
        let output = String::from_utf8(unit.finish()).unwrap();
        assert!(output.contains("__birnam_resumable_context_begin_v1"));
        assert!(output.contains("__birnam_resumable_prepare_7(_blk)"));
        assert!(output.contains("(void*)&__birnam_resumable_thunk_7"));
        assert!(output.contains("__birnam_resumable_abort_7()"));
        assert!(!output.contains("__amtramp0"));

        let directory = std::env::temp_dir();
        let path = directory.join(format!(
            "birnam-selected-named-{}-{}.c",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        fs::write(&path, output).unwrap();
        let status = Command::new("cc")
            .args([
                "-std=gnu11",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-Wno-unused-variable",
                "-fsyntax-only",
            ])
            .arg(&path)
            .status()
            .unwrap();
        let _ = fs::remove_file(path);
        assert!(status.success());
    }

    #[test]
    fn selected_block_resolver_uses_manifest_order_across_source_revisions() {
        let block = |root_index, symbol_index, manifest_entry_index| BlockResumableEmission {
            key: ImplementationKey {
                root_kind: InventoryRootKind::MethodOperation,
                root_index,
                path: vec![0],
                implementation_kind: InventoryImplementationKind::BlockBody,
            },
            symbol_index,
            manifest_entry_index,
            selector: b"value".to_vec(),
            captures: Vec::new(),
            c_source: Vec::new(),
        };
        let emitter = Emitter::new_selected_resumable_candidate_artifact(
            11,
            Vec::new(),
            vec![block(7, 3, 9), block(8, 4, 2)],
        )
        .unwrap();
        let (functions, _) = emitter.finish_parts();
        let functions = String::from_utf8(functions).unwrap();
        let earlier = functions.find("case 2U:").unwrap();
        let later = functions.find("case 9U:").unwrap();
        assert!(earlier < later);
    }

    #[test]
    fn synchronous_portable_lowering_obeys_mixed_nested_block_selection() {
        let nested = || {
            Expr::Block(Box::new(Block {
                params: Vec::new(),
                body: vec![Statement::Expr(Expr::SmallInt(7))],
            }))
        };
        let program = Program {
            body: vec![Statement::Let {
                name: b"outer",
                value: Expr::Block(Box::new(Block {
                    params: Vec::new(),
                    body: vec![Statement::Expr(nested()), Statement::Expr(nested())],
                })),
            }],
        };
        let selected = BlockResumableEmission {
            key: ImplementationKey {
                root_kind: InventoryRootKind::NonMethodProgram,
                root_index: 0,
                path: vec![0, 0],
                implementation_kind: InventoryImplementationKind::BlockBody,
            },
            symbol_index: 7,
            manifest_entry_index: 3,
            selector: b"value".to_vec(),
            captures: Vec::new(),
            c_source: Vec::new(),
        };
        let mut emitter =
            Emitter::new_selected_resumable_candidate_artifact(11, Vec::new(), vec![selected])
                .unwrap();
        emitter.emit_program(&program).unwrap();
        let output = String::from_utf8(emitter.finish()).unwrap();
        assert!(output.contains("__birnam_resolve_block_vtable_v1(3U)"));
        assert!(output.contains("(void*)&__blk2"));
        assert!(output.contains("void* __n0=MKV(7L);"));
    }

    #[test]
    fn nested_reflection_does_not_consume_a_top_level_operation_index() {
        let define = |selector: &'static [u8], source: &'static [u8]| Expr::Send {
            receiver: Box::new(Expr::Global(b"Reflect")),
            selector: Cow::Borrowed(b"defineMethod:on:as:"),
            args: vec![
                Expr::Symbol(selector),
                Expr::Global(b"Object"),
                Expr::Block(Box::new(Block {
                    params: vec![b"self"],
                    body: vec![Statement::Expr(Expr::Nil)],
                })),
            ],
            source,
        };
        let program = Program {
            body: vec![
                Statement::Let {
                    name: b"dynamicInstaller",
                    value: Expr::Block(Box::new(Block {
                        params: Vec::new(),
                        body: vec![Statement::Expr(define(b"nested", b"nested source"))],
                    })),
                },
                Statement::Expr(define(b"topLevel", b"top-level source")),
            ],
        };
        let mut emitter = Emitter::new_resumable_candidate_artifact(
            11,
            vec![NamedResumableEmission {
                operation_index: 0,
                symbol_index: 7,
                c_source: Vec::new(),
            }],
        )
        .unwrap();
        emitter.emit_program(&program).unwrap();
        let output = String::from_utf8(emitter.finish()).unwrap();
        assert!(output.contains("birnam_intern(\"defineMethod:on:as:source:\")"));
        assert!(output.contains("birnam_intern(\"topLevel\"), (void*)&__birnam_resumable_thunk_7"));
        assert!(!output.contains("birnam_intern(\"nested\"), (void*)&__birnam_resumable_thunk_7"));
    }

    #[test]
    fn binary_operator_method_trampolines_forward_the_operand() {
        assert_eq!(selector_arity(b"="), 1);
        assert_eq!(selector_arity(b"value"), 0);
        assert_eq!(selector_arity(b"at:"), 1);

        let expression = Expr::Send {
            receiver: Box::new(Expr::Global(b"Reflect")),
            selector: Cow::Borrowed(b"defineMethod:on:as:"),
            args: vec![
                Expr::Symbol(b"="),
                Expr::Global(b"Object"),
                Expr::Block(Box::new(Block {
                    params: vec![b"self", b"other"],
                    body: vec![Statement::Expr(Expr::Send {
                        receiver: Box::new(Expr::Global(b"self")),
                        selector: Cow::Borrowed(b"=="),
                        args: vec![Expr::Global(b"other")],
                        source: b"",
                    })],
                })),
            ],
            source: b"[Reflect defineMethod: #= on: Object as: [:self :other | [self == other]]]",
        };
        let mut emitter = Emitter::new_candidate_artifact(7);
        emitter.emit_expr(&expression).unwrap();
        let output = String::from_utf8(emitter.finish()).unwrap();
        assert!(output.contains(
            "return birnam_send(birnam_candidate_root_get(__birnam_code_generation_identity,0L), birnam_intern(\"value:value:\"), a, 2);"
        ));
        assert!(output.contains("for(i=0;i<1;i++) a[i+1]=args[i];"));
    }

    #[test]
    fn emits_float_singletons_and_source_string_escapes_like_reference() {
        let expression = Expr::Send {
            receiver: Box::new(Expr::Float(b"-0.25")),
            selector: Cow::Borrowed(b"ifTrue:ifFalse:"),
            args: vec![Expr::True, Expr::StringLiteral(br#""a\n\c""#)],
            source: b"",
        };
        let mut emitter = Emitter::new();
        emitter.emit_expr(&expression).unwrap();
        assert_eq!(
            String::from_utf8(emitter.finish()).unwrap(),
            "({void* __birnam_send_0_receiver=birnam_mkFloat(-0.25);void* __birnam_send_0_arg_0=birnam_true();void* __birnam_send_0_arg_1=birnam_mkString(\"a\\n\\\\c\", 4L);void* __birnam_send_0_args[]={__birnam_send_0_arg_0,__birnam_send_0_arg_1};birnam_send(__birnam_send_0_receiver,birnam_intern(\"ifTrue:ifFalse:\"),__birnam_send_0_args,2L);})"
        );

        let mut emitter = Emitter::new();
        emitter.emit_expr(&Expr::Nil).unwrap();
        emitter.push(b"; ");
        emitter.emit_expr(&Expr::False).unwrap();
        assert_eq!(
            String::from_utf8(emitter.finish()).unwrap(),
            "birnam_nil(); birnam_false()"
        );
    }

    #[test]
    fn lowers_cst_literals_without_runtime_or_allocator_access() {
        let integer = node(b"number", 0, 3, vec![]);
        assert_eq!(
            lower_literal(&integer, b"-42"),
            Ok(Expr::SmallIntLiteral(b"-42"))
        );
        let owned = OwnedCst::new(b"number", 0, 2, Vec::new());
        assert_eq!(
            lower_literal(&owned, b"17"),
            Ok(Expr::SmallIntLiteral(b"17"))
        );
        let embedded = OwnedCst::embedded_string(0, 3, b"a\0b\xff".to_vec());
        assert_eq!(
            lower_literal(&embedded, b"foo"),
            Ok(Expr::StringOwned(b"a\0b\xff".to_vec()))
        );
        let mut emitter = Emitter::new();
        emitter
            .emit_expr(&Expr::StringOwned(b"a\0b\xff".to_vec()))
            .unwrap();
        assert_eq!(emitter.finish(), br#"birnam_mkString("a\000b\377", 4L)"#);

        let wrapped = node(
            b"expr",
            0,
            3,
            vec![node(b"primary", 0, 3, vec![node(b"float", 0, 3, vec![])])],
        );
        assert_eq!(lower_literal(&wrapped, b"3.5"), Ok(Expr::Float(b"3.5")));

        let string = node(b"string", 0, 7, vec![]);
        assert_eq!(
            lower_literal(&string, br#""a\n\c""#),
            Ok(Expr::StringLiteral(br#""a\n\c""#))
        );

        let symbol = node(b"symbol", 0, 4, vec![]);
        assert_eq!(lower_literal(&symbol, b"#at:"), Ok(Expr::Symbol(b"at:")));

        for (name, expected) in [
            (b"nil".as_slice(), Expr::Nil),
            (b"true".as_slice(), Expr::True),
            (b"false".as_slice(), Expr::False),
        ] {
            let literal = node(b"ident", 0, name.len(), vec![]);
            assert_eq!(lower_literal(&literal, name), Ok(expected));
        }

        let global = node(b"ident", 0, 7, vec![]);
        assert_eq!(
            lower_literal(&global, b"Integer"),
            Ok(Expr::Global(b"Integer"))
        );

        let invalid = node(b"number", 1, 4, vec![]);
        assert_eq!(lower_literal(&invalid, b"42"), Err(LowerError::InvalidSpan));

        let unsupported = node(b"send", 0, 3, vec![]);
        assert_eq!(
            lower_literal(&unsupported, b"foo"),
            Err(LowerError::UnsupportedRule)
        );

        let mut emitter = Emitter::new();
        emitter.emit_expr(&Expr::SmallIntLiteral(b"-42")).unwrap();
        assert_eq!(emitter.finish(), b"MKV(-42L)");
    }

    #[test]
    fn lowers_cst_sends_and_keyword_selector_parts_deterministically() {
        let binary_source = b"[3 + 4]";
        let binary = node(
            b"send",
            0,
            binary_source.len(),
            vec![
                node(
                    b"expr",
                    1,
                    2,
                    vec![node(b"primary", 1, 2, vec![node(b"number", 1, 2, vec![])])],
                ),
                node(
                    b"message",
                    3,
                    6,
                    vec![node(
                        b"binaryMsg",
                        3,
                        6,
                        vec![
                            node(b"binops", 3, 4, vec![]),
                            node(
                                b"expr",
                                5,
                                6,
                                vec![node(b"primary", 5, 6, vec![node(b"number", 5, 6, vec![])])],
                            ),
                        ],
                    )],
                ),
            ],
        );
        let mut emitter = Emitter::new();
        emitter
            .emit_expr(&lower_expr(&binary, binary_source).unwrap())
            .unwrap();
        assert_eq!(
            emitter.finish(),
            b"({void* __birnam_send_0_receiver=MKV(3L);void* __birnam_send_0_arg_0=MKV(4L);void* __birnam_send_0_args[]={__birnam_send_0_arg_0};birnam_send(__birnam_send_0_receiver,birnam_intern(\"+\"),__birnam_send_0_args,1L);})"
        );

        let keyword_source = b"[x foo: 1 bar: 2]";
        let keyword = node(
            b"send",
            0,
            keyword_source.len(),
            vec![
                node(
                    b"expr",
                    1,
                    2,
                    vec![node(b"primary", 1, 2, vec![node(b"ident", 1, 2, vec![])])],
                ),
                node(
                    b"message",
                    3,
                    16,
                    vec![node(
                        b"keywordMsg",
                        3,
                        16,
                        vec![
                            node(b"keyword", 3, 7, vec![]),
                            node(
                                b"expr",
                                8,
                                9,
                                vec![node(b"primary", 8, 9, vec![node(b"number", 8, 9, vec![])])],
                            ),
                            node(b"keyword", 10, 14, vec![]),
                            node(
                                b"expr",
                                15,
                                16,
                                vec![node(
                                    b"primary",
                                    15,
                                    16,
                                    vec![node(b"number", 15, 16, vec![])],
                                )],
                            ),
                        ],
                    )],
                ),
            ],
        );
        let mut emitter = Emitter::new();
        emitter
            .emit_expr(&lower_expr(&keyword, keyword_source).unwrap())
            .unwrap();
        assert_eq!(
            emitter.finish(),
            b"({void* __birnam_send_0_receiver=birnam_global_b((void*)__g0);void* __birnam_send_0_arg_0=MKV(1L);void* __birnam_send_0_arg_1=MKV(2L);void* __birnam_send_0_args[]={__birnam_send_0_arg_0,__birnam_send_0_arg_1};birnam_send(__birnam_send_0_receiver,birnam_intern(\"foo:bar:\"),__birnam_send_0_args,2L);})"
        );
    }

    #[test]
    fn rewrites_live_reflect_method_definitions_to_source_carrying_selectors() {
        let source = b"[Reflect defineMethod: #value: on: Object as: [:self :x | x]]";
        let mut emitter = Emitter::new();
        emitter
            .emit_expr(&Expr::Send {
                receiver: Box::new(Expr::Global(b"Reflect")),
                selector: Cow::Borrowed(b"defineMethod:on:as:"),
                args: vec![Expr::Symbol(b"value:"), Expr::Global(b"Object"), Expr::Nil],
                source,
            })
            .unwrap();
        let output = emitter.finish();
        assert!(
            output
                .windows(b"defineMethod:on:as:source:".len())
                .any(|window| { window == b"defineMethod:on:as:source:" })
        );
        assert!(
            output
                .windows(b"birnam_mkString(\"[Reflect defineMethod:".len())
                .any(|window| { window == b"birnam_mkString(\"[Reflect defineMethod:" })
        );
        assert!(output.ends_with(b"__birnam_send_0_args,4L);})"));
    }

    #[test]
    fn folds_same_category_cst_message_chains_left_to_right() {
        let source = b"[1 + 2 + 3]";
        let number = |start, end| {
            node(
                b"expr",
                start,
                end,
                vec![node(
                    b"primary",
                    start,
                    end,
                    vec![node(b"number", start, end, vec![])],
                )],
            )
        };
        let binary_message = |start, end, arg_start, arg_end| {
            node(
                b"binaryMsg",
                start,
                end,
                vec![
                    node(b"binops", start, start + 1, vec![]),
                    number(arg_start, arg_end),
                ],
            )
        };
        let send = node(
            b"send",
            0,
            source.len(),
            vec![
                number(1, 2),
                node(
                    b"message",
                    3,
                    10,
                    vec![binary_message(3, 6, 5, 6), binary_message(7, 10, 9, 10)],
                ),
            ],
        );
        let mut emitter = Emitter::new();
        emitter
            .emit_expr(&lower_expr(&send, source).unwrap())
            .unwrap();
        let output = String::from_utf8(emitter.finish()).unwrap();
        let receiver = output.find("__birnam_send_1_receiver=MKV(1L)").unwrap();
        let first = output.find("__birnam_send_1_arg_0=MKV(2L)").unwrap();
        let first_lookup = output.find("birnam_send(__birnam_send_1_receiver").unwrap();
        let second = output.find("__birnam_send_0_arg_0=MKV(3L)").unwrap();
        let second_lookup = output.find("birnam_send(__birnam_send_0_receiver").unwrap();
        assert!(receiver < first && first < first_lookup && first_lookup < second);
        assert!(second < second_lookup, "outer lookup follows its argument");
    }

    #[test]
    fn folds_threaded_dispatches_left_to_right_without_a_new_expr_shape() {
        let source = b"[x => foo => + 1 => bar: 2]";
        let primary = |rule: &'static [u8], start, end| {
            node(
                b"expr",
                start,
                end,
                vec![node(
                    b"primary",
                    start,
                    end,
                    vec![node(rule, start, end, vec![])],
                )],
            )
        };
        let dispatch = |start, end, message| node(b"threadDispatch", start, end, vec![message]);
        let send = node(
            b"send",
            0,
            source.len(),
            vec![
                primary(b"ident", 1, 2),
                node(
                    b"message",
                    3,
                    26,
                    vec![node(
                        b"threadedMsg",
                        3,
                        26,
                        vec![
                            dispatch(6, 9, node(b"unaryMsg", 6, 9, vec![])),
                            dispatch(
                                13,
                                16,
                                node(
                                    b"binaryMsg",
                                    13,
                                    16,
                                    vec![
                                        node(b"binops", 13, 14, vec![]),
                                        primary(b"number", 15, 16),
                                    ],
                                ),
                            ),
                            dispatch(
                                20,
                                26,
                                node(
                                    b"keywordMsg",
                                    20,
                                    26,
                                    vec![
                                        node(b"keyword", 20, 24, vec![]),
                                        primary(b"number", 25, 26),
                                    ],
                                ),
                            ),
                        ],
                    )],
                ),
            ],
        );
        let mut emitter = Emitter::new();
        emitter
            .emit_expr(&lower_expr(&send, source).unwrap())
            .unwrap();
        let output = String::from_utf8(emitter.finish()).unwrap();
        let receiver = output
            .find("__birnam_send_2_receiver=birnam_global_b")
            .unwrap();
        let first_lookup = output.find("birnam_send(__birnam_send_2_receiver").unwrap();
        let first_arg = output.find("__birnam_send_1_arg_0=MKV(1L)").unwrap();
        let second_lookup = output.find("birnam_send(__birnam_send_1_receiver").unwrap();
        let second_arg = output.find("__birnam_send_0_arg_0=MKV(2L)").unwrap();
        let final_lookup = output.find("birnam_send(__birnam_send_0_receiver").unwrap();
        assert!(receiver < first_lookup && first_lookup < first_arg);
        assert!(first_arg < second_lookup && second_lookup < second_arg);
        assert!(second_arg < final_lookup);
    }

    #[test]
    fn lowers_collections_and_logic_variables_to_the_reference_send_shapes() {
        let vector_source = b"#[1 2]";
        let vector = node(
            b"vectorLit",
            0,
            vector_source.len(),
            vec![
                node(b"expr", 2, 3, vec![node(b"number", 2, 3, vec![])]),
                node(b"expr", 4, 5, vec![node(b"number", 4, 5, vec![])]),
            ],
        );
        let mut emitter = Emitter::new();
        emitter
            .emit_expr(&lower_expr(&vector, vector_source).unwrap())
            .unwrap();
        let output = String::from_utf8(emitter.finish()).unwrap();
        let new_lookup = output.find("birnam_send(__birnam_send_2_receiver").unwrap();
        let first = output.find("__birnam_send_1_arg_0=MKV(1L)").unwrap();
        let second = output.find("__birnam_send_0_arg_0=MKV(2L)").unwrap();
        assert!(new_lookup < first && first < second);

        let map_source = b"{#a -> 1}";
        let map = node(
            b"mapLit",
            0,
            map_source.len(),
            vec![node(
                b"mapEntry",
                1,
                8,
                vec![
                    node(b"expr", 1, 3, vec![node(b"symbol", 1, 3, vec![])]),
                    node(b"expr", 7, 8, vec![node(b"number", 7, 8, vec![])]),
                ],
            )],
        );
        let mut emitter = Emitter::new();
        emitter
            .emit_expr(&lower_expr(&map, map_source).unwrap())
            .unwrap();
        let output = String::from_utf8(emitter.finish()).unwrap();
        let new_lookup = output.find("birnam_send(__birnam_send_1_receiver").unwrap();
        let key = output
            .find("__birnam_send_0_arg_0=birnam_intern(\"a\")")
            .unwrap();
        let value = output.find("__birnam_send_0_arg_1=MKV(1L)").unwrap();
        let put_lookup = output.find("birnam_send(__birnam_send_0_receiver").unwrap();
        assert!(new_lookup < key && key < value && value < put_lookup);

        let logic_source = b"?p";
        let logic = node(
            b"logicVar",
            0,
            logic_source.len(),
            vec![node(b"ident", 1, 2, vec![])],
        );
        let mut emitter = Emitter::new();
        emitter
            .emit_expr(&lower_expr(&logic, logic_source).unwrap())
            .unwrap();
        assert_eq!(
            emitter.finish(),
            b"({void* __birnam_send_0_receiver=birnam_global_b((void*)__g0);void* __birnam_send_0_arg_0=birnam_intern(\"p\");void* __birnam_send_0_args[]={__birnam_send_0_arg_0};birnam_send(__birnam_send_0_receiver,birnam_intern(\"named:\"),__birnam_send_0_args,1L);})"
        );
    }

    #[test]
    fn lowers_block_parameters_and_statement_bindings_before_emission() {
        let source = b"[:x | let y := x; y]";
        let block = node(
            b"block",
            0,
            source.len(),
            vec![
                node(b"ident", 2, 3, vec![]),
                node(
                    b"statements",
                    5,
                    19,
                    vec![
                        node(
                            b"statement",
                            6,
                            16,
                            vec![node(
                                b"letBinding",
                                6,
                                16,
                                vec![
                                    node(b"ident", 10, 11, vec![]),
                                    node(b"expr", 15, 16, vec![node(b"ident", 15, 16, vec![])]),
                                ],
                            )],
                        ),
                        node(
                            b"statement",
                            18,
                            19,
                            vec![node(b"expr", 18, 19, vec![node(b"ident", 18, 19, vec![])])],
                        ),
                    ],
                ),
            ],
        );
        let lowered = lower_expr(&block, source).unwrap();
        let Expr::Block(block) = &lowered else {
            panic!("expected block expression");
        };
        assert_eq!(block.params, vec![b"x".as_slice()]);
        assert_eq!(
            block.body,
            vec![
                Statement::Let {
                    name: b"y",
                    value: Expr::Global(b"x"),
                },
                Statement::Expr(Expr::Global(b"y")),
            ]
        );

        let mut emitter = Emitter::new();
        emitter.emit_expr(&lowered).unwrap();
        let output = String::from_utf8(emitter.finish()).unwrap();
        assert!(output.starts_with(
            "void* __blk0(void* self, void** args, long n){ (void)self;(void)args;(void)n;\nvoid* __b0=args[0];\nvoid* __n0=__b0;\nvoid* __b1=__n0;\nvoid* __n1=__b1;\nreturn __n1;\n}"
        ));
        assert!(output.contains("birnam_portable_code_admit_v1(__pp0,sizeof(__pp0))"));
        assert!(output.contains("birnam_portable_block_new_v1(__vt0,__co0,(void**)0,0UL)"));
        assert!(!output.contains("birnam_alloc(__vt0"));

        let nested = Expr::Block(Box::new(Block {
            params: vec![b"x"],
            body: vec![Statement::Expr(Expr::Block(Box::new(Block {
                params: Vec::new(),
                body: vec![Statement::Expr(Expr::Global(b"x"))],
            })))],
        }));
        let mut emitter = Emitter::new();
        emitter.emit_expr(&nested).unwrap();
        let output = String::from_utf8(emitter.finish()).unwrap();
        assert!(output.contains("void* __blk1(void* self, void** args, long n)"));
        assert!(output.contains("void* __b0=((void**)self)[1];"));
        assert!(output.contains("__captures1[0]=__b0;"));
        assert!(output.contains("birnam_portable_block_new_v1(__vt1,__co1,__captures1,1UL)"));
    }

    #[test]
    fn lowers_and_emits_program_bodies_with_transactional_local_scope() {
        let source = b"let x := 1; x";
        let program = node(
            b"program",
            0,
            source.len(),
            vec![node(
                b"statements",
                0,
                source.len(),
                vec![
                    node(
                        b"statement",
                        0,
                        10,
                        vec![node(
                            b"letBinding",
                            0,
                            10,
                            vec![
                                node(b"ident", 4, 5, vec![]),
                                node(b"expr", 9, 10, vec![node(b"number", 9, 10, vec![])]),
                            ],
                        )],
                    ),
                    node(
                        b"statement",
                        12,
                        13,
                        vec![node(b"expr", 12, 13, vec![node(b"ident", 12, 13, vec![])])],
                    ),
                ],
            )],
        );
        let lowered = lower_program(&program, source).unwrap();
        assert_eq!(
            lowered,
            Program {
                body: vec![
                    Statement::Let {
                        name: b"x",
                        value: Expr::SmallIntLiteral(b"1"),
                    },
                    Statement::Expr(Expr::Global(b"x")),
                ],
            }
        );
        let mut emitter = Emitter::new();
        emitter.emit_program(&lowered).unwrap();
        assert_eq!(emitter.finish(), b"void* b_x = MKV(1L);\nreturn b_x;\n");

        let mut emitter = Emitter::new();
        emitter.emit_program_persisting(&lowered).unwrap();
        assert_eq!(
            emitter.finish(),
            b"void* b_x = MKV(1L);\nvoid* __birnam_global_set_result_0 = birnam_global_set(\"x\", b_x);\nif(__birnam_global_set_result_0 != b_x) return __birnam_global_set_result_0;\nreturn b_x;\n"
        );
    }

    #[test]
    fn assembles_jit_and_artifact_units_deterministically() {
        let mut jit = TranslationUnit::new(false);
        jit.add_baked_global(b"Integer");
        jit.push_body(b"return MKV(42L);\n");
        let output = String::from_utf8(jit.finish()).unwrap();
        assert!(output.contains("extern void* birnam_global(const char* name);\n"));
        assert!(output.ends_with("extern char __g0[];\nvoid* __ev(void* self, void** args, long n){ (void)self;(void)args;(void)n;\nreturn MKV(42L);\n}\n"));

        let mut artifact = TranslationUnit::new(true);
        artifact.add_baked_global(b"Integer");
        artifact.push_body(b"return __g0;\n");
        let output = String::from_utf8(artifact.finish()).unwrap();
        assert!(output.contains("static void* __g0;"));
        assert!(output.contains("void __birnam_unit_init(void){"));
        assert!(output.contains("__g0 = birnam_intern(\"Integer\");"));
    }
}
