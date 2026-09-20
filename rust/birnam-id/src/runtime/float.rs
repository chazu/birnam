//! Explicit Float dependency of Machine and its representation consumers.
//! Id's bootstrap, Values and Heap do not own this family.
use super::*;
use birnam_abi::float::{FloatApiV1, FloatStorageV1};

#[derive(Clone, Copy)]
pub(super) struct Binding {
    pointer: *mut FloatApiV1,
    admitted: FloatApiV1,
    storage: FloatStorageV1,
}

#[cfg(test)]
pub(super) struct TestOwner {
    _api: Box<birnam_abi::IdApi>,
    pub(super) instance: Box<birnam_float::storage::Instance>,
}

impl Runtime {
    pub(crate) unsafe fn attach_float(&self, pointer: *mut FloatApiV1) -> bool {
        self.assert_owner();
        if pointer.is_null() {
            return false;
        }
        if let Some(binding) = self.float.get() {
            return binding.pointer == pointer;
        }
        let ops = unsafe { pointer.read() };
        if ops.version != 1
            || ops.byte_size != size_of::<FloatApiV1>()
            || ops.context.is_null()
            || ops.behavior.is_null()
            || ops.prototype.is_null()
            || ops.storage.is_null()
        {
            return false;
        }
        let storage = unsafe { ops.storage.read() };
        if storage.version != 1
            || storage.byte_size != size_of::<FloatStorageV1>()
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
        self.float.set(Some(Binding {
            pointer,
            admitted: ops,
            storage,
        }));
        true
    }

    fn float_binding(&self) -> Binding {
        self.assert_owner();
        self.float.get().expect("Float dependency not mounted")
    }

    pub(crate) fn float_behavior(&self) -> Oop {
        self.float_binding().admitted.behavior
    }
    #[cfg(test)]
    pub(crate) fn float_prototype(&self) -> Oop {
        self.float_binding().admitted.prototype
    }
    pub(crate) fn float_storage(&self) -> FloatStorageV1 {
        self.float_binding().storage
    }
    pub(crate) fn make_float(&self, value: f64) -> Oop {
        let binding = self.float_binding();
        let ops = unsafe { binding.pointer.read() };
        // Callbacks may collect or reenter. No RuntimeState borrow survives.
        self.enter(|| unsafe { (ops.make)(ops.context, value) })
    }
    pub(crate) fn float_value(&self, object: Oop) -> f64 {
        let storage = self.float_storage();
        // Machine's internal reader retains its pure-payload contract, including
        // callers inside a state borrow. Ordinary native clients use live ops.
        f64::from_bits(unsafe { (storage.read_bits)(object.cast()) })
    }
    pub(crate) fn is_float(&self, object: Oop) -> bool {
        self.assert_owner();
        self.float
            .get()
            .is_some_and(|binding| self.checked_vtable_of(object) == binding.admitted.behavior)
    }

    #[cfg(test)]
    pub(crate) fn mount_test_float(&self, mut api: Box<birnam_abi::IdApi>) {
        let instance = unsafe { birnam_float::storage::Instance::empty(api.as_mut(), self.namespace_presence(), self.namespace_import(), self.namespace_export(), self.payload_layout()) };
        let pointer = core::ptr::from_ref(instance.as_ref());
        unsafe {
            *self.test_float.get() = Some(TestOwner {
                _api: api,
                instance,
            })
        };
        self.enter(|| unsafe { (*pointer).prepare() });
        assert!(unsafe { self.attach_float((*pointer).operations()) });
        self.enter(|| unsafe { (*pointer).publish() });
    }
}

#[cfg(test)]
mod tests {
    use crate::dispatch::build_bootstrap;

    #[test]
    fn float_instances_keep_runtime_identity_bits_and_roots_separate() {
        let first = build_bootstrap();
        let second = build_bootstrap();
        let a = &first.runtime;
        let b = &second.runtime;
        assert_ne!(a.float_behavior(), b.float_behavior());
        assert_ne!(a.float_prototype(), b.float_prototype());
        let foreign = b.float_binding().pointer;
        assert!(!unsafe { a.attach_float(foreign) });
        for bits in [
            0,
            1,
            0x8000_0000_0000_0000,
            0x7ff0_0000_0000_0000,
            0xfff0_0000_0000_0000,
            0x7ff8_dead_beef_1234,
            0x7ff0_0000_0000_0001,
        ] {
            let left = a.make_float(f64::from_bits(bits));
            let right = b.make_float(f64::from_bits(bits));
            assert!(a.is_float(left) && b.is_float(right));
            assert!(!a.is_float(right) && !b.is_float(left));
            assert_eq!(a.float_value(left).to_bits(), bits);
            assert_eq!(b.float_value(right).to_bits(), bits);
        }
        for runtime in [a, b] {
            let behavior = runtime.float_behavior();
            let prototype = runtime.float_prototype();
            runtime.export(b"Float", runtime.canonical().nil).unwrap();
            runtime.export(b"FloatVt", runtime.canonical().nil).unwrap();
            runtime.gc_collect_from_native_range(0, 0);
            assert_eq!(
                runtime.checked_vtable_of(behavior),
                runtime.globals().vtable_vt
            );
            assert_eq!(runtime.checked_vtable_of(prototype), behavior);
            assert_eq!(runtime.float_value(prototype).to_bits(), 0);
            let value = runtime.make_float(-0.0);
            assert_eq!(runtime.float_value(value).to_bits(), 1 << 63);
        }
    }
}
