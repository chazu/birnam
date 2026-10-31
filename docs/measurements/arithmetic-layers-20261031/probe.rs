use birnam_abi as abi;
use birnam_host_support::{Context, Resources};

use std::{hint::black_box, os::unix::ffi::OsStrExt, path::PathBuf, time::Instant};
fn context(prefix: PathBuf) -> Context {
    let lib = prefix.join("lib");
    let id = birnam_siphon::load(
        lib.join("libbirnam_id.dylib").as_os_str().as_bytes(),
        &[
            lib.join("libbirnam_bootstrap.dylib").as_os_str().as_bytes(),
            lib.as_os_str().as_bytes(),
            b"nameWithBytes",
            b"sequence",
        ],
    )
    .unwrap();
    unsafe {
        let constructor = id.send(id.literal(2).unwrap(), id.intern, &[]).unwrap();
        let seq = id.send(id.literal(3).unwrap(), id.intern, &[]).unwrap();
        let args = id.send(id.root, seq, &[id.literal(1).unwrap()]).unwrap();
        let ns = id.send(id.literal(0).unwrap(), id.load, &[args]).unwrap();
        Context::from_namespace(
            id,
            ns,
            Resources {
                prefix: prefix.clone(),
                lib_dir: lib.clone(),
                id_dylib: lib.join("libbirnam_id.dylib"),
                rust_workspace_dir: prefix.join("share/birnam/src"),
                guide_dir: prefix.join("share/birnam/docs/guide"),
            },
            constructor,
        )
        .unwrap()
    }
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let h = context(PathBuf::from(&args[1]));
    let q = h.scope();
    let integer = h.named(b"Integer").unwrap();
    let word = h.named(b"Word").unwrap();
    let gc = h.named(b"GC").unwrap();
    let codegen = h.named(b"Codegen").unwrap();
    let a = q
        .call(integer, b"fromWord:", &[abi::size(4).unwrap()])
        .unwrap();
    let b = q
        .call(integer, b"fromWord:", &[abi::size(3).unwrap()])
        .unwrap();
    let wa = q
        .call(word, b"fromBits:", &[abi::size(4).unwrap()])
        .unwrap();
    let wb = q
        .call(word, b"fromBits:", &[abi::size(3).unwrap()])
        .unwrap();
    let add = h.global(&q, b"intAddOperation").unwrap();
    let source = q
        .string(
            br#"let arithmeticProbe := [Object clone];
 [Reflect defineMethod: #return:with: on: arithmeticProbe as: [:self :left :right | left]];
 [Reflect defineMethod: #add:with: on: arithmeticProbe as: [:self :left :right | [left + right]]];
 arithmeticProbe"#,
        )
        .unwrap();
    let method = q.call(codegen, b"compileSynchronous:", &[source]).unwrap();
    q.check_result(method).unwrap();
    let captures = q.call(h.named(b"Vector").unwrap(), b"new", &[]).unwrap();
    let block = q
        .call(
            h.named(b"Block").unwrap(),
            b"fromMethod:captures:",
            &[method, captures],
        )
        .unwrap();
    let probe = q.call(block, b"value", &[]).unwrap();
    q.check_result(probe).unwrap();
    // Each result kind is checked outside timing. Inputs remain rooted for all samples.
    let mut cases = vec![
        (
            "return argument",
            probe,
            b"return:with:".as_slice(),
            vec![a, b],
            "integer",
            4,
        ),
        ("Integer equality", a, b"=", vec![b], "false", 0),
        ("Word wrappingAdd", wa, b"wrappingAdd:", vec![wb], "word", 7),
        (
            "Integer bits conversion",
            integer,
            b"integerBitsOf:",
            vec![a],
            "word",
            4,
        ),
        (
            "Integer result construction",
            integer,
            b"integerFromBits:",
            vec![wa],
            "integer",
            4,
        ),
        (
            "Integer add bits and overflow",
            integer,
            b"integerAddBits:with:",
            vec![wa, wb],
            "integer",
            7,
        ),
        (
            "Integer operation dispatch",
            add,
            b"applyInteger:with:",
            vec![wa, wb],
            "integer",
            7,
        ),
        (
            "Integer exact Error check",
            integer,
            b"integerExactError:",
            vec![b],
            "false",
            0,
        ),
        ("Word notNil", wa, b"notNil", vec![], "true", 0),
        ("Word isNil", wa, b"isNil", vec![], "false", 0),
        (
            "Block return nil",
            h.global(&q, b"intNil5").unwrap(),
            b"value:value:value:value:value:",
            vec![a, b, add, wa, wb],
            "nil",
            0,
        ),
        (
            "Block apply Integer",
            h.global(&q, b"intApplyInteger").unwrap(),
            b"value:value:value:value:value:",
            vec![a, b, add, wa, wb],
            "integer",
            7,
        ),
        (
            "Block try Integer",
            h.global(&q, b"intTryInteger").unwrap(),
            b"value:value:value:value:value:",
            vec![a, b, add, wa, wb],
            "integer",
            7,
        ),
        (
            "Block validate other",
            h.global(&q, b"intValidateOther").unwrap(),
            b"value:value:value:value:value:",
            vec![a, b, add, wa, wb],
            "integer",
            7,
        ),
        (
            "Integer binary dispatch",
            a,
            b"integerBinary:operation:",
            vec![b, add],
            "integer",
            7,
        ),
        ("Integer addition", a, b"+", vec![b], "integer", 7),
        (
            "wrapped Integer addition",
            probe,
            b"add:with:",
            vec![a, b],
            "integer",
            7,
        ),
    ];
    if std::env::var_os("ARITHMETIC_REVERSE").is_some() {
        cases.reverse();
    }
    let epoch = q.keep(h.intern(b"allocationEpoch").unwrap()).unwrap();
    let bytes = q.keep(h.intern(b"allocationBytesTotal").unwrap()).unwrap();
    let count = |key| unsafe { abi::as_size(h.id.send(h.id.root, key, &[]).unwrap()).unwrap() };
    let n = 1000;
    let mut results = vec![];
    for automatic in [true, false] {
        q.call(
            gc,
            b"automaticCollection:",
            &[abi::size(usize::from(automatic)).unwrap()],
        )
        .unwrap();
        for (label, receiver, name, arguments, kind, expected) in &cases {
            let selector = q.keep(h.intern(name).unwrap()).unwrap();
            let check = || {
                let local = h.scope();
                let v = local
                    .keep(unsafe { h.id.send(*receiver, selector, arguments).unwrap() })
                    .unwrap();
                local.check_result(v).unwrap();
                match *kind {
                    "word" => assert_eq!(local.word_bits(v).unwrap(), *expected),
                    "integer" => assert_eq!(
                        local
                            .word_bits(local.call(integer, b"wordOf:", &[v]).unwrap())
                            .unwrap(),
                        *expected
                    ),
                    _ => assert_eq!(v, h.named(kind.as_bytes()).unwrap()),
                }
            };
            check();
            for _ in 0..100 {
                let local = h.scope();
                black_box(
                    local
                        .keep(unsafe { h.id.send(*receiver, selector, arguments).unwrap() })
                        .unwrap(),
                );
            }
            let mut samples = vec![];
            for _ in 0..5 {
                q.call(gc, b"collectPrecisely", &[]).unwrap();
                let before = count(epoch);
                let charged = count(bytes);
                let start = Instant::now();
                for _ in 0..n {
                    let local = h.scope();
                    black_box(
                        local
                            .keep(unsafe { h.id.send(*receiver, selector, arguments).unwrap() })
                            .unwrap(),
                    );
                }
                let elapsed = start.elapsed();
                samples.push(serde_json::json!({"us":elapsed.as_secs_f64()*1e6/n as f64,"allocations":(count(epoch)-before) as f64/n as f64,"id_bytes":(count(bytes)-charged) as f64/n as f64}));
            }
            check();
            let result = serde_json::json!({"case":label,"automatic_gc":automatic,"repetitions":n,"samples":samples});
            println!("{result}");
            results.push(result);
        }
    }
    std::fs::write(&args[2], serde_json::to_vec_pretty(&results).unwrap()).unwrap();
}
