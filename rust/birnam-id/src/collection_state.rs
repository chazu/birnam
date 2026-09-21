//! Borrow the completed collection owner; Runtime installs a complete admission extension.
use birnam_abi::collection_state::{AccessV1, OwnedV1};
use core::ffi::c_void;
use std::rc::Rc;
pub(crate) struct Owner(pub(crate) OwnedV1);
pub(crate) type Shared = Rc<Owner>;
impl Owner {
    pub(crate) unsafe fn adopt(pointer: *const OwnedV1) -> Shared {
        assert!(unsafe { birnam_abi::collection_state::valid(pointer) });
        Rc::new(Self(unsafe { *pointer }))
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        unsafe { (self.0.destroy)(self.0.context) };
    }
}
pub(crate) fn install_access(owner: &Shared, runtime: &crate::runtime::Runtime) {
    unsafe extern "C" fn settings(context: *mut c_void) {
        unsafe { &*context.cast::<crate::runtime::Runtime>() }.admit_gc_settings();
    }
    unsafe extern "C" fn stack(context: *mut c_void) {
        unsafe { &*context.cast::<crate::runtime::Runtime>() }.admit_native_stack();
    }
    unsafe {
        (owner.0.set_access)(
            owner.0.context,
            AccessV1 {
                context: core::ptr::from_ref(runtime).cast_mut().cast(),
                settings,
                stack,
            },
        )
    };
}
