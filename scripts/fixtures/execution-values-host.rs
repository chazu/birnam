//! Stop actual production startup at the first unavailable compiler module.
use birnam_abi::id_protocol::{self as native, Operand};
use birnam_host::{LoadedStack, ModuleError, Overrides};
use std::{path::Path, os::unix::ffi::OsStrExt};
fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    let prefix = Path::new(&args[1]);
    for library in ["id_artifact", "id_peg", "id_lang", "id_tcc", "id_codegen", "id_prelude", "portable_code", "id_image"] {
        assert!(!prefix.join(format!("libbirnam_{library}.dylib")).exists());
    }
    let resources = birnam_host::resolve_from_exe_dir(prefix, Overrides { lib_dir: Some(prefix), ..Overrides::default() }).unwrap();
    let floor = birnam_host::begin_id(&resources.id_dylib, None, None).unwrap().finish().unwrap();
    let mut stack = LoadedStack::new(floor);
    match stack.load_script_modules(&resources) {
        Err(ModuleError::OpenFailed { path }) => assert_eq!(path, prefix.join("libbirnam_id_artifact.dylib")),
        other => panic!("expected completed native prefix before missing Artifact, got {other:?}"),
    }
    unsafe { birnam_abi::gc_settings::set_precise(stack.gc_settings(), 1) };
    let integer = unsafe { birnam_abi::integer::operations() };
    let string = unsafe { birnam_abi::string::operations() };
    let arguments = [
        Operand::word((stack.api() as *const birnam_abi::IdApi).addr()),
        Operand::word(stack.imports().addr()), Operand::word(stack.root_registry().addr()),
        Operand::word((&integer as *const birnam_abi::integer::IntegerApiV1).addr()),
        Operand::word((&string as *const birnam_abi::string::StringApiV1).addr()),
    ];
    let result = unsafe { stack.load_native(Path::new(&args[2]).as_os_str().as_bytes(), &arguments) };
    assert_eq!(result.status, native::LOADED);
    let original = result.value.as_object().unwrap();
    stack.load_module(&prefix.join("libbirnam_block_loop.dylib")).unwrap();
    assert_eq!(unsafe { birnam_abi::namespace_import::import(stack.imports(), c"Block".as_ptr()) }, original);
    let scratch = Path::new(&args[3]);
    let path = std::ffi::CString::new(prefix.join("libbirnam_error_value.dylib").as_os_str().as_bytes()).unwrap();
    let image = unsafe { (stack.api().dlopen)(path.as_ptr(), 2) };
    assert!(!image.is_null());
    let provider = unsafe { (stack.api().dlsym)(image, c"birnam_error_value_provider_v3".as_ptr()) };
    assert!(!provider.is_null());
    let provider: birnam_abi::error_value::ProviderV3 = unsafe { std::mem::transmute(provider) };
    let error = unsafe { provider() };
    for mode in [1, 2] {
        let library = scratch.join(format!("block-loop-{mode}.dylib"));
        let reentry = [arguments[0], Operand::word(stack.presence().addr()), arguments[1],
            Operand::word(error.addr()), Operand::bytes(library.as_os_str().as_bytes()), Operand::word(mode)];
        let result = unsafe { stack.load_native(scratch.join("block-loop-reentry.dylib").as_os_str().as_bytes(), &reentry) };
        assert_eq!(result.status, native::LOADED);
    }
    println!("Block Loop retains the original execution table through first intern and binding reentry");
    println!("native Block, Boolean control, collections, Store and Machine work before parser/compiler modules, including errors, captures and GC");
}
