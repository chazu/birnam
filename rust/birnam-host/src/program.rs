use crate::{HostError, HostRuntime};
use birnam_abi::id_protocol::Operand;
use std::{os::unix::ffi::OsStrExt, path::Path};
impl HostRuntime {
    pub fn eval_script(&self, path: &Path) -> Result<(), HostError> {
        let q = self.scope();
        let path = q.string(path.as_os_str().as_bytes())?;
        self.call(b"HostEntry", b"evalScript:", &[Operand::object(path)])
    }
    pub fn compile_module(&self, path: &Path, output: &Path) -> Result<(), HostError> {
        let q = self.scope();
        let path = q.string(path.as_os_str().as_bytes())?;
        let output = q.string(output.as_os_str().as_bytes())?;
        self.call(
            b"HostEntry",
            b"compile:output:",
            &[Operand::object(path), Operand::object(output)],
        )
    }
    pub fn load_artifact(&self, path: &Path) -> Result<(), HostError> {
        self.context.load_artifact(path)
    }
}
