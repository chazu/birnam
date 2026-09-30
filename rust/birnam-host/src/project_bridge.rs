//! Scoped, owner-thread callbacks for the authored LSP. No global Host pointer.
use super::project_runtime::values::Builder;
use super::*;
use crate::project_path_from_uri;
use birnam_abi::{borrowed_slice, id_protocol::NativeEntry};
use std::ffi::c_void;

unsafe extern "C" fn closed(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::failed(1, 410)
}
struct BridgeGuard<'a> {
    host: &'a HostRuntime,
    behavior: Oop,
    selectors: Vec<Oop>,
    restore_global: Option<(Oop, Oop, Oop)>,
}
impl Drop for BridgeGuard<'_> {
    fn drop(&mut self) {
        // Revocation is mandatory: native entries must never outlive this borrow.
        for selector in &self.selectors {
            if self
                .host
                .send(
                    self.behavior,
                    b"addMethod",
                    &[
                        Operand::object(*selector),
                        Operand::entry(NativeEntry::new(closed, std::ptr::null_mut())),
                    ],
                )
                .is_err()
            {
                std::process::abort();
            }
        }
        if let Some((globals, key, prior)) = self.restore_global {
            if self
                .host
                .send(
                    globals,
                    b"at:put:",
                    &[Operand::object(key), Operand::object(prior)],
                )
                .is_err()
            {
                std::process::abort();
            }
        }
    }
}
fn failure(q: &Scope<'_>, kind: &[u8], message: &[u8]) -> Result<Oop, HostError> {
    let mut map = q.call(q.host.named(b"Map")?, b"new", &[])?;
    for (key, value) in [
        (b"kind".as_slice(), q.string(kind)?),
        (b"message", q.string(message)?),
        (b"ok", q.host.named(b"false")?),
    ] {
        let key = q.string(key)?;
        map = q.call(
            map,
            b"at:put:",
            &[Operand::object(key), Operand::object(value)],
        )?;
    }
    Ok(map)
}
unsafe fn callback(
    context: *mut c_void,
    args: *const Operand,
    count: usize,
    snapshot: bool,
) -> Outcome {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> Result<Oop, HostError> {
            let host = unsafe { context.cast::<HostRuntime>().as_ref() }
                .ok_or_else(|| HostError("closed project bridge".into()))?;
            let arguments = unsafe { borrowed_slice(args, count) }
                .ok_or_else(|| HostError("invalid project arguments".into()))?;
            let [argument] = arguments else {
                return Err(HostError("project bridge expects one URI".into()));
            };
            let q = host.scope();
            let argument = q.keep(*argument)?;
            let uri = q.text(argument)?;
            let path = match project_path_from_uri(&uri) {
                Ok(path) => path,
                Err(_) => {
                    return failure(&q, b"projectUriInvalid", b"could not decode workspace URI");
                }
            };
            if !snapshot {
                return q.string(path.as_os_str().as_bytes());
            }
            let start = if path.is_dir() {
                path.as_path()
            } else {
                path.parent().unwrap_or(&path)
            };
            let observation = match host.observe_project_snapshot(start) {
                Ok(snapshot) => snapshot,
                Err(e) => return failure(&q, b"projectResolutionFailed", e.0.as_bytes()),
            };
            let input = Builder { q: &q }.snapshot(&observation)?;
            let result = q.call(
                host.global(&q, b"ProjectPolicy")?,
                b"resolveSnapshot:",
                &[Operand::object(input)],
            )?;
            if q.call(result, b"isError", &[])? == host.named(b"true")? {
                let kind = q.symbol_text(q.call(result, b"kind", &[])?)?;
                let message = q.string_bytes(q.call(result, b"message", &[])?)?;
                return failure(&q, kind.as_bytes(), &message);
            }
            q.exact(result, b"MapVt")?;
            let key = q.string(b"ok")?;
            q.call(
                result,
                b"at:put:",
                &[Operand::object(key), Operand::object(host.named(b"true")?)],
            )
        },
    ));
    match result {
        Ok(Ok(value)) => Outcome::loaded(Operand::object(value)),
        _ => Outcome::failed(1, 411),
    }
}
unsafe extern "C" fn path(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    unsafe { callback(context, args, count, false) }
}
unsafe extern "C" fn snapshot(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    unsafe { callback(context, args, count, true) }
}
impl HostRuntime {
    pub(super) fn with_project_bridge<T>(
        &self,
        invoke: impl FnOnce() -> Result<T, HostError>,
    ) -> Result<T, HostError> {
        self.project_support()?;
        let q = self.scope();
        let project = q.call(self.named(b"Object")?, b"clone", &[])?;
        let behavior = q.call(
            self.id.receiver,
            b"ensurePrivateVtableFor:",
            &[Operand::object(project)],
        )?;
        let mut guard = BridgeGuard {
            host: self,
            behavior,
            selectors: vec![],
            restore_global: None,
        };
        for (name, method) in [
            (
                b"pathForUri:".as_slice(),
                path as birnam_abi::id_protocol::Method,
            ),
            (b"snapshotForUri:", snapshot),
        ] {
            let selector = unsafe { self.id.intern_name(name) }
                .map_err(|e| native_error("project selector", e))?;
            guard.selectors.push(selector);
            self.send(
                behavior,
                b"addMethod",
                &[
                    Operand::object(selector),
                    Operand::entry(NativeEntry::new(
                        method,
                        (self as *const Self).cast_mut().cast(),
                    )),
                ],
            )?;
        }
        let key =
            unsafe { self.id.intern_name(b"Project") }.map_err(|e| native_error("Project", e))?;
        let globals = self.named(b"Globals")?;
        let prior = q.call(globals, b"at:", &[Operand::object(key)])?;
        guard.restore_global = Some((globals, key, prior));
        q.call(
            globals,
            b"at:put:",
            &[Operand::object(key), Operand::object(project)],
        )?;
        let result = invoke();
        // Revocation also happens on unwinding; a retained Project then fails closed.
        drop(guard);
        result
    }
}
