//! Selected-floor export enumeration (81AY); no IdApi fields or global fallback.
use crate::Oop;
use core::cell::UnsafeCell;
use core::ffi::c_void;
use core::ptr;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FloorV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub preflight: Option<unsafe extern "C" fn(*mut c_void)>,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CountV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub count: Option<unsafe extern "C" fn(*mut c_void) -> usize>,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NameAtV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub name_at: Option<unsafe extern "C" fn(*mut c_void, usize) -> Oop>,
}
#[repr(C)]
pub struct ApiV1 {
    pub count: CountV1,
    pub name_at: NameAtV1,
}
/// A floor-owned data export points to a Namespace-owned descriptor. Publication
/// happens once during owner-thread initialization, before any consumer can use
/// the floor. Selected descriptors/code/context stay pinned and immutable for
/// the complete enumeration; no concurrent access to publication is permitted.
#[repr(C)]
pub struct PublicationV1<T> {
    pub version: u32,
    pub byte_size: usize,
    pub entry: UnsafeCell<*const T>,
}
// SAFETY: the native contract serializes publication and access on the owner
// thread; this type only supplies a stable, process-pinned exported address.
unsafe impl<T> Sync for PublicationV1<T> {}
impl<T> Default for PublicationV1<T> {
    fn default() -> Self {
        Self::new()
    }
}
impl<T> PublicationV1<T> {
    pub const fn new() -> Self {
        Self {
            version: 1,
            byte_size: size_of::<Self>(),
            entry: UnsafeCell::new(ptr::null()),
        }
    }
    /// # Safety
    /// Single owner-thread publication before consumers; entry stays pinned.
    pub unsafe fn publish(&self, entry: *const T) {
        assert!(!entry.is_null() && unsafe { *self.entry.get() }.is_null());
        unsafe { *self.entry.get() = entry };
    }
}
unsafe fn resolve<T: Copy>(pointer: *const PublicationV1<T>) -> Option<T> {
    if pointer.is_null() {
        return None;
    }
    let publication = unsafe { &*pointer };
    if publication.version != 1 || publication.byte_size != size_of::<PublicationV1<T>>() {
        return None;
    }
    let entry = unsafe { *publication.entry.get() };
    if entry.is_null() {
        return None;
    }
    Some(unsafe { *entry })
}
/// # Safety
/// Readable publication/descriptor selected through the floor's live lookup;
/// owner-thread access and pinned native lifetime apply to the returned copy.
pub unsafe fn resolve_count(pointer: *const PublicationV1<CountV1>) -> Option<CountV1> {
    let entry = unsafe { resolve(pointer) }?;
    (entry.version == 1
        && entry.byte_size == size_of::<CountV1>()
        && !entry.context.is_null()
        && entry.count.is_some())
    .then_some(entry)
}
/// # Safety
/// Same selection, owner-thread and lifetime contract as resolve_count.
pub unsafe fn resolve_name_at(pointer: *const PublicationV1<NameAtV1>) -> Option<NameAtV1> {
    let entry = unsafe { resolve(pointer) }?;
    (entry.version == 1
        && entry.byte_size == size_of::<NameAtV1>()
        && !entry.context.is_null()
        && entry.name_at.is_some())
    .then_some(entry)
}
const _: () = {
    assert!(size_of::<FloorV1>() == 32);
    assert!(size_of::<CountV1>() == 32);
    assert!(size_of::<NameAtV1>() == 32);
    assert!(size_of::<ApiV1>() == 64);
    assert!(size_of::<PublicationV1<CountV1>>() == 24);
    assert!(core::mem::offset_of!(PublicationV1<CountV1>, entry) == 16);
};

#[cfg(test)]
mod tests {
    use super::*;
    unsafe extern "C" fn count(context: *mut c_void) -> usize {
        context as usize
    }
    unsafe extern "C" fn name(context: *mut c_void, index: usize) -> Oop {
        ptr::without_provenance_mut(context as usize + index)
    }
    #[test]
    fn validates_publications_and_each_entry_without_invoking_operations() {
        let count_api = CountV1 {
            version: 1,
            byte_size: size_of::<CountV1>(),
            context: ptr::dangling_mut::<c_void>(),
            count: Some(count),
        };
        let name_api = NameAtV1 {
            version: 1,
            byte_size: size_of::<NameAtV1>(),
            context: ptr::dangling_mut::<c_void>(),
            name_at: Some(name),
        };
        let count_slot = PublicationV1::new();
        let name_slot = PublicationV1::new();
        unsafe {
            assert!(resolve_count(ptr::null()).is_none());
            assert!(resolve_name_at(ptr::null()).is_none());
            assert!(resolve_count(&count_slot).is_none());
            assert!(resolve_name_at(&name_slot).is_none());
            count_slot.publish(&count_api);
            name_slot.publish(&name_api);
            let selected = resolve_count(&count_slot).unwrap();
            let names = resolve_name_at(&name_slot).unwrap();
            assert_eq!(selected.count.unwrap()(selected.context), 1);
            assert_eq!(names.name_at.unwrap()(names.context, 4) as usize, 5);
            for invalid in [
                CountV1 {
                    version: 2,
                    ..count_api
                },
                CountV1 {
                    byte_size: 31,
                    ..count_api
                },
                CountV1 {
                    context: ptr::null_mut(),
                    ..count_api
                },
                CountV1 {
                    count: None,
                    ..count_api
                },
            ] {
                let slot = PublicationV1::new();
                slot.publish(&invalid);
                assert!(resolve_count(&slot).is_none());
            }
            for invalid in [
                NameAtV1 {
                    version: 2,
                    ..name_api
                },
                NameAtV1 {
                    byte_size: 31,
                    ..name_api
                },
                NameAtV1 {
                    context: ptr::null_mut(),
                    ..name_api
                },
                NameAtV1 {
                    name_at: None,
                    ..name_api
                },
            ] {
                let slot = PublicationV1::new();
                slot.publish(&invalid);
                assert!(resolve_name_at(&slot).is_none());
            }
            for (version, byte_size) in [(2, 24), (1, 23)] {
                let count_bad = PublicationV1 {
                    version,
                    byte_size,
                    entry: UnsafeCell::new(&count_api as *const _),
                };
                let name_bad = PublicationV1 {
                    version,
                    byte_size,
                    entry: UnsafeCell::new(&name_api as *const _),
                };
                assert!(
                    resolve_count(&count_bad).is_none() && resolve_name_at(&name_bad).is_none()
                );
            }
            // A later native lookup may select another publication; the prior
            // enumeration retains its exact operation/context snapshot.
            let other = CountV1 {
                context: ptr::without_provenance_mut(7),
                ..count_api
            };
            let another = PublicationV1::new();
            another.publish(&other);
            let current = resolve_count(&another).unwrap();
            assert_eq!(current.count.unwrap()(current.context), 7);
            assert_eq!(selected.count.unwrap()(selected.context), 1);
        }
    }
}
