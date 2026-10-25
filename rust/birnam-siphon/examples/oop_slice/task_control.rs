//! Unchanged source control policy exercised through the real Oop runtime.
use birnam_abi::{self as abi, Oop, id_protocol::Protocol};
pub(super) unsafe fn check(
    id: &Protocol,
    send: &impl Fn(Oop, &[u8], &[Oop]) -> Result<Oop, Oop>,
    sel: &impl Fn(&[u8]) -> Oop,
    objects: [Oop; 5],
) {
    let [ns, roots, gc, string, integer] = objects;
    let retain = |v| send(roots, b"retain:", &[v]).unwrap();
    let release = |v| {
        send(roots, b"release:", &[v]).unwrap();
    };
    let text = |bytes: &[u8]| {
        let data: Vec<_> = bytes
            .iter()
            .map(|b| abi::size(*b as usize).unwrap())
            .collect();
        let data = send(id.root, b"sequence", &data).unwrap();
        send(string, b"fromBytes:", &[data]).unwrap()
    };
    let codegen = send(ns, b"at:", &[sel(b"Codegen")]).unwrap();
    let truth = send(ns, b"at:", &[sel(b"true")]).unwrap();
    let evaluate = |source: &str| {
        let source = text(source.as_bytes());
        let st = retain(source);
        let result = send(codegen, b"eval:", &[source]).unwrap();
        let rt = retain(result);
        assert_ne!(
            send(result, b"isError", &[]),
            Ok(truth),
            "source evaluation failed"
        );
        release(st);
        send(gc, b"collectPrecisely", &[]).unwrap();
        release(rt);
        result
    };
    send(ns, b"at:put:", &[sel(b"Globals"), ns]).unwrap();
    let answer = evaluate(
        "let taskSmokeResult := [Object clone]; [Reflect defineMethod: #plus: on: taskSmokeResult as: [:self :x | [x + 7]]]; [taskSmokeResult plus: 5]",
    );
    let bits = |v| {
        let w = send(integer, b"wordOf:", &[v]).unwrap();
        abi::as_size(w).unwrap_or_else(|| abi::as_size(send(w, b"low32", &[]).unwrap()).unwrap())
    };
    assert_eq!(bits(answer), 12);
    let selector = "a:".repeat(16);
    let parameters = (0..16)
        .map(|i| format!(":x{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    let arguments = (0..16)
        .map(|i| format!("a: {i}"))
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(
        bits(evaluate(&format!(
            "[Reflect defineMethod: #{selector} on: taskSmokeResult as: [:self {parameters} | [x0 + x15]]]; [taskSmokeResult {arguments}]"
        ))),
        15
    );
    println!("SOURCE_METHOD_DEFINITION_OK");
    let root = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    send(ns, b"at:put:", &[sel(b"Globals"), std::ptr::null_mut()]).unwrap();
    send(ns, b"at:put:", &[sel(b"Error"), std::ptr::null_mut()]).unwrap();
    let symbol_type = send(id.root, b"behaviorOf:", &[sel(b"SymbolVt")]).unwrap();
    send(ns, b"at:put:", &[sel(b"SymbolVt"), symbol_type]).unwrap();
    let collection = send(ns, b"at:", &[sel(b"CollectionVt")]).unwrap();
    send(ns, b"at:put:", &[sel(b"Collection"), collection]).unwrap();
    send(ns, b"at:put:", &[sel(b"Vtable"), id.root]).unwrap();
    for name in [
        "error",
        "globals-owner",
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
    ] {
        println!("SOURCE_CONTROL_LOADING {name}");
        evaluate(&std::fs::read_to_string(root.join(format!("modules/{name}.birnam"))).unwrap());
        println!("SOURCE_CONTROL_LOADED {name}");
    }
    evaluate(&std::fs::read_to_string(root.join("modules/source-registry.birnam")).unwrap());
    unsafe {
        super::source_registration::check(id, send, sel, [ns, roots, gc, string, integer]);
    }
    evaluate(&std::fs::read_to_string(root.join("modules/source-loader.birnam")).unwrap());
    unsafe {
        super::source_loading::check(id, send, sel, [ns, roots, gc, string, integer]);
    }
    let target = std::env::var_os("BIRNAM_OOP_TARGET")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join(".cargo-target/oop-slice"));
    let path: Vec<_> = target
        .join("fixtures/continuation-native.dylib")
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .map(|b| abi::size(*b as usize).unwrap())
        .collect();
    let path = send(id.root, b"nameWithBytes", &path).unwrap();
    let pt = retain(path);
    let args = send(id.root, b"sequence", &[]).unwrap();
    let image = unsafe { id.send(path, id.load, &[args]) }.unwrap();
    let it = retain(image);
    release(pt);
    let observer = evaluate("[Object clone]");
    let ot = retain(observer);
    let cleanup = evaluate("[| nil]");
    let context = send(id.root, b"sequence", &[cleanup, sel(b"mustNotRecord")]).unwrap();
    let ct = retain(context);
    let vt = send(id.root, b"ensurePrivateVtableFor:", &[observer]).unwrap();
    for name in [b"observe:".as_slice(), b"failTask:", b"ensureTask:body:"] {
        let method = send(image, b"methodAt:", &[sel(name)]).unwrap();
        let method = send(method, b"withContext:", &[context]).unwrap();
        send(vt, b"addMethod", &[sel(name), method]).unwrap();
    }
    let globals = send(ns, b"at:", &[sel(b"Globals")]).unwrap();
    send(globals, b"at:put:", &[sel(b"NativeTaskObserver"), observer]).unwrap();
    release(ct);
    release(ot);
    release(it);
    send(gc, b"budget:", &[abi::size(16 << 20).unwrap()]).unwrap();
    evaluate("[System autoGc: true]; [System preciseGc: true]; [System liveObjects]");
    evaluate(&std::fs::read_to_string(root.join("scripts/task-control-smoke.birnam")).unwrap());
    evaluate("[TaskCheck run]");
    let globals = send(ns, b"at:", &[sel(b"Globals")]).unwrap();
    let failures = send(globals, b"at:", &[sel(b"TaskCheckFailures")]).unwrap();
    assert_eq!(bits(send(failures, b"value", &[]).unwrap()), 0);
    let checks = send(globals, b"at:", &[sel(b"TaskChecks")]).unwrap();
    let count = bits(send(checks, b"value", &[]).unwrap());
    assert_eq!(count, 177);
    println!("SOURCE_TASK_CONTROL_OK checks={count}");
    let handled = evaluate(
        "[[| let seen := [0 asCell]; let task := [TaskControl start: [:task | [[| [Error kind: #wanted]] on: #wanted do: [:error | [task wait]; [seen inc]; 17]]]]; [GC collectPrecisely]; [task resolve: [task waitToken] with: nil]; #[[seen value] [task pollOutcome]]] value]",
    );
    let ht = retain(handled);
    assert_eq!(
        send(handled, b"=", &[evaluate("#[1 #[#success 17]]")]),
        Ok(truth)
    );
    release(ht);
    let ensured = evaluate(
        "[[| let seen := [0 asCell]; let task := [TaskControl start: [:task | [[| 23] ensure: [| [task wait]; [seen inc]]]]]; [GC collectPrecisely]; [task resolve: [task waitToken] with: nil]; #[[seen value] [task pollOutcome]]] value]",
    );
    let et = retain(ensured);
    assert_eq!(
        send(ensured, b"=", &[evaluate("#[1 #[#success 23]]")]),
        Ok(truth)
    );
    release(et);
    assert_eq!(evaluate("[[Codegen eval: \"[|\"] isError]"), truth);
    println!("SOURCE_HANDLER_CLEANUP_OK");
    evaluate("[System autoGc: false]");
}
