//! Compilation policy at the user-image loading boundary, in fresh processes.
use crate::test_hooks::Hooks;
use crate::{HostRuntime, Overrides, resolve_from_exe_dir};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

#[test]
#[ignore = "requires production native modules and the installed bootstrap image"]
fn user_images_compile_only_without_code_or_on_explicit_rebuild() {
    const TEST: &str =
        "startup::image_loading_test::user_images_compile_only_without_code_or_on_explicit_rebuild";
    let lib = PathBuf::from(env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").unwrap());
    if let Ok(case) = env::var("BIRNAM_IMAGE_POLICY_CASE") {
        let resources = resolve_from_exe_dir(
            Path::new("/unused/bin"),
            Overrides {
                lib_dir: Some(&lib),
                ..Default::default()
            },
        )
        .unwrap();
        let host = HostRuntime::start_native(&resources, false).unwrap();
        let fixtures = Hooks::new(&host.context);
        let calls = fixtures.context(&[fixtures.fault(73129), birnam_abi::size(0).unwrap()]);
        let mapped_calls = fixtures.context(&[fixtures.fault(73130), birnam_abi::size(0).unwrap()]);
        let install = |receiver: &[u8], selector: &[u8], counter| {
            let receiver = host.named(receiver).unwrap();
            let behavior = host
                .send(host.context.id.root, b"behaviorOf:", &[receiver])
                .unwrap();
            host.send(
                behavior,
                b"addMethod",
                &[fixtures.symbol(selector), fixtures.method(b"deny", counter)],
            )
            .unwrap();
        };
        // Reject every compiler route. Merely loading saved object code is allowed.
        for (receiver, selector) in [
            (
                b"Codegen".as_slice(),
                b"beginPortableImageRebuild:bindings:selection:".as_slice(),
            ),
            (b"Codegen", b"compilePortableImageCode:bindings:selection:"),
            (
                b"NativeBackend",
                b"emit:source:filename:persistent:synchronous:",
            ),
            (b"TCC", b"compile:"),
            (b"TCC", b"compileObject:"),
        ] {
            install(receiver, selector, calls);
        }
        if case == "mapped_failure" {
            install(
                b"Codegen",
                b"beginPortableImageMapped:programs:bindings:selection:",
                mapped_calls,
            );
        }
        let image = PathBuf::from(env::var_os("BIRNAM_IMAGE_POLICY_PATH").unwrap());
        let result = host.load_image_mode(&image, case == "rebuild", false);
        match case.as_str() {
            "mapped" => {
                result.unwrap();
                assert_eq!(fixtures.count(calls, 1), 0);
            }
            "mapped_failure" => {
                assert!(result.is_err());
                assert_eq!(fixtures.count(mapped_calls, 1), 1);
                assert_eq!(
                    fixtures.count(calls, 1),
                    0,
                    "failed mapping must never fall back to compilation"
                );
            }
            "without_code" | "rebuild" => {
                assert!(result.is_err(), "the compiler rejection must propagate");
                assert_eq!(
                    fixtures.count(calls, 1),
                    1,
                    "this mode must select compilation"
                );
            }
            _ => panic!("unknown image case"),
        }
        return;
    }

    let compiled = lib.join("birnam-bootstrap.image");
    let bytes = fs::read(&compiled).unwrap();
    assert_eq!(&bytes[..8], b"BIRNAMIM");
    // Image-v8 consists of a 28-byte header followed by kind/length records.
    // Remove only the final native-code record; retain every portable program.
    let mut offset = 28;
    let code_start = loop {
        let kind = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        let length =
            u64::from_le_bytes(bytes[offset + 4..offset + 12].try_into().unwrap()) as usize;
        let end = offset.checked_add(12).unwrap().checked_add(length).unwrap();
        assert!(end <= bytes.len());
        if kind == 4 {
            assert_eq!(end, bytes.len());
            break offset;
        }
        offset = end;
    };
    let scratch = env::temp_dir().join(format!("birnam-image-policy-{}", std::process::id()));
    fs::create_dir(&scratch).unwrap();
    let without_code = scratch.join("without-code.image");
    fs::write(&without_code, &bytes[..code_start]).unwrap();
    let executable = env::current_exe().unwrap();
    for case in ["mapped", "mapped_failure", "without_code", "rebuild"] {
        let output = Command::new(&executable)
            .args(["--exact", TEST, "--ignored", "--nocapture"])
            .env("BIRNAM_IMAGE_POLICY_CASE", case)
            .env(
                "BIRNAM_IMAGE_POLICY_PATH",
                if case == "without_code" {
                    &without_code
                } else {
                    &compiled
                },
            )
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{case}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fs::remove_dir_all(scratch).unwrap();
}
