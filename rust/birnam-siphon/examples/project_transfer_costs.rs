//! Measure project resolution using the same observed snapshot repeatedly.
#[allow(dead_code)]
#[path = "../tests/support/oop.rs"]
mod oop;
use birnam_host_support::project::ProjectResolutionObservation;
use birnam_project_support::ProjectSnapshot;
use std::{os::unix::ffi::OsStrExt, time::Instant};
fn main() {
    let legacy = std::env::args().any(|v| v == "--legacy-json");
    let h = oop::bootstrap();
    let q = h.scope();
    let r = &h.resources;
    let mut args = vec![h.namespace];
    for path in [
        &r.prefix,
        &r.lib_dir,
        &r.rust_workspace_dir,
        &r.guide_dir,
        &r.id_dylib,
    ] {
        args.push(q.string(path.as_os_str().as_bytes()).unwrap());
    }
    for name in [
        "host_entry",
        "host_project",
        "project_build",
        "host_interaction",
    ] {
        h.load_native(
            &q,
            &r.lib_dir.join(format!("libbirnam_{name}.dylib")),
            &args,
        )
        .unwrap();
    }
    h.owner_call(
        &q,
        b"HostEntry",
        b"bootstrap:tools:",
        &[h.named(b"false").unwrap(), h.named(b"false").unwrap()],
    )
    .unwrap();
    h.owner_call(&q, b"HostProject", b"support", &[]).unwrap();
    let path = std::env::temp_dir().join(format!("birnam-transfer-bench-{}", std::process::id()));
    std::fs::create_dir_all(path.join("src")).unwrap();
    std::fs::write(
        path.join("birnam.toml"),
        b"name=\"benchmark\"\nload=[\"example\"]\n",
    )
    .unwrap();
    for bytes in [8192usize, 262144] {
        let source = format!(
            "[Module named: #example requires: #[]];\n// {}\ntrue",
            "a".repeat(bytes)
        );
        std::fs::write(path.join("src/example.birnam"), source.as_bytes()).unwrap();
        let input = h
            .owner_call(
                &q,
                b"HostProject",
                b"snapshot:",
                &[q.string(path.as_os_str().as_bytes()).unwrap()],
            )
            .unwrap();
        let snapshot: Option<ProjectSnapshot> =
            legacy.then(|| serde_json::from_slice(&q.string_bytes(input).unwrap()).unwrap());
        for sample in 0..7 {
            q.call(h.named(b"GC").unwrap(), b"collectPrecisely", &[])
                .unwrap();
            let calls = 5;
            let start = Instant::now();
            for _ in 0..calls {
                let scope = h.scope();
                let argument = match &snapshot {
                    Some(v) => scope.string(&serde_json::to_vec(v).unwrap()).unwrap(),
                    None => input,
                };
                let result = h
                    .owner_call(&scope, b"HostProject", b"resolveSnapshot:", &[argument])
                    .unwrap();
                let result: ProjectResolutionObservation = if legacy {
                    serde_json::from_slice(&scope.string_bytes(result).unwrap()).unwrap()
                } else {
                    birnam_project_support::values::decode_resolution(&scope, result).unwrap()
                };
                assert_eq!(result.modules.len(), 1);
                assert_eq!(result.modules[0].source, source.as_bytes());
            }
            let ns = start.elapsed().as_nanos() as f64 / calls as f64;
            println!(
                "{{\"operation\":\"project_resolution\",\"bytes\":{bytes},\"sample\":{sample},\"calls\":{calls},\"ns_per_call\":{ns}}}"
            );
        }
    }
    std::fs::remove_dir_all(path).unwrap();
}
