//! File I/O preserves policy and snapshots filenames before reading contents.
use super::*;

pub(super) fn check(h: &H<'_>) {
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
    assert_eq!(h.text(read), data);
    assert_ne!(read, contents);
    h.bind(b"ServiceFilePath", path);
    h.bind(b"ServiceFileContents", contents);
    let read = h.eval(b"[System writeFile: ServiceFilePath contents: ServiceFileContents]; [System readFile: ServiceFilePath]");
    assert_eq!(h.text(read), data);
    let empty = h.string(b"");
    assert_eq!(h.system(b"writeFile:contents:", &[path, empty]), system);
    assert!(
        fs::read(&file).unwrap().is_empty(),
        "write truncates existing file"
    );
    assert_eq!(h.text(h.system(b"readFile:", &[path])), b"");
    let invalid = h.string(&[0xff]);
    let nul = h.string(b"bad\0path");
    let directory = h.string(root.as_os_str().as_bytes());
    let missing = h.string(root.join("missing").as_os_str().as_bytes());
    for path in [invalid, nul, directory, missing, empty] {
        h.error(h.system(b"readFile:", &[path]), b"could not read file");
    }
    for path in [invalid, nul, directory, empty] {
        h.error(
            h.system(b"writeFile:contents:", &[path, contents]),
            b"could not write file",
        );
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
    h.call(h.host.id.root, b"ensurePrivateVtableFor:", &[inherited]);
    assert_eq!(h.system(b"readFile:", &[inherited]), nil);
    assert_eq!(
        h.system(b"writeFile:contents:", &[inherited, contents]),
        nil
    );

    // The path's replaced bytes method returns a mutable provider. Changing that
    // provider in the later contents callback must not redirect the file write.
    let first = root.join("snapshot-a");
    let second = root.join("snapshot-b");
    let snapshot_path = h.string(first.as_os_str().as_bytes());
    let svt = h.named(b"StringVt");
    let original = h.save(svt, b"bytes", b"fileBytes");
    let path_bytes = h.q.byte_values(first.as_os_str().as_bytes()).unwrap();
    let replacement = h.q.byte_values(second.as_os_str().as_bytes()).unwrap();
    let seen = h.hooks.context(&[ptr::null_mut(); 4]);
    let changing = h.hooks.context(&[
        h.named(b"GC"),
        h.key(b"fileBytes"),
        snapshot_path,
        contents,
        path_bytes,
        replacement,
        seen,
        size(0).unwrap(),
        ptr::null_mut(),
    ]);
    h.install(svt, b"bytes", h.method(b"changing", changing));
    assert_eq!(
        h.system(b"writeFile:contents:", &[snapshot_path, contents]),
        system
    );
    assert_eq!(h.hooks.count(changing, 7), 2);
    assert_eq!(h.hooks.field(seen, 0), snapshot_path);
    assert_eq!(h.hooks.field(seen, 1), contents);
    assert_eq!(
        h.q.read_bytes(path_bytes).unwrap(),
        second.as_os_str().as_bytes()
    );
    assert_eq!(fs::read(&first).unwrap(), data);
    assert!(
        !second.exists(),
        "contents callback cannot redirect the filename snapshot"
    );
    let failure = h.hooks.fault(961);
    h.hooks.put(changing, 8, failure);
    assert_eq!(
        h.fail(system, b"writeFile:contents:", &[snapshot_path, contents]),
        failure
    );
    assert_eq!(h.hooks.count(changing, 7), 4);
    assert_eq!(h.hooks.field(seen, 2), snapshot_path);
    assert_eq!(h.hooks.field(seen, 3), contents);
    assert!(!second.exists(), "failed contents send does not write");
    h.restore(svt, b"bytes", b"fileBytes", original);

    // File reads and late Error lookup use the current collecting factories.
    fs::write(&file, &data).unwrap();
    let original = h.save(svt, b"fromBytes:", b"fileString:");
    let forwarding = h.forward(b"fileString:");
    h.install(svt, b"fromBytes:", h.method(b"forward", forwarding));
    assert_eq!(h.text(h.system(b"readFile:", &[path])), data);
    assert_eq!(h.hooks.count(forwarding, 2), 1);
    let failure = h.hooks.fault(951);
    h.hooks.put(forwarding, 3, failure);
    assert_eq!(h.fail(system, b"readFile:", &[path]), failure);
    h.hooks.put(forwarding, 3, ptr::null_mut());
    let error_factory = h.named(b"Error");
    h.bind(b"Error", ptr::null_mut());
    assert_eq!(h.system(b"readFile:", &[missing]), nil);
    h.hooks.put(forwarding, 5, error_factory);
    h.error(
        h.system(b"writeFile:contents:", &[directory, contents]),
        b"could not write file",
    );
    h.restore(svt, b"fromBytes:", b"fileString:", original);
    let evt = h.named(b"ErrorFactoryVt");
    let original = h.save(evt, b"kind:message:", b"fileError:message:");
    h.install(evt, b"kind:message:", h.hooks.method(b"fixed", nil));
    assert_eq!(h.system(b"readFile:", &[missing]), nil);
    assert_eq!(
        h.system(b"writeFile:contents:", &[directory, contents]),
        nil
    );
    h.restore(evt, b"kind:message:", b"fileError:message:", original);
    let vt = h.named(b"SystemVt");
    for (selector, alias, args) in [
        (b"readFile:".as_slice(), b"fileRead:".as_slice(), vec![path]),
        (
            b"writeFile:contents:",
            b"fileWrite:contents:",
            vec![path, contents],
        ),
    ] {
        let original = h.save(vt, selector, alias);
        h.install(vt, selector, h.hooks.method(b"fixed", contents));
        assert!(h.load("file_io").is_null());
        assert_eq!(h.system(selector, &args), contents);
        h.restore(vt, selector, alias, original);
    }
    // Large native files use bounded byte packets, not a managed reference for
    // every byte. Both construction and each append remain replaceable and rooted.
    let id_vt = h.call(h.host.id.root, b"objectBehavior", &[]);
    let sequence = h.save(id_vt, b"sequence", b"fileSequence");
    let bounded = h.forward(b"fileSequence");
    let bytes_vt = h.named(b"BytesVt");
    let append = h.save(bytes_vt, b"appendByteValues", b"fileAppendBytes");
    let appending = h.forward(b"fileAppendBytes");
    h.install(id_vt, b"sequence", h.method(b"bounded", bounded));
    h.install(bytes_vt, b"appendByteValues", h.method(b"freshAppend", appending));
    for length in [0, 8192, 8193, 65537] {
        let payload = (0..length).map(|i| (i % 256) as u8).collect::<Vec<_>>();
        fs::write(&file, &payload).unwrap();
        assert_eq!(h.text(h.system(b"readFile:", &[path])), payload);
        let snapshot = h.system(b"readFileSnapshot:", &[path]);
        assert_eq!(h.text(h.q.vector_at(snapshot, 0).unwrap()), file.as_os_str().as_bytes());
        assert_eq!(h.text(h.q.vector_at(snapshot, 1).unwrap()), payload);
        assert_eq!(h.text(h.system(b"embedFile:", &[path])), payload);
    }
    assert!(h.hooks.count(appending, 2) > 0);
    let failure = h.hooks.fault(969);
    h.hooks.put(appending, 3, failure);
    assert_eq!(h.fail(system, b"readFile:", &[path]), failure);
    h.restore(bytes_vt, b"appendByteValues", b"fileAppendBytes", append);
    h.restore(id_vt, b"sequence", b"fileSequence", sequence);
    h.bind(b"ServiceFilePath", ptr::null_mut());
    h.bind(b"ServiceFileContents", ptr::null_mut());
}
