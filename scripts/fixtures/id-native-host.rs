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

    // Only now make the upper libraries available, and use normal host startup.
    for entry in std::fs::read_dir(prefix.join("lib")).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name() != "libbirnam_id.dylib" {
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
        unsafe { birnam_host::start(resources, StartupPolicy::from_flags(false, false)) }.unwrap();
    let mut anchor = 0usize;
    unsafe { runtime.prepare_execution(&mut anchor) };
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
    let later = load(&id, &scratch.join("0.dylib"), b"laterNativeEcho:");
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
