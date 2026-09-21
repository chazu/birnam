//! Float-owned canonical representation and native primitive operations.
use birnam_abi::float::{FloatApiV1, FloatStorageV1};
use birnam_abi::{IdApi, MarkFn, Oop};
use core::cell::UnsafeCell;
use core::ffi::{c_int, c_void};
use core::ptr;

#[repr(C)]
struct RawFloat {
    bits: u64,
}
const _: () = {
    assert!(size_of::<RawFloat>() == 8);
    assert!(align_of::<RawFloat>() == 8);
};
unsafe extern "C" fn initialize(payload: *mut u8, bits: u64) {
    // Native object storage meets payload_alignment. Unaligned access also
    // permits callers to prepare owned byte recipes without a typed alias.
    unsafe {
        payload
            .cast::<RawFloat>()
            .write_unaligned(RawFloat { bits })
    };
}
unsafe extern "C" fn read_payload(payload: *const u8) -> u64 {
    unsafe { payload.cast::<RawFloat>().read_unaligned().bits }
}
pub static STORAGE: FloatStorageV1 = FloatStorageV1 {
    version: 1,
    byte_size: size_of::<FloatStorageV1>(),
    payload_size: size_of::<RawFloat>(),
    payload_alignment: align_of::<RawFloat>(),
    initialize_bits: initialize,
    read_bits: read_payload,
};

#[derive(Clone, Copy, Default)]
struct Roots {
    behavior: Oop,
    prototype: Oop,
}

#[derive(Clone, Copy)]
enum Construction {
    Native(crate::native::Access),
    Legacy {
        api: *mut IdApi,
        presence: *const birnam_abi::namespace_presence::ApiV1,
        imports: *const birnam_abi::namespace_import::ApiV1,
        exports: *const birnam_abi::namespace_export::ApiV1,
        layouts: *const birnam_abi::payload_layout::ApiV1,
    },
}

/// One mounting, with completed native inputs or a legacy IdApi adapter.
pub struct Instance {
    construction: Construction,
    roots: UnsafeCell<Roots>,
    operations: UnsafeCell<FloatApiV1>,
}

