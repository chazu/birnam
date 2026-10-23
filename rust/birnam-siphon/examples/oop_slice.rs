//! Cold process acceptance driver. Upper Birnam libraries/images are absent.
use birnam_abi::{self as abi, Oop, id_protocol::Protocol};
use std::{os::unix::ffi::OsStrExt, path::PathBuf};
#[path="oop_slice/numeric.rs"]
mod numeric;
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
            library.join("libbirnam_object_model.dylib"),
            library.join("libbirnam_word.dylib"),
            library.join("libbirnam_bytes.dylib"),
            library.join("libbirnam_string.dylib"),
            library.join("libbirnam_float.dylib"),
            library.join("libbirnam_integer.dylib"),
            library.join("libbirnam_vector.dylib"),
        ];
        let words: &[&[u8]] = &[
            b"CollectionVt",b"collectionProbe",
            b"Vector",b"VectorNode",b"sizeWord",b"atWord:",b"reversed",b"sortBy:",b"mergeSortBy:",b"inject:into:",b"do:",b"sumValues",b"vectorKey",b"value:value:",
            b"value:",b"behaviorEpoch",
            b"installFloat:",b"integerValue:",b"compareInteger:with:",b"word",b"mod:",b"to:do:",b"asFloat",b"hasBootstrapArithmetic:",b"canOmitIndexing",b"evaluate:receiver:argument:with:",b"evaluatePredicate:then:left:right:limit:in:epoch:with:",b"numericOperation:left:right:",b"commitNumericPredicate:",b"numericDecline",b"numericCommit",b"loopValue",
            b"Float",b"FloatVt",b"integerInput",b"errorFactory",b"numericAllocate",b"Error",b"ErrorVt",b"kind:message:",b"overflow",b"zeroDivide",b"domain",b"typeMismatch",b"+",b"-",b"*",b"/",b"<",b"<=",b">",b">=",b"negated",b"abs",b"floor",b"ceiling",b"rounded",b"sqrt",
            b"String",b"StringBuilder",b"asBytes",b"bytes",b"trimmed",b"replace:with:",b"asSymbol",b"asString",b"add:",b"new",b"clone",b",",b"=",b"installNumbers:",b"installCollections:",b"byteAt:",b"copyFrom:to:",b"indexOf:",b"bracketDepth",b"asInteger",b"asNumber",b"addByte:",b"split:",b"bitsOf:",b"numericWord",b"vectorNew",b"vectorAdd",b"collectionEquals:",b"collectionHash",
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
            b"globalLookup", b"setGlobalLookup:", b"globalLookupCalls",
            b"caughtFailure", b"rethrow:", b"fault:detail:", b"allocationAt:",
            b"canOmitAllocationFor:", b"allocate:bytes:", b"nativeBindingsMatch:", b"bind:to:",
            b"setLayoutOf:to:rest:", b"slotAt:in:", b"slotAt:in:put:", b"clone", b"objectPrototype",
            b"ownerOf:", b"ownerOf:put:", b"ensurePrivateVtableFor:", b"cloneAllocate", b"convertIndex",
            b"Integer", b"wordOf:", b"slot:in:", b"imageSizeOf:", b"slotKindOf:at:",
            b"fromUnsignedLow:high:", b"low32", b"high32", b"fromBits:", b"bits", b"wrappingAdd:", b"wrappingSubtract:",
            b"shiftLeft:", b"unsignedDivide:", b"tryPrimitiveExpression:", b"tryPrimitiveExpressionAtEpoch:",
            b"imageSlotAt:in:", b"restoreImageSlotAt:in:put:", b"apply:to:with:",
            b"fromByteValues", b"appendByteValues", b"appendBytes:", b"appendTo:", b"bytes", b"length", b"byteAtWord:",
            b"sliceFromWord:toWord:", b"sha256", b"readU64At:", b"readTaggedTableWithSchema:limit:", b"withSymbolName:",
            b"fromWord:", b"numberIdentity", b"externalCharge",
            b"activate", b"automaticCollection:", b"budget:", b"allocationBytes",
            b"allocationChargeFor:", b"chargeExternalAllocation:", b"prepareAllocation",
            b"precisePayloads", b"precisePayloads:", b"chargeAfterGC",
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
        assert!(send(gc, b"collectPrecisely", &[]).is_err());
        if std::env::args().nth(2).as_deref() == Some("allocation-policy") {
            let base = send(id.root, b"objectBehavior", &[]).unwrap();
            let unknown = send(base, b"allocate", &[abi::size(8).unwrap()]).unwrap();
            assert!(send(gc, b"activate", &[]).is_err());
            send(id.root, b"identityOf:", &[unknown]).unwrap();
            assert!(send(gc, b"collectPrecisely", &[]).is_err());
            let owner = send(base, b"allocate", &[abi::size(0).unwrap()]).unwrap();
            send(gc, b"trace:with:", &[base, owner]).unwrap();
        }
        send(gc, b"activate", &[]).expect("GC activation");
        if std::env::args().nth(2).as_deref() == Some("allocation-policy") {
            let base = send(id.root, b"objectBehavior", &[]).unwrap();
            send(gc, b"trace:with:", &[base, core::ptr::null_mut()]).unwrap();
        }
        send(gc, b"automaticCollection:", &[abi::size(0).unwrap()]).unwrap();
        let load_args = send(id.root, b"sequence", &[roots, gc]).unwrap();
        let actor = id
            .send(names[2], id.load, &[load_args])
            .expect("independent native module loaded");
        println!("cold bootstrap and three native modules: passed");
        if std::env::args().nth(2).as_deref() == Some("global-lookup") {
            let base=send(id.root,b"objectBehavior",&[]).unwrap();
            let meta=send(id.root,b"behaviorOf:",&[base]).unwrap();
            let original=send(meta,b"lookup",&[sel(b"lookup")]).unwrap();
            let actor_type=send(id.root,b"behaviorOf:",&[actor]).unwrap();
            let replacement=send(actor_type,b"lookup",&[sel(b"globalLookup")]).unwrap();
            send(actor,b"setGlobalLookup:",&[original]).unwrap();
            send(meta,b"addMethod",&[sel(b"lookup"),replacement]).unwrap();
            println!("GLOBAL_LOOKUP_INSTALLED");
            send(id.root,b"allocationCount",&[]).unwrap();
            send(gc,b"collectPrecisely",&[]).unwrap();
            send(meta,b"addMethod",&[sel(b"lookup"),original]).unwrap();
            let before=abi::as_size(send(actor,b"globalLookupCalls",&[]).unwrap()).unwrap();
            assert!(before>0);
            // The base case is an object graph relation, not a fixed list of
            // recognized metavtables. A newly constructed root works too.
            let independent=send(meta,b"delegated",&[]).unwrap();
            send(independent,b"addMethod",&[sel(b"lookup"),replacement]).unwrap();
            send(id.root,b"reassociateBehaviorOf:with:",&[independent,independent]).unwrap();
            let behavior=send(base,b"delegated",&[]).unwrap();
            send(id.root,b"reassociateBehaviorOf:with:",&[behavior,independent]).unwrap();
            let receiver=send(behavior,b"allocate",&[abi::size(0).unwrap()]).unwrap();
            assert_eq!(send(receiver,b"yourself",&[]).unwrap(),receiver);
            assert!(abi::as_size(send(actor,b"globalLookupCalls",&[]).unwrap()).unwrap()>before);
            println!("GLOBAL_LOOKUP_OK");
            return;
        }
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
        assert!(!abi::is_fault(failure), "caught errors are ordinary object references");
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

        if std::env::args().nth(2).as_deref() == Some("allocation-policy") {
            let owner = send(base, b"allocate", &[abi::size(0).unwrap()]).unwrap();
            send(gc, b"trace:with:", &[base, owner]).unwrap();
            send(gc, b"collectPrecisely", &[]).unwrap();
            assert_eq!(abi::as_size(send(gc, b"allocationBytes", &[]).unwrap()), Some(0));
            let charge = send(id.root, b"allocationChargeFor:", &[abi::size(16).unwrap()]).unwrap();
            send(base, b"allocate", &[abi::size(16).unwrap()]).unwrap();
            assert_eq!(send(gc, b"allocationBytes", &[]).unwrap(), charge);
            send(gc, b"chargeExternalAllocation:", &[abi::size(7).unwrap()]).unwrap();
            let charged = abi::as_size(charge).unwrap()+7;
            assert_eq!(abi::as_size(send(gc, b"allocationBytes", &[]).unwrap()), Some(charged));
            send(gc, b"budget:", &[abi::size(1).unwrap()]).unwrap();
            send(gc, b"automaticCollection:", &[abi::size(1).unwrap()]).unwrap();
            assert_eq!(abi::as_size(send(gc, b"allocationBytes", &[]).unwrap()), Some(charged));
            send(gc, b"prepareAllocation", &[]).unwrap();
            assert_eq!(abi::as_size(send(gc, b"allocationBytes", &[]).unwrap()), Some(0));
            send(gc, b"automaticCollection:", &[abi::size(0).unwrap()]).unwrap();
            let root_type = send(id.root, b"behaviorOf:", &[id.root]).unwrap();
            let original = send(root_type, b"lookup", &[sel(b"allocationChargeFor:")]).unwrap();
            let one = send(actor_vt, b"lookup", &[sel(b"doesNotUnderstand:")]).unwrap();
            send(root_type, b"addMethod", &[sel(b"allocationChargeFor:"), one]).unwrap();
            send(base, b"allocate", &[abi::size(0).unwrap()]).unwrap();
            assert_eq!(abi::as_size(send(gc, b"allocationBytes", &[]).unwrap()), Some(1));
            send(root_type, b"addMethod", &[sel(b"allocationChargeFor:"), original]).unwrap();
            send(actor, b"at:put:", &[abi::size(27).unwrap(), original]).unwrap();
            let collecting_charge = send(actor_vt, b"lookup", &[sel(b"chargeAfterGC")]).unwrap();
            send(root_type, b"addMethod", &[sel(b"allocationChargeFor:"), collecting_charge]).unwrap();
            send(gc, b"chargeExternalAllocation:", &[abi::size(9).unwrap()]).unwrap();
            let fresh = send(base, b"allocate", &[abi::size(0).unwrap()]).unwrap();
            send(id.root, b"identityOf:", &[fresh]).unwrap();
            assert_eq!(abi::as_size(send(gc, b"allocationBytes", &[]).unwrap()), Some(8));
            send(root_type, b"addMethod", &[sel(b"allocationChargeFor:"), original]).unwrap();
            send(gc, b"precisePayloads:", &[abi::size(1).unwrap()]).unwrap();
            assert_eq!(abi::as_size(send(gc, b"precisePayloads", &[]).unwrap()), Some(1));
            println!("ALLOCATION_POLICY_OK");
        }

        let caught = send(actor, b"caughtFailure", &[]).unwrap();
        assert_eq!(caught, abi::object_reference(id.invalid));
        assert!(!abi::is_fault(caught));
        let detail = send(base, b"allocate", &[abi::size(0).unwrap()]).unwrap();
        let error = send(id.root, b"fault:detail:", &[sel(b"missingProbe"), detail]).unwrap_err();
        let data = send(id.root, b"sequence", &[error]).unwrap();
        let token = send(roots, b"retain:", &[data]).unwrap();
        send(gc, b"collectPrecisely", &[]).unwrap();
        let value = send(data, b"at:", &[abi::size(0).unwrap()]).unwrap();
        assert_eq!(value, error);
        assert_eq!(send(value, b"detail", &[]).unwrap(), detail);
        assert_eq!(send(id.root, b"allocationAt:", &[abi::size(value.addr()).unwrap()]).unwrap(), value);
        assert_eq!(send(actor, b"rethrow:", &[value]).unwrap_err(), value);
        assert_eq!(send(id.root, b"identityOf:", &[value]).unwrap(), send(id.root, b"identityOf:", &[abi::fault_reference(value)]).unwrap());
        send(roots, b"release:", &[token]).unwrap();
        println!("caught failures are ordinary data, survive GC and rethrow with the same identity: passed");

        assert_eq!(abi::as_size(send(gc, b"canOmitAllocationFor:", &[base]).unwrap()), Some(1));
        let allocation = send(id.root, b"bind:to:", &[sel(b"allocate"), base]).unwrap();
        let witness = send(allocation, b"withContext:", &[gc]).unwrap();
        assert_eq!(abi::as_size(send(id.root, b"nativeBindingsMatch:", &[base, sel(b"allocate"), witness]).unwrap()), Some(1));
        let root_type = send(id.root, b"behaviorOf:", &[id.root]).unwrap();
        let raw = send(root_type, b"lookup", &[sel(b"allocate:bytes:")]).unwrap();
        let replacement = send(actor_vt, b"lookup", &[sel(b"doesNotUnderstand:")]).unwrap();
        send(root_type, b"addMethod", &[sel(b"allocate:bytes:"), replacement]).unwrap();
        assert_eq!(abi::as_size(send(gc, b"canOmitAllocationFor:", &[base]).unwrap()), Some(0));
        send(root_type, b"addMethod", &[sel(b"allocate:bytes:"), raw]).unwrap();
        assert_eq!(abi::as_size(send(gc, b"canOmitAllocationFor:", &[base]).unwrap()), Some(1));
        println!("allocation omission checks current Method objects and raw storage meanings: passed");

        let model_args=send(id.root,b"sequence",&[namespace]).unwrap();
        let model=id.send(names[10],id.load,&[model_args]).unwrap();
        send(actor,b"at:put:",&[abi::size(28).unwrap(),model]).unwrap();
        let record_type=send(base,b"delegated",&[]).unwrap();
        let layout=send(id.root,b"sequence",&[abi::size(1).unwrap(),abi::size(0).unwrap()]).unwrap();
        send(model,b"setLayoutOf:to:rest:",&[record_type,layout,abi::size(0).unwrap()]).unwrap();
        let record=send(model,b"allocate:bytes:",&[record_type,abi::size(16).unwrap()]).unwrap();
        send(actor,b"at:put:",&[abi::size(29).unwrap(),record]).unwrap();
        let child=send(actor,b"dynamicName:",&[abi::size(49).unwrap()]).unwrap();
        send(model,b"slotAt:in:put:",&[abi::size(0).unwrap(),record,child]).unwrap();
        // The fixture defines this record's second field as raw machine bits.
        record.cast::<u64>().add(1).write(0xfedc_ba98_7654_3210);
        assert!(send(model,b"slotAt:in:",&[abi::size(1).unwrap(),record]).is_err());
        let replacement_child=send(actor,b"dynamicName:",&[abi::size(51).unwrap()]).unwrap();
        send(actor,b"at:put:",&[abi::size(31).unwrap(),replacement_child]).unwrap();
        let meta=send(id.root,b"behaviorOf:",&[record_type]).unwrap();
        let original_allocate=send(meta,b"lookup",&[sel(b"allocate")]).unwrap();
        send(actor,b"at:put:",&[abi::size(30).unwrap(),original_allocate]).unwrap();
        let custom_meta=send(meta,b"delegated",&[]).unwrap();
        let replacing=send(actor_vt,b"lookup",&[sel(b"cloneAllocate")]).unwrap();
        send(custom_meta,b"addMethod",&[sel(b"allocate"),replacing]).unwrap();
        send(id.root,b"reassociateBehaviorOf:with:",&[record_type,custom_meta]).unwrap();
        let copy=send(record,b"clone",&[]).unwrap();
        let copy_token=send(roots,b"retain:",&[copy]).unwrap();
        send(id.root,b"reassociateBehaviorOf:with:",&[record_type,meta]).unwrap();
        assert_eq!(send(model,b"slotAt:in:",&[abi::size(0).unwrap(),record]).unwrap(),replacement_child);
        assert_eq!(send(model,b"slotAt:in:",&[abi::size(0).unwrap(),copy]).unwrap(),child);
        assert_eq!(copy.cast::<u64>().add(1).read(),0xfedc_ba98_7654_3210);
        let private=send(id.root,b"ensurePrivateVtableFor:",&[record]).unwrap();
        assert_eq!(send(id.root,b"ownerOf:",&[private]).unwrap(),record);
        assert_eq!(send(id.root,b"ensurePrivateVtableFor:",&[record]).unwrap(),private);
        record.cast::<usize>().add(1).write(replacement_child.addr());
        assert!(send(model,b"slotAt:in:",&[abi::size(1).unwrap(),record]).is_err(), "private behavior must preserve the inherited raw-word layout");
        let integer_type=send(base,b"delegated",&[]).unwrap();
        let conversion=send(actor_vt,b"lookup",&[sel(b"convertIndex")]).unwrap();
        send(integer_type,b"addMethod",&[sel(b"wordOf:"),conversion]).unwrap();
        let integer=send(integer_type,b"allocate",&[abi::size(0).unwrap()]).unwrap();
        send(namespace,b"at:put:",&[sel(b"Integer"),integer]).unwrap();
        let index=send(base,b"allocate",&[abi::size(0).unwrap()]).unwrap();
        send(actor,b"at:put:",&[abi::size(33).unwrap(),index]).unwrap();
        assert_eq!(send(model,b"slot:in:",&[index,record]).unwrap(),replacement_child);
        let raw_only=send(base,b"allocate",&[abi::size(0).unwrap()]).unwrap();
        record.cast::<usize>().add(1).write(raw_only.addr());
        let precision=send(gc,b"precisePayloads",&[]).unwrap();
        send(gc,b"precisePayloads:",&[abi::size(1).unwrap()]).unwrap();
        send(gc,b"collectPrecisely",&[]).unwrap();
        assert!(send(id.root,b"identityOf:",&[raw_only]).is_err(), "raw words in inherited layouts are not references");
        send(gc,b"precisePayloads:",&[precision]).unwrap();
        send(gc,b"collectPrecisely",&[]).unwrap();
        assert_eq!(abi::as_size(send(model,b"imageSizeOf:",&[copy]).unwrap()),Some(16));
        send(roots,b"release:",&[copy_token]).unwrap();
        println!("ObjectModel layouts, clone snapshots, private behaviors and nested conversion roots: passed");

        let word_args=send(id.root,b"sequence",&[namespace]).unwrap();
        let word=id.send(names[11],id.load,&[word_args]).unwrap();
        let max=send(word,b"fromUnsignedLow:high:",&[abi::size(u32::MAX as usize).unwrap(),abi::size(u32::MAX as usize).unwrap()]).unwrap();
        let max_token=send(roots,b"retain:",&[max]).unwrap();
        assert_eq!(send(max,b"bits",&[]).unwrap(),max);
        assert_eq!(abi::as_size(send(max,b"low32",&[]).unwrap()),Some(u32::MAX as usize));
        assert_eq!(abi::as_size(send(max,b"high32",&[]).unwrap()),Some(u32::MAX as usize));
        let copied=send(word,b"fromBits:",&[max]).unwrap();
        assert_ne!(max,copied);
        assert_eq!(abi::as_size(send(copied,b"high32",&[]).unwrap()),Some(u32::MAX as usize));
        let one=send(word,b"fromBits:",&[abi::size(1).unwrap()]).unwrap();
        let one_token=send(roots,b"retain:",&[one]).unwrap();
        let wrapped=send(max,b"wrappingAdd:",&[one]).unwrap();
        assert_eq!(abi::as_size(send(wrapped,b"bits",&[]).unwrap()),Some(0));
        let shifted=send(one,b"shiftLeft:",&[abi::size(64).unwrap()]).unwrap();
        assert_eq!(abi::as_size(send(shifted,b"bits",&[]).unwrap()),Some(0));
        assert!(send(one,b"unsignedDivide:",&[abi::size(0).unwrap()]).is_err());
        let bytecode:Vec<_>=[1,2,1,2,6,0,1].into_iter().map(|n|abi::size(n).unwrap()).collect();
        let program=send(id.root,b"sequence",&bytecode).unwrap();
        let program_token=send(roots,b"retain:",&[program]).unwrap();
        let answer=send(word,b"tryPrimitiveExpression:",&[program,max,one]).unwrap();
        assert!(!answer.is_null());
        assert_eq!(abi::as_size(send(answer,b"bits",&[]).unwrap()),Some(0));
        let word_type=send(id.root,b"behaviorOf:",&[word]).unwrap();
        let addition=send(word_type,b"lookup",&[sel(b"wrappingAdd:")]).unwrap();
        send(word_type,b"addMethod",&[sel(b"wrappingAdd:"),replacement]).unwrap();
        assert!(send(word,b"tryPrimitiveExpression:",&[program,max,one]).unwrap().is_null());
        send(word_type,b"addMethod",&[sel(b"wrappingAdd:"),addition]).unwrap();
        let program_type=send(id.root,b"behaviorOf:",&[program]).unwrap();
        let custom_program=send(program_type,b"delegated",&[]).unwrap();
        send(custom_program,b"addMethod",&[sel(b"apply:to:with:"),replacement]).unwrap();
        send(id.root,b"reassociateBehaviorOf:with:",&[program,custom_program]).unwrap();
        assert_eq!(abi::as_size(send(word,b"tryPrimitiveExpression:",&[program,max,one]).unwrap()),Some(5));
        send(id.root,b"reassociateBehaviorOf:with:",&[program,program_type]).unwrap();
        send(model,b"restoreImageSlotAt:in:put:",&[abi::size(1).unwrap(),record,max]).unwrap();
        let restored=send(model,b"imageSlotAt:in:",&[abi::size(1).unwrap(),record]).unwrap();
        assert_eq!(abi::as_size(send(restored,b"low32",&[]).unwrap()),Some(u32::MAX as usize));
        assert_eq!(abi::as_size(send(restored,b"high32",&[]).unwrap()),Some(u32::MAX as usize));
        send(gc,b"collectPrecisely",&[]).unwrap();
        assert_eq!(abi::as_size(send(max,b"high32",&[]).unwrap()),Some(u32::MAX as usize));
        for token in [max_token,one_token,program_token]{send(roots,b"release:",&[token]).unwrap();}
        println!("Word full-width values, arithmetic, live expression guards and image slots: passed");

        let identity_number=send(actor_vt,b"lookup",&[sel(b"numberIdentity")]).unwrap();
        send(integer_type,b"addMethod",&[sel(b"wordOf:"),identity_number]).unwrap();
        send(integer_type,b"addMethod",&[sel(b"fromWord:"),identity_number]).unwrap();
        let bytes_args=send(id.root,b"sequence",&[namespace]).unwrap();
        let bytes=id.send(names[12],id.load,&[bytes_args]).unwrap();
        let gc_type=send(id.root,b"behaviorOf:",&[gc]).unwrap();
        let original_charge=send(gc_type,b"lookup",&[sel(b"chargeExternalAllocation:")]).unwrap();
        send(actor,b"at:put:",&[abi::size(35).unwrap(),original_charge]).unwrap();
        let collecting_charge=send(actor_vt,b"lookup",&[sel(b"externalCharge")]).unwrap();
        send(gc_type,b"addMethod",&[sel(b"chargeExternalAllocation:"),collecting_charge]).unwrap();
        let data=send(bytes,b"fromByteValues",&[abi::size(0).unwrap(),abi::size(255).unwrap(),abi::size(17).unwrap(),abi::size(65).unwrap()]).unwrap();
        let data_token=send(roots,b"retain:",&[data]).unwrap();
        send(data,b"appendBytes:",&[data]).unwrap();
        assert_eq!(abi::as_size(send(data,b"length",&[]).unwrap()),Some(8));
        for (i,expected) in [0,255,17,65,0,255,17,65].into_iter().enumerate(){
            assert_eq!(abi::as_size(send(data,b"byteAtWord:",&[abi::size(i).unwrap()]).unwrap()),Some(expected));
        }
        send(gc_type,b"addMethod",&[sel(b"chargeExternalAllocation:"),original_charge]).unwrap();
        let name=send(id.root,b"nameWithBytes",&[abi::size(104).unwrap(),abi::size(105).unwrap()]).unwrap();
        let text=send(bytes,b"fromBytes:",&[name]).unwrap();
        assert_eq!(abi::as_size(send(text,b"length",&[]).unwrap()),Some(2));
        let name_type=send(id.root,b"behaviorOf:",&[name]).unwrap();
        let original_append=send(name_type,b"lookup",&[sel(b"appendTo:")]).unwrap();
        let alternate=send(actor_vt,b"lookup",&[sel(b"doesNotUnderstand:")]).unwrap();
        send(name_type,b"addMethod",&[sel(b"appendTo:"),alternate]).unwrap();
        let changed=send(bytes,b"fromBytes:",&[name]).unwrap();
        assert_eq!(abi::as_size(send(changed,b"length",&[]).unwrap()),Some(0));
        send(name_type,b"addMethod",&[sel(b"appendTo:"),original_append]).unwrap();
        let symbol_copy=send(text,b"withSymbolName:",&[sel(b"missingProbe")]).unwrap();
        assert_eq!(abi::as_size(send(symbol_copy,b"length",&[]).unwrap()),Some(14));
        let full=send(bytes,b"fromByteValues",&vec![abi::size(255).unwrap();8]).unwrap();
        let full_word=send(full,b"readU64At:",&[abi::size(0).unwrap()]).unwrap();
        assert_eq!(abi::as_size(send(full_word,b"high32",&[]).unwrap()),Some(u32::MAX as usize));
        let digest=send(data,b"sha256",&[]).unwrap();
        assert_eq!(abi::as_size(send(digest,b"length",&[]).unwrap()),Some(32));
        for (index,expected) in [41, 66, 222, 45, 53, 6, 155, 43, 132, 77, 179, 102, 216, 217, 185, 241, 93, 238, 8, 18, 225, 239, 73, 145, 7, 34, 38, 160, 2, 133, 134, 19].into_iter().enumerate(){
            assert_eq!(abi::as_size(send(digest,b"byteAtWord:",&[abi::size(index).unwrap()]).unwrap()),Some(expected));
        }
        let source:Vec<_>=[1,0,0,0,2,7,0,0,0,1,0,0,0,4,3,0,0,0,120,121,122].into_iter().map(|v|abi::size(v).unwrap()).collect();
        let source=send(bytes,b"fromByteValues",&source).unwrap();
        let schema=send(bytes,b"fromByteValues",&[48,48,50,48,52].map(|v|abi::size(v).unwrap())).unwrap();
        let table=send(source,b"readTaggedTableWithSchema:limit:",&[schema,abi::size(10).unwrap()]).unwrap();
        let table_token=send(roots,b"retain:",&[table]).unwrap();
        send(gc,b"collectPrecisely",&[]).unwrap();
        assert!(send(id.root,b"identityOf:",&[source]).is_err());
        assert_eq!(abi::as_size(send(table,b"size",&[]).unwrap()),Some(1));
        let row=send(table,b"at:",&[abi::size(0).unwrap()]).unwrap();
        let fields=send(row,b"at:",&[abi::size(1).unwrap()]).unwrap();
        let field=send(fields,b"at:",&[abi::size(0).unwrap()]).unwrap();
        let contents=send(field,b"at:",&[abi::size(2).unwrap()]).unwrap();
        assert_eq!(abi::as_size(send(contents,b"length",&[]).unwrap()),Some(3));
        assert_eq!(abi::as_size(send(contents,b"byteAtWord:",&[abi::size(0).unwrap()]).unwrap()),Some(120));
        for token in [data_token,table_token]{send(roots,b"release:",&[token]).unwrap();}
        send(gc,b"collectPrecisely",&[]).unwrap();
        println!("Bytes binary values, live accounting with GC, full-width reads and independent decoded views: passed");


        let replacement=send(actor_vt,b"lookup",&[sel(b"replacement")]).unwrap();
        let string_args=send(id.root,b"sequence",&[namespace]).unwrap();
        let string=id.send(names[13],id.load,&[string_args]).expect("String loaded");
        let string_type=send(id.root,b"behaviorOf:",&[string]).unwrap();
        let make_text=|text:&[u8]|{
            let values:Vec<_>=text.iter().map(|&b|abi::size(b as usize).unwrap()).collect();
            let source=send(id.root,b"sequence",&values).unwrap();
            send(string,b"fromBytes:",&[source]).unwrap()
        };
        let text_bytes=|value|{
            let bytes=send(value,b"asBytes",&[]).unwrap();
            let length=abi::as_size(send(bytes,b"length",&[]).unwrap()).unwrap();
            (0..length).map(|i|abi::as_size(send(bytes,b"byteAtWord:",&[abi::size(i).unwrap()]).unwrap()).unwrap() as u8).collect::<Vec<_>>()
        };
        send(gc_type,b"addMethod",&[sel(b"chargeExternalAllocation:"),collecting_charge]).unwrap();
        let original=make_text(b" \tab\0\xff ab\r\n");
        let original_token=send(roots,b"retain:",&[original]).unwrap();
        assert_eq!(send(id.root,b"allocationSizeOf:",&[original]).unwrap(),abi::size(0).unwrap());
        assert_eq!(text_bytes(original),b" \tab\0\xff ab\r\n");
        let trimmed=send(original,b"trimmed",&[]).unwrap();
        let trimmed_token=send(roots,b"retain:",&[trimmed]).unwrap();
        assert_eq!(text_bytes(trimmed),b"ab\0\xff ab");
        let copy=send(trimmed,b"clone",&[]).unwrap();
        let copy_token=send(roots,b"retain:",&[copy]).unwrap();
        assert_ne!(trimmed,copy);
        assert_eq!(send(trimmed,b"=",&[copy]).unwrap(),truth);
        assert_eq!(send(trimmed,b"collectionEquals:",&[copy]).unwrap(),truth);
        assert_eq!(send(trimmed,b"collectionHash",&[]).unwrap(),send(copy,b"collectionHash",&[]).unwrap());
        let needle=make_text(b"ab");let needle_token=send(roots,b"retain:",&[needle]).unwrap();
        let insert=make_text(b"X");let insert_token=send(roots,b"retain:",&[insert]).unwrap();
        let replaced=send(trimmed,b"replace:with:",&[needle,insert]).unwrap();
        assert_eq!(text_bytes(replaced),b"X\0\xff X");
        let symbol=send(trimmed,b"asSymbol",&[]).unwrap();
        let expected_name=send(id.root,b"nameWithBytes",&[abi::size(97).unwrap(),abi::size(98).unwrap()]).unwrap();
        assert_eq!(symbol,id.send(expected_name,id.intern,&[]).unwrap());
        let source=send(trimmed,b"bytes",&[]).unwrap();
        let source_token=send(roots,b"retain:",&[source]).unwrap();
        send(source,b"appendByteValues",&[abi::size(33).unwrap()]).unwrap();
        assert_eq!(text_bytes(trimmed),b"ab\0\xff ab");
        let from_buffer=send(string,b"fromBytes:",&[source]).unwrap();
        assert_eq!(text_bytes(from_buffer),b"ab\0\xff ab!");
        let builder=send(namespace,b"at:",&[sel(b"StringBuilder")]).unwrap();
        let builder=send(builder,b"new",&[]).unwrap();
        let builder_token=send(roots,b"retain:",&[builder]).unwrap();
        send(builder,b"add:",&[needle]).unwrap();send(builder,b"add:",&[builder]).unwrap();
        assert_eq!(text_bytes(send(builder,b"asString",&[]).unwrap()),b"abab");
        let empty_builder=send(builder,b"clone",&[]).unwrap();
        assert_eq!(text_bytes(send(empty_builder,b"asString",&[]).unwrap()),b"");
        // Subsequent construction observes replacement of the provider protocol.
        let name=send(id.root,b"nameWithBytes",&[abi::size(97).unwrap()]).unwrap();
        let name_token=send(roots,b"retain:",&[name]).unwrap();
        let name_type=send(id.root,b"behaviorOf:",&[name]).unwrap();
        let apply=send(name_type,b"lookup",&[sel(b"apply:to:with:")]).unwrap();
        let apply_token=send(roots,b"retain:",&[apply]).unwrap();
        send(name_type,b"addMethod",&[sel(b"apply:to:with:"),replacement]).unwrap();
        assert_eq!(send(string,b"fromBytes:",&[name]).unwrap(),expected);
        send(name_type,b"addMethod",&[sel(b"apply:to:with:"),apply]).unwrap();
        let factory=send(string_type,b"lookup",&[sel(b"fromBytes:")]).unwrap();
        let factory_token=send(roots,b"retain:",&[factory]).unwrap();
        send(string_type,b"addMethod",&[sel(b"fromBytes:"),replacement]).unwrap();
        assert_eq!(send(original,b"trimmed",&[]).unwrap(),expected);
        send(string_type,b"addMethod",&[sel(b"fromBytes:"),factory]).unwrap();
        let bytes_type=send(id.root,b"behaviorOf:",&[bytes]).unwrap();
        let bytes_factory=send(bytes_type,b"lookup",&[sel(b"fromBytes:")]).unwrap();
        let bytes_factory_token=send(roots,b"retain:",&[bytes_factory]).unwrap();
        send(bytes_type,b"addMethod",&[sel(b"fromBytes:"),replacement]).unwrap();
        assert_eq!(send(original,b"asBytes",&[]).unwrap(),expected);
        send(bytes_type,b"addMethod",&[sel(b"fromBytes:"),bytes_factory]).unwrap();
        send(roots,b"release:",&[bytes_factory_token]).unwrap();
        send(gc_type,b"addMethod",&[sel(b"chargeExternalAllocation:"),original_charge]).unwrap();
        if std::env::args().nth(2).as_deref()==Some("numeric-integration") {
            numeric::check(&id,&send,&sel,[names[15],names[14],names[16]],[namespace,roots,gc,actor,base,word,string]);
            return;
        }
        // Numeric and collection adapters are independent C objects for this slice.
        let numeric_type=send(base,b"delegated",&[]).unwrap();
        let numeric_word=send(actor_vt,b"lookup",&[sel(b"numericWord")]).unwrap();
        for selector in [b"wordOf:".as_slice(),b"bitsOf:"]{send(numeric_type,b"addMethod",&[sel(selector),numeric_word]).unwrap();}
        for selector in [b"fromWord:".as_slice(),b"fromBits:"]{send(numeric_type,b"addMethod",&[sel(selector),identity_number]).unwrap();}
        let number_factory=send(numeric_type,b"allocate",&[abi::size(0).unwrap()]).unwrap();
        send(string,b"installNumbers:",&[number_factory,number_factory]).unwrap();
        assert_eq!(abi::as_size(send(trimmed,b"size",&[]).unwrap()),Some(7));
        assert_eq!(abi::as_size(send(trimmed,b"byteAt:",&[abi::size(3).unwrap()]).unwrap()),Some(255));
        assert_eq!(text_bytes(send(trimmed,b"at:",&[abi::size(3).unwrap()]).unwrap()),b"\xff");
        assert_eq!(text_bytes(send(trimmed,b"copyFrom:to:",&[abi::size(2).unwrap(),abi::size(4).unwrap()]).unwrap()),b"\0\xff");
        assert_eq!(send(trimmed,b"byteAt:",&[abi::size(7).unwrap()]).unwrap(),nil);
        assert_eq!(abi::as_size(send(trimmed,b"indexOf:",&[needle]).unwrap()),Some(0));
        send(builder,b"addByte:",&[abi::size(255).unwrap()]).unwrap();
        assert_eq!(abi::as_size(send(builder,b"size",&[]).unwrap()),Some(5));
        let signed=send(make_text(b" -9223372036854775808 "),b"asInteger",&[]).unwrap();
        assert_eq!(abi::as_size(send(signed,b"high32",&[]).unwrap()),Some(0x80000000));
        assert_eq!(abi::as_size(send(signed,b"low32",&[]).unwrap()),Some(0));
        assert_eq!(send(make_text(b"9223372036854775808"),b"asInteger",&[]).unwrap(),nil);
        let float_bits=send(make_text(b"-1.5"),b"asNumber",&[]).unwrap();
        assert_eq!(abi::as_size(send(float_bits,b"high32",&[]).unwrap()),Some(0xbff80000));
        assert_eq!(abi::as_size(send(float_bits,b"low32",&[]).unwrap()),Some(0));
        assert_eq!(abi::as_size(send(make_text(b"[\"[\" // ]\n{"),b"bracketDepth",&[]).unwrap()),Some(2));
        // Capture a numeric Method, detach it, collect, then invoke it.
        let numeric_method=send(string_type,b"lookup",&[sel(b"size")]).unwrap();
        let numeric_token=send(roots,b"retain:",&[numeric_method]).unwrap();
        for selector in [b"size".as_slice(),b"at:",b"byteAt:",b"copyFrom:to:",b"indexOf:",b"bracketDepth",b"asInteger",b"asNumber"]{send(string_type,b"removeMethod:",&[sel(selector)]).unwrap();}
        let builder_type=send(id.root,b"behaviorOf:",&[builder]).unwrap();
        for selector in [b"size".as_slice(),b"addByte:"]{send(builder_type,b"removeMethod:",&[sel(selector)]).unwrap();}
        send(gc,b"collectPrecisely",&[]).unwrap();
        send(id.root,b"identityOf:",&[number_factory]).unwrap();
        let empty=send(id.root,b"sequence",&[]).unwrap();
        assert_eq!(send(numeric_method,b"invokeOn:arguments:",&[trimmed,empty]).unwrap(),abi::size(7).unwrap());
        send(string_type,b"addMethod",&[sel(b"size"),numeric_method]).unwrap();
        let vector_type=send(base,b"delegated",&[]).unwrap();
        let vector_new=send(actor_vt,b"lookup",&[sel(b"vectorNew")]).unwrap();
        send(vector_type,b"addMethod",&[sel(b"new"),vector_new]).unwrap();
        let vector=send(vector_type,b"allocate",&[abi::size(0).unwrap()]).unwrap();
        let sample=send(id.root,b"sequence",&[]).unwrap();
        let sequence_type=send(id.root,b"behaviorOf:",&[sample]).unwrap();
        let vector_add=send(actor_vt,b"lookup",&[sel(b"vectorAdd")]).unwrap();
        send(sequence_type,b"addMethod",&[sel(b"add:"),vector_add]).unwrap();
        send(string,b"installCollections:",&[vector]).unwrap();
        let split_text=make_text(b"abXabX");let split_token=send(roots,b"retain:",&[split_text]).unwrap();
        let parts=send(split_text,b"split:",&[insert]).unwrap();let parts_token=send(roots,b"retain:",&[parts]).unwrap();
        assert_eq!(send(parts,b"at:",&[abi::size(0).unwrap()]).unwrap(),abi::size(3).unwrap());
        for (i,want) in [b"ab".as_slice(),b"ab",b""].into_iter().enumerate(){let part=send(parts,b"at:",&[abi::size(i+1).unwrap()]).unwrap();assert_eq!(text_bytes(part),want);}
        send(vector_type,b"addMethod",&[sel(b"new"),replacement]).unwrap();
        assert!(send(split_text,b"split:",&[insert]).is_err());
        send(vector_type,b"addMethod",&[sel(b"new"),vector_new]).unwrap();
        send(sequence_type,b"removeMethod:",&[sel(b"add:")]).unwrap();
        for token in [original_token,trimmed_token,copy_token,needle_token,insert_token,source_token,builder_token,name_token,apply_token,factory_token,numeric_token,split_token,parts_token]{send(roots,b"release:",&[token]).unwrap();}
        send(gc,b"collectPrecisely",&[]).unwrap();
        assert!(send(id.root,b"identityOf:",&[original]).is_err());
        println!("String Oop transfers, binary text, builders, full-width parsing, live factories and captured extensions with GC: passed");


        let integer_input=send(actor_vt,b"lookup",&[sel(b"integerInput")]).unwrap();
        send(integer_type,b"addMethod",&[sel(b"wordOf:"),integer_input]).unwrap();
        let error_type=send(sequence_type,b"delegated",&[]).unwrap();
        let error_factory_method=send(actor_vt,b"lookup",&[sel(b"errorFactory")]).unwrap();
        send(error_type,b"addMethod",&[sel(b"kind:message:"),error_factory_method]).unwrap();
        let error_factory=send(id.root,b"sequence",&[]).unwrap();
        send(id.root,b"reassociateBehaviorOf:with:",&[error_factory,error_type]).unwrap();
        send(namespace,b"at:put:",&[sel(b"Error"),error_factory]).unwrap();
        send(namespace,b"at:put:",&[sel(b"ErrorVt"),error_type]).unwrap();
        let float_args=send(id.root,b"sequence",&[namespace]).unwrap();
        let float=id.send(names[14],id.load,&[float_args]).expect("Float loaded");
        let float_type=send(id.root,b"behaviorOf:",&[float]).unwrap();
        let float_meta=send(id.root,b"behaviorOf:",&[float_type]).unwrap();
        let private_meta=send(float_meta,b"delegated",&[]).unwrap();
        let collecting_allocate=send(actor_vt,b"lookup",&[sel(b"numericAllocate")]).unwrap();
        send(private_meta,b"addMethod",&[sel(b"allocate"),collecting_allocate]).unwrap();
        send(id.root,b"reassociateBehaviorOf:with:",&[float_type,private_meta]).unwrap();
        let word_value=|bits:u64|send(word,b"fromUnsignedLow:high:",&[abi::size(bits as u32 as usize).unwrap(),abi::size((bits>>32)as usize).unwrap()]).unwrap();
        let full_bits=|value|->u64{
            if let Some(bits)=abi::as_size(value){return bits as u64;}
            let lo=abi::as_size(send(value,b"low32",&[]).unwrap()).unwrap()as u64;
            let hi=abi::as_size(send(value,b"high32",&[]).unwrap()).unwrap()as u64;
            lo|(hi<<32)
        };
        let make_float=|bits:u64|send(float,b"fromBits:",&[word_value(bits)]).unwrap();
        let read_float=|value|full_bits(send(value,b"bits",&[]).unwrap());
        for bits in [0,1,0x8000_0000_0000_0000,0x7ff0_0000_0000_0000,0xfff0_0000_0000_0000,0xfff8_1234_5678_9abc,0x7ff0_1234_5678_9abc]{
            let value=make_float(bits);let token=send(roots,b"retain:",&[value]).unwrap();
            assert_eq!(read_float(value),bits);
            assert_eq!(full_bits(send(float,b"bitsOf:",&[value]).unwrap()),bits);
            send(gc,b"collectPrecisely",&[]).unwrap();assert_eq!(read_float(value),bits);
            send(roots,b"release:",&[token]).unwrap();
        }
        let left=make_float(2.5f64.to_bits());let left_token=send(roots,b"retain:",&[left]).unwrap();
        let right=make_float((-1.5f64).to_bits());let right_token=send(roots,b"retain:",&[right]).unwrap();
        assert_eq!(read_float(send(left,b"+",&[right]).unwrap()),1.0f64.to_bits());
        assert_eq!(read_float(send(left,b"+",&[abi::size(2).unwrap()]).unwrap()),4.5f64.to_bits());
        assert_eq!(send(left,b">",&[right]).unwrap(),truth);
        assert_eq!(read_float(send(right,b"negated",&[]).unwrap()),1.5f64.to_bits());
        assert_eq!(read_float(send(right,b"abs",&[]).unwrap()),1.5f64.to_bits());
        assert_eq!(full_bits(send(right,b"asInteger",&[]).unwrap()),(-1i64)as u64);
        assert_eq!(full_bits(send(right,b"floor",&[]).unwrap()),(-2i64)as u64);
        assert_eq!(full_bits(send(right,b"ceiling",&[]).unwrap()),(-1i64)as u64);
        assert_eq!(full_bits(send(right,b"rounded",&[]).unwrap()),(-2i64)as u64);
        let positive_zero=make_float(0);let pos_token=send(roots,b"retain:",&[positive_zero]).unwrap();
        let negative_zero=make_float(1<<63);let neg_token=send(roots,b"retain:",&[negative_zero]).unwrap();
        assert_eq!(send(positive_zero,b"collectionHash",&[]).unwrap(),send(negative_zero,b"collectionHash",&[]).unwrap());
        assert_eq!(send(positive_zero,b"=",&[negative_zero]).unwrap(),truth);
        for zero in [positive_zero,negative_zero]{
            let error=send(left,b"/",&[zero]).unwrap();
            assert_eq!(send(error,b"at:",&[abi::size(0).unwrap()]).unwrap(),sel(b"zeroDivide"));
            assert_eq!(text_bytes(send(error,b"at:",&[abi::size(1).unwrap()]).unwrap()),b"division by zero");
        }
        let domain_error=send(right,b"sqrt",&[]).unwrap();let domain_token=send(roots,b"retain:",&[domain_error]).unwrap();
        assert_eq!(send(domain_error,b"at:",&[abi::size(0).unwrap()]).unwrap(),sel(b"domain"));
        assert_eq!(send(left,b"+",&[domain_error]).unwrap(),domain_error);
        let infinity=make_float(f64::INFINITY.to_bits());let infinity_token=send(roots,b"retain:",&[infinity]).unwrap();
        let overflow=send(infinity,b"asInteger",&[]).unwrap();
        assert_eq!(send(overflow,b"at:",&[abi::size(0).unwrap()]).unwrap(),sel(b"overflow"));
        assert_eq!(text_bytes(send(negative_zero,b"printString",&[]).unwrap()),b"-0.0");
        let nan=make_float(0xfff8_1234_5678_9abc);let nan_token=send(roots,b"retain:",&[nan]).unwrap();
        assert_eq!(send(nan,b"=",&[nan]).unwrap(),falsity);
        assert_eq!(send(nan,b"collectionEquals:",&[nan]).unwrap(),truth);
        let nan2=make_float(0xfff8_1234_5678_9abc);
        assert_eq!(send(nan,b"collectionEquals:",&[nan2]).unwrap(),falsity);
        let constructor=send(float_type,b"lookup",&[sel(b"fromBits:")]).unwrap();let constructor_token=send(roots,b"retain:",&[constructor]).unwrap();
        send(float_type,b"addMethod",&[sel(b"fromBits:"),replacement]).unwrap();
        assert_eq!(send(left,b"+",&[right]).unwrap(),expected);
        send(float_type,b"addMethod",&[sel(b"fromBits:"),constructor]).unwrap();
        let error_constructor=send(error_type,b"lookup",&[sel(b"kind:message:")]).unwrap();let error_constructor_token=send(roots,b"retain:",&[error_constructor]).unwrap();
        send(error_type,b"addMethod",&[sel(b"kind:message:"),replacement]).unwrap();
        assert_eq!(send(right,b"sqrt",&[]).unwrap(),expected);
        send(error_type,b"addMethod",&[sel(b"kind:message:"),error_constructor]).unwrap();
        send(namespace,b"at:put:",&[sel(b"ErrorVt"),float_type]).unwrap();
        assert_eq!(send(left,b"+",&[right]).unwrap(),right);
        send(namespace,b"at:put:",&[sel(b"ErrorVt"),error_type]).unwrap();
        let text_constructor=send(string_type,b"lookup",&[sel(b"fromBytes:")]).unwrap();
        let text_constructor_token=send(roots,b"retain:",&[text_constructor]).unwrap();
        send(string_type,b"addMethod",&[sel(b"fromBytes:"),replacement]).unwrap();
        assert_eq!(send(left,b"printString",&[]).unwrap(),expected);
        send(string_type,b"addMethod",&[sel(b"fromBytes:"),text_constructor]).unwrap();
        send(roots,b"release:",&[text_constructor_token]).unwrap();
        send(integer_type,b"addMethod",&[sel(b"fromWord:"),replacement]).unwrap();
        assert_eq!(send(right,b"asInteger",&[]).unwrap(),expected);
        send(integer_type,b"addMethod",&[sel(b"fromWord:"),identity_number]).unwrap();
        let malformed=send(float_type,b"allocate",&[abi::size(0).unwrap()]).unwrap();
        assert!(send(float,b"bitsOf:",&[malformed]).unwrap().is_null());
        let private=send(id.root,b"ensurePrivateVtableFor:",&[left]).unwrap();
        assert_ne!(private,float_type);
        assert!(send(float,b"bitsOf:",&[left]).unwrap().is_null());
        send(id.root,b"reassociateBehaviorOf:with:",&[left,float_type]).unwrap();
        send(id.root,b"reassociateBehaviorOf:with:",&[float_type,float_meta]).unwrap();
        for token in [left_token,right_token,pos_token,neg_token,domain_token,infinity_token,nan_token,constructor_token,error_constructor_token]{send(roots,b"release:",&[token]).unwrap();}
        send(gc,b"collectPrecisely",&[]).unwrap();
        println!("Float exact IEEE bits, arithmetic, conversions, live constructors/ErrorVt and allocation-time GC: passed");

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
