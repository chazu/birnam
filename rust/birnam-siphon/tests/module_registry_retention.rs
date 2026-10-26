//! Measure retained authored state after the language call has returned.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::as_size;
use std::os::unix::ffi::OsStrExt;

#[test]
#[ignore = "requires production modules"]
fn registry_batches_leave_no_objects_after_return_and_collection() {
    let h = oop::bootstrap();
    let gc = h.named(b"GC").unwrap();
    h.send(gc, b"precisePayloads:", &[birnam_abi::size(1).unwrap()])
        .unwrap();
    // Match the installed CLI's complete tools image and provider directory.
    {
        let q = h.scope();
        let module = h.global(&q, b"Module").unwrap();
        q.call(
            module,
            b"libDir:",
            &[q.string(h.resources.lib_dir.as_os_str().as_bytes())
                .unwrap()],
        )
        .unwrap();
        let path = q
            .string(
                h.resources
                    .lib_dir
                    .join("birnam-tools.image")
                    .as_os_str()
                    .as_bytes(),
            )
            .unwrap();
        let loaded = q
            .call(h.named(b"System").unwrap(), b"loadImage:", &[path])
            .unwrap();
        q.check_result(loaded).unwrap();
        assert_eq!(loaded, h.named(b"true").unwrap());
    }
    let fixture = include_str!("../../../tests/fixtures/module-registry.birnam");
    let source = format!(
        "[Module libDir: {:?}];\n{}\n[[[[Module isLoaded: #os] & [Module isLoaded: #json]] & [TaskExecutor notNil]] ifTrue: [| batch] ifFalse: [| [Error kind: #retentionSetupFailed]]]\n] value]",
        h.resources.lib_dir.to_str().unwrap(),
        fixture.split("[batch value: 2];").next().unwrap()
    );

    let retained = h.scope();
    let batch = {
        let q = h.scope();
        let text = q.string(source.as_bytes()).unwrap();
        let value = q
            .call(h.named(b"Codegen").unwrap(), b"eval:", &[text])
            .unwrap();
        q.check_result(value).unwrap();
        retained.keep(value).unwrap()
    };
    let integer = h.named(b"Integer").unwrap();
    for n in [2, 0, 1, 30, 100, 100, 100] {
        let q = h.scope();
        let argument = q
            .call(integer, b"fromWord:", &[birnam_abi::size(n).unwrap()])
            .unwrap();
        h.send(gc, b"collectPrecisely", &[]).unwrap();
        let before = as_size(h.send(h.id.root, b"allocationCount", &[]).unwrap()).unwrap();
        // Deliberately retain no returned value through the second collection.
        let facts = h.send(batch, b"value:", &[argument]).unwrap();
        assert_eq!(
            as_size(h.send(integer, b"wordOf:", &[facts]).unwrap()),
            Some(0)
        );
        h.send(gc, b"collectPrecisely", &[]).unwrap();
        let after = as_size(h.send(h.id.root, b"allocationCount", &[]).unwrap()).unwrap();
        eprintln!("registry native sample {n}: before={before} after={after}");
        if n != 2 {
            assert_eq!(
                after, before,
                "registry batch retained objects at {n} calls"
            );
        }
    }
}
