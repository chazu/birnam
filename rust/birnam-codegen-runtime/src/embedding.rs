//! Compile-time file embedding. Filesystem access belongs to System's file
//! owner, reached by a real send. The portable program stores the copied payload
//! as its existing binary-safe String literal followed by a live `asBytes` send.
use super::*;
use birnam_codegen::{Expr, Program, Statement, literal};
use std::{borrow::Cow, path::Path};
const LIMIT: usize = 16 * 1024 * 1024;
impl State {
    pub(super) unsafe fn embed_program(
        &self,
        scope: &Scope<'_>,
        program: &mut Program<'_>,
        filename: &[u8],
    ) -> Result<Option<Oop>, Outcome> {
        unsafe { self.embed_statements(scope, &mut program.body, filename, false, true) }
    }
    unsafe fn embed_statements(
        &self,
        scope: &Scope<'_>,
        statements: &mut [Statement<'_>],
        filename: &[u8],
        mut shadowed: bool,
        top_level: bool,
    ) -> Result<Option<Oop>, Outcome> {
        unsafe {
            for statement in statements {
                let binds_system = matches!(
                    statement,
                    Statement::Let {
                        name: b"System",
                        ..
                    }
                ) && !top_level;
                let (Statement::Let { value: expr, .. } | Statement::Expr(expr)) = statement;
                if let Some(error) = self.embed_expr(scope, expr, filename, shadowed)? {
                    return Ok(Some(error));
                }
                shadowed |= binds_system;
            }
            Ok(None)
        }
    }
    unsafe fn embed_error(
        &self,
        scope: &Scope<'_>,
        message: &[u8],
    ) -> Result<Option<Oop>, Outcome> {
        unsafe { Ok(Some(scope.keep(object(self.error(scope, message)?)?)?)) }
    }
    unsafe fn embed_expr(
        &self,
        scope: &Scope<'_>,
        expr: &mut Expr<'_>,
        filename: &[u8],
        shadowed: bool,
    ) -> Result<Option<Oop>, Outcome> {
        unsafe {
            match expr {
                Expr::Send {
                    receiver,
                    selector,
                    args,
                    source,
                } if !shadowed
                    && matches!(&**receiver, Expr::Global(b"System"))
                    && selector.as_ref() == b"embedFile:" =>
                {
                    let [Expr::StringLiteral(raw)] = args.as_slice() else {
                        return self
                            .embed_error(scope, b"embedFile: requires a literal String path");
                    };
                    let relative = literal::decode_body(&raw[1..raw.len() - 1]);
                    if relative.is_empty()
                        || relative.starts_with(b"/")
                        || relative.contains(&0)
                        || relative
                            .split(|&b| b == b'/')
                            .any(|p| p == b"." || p == b"..")
                    {
                        return self.embed_error(scope, b"embedFile: path must be a non-empty relative path without '.' or '..'");
                    }
                    let (Ok(relative), Ok(filename)) = (
                        std::str::from_utf8(&relative),
                        std::str::from_utf8(filename),
                    ) else {
                        return self.embed_error(scope, b"embedFile: path must be UTF-8");
                    };
                    let path = Path::new(filename)
                        .parent()
                        .unwrap_or(Path::new("."))
                        .join(relative);
                    let path = scope.keep(object(self.send(
                        self.named(b"String")?,
                        b"fromBytes:",
                        &[Operand::bytes(path.as_os_str().as_encoded_bytes())],
                    )?)?)?;
                    // Match generated source lookup: current Globals, then Namespace.
                    let name = scope.keep(self.id.intern_name(b"System")?)?;
                    let globals = scope.keep(self.named(b"Globals")?)?;
                    let nil = scope.keep(self.named(b"nil")?)?;
                    let mut system = scope.keep(object(self.send(
                        globals,
                        b"at:",
                        &[Operand::object(name)],
                    )?)?)?;
                    if system == nil {
                        system = scope.keep(self.named(b"System")?)?;
                    }
                    let value = scope.keep(object(self.send(
                        system,
                        b"embedFile:",
                        &[Operand::object(path)],
                    )?)?)?;
                    if self.is_error(value)? {
                        return Ok(Some(value));
                    }
                    if value == nil {
                        return self.embed_error(scope, b"embedFile: returned nil");
                    }
                    // Copy before any subsequent send, including scope cleanup.
                    let raw = self
                        .send(value, b"bytes", &[])?
                        .as_bytes()
                        .ok_or_else(invalid)?;
                    if raw.length > LIMIT {
                        return self
                            .embed_error(scope, b"embedFile: file exceeds the 16 MiB limit");
                    }
                    let bytes = borrowed_bytes(raw.pointer, raw.length)
                        .ok_or_else(invalid)?
                        .to_vec();
                    *expr = Expr::Send {
                        receiver: Box::new(Expr::StringOwned(bytes)),
                        selector: Cow::Borrowed(b"asBytes"),
                        args: Vec::new(),
                        source,
                    };
                }
                Expr::Send { receiver, args, .. } => {
                    if let Some(error) = self.embed_expr(scope, receiver, filename, shadowed)? {
                        return Ok(Some(error));
                    }
                    for arg in args {
                        if let Some(error) = self.embed_expr(scope, arg, filename, shadowed)? {
                            return Ok(Some(error));
                        }
                    }
                }
                Expr::Block(block) => {
                    return self.embed_statements(
                        scope,
                        &mut block.body,
                        filename,
                        shadowed || block.params.contains(&b"System".as_slice()),
                        false,
                    );
                }
                _ => {}
            }
            Ok(None)
        }
    }
}
