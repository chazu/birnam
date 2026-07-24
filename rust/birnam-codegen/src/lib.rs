//! Pure deterministic C-emission primitives for the N6.2 Codegen port.
//!
//! This crate deliberately has no IdApi, loader, allocator, or runtime
//! dependency. It owns byte-buffer construction and a pure CST-to-expression
//! adapter; runtime orchestration arrives in later N6.2/N6.3 slices.

use std::borrow::Cow;

/// A bounded-independent byte emitter. Callers retain ownership of the returned
/// translation-unit bytes and may compare them byte-for-byte with the Zig oracle.
#[derive(Debug, Default)]
pub struct Emitter {
    bytes: Vec<u8>,
    function_defs: Vec<u8>,
    bound_names: Vec<Vec<u8>>,
    next_block: usize,
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
    /// unknown escape keeps its backslash, matching the Zig emitter.
    StringLiteral(&'a [u8]),
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
}

/// An owned CST projection used at the runtime boundary. The core emitter does
/// not know about IdApi or Oop objects; the Rust PEG adapter materializes its
/// transient nodes into this byte-owned tree before lowering.
#[derive(Debug, PartialEq, Eq)]
pub struct OwnedCst {
    rule: Vec<u8>,
    start: usize,
    end: usize,
    children: Vec<OwnedCst>,
}

impl OwnedCst {
    pub fn new(rule: &[u8], start: usize, end: usize, children: Vec<OwnedCst>) -> Self {
        Self {
            rule: rule.to_vec(),
            start,
            end,
            children,
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
}

#[derive(Debug, PartialEq, Eq)]
pub enum LowerError {
    DepthLimit,
    InvalidSpan,
    MissingChild,
    InvalidSymbol,
    InvalidShape,
    UnsupportedRule,
}

/// Lower the literal subset of a CST node into the pure emitter IR.
///
/// `expr`, `primary`, and `message` wrappers are transparent when they have a
/// single child, matching the Zig emitter's `skip` operation. Sends, blocks,
/// collections, and logic variables deliberately remain unsupported until the
/// corresponding CST-shape slices are frozen.
pub fn lower_literal<'a, N: CstNode>(node: &N, source: &'a [u8]) -> Result<Expr<'a>, LowerError> {
    let node = skip_wrappers(node)?;
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
        if matches!(rule, b"expr" | b"primary" | b"message") && current.child_count() == 1 {
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
        b"number" | b"float" | b"string" | b"symbol" | b"ident" => lower_literal(node, source),
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
    let message = skip_wrappers(node.child(1).ok_or(LowerError::MissingChild)?)?;
    if message.rule() == b"message" && message.child_count() > 1 {
        let mut accumulated = receiver;
        for index in 0..message.child_count() {
            let part = message.child(index).ok_or(LowerError::MissingChild)?;
            accumulated = lower_message(accumulated, part, source, depth)?;
        }
        return Ok(accumulated);
    }
    lower_message(receiver, message, source, depth)
}

fn lower_message<'a, N: CstNode>(
    receiver: Expr<'a>,
    message: &N,
    source: &'a [u8],
    depth: usize,
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

const C_RUNTIME_PRELUDE: &[u8] = br#"extern void* birnam_mkSmallInt(long v);
#define MKV(x) (birnam_mkSmallInt((long)(x)))
extern void* birnam_send(void* rcv, void* sel, void** args, long n);
extern void* birnam_intern(const char* name);
extern void* birnam_mkString(const char* p, long n);
extern void* birnam_mkFloat(double v);
extern void* birnam_nil(void);
extern void* birnam_true(void);
extern void* birnam_false(void);
extern void* birnam_global(const char* name);
extern void* birnam_global_set(const char* name, void* value);
extern void* birnam_proto(void* parent);
extern void* birnam_blockVt(void);
extern void* birnam_method(void* vt, void* selector, void* method);
extern void* birnam_layoutAllOops(void* vt);
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
        for &byte in bytes {
            match byte {
                b'"' => self.push(br#"\""#),
                b'\\' => self.push(br#"\\"#),
                b'\n' => self.push(br#"\n"#),
                b'\r' => self.push(br#"\r"#),
                b'\t' => self.push(br#"\t"#),
                0x20..=0x7e => self.bytes.push(byte),
                other => {
                    // Three octal digits avoid greedy `\\x` parsing when the
                    // following source byte is itself a hexadecimal digit.
                    self.bytes.extend_from_slice(&[
                        b'\\',
                        b'0' + ((other >> 6) & 7),
                        b'0' + ((other >> 3) & 7),
                        b'0' + (other & 7),
                    ]);
                }
            }
        }
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
            self.emit_global(name);
        }
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
        self.emit_expr_at_depth(expr, 0)
    }

    /// Emit a program body transactionally. The caller owns the surrounding
    /// translation-unit wrapper; this method only appends the ordered body and
    /// any nested block definitions. Runtime error propagation and persistent
    /// top-level global writes remain N6.3 orchestration concerns.
    pub fn emit_program(&mut self, program: &Program<'_>) -> Result<(), EmitError> {
        self.emit_program_mode(program, false)
    }

    /// Emit a top-level program whose `let` bindings update the shared global
    /// environment after their local value is computed. Nested block locals
    /// remain lexical and never persist.
    pub fn emit_program_persisting(&mut self, program: &Program<'_>) -> Result<(), EmitError> {
        self.emit_program_mode(program, true)
    }

    fn emit_program_mode(
        &mut self,
        program: &Program<'_>,
        persist_top_level: bool,
    ) -> Result<(), EmitError> {
        let saved_bytes = std::mem::take(&mut self.bytes);
        let saved_bound_names = std::mem::take(&mut self.bound_names);
        let saved_function_defs_len = self.function_defs.len();
        self.bytes = Vec::new();
        self.bound_names = Vec::new();
        let body_result = self.emit_block_body(&program.body, persist_top_level);
        let body = std::mem::take(&mut self.bytes);
        self.bytes = saved_bytes;
        self.bound_names = saved_bound_names;
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
        match expr {
            Expr::SmallInt(value) => self.emit_small_int(*value),
            Expr::SmallIntLiteral(literal) => self.emit_small_int_literal(literal),
            Expr::Float(literal) => self.emit_float(literal),
            Expr::String(bytes) => self.emit_string(bytes),
            Expr::StringLiteral(source) => self.emit_string_literal(source),
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
            } => {
                self.bytes.extend_from_slice(b"birnam_send(");
                self.emit_expr_at_depth(receiver, depth + 1)?;
                self.bytes.extend_from_slice(b", ");
                self.emit_symbol(selector.as_ref());
                if args.is_empty() {
                    self.bytes.extend_from_slice(b", 0, 0L)");
                    return Ok(());
                }
                self.bytes.extend_from_slice(b", (void*[]){");
                for (index, arg) in args.iter().enumerate() {
                    if index != 0 {
                        self.bytes.extend_from_slice(b", ");
                    }
                    self.emit_expr_at_depth(arg, depth + 1)?;
                }
                self.bytes.extend_from_slice(b"}, ");
                self.bytes
                    .extend_from_slice(args.len().to_string().as_bytes());
                self.bytes.extend_from_slice(b"L)");
            }
        }
        Ok(())
    }

    fn emit_block(&mut self, block: &Block<'_>) -> Result<(), EmitError> {
        let mut references = Vec::new();
        for statement in &block.body {
            collect_statement_refs(statement, &mut references);
        }
        let captures = self
            .bound_names
            .iter()
            .filter(|name| {
                references.iter().any(|reference| reference == *name)
                    && !block.params.contains(&name.as_slice())
                    && !block_has_let(block, name)
            })
            .cloned()
            .collect::<Vec<_>>();

        let number = self.next_block;
        self.next_block += 1;
        let saved_function_defs_len = self.function_defs.len();
        let saved_bytes = std::mem::take(&mut self.bytes);
        let saved_bound_names = std::mem::take(&mut self.bound_names);
        self.bound_names = block
            .params
            .iter()
            .map(|name| name.to_vec())
            .chain(captures.iter().cloned())
            .collect();
        self.bytes = Vec::new();
        for (index, name) in block.params.iter().enumerate() {
            self.bytes.extend_from_slice(b"void* b_");
            self.bytes.extend_from_slice(name);
            self.bytes.extend_from_slice(b" = args[");
            self.bytes.extend_from_slice(index.to_string().as_bytes());
            self.bytes.extend_from_slice(b"];\n");
        }
        for (index, name) in captures.iter().enumerate() {
            self.bytes.extend_from_slice(b"void* b_");
            self.bytes.extend_from_slice(name);
            self.bytes.extend_from_slice(b" = ((void**)self)[");
            self.bytes.extend_from_slice(index.to_string().as_bytes());
            self.bytes.extend_from_slice(b"];\n");
        }
        let body_result = self.emit_block_body(&block.body, false);
        let body = std::mem::take(&mut self.bytes);
        self.bytes = saved_bytes;
        self.bound_names = saved_bound_names;
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

        self.bytes.extend_from_slice(b"({ static void* __vt");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes.extend_from_slice(b" = 0; if(!__vt");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes.extend_from_slice(b"){ __vt");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes
            .extend_from_slice(b" = birnam_proto(birnam_blockVt()); birnam_method(__vt");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes.extend_from_slice(b", birnam_intern(\"");
        if block.params.is_empty() {
            self.bytes.extend_from_slice(b"value");
        } else {
            for _ in &block.params {
                self.bytes.extend_from_slice(b"value:");
            }
        }
        self.bytes.extend_from_slice(b"\"), (void*)&__blk");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes
            .extend_from_slice(b"); birnam_layoutAllOops(__vt");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes.extend_from_slice(b"); } void* __o");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes.extend_from_slice(b" = birnam_alloc(__vt");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes.extend_from_slice(b", ");
        self.bytes
            .extend_from_slice(captures.len().to_string().as_bytes());
        self.bytes.extend_from_slice(b"*sizeof(void*));\n");
        for (index, name) in captures.iter().enumerate() {
            self.bytes.extend_from_slice(b"((void**)__o");
            self.bytes.extend_from_slice(number.to_string().as_bytes());
            self.bytes.extend_from_slice(b")[");
            self.bytes.extend_from_slice(index.to_string().as_bytes());
            self.bytes.extend_from_slice(b"] = b_");
            self.bytes.extend_from_slice(name);
            self.bytes.extend_from_slice(b";\n");
        }
        self.bytes.extend_from_slice(b"__o");
        self.bytes.extend_from_slice(number.to_string().as_bytes());
        self.bytes.extend_from_slice(b"; })");
        Ok(())
    }

    fn emit_block_body(
        &mut self,
        body: &[Statement<'_>],
        persist_top_level: bool,
    ) -> Result<(), EmitError> {
        if body.is_empty() {
            self.bytes.extend_from_slice(b"return birnam_nil();\n");
            return Ok(());
        }
        for (index, statement) in body.iter().enumerate() {
            let last = index + 1 == body.len();
            match statement {
                Statement::Let { name, value } => {
                    self.bytes.extend_from_slice(b"void* b_");
                    self.bytes.extend_from_slice(name);
                    self.bytes.extend_from_slice(b" = ");
                    self.emit_expr_at_depth(value, 1)?;
                    self.bytes.extend_from_slice(b";\n");
                    if persist_top_level {
                        self.bytes.extend_from_slice(b"birnam_global_set(\"");
                        self.push_c_string_body(name);
                        self.bytes.extend_from_slice(b"\", b_");
                        self.bytes.extend_from_slice(name);
                        self.bytes.extend_from_slice(b");\n");
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
                        self.emit_expr_at_depth(expression, 1)?;
                        self.bytes.extend_from_slice(b";\n");
                    } else {
                        self.bytes.extend_from_slice(b"(void)(");
                        self.emit_expr_at_depth(expression, 1)?;
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
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::{
        Block, CstNode, EmitError, Emitter, Expr, LowerError, OwnedCst, Program, Statement,
        TranslationUnit, lower_expr, lower_literal, lower_program,
    };

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
    fn escapes_like_zig_codegen() {
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
        };
        let mut emitter = Emitter::new();
        emitter.emit_expr(&expression).unwrap();
        assert_eq!(
            String::from_utf8(emitter.finish()).unwrap(),
            "birnam_send(birnam_global(\"Integer\"), birnam_intern(\"at:\"), (void*[]){birnam_mkString(\"answer\", 6L)}, 1L)"
        );

        let mut deep = Expr::SmallInt(0);
        for _ in 0..4096 {
            deep = Expr::Send {
                receiver: Box::new(deep),
                selector: Cow::Borrowed(b"value"),
                args: Vec::new(),
            };
        }
        let mut emitter = Emitter::new();
        assert_eq!(emitter.emit_expr(&deep), Err(EmitError::DepthLimit));
    }

    #[test]
    fn emits_float_singletons_and_source_string_escapes_like_zig() {
        let expression = Expr::Send {
            receiver: Box::new(Expr::Float(b"-0.25")),
            selector: Cow::Borrowed(b"ifTrue:ifFalse:"),
            args: vec![Expr::True, Expr::StringLiteral(br#""a\n\c""#)],
        };
        let mut emitter = Emitter::new();
        emitter.emit_expr(&expression).unwrap();
        assert_eq!(
            String::from_utf8(emitter.finish()).unwrap(),
            "birnam_send(birnam_mkFloat(-0.25), birnam_intern(\"ifTrue:ifFalse:\"), (void*[]){birnam_true(), birnam_mkString(\"a\\n\\\\c\", 4L)}, 2L)"
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
            b"birnam_send(MKV(3L), birnam_intern(\"+\"), (void*[]){MKV(4L)}, 1L)"
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
            b"birnam_send(birnam_global(\"x\"), birnam_intern(\"foo:bar:\"), (void*[]){MKV(1L), MKV(2L)}, 2L)"
        );
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
        assert_eq!(
            emitter.finish(),
            b"birnam_send(birnam_send(MKV(1L), birnam_intern(\"+\"), (void*[]){MKV(2L)}, 1L), birnam_intern(\"+\"), (void*[]){MKV(3L)}, 1L)"
        );
    }

    #[test]
    fn lowers_collections_and_logic_variables_to_the_zig_send_shapes() {
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
        assert_eq!(
            emitter.finish(),
            b"birnam_send(birnam_send(birnam_send(birnam_global(\"Vector\"), birnam_intern(\"new\"), 0, 0L), birnam_intern(\"add:\"), (void*[]){MKV(1L)}, 1L), birnam_intern(\"add:\"), (void*[]){MKV(2L)}, 1L)"
        );

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
        assert_eq!(
            emitter.finish(),
            b"birnam_send(birnam_send(birnam_global(\"Map\"), birnam_intern(\"new\"), 0, 0L), birnam_intern(\"at:put:\"), (void*[]){birnam_intern(\"a\"), MKV(1L)}, 2L)"
        );

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
            b"birnam_send(birnam_global(\"LogicVar\"), birnam_intern(\"named:\"), (void*[]){birnam_intern(\"p\")}, 1L)"
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
        assert_eq!(
            String::from_utf8(emitter.finish()).unwrap(),
            "void* __blk0(void* self, void** args, long n){ (void)self;(void)args;(void)n;\nvoid* b_x = args[0];\nvoid* b_y = b_x;\nreturn b_y;\n}\n({ static void* __vt0 = 0; if(!__vt0){ __vt0 = birnam_proto(birnam_blockVt()); birnam_method(__vt0, birnam_intern(\"value:\"), (void*)&__blk0); birnam_layoutAllOops(__vt0); } void* __o0 = birnam_alloc(__vt0, 0*sizeof(void*));\n__o0; })"
        );

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
        assert!(output.contains("void* b_x = ((void**)self)[0];"));
        assert!(output.contains("((void**)__o1)[0] = b_x;"));
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
            b"void* b_x = MKV(1L);\nbirnam_global_set(\"x\", b_x);\nreturn b_x;\n"
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
