// Native test setup, shared by tests which initialize the real selected floor.
use std::ffi::{CString, c_char, c_int, c_void};
use std::path::Path;
unsafe extern "C" {
    fn dlopen(path: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
}
pub unsafe fn prepare(floor: *mut c_void, floor_path: &Path) {
    let path = CString::new(
        floor_path
            .with_file_name("libbirnam_namespace.dylib")
            .as_os_str()
            .as_encoded_bytes(),
    )
    .unwrap();
    let image = unsafe { dlopen(path.as_ptr(), 6) };
    assert!(!image.is_null());
    let provider = unsafe { dlsym(image, c"birnam_namespace_provider_v1".as_ptr()) };
    let dependency = unsafe { dlsym(floor, c"birnam_namespace_dependency_v1".as_ptr()) };
    assert!(!provider.is_null() && !dependency.is_null());
    let provider: birnam_abi::namespace::ProviderV1 = unsafe { std::mem::transmute(provider) };
    let attach: birnam_abi::namespace::DependencyV1 = unsafe { std::mem::transmute(dependency) };
    assert_eq!(unsafe { attach(provider()) }, 1);
    // The process-owned runtime retains this descriptor and code.
}
