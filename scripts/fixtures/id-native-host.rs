//! The installed production Id first, then the production host adopting it.
use birnam_abi::id_protocol::{self as native, Operand, Outcome, Protocol};
use birnam_host::{Overrides, StartupPolicy};
use std::{
    os::unix::{ffi::OsStrExt, fs::symlink},
    path::Path,
};

fn send(id: &Protocol, selector: *mut core::ffi::c_void, args: &[Operand]) -> Outcome {
    unsafe { id.message.unwrap()(id.context, id.receiver, selector, args.as_ptr(), args.len()) }
}
fn intern(id: &Protocol, name: &[u8]) -> *mut core::ffi::c_void {
    let answer = send(id, id.intern, &[Operand::bytes(name)]);
    assert_eq!(answer.status, native::LOADED);
    answer.value.as_object().unwrap()
}
fn load(id: &Protocol, path: &Path, name: &[u8]) -> Outcome {
    send(
        id,
        id.load,
        &[
            Operand::bytes(path.as_os_str().as_bytes()),
            Operand::bytes(name),
            Operand::word(0x83),
        ],
    )
}
unsafe extern "C" fn collect(
    context: *mut core::ffi::c_void,
    _: *mut core::ffi::c_void,
    _: *const Operand,
    count: usize,
) -> Outcome {
    assert_eq!(count, 0);
    let api = unsafe { &*context.cast::<birnam_abi::IdApi>() };
    unsafe { (api.gcCollect)() };
    Outcome::loaded(Operand::unit())
}
fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    assert_eq!(args.len(), 3, "PREFIX SCRATCH");
    let prefix = Path::new(&args[1]);
    let scratch = Path::new(&args[2]);
    let isolated = scratch.join("isolated");
    let id_path = isolated.join("libbirnam_id.dylib");
    assert_eq!(
        std::fs::read_dir(&isolated).unwrap().count(),
        1,
        "only the actual Id is available"
    );
    let id = birnam_host::siphon::load(id_path.as_os_str().as_bytes()).unwrap();
    let primitives = unsafe { birnam_abi::id_primitives::select(id) }.unwrap_or_else(|_| panic!("initial primitives"));
    let early = intern(&id, b"earlyNativeEcho:");
    assert_eq!(
        load(&id, &scratch.join("0.dylib"), b"earlyNativeEcho:")
            .value
            .as_object(),
        Some(early)
    );
    assert_eq!(
        send(&id, early, &[Operand::word(83)]).value.as_word(),
        Some(83)
    );
    // Save the actual primitive entries, then call them after message() has
    // returned. They must not depend on a hidden active Id scope.
    let lookup_name = intern(&id, b"lookup");
    let selected = |name| send(&id, lookup_name, &[Operand::object(intern(&id, name))]).value.as_entry().unwrap();
    let saved_lookup = selected(b"lookup");
    let saved_add = selected(b"addMethod");
    let saved_remove = selected(b"removeMethod:");
    let invoke = |entry: native::NativeEntry, arguments: &[Operand]| unsafe {
        entry.method.unwrap()(entry.context, id.receiver, arguments.as_ptr(), arguments.len())
    };
    let echo = invoke(saved_lookup, &[Operand::object(early)]).value.as_entry().unwrap();
    let alias = intern(&id, b"savedEntryAlias:");
    assert_eq!(invoke(saved_add, &[Operand::object(alias), Operand::entry(echo)]).value.as_entry(), Some(echo));
    assert_eq!(send(&id, alias, &[Operand::word(86)]).value.as_word(), Some(86));
    assert_eq!(invoke(saved_remove, &[Operand::object(alias)]).value.as_entry(), Some(echo));
    assert_eq!(invoke(saved_lookup, &[Operand::object(alias)]).value.kind(), native::UNIT);
    println!("saved Id primitives invoke outside message scope with their selected context");
    let failed = load(&id, &scratch.join("1.dylib"), b"failedNativeEcho:");
    assert_eq!(
        (failed.status, failed.detail),
        (native::INITIALIZER_FAILED, 42)
    );
    let failed_selector = intern(&id, b"failedNativeEcho:");
    assert_eq!(
        send(&id, failed_selector, &[Operand::word(84)])
            .value
            .as_word(),
        Some(84)
    );
    for (mode, status) in [
        (2, native::MISSING_ENTRY),
        (3, native::INCOMPATIBLE_ABI),
        (4, native::INCOMPATIBLE_ABI),
        (5, native::INCOMPATIBLE_ABI),
    ] {
        assert_eq!(
            load(&id, &scratch.join(format!("{mode}.dylib")), b"unused").status,
            status
        );
    }
    assert_eq!(
        load(&id, &scratch.join("missing.dylib"), b"unused").status,
        native::MAPPING_FAILED
    );
    for path in [&b""[..], &b"bad\0path"[..]] {
        assert_eq!(
            send(&id, id.load, &[Operand::bytes(path)]).status,
            native::INVALID_REQUEST
        );
    }
    let log = scratch.join("unloaded");
    assert_eq!(std::fs::read_to_string(&log).unwrap(), "2\n3\n4\n5\n");
    println!(
        "actual Id loads and sends without upper libraries; admission and failure pinning pass"
    );

    // Bootstrap is the first upper module. Its nested initializer result lives
    // in the early root set before a root-registry library is even available.
    symlink(prefix.join("lib/libbirnam_bootstrap.dylib"), isolated.join("libbirnam_bootstrap.dylib")).unwrap();
    let mut initial = birnam_host::begin_id(&id_path, None, None).unwrap();
    let bootstrap = initial.bootstrap().unwrap();
    let bootstrap_path = scratch.join("bootstrap-result-1.dylib");
    let object_path = scratch.join("bootstrap-result-0.dylib");
    let bootstrap_args = [Operand::word(bootstrap as usize), Operand::bytes(object_path.as_os_str().as_bytes()), Operand::bytes(id_path.as_os_str().as_bytes())];
    let retained = unsafe { initial.load_native(bootstrap_path.as_os_str().as_bytes(), &bootstrap_args) };
    assert_eq!(retained.status, native::LOADED);
    let bootstrap_identity = retained.value.as_word().unwrap();
    let bootstrap_query = intern(&id, b"bootstrapResultIdentity");
    let bootstrap_release = intern(&id, b"releaseBootstrapResult");
    assert_eq!(std::fs::read_dir(&isolated).unwrap().count(), 2);
    assert_eq!(unsafe { initial.load_native(b"/missing/bootstrap/module.dylib", &[]) }.status, native::MAPPING_FAILED);
    // Construct the real root owner and explicitly adopt Bootstrap's live set.
    symlink(prefix.join("lib/libbirnam_root_registry.dylib"), isolated.join("libbirnam_root_registry.dylib")).unwrap();
    let root_path = isolated.join("libbirnam_root_registry.dylib");
    assert_eq!(send(&id, id.load, &[Operand::bytes(root_path.as_os_str().as_bytes()), Operand::word(83)]).status, native::INVALID_REQUEST);
    initial.initialize_roots().unwrap();
    assert_eq!(initial.id_protocol().context, id.context);
    let roots = initial.roots().expect("production initialized root owner");
    let root_path = scratch.join("early-root.dylib");
    let retained = unsafe { initial.load_native(root_path.as_os_str().as_bytes(), &[Operand::word(roots as usize)]) };
    assert_eq!(retained.status, native::LOADED);
    assert!(retained.value.as_word().unwrap() > 0);
    let root_identity = intern(&id, b"earlyRootIdentity");
    let release_root = intern(&id, b"releaseEarlyRoot");
    let early_identity = send(&id, root_identity, &[]).value.as_word().unwrap();
    assert_eq!(std::fs::read_dir(&isolated).unwrap().count(), 3);
    // Complete Lifetime with only Id, roots and the three primitive record
    // algorithms present; every value/collector/Runtime dependency is absent.
    for name in ["payload_layout", "trace_hooks", "finalizers", "lifetime"] {
        let name = format!("libbirnam_{name}.dylib");
        symlink(prefix.join("lib").join(&name), isolated.join(&name)).unwrap();
    }
    let lifetime_path = isolated.join("libbirnam_lifetime.dylib");
    assert_eq!(send(&id, id.load, &[Operand::bytes(lifetime_path.as_os_str().as_bytes())]).status, native::INVALID_REQUEST);
    let records = initial.initialize_lifetime().unwrap().unwrap();
    let path = scratch.join("early-lifetime.dylib");
    assert_eq!(unsafe { initial.load_native(path.as_os_str().as_bytes(), &[Operand::word(roots as usize), Operand::word(records as usize)]) }.status, native::LOADED);
    let lifetime_check = intern(&id, b"earlyLifetimeCheck");
    let lifetime_release = intern(&id, b"releaseEarlyLifetime");
    assert_eq!(std::fs::read_dir(&isolated).unwrap().count(), 7);
    // Base values work with no String, Integer, Block or Runtime available.
    for name in ["boolean", "nil"] {
        let file = format!("libbirnam_{name}.dylib");
        symlink(prefix.join("lib").join(&file), isolated.join(&file)).unwrap();
        let path = isolated.join(&file);
        assert_eq!(unsafe { initial.load_native(path.as_os_str().as_bytes(), &[]) }.status, native::INVALID_REQUEST);
    }
    let values = initial.initialize_base_values().unwrap().unwrap();
    assert_eq!(initial.initialize_base_values().unwrap(), Some(values));
    let path = scratch.join("base-values.dylib");
    assert_eq!(unsafe { initial.load_native(path.as_os_str().as_bytes(), &[Operand::word(values as usize), Operand::word(records as usize)]) }.status, native::LOADED);
    // Loading completed base entries again must preserve live replacements.
    let boolean_path = isolated.join("libbirnam_boolean.dylib");
    let repeated = unsafe { initial.load_native(boolean_path.as_os_str().as_bytes(), &[Operand::word(roots as usize), Operand::word(records as usize)]) };
    assert_eq!(repeated.status, native::LOADED);
    assert_eq!(repeated.value.as_word(), Some(values as usize));
    let nil_path = isolated.join("libbirnam_nil.dylib");
    assert_eq!(unsafe { initial.load_native(nil_path.as_os_str().as_bytes(), &[Operand::word(values as usize)]) }.status, native::LOADED);
    let base_check = intern(&id, b"checkBaseValues");
    assert_eq!(std::fs::read_dir(&isolated).unwrap().count(), 9);
    symlink(prefix.join("lib/libbirnam_namespace.dylib"), isolated.join("libbirnam_namespace.dylib")).unwrap();
    let namespace_path = isolated.join("libbirnam_namespace.dylib");
    assert_eq!(unsafe { initial.load_native(namespace_path.as_os_str().as_bytes(), &[]) }.status, native::INVALID_REQUEST);
    let namespace = initial.initialize_namespace().unwrap().unwrap();
    assert_eq!(initial.initialize_namespace().unwrap(), Some(namespace));
    let path = scratch.join("early-namespace.dylib");
    let answer = unsafe { initial.load_native(path.as_os_str().as_bytes(), &[Operand::word(namespace as usize)]) };
    assert_eq!(answer.status, native::LOADED);
    let namespace_identity = answer.value.as_word().unwrap();
    let namespace_check = intern(&id, b"earlyNamespaceCheck");
    let namespace_release = intern(&id, b"releaseEarlyNamespaceAlias");
    assert_eq!(std::fs::read_dir(&isolated).unwrap().count(), 10);
    symlink(prefix.join("lib/libbirnam_object_model.dylib"), isolated.join("libbirnam_object_model.dylib")).unwrap();
    let model_path = isolated.join("libbirnam_object_model.dylib");
    assert_eq!(unsafe { initial.load_native(model_path.as_os_str().as_bytes(), &[]) }.status, native::INVALID_REQUEST);
    let model = initial.initialize_object_model().unwrap().unwrap();
    assert_eq!(initial.initialize_object_model().unwrap(), Some(model));
    let model_fixture = scratch.join("early-object-model.dylib");
    let model_arguments = [Operand::word(roots as usize), Operand::word(model as usize), Operand::word(0)];
    let result = unsafe { initial.load_native(model_fixture.as_os_str().as_bytes(), &model_arguments) };
    assert_eq!(result.status, native::LOADED);
    let model_identity = result.value.as_word().unwrap();
    let model_check = intern(&id, b"earlyObjectModelCheck");
    let model_release = intern(&id, b"releaseEarlyPrivateBehavior");
    assert_eq!(std::fs::read_dir(&isolated).unwrap().count(), 11);
    symlink(prefix.join("lib/libbirnam_string.dylib"), isolated.join("libbirnam_string.dylib")).unwrap();
    let string_path = isolated.join("libbirnam_string.dylib");
    assert_eq!(unsafe { initial.load_native(string_path.as_os_str().as_bytes(), &[]) }.status, native::INVALID_REQUEST);
    let string = initial.initialize_string().unwrap().unwrap();
    assert_eq!(initial.initialize_string().unwrap(), Some(string));
    let repeated = [Operand::word(roots as usize), Operand::word(records as usize), Operand::word(namespace as usize)];
    assert_eq!(unsafe { initial.load_native(string_path.as_os_str().as_bytes(), &repeated) }.status, native::INVALID_REQUEST);
    let string_fixture = scratch.join("early-string.dylib");
    let string_arguments = [Operand::word(roots as usize), Operand::word(string as usize), Operand::word(records as usize), Operand::word(0)];
    let result = unsafe { initial.load_native(string_fixture.as_os_str().as_bytes(), &string_arguments) };
    assert_eq!(result.status, native::LOADED);
    let string_identity = result.value.as_word().unwrap();
    let string_check = intern(&id, b"earlyStringCheck");
    let string_release = intern(&id, b"releaseEarlyStrings");
    assert_eq!(std::fs::read_dir(&isolated).unwrap().count(), 12);
    symlink(prefix.join("lib/libbirnam_id_error.dylib"), isolated.join("libbirnam_id_error.dylib")).unwrap();
    let error_path = isolated.join("libbirnam_id_error.dylib");
    assert_eq!(unsafe { initial.load_native(error_path.as_os_str().as_bytes(), &[]) }.status, native::INVALID_REQUEST);
    let error = initial.initialize_error().unwrap().unwrap();
    assert_eq!(initial.initialize_error().unwrap(), Some(error));
    let repeated = [Operand::word(roots as usize), Operand::word(records as usize), Operand::word(values as usize), Operand::word(namespace as usize), Operand::word(string as usize)];
    assert_eq!(unsafe { initial.load_native(error_path.as_os_str().as_bytes(), &repeated) }.status, native::INVALID_REQUEST);
    let error_fixture = scratch.join("early-error.dylib");
    let error_arguments = [Operand::word(roots as usize), Operand::word(records as usize), Operand::word(namespace as usize), Operand::word(values as usize), Operand::word(string as usize), Operand::object(error), Operand::word(0)];
    let result = unsafe { initial.load_native(error_fixture.as_os_str().as_bytes(), &error_arguments) };
    assert_eq!(result.status, native::LOADED);
    let error_identity = result.value.as_word().unwrap();
    let error_check = intern(&id, b"earlyErrorCheck");
    let error_release = intern(&id, b"releaseEarlyError");
    assert_eq!(std::fs::read_dir(&isolated).unwrap().count(), 13);
    symlink(prefix.join("lib/libbirnam_integer.dylib"), isolated.join("libbirnam_integer.dylib")).unwrap();
    let integer_path = isolated.join("libbirnam_integer.dylib");
    assert_eq!(unsafe { initial.load_native(integer_path.as_os_str().as_bytes(), &[]) }.status, native::INVALID_REQUEST);
    let integer = initial.initialize_integer().unwrap().unwrap();
    assert_eq!(initial.initialize_integer().unwrap(), Some(integer));
    let repeated = [Operand::word(roots as usize), Operand::word(records as usize), Operand::word(values as usize), Operand::word(namespace as usize), Operand::word(string as usize), Operand::object(error)];
    assert_eq!(unsafe { initial.load_native(integer_path.as_os_str().as_bytes(), &repeated) }.status, native::INVALID_REQUEST);
    let integer_fixture = scratch.join("early-integer.dylib");
    let integer_arguments = [Operand::word(roots as usize), Operand::word(namespace as usize), Operand::word(values as usize), Operand::word(string as usize), Operand::word(integer as usize), Operand::word(0), Operand::word(0)];
    let result = unsafe { initial.load_native(integer_fixture.as_os_str().as_bytes(), &integer_arguments) };
    assert_eq!(result.status, native::LOADED);
    let integer_identity = result.value.as_word().unwrap();
    let integer_check = intern(&id, b"earlyIntegerCheck");
    let integer_release = intern(&id, b"releaseEarlyInteger");
    assert_eq!(std::fs::read_dir(&isolated).unwrap().count(), 14);
    symlink(prefix.join("lib/libbirnam_float.dylib"), isolated.join("libbirnam_float.dylib")).unwrap();
    let float_path = isolated.join("libbirnam_float.dylib");
    assert_eq!(unsafe { initial.load_native(float_path.as_os_str().as_bytes(), &[]) }.status, native::INVALID_REQUEST);
    let float = initial.initialize_float().unwrap().unwrap();
    assert_eq!(initial.initialize_float().unwrap(), Some(float));
    let repeated = [Operand::word(roots as usize), Operand::word(records as usize), Operand::word(values as usize), Operand::word(namespace as usize), Operand::word(string as usize), Operand::object(error), Operand::word(integer as usize)];
    assert_eq!(unsafe { initial.load_native(float_path.as_os_str().as_bytes(), &repeated) }.status, native::INVALID_REQUEST);
    let float_fixture = scratch.join("early-float.dylib");
    let float_arguments = [Operand::word(roots as usize), Operand::word(namespace as usize), Operand::word(values as usize), Operand::word(string as usize), Operand::word(integer as usize), Operand::word(float as usize), Operand::word(0), Operand::word(0)];
    let result = unsafe { initial.load_native(float_fixture.as_os_str().as_bytes(), &float_arguments) };
    assert_eq!(result.status, native::LOADED);
    let float_identity = result.value.as_word().unwrap();
    let float_check = intern(&id, b"earlyFloatCheck");
    let float_release = intern(&id, b"releaseEarlyFloat");
    assert_eq!(std::fs::read_dir(&isolated).unwrap().count(), 15);
    symlink(prefix.join("lib/libbirnam_word.dylib"), isolated.join("libbirnam_word.dylib")).unwrap();
    let word_path = isolated.join("libbirnam_word.dylib");
    assert_eq!(unsafe { initial.load_native(word_path.as_os_str().as_bytes(), &[]) }.status, native::INVALID_REQUEST);
    let word = initial.initialize_word().unwrap().unwrap();
    assert_eq!(initial.initialize_word().unwrap(), Some(word));
    let repeated = [Operand::word(roots as usize), Operand::word(records as usize), Operand::word(values as usize), Operand::word(namespace as usize), Operand::word(string as usize), Operand::object(error), Operand::word(integer as usize)];
    assert_eq!(unsafe { initial.load_native(word_path.as_os_str().as_bytes(), &repeated) }.status, native::INVALID_REQUEST);
    let word_fixture = scratch.join("early-word.dylib");
    let word_arguments = [Operand::word(roots as usize), Operand::word(namespace as usize), Operand::word(values as usize), Operand::word(string as usize), Operand::word(integer as usize), Operand::word(word as usize), Operand::word(0), Operand::word(0)];
    let result = unsafe { initial.load_native(word_fixture.as_os_str().as_bytes(), &word_arguments) };
    assert_eq!(result.status, native::LOADED);
    let word_identity = result.value.as_word().unwrap();
    let word_check = intern(&id, b"earlyWordCheck");
    let word_release = intern(&id, b"releaseEarlyWord");
    assert_eq!(std::fs::read_dir(&isolated).unwrap().count(), 16);
    initial.initialize_text().unwrap();
    initial.initialize_text().unwrap();
    let text_fixture = scratch.join("early-text.dylib");
    let text_arguments = [Operand::word(roots as usize), Operand::word(namespace as usize), Operand::word(values as usize), Operand::word(string as usize), Operand::word(integer as usize), Operand::word(float as usize), Operand::word(0), Operand::word(0)];
    let result = unsafe { initial.load_native(text_fixture.as_os_str().as_bytes(), &text_arguments) };
    assert_eq!(result.status, native::LOADED);
    let text_identity = result.value.as_word().unwrap();
    let text_check = intern(&id, b"earlyTextCheck");
    let text_release = intern(&id, b"releaseEarlyText");
    assert_eq!(std::fs::read_dir(&isolated).unwrap().count(), 16);
    for library in ["native_mark", "trace"] {
        let file = format!("libbirnam_{library}.dylib");
        symlink(prefix.join("lib").join(&file), isolated.join(file)).unwrap();
    }
    let graph = initial.initialize_trace().unwrap().unwrap();
    assert_eq!(initial.initialize_trace().unwrap(), Some(graph));
    assert_eq!(std::fs::read_dir(&isolated).unwrap().count(), 18);
    let trace_fixture = scratch.join("early-trace.dylib");
    let trace_arguments = [Operand::word(graph as usize), Operand::word(roots as usize), Operand::word(records as usize), Operand::word(model as usize), Operand::word(0)];
    assert_eq!(unsafe { initial.load_native(trace_fixture.as_os_str().as_bytes(), &trace_arguments) }.status, native::LOADED);
    let trace_check = intern(&id, b"earlyTraceCheck");
    for module in ["native_stack", "gc_settings", "allocation_budget", "collect"] {
        let file = format!("libbirnam_{module}.dylib");
        symlink(prefix.join("lib").join(&file), isolated.join(file)).unwrap();
    }
    let collection = initial.initialize_collection().unwrap().unwrap();
    assert_eq!(initial.initialize_collection().unwrap(), Some(collection));
    assert_eq!(std::fs::read_dir(&isolated).unwrap().count(), 22);
    let saved_collector = unsafe { *(*collection).collector };
    let collection_fixture = scratch.join("early-collection.dylib");
    let arguments = [Operand::word(collection as usize), Operand::word(roots as usize), Operand::word(records as usize)];
    assert_eq!(unsafe { initial.load_native(collection_fixture.as_os_str().as_bytes(), &arguments) }.status, native::LOADED);
    let collection_check = intern(&id, b"earlyCollectionCheck");
    let collection_release = intern(&id, b"releaseEarlyCollection");
    let collection_adopted = intern(&id, b"earlyCollectionAdopted");
    // Only now make the remaining upper libraries available and continue the
    // same production construction, adopting the already initialized registry.
    for entry in std::fs::read_dir(prefix.join("lib")).unwrap() {
        let entry = entry.unwrap();
        if !isolated.join(entry.file_name()).exists() {
            symlink(entry.path(), isolated.join(entry.file_name())).unwrap();
        }
    }
    let resources = birnam_host::resolve_from_exe_dir(
        &prefix.join("bin"),
        Overrides {
            id_dylib: Some(&id_path),
            ..Overrides::default()
        },
    )
    .unwrap();
    let activation_fixture = scratch.join("early-activation.dylib");
    let activation_args = [Operand::word(collection as usize), Operand::word(initial.bootstrap().unwrap() as usize),
        Operand::word(roots as usize), Operand::word(records as usize), Operand::word(0), Operand::word(integer as usize)];
    assert_eq!(unsafe { initial.load_native(activation_fixture.as_os_str().as_bytes(), &activation_args) }.status, native::LOADED);
    let activation_save = intern(&id, b"saveActivationConstructors");
    let activation_exercise = intern(&id, b"exerciseActivationConstructors");
    initial.activate_collection().unwrap();
    assert_eq!(send(&id, activation_save, &[]).status, native::LOADED);
    let runtime_path = isolated.join("libbirnam_runtime.dylib");
    let support = initial.initialize_runtime_support().unwrap();
    let operations = initial.initialize_runtime_values().unwrap();
    let construction = [Operand::word(roots as usize), Operand::word(records as usize),
        Operand::word(values as usize), Operand::word(namespace as usize), Operand::word(model as usize),
        Operand::word(graph as usize), Operand::word(collection as usize),
        Operand::word((&primitives as *const birnam_abi::id_primitives::PrimitivesV1).addr()),
        Operand::word((&support as *const birnam_abi::runtime_support::FactoriesV1).addr()),
        Operand::word((&operations as *const birnam_abi::runtime_values::ApisV1).addr())];
    let construction_bytes = unsafe { std::slice::from_raw_parts(construction.as_ptr().cast::<u8>(), size_of_val(&construction)) };

    assert_eq!(unsafe { initial.load_native(scratch.join("runtime-construction.dylib").as_os_str().as_bytes(),
        &[Operand::bytes(runtime_path.as_os_str().as_bytes()), Operand::bytes(construction_bytes)]) }.status, native::LOADED);
    println!("separate Runtime rejects malformed constructors before adoption");
    let floor = initial.finish().unwrap();
    assert_eq!(floor.gc_settings(), unsafe { (*collection).settings }.cast_const());
    assert_eq!(floor.native_stack(), unsafe { (*collection).stack }.cast_const());
    assert_eq!(send(&id, collection_adopted, &[]).value.as_word(), Some(1));
    // An unbound compatibility Symbol is rooted by Runtime, not the native
    // Trace graph. Empty input must still include it through the extension.
    let compatibility_root = intern(&id, b"savedCollectorRuntimeRootProbe");
    let owner = unsafe { &*collection };
    let records = unsafe { &*owner.records };
    let phase = unsafe { &*owner.phase_factory };
    let input = birnam_abi::collector::InputV1 {
        start: 0, bottom: 0, context: core::ptr::null_mut(), roots: None, retirement: core::ptr::null(),
    };
    unsafe {
        assert_eq!(phase.set.unwrap()(owner.phase_context, 1), 1);
        (records.clear)(records.context);
        assert_eq!((saved_collector.trace)(saved_collector.context, &input), 0);
        assert_eq!((records.get)(records.context, compatibility_root.addr()), 1);
        assert_eq!(phase.set.unwrap()(owner.phase_context, 0), 1);
    }

    let runtime =
        unsafe { birnam_host::start_with_floor(resources, StartupPolicy::from_flags(false, false), floor) }.unwrap();
    let mut anchor = 0usize;
    unsafe { runtime.prepare_execution(&mut anchor) };
    let settings = runtime.stack.gc_settings();
    unsafe { birnam_abi::gc_settings::set_precise(settings, 1); (runtime.stack.api().gcCollect)(); }
    assert_eq!(send(&id, collection_check, &[]).value.as_word(), Some(2));
    assert_eq!(send(&id, collection_release, &[]).status, native::LOADED);
    // Use the entry saved before Runtime existed, with no caller-supplied upper
    // roots or retirement. The installed extension must provide them itself.
    unsafe extern "C" fn collect_saved(context: *mut core::ffi::c_void, start: usize, bottom: usize) {
        let collector = unsafe { &*context.cast::<birnam_abi::collector::ApiV3>() };
        let input = birnam_abi::collector::InputV1 {
            start, bottom, context: core::ptr::null_mut(), roots: None, retirement: core::ptr::null(),
        };
        let mut count = 0;
        assert_eq!(unsafe { (collector.collect)(collector.context, &input, &mut count) }, 0);
    }
    let factory = unsafe { &*(*collection).stack_factory };
    assert_eq!(unsafe { factory.capture.unwrap()((&raw mut anchor).addr(),
        core::ptr::from_ref(&saved_collector).cast_mut().cast(), Some(collect_saved)) }, 1);
    assert_eq!(send(&id, collection_check, &[]).value.as_word(), Some(3));
    println!("native collection records/controls survive adoption; real marking, phase, budget, finalization and removal use the same owner");
    assert_eq!(send(&id, activation_exercise, &[]).value.as_word(), Some(1));
    println!("saved early collecting constructors use completed Runtime roots after adoption");
    assert_eq!(send(&id, trace_check, &[]).value.as_word(), Some(1));
    println!("native Trace roots, layouts, inherited hooks and behavior owners work before Runtime and after adoption/GC");
    let base = unsafe { *values };
    assert_eq!((runtime.stack.api().nilObj, runtime.stack.api().trueObj, runtime.stack.api().falseObj),
        (base.nil, base.true_object, base.false_object));
    assert_eq!(send(&id, base_check, &[]).value.as_word(), Some(1));
    println!("Nil/Boolean bases work before successors; identities, layouts and live replacements survive reload/adoption/GC");
    assert_eq!(send(&id, namespace_check, &[]).value.as_word(), Some(namespace_identity));
    let first = unsafe { birnam_abi::namespace_import::import(runtime.stack.imports(), c"earlyNamespaceFirst".as_ptr()) };
    let second = unsafe { birnam_abi::namespace_import::import(runtime.stack.imports(), c"earlyNamespaceSecond".as_ptr()) };
    assert!(!first.is_null()); assert_eq!(first, second);
    assert_eq!(send(&id, namespace_release, &[Operand::word(0)]).status, native::LOADED);
    unsafe { (runtime.stack.api().gcCollect)(); }
    assert_eq!(send(&id, namespace_check, &[]).value.as_word(), Some(namespace_identity));
    assert_eq!(send(&id, namespace_release, &[Operand::word(1)]).status, native::LOADED);
    unsafe { (runtime.stack.api().gcCollect)(); }
    assert_eq!(send(&id, namespace_check, &[]).value.kind(), native::UNIT);
    assert!(unsafe { birnam_abi::namespace_import::import(runtime.stack.imports(), c"earlyNamespaceSecond".as_ptr()) }.is_null());
    println!("Namespace binds/enumerates before successors; aliases survive adoption and release permits reclamation");
    assert_eq!(send(&id, model_check, &[]).value.as_word(), Some(model_identity));
    assert_eq!(send(&id, model_release, &[]).status, native::LOADED);
    unsafe { (runtime.stack.api().gcCollect)(); }
    assert_eq!(send(&id, model_check, &[]).value.kind(), native::UNIT);
    let later_arguments = [model_arguments[0], model_arguments[1], Operand::word(1)];
    let later = unsafe { runtime.stack.load_native(model_fixture.as_os_str().as_bytes(), &later_arguments) };
    assert_eq!(later.status, native::LOADED);
    let later_identity = later.value.as_word().unwrap(); assert_ne!(later_identity, model_identity);
    unsafe { (runtime.stack.api().gcCollect)(); }
    assert_eq!(send(&id, model_check, &[]).value.as_word(), Some(later_identity));
    assert_eq!(send(&id, model_release, &[]).status, native::LOADED);
    unsafe { (runtime.stack.api().gcCollect)(); }
    assert_eq!(send(&id, model_check, &[]).value.kind(), native::UNIT);
    println!("Object Model constructs private behaviors early; adopted owner edges retain objects and retire after release");
    let adopted_string = unsafe { birnam_abi::string::operations() };
    assert_eq!((adopted_string.context, adopted_string.behavior, adopted_string.prototype),
        unsafe { ((*string).context, (*string).behavior, (*string).prototype) });
    assert_eq!(unsafe { birnam_abi::namespace_import::import(runtime.stack.imports(), c"String".as_ptr()) }, adopted_string.prototype);
    assert_eq!(send(&id, string_check, &[]).value.as_word(), Some(string_identity));
    assert_eq!(send(&id, string_release, &[]).status, native::LOADED);
    unsafe { (runtime.stack.api().gcCollect)(); }
    assert_eq!(send(&id, string_check, &[]).value.kind(), native::UNIT);
    let later_arguments = [string_arguments[0], string_arguments[1], string_arguments[2], Operand::word(1)];
    let later = unsafe { runtime.stack.load_native(string_fixture.as_os_str().as_bytes(), &later_arguments) };
    assert_eq!(later.status, native::LOADED);
    let later_identity = later.value.as_word().unwrap(); assert_ne!(later_identity, string_identity);
    unsafe { (runtime.stack.api().gcCollect)(); }
    assert_eq!(send(&id, string_check, &[]).value.as_word(), Some(later_identity));
    assert_eq!(send(&id, string_release, &[]).status, native::LOADED);
    unsafe { (runtime.stack.api().gcCollect)(); }
    assert_eq!(send(&id, string_check, &[]).value.kind(), native::UNIT);
    println!("String owns bytes before Error/numbers; live allocation, clone and finalizers survive adoption and release");
    assert_eq!(send(&id, error_check, &[]).value.as_word(), Some(error_identity));
    assert_eq!(send(&id, error_release, &[]).status, native::LOADED);
    unsafe { (runtime.stack.api().gcCollect)(); }
    assert_eq!(send(&id, error_check, &[]).value.kind(), native::UNIT);
    let mut later_arguments = error_arguments; later_arguments[6] = Operand::word(1);
    let later = unsafe { runtime.stack.load_native(error_fixture.as_os_str().as_bytes(), &later_arguments) };
    assert_eq!(later.status, native::LOADED);
    let later_identity = later.value.as_word().unwrap(); assert_ne!(later_identity, error_identity);
    unsafe { (runtime.stack.api().gcCollect)(); }
    assert_eq!(send(&id, error_check, &[]).value.as_word(), Some(later_identity));
    assert_eq!(send(&id, error_release, &[]).status, native::LOADED);
    unsafe { (runtime.stack.api().gcCollect)(); }
    assert_eq!(send(&id, error_check, &[]).value.kind(), native::UNIT);
    println!("Error constructs and propagates before numbers; factory replacements, payload roots and DNU survive adoption/GC");
    let adopted_float = unsafe { birnam_abi::float::operations() };
    assert_eq!((adopted_float.context, adopted_float.behavior, adopted_float.prototype),
        unsafe { ((*float).context, (*float).behavior, (*float).prototype) });
    assert_eq!(send(&id, float_check, &[]).value.as_word(), Some(float_identity));
    assert_eq!(send(&id, float_release, &[]).status, native::LOADED);
    unsafe { (runtime.stack.api().gcCollect)(); }
    assert_eq!(send(&id, float_check, &[]).value.kind(), native::UNIT);
    let mut later_arguments = float_arguments; later_arguments[6] = Operand::word(1);
    later_arguments[7] = Operand::word(runtime.stack.api().gcCollect as usize);
    let later = unsafe { runtime.stack.load_native(float_fixture.as_os_str().as_bytes(), &later_arguments) };
    assert_eq!(later.status, native::LOADED);
    let later_identity = later.value.as_word().unwrap(); assert_ne!(later_identity, float_identity);
    unsafe { (runtime.stack.api().gcCollect)(); }
    assert_eq!(send(&id, float_check, &[]).value.as_word(), Some(later_identity));
    assert_eq!(send(&id, float_release, &[]).status, native::LOADED);
    unsafe { (runtime.stack.api().gcCollect)(); }
    assert_eq!(send(&id, float_check, &[]).value.kind(), native::UNIT);
    println!("Float and mixed Integer operations work before Word/Block/Runtime; bits and native replacements survive adoption/GC");
    let adopted_word = unsafe { birnam_abi::word::operations() };
    assert_eq!((adopted_word.context, adopted_word.behavior, adopted_word.prototype),
        unsafe { ((*word).context, (*word).behavior, (*word).prototype) });
    assert_eq!(send(&id, word_check, &[]).value.as_word(), Some(word_identity));
    assert_eq!(send(&id, word_release, &[]).status, native::LOADED);
    unsafe { (runtime.stack.api().gcCollect)(); }
    assert_eq!(send(&id, word_check, &[]).value.kind(), native::UNIT);
    let mut later_arguments = word_arguments; later_arguments[6] = Operand::word(1);
    later_arguments[7] = Operand::word(runtime.stack.api().gcCollect as usize);
    let later = unsafe { runtime.stack.load_native(word_fixture.as_os_str().as_bytes(), &later_arguments) };
    assert_eq!(later.status, native::LOADED);
    let later_identity = later.value.as_word().unwrap(); assert_ne!(later_identity, word_identity);
    unsafe { (runtime.stack.api().gcCollect)(); }
    assert_eq!(send(&id, word_check, &[]).value.as_word(), Some(later_identity));
    assert_eq!(send(&id, word_release, &[]).status, native::LOADED);
    unsafe { (runtime.stack.api().gcCollect)(); }
    assert_eq!(send(&id, word_check, &[]).value.kind(), native::UNIT);
    println!("Word works before Block/Runtime; bits, conversion and native replacements survive adoption/GC");
    assert_eq!(send(&id, text_check, &[]).value.as_word(), Some(text_identity));
    assert_eq!(send(&id, text_release, &[]).status, native::LOADED);
    unsafe { (runtime.stack.api().gcCollect)(); }
    assert_eq!(send(&id, text_check, &[]).value.kind(), native::UNIT);
    let mut later_arguments = text_arguments; later_arguments[6] = Operand::word(1);
    later_arguments[7] = Operand::word(runtime.stack.api().gcCollect as usize);
    let later = unsafe { runtime.stack.load_native(text_fixture.as_os_str().as_bytes(), &later_arguments) };
    assert_eq!(later.status, native::LOADED);
    let later_identity = later.value.as_word().unwrap(); assert_ne!(later_identity, text_identity);
    unsafe { (runtime.stack.api().gcCollect)(); }
    assert_eq!(send(&id, text_check, &[]).value.as_word(), Some(later_identity));
    assert_eq!(send(&id, text_release, &[]).status, native::LOADED);
    unsafe { (runtime.stack.api().gcCollect)(); }
    assert_eq!(send(&id, text_check, &[]).value.kind(), native::UNIT);
    println!("String text and Nil/Boolean printing work before Block/Runtime; native replacements and callback roots survive adoption/GC");
    let adopted_integer = unsafe { birnam_abi::integer::operations() };
    assert_eq!((adopted_integer.context, adopted_integer.behavior, adopted_integer.prototype),
        unsafe { ((*integer).context, (*integer).behavior, (*integer).prototype) });
    assert_eq!(send(&id, integer_check, &[]).value.as_word(), Some(integer_identity));
    assert_eq!(send(&id, integer_release, &[]).status, native::LOADED);
    unsafe { (runtime.stack.api().gcCollect)(); }
    assert_eq!(send(&id, integer_check, &[]).value.kind(), native::UNIT);
    let mut later_arguments = integer_arguments; later_arguments[5] = Operand::word(1);
    later_arguments[6] = Operand::word(runtime.stack.api().gcCollect as usize);
    let later = unsafe { runtime.stack.load_native(integer_fixture.as_os_str().as_bytes(), &later_arguments) };
    assert_eq!(later.status, native::LOADED);
    let later_identity = later.value.as_word().unwrap(); assert_ne!(later_identity, integer_identity);
    unsafe { (runtime.stack.api().gcCollect)(); }
    assert_eq!(send(&id, integer_check, &[]).value.as_word(), Some(later_identity));
    assert_eq!(send(&id, integer_release, &[]).status, native::LOADED);
    unsafe { (runtime.stack.api().gcCollect)(); }
    assert_eq!(send(&id, integer_check, &[]).value.kind(), native::UNIT);
    println!("Integer works before Float; canonical storage and base replacements survive mixed extensions and precise GC");
    assert_eq!(send(&id, root_identity, &[]).value.as_word(), Some(early_identity));
    assert_eq!(send(&id, release_root, &[]).value.as_word(), Some(1));
    unsafe { (runtime.stack.api().gcCollect)(); }
    assert_eq!(send(&id, root_identity, &[]).value.kind(), native::UNIT);
    assert_eq!(send(&id, release_root, &[]).value.as_word(), Some(0));
    println!("native module root registered before Runtime survives adoption/GC and is reclaimed after release");
    assert_eq!(send(&id, lifetime_check, &[]).value.as_word(), Some(0));
    assert_eq!(send(&id, lifetime_release, &[]).value.as_word(), Some(1));
    unsafe { (runtime.stack.api().gcCollect)(); }
    assert_eq!(send(&id, lifetime_check, &[]).value.as_word(), Some(7));
    println!("pre-Runtime Lifetime layouts/hooks/finalizers survive adoption, trace exact edges and retire after release");
    assert_eq!(send(&id, bootstrap_query, &[]).value.as_word(), Some(bootstrap_identity));
    assert_eq!(send(&id, bootstrap_release, &[]).value.as_word(), Some(1));
    assert_eq!(send(&id, bootstrap_release, &[]).value.as_word(), Some(0));
    unsafe { (runtime.stack.api().gcCollect)(); }
    assert_eq!(send(&id, bootstrap_query, &[]).value.kind(), native::UNIT);
    let later_result = unsafe { runtime.stack.load_native(bootstrap_path.as_os_str().as_bytes(), &bootstrap_args) };
    assert_eq!(later_result.status, native::LOADED);
    let later_identity = later_result.value.as_word().unwrap();
    assert_ne!(later_identity, bootstrap_identity);
    unsafe { (runtime.stack.api().gcCollect)(); }
    assert_eq!(send(&id, bootstrap_query, &[]).value.as_word(), Some(later_identity));
    assert_eq!(send(&id, bootstrap_release, &[]).value.as_word(), Some(1));
    unsafe { (runtime.stack.api().gcCollect)(); }
    assert_eq!(send(&id, bootstrap_query, &[]).value.kind(), native::UNIT);
    println!("Bootstrap retains nested initializer results before roots exist and after Runtime; transfer, release and reclamation pass");
    let adopted = runtime.stack.id_protocol();
    assert_eq!(
        (
            adopted.context,
            adopted.receiver,
            adopted.intern,
            adopted.load
        ),
        (id.context, id.receiver, id.intern, id.load)
    );
    assert_eq!(intern(&id, b"earlyNativeEcho:"), early);
    assert_eq!(
        send(&id, early, &[Operand::object(id.receiver)])
            .value
            .as_object(),
        Some(id.receiver)
    );
    assert_eq!(
        send(&id, failed_selector, &[Operand::word(85)])
            .value
            .as_word(),
        Some(85)
    );
    assert_eq!(invoke(saved_lookup, &[Operand::object(early)]).value.as_entry(), Some(echo));
    println!("saved Id lookup remains directly callable after Runtime adoption");
    // Ordinary language sends must invoke the same typed entry and context.
    let api = runtime.stack.api();
    assert_eq!(
        unsafe { (api.send)(id.receiver, early, &id.receiver, 1) },
        id.receiver
    );
    for (selector, expected) in [
        (
            b"nativeReturnsWord".as_slice(),
            b"nativeResultRequiresWrapper".as_slice(),
        ),
        (
            b"nativeRejectsArgument".as_slice(),
            b"nativeInvalidRequest".as_slice(),
        ),
    ] {
        let selector = intern(&id, selector);
        let error = unsafe { (api.send)(id.receiver, selector, core::ptr::null(), 0) };
        let kind = intern(&id, b"kind");
        assert_eq!(
            unsafe { (api.send)(error, kind, core::ptr::null(), 0) },
            intern(&id, expected)
        );
    }
    let settings = runtime.stack.gc_settings();
    unsafe { birnam_abi::gc_settings::set_precise(settings, 1) };
    let later_path = scratch.join("0.dylib");
    let later = send(
        &id,
        id.load,
        &[
            Operand::bytes(later_path.as_os_str().as_bytes()),
            Operand::bytes(b"laterNativeEcho:"),
            Operand::word(0x83),
            Operand::word(runtime.stack.root_registry() as usize),
            Operand::entry(native::NativeEntry::new(
                collect,
                core::ptr::from_ref(api).cast_mut().cast(),
            )),
        ],
    );
    assert_eq!(later.status, native::LOADED);
    assert_eq!(
        send(&id, later.value.as_object().unwrap(), &[Operand::word(86)])
            .value
            .as_word(),
        Some(86)
    );
    assert_eq!(std::fs::read_to_string(&log).unwrap(), "2\n3\n4\n5\n");
    println!(
        "production host adopts the same carrier, objects and methods; later native loading passes"
    );
}
