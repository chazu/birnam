use crate::{Resources,StartupPolicy,runtime::{HostError,HostRuntime}};
use birnam_abi::id_protocol::Operand;
#[cfg(test)]use crate::runtime::object;
#[cfg(test)]use birnam_abi::{Oop,id_protocol::{Outcome,Protocol}};
#[cfg(test)]use std::{env,path::Path,os::unix::ffi::OsStrExt};
impl HostRuntime {
 pub fn start(resources:&Resources,policy:StartupPolicy)->Result<Self,HostError>{let host=Self::start_native(resources,policy.precise_gc)?;if !policy.skip_stdlib {host.bootstrap(policy.source_bootstrap,policy.tool_bootstrap)?;}Ok(host)}
 pub(crate) fn bootstrap(&self,source:bool,tools:bool)->Result<(),HostError>{self.call(b"HostEntry",b"bootstrap:tools:",&[Operand::object(self.named(if source{b"true"}else{b"false"})?),Operand::object(self.named(if tools{b"true"}else{b"false"})?)])}
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
    use birnam_abi::id_protocol::{self as native, NativeEntry};
    use std::{cell::Cell, ffi::c_void, path::PathBuf};

    unsafe extern "C" fn deny_compilation(
        context: *mut c_void,
        _: Oop,
        _: *const Operand,
        _: usize,
    ) -> Outcome {
        let calls = unsafe { &*context.cast::<Cell<usize>>() };
        calls.set(calls.get() + 1);
        Outcome::failed(native::INVALID_REQUEST, 98765)
    }

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
        let fixture = env::temp_dir().join(format!("birnam-compiled-metadata-{}.o", std::process::id()));
        {
            let q = host.scope();
            let source = q.string(b"[Module named: #metadataFixture requires: #[]]; [Reflect defineMethod: #metadataFixtureAnswer on: Integer as: [:self | 42]]; true").unwrap();
            let fixture_path = q.string(fixture.as_os_str().as_bytes()).unwrap();
            let result = q.call(host.named(b"Codegen").unwrap(), b"compile:output:", &[Operand::object(source), Operand::object(fixture_path)]).unwrap();
            q.check_result(result).unwrap();
            assert_eq!(result, fixture_path);
            let key = unsafe { host.context.id.intern_name(b"compiledSourceMetadata") }.unwrap();
            let metadata = q.call(host.named(b"Store").unwrap(), b"currentValue:attr:", &[Operand::object(source), Operand::object(key)]).unwrap();
            assert_eq!(metadata, host.named(b"nil").unwrap(), "artifact compilation created a metadata fact");
        }
        let calls = Box::new(Cell::new(0usize));
        let mut original_method_tokens = Vec::new();
        let mut originals = Vec::new();
        for (family, name) in [
            (b"TCCVt".as_slice(), b"compile:".as_slice()),
            (b"TCCVt", b"compileObject:"),
            (b"LangVt", b"parse:"),
            (b"PegGrammarVt", b"parse:from:"),
            (b"NativeBackendVt", b"emit:source:filename:persistent:synchronous:"),
            (b"NativeBackendVt", b"factory:source:dispositions:"),
            (b"NativeBackendVt", b"named:synchronous:"),
        ] {
            let tcc = host.named(family).unwrap();
            let selector = unsafe { host.context.id.intern_name(name) }.unwrap();
            let original = host
                .send(
                    host.context.id.receiver,
                    b"bindingOf:in:",
                    &[Operand::object(selector), Operand::object(tcc)],
                )
                .unwrap()
                .as_entry()
                .unwrap();
            if family == b"LangVt" {
                let method = host.send(host.named(b"Execution").unwrap(),
                    b"methodOfEntry:", &[Operand::entry(original)]).unwrap();
                let token = host.send(host.context.roots, b"retain:", &[method]).unwrap().as_word().unwrap();
                original_method_tokens.push(token);
            }
            host.send(
                tcc,
                b"addMethod",
                &[
                    Operand::object(selector),
                    Operand::entry(NativeEntry::new(
                        deny_compilation,
                        (&*calls as *const Cell<usize>).cast_mut().cast(),
                    )),
                ],
            )
            .unwrap();
            originals.push((tcc, selector, original));
        }
        // Resources are now owned by native HostEntry; changing the facade's
        // private copy cannot simulate a missing bootstrap. Exercise the real
        // file boundary, restoring the fixture even if an assertion unwinds.
        struct MissingBootstrap { original: PathBuf, saved: PathBuf }
        impl Drop for MissingBootstrap {
            fn drop(&mut self) { let _=std::fs::rename(&self.saved,&self.original); }
        }
        let original=host.context.resources.lib_dir.join("birnam-bootstrap.image");
        let saved=original.with_extension(format!("disabled-{}",std::process::id()));
        std::fs::rename(&original,&saved).unwrap();
        let missing=MissingBootstrap{original,saved};
        let error = host.bootstrap(false, false).unwrap_err();
        assert!(error.0.contains("reinstall Birnam"), "{error}");
        assert_eq!(calls.get(),0,"missing bootstrap invoked a source fallback");
        drop(missing);
        host.bootstrap(false, false).unwrap();
        for name in [b"repl".as_slice(), b"socket", b"config", b"toml", b"file"] {
            host.load_tool(name).unwrap();
        }
        let q = host.scope();
        let cg = host.named(b"Codegen").unwrap();
        let path = q.string(fixture.as_os_str().as_bytes()).unwrap();
        let result = q.call(cg, b"load:", &[Operand::object(path)]).unwrap();
        q.check_result(result).unwrap();
        assert_eq!(result, host.named(b"true").unwrap());
        std::fs::remove_file(&fixture).unwrap();
        let name = unsafe { host.context.id.intern_name(b"stdlib") }.unwrap();
        let source = q.call(cg, b"moduleSourceFor:", &[Operand::object(name)]).unwrap();
        let generation=host.named(b"Generation").unwrap();
        let _record=q.call(cg,b"compiledSourceFor:",&[Operand::object(source)]).unwrap();
        let snapshot=q.call(generation,b"sourceRegistry",&[]).unwrap();
        let mut row_index=0;
        for i in 1..q.vector_size(snapshot).unwrap(){
            if q.vector_at(q.vector_at(snapshot,i).unwrap(),0).unwrap()==name {row_index=i;break}
        }
        assert_ne!(row_index,0);
        let owner_vt=host.named(b"GenerationVt").unwrap();
        let selector=unsafe{host.context.id.intern_name(b"sourceRegistry")}.unwrap();
        let original=host.send(host.context.id.receiver,b"bindingOf:in:",&[Operand::object(selector),Operand::object(owner_vt)]).unwrap().as_entry().unwrap();
        unsafe extern "C" fn snapshot_value(c:*mut c_void,_:Oop,_:*const Operand,_:usize)->Outcome {Outcome::loaded(Operand::object(c.cast()))}
        let malformed=q.call(host.named(b"Vector").unwrap(),b"new",&[]).unwrap();
        let replacing=|vector,index,value| {
            let mut out=q.call(host.named(b"Vector").unwrap(),b"new",&[]).unwrap();
            for i in 0..q.vector_size(vector).unwrap() {
                let item=if i==index {value}else{q.vector_at(vector,i).unwrap()};
                out=q.call(out,b"add:",&[Operand::object(item)]).unwrap();
            }
            out
        };
        for (bad,is_missing) in [(host.named(b"nil").unwrap(),true),(malformed,false)] {
            let bad_record=q.call(cg,b"compiledSource:metadata:",&[Operand::object(source),Operand::object(bad)]).unwrap();
            let row=q.vector_at(snapshot,row_index).unwrap();
            let bad_row=replacing(row,1,bad_record);
            let bad_snapshot=replacing(snapshot,row_index,bad_row);
            host.send(owner_vt,b"addMethod",&[Operand::object(selector),Operand::entry(NativeEntry::new(snapshot_value,bad_snapshot.cast()))]).unwrap();
            let rejected=q.call(cg,b"initialGenerationProjection",&[]);
            if is_missing {assert!(q.check_result(rejected.unwrap()).is_err(),"missing metadata accepted")}
            else {assert!(rejected.is_err(),"malformed metadata accepted")}
            assert_eq!(calls.get(),0,"metadata rejection invoked a parser fallback");
        }
        host.send(owner_vt,b"addMethod",&[Operand::object(selector),Operand::entry(original)]).unwrap();
        let docs = q.call(cg, b"recordedMethodSources", &[]).unwrap();
        assert!(q.vector_size(docs).unwrap() > 0);
        let repl = host.global(&q, b"Repl").unwrap();
        let result = q.call(repl, b"consoleHostInitNoConfig", &[]).unwrap();
        q.check_result(result).unwrap();
        let result = q
            .call(
                host.global(&q, b"DispatchGeneration").unwrap(),
                b"initializeCurrent",
                &[],
            )
            .unwrap();
        q.check_result(result).unwrap();
        let result = q.call(repl, b"consoleHostReadyNoMain", &[]).unwrap();
        q.check_result(result).unwrap();
        let result = q.call(repl, b"consoleDone", &[]).unwrap();
        q.check_result(result).unwrap();
        host.bootstrap(false, true).unwrap();
        for name in [b"lsp".as_slice(), b"cli", b"doc"] {
            host.load_tool(name).unwrap();
        }
        assert_eq!(calls.get(), 0, "bootstrap or console invoked the compiler or parser");
        for (tcc, selector, original) in originals {
            host.send(
                tcc,
                b"addMethod",
                &[Operand::object(selector), Operand::entry(original)],
            )
            .unwrap();
        }
        for token in original_method_tokens {
            host.send(host.context.roots, b"release:", &[Operand::word(token)]).unwrap();
        }
    }

    struct CollectingSend {
        id: Protocol,
        gc: Oop,
        original: NativeEntry,
        calls: Cell<usize>,
        fail: Cell<bool>,
    }
    unsafe extern "C" fn collecting_send(
        context: *mut c_void,
        receiver: Oop,
        args: *const Operand,
        count: usize,
    ) -> Outcome {
        let state = unsafe { &*context.cast::<CollectingSend>() };
        let call = state.calls.get() + 1;
        // Small operations remain exhaustive. Sample large resident-image
        // restoration geometrically instead of collecting once per core slot.
        if call <= 8 || call.is_power_of_two() {
            if let Err(error) = unsafe { state.id.send_named(state.gc, b"collectPrecisely", &[]) } {
                return error;
            }
        }
        state.calls.set(call);
        if state.fail.get() {
            return Outcome::failed(native::UNBOUND, 12345);
        }
        unsafe { state.original.method.unwrap()(state.original.context, receiver, args, count) }
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
        let cg = host.named(b"CodegenVt").unwrap();
        let gc = host.named(b"GC").unwrap();
        let mut hooks = Vec::new();
        for name in [b"evalPersisting:from:".as_slice(), b"lastError"] {
            let symbol = unsafe { host.context.id.intern_name(name) }.unwrap();
            let original = host
                .send(
                    host.context.id.receiver,
                    b"bindingOf:in:",
                    &[Operand::object(symbol), Operand::object(cg)],
                )
                .unwrap()
                .as_entry()
                .unwrap();
            let mut hook = Box::new(CollectingSend {
                id: host.context.id,
                gc,
                original,
                calls: Cell::new(0),
                fail: Cell::new(false),
            });
            host.send(
                cg,
                b"addMethod",
                &[
                    Operand::object(symbol),
                    Operand::entry(NativeEntry::new(
                        collecting_send,
                        (&mut *hook as *mut CollectingSend).cast(),
                    )),
                ],
            )
            .unwrap();
            hooks.push((symbol, hook));
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
            .evaluate(b"[Error kind: #hostKindOnly]", Path::new("<root-test>"), None)
            .unwrap_err();
        assert_eq!(error.0, "error(#hostKindOnly)");
        let error = host
            .scope()
            .evaluate(b"[", Path::new("<root-test>"), None)
            .unwrap_err();
        assert!(error.0.contains("syntax error"), "{error}");
        hooks[0].1.fail.set(true);
        let error = host
            .scope()
            .evaluate(b"nil", Path::new("<root-test>"), None)
            .unwrap_err();
        assert!(error.0.contains("native status=6 detail=12345"), "{error}");
        for (symbol, hook) in hooks {
            assert!(hook.calls.get() >= 6);
            host.send(
                cg,
                b"addMethod",
                &[Operand::object(symbol), Operand::entry(hook.original)],
            )
            .unwrap();
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
                host.context.id.receiver,
                b"allocationAt:",
                &[Operand::word(temporary.addr())]
            )
            .unwrap()
            .as_object(),
            Some(temporary)
        );
    }
    unsafe extern "C" fn deny_compile(
        _: *mut c_void,
        _: Oop,
        _: *const Operand,
        _: usize,
    ) -> Outcome {
        Outcome::failed(native::UNBOUND, 54321)
    }
    struct MainSend {
        hook: CollectingSend,
        globals: Oop,
        name: Oop,
        nil: Oop,
    }
    unsafe extern "C" fn remove_main_and_collect(
        context: *mut c_void,
        receiver: Oop,
        args: *const Operand,
        count: usize,
    ) -> Outcome {
        let state = unsafe { &*context.cast::<MainSend>() };
        if let Err(error) = unsafe {
            state.hook.id.send_named(
                state.globals,
                b"at:put:",
                &[Operand::object(state.name), Operand::object(state.nil)],
            )
        } {
            return error;
        }
        unsafe {
            collecting_send(
                (&state.hook as *const CollectingSend).cast_mut().cast(),
                receiver,
                args,
                count,
            )
        }
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
        let symbol = |name: &[u8]| unsafe { host.context.id.intern_name(name) }.unwrap();
        let binding = |vt, name| {
            host.send(
                host.context.id.receiver,
                b"bindingOf:in:",
                &[Operand::object(name), Operand::object(vt)],
            )
            .unwrap()
            .as_entry()
            .unwrap()
        };
        let bind = |vt, name, entry| {
            host.send(
                vt,
                b"addMethod",
                &[Operand::object(name), Operand::entry(entry)],
            )
            .unwrap();
        };
        let behavior = |o| {
            object(
                host.send(host.context.id.receiver, b"behaviorOf:", &[Operand::object(o)])
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
                NativeEntry::new(deny_compile, core::ptr::null_mut()),
            );
        }
        let model = behavior(host.named(b"ObjectModel").unwrap());
        let restore = symbol(b"restoreImageSlotAt:in:put:");
        let mut hook = Box::new(CollectingSend {
            id: host.context.id,
            gc: host.named(b"GC").unwrap(),
            original: binding(model, restore),
            calls: Cell::new(0),
            fail: Cell::new(false),
        });
        bind(
            model,
            restore,
            NativeEntry::new(collecting_send, (&mut *hook as *mut CollectingSend).cast()),
        );
        host.load_image(&path).unwrap();
        assert!(hook.calls.get() > 0);
        bind(model, restore, hook.original);
        let scope = host.scope();
        let main = host.global(&scope, b"main").unwrap();
        let vt = behavior(main);
        let value = symbol(b"value");
        let mut main_send = Box::new(MainSend {
            hook: CollectingSend {
                id: host.context.id,
                gc: host.named(b"GC").unwrap(),
                original: binding(vt, value),
                calls: Cell::new(0),
                fail: Cell::new(true),
            },
            globals: host.named(b"Globals").unwrap(),
            name: symbol(b"main"),
            nil: host.named(b"nil").unwrap(),
        });
        // A native execution failure keeps its status and detail through Host.
        bind(
            vt,
            value,
            NativeEntry::new(
                collecting_send,
                (&mut main_send.hook as *mut CollectingSend).cast(),
            ),
        );
        let error = host.run_image_main().unwrap_err();
        assert!(error.0.contains("native status=6 detail=12345"), "{error}");
        main_send.hook.fail.set(false);
        bind(
            vt,
            value,
            NativeEntry::new(
                remove_main_and_collect,
                (&mut *main_send as *mut MainSend).cast(),
            ),
        );
        drop(scope); // The next call must root main itself, before it loses Globals.
        host.run_image_main().unwrap();
        // The fixture callback context must stay live until its binding is gone.
        bind(vt, value, main_send.hook.original);
        assert_eq!(main_send.hook.calls.get(), 2);
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
