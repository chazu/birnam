//! Existing reflection queries use the current owners, including during GC.
use crate::{HostRuntime, Overrides, resolve_from_exe_dir, test_hooks::Hooks};
use std::{
    env,
    path::{Path, PathBuf},
};

#[test]
#[ignore = "requires production native modules"]
fn reflection_globals_and_parents_use_live_owner_messages() {
    let lib = PathBuf::from(env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").unwrap());
    let resources = resolve_from_exe_dir(
        Path::new("/unused/bin"),
        Overrides {
            lib_dir: Some(&lib),
            ..Default::default()
        },
    )
    .unwrap();
    let host = HostRuntime::start_native(&resources, true).unwrap();
    let hooks = Hooks::new(&host.context);
    let q = host.scope();
    let ns = host.context.namespace;
    let globals = host.named(b"Globals").unwrap();
    let reflect = host.named(b"Reflect").unwrap();
    let nil = host.named(b"nil").unwrap();
    let object = host.named(b"Object").unwrap();
    let exported = q.call(object, b"clone", &[]).unwrap();
    let authored = q.call(object, b"clone", &[]).unwrap();
    let exported_name = hooks.symbol(b"ReflectionExport");
    let authored_name = hooks.symbol(b"ReflectionGlobalOnly");
    let nil_name = hooks.symbol(b"ReflectionNilExport");
    let missing = hooks.symbol(b"ReflectionMissing");
    q.call(ns, b"at:put:", &[exported_name, exported]).unwrap();
    q.call(ns, b"at:put:", &[nil_name, nil]).unwrap();
    for key in [exported_name, authored_name, nil_name] {
        q.call(globals, b"at:put:", &[key, authored]).unwrap();
    }
    for (key, expected) in [
        (exported_name, exported),
        (authored_name, authored),
        (nil_name, nil),
        (missing, nil),
    ] {
        assert_eq!(q.call(reflect, b"globalNamed:", &[key]).unwrap(), expected);
    }
    let globals_vt = q
        .call(host.context.id.root, b"behaviorOf:", &[globals])
        .unwrap();
    let at_key = hooks.symbol(b"at:");
    let original_at = hooks.capture(globals_vt, at_key);
    let global_calls = hooks.context(&[authored, birnam_abi::size(0).unwrap()]);
    q.call(
        globals_vt,
        b"addMethod",
        &[at_key, hooks.method(b"fixedCount", global_calls)],
    )
    .unwrap();
    assert_eq!(
        q.call(reflect, b"globalNamed:", &[missing]).unwrap(),
        authored
    );
    assert_eq!(
        q.call(reflect, b"globalNamed:", &[exported_name]).unwrap(),
        exported
    );
    assert_eq!(
        hooks.count(global_calls, 1),
        1,
        "native export lookup consulted Globals"
    );
    q.call(globals_vt, b"addMethod", &[at_key, original_at])
        .unwrap();
    let names = q.call(reflect, b"allGlobalNames", &[]).unwrap();
    let names: Vec<_> = (0..q.vector_size(names).unwrap())
        .map(|i| q.vector_at(names, i).unwrap())
        .collect();
    assert!(names.contains(&exported_name));
    assert!(names.contains(&hooks.symbol(b"System")));
    assert!(
        !names.contains(&authored_name),
        "reflection export list included authored-only Globals"
    );

    let behavior = q.call(host.context.id.root, b"behaviorOf:", &[ns]).unwrap();
    let count_key = hooks.symbol(b"count");
    let name_key = hooks.symbol(b"nameAt:");
    let count_original = hooks.capture(behavior, count_key);
    let name_original = hooks.capture(behavior, name_key);
    let count_context =
        hooks.context(&[birnam_abi::size(2).unwrap(), birnam_abi::size(0).unwrap()]);
    let names_context = hooks.context(&[
        name_original,
        host.named(b"GC").unwrap(),
        birnam_abi::size(0).unwrap(),
    ]);
    q.call(
        behavior,
        b"addMethod",
        &[count_key, hooks.method(b"fixedCount", count_context)],
    )
    .unwrap();
    q.call(
        behavior,
        b"addMethod",
        &[name_key, hooks.method(b"collectEvery", names_context)],
    )
    .unwrap();
    let selected = q.call(reflect, b"allGlobalNames", &[]).unwrap();
    assert_eq!(q.vector_size(selected).unwrap(), 2);
    assert_eq!(hooks.count(count_context, 1), 1);
    assert_eq!(hooks.count(names_context, 2), 2);
    for i in 0..2 {
        assert_eq!(q.vector_at(selected, i).unwrap(), names[i]);
    }
    let failure = hooks.fault(54123);
    let fault_context = hooks.context(&[failure, birnam_abi::size(0).unwrap()]);
    q.call(
        behavior,
        b"addMethod",
        &[name_key, hooks.method(b"deny", fault_context)],
    )
    .unwrap();
    let rejected = q.call(reflect, b"allGlobalNames", &[]).unwrap_err();
    q.call(behavior, b"addMethod", &[count_key, count_original])
        .unwrap();
    q.call(behavior, b"addMethod", &[name_key, name_original])
        .unwrap();
    assert!(rejected.0.contains(&format!("{failure:p}")), "{rejected}");

    let parent = host.named(b"ObjectVt").unwrap();
    let child = q.call(parent, b"delegated", &[]).unwrap();
    assert_eq!(q.call(reflect, b"parentOf:", &[child]).unwrap(), parent);
    let root_behavior = q
        .call(
            host.context.id.root,
            b"behaviorOf:",
            &[host.context.id.root],
        )
        .unwrap();
    let key = hooks.symbol(b"parentOf:");
    let original = hooks.capture(root_behavior, key);
    let context = hooks.context(&[authored, birnam_abi::size(0).unwrap()]);
    q.call(
        root_behavior,
        b"addMethod",
        &[key, hooks.method(b"fixedCount", context)],
    )
    .unwrap();
    assert_eq!(q.call(reflect, b"parentOf:", &[child]).unwrap(), authored);
    assert_eq!(hooks.count(context, 1), 1);
    q.call(root_behavior, b"addMethod", &[key, original])
        .unwrap();
}
