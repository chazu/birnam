use crate::HostRuntime; use birnam_host_support::{StartupPolicy,HostError,native_error}; use birnam_abi::{Oop};use std::{os::unix::ffi::OsStrExt,ffi::OsString};
impl HostRuntime<'_> {
    pub(crate) fn load_tool(&self, name: &[u8]) -> Result<(), HostError> {
        let q = self.scope();
        let module = self.global(&q, b"Module")?;
        let previous = q.call(module, b"libDir", &[])?;
        let directory = q.string(self.resources.lib_dir.as_os_str().as_bytes())?;
        q.call(module, b"libDir:", &[directory])?;
        let loaded = (|| {
            let name =
                self.intern(name).map_err(|e| native_error("tool name", e))?;
            let result = q.call(module, b"load:", &[name])?;
            q.check_result(result)
        })();
        let restored = q.call(module, b"libDir:", &[previous]);
        loaded?;
        q.check_result(restored?)
    }

    /// Project/image entry already established its context; it must not run
    /// nearest-directory Config or application main again.
    pub fn run_repl(&self, apply_config: bool) -> Result<(), HostError> {
        // Module and Lang are present even on the minimal compiled floor.
        // REPL entry requires the authored bootstrap, but does not adopt a baseline.
        let q = self.scope();
        let module = self.global(&q, b"Module")?;
        let stdlib = self.intern(b"stdlib")
            .map_err(|e| native_error("stdlib name", e))?;
        if module == self.named(b"nil")?
            || q.call(module, b"isLoaded:", &[stdlib])? != self.named(b"true")?
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
                        let value = q.vector_at(step, 1)?;
                        let printed =
                            q.call(repl, b"printHostResult:", &[value])?;
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
            &[birnam_host_support::size(port as usize)?],
        )?;
        let bind = q.string(bind.as_bytes())?;
        let repl = self.global(&q, b"Repl")?;
        let socket = self.global(&q, b"Socket")?;
        let listener = q.call(
            repl,
            b"socketHostListen:bind:",
            &[port, bind],
        )?;
        q.check_result(listener)?;
        let zero = q.call(self.named(b"Integer")?, b"fromWord:", &[birnam_host_support::size(0)?])?;
        let serving = (|| -> Result<(), HostError> {
            loop {
                let client = self.scope();
                let fd = client.call(socket, b"rawAccept:", &[listener])?;
                client.check_result(fd)?;
                if client.call(fd, b"<", &[zero])? == self.named(b"true")? {
                    continue;
                }
                let connection: Result<Oop, HostError> = (|| {
                    let conn = client.call(socket, b"wrapFd:", &[fd])?;
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
        let sent = session.call(conn, b"sendString:", &[greeting])?;
        session.check_result(sent)?;
        loop {
            let q = self.scope();
            let step = q.call(
                repl,
                b"socketHostStep:buffer:",
                &[conn, buffer],
            )?;
            q.check_result(step)?;
            if q.vector_size(step)? != 2 {
                return Err(HostError("Repl socketHostStep: expected two fields".into()));
            }
            match q.symbol_text(q.vector_at(step, 0)?)?.as_str() {
                "done" => return Ok(()),
                "continue" => {}
                "evaluated" => {
                    let value = q.vector_at(step, 1)?;
                    let printed = q.call(
                        repl,
                        b"printSocketResult:to:",
                        &[value, conn],
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
            argv = q.call(argv, b"add:", &[text])?;
        }
        let workspace = q.string(self.resources.rust_workspace_dir.as_os_str().as_bytes())?;
        let guides = q.string(self.resources.guide_dir.as_os_str().as_bytes())?;
        let invoke = || {
            let status = q.call(
                self.global(&q, b"Cli")?,
                b"run:rustWorkspace:guides:",
                &[
                    argv,
                    workspace,
                    guides,
                ],
            )?;
            q.check_result(status)?;
            q.exact(status, b"IntegerVt")?;
            let status=q.word_bits(q.call(self.named(b"Integer")?,b"wordOf:",&[status])?)?;
            if status == 0 {
                Ok(())
            } else {
                Err(HostError(format!("tool exited with status {status}")))
            }
        };
        invoke()
    }


}
