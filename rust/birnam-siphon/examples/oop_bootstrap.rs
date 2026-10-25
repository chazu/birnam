//! A fresh Id process booting the compiler seed without the integration fixture.
use birnam_abi as abi;
use std::{os::unix::ffi::OsStrExt, path::PathBuf};
fn main() {
    unsafe {
        let directory = PathBuf::from(std::env::args().nth(1).expect("native directory"));
        let root = PathBuf::from(std::env::args().nth(2).expect("checkout"));
        let id_path = directory.join("libbirnam_id.dylib");
        let bootstrap = directory.join("libbirnam_bootstrap.dylib");
        let selectors: &[&[u8]] = &[
            b"Error",
            b"Globals",
            b"SourceRegistry",
            b"faultKind",
            b"detail",
            b"mappingFailed",
            b"Lang",
            b"behaviorOf:",
            b"lookup",
            b"addMethod",
            b"objectBehavior",
            b"yourself",
            b"parse:",
            b"compile:output:from:synchronousBodies:",
            b"load:",
            b"System",
            b"loadModule:arguments:",
            b"loadModule:",
            b"moduleError",
            b"ioError",
            b"notAModule",
            b"sequence",
            b"at:",
            b"at:put:",
            b"nameWithBytes",
            b"fromBytes:",
            b"eval:",
            b"isError",
            b"kind",
            b"wordOf:",
            b"low32",
            b"Codegen",
            b"String",
            b"Integer",
            b"true",
            b"nil",
            b"Roots",
            b"retain:",
            b"release:",
            b"GC",
            b"collectPrecisely",
            b"Bootstrap",
            b"loadResultAt:arguments:",
            b"loaded",
            b"bytes",
            b"sizeWord",
            b"atWord:",
        ];
        let mut seeds = vec![
            bootstrap.as_os_str().as_bytes(),
            directory.as_os_str().as_bytes(),
        ];
        seeds.extend_from_slice(selectors);
        let id = birnam_siphon::load(id_path.as_os_str().as_bytes(), &seeds).unwrap();
        // Seed Names are retained by Id. Intern at use until Bootstrap has
        // constructed Roots; a native Vec of Symbols is not a GC root.
        let sel = |name: &[u8]| {
            let i = selectors.iter().position(|n| *n == name).unwrap();
            id.send(id.literal(i + 2).unwrap(), id.intern, &[]).unwrap()
        };
        let send = |r, n: &[u8], a: &[abi::Oop]| {
            id.send(r, sel(n), a).unwrap_or_else(|e| {
                panic!(
                    "bootstrap caller {} failed {e:?}",
                    String::from_utf8_lossy(n)
                )
            })
        };
        let args = send(id.root, b"sequence", &[id.literal(1).unwrap()]);
        let loaded = id.send(id.literal(0).unwrap(), id.load, &[args]);
        if std::env::args().nth(3).as_deref() == Some("published-rejection") {
            let error =
                loaded.expect_err("published bootstrap must require the complete owner closure");
            assert_eq!(send(error, b"faultKind", &[]), sel(b"mappingFailed"));
            let detail = send(error, b"detail", &[]);
            let path = id
                .read_bytes(detail, sel(b"sizeWord"), sel(b"atWord:"))
                .unwrap();
            assert!(path.ends_with(b"libbirnam_portable_code.dylib"));
            let load = |name: &str, args: &[abi::Oop]| {
                let path = directory.join(format!("libbirnam_{name}.dylib"));
                let values: Vec<_> = path
                    .as_os_str()
                    .as_bytes()
                    .iter()
                    .map(|b| abi::size(*b as usize).unwrap())
                    .collect();
                let path = send(id.root, b"nameWithBytes", &values);
                let args = send(id.root, b"sequence", args);
                id.send(path, id.load, &[args]).unwrap()
            };
            let roots = load("root_registry", &[]);
            let ns = load("namespace", &[roots]);
            assert!(
                send(ns, b"at:", &[sel(b"Lang")]).is_null(),
                "published mode must not load the grammar seed"
            );
            assert!(!send(ns, b"at:", &[sel(b"Error")]).is_null());
            let globals = send(ns, b"at:", &[sel(b"Globals")]);
            let registry = send(globals, b"at:", &[sel(b"SourceRegistry")]);
            assert!(!registry.is_null());
            println!("OOP_PUBLISHED_BOOTSTRAP_REQUIRES_PROVIDERS_OK");
            return;
        }
        let ns = loaded.expect("cold compiler bootstrap");
        assert_eq!(id.send(id.literal(0).unwrap(), id.load, &[args]), Ok(ns));
        let named = |n: &[u8]| send(ns, b"at:", &[sel(n)]);
        let roots = named(b"Roots");
        let gc = named(b"GC");
        let codegen = named(b"Codegen");
        let string = named(b"String");
        let integer = named(b"Integer");
        let keep = |v| {
            send(roots, b"retain:", &[v]);
            v
        };
        let text = |b: &[u8]| {
            let a: Vec<_> = b.iter().map(|b| abi::size(*b as usize).unwrap()).collect();
            let values = send(id.root, b"sequence", &a);
            keep(send(string, b"fromBytes:", &[values]))
        };
        let eval = |s: &str| {
            let value = keep(send(codegen, b"eval:", &[text(s.as_bytes())]));
            assert_ne!(send(value, b"isError", &[]), named(b"true"), "{s}");
            value
        };
        assert_eq!(
            abi::as_size(send(integer, b"wordOf:", &[eval("[1 + 2]")])),
            Some(3)
        );
        send(gc, b"collectPrecisely", &[]);
        for name in [
            "cell",
            "modules",
            "printing",
            "boolean",
            "stdlib",
            "block-loop-control",
            "block-loop",
            "range",
            "system-gc",
            "stackless-control",
            "native-loader",
        ] {
            eval(&std::fs::read_to_string(root.join(format!("modules/{name}.birnam"))).unwrap());
        }
        let outcome = eval("[[TaskControl start: [:task | 42]] pollOutcome]");
        keep(outcome);
        assert_eq!(
            eval("[[[TaskControl start: [:task | 42]] pollOutcome] = #[#success 42]]"),
            named(b"true")
        );
        send(gc, b"collectPrecisely", &[]);
        let lang_source = std::fs::read_to_string(root.join("modules/lang.birnam")).unwrap();
        let output = directory.join("libbirnam_lang.dylib");
        let output_value = text(output.as_os_str().as_bytes());
        assert_eq!(
            send(
                codegen,
                b"compile:output:from:synchronousBodies:",
                &[
                    text(lang_source.as_bytes()),
                    output_value,
                    text(b"modules/lang.birnam"),
                    named(b"true")
                ]
            ),
            output_value
        );
        let seed = named(b"Lang");
        let seed_vt = send(id.root, b"behaviorOf:", &[seed]);
        let base = send(id.root, b"objectBehavior", &[]);
        let reject = send(base, b"lookup", &[sel(b"yourself")]);
        send(seed_vt, b"addMethod", &[sel(b"parse:"), reject]);
        let language = send(codegen, b"load:", &[output_value]);
        assert_ne!(language, seed);
        assert_eq!(named(b"Lang"), language);
        send(gc, b"collectPrecisely", &[]);
        assert_eq!(
            abi::as_size(send(integer, b"wordOf:", &[eval("[40 + 2]")])),
            Some(42)
        );
        println!("OOP_COMPILED_LANG_HANDOFF_OK");
        let fixture = directory.join("returns.dylib");
        let path = text(fixture.as_os_str().as_bytes());
        let immediate = abi::size(73).unwrap();
        let bootstrap = named(b"Bootstrap");
        let raw_path = keep(send(path, b"bytes", &[]));
        let args = eval("#[nil]");
        // Source-level native loading preserves arbitrary Oop results and maps faults.
        let value = eval(&format!(
            "[System loadModule: \"{}\" arguments: #[41]]",
            fixture.display()
        ));
        assert_eq!(abi::as_size(send(integer, b"wordOf:", &[value])), Some(41));
        assert_eq!(
            abi::as_size(send(bootstrap, b"release:", &[value])),
            Some(1)
        );
        assert_eq!(
            abi::as_size(send(bootstrap, b"release:", &[value])),
            Some(0)
        );
        let sequence = keep(send(id.root, b"sequence", &[immediate]));
        let native_path_values: Vec<_> = fixture
            .as_os_str()
            .as_bytes()
            .iter()
            .map(|b| abi::size(*b as usize).unwrap())
            .collect();
        let native_path = send(id.root, b"nameWithBytes", &native_path_values);
        assert_eq!(id.send(native_path, id.load, &[sequence]), Ok(immediate));
        let result = keep(send(
            bootstrap,
            b"loadResultAt:arguments:",
            &[raw_path, args],
        ));
        assert_eq!(send(result, b"at:", &[eval("0")]), sel(b"loaded"));
        assert_eq!(send(result, b"at:", &[eval("1")]), named(b"nil"));
        let absent = text(directory.join("absent.dylib").as_os_str().as_bytes());
        let missing = send(named(b"System"), b"loadModule:", &[absent]);
        assert_eq!(send(missing, b"kind", &[]), sel(b"ioError"));
        let rejected = send(named(b"System"), b"loadModule:", &[path]);
        assert_eq!(send(rejected, b"kind", &[]), sel(b"moduleError"));
        let bad = text(directory.join("not-module.dylib").as_os_str().as_bytes());
        let not_module = keep(send(named(b"System"), b"loadModule:", &[bad]));
        assert_eq!(send(not_module, b"kind", &[]), sel(b"notAModule"));
        println!("OOP_NATIVE_LOADER_STATUS_OK");
        println!("OOP_COLD_COMPILER_BOOTSTRAP_OK");
    }
}
