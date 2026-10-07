//! Short precise-GC benchmark of a stable explicitly rooted heap. No images.
use birnam_abi::{
    Oop,
    id_protocol::{Operand, Protocol},
};
use std::{os::unix::ffi::OsStrExt, time::Instant};
#[path = "micro_support/clock.rs"]
mod clock;
unsafe fn send(id: Protocol, r: Oop, name: &[u8], a: &[Operand]) -> Operand {
    unsafe { id.send_named(r, name, a) }.unwrap()
}
fn obj(v: Operand) -> Oop {
    v.as_object().unwrap()
}
fn main() {
    unsafe {
        let prefix =
            std::path::PathBuf::from(std::env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").unwrap());
        let id =
            birnam_siphon::load(prefix.join("libbirnam_id.dylib").as_os_str().as_bytes()).unwrap();
        let ns = obj(send(
            id,
            id.receiver,
            b"loadNative:",
            &[
                Operand::bytes(
                    prefix
                        .join("libbirnam_bootstrap.dylib")
                        .as_os_str()
                        .as_bytes(),
                ),
                Operand::bytes(prefix.as_os_str().as_bytes()),
            ],
        ));
        let named = |name: &[u8]| {
            obj(send(
                id,
                ns,
                b"at:",
                &[Operand::object(id.intern_name(name).unwrap())],
            ))
        };
        let roots = named(b"Roots");
        let gc = named(b"GC");
        let behavior = named(b"ObjectVt");
        send(id, gc, b"automaticCollection:", &[Operand::word(0)]);
        let keep = id.intern_name(b"retain:").unwrap();
        for _ in 0..512 {
            let value = obj(send(id, behavior, b"allocate", &[Operand::word(0)]));
            for _ in 0..4 {
                id.send(roots, keep, &[Operand::object(value)]).unwrap();
            }
        }
        let selector = id.intern_name(b"collectPrecisely").unwrap();
        for _ in 0..3 {
            id.send(gc, selector, &[]).unwrap();
        }
        let mut samples = Vec::new();
        for _ in 0..6 {
            let start = Instant::now();
            let cpu = clock::cpu_ns();
            let mut count = 0;
            while start.elapsed().as_millis() < 100 {
                assert_eq!(id.send(gc, selector, &[]).unwrap().as_word(), Some(0));
                count += 1;
            }
            let elapsed = clock::cpu_ns() - cpu;
            samples.push(serde_json::json!({"iterations":count,"cpu_ns":elapsed,"wall_ns":start.elapsed().as_nanos()}));
        }
        println!(
            "{}",
            serde_json::json!({"objects":512,"root_copies":4,"samples":samples})
        );
    }
}
