//! Optional native facts, obtained from and pinned to the actual loaded image.
use super::*;
use birnam_codegen::numeric_chain::Operator;
use core::ptr::NonNull;

#[repr(C)]
#[derive(Default)]
struct DlInfo {
    filename: *const c_char,
    base: *mut c_void,
    symbol_name: *const c_char,
    symbol_address: *mut c_void,
}

unsafe extern "C" {
    fn dladdr(address: *const c_void, info: *mut DlInfo) -> c_int;
    fn dlclose(handle: *mut c_void) -> c_int;
}

pub(super) struct ImagePin(NonNull<c_void>);
impl Drop for ImagePin {
    fn drop(&mut self) {
        // Every successful NOLOAD acquisition owns one reference. A selected
        // summary retains this pin even when its cache entry is evicted.
        unsafe { dlclose(self.0.as_ptr()) };
    }
}

pub(super) fn resident_declaration(
    method: crate::abi::Method,
    name: &std::ffi::CStr,
) -> Option<(ImagePin, *mut c_void)> {
    resident_address_declaration(method as *const c_void, name)
}

/// Constructor capabilities have their own signatures; discovery inspects the
/// address's owner without casting it to or invoking it as an Id Method.
pub(super) fn resident_address_declaration(
    address: *const c_void,
    name: &std::ffi::CStr,
) -> Option<(ImagePin, *mut c_void)> {
    let mut owner = DlInfo::default();
    if unsafe { dladdr(address, &mut owner) } == 0
        || owner.filename.is_null()
        || owner.base.is_null()
    {
        return None;
    }
    let image = ImagePin(NonNull::new(unsafe {
        system_dlopen(owner.filename, 0x1 | 0x4 | 0x10)
    })?);
    let declaration = unsafe { system_dlsym(image.0.as_ptr(), name.as_ptr()) };
    if declaration.is_null() {
        return None;
    }
    let mut declared_by = DlInfo::default();
    if unsafe { dladdr(declaration, &mut declared_by) } == 0 || declared_by.base != owner.base {
        return None;
    }
    Some((image, declaration))
}

pub(crate) struct NumericSummary {
    method: *const c_void,
    query: unsafe extern "C" fn(*const c_void, crate::abi::Oop, *const usize) -> u32,
    _image: ImagePin,
}

impl NumericSummary {
    /// The caller already owns a live Method through ordinary dispatch. Query
    /// only its resident image; never load a different file to obtain a fact.
    pub(crate) fn discover(
        method: crate::abi::Method,
        integer_vtable: crate::abi::Oop,
    ) -> Option<Self> {
        let address = method as *const c_void;
        let (image, declaration) =
            resident_declaration(method, c"birnam_native_numeric_semantics_v1")?;
        // A native image exporting this optional symbol promises the versioned
        // pure query contract, just as it promises its Method/TraceHook ABI.
        let query: unsafe extern "C" fn(*const c_void, crate::abi::Oop, *const usize) -> u32 =
            unsafe { core::mem::transmute(declaration) };
        let summary = Self {
            method: address,
            query,
            _image: image,
        };
        summary.operator(integer_vtable)?;
        Some(summary)
    }

    pub(crate) fn operator(&self, integer_vtable: crate::abi::Oop) -> Option<Operator> {
        // A module may have an instrumented/copied IdApi table. Revalidate
        // every invocation rather than caching assumptions about its pointers.
        let functions = crate::public_api::numeric_functions();
        Operator::from_declaration(unsafe {
            (self.query)(self.method, integer_vtable, functions.as_ptr())
        })
    }
}
