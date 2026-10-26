//! Permanent read-only project queries. Their contexts belong to the native
//! owner for process lifetime; editor entry does not install or revoke callbacks.
use super::{Project, data::*, values::Builder};
use birnam_abi::Oop;
use birnam_host_support::{HostError, Scope};
use std::os::unix::ffi::OsStrExt;
fn failure(q: &Scope<'_>, kind: &[u8], message: &[u8]) -> Result<Oop, HostError> {
    let mut map = q.call(q.host.named(b"Map")?, b"new", &[])?;
    for (key, value) in [
        (b"kind".as_slice(), q.string(kind)?),
        (b"message", q.string(message)?),
        (b"ok", q.host.named(b"false")?),
    ] {
        let key = q.string(key)?;
        map = q.call(map, b"at:put:", &[key, value])?;
    }
    Ok(map)
}
impl Project<'_> {
    pub(crate) fn path_for_uri(&self, uri: &str) -> Result<Oop, HostError> {
        let q = self.host.scope();
        let result = match project_path_from_uri(uri) {
            Ok(path) => q.string(path.as_os_str().as_bytes()),
            Err(_) => failure(&q, b"projectUriInvalid", b"could not decode workspace URI"),
        };
        result.and_then(|v| q.finish(v))
    }
    pub(crate) fn snapshot_for_uri(&self, uri: &str) -> Result<Oop, HostError> {
        let q = self.host.scope();
        let result = (|| {
            let path = match project_path_from_uri(uri) {
                Ok(path) => path,
                Err(_) => {
                    return failure(&q, b"projectUriInvalid", b"could not decode workspace URI");
                }
            };
            let start = if path.is_dir() {
                path.as_path()
            } else {
                path.parent().unwrap_or(&path)
            };
            let observation = match self.observe_project_snapshot(start) {
                Ok(snapshot) => snapshot,
                Err(e) => return failure(&q, b"projectResolutionFailed", e.0.as_bytes()),
            };
            let input = Builder { q: &q }.snapshot(&observation)?;
            let result = q.call(
                self.host.global(&q, b"ProjectPolicy")?,
                b"resolveSnapshot:",
                &[input],
            )?;
            if q.call(result, b"isError", &[])? == self.host.named(b"true")? {
                let kind = q.symbol_text(q.call(result, b"kind", &[])?)?;
                let message = q.string_bytes(q.call(result, b"message", &[])?)?;
                return failure(&q, kind.as_bytes(), &message);
            }
            q.exact(result, b"MapVt")?;
            let key = q.string(b"ok")?;
            q.call(result, b"at:put:", &[key, self.host.named(b"true")?])
        })();
        result.and_then(|v| q.finish(v))
    }
}
