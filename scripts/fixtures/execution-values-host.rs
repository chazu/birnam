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
    let mut initial = birnam_host::begin_id(&resources.id_dylib, None, None).unwrap();
    let values = initial.initialize_runtime_values().unwrap();
    let floor = initial.finish().unwrap();
    // These completed lower modules must not require a second legacy load.
    // The fixture owns this copied prefix; Id already retains the loaded code.
    for library in ["namespace", "float", "word", "object_model", "root_registry",
                    "gc_settings", "payload_layout", "trace_hooks", "finalizers",
                    "native_stack", "native_mark", "allocation_budget", "nil", "string"] {
        let path = prefix.join(format!("libbirnam_{library}.dylib"));
        std::fs::rename(&path, path.with_extension("constructed")).unwrap();
    }
    let mut stack = LoadedStack::new(floor);
    let runtime_path = prefix.join("libbirnam_runtime.dylib");
    let arguments = [
        Operand::word((stack.api() as *const birnam_abi::IdApi).addr()),
        Operand::word(stack.imports().addr()), Operand::word(stack.root_registry().addr()),
        Operand::word(values.integer.addr()), Operand::word(values.string.addr()),
        Operand::bytes(runtime_path.as_os_str().as_bytes()), Operand::word(0),
    ];
    assert_eq!(unsafe { stack.load_native(Path::new(&args[2]).as_os_str().as_bytes(), &arguments) }.status, native::LOADED);
    println!("Runtime Machine works immediately at construction, before legacy module entries");
    let later = ["string_builder", "collection_value", "vector", "range", "map", "string_collection", "id_collection", "store"];
    for library in later {
        let path = prefix.join(format!("libbirnam_{library}.dylib"));
        std::fs::rename(&path, path.with_extension("later")).unwrap();
    }
    stack.initialize_control().unwrap();
    assert_eq!(stack.module_count(), 0, "control used legacy module initialization");
    let mut control_arguments = arguments;
    control_arguments[6] = Operand::word(2);
    assert_eq!(unsafe { stack.load_native(Path::new(&args[2]).as_os_str().as_bytes(), &control_arguments) }.status, native::LOADED);
    println!("native control and printing work while collections, Store and compiler libraries are absent");
    if args.get(5).is_some_and(|mode| mode == "system") {
        for library in ["id_prelude", "text_view", "system_gc", "system_output", "system_input", "environment", "file_io", "native_loader"] {
            let file = format!("libbirnam_{library}.dylib");
            std::fs::copy(Path::new(&args[4]).join(&file), prefix.join(&file)).unwrap();
        }
        unsafe {
            std::env::set_var("BIRNAM_83_SYSTEM_VALUE", "native-system");
            std::env::set_var("BIRNAM_83_SYSTEM_FILE", prefix.join("system-io.txt"));
        }
        stack.initialize_system_io().unwrap();
        assert_eq!(stack.module_count(), 0, "System IO used legacy initialization");
        let system = unsafe { birnam_abi::namespace_import::import(stack.imports(), c"System".as_ptr()) };
        control_arguments[6] = Operand::word(6);
        assert_eq!(unsafe { stack.load_native(Path::new(&args[2]).as_os_str().as_bytes(), &control_arguments) }.status, native::LOADED);
        stack.initialize_system_io().unwrap();
        assert_eq!(unsafe { birnam_abi::namespace_import::import(stack.imports(), c"System".as_ptr()) }, system);
        println!("System IO, GC, native-load failure and environment/file operations work before collections, Store, parsers and compilers; source loading is absent");
        return;
    }
    for library in later {
        let path = prefix.join(format!("libbirnam_{library}.dylib"));
        std::fs::rename(path.with_extension("later"), &path).unwrap();
    }
    let store_path = prefix.join("libbirnam_store.dylib");
    std::fs::rename(&store_path, store_path.with_extension("later")).unwrap();
    stack.initialize_collections().unwrap();
    assert_eq!(stack.module_count(), 0, "collections used legacy initialization");
    let mut collection_arguments = arguments;
    collection_arguments[6] = Operand::word(3);
    assert_eq!(unsafe { stack.load_native(Path::new(&args[2]).as_os_str().as_bytes(), &collection_arguments) }.status, native::LOADED);
    println!("complete native collections work before Store and all parser/compiler libraries");
    std::fs::rename(store_path.with_extension("later"), &store_path).unwrap();
    stack.initialize_store().unwrap();
    assert_eq!(stack.module_count(), 0, "Store used legacy initialization");
    if args.len() > 4 {
        for library in ["id_artifact", "id_peg", "id_lang"] {
            let file = format!("libbirnam_{library}.dylib");
            std::fs::copy(Path::new(&args[4]).join(&file), prefix.join(&file)).unwrap();
        }
        stack.initialize_parsers().unwrap();
        assert_eq!(stack.module_count(), 0, "parsers used legacy initialization");
        let mut parser_arguments = arguments;
        parser_arguments[6] = Operand::word(4);
        assert_eq!(unsafe { stack.load_native(Path::new(&args[2]).as_os_str().as_bytes(), &parser_arguments) }.status, native::LOADED);
        let grammar = unsafe { birnam_abi::namespace_import::import(stack.imports(), c"BirnamGrammar".as_ptr()) };
        stack.initialize_parsers().unwrap();
        assert_eq!(unsafe { birnam_abi::namespace_import::import(stack.imports(), c"BirnamGrammar".as_ptr()) }, grammar);
        println!("Artifact, PEG and Lang work with TCC, Codegen and runtime compiler admission absent; repeated construction preserves the grammar");
        if args.len() > 5 {
            for library in ["id_tcc", "portable_code", "id_codegen"] {
                let file = format!("libbirnam_{library}.dylib");
                std::fs::copy(Path::new(&args[4]).join(&file), prefix.join(&file)).unwrap();
            }
            let selected_tcc = prefix.join("selected-tcc.dylib");
            std::fs::rename(prefix.join("libbirnam_id_tcc.dylib"), &selected_tcc).unwrap();
            unsafe { std::env::set_var("BIRNAM_TCC_DYLIB", &selected_tcc); }
            stack.initialize_compiler().unwrap();
            assert_eq!(stack.module_count(), 0, "compiler used legacy initialization");
            parser_arguments[6] = Operand::word(5);
            assert_eq!(unsafe { stack.load_native(Path::new(&args[2]).as_os_str().as_bytes(), &parser_arguments) }.status, native::LOADED);
            stack.initialize_compiler().unwrap();
            println!("TCC and Codegen execute native/source compilation with Portable Code and Store compensation already available, before System and Image libraries");
            for library in ["id_prelude", "text_view", "system_gc", "system_output", "system_input", "environment", "file_io", "native_loader", "source_loader", "id_isolate", "id_relational", "id_alien"] {
                let file = format!("libbirnam_{library}.dylib");
                std::fs::copy(Path::new(&args[4]).join(&file), prefix.join(&file)).unwrap();
            }
            stack.initialize_application_base().unwrap();
            assert_eq!(stack.module_count(), 0, "application base used legacy initialization");
            parser_arguments[6] = Operand::word(7);
            assert_eq!(unsafe { stack.load_native(Path::new(&args[2]).as_os_str().as_bytes(), &parser_arguments) }.status, native::LOADED);
            std::fs::copy(Path::new(&args[4]).join("libbirnam_id_image.dylib"), prefix.join("libbirnam_id_image.dylib")).unwrap();
            stack.load_script_modules(&resources).unwrap();
            assert_eq!(stack.module_count(), 0, "native startup used legacy initialization");
            parser_arguments[6] = Operand::word(8);
            let result = unsafe { stack.load_native(Path::new(&args[2]).as_os_str().as_bytes(), &parser_arguments) };
            assert_eq!(result.status, native::LOADED);
            stack.load_script_modules(&resources).unwrap();
            let repeated = unsafe { stack.load_native(Path::new(&args[2]).as_os_str().as_bytes(), &parser_arguments) };
            assert_eq!(repeated.value.as_object(), result.value.as_object());
            println!("all native application operations mount through Bootstrap with zero legacy module initializations, before authored modules; Image repeats preserve its report");
        }
        return;
    }
    match stack.load_script_modules(&resources) {
        Err(ModuleError::NativeConstruction { stage, status, .. }) => { assert_eq!(stage, "parsers"); assert_ne!(status, native::LOADED); },
        other => panic!("expected completed native prefix before missing Artifact, got {other:?}"),
    }
    // dyld can reuse an already mapped image even after its file was moved.
    // Missing native parsers stop construction before any legacy entry runs.
    assert_eq!(stack.module_count(), 0, "completed lower modules initialized again");
    unsafe { birnam_abi::gc_settings::set_precise(stack.gc_settings(), 1) };
    let mut arguments = arguments;
    arguments[6] = Operand::word(1);
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
    for mode in [1, 2, 3, 4] {
        let library = scratch.join(format!("block-loop-{mode}.dylib"));
        let reentry = [arguments[0], Operand::word(stack.presence().addr()), arguments[1],
            Operand::word(error.addr()), Operand::bytes(library.as_os_str().as_bytes()), Operand::word(mode)];
        let result = unsafe { stack.load_native(scratch.join("block-loop-reentry.dylib").as_os_str().as_bytes(), &reentry) };
        assert_eq!(result.status, native::LOADED);
    }
    println!("Block Loop retains its original execution table; native construction rejects nested entry and repeats its completed result");
    println!("native Block, Boolean control, collections, Store and Machine work before parser/compiler modules, including errors, captures and GC; completed lower modules are not reopened");
}
