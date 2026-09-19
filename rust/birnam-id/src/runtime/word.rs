//! Explicit Word dependency of Machine and its representation consumers.
//! Id's bootstrap, Values and Heap do not own this family.
use super::*;
use birnam_abi::word::{WordApiV1, WordStorageV1};

#[derive(Clone, Copy)]
pub(super) struct Binding {
    pointer: *mut WordApiV1,
    admitted: WordApiV1,
    storage: WordStorageV1,
}

#[cfg(test)]
pub(super) struct TestOwner {
    _api: Box<birnam_abi::IdApi>,
    pub(super) instance: Box<birnam_word::storage::Instance>,
}

impl Runtime {
    pub(crate) unsafe fn attach_word(&self, pointer: *mut WordApiV1) -> bool {
        self.assert_owner();
        if pointer.is_null() {
            return false;
        }
        if let Some(binding) = self.word.get() {
            return binding.pointer == pointer;
        }
        let ops = unsafe { pointer.read() };
        if ops.version != 1
            || ops.byte_size != size_of::<WordApiV1>()
            || ops.context.is_null()
            || ops.behavior.is_null()
            || ops.prototype.is_null()
            || ops.storage.is_null()
        {
            return false;
        }
        let storage = unsafe { ops.storage.read() };
        if storage.version != 1
            || storage.byte_size != size_of::<WordStorageV1>()
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
        self.word.set(Some(Binding {
            pointer,
            admitted: ops,
            storage,
        }));
        true
    }

    fn word_binding(&self) -> Binding {
        self.assert_owner();
        self.word.get().expect("Word dependency not mounted")
    }

    pub(crate) fn word_behavior(&self) -> Oop {
        self.word_binding().admitted.behavior
    }
    #[cfg(test)]
    pub(crate) fn word_prototype(&self) -> Oop {
        self.word_binding().admitted.prototype
    }
    pub(crate) fn word_storage(&self) -> WordStorageV1 {
        self.word_binding().storage
    }
    pub(crate) fn make_word(&self, value: u64) -> Oop {
        let binding = self.word_binding();
        let ops = unsafe { binding.pointer.read() };
        // Callbacks may collect or reenter. No RuntimeState borrow survives.
        self.enter(|| unsafe { (ops.make)(ops.context, value) })
    }
    pub(crate) fn word_value(&self, object: Oop) -> u64 {
        let storage = self.word_storage();
        // Machine's internal reader retains its pure-payload contract, including
        // callers inside a state borrow. Ordinary native clients use live ops.
        unsafe { (storage.read)(object.cast()) }
    }
    pub(crate) fn is_word(&self, object: Oop) -> bool {
        self.assert_owner();
        self.word
            .get()
            .is_some_and(|binding| self.checked_vtable_of(object) == binding.admitted.behavior)
    }

    #[cfg(test)]
    pub(crate) fn mount_test_word(&self, mut api: Box<birnam_abi::IdApi>) {
        let instance = unsafe { birnam_word::storage::Instance::empty(api.as_mut(), self.namespace_presence()) };
        let pointer = core::ptr::from_ref(instance.as_ref());
        unsafe {
            *self.test_word.get() = Some(TestOwner {
                _api: api,
                instance,
            })
        };
        self.enter(|| unsafe { (*pointer).prepare() });
        assert!(unsafe { self.attach_word((*pointer).operations()) });
        self.enter(|| unsafe { (*pointer).publish() });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dispatch::build_bootstrap;

    #[test]
    fn word_instances_keep_runtime_identity_bits_and_roots_separate() {
        let first = build_bootstrap();
        let second = build_bootstrap();
        let a = &first.runtime;
        let b = &second.runtime;
        assert_ne!(a.word_behavior(), b.word_behavior());
        assert_ne!(a.word_prototype(), b.word_prototype());
        let foreign = b.word_binding().pointer;
        assert!(!unsafe { a.attach_word(foreign) });
        for bits in [
            0,
            1,
            0x8000_0000_0000_0000,
            i64::MAX as u64,
            u64::MAX - 1,
            u64::MAX,
            0xfedc_ba98_7654_3210,
        ] {
            let left = a.make_word(bits);
            let right = b.make_word(bits);
            assert!(a.is_word(left) && b.is_word(right));
            assert!(!a.is_word(right) && !b.is_word(left));
            assert_eq!(a.word_value(left), bits);
            assert_eq!(b.word_value(right), bits);
        }
        for runtime in [a, b] {
            let behavior = runtime.word_behavior();
            let prototype = runtime.word_prototype();
            runtime.export(b"Word", runtime.canonical().nil).unwrap();
            runtime.export(b"WordVt", runtime.canonical().nil).unwrap();
            runtime.gc_collect_from_native_range(0, 0);
            assert_eq!(
                runtime.checked_vtable_of(behavior),
                runtime.globals().vtable_vt
            );
            assert_eq!(runtime.checked_vtable_of(prototype), behavior);
            assert_eq!(runtime.word_value(prototype), 0);
            let value = runtime.make_word(1 << 63);
            assert_eq!(runtime.word_value(value), 1 << 63);
        }
    }
    #[test]
    fn runtime_constructor_observes_live_reentrant_word_operations() {
        unsafe extern "C" fn wrapped(_: *mut core::ffi::c_void, value: u64) -> Oop {
            super::super::with_active(|runtime| {
                runtime.gc_collect_from_native_range(0, 0);
                let marker = runtime.make_integer(9);
                assert!(runtime.is_integer(marker));
                let original = runtime.word_binding().admitted;
                unsafe { (original.make)(original.context, value ^ (1 << 63)) }
            })
        }
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let binding = runtime.word_binding();
        unsafe { (*binding.pointer).make = wrapped };
        let value = runtime.make_word(42);
        assert_eq!(runtime.word_value(value), 42 ^ (1 << 63));
        unsafe { (*binding.pointer).make = binding.admitted.make };
        let value = runtime.make_word(42);
        assert_eq!(runtime.word_value(value), 42);
    }
}
