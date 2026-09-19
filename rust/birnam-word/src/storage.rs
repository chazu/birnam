//! Word-owned canonical representation and native primitive operations.
use birnam_abi::word::{WordApiV1, WordStorageV1};
use birnam_abi::{IdApi, MarkFn, Oop};
use core::cell::UnsafeCell;
use core::ffi::{c_int, c_void};
use core::ptr;

#[repr(C)]
struct RawWord {
    bits: u64,
}
const _: () = {
    assert!(size_of::<RawWord>() == 8);
    assert!(align_of::<RawWord>() == 8);
};
unsafe extern "C" fn initialize(payload: *mut u8, bits: u64) {
    // Native object storage meets payload_alignment. Unaligned access also
    // permits callers to prepare owned byte recipes without a typed alias.
    unsafe { payload.cast::<RawWord>().write_unaligned(RawWord { bits }) };
}
unsafe extern "C" fn read_payload(payload: *const u8) -> u64 {
    unsafe { payload.cast::<RawWord>().read_unaligned().bits }
}
pub static STORAGE: WordStorageV1 = WordStorageV1 {
    version: 1,
    byte_size: size_of::<WordStorageV1>(),
    payload_size: size_of::<RawWord>(),
    payload_alignment: align_of::<RawWord>(),
    initialize,
    read: read_payload,
};

#[derive(Clone, Copy, Default)]
struct Roots {
    behavior: Oop,
    prototype: Oop,
}

/// One mounting. Tests use the same implementation with their explicit Id
/// adapter. The opaque context does not contain a Runtime/Heap pointer.
pub struct Instance {
    api: *mut IdApi,
    roots: UnsafeCell<Roots>,
    operations: UnsafeCell<WordApiV1>,
}

impl Instance {
    /// Allocate native state before registering its partially initialized roots.
    /// # Safety
    /// The API lives until this instance and all installed consumers are gone.
    pub unsafe fn empty(api: *mut IdApi) -> Box<Self> {
        assert!(!api.is_null());
        let result = Box::new(Self {
            api,
            roots: UnsafeCell::new(Roots::default()),
            operations: UnsafeCell::new(WordApiV1 {
                version: 1,
                byte_size: size_of::<WordApiV1>(),
                context: ptr::null_mut(),
                behavior: ptr::null_mut(),
                prototype: ptr::null_mut(),
                make,
                read,
                is_exact,
                storage: &STORAGE,
            }),
        });
        unsafe {
            (*result.operations.get()).context = ptr::from_ref(result.as_ref()).cast_mut().cast()
        };
        result
    }

    /// The caller roots this partially initialized instance before entering.
    /// No state borrow is retained across an Id call.
    /// # Safety
    /// Owner-thread initialization, exactly once on the selected live Id.
    pub unsafe fn prepare(&self) {
        let api = self.api;
        let (proto, alloc, layout, object_vt) = unsafe {
            (
                (*api).proto,
                (*api).alloc,
                (*api).setLayout,
                (*api).object_vt,
            )
        };
        let behavior = unsafe { proto(object_vt) };
        assert!(!behavior.is_null());
        unsafe {
            (*self.roots.get()).behavior = behavior;
        }
        unsafe { layout(behavior, ptr::null(), 0, birnam_abi::SLOT_RAW) };
        let prototype = unsafe { alloc(behavior, STORAGE.payload_size) };
        assert!(!prototype.is_null());
        unsafe { initialize(prototype.cast(), 0) };
        unsafe {
            (*self.roots.get()).prototype = prototype;
        }
        unsafe {
            (*self.operations.get()).behavior = behavior;
            (*self.operations.get()).prototype = prototype;
        }
    }

    /// Publish only after the loader has attached the primitive Word dependency.
    /// # Safety
    /// Owner-thread activation, after preparation, exactly once.
    pub unsafe fn publish(&self) {
        let api = self.api;
        let [behavior, prototype] = self.identities();
        unsafe {
            birnam_abi::set_vtable_owner(&*api, behavior, prototype);
            ((*api).exportObj)(c"Word".as_ptr(), prototype);
            ((*api).exportObj)(c"WordVt".as_ptr(), behavior);
        }
    }

    pub fn operations(&self) -> *mut WordApiV1 {
        self.operations.get()
    }
    pub fn api(&self) -> *mut IdApi {
        self.api
    }
    pub fn identities(&self) -> [Oop; 2] {
        let roots = unsafe { *self.roots.get() };
        [roots.behavior, roots.prototype]
    }
    /// # Safety
    /// Called only from this runtime's marking phase. MarkFn does not collect.
    pub unsafe fn trace(&self, mark: MarkFn) {
        for object in self.identities() {
            if !object.is_null() {
                unsafe { mark(object) };
            }
        }
    }
}

unsafe extern "C" fn make(context: *mut c_void, value: u64) -> Oop {
    // Copy context data before the allocator can invoke arbitrary native code.
    let instance = context.cast::<Instance>();
    let (api, behavior) = unsafe { ((*instance).api, (*(*instance).roots.get()).behavior) };
    let allocate = unsafe { (*api).alloc };
    let object = unsafe { allocate(behavior, STORAGE.payload_size) };
    assert!(!object.is_null());
    unsafe { initialize(object.cast(), value) };
    object
}
unsafe extern "C" fn read(_: *mut c_void, object: Oop) -> u64 {
    // Same unsafe, callback-free read contract as the previous native accessor.
    unsafe { read_payload(object.cast()) }
}
unsafe extern "C" fn is_exact(context: *mut c_void, object: Oop) -> c_int {
    let instance = context.cast::<Instance>();
    let (api, behavior) = unsafe { ((*instance).api, (*(*instance).roots.get()).behavior) };
    let behavior_of = unsafe { (*api).vtableOf };
    c_int::from(unsafe { behavior_of(object) } == behavior)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_codec_preserves_payloads_without_touching_adjacent_bytes() {
        let mut seed = 0x0123_4567_89ab_cdefu64;
        for _ in 0..1024 {
            let mut bytes = [0xabu8; 10];
            unsafe { (STORAGE.initialize)(bytes.as_mut_ptr().add(1), seed) };
            assert_eq!(&bytes[1..9], &seed.to_ne_bytes());
            assert_eq!(unsafe { (STORAGE.read)(bytes.as_ptr().add(1)) }, seed);
            assert_eq!((bytes[0], bytes[9]), (0xab, 0xab));
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        }
    }
}
