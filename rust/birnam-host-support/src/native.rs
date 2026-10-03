//! Local constructor, binding and value-copy helpers. No language owner API is
//! represented by a Rust pointer or dependency-specific service table.
use crate::{Context, HostError, Resources, Scope, native_error};
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{self as abi, NativeEntry, Operand, Outcome, Protocol},
};
use std::{
    ffi::{OsString, c_void},
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};
pub type Handler = fn(&Context, &str, &[Operand]) -> Result<Operand, HostError>;
pub struct Owner {
    context: Context,
    receiver: Oop,
}
struct Binding {
    owner: *const Owner,
    operation: &'static str,
    handler: Handler,
}
fn invalid() -> Outcome {
    Outcome::failed(abi::INVALID_REQUEST, 0)
}
unsafe extern "C" fn invoke(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    let result = (|| -> Result<Operand, HostError> {
        let binding = unsafe { &*c.cast::<Binding>() };
        let owner = unsafe { &*binding.owner };
        let h = &owner.context;
        if h.thread != std::thread::current().id() {
            return Err(HostError(
                "native Host owner called from another thread".into(),
            ));
        }
        let args = unsafe { borrowed_slice(a, n) }
            .ok_or_else(|| HostError("invalid Host arguments".into()))?;
        let q = h.scope();
        q.keep(Operand::object(r))?;
        for arg in args {
            if arg.kind() == abi::OBJECT {
                q.keep(*arg)?;
            }
        }
        match (binding.handler)(h, binding.operation, args) {
            Ok(value) => Ok(value),
            Err(error) => {
                let kind = q.keep(Operand::object(
                    unsafe { h.id.intern_name(b"hostError") }
                        .map_err(|e| native_error("host error kind", e))?,
                ))?;
                let message = q.string(error.0.as_bytes())?;
                Ok(Operand::object(q.call(
                    h.named(b"Error")?,
                    b"kind:message:",
                    &[Operand::object(kind), Operand::object(message)],
                )?))
            }
        }
    })();
    result.map_or_else(|_| invalid(), Outcome::loaded)
}
/// Each library supplies its own singleton, selectors and local handler.
/// Constructor operands are Namespace, prefix, lib, workspace, guides, Id path.
pub unsafe fn initialize(
    id: *const Protocol,
    a: *const Operand,
    n: usize,
    instance: &AtomicPtr<Owner>,
    busy: &AtomicBool,
    name: &[u8],
    methods: &[&'static str],
    handler: Handler,
) -> Outcome {
    let result = (|| -> Result<Operand, Outcome> {
        if !unsafe { abi::valid_protocol(id) } {
            return Err(invalid());
        }
        let [ns, prefix, lib, workspace, guides, id_path] =
            unsafe { borrowed_slice(a, n) }.ok_or_else(invalid)?
        else {
            return Err(invalid());
        };
        let ns = ns
            .as_object()
            .filter(|p| !p.is_null())
            .ok_or_else(invalid)?;
        let id = unsafe { *id };
        let prior = instance.load(Ordering::Acquire);
        if !prior.is_null() {
            let s = unsafe { &*prior };
            if s.context.id.receiver != id.receiver
                || s.context.namespace != ns
                || s.context.thread != std::thread::current().id()
            {
                return Err(invalid());
            }
        }
        if busy.swap(true, Ordering::AcqRel) {
            return Err(invalid());
        }
        let initialized = (|| -> Result<Operand, Outcome> {
            let mut h =
                Context::from_namespace(id, ns, Resources::default()).map_err(|_| invalid())?;
            let q = h.scope();
            // These are ordinary String Objects, usable from System's native
            // loader as well as the executable. Retain every initializer input
            // before any effectful value projection, and copy each byte borrow.
            for value in [prefix, lib, workspace, guides, id_path] {
                q.keep(*value).map_err(|_| invalid())?;
            }
            let path = |arg: &Operand| -> Result<PathBuf, Outcome> {
                Ok(PathBuf::from(OsString::from_vec(
                    q.string_bytes(arg.as_object().ok_or_else(invalid)?)
                        .map_err(|_| invalid())?,
                )))
            };
            let resources = Resources {
                prefix: path(prefix)?,
                lib_dir: path(lib)?,
                rust_workspace_dir: path(workspace)?,
                guide_dir: path(guides)?,
                id_dylib: path(id_path)?,
            };
            drop(q);
            h.resources = resources;
            let prior = instance.load(Ordering::Acquire);
            if !prior.is_null() {
                let s = unsafe { &*prior };
                return if s.context.id.receiver == id.receiver
                    && s.context.namespace == ns
                    && s.context.resources == h.resources
                    && s.context.thread == std::thread::current().id()
                {
                    Ok(Operand::object(s.receiver))
                } else {
                    Err(invalid())
                };
            }
            let built = (|| -> Result<Operand, HostError> {
                let q = h.scope();
                let receiver = q.call(h.named(b"Object")?, b"clone", &[])?;
                let behavior = q.call(
                    id.receiver,
                    b"ensurePrivateVtableFor:",
                    &[Operand::object(receiver)],
                )?;
                // Completed owner and its callback contexts have process lifetime.
                h.send(h.roots, b"retain:", &[Operand::object(receiver)])?;
                h.send(h.roots, b"retain:", &[Operand::object(behavior)])?;
                drop(q);
                let state = Box::into_raw(Box::new(Owner {
                    context: h,
                    receiver,
                }));
                let h = &unsafe { &*state }.context;
                for operation in methods {
                    let selector = unsafe { id.intern_name(operation.as_bytes()) }
                        .map_err(|e| native_error("Host selector", e))?;
                    let binding = Box::into_raw(Box::new(Binding {
                        owner: state,
                        operation,
                        handler,
                    }));
                    h.send(
                        behavior,
                        b"addMethod",
                        &[
                            Operand::object(selector),
                            Operand::entry(NativeEntry::new(invoke, binding.cast())),
                        ],
                    )?;
                }
                let symbol = unsafe { id.intern_name(name) }
                    .map_err(|e| native_error("Host owner name", e))?;
                h.send(
                    ns,
                    b"at:put:",
                    &[Operand::object(symbol), Operand::object(receiver)],
                )?;
                instance.store(state, Ordering::Release);
                Ok(Operand::object(receiver))
            })();
            built.map_err(|_| invalid())
        })();
        busy.store(false, Ordering::Release);
        initialized
    })();
    result.map_or_else(|e| e, Outcome::loaded)
}
pub fn argument(q: &Scope<'_>, args: &[Operand], i: usize) -> Result<Oop, HostError> {
    q.keep(
        *args
            .get(i)
            .ok_or_else(|| HostError("missing Host argument".into()))?,
    )
}
pub fn bytes(q: &Scope<'_>, args: &[Operand], i: usize) -> Result<Vec<u8>, HostError> {
    q.string_bytes(argument(q, args, i)?)
}
pub fn path(q: &Scope<'_>, args: &[Operand], i: usize) -> Result<PathBuf, HostError> {
    Ok(PathBuf::from(OsString::from_vec(bytes(q, args, i)?)))
}
pub fn text(q: &Scope<'_>, args: &[Operand], i: usize) -> Result<String, HostError> {
    String::from_utf8(bytes(q, args, i)?).map_err(|e| HostError(e.to_string()))
}
pub fn boolean(q: &Scope<'_>, args: &[Operand], i: usize) -> Result<bool, HostError> {
    let o = argument(q, args, i)?;
    if o == q.host.named(b"true")? {
        Ok(true)
    } else if o == q.host.named(b"false")? {
        Ok(false)
    } else {
        Err(HostError("expected Boolean".into()))
    }
}
pub fn word(q: &Scope<'_>, args: &[Operand], i: usize) -> Result<usize, HostError> {
    let o = argument(q, args, i)?;
    q.exact(o, b"IntegerVt")?;
    q.host
        .send(q.host.named(b"Integer")?, b"wordOf:", &[Operand::object(o)])?
        .as_word()
        .ok_or_else(|| HostError("expected Integer word".into()))
}
pub fn argv(q: &Scope<'_>, args: &[Operand], i: usize) -> Result<Vec<OsString>, HostError> {
    let a = argument(q, args, i)?;
    (0..q.vector_size(a)?)
        .map(|j| Ok(OsString::from_vec(q.string_bytes(q.vector_at(a, j)?)?)))
        .collect()
}
pub fn encode<T: serde::Serialize>(q: &Scope<'_>, value: &T) -> Result<Operand, HostError> {
    let bytes = serde_json::to_vec(value).map_err(|e| HostError(e.to_string()))?;
    Ok(Operand::object(q.string(&bytes)?))
}
pub fn decode<T: serde::de::DeserializeOwned>(
    q: &Scope<'_>,
    args: &[Operand],
    i: usize,
) -> Result<T, HostError> {
    serde_json::from_slice(&bytes(q, args, i)?).map_err(|e| HostError(e.to_string()))
}
pub mod path_bytes {
    use super::*;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    pub fn serialize<S: Serializer>(p: &Path, s: S) -> Result<S::Ok, S::Error> {
        p.as_os_str().as_bytes().serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<PathBuf, D::Error> {
        Vec::<u8>::deserialize(d).map(|b| PathBuf::from(OsString::from_vec(b)))
    }
}
pub mod optional_path_bytes {
    use super::*;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    pub fn serialize<S: Serializer>(p: &Option<PathBuf>, s: S) -> Result<S::Ok, S::Error> {
        p.as_ref().map(|p| p.as_os_str().as_bytes()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<PathBuf>, D::Error> {
        Option::<Vec<u8>>::deserialize(d).map(|p| p.map(|b| PathBuf::from(OsString::from_vec(b))))
    }
}
pub mod argv_bytes {
    use super::*;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    pub fn serialize<S: Serializer>(p: &[OsString], s: S) -> Result<S::Ok, S::Error> {
        p.iter()
            .map(|p| p.as_bytes())
            .collect::<Vec<_>>()
            .serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<OsString>, D::Error> {
        Vec::<Vec<u8>>::deserialize(d).map(|p| p.into_iter().map(OsString::from_vec).collect())
    }
}
