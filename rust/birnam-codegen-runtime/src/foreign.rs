//! Explicit build-time foreign declarations; ordinary wrap: stays dynamic.
use super::*;
use birnam_codegen::{Expr, Program, Statement, literal};
use std::borrow::Cow;

fn bytes_expr<'a>(bytes: Vec<u8>) -> Expr<'a> {
    Expr::Send {
        receiver: Box::new(Expr::StringOwned(bytes)),
        selector: Cow::Borrowed(b"asBytes"),
        args: vec![],
        source: b"",
    }
}
impl State {
    pub(super) unsafe fn prepare_foreign(
        &self,
        q: &Scope<'_>,
        program: &mut Program<'_>,
    ) -> Result<Option<Oop>, Outcome> {
        unsafe { self.foreign_statements(q, &mut program.body, false, 0) }
    }
    unsafe fn foreign_statements(
        &self,
        q: &Scope<'_>,
        body: &mut [Statement<'_>],
        mut shadowed: bool,
        depth: usize,
    ) -> Result<Option<Oop>, Outcome> {
        unsafe {
            for statement in body {
                let binds = matches!(statement, Statement::Let { name: b"Alien", .. });
                let (Statement::Let { value, .. } | Statement::Expr(value)) = statement;
                if let Some(error) = self.foreign_expr(q, value, shadowed, depth)? {
                    return Ok(Some(error));
                }
                shadowed |= binds;
            }
            Ok(None)
        }
    }
    unsafe fn foreign_expr(
        &self,
        q: &Scope<'_>,
        expr: &mut Expr<'_>,
        shadowed: bool,
        depth: usize,
    ) -> Result<Option<Oop>, Outcome> {
        unsafe {
            if depth > 4096 {
                return self.foreign_error(q, b"foreign declaration nesting exceeds limit");
            }
            match expr {
                Expr::Send {
                    receiver,
                    selector,
                    args,
                    ..
                } => {
                    if let Some(e) = self.foreign_expr(q, receiver, shadowed, depth + 1)? {
                        return Ok(Some(e));
                    }
                    for arg in args.iter_mut() {
                        if let Some(e) = self.foreign_expr(q, arg, shadowed, depth + 1)? {
                            return Ok(Some(e));
                        }
                    }
                    if !shadowed
                        && matches!(&**receiver, Expr::Global(b"Alien"))
                        && selector.as_ref() == b"bind:signature:as:on:"
                    {
                        let [
                            Expr::StringLiteral(function),
                            Expr::StringLiteral(signature),
                            _,
                            _,
                        ] = args.as_slice()
                        else {
                            return self.foreign_error(q, b"Alien bind: requires literal function and signature Strings; use wrap: for runtime bindings");
                        };
                        let function = q.keep(
                            self.text(q, &literal::decode_body(&function[1..function.len() - 1]))?,
                        )?;
                        let signature = q.keep(self.text(
                            q,
                            &literal::decode_body(&signature[1..signature.len() - 1]),
                        )?)?;
                        let key = q.keep(self.id.intern_name(b"AlienCompiler")?)?;
                        let compiler = q.keep(object(self.send(
                            self.named(b"Globals")?,
                            b"at:",
                            &[Operand::object(key)],
                        )?)?)?;
                        let c = q.keep(object(self.send(
                            compiler,
                            b"compile:signature:",
                            &[Operand::object(function), Operand::object(signature)],
                        )?)?)?;
                        if self.is_error(c)? {
                            return Ok(Some(c));
                        }
                        let bytes = q.keep(object(self.send(
                            self.named(b"TCC")?,
                            b"compileObject:",
                            &[Operand::object(c)],
                        )?)?)?;
                        if self.is_error(bytes)? {
                            return Ok(Some(bytes));
                        }
                        args.push(bytes_expr(self.bytes(bytes)?));
                        *selector = Cow::Borrowed(b"bind:signature:as:on:code:");
                    }
                }
                Expr::Block(block) => {
                    return self.foreign_statements(
                        q,
                        &mut block.body,
                        shadowed || block.params.contains(&b"Alien".as_slice()),
                        depth + 1,
                    );
                }
                _ => {}
            }
            Ok(None)
        }
    }
    unsafe fn foreign_error(&self, q: &Scope<'_>, message: &[u8]) -> Result<Option<Oop>, Outcome> {
        unsafe { Ok(Some(q.keep(object(self.error(q, message)?)?)?)) }
    }
}
