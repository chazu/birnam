//! String owns its raw payload and the address-keyed native byte buffers.
use birnam_abi::native_allocation::{NativeAllocationPlanV1, NativeAllocationV1};
use birnam_abi::string::{StringApiV1, StringStorageV1};
use birnam_abi::{Finalizer, IdApi, MarkFn, Method, Oop};
use core::cell::UnsafeCell;
use core::ffi::{c_int, c_void};
use core::hash::{BuildHasherDefault, Hasher};
use core::ptr;
use std::collections::HashMap;

#[derive(Default)]
struct AddressHasher(u64);
impl Hasher for AddressHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        let mut hash = 0xcbf29ce484222325u64;
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        self.0 = hash;
    }
    fn write_usize(&mut self, address: usize) {
        self.0 = ((address >> 3) as u64).wrapping_mul(0x9e3779b97f4a7c15);
    }
}
type Owners = HashMap<usize, Box<[u8]>, BuildHasherDefault<AddressHasher>>;
#[repr(C)]
struct RawString {
    pointer: *const u8,
    length: usize,
}
const _: () = {
    assert!(size_of::<RawString>() == 16);
    assert!(align_of::<RawString>() == 8);
    assert!(core::mem::offset_of!(RawString, pointer) == 0);
    assert!(core::mem::offset_of!(RawString, length) == 8);
};
#[derive(Clone, Copy, Default)]
struct Roots {
    behavior: Oop,
    prototype: Oop,
}

#[derive(Clone, Copy)]
enum ConstructionApi {
    Native(crate::native::Access),
    Legacy {
        api: *mut IdApi,
        exports: *const birnam_abi::namespace_export::ApiV1,
        layouts: *const birnam_abi::payload_layout::ApiV1,
        allocation: NativeAllocationV1,
    },
}

