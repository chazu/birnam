//! Explicit String dependency of Machine and its representation consumers.
//! Id's bootstrap, Values and Heap do not own this family.
use super::*;
use birnam_abi::string::{StringApiV1, StringStorageV1};

#[derive(Clone, Copy)]
pub(super) struct Binding {
    pointer: *mut StringApiV1,
    admitted: StringApiV1,
    storage: StringStorageV1,
}

#[cfg(test)]
pub(super) struct TestOwner {
    _api: Box<birnam_abi::IdApi>,
    pub(super) instance: Box<birnam_string::storage::Instance>,
}

impl Runtime {
    pub(crate) unsafe fn attach_string(&self, pointer: *mut StringApiV1) -> bool {
        self.assert_owner();
        if pointer.is_null() {
            return false;
        }
        if let Some(binding) = self.string.get() {
            return binding.pointer == pointer;
        }
        let ops = unsafe { pointer.read() };
        if ops.version != 1
            || ops.byte_size != size_of::<StringApiV1>()
            || ops.context.is_null()
            || ops.behavior.is_null()
            || ops.prototype.is_null()
            || ops.storage.is_null()
        {
            return false;
        }
        let storage = unsafe { ops.storage.read() };
        if storage.version != 1
            || storage.byte_size != size_of::<StringStorageV1>()
            || storage.payload_size == 0
            || !storage.payload_alignment.is_power_of_two()
            || storage.payload_alignment > align_of::<Oop>()
        {
            return false;
        }
        if self.checked_vtable_of(ops.behavior) != self.globals().vtable_vt
            || self.checked_vtable_of(ops.prototype) != ops.behavior
            || !self
                .payload_size(ops.prototype)
                .is_some_and(|size| size >= storage.payload_size)
        {
            return false;
        }
        self.string.set(Some(Binding {
            pointer,
            admitted: ops,
            storage,
        }));
        true
    }

    fn string_binding(&self) -> Binding {
        self.assert_owner();
        self.string.get().expect("String dependency not mounted")
    }

    pub(crate) fn string_behavior(&self) -> Oop {
        self.string_binding().admitted.behavior
    }
    #[cfg(test)]
    pub(crate) fn string_prototype(&self) -> Oop {
        self.string_binding().admitted.prototype
    }
    pub(crate) fn string_storage(&self) -> StringStorageV1 {
        self.string_binding().storage
    }
    pub(crate) fn string_context(&self) -> *mut core::ffi::c_void {
        self.string_binding().admitted.context
    }
    pub(crate) fn string_finalizer(&self) -> crate::abi::Finalizer {
        self.string_storage().cleanup
    }
    pub(crate) fn string_cleanup_matches_runtime(&self, finalizer: crate::abi::Finalizer) -> bool {
        let Some(binding) = self.string.get() else {
            return false;
        };
        unsafe { (binding.storage.cleanup_owner)(finalizer) == binding.admitted.context }
    }
    pub(crate) fn string_bytes(&self, object: Oop) -> Option<&[u8]> {
        let binding = self.string_binding();
        let mut length = 0;
        let bytes =
            unsafe { (binding.storage.borrow)(binding.admitted.context, object, &mut length) };
        if bytes.is_null() {
            None
        } else {
            Some(unsafe { core::slice::from_raw_parts(bytes, length) })
        }
    }
    #[cfg(test)]
    pub(crate) fn release_string(&self, object: Oop) {
        let binding = self.string_binding();
        assert_eq!(
            unsafe { (binding.storage.release)(binding.admitted.context, object) },
            1,
            "String finalizer lost byte ownership"
        );
    }
    pub(crate) fn make_string_with_finalizer(
        &self,
        bytes: &[u8],
        finalizer: crate::abi::Finalizer,
    ) -> Oop {
        let binding = self.string_binding();
        #[cfg(test)]
        {
            self.enter(|| unsafe {
                (binding.storage.construct)(
                    binding.admitted.context,
                    bytes.as_ptr(),
                    bytes.len(),
                    Some(finalizer),
                )
            })
        }
        #[cfg(not(test))]
        {
            unsafe {
                (binding.storage.construct)(
                    binding.admitted.context,
                    bytes.as_ptr(),
                    bytes.len(),
                    Some(finalizer),
                )
            }
        }
    }
    /// # Safety
    /// The matching owner-thread runtime is active; ordinary root rules apply.
    pub(crate) unsafe fn make_string_with_finalizer_active(
        &self,
        bytes: &[u8],
        finalizer: crate::abi::Finalizer,
    ) -> Oop {
        let binding = self.string_binding();
        unsafe {
            (binding.storage.construct)(
                binding.admitted.context,
                bytes.as_ptr(),
                bytes.len(),
                Some(finalizer),
            )
        }
    }
    #[cfg(test)]
    pub(crate) fn allocate_string(&self, vtable: Oop, bytes: &[u8]) -> Oop {
        assert_eq!(vtable, self.string_behavior());
        let binding = self.string_binding();
        self.enter(|| unsafe {
            (binding.storage.construct)(binding.admitted.context, bytes.as_ptr(), bytes.len(), None)
        })
    }
    #[cfg(test)]
    pub(crate) fn clone_string(&self, object: Oop, finalizer: crate::abi::Finalizer) -> Oop {
        let binding = self.string_binding();
        self.enter(|| unsafe {
            (binding.storage.clone)(binding.admitted.context, object, finalizer)
        })
    }

