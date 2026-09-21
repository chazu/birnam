//! Upper Runtime consumer of the already initialized native Lifetime owner.
use crate::{abi::Oop, runtime::Runtime};
use birnam_abi::lifetime::{AccessV1, OwnedV2};
use core::ffi::c_void;
use std::rc::Rc;

pub(crate) struct Owner(pub(crate) OwnedV2);
pub(crate) type Shared = Rc<Owner>;
impl Owner {
    pub(crate) unsafe fn adopt(pointer: *const OwnedV2) -> Shared {
        assert!(unsafe { birnam_abi::lifetime::valid(pointer) });
        Rc::new(Self(unsafe { *pointer }))
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        unsafe { (self.0.destroy)(self.0.context) };
    }
}
pub(crate) fn install_access(owner: OwnedV2, runtime: &Runtime) {
    unsafe extern "C" fn admit(context: *mut c_void) {
        unsafe { &*context.cast::<Runtime>() }.admit_payload_layout();
    }
    unsafe extern "C" fn facts(context: *mut c_void, object: Oop) -> u32 {
        unsafe { &*context.cast::<Runtime>() }.lifetime_facts(object)
    }
    unsafe {
        (owner.set_access)(
            owner.context,
            AccessV1 {
                context: core::ptr::from_ref(runtime).cast_mut().cast(),
                admit,
                facts,
            },
        )
    };
}
