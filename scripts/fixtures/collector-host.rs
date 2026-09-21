//! Real collector construction with every Runtime successor unavailable.
use birnam_abi::id_protocol::{self as native, Operand};
use std::{os::unix::ffi::OsStrExt, path::Path};
fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    let prefix = Path::new(&args[1]);
    assert_eq!(std::fs::read_dir(prefix).unwrap().count(), 22);
    let mut initial = birnam_host::begin_id(&prefix.join("libbirnam_id.dylib"), None, None).unwrap();
    let collector = initial.initialize_collection().unwrap().unwrap();
    let roots = initial.roots().unwrap();
    let lifetime = initial.initialize_lifetime().unwrap().unwrap();
    let fail = args.get(3).is_some();
    let arguments = [Operand::word(collector as usize), Operand::word(roots as usize), Operand::word(lifetime as usize), Operand::word(usize::from(fail))];
    let answer = unsafe { initial.load_native(Path::new(&args[2]).as_os_str().as_bytes(), &arguments) };
    assert_eq!(answer.status, native::LOADED);
    println!("native collector before Runtime: {}", if fail { "trace failure prevents retirement and reclamation" } else { "roots, marking, weak retirement, finalizers and release pass" });
}
