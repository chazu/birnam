//! Ordered native root-provider registration above Id (81BJ).
use crate::RootProvider;
use core::ffi::{c_int, c_void};
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

pub const CAPACITY: usize = 32;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FloorV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub admit: Option<unsafe extern "C" fn(*mut c_void)>,
    pub overflow: Option<unsafe extern "C" fn(*mut c_void) -> !>,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub register: Option<unsafe extern "C" fn(*mut c_void, RootProvider)>,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SnapshotV1 {
    pub count: usize,
    pub entries: [Option<RootProvider>; CAPACITY],
}
/// Complete native owner returned by the root registry's module initializer.
/// Its operations work before Runtime/GC. Adopt once; destroy through factory.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct OwnedV1 {
    pub version: u32,
    pub byte_size: usize,
    pub api: *mut ApiV1,
    pub factory: *const FactoryV1,
    pub context: *mut c_void,
    pub retain: Option<unsafe extern "C" fn(*mut c_void, crate::Oop) -> usize>,
    pub release: Option<unsafe extern "C" fn(*mut c_void, usize) -> c_int>,
    pub roots: Option<unsafe extern "C" fn(*mut c_void) -> RootsV1>,
    pub set_admission: Option<unsafe extern "C" fn(*mut c_void, *const FloorV1) -> c_int>,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RootsV1 {
    pub entries: *const crate::Oop,
    pub count: usize,
}
const _: () = assert!(size_of::<OwnedV1>() == 72 && size_of::<RootsV1>() == 16);
/// # Safety
/// A readable descriptor returned by the native initializer, with live code.
pub unsafe fn valid_owned(pointer: *const OwnedV1) -> bool {
    if pointer.is_null() { return false; }
    let owned = unsafe { *pointer };
    owned.version == 1 && owned.byte_size == size_of::<OwnedV1>()
        && !owned.context.is_null() && unsafe { valid(owned.api) && valid_factory(owned.factory) }
        && owned.retain.is_some() && owned.release.is_some() && owned.roots.is_some() && owned.set_admission.is_some()
}
pub type AppendV1 = unsafe extern "C" fn(*mut ApiV1, RootProvider) -> c_int;
pub type SnapshotFnV1 = unsafe extern "C" fn(*const ApiV1) -> SnapshotV1;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FactoryV1 {
    pub version: u32,
    pub byte_size: usize,
    pub create: Option<unsafe extern "C" fn(*const FloorV1) -> *mut ApiV1>,
    pub destroy: Option<unsafe extern "C" fn(*mut ApiV1)>,
    pub append: Option<AppendV1>,
    pub snapshot: Option<SnapshotFnV1>,
}
const _: () = {
    assert!(size_of::<FloorV1>() == 40 && core::mem::offset_of!(FloorV1, overflow) == 32);
    assert!(size_of::<ApiV1>() == 32 && core::mem::offset_of!(ApiV1, register) == 24);
    assert!(size_of::<FactoryV1>() == 48 && core::mem::offset_of!(FactoryV1, snapshot) == 40);
    assert!(size_of::<SnapshotV1>() == 264 && core::mem::offset_of!(SnapshotV1, entries) == 8);
};
pub type FactoryProviderV1 = unsafe extern "C" fn() -> *const FactoryV1;
pub type DependencyV1 = unsafe extern "C" fn(*const FactoryV1) -> c_int;
pub type ProviderV1 = unsafe extern "C" fn() -> *mut ApiV1;
/// # Safety
/// Readable descriptor; validation invokes no operations.
pub unsafe fn valid(pointer: *const ApiV1) -> bool {
    if pointer.is_null() { return false; }
    let api = unsafe { *pointer };
    api.version == 1 && api.byte_size == size_of::<ApiV1>() && api.register.is_some()
}
/// # Safety
/// Owner-thread access. The live selected operation/context and registered
/// provider's native code outlive the registry and its invocation snapshots.
pub unsafe fn register(pointer: *const ApiV1, provider: RootProvider) {
    let api = unsafe { *pointer };
    unsafe { api.register.unwrap()(api.context, provider) };
}
/// # Safety
/// Readable immutable descriptor; validation invokes no operations.
pub unsafe fn valid_factory(pointer: *const FactoryV1) -> bool {
    if pointer.is_null() { return false; }
    let factory = unsafe { *pointer };
    factory.version == 1 && factory.byte_size == size_of::<FactoryV1>()
        && factory.create.is_some() && factory.destroy.is_some()
        && factory.append.is_some() && factory.snapshot.is_some()
}
static FACTORY: AtomicPtr<FactoryV1> = AtomicPtr::new(ptr::null_mut());
/// # Safety
/// Owner-thread preparation before Id initialization; factory/code stay pinned.
pub unsafe fn attach(pointer: *const FactoryV1) -> c_int {
    if !unsafe { valid_factory(pointer) } { return 0; }
    match FACTORY.compare_exchange(ptr::null_mut(), pointer.cast_mut(), Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => 1,
        Err(existing) => c_int::from(ptr::eq(existing, pointer)),
    }
}
/// # Safety
/// Call after the host attaches the declared factory dependency.
pub unsafe fn operations() -> FactoryV1 {
    let pointer = FACTORY.load(Ordering::Acquire);
    assert!(!pointer.is_null(), "native root registry factory was not mounted");
    unsafe { *pointer }
}