    #[cfg(test)]
    pub(crate) fn mount_test_string(&self, mut api: Box<birnam_abi::IdApi>) {
        unsafe extern "C" fn allocate(
            _: *mut core::ffi::c_void,
            plan: *const birnam_abi::native_allocation::NativeAllocationPlanV1,
        ) -> Oop {
            with_active(|runtime| {
                let ops = runtime.native_allocation();
                unsafe { (ops.allocate)(ops.context, plan) }
            })
        }
        unsafe extern "C" fn clone(receiver: Oop, _: *const Oop, _: usize) -> Oop {
            with_active(|runtime| {
                runtime.clone_string(receiver, crate::public_api::string_finalizer)
            })
        }
        let allocation = birnam_abi::native_allocation::NativeAllocationV1 {
            version: 1,
            byte_size: size_of::<birnam_abi::native_allocation::NativeAllocationV1>(),
            context: core::ptr::null_mut(),
            allocate,
        };
        let instance = unsafe {
            birnam_string::storage::Instance::empty(
                api.as_mut(),
                allocation,
                crate::public_api::string_finalizer,
                crate::public_api::test_string_cleanup_owner,
            )
        };
        let pointer = core::ptr::from_ref(instance.as_ref());
        unsafe {
            *self.test_string.get() = Some(TestOwner {
                _api: api,
                instance,
            })
        };
        self.enter(|| unsafe { (*pointer).prepare(clone) });
        assert!(unsafe { self.attach_string((*pointer).operations()) });
        self.enter(|| unsafe { (*pointer).publish() });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dispatch::build_bootstrap;
    unsafe extern "C" fn inert(_: Oop) {}

    #[test]
    fn owners_are_isolated_and_ignore_raw_payload_and_behavior_changes() {
        let first = build_bootstrap();
        let second = build_bootstrap();
        let a = &first.runtime;
        let b = &second.runtime;
        assert_ne!(a.string_behavior(), b.string_behavior());
        assert!(!unsafe { a.attach_string(b.string_binding().pointer) });
        let value = a.make_string_with_finalizer(b"owned\0\xffbytes", inert);
        assert_eq!(b.string_bytes(value), None);
        let pointer = a.string_bytes(value).unwrap().as_ptr();
        // User-visible raw slot mutation must not redirect native borrowing.
        unsafe {
            value.cast::<usize>().write(0);
            value.cast::<usize>().add(1).write(usize::MAX);
        }
        let child = a.create_vtable(a.string_behavior());
        assert!(unsafe { (&mut *a.state.get()).heap.set_vtable(value, child) });
        assert_eq!(a.string_bytes(value), Some(&b"owned\0\xffbytes"[..]));
        assert_eq!(a.string_bytes(value).unwrap().as_ptr(), pointer);
        let cloned = a.clone_string(value, inert);
        assert_eq!(a.checked_vtable_of(cloned), a.string_behavior());
        assert_eq!(a.string_bytes(cloned), a.string_bytes(value));
        assert_ne!(a.string_bytes(cloned).unwrap().as_ptr(), pointer);
        assert_eq!(a.has_finalizer(cloned), Some(true));
        a.release_string(value);
        assert_eq!(a.string_bytes(value), None);
        for runtime in [a, b] {
            let prototype = runtime.string_prototype();
            runtime.export(b"String", runtime.canonical().nil).unwrap();
            runtime
                .export(b"StringVt", runtime.canonical().nil)
                .unwrap();
            runtime.gc_collect_from_native_range(0, 0);
            assert_eq!(
                runtime.checked_vtable_of(prototype),
                runtime.string_behavior()
            );
            assert_eq!(runtime.string_bytes(prototype), Some(&b""[..]));
        }
    }

    #[test]
    fn construction_copies_after_collection_and_bypasses_replaceable_heap_callbacks() {
        thread_local! { static INPUT: Cell<*mut u8> = const { Cell::new(core::ptr::null_mut()) }; }
        unsafe extern "C" fn roots(_: crate::abi::MarkFn) {
            INPUT.with(|p| {
                if !p.get().is_null() {
                    unsafe { p.get().write(b'z') };
                }
            });
        }
        unsafe extern "C" fn forbidden_alloc(_: Oop, _: usize) -> Oop {
            panic!("String used replaceable alloc");
        }
        unsafe extern "C" fn forbidden_finalizer(_: Oop, _: crate::abi::Finalizer) {
            panic!("String used replaceable setFinalizer");
        }
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let api = unsafe {
            (&*runtime.test_string.get())
                .as_ref()
                .unwrap()
                .instance
                .api()
        };
        unsafe {
            (*api).alloc = forbidden_alloc;
            (*api).setFinalizer = forbidden_finalizer;
        }
        runtime.gc_register_root_provider(roots);
        let anchor = 0usize;
        runtime.gc_set_stack_bottom(core::ptr::from_ref(&anchor).addr());
        runtime.gc_set_auto_requested(false);
        let _ = runtime.allocate_instance(
            runtime.globals().object_vt,
            crate::gc::AUTO_COLLECTION_THRESHOLD,
        );
        let mut input = [b'a', 0, 0xff];
        INPUT.with(|p| p.set(input.as_mut_ptr()));
        runtime.gc_set_auto_requested(true);
        let ops = runtime.string_binding().admitted;
        let result =
            runtime.enter(|| unsafe { (ops.make)(ops.context, input.as_ptr(), input.len()) });
        INPUT.with(|p| p.set(core::ptr::null_mut()));
        runtime.gc_set_auto_requested(false);
        assert_eq!(input, [b'z', 0, 0xff]);
        assert_eq!(runtime.string_bytes(result), Some(&[b'z', 0, 0xff][..]));
        assert_eq!(runtime.has_finalizer(result), Some(true));
        let clone = runtime.clone_string(result, inert);
        assert_eq!(runtime.string_bytes(clone), runtime.string_bytes(result));
    }
}
