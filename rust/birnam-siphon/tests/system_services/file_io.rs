//! File I/O preserves existing policy and snapshots names before contents sends.
use super::*;
use std::cell::RefCell;

struct ChangingBytes {
    id: Protocol,
    gc: Oop,
    alias: Oop,
    path: Oop,
    contents: Oop,
    path_bytes: RefCell<Vec<u8>>,
    replacement: Vec<u8>,
    seen: RefCell<Vec<Oop>>,
    fail_contents: Cell<bool>,
}
unsafe extern "C" fn changing_bytes(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    unsafe {
        let p = &*c.cast::<ChangingBytes>();
        assert_eq!(n, 0);
        collect(p.id, p.gc);
        p.seen.borrow_mut().push(r);
        if r == p.path {
            return Outcome::loaded(Operand::bytes(&p.path_bytes.borrow()));
        }
        if r == p.contents {
            p.path_bytes.borrow_mut().copy_from_slice(&p.replacement);
            if p.fail_contents.get() {
                return Outcome::failed(native::INVALID_REQUEST, 961);
            }
        }
        p.id.send(r, p.alias, borrowed_slice(a, n).unwrap())
            .map_or_else(|e| e, Outcome::loaded)
    }
}

pub(super) unsafe fn check(h: &mut H) {
    unsafe {
        let id = h.id;
        let nil = h.named(b"nil");
        let system = h.named(b"System");
        let root = std::path::PathBuf::from(std::env::var_os("BIRNAM_SERVICES_ROOT").unwrap());
        let file = root.join("binary");
        let path = h.string(file.as_os_str().as_bytes());
        let data: Vec<u8> = (0..=255).collect();
        let contents = h.string(&data);
        let receiver = h.call(system, b"clone", &[]);
        assert_eq!(
            h.call(receiver, b"writeFile:contents:", &[path, contents]),
            receiver
        );
        assert_eq!(fs::read(&file).unwrap(), data);
        let read = h.system(b"readFile:", &[path]);
        assert_eq!(text(id, read), data);
        assert_ne!(read, contents);
        h.bind(b"ServiceFilePath", Operand::object(path));
        h.bind(b"ServiceFileContents", Operand::object(contents));
        let read = h.eval(b"[System writeFile: ServiceFilePath contents: ServiceFileContents]; [System readFile: ServiceFilePath]");
        assert_eq!(text(id, read), data);
        let empty = h.string(b"");
        assert_eq!(h.system(b"writeFile:contents:", &[path, empty]), system);
        assert!(
            fs::read(&file).unwrap().is_empty(),
            "write truncates existing file"
        );
        let read = h.system(b"readFile:", &[path]);
        assert_eq!(text(id, read), b"");
        let invalid = h.string(&[0xff]);
        let nul = h.string(b"bad\0path");
        let directory = h.string(root.as_os_str().as_bytes());
        let missing = h.string(root.join("missing").as_os_str().as_bytes());
        for path in [invalid, nul, directory, missing, empty] {
            let error = h.system(b"readFile:", &[path]);
            h.error(error, b"could not read file");
        }
        for path in [invalid, nul, directory, empty] {
            let error = h.system(b"writeFile:contents:", &[path, contents]);
            h.error(error, b"could not write file");
        }
        for (selector, args) in [
            (b"readFile:".as_slice(), vec![]),
            (b"readFile:", vec![nil]),
            (b"writeFile:contents:", vec![path]),
            (b"writeFile:contents:", vec![nil, contents]),
            (b"writeFile:contents:", vec![path, nil]),
            (b"writeFile:contents:", vec![invalid, nil]),
        ] {
            assert_eq!(h.system(selector, &args), nil);
        }
        assert!(
            fs::read(&file).unwrap().is_empty(),
            "refused write has no side effect"
        );
        let inherited = h.string(file.as_os_str().as_bytes());
        send(
            id,
            id.receiver,
            b"ensurePrivateVtableFor:",
            &[Operand::object(inherited)],
        );
        assert_eq!(h.system(b"readFile:", &[inherited]), nil);
        assert_eq!(
            h.system(b"writeFile:contents:", &[inherited, contents]),
            nil
        );

        // A replaced bytes method lends mutable fixture storage. Mutating that
        // earlier view from the contents send must not redirect this write.
        let first = root.join("snapshot-a");
        let second = root.join("snapshot-b");
        let snapshot_path = h.string(first.as_os_str().as_bytes());
        let svt = h.named(b"StringVt");
        let original = save(h, svt, b"bytes", b"fileBytes");
        let mut p = ChangingBytes {
            id,
            gc: h.named(b"GC"),
            alias: h.keep(id.intern_name(b"fileBytes").unwrap()),
            path: snapshot_path,
            contents,
            path_bytes: RefCell::new(first.as_os_str().as_bytes().to_vec()),
            replacement: second.as_os_str().as_bytes().to_vec(),
            seen: RefCell::new(Vec::new()),
            fail_contents: Cell::new(false),
        };
        install(
            id,
            svt,
            b"bytes",
            changing_bytes,
            ptr::from_mut(&mut p).cast(),
        );
        assert_eq!(
            h.system(b"writeFile:contents:", &[snapshot_path, contents]),
            system
        );
        assert_eq!(*p.seen.borrow(), vec![snapshot_path, contents]);
        assert_eq!(*p.path_bytes.borrow(), p.replacement);
        assert_eq!(fs::read(&first).unwrap(), data);
        assert!(
            !second.exists(),
            "contents callback cannot redirect the filename snapshot"
        );
        p.fail_contents.set(true);
        let failure = id
            .send_named(
                system,
                b"writeFile:contents:",
                &[Operand::object(snapshot_path), Operand::object(contents)],
            )
            .unwrap_err();
        assert_eq!(
            (failure.status, failure.detail),
            (native::INVALID_REQUEST, 961)
        );
        assert!(!second.exists(), "failed contents send does not write");
        restore(h, svt, b"bytes", b"fileBytes", original);

        // File reads and lazy errors use current collecting factories.
        fs::write(&file, &data).unwrap();
        let original = save(h, svt, b"fromBytes:", b"fileString:");
        let mut forwarding_state = Forward {
            id,
            gc: h.named(b"GC"),
            ns: h.ns,
            alias: h.keep(id.intern_name(b"fileString:").unwrap()),
            calls: Cell::new(0),
            failure: Cell::new(false),
            publish_error: Cell::new(ptr::null_mut()),
        };
        install(
            id,
            svt,
            b"fromBytes:",
            forwarding,
            ptr::from_mut(&mut forwarding_state).cast(),
        );
        let read = h.system(b"readFile:", &[path]);
        assert_eq!(text(id, read), data);
        assert_eq!(forwarding_state.calls.get(), 1);
        forwarding_state.failure.set(true);
        let failure = id
            .send_named(system, b"readFile:", &[Operand::object(path)])
            .unwrap_err();
        assert_eq!(
            (failure.status, failure.detail),
            (native::INVALID_REQUEST, 951)
        );
        forwarding_state.failure.set(false);
        let error_factory = h.named(b"Error");
        h.bind(b"Error", Operand::unit());
        assert_eq!(h.system(b"readFile:", &[missing]), nil);
        forwarding_state.publish_error.set(error_factory);
        let error = h.system(b"writeFile:contents:", &[directory, contents]);
        h.error(error, b"could not write file");
        restore(h, svt, b"fromBytes:", b"fileString:", original);
        let evt = h.named(b"ErrorFactoryVt");
        let original = save(h, evt, b"kind:message:", b"fileError:message:");
        install(id, evt, b"kind:message:", answer, nil);
        assert_eq!(h.system(b"readFile:", &[missing]), nil);
        assert_eq!(
            h.system(b"writeFile:contents:", &[directory, contents]),
            nil
        );
        restore(h, evt, b"kind:message:", b"fileError:message:", original);
        let vt = h.named(b"SystemVt");
        for (selector, alias, args) in [
            (b"readFile:".as_slice(), b"fileRead:".as_slice(), vec![path]),
            (
                b"writeFile:contents:",
                b"fileWrite:contents:",
                vec![path, contents],
            ),
        ] {
            let original = save(h, vt, selector, alias);
            install(id, vt, selector, answer, contents);
            let library = super::directory().join("libbirnam_file_io.dylib");
            assert_eq!(
                send(
                    id,
                    id.receiver,
                    b"loadNative:",
                    &[
                        Operand::bytes(library.as_os_str().as_bytes()),
                        Operand::object(h.ns)
                    ]
                )
                .kind(),
                native::UNIT
            );
            assert_eq!(h.system(selector, &args), contents);
            restore(h, vt, selector, alias, original);
        }
        h.bind(b"ServiceFilePath", Operand::unit());
        h.bind(b"ServiceFileContents", Operand::unit());
    }
}