/// One mounting. Tests use local instances and explicit adapter callbacks.
pub struct Instance {
    construction: ConstructionApi,
    roots: UnsafeCell<Roots>,
    owners: UnsafeCell<Owners>,
    storage: StringStorageV1,
    operations: UnsafeCell<StringApiV1>,
}
impl Instance {
    /// # Safety
    /// Owner-thread construction; every supplied capability outlives this state.
    pub unsafe fn empty(
        api: *mut IdApi,
        exports: *const birnam_abi::namespace_export::ApiV1,
        layouts: *const birnam_abi::payload_layout::ApiV1,
        allocation: NativeAllocationV1,
        cleanup: Finalizer,
        cleanup_owner: unsafe extern "C" fn(Finalizer) -> *mut c_void,
    ) -> Box<Self> {
        assert!(!api.is_null());
        assert_eq!(allocation.version, 1);
        assert_eq!(allocation.byte_size, size_of::<NativeAllocationV1>());
        Self::new(
            ConstructionApi::Legacy {
                api,
                exports,
                layouts,
                allocation,
            },
            cleanup,
            cleanup_owner,
        )
    }
    pub(crate) fn native(
        access: crate::native::Access,
        cleanup: Finalizer,
        cleanup_owner: unsafe extern "C" fn(Finalizer) -> *mut c_void,
    ) -> Box<Self> {
        Self::new(ConstructionApi::Native(access), cleanup, cleanup_owner)
    }
    fn new(
        construction: ConstructionApi,
        cleanup: Finalizer,
        cleanup_owner: unsafe extern "C" fn(Finalizer) -> *mut c_void,
    ) -> Box<Self> {
        let result = Box::new(Self {
            construction,
            roots: UnsafeCell::new(Roots::default()),
            owners: UnsafeCell::new(Owners::default()),
            storage: StringStorageV1 {
                version: 1,
                byte_size: size_of::<StringStorageV1>(),
                payload_size: size_of::<RawString>(),
                payload_alignment: align_of::<RawString>(),
                construct,
                clone: clone_owned,
                borrow,
                release,
                cleanup,
                cleanup_owner,
            },
            operations: UnsafeCell::new(StringApiV1 {
                version: 1,
                byte_size: size_of::<StringApiV1>(),
                context: ptr::null_mut(),
                behavior: ptr::null_mut(),
                prototype: ptr::null_mut(),
                make,
                bytes,
                storage: ptr::null(),
            }),
        });
        unsafe {
            (*result.operations.get()).context = ptr::from_ref(result.as_ref()).cast_mut().cast();
            (*result.operations.get()).storage = &result.storage;
        }
        result
    }
    pub(crate) fn owns(&self, object: Oop) -> bool {
        unsafe { (&*self.owners.get()).contains_key(&object.addr()) }
    }
    pub(crate) fn native_access(&self) -> Option<crate::native::Access> {
        match self.construction {
            ConstructionApi::Native(access) => Some(access),
            ConstructionApi::Legacy { .. } => None,
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
    pub(crate) unsafe fn construct_native(
        &self,
        bytes: &[u8],
        source: Oop,
    ) -> Result<Oop, birnam_abi::id_protocol::Outcome> {
        unsafe {
            allocate(
                self,
                bytes.as_ptr(),
                bytes.len(),
                source,
                Some(self.storage.cleanup),
            )
        }
    }
    /// # Safety
    /// The caller registers partial roots first. Run once on the floor owner.
    pub unsafe fn prepare(&self, clone_method: Method) {
        let ConstructionApi::Legacy { api, layouts, .. } = self.construction else {
            unreachable!()
        };
        let behavior = unsafe { ((*api).proto)((*api).object_vt) };
        assert!(!behavior.is_null());
        unsafe {
            (*self.roots.get()).behavior = behavior;
        }
        unsafe {
            birnam_abi::payload_layout::set(layouts, behavior, ptr::null(), 0, birnam_abi::SLOT_RAW)
        };
        let prototype = unsafe {
            construct(
                (*self.operations.get()).context,
                ptr::null(),
                0,
                Some(self.storage.cleanup),
            )
        };
        unsafe {
            (*self.roots.get()).prototype = prototype;
            (*self.operations.get()).behavior = behavior;
            (*self.operations.get()).prototype = prototype;
        }
        // These defaults used to be installed by bootstrap, once. Activation
        // must not reinstall them after a user's later replacement.
        for selector in [c"clone", c"new"] {
            let symbol = unsafe { ((*api).intern)(selector.as_ptr()) };
            unsafe { ((*api).method)(behavior, symbol, clone_method) };
        }
    }
    /// # Safety
    /// Called once, after primitives have been attached to all early consumers.
    pub unsafe fn publish(&self) {
        let ConstructionApi::Legacy { api, exports, .. } = self.construction else {
            unreachable!()
        };
        let [behavior, prototype] = self.identities();
        unsafe {
            birnam_abi::set_vtable_owner(&*api, behavior, prototype);
            birnam_abi::namespace_export::export(exports, c"String".as_ptr(), prototype);
            birnam_abi::namespace_export::export(exports, c"StringVt".as_ptr(), behavior);
        }
    }
    pub fn api(&self) -> *mut IdApi {
        match self.construction {
            ConstructionApi::Legacy { api, .. } => api,
            ConstructionApi::Native(_) => ptr::null_mut(),
        }
    }
    pub fn operations(&self) -> *mut StringApiV1 {
        self.operations.get()
    }
    pub fn identities(&self) -> [Oop; 2] {
        let roots = unsafe { *self.roots.get() };
        [roots.behavior, roots.prototype]
    }
    /// # Safety
    /// Marking-phase callback on this mounting's owner thread; mark cannot GC.
    pub unsafe fn trace(&self, mark: MarkFn) {
        let native = self.native_access().map(|access| access.references());
        for object in self
            .identities()
            .into_iter()
            .chain(native.into_iter().flatten())
        {
            if !object.is_null() {
                unsafe { mark(object) };
            }
        }
    }
}

struct Construction {
    instance: *const Instance,
    bytes: *const u8,
    length: usize,
    source: Oop,
    owned: Option<Box<[u8]>>,
}
unsafe extern "C" fn prepare(context: *mut c_void) {
    let plan = unsafe { &mut *context.cast::<Construction>() };
    if !plan.source.is_null() {
        // Retain clone's two copies, both after the floor collection check.
        let owners = unsafe { &*(*plan.instance).owners.get() };
        let bytes = owners
            .get(&plan.source.addr())
            .expect("String clone lost byte ownership")
            .to_vec();
        plan.owned = Some(bytes.to_vec().into_boxed_slice());
    } else {
        let bytes = if plan.length == 0 {
            &[]
        } else {
            unsafe { core::slice::from_raw_parts(plan.bytes, plan.length) }
        };
        plan.owned = Some(bytes.to_vec().into_boxed_slice());
    }
}
unsafe extern "C" fn initialize(context: *mut c_void, object: Oop) {
    let plan = unsafe { &mut *context.cast::<Construction>() };
    let bytes = plan
        .owned
        .take()
        .expect("String allocation was not prepared");
    unsafe {
        object.cast::<RawString>().write(RawString {
            pointer: bytes.as_ptr(),
            length: bytes.len(),
        })
    };
    let previous = unsafe { (&mut *(*plan.instance).owners.get()).insert(object.addr(), bytes) };
    assert!(previous.is_none(), "duplicate String object address");
}
unsafe fn allocate(
    instance: *const Instance,
    bytes: *const u8,
    length: usize,
    source: Oop,
    finalizer: Option<Finalizer>,
) -> Result<Oop, birnam_abi::id_protocol::Outcome> {
    // No state reference crosses the collecting allocation boundary.
    let native = unsafe { (*instance).native_access() };
    let behavior = unsafe { (*(*instance).roots.get()).behavior };
    let mut construction = Construction {
        instance,
        bytes,
        length,
        source,
        owned: None,
    };
    let plan = NativeAllocationPlanV1 {
        context: ptr::from_mut(&mut construction).cast(),
        behavior,
        payload_size: size_of::<RawString>(),
        finalizer,
        prepare,
        initialize,
    };
    if let Some(access) = native {
        unsafe { crate::native::allocate(access, &plan, source, &*instance) }
    } else {
        let ConstructionApi::Legacy { allocation, .. } = (unsafe { (*instance).construction })
        else {
            unreachable!()
        };
        Ok(unsafe { (allocation.allocate)(allocation.context, &plan) })
    }
}
unsafe extern "C" fn construct(
    context: *mut c_void,
    bytes: *const u8,
    length: usize,
    finalizer: Option<Finalizer>,
) -> Oop {
    assert!(
        !bytes.is_null() || length == 0,
        "String bytes are null with nonzero length"
    );
    unsafe { allocate(context.cast(), bytes, length, ptr::null_mut(), finalizer) }
        .unwrap_or_else(|_| crate::abort("native String allocation failed"))
}
unsafe extern "C" fn clone_owned(context: *mut c_void, object: Oop, finalizer: Finalizer) -> Oop {
    if object.is_null() {
        return object;
    }
    unsafe { allocate(context.cast(), ptr::null(), 0, object, Some(finalizer)) }
        .unwrap_or_else(|_| crate::abort("native String clone failed"))
}
unsafe extern "C" fn make(context: *mut c_void, bytes: *const u8, length: usize) -> Oop {
    let cleanup = unsafe { (*context.cast::<Instance>()).storage.cleanup };
    unsafe { construct(context, bytes, length, Some(cleanup)) }
}
unsafe extern "C" fn borrow(context: *mut c_void, object: Oop, length: *mut usize) -> *const u8 {
    assert!(!length.is_null(), "String byte length output is null");
    let owners = unsafe { &*(*context.cast::<Instance>()).owners.get() };
    let Some(bytes) = owners.get(&object.addr()) else {
        return ptr::null();
    };
    unsafe { length.write(bytes.len()) };
    bytes.as_ptr()
}
unsafe extern "C" fn bytes(context: *mut c_void, object: Oop, length: *mut usize) -> *const u8 {
    let pointer = unsafe { borrow(context, object, length) };
    assert!(!pointer.is_null(), "String bytes object is not owned");
    pointer
}
unsafe extern "C" fn release(context: *mut c_void, object: Oop) -> c_int {
    c_int::from(unsafe {
        (&mut *(*context.cast::<Instance>()).owners.get())
            .remove(&object.addr())
            .is_some()
    })
}
