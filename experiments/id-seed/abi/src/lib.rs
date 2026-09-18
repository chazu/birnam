//! Experimental native carrier, not the installed Birnam ABI.
//! Oops are stable payload addresses. Word/Bytes/Method are explicit native
//! operands, not boxed language values. All borrowed operands last for a call.
//!
//! Trusted native callers must supply valid, aligned pointers; argument storage
//! and output storage must not overlap. A native method must not unwind. Oops
//! belong to exactly one live seed. Only a caller's declared payload may be
//! accessed directly, with at most usize alignment; seed headers/state are
//! opaque. This carrier is not a boundary for untrusted machine code.
use core::ffi::c_void;

pub type Oop = usize;
pub type Status = u32;
pub const OK: Status = 0;
pub const INVALID: Status = 1;
pub const UNBOUND: Status = 2;
pub const RECURSION: Status = 3;
pub const VERSION: u32 = 1;
pub const NONE: u32 = 0;
pub const OBJECT: u32 = 1;
pub const WORD: u32 = 2;
pub const BYTES: u32 = 3;
pub const METHOD: u32 = 4;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Value {
    pub kind: u32,
    pub reserved: u32,
    pub a: usize,
    pub b: usize,
}
impl Value {
    pub const fn none() -> Self {
        Self {
            kind: NONE,
            reserved: 0,
            a: 0,
            b: 0,
        }
    }
    pub const fn object(oop: Oop) -> Self {
        Self {
            kind: OBJECT,
            reserved: 0,
            a: oop,
            b: 0,
        }
    }
    pub const fn word(word: usize) -> Self {
        Self {
            kind: WORD,
            reserved: 0,
            a: word,
            b: 0,
        }
    }
    pub fn bytes(bytes: &[u8]) -> Self {
        Self {
            kind: BYTES,
            reserved: 0,
            a: bytes.as_ptr() as usize,
            b: bytes.len(),
        }
    }
    pub fn method(method: Method) -> Self {
        Self {
            kind: METHOD,
            reserved: 0,
            a: method.entry as usize,
            b: method.data as usize,
        }
    }
    /// # Safety
    /// METHOD operands must originate from a live native entry and data owner.
    pub unsafe fn as_method(self) -> Option<Method> {
        if self.kind != METHOD || self.a == 0 || self.reserved != 0 {
            return None;
        }
        Some(Method {
            entry: unsafe { core::mem::transmute::<usize, Entry>(self.a) },
            data: self.b as *mut c_void,
        })
    }
}

pub type Entry = unsafe extern "C" fn(
    *const SeedApi,
    *mut c_void,
    Oop,
    *const Value,
    usize,
    *mut Value,
) -> Status;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Method {
    pub entry: Entry,
    pub data: *mut c_void,
}
impl Method {
    pub fn plain(entry: Entry) -> Self {
        Self {
            entry,
            data: core::ptr::null_mut(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Roots {
    pub object: Oop,
    pub vtable: Oop,
    pub symbol: Oop,
    pub symbol_prototype: Oop,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Selectors {
    pub lookup: Oop,
    pub add_method: Oop,
    pub remove_method: Oop,
    pub allocate: Oop,
    pub delegated: Oop,
    pub intern: Oop,
    pub parent: Oop,
    pub set_parent: Oop,
}

/// The consumer sees no Rust Seed, heap, vtable, or interner state.
/// Entries are owner-thread-only; API/data/code remain live until destruction.
/// Reassociation is a raw object-model operation, not ordinary allocation.
#[repr(C)]
pub struct SeedApi {
    pub version: u32,
    pub size: u32,
    pub context: *mut c_void,
    pub roots: Roots,
    pub selectors: Selectors,
    pub send:
        unsafe extern "C" fn(*const SeedApi, Oop, Oop, *const Value, usize, *mut Value) -> Status,
    pub behavior_of: unsafe extern "C" fn(*const SeedApi, Oop) -> Oop,
    pub reassociate: unsafe extern "C" fn(*const SeedApi, Oop, Oop) -> Status,
}
pub type SeedCreate = unsafe extern "C" fn() -> *mut SeedApi;
/// Destruction is allowed only with no active calls and no future consumer use.
/// Method data owners stay alive until AFTER the seed is destroyed.
pub type SeedDestroy = unsafe extern "C" fn(*mut SeedApi) -> Status;

#[repr(C)]
pub struct LibraryApi {
    pub version: u32,
    pub size: u32,
    pub context: *mut c_void,
    pub ensure_private: Oop,
    pub created_count: unsafe extern "C" fn(*const LibraryApi) -> usize,
}
pub type LibraryCreate = unsafe extern "C" fn(*const SeedApi) -> *mut LibraryApi;
/// Destroy the seed first: its dictionary contains a method referring here.
pub type LibraryDestroy = unsafe extern "C" fn(*mut LibraryApi);

/// # Safety
/// API and operands must obey their documented call lifetimes.
pub unsafe fn call(
    api: *const SeedApi,
    receiver: Oop,
    selector: Oop,
    args: &[Value],
) -> Result<Value, Status> {
    let mut answer = Value::none();
    let status = unsafe {
        ((*api).send)(
            api,
            receiver,
            selector,
            args.as_ptr(),
            args.len(),
            &mut answer,
        )
    };
    if status == OK {
        Ok(answer)
    } else {
        Err(status)
    }
}
/// # Safety
/// API must be live on its owner thread.
pub unsafe fn intern(api: *const SeedApi, name: &[u8]) -> Result<Oop, Status> {
    let result = unsafe {
        call(
            api,
            (*api).roots.symbol_prototype,
            (*api).selectors.intern,
            &[Value::bytes(name)],
        )
    }?;
    if result.kind == OBJECT {
        Ok(result.a)
    } else {
        Err(INVALID)
    }
}
