//! Run independent C oracles using only services selected by production startup.
use birnam_abi::{IdApi, id_protocol::{self as native, Operand}};
use std::{os::unix::ffi::OsStrExt, path::Path};
fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    assert_eq!(args.len(), 5, "LIBRARY_DIRECTORY FIXTURE MODE KIND");
    let lib = Path::new(&args[1]);
    let floor = birnam_host::load_floor(&lib.join("libbirnam_id.dylib"), None, None).unwrap();
    let runtime_path = lib.join("libbirnam_runtime.dylib");
    let factory_path = lib.join(format!("libbirnam_{}.dylib", args[4].to_str().unwrap().replace('-', "_")));
    let api = Operand::word((floor.api() as *const IdApi).addr());
    let roots = Operand::word(floor.root_registry().addr());
    let settings = Operand::word(floor.gc_settings().addr());
    let stack = Operand::word(floor.native_stack().addr());
    let mode = Operand::bytes(args[3].as_bytes());
    let scan_path = lib.join("libbirnam_native_stack.dylib");
    let inputs = match args[4].to_str().unwrap() {
        "native-stack" => vec![api, roots, settings, stack, Operand::word(floor.finalizers().addr()),
            Operand::bytes(runtime_path.as_os_str().as_bytes()), mode],
        "gc-settings" | "root-registry" => vec![api, roots, settings, stack,
            Operand::bytes(runtime_path.as_os_str().as_bytes()),
            Operand::bytes(factory_path.as_os_str().as_bytes()), mode],
        "vtable-enumeration" | "vtable-enumeration-boundary" | "selector-enumeration" =>
            vec![api, roots, settings, stack, Operand::word(floor.vtables().addr()),
                Operand::bytes(runtime_path.as_os_str().as_bytes()), mode],
        "namespace-presence" | "namespace-import" | "namespace-publication" => {
            let kind = args[4].to_str().unwrap();
            let service = match kind {
                "namespace-presence" => floor.presence().addr(),
                "namespace-import" => floor.imports().addr(),
                _ => floor.exports().addr(),
            };
            let mut inputs = vec![api, roots, settings, stack, Operand::word(service),
                Operand::bytes(runtime_path.as_os_str().as_bytes()), mode, Operand::bytes(kind.as_bytes())];
            if kind == "namespace-publication" { inputs.push(Operand::word(floor.imports().addr())); }
            inputs
        },
        "native-stack-scan" => vec![Operand::bytes(scan_path.as_os_str().as_bytes()), mode],
        other => panic!("unknown fixture {other}"),
    };
    let id = floor.id_protocol();
    let path = Operand::bytes(args[2].as_bytes());
    let result = unsafe { (id.message.unwrap())(id.context, id.receiver, id.load, &path, 1) };
    assert_eq!(result.status, native::LOADED);
    let entry = result.value.as_entry().expect("pinned native fixture entry");
    // Diagnostics require the loading message and its managed turn to have returned.
    let result = unsafe { (entry.method.unwrap())(entry.context, id.receiver, inputs.as_ptr(), inputs.len()) };
    assert_eq!(result.status, native::LOADED);
}
