//! Compiler-owned optimization state, constructed by the later native entry.
use super::*;
use birnam_abi::runtime_compiler::ApiV1;
use core::ffi::c_void;

pub(super) struct Compiler {
    pub(super) native_tier: core::cell::RefCell<native_tier::Registry>,
    pub(super) numeric: core::cell::RefCell<numeric::Registry>,
    api: core::cell::OnceCell<ApiV1>,
}
impl Compiler {
    fn new() -> Self {
        Self { native_tier: Default::default(), numeric: Default::default(),
            api: Default::default() }
    }
}
pub(super) fn initial() -> core::cell::OnceCell<Compiler> {
    // Local harnesses exercise the complete Runtime; production uses the
    // separate native initialization request after completed values exist.
    #[cfg(any(test, feature = "ffi-test-hooks"))]
    { core::cell::OnceCell::from(Compiler::new()) }
    #[cfg(not(any(test, feature = "ffi-test-hooks")))]
    { core::cell::OnceCell::new() }
}
impl Runtime {
    pub(super) fn compiler(&self) -> &Compiler {
        self.compiler.get().expect("compiler extension was not initialized")
    }
    pub(crate) fn compiler_available(&self) -> bool { self.compiler.get().is_some() }
    pub(crate) fn initialize_compiler(&self) -> Option<*const ApiV1> {
        self.assert_owner();
        if self.integer.get().is_none() || self.float.get().is_none()
            || self.word.get().is_none() || self.string.get().is_none() { return None; }
        if !self.compiler_available() {
            unsafe { (*self.state.get()).admission = Some(CompilerAdmission::new()) };
            self.compiler.set(Compiler::new()).ok().expect("one compiler initialization");
        }
        Some(self.compiler_api())
    }
    pub(crate) fn compiler_uses_id(&self, id: &birnam_abi::id_protocol::Protocol) -> bool {
        let own = unsafe { (*self.state.get()).id_protocol };
        own.is_some_and(|own| own.context == id.context && own.receiver == id.receiver
            && own.message.map(|f| f as usize) == id.message.map(|f| f as usize))
    }
    pub(crate) fn compiler_api(&self) -> *const ApiV1 {
        self.assert_owner();
        self.compiler.get().map_or(core::ptr::null(), |compiler| compiler.api.get_or_init(|| ApiV1 {
            version: 1, byte_size: size_of::<ApiV1>(),
            context: core::ptr::from_ref(self).cast_mut().cast(), resolve: Some(resolve),
        }))
    }
}
unsafe extern "C" fn resolve(context: *mut c_void, name: *const u8, length: usize) -> *mut c_void {
    let runtime = unsafe { &*context.cast::<Runtime>() };
    runtime.assert_owner();
    let Some(name) = (unsafe { birnam_abi::borrowed_bytes(name, length) }) else { return core::ptr::null_mut(); };
    match name {
        b"birnam_register_resumable_v1" => crate::registration::birnam_register_resumable_v1 as *const () as *mut c_void,
        b"birnam_register_resumable_v2" => crate::registration::birnam_register_resumable_v2 as *const () as *mut c_void,
        b"birnam_id_begin_resumable_unit_v1" => crate::registration::birnam_id_begin_resumable_unit_v1 as *const () as *mut c_void,
        b"birnam_id_begin_portable_resumable_unit_v1" => crate::registration::birnam_id_begin_portable_resumable_unit_v1 as *const () as *mut c_void,
        b"birnam_id_finish_resumable_unit_v1" => crate::registration::birnam_id_finish_resumable_unit_v1 as *const () as *mut c_void,
        b"birnam_id_cancel_resumable_unit_v1" => crate::registration::birnam_id_cancel_resumable_unit_v1 as *const () as *mut c_void,
        b"birnam_id_try_acquire_stackless_generation_lease_v1" => crate::registration::birnam_id_try_acquire_stackless_generation_lease_v1 as *const () as *mut c_void,
        b"birnam_id_release_stackless_generation_lease_v1" => crate::registration::birnam_id_release_stackless_generation_lease_v1 as *const () as *mut c_void,
        b"birnam_id_retire_stackless_code_generation_v1" => crate::registration::birnam_id_retire_stackless_code_generation_v1 as *const () as *mut c_void,
        b"birnam_id_stackless_block_vtable_image_reference_v1" => crate::registration::birnam_id_stackless_block_vtable_image_reference_v1 as *const () as *mut c_void,
        b"birnam_id_stackless_block_vtable_matches_v1" => crate::registration::birnam_id_stackless_block_vtable_matches_v1 as *const () as *mut c_void,
        b"birnam_id_staged_portable_block_vtable_v1" => crate::registration::birnam_id_staged_portable_block_vtable_v1 as *const () as *mut c_void,
        b"birnam_id_stackless_resumable_manifest_matches_v1" => crate::registration::birnam_id_stackless_resumable_manifest_matches_v1 as *const () as *mut c_void,
        b"birnam_id_warm_block_construction_v1" => crate::birnam_id_warm_block_construction_v1 as *const () as *mut c_void,
        b"birnam_id_try_closure_elision_v2" => crate::birnam_id_try_closure_elision_v2 as *const () as *mut c_void,
        _ => core::ptr::null_mut(),
    }
}
