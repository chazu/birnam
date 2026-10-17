use crate::HostRuntime; use birnam_host_support::{StartupPolicy,HostError,native_error}; use birnam_abi::{Oop,id_protocol::Operand};use std::{os::unix::ffi::OsStrExt,ffi::OsString};
impl HostRuntime<'_> {
    pub(crate) fn load_tool(&self, name: &[u8]) -> Result<(), HostError> {
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
        // Module and Lang are present even on the minimal compiled floor.
        // REPL entry also requires the authored generation/control bootstrap.
        if self.global(&self.scope(), b"Module")? == self.named(b"nil")?
            || self.global(&self.scope(), b"DispatchGeneration")? == self.named(b"nil")?
        {
            if !StartupPolicy::from_environment().source_bootstrap {
                return Err(HostError("image lacks an authored bootstrap; regenerate it with the current Birnam or use BIRNAM_FORCE_SOURCE=1 for explicit source recovery".into()));
            }
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
        let repl = self.global(&q, b"Repl")?;
        let socket = self.global(&q, b"Socket")?;
        let sealed = q.call(
            self.global(&q, b"DispatchGeneration")?,
            b"initializeCurrent",
            &[],
        )?;
        q.check_result(sealed)?;
        let listener = q.call(
            repl,
            b"socketHostListen:bind:",
            &[Operand::object(port), Operand::object(bind)],
        )?;
        q.check_result(listener)?;
        let zero = q.call(self.named(b"Integer")?, b"fromWord:", &[Operand::word(0)])?;
        let serving = (|| -> Result<(), HostError> {
            loop {
                let client = self.scope();
                let fd = client.call(socket, b"rawAccept:", &[Operand::object(listener)])?;
                client.check_result(fd)?;
                if client.call(fd, b"<", &[Operand::object(zero)])? == self.named(b"true")? {
                    continue;
                }
                let connection: Result<Oop, HostError> = (|| {
                    let conn = client.call(socket, b"wrapFd:", &[Operand::object(fd)])?;
                    client.check_result(conn)?;
                    Ok(conn)
                })();
                match connection {
                    Ok(conn) => {
                        // A client fault ends only this session. Always close its
                        // buffer and descriptor before accepting the next client.
                        let _ = self.socket_session(repl, conn);
                        let _ = client.call(conn, b"close", &[]);
                    }
                    Err(_) => {
                        let _ = client.call(fd, b"closeFd", &[]);
                    }
                }
                drop(client);
                self.send(self.named(b"Isolate")?, b"checkpoint", &[])?;
            }
        })();
        let closed = q.call(listener, b"closeFd", &[]);
        serving?;
        q.check_result(closed?)
    }

    fn socket_session(&self, repl: Oop, conn: Oop) -> Result<(), HostError> {
        let session = self.scope();
        let empty = session.string(b"")?;
        let buffer = session.call(empty, b"asCell", &[])?;
        let greeting = session.string(
            "birnam socket REPL — type a form (multi-line ok), :quit to leave.\r\nbirnam> "
                .as_bytes(),
        )?;
        let sent = session.call(conn, b"sendString:", &[Operand::object(greeting)])?;
        session.check_result(sent)?;
        loop {
            let q = self.scope();
            let step = q.call(
                repl,
                b"socketHostStep:buffer:",
                &[Operand::object(conn), Operand::object(buffer)],
            )?;
            q.check_result(step)?;
            if q.vector_size(step)? != 2 {
                return Err(HostError("Repl socketHostStep: expected two fields".into()));
            }
            match q.symbol_text(q.vector_at(step, 0)?)?.as_str() {
                "done" => return Ok(()),
                "continue" => {}
                "evaluated" => {
                    let value = q.fulfill_intent(q.vector_at(step, 1)?)?;
                    let printed = q.call(
                        repl,
                        b"printSocketResult:to:",
                        &[Operand::object(value), Operand::object(conn)],
                    )?;
                    q.check_result(printed)?;
                }
                _ => return Err(HostError("Repl socketHostStep: invalid tag".into())),
            }
            drop(q);
            self.send(self.named(b"Isolate")?, b"checkpoint", &[])?;
        }
    }

    pub fn run_cli_direct(&self, args: &[OsString]) -> Result<(), HostError> {
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
        invoke()
    }


}
