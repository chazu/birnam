//! Completed Runtime policy installed as ordinary extensions of Id methods.
use super::Runtime;
use crate::abi::{
    Oop,
    id_protocol::{self as native, NativeEntry, Operand, Outcome},
};
use core::ffi::c_void;

impl Runtime {
    pub(crate) fn install_id_runtime_extensions(&self) {
        self.assert_owner();
        let state = unsafe { &mut *self.state.get() };
        let Some(id) = state.id_protocol.as_ref() else {
            return;
        };
        let protocol = id.protocol;
        let context = core::ptr::from_ref(self).cast_mut().cast();
        let dictionaries =
            unsafe { &mut *crate::behaviors::Behaviors::dictionaries_ptr(&mut state.vtables) };
        dictionaries.add_method(
            protocol.receiver,
            protocol.intern,
            NativeEntry::new(intern, context),
        );
        dictionaries.add_method(
            protocol.receiver,
            protocol.load,
            NativeEntry::new(load, context),
        );
        let add = state.symbols.existing(b"addMethod").unwrap();
        let remove = state.symbols.existing(b"removeMethod:").unwrap();
        dictionaries.add_method(
            protocol.receiver,
            add,
            NativeEntry::new(add_method, context),
        );
        dictionaries.add_method(
            protocol.receiver,
            remove,
            NativeEntry::new(remove_method, context),
        );
    }
}
unsafe extern "C" fn intern(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([argument]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let Some(bytes) = argument.as_bytes() else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let Some(bytes) = (unsafe { crate::abi::borrowed_bytes(bytes.pointer, bytes.length) })
        .filter(|bytes| !bytes.contains(&0))
    else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let runtime = unsafe { &*context.cast::<Runtime>() };
    let symbol =
        runtime.enter_application(|| runtime.intern(bytes).expect("validated Symbol name"));
    Outcome::loaded(Operand::object(symbol))
}
unsafe extern "C" fn load(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some(arguments) = (unsafe { crate::abi::borrowed_slice(args, count) }) else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let Some((path, arguments)) = arguments.split_first() else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let Some(path) = path.as_bytes() else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let Some(path) = (unsafe { crate::abi::borrowed_bytes(path.pointer, path.length) }) else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let runtime = unsafe { &*context.cast::<Runtime>() };
    let protocol = unsafe {
        (&*runtime.state.get())
            .id_protocol
            .as_ref()
            .unwrap()
            .protocol
    };
    let mut result = Outcome::failed(native::INITIALIZER_FAILED, 0);
    runtime.enter_application(|| {
        result = unsafe { crate::abi::native_module::load(&protocol, path, arguments) };
        // enter_application retains an outward Object through its checkpoint.
        result.value.as_object().unwrap_or(runtime.canonical().nil)
    });
    result
}

pub(crate) unsafe extern "C" fn add_method(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([selector, entry]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let (Some(selector), Some(entry)) = (
        selector.as_object(),
        entry.as_entry().filter(|entry| entry.method.is_some()),
    ) else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let runtime = unsafe { &*context.cast::<Runtime>() };
    runtime.enter_application(|| {
        runtime.add_native_method(receiver, selector, entry);
        receiver
    });
    Outcome::loaded(Operand::entry(entry))
}
pub(crate) unsafe extern "C" fn remove_method(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([selector]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let Some(selector) = selector.as_object() else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let runtime = unsafe { &*context.cast::<Runtime>() };
    let mut removed = None;
    runtime.enter_application(|| {
        removed = runtime.remove_native_method(receiver, selector);
        receiver
    });
    Outcome::loaded(removed.map(Operand::entry).unwrap_or_else(Operand::unit))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_same_carrier_survives_adoption_and_gc_accounts_for_later_symbols() {
        let initial = crate::initial_objects::InitialObjects::new();
        let protocol = initial.protocol.protocol;
        let intern = |bytes: &[u8]| unsafe {
            protocol.message.unwrap()(
                protocol.context,
                protocol.receiver,
                protocol.intern,
                &Operand::bytes(bytes),
                1,
            )
        };
        let before = intern(b"native-before-adoption").value.as_object().unwrap();
        let bootstrap = Box::new(crate::dispatch::build_bootstrap_from(initial));
        let runtime = &bootstrap.runtime;
        runtime.install_id_runtime_extensions();
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        runtime.enable_symbol_compaction_for_test();
        assert_eq!(
            intern(b"native-before-adoption").value.as_object(),
            Some(before)
        );
        let _before_root = crate::runtime::ScopedOopRootGuard::new(runtime, before);
        let after = intern(b"native-after-adoption").value.as_object().unwrap();
        assert_eq!(
            runtime.permanence_of(after),
            Some(crate::heap::Permanence::Collectable)
        );
        {
            let _after_root = crate::runtime::ScopedOopRootGuard::new(runtime, after);
            runtime.enter(|| runtime.gc_collect_from_native_range(0, 0));
            assert_eq!(
                runtime.symbol_bytes(before),
                Some(&b"native-before-adoption"[..])
            );
            assert_eq!(
                runtime.symbol_bytes(after),
                Some(&b"native-after-adoption"[..])
            );
        }
        runtime.enter(|| runtime.gc_collect_from_native_range(0, 0));
        assert_eq!(runtime.symbol_bytes(after), None);
        assert_eq!(intern(b"bad\0name").status, native::INVALID_REQUEST);
        let retained = runtime.intern(b"intern:").unwrap();
        assert_eq!(retained, protocol.intern);
    }
}
