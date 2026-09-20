//! Integer-owned canonical representation and native primitive operations.
use birnam_abi::integer::{IntegerApiV1, IntegerStorageV1};
use birnam_abi::{IdApi, MarkFn, Oop};
use core::cell::UnsafeCell;
use core::ffi::{c_int, c_void};
use core::ptr;

#[repr(C)]
struct RawInteger {
    value: i64,
}
const _: () = {
    assert!(size_of::<RawInteger>() == 8);
    assert!(align_of::<RawInteger>() == 8);
};
unsafe extern "C" fn initialize(payload: *mut u8, value: i64) {
    // Native object storage meets payload_alignment. Unaligned access also
    // permits callers to prepare owned byte recipes without a typed alias.
    unsafe {
        payload
            .cast::<RawInteger>()
            .write_unaligned(RawInteger { value })
    };
}
unsafe extern "C" fn read_payload(payload: *const u8) -> i64 {
    unsafe { payload.cast::<RawInteger>().read_unaligned().value }
}
pub static STORAGE: IntegerStorageV1 = IntegerStorageV1 {
    version: 1,
    byte_size: size_of::<IntegerStorageV1>(),
    payload_size: size_of::<RawInteger>(),
    payload_alignment: align_of::<RawInteger>(),
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
    exports: *const birnam_abi::namespace_export::ApiV1,
    roots: UnsafeCell<Roots>,
    operations: UnsafeCell<IntegerApiV1>,
}

impl Instance {
    /// Allocate native state before registering its partially initialized roots.
    /// # Safety
    /// The API lives until this instance and all installed consumers are gone.
    pub unsafe fn empty(api: *mut IdApi, exports: *const birnam_abi::namespace_export::ApiV1) -> Box<Self> {
        assert!(!api.is_null());
        let result = Box::new(Self {
            api,
            exports,
            roots: UnsafeCell::new(Roots::default()),
            operations: UnsafeCell::new(IntegerApiV1 {
                version: 1,
                byte_size: size_of::<IntegerApiV1>(),
                context: ptr::null_mut(),
                behavior: ptr::null_mut(),
                prototype: ptr::null_mut(),
                make,
                read,
                is_exact,
                storage: &STORAGE,
                allocation_context,
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
    pub unsafe fn populate(&self) {
        let api = self.api;
        let (proto, alloc, export, layout, object_vt) = unsafe {
            (
                (*api).proto,
                (*api).alloc,
                *self.exports,
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
        // Existing owner protocol can collect/reenter: both identities already
        // belong to the module's partially initialized root provider.
        unsafe { birnam_abi::set_vtable_owner(&*api, behavior, prototype) };
        unsafe {
            (*self.operations.get()).behavior = behavior;
            (*self.operations.get()).prototype = prototype;
            export.export.unwrap()(export.context, c"Integer".as_ptr(), prototype);
            export.export.unwrap()(export.context, c"IntegerVt".as_ptr(), behavior);
        }
    }

    pub fn operations(&self) -> *mut IntegerApiV1 {
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

unsafe extern "C" fn make(context: *mut c_void, value: i64) -> Oop {
    // Copy context data before the allocator can invoke arbitrary native code.
    let instance = context.cast::<Instance>();
    let (api, behavior) = unsafe { ((*instance).api, (*(*instance).roots.get()).behavior) };
    let allocate = unsafe { (*api).alloc };
    let object = unsafe { allocate(behavior, STORAGE.payload_size) };
    assert!(!object.is_null());
    unsafe { initialize(object.cast(), value) };
    object
}
unsafe extern "C" fn read(_: *mut c_void, object: Oop) -> i64 {
    // Same unsafe, callback-free read contract as the previous native accessor.
    unsafe { read_payload(object.cast()) }
}
unsafe extern "C" fn is_exact(context: *mut c_void, object: Oop) -> c_int {
    let instance = context.cast::<Instance>();
    let (api, behavior) = unsafe { ((*instance).api, (*(*instance).roots.get()).behavior) };
    let behavior_of = unsafe { (*api).vtableOf };
    c_int::from(unsafe { behavior_of(object) } == behavior)
}

unsafe extern "C" fn allocation_context(context: *mut c_void) -> usize {
    let api = unsafe { (*context.cast::<Instance>()).api };
    unsafe { (*api).alloc as usize }
}
