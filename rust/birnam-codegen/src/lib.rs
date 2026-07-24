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
    String(&'a [u8]),
    Symbol(&'a [u8]),
    Global(&'a [u8]),
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
        self.bytes.extend_from_slice(bytes.len().to_string().as_bytes());
        self.bytes.extend_from_slice(b"L)");
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
        self.bytes.extend_from_slice(args.len().to_string().as_bytes());
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
            Expr::String(bytes) => self.emit_string(bytes),
            Expr::Symbol(name) => self.emit_symbol(name),
            Expr::Global(name) => self.emit_global(name),
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
                self.bytes.extend_from_slice(args.len().to_string().as_bytes());
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
    use super::{EmitError, Emitter, Expr};

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
}
