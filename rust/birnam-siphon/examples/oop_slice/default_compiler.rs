//! Standard compiler selectors, live evaluation, and owned source suspension.
use birnam_abi::{self as abi, Oop, id_protocol::Protocol};
pub(super) unsafe fn check(
    id: &Protocol,
    send: &impl Fn(Oop, &[u8], &[Oop]) -> Result<Oop, Oop>,
    sel: &impl Fn(&[u8]) -> Oop,
    objects: [Oop; 7],
) {
    let [ns, roots, gc, string, integer, blocks, actor] = objects;
    let retain = |v| send(roots, b"retain:", &[v]).unwrap();
    let release = |v| {
        send(roots, b"release:", &[v]).unwrap();
    };
    let text = |bytes: &[u8]| {
        let values: Vec<_> = bytes
            .iter()
            .map(|b| abi::size(*b as usize).unwrap())
            .collect();
        let values = send(id.root, b"sequence", &values).unwrap();
        send(string, b"fromBytes:", &[values]).unwrap()
    };
    let named = |name: &[u8]| send(ns, b"at:", &[sel(name)]).unwrap();
    let codegen = named(b"Codegen");
    let tcc = named(b"TCC");
    let truth = named(b"true");
    let machines = named(b"Machine");
    let units = || abi::as_size(send(tcc, b"liveNativeUnits", &[]).unwrap()).unwrap();
    let collect = || {
        send(gc, b"collectPrecisely", &[]).unwrap();
        send(tcc, b"checkpoint", &[]).unwrap();
    };
    let bits = |v| {
        let w = send(integer, b"wordOf:", &[v]).unwrap();
        abi::as_size(w).unwrap_or_else(|| abi::as_size(send(w, b"low32", &[]).unwrap()).unwrap())
    };
    let count =
        |name: &[u8]| abi::as_size(send(actor, b"resumeCount:", &[sel(name)]).unwrap()).unwrap();
    let old_globals = named(b"Globals");
    let gt = retain(old_globals);
    send(ns, b"at:put:", &[sel(b"Globals"), ns]).unwrap();
    collect();
    let baseline = units();
    send(actor, b"resumeReset", &[]).unwrap();
    let source = text(b"[CompilerProbe resumeMark: #before]; 23");
    let st = retain(source);
    let compiled = send(codegen, b"compile:", &[source]).unwrap();
    let mt = retain(compiled);
    assert_ne!(send(compiled, b"isError", &[]), Ok(truth));
    assert_eq!(count(b"before"), 0);
    let block = send(blocks, b"fromMethod:captureOperands:", &[compiled]).unwrap();
    let bt = retain(block);
    assert_eq!(bits(send(block, b"value", &[]).unwrap()), 23);
    assert_eq!(count(b"before"), 1);
    release(bt);
    release(st);

    let bad = text(b"[|");
    let bad_token = retain(bad);
    for name in [b"compile:".as_slice(), b"emit:", b"eval:"] {
        let error = send(codegen, name, &[bad]).unwrap();
        assert_eq!(send(error, b"isError", &[]), Ok(truth));
    }
    assert_eq!(count(b"before"), 1);
    // Eval performs the current compile: send and preserves its answer/failure.
    let cv = send(id.root, b"behaviorOf:", &[codegen]).unwrap();
    let original = send(cv, b"lookup", &[sel(b"compile:")]).unwrap();
    let ot = retain(original);
    let av = send(id.root, b"behaviorOf:", &[actor]).unwrap();
    let constant = send(av, b"lookup", &[sel(b"constantValue")]).unwrap();
    let replacement = send(constant, b"withContext:", &[compiled]).unwrap();
    send(cv, b"addMethod", &[sel(b"compile:"), replacement]).unwrap();
    collect();
    assert_eq!(bits(send(codegen, b"eval:", &[bad]).unwrap()), 23);
    assert_eq!(count(b"before"), 2);
    let error = send(codegen, b"compileResumable:", &[bad]).unwrap();
    let et = retain(error);
    let replacement = send(constant, b"withContext:", &[error]).unwrap();
    send(cv, b"addMethod", &[sel(b"compile:"), replacement]).unwrap();
    assert_eq!(send(codegen, b"eval:", &[bad]), Ok(error));
    let rethrow = send(av, b"lookup", &[sel(b"rethrow:")]).unwrap();
    send(cv, b"addMethod", &[sel(b"compile:"), rethrow]).unwrap();
    assert_eq!(send(codegen, b"eval:", &[bad]), Err(bad));
    send(cv, b"addMethod", &[sel(b"compile:"), original]).unwrap();
    release(ot);
    release(et);
    release(bad_token);
    release(mt);
    collect();
    assert_eq!(units(), baseline);

    let source = text(b"41");
    let st = retain(source);
    let emitted = send(codegen, b"emit:", &[source]).unwrap();
    let emitted_token = retain(emitted);
    assert_eq!(units(), baseline, "emit must not map executable code");
    let unit = send(tcc, b"compile:", &[emitted]).unwrap();
    let ut = retain(unit);
    let method = send(unit, b"initialize", &[]).unwrap();
    let mt = retain(method);
    let block = send(blocks, b"fromMethod:captureOperands:", &[method]).unwrap();
    let bt = retain(block);
    assert_eq!(bits(send(block, b"value", &[]).unwrap()), 41);
    release(bt);
    release(mt);
    release(ut);
    release(emitted_token);
    release(st);
    collect();
    assert_eq!(units(), baseline);
    println!("DEFAULT_COMPILER_ENTRYPOINTS_OK");

    let zero = send(integer, b"fromWord:", &[abi::size(0).unwrap()]).unwrap();
    let zt = retain(zero);
    let one = send(integer, b"fromWord:", &[abi::size(1).unwrap()]).unwrap();
    let it = retain(one);
    for bounded in [false, true] {
        send(actor, b"resumeReset", &[]).unwrap();
        collect();
        let baseline = units();
        let machine = send(machines, b"new", &[]).unwrap();
        let mt = retain(machine);
        send(ns, b"at:put:", &[sel(b"ControlMachine"), machine]).unwrap();
        let source=text(b"[CompilerProbe resumeMark: #before]; [ControlMachine park]; [CompilerProbe resumeMark: #after]; 41");
        send(ns, b"at:put:", &[sel(b"DefaultEvalBody"), source]).unwrap();
        let outer = text(b"[Codegen eval: DefaultEvalBody]");
        let ot = retain(outer);
        let method = send(codegen, b"compile:", &[outer]).unwrap();
        let ct = retain(method);
        release(ot);
        let block = send(blocks, b"fromMethod:captureOperands:", &[method]).unwrap();
        let bt = retain(block);
        assert_eq!(send(machine, b"runBlock:", &[block]), Ok(machine));
        assert_eq!(send(machine, b"executionState", &[]), Ok(sel(b"suspended")));
        assert_eq!(count(b"before"), 1);
        assert_eq!(count(b"after"), 0);
        release(bt);
        release(ct);
        let changed = text(b"99");
        send(ns, b"at:put:", &[sel(b"DefaultEvalBody"), changed]).unwrap();
        collect();
        // runBlock retains its input Block until return; both code owners are live.
        assert_eq!(
            units(),
            baseline + 2,
            "caller and entered eval code remain mapped"
        );
        let mut result = if bounded {
            send(machine, b"resumeWith:budget:", &[zero, zero])
        } else {
            send(machine, b"resumeWith:", &[zero])
        }
        .unwrap();
        let mut turns = 0;
        while result == machine {
            collect();
            result = send(machine, b"continueWithBudget:", &[one]).unwrap();
            turns += 1;
            assert!(turns < 100);
        }
        assert_eq!(bits(result), 41);
        assert_eq!(count(b"before"), 1);
        assert_eq!(count(b"after"), 1);
        assert_eq!(send(machine, b"executionState", &[]), Ok(sel(b"completed")));
        assert_eq!(bits(send(codegen, b"eval:", &[changed]).unwrap()), 99);
        send(
            ns,
            b"at:put:",
            &[sel(b"ControlMachine"), std::ptr::null_mut()],
        )
        .unwrap();
        release(mt);
        collect();
        assert_eq!(units(), baseline);
        println!("DEFAULT_EVAL_SUSPEND_GC_OK bounded={bounded}");
    }
    send(
        ns,
        b"at:put:",
        &[sel(b"DefaultEvalBody"), std::ptr::null_mut()],
    )
    .unwrap();
    send(ns, b"at:put:", &[sel(b"Globals"), old_globals]).unwrap();
    release(gt);
    release(zt);
    release(it);
}
