//! Host-local construction and value copies. All cross-owner traffic is Oops.
use crate::{Context, HostError, Resources, Scope, native_error};
use birnam_abi::{self as abi, Oop, borrowed_slice, id_protocol::Protocol};
use std::{
    cell::Cell,
    ffi::OsString,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};
pub use crate::roots::trace_entry as trace_roots;
pub type Handler = fn(&Context, &str, &[Oop]) -> Result<Oop, HostError>;
pub struct Owner {
    context: Context,
    receiver: Oop,
    ready: Cell<bool>,
}
pub unsafe fn invoke(
    instance: &AtomicPtr<Owner>,
    root: Oop,
    c: Oop,
    r: Oop,
    a: *const Oop,
    n: usize,
    operation: &str,
    handler: Handler,
) -> Oop {
    unsafe {
        let p = instance.load(Ordering::Acquire);
        if p.is_null() {
            return abi::fault_reference(root);
        }
        let owner = &*p;
        let h = &owner.context;
        if h.thread != std::thread::current().id() || owner.receiver != c {
            return h.id.invalid;
        }
        let q = h.scope();
        let result = (|| -> Result<Oop, HostError> {
            let args =
                borrowed_slice(a, n).ok_or_else(|| HostError("invalid Host arguments".into()))?;
            q.keep(r)?;
            for v in args {
                q.keep(*v)?;
            }
            match handler(h, operation, args) {
                Ok(v) => Ok(v),
                Err(e) => {
                    let kind = q.keep(
                        h.intern(b"hostError")
                            .map_err(|e| native_error("Host error kind", e))?,
                    )?;
                    let text = q.string(e.0.as_bytes())?;
                    q.call(h.named(b"Error")?, b"kind:message:", &[kind, text])
                }
            }
        })();
        match result {
            Ok(v) => q.finish(v).unwrap_or(h.id.invalid),
            Err(_) => h.id.invalid,
        }
    }
}
/// The initializer receives Namespace, prefix, lib, workspace, guides and Id path.
pub unsafe fn initialize(
    p: *const Protocol,
    image: Oop,
    l: *const Oop,
    n: usize,
    a: *const Oop,
    count: usize,
    instance: &AtomicPtr<Owner>,
    busy: &AtomicBool,
    methods: &[&str],
) -> Oop {
    unsafe {
        let id = *p;
        let invalid = id.invalid;
        let result = (|| -> Result<Oop, HostError> {
            let symbols = borrowed_slice(l, n)
                .filter(|s| s.len() == methods.len() + 3 + abi::byte_transfer::NAMES.len())
                .ok_or_else(|| HostError("invalid Host literals".into()))?;
            let [ns, prefix, lib, workspace, guides, id_path] = borrowed_slice(a, count)
                .ok_or_else(|| HostError("invalid Host initialization".into()))?
            else {
                return Err(HostError("Host resources missing".into()));
            };
            if busy.swap(true, Ordering::AcqRel) {
                return Err(HostError("Host initialization reentered".into()));
            }
            struct Initializing<'a>(&'a AtomicBool);
            impl Drop for Initializing<'_> {
                fn drop(&mut self) {
                    self.0.store(false, Ordering::Release);
                }
            }
            let _initializing = Initializing(busy);
            let prior = instance.load(Ordering::Acquire);
            if !prior.is_null()
                && ((*prior).context.id.root != id.root
                    || (*prior).context.thread != std::thread::current().id())
            {
                return Err(HostError("Host runtime mismatch".into()));
            }
            let mut h = Context::from_namespace_unprepared(id, *ns, Resources::default(), symbols[0])?;
            h.install_byte_reader(image)?;
            let q = h.scope();
            for value in [prefix, lib, workspace, guides, id_path] {
                q.keep(*value)?;
            }
            let path = |value: Oop| -> Result<PathBuf, HostError> {
                Ok(PathBuf::from(OsString::from_vec(q.string_bytes(value)?)))
            };
            let resources = Resources {
                prefix: path(*prefix)?,
                lib_dir: path(*lib)?,
                rust_workspace_dir: path(*workspace)?,
                guide_dir: path(*guides)?,
                id_dylib: path(*id_path)?,
            };
            drop(q);
            h.resources = resources;
            if !prior.is_null() {
                let o = &*prior;
                return if o.ready.get()
                    && o.context.namespace == *ns
                    && o.context.resources == h.resources
                {
                    Ok(o.receiver)
                } else {
                    Err(HostError("Host resources mismatch".into()))
                };
            }
            let q = h.scope();
            let receiver = q.call(h.named(b"Object")?, b"clone", &[])?;
            let behavior = q.call(id.root, b"ensurePrivateVtableFor:", &[receiver])?;
            h.send(h.roots, b"retain:", &[receiver])?;
            h.send(h.roots, b"retain:", &[behavior])?;
            drop(q);
            let owner = Box::leak(Box::new(Owner {
                context: h,
                receiver,
                ready: Cell::new(false),
            }));
            instance.store(owner, Ordering::Release);
            let h = &owner.context;
            let q = h.scope();
            for i in 0..methods.len() {
                let method = q.call(image, b"newMethodAt:", &[abi::size(i).unwrap()])?;
                let method = q.call(method, b"withContext:", &[receiver])?;
                h.send(behavior, b"addMethod", &[symbols[i + 2], method])?;
            }
            h.send(*ns, b"at:put:", &[symbols[1], receiver])?;
            let receiver = q.finish(receiver)?;
            owner.ready.set(true);
            Ok(receiver)
        })();
        result.unwrap_or(invalid)
    }
}
// Native declarations are immutable loader metadata. The local Rust helper is
// shared by Host owners; no native pointer is passed through an Id message.
#[macro_export]
macro_rules! host_owner {
 ($name:literal,$dispatch:path,$(($entry:ident,$selector:literal)),+$(,)?)=>{
  static INSTANCE:std::sync::atomic::AtomicPtr<$crate::native::Owner>=std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
  static BUSY:std::sync::atomic::AtomicBool=std::sync::atomic::AtomicBool::new(false);
  $(unsafe extern "C" fn $entry(root:birnam_abi::Oop,c:birnam_abi::Oop,r:birnam_abi::Oop,a:*const birnam_abi::Oop,n:usize)->birnam_abi::Oop{
   unsafe{$crate::native::invoke(&INSTANCE,root,c,r,a,n,$selector,$dispatch)}
  })+
  static LITERALS:&[birnam_abi::id_protocol::Literal]=&{
   const OWN:&[&[u8]]=&[b"nameWithBytes",$name,$($selector.as_bytes()),+];
   let mut a=[const {birnam_abi::id_protocol::Literal::new(b"")};OWN.len()+birnam_abi::byte_transfer::NAMES.len()+1];
   let mut i=0;while i<OWN.len(){a[i]=birnam_abi::id_protocol::Literal::new(OWN[i]);i+=1;}
   let mut j=0;while j<birnam_abi::byte_transfer::NAMES.len(){a[i+j]=birnam_abi::id_protocol::Literal::new(birnam_abi::byte_transfer::NAMES[j]);j+=1;}a[i+j]=birnam_abi::id_protocol::Literal::new(b"traceHostRoots");a
  };
  static METHODS:&[birnam_abi::id_protocol::MethodSpec]=&{
   const ENTRIES:&[birnam_abi::id_protocol::Method]=&[$($entry as birnam_abi::id_protocol::Method),+];
   let mut specs=[const {birnam_abi::id_protocol::MethodSpec{name:0,entry:None}};ENTRIES.len()+2];
   let mut i=0;while i<ENTRIES.len(){specs[i]=birnam_abi::id_protocol::MethodSpec{name:i+2,entry:Some(ENTRIES[i])};i+=1;}specs[i]=birnam_abi::id_protocol::MethodSpec{name:i+2,entry:Some(birnam_abi::byte_transfer::receive_entry)};specs[i+1]=birnam_abi::id_protocol::MethodSpec{name:i+2+birnam_abi::byte_transfer::NAMES.len(),entry:Some($crate::native::trace_roots)};specs
  };
  unsafe extern "C" fn initialize(p:*const birnam_abi::id_protocol::Protocol,image:birnam_abi::Oop,l:*const birnam_abi::Oop,n:usize,a:*const birnam_abi::Oop,count:usize)->birnam_abi::Oop{
   unsafe{$crate::native::initialize(p,image,l,n,a,count,&INSTANCE,&BUSY,&[$($selector),+])}
  }
  #[unsafe(no_mangle)]pub static birnam_native_module_v2:birnam_abi::id_protocol::Module=birnam_abi::id_protocol::Module{version:2,byte_size:size_of::<birnam_abi::id_protocol::Module>() as u32,literals:LITERALS.as_ptr(),literal_count:LITERALS.len(),methods:METHODS.as_ptr(),method_count:METHODS.len(),initialize:Some(initialize)};
 };
}
pub fn argument(q: &Scope<'_>, args: &[Oop], i: usize) -> Result<Oop, HostError> {
    q.keep(
        *args
            .get(i)
            .ok_or_else(|| HostError("missing Host argument".into()))?,
    )
}
pub fn bytes(q: &Scope<'_>, args: &[Oop], i: usize) -> Result<Vec<u8>, HostError> {
    q.string_bytes(argument(q, args, i)?)
}
pub fn path(q: &Scope<'_>, args: &[Oop], i: usize) -> Result<PathBuf, HostError> {
    Ok(PathBuf::from(OsString::from_vec(bytes(q, args, i)?)))
}
pub fn text(q: &Scope<'_>, args: &[Oop], i: usize) -> Result<String, HostError> {
    String::from_utf8(bytes(q, args, i)?).map_err(|e| HostError(e.to_string()))
}
pub fn boolean(q: &Scope<'_>, args: &[Oop], i: usize) -> Result<bool, HostError> {
    let o = argument(q, args, i)?;
    if o == q.host.named(b"true")? {
        Ok(true)
    } else if o == q.host.named(b"false")? {
        Ok(false)
    } else {
        Err(HostError("expected Boolean".into()))
    }
}
pub fn word(q: &Scope<'_>, args: &[Oop], i: usize) -> Result<usize, HostError> {
    let o = argument(q, args, i)?;
    q.exact(o, b"IntegerVt")?;
    q.word_bits(q.call(q.host.named(b"Integer")?, b"wordOf:", &[o])?)
}
pub fn argv(q: &Scope<'_>, args: &[Oop], i: usize) -> Result<Vec<OsString>, HostError> {
    let a = argument(q, args, i)?;
    (0..q.vector_size(a)?)
        .map(|j| Ok(OsString::from_vec(q.string_bytes(q.vector_at(a, j)?)?)))
        .collect()
}
pub fn encode<T: serde::Serialize>(q: &Scope<'_>, value: &T) -> Result<Oop, HostError> {
    let bytes = serde_json::to_vec(value).map_err(|e| HostError(e.to_string()))?;
    Ok(q.string(&bytes)?)
}
pub fn decode<T: serde::de::DeserializeOwned>(
    q: &Scope<'_>,
    args: &[Oop],
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
