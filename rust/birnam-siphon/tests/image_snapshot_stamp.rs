//! Image save rejects intervening edits without replay or file replacement.
#[path = "support/generation.rs"]
#[allow(dead_code)]
mod support;
use birnam_abi::{Oop, as_size, size};
use support::{H, bootstrap};

fn private_vtable(h: &H<'_>, receiver: Oop) -> Oop {
    h.call(h.host.id.root, b"ensurePrivateVtableFor:", &[receiver])
}

fn projection_roots(h: &H<'_>, filename: Oop) {
    let model = h.named(b"ObjectModel");
    let vt = private_vtable(h, model);
    let original = h.binding(vt, b"imageSizeOf:");
    let alias = h.key(b"snapshotOriginalImageSize:");
    h.bind(vt, b"snapshotOriginalImageSize:", original);
    let monitor = h.call(h.named(b"Object"), b"clone", &[]);
    let monitor_vt = private_vtable(h, monitor);
    h.bind(
        monitor_vt,
        b"allocation:bytes:identity:",
        h.method(b"census", std::ptr::null_mut()),
    );
    h.bind(
        monitor_vt,
        b"present:",
        h.method(b"present:", std::ptr::null_mut()),
    );
    let context = h.context(&[alias, h.gc, model, monitor, size(0).unwrap()]);
    h.bind(vt, b"imageSizeOf:", h.method(b"size", context));
    h.bind(vt, b"noteImageProjection:", h.method(b"note", context));
    h.bind(
        monitor_vt,
        b"projection:",
        h.method(b"projection:", context),
    );
    h.eval(b"let projectionRootProbe := [ObjectModel newSlots: #[nil] parent: ObjectVt]; [Reflect defineMethod: #imagePayload on: projectionRootProbe as: [:self | [ObjectModel noteImageProjection: [ObjectModel newSlots: #[[Object clone]] parent: ObjectVt]]]]; true");
    assert_eq!(
        h.call(h.named(b"System"), b"saveImage:", &[filename]),
        h.named(b"true")
    );
    assert_eq!(
        h.count(context, 4),
        1,
        "projection is visited once without replay"
    );
    h.bind(vt, b"imageSizeOf:", original);
    h.call(vt, b"removeMethod:", &[h.key(b"noteImageProjection:")]);
    h.call(vt, b"removeMethod:", &[alias]);
    h.eval(b"[Globals remove: #projectionRootProbe]; true");
    h.call(h.gc, b"collectPrecisely", &[]);
    for index in 0..2 {
        let identity = h.call(monitor, b"projection:", &[size(index).unwrap()]);
        assert!(as_size(identity).unwrap() > 0);
        assert_eq!(
            h.call(monitor, b"present:", &[identity]),
            size(0).unwrap(),
            "temporary projection roots drain after saving"
        );
    }
}

#[test]
#[ignore = "production modules"]
fn snapshot_does_not_publish_stale_code_or_erase_the_intervening_edit() {
    let host = bootstrap();
    let h = H::with_fixture(&host, "image");
    h.modules(&["store"]);
    h.eval(b"[Reflect defineMethod: #snapshotOwned on: Integer as: [:self | 41]]");
    let path = std::env::temp_dir().join(format!(
        "birnam-snapshot-stamp-{}.image",
        std::process::id()
    ));
    let filename = h.text(path.as_os_str().as_encoded_bytes());
    let system = h.named(b"System");
    assert_eq!(h.call(system, b"saveImage:", &[filename]), h.named(b"true"));
    let bytes = std::fs::read(&path).unwrap();
    let codegen = h.named(b"Codegen");
    let vt = private_vtable(&h, codegen);
    let selector = b"compilePortableImageCode:bindings:selection:";
    let original = h.binding(vt, selector);
    let alias = h.key(b"snapshotOriginalCompile:bindings:selection:");
    h.bind(vt, b"snapshotOriginalCompile:bindings:selection:", original);
    let probe = h.key(b"snapshotIntervened");
    let answer = h.integer(73);
    let store = h.named(b"Store");
    let module = h.prototype(b"Module");
    let field = h.key(b"imageProvider");
    let provider = h.key(b"store");
    assert_eq!(
        h.call(module, b"field:of:", &[field, provider]),
        h.named(b"true")
    );
    let context = h.context(&[
        alias,
        host.roots,
        h.gc,
        size(0).unwrap(),
        size(0).unwrap(),
        h.named(b"IntegerVt"),
        probe,
        h.method(b"fixed", answer),
        store,
        h.integer(1),
        module,
        field,
        provider,
        h.named(b"false"),
    ]);
    let changed = h.method(b"change", context);
    h.bind(vt, selector, changed);
    let outcome = h.call(system, b"saveImage:", &[filename]);
    h.bind(vt, selector, original);
    assert_eq!(outcome, h.named(b"nil"));
    assert_eq!(h.count(context, 4), 1);
    assert_eq!(h.call(h.integer(1), b"snapshotIntervened", &[]), answer);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    h.call(h.named(b"IntegerVt"), b"removeMethod:", &[probe]);

    // Retention edits change the optional image data revision, not the fact basis.
    let basis = h.number(h.call(store, b"basis", &[]));
    h.put(context, 3, size(1).unwrap());
    h.bind(vt, selector, changed);
    assert_eq!(h.call(system, b"saveImage:", &[filename]), h.named(b"nil"));
    h.bind(vt, selector, original);
    assert_eq!(h.count(context, 4), 2);
    assert_eq!(h.number(h.call(store, b"basis", &[])), basis);
    assert_eq!(h.number(h.call(store, b"retentionPolicyCode", &[])), 2);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);

    // A provider marker changed during compilation survives the rejected save.
    h.put(context, 3, size(2).unwrap());
    h.bind(vt, selector, changed);
    assert_eq!(h.call(system, b"saveImage:", &[filename]), h.named(b"nil"));
    h.bind(vt, selector, original);
    assert_eq!(h.count(context, 4), 3);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(
        h.call(module, b"field:of:", &[field, provider]),
        h.named(b"false")
    );
    h.call(
        module,
        b"field:of:put:",
        &[field, provider, h.named(b"true")],
    );
    h.call(vt, b"removeMethod:", &[alias]);
    projection_roots(&h, filename);
    std::fs::remove_file(path).unwrap();
}
