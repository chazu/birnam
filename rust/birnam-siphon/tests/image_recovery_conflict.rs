//! Failed live image loads retain applied and callback writes without inverses.
#[path = "support/generation.rs"]
#[allow(dead_code)]
mod support;
use birnam_abi::size;
use support::{H, bootstrap};

#[test]
#[ignore = "production native modules"]
fn image_compensation_characterizes_intervening_dictionary_write() {
    let host = bootstrap();
    let h = H::with_fixture(&host, "image");
    h.modules(&["store"]);
    let enabled = |name| std::env::var_os(name).is_some();
    let authored = enabled("BIRNAM_IMAGE_AUTHORED");
    let late = enabled("BIRNAM_IMAGE_LATE_GLOBALS");
    let startup = enabled("BIRNAM_IMAGE_STARTUP");
    let system = h.named(b"System");
    if authored {
        let image = h.text(
            host.resources
                .lib_dir
                .join("birnam-tools.image")
                .as_os_str()
                .as_encoded_bytes(),
        );
        assert_eq!(
            h.call(system, b"startImageMapped:", &[image]),
            h.named(b"true")
        );
    }
    if late {
        h.eval(b"let ImageConflictLate := [Object clone]; [Reflect defineMethod: #imageConflictProbe on: ImageConflictLate as: [:self | #saved]]");
    } else {
        h.eval(b"[Reflect defineMethod: #imageConflictProbe on: Integer as: [:self | #saved]]");
    }
    let dir = std::env::temp_dir().join(format!("birnam-image-conflict-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = h.text(dir.join("probe.image").as_os_str().as_encoded_bytes());
    assert_eq!(h.call(system, b"saveImage:", &[path]), h.named(b"true"));
    h.eval(if late {
        b"[Reflect defineMethod: #imageConflictProbe on: ImageConflictLate as: [:self | #before]]"
    } else {
        b"[Reflect defineMethod: #imageConflictProbe on: Integer as: [:self | #before]]"
    });
    let codegen = h.named(b"Codegen");
    let vt = h.call(host.id.root, b"ensurePrivateVtableFor:", &[codegen]);
    let selector = b"installPortableImageBinding:on:as:source:";
    let original = h.binding(vt, selector);
    let alias = h.key(b"conflictOriginalInstall:on:as:source:");
    h.bind(vt, b"conflictOriginalInstall:on:as:source:", original);
    let nested = if enabled("BIRNAM_IMAGE_BOUND_NIL") {
        h.named(b"nil")
    } else {
        h.key(b"nested")
    };
    let error = if enabled("BIRNAM_IMAGE_RETURN_ERROR") {
        h.call(
            h.named(b"Error"),
            b"kind:",
            &[h.key(b"originalImageFailure")],
        )
    } else {
        std::ptr::null_mut()
    };
    let failure = h.keep(
        unsafe {
            host.id.send(
                h.named(b"Execution"),
                h.key(b"failNative:detail:"),
                &[size(881).unwrap(), size(73).unwrap()],
            )
        }
        .unwrap_err(),
    );
    let source = if authored {
        std::ptr::null_mut()
    } else {
        h.text(b"[Module named: #imageConflictSource requires: #[]]; true")
    };
    let source_name = h.key(b"imageConflictSource");
    let side_key = h.key(b"ImageConflictIndependentWrite");
    let globals = h.named(b"Globals");
    let store = h.named(b"Store");
    let context = h.context(&[
        alias,
        h.key(b"imageConflictProbe"),
        nested,
        h.method(b"fixed", nested),
        globals,
        store,
        side_key,
        h.gc,
        source,
        source_name,
        codegen,
        std::ptr::null_mut(),
        size(0).unwrap(),
        error,
        failure,
    ]);
    let replacement = h.method(b"conflict", context);
    for rebuilt in [false, true] {
        h.bind(vt, selector, replacement);
        let result = unsafe {
            host.id.send(
                system,
                h.key(if startup {
                    b"startImage:rebuild:"
                } else {
                    b"loadImage:rebuild:"
                }),
                &[path, h.named(if rebuilt { b"true" } else { b"false" })],
            )
        };
        // Keep the returned object while restoring the method binding.
        if let Ok(value) = result {
            h.keep(value);
        }
        if let Err(value) = result {
            h.keep(value);
        }
        h.bind(vt, selector, original);
        if error.is_null() {
            assert_eq!(
                result.unwrap_err(),
                failure,
                "native failure identity must survive failed loading"
            );
        } else {
            assert_eq!(
                result.unwrap(),
                error,
                "ordinary Error identity must survive failed loading"
            );
        }
        // A partial load need not publish the restored target in Globals yet.
        let target = h.call(context, b"at:", &[size(11).unwrap()]);
        let got = h.call(target, b"imageConflictProbe", &[]);
        assert_eq!(
            h.count(context, 12),
            1,
            "installation callback must not replay"
        );
        assert_eq!(
            got, nested,
            "failed loading must preserve an intervening dictionary edit"
        );
        assert_eq!(
            h.call(globals, b"at:", &[side_key]),
            nested,
            "unrelated Global write must survive"
        );
        assert_eq!(
            h.call(globals, b"includes:", &[side_key]),
            h.named(b"true"),
            "bound nil must remain present"
        );
        assert_eq!(
            h.call(store, b"currentValue:attr:", &[side_key, side_key]),
            nested,
            "explicit user fact must survive"
        );
        assert_eq!(
            h.call(store, b"hasValue:attr:", &[side_key, side_key]),
            h.named(b"true")
        );
        if !source.is_null() {
            assert_eq!(
                h.call(codegen, b"moduleSourceFor:", &[source_name]),
                source,
                "source introduced by callback must survive"
            );
        }
        if !startup {
            let report = h.call(system, b"imageSaveReport", &[]);
            assert_eq!(h.call(report, b"status", &[]), h.key(b"partialLoad"));
        }
        assert_eq!(
            h.call(system, b"imageCompositionIdentity", &[]),
            h.named(b"nil")
        );
        eprintln!(
            "image conflict rebuilt={rebuilt} authored={authored} late={late} startup={startup}: callback writes and failure preserved"
        );
        h.put(context, 12, size(0).unwrap());
    }
    h.call(vt, b"removeMethod:", &[alias]);
    std::fs::remove_dir_all(dir).unwrap();
}
