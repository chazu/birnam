//! Exercise the actual production Id dylib, with no host or upper modules.
use birnam_abi::{Oop, id_protocol::Protocol, size};
use std::os::unix::ffi::OsStrExt;

fn intern(protocol: &Protocol, constructor: Oop, name: &[u8]) -> Oop {
    let bytes: Vec<_> = name.iter().map(|&b| size(b as usize).unwrap()).collect();
    unsafe {
        let name = protocol.send(protocol.root, constructor, &bytes).unwrap();
        protocol.send(name, protocol.intern, &[]).unwrap()
    }
}

#[test]
#[ignore = "scripts/siphon-test.sh builds the production Id library and fixture first"]
fn real_id_takes_over_messages_and_loading() {
    let path = std::env::var_os("BIRNAM_TEST_ID_LIBRARY").expect("production Id path");
    let module = std::env::var_os("BIRNAM_TEST_ID_MODULE").expect("independent native module");
    let protocol = birnam_siphon::load(
        path.as_bytes(),
        &[b"nameWithBytes", b"sequence", module.as_bytes()],
    )
    .expect("Siphon loads Id");
    // Siphon's temporary seed storage has returned. Only Id messages follow.
    unsafe {
        let constructor = protocol
            .send(protocol.literal(0).unwrap(), protocol.intern, &[])
            .unwrap();
        let sequence = protocol
            .send(protocol.literal(1).unwrap(), protocol.intern, &[])
            .unwrap();
        let symbol = intern(&protocol, constructor, b"siphonHandoff");
        assert!(!symbol.is_null());
        assert_eq!(intern(&protocol, constructor, b"siphonHandoff"), symbol);
        assert_ne!(intern(&protocol, constructor, b"anotherSymbol"), symbol);

        // The native initializer receives and returns ordinary objects. Repeated
        // loading must enter it with the current arguments, not a cached answer.
        for value in [symbol, protocol.root] {
            let arguments = protocol.send(protocol.root, sequence, &[value]).unwrap();
            assert_eq!(
                protocol
                    .send(protocol.literal(2).unwrap(), protocol.load, &[arguments])
                    .unwrap(),
                value,
            );
        }
        let empty = protocol.send(protocol.root, sequence, &[]).unwrap();
        assert!(
            protocol
                .send(protocol.literal(2).unwrap(), protocol.load, &[empty])
                .is_err()
        );

        // Reopening the process-owned Id preserves its object identities.
        // The bootstrap interface itself is not a language message result.
        let reopened = birnam_siphon::load(path.as_bytes(), &[]).unwrap();
        assert_eq!(reopened.root, protocol.root);
        assert_eq!(reopened.seeds, protocol.seeds);
        assert_eq!(intern(&reopened, constructor, b"siphonHandoff"), symbol);
    }
}

#[test]
fn rejects_invalid_library_paths() {
    for path in [
        b"".as_slice(),
        b"invalid\0path",
        b"/nonexistent/birnam-siphon-test/id.dylib",
    ] {
        assert!(matches!(
            birnam_siphon::load(path, &[]),
            Err("cannot load Id")
        ));
    }
}
