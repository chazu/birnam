use crate::{HostError, HostRuntime};
use std::{ffi::OsString, os::unix::ffi::OsStrExt};
impl HostRuntime {
    pub fn run_repl(&self, apply_config: bool) -> Result<(), HostError> {
        self.context.run_repl(apply_config)
    }
    pub fn serve_socket(&self, port: u16, bind: &str) -> Result<(), HostError> {
        self.context.serve_socket(port, bind)
    }
    pub fn run_cli(&self, args: &[OsString]) -> Result<(), HostError> {
        let q = self.scope();
        let mut argv = q.call(self.named(b"Vector")?, b"new", &[])?;
        for arg in args {
            let s = q.string(arg.as_bytes())?;
            argv = q.call(argv, b"add:", &[s])?;
        }
        self.call(b"HostInteraction", b"cli:", &[argv])
    }
}
