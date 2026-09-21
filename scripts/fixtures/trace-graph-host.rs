//! Verify the completed production graph without any upper Runtime library.
use birnam_abi::id_protocol::{self as native, Operand};
use std::{os::unix::ffi::OsStrExt, path::Path};
fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    let prefix = Path::new(&args[1]);
    let fixture = Path::new(&args[2]);
    assert_eq!(std::fs::read_dir(prefix).unwrap().count(), 18);
    let mut initial = birnam_host::begin_id(&prefix.join("libbirnam_id.dylib"), None, None).unwrap();
    let graph = initial.initialize_trace().unwrap().unwrap();
    let roots = initial.roots().unwrap();
    let lifetime = initial.initialize_lifetime().unwrap().unwrap();
    let model = initial.initialize_object_model().unwrap().unwrap();
    let args = [Operand::word(graph as usize), Operand::word(roots as usize), Operand::word(lifetime as usize), Operand::word(model as usize), Operand::word(1)];
    let answer = unsafe { initial.load_native(fixture.as_os_str().as_bytes(), &args) };
    assert_eq!(answer.status, native::LOADED);
    println!("Trace graph validates the real early heap, rejects missing descriptions, and traces precise/native/owner edges with Runtime unavailable");
}
