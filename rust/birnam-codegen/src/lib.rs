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
        self.bytes.extend_from_slice(b"birnam_mkSmallInt(");
        self.bytes.extend_from_slice(value.to_string().as_bytes());
        self.bytes.extend_from_slice(b")");
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

    pub fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::Emitter;

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
        emitter.emit_send(b"birnam_mkSmallInt(1)", b"+", &[b"birnam_mkSmallInt(2)"]);
        assert_eq!(
            String::from_utf8(emitter.finish()).unwrap(),
            "birnam_mkSmallInt(42); birnam_global(\"Integer\"); birnam_send(birnam_mkSmallInt(1), birnam_intern(\"+\"), (void*[]){birnam_mkSmallInt(2)}, 1L)"
        );
    }
}
