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

    // Construct the real root owner while every other upper library is absent.
    symlink(prefix.join("lib/libbirnam_root_registry.dylib"), isolated.join("libbirnam_root_registry.dylib")).unwrap();
    let root_path = isolated.join("libbirnam_root_registry.dylib");
    assert_eq!(send(&id, id.load, &[Operand::bytes(root_path.as_os_str().as_bytes()), Operand::word(83)]).status, native::INVALID_REQUEST);
    let mut initial = birnam_host::begin_floor(&id_path, None, None).unwrap();
    assert_eq!(initial.id_protocol().context, id.context);
    let roots = initial.roots().expect("production initialized root owner");
    let root_path = scratch.join("early-root.dylib");
    let retained = send(&id, id.load, &[
        Operand::bytes(root_path.as_os_str().as_bytes()), Operand::word(roots as usize),
    ]);
    assert_eq!(retained.status, native::LOADED);
    assert!(retained.value.as_word().unwrap() > 0);
    let root_identity = intern(&id, b"earlyRootIdentity");
    let release_root = intern(&id, b"releaseEarlyRoot");
    let early_identity = send(&id, root_identity, &[]).value.as_word().unwrap();
    assert_eq!(std::fs::read_dir(&isolated).unwrap().count(), 2);
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
    assert_eq!(send(&id, id.load, &[Operand::bytes(path.as_os_str().as_bytes()), Operand::word(roots as usize), Operand::word(records as usize)]).status, native::LOADED);
    let lifetime_check = intern(&id, b"earlyLifetimeCheck");
    let lifetime_release = intern(&id, b"releaseEarlyLifetime");
    assert_eq!(std::fs::read_dir(&isolated).unwrap().count(), 6);
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
    let runtime =
        unsafe { birnam_host::start_with_floor(resources, StartupPolicy::from_flags(false, false), initial.finish().unwrap()) }.unwrap();
    let mut anchor = 0usize;
    unsafe { runtime.prepare_execution(&mut anchor) };
    let settings = runtime.stack.gc_settings();
    unsafe { birnam_abi::gc_settings::set_precise(settings, 1); (runtime.stack.api().gcCollect)(); }
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
