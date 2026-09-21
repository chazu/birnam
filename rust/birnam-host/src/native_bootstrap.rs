//! Host ownership of Bootstrap loaded through the real Id message API.
use birnam_abi::{
    bootstrap::OwnedV2,
    id_protocol::{self as native, Operand, Outcome, Protocol},
};

pub(crate) struct Owner {
    pointer: *const OwnedV2,
    api: OwnedV2,
}
impl Owner {
    pub(crate) fn load(id: Protocol, path: &[u8]) -> Result<Self, (u32, usize)> {
        let answer = unsafe {
            id.message.unwrap()(id.context, id.receiver, id.load, &Operand::bytes(path), 1)
        };
        if answer.status != native::LOADED {
            return Err((answer.status, answer.detail));
        }
        let pointer = answer.value.as_word().unwrap_or(0) as *const OwnedV2;
        if !unsafe { birnam_abi::bootstrap::valid(pointer) } {
            return Err((native::INCOMPATIBLE_ABI, 0));
        }
        Ok(Self {
            pointer,
            api: unsafe { *pointer },
        })
    }
    pub(crate) fn descriptor(&self) -> *const OwnedV2 {
        self.pointer
    }
    pub(crate) unsafe fn initialize(&self, path: &[u8], args: &[Operand]) -> Outcome {
        unsafe {
            (self.api.load)(
                self.api.context,
                path.as_ptr(),
                path.len(),
                args.as_ptr(),
                args.len(),
            )
        }
    }
    pub(crate) unsafe fn adopt_roots(
        &self,
        roots: *const birnam_abi::root_registry::OwnedV1,
    ) -> u32 {
        unsafe { (self.api.adopt_roots)(self.api.context, roots) }
    }
    pub(crate) unsafe fn release(&self, result: Operand) -> usize {
        unsafe { (self.api.release_result)(self.api.context, result) }
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        unsafe { (self.api.destroy)(self.api.context) };
    }
}
