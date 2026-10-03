use crate::{HostError, HostRuntime, Resources, StartupPolicy};
use birnam_abi::id_protocol::Operand;
use std::{os::unix::ffi::OsStrExt, path::Path};
impl HostRuntime {
    pub fn run_worker(
        r: &Resources,
        p: StartupPolicy,
        path: &Path,
        receiver: &str,
        selector: &str,
        fd: i32,
    ) -> Result<(), HostError> {
        if fd < 0 {
            return Err(HostError("worker channel must be nonnegative".into()));
        }
        let h = Self::start_native(r, p.precise_gc)?;
        let q = h.scope();
        let path = q.string(path.as_os_str().as_bytes())?;
        let receiver = q.string(receiver.as_bytes())?;
        let selector = q.string(selector.as_bytes())?;
        let fd = q.call(
            h.named(b"Integer")?,
            b"fromWord:",
            &[Operand::word(fd as usize)],
        )?;
        h.call(
            b"HostEntry",
            b"worker:receiver:selector:fd:",
            &[
                Operand::object(path),
                Operand::object(receiver),
                Operand::object(selector),
                Operand::object(fd),
            ],
        )
    }
}
