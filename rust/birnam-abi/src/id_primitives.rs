//! Original Id entries selected by Bootstrap immediately after loading Id.
//! This completed dependency supplies allocation and primitive identities without
//! exposing private Rust symbols or adding future callback slots to Id.
use crate::id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PrimitivesV1 {
    pub version: u32,
    pub byte_size: usize,
    pub allocate: NativeEntry,
    pub intern: NativeEntry,
    pub lookup: NativeEntry,
    pub add_method: NativeEntry,
    pub remove_method: NativeEntry,
    pub allocation_size: NativeEntry,
    pub behavior_of: NativeEntry,
    pub symbol_bytes: NativeEntry,
}
const _: () = assert!(size_of::<PrimitivesV1>() == 144);
/// # Safety
/// Select immediately after trusted Id construction, before loading upper modules.
/// Calling this after method replacement cannot establish primitive semantics.
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
        lookup: select(lookup)?,
        add_method: select(intern(b"addMethod")?)?,
        remove_method: select(intern(b"removeMethod:")?)?,
        allocation_size: select(intern(b"allocationSizeOf:")?)?,
        behavior_of: select(intern(b"behaviorOf:")?)?,
        symbol_bytes: select(intern(b"symbolBytesOf:")?)?,
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
        let entries = &*pointer;
        [
            entries.allocate,
            entries.intern,
            entries.lookup,
            entries.add_method,
            entries.remove_method,
            entries.allocation_size,
            entries.behavior_of,
            entries.symbol_bytes,
        ]
        .iter()
        .all(|entry| entry.method.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validation_requires_all_entries_without_invoking_them() {
        unsafe extern "C" fn unused(
            _: *mut core::ffi::c_void,
            _: crate::Oop,
            _: *const Operand,
            _: usize,
        ) -> Outcome {
            panic!("admission must be inert")
        }
        let entry = NativeEntry::new(unused, core::ptr::null_mut());
        let complete = PrimitivesV1 {
            version: 1,
            byte_size: size_of::<PrimitivesV1>(),
            allocate: entry,
            intern: entry,
            lookup: entry,
            add_method: entry,
            remove_method: entry,
            allocation_size: entry,
            behavior_of: entry,
            symbol_bytes: entry,
        };
        assert!(unsafe { valid(&complete) });
        assert!(!unsafe { valid(core::ptr::null()) });
        #[repr(C)]
        struct Header {
            version: u32,
            byte_size: usize,
        }
        let old = Header {
            version: 1,
            byte_size: 48,
        };
        assert!(!unsafe { valid((&raw const old).cast()) });
        for index in 0..8 {
            let mut incomplete = complete;
            [
                &mut incomplete.allocate,
                &mut incomplete.intern,
                &mut incomplete.lookup,
                &mut incomplete.add_method,
                &mut incomplete.remove_method,
                &mut incomplete.allocation_size,
                &mut incomplete.behavior_of,
                &mut incomplete.symbol_bytes,
            ][index]
                .method = None;
            assert!(!unsafe { valid(&incomplete) }, "missing entry {index}");
        }
    }
}
