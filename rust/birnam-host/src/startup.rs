use crate::{Resources,StartupPolicy,runtime::{HostError,HostRuntime}};
#[cfg(test)]use crate::runtime::object;
#[cfg(test)]use std::{env,path::Path,os::unix::ffi::OsStrExt};
impl HostRuntime {
 pub fn start(resources:&Resources,policy:StartupPolicy)->Result<Self,HostError>{let host=Self::start_native(resources,policy.precise_gc)?;if !policy.skip_stdlib {host.bootstrap(policy.source_bootstrap,policy.tool_bootstrap)?;}Ok(host)}
 pub(crate) fn bootstrap(&self,source:bool,tools:bool)->Result<(),HostError>{self.call(b"HostEntry",b"bootstrap:tools:",&[self.named(if source{b"true"}else{b"false"})?,self.named(if tools{b"true"}else{b"false"})?])}
}
#[cfg(test)]
#[path = "lifecycle_test.rs"]
mod lifecycle_test;

#[cfg(test)]
#[path = "image_loading_test.rs"]
mod image_loading_test;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_hooks::Hooks;
    use std::path::PathBuf;

    #[test]
    #[ignore = "requires an installed prefix"]
    fn installed_bootstrap_and_console_do_not_compile_or_parse() {
        let lib = PathBuf::from(env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").unwrap());
        let resources = crate::resolve_from_exe_dir(
            Path::new("/unused/bin"),
            crate::Overrides {
                lib_dir: Some(&lib),
                ..Default::default()
            },
        )
        .unwrap();
        let host = HostRuntime::start_native(&resources, true).unwrap();
        let hooks = Hooks::new(&host.context);
        let fixture =
            env::temp_dir().join(format!("birnam-compiled-metadata-{}.o", std::process::id()));
        {
            let q = host.scope();
            let source = q.string(b"[Module named: #metadataFixture requires: #[]]; [Reflect defineMethod: #metadataFixtureAnswer on: Integer as: [:self | 42]]; true").unwrap();
            let fixture_path = q.string(fixture.as_os_str().as_bytes()).unwrap();
            let result = q
                .call(
                    host.named(b"Codegen").unwrap(),
                    b"compile:output:",
                    &[source, fixture_path],
                )
                .unwrap();
            q.check_result(result).unwrap();
            assert_eq!(result, fixture_path);
            let key = host.context.intern(b"Store").unwrap();
            let present = q
                .call(host.named(b"Namespace").unwrap(), b"includesKey:", &[key])
                .unwrap();
            assert_eq!(
                present,
                host.named(b"false").unwrap(),
                "artifact compilation activated Store"
            );
        }
        let calls = hooks.context(&[hooks.fault(98765), birnam_abi::size(0).unwrap()]);
        let deny = hooks.method(b"deny", calls);
        let mut originals = Vec::new();
        for (family, name) in [
            (b"TCCVt".as_slice(), b"compile:".as_slice()),
            (b"TCCVt", b"compileObject:"),
            (b"LangVt", b"parse:"),
            (b"PegGrammarVt", b"parse:from:"),
            (
                b"NativeBackendVt",
                b"emit:source:filename:persistent:synchronous:",
            ),
            (b"NativeBackendVt", b"factory:source:dispositions:"),
        ] {
            let tcc = if family == b"LangVt" || family == b"NativeBackendVt" {
                let owner = if family == b"LangVt" {
                    b"Lang".as_slice()
                } else {
                    b"NativeBackend".as_slice()
                };
                host.send(
                    host.context.id.root,
                    b"behaviorOf:",
                    &[host.named(owner).unwrap()],
                )
                .unwrap()
            } else {
                host.named(family).unwrap()
            };
            let selector = host.context.intern(name).unwrap();
            let original = hooks.capture(tcc, selector);
            host.send(tcc, b"addMethod", &[selector, deny]).unwrap();
            originals.push((tcc, selector, original));
        }
        // Resources are now owned by native HostEntry; changing the facade's
        // private copy cannot simulate a missing bootstrap. Exercise the real
        // file boundary, restoring the fixture even if an assertion unwinds.
        struct MissingBootstrap {
            original: PathBuf,
            saved: PathBuf,
        }
        impl Drop for MissingBootstrap {
            fn drop(&mut self) {
                let _ = std::fs::rename(&self.saved, &self.original);
            }
        }
        let original = host
            .context
            .resources
            .lib_dir
            .join("birnam-bootstrap.image");
        let saved = original.with_extension(format!("disabled-{}", std::process::id()));
        std::fs::rename(&original, &saved).unwrap();
        let missing = MissingBootstrap { original, saved };
        let error = host.bootstrap(false, false).unwrap_err();
        assert!(error.0.contains("reinstall Birnam"), "{error}");
        assert_eq!(
            hooks.count(calls, 1),
            0,
            "missing bootstrap invoked a source fallback"
        );
        drop(missing);
        host.bootstrap(false, false).unwrap();
        for name in [b"repl".as_slice(), b"socket", b"config", b"toml", b"file"] {
            host.load_tool(name).unwrap();
        }
        let q = host.scope();
        let cg = host.named(b"Codegen").unwrap();
        let path = q.string(fixture.as_os_str().as_bytes()).unwrap();
        let result = q.call(cg, b"load:", &[path]).unwrap();
        q.check_result(result).unwrap();
        assert_eq!(result, host.named(b"true").unwrap());
        std::fs::remove_file(&fixture).unwrap();
        let name = host.context.intern(b"stdlib").unwrap();
        let source = q.call(cg, b"moduleSourceFor:", &[name]).unwrap();
        let source_owner = host.global(&q, b"SourceRegistry").unwrap();
        let _record = q.call(cg, b"compiledSourceFor:", &[source]).unwrap();
        let snapshot = q.call(source_owner, b"sourceRegistry", &[]).unwrap();
        let mut row_index = 0;
        for i in 1..q.vector_size(snapshot).unwrap() {
            if q.vector_at(q.vector_at(snapshot, i).unwrap(), 0).unwrap() == name {
                row_index = i;
                break;
            }
        }
        assert_ne!(row_index, 0);
        let owner_vt = q
            .call(host.context.id.root, b"behaviorOf:", &[source_owner])
            .unwrap();
        let selector = host.context.intern(b"sourceRegistry").unwrap();
        let _original_method = q.call(source_owner, b"methodFor:", &[selector]).unwrap();
        let original = hooks.capture(owner_vt, selector);
        let malformed = q.call(host.named(b"Vector").unwrap(), b"new", &[]).unwrap();
        let replacing = |vector, index, value| {
            let mut out = q.call(host.named(b"Vector").unwrap(), b"new", &[]).unwrap();
            for i in 0..q.vector_size(vector).unwrap() {
                let item = if i == index {
                    value
                } else {
                    q.vector_at(vector, i).unwrap()
                };
                out = q.call(out, b"add:", &[item]).unwrap();
            }
            out
        };
        let old_shape = replacing(snapshot, 0, host.named(b"false").unwrap());
        assert!(
            q.call(cg, b"preparePortableImageSources:", &[snapshot])
                .is_err(),
            "image source staging accepted an obsolete registry shape"
        );
        host.send(
            owner_vt,
            b"addMethod",
            &[selector, hooks.method(b"fixed", old_shape)],
        )
        .unwrap();
        assert!(
            q.call(cg, b"initialGenerationProjection", &[]).is_err(),
            "old sealed-flag source registry accepted"
        );
        for (bad, is_missing) in [(host.named(b"nil").unwrap(), true), (malformed, false)] {
            let bad_record = q
                .call(cg, b"compiledSource:metadata:", &[source, bad])
                .unwrap();
            let row = q.vector_at(snapshot, row_index).unwrap();
            let bad_row = replacing(row, 1, bad_record);
            let bad_snapshot = replacing(snapshot, row_index, bad_row);
            host.send(
                owner_vt,
                b"addMethod",
                &[selector, hooks.method(b"fixed", bad_snapshot)],
            )
            .unwrap();
            let rejected = q.call(cg, b"initialGenerationProjection", &[]);
            if is_missing {
                let error = rejected.unwrap();
                assert_eq!(
                    q.call(error, b"kind", &[]).unwrap(),
                    hooks.symbol(b"compileError")
                );
                assert!(q.check_result(error).unwrap_err().0.contains(
                    "compiled source metadata is missing; rebuild the artifact or image"
                ));
            } else {
                assert!(rejected.is_err(), "malformed metadata accepted")
            }
            assert_eq!(
                hooks.count(calls, 1),
                0,
                "metadata rejection invoked a parser fallback"
            );
        }
        host.send(owner_vt, b"addMethod", &[selector, original])
            .unwrap();
        let docs = q.call(cg, b"recordedMethodSources", &[]).unwrap();
        assert!(q.vector_size(docs).unwrap() > 0);
        let repl = host.global(&q, b"Repl").unwrap();
        let result = q.call(repl, b"consoleHostInitNoConfig", &[]).unwrap();
        q.check_result(result).unwrap();
        assert_eq!(
            q.call(
                host.global(&q, b"DispatchGeneration").unwrap(),
                b"current",
                &[]
            )
            .unwrap(),
            host.named(b"nil").unwrap()
        );
        let result = q.call(repl, b"consoleHostReadyNoMain", &[]).unwrap();
        q.check_result(result).unwrap();
        let result = q.call(repl, b"consoleDone", &[]).unwrap();
        q.check_result(result).unwrap();
        host.bootstrap(false, true).unwrap();
        for name in [b"lsp".as_slice(), b"cli", b"doc"] {
            host.load_tool(name).unwrap();
        }
        assert_eq!(
            hooks.count(calls, 1),
            0,
            "bootstrap or console invoked the compiler or parser"
        );
        for (tcc, selector, original) in originals {
            host.send(tcc, b"addMethod", &[selector, original]).unwrap();
        }
    }

    #[test]
    #[ignore = "requires production installed native modules; scripts/host-cli-test.sh"]
    fn host_roots_survive_collecting_sends() {
        let lib = PathBuf::from(env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").unwrap());
        let resources = crate::resolve_from_exe_dir(
            Path::new("/unused/bin"),
            crate::Overrides {
                lib_dir: Some(&lib),
                ..Default::default()
            },
        )
        .unwrap();
        let host = HostRuntime::start(
            &resources,
            StartupPolicy {
                skip_stdlib: true,
                precise_gc: true,
                source_bootstrap: false,
                tool_bootstrap: false,
            },
        )
        .unwrap();
        let fixtures = Hooks::new(&host.context);
        let cg = host
            .send(
                host.context.id.root,
                b"behaviorOf:",
                &[host.named(b"Codegen").unwrap()],
            )
            .unwrap();
        let gc = host.named(b"GC").unwrap();
        let mut hooks = Vec::new();
        for name in [b"evalPersisting:from:".as_slice(), b"lastError"] {
            let symbol = host.context.intern(name).unwrap();
            let original = fixtures.capture(cg, symbol);
            let context = fixtures.context(&[
                original,
                gc,
                birnam_abi::size(0).unwrap(),
                std::ptr::null_mut(),
            ]);
            host.send(
                cg,
                b"addMethod",
                &[symbol, fixtures.method(b"collecting", context)],
            )
            .unwrap();
            hooks.push((symbol, original, context));
        }
        for source in [
            b"nil".as_slice(),
            b"false",
            b"#[1 2 3]",
            b"let value := \"rooted\"; [System gc]; value",
        ] {
            host.scope()
                .evaluate(source, Path::new("<root-test>"), None)
                .unwrap();
        }
        let error = host
            .scope()
            .evaluate(
                b"[System readFile: \"/absent/birnam-host-fixture\"]",
                Path::new("<root-test>"),
                None,
            )
            .unwrap_err();
        assert!(error.0.contains("could not read file"), "{error}");
        let error = host
            .scope()
            .evaluate(
                b"[Error kind: #hostKindOnly]",
                Path::new("<root-test>"),
                None,
            )
            .unwrap_err();
        assert_eq!(error.0, "error(#hostKindOnly)");
        let error = host
            .scope()
            .evaluate(b"[", Path::new("<root-test>"), None)
            .unwrap_err();
        assert!(error.0.contains("syntax error"), "{error}");
        let failure = fixtures.fault(12345);
        fixtures.put(hooks[0].2, 3, failure);
        let error = host
            .scope()
            .evaluate(b"nil", Path::new("<root-test>"), None)
            .unwrap_err();
        assert!(error.0.contains(&format!("{failure:p}")), "{error}");
        for (symbol, original, context) in hooks {
            assert!(fixtures.count(context, 2) >= 6);
            host.send(cg, b"addMethod", &[symbol, original]).unwrap();
        }
        // Exact roots retain an intermediate and release it when the scope exits.
        let temporary;
        {
            let scope = host.scope();
            temporary = scope.string(b"host-only transient").unwrap();
            host.send(gc, b"collectPrecisely", &[]).unwrap();
            assert_eq!(scope.text(temporary).unwrap(), "host-only transient");
        }
        host.send(gc, b"collectPrecisely", &[]).unwrap();
        assert_ne!(
            host.send(
                host.context.id.root,
                b"allocationAt:",
                &[birnam_abi::size(temporary.addr()).unwrap()]
            )
            .unwrap(),
            temporary
        );
    }
    #[test]
    #[ignore = "requires installed modules and a saved CLI image; scripts/host-cli-test.sh"]
    fn image_entry_uses_saved_code_and_roots() {
        let lib = PathBuf::from(env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").unwrap());
        let path = PathBuf::from(env::var_os("BIRNAM_TEST_IMAGE").unwrap());
        let resources = crate::resolve_from_exe_dir(
            Path::new("/unused/bin"),
            crate::Overrides {
                lib_dir: Some(&lib),
                ..Default::default()
            },
        )
        .unwrap();
        let host = HostRuntime::start_native(&resources, true).unwrap();
        let fixtures = Hooks::new(&host.context);
        let symbol = |name: &[u8]| fixtures.symbol(name);
        let binding = |vt, name| fixtures.capture(vt, name);
        let bind = |vt, name, entry| {
            host.send(vt, b"addMethod", &[name, entry]).unwrap();
        };
        let behavior = |o| {
            object(
                host.send(host.context.id.root, b"behaviorOf:", &[o])
                    .unwrap(),
            )
            .unwrap()
        };
        let mut denied = Vec::new();
        for (receiver, selector) in [
            (b"TCC".as_slice(), b"compile:".as_slice()),
            (b"TCC", b"compileObject:"),
            (b"Codegen", b"evalPersisting:from:"),
        ] {
            let vt = behavior(host.named(receiver).unwrap());
            let name = symbol(selector);
            denied.push((vt, name, binding(vt, name)));
            bind(
                vt,
                name,
                fixtures.method(
                    b"deny",
                    fixtures.context(&[fixtures.fault(54321), birnam_abi::size(0).unwrap()]),
                ),
            );
        }
        let model = behavior(host.named(b"ObjectModel").unwrap());
        let restore = symbol(b"restoreImageSlotAt:in:put:");
        let original = binding(model, restore);
        let hook = fixtures.context(&[
            original,
            host.named(b"GC").unwrap(),
            birnam_abi::size(0).unwrap(),
            std::ptr::null_mut(),
        ]);
        bind(model, restore, fixtures.method(b"collecting", hook));
        host.load_image(&path).unwrap();
        assert!(fixtures.count(hook, 2) > 0);
        bind(model, restore, original);
        let scope = host.scope();
        let main = host.global(&scope, b"main").unwrap();
        let vt = behavior(main);
        let value = symbol(b"value");
        let failure = fixtures.fault(12345);
        let original = binding(vt, value);
        // Intercept only main. Globals mutation itself evaluates Blocks, which
        // must keep their normal value method while this hook removes main.
        let binding_scope = host.scope();
        let vt = binding_scope.call(vt, b"delegated", &[]).unwrap();
        host.send(host.context.id.root, b"reassociateBehaviorOf:with:", &[main, vt])
            .unwrap();
        let main_send = fixtures.context(&[
            original,
            host.named(b"GC").unwrap(),
            birnam_abi::size(0).unwrap(),
            failure,
            host.named(b"Globals").unwrap(),
            symbol(b"main"),
            host.named(b"nil").unwrap(),
        ]);
        bind(vt, value, fixtures.method(b"collecting", main_send));
        let error = host.run_image_main().unwrap_err();
        assert!(error.0.contains(&format!("{failure:p}")), "{error}");
        fixtures.put(main_send, 3, std::ptr::null_mut());
        bind(vt, value, fixtures.method(b"removeMain", main_send));
        drop(scope); // The next call must root main itself, before it loses Globals.
        host.run_image_main().unwrap();
        // The fixture callback context must stay live until its binding is gone.
        bind(vt, value, original);
        assert_eq!(fixtures.count(main_send, 2), 2);
        for (vt, name, entry) in denied {
            bind(vt, name, entry);
        }
        assert!(
            host.run_image_main()
                .unwrap_err()
                .0
                .contains("no global main")
        );
        host.send(host.named(b"GC").unwrap(), b"collectPrecisely", &[])
            .unwrap();
    }
}
