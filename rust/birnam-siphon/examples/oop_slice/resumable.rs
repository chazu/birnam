//! Real source, retained frame engine, current Id lookup, and actual TCC mappings.
use birnam_abi::{self as abi, Oop, id_protocol::Protocol};
pub(super) unsafe fn check(
    id: &Protocol,
    send: &impl Fn(Oop, &[u8], &[Oop]) -> Result<Oop, Oop>,
    sel: &impl Fn(&[u8]) -> Oop,
    objects: [Oop; 7],
) {
    unsafe {
        let [ns, roots, gc, string, actor, integer, blocks] = objects;
        let keep = |v| {
            send(roots, b"retain:", &[v]).unwrap();
            v
        };
        let token = |v| send(roots, b"retain:", &[v]).unwrap();
        let release = |t| {
            send(roots, b"release:", &[t]).unwrap();
        };
        let text = |bytes: &[u8]| {
            let values: Vec<_> = bytes
                .iter()
                .map(|&b| abi::size(b as usize).unwrap())
                .collect();
            let sequence = send(id.root, b"sequence", &values).unwrap();
            keep(send(string, b"fromBytes:", &[sequence]).unwrap())
        };
        let codegen = send(ns, b"at:", &[sel(b"Codegen")]).unwrap();
        let machines = send(ns, b"at:", &[sel(b"Machine")]).unwrap();
        let tcc = send(ns, b"at:", &[sel(b"TCC")]).unwrap();
        let truth = send(ns, b"at:", &[sel(b"true")]).unwrap();
        let compile = |source: &str| {
            let method = send(codegen, b"compileResumable:", &[text(source.as_bytes())]).unwrap();
            assert_ne!(send(method, b"isError", &[]).unwrap(), truth, "{source}");
            method
        };
        let int = |n| keep(send(integer, b"fromWord:", &[abi::size(n).unwrap()]).unwrap());
        let bits = |value| {
            let word = send(integer, b"wordOf:", &[value]).unwrap();
            if let Some(n) = abi::as_size(word) {
                n
            } else {
                abi::as_size(send(word, b"low32", &[]).unwrap()).unwrap()
            }
        };
        let collect = || {
            send(gc, b"collectPrecisely", &[]).unwrap();
            send(tcc, b"checkpoint", &[]).unwrap();
        };
        let units = || abi::as_size(send(tcc, b"liveNativeUnits", &[]).unwrap()).unwrap();
        let block = |source: &str| {
            let method = compile(source);
            let mt = token(method);
            let root = send(blocks, b"fromMethod:captureOperands:", &[method]).unwrap();
            let rt = token(root);
            let result =
                send(root, b"value", &[]).unwrap_or_else(|e| panic!("run source {source}: {e:?}"));
            let result_token = token(result);
            release(rt);
            release(mt);
            (result, result_token)
        };
        let zero = int(0);
        let one = int(1);
        let five = int(5);
        let ten = int(10);
        let machine = send(machines, b"new", &[]).unwrap();
        let machine_token = token(machine);
        assert!(send(machine, b"continueWithBudget:", &[one]).is_err());
        let method = compile("[1 + 2]");
        let mt = token(method);
        let initial = send(blocks, b"fromMethod:captureOperands:", &[method]).unwrap();
        let it = token(initial);
        assert_eq!(
            send(machine, b"runBlock:budget:", &[initial, zero]),
            Ok(machine)
        );
        assert_eq!(send(machine, b"executionState", &[]), Ok(sel(b"runnable")));
        assert_eq!(send(machine, b"isRunnable", &[]), Ok(truth));
        let mut answer = machine;
        let mut steps = 0;
        while answer == machine {
            answer = send(machine, b"continueWithBudget:", &[one]).unwrap();
            steps += 1;
            assert!(steps < 30);
            if answer != machine {
                keep(answer);
            }
            collect();
        }
        assert_eq!(bits(answer), 3);
        assert_eq!(send(machine, b"executionState", &[]), Ok(sel(b"completed")));
        assert_eq!(bits(send(machine, b"liveFrameCount", &[]).unwrap()), 0);
        assert!(send(machine, b"continueWithBudget:", &[one]).is_err());
        release(it);
        release(mt);
        release(machine_token);
        collect();
        println!("resumable arithmetic and runnable budgets: passed");
        // Real captured data enters resumable frames via current getters.
        let (outer, ot) = block("[:x | [:y | [x + y]]]");
        let inner = send(outer, b"value:", &[five]).unwrap();
        let inner_token = token(inner);
        collect();
        assert_eq!(bits(send(inner, b"value:", &[ten]).unwrap()), 15);
        release(inner_token);
        release(ot);
        collect();
        let actor_type = send(id.root, b"behaviorOf:", &[actor]).unwrap();
        send(ns, b"at:put:", &[sel(b"CompilerProbe"), actor]).unwrap();
        send(actor, b"resumeReset", &[]).unwrap();
        let count = |name: &[u8]| {
            abi::as_size(send(actor, b"resumeCount:", &[sel(name)]).unwrap()).unwrap()
        };
        let baseline = units();
        let old_child = compile("[CompilerProbe resumeMark: #entered]; [CompilerProbe later]; 11");
        let old_token = token(old_child);
        send(actor_type, b"addMethod", &[sel(b"child"), old_child]).unwrap();
        let constant = send(actor_type, b"lookup", &[sel(b"constantValue")]).unwrap();
        let old_later = send(constant, b"withContext:", &[five]).unwrap();
        send(actor_type, b"addMethod", &[sel(b"later"), old_later]).unwrap();
        let (parent, parent_token) = block(
            "[| [CompilerProbe resumeMark: #before]; let answer := [CompilerProbe child]; [CompilerProbe resumeMark: #after]; [answer + [CompilerProbe later]]]",
        );
        let replacement = compile("99");
        let replacement_token = token(replacement);
        let new_later = compile("[CompilerProbe resumeMark: #later]; 7");
        let later_token = token(new_later);
        let machine = send(machines, b"new", &[]).unwrap();
        let machine_token = token(machine);
        assert_eq!(
            send(machine, b"runBlock:budget:", &[parent, zero]),
            Ok(machine)
        );
        assert_eq!(count(b"before"), 0);
        assert_eq!(send(machine, b"continueWithBudget:", &[one]), Ok(machine));
        let mut drives = 0;
        while count(b"entered") == 0 {
            assert_eq!(send(machine, b"continueWithBudget:", &[one]), Ok(machine));
            drives += 1;
            assert!(drives < 40);
        }
        assert_eq!(count(b"before"), 1);
        assert_eq!(count(b"entered"), 1);
        assert_eq!(count(b"after"), 0);
        assert!(
            send(machine, b"runBlock:budget:", &[parent, one]).is_err(),
            "cannot restart a runnable Machine"
        );
        assert!(bits(send(machine, b"liveFrameCount", &[]).unwrap()) >= 2);
        send(actor_type, b"addMethod", &[sel(b"child"), replacement]).unwrap();
        send(actor_type, b"addMethod", &[sel(b"later"), new_later]).unwrap();
        release(old_token);
        release(parent_token);
        release(replacement_token);
        release(later_token);
        collect();
        assert_eq!(
            units(),
            baseline + 4,
            "paused frames retain old child and parent Units"
        );
        let mut answer = machine;
        while answer == machine {
            answer = send(machine, b"continueWithBudget:", &[one]).unwrap();
            drives += 1;
            assert!(drives < 100);
            if answer == machine {
                collect();
            }
        }
        assert_eq!(
            bits(answer),
            18,
            "entered child returns 11; later sends observe replacement 7"
        );
        assert_eq!(count(b"before"), 1);
        assert_eq!(count(b"entered"), 1);
        assert_eq!(count(b"after"), 1);
        assert_eq!(count(b"later"), 2);
        collect();
        assert_eq!(
            units(),
            baseline + 2,
            "completed frames release their old code"
        );
        send(actor_type, b"removeMethod:", &[sel(b"child")]).unwrap();
        send(actor_type, b"removeMethod:", &[sel(b"later")]).unwrap();
        release(machine_token);
        collect();
        assert_eq!(units(), baseline);
        // Abandoning a paused computation releases its frame pins and Unit too.
        let (abandoned, abandoned_token) = block("[| [CompilerProbe resumeMark: #before]; 5]");
        let machine = send(machines, b"new", &[]).unwrap();
        let mt = token(machine);
        assert_eq!(
            send(machine, b"runBlock:budget:", &[abandoned, one]),
            Ok(machine)
        );
        release(abandoned_token);
        collect();
        assert_eq!(units(), baseline + 1);
        send(machine, b"destroy", &[]).unwrap();
        assert!(send(machine, b"continueWithBudget:", &[one]).is_err());
        release(mt);
        collect();
        assert_eq!(units(), baseline);
        // Replaced Block value is selected normally; it must not be classified using
        // cached behavior alone and bypassed with the old descriptor.
        let (target, target_token) = block("[| 123]");
        let bt = send(ns, b"at:", &[sel(b"BlockVt")]).unwrap();
        let original = keep(send(bt, b"lookup", &[sel(b"value")]).unwrap());
        let changed = send(constant, b"withContext:", &[five]).unwrap();
        send(bt, b"addMethod", &[sel(b"value"), changed]).unwrap();
        let machine = send(machines, b"new", &[]).unwrap();
        let mt = token(machine);
        assert_eq!(send(machine, b"runBlock:", &[target]), Ok(five));
        send(bt, b"addMethod", &[sel(b"value"), original]).unwrap();
        release(target_token);
        release(mt);
        collect();
        // A source send to a native Method must not look up invokeOn: on that
        // Method after selecting it. Direct Id and Machine sends must agree.
        let native = send(actor_type, b"lookup", &[sel(b"numberIdentity")]).unwrap();
        send(actor_type, b"addMethod", &[sel(b"dependency:"), native]).unwrap();
        let native_type = send(id.root, b"behaviorOf:", &[native]).unwrap();
        let private = keep(send(native_type, b"delegated", &[]).unwrap());
        let meta = send(id.root, b"behaviorOf:", &[private]).unwrap();
        let private_meta = keep(send(meta, b"delegated", &[]).unwrap());
        let reject = send(actor_type, b"lookup", &[sel(b"rethrow:")]).unwrap();
        send(private_meta, b"addMethod", &[sel(b"lookup"), reject]).unwrap();
        send(
            id.root,
            b"reassociateBehaviorOf:with:",
            &[private, private_meta],
        )
        .unwrap();
        send(id.root, b"reassociateBehaviorOf:with:", &[native, private]).unwrap();
        assert_eq!(send(actor, b"dependency:", &[five]), Ok(five));
        let (caller, caller_token) = block("[| [CompilerProbe dependency: 5]]");
        let machine = send(machines, b"new", &[]).unwrap();
        let mt = token(machine);
        let result = send(machine, b"runBlock:", &[caller]).unwrap();
        assert_eq!(
            bits(result),
            5,
            "native fallback must not perform another lookup on its selected Method"
        );
        send(
            id.root,
            b"reassociateBehaviorOf:with:",
            &[native, native_type],
        )
        .unwrap();
        release(caller_token);
        release(mt);
        collect();
        // Custom lookup is executed once, including collection inside it. The
        // default-lookup probe must not replay that callback on the fallback.
        let base = send(id.root, b"objectBehavior", &[]).unwrap();
        let probe_type = keep(send(base, b"delegated", &[]).unwrap());
        let probe = keep(send(probe_type, b"allocate", &[abi::size(0).unwrap()]).unwrap());
        send(probe_type, b"addMethod", &[sel(b"dependency:"), native]).unwrap();
        let dnu = send(actor_type, b"lookup", &[sel(b"doesNotUnderstand:")]).unwrap();
        send(probe_type, b"addMethod", &[sel(b"doesNotUnderstand:"), dnu]).unwrap();
        send(ns, b"at:put:", &[sel(b"LookupProbe"), probe]).unwrap();
        let (caller, caller_token) = block("[| [LookupProbe dependency: 5]]");
        let (missing, missing_token) = block("[| [LookupProbe missingUnregistered]]");
        let meta = send(id.root, b"behaviorOf:", &[probe_type]).unwrap();
        let private = keep(send(meta, b"delegated", &[]).unwrap());
        let custom = send(actor_type, b"lookup", &[sel(b"lookupOverride")]).unwrap();
        send(private, b"addMethod", &[sel(b"lookup"), custom]).unwrap();
        send(
            id.root,
            b"reassociateBehaviorOf:with:",
            &[probe_type, private],
        )
        .unwrap();
        send(
            actor,
            b"at:put:",
            &[abi::size(8).unwrap(), abi::size(0).unwrap()],
        )
        .unwrap();
        let machine = send(machines, b"new", &[]).unwrap();
        let mt = token(machine);
        let value = send(machine, b"runBlock:", &[caller]).unwrap();
        assert_eq!(bits(value), 5);
        assert_eq!(
            send(actor, b"at:", &[abi::size(8).unwrap()]),
            Ok(abi::size(1).unwrap())
        );
        release(mt);
        send(
            actor,
            b"at:put:",
            &[abi::size(8).unwrap(), abi::size(0).unwrap()],
        )
        .unwrap();
        let machine = send(machines, b"new", &[]).unwrap();
        let mt = token(machine);
        assert_eq!(
            send(machine, b"runBlock:", &[missing]),
            Ok(abi::size(1).unwrap())
        );
        assert_eq!(
            send(actor, b"at:", &[abi::size(8).unwrap()]),
            Ok(abi::size(2).unwrap()),
            "one lookup for the missing selector and one for DNU"
        );
        release(mt);
        release(caller_token);
        release(missing_token);
        collect();
        // Capture aliases preserve resumability and entered code without any
        // lookup on the selected Method. Poison every such lookup explicitly.
        let baseline = units();
        let method = compile("23");
        send(actor_type, b"addMethod", &[sel(b"resumableBound"), method]).unwrap();
        let captured = send(actor, b"methodFor:", &[sel(b"resumableBound")]).unwrap();
        let captured_token = token(captured);
        assert_ne!(method, captured);
        let method_type = send(id.root, b"behaviorOf:", &[method]).unwrap();
        let private = keep(send(method_type, b"delegated", &[]).unwrap());
        let meta = send(id.root, b"behaviorOf:", &[private]).unwrap();
        let private_meta = keep(send(meta, b"delegated", &[]).unwrap());
        let reject = send(actor_type, b"lookup", &[sel(b"rethrow:")]).unwrap();
        send(private_meta, b"addMethod", &[sel(b"lookup"), reject]).unwrap();
        send(
            id.root,
            b"reassociateBehaviorOf:with:",
            &[private, private_meta],
        )
        .unwrap();
        for selected in [method, captured] {
            send(
                id.root,
                b"reassociateBehaviorOf:with:",
                &[selected, private],
            )
            .unwrap();
        }
        assert_eq!(bits(send(actor, b"resumableBound", &[]).unwrap()), 23);
        send(actor_type, b"removeMethod:", &[sel(b"resumableBound")]).unwrap();
        let block = send(blocks, b"fromMethod:captureOperands:", &[captured]).unwrap();
        let bt = token(block);
        release(captured_token);
        let machine = send(machines, b"new", &[]).unwrap();
        let mt = token(machine);
        assert_eq!(
            send(machine, b"runBlock:budget:", &[block, one]),
            Ok(machine)
        );
        release(bt);
        collect();
        assert_eq!(units(), baseline + 1);
        let mut answer = machine;
        for _ in 0..20 {
            if answer != machine {
                break;
            }
            answer = send(machine, b"continueWithBudget:", &[one]).unwrap();
        }
        assert_ne!(answer, machine);
        assert_eq!(bits(answer), 23);
        release(mt);
        collect();
        assert_eq!(units(), baseline);
        let execution = send(ns, b"at:", &[sel(b"Execution")]).unwrap();
        let harmless = send(actor_type, b"lookup", &[sel(b"numberIdentity")]).unwrap();
        assert!(
            send(
                execution,
                b"fromStep:descriptor:selection:owner:",
                &[
                    harmless,
                    text(b"bad descriptor"),
                    text(b"bad selection"),
                    actor
                ]
            )
            .is_err()
        );
        collect();
        assert_eq!(units(), baseline);
        // Top-level publication preserves its early-return contract: a changed
        // setter result stops subsequent source effects rather than replaying them.
        let old_globals = send(ns, b"at:", &[sel(b"Globals")]).unwrap();
        let base = send(id.root, b"objectBehavior", &[]).unwrap();
        let gt = keep(send(base, b"delegated", &[]).unwrap());
        let globals = keep(send(gt, b"allocate", &[abi::size(0).unwrap()]).unwrap());
        let setter = send(constant, b"withContext:", &[five]).unwrap();
        send(gt, b"addMethod", &[sel(b"at:put:"), setter]).unwrap();
        send(ns, b"at:put:", &[sel(b"Globals"), globals]).unwrap();
        let method = compile("let stopped := 1; [CompilerProbe resumeMark: #after]; 99");
        let mt = token(method);
        let root = send(blocks, b"fromMethod:captureOperands:", &[method]).unwrap();
        let rt = token(root);
        let machine = send(machines, b"new", &[]).unwrap();
        let token_machine = token(machine);
        let before = count(b"after");
        assert_eq!(send(machine, b"runBlock:", &[root]), Ok(five));
        assert_eq!(count(b"after"), before);
        send(ns, b"at:put:", &[sel(b"Globals"), old_globals]).unwrap();
        release(mt);
        release(rt);
        release(token_machine);
        collect();
        assert_eq!(units(), baseline);
        // The same proper tail call must stay bounded under a forwarding custom
        // lookup. Native callbacks are not permission to retain source tail frames.
        let base = send(id.root, b"objectBehavior", &[]).unwrap();
        let probe_type = keep(send(base, b"delegated", &[]).unwrap());
        let probe = keep(send(probe_type, b"allocate", &[abi::size(0).unwrap()]).unwrap());
        send(ns, b"at:put:", &[sel(b"TailProbe"), probe]).unwrap();
        let method = keep(compile("[TailProbe tailTick]; [TailProbe tailRecurse]"));
        let root = keep(send(blocks, b"fromMethod:captureOperands:", &[method]).unwrap());
        let stop = keep(send(constant, b"withContext:", &[five]).unwrap());
        let tick = send(actor_type, b"lookup", &[sel(b"tailTick")]).unwrap();
        let original_meta = send(id.root, b"behaviorOf:", &[probe_type]).unwrap();
        let custom_meta = keep(send(original_meta, b"delegated", &[]).unwrap());
        let lookup_override = send(actor_type, b"lookup", &[sel(b"lookupOverride")]).unwrap();
        send(
            custom_meta,
            b"addMethod",
            &[sel(b"lookup"), lookup_override],
        )
        .unwrap();
        let mut observations = Vec::new();
        for custom in [false, true] {
            send(
                id.root,
                b"reassociateBehaviorOf:with:",
                &[probe_type, if custom { custom_meta } else { original_meta }],
            )
            .unwrap();
            for iterations in [8usize, 24] {
                let machine = keep(send(machines, b"new", &[]).unwrap());
                let context =
                    send(id.root, b"sequence", &[probe_type, stop, machine, integer]).unwrap();
                let bound_tick = send(tick, b"withContext:", &[context]).unwrap();
                send(probe_type, b"addMethod", &[sel(b"tailTick"), bound_tick]).unwrap();
                send(probe_type, b"addMethod", &[sel(b"tailRecurse"), method]).unwrap();
                send(actor, b"tailSetup:", &[abi::size(iterations).unwrap()]).unwrap();
                assert_eq!(send(machine, b"runBlock:", &[root]), Ok(five));
                let peak = abi::as_size(send(actor, b"tailPeak", &[]).unwrap()).unwrap();
                assert_eq!(
                    send(actor, b"tailCalls", &[]),
                    Ok(abi::size(iterations).unwrap())
                );
                println!("TAIL_CONTRACT custom={custom} calls={iterations} peak_frames={peak}");
                observations.push((custom, iterations, peak));
            }
        }
        for (custom, _, peak) in observations {
            assert_eq!(
                peak, 2,
                "tail calls must reuse the source frame with custom lookup={custom}"
            );
        }
        // Actual compiled source runs inside a lookup continuation, then the
        // selected target parks. Selection and effects must survive replacement.
        send(id.root,b"reassociateBehaviorOf:with:",&[probe_type,original_meta]).unwrap();
        let target_dir=std::env::var_os("BIRNAM_OOP_TARGET").map(std::path::PathBuf::from)
            .unwrap_or_else(||std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.cargo-target/oop-slice"));
        let path=target_dir.join("fixtures/continuation.dylib");
        let path_bytes:Vec<_>=path.as_os_str().as_encoded_bytes().iter().map(|&b|abi::size(b as usize).unwrap()).collect();
        let path=send(id.root,b"nameWithBytes",&path_bytes).unwrap();
        let empty=send(id.root,b"sequence",&[]).unwrap();
        let fixture=keep(id.send(path,id.load,&[empty]).unwrap());
        let template=|name:&[u8]|send(fixture,b"methodAt:",&[sel(name)]).unwrap();
        let sequence=|values:&[Oop]|keep(send(id.root,b"sequence",values).unwrap());
        let field=|object,index|send(object,b"at:",&[abi::size(index).unwrap()]).unwrap();
        let block_vt=send(id.root,b"behaviorOf:",&[blocks]).unwrap();
        let original_factory=keep(send(block_vt,b"lookup",&[sel(b"fromMethod:captureOperands:")]).unwrap());
        let captured=sequence(&[std::ptr::null_mut()]);
        let capture_ctx=sequence(&[original_factory,captured]);
        let capture_factory=keep(send(template(b"captureFactory"),b"withContext:",&[capture_ctx]).unwrap());
        send(block_vt,b"addMethod",&[sel(b"fromMethod:captureOperands:"),capture_factory]).unwrap();
        let maker=keep(compile("[:x | [CompilerProbe resumeMark: #entered]; x]"));
        let maker_block=keep(send(blocks,b"fromMethod:captureOperands:",&[maker]).unwrap());
        let helper_block=keep(send(maker_block,b"value",&[]).unwrap());
        let helper=keep(field(captured,0));
        let target_maker=keep(compile("[:x | 42]"));
        let target_maker_block=keep(send(blocks,b"fromMethod:captureOperands:",&[target_maker]).unwrap());
        let target_block=keep(send(target_maker_block,b"value",&[]).unwrap());
        let target=keep(field(captured,0));
        send(block_vt,b"addMethod",&[sel(b"fromMethod:captureOperands:"),original_factory]).unwrap();
        // The source entry is capture-free; install it on the same receiver as
        // the pending target, with the same argument count as the helper call.
        send(probe_type,b"addMethod",&[sel(b"helper"),helper]).unwrap();
        send(probe_type,b"addMethod",&[sel(b"later:"),target]).unwrap();
        let parent=keep(compile("[TailProbe later: 8]"));
        let parent_block=keep(send(blocks,b"fromMethod:captureOperands:",&[parent]).unwrap());
        let stats=sequence(&[abi::size(0).unwrap(),abi::size(0).unwrap(),std::ptr::null_mut(),abi::size(0).unwrap()]);
        let original_lookup=keep(send(original_meta,b"lookup",&[sel(b"lookup")]).unwrap());
        let local_ctx=sequence(&[original_lookup,gc,sel(b"later:"),stats,template(b"selected"),probe,sel(b"helper"),template(b"resumeLookup")]);
        let lookup=keep(send(template(b"localLookup"),b"withContext:",&[local_ctx]).unwrap());
        let meta=keep(send(original_meta,b"delegated",&[]).unwrap());
        send(meta,b"addMethod",&[sel(b"lookup"),lookup]).unwrap();
        send(id.root,b"reassociateBehaviorOf:with:",&[probe_type,meta]).unwrap();
        send(actor,b"resumeReset",&[]).unwrap();
        let suspended=keep(send(machines,b"new",&[]).unwrap());
        let wide=int(1000);
        assert_eq!(send(suspended,b"runBlock:budget:",&[parent_block,wide]),Ok(suspended));
        assert_eq!(field(stats,0),abi::size(1).unwrap());
        assert_eq!(field(stats,1),abi::size(1).unwrap());
        assert_eq!(count(b"entered"),1);
        send(probe_type,b"addMethod",&[sel(b"later:"),stop]).unwrap();
        collect();
        let completed=send(suspended,b"continueWithBudget:",&[wide]).unwrap();
        assert_eq!(bits(completed),42);
        assert_eq!(count(b"entered"),1);
        assert_eq!(field(stats,0),abi::size(1).unwrap());
        send(id.root,b"reassociateBehaviorOf:with:",&[probe_type,original_meta]).unwrap();
        let _=(helper_block,target_block);
        println!("COMPILED_LOOKUP_SUSPENSION_REENTRY_REPLACEMENT_OK");

        // Two compiled computations alternate owner budgets; completed results
        // and both pending computations survive collection between turns.
        send(actor,b"resumeReset",&[]).unwrap();
        let mut running=Vec::new();
        for (expected,marker) in [(13usize,"before"),(35,"after")] {
            let code=keep(compile(&format!("{} {expected}",format!("[CompilerProbe resumeMark: #{marker}]; ").repeat(64))));
            let block=keep(send(blocks,b"fromMethod:captureOperands:",&[code]).unwrap());
            let m=keep(send(machines,b"new",&[]).unwrap());
            assert_eq!(send(m,b"runBlock:budget:",&[block,zero]),Ok(m));
            running.push((m,expected,false));
        }
        let mut turns=0;
        while running.iter().any(|(_,_,done)|!*done){
            for (m,expected,done) in &mut running {
                if *done {continue;}
                let result=send(*m,b"continueWithBudget:",&[one]).unwrap();
                if result!=*m {keep(result);assert_eq!(bits(result),*expected);*done=true;}
                collect();turns+=1;assert!(turns<2000);
            }
        }
        assert_eq!(count(b"before"),64);assert_eq!(count(b"after"),64);
        println!("COMPILED_INTERLEAVING_GC_OK turns={turns}");

        let auto_code=keep(compile(&format!("{} 42", "[CompilerProbe resumeMark: #before]; ".repeat(128))));
        let auto_block=keep(send(blocks,b"fromMethod:captureOperands:",&[auto_code]).unwrap());
        let auto_machine=keep(send(machines,b"new",&[]).unwrap());
        collect();let heap_before=abi::as_size(send(id.root,b"allocationCount",&[]).unwrap()).unwrap();
        send(gc,b"budget:",&[abi::size(2048).unwrap()]).unwrap();
        send(gc,b"automaticCollection:",&[abi::size(1).unwrap()]).unwrap();
        let auto_result=keep(send(auto_machine,b"runBlock:",&[auto_block]).unwrap());
        let heap_after=abi::as_size(send(id.root,b"allocationCount",&[]).unwrap()).unwrap();
        assert_eq!(bits(auto_result),42);assert!(heap_after<heap_before+512,"automatic collection failed to bound transfer allocations: {heap_before} -> {heap_after}");
        send(gc,b"automaticCollection:",&[abi::size(0).unwrap()]).unwrap();
        send(gc,b"budget:",&[abi::size(16<<20).unwrap()]).unwrap();
        println!("COMPILED_AUTOMATIC_GC_OK before={heap_before} after={heap_after}");

        // A real native callback re-enters compiled source on the owned
        // Machine; its running drive cannot be recursively resumed.
        let native_path=target_dir.join("fixtures/continuation-native.dylib");
        let native_bytes:Vec<_>=native_path.as_os_str().as_encoded_bytes().iter().map(|&b|abi::size(b as usize).unwrap()).collect();
        let native_path=send(id.root,b"nameWithBytes",&native_bytes).unwrap();
        let empty=send(id.root,b"sequence",&[]).unwrap();let native=keep(id.send(native_path,id.load,&[empty]).unwrap());
        let callback_machine=keep(send(machines,b"new",&[]).unwrap());
        let inspect=send(native,b"methodAt:",&[sel(b"inspectFrames")]).unwrap();
        let inspect=keep(send(inspect,b"withContext:",&[callback_machine]).unwrap());
        send(probe_type,b"addMethod",&[sel(b"inspectFrames"),inspect]).unwrap();
        let child=keep(compile("[CompilerProbe resumeMark: #entered]; [TailProbe inspectFrames]"));
        let child_block=keep(send(blocks,b"fromMethod:captureOperands:",&[child]).unwrap());
        let callback_context=sequence(&[callback_machine,child_block,one,gc]);
        let callback=send(native,b"methodAt:",&[sel(b"reenter")]).unwrap();
        let callback=keep(send(callback,b"withContext:",&[callback_context]).unwrap());
        send(probe_type,b"addMethod",&[sel(b"reenter"),callback]).unwrap();
        let parent=keep(compile("[[TailProbe reenter] + 5]"));
        let parent_block=keep(send(blocks,b"fromMethod:captureOperands:",&[parent]).unwrap());
        send(actor,b"resumeReset",&[]).unwrap();
        let mut callback_result=send(callback_machine,b"runBlock:budget:",&[parent_block,one]).unwrap();
        let mut callback_turns=0;
        while callback_result==callback_machine {
            collect();callback_result=send(callback_machine,b"continueWithBudget:",&[one]).unwrap();callback_turns+=1;assert!(callback_turns<100);
        }
        assert_eq!(bits(callback_result),8);
        assert_eq!(count(b"entered"),1);
        println!("COMPILED_NATIVE_CALLBACK_SAME_MACHINE_OK");

        let other_machine=keep(send(machines,b"new",&[]).unwrap());
        let other_body=keep(compile("21"));let other_block=keep(send(blocks,b"fromMethod:captureOperands:",&[other_body]).unwrap());
        let other_context=sequence(&[other_machine,other_block]);
        let run_other=send(native,b"methodAt:",&[sel(b"runOther")]).unwrap();
        let run_other=keep(send(run_other,b"withContext:",&[other_context]).unwrap());
        send(probe_type,b"addMethod",&[sel(b"runOther"),run_other]).unwrap();
        let outer_machine=keep(send(machines,b"new",&[]).unwrap());
        let outer=keep(compile("[[TailProbe runOther] + 5]"));let outer_block=keep(send(blocks,b"fromMethod:captureOperands:",&[outer]).unwrap());
        assert_eq!(bits(send(outer_machine,b"runBlock:",&[outer_block]).unwrap()),26);
        println!("COMPILED_NESTED_MACHINE_NATIVE_BOUNDARY_OK");

        // Keep the existing native-error behavior for resumable bodies too.
        let error_body=keep(compile("[CompilerProbe rethrow: [1 / 0]]"));
        let error_block=keep(send(blocks,b"fromMethod:captureOperands:",&[error_body]).unwrap());
        assert_eq!(send(send(error_block,b"value",&[]).unwrap(),b"isError",&[]),Ok(truth));
        let exec=send(ns,b"at:",&[sel(b"Execution")]).unwrap();
        assert!(send(exec,b"evaluatePreservingFailures:",&[error_block]).is_err());
        let invalid_lookup=keep(send(constant,b"withContext:",&[five]).unwrap());
        send(meta,b"addMethod",&[sel(b"lookup"),invalid_lookup]).unwrap();
        send(id.root,b"reassociateBehaviorOf:with:",&[probe_type,meta]).unwrap();
        let bad_lookup_body=keep(compile("[TailProbe later: 8]"));
        let bad_lookup_block=keep(send(blocks,b"fromMethod:captureOperands:",&[bad_lookup_body]).unwrap());
        let bad_lookup_result=keep(send(bad_lookup_block,b"value",&[]).unwrap());
        assert_eq!(send(bad_lookup_result,b"isError",&[]),Ok(truth));
        send(id.root,b"reassociateBehaviorOf:with:",&[probe_type,original_meta]).unwrap();
        collect();
        println!("RESUMABLE_ERROR_CONVENTION_OK");

        println!("RESUMABLE_EXECUTION_OOP_OK");
    }
}
