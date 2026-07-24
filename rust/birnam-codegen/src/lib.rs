//! Pure deterministic C-emission primitives for the N6.2 Codegen port.
//!
//! This crate deliberately has no IdApi, loader, allocator, or runtime
//! dependency. It owns only byte-buffer construction; CST walking and runtime
//! orchestration arrive in later N6.2/N6.3 slices.

/// A bounded-independent byte emitter. Callers retain ownership of the returned
/// translation-unit bytes and may compare them byte-for-byte with the Zig oracle.
#[derive(Debug, Default)]
pub struct Emitter {
    bytes: Vec<u8>,
}

/// The pure expression subset consumed by the first Rust emission seam.
///
/// This is intentionally not a parser CST or a runtime Oop graph. A later
/// adapter will lower the Rust PEG CST into this representation after the
/// generated-byte contract is frozen.
#[derive(Debug, PartialEq)]
pub enum Expr<'a> {
    SmallInt(i64),
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
    Send {
        receiver: Box<Expr<'a>>,
        selector: &'a [u8],
        args: Vec<Expr<'a>>,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum EmitError {
    DepthLimit,
}

const MAX_EXPR_DEPTH: usize = 4096;

const C_RUNTIME_PRELUDE: &[u8] = br#"extern void* birnam_mkSmallInt(long v);
#define MKV(x) (birnam_mkSmallInt((long)(x)))
extern void* birnam_send(void* rcv, void* sel, void** args, long n);
extern void* birnam_intern(const char* name);
extern void* birnam_mkString(const char* p, long n);
extern void* birnam_mkFloat(double v);
extern void* birnam_nil(void);
extern void* birnam_true(void);
extern void* birnam_false(void);
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

    pub fn emit_send(&mut self, receiver: &[u8], selector: &[u8], args: &[&[u8]]) {
        self.bytes.extend_from_slice(b"birnam_send(");
        self.push(receiver);
        self.bytes.extend_from_slice(b", ");
        self.emit_symbol(selector);
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

    fn emit_expr_at_depth(&mut self, expr: &Expr<'_>, depth: usize) -> Result<(), EmitError> {
        if depth >= MAX_EXPR_DEPTH {
            return Err(EmitError::DepthLimit);
        }
        match expr {
            Expr::SmallInt(value) => self.emit_small_int(*value),
            Expr::Float(literal) => self.emit_float(literal),
            Expr::String(bytes) => self.emit_string(bytes),
            Expr::StringLiteral(source) => self.emit_string_literal(source),
            Expr::Symbol(name) => self.emit_symbol(name),
            Expr::Global(name) => self.emit_global(name),
            Expr::Nil => self.emit_nil(),
            Expr::True => self.emit_true(),
            Expr::False => self.emit_false(),
            Expr::Send {
                receiver,
                selector,
                args,
            } => {
                self.bytes.extend_from_slice(b"birnam_send(");
                self.emit_expr_at_depth(receiver, depth + 1)?;
                self.bytes.extend_from_slice(b", ");
                self.emit_symbol(selector);
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

    pub fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::{EmitError, Emitter, Expr, TranslationUnit};

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
            selector: b"at:",
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
                selector: b"value",
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
            selector: b"ifTrue:ifFalse:",
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
    fn assembles_jit_and_artifact_units_deterministically() {
        let mut jit = TranslationUnit::new(false);
        jit.add_baked_global(b"Integer");
        jit.push_body(b"return MKV(42L);\n");
        assert_eq!(
            String::from_utf8(jit.finish()).unwrap(),
            "extern void* birnam_mkSmallInt(long v);\n#define MKV(x) (birnam_mkSmallInt((long)(x)))\nextern void* birnam_send(void* rcv, void* sel, void** args, long n);\nextern void* birnam_intern(const char* name);\nextern void* birnam_mkString(const char* p, long n);\nextern void* birnam_mkFloat(double v);\nextern void* birnam_nil(void);\nextern void* birnam_true(void);\nextern void* birnam_false(void);\nextern char __g0[];\nvoid* __ev(void* self, void** args, long n){ (void)self;(void)args;(void)n;\nreturn MKV(42L);\n}\n"
        );

        let mut artifact = TranslationUnit::new(true);
        artifact.add_baked_global(b"Integer");
        artifact.push_body(b"return __g0;\n");
        let output = String::from_utf8(artifact.finish()).unwrap();
        assert!(output.contains("static void* __g0;"));
        assert!(output.contains("void __birnam_unit_init(void){"));
        assert!(output.contains("__g0 = birnam_intern(\"Integer\");"));
    }
}
