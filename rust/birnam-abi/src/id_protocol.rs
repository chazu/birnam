use crate::{Oop, is_fault, object_reference};
use core::ffi::c_void;
pub const VERSION: u32 = 2;
/// Version 3 declares the calling convention per Method and supplies the
/// continuation control primitive at initialization.
pub const CONTINUATION_MODULE_VERSION: u32 = 3;
pub type Method = unsafe extern "C" fn(Oop, Oop, Oop, *const Oop, usize) -> Oop;
pub type Message = unsafe extern "C" fn(Oop, Oop, Oop, *const Oop, usize) -> Oop;
pub type Initialize =
    unsafe extern "C" fn(*const Protocol, Oop, *const Oop, usize, *const Oop, usize) -> Oop;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Protocol {
    pub version: u32,
    pub byte_size: u32,
    pub message: Message,
    pub root: Oop,
    pub intern: Oop,
    pub load: Oop,
    pub at: Oop,
    pub seeds: Oop,
    pub invalid: Oop,
}
impl Protocol {
    /// A failing native return marks its object reference. Catching it removes
    /// that marker: the error can be stored, inspected or returned as data.
    /// Native entry points mark Err values again with `fault_reference`.
    /// # Safety
    /// Receiver, selector and every argument are live Oops from this runtime
    /// (including its valid immediates). The argument slice is readable and unchanged until
    /// return. Id protects these values during lookup, invocation and callbacks;
    /// callers protect other values they retain across subsequent sends.
    pub unsafe fn send(&self, r: Oop, s: Oop, a: &[Oop]) -> Result<Oop, Oop> {
        let v = unsafe { (self.message)(self.root, r, s, a.as_ptr(), a.len()) };
        if is_fault(v) {
            Err(object_reference(v))
        } else {
            Ok(v)
        }
    }
    /// Read byte values through the existing indexed-object messages. The
    /// receiver owns its representation; this helper never reads its payload.
    /// # Safety
    /// The receiver and selectors are live Oops in this runtime. Both selectors
    /// follow the sizeWord/atWord: contract and return immediate sizes/bytes.
    pub unsafe fn read_bytes(
        &self,
        receiver: Oop,
        size_selector: Oop,
        at_selector: Oop,
    ) -> Result<Vec<u8>, Oop> {
        unsafe {
            let invalid = object_reference(self.invalid);
            let count = crate::as_size(self.send(receiver, size_selector, &[])?).ok_or(invalid)?;
            let mut bytes = Vec::new();
            bytes.try_reserve_exact(count).map_err(|_| invalid)?;
            for i in 0..count {
                let value = self.send(receiver, at_selector, &[crate::size(i).unwrap()])?;
                bytes.push(
                    crate::as_size(value)
                        .and_then(|v| u8::try_from(v).ok())
                        .ok_or(invalid)?,
                );
            }
            Ok(bytes)
        }
    }
    pub unsafe fn literal(&self, index: usize) -> Result<Oop, Oop> {
        unsafe { self.send(self.seeds, self.at, &[crate::size(index).unwrap()]) }
    }
}
/// Fixed control entry for continuation declarations. `operation` is an interned
/// Oop naming prepareSend, prepareEntry, answerTo:with:, drive:budget: or pause:.
/// All supplied and returned values are Oops. Construction must not perform
/// lookup in order to return the result of lookup itself.
pub type Control = unsafe extern "C" fn(Oop, Oop, *const Oop, usize) -> Oop;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ContinuationProtocol {
    pub common: Protocol,
    pub control: Control,
}
pub type ContinuationInitialize = unsafe extern "C" fn(
    *const ContinuationProtocol,
    Oop,
    *const Oop,
    usize,
    *const Oop,
    usize,
) -> Oop;

// Descriptor records are immutable native object-file metadata, consumed once
// by the loader. Code pointers never travel through Id as values.
#[repr(C)]
pub struct Literal {
    pub bytes: *const u8,
    pub length: usize,
}
unsafe impl Sync for Literal {}
impl Literal {
    pub const fn new(b: &'static [u8]) -> Self {
        Self {
            bytes: b.as_ptr(),
            length: b.len(),
        }
    }
}
#[repr(C)]
pub struct MethodSpec {
    pub name: usize,
    pub entry: Option<Method>,
}
#[repr(C)]
pub struct Module {
    pub version: u32,
    pub byte_size: u32,
    pub literals: *const Literal,
    pub literal_count: usize,
    pub methods: *const MethodSpec,
    pub method_count: usize,
    pub initialize: Option<Initialize>,
}
unsafe impl Sync for Module {}
/// Calling convention is native declaration metadata, not an object kind.
pub const SYNCHRONOUS_ENTRY: u32 = 0;
pub const CONTINUATION_ENTRY: u32 = 1;
#[repr(C)]
pub struct ContinuationMethodSpec {
    pub name: usize,
    pub entry: Option<Method>,
    pub convention: u32,
}
/// Immutable v3 declarations may mix synchronous and continuation entries.
/// For continuation entries, arguments[0] is the destination; a successful
/// return is the next captured work Method, or nil to stop.
#[repr(C)]
pub struct ContinuationModule {
    pub version: u32,
    pub byte_size: u32,
    pub literals: *const Literal,
    pub literal_count: usize,
    pub methods: *const ContinuationMethodSpec,
    pub method_count: usize,
    pub initialize: Option<ContinuationInitialize>,
}
unsafe impl Sync for ContinuationModule {}

#[repr(C)]
pub struct Seed {
    pub bytes: *const u8,
    pub length: usize,
}
pub type Bootstrap = unsafe extern "C" fn(*const Seed, usize) -> *const Protocol;
const _: () = {
    assert!(size_of::<Oop>() == size_of::<*const c_void>());
};
