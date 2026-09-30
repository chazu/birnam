//! Synchronous authored tools. Every intermediate stays rooted on the owner thread.
use super::*;
use std::ffi::OsString;

impl HostRuntime {
    pub(super) fn load_tool(&self, name: &[u8]) -> Result<(), HostError> {
        let q = self.scope();
        let module = self.global(&q, b"Module")?;
        let previous = q.call(module, b"libDir", &[])?;
        let directory = q.string(self.resources.lib_dir.as_os_str().as_bytes())?;
        q.call(module, b"libDir:", &[Operand::object(directory)])?;
        let loaded = (|| {
            let name =
                unsafe { self.id.intern_name(name) }.map_err(|e| native_error("tool name", e))?;
            let result = q.call(module, b"load:", &[Operand::object(name)])?;
            q.check_result(result)
        })();
        let restored = q.call(module, b"libDir:", &[Operand::object(previous)]);
        loaded?;
        q.check_result(restored?)
    }

    /// Project/image entry already established its context; it must not run
    /// nearest-directory Config or application main again.
    pub fn run_repl(&self, apply_config: bool) -> Result<(), HostError> {
        if self.global(&self.scope(), b"Module")? == self.named(b"nil")? {
            self.bootstrap_source()?;
        }
        self.load_tool(b"repl")?;
        let session = self.scope();
        let repl = self.global(&session, b"Repl")?;
        let result = session.call(
            repl,
            if apply_config {
                b"consoleHostInit"
            } else {
                b"consoleHostInitNoConfig"
            },
            &[],
        )?;
        // Teardown runs after initialization and on every subsequent failure.
        let run = (|| {
            session.check_result(result)?;
            let sealed = session.call(
                self.global(&session, b"DispatchGeneration")?,
                b"initializeCurrent",
                &[],
            )?;
            session.check_result(sealed)?;
            self.enter_running()?;
            let ready = session.call(
                repl,
                if apply_config {
                    b"consoleHostReady"
                } else {
                    b"consoleHostReadyNoMain"
                },
                &[],
            )?;
            session.check_result(ready)?;
            loop {
                // A per-turn scope prevents retaining the entire session history.
                let q = self.scope();
                let step = q.call(repl, b"hostStep", &[])?;
                q.check_result(step)?;
                if q.vector_size(step)? != 2 {
                    return Err(HostError("Repl hostStep: expected two fields".into()));
                }
                match q.symbol_text(q.vector_at(step, 0)?)?.as_str() {
                    "done" => break,
                    "continue" => {}
                    "evaluated" => {
                        let value = q.fulfill_intent(q.vector_at(step, 1)?)?;
                        let printed =
                            q.call(repl, b"printHostResult:", &[Operand::object(value)])?;
                        q.check_result(printed)?;
                    }
                    _ => return Err(HostError("Repl hostStep: invalid tag".into())),
                }
                self.send(self.named(b"Isolate")?, b"checkpoint", &[])?;
            }
            Ok(())
        })();
        let finished = session.call(repl, b"consoleDone", &[]);
        run?;
        session.check_result(finished?)
    }

    pub fn serve_socket(&self, port: u16, bind: &str) -> Result<(), HostError> {
        self.load_tool(b"socket")?;
        self.load_tool(b"repl")?;
        let q = self.scope();
        let port = q.call(
            self.named(b"Integer")?,
            b"fromWord:",
            &[Operand::word(port as usize)],
        )?;
        let bind = q.string(bind.as_bytes())?;
        self.enter_running()?;
        let result = q.call(
            self.global(&q, b"Repl")?,
            b"serveOn:bind:",
            &[Operand::object(port), Operand::object(bind)],
        )?;
        q.check_result(result)
    }

    pub fn run_cli(&self, args: &[OsString]) -> Result<(), HostError> {
        self.load_tool(b"cli")?;
        let q = self.scope();
        let mut argv = q.call(self.named(b"Vector")?, b"new", &[])?;
        for arg in args {
            let text = q.string(arg.as_os_str().as_bytes())?;
            argv = q.call(argv, b"add:", &[Operand::object(text)])?;
        }
        let workspace = q.string(self.resources.rust_workspace_dir.as_os_str().as_bytes())?;
        let guides = q.string(self.resources.guide_dir.as_os_str().as_bytes())?;
        let invoke = || {
            let status = q.call(
                self.global(&q, b"Cli")?,
                b"run:rustWorkspace:guides:",
                &[
                    Operand::object(argv),
                    Operand::object(workspace),
                    Operand::object(guides),
                ],
            )?;
            q.check_result(status)?;
            q.exact(status, b"IntegerVt")?;
            let status = self
                .send(
                    self.named(b"Integer")?,
                    b"wordOf:",
                    &[Operand::object(status)],
                )?
                .as_word()
                .ok_or_else(|| HostError("Cli returned an invalid status".into()))?;
            if status == 0 {
                Ok(())
            } else {
                Err(HostError(format!("tool exited with status {status}")))
            }
        };
        if args.first().is_some_and(|arg| arg == "--lsp") {
            self.with_project_bridge(invoke)
        } else {
            invoke()
        }
    }

    pub fn image_has_main(&self) -> Result<bool, HostError> {
        let q = self.scope();
        Ok(self.global(&q, b"main")? != self.named(b"nil")?)
    }
}
