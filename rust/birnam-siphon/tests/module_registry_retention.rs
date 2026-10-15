//! Measure the actual authored registry batch outside the language telemetry path.
mod support;
use birnam_abi::{
    Oop,
    id_protocol::{Operand, Protocol},
};
use std::os::unix::ffi::OsStrExt;
use support::*;

unsafe fn named(id: Protocol, ns: Oop, name: &[u8]) -> Oop {
    unsafe {
        object(send(
            id,
            ns,
            b"at:",
            &[Operand::object(id.intern_name(name).unwrap())],
        ))
    }
}
unsafe fn count(id: Protocol) -> usize {
    unsafe {
        send(id, id.receiver, b"allocationCount", &[])
            .as_word()
            .unwrap()
    }
}
#[test]
#[ignore = "requires production modules"]
fn registry_batches_leave_no_objects_after_return_and_collection() {
    unsafe {
        let id = id();
        let ns = load(
            id,
            "bootstrap",
            &[Operand::bytes(directory().as_os_str().as_bytes())],
        );
        let roots = named(id, ns, b"Roots");
        let gc = named(id, ns, b"GC");
        send(id, gc, b"precisePayloads:", &[Operand::word(1)]);
        // Match ordinary CLI startup, including TaskExecutor's authored policy.
        let startup = directory().join("birnam-tools.image");
        let path = object(send(
            id,
            named(id, ns, b"String"),
            b"fromBytes:",
            &[Operand::bytes(startup.as_os_str().as_bytes())],
        ));
        let path_root = retain(id, roots, path);
        let loaded = object(send(
            id,
            named(id, ns, b"System"),
            b"loadImage:",
            &[Operand::object(path)],
        ));
        assert_eq!(loaded, named(id, ns, b"true"));
        release(id, roots, path_root);
        let fixture = include_str!("../../../tests/fixtures/module-registry.birnam");
        let source = format!(
            "[Module libDir: {:?}];\n{}\n[[[[Module isLoaded: #os] & [Module isLoaded: #json]] & [TaskExecutor notNil]] ifTrue: [| batch] ifFalse: [| [Error kind: #retentionSetupFailed]]]\n] value]",
            directory().to_str().unwrap(),
            fixture.split("[batch value: 2];").next().unwrap()
        );
        let text = object(send(
            id,
            named(id, ns, b"String"),
            b"fromBytes:",
            &[Operand::bytes(source.as_bytes())],
        ));
        let text_root = retain(id, roots, text);
        let batch = object(send(
            id,
            named(id, ns, b"Codegen"),
            b"eval:",
            &[Operand::object(text)],
        ));
        let batch_root = retain(id, roots, batch);
        release(id, roots, text_root);
        assert_eq!(
            object(send(id, batch, b"isError", &[])),
            named(id, ns, b"false")
        );
        let integer = named(id, ns, b"Integer");
        for n in [2, 0, 1, 30, 100, 100, 100] {
            let argument = object(send(id, integer, b"fromWord:", &[Operand::word(n)]));
            let arg_root = retain(id, roots, argument);
            collect(id, gc);
            let before = count(id);
            let facts = object(send(id, batch, b"value:", &[Operand::object(argument)]));
            assert_eq!(
                send(id, integer, b"wordOf:", &[Operand::object(facts)]).as_word(),
                Some(0)
            );
            collect(id, gc);
            let after = count(id);
            eprintln!("registry native sample {n}: before={before} after={after}");
            if n != 2 {
                assert_eq!(
                    after, before,
                    "registry batch retained objects at {n} calls"
                );
            }
            release(id, roots, arg_root);
        }
        release(id, roots, batch_root);
    }
}
