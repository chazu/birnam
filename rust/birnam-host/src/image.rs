use crate::{HostError, HostRuntime, Resources, StartupPolicy};
use std::{os::unix::ffi::OsStrExt, path::Path};
impl HostRuntime {
    pub fn start_image(r: &Resources, p: StartupPolicy, path: &Path) -> Result<Self, HostError> {
        let h = Self::start_native(r, p.precise_gc)?;
        h.start_image_mode(path, false, false)?;
        Ok(h)
    }
    pub fn start_image_rebuilding(
        r: &Resources,
        p: StartupPolicy,
        path: &Path,
    ) -> Result<Self, HostError> {
        let h = Self::start_native(r, p.precise_gc)?;
        h.start_image_mode(path, true, false)?;
        Ok(h)
    }
    pub fn start_image_bytes(
        r: &Resources,
        p: StartupPolicy,
        bytes: &[u8],
    ) -> Result<Self, HostError> {
        let h = Self::start_native(r, p.precise_gc)?;
        let q = h.scope();
        let mut chunks = bytes.chunks(8192);
        let values = |bytes: &[u8]| bytes.iter()
            .map(|b| birnam_abi::size(*b as usize).unwrap()).collect::<Vec<_>>();
        let mut buffer = q.call(
            h.named(b"Bytes")?, b"fromByteValues", &values(chunks.next().unwrap_or(&[])),
        )?;
        for chunk in chunks {
            let next = h.send(buffer, b"appendByteValues", &values(chunk))?;
            if next != buffer { q.keep(next)?; }
            buffer = next;
        }
        h.call(b"HostEntry", b"startImageBytes:", &[buffer])?;
        drop(q);
        Ok(h)
    }
    pub fn load_image(&self, path: &Path) -> Result<(), HostError> {
        self.load_image_mode(path, false, false)
    }
    fn start_image_mode(&self, path: &Path, rebuild: bool, compiled: bool) -> Result<(), HostError> {
        self.image_mode(path, rebuild, compiled, true)
    }
    pub fn load_image_mode(&self, path: &Path, rebuild: bool, compiled: bool) -> Result<(), HostError> {
        self.image_mode(path, rebuild, compiled, false)
    }
    fn image_mode(&self, path: &Path, rebuild: bool, compiled: bool, startup: bool) -> Result<(), HostError> {
        let q = self.scope();
        let path = q.string(path.as_os_str().as_bytes())?;
        self.call(
            b"HostEntry",
            if startup { b"startImage:rebuild:compiled:" } else { b"loadImage:rebuild:compiled:" },
            &[
                path,
                self.named(if rebuild { b"true" } else { b"false" })?,
                self.named(if compiled { b"true" } else { b"false" })?,
            ],
        )
    }
    pub fn run_image_main(&self) -> Result<(), HostError> {
        self.call(b"HostEntry", b"imageMain", &[])
    }
    pub fn image_has_main(&self) -> Result<bool, HostError> {
        let q = self.scope();
        Ok(self
            .context
            .owner_call(&q, b"HostEntry", b"imageHasMain", &[])?
            == self.named(b"true")?)
    }
}