impl Instance {
    /// Allocate native state before registering its partially initialized roots.
    /// # Safety
    /// The API lives until this instance and all installed consumers are gone.
    pub unsafe fn empty(
        api: *mut IdApi,
        presence: *const birnam_abi::namespace_presence::ApiV1,
        imports: *const birnam_abi::namespace_import::ApiV1,
        exports: *const birnam_abi::namespace_export::ApiV1,
        layouts: *const birnam_abi::payload_layout::ApiV1,
    ) -> Box<Self> {
        assert!(!api.is_null());
        Self::new(Construction::Legacy {
            api,
            presence,
            imports,
            exports,
            layouts,
        })
    }
    pub(crate) fn native(access: crate::native::Access) -> Box<Self> {
        Self::new(Construction::Native(access))
    }
    fn new(construction: Construction) -> Box<Self> {
        let result = Box::new(Self {
            construction,
            roots: UnsafeCell::new(Roots::default()),
            operations: UnsafeCell::new(FloatApiV1 {
                version: 1,
                byte_size: size_of::<FloatApiV1>(),
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
        let Construction::Legacy { api, layouts, .. } = self.construction else {
            unreachable!()
        };
        let (proto, alloc, layout, object_vt) =
            unsafe { ((*api).proto, (*api).alloc, *layouts, (*api).object_vt) };
        let behavior = unsafe { proto(object_vt) };
        assert!(!behavior.is_null());
        unsafe {
            (*self.roots.get()).behavior = behavior;
        }
        unsafe {
            layout.set.unwrap()(
                layout.context,
                behavior,
                ptr::null(),
                0,
                birnam_abi::SLOT_RAW,
            )
        };
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

    /// Publish only after the loader has attached both numeric dependencies.
    /// # Safety
    /// Owner-thread activation, after preparation, exactly once.
    pub unsafe fn publish(&self) {
        let Construction::Legacy { api, exports, .. } = self.construction else {
            unreachable!()
        };
        let [behavior, prototype] = self.identities();
        unsafe {
            birnam_abi::set_vtable_owner(&*api, behavior, prototype);
            birnam_abi::namespace_export::export(exports, c"Float".as_ptr(), prototype);
            birnam_abi::namespace_export::export(exports, c"FloatVt".as_ptr(), behavior);
        }
    }

    pub(crate) fn native_access(&self) -> Option<crate::native::Access> {
        match self.construction {
            Construction::Native(access) => Some(access),
            _ => None,
        }
    }
    pub(crate) unsafe fn set_behavior(&self, behavior: Oop) {
        unsafe {
            (*self.roots.get()).behavior = behavior;
            (*self.operations.get()).behavior = behavior;
        }
    }
    pub(crate) unsafe fn set_prototype(&self, prototype: Oop) {
        unsafe {
            (*self.roots.get()).prototype = prototype;
            (*self.operations.get()).prototype = prototype;
        }
    }
    pub fn operations(&self) -> *mut FloatApiV1 {
        self.operations.get()
    }
    pub fn imports(&self) -> *const birnam_abi::namespace_import::ApiV1 {
        match self.construction {
            Construction::Legacy { imports, .. } => imports,
            _ => ptr::null(),
        }
    }
    pub fn presence(&self) -> *const birnam_abi::namespace_presence::ApiV1 {
        match self.construction {
            Construction::Legacy { presence, .. } => presence,
            _ => ptr::null(),
        }
    }
    pub fn api(&self) -> *mut IdApi {
        match self.construction {
            Construction::Legacy { api, .. } => api,
            _ => ptr::null_mut(),
        }
    }
    pub fn identities(&self) -> [Oop; 2] {
        let roots = unsafe { *self.roots.get() };
        [roots.behavior, roots.prototype]
    }
    /// # Safety
    /// Called only from this runtime's marking phase. MarkFn does not collect.
    pub unsafe fn trace(&self, mark: MarkFn) {
        let references = self.native_access().map(|access| access.references());
        for object in self
            .identities()
            .into_iter()
            .chain(references.into_iter().flatten())
        {
            if !object.is_null() {
                unsafe { mark(object) };
            }
        }
    }
}

unsafe extern "C" fn make(context: *mut c_void, value: f64) -> Oop {
    // Copy context data before the allocator can invoke arbitrary native code.
    let instance = context.cast::<Instance>();
    if let Some(access) = unsafe { (*instance).native_access() } {
        return unsafe { crate::native::make(&*instance, access, value.to_bits()) }
            .unwrap_or_else(|_| crate::abort("native Float construction failed"));
    }
    let (api, behavior) = unsafe { ((*instance).api(), (*(*instance).roots.get()).behavior) };
    let allocate = unsafe { (*api).alloc };
    let object = unsafe { allocate(behavior, STORAGE.payload_size) };
    assert!(!object.is_null());
    unsafe { initialize(object.cast(), value.to_bits()) };
    object
}
unsafe extern "C" fn read(_: *mut c_void, object: Oop) -> f64 {
    // Same unsafe, callback-free read contract as the previous native accessor.
    f64::from_bits(unsafe { read_payload(object.cast()) })
}
unsafe extern "C" fn is_exact(context: *mut c_void, object: Oop) -> c_int {
    let instance = context.cast::<Instance>();
    if let Some(access) = unsafe { (*instance).native_access() } {
        return c_int::from(unsafe { crate::native::exact(&*instance, access, object) });
    }
    let (api, behavior) = unsafe { ((*instance).api(), (*(*instance).roots.get()).behavior) };
    let behavior_of = unsafe { (*api).vtableOf };
    c_int::from(unsafe { behavior_of(object) } == behavior)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_codec_preserves_payloads_without_touching_adjacent_bytes() {
        let mut seed = 0x7ff0_0000_0000_0001u64;
        for _ in 0..1024 {
            let mut bytes = [0xabu8; 10];
            unsafe { (STORAGE.initialize_bits)(bytes.as_mut_ptr().add(1), seed) };
            assert_eq!(&bytes[1..9], &seed.to_ne_bytes());
            assert_eq!(unsafe { (STORAGE.read_bits)(bytes.as_ptr().add(1)) }, seed);
            assert_eq!((bytes[0], bytes[9]), (0xab, 0xab));
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        }
    }
}
