//! Complete synchronous source -> PEG -> lowering -> emitted C -> TCC -> Execution.
use birnam_abi::{self as abi, Oop, id_protocol::Protocol};
pub(super) unsafe fn check(
    id: &Protocol,
    send: &impl Fn(Oop, &[u8], &[Oop]) -> Result<Oop, Oop>,
    sel: &impl Fn(&[u8]) -> Oop,
    paths: [Oop; 3],
    objects: [Oop; 7],
) {
    unsafe {
        let [ns, roots, gc, string, base, actor, integer] = objects;
        // Earlier enumeration tests deliberately removed the initial Namespace export.
        send(ns, b"at:put:", &[sel(b"Namespace"), ns]).unwrap();
        let keep = |v| {
            send(roots, b"retain:", &[v]).unwrap();
            v
        };
        let text = |bytes: &[u8]| {
            let values: Vec<_> = bytes
                .iter()
                .map(|&b| abi::size(b as usize).unwrap())
                .collect();
            let source = send(id.root, b"sequence", &values).unwrap();
            keep(send(string, b"fromBytes:", &[source]).unwrap())
        };
        let actor_type = send(id.root, b"behaviorOf:", &[actor]).unwrap();
        let constant = send(actor_type, b"lookup", &[sel(b"constantValue")]).unwrap();
        let truth = send(ns, b"at:", &[sel(b"true")]).unwrap();
        let falsity = send(ns, b"at:", &[sel(b"false")]).unwrap();
        let error_type = send(ns, b"at:", &[sel(b"ErrorVt")]).unwrap();
        for (behavior, value) in [(base, falsity), (error_type, truth)] {
            let method = send(constant, b"withContext:", &[value]).unwrap();
            send(behavior, b"addMethod", &[sel(b"isError"), method]).unwrap();
        }
        let error_kind = send(actor_type, b"lookup", &[sel(b"errorKind")]).unwrap();
        send(error_type, b"addMethod", &[sel(b"kind:"), error_kind]).unwrap();
        for path in paths {
            let args = send(id.root, b"sequence", &[ns]).unwrap();
            id.send(path, id.load, &[args])
                .expect("compiler owner loaded");
        }
        let codegen = send(ns, b"at:", &[sel(b"Codegen")]).unwrap();
        let blocks = send(ns, b"at:", &[sel(b"Block")]).unwrap();
        let tcc = send(ns, b"at:", &[sel(b"TCC")]).unwrap();
        let mut result_tokens = Vec::new();
        let mut evaluate = |source: &str| {
            let method = send(codegen, b"compileSynchronous:", &[text(source.as_bytes())])
                .expect("source compiled");
            assert_ne!(
                send(method, b"isError", &[]).unwrap(),
                truth,
                "compileError for {source}"
            );
            let token = send(roots, b"retain:", &[method]).unwrap();
            let block = send(blocks, b"fromMethod:captureOperands:", &[method]).unwrap();
            let block_token = send(roots, b"retain:", &[block]).unwrap();
            let result = send(block, b"value", &[]).expect("compiled root executed");
            result_tokens.push(send(roots, b"retain:", &[result]).unwrap());
            send(roots, b"release:", &[block_token]).unwrap();
            send(roots, b"release:", &[token]).unwrap();
            result
        };
        let int = |n: usize| keep(send(integer, b"fromWord:", &[abi::size(n).unwrap()]).unwrap());
        let bits = |v| {
            let v = send(integer, b"wordOf:", &[v]).unwrap();
            if let Some(n) = abi::as_size(v) {
                return n as u64;
            }
            let low = abi::as_size(send(v, b"low32", &[]).unwrap()).unwrap() as u64;
            let high = abi::as_size(send(v, b"high32", &[]).unwrap()).unwrap() as u64;
            low | (high << 32)
        };
        let five = int(5);
        let six = int(6);
        assert_eq!(bits(evaluate("[1 + 2]")), 3);
        let old_globals = send(ns, b"at:", &[sel(b"Globals")]).unwrap();
        send(ns, b"at:put:", &[sel(b"Globals"), ns]).unwrap();
        let published = evaluate("let milestoneValue := 7; milestoneValue");
        assert_eq!(bits(published), 7);
        assert_eq!(send(ns, b"at:", &[sel(b"milestoneValue")]), Ok(published));
        send(ns, b"at:put:", &[sel(b"Globals"), old_globals]).unwrap();
        assert_eq!(bits(evaluate("-9223372036854775808")), i64::MIN as u64);
        assert_eq!(bits(evaluate("9223372036854775807")), i64::MAX as u64);
        let ordinary_error = evaluate("[1 / 0]");
        assert_eq!(send(ordinary_error, b"isError", &[]), Ok(truth));

        let increment = evaluate("[:x | [x + 1]]");
        assert_eq!(bits(send(increment, b"value:", &[five]).unwrap()), 6);
        let collecting = evaluate("[:x | [GC collectPrecisely]; x]");
        let unrooted = send(base, b"allocate", &[abi::size(0).unwrap()]).unwrap();
        assert_eq!(send(collecting, b"value:", &[unrooted]), Ok(unrooted));
        let outer = evaluate("[:x | [:y | [x + y]]]");
        let captured = keep(send(outer, b"value:", &[five]).unwrap());
        send(gc, b"collectPrecisely", &[]).unwrap();
        assert_eq!(bits(send(captured, b"value:", &[six]).unwrap()), 11);
        let clone = keep(send(captured, b"clone", &[]).unwrap());
        assert_ne!(clone, captured);
        assert_eq!(bits(send(clone, b"value:", &[six]).unwrap()), 11);
        let block_type = send(ns, b"at:", &[sel(b"BlockVt")]).unwrap();
        let getter = keep(send(block_type, b"lookup", &[sel(b"captureWordAt:")]).unwrap());
        let changed = send(constant, b"withContext:", &[six]).unwrap();
        send(block_type, b"addMethod", &[sel(b"captureWordAt:"), changed]).unwrap();
        assert_eq!(bits(send(captured, b"value:", &[six]).unwrap()), 12);
        send(block_type, b"addMethod", &[sel(b"captureWordAt:"), getter]).unwrap();
        assert_eq!(bits(send(captured, b"value:", &[six]).unwrap()), 11);
        // Replacement remains live inside compiled source. The entered compiled code
        // and its retained captures remain valid while dependency bindings change.
        send(ns, b"at:put:", &[sel(b"CompilerProbe"), actor]).unwrap();
        let identity = send(actor_type, b"lookup", &[sel(b"numberIdentity")]).unwrap();
        send(actor_type, b"addMethod", &[sel(b"dependency:"), identity]).unwrap();
        let dependent = evaluate("[:x | [CompilerProbe dependency: x]]");
        assert_eq!(send(dependent, b"value:", &[five]), Ok(five));
        let replacement = send(actor_type, b"lookup", &[sel(b"replacement")]).unwrap();
        let expected = send(actor, b"at:", &[abi::size(6).unwrap()]).unwrap();
        send(
            actor_type,
            b"addMethod",
            &[sel(b"dependency:"), replacement],
        )
        .unwrap();
        send(gc, b"collectPrecisely", &[]).unwrap();
        assert_eq!(send(dependent, b"value:", &[five]), Ok(expected));
        send(actor_type, b"addMethod", &[sel(b"dependency:"), identity]).unwrap();
        // Reflection captures fresh identity and the entered code/context.
        let capture_a = keep(send(actor, b"methodFor:", &[sel(b"dependency:")]).unwrap());
        let capture_b = keep(send(actor_type, b"methodFor:", &[sel(b"dependency:")]).unwrap());
        assert_ne!(capture_a, identity);
        assert_ne!(capture_b, capture_a);
        send(
            actor_type,
            b"addMethod",
            &[sel(b"dependency:"), replacement],
        )
        .unwrap();
        send(gc, b"collectPrecisely", &[]).unwrap();
        let arguments = send(id.root, b"sequence", &[five]).unwrap();
        assert_eq!(
            send(capture_a, b"invokeOn:arguments:", &[actor, arguments]),
            Ok(five)
        );
        send(actor_type, b"addMethod", &[sel(b"dependency:"), identity]).unwrap();
        // A captured native Method can itself be installed directly. Destroying all
        // lookup on that selected Method must not add a hidden application stage.
        let method = send(codegen, b"compileSynchronous:", &[text(b"41")]).unwrap();
        let token = send(roots, b"retain:", &[method]).unwrap();
        send(actor_type, b"addMethod", &[sel(b"compiledProbe:"), method]).unwrap();
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
        send(id.root, b"reassociateBehaviorOf:with:", &[method, private]).unwrap();
        assert_eq!(bits(send(actor, b"compiledProbe:", &[]).unwrap()), 41);
        send(actor_type, b"removeMethod:", &[sel(b"compiledProbe:")]).unwrap();
        send(roots, b"release:", &[token]).unwrap();
        // Cleanup keeps a freshly returned object alive through collecting release.
        let roots_type = send(id.root, b"behaviorOf:", &[roots]).unwrap();
        let release = send(roots_type, b"lookup", &[sel(b"release:")]).unwrap();
        send(roots_type, b"addMethod", &[sel(b"savedRelease:"), release]).unwrap();
        let collecting_release = send(actor_type, b"lookup", &[sel(b"collectingRelease")]).unwrap();
        send(
            roots_type,
            b"addMethod",
            &[sel(b"release:"), collecting_release],
        )
        .unwrap();
        let protected = evaluate("\"survives collecting cleanup\"");
        assert_eq!(send(protected, b"isError", &[]), Ok(falsity));
        send(roots_type, b"addMethod", &[sel(b"release:"), release]).unwrap();
        // Failed returns remain distinct from an ordinary Error result.
        let failing = send(
            codegen,
            b"compileSynchronous:",
            &[text(b"[CompilerProbe rethrow: [1 / 0]]")],
        )
        .unwrap();
        let fail_token = send(roots, b"retain:", &[failing]).unwrap();
        let failing_block = send(blocks, b"fromMethod:captureOperands:", &[failing]).unwrap();
        let block_token = send(roots, b"retain:", &[failing_block]).unwrap();
        let execution = send(ns, b"at:", &[sel(b"Execution")]).unwrap();
        let failure = send(execution, b"evaluatePreservingFailures:", &[failing_block])
            .expect_err("preserved failed return");
        assert!(!abi::is_fault(failure));
        assert_eq!(send(failure, b"isError", &[]), Ok(truth));
        let converted = send(failing_block, b"value", &[]).unwrap();
        assert_eq!(send(converted, b"isError", &[]), Ok(truth));
        assert_eq!(
            send(converted, b"at:", &[abi::size(0).unwrap()]),
            Ok(sel(b"nativeFailure"))
        );
        send(roots, b"release:", &[block_token]).unwrap();
        send(roots, b"release:", &[fail_token]).unwrap();
        let invalid = send(codegen, b"compileSynchronous:", &[text(b"[ :x |")]).unwrap();
        assert_eq!(send(invalid, b"isError", &[]), Ok(truth));
        // Release this fixture's result roots before the isolated lifetime checks.
        drop(evaluate);
        for token in result_tokens {
            send(roots, b"release:", &[token]).unwrap();
        }
        send(gc, b"collectPrecisely", &[]).unwrap();
        send(tcc, b"checkpoint", &[]).unwrap();
        // Only the inner Block retains this Unit after its initializer/root are gone.
        let before = abi::as_size(send(tcc, b"liveNativeUnits", &[]).unwrap()).unwrap();
        let method = send(codegen, b"compileSynchronous:", &[text(b"[:x | x]")]).unwrap();
        let token = send(roots, b"retain:", &[method]).unwrap();
        let root_block = send(blocks, b"fromMethod:captureOperands:", &[method]).unwrap();
        let root_token = send(roots, b"retain:", &[root_block]).unwrap();
        let closure = send(root_block, b"value", &[]).unwrap();
        let closure_token = send(roots, b"retain:", &[closure]).unwrap();
        send(roots, b"release:", &[root_token]).unwrap();
        send(roots, b"release:", &[token]).unwrap();
        send(gc, b"collectPrecisely", &[]).unwrap();
        send(tcc, b"checkpoint", &[]).unwrap();
        assert_eq!(
            abi::as_size(send(tcc, b"liveNativeUnits", &[]).unwrap()),
            Some(before + 1)
        );
        assert_eq!(send(closure, b"value:", &[five]), Ok(five));
        send(roots, b"release:", &[closure_token]).unwrap();
        send(gc, b"collectPrecisely", &[]).unwrap();
        send(tcc, b"checkpoint", &[]).unwrap();
        assert_eq!(
            abi::as_size(send(tcc, b"liveNativeUnits", &[]).unwrap()),
            Some(before)
        );
        // Entered source code remains callable after its binding is removed and a
        // collection/checkpoint runs from a nested dependency send.
        let remove = send(actor_type, b"lookup", &[sel(b"removeDuringCall:")]).unwrap();
        send(
            actor_type,
            b"addMethod",
            &[sel(b"removeDuringCall:"), remove],
        )
        .unwrap();
        let running = send(
            codegen,
            b"compileSynchronous:",
            &[text(b"[CompilerProbe removeDuringCall: Namespace]; 73")],
        )
        .unwrap();
        send(
            actor_type,
            b"addMethod",
            &[sel(b"compiledRunning"), running],
        )
        .unwrap();
        assert_eq!(bits(send(actor, b"compiledRunning", &[]).unwrap()), 73);
        assert!(
            send(actor_type, b"lookup", &[sel(b"compiledRunning")])
                .unwrap()
                .is_null()
        );
        send(gc, b"collectPrecisely", &[]).unwrap();
        send(tcc, b"checkpoint", &[]).unwrap();
        assert_eq!(
            abi::as_size(send(tcc, b"liveNativeUnits", &[]).unwrap()),
            Some(before)
        );
        super::resumable::check(id,send,sel,[ns,roots,gc,string,actor,integer,blocks]);
        super::task_control::check(id,send,sel,[ns,roots,gc,string,integer,blocks]);
        println!("COMPILER_EXECUTION_OOP_OK");
    }
}
