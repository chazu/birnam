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
    let provider = unsafe { dlsym(image, c"birnam_namespace_provider_v8".as_ptr()) };
    let dependency = unsafe { dlsym(floor, c"birnam_namespace_dependency_v8".as_ptr()) };
    assert!(!provider.is_null() && !dependency.is_null());
    let provider: birnam_abi::namespace::ProviderV8 = unsafe { std::mem::transmute(provider) };
    let attach: birnam_abi::namespace::DependencyV8 = unsafe { std::mem::transmute(dependency) };
    assert_eq!(unsafe { attach(provider()) }, 1);
    let path = CString::new(floor_path.with_file_name("libbirnam_object_model.dylib").as_os_str().as_encoded_bytes()).unwrap();
    let image = unsafe { dlopen(path.as_ptr(), 6) };
    assert!(!image.is_null());
    let provider = unsafe { dlsym(image, c"birnam_vtable_enumeration_factory_v2".as_ptr()) };
    let dependency = unsafe { dlsym(floor, c"birnam_vtable_enumeration_dependency_v2".as_ptr()) };
    assert!(!provider.is_null() && !dependency.is_null());
    let provider: birnam_abi::vtable_enumeration::FactoryProviderV2 = unsafe { std::mem::transmute(provider) };
    let attach: birnam_abi::vtable_enumeration::DependencyV2 = unsafe { std::mem::transmute(dependency) };
    assert_eq!(unsafe { attach(provider()) }, 1);
    let path = CString::new(floor_path.with_file_name("libbirnam_method_observation.dylib").as_os_str().as_encoded_bytes()).unwrap();
    let image = unsafe { dlopen(path.as_ptr(), 6) };
    assert!(!image.is_null());
    let provider = unsafe { dlsym(image, c"birnam_method_observation_factory_v1".as_ptr()) };
    let dependency = unsafe { dlsym(floor, c"birnam_method_observation_dependency_v1".as_ptr()) };
    assert!(!provider.is_null() && !dependency.is_null());
    let provider: birnam_abi::method_observation::FactoryProviderV1 = unsafe { std::mem::transmute(provider) };
    let attach: birnam_abi::method_observation::DependencyV1 = unsafe { std::mem::transmute(dependency) };
    assert_eq!(unsafe { attach(provider()) }, 1);
    let path = CString::new(floor_path.with_file_name("libbirnam_root_registry.dylib").as_os_str().as_encoded_bytes()).unwrap();
    let image = unsafe { dlopen(path.as_ptr(), 6) };
    assert!(!image.is_null());
    let provider = unsafe { dlsym(image, c"birnam_root_registry_factory_v1".as_ptr()) };
    let dependency = unsafe { dlsym(floor, c"birnam_root_registry_dependency_v1".as_ptr()) };
    assert!(!provider.is_null() && !dependency.is_null());
    let provider: birnam_abi::root_registry::FactoryProviderV1 = unsafe { std::mem::transmute(provider) };
    let attach: birnam_abi::root_registry::DependencyV1 = unsafe { std::mem::transmute(dependency) };
    assert_eq!(unsafe { attach(provider()) }, 1);
    let path = CString::new(floor_path.with_file_name("libbirnam_gc_settings.dylib").as_os_str().as_encoded_bytes()).unwrap();
    let image = unsafe { dlopen(path.as_ptr(), 6) };
    assert!(!image.is_null());
    let provider = unsafe { dlsym(image, c"birnam_gc_settings_factory_v1".as_ptr()) };
    let dependency = unsafe { dlsym(floor, c"birnam_gc_settings_dependency_v1".as_ptr()) };
    assert!(!provider.is_null() && !dependency.is_null());
    let provider: birnam_abi::gc_settings::FactoryProviderV1 = unsafe { std::mem::transmute(provider) };
    let attach: birnam_abi::gc_settings::DependencyV1 = unsafe { std::mem::transmute(dependency) };
    assert_eq!(unsafe { attach(provider()) }, 1);
    let path = CString::new(floor_path.with_file_name("libbirnam_payload_layout.dylib").as_os_str().as_encoded_bytes()).unwrap();
    let image = unsafe { dlopen(path.as_ptr(), 6) };
    assert!(!image.is_null());
    let provider = unsafe { dlsym(image, c"birnam_payload_layout_factory_v1".as_ptr()) };
    let dependency = unsafe { dlsym(floor, c"birnam_payload_layout_dependency_v1".as_ptr()) };
    assert!(!provider.is_null() && !dependency.is_null());
    let provider: birnam_abi::payload_layout::FactoryProviderV1 = unsafe { std::mem::transmute(provider) };
    let attach: birnam_abi::payload_layout::DependencyV1 = unsafe { std::mem::transmute(dependency) };
    assert_eq!(unsafe { attach(provider()) }, 1);
    let path = CString::new(floor_path.with_file_name("libbirnam_trace_hooks.dylib").as_os_str().as_encoded_bytes()).unwrap();
    let image = unsafe { dlopen(path.as_ptr(), 6) };
    assert!(!image.is_null());
    let provider = unsafe { dlsym(image, c"birnam_trace_hooks_factory_v1".as_ptr()) };
    let dependency = unsafe { dlsym(floor, c"birnam_trace_hooks_dependency_v1".as_ptr()) };
    assert!(!provider.is_null() && !dependency.is_null());
    let provider: birnam_abi::trace_hooks::FactoryProviderV1 = unsafe { std::mem::transmute(provider) };
    let attach: birnam_abi::trace_hooks::DependencyV1 = unsafe { std::mem::transmute(dependency) };
    assert_eq!(unsafe { attach(provider()) }, 1);
    let path = CString::new(floor_path.with_file_name("libbirnam_finalizers.dylib").as_os_str().as_encoded_bytes()).unwrap();
    let image = unsafe { dlopen(path.as_ptr(), 6) };
    assert!(!image.is_null());
    let provider = unsafe { dlsym(image, c"birnam_finalizers_factory_v1".as_ptr()) };
    let dependency = unsafe { dlsym(floor, c"birnam_finalizers_dependency_v1".as_ptr()) };
    assert!(!provider.is_null() && !dependency.is_null());
    let provider: birnam_abi::finalizers::FactoryProviderV1 = unsafe { std::mem::transmute(provider) };
    let attach: birnam_abi::finalizers::DependencyV1 = unsafe { std::mem::transmute(dependency) };
    assert_eq!(unsafe { attach(provider()) }, 1);
    let path = CString::new(floor_path.with_file_name("libbirnam_native_stack.dylib").as_os_str().as_encoded_bytes()).unwrap();
    let image = unsafe { dlopen(path.as_ptr(), 6) };
    assert!(!image.is_null());
    let provider = unsafe { dlsym(image, c"birnam_native_stack_factory_v1".as_ptr()) };
    let dependency = unsafe { dlsym(floor, c"birnam_native_stack_dependency_v1".as_ptr()) };
    assert!(!provider.is_null() && !dependency.is_null());
    let provider: birnam_abi::native_stack::FactoryProviderV1 = unsafe { std::mem::transmute(provider) };
    let attach: birnam_abi::native_stack::DependencyV1 = unsafe { std::mem::transmute(dependency) };
    assert_eq!(unsafe { attach(provider()) }, 1);
    // The process-owned runtime retains this descriptor and code.
}
