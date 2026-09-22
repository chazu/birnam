#![allow(dead_code)]
use birnam_abi::{
    Oop,
    id_protocol::{Method, NativeEntry, Operand, Protocol},
};
use core::ffi::c_void;
use std::{os::unix::ffi::OsStrExt, path::PathBuf};
pub fn directory() -> PathBuf {
    PathBuf::from(
        std::env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").expect("production module directory"),
    )
}
pub unsafe fn id() -> Protocol {
    birnam_siphon::load(
        directory()
            .join("libbirnam_id.dylib")
            .as_os_str()
            .as_bytes(),
    )
    .unwrap()
}
pub unsafe fn send(id: Protocol, receiver: Oop, name: &[u8], args: &[Operand]) -> Operand {
    unsafe { id.send_named(receiver, name, args) }
        .unwrap_or_else(|e| panic!("{}: {e:?}", String::from_utf8_lossy(name)))
}
pub fn object(value: Operand) -> Oop {
    value.as_object().unwrap()
}
pub unsafe fn load(id: Protocol, name: &str, args: &[Operand]) -> Oop {
    let path = directory().join(format!("libbirnam_{name}.dylib"));
    let mut request = vec![Operand::bytes(path.as_os_str().as_bytes())];
    request.extend_from_slice(args);
    object(unsafe { id.send(id.receiver, id.load, &request) }.unwrap())
}
pub unsafe fn behavior(id: Protocol) -> Oop {
    object(unsafe { send(id, id.receiver, b"delegated", &[]) })
}
pub unsafe fn allocate(id: Protocol, behavior: Oop, bytes: usize) -> Oop {
    object(unsafe { send(id, behavior, b"allocate", &[Operand::word(bytes)]) })
}
pub unsafe fn install(
    id: Protocol,
    behavior: Oop,
    name: &[u8],
    method: Method,
    context: *mut c_void,
) {
    unsafe {
        send(
            id,
            behavior,
            b"addMethod",
            &[
                Operand::object(id.intern_name(name).unwrap()),
                Operand::entry(NativeEntry::new(method, context)),
            ],
        );
    }
}
pub unsafe fn retain(id: Protocol, roots: Oop, value: Oop) -> usize {
    unsafe { send(id, roots, b"retain:", &[Operand::object(value)]) }
        .as_word()
        .unwrap()
}
pub unsafe fn release(id: Protocol, roots: Oop, token: usize) {
    assert_eq!(
        unsafe { send(id, roots, b"release:", &[Operand::word(token)]) }.as_word(),
        Some(1)
    );
}
pub unsafe fn present(id: Protocol, value: Oop) -> bool {
    unsafe {
        send(
            id,
            id.receiver,
            b"allocationAt:",
            &[Operand::word(value.addr())],
        )
    }
    .as_object()
        == Some(value)
}
pub unsafe fn collect(id: Protocol, gc: Oop) -> usize {
    unsafe { send(id, gc, b"collectPrecisely", &[]) }
        .as_word()
        .unwrap()
}