#[cfg(test)]
mod tests {
    use super::*;
    unsafe extern "C" fn create(_: *const FloorV1) -> *mut ApiV1 { panic!("admission invoked creation"); }
    unsafe extern "C" fn destroy(_: *mut ApiV1) { panic!("admission invoked destruction"); }
    unsafe extern "C" fn append(_: *mut ApiV1, _: RootProvider) -> c_int { panic!("admission invoked append"); }
    unsafe extern "C" fn snapshot(_: *const ApiV1) -> SnapshotV1 { panic!("admission invoked snapshot"); }
    static FIRST: FactoryV1 = FactoryV1 { version:1, byte_size:size_of::<FactoryV1>(), create:Some(create), destroy:Some(destroy), append:Some(append), snapshot:Some(snapshot) };
    static SECOND: FactoryV1 = FactoryV1 { version:1, byte_size:size_of::<FactoryV1>(), create:Some(create), destroy:Some(destroy), append:Some(append), snapshot:Some(snapshot) };
    #[test]
    fn factory_admission_is_inert_repeats_same_pointer_and_rejects_retargeting() {
        unsafe {
            assert_eq!(attach(ptr::null()), 0);
            let mut invalid = FIRST;
            invalid.version=2; assert_eq!(attach(&invalid),0);
            invalid=FIRST; invalid.byte_size-=1; assert_eq!(attach(&invalid),0);
            invalid=FIRST; invalid.create=None; assert_eq!(attach(&invalid),0);
            invalid=FIRST; invalid.destroy=None; assert_eq!(attach(&invalid),0);
            invalid=FIRST; invalid.append=None; assert_eq!(attach(&invalid),0);
            invalid=FIRST; invalid.snapshot=None; assert_eq!(attach(&invalid),0);
            assert_eq!(attach(&FIRST),1); assert_eq!(attach(&FIRST),1);
            assert_eq!(attach(&SECOND),0);
        }
    }
}

/// Temporary roots for synchronous native construction above the root owner.
/// The completed owner and its code outlive the scope and every retained object.
pub struct ConstructionRoots { owner: OwnedV1, tokens: Vec<usize> }
impl ConstructionRoots {
    /// # Safety
    /// The readable, completed owner remains live until this scope is dropped.
    pub unsafe fn new(owner: *const OwnedV1) -> Self {
        assert!(unsafe { valid_owned(owner) });
        Self { owner: unsafe { *owner }, tokens: Vec::new() }
    }
    /// # Safety
    /// The object is live on the owner thread when admitted to the root set.
    pub unsafe fn keep(&mut self, object: crate::Oop) -> crate::Oop {
        if !object.is_null() {
            let token = unsafe { self.owner.retain.unwrap()(self.owner.context, object) };
            assert_ne!(token, 0);
            self.tokens.push(token);
        }
        object
    }
}
impl Drop for ConstructionRoots {
    fn drop(&mut self) {
        for token in self.tokens.drain(..) {
            assert_eq!(unsafe { self.owner.release.unwrap()(self.owner.context, token) }, 1);
        }
    }
}
