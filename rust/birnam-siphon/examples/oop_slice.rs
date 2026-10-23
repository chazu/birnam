//! Cold process acceptance driver. Upper Birnam libraries/images are absent.
use birnam_abi::{self as abi, Oop, id_protocol::Protocol};
use std::{os::unix::ffi::OsStrExt, path::PathBuf};
fn main() {
    unsafe { run() }
}
unsafe fn run() {
    unsafe {
        let root = PathBuf::from(std::env::args().nth(1).expect("worktree root"));
        let target = std::env::var_os("BIRNAM_OOP_TARGET")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join(".cargo-target/oop-slice"));
        let library = target.join("release");
        let fixtures = target.join("fixtures");
        let paths = [
            library.join("libbirnam_root_registry.dylib"),
            library.join("libbirnam_collect.dylib"),
            fixtures.join("fixture.dylib"),
            fixtures.join("failure.dylib"),
            fixtures.join("rejected.dylib"),
            fixtures.join("rejected-null.dylib"),
            library.join("libbirnam_namespace.dylib"),
            library.join("libbirnam_storage.dylib"),
            library.join("libbirnam_boolean.dylib"),
            library.join("libbirnam_nil.dylib"),
        ];
        let words: &[&[u8]] = &[
            b"sequence",
            b"objectBehavior",
            b"allocate",
            b"at:",
            b"at:put:",
            b"collectPrecisely",
            b"dynamicName:",
            b"exercise:",
            b"finalized",
            b"identityOf:",
            b"failTracing:",
            b"retain:",
            b"release:",
            b"faultKind",
            b"detail",
            b"invokeOn:arguments:",
            b"behaviorOf:",
            b"lookup",
            b"withContext:",
            b"addMethod",
            b"contextRun",
            b"nameBehavior",
            b"nameWithBytes",
            b"replacement",
            b"removeMethod:",
            b"yourself",
            b"allocationCount",
            b"allocationSizeOf:",
            b"newFinalizable",
            b"lookupOverride",
            b"doesNotUnderstand:",
            b"missingProbe",
            b"delegated",
            b"reassociateBehaviorOf:with:",
            b"badCoreReferences",
            b"badAllocations",
            b"allocationsDo:",
            b"referencesOf:do:",
            b"collectArguments",
            b"snapshotArgument:",
            b"apply",
            b"applyValue",
            b"apply:to:",
            b"argumentProxy",
            b"nameFactory",
            b"methodFactory",
            b"moduleBehavior",
            b"newMethodAt:",
            b"count",
            b"nameAt:",
            b"nameOf:",
            b"valueAt:",
            b"nil",
            b"bindingsDo:",
            b"liveBindingsDo:",
            b"isCanonicalLookup:",
            b"prepareOwner:",
            b"nodeWithChild:",
            b"trace:with:",
            b"referenceOwnerOf:",
            b"Roots",
            b"GC",
            b"observe:",
            b"visit:",
            b"registerRetirement:",
            b"true", b"false", b"not", b"==", b"&", b"|", b"ifTrue:", b"ifFalse:",
            b"ifTrue:ifFalse:", b"and:", b"or:", b"value", b"valueProbe", b"echoName",
            b"isNil", b"notNil", b"installPrinting:", b"printString", b"fromBytes:", b"size",
        ];
        let mut seeds: Vec<&[u8]> = paths.iter().map(|p| p.as_os_str().as_bytes()).collect();
        seeds.extend(words.iter().copied());
        let id = birnam_siphon::load(
            library.join("libbirnam_id.dylib").as_os_str().as_bytes(),
            &seeds,
        )
        .expect("cold Id bootstrap");
        let repeated = birnam_siphon::load(
            library.join("libbirnam_id.dylib").as_os_str().as_bytes(),
            &seeds,
        )
        .unwrap();
        assert_eq!(repeated.root, id.root);
        assert_eq!(repeated.seeds, id.seeds);
        let names: Vec<_> = (0..seeds.len()).map(|i| id.literal(i).unwrap()).collect();
        let selectors: Vec<_> = names[paths.len()..]
            .iter()
            .map(|&o| id.send(o, id.intern, &[]).unwrap())
            .collect();
        let sel = |name: &[u8]| selectors[words.iter().position(|&w| w == name).unwrap()];
        let send = |r: Oop, n: &[u8], a: &[Oop]| id.send(r, sel(n), a);
        let empty = send(id.root, b"sequence", &[]).unwrap();
        let roots = id.send(names[0], id.load, &[empty]).expect("Roots loaded");
        for &selector in &selectors {
            send(roots, b"retain:", &[selector]).unwrap();
        }
        let load_args = send(id.root, b"sequence", &[roots]).unwrap();
        let gc = id.send(names[1], id.load, &[load_args]).expect("GC loaded");
        let load_args = send(id.root, b"sequence", &[roots, gc]).unwrap();
        let actor = id
            .send(names[2], id.load, &[load_args])
            .expect("independent native module loaded");
        println!("cold bootstrap and three native modules: passed");
        let before = send(id.root, b"allocationCount", &[]).unwrap();
        let object_type = send(id.root, b"objectBehavior", &[]).unwrap();
        assert!(
            send(
                object_type,
                b"allocate",
                &[abi::size(usize::MAX >> 1).unwrap()]
            )
            .is_err()
        );
        for bad in [abi::size(0).unwrap(), abi::size(256).unwrap(), id.root] {
            assert!(send(id.root, b"nameWithBytes", &[bad]).is_err());
        }
        assert_eq!(send(id.root, b"allocationCount", &[]).unwrap(), before);
        let empty_name = send(id.root, b"nameWithBytes", &[]).unwrap();
        assert_eq!(
            id.send(empty_name, id.intern, &[]).unwrap(),
            id.send(empty_name, id.intern, &[]).unwrap()
        );
        println!("size limits and invalid byte values rejected without allocating: passed");
        if std::env::args().nth(2).as_deref() == Some("finalizer-failure") {
            send(
                actor,
                b"at:put:",
                &[abi::size(9).unwrap(), abi::size(1).unwrap()],
            )
            .unwrap();
            let node = send(actor, b"newFinalizable", &[]).unwrap();
            assert!(send(gc, b"collectPrecisely", &[]).is_err());
            assert!(send(gc, b"collectPrecisely", &[]).is_err());
            assert_eq!(
                abi::as_size(send(id.root, b"allocationSizeOf:", &[node]).unwrap()),
                Some(16)
            );
            println!("FINALIZER_FAILURE_CONTAINED");
            return;
        }
        let name = send(actor, b"dynamicName:", &[abi::size(64).unwrap()]).unwrap();
        let first = id.send(name, id.intern, &[]).unwrap();
        let token = send(roots, b"retain:", &[first]).unwrap();
        let vt = send(actor, b"at:", &[abi::size(2).unwrap()]).unwrap();
        let receiver = send(vt, b"allocate", &[abi::size(16).unwrap()]).unwrap();
        let answer =
            send(receiver, b"exercise:", &[name]).expect("collection from entered native method");
        assert_eq!(answer, first);
        assert_eq!(
            abi::as_size(send(actor, b"finalized", &[]).unwrap()),
            Some(128)
        );
        send(roots, b"release:", &[token]).unwrap();
        println!("entered receiver/argument survival, 128 finalizers and nested GC: passed");
        let arguments: Vec<_> = (0..64)
            .map(|i| send(actor, b"dynamicName:", &[abi::size(16 + i).unwrap()]).unwrap())
            .collect();
        assert_eq!(
            abi::as_size(send(actor, b"collectArguments", &arguments).unwrap()),
            Some(64)
        );
        println!("all 64 borrowed object arguments survived collection: passed");
        // Only the selected clone links this context to live roots after its binding
        // is removed inside contextRun. Host locals are deliberately not GC roots.
        let context_name = send(actor, b"dynamicName:", &[abi::size(32).unwrap()]).unwrap();
        let context = send(id.root, b"sequence", &[gc, context_name]).unwrap();
        let actor_vt = send(id.root, b"behaviorOf:", &[actor]).unwrap();
        let template = send(actor_vt, b"lookup", &[sel(b"contextRun")]).unwrap();
        let method = send(template, b"withContext:", &[context]).unwrap();
        send(vt, b"addMethod", &[sel(b"contextRun"), method]).unwrap();
        let receiver = send(vt, b"allocate", &[abi::size(16).unwrap()]).unwrap();
        let result = send(receiver, b"contextRun", &[]).unwrap();
        assert!(!result.is_null());
        send(id.root, b"identityOf:", &[method]).expect("entered Method retained");
        assert_eq!(
            send(context, b"at:", &[abi::size(1).unwrap()]).unwrap(),
            context_name
        );
        println!("removed entered Method and otherwise-unrooted context survived GC: passed");
        let argument = send(actor, b"dynamicName:", &[abi::size(27).unwrap()]).unwrap();
        let source = send(id.root, b"sequence", &[argument]).unwrap();
        send(actor, b"at:put:", &[abi::size(12).unwrap(), source]).unwrap();
        let snapshot_method = send(actor_vt, b"lookup", &[sel(b"snapshotArgument:")]).unwrap();
        assert_eq!(
            send(snapshot_method, b"invokeOn:arguments:", &[actor, source]).unwrap(),
            argument
        );
        assert_ne!(
            send(source, b"at:", &[abi::size(0).unwrap()]).unwrap(),
            argument
        );
        println!(
            "changing the source fields preserves the entered call's argument snapshot: passed"
        );
        // Live replacement of interning, then restoration via a captured Method Oop.
        let name = send(actor, b"dynamicName:", &[abi::size(64).unwrap()]).unwrap();
        let name_token = send(roots, b"retain:", &[name]).unwrap();
        let name_vt = send(id.root, b"nameBehavior", &[]).unwrap();
        let original = send(name_vt, b"lookup", &[id.intern]).unwrap();
        let original_token = send(roots, b"retain:", &[original]).unwrap();
        let replacement = send(actor_vt, b"lookup", &[sel(b"replacement")]).unwrap();
        send(name_vt, b"addMethod", &[id.intern, replacement]).unwrap();
        send(gc, b"collectPrecisely", &[]).unwrap();
        assert_eq!(
            id.send(name, id.intern, &[]).unwrap(),
            send(actor, b"at:", &[abi::size(6).unwrap()]).unwrap()
        );
        send(name_vt, b"addMethod", &[id.intern, original]).unwrap();
        assert!(!id.send(name, id.intern, &[]).unwrap().is_null());
        send(roots, b"release:", &[original_token]).unwrap();
        send(roots, b"release:", &[name_token]).unwrap();
        println!("intern method replacement across collection and captured restoration: passed");
        // A tracing failure must stop before retirement and permit a later retry.
        let node = send(vt, b"allocate", &[abi::size(16).unwrap()]).unwrap();
        let token = send(roots, b"retain:", &[node]).unwrap();
        send(actor, b"failTracing:", &[abi::size(1).unwrap()]).unwrap();
        let before = send(id.root, b"allocationCount", &[]).unwrap();
        assert!(send(gc, b"collectPrecisely", &[]).is_err());
        assert_eq!(send(id.root, b"allocationCount", &[]).unwrap(), before);
        send(actor, b"failTracing:", &[abi::size(0).unwrap()]).unwrap();
        send(gc, b"collectPrecisely", &[]).unwrap();
        send(roots, b"release:", &[token]).unwrap();
        // Allocating from a tracer must also invalidate the census before sweeping.
        let token = send(roots, b"retain:", &[node]).unwrap();
        send(actor, b"failTracing:", &[abi::size(2).unwrap()]).unwrap();
        assert!(send(gc, b"collectPrecisely", &[]).is_err());
        send(id.root, b"identityOf:", &[node]).unwrap();
        send(actor, b"failTracing:", &[abi::size(0).unwrap()]).unwrap();
        send(gc, b"collectPrecisely", &[]).unwrap();
        send(roots, b"release:", &[token]).unwrap();
        // Even an owner that swallows an invalid visitor result must not permit sweeping.
        let token = send(roots, b"retain:", &[node]).unwrap();
        send(actor, b"failTracing:", &[abi::size(3).unwrap()]).unwrap();
        let count = send(id.root, b"allocationCount", &[]).unwrap();
        assert!(send(gc, b"collectPrecisely", &[]).is_err());
        assert_eq!(send(id.root, b"allocationCount", &[]).unwrap(), count);
        send(actor, b"failTracing:", &[abi::size(0).unwrap()]).unwrap();
        send(roots, b"release:", &[token]).unwrap();
        println!(
            "trace failure/allocation and swallowed visitor failure refused before retirement: passed"
        );
        // Repeat the swallowed-error case for Id's core-edge and census callbacks.
        for (selector, wrapper, argc) in [
            (
                b"referencesOf:do:".as_slice(),
                b"badCoreReferences".as_slice(),
                2,
            ),
            (
                b"allocationsDo:".as_slice(),
                b"badAllocations".as_slice(),
                1,
            ),
        ] {
            let original_behavior = send(id.root, b"behaviorOf:", &[id.root]).unwrap();
            let original = send(original_behavior, b"lookup", &[sel(selector)]).unwrap();
            let private = send(original_behavior, b"delegated", &[]).unwrap();
            let scratch = send(id.root, b"sequence", &vec![core::ptr::null_mut(); argc]).unwrap();
            send(actor, b"at:put:", &[abi::size(11).unwrap(), original]).unwrap();
            send(actor, b"at:put:", &[abi::size(12).unwrap(), scratch]).unwrap();
            let wrapper = send(actor_vt, b"lookup", &[sel(wrapper)]).unwrap();
            send(private, b"addMethod", &[sel(selector), wrapper]).unwrap();
            send(id.root, b"reassociateBehaviorOf:with:", &[id.root, private]).unwrap();
            let count = send(id.root, b"allocationCount", &[]).unwrap();
            assert!(send(gc, b"collectPrecisely", &[]).is_err());
            assert_eq!(send(id.root, b"allocationCount", &[]).unwrap(), count);
            send(
                id.root,
                b"reassociateBehaviorOf:with:",
                &[id.root, original_behavior],
            )
            .unwrap();
            send(gc, b"collectPrecisely", &[]).unwrap();
        }
        println!("swallowed core-edge and allocation-census failures also refused safely: passed");
        // Loading observes replaced constructors; each replacement collects before
        // calling its captured original, exercising partially built module roots.
        let module_type = send(id.root, b"moduleBehavior", &[]).unwrap();
        let original_name_factory = send(object_type, b"lookup", &[sel(b"nameWithBytes")]).unwrap();
        let original_method_factory = send(module_type, b"lookup", &[sel(b"newMethodAt:")]).unwrap();
        send(actor, b"at:put:", &[abi::size(13).unwrap(), original_name_factory]).unwrap();
        send(actor, b"at:put:", &[abi::size(14).unwrap(), original_method_factory]).unwrap();
        let name_factory = send(actor_vt, b"lookup", &[sel(b"nameFactory")]).unwrap();
        let method_factory = send(actor_vt, b"lookup", &[sel(b"methodFactory")]).unwrap();
        send(object_type, b"addMethod", &[sel(b"nameWithBytes"), name_factory]).unwrap();
        send(module_type, b"addMethod", &[sel(b"newMethodAt:"), method_factory]).unwrap();
        // Failure is an object and can be inspected; ordinary object data remains data.
        let published = send(id.root, b"sequence", &[core::ptr::null_mut()]).unwrap();
        let token = send(roots, b"retain:", &[published]).unwrap();
        let args = send(id.root, b"sequence", &[published]).unwrap();
        let failure = id
            .send(names[3], id.load, &[args])
            .expect_err("intentional initializer failure");
        send(object_type, b"addMethod", &[sel(b"nameWithBytes"), original_name_factory]).unwrap();
        send(module_type, b"addMethod", &[sel(b"newMethodAt:"), original_method_factory]).unwrap();
        assert_eq!(abi::as_size(send(actor, b"at:", &[abi::size(15).unwrap()]).unwrap()), Some(5));
        assert_eq!(abi::as_size(send(actor, b"at:", &[abi::size(16).unwrap()]).unwrap()), Some(1));
        println!("module construction follows current factories and survives GC between declarations: passed");
        assert!(abi::is_fault(failure));
        assert!(!send(failure, b"faultKind", &[]).unwrap().is_null());
        assert_eq!(send(failure, b"detail", &[]).unwrap(), published);
        send(gc, b"collectPrecisely", &[]).unwrap();
        let method = send(published, b"at:", &[abi::size(0).unwrap()]).unwrap();
        let empty = send(id.root, b"sequence", &[]).unwrap();
        assert_eq!(
            abi::as_size(
                send(
                    method,
                    b"invokeOn:arguments:",
                    &[core::ptr::null_mut(), empty]
                )
                .unwrap()
            ),
            Some(777)
        );
        assert_eq!(send(published, b"yourself", &[]).unwrap(), published);
        send(roots, b"release:", &[token]).unwrap();
        let empty = send(id.root, b"sequence", &[]).unwrap();
        assert!(id.send(names[4], id.load, &[empty]).is_err());
        assert!(id.send(names[5], id.load, &[empty]).is_err());
        println!(
            "partial publication/code residency and incompatible/null initializer rejection: passed"
        );
        // Arbitrary valid Oops must not be interpreted as native context pointers.
        let empty = send(id.root, b"sequence", &[]).unwrap();
        for (behavior, selector) in [
            (name_vt, id.intern),
            (
                send(id.root, b"behaviorOf:", &[roots]).unwrap(),
                sel(b"retain:"),
            ),
            (
                send(id.root, b"behaviorOf:", &[gc]).unwrap(),
                sel(b"collectPrecisely"),
            ),
        ] {
            let original = send(behavior, b"lookup", &[selector]).unwrap();
            let wrong = send(original, b"withContext:", &[actor]).unwrap();
            assert!(send(wrong, b"invokeOn:arguments:", &[actor, empty]).is_err());
        }
        println!("invalid core/Roots/GC method contexts return Fault objects: passed");
        // Custom metabehavior must be consulted; the original captured lookup still
        // executes when the override delegates. DNU forwards every argument.
        let meta = send(id.root, b"behaviorOf:", &[vt]).unwrap();
        let original_lookup = send(meta, b"lookup", &[sel(b"lookup")]).unwrap();
        send(actor, b"at:put:", &[abi::size(7).unwrap(), original_lookup]).unwrap();
        let replace = send(actor_vt, b"lookup", &[sel(b"replacement")]).unwrap();
        send(actor, b"at:put:", &[abi::size(10).unwrap(), replace]).unwrap();
        let custom_meta = send(meta, b"delegated", &[]).unwrap();
        let plain_type = send(id.root, b"objectBehavior", &[]).unwrap();
        let custom_type = send(plain_type, b"delegated", &[]).unwrap();
        send(
            id.root,
            b"reassociateBehaviorOf:with:",
            &[custom_type, custom_meta],
        )
        .unwrap();
        let lookup_method = send(actor_vt, b"lookup", &[sel(b"lookupOverride")]).unwrap();
        send(custom_meta, b"addMethod", &[sel(b"lookup"), lookup_method]).unwrap();
        let custom = send(custom_type, b"allocate", &[abi::size(0).unwrap()]).unwrap();
        assert_eq!(
            send(custom, b"missingProbe", &[]).unwrap(),
            send(actor, b"at:", &[abi::size(6).unwrap()]).unwrap()
        );
        assert!(abi::as_size(send(actor, b"at:", &[abi::size(8).unwrap()]).unwrap()).unwrap() > 0);
        let base = send(id.root, b"objectBehavior", &[]).unwrap();
        let dnu_type = send(base, b"delegated", &[]).unwrap();
        let dnu = send(actor_vt, b"lookup", &[sel(b"doesNotUnderstand:")]).unwrap();
        send(dnu_type, b"addMethod", &[sel(b"doesNotUnderstand:"), dnu]).unwrap();
        let object = send(dnu_type, b"allocate", &[abi::size(0).unwrap()]).unwrap();
        assert_eq!(
            abi::as_size(send(object, b"missingProbe", &vec![abi::size(3).unwrap(); 19]).unwrap()),
            Some(20)
        );
        println!("custom metacircular lookup and full-arity DNU: passed");
        // A different callable layout is ordinary object behavior, not a new Id type.
        let value = send(actor, b"dynamicName:", &[abi::size(21).unwrap()]).unwrap();
        let callable = send(id.root, b"sequence", &[value]).unwrap();
        let token = send(roots, b"retain:", &[callable]).unwrap();
        let original_type = send(id.root, b"behaviorOf:", &[callable]).unwrap();
        let callable_type = send(original_type, b"delegated", &[]).unwrap();
        let applier = send(actor_vt, b"lookup", &[sel(b"applyValue")]).unwrap();
        send(callable_type, b"addMethod", &[sel(b"apply"), applier]).unwrap();
        send(
            id.root,
            b"reassociateBehaviorOf:with:",
            &[callable, callable_type],
        )
        .unwrap();
        let receiver_type = send(base, b"delegated", &[]).unwrap();
        send(
            receiver_type,
            b"addMethod",
            &[sel(b"missingProbe"), callable],
        )
        .unwrap();
        let receiver = send(receiver_type, b"allocate", &[abi::size(0).unwrap()]).unwrap();
        assert_eq!(send(receiver, b"missingProbe", &[]).unwrap(), value);
        send(roots, b"release:", &[token]).unwrap();
        println!(
            "non-native callable layout applies through live Id lookup and survives GC: passed"
        );

        let proxy_type = send(base, b"delegated", &[]).unwrap();
        let proxy_method = send(actor_vt, b"lookup", &[sel(b"argumentProxy")]).unwrap();
        send(proxy_type, b"addMethod", &[sel(b"apply:to:"), proxy_method]).unwrap();
        let proxy = send(proxy_type, b"allocate", &[abi::size(0).unwrap()]).unwrap();
        let proxy_token = send(roots, b"retain:", &[proxy]).unwrap();
        let collect_method = send(actor_vt, b"lookup", &[sel(b"collectArguments")]).unwrap();
        assert_eq!(abi::as_size(send(collect_method, b"invokeOn:arguments:", &[actor, proxy]).unwrap()), Some(1));
        let count_method = send(actor_vt, b"lookup", &[sel(b"doesNotUnderstand:")]).unwrap();
        send(proxy_type, b"addMethod", &[sel(b"apply:to:"), count_method]).unwrap();
        assert_eq!(abi::as_size(send(collect_method, b"invokeOn:arguments:", &[actor, proxy]).unwrap()), Some(2));
        send(roots, b"release:", &[proxy_token]).unwrap();
        println!("argument containers use current application behavior and survive GC: passed");

        // A tracer can replace a later owner. The new owner is deliberately
        // unrooted until registration inside the tracer adds it to the mark walk.
        let child = send(actor, b"dynamicName:", &[abi::size(43).unwrap()]).unwrap();
        let node = send(actor, b"nodeWithChild:", &[child]).unwrap();
        let node_token = send(roots, b"retain:", &[node]).unwrap();
        let new_type = send(base, b"delegated", &[]).unwrap();
        let trace_method = send(actor_vt, b"lookup", &[sel(b"referencesOf:do:")]).unwrap();
        send(new_type, b"addMethod", &[sel(b"referencesOf:do:"), trace_method]).unwrap();
        let new_owner = send(new_type, b"allocate", &[abi::size(0).unwrap()]).unwrap();
        send(actor, b"prepareOwner:", &[new_owner]).unwrap();
        send(gc, b"collectPrecisely", &[]).unwrap();
        assert_eq!(send(gc, b"referenceOwnerOf:", &[vt]).unwrap(), new_owner);
        send(id.root, b"identityOf:", &[new_owner]).unwrap();
        send(id.root, b"identityOf:", &[child]).unwrap();
        let derived = send(vt, b"delegated", &[]).unwrap();
        send(id.root, b"reassociateBehaviorOf:with:", &[node, derived]).unwrap();
        send(gc, b"collectPrecisely", &[]).unwrap();
        send(id.root, b"identityOf:", &[child]).unwrap();
        send(gc, b"trace:with:", &[vt, core::ptr::null_mut()]).unwrap();
        assert!(send(gc, b"referenceOwnerOf:", &[derived]).unwrap().is_null());
        let count = send(id.root, b"allocationCount", &[]).unwrap();
        assert!(send(gc, b"collectPrecisely", &[]).is_err());
        assert_eq!(send(id.root, b"allocationCount", &[]).unwrap(), count);
        send(gc, b"trace:with:", &[vt, actor]).unwrap();
        send(gc, b"collectPrecisely", &[]).unwrap();
        send(id.root, b"identityOf:", &[child]).unwrap();
        send(roots, b"release:", &[node_token]).unwrap();
        println!("GC owner replacement during tracing, inherited behavior and owner removal: passed");

        let ns_args = send(id.root, b"sequence", &[roots]).unwrap();
        let namespace = id.send(names[6], id.load, &[ns_args]).unwrap();
        send(actor, b"at:put:", &[abi::size(17).unwrap(), namespace]).unwrap();
        let ns_type = send(id.root, b"behaviorOf:", &[namespace]).unwrap();
        let lookup = send(ns_type, b"lookup", &[sel(b"at:")]).unwrap();
        assert_eq!(abi::as_size(send(namespace, b"isCanonicalLookup:", &[lookup]).unwrap()), Some(1));
        let value = send(actor, b"dynamicName:", &[abi::size(37).unwrap()]).unwrap();
        send(namespace, b"at:put:", &[sel(b"missingProbe"), value]).unwrap();
        send(namespace, b"at:put:", &[sel(b"nil"), value]).unwrap();
        assert_eq!(send(namespace, b"nameOf:", &[value]).unwrap(), sel(b"missingProbe"));
        assert_eq!(send(namespace, b"valueAt:", &[sel(b"contextRun")]).unwrap(), value);
        assert!(send(namespace, b"at:put:", &[value, value]).is_err());
        let replacement = send(actor_vt, b"lookup", &[sel(b"replacement")]).unwrap();
        send(ns_type, b"addMethod", &[sel(b"at:"), replacement]).unwrap();
        assert_eq!(send(namespace, b"valueAt:", &[sel(b"missingProbe")]).unwrap(), send(actor, b"at:", &[abi::size(6).unwrap()]).unwrap());
        send(ns_type, b"addMethod", &[sel(b"at:"), lookup]).unwrap();
        let count = send(namespace, b"count", &[]).unwrap();
        send(namespace, b"bindingsDo:", &[actor]).unwrap();
        assert_eq!(send(actor, b"at:", &[abi::size(18).unwrap()]).unwrap(), count);
        send(namespace, b"liveBindingsDo:", &[actor]).unwrap();
        assert_eq!(send(actor, b"at:", &[abi::size(18).unwrap()]).unwrap(), count);
        assert!(send(namespace, b"at:", &[sel(b"missingProbe")]).unwrap().is_null());
        println!("Namespace Oop bindings, live lookup and enumeration mutation with GC: passed");

        send(namespace, b"at:put:", &[sel(b"Roots"), roots]).unwrap();
        send(namespace, b"at:put:", &[sel(b"GC"), gc]).unwrap();
        let storage_args = send(id.root, b"sequence", &[namespace]).unwrap();
        if std::env::args().nth(2).as_deref() == Some("storage-init-failure") {
            let original = send(ns_type, b"lookup", &[sel(b"at:put:")]).unwrap();
            let token = send(roots, b"retain:", &[original]).unwrap();
            // This one-argument method rejects the two-argument publication.
            let reject = send(actor_vt, b"lookup", &[sel(b"failTracing:")]).unwrap();
            send(ns_type, b"addMethod", &[sel(b"at:put:"), reject]).unwrap();
            assert!(id.send(names[7], id.load, &[storage_args]).is_err());
            send(ns_type, b"addMethod", &[sel(b"at:put:"), original]).unwrap();
            assert!(id.send(names[7], id.load, &[storage_args]).is_err());
            send(roots, b"release:", &[token]).unwrap();
            send(gc, b"collectPrecisely", &[]).unwrap();
            println!("STORAGE_PUBLICATION_FAILURE_CONTAINED");
            return;
        }
        let storage = id.send(names[7], id.load, &[storage_args]).unwrap();
        send(actor, b"at:put:", &[abi::size(19).unwrap(), storage]).unwrap();
        let child = send(actor, b"dynamicName:", &[abi::size(47).unwrap()]).unwrap();
        let node = send(actor, b"nodeWithChild:", &[child]).unwrap();
        let node_token = send(roots, b"retain:", &[node]).unwrap();
        send(actor, b"failTracing:", &[abi::size(6).unwrap()]).unwrap();
        assert_eq!(abi::as_size(send(storage, b"observe:", &[actor]).unwrap()), Some(1));
        let expired = send(actor, b"at:", &[abi::size(20).unwrap()]).unwrap();
        assert!(!expired.is_null());
        assert!(send(expired, b"visit:", &[id.root]).is_err());
        send(actor, b"failTracing:", &[abi::size(0).unwrap()]).unwrap();
        for index in 21..=23 {
            assert!(abi::as_size(send(actor, b"at:", &[abi::size(index).unwrap()]).unwrap()).unwrap()>0);
            send(actor, b"at:put:", &[abi::size(index).unwrap(), abi::size(0).unwrap()]).unwrap();
        }
        assert_eq!(abi::as_size(send(storage, b"observe:", &[actor]).unwrap()), Some(1));
        send(actor, b"failTracing:", &[abi::size(3).unwrap()]).unwrap();
        assert!(send(storage, b"observe:", &[actor]).is_err());
        send(actor, b"failTracing:", &[abi::size(0).unwrap()]).unwrap();
        send(roots, b"release:", &[node_token]).unwrap();
        send(gc, b"collectPrecisely", &[]).unwrap();
        println!("Storage Oop observation, nested refusal, expired callbacks and reporting-time GC: passed");

        let value_args = send(id.root, b"sequence", &[namespace]).unwrap();
        let truth = id.send(names[8], id.load, &[value_args]).unwrap();
        assert!(id.send(names[9], id.load, &[value_args]).unwrap().is_null());
        let falsity = send(namespace, b"at:", &[sel(b"false")]).unwrap();
        let nil = send(namespace, b"at:", &[sel(b"nil")]).unwrap();
        assert_eq!(send(truth, b"not", &[]).unwrap(), falsity);
        assert_eq!(send(falsity, b"not", &[]).unwrap(), truth);
        assert_eq!(send(nil, b"isNil", &[]).unwrap(), truth);
        assert_eq!(send(truth, b"isNil", &[]).unwrap(), falsity);
        assert_eq!(send(nil, b"notNil", &[]).unwrap(), falsity);
        assert_eq!(send(truth, b"==", &[falsity]).unwrap(), falsity);
        assert_eq!(send(truth, b"&", &[nil]).unwrap(), nil);
        assert_eq!(send(falsity, b"|", &[nil]).unwrap(), nil);
        let block_type = send(base, b"delegated", &[]).unwrap();
        let value_method = send(actor_vt, b"lookup", &[sel(b"valueProbe")]).unwrap();
        send(block_type, b"addMethod", &[sel(b"value"), value_method]).unwrap();
        let block = send(block_type, b"allocate", &[abi::size(0).unwrap()]).unwrap();
        let block_token = send(roots, b"retain:", &[block]).unwrap();
        let untaken = send(base, b"allocate", &[abi::size(0).unwrap()]).unwrap();
        let untaken_token = send(roots, b"retain:", &[untaken]).unwrap();
        let expected = send(actor, b"at:", &[abi::size(6).unwrap()]).unwrap();
        assert_eq!(send(truth, b"ifTrue:ifFalse:", &[block, untaken]).unwrap(), expected);
        assert_eq!(send(falsity, b"ifTrue:ifFalse:", &[untaken, block]).unwrap(), expected);
        assert_eq!(send(truth, b"ifFalse:", &[untaken]).unwrap(), nil);
        assert_eq!(send(falsity, b"and:", &[untaken]).unwrap(), falsity);
        assert_eq!(send(truth, b"or:", &[untaken]).unwrap(), truth);
        assert_eq!(abi::as_size(send(actor, b"at:", &[abi::size(24).unwrap()]).unwrap()), Some(2));
        let alternate = send(actor_vt, b"lookup", &[sel(b"doesNotUnderstand:")]).unwrap();
        send(block_type, b"addMethod", &[sel(b"value"), alternate]).unwrap();
        assert_eq!(abi::as_size(send(truth, b"ifTrue:", &[block]).unwrap()), Some(0));
        let string_type = send(base, b"delegated", &[]).unwrap();
        let echo = send(actor_vt, b"lookup", &[sel(b"echoName")]).unwrap();
        send(string_type, b"addMethod", &[sel(b"fromBytes:"), echo]).unwrap();
        let string = send(string_type, b"allocate", &[abi::size(0).unwrap()]).unwrap();
        send(truth, b"installPrinting:", &[string]).unwrap();
        let nil_string = send(string_type, b"allocate", &[abi::size(0).unwrap()]).unwrap();
        send(nil, b"installPrinting:", &[nil_string]).unwrap();
        for (value, length, first) in [(truth, 4, b't'), (falsity, 5, b'f'), (nil, 3, b'n')] {
            let text = send(value, b"printString", &[]).unwrap();
            assert_eq!(abi::as_size(send(text, b"size", &[]).unwrap()), Some(length));
            assert_eq!(abi::as_size(send(text, b"at:", &[abi::size(0).unwrap()]).unwrap()), Some(first as usize));
        }
        let boolean_type = send(id.root, b"behaviorOf:", &[truth]).unwrap();
        let printing = send(boolean_type, b"lookup", &[sel(b"printString")]).unwrap();
        let printing_token = send(roots, b"retain:", &[printing]).unwrap();
        send(boolean_type, b"removeMethod:", &[sel(b"printString")]).unwrap();
        send(gc, b"collectPrecisely", &[]).unwrap();
        send(id.root, b"identityOf:", &[string]).unwrap();
        send(string_type, b"addMethod", &[sel(b"fromBytes:"), replacement]).unwrap();
        let no_args = send(id.root, b"sequence", &[]).unwrap();
        assert_eq!(send(printing, b"invokeOn:arguments:", &[truth, no_args]).unwrap(), expected);
        for token in [block_token, untaken_token, printing_token] { send(roots, b"release:", &[token]).unwrap(); }
        println!("Boolean/nil identities, selected branches, live dependencies and captured printing contexts: passed");

        send(gc, b"registerRetirement:", &[actor]).unwrap();
        send(gc, b"registerRetirement:", &[actor]).unwrap();
        let dead = send(base, b"allocate", &[abi::size(0).unwrap()]).unwrap();
        if std::env::args().nth(2).as_deref() == Some("retirement-failure") {
            send(actor, b"at:put:", &[abi::size(26).unwrap(), abi::size(1).unwrap()]).unwrap();
            assert!(send(gc, b"collectPrecisely", &[]).is_err());
            send(id.root, b"identityOf:", &[dead]).unwrap();
            assert!(send(gc, b"collectPrecisely", &[]).is_err());
            println!("RETIREMENT_FAILURE_CONTAINED");
            return;
        }
        let reclaimed = send(gc, b"collectPrecisely", &[]).unwrap();
        assert_eq!(send(actor, b"at:", &[abi::size(25).unwrap()]).unwrap(), reclaimed);
        assert!(send(id.root, b"identityOf:", &[dead]).is_err());
        println!("retirement owners see live Oops before release and are registered once: passed");

        let boxed = Box::new(id);
        let address = (&*boxed as *const Protocol).addr();
        let selector = sel(b"allocationCount").addr();
        let refused = std::thread::spawn(move || {
            let p = *(address as *const Protocol);
            p.send(
                p.root,
                core::ptr::with_exposed_provenance_mut(selector),
                &[],
            )
            .is_err()
        })
        .join()
        .unwrap();
        assert!(refused);
        println!("cross-thread Id entry rejected: passed");
        let source = std::fs::read_to_string(root.join("tests/fixtures/oop-id-v2/tcc-caller.c"))
            .unwrap()
            .replace(
                "\"id.h\"",
                &format!("{:?}", root.join("tests/fixtures/oop-id-v2/id.h")),
            );
        let source = std::ffi::CString::new(source).unwrap();
        let unit =
            birnam_native_compiler::Unit::compile(&source, &[]).expect("vendored TCC caller");
        type Check =
            unsafe extern "C" fn(*const Protocol, Oop, Oop, Oop, Oop, Oop, Oop, Oop) -> Oop;
        let check: Check = core::mem::transmute(unit.symbol(c"check_oop_id").unwrap().as_ptr());
        let value = check(
            &id,
            actor,
            sel(b"dynamicName:"),
            gc,
            sel(b"collectPrecisely"),
            roots,
            sel(b"retain:"),
            sel(b"release:"),
        );
        assert!(!abi::is_fault(value) && !value.is_null());
        println!("vendored TinyCC caller through real Oop Id and GC: passed");
        println!("OOP_SLICE_OK");
    }
}
