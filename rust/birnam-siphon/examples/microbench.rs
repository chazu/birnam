//! Short paired benchmarks through Id; compilation/bootstrap are untimed.
#[path = "micro_support/clock.rs"]
mod clock;
#[path = "../tests/support/mod.rs"]
mod support;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use serde_json::{Value, json};
use std::{
    ffi::c_void,
    os::unix::ffi::OsStrExt,
    path::PathBuf,
    time::{Duration, Instant},
};
use support::*;
// Echo an ABI carrier without inspecting an Object payload. The caller owns
// any byte borrow for this complete Id invocation.
unsafe extern "C" fn native_echo(_: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    let args = unsafe { borrowed_slice(a,n) }.unwrap();
    assert_eq!(args.len(),1);
    Outcome::loaded(args[0])
}
struct Compiler {
    id: Protocol,
    integer: Oop,
    mask: usize,
}
struct Hook {
    compiler: *mut Compiler,
    factory: bool,
}
unsafe extern "C" fn compile_options(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    let hook = unsafe { &*c.cast::<Hook>() };
    let s = unsafe { &*hook.compiler };
    let mask = unsafe {
        s.id.send_named(s.integer, b"fromWord:", &[Operand::word(s.mask)])
    };
    let Ok(mask) = mask else {
        return mask.unwrap_err();
    };
    let mut args = unsafe { borrowed_slice(a, n) }.unwrap().to_vec();
    args.push(mask);
    unsafe {
        s.id.send_named(
            r,
            if hook.factory {
                b"factory:source:dispositions:optimizations:"
            } else {
                b"emit:source:filename:persistent:synchronous:optimizations:"
            },
            &args,
        )
    }
    .map_or_else(|e| e, Outcome::loaded)
}
struct Counts {
    id: Protocol,
    families: Vec<(Oop, &'static str)>,
    counts: std::collections::BTreeMap<&'static str, usize>,
}
unsafe extern "C" fn allocation(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    let c = unsafe { &mut *c.cast::<Counts>() };
    let mut at = unsafe {
        send(
            c.id,
            c.id.receiver,
            b"behaviorOf:",
            &[borrowed_slice(a, n).unwrap()[0]],
        )
    }
    .as_object()
    .unwrap();
    let mut name = "other";
    for _ in 0..64 {
        if let Some((_, kind)) = c.families.iter().find(|(v, _)| *v == at) {
            name = kind;
            break;
        }
        let Some(parent) =
            unsafe { send(c.id, c.id.receiver, b"parentOf:", &[Operand::object(at)]) }.as_object()
        else {
            break;
        };
        at = parent;
    }
    *c.counts.entry(name).or_default() += 1;
    Outcome::loaded(Operand::unit())
}
unsafe fn counts(
    id: Protocol,
    families: &[(Oop, &'static str)],
) -> std::collections::BTreeMap<&'static str, usize> {
    let mut c = Counts {
        id,
        families: families.to_vec(),
        counts: Default::default(),
    };
    unsafe {
        inventory(id, allocation, (&raw mut c).cast());
    }
    c.counts
}
struct Case {
    name: String,
    source: String,
    mask: usize,
    expected: usize,
    result_selector: &'static [u8],
    integer_inputs: bool,
}
fn cases() -> Vec<Case> {
    let wrapper = |body: &str| {
        format!(
            r#"let probe := [Object clone]; [Reflect defineMethod: #run:with: on: probe as: [Execution block: [:self :a :b | {body}] withNativeSignature: "oOO"]]; [Globals at: #MicroProbe put: probe]; true"#
        )
    };
    let mut cases=vec![
        Case{name:"word-nested".into(),source:wrapper("[[[[a bitXor: b] wrappingAdd: a] bitAnd: b] wrappingAdd: a]"),mask:1,expected:16,result_selector:b"bits",integer_inputs:false},
        Case{name:"word-locals".into(),source:wrapper("let x := [a bitXor: b]; let sum := [x wrappingAdd: a]; let masked := [sum bitAnd: b]; [masked wrappingAdd: a]"),mask:1,expected:16,result_selector:b"bits",integer_inputs:false},
        Case{name:"abi".into(),source:wrapper(r#"let bridge := Execution; let target := Integer; let signature := "Wo"; let factory := Word; [bridge sendNativeTo: target selector: #wordOf: arguments: #[a] signature: signature resultFactory: factory]"#),mask:2,expected:12345,result_selector:b"bits",integer_inputs:true},
        Case{name:"helper".into(),source:r#"let mask := [Execution wordResultOf: 15 selector: #word]; let probe := [Object clone]; [Reflect defineMethod: #run:with: on: probe as: [:self :a :b | let x := [a bitXor: b]; let sum := [x wrappingAdd: a]; [[sum bitXor: mask] wrappingSubtract: sum]]]; [Globals at: #MicroProbe put: probe]; true"#.into(),mask:4,expected:3,result_selector:b"bits",integer_inputs:false},
    ];
    cases.push(Case{name:"abi-captured".into(),source:r#"let bridge := Execution; let target := Integer; let signature := "Wo"; let factory := Word; let probe := [Object clone]; [Reflect defineMethod: #run:with: on: probe as: [Execution block: [:self :a :b | [bridge sendNativeTo: target selector: #wordOf: arguments: #[a] signature: signature resultFactory: factory]] withNativeSignature: "oOO"]]; [Globals at: #MicroProbe put: probe]; true"#.into(),mask:2,expected:12345,result_selector:b"bits",integer_inputs:true});
    cases.push(Case{name:"abi-audit-object-incoming".into(),source:wrapper("a"),mask:31,expected:12345,result_selector:b"word",integer_inputs:true});
    let captured = |body: &str| format!(r#"let bridge := Execution; let target := Integer; let factory := Word; let signature := "Wo"; let resultSignature := "ow"; {}"#, wrapper(body));
    cases.push(Case{name:"abi-audit-read".into(),source:captured("[bridge sendNativeTo: target selector: #wordOf: arguments: #[a] signature: signature resultFactory: factory]"),mask:31,expected:12345,result_selector:b"bits",integer_inputs:true});
    cases.push(Case{name:"abi-audit-roundtrip".into(),source:captured("let bits := [bridge sendNativeTo: target selector: #wordOf: arguments: #[a] signature: signature resultFactory: factory]; [bridge sendNativeTo: target selector: #fromWord: arguments: #[bits] signature: resultSignature resultFactory: factory]"),mask:31,expected:12345,result_selector:b"word",integer_inputs:true});
    for (name, signature, preamble, argument, result, expected) in [
        ("abi-audit-word-incoming","ww","","a","result",10),
        ("abi-audit-bytes-incoming","bb","let payload := [\"hello\" asBytes];","payload","[result size]",5),
    ] {
        let source = format!(r#"let bridge := Execution; let target := [Object clone]; let factory := {factory}; let signature := "{signature}"; {preamble} [Reflect defineMethod: #echo: on: target as: [Execution block: [:self :value | value] withNativeSignature: "{signature}"]]; [Globals at: #MicroAbiTarget put: target]; {wrapper}"#,
            factory=if signature=="bb" {"Bytes"}else{"Word"},
            wrapper=wrapper(&format!("let result := [bridge sendNativeTo: target selector: #echo: arguments: #[{argument}] signature: signature resultFactory: factory]; {result}")));
        cases.push(Case{name:name.into(),source,mask:31,expected,result_selector:if signature=="bb" {b"word"}else{b"bits"},integer_inputs:false});
    }
    // Isolate the real arithmetic policy in two private factories. Only the
    // receiver/constructor bridge is adapted; no arithmetic is reimplemented.
    let original =
        std::fs::read_to_string("modules/integer.birnam").expect("run from repository root");
    let cutoff = original
        .find("[[intFactory hasBootstrapArithmetic:")
        .unwrap();
    let source = original[..cutoff]
        .replace("#integerValues", "#microInteger")
        .replace(
            "let intFactory := Integer;",
            "let intSeed := Integer; let intFactory := [Object clone];",
        )
        .replace("on: Integer as:", "on: intFactory as:")
        .replace(
            "sendNativeTo: intFactory selector: #wordOf:",
            "sendNativeTo: intSeed selector: #wordOf:",
        )
        .replace(
            "sendNativeTo: self selector: #fromWord:",
            "sendNativeTo: intSeed selector: #fromWord:",
        )
        .replace(
            "#integerBinary:operation:",
            "#integerBinary:with:operation:",
        )
        .replace(
            "[:self :other :operation |",
            "[:self :leftObject :other :operation |",
        )
        .replace("integerBitsOf: self", "integerBitsOf: leftObject");
    for (name, operator, expected) in [
        ("integer-add", "add", 12382),
        ("integer-multiply", "multiply", 456765),
    ] {
        cases.push(Case{name:name.into(),source:format!("{source}\nlet probe := [Object clone]; [Reflect defineMethod: #run:with: on: probe as: [:self :a :b | [intFactory integerBinary: a with: b operation: #{operator}]]]; [Globals at: #MicroProbe put: probe]; true"),mask:7,expected,result_selector:b"word",integer_inputs:true});
    }
    cases.push(Case{name:"integer-add-core".into(),source:format!("{source}\nlet probe := [Object clone]; [Reflect defineMethod: #run:with: on: probe as: [:self :a :b | [intFactory integerAddBits: a with: b]]]; [Globals at: #MicroProbe put: probe]; true"),mask:7,expected:16,result_selector:b"word",integer_inputs:false});
    let mut body = "let x0 := a;".to_owned();
    for i in 1..=16 {
        body.push_str(&format!(" let x{i} := [x{} + b];", i - 1));
    }
    body.push_str(" x16");
    cases.push(Case {
        name: "public-integer-loop".into(),
        source: wrapper(&body),
        mask: 7,
        expected: 12937,
        result_selector: b"word",
        integer_inputs: true,
    });
    let public = cases.last().unwrap();
    cases.push(Case {
        name: "public-integer-roots".into(),
        source: public.source.clone(),
        mask: 15,
        expected: public.expected,
        result_selector: b"word",
        integer_inputs: true,
    });
    let public = cases
        .iter()
        .find(|c| c.name == "public-integer-roots")
        .unwrap();
    cases.push(Case {
        name: "public-integer-retains".into(),
        source: public.source.clone(),
        mask: 31,
        expected: public.expected,
        result_selector: b"word",
        integer_inputs: true,
    });
    let public = cases.iter().find(|c| c.name == "public-integer-retains").unwrap();
    cases.push(Case {
        name: "public-integer-bindings".into(), source: public.source.clone(), mask: 31,
        expected: public.expected, result_selector: b"word", integer_inputs: true,
    });
    for (name, preamble, expression, expected) in [
        (
            "semantic-string-length",
            "let text := \"hello\";",
            "[text size]",
            5,
        ),
        (
            "semantic-vector-access",
            "let values := #[11 22 33]; let index := 1;",
            "[values at: index]",
            22,
        ),
    ] {
        let source = format!(
            "{preamble} let probe := [Object clone]; [Reflect defineMethod: #run:with: on: probe as: [Execution block: [:self :a :b | {expression}] withNativeSignature: \"oOO\"]]; [Globals at: #MicroProbe put: probe]; true"
        );
        cases.push(Case {
            name: name.into(),
            source,
            mask: 31,
            expected,
            result_selector: b"word",
            integer_inputs: false,
        });
    }
    cases
}
#[derive(Clone, Copy)]
struct Variant {
    receiver: Oop,
    arithmetic: Option<usize>,
    binding_proofs: Option<bool>,
    native_read: Option<(Oop, Oop, Option<Oop>)>,
}
struct Runtime {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    gc: Oop,
    selector: Oop,
    families: Vec<(Oop, &'static str)>,
    arithmetic: Vec<Vec<(Oop, NativeEntry)>>,
}
impl Runtime {
    unsafe fn activate(&self, v: Variant) {
        unsafe { send(self.id, self.named(b"Execution"), b"bindingProofBatches:",
            &[Operand::word(v.binding_proofs.unwrap_or(true) as usize)]) };
        if let Some(index) = v.arithmetic {
            unsafe {
                let vt = self.named(b"IntegerVt");
                for &(selector, entry) in &self.arithmetic[index] {
                    self.id
                        .send_named(
                            vt,
                            b"addMethod",
                            &[Operand::object(selector), Operand::entry(entry)],
                        )
                        .unwrap();
                }
            }
        }
    }
    unsafe fn named(&self, name: &[u8]) -> Oop {
        object(unsafe {
            send(
                self.id,
                self.ns,
                b"at:",
                &[Operand::object(self.id.intern_name(name).unwrap())],
            )
        })
    }
    unsafe fn run(
        &self,
        v: Variant,
        args: &[Operand],
        n: usize,
        deadline: Instant,
    ) -> (usize, Operand) {
        let mut last = Operand::unit();
        let mut done = 0;
        for i in 0..n {
            if i % 8 == 0 && Instant::now() >= deadline {
                break;
            }
            last = if let Some((receiver, selector, roundtrip)) = v.native_read {
                let result = unsafe { self.id.send(receiver, selector, &args[..1]) }.unwrap();
                if let Some(constructor) = roundtrip { unsafe { self.id.send(receiver, constructor, &[result]) }.unwrap() } else { result }
            } else { unsafe { self.id.send(v.receiver, self.selector, args) }.unwrap() };
            done += 1;
            std::hint::black_box(last);
        }
        (done, last)
    }
    unsafe fn check(&self, case: &Case, value: Operand) {
        let result = if value.as_word().is_some() { value.as_word() }
            else { unsafe { send(self.id, object(value), case.result_selector, &[]) }.as_word() };
        assert_eq!(
            result,
            Some(case.expected),
            "{} result",
            case.name
        );
    }
}
fn compilation_source(case: &Case, mask: usize, baseline: bool) -> String {
    let source = if case.name == "abi-audit-object-incoming" && baseline {
        case.source.replace("[Execution block: [:self :a :b | a] withNativeSignature: \"oOO\"]", "[:self :a :b | a]")
    } else { case.source.clone() };
    source.replace(
        "#microInteger",
        &format!("#microInteger{}{}", case.name.replace('-', ""), mask),
    )
}
unsafe fn compile_case(
    rt: &Runtime,
    case: &Case,
    mask: usize,
    compiler: &mut Compiler,
    scratch: &std::path::Path,
    baseline: bool,
) -> Variant {
    unsafe {
        compiler.mask = mask;
        let path = scratch.join(format!("{}-{mask}-{}.dylib", case.name, if baseline {"baseline"} else {"candidate"}));
        let source = compilation_source(case, mask, baseline);
        let text = object(send(
            rt.id,
            rt.named(b"String"),
            b"fromBytes:",
            &[Operand::bytes(source.as_bytes())],
        ));
        let tt = retain(rt.id, rt.roots, text);
        let out = object(send(
            rt.id,
            rt.named(b"String"),
            b"fromBytes:",
            &[Operand::bytes(path.as_os_str().as_bytes())],
        ));
        let ot = retain(rt.id, rt.roots, out);
        let result = object(send(
            rt.id,
            rt.named(b"Codegen"),
            b"compile:output:from:synchronousBodies:",
            &[
                Operand::object(text),
                Operand::object(out),
                Operand::object(out),
                Operand::object(rt.named(b"true")),
            ],
        ));
        assert_eq!(
            object(send(rt.id, result, b"isError", &[])),
            rt.named(b"false"),
            "compile {} mask {mask}",
            case.name
        );
        let loaded = object(send(
            rt.id,
            rt.named(b"Codegen"),
            b"load:",
            &[Operand::object(out)],
        ));
        assert_eq!(
            object(send(rt.id, loaded, b"isError", &[])),
            rt.named(b"false")
        );
        let receiver = object(send(
            rt.id,
            rt.named(b"Globals"),
            b"at:",
            &[Operand::object(rt.id.intern_name(b"MicroProbe").unwrap())],
        ));
        retain(rt.id, rt.roots, receiver);
        release(rt.id, rt.roots, ot);
        release(rt.id, rt.roots, tt);
        if case.name == "abi-audit-object-incoming" {
            let selected = send(rt.id, rt.id.receiver, b"bind:to:", &[
                Operand::object(rt.selector), Operand::object(receiver),
            ]).as_entry().unwrap();
            let implementation = object(send(rt.id, rt.named(b"Execution"), b"implementationOfEntry:", &[Operand::entry(selected)]));
            let code = object(send(rt.id, implementation, b"codeObject", &[]));
            let signature = object(send(rt.id, code, b"nativeSignature", &[]));
            assert_eq!(signature == rt.named(b"nil"), baseline, "the control really has an untyped/typed incoming ABI");
        }
        if baseline && matches!(case.name.as_str(), "abi-audit-word-incoming" | "abi-audit-bytes-incoming") {
            let target = object(send(rt.id, rt.named(b"Globals"), b"at:", &[Operand::object(rt.id.intern_name(b"MicroAbiTarget").unwrap())]));
            let behavior = object(send(rt.id, rt.id.receiver, b"behaviorOf:", &[Operand::object(target)]));
            install(rt.id, behavior, b"echo:", native_echo, std::ptr::null_mut());
        }
        Variant {
            receiver,
            arithmetic: None,
            binding_proofs: None,
            native_read: None,
        }
    }
}
unsafe fn messages(rt: &Runtime, case: &Case, v: Variant, args: &[Operand], warm_after_gc: bool, include_trace: bool) -> Value {
    unsafe {
        rt.activate(v);
        let (_, warm) = rt.run(v, args, 8, Instant::now() + Duration::from_secs(2));
        rt.check(case, warm);
        collect(rt.id, rt.gc);
        send(rt.id, rt.gc, b"budget:", &[Operand::word(usize::MAX)]);
        if warm_after_gc {
            let (_, result) = rt.run(v, args, 8, Instant::now() + Duration::from_secs(2));
            rt.check(case, result);
        }
        let begin = rt.id.intern_name(b"dispatchCountsBegin").unwrap();
        let end = rt.id.intern_name(b"dispatchCountsEnd").unwrap();
        rt.id.send(rt.id.receiver, begin, &[]).unwrap();
        let (n, result) = rt.run(v, args, 1, Instant::now() + Duration::from_secs(2));
        assert_eq!(n, 1);
        let snapshot = rt
            .id
            .send(rt.id.receiver, end, &[])
            .unwrap()
            .as_bytes()
            .unwrap();
        let owned = birnam_abi::borrowed_bytes(snapshot.pointer, snapshot.length)
            .unwrap()
            .to_vec();
        rt.check(case, result);
        let mut counts = std::collections::BTreeMap::new();
        for line in std::str::from_utf8(&owned).unwrap().lines() {
            let (count, hex) = line.split_once('\t').unwrap();
            let name = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                .collect::<Vec<_>>();
            let name = String::from_utf8(name).unwrap();
            if name == "dispatchCountsEnd" {
                continue;
            }
            counts.insert(name, count.parse::<usize>().unwrap());
        }
        let root_result = rt
            .id
            .send_named(rt.id.receiver, b"dispatchRootSummary", &[])
            .unwrap()
            .as_bytes()
            .unwrap();
        let root_owned = birnam_abi::borrowed_bytes(root_result.pointer, root_result.length)
            .unwrap()
            .to_vec();
        let root_summary: Value = serde_json::from_slice(&root_owned).unwrap();
        let raw_edges = rt
            .id
            .send_named(rt.id.receiver, b"dispatchEdges", &[])
            .unwrap()
            .as_bytes()
            .unwrap();
        let edge_bytes = birnam_abi::borrowed_bytes(raw_edges.pointer, raw_edges.length)
            .unwrap()
            .to_vec();
        let decode = |text: &str| {
            if text == "-" {
                "<entry>".to_owned()
            } else {
                String::from_utf8(
                    (0..text.len())
                        .step_by(2)
                        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
                        .collect(),
                )
                .unwrap()
            }
        };
        let mut edges = Vec::new();
        for line in std::str::from_utf8(&edge_bytes).unwrap().lines() {
            let fields: Vec<_> = line.split('\t').collect();
            let child = decode(fields[4]);
            if child == "dispatchCountsEnd" {
                continue;
            }
            edges.push(json!({"count":fields[0].parse::<usize>().unwrap(),"parent_behavior":fields[1],"parent_selector":decode(fields[2]),"behavior":fields[3],"selector":child}));
        }
        assert_eq!(
            edges
                .iter()
                .map(|e| e["count"].as_u64().unwrap())
                .sum::<u64>(),
            counts.values().copied().sum::<usize>() as u64
        );
        let region_result = rt.id.send_named(rt.id.receiver, b"dispatchRegions", &[]).unwrap().as_bytes().unwrap();
        let region_bytes = birnam_abi::borrowed_bytes(region_result.pointer, region_result.length).unwrap().to_vec();
        let regions: Vec<Value> = std::str::from_utf8(&region_bytes).unwrap().lines().filter_map(|line| {
            let fields: Vec<_> = line.split('\t').collect();
            let selector = decode(fields[2]);
            if selector == "dispatchCountsEnd" { return None; }
            Some(json!({"region":decode(fields[1]),"selector":selector,"count":fields[0].parse::<usize>().unwrap()}))
        }).collect();
        assert_eq!(regions.iter().map(|r|r["count"].as_u64().unwrap()).sum::<u64>(), counts.values().copied().sum::<usize>() as u64);
        let subtree_result = rt.id.send_named(rt.id.receiver, b"dispatchSubtrees", &[]).unwrap().as_bytes().unwrap();
        let subtree_bytes = birnam_abi::borrowed_bytes(subtree_result.pointer, subtree_result.length).unwrap().to_vec();
        let subtrees: Vec<Value> = std::str::from_utf8(&subtree_bytes).unwrap().lines().filter_map(|line| {
            let fields: Vec<_> = line.split('\t').collect();
            let selector = decode(fields[4]);
            if selector == "dispatchCountsEnd" { return None; }
            Some(json!({"region":decode(fields[2]),"parent_selector":decode(fields[3]),"selector":selector,"calls":fields[0].parse::<usize>().unwrap(),"inclusive_sends":fields[1].parse::<usize>().unwrap()}))
        }).collect();
        assert_eq!(subtrees.iter().map(|r|r["calls"].as_u64().unwrap()).sum::<u64>(), counts.values().copied().sum::<usize>() as u64);
        let mut trace_rows=Vec::new();
        if include_trace {
            let raw=rt.id.send_named(rt.id.receiver,b"dispatchTrace",&[]).unwrap().as_bytes().unwrap();
            let copied=birnam_abi::borrowed_bytes(raw.pointer,raw.length).unwrap().to_vec();
            for line in std::str::from_utf8(&copied).unwrap().lines() {
                let f:Vec<_>=line.split('\t').collect();let selector=decode(f[4]);
                if selector=="dispatchCountsEnd" {continue;}
                trace_rows.push(json!({"id":f[0].parse::<usize>().unwrap(),"parent":f[1].parse::<usize>().unwrap(),"receiver":f[2],"behavior":f[3],"selector":selector,"key":decode(f[5]),"argument":f[6],"result":f[7],"inclusive":f[8].parse::<usize>().unwrap(),"status":f[9].parse::<usize>().unwrap()}));
            }
            assert_eq!(trace_rows.len(),counts.values().copied().sum::<usize>());
        }
        json!({"trace":trace_rows,"subtrees":subtrees,"regions":regions,"invocations":n,"operations":if case.name.starts_with("public-integer-"){16}else{1},"selectors":counts,"root_summary":root_summary,"edges":edges})
    }
}
unsafe fn work(rt: &Runtime, case: &Case, v: Variant, args: &[Operand]) -> Value {
    unsafe {
        rt.activate(v);
        let (_, warm) = rt.run(v, args, 8, Instant::now() + Duration::from_secs(2));
        rt.check(case, warm);
        collect(rt.id, rt.gc);
        send(rt.id, rt.gc, b"budget:", &[Operand::word(usize::MAX)]);
        let before = counts(rt.id, &rt.families);
        let bytes = send(rt.id, rt.gc, b"allocationBytes", &[])
            .as_word()
            .unwrap();
        let (n, result) = rt.run(v, args, 8, Instant::now() + Duration::from_secs(2));
        assert_eq!(n, 8);
        rt.check(case, result);
        let charged = send(rt.id, rt.gc, b"allocationBytes", &[])
            .as_word()
            .unwrap()
            - bytes;
        let after = counts(rt.id, &rt.families);
        let delta: std::collections::BTreeMap<_, _> = after
            .iter()
            .map(|(k, v)| (*k, v - before.get(k).copied().unwrap_or(0)))
            .collect();
        json!({"iterations":n,"allocation_objects":delta,"gc_charge_bytes":charged})
    }
}
// The process owns both loaded Unit/Method generations. Only Id installs or
// selects their opaque entries; no foreign context/payload is inspected.
unsafe fn public_arithmetic(
    rt: &mut Runtime,
    compiler: &mut Compiler,
    scratch: &std::path::Path,
    mask: usize,
) -> usize {
    unsafe {
        let source = std::fs::read_to_string("modules/integer.birnam")
            .unwrap()
            .replace("#integerValues", &format!("#microPublicInteger{mask}"));
        let names: Vec<Vec<u8>> = source
            .split("[Reflect defineMethod: #")
            .skip(1)
            .map(|s| s.split_whitespace().next().unwrap().as_bytes().to_vec())
            .collect();
        let case = Case {
            name: format!("public-policy-{mask}"),
            source: format!("{source}\n[Globals at: #MicroProbe put: Integer]; true"),
            mask,
            expected: 0,
            result_selector: b"word",
            integer_inputs: true,
        };
        compile_case(rt, &case, mask, compiler, scratch, false);
        let integer = rt.named(b"Integer");
        let mut entries = vec![];
        for name in names {
            let selector = rt.id.intern_name(&name).unwrap();
            let entry = rt
                .id
                .send_named(
                    rt.id.receiver,
                    b"bind:to:",
                    &[Operand::object(selector), Operand::object(integer)],
                )
                .unwrap()
                .as_entry()
                .unwrap();
            let method = object(
                rt.id
                    .send_named(
                        rt.named(b"Execution"),
                        b"methodOfEntry:",
                        &[Operand::entry(entry)],
                    )
                    .unwrap(),
            );
            retain(rt.id, rt.roots, method);
            entries.push((selector, entry));
        }
        let index = rt.arithmetic.len();
        rt.arithmetic.push(entries);
        index
    }
}
fn main() {
    unsafe {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let get = |key: &str, default: &str| {
            args.iter()
                .position(|a| a == key)
                .map(|i| args[i + 1].clone())
                .unwrap_or(default.into())
        };
        let budget: get_budget::Budget =
            get_budget::Budget::new(get("--seconds", "10").parse().unwrap());
        let batch_ms: f64 = get("--batch-ms", "25").parse().unwrap();
        assert!((1.0..=100.0).contains(&batch_ms));
        let counts_only = args.iter().any(|a| a == "--counts-only");
        let trace = args.iter().any(|a| a == "--trace");
        assert!(!trace || counts_only);
        let selected = get("--case", "all");
        let gc_mode = get("--gc", "normal");
        assert!(matches!(gc_mode.as_str(), "normal" | "off"));
        let profile = get("--profile", "none");
        assert!(matches!(
            profile.as_str(),
            "none" | "baseline" | "candidate"
        ));
        let cases: Vec<_> = cases()
            .into_iter()
            .filter(|c| (selected == "all" && !c.name.starts_with("abi-audit-")) || (selected == "abi-audit" && c.name.starts_with("abi-audit-")) || c.name == selected)
            .collect();
        assert!(!cases.is_empty(), "unknown case");
        let setup = Instant::now();
        let id = id();
        let ns = load(
            id,
            "bootstrap",
            &[Operand::bytes(directory().as_os_str().as_bytes())],
        );
        let mut rt = Runtime {
            id,
            ns,
            roots: std::ptr::null_mut(),
            gc: std::ptr::null_mut(),
            selector: id.intern_name(b"run:with:").unwrap(),
            families: vec![],
            arithmetic: vec![],
        };
        rt.roots = rt.named(b"Roots");
        rt.gc = rt.named(b"GC");
        rt.families = vec![
            (rt.named(b"IntegerVt"), "integer"),
            (rt.named(b"WordVt"), "word"),
            (rt.named(b"VectorVt"), "vector"),
            (rt.named(b"BlockVt"), "block"),
        ];
        let backend = rt.named(b"NativeBackend");
        let backend_vt = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(backend)],
        ));
        let original = send(
            id,
            id.receiver,
            b"bind:to:",
            &[
                Operand::object(
                    id.intern_name(b"emit:source:filename:persistent:synchronous:")
                        .unwrap(),
                ),
                Operand::object(backend),
            ],
        )
        .as_entry()
        .unwrap();
        let mut compiler = Compiler {
            id,
            integer: rt.named(b"Integer"),
            mask: 0,
        };
        let factory_original = send(
            id,
            id.receiver,
            b"bind:to:",
            &[
                Operand::object(id.intern_name(b"factory:source:dispositions:").unwrap()),
                Operand::object(backend),
            ],
        )
        .as_entry()
        .unwrap();
        let mut program_hook = Hook {
            compiler: &raw mut compiler,
            factory: false,
        };
        let mut factory_hook = Hook {
            compiler: &raw mut compiler,
            factory: true,
        };
        install(
            id,
            backend_vt,
            b"emit:source:filename:persistent:synchronous:",
            compile_options,
            (&raw mut program_hook).cast(),
        );
        install(
            id,
            backend_vt,
            b"factory:source:dispositions:",
            compile_options,
            (&raw mut factory_hook).cast(),
        );
        let scratch = PathBuf::from(get("--scratch", "/tmp/birnam-microbench"));
        std::fs::create_dir_all(&scratch).unwrap();
        let mut prepared = vec![];
        for case in cases {
            let baseline_mask = if case.name.starts_with("semantic-") || case.name.starts_with("abi-audit-") || case.name == "public-integer-bindings" {
                31
            } else if case.name == "public-integer-roots" {
                7
            } else if case.name == "public-integer-retains"
            {
                15
            } else {
                0
            };
            let mut baseline = compile_case(&rt, &case, baseline_mask, &mut compiler, &scratch, true);
            let mut candidate = compile_case(&rt, &case, case.mask, &mut compiler, &scratch, false);
            if case.name.starts_with("public-integer-") {
                baseline.arithmetic = Some(public_arithmetic(
                    &mut rt,
                    &mut compiler,
                    &scratch,
                    baseline_mask,
                ));
                candidate.arithmetic = Some(public_arithmetic(
                    &mut rt,
                    &mut compiler,
                    &scratch,
                    case.mask,
                ));
            }
            if matches!(case.name.as_str(), "abi-audit-read" | "abi-audit-roundtrip") {
                baseline.native_read = Some((rt.named(b"Integer"), rt.id.intern_name(b"wordOf:").unwrap(), if case.name == "abi-audit-roundtrip" { Some(rt.id.intern_name(b"fromWord:").unwrap()) } else { None }));
            }
            if case.name == "public-integer-bindings" {
                baseline.binding_proofs = Some(false);
                candidate.binding_proofs = Some(true);
            }
            let make = |value| {
                object(send(
                    id,
                    rt.named(if case.integer_inputs {
                        b"Integer"
                    } else {
                        b"Word"
                    }),
                    if case.integer_inputs {
                        b"fromWord:"
                    } else {
                        b"fromBits:"
                    },
                    &[Operand::word(value)],
                ))
            };
            let a = make(if case.integer_inputs { 12345 } else { 10 });
            let b = make(if case.integer_inputs { 37 } else { 6 });
            retain(id, rt.roots, a);
            retain(id, rt.roots, b);
            let inputs = [Operand::object(a), Operand::object(b)];
            for v in [baseline, candidate] {
                rt.activate(v);
                let (_, result) = rt.run(v, &inputs, 20, Instant::now() + Duration::from_secs(2));
                rt.check(&case, result);
            }
            let mut work = [
                work(&rt, &case, baseline, &inputs),
                work(&rt, &case, candidate, &inputs),
            ];
            if counts_only {
                work[0]["messages"] = messages(&rt, &case, baseline, &inputs, false, false);
                work[1]["messages"] = messages(&rt, &case, candidate, &inputs, false, false);
                if case.name.starts_with("abi-audit-") || case.name == "public-integer-bindings" {
                    work[0]["steady_messages"] = messages(&rt, &case, baseline, &inputs, true, false);
                    work[1]["steady_messages"] = messages(&rt, &case, candidate, &inputs, true, trace);
                }
            }
            let key = if case.name.starts_with("abi") {
                "vector"
            } else {
                "word"
            };
            if case.name.starts_with("abi-audit-") {
                // Deliberate cost contrasts; each result was checked above.
            } else if case.name.starts_with("semantic-") || case.name == "public-integer-bindings"
                || case.name == "public-integer-roots"
                || case.name == "public-integer-retains"
            {
                assert_eq!(work[0]["allocation_objects"], work[1]["allocation_objects"]);
                if counts_only && case.name == "public-integer-roots" {
                    assert!(
                        work[0]["messages"]["selectors"]["release:"].as_u64()
                            > work[1]["messages"]["selectors"]["release:"].as_u64()
                    );
                }
            } else {
                assert!(
                    work[0]["allocation_objects"][key].as_u64().unwrap()
                        > work[1]["allocation_objects"][key].as_u64().unwrap(),
                    "{} controls did not change {key} allocations: {work:?}",
                    case.name
                );
            }
            prepared.push((case, [baseline, candidate], inputs, work));
        }
        install(
            id,
            backend_vt,
            b"emit:source:filename:persistent:synchronous:",
            original.method.unwrap(),
            original.context,
        );
        install(
            id,
            backend_vt,
            b"factory:source:dispositions:",
            factory_original.method.unwrap(),
            factory_original.context,
        );
        println!(
            "{}",
            json!({"event":"ready","pid":std::process::id(),"setup_ms":setup.elapsed().as_secs_f64()*1000.0})
        );
        use std::io::Write;
        std::io::stdout().flush().unwrap();
        // A profiling parent attaches after this barrier; waiting consumes no CPU.
        if profile != "none" || counts_only {
            let mut line = String::new();
            std::io::stdin().read_line(&mut line).unwrap();
        }
        let start = Instant::now();
        let deadline = start + budget.duration();
        let mut records = vec![];
        let case_count = prepared.len();
        for (index, (case, variants, inputs, work)) in prepared.into_iter().enumerate() {
            let stop = std::cmp::min(
                deadline,
                start
                    + budget
                        .duration()
                        .mul_f64((index + 1) as f64 / case_count as f64),
            );
            let mut n = 8usize;
            if profile != "none" {
                let selected = usize::from(profile == "candidate");
                rt.activate(variants[selected]);
                collect(id, rt.gc);
                send(
                    id,
                    rt.gc,
                    b"budget:",
                    &[Operand::word(if gc_mode == "off" {
                        usize::MAX
                    } else {
                        1 << 20
                    })],
                );
            }
            let mut samples = vec![];
            let mut pair = 0;
            while !counts_only && Instant::now() < stop {
                let order = if pair % 2 == 0 { [0, 1] } else { [1, 0] };
                let mut row = vec![];
                for variant in order {
                    if profile != "none" && variant != usize::from(profile == "candidate") {
                        continue;
                    }
                    if Instant::now() >= stop {
                        break;
                    }
                    if profile == "none" {
                        rt.activate(variants[variant]);
                        collect(id, rt.gc);
                        send(
                            id,
                            rt.gc,
                            b"budget:",
                            &[Operand::word(if gc_mode == "off" {
                                usize::MAX
                            } else {
                                1 << 20
                            })],
                        );
                        let (_, warm) = rt.run(variants[variant], &inputs, 8, stop);
                        if warm.kind() == 0 {
                            break;
                        }
                        rt.check(&case, warm);
                    }
                    let wall = Instant::now();
                    let cpu = clock::cpu_ns();
                    let (done, result) = rt.run(variants[variant], &inputs, n, stop);
                    let cpu_ns = clock::cpu_ns() - cpu;
                    let wall_ns = wall.elapsed().as_nanos() as u64;
                    if done == 0 {
                        break;
                    }
                    rt.check(&case, result);
                    row.push(json!({"variant":if variant==0{"baseline"}else{"candidate"},"iterations":done,"cpu_ns":cpu_ns,"wall_ns":wall_ns}));
                }
                if row.len() == 2 || (profile != "none" && row.len() == 1) {
                    let slowest = row
                        .iter()
                        .map(|r| {
                            r["cpu_ns"].as_u64().unwrap() as f64
                                / r["iterations"].as_u64().unwrap() as f64
                        })
                        .fold(1.0, f64::max);
                    // Recalibrating every pair oscillates across GC thresholds.
                    // Freeze the operation count after the first calibration pair.
                    if pair == 0 {
                        n = ((batch_ms * 1_000_000.0 / slowest) as usize).clamp(1, 8192);
                    }
                    samples.push(json!({"pair":pair,"order":order,"batches":row}));
                    pair += 1;
                } else {
                    break;
                }
            }
            records.push(json!({"compiled_sources":{"baseline":compilation_source(&case,if case.name.starts_with("semantic-") || case.name.starts_with("abi-audit-") || case.name == "public-integer-bindings" {31} else if case.name == "public-integer-roots" {7} else if case.name == "public-integer-retains" {15} else {0},true),"candidate":compilation_source(&case,case.mask,false)},"comparison_kind":if case.name.starts_with("abi-audit-"){ "ABI cost contrast, not an optimizer toggle" } else { "paired optimization/control" },"baseline_execution":if case.name=="abi-audit-read"{"direct Id Integer wordOf:"}else if case.name=="abi-audit-roundtrip"{"direct Id wordOf: then fromWord:"}else if matches!(case.name.as_str(),"abi-audit-word-incoming"|"abi-audit-bytes-incoming"){"compiled wrapper with native Id echo leaf"}else{"compiled source"},"case":case.name,"source":case.source,"candidate_mask":case.mask,"operations_per_invocation":if case.name.starts_with("public-integer-"){16}else{1},"work_counts":{"baseline":work[0],"candidate":work[1]},"samples":samples}));
        }
        println!(
            "{}",
            json!({"event":"result","schema":1,"measurement_ms":start.elapsed().as_secs_f64()*1000.0,"budget_seconds":budget.seconds(),"gc":gc_mode,"profile":if counts_only {"counts-only"} else {profile.as_str()},"cases":records})
        );
    }
}
mod get_budget {
    use std::time::Duration;
    pub struct Budget(f64);
    impl Budget {
        pub fn new(seconds: f64) -> Self {
            assert!((0.1..=10.0).contains(&seconds));
            Self(seconds)
        }
        pub fn seconds(&self) -> f64 {
            self.0
        }
        pub fn duration(&self) -> Duration {
            Duration::from_secs_f64(self.0)
        }
    }
}
