//! Generated-code checks use production TCC and Id, without a language compiler.
use birnam_abi::{self as abi, Oop, id_protocol::Protocol};
pub(super) unsafe fn check(
    id: &Protocol,
    send: &impl Fn(Oop, &[u8], &[Oop]) -> Result<Oop, Oop>,
    sel: &impl Fn(&[u8]) -> Oop,
    path: Oop,
    objects: [Oop; 6],
) {
    unsafe {
        let [ns, roots, gc, string, base, actor] = objects;
        let keep = |v| {
            send(roots, b"retain:", &[v]).unwrap();
            v
        };
        let text = |bytes: &[u8]| {
            let values: Vec<_> = bytes
                .iter()
                .map(|&b| abi::size(b as usize).unwrap())
                .collect();
            let source = send(id.root, b"sequence", &values).unwrap();
            keep(send(string, b"fromBytes:", &[source]).unwrap())
        };
        let args = send(id.root, b"sequence", &[ns]).unwrap();
        let tcc = id.send(path, id.load, &[args]).expect("TCC loaded");
        let source = format!(
            "{}\n{}",
            include_str!("../../../../tests/fixtures/oop-id-v2/id.h"),
            include_str!("../../../../tests/fixtures/oop-id-v2/compiled.c")
        );
        let unit = send(tcc, b"compile:", &[text(source.as_bytes())]).expect("C compiled");
        let token = send(roots, b"retain:", &[unit]).unwrap();
        let identity = send(tcc, b"mappingIdentityOf:", &[unit]).unwrap();
        let method = send(unit, b"initialize", &[]).expect("generated initializer");
        assert_eq!(send(unit, b"initialize", &[]), Ok(method));
        assert!(send(unit, b"initializeWith:", &[ns]).is_err());
        let vt = keep(send(base, b"delegated", &[]).unwrap());
        let receiver = keep(send(vt, b"allocate", &[abi::size(0).unwrap()]).unwrap());
        send(vt, b"addMethod", &[sel(b"compiledEcho:"), method]).unwrap();
        send(roots, b"release:", &[token]).unwrap();
        send(gc, b"collectPrecisely", &[]).unwrap();
        let input = send(base, b"allocate", &[abi::size(0).unwrap()]).unwrap();
        assert_eq!(send(receiver, b"compiledEcho:", &[input]), Ok(input));
        assert!(abi::as_size(send(tcc, b"mappingBytes:", &[identity]).unwrap()).unwrap() > 0);
        // Fail every lookup on the selected Method. Ordinary dispatch still enters it.
        let method_type = send(id.root, b"behaviorOf:", &[method]).unwrap();
        let private = keep(send(method_type, b"delegated", &[]).unwrap());
        let meta = send(id.root, b"behaviorOf:", &[private]).unwrap();
        let private_meta = keep(send(meta, b"delegated", &[]).unwrap());
        let actor_type = send(id.root, b"behaviorOf:", &[actor]).unwrap();
        let reject = send(actor_type, b"lookup", &[sel(b"rethrow:")]).unwrap();
        send(private_meta, b"addMethod", &[sel(b"lookup"), reject]).unwrap();
        send(
            id.root,
            b"reassociateBehaviorOf:with:",
            &[private, private_meta],
        )
        .unwrap();
        send(id.root, b"reassociateBehaviorOf:with:", &[method, private]).unwrap();
        assert_eq!(send(receiver, b"compiledEcho:", &[receiver]), Ok(receiver));
        send(
            id.root,
            b"reassociateBehaviorOf:with:",
            &[method, method_type],
        )
        .unwrap();
        // Captured code survives removal of its binding and collection.
        let captured = send(roots, b"retain:", &[method]).unwrap();
        send(vt, b"removeMethod:", &[sel(b"compiledEcho:")]).unwrap();
        send(gc, b"collectPrecisely", &[]).unwrap();
        let arguments = send(id.root, b"sequence", &[receiver]).unwrap();
        assert_eq!(
            send(method, b"invokeOn:arguments:", &[receiver, arguments]),
            Ok(receiver)
        );
        send(roots, b"release:", &[captured]).unwrap();
        // A separate generated Unit supplies only the checkpoint test fixture. This
        // is not Execution qualification; it exercises TCC's existing idle handshake.
        let control = keep(send(tcc, b"compile:", &[text(source.as_bytes())]).unwrap());
        send(control, b"initialize", &[]).unwrap();
        let control_type = keep(send(base, b"delegated", &[]).unwrap());
        for (name, index) in [
            (b"retirementReady".as_slice(), 1),
            (b"checkpoint".as_slice(), 2),
        ] {
            let method = send(control, b"declaredMethodAt:", &[abi::size(index).unwrap()]).unwrap();
            send(control_type, b"addMethod", &[sel(name), method]).unwrap();
        }
        let control_object = send(control_type, b"allocate", &[abi::size(0).unwrap()]).unwrap();
        send(ns, b"at:put:", &[sel(b"Execution"), control_object]).unwrap();
        send(gc, b"collectPrecisely", &[]).unwrap();
        assert!(abi::as_size(send(tcc, b"mappingBytes:", &[identity]).unwrap()).unwrap() > 0);
        send(tcc, b"checkpoint", &[]).unwrap();
        assert_eq!(
            send(tcc, b"mappingBytes:", &[identity]),
            Ok(abi::size(0).unwrap())
        );
        // Explicit object compilation/loading executes the supplied machine code.
        let object = keep(send(tcc, b"compileObject:", &[text(source.as_bytes())]).unwrap());
        let loaded = keep(send(tcc, b"loadObject:", &[object]).unwrap());
        let loaded_method = send(loaded, b"initialize", &[]).unwrap();
        let arguments = send(id.root, b"sequence", &[receiver]).unwrap();
        assert_eq!(
            send(
                loaded_method,
                b"invokeOn:arguments:",
                &[receiver, arguments]
            ),
            Ok(receiver)
        );
        // Incompatible descriptors are rejected before entering their initializer.
        let rejected = source.replace("{2,sizeof(IdModule)", "{1,sizeof(IdModule)");
        let bad = keep(send(tcc, b"compile:", &[text(rejected.as_bytes())]).unwrap());
        assert!(send(bad, b"initialize", &[]).is_err());
        // A failed initializer can still expose an entered Method. Its retained
        // context keeps code alive, while the failure marker is not stored as data.
        let failed_source = format!("#define FAIL_INITIALIZER 1\n{source}");
        let failed = send(tcc, b"compile:", &[text(failed_source.as_bytes())]).unwrap();
        let failed_token = send(roots, b"retain:", &[failed]).unwrap();
        let published =
            send(failed, b"initialize", &[]).expect_err("initializer fails with its Method");
        let published_token = send(roots, b"retain:", &[published]).unwrap();
        send(roots, b"release:", &[failed_token]).unwrap();
        send(gc, b"collectPrecisely", &[]).unwrap();
        let arguments = send(id.root, b"sequence", &[receiver]).unwrap();
        assert_eq!(
            send(published, b"invokeOn:arguments:", &[receiver, arguments]),
            Ok(receiver)
        );
        send(roots, b"release:", &[published_token]).unwrap();
        println!("TCC_OOP_OK");
    }
}
