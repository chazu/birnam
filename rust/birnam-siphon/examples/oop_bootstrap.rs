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
            b"CodeObject",
            b"false",
            b"none",
            b"message",
            b"invalidEmbeddedLibrary",
            b"Bytes",
            b"fromCanonicalBytes:",
            b"canonicalBytesOf:",
            b"CodeNode",
            b"size",
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
        if std::env::args().nth(3).as_deref() == Some("published-missing") {
            let error = loaded.expect_err("published Bootstrap requires every startup artifact");
            assert_eq!(send(error, b"kind", &[]), sel(b"ioError"));
            println!("OOP_PUBLISHED_MISSING_ARTIFACT_REJECTED");
            return;
        }
        let ns = loaded.expect("Bootstrap initialization");
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
            if send(value, b"isError", &[]) == named(b"true") {
                let message = keep(send(value, b"message", &[]));
                let bytes = keep(send(message, b"bytes", &[]));
                panic!(
                    "{s}: {:?}",
                    id.read_bytes(bytes, sel(b"sizeWord"), sel(b"atWord:"))
                        .map(|b| String::from_utf8_lossy(&b).into_owned())
                );
            }
            value
        };
        assert_eq!(
            abi::as_size(send(integer, b"wordOf:", &[eval("[1 + 2]")])),
            Some(3)
        );
        send(gc, b"collectPrecisely", &[]);
        if std::env::args().nth(3).as_deref() == Some("published") {
            assert_eq!(eval("[[Module libDir] = [Module libDir]]"), named(b"true"));
            assert_eq!(eval("[[Alien alloc: 16] isNull]"), named(b"false"));
            assert_eq!(
                eval(
                    "[[| let p := [Alien alloc: 16]; [p pokeInt: -9223372036854775808 at: 0]; let result := [[p peekIntAt: 0] = -9223372036854775808]; [p free]; result] value]"
                ),
                named(b"true")
            );
            eval(
                "[Globals at: #Roundtrip put: [Isolate decode: [Isolate encode: #[1 -9223372036854775808 3.5 \"hello\" #symbol]]]]",
            );
            for (index, value) in ["1", "-9223372036854775808", "3.5", "\"hello\"", "#symbol"]
                .iter()
                .enumerate()
            {
                let comparison = if index == 4 { "==" } else { "=" };
                assert_eq!(
                    eval(&format!("[[Roundtrip at: {index}] {comparison} {value}]")),
                    named(b"true"),
                    "roundtrip index {index}"
                );
            }
            assert_eq!(
                eval("[[Isolate encode: [Object clone]] isError]"),
                named(b"true")
            );
            let generated = eval("[AlienCompiler compile: \"labs\" signature: \"l l\"]");
            let generated = keep(send(generated, b"bytes", &[]));
            std::fs::write(
                directory.join("alien-generated.c"),
                id.read_bytes(generated, sel(b"sizeWord"), sel(b"atWord:"))
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(
                eval(
                    "[Alien wrap: \"labs\" signature: \"l l\" as: #publishedAbsolute on: Integer]"
                ),
                integer
            );
            assert_eq!(eval("[[-42 publishedAbsolute] = 42]"), named(b"true"));
            assert_eq!(
                eval(&format!(
                    "[Alien dlopen: \"{}\"]",
                    directory.join("foreign.dylib").display()
                )),
                named(b"true")
            );
            eval(
                "[Alien wrap: \"oop_callback\" signature: \"l p l\" as: #callPublished: on: ForeignAddress]",
            );
            eval(
                "[Globals at: #PublishedCallback put: [Alien callback: [:value | [GC collectPrecisely]; [value + 1]] signature: \"l l\"]]",
            );
            assert_eq!(
                eval("[[PublishedCallback callPublished: 41] = 42]"),
                named(b"true")
            );
            eval("[Alien releaseCallback: PublishedCallback]");
            assert_eq!(eval("[PublishedCallback isNull]"), named(b"true"));
            eval("[Alien wrap: \"oop_string\" signature: \"s s\" as: #publishedCopy on: String]");
            assert_eq!(
                eval("[[\"copied\" publishedCopy] = \"copied\"]"),
                named(b"true")
            );
            eval("[Alien wrap: \"oop_double\" signature: \"d d\" as: #publishedCopy on: Float]");
            assert_eq!(eval("[[3.25 publishedCopy] = 3.25]"), named(b"true"));
            eval("[Alien wrap: \"oop_unsigned\" signature: \"u u\" as: #publishedCopy on: Word]");
            assert_eq!(
                eval(
                    "[[[Execution wordResultOf: -1 selector: #word] publishedCopy] = [Execution wordResultOf: -1 selector: #word]]"
                ),
                named(b"true")
            );
            assert_eq!(
                eval(
                    "[[| let fake := [Object clone]; [Reflect defineMethod: #bytes on: fake as: [:self | self]]; [Reflect defineMethod: #sizeWord on: fake as: [:self | [Integer wordOf: 268435457]]]; [[Alien provideLibrary: fake named: \"oversized\"] kind]] value]"
                ),
                sel(b"invalidEmbeddedLibrary")
            );
            println!("OOP_PUBLISHED_ALIEN_CALLBACK_OK");

            assert_eq!(eval("[[System imageSaveReport] status]"), sel(b"none"));
            send(gc, b"collectPrecisely", &[]);
            assert_eq!(
                abi::as_size(send(integer, b"wordOf:", &[eval("[40 + 2]")])),
                Some(42)
            );
            println!("OOP_FULL_PUBLISHED_STARTUP_OK");
            return;
        }
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
        let portable_path = directory.join("libbirnam_portable_code.dylib");
        let values: Vec<_> = portable_path
            .as_os_str()
            .as_bytes()
            .iter()
            .map(|b| abi::size(*b as usize).unwrap())
            .collect();
        let portable_path = keep(send(id.root, b"nameWithBytes", &values));
        let args = keep(send(id.root, b"sequence", &[ns]));
        id.send(portable_path, id.load, &[args]).unwrap();
        assert_eq!(
            eval(
                "[[| let p := [CodeProgram parameters: #[\"x\"] captures: #[] statements: #[[CodeStatement expression: [CodeNode lexicalReadAt: 0]]]]; let code := [CodeObject fromProgram: p]; let bytes := [CodeObject canonicalBytesOf: code]; [GC collectPrecisely]; let copy := [CodeObject fromCanonicalBytes: bytes]; [[[copy program] arity] = 1]] value]"
            ),
            named(b"true")
        );
        assert_eq!(
            eval("[[[CodeNode integer: -9223372036854775808] integer] = -9223372036854775808]"),
            named(b"true")
        );
        let mut checked = 0;
        for entry in std::fs::read_dir(root.join("tests/fixtures/portable-code-v1")).unwrap() {
            let path = entry.unwrap().path();
            if !path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("program-")
            {
                continue;
            }
            let bytes = std::fs::read(&path).unwrap();
            let valid = birnam_artifact_codec::portable_program::inspect(&bytes).is_ok();
            let values: Vec<_> = bytes
                .iter()
                .map(|b| abi::size(*b as usize).unwrap())
                .collect();
            let values = keep(send(id.root, b"sequence", &values));
            let input = keep(send(named(b"Bytes"), b"fromBytes:", &[values]));
            let value = keep(send(named(b"CodeObject"), b"fromCanonicalBytes:", &[input]));
            if valid {
                assert_ne!(send(value, b"isError", &[]), named(b"true"));
                send(gc, b"collectPrecisely", &[]);
                let actual = keep(send(named(b"CodeObject"), b"canonicalBytesOf:", &[value]));
                let actual = keep(send(actual, b"bytes", &[]));
                assert_eq!(
                    id.read_bytes(actual, sel(b"sizeWord"), sel(b"atWord:"))
                        .unwrap(),
                    bytes
                );
            } else {
                assert_eq!(send(value, b"isError", &[]), named(b"true"));
            }
            checked += 1;
        }
        assert!(checked >= 10);
        println!("OOP_PORTABLE_CODE_OWNER_OK fixtures={checked}");
        let outcome = eval("[[TaskControl start: [:task | 42]] pollOutcome]");
        keep(outcome);
        assert_eq!(
            eval("[[[TaskControl start: [:task | 42]] pollOutcome] = #[#success 42]]"),
            named(b"true")
        );
        send(gc, b"collectPrecisely", &[]);
        for (name, file) in [
            ("nativeValues", "native-values"),
            ("integerValues", "integer"),
            ("trace", "trace"),
        ] {
            let source =
                std::fs::read_to_string(root.join(format!("modules/{file}.birnam"))).unwrap();
            let output = directory.join(format!("libbirnam_{name}.dylib"));
            let path = text(output.as_os_str().as_bytes());
            assert_eq!(
                send(
                    codegen,
                    b"compile:output:from:synchronousBodies:",
                    &[
                        text(source.as_bytes()),
                        path,
                        text(format!("modules/{file}.birnam").as_bytes()),
                        named(b"true")
                    ]
                ),
                path
            );
            let value = keep(send(codegen, b"load:", &[path]));
            assert_ne!(send(value, b"isError", &[]), named(b"true"), "{name}");
        }
        assert_eq!(
            abi::as_size(send(integer, b"wordOf:", &[eval("[40 + 2]")])),
            Some(42)
        );
        assert_eq!(
            eval("[[9223372036854775807 - 1] = 9223372036854775806]"),
            named(b"true")
        );
        assert_eq!(
            eval("[[-9223372036854775808 + 1] = -9223372036854775807]"),
            named(b"true")
        );
        assert_eq!(
            eval(
                "[[| let original := [Storage methodFor: #observe:]; let target := [Object clone]; let finished := [false asCell]; let observer := [Object clone]; [Reflect defineMethod: #reached:identity: on: observer as: [:self :value :identity | nil]]; [Reflect defineMethod: #notReached:identity: on: observer as: [:self :value :identity | nil]]; [Reflect defineMethod: #complete: on: observer as: [:self :gaps | [finished set: true]]]; [Reflect defineMethod: #observe: on: Storage as: [:self :visitor | [visitor allocation: target bytes: [Vtable allocationSizeOf: target] identity: [Vtable identityOf: target]]; [visitor root: target]; [visitor complete: [Integer wordOf: 0]]]]; [Trace report: observer acceptingGaps: [Integer wordOf: 0]]; [[Vtable behaviorOf: Storage] addMethod: original as: #observe:]; [finished value]] value]"
            ),
            named(b"true")
        );
        println!("OOP_NATIVE_FACING_SOURCE_OK");
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
        for (name, file) in [
            ("cell", "cell"),
            ("modules", "modules"),
            ("booleanValues", "boolean"),
            ("blockLoopControl", "block-loop-control"),
            ("blockLoop", "block-loop"),
            ("range", "range"),
            ("systemGc", "system-gc"),
            ("printing", "printing"),
            ("sourceLoader", "source-loader"),
            ("nativeLoader", "native-loader"),
            ("alienCompiler", "alien-compiler"),
            ("alienRecipes", "alien-recipes"),
            ("imageFormat", "image-format"),
            ("globals", "globals"),
            ("methodInvocation", "method-invocation"),
        ] {
            let source_path = root.join(format!("modules/{file}.birnam"));
            let source = std::fs::read(&source_path).unwrap();
            let path = text(
                directory
                    .join(format!("libbirnam_{name}.dylib"))
                    .as_os_str()
                    .as_bytes(),
            );
            assert_eq!(
                send(
                    codegen,
                    b"compile:output:from:synchronousBodies:",
                    &[
                        text(&source),
                        path,
                        text(source_path.as_os_str().as_bytes()),
                        named(if name == "methodInvocation" {
                            b"false"
                        } else {
                            b"true"
                        })
                    ]
                ),
                path,
                "compile {name}"
            );
            println!("OOP_PUBLISHED_ARTIFACT {name}");
        }
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
