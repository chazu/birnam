use crate::{Oop, is_fault, object_reference};
use core::ffi::c_void;
pub const VERSION: u32 = 2;
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
        if is_fault(v) { Err(object_reference(v)) } else { Ok(v) }
    }
    pub unsafe fn literal(&self, index: usize) -> Result<Oop, Oop> {
        unsafe { self.send(self.seeds, self.at, &[crate::size(index).unwrap()]) }
    }
}
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
#[repr(C)]
pub struct Seed {
    pub bytes: *const u8,
    pub length: usize,
}
pub type Bootstrap = unsafe extern "C" fn(*const Seed, usize) -> *const Protocol;
const _: () = {
    assert!(size_of::<Oop>() == size_of::<*const c_void>());
};
