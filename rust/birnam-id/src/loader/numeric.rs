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

pub(crate) fn integer_native_construction(ops: &birnam_abi::integer::IntegerApiV1)
    -> Option<birnam_abi::integer_native::ConstructionV1>
{
    let (_pin, declaration) = resident_address_declaration(ops.make as *const c_void,
        c"birnam_integer_construction_v1")?;
    let query: birnam_abi::integer_native::ConstructionQueryV1 = unsafe { core::mem::transmute(declaration) };
    let mut facts = core::mem::MaybeUninit::uninit();
    if unsafe { query(ops, facts.as_mut_ptr()) } != 1 { return None; }
    // The Integer dependency contract retains the constructor, context and
    // declared dependencies for the module lifetime; only copied facts escape.
    Some(unsafe { facts.assume_init() })
}

pub(crate) fn namespace_lookup_declared(entry: crate::abi::id_protocol::NativeEntry,
    owner: &birnam_abi::namespace_owner::OwnedV1, id: &birnam_abi::id_protocol::Protocol) -> bool
{
    let Some(method) = entry.method else { return false; };
    let Some((_pin, declaration)) = resident_address_declaration(method as *const c_void,
        c"birnam_namespace_lookup_semantics_v1") else { return false; };
    let query: unsafe extern "C" fn(crate::abi::id_protocol::NativeEntry,
        *const birnam_abi::namespace_owner::OwnedV1, *const birnam_abi::id_protocol::Protocol) -> i32
        = unsafe { core::mem::transmute(declaration) };
    unsafe { query(entry, owner, id) == 1 }
}

enum Query {
    Legacy(unsafe extern "C" fn(*const c_void, crate::abi::Oop, *const usize) -> u32),
    Native(birnam_abi::integer_native::NumericQueryV5),
}
pub(crate) struct NumericSummary {
    entry: crate::abi::id_protocol::NativeEntry,
    query: Query,
    _image: ImagePin,
}

impl NumericSummary {
    #[cfg(test)]
    pub(crate) fn discover(method: crate::abi::Method, runtime: &crate::runtime::Runtime) -> Option<Self> {
        Self::discover_native(crate::legacy_method::encode(method, false), runtime)
    }
    pub(crate) fn discover_native(entry: crate::abi::id_protocol::NativeEntry,
        runtime: &crate::runtime::Runtime) -> Option<Self>
    {
        let (image, query) = if let Some(legacy) = crate::legacy_method::try_decode(entry) {
            let (image, declaration) = resident_declaration(legacy.to_method(), c"birnam_native_numeric_semantics_v4")?;
            (image, Query::Legacy(unsafe { core::mem::transmute(declaration) }))
        } else {
            let (image, declaration) = resident_address_declaration(entry.method? as *const c_void,
                c"birnam_native_numeric_semantics_v5")?;
            (image, Query::Native(unsafe { core::mem::transmute(declaration) }))
        };
        let summary = Self { entry, query, _image: image };
        summary.operator(runtime)?;
        Some(summary)
    }
    pub(crate) fn operator(&self, runtime: &crate::runtime::Runtime) -> Option<Operator> {
        if !runtime.integer_context_current() { return None; }
        let code = match self.query {
            Query::Legacy(query) => {
                let method = crate::legacy_method::try_decode(self.entry)?;
                let functions = crate::public_api::numeric_functions(runtime);
                unsafe { query(method.to_method() as *const c_void, runtime.integer_behavior(), functions.as_ptr()) }
            }
            Query::Native(query) => {
                if !runtime.integer_native_numeric_current() { return None; }
                unsafe { query(self.entry, &runtime.integer_operations()) }
            }
        };
        Operator::from_declaration(code)
    }
}
