//! Production loading with only Id, Bootstrap and the three factory libraries.
use birnam_abi::id_protocol::{self as native, Operand};
use std::{path::Path, os::unix::ffi::OsStrExt};
fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    let prefix = Path::new(&args[1]);
    assert!(!prefix.join("libbirnam_runtime.dylib").exists());
    assert_eq!(std::fs::read_dir(prefix).unwrap().count(), 5);
    let initial = birnam_host::begin_id(&prefix.join("libbirnam_id.dylib"), None, None).unwrap();
    for library in ["method_observation", "storage", "dispatch_cache"] {
        let path = prefix.join(format!("libbirnam_{library}.dylib"));
        for args in [&[][..], &[Operand::word(83)][..], &[Operand::bytes(b"unknown")][..]] {
            assert_eq!(unsafe { initial.load_native(path.as_os_str().as_bytes(), args) }.status, native::INVALID_REQUEST);
        }
    }
    let first = initial.initialize_runtime_support().unwrap();
    let second = initial.initialize_runtime_support().unwrap();
    for support in [&first, &second] {
        assert!(unsafe { birnam_abi::runtime_support::valid(support) });
        let result = unsafe { initial.load_native(Path::new(&args[2]).as_os_str().as_bytes(),
            &[Operand::word((support as *const birnam_abi::runtime_support::FactoriesV1).addr())]) };
        assert_eq!(result.status, native::LOADED);
    }
    println!("six native support factories work before Runtime; independent state, all operations, malformed requests and reload pass");
}
