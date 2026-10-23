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
        let before=send(id.root,b"allocationCount",&[]).unwrap();
        let object_type=send(id.root,b"objectBehavior",&[]).unwrap();
        assert!(send(object_type,b"allocate",&[abi::size(usize::MAX >> 1).unwrap()]).is_err());
        for bad in [abi::size(0).unwrap(),abi::size(256).unwrap(),id.root] {
            assert!(send(id.root,b"nameWithBytes",&[bad]).is_err());
        }
        assert_eq!(send(id.root,b"allocationCount",&[]).unwrap(),before);
        let empty_name=send(id.root,b"nameWithBytes",&[]).unwrap();
        assert_eq!(id.send(empty_name,id.intern,&[]).unwrap(),id.send(empty_name,id.intern,&[]).unwrap());
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
        // Failure is an object and can be inspected; ordinary object data remains data.
        let published = send(id.root, b"sequence", &[core::ptr::null_mut()]).unwrap();
        let token = send(roots, b"retain:", &[published]).unwrap();
        let args = send(id.root, b"sequence", &[published]).unwrap();
        let failure = id
            .send(names[3], id.load, &[args])
            .expect_err("intentional initializer failure");
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
        let custom_type = send(vt, b"delegated", &[]).unwrap();
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
