//! Opaque per-runtime raw namespace storage (81AW), independent of Id objects.
use crate::Oop;
use core::ffi::{c_int, c_void};
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BindingV1 {
    pub name: Oop,
    pub value: Oop,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ViewV1 {
    pub bindings: *const BindingV1,
    pub count: usize,
}
/// Restricted primitive: intern the borrowed bytes in the supplied runtime.
/// Null means embedded NUL; no sends, collection, reentry or retained borrows.
pub type InternV1 = unsafe extern "C" fn(*mut c_void, *const u8, usize) -> Oop;
pub type FindV1 = unsafe extern "C" fn(*const c_void, Oop) -> Oop;
pub type BindV1 = unsafe extern "C" fn(*mut c_void, Oop, Oop) -> Oop;
/// Restricted primitive: update the existing weak dictionary mirror, if present.
/// No sends, collection, reentry or retained borrows.
pub type MirrorV1 = unsafe extern "C" fn(*mut c_void, Oop, Oop);
/// Intern once, bind, then mirror the original Symbol/value pair. Zero leaves
/// output untouched; one writes the selected bind's return value (possibly null).
pub type PublishV1 = unsafe extern "C" fn(
    *mut c_void,
    BindV1,
    *mut c_void,
    InternV1,
    MirrorV1,
    *const u8,
    usize,
    Oop,
    *mut Oop,
) -> c_int;
/// Intern once, then use the selected raw find. Zero leaves output untouched;
/// one writes the binding, which may itself be null. All pointers are borrowed.
pub type LookupV1 = unsafe extern "C" fn(
    *const c_void,
    FindV1,
    *mut c_void,
    InternV1,
    *const u8,
    usize,
    *mut Oop,
) -> c_int;

pub type CreatePresenceV1 = unsafe extern "C" fn(
    *const c_void,
    Option<FindV1>,
    Option<LookupV1>,
    *const crate::namespace_presence::FloorV1,
) -> *mut crate::namespace_presence::ApiV1;

pub type CreateImportV1 = unsafe extern "C" fn(
    *const c_void, Option<FindV1>, Option<LookupV1>,
    *const crate::namespace_import::FloorV1,
) -> *mut crate::namespace_import::ApiV1;

pub type CreateExportV1 = unsafe extern "C" fn(
    *mut c_void, Option<BindV1>, Option<PublishV1>,
    *const crate::namespace_export::FloorV1,
) -> *mut crate::namespace_export::ApiV1;

pub type CreateEnumerationV1 = unsafe extern "C" fn(
    *const c_void,
    Option<unsafe extern "C" fn(*const c_void) -> usize>,
    Option<unsafe extern "C" fn(*const c_void, usize) -> Oop>,
    *const crate::namespace_enumeration::FloorV1,
) -> *mut crate::namespace_enumeration::ApiV1;

/// Immutable descriptor and code stay pinned until all contexts are destroyed.
/// Raw storage operations are owner-serialized, with no Id calls, GC, callbacks
/// or reentry. create/bind may allocate native storage; reads do not mutate.
/// lookup/publish call only the supplied restricted interner, selected storage
/// operation and (for publish) weak mirror. Service operations retain their
/// separately documented floor access contracts.
/// Oops are opaque identities, never dereferenced. View data is borrowed until
/// the next bind/destroy and must not escape its synchronous use by the caller.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV8 {
    pub version: u32,
    pub byte_size: usize,
    pub create: Option<unsafe extern "C" fn() -> *mut c_void>,
    pub destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    pub bind: Option<unsafe extern "C" fn(*mut c_void, Oop, Oop) -> Oop>,
    pub find: Option<unsafe extern "C" fn(*const c_void, Oop) -> Oop>,
    pub reverse: Option<unsafe extern "C" fn(*const c_void, Oop) -> Oop>,
    pub count: Option<unsafe extern "C" fn(*const c_void) -> usize>,
    pub name_at: Option<unsafe extern "C" fn(*const c_void, usize) -> Oop>,
    pub bindings: Option<unsafe extern "C" fn(*const c_void) -> ViewV1>,
    // Service creation only copies the access descriptor: no floor callbacks.
    pub create_names: Option<
        unsafe extern "C" fn(
            *mut c_void,
            Option<unsafe extern "C" fn(*const c_void, Oop) -> Oop>,
            *const crate::namespace_names::FloorV1,
        ) -> *mut crate::namespace_names::ApiV1,
    >,
    pub destroy_names: Option<unsafe extern "C" fn(*mut crate::namespace_names::ApiV1)>,
    pub create_enumeration: Option<CreateEnumerationV1>,
    pub destroy_enumeration: Option<unsafe extern "C" fn(*mut crate::namespace_enumeration::ApiV1)>,
    pub lookup: Option<LookupV1>,
    pub create_presence: Option<CreatePresenceV1>,
    pub destroy_presence: Option<unsafe extern "C" fn(*mut crate::namespace_presence::ApiV1)>,
    pub publish: Option<PublishV1>,
    pub create_import: Option<CreateImportV1>,
    pub destroy_import: Option<unsafe extern "C" fn(*mut crate::namespace_import::ApiV1)>,
    pub create_export: Option<CreateExportV1>,
    pub destroy_export: Option<unsafe extern "C" fn(*mut crate::namespace_export::ApiV1)>,
}
const _: () = {
    assert!(size_of::<ApiV8>() == 176);
    assert!(core::mem::offset_of!(ApiV8, create_export) == 160);
    assert!(core::mem::offset_of!(ApiV8, destroy_export) == 168);
    assert!(core::mem::offset_of!(ApiV8, create_import) == 144);
    assert!(core::mem::offset_of!(ApiV8, destroy_import) == 152);
    assert!(core::mem::offset_of!(ApiV8, publish) == 136);
    assert!(core::mem::offset_of!(ApiV8, create_presence) == 120);
    assert!(core::mem::offset_of!(ApiV8, destroy_presence) == 128);
    assert!(core::mem::offset_of!(ApiV8, create) == 16);
    assert!(core::mem::offset_of!(ApiV8, bindings) == 72);
    assert!(core::mem::offset_of!(ApiV8, lookup) == 112);
    assert!(size_of::<BindingV1>() == 16);
    assert!(size_of::<ViewV1>() == 16);
};
pub type ProviderV8 = unsafe extern "C" fn() -> *const ApiV8;
pub type DependencyV8 = unsafe extern "C" fn(*const ApiV8) -> c_int;
static DEPENDENCY: AtomicPtr<ApiV8> = AtomicPtr::new(ptr::null_mut());
/// # Safety
/// Readable aligned descriptor; valid immutable operations/code stay pinned.
pub unsafe fn valid(pointer: *const ApiV8) -> bool {
    if pointer.is_null() {
        return false;
    }
    let a = unsafe { *pointer };
    a.version == 8
        && a.byte_size == size_of::<ApiV8>()
        && a.create.is_some()
        && a.destroy.is_some()
        && a.bind.is_some()
        && a.find.is_some()
        && a.reverse.is_some()
        && a.count.is_some()
        && a.name_at.is_some()
        && a.bindings.is_some()
        && a.create_names.is_some()
        && a.destroy_names.is_some()
        && a.create_enumeration.is_some()
        && a.destroy_enumeration.is_some()
        && a.lookup.is_some()
        && a.create_presence.is_some()
        && a.destroy_presence.is_some()
        && a.publish.is_some()
        && a.create_import.is_some()
        && a.destroy_import.is_some()
        && a.create_export.is_some()
        && a.destroy_export.is_some()
}
/// # Safety
/// Owner-thread admission before constructing any namespace. Descriptor and code
/// are immutable and process-pinned. Different descriptors cannot retarget it.
pub unsafe fn attach(pointer: *const ApiV8) -> c_int {
    if !unsafe { valid(pointer) } {
        return 0;
    }
    match DEPENDENCY.compare_exchange(
        ptr::null_mut(),
        pointer.cast_mut(),
        Ordering::AcqRel,
        Ordering::Acquire,
    ) {
        Ok(_) => 1,
        Err(existing) => c_int::from(ptr::eq(existing, pointer)),
    }
}
/// # Safety
/// Use only after the declared dependency is prepared by the host.
pub unsafe fn operations() -> ApiV8 {
    let pointer = DEPENDENCY.load(Ordering::Acquire);
    assert!(
        !pointer.is_null(),
        "native Namespace dependency was not mounted"
    );
    unsafe { *pointer }
}
#[macro_export]
macro_rules! namespace_dependency {
    () => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn birnam_namespace_dependency_v8(
            api: *const $crate::namespace::ApiV8,
        ) -> core::ffi::c_int {
            unsafe { $crate::namespace::attach(api) }
        }
    };
}
