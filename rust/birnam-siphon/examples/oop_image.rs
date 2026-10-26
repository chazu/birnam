//! Fresh-process image acceptance with the source parser disabled. Mapped loads
//! additionally disable the native compiler, so successful output is not fallback.
use birnam_abi::{self as abi, Oop};
use std::{os::unix::ffi::OsStrExt, path::PathBuf};
fn main() {
    unsafe {
        let directory = PathBuf::from(std::env::args().nth(1).unwrap());
        let path = PathBuf::from(std::env::args().nth(2).unwrap());
        let rebuild = std::env::args().nth(3).as_deref() == Some("rebuild");
        let bootstrap = directory.join("libbirnam_bootstrap.dylib");
        let names: &[&[u8]] = &[
            b"sequence",
            b"at:",
            b"nameWithBytes",
            b"Roots",
            b"retain:",
            b"String",
            b"fromBytes:",
            b"methodAt:",
            b"deny",
            b"calls",
            b"behaviorOf:",
            b"addMethod",
            b"Lang",
            b"parse:",
            b"TCC",
            b"compile:",
            b"compileObject:",
            b"System",
            b"startImageMapped:",
            b"startImage:rebuild:",
            b"true",
            b"false",
            b"nil",
            b"Globals",
            b"main",
            b"value",
            b"invokeOn:arguments:",
            b"GC",
            b"collectPrecisely",
        ];
        let mut seeds = vec![
            bootstrap.as_os_str().as_bytes(),
            directory.as_os_str().as_bytes(),
        ];
        seeds.extend_from_slice(names);
        let id = birnam_siphon::load(
            directory.join("libbirnam_id.dylib").as_os_str().as_bytes(),
            &seeds,
        )
        .unwrap();
        let sel = |n: &[u8]| {
            id.send(
                id.literal(names.iter().position(|v| *v == n).unwrap() + 2)
                    .unwrap(),
                id.intern,
                &[],
            )
            .unwrap()
        };
        let send = |r, n: &[u8], a: &[Oop]| {
            id.send(r, sel(n), a)
                .unwrap_or_else(|e| panic!("{} failed {e:?}", String::from_utf8_lossy(n)))
        };
        let args = send(id.root, b"sequence", &[id.literal(1).unwrap()]);
        let ns = id.send(id.literal(0).unwrap(), id.load, &[args]).unwrap();
        let named = |n: &[u8]| send(ns, b"at:", &[sel(n)]);
        let roots = named(b"Roots");
        let keep = |v| {
            send(roots, b"retain:", &[v]);
            v
        };
        let text = |b: &[u8]| {
            let data: Vec<_> = b.iter().map(|b| abi::size(*b as usize).unwrap()).collect();
            let data = keep(send(id.root, b"sequence", &data));
            keep(send(named(b"String"), b"fromBytes:", &[data]))
        };
        let fixture = directory.join("no-compiler.dylib");
        let bytes: Vec<_> = fixture
            .as_os_str()
            .as_bytes()
            .iter()
            .map(|b| abi::size(*b as usize).unwrap())
            .collect();
        let fixture = keep(send(id.root, b"nameWithBytes", &bytes));
        let empty = keep(send(id.root, b"sequence", &[]));
        let fixture = keep(id.send(fixture, id.load, &[empty]).unwrap());
        let deny = keep(send(fixture, b"methodAt:", &[sel(b"deny")]));
        let replace = |owner: &[u8], name: &[u8]| {
            let vt = keep(send(id.root, b"behaviorOf:", &[named(owner)]));
            send(vt, b"addMethod", &[sel(name), deny]);
        };
        replace(b"Lang", b"parse:");
        if !rebuild {
            replace(b"TCC", b"compile:");
            replace(b"TCC", b"compileObject:");
        }
        let path = text(path.as_os_str().as_bytes());
        let result = if rebuild {
            send(
                named(b"System"),
                b"startImage:rebuild:",
                &[path, named(b"true")],
            )
        } else {
            send(named(b"System"), b"startImageMapped:", &[path])
        };
        assert_eq!(result, named(if rebuild { b"false" } else { b"true" }));
        send(named(b"GC"), b"collectPrecisely", &[]);
        let main = keep(send(named(b"Globals"), b"at:", &[sel(b"main")]));
        assert_ne!(main, named(b"nil"));
        send(main, b"value", &[]);
        let counter = keep(send(fixture, b"methodAt:", &[sel(b"calls")]));
        assert_eq!(
            abi::as_size(send(counter, b"invokeOn:arguments:", &[fixture, empty])),
            Some(0)
        );
        println!(
            "OOP_IMAGE_{}_OK",
            if rebuild { "REBUILT" } else { "MAPPED" }
        );
    }
}
