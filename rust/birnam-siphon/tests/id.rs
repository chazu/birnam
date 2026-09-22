//! Exercise the actual production Id dylib, with no host or upper modules.
use birnam_abi::id_protocol::{self as id, Operand, Protocol};
use std::os::unix::ffi::OsStrExt;

fn intern(protocol: &Protocol, name: &[u8]) -> birnam_abi::Oop {
    let argument = Operand::bytes(name);
    let result = unsafe {
        protocol.message.unwrap()(protocol.context, protocol.receiver, protocol.intern, &argument, 1)
    };
    assert_eq!(result.status, id::LOADED);
    result.value.as_object().expect("intern returns an opaque Symbol")
}

#[test]
#[ignore = "scripts/siphon-test.sh builds the production Id library first"]
fn real_id_takes_over_messages_and_loading() {
    let path = std::env::var_os("BIRNAM_TEST_ID_LIBRARY").expect("production Id path");
    let protocol = birnam_siphon::load(path.as_bytes()).expect("Siphon loads Id");
    // Siphon's stack-local context has returned. Only the real Id is used below.
    let symbol = intern(&protocol, b"siphonHandoff");
    assert!(!symbol.is_null());
    assert_eq!(intern(&protocol, b"siphonHandoff"), symbol);
    assert_ne!(intern(&protocol, b"anotherSymbol"), symbol);

    let argument = Operand::bytes(path.as_bytes());
    let loaded = unsafe {
        protocol.message.unwrap()(protocol.context, protocol.receiver, protocol.load, &argument, 1)
    };
    assert_eq!(loaded.status, id::LOADED);
    let real = loaded.value.as_protocol().expect("Id loader returns Id's protocol");
    assert!(!real.is_null());
    // Production Id is process-owned; loading it again preserves its identity.
    assert_eq!(unsafe { (*real).context }, protocol.context);
    assert_eq!(intern(unsafe { &*real }, b"siphonHandoff"), symbol);
}

#[test]
fn rejects_invalid_library_paths() {
    for path in [b"".as_slice(), b"invalid\0path"] {
        assert!(matches!(birnam_siphon::load(path), Err((id::INVALID_REQUEST, _))));
    }
    assert!(matches!(
        birnam_siphon::load(b"/nonexistent/birnam-siphon-test/id.dylib"),
        Err((id::MAPPING_FAILED, _))
    ));
}
