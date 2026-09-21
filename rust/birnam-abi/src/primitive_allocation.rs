//! Completed Id primitives retained by Bootstrap before collecting methods mount.
//! Upper allocation policy surrounds ordinary calls; deferred materialization
//! may invoke these exact non-collecting entries during collection.
use crate::id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PrimitivesV1 {
    pub version: u32,
    pub byte_size: usize,
    pub allocate: NativeEntry,
    pub intern: NativeEntry,
}
const _: () = assert!(size_of::<PrimitivesV1>() == 48);
/// # Safety
/// Select during trusted Id bootstrap, before upper allocation replacements.
/// The caller retains Id and the selected entries' code/contexts throughout use.
pub unsafe fn select(id: Protocol) -> Result<PrimitivesV1, Outcome> {
    let intern = |name: &[u8]| -> Result<crate::Oop, Outcome> {
        unsafe { id.message.unwrap()(id.context, id.receiver, id.intern, &Operand::bytes(name), 1) }
            .into_result()?
            .as_object()
            .ok_or_else(invalid)
    };
    let lookup = intern(b"lookup")?;
    let select = |selector| -> Result<NativeEntry, Outcome> {
        unsafe {
            id.message.unwrap()(
                id.context,
                id.receiver,
                lookup,
                &Operand::object(selector),
                1,
            )
        }
        .into_result()?
        .as_entry()
        .filter(|entry| entry.method.is_some())
        .ok_or_else(invalid)
    };
    Ok(PrimitivesV1 {
        version: 1,
        byte_size: size_of::<PrimitivesV1>(),
        allocate: select(intern(b"allocate")?)?,
        intern: select(id.intern)?,
    })
}
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
/// # Safety
/// Readable native descriptor header and the declared extent if admitted.
pub unsafe fn valid(pointer: *const PrimitivesV1) -> bool {
    if pointer.is_null() || !pointer.addr().is_multiple_of(align_of::<PrimitivesV1>()) {
        return false;
    }
    unsafe {
        if core::ptr::addr_of!((*pointer).version).read() != 1
            || core::ptr::addr_of!((*pointer).byte_size).read() != size_of::<PrimitivesV1>()
        {
            return false;
        }
        (*pointer).allocate.method.is_some() && (*pointer).intern.method.is_some()
    }
}
