//! Fresh-process integration of the actual Integer, Float and String owners.
use birnam_abi::{self as abi, Oop, id_protocol::Protocol};
pub(super) unsafe fn check(
    id: &Protocol,
    send: &impl Fn(Oop, &[u8], &[Oop]) -> Result<Oop, Oop>,
    sel: &impl Fn(&[u8]) -> Oop,
    modules: [Oop; 16],
    objects: [Oop; 7],
) {
    unsafe {
        let [namespace, roots, gc, actor, base, word, string] = objects;
        let keep = |v| {
            send(roots, b"retain:", &[v]).unwrap();
            v
        };
        let actor_type = send(id.root, b"behaviorOf:", &[actor]).unwrap();
        let replacement = send(actor_type, b"lookup", &[sel(b"replacement")]).unwrap();
        let expected = send(actor, b"at:", &[abi::size(6).unwrap()]).unwrap();
        let sequence = send(id.root, b"sequence", &[]).unwrap();
        let sequence_type = send(id.root, b"behaviorOf:", &[sequence]).unwrap();
        let error_type = keep(send(sequence_type, b"delegated", &[]).unwrap());
        let error_method = send(actor_type, b"lookup", &[sel(b"errorFactory")]).unwrap();
        send(
            error_type,
            b"addMethod",
            &[sel(b"kind:message:"), error_method],
        )
        .unwrap();
        let error = send(id.root, b"sequence", &[]).unwrap();
        send(
            id.root,
            b"reassociateBehaviorOf:with:",
            &[error, error_type],
        )
        .unwrap();
        send(namespace, b"at:put:", &[sel(b"Error"), error]).unwrap();
        send(namespace, b"at:put:", &[sel(b"ErrorVt"), error_type]).unwrap();
        let args = send(id.root, b"sequence", &[namespace]).unwrap();
        let integer = id
            .send(modules[0], id.load, &[args])
            .expect("real Integer loaded");
        let integer_type = send(id.root, b"behaviorOf:", &[integer]).unwrap();
        let args = send(id.root, b"sequence", &[namespace]).unwrap();
        let float = id
            .send(modules[1], id.load, &[args])
            .expect("real Float loaded");
        let make_word = |bits: u64| {
            send(
                word,
                b"fromUnsignedLow:high:",
                &[
                    abi::size(bits as u32 as usize).unwrap(),
                    abi::size((bits >> 32) as usize).unwrap(),
                ],
            )
            .unwrap()
        };
        let bits = |v| -> u64 {
            if let Some(n) = abi::as_size(v) {
                return n as u64;
            }
            let lo = abi::as_size(send(v, b"low32", &[]).unwrap()).unwrap() as u64;
            let hi = abi::as_size(send(v, b"high32", &[]).unwrap()).unwrap() as u64;
            lo | (hi << 32)
        };
        let make_int = |v: i64| keep(send(integer, b"fromWord:", &[make_word(v as u64)]).unwrap());
        let make_float =
            |v: f64| keep(send(float, b"fromBits:", &[make_word(v.to_bits())]).unwrap());
        let int_value = |v| bits(send(integer, b"wordOf:", &[v]).unwrap()) as i64;
        let float_value = |v| f64::from_bits(bits(send(float, b"bitsOf:", &[v]).unwrap()));
        let text = |bytes: &[u8]| {
            let a: Vec<_> = bytes
                .iter()
                .map(|&b| abi::size(b as usize).unwrap())
                .collect();
            let source = send(id.root, b"sequence", &a).unwrap();
            keep(send(string, b"fromBytes:", &[source]).unwrap())
        };
        let text_bytes = |v| {
            let b = send(v, b"asBytes", &[]).unwrap();
            let len = abi::as_size(send(b, b"length", &[]).unwrap()).unwrap();
            (0..len)
                .map(|i| {
                    abi::as_size(send(b, b"byteAtWord:", &[abi::size(i).unwrap()]).unwrap())
                        .unwrap() as u8
                })
                .collect::<Vec<_>>()
        };
        let truth = send(namespace, b"at:", &[sel(b"true")]).unwrap();
        let falsity = send(namespace, b"at:", &[sel(b"false")]).unwrap();
        let nil = send(namespace, b"at:", &[sel(b"nil")]).unwrap();
        for v in [i64::MIN, i64::MAX, -1, 0, 1, 9007199254740993] {
            assert_eq!(int_value(make_int(v)), v);
        }
        let min = make_int(i64::MIN);
        let max = make_int(i64::MAX);
        let zero = make_int(0);
        let one = make_int(1);
        let two = make_int(2);
        let minus = make_int(-2);
        let three = make_int(3);
        assert_ne!(one, make_int(1));
        assert_eq!(int_value(send(two, b"+", &[three]).unwrap()), 5);
        assert_eq!(int_value(send(minus, b"*", &[three]).unwrap()), -6);
        assert_eq!(int_value(send(make_int(-7), b"/", &[three]).unwrap()), -2);
        assert_eq!(
            int_value(send(make_int(-7), b"mod:", &[three]).unwrap()),
            -1
        );
        let overflow = keep(send(max, b"+", &[one]).unwrap());
        assert_eq!(
            send(overflow, b"at:", &[abi::size(0).unwrap()]).unwrap(),
            sel(b"overflow")
        );
        let zero_error = keep(send(two, b"/", &[zero]).unwrap());
        assert_eq!(
            send(zero_error, b"at:", &[abi::size(0).unwrap()]).unwrap(),
            sel(b"zeroDivide")
        );
        assert_eq!(send(two, b"+", &[overflow]).unwrap(), overflow);
        assert_eq!(send(two, b"=", &[overflow]).unwrap(), falsity);
        assert_eq!(
            int_value(send(integer, b"compareInteger:with:", &[minus, max]).unwrap()),
            -1
        );
        assert_eq!(
            int_value(send(integer, b"integerValue:", &[min]).unwrap()),
            i64::MIN
        );
        assert_eq!(
            text_bytes(send(min, b"printString", &[]).unwrap()),
            b"-9223372036854775808"
        );
        let old_add = keep(send(integer_type, b"lookup", &[sel(b"+")]).unwrap());
        send(integer_type, b"addMethod", &[sel(b"+"), replacement]).unwrap();
        send(integer, b"installFloat:", &[float]).unwrap();
        assert_eq!(send(two, b"+", &[three]).unwrap(), expected);
        send(integer_type, b"addMethod", &[sel(b"+"), old_add]).unwrap();
        assert!(send(integer, b"installFloat:", &[float]).is_err());
        let half = make_float(0.5);
        assert_eq!(float_value(send(two, b"-", &[half]).unwrap()), 1.5);
        assert_eq!(float_value(send(half, b"+", &[two]).unwrap()), 2.5);
        assert_eq!(float_value(send(minus, b"asFloat", &[]).unwrap()), -2.0);
        assert_eq!(send(two, b">", &[half]).unwrap(), truth);
        assert_eq!(
            send(two, b"hasBootstrapArithmetic:", &[sel(b"-")]).unwrap(),
            truth
        );
        // Copied native Methods still count, while arbitrary replacements do not.
        let copied = keep(send(old_add, b"withContext:", &[integer]).unwrap());
        send(integer_type, b"addMethod", &[sel(b"+"), copied]).unwrap();
        assert_eq!(
            send(two, b"hasBootstrapArithmetic:", &[sel(b"+")]).unwrap(),
            truth
        );
        send(integer_type, b"addMethod", &[sel(b"+"), old_add]).unwrap();
        send(string, b"installNumbers:", &[integer, float]).unwrap();
        let parsed = send(text(b"-9223372036854775808"), b"asInteger", &[]).unwrap();
        assert_eq!(int_value(parsed), i64::MIN);
        assert_eq!(
            float_value(send(text(b"-1.5"), b"asNumber", &[]).unwrap()),
            -1.5
        );
        assert_eq!(int_value(send(text(b"abc"), b"size", &[]).unwrap()), 3);
        assert_eq!(
            int_value(send(text(b"a\xff"), b"byteAt:", &[one]).unwrap()),
            255
        );
        assert_eq!(send(text(b"abc"), b"byteAt:", &[minus]).unwrap(), nil);
        let raw = send(text(b"a\xff"), b"asBytes", &[]).unwrap();
        assert_eq!(int_value(send(raw, b"at:", &[one]).unwrap()), 255);
        let block_type = send(base, b"delegated", &[]).unwrap();
        let loop_value = send(actor_type, b"lookup", &[sel(b"loopValue")]).unwrap();
        send(block_type, b"addMethod", &[sel(b"value:"), loop_value]).unwrap();
        let block = keep(send(block_type, b"allocate", &[abi::size(0).unwrap()]).unwrap());
        send(
            actor,
            b"at:put:",
            &[abi::size(36).unwrap(), abi::size(0).unwrap()],
        )
        .unwrap();
        assert_eq!(send(one, b"to:do:", &[three, block]).unwrap(), one);
        assert_eq!(
            send(actor, b"at:", &[abi::size(36).unwrap()]).unwrap(),
            abi::size(3).unwrap()
        );
        // Both the iterator and numeric folding must honor the live ErrorVt binding.
        send(namespace, b"at:put:", &[sel(b"ErrorVt"), integer_type]).unwrap();
        send(
            actor,
            b"at:put:",
            &[abi::size(36).unwrap(), abi::size(0).unwrap()],
        )
        .unwrap();
        assert_eq!(int_value(send(one, b"to:do:", &[three, block]).unwrap()), 1);
        assert_eq!(
            send(actor, b"at:", &[abi::size(36).unwrap()]).unwrap(),
            abi::size(1).unwrap()
        );
        send(namespace, b"at:put:", &[sel(b"ErrorVt"), error_type]).unwrap();
        let visitor_type = send(base, b"delegated", &[]).unwrap();
        for (selector, implementation) in [
            (
                b"numericOperation:left:right:".as_slice(),
                b"numericDecline".as_slice(),
            ),
            (b"commitNumericPredicate:", b"numericCommit"),
        ] {
            let m = send(actor_type, b"lookup", &[sel(implementation)]).unwrap();
            send(visitor_type, b"addMethod", &[sel(selector), m]).unwrap();
        }
        let visitor = keep(send(visitor_type, b"allocate", &[abi::size(0).unwrap()]).unwrap());
        // Boxing both negative values can collect while the first box is retained.
        let word_type = send(id.root, b"behaviorOf:", &[word]).unwrap();
        let meta = send(id.root, b"behaviorOf:", &[word_type]).unwrap();
        let private_meta = send(meta, b"delegated", &[]).unwrap();
        let allocating = send(actor_type, b"lookup", &[sel(b"numericAllocate")]).unwrap();
        send(private_meta, b"addMethod", &[sel(b"allocate"), allocating]).unwrap();
        send(
            id.root,
            b"reassociateBehaviorOf:with:",
            &[word_type, private_meta],
        )
        .unwrap();
        let result = send(
            integer,
            b"evaluate:receiver:argument:with:",
            &[old_add, minus, minus, visitor],
        )
        .unwrap();
        assert_eq!(int_value(result), -4);
        let mixed_sub = keep(send(integer_type, b"lookup", &[sel(b"-")]).unwrap());
        let result = send(
            integer,
            b"evaluate:receiver:argument:with:",
            &[mixed_sub, two, half, visitor],
        )
        .unwrap();
        assert_eq!(float_value(result), 1.5);
        send(id.root, b"reassociateBehaviorOf:with:", &[word_type, meta]).unwrap();
        assert_eq!(
            send(integer, b"canOmitIndexing", &[]).unwrap(),
            abi::size(1).unwrap()
        );
        let fold = || {
            let epoch = send(id.root, b"behaviorEpoch", &[]).unwrap();
            send(
                integer,
                b"evaluatePredicate:then:left:right:limit:in:epoch:with:",
                &[
                    sel(b"+"),
                    sel(b"<"),
                    one,
                    two,
                    max,
                    namespace,
                    epoch,
                    visitor,
                ],
            )
            .unwrap()
        };
        assert_eq!(fold(), truth);
        let from_word = keep(send(integer_type, b"lookup", &[sel(b"fromWord:")]).unwrap());
        send(
            integer_type,
            b"addMethod",
            &[sel(b"fromWord:"), replacement],
        )
        .unwrap();
        assert_eq!(
            send(integer, b"canOmitIndexing", &[]).unwrap(),
            abi::size(0).unwrap()
        );
        assert!(fold().is_null());
        assert_eq!(send(two, b"+", &[three]).unwrap(), expected);
        send(integer_type, b"addMethod", &[sel(b"fromWord:"), from_word]).unwrap();
        send(namespace, b"at:put:", &[sel(b"ErrorVt"), integer_type]).unwrap();
        assert!(fold().is_null());
        send(namespace, b"at:put:", &[sel(b"ErrorVt"), error_type]).unwrap();
        send(integer_type, b"addMethod", &[sel(b"+"), replacement]).unwrap();
        assert!(fold().is_null());
        assert_eq!(
            send(two, b"hasBootstrapArithmetic:", &[sel(b"+")]).unwrap(),
            falsity
        );
        send(integer_type, b"addMethod", &[sel(b"+"), old_add]).unwrap();
        send(gc, b"collectPrecisely", &[]).unwrap();
        assert_eq!(int_value(min), i64::MIN);

        let collection_parent = keep(send(base, b"delegated", &[]).unwrap());
        send(
            collection_parent,
            b"addMethod",
            &[sel(b"collectionProbe"), replacement],
        )
        .unwrap();
        send(
            namespace,
            b"at:put:",
            &[sel(b"CollectionVt"), collection_parent],
        )
        .unwrap();
        let args = send(id.root, b"sequence", &[namespace]).unwrap();
        let vector = id
            .send(modules[2], id.load, &[args])
            .expect("real Vector loaded");
        assert_eq!(send(vector, b"collectionProbe", &[]).unwrap(), expected);
        let vector_type = send(id.root, b"behaviorOf:", &[vector]).unwrap();
        let node_type = send(namespace, b"at:", &[sel(b"VectorNode")]).unwrap();
        let mut current = vector;
        let mut current_token = send(roots, b"retain:", &[current]).unwrap();
        let mut snapshots = Vec::new();
        for i in 0..1070 {
            let value = make_int(i);
            let next = send(current, b"add:", &[value]).unwrap();
            let token = send(roots, b"retain:", &[next]).unwrap();
            send(roots, b"release:", &[current_token]).unwrap();
            current = next;
            current_token = token;
            if [1, 31, 32, 33, 1024, 1056, 1057].contains(&(i + 1)) {
                keep(current);
                snapshots.push((i + 1, current));
                send(gc, b"collectPrecisely", &[]).unwrap();
            }
        }
        assert_eq!(int_value(send(current, b"size", &[]).unwrap()), 1070);
        for i in [0, 31, 32, 33, 1023, 1024, 1055, 1056, 1069] {
            assert_eq!(
                int_value(send(current, b"atWord:", &[abi::size(i).unwrap()]).unwrap()),
                i as i64
            );
        }
        for (length, snapshot) in snapshots {
            assert_eq!(
                send(snapshot, b"sizeWord", &[]).unwrap(),
                abi::size(length as usize).unwrap()
            );
            assert_eq!(
                int_value(
                    send(
                        snapshot,
                        b"atWord:",
                        &[abi::size(length as usize - 1).unwrap()]
                    )
                    .unwrap()
                ),
                length - 1
            );
        }
        assert_eq!(send(current, b"at:", &[minus]).unwrap(), nil);
        assert_eq!(
            send(current, b"atWord:", &[abi::size(1070).unwrap()]).unwrap(),
            nil
        );
        // Each new tree allocation can collect, including nested sort work.
        let vector_meta = send(id.root, b"behaviorOf:", &[vector_type]).unwrap();
        let private_meta = send(vector_meta, b"delegated", &[]).unwrap();
        let allocating = send(actor_type, b"lookup", &[sel(b"numericAllocate")]).unwrap();
        send(private_meta, b"addMethod", &[sel(b"allocate"), allocating]).unwrap();
        send(
            id.root,
            b"reassociateBehaviorOf:with:",
            &[vector_type, private_meta],
        )
        .unwrap();
        send(
            id.root,
            b"reassociateBehaviorOf:with:",
            &[node_type, private_meta],
        )
        .unwrap();
        let grown = keep(send(current, b"add:", &[minus]).unwrap());
        assert_eq!(
            int_value(send(grown, b"atWord:", &[abi::size(1070).unwrap()]).unwrap()),
            -2
        );
        let two_b = make_int(2);
        let mut small = vector;
        for v in [two, one, two_b] {
            small = keep(send(small, b"add:", &[v]).unwrap());
        }
        let copied = keep(send(small, b"copyFrom:to:", &[one, three]).unwrap());
        assert_eq!(
            send(copied, b"atWord:", &[abi::size(0).unwrap()]).unwrap(),
            one
        );
        let reversed = keep(send(small, b"reversed", &[]).unwrap());
        assert_eq!(
            send(reversed, b"atWord:", &[abi::size(0).unwrap()]).unwrap(),
            two_b
        );
        let concat = keep(send(small, b",", &[reversed]).unwrap());
        assert_eq!(int_value(send(concat, b"size", &[]).unwrap()), 6);
        assert_eq!(
            int_value(send(small, b"indexOf:", &[make_int(2)]).unwrap()),
            0
        );
        let sorter_type = send(base, b"delegated", &[]).unwrap();
        let key = send(actor_type, b"lookup", &[sel(b"vectorKey")]).unwrap();
        send(sorter_type, b"addMethod", &[sel(b"value:"), key]).unwrap();
        let sorter = keep(send(sorter_type, b"allocate", &[abi::size(0).unwrap()]).unwrap());
        send(
            actor,
            b"at:put:",
            &[abi::size(36).unwrap(), abi::size(0).unwrap()],
        )
        .unwrap();
        send(actor, b"at:put:", &[abi::size(37).unwrap(), small]).unwrap();
        for selector in [b"sortBy:".as_slice(), b"mergeSortBy:"] {
            let sorted = keep(send(small, selector, &[sorter]).unwrap());
            for (i, value) in [one, two, two_b].into_iter().enumerate() {
                assert_eq!(
                    send(sorted, b"atWord:", &[abi::size(i).unwrap()]).unwrap(),
                    value
                );
            }
        }
        assert_eq!(
            send(actor, b"at:", &[abi::size(36).unwrap()]).unwrap(),
            abi::size(6).unwrap()
        );
        send(
            actor,
            b"at:put:",
            &[abi::size(37).unwrap(), core::ptr::null_mut()],
        )
        .unwrap();
        let reducer_type = send(base, b"delegated", &[]).unwrap();
        let sum = send(actor_type, b"lookup", &[sel(b"sumValues")]).unwrap();
        send(reducer_type, b"addMethod", &[sel(b"value:value:"), sum]).unwrap();
        let reducer = keep(send(reducer_type, b"allocate", &[abi::size(0).unwrap()]).unwrap());
        assert_eq!(
            int_value(send(small, b"inject:into:", &[zero, reducer]).unwrap()),
            5
        );
        let gc_type = send(id.root, b"behaviorOf:", &[gc]).unwrap();
        let original_charge =
            send(gc_type, b"lookup", &[sel(b"chargeExternalAllocation:")]).unwrap();
        let collecting_charge = send(actor_type, b"lookup", &[sel(b"externalCharge")]).unwrap();
        send(
            gc_type,
            b"addMethod",
            &[sel(b"chargeExternalAllocation:"), collecting_charge],
        )
        .unwrap();
        assert_eq!(
            text_bytes(send(small, b"printString", &[]).unwrap()),
            b"#[2 1 2]"
        );
        send(
            gc_type,
            b"addMethod",
            &[sel(b"chargeExternalAllocation:"), original_charge],
        )
        .unwrap();
        send(string, b"installCollections:", &[vector]).unwrap();
        let parts = keep(send(text(b"a,b,"), b"split:", &[text(b",")]).unwrap());
        assert_eq!(int_value(send(parts, b"size", &[]).unwrap()), 3);
        for (i, value) in [b"a".as_slice(), b"b", b""].into_iter().enumerate() {
            assert_eq!(
                text_bytes(send(parts, b"atWord:", &[abi::size(i).unwrap()]).unwrap()),
                value
            );
        }
        let new_method = keep(send(vector_type, b"lookup", &[sel(b"new")]).unwrap());
        send(vector_type, b"addMethod", &[sel(b"new"), replacement]).unwrap();
        assert!(send(text(b"a,b"), b"split:", &[text(b",")]).is_err());
        send(vector_type, b"addMethod", &[sel(b"new"), new_method]).unwrap();
        send(
            id.root,
            b"reassociateBehaviorOf:with:",
            &[vector_type, vector_meta],
        )
        .unwrap();
        send(
            id.root,
            b"reassociateBehaviorOf:with:",
            &[node_type, vector_meta],
        )
        .unwrap();
        send(roots, b"release:", &[current_token]).unwrap();
        send(gc, b"collectPrecisely", &[]).unwrap();
        println!(
            "Vector persistent tree boundaries, nested sort roots, callbacks, printing and real String split: passed"
        );

        let args = send(id.root, b"sequence", &[namespace]).unwrap();
        let map = id
            .send(modules[3], id.load, &[args])
            .expect("real Map loaded");
        assert_eq!(send(map, b"collectionProbe", &[]).unwrap(), expected);
        let map_type = send(id.root, b"behaviorOf:", &[map]).unwrap();
        let map_node = send(namespace, b"at:", &[sel(b"MapNode")]).unwrap();
        let single = keep(send(map, b"at:put:", &[one, two]).unwrap());
        assert_eq!(
            send(single, b"at:put:", &[make_int(1), two]).unwrap(),
            single
        );
        let replaced = keep(send(single, b"at:put:", &[make_int(1), three]).unwrap());
        assert_eq!(send(single, b"at:", &[one]).unwrap(), two);
        assert_eq!(send(replaced, b"at:", &[one]).unwrap(), three);
        let keys = send(replaced, b"keys", &[]).unwrap();
        assert_eq!(
            send(keys, b"atWord:", &[abi::size(0).unwrap()]).unwrap(),
            one
        );
        let values = send(replaced, b"values", &[]).unwrap();
        assert_eq!(
            send(values, b"atWord:", &[abi::size(0).unwrap()]).unwrap(),
            three
        );
        assert_eq!(send(single, b"includesKey:", &[one]).unwrap(), truth);
        assert_eq!(send(single, b"includesKey:", &[three]).unwrap(), falsity);
        let absent_type = send(base, b"delegated", &[]).unwrap();
        send(absent_type, b"addMethod", &[sel(b"value"), replacement]).unwrap();
        let absent = keep(send(absent_type, b"allocate", &[abi::size(0).unwrap()]).unwrap());
        assert_eq!(
            send(single, b"at:ifAbsent:", &[three, absent]).unwrap(),
            expected
        );
        assert_eq!(send(single, b"at:ifAbsent:", &[one, absent]).unwrap(), two);
        let binary_key = text(b"x\0y");
        let with_string = keep(send(single, b"at:put:", &[binary_key, minus]).unwrap());
        assert_eq!(send(with_string, b"at:", &[text(b"x\0y")]).unwrap(), minus);
        let float_zero = make_float(0.0);
        let with_float = keep(send(with_string, b"at:put:", &[float_zero, max]).unwrap());
        assert_eq!(send(with_float, b"at:", &[make_float(-0.0)]).unwrap(), max);
        let nan = make_float(f64::NAN);
        let with_nan = keep(send(with_float, b"at:put:", &[nan, three]).unwrap());
        assert_eq!(send(with_nan, b"at:", &[nan]).unwrap(), three);
        assert_eq!(
            send(with_nan, b"at:", &[make_float(f64::NAN)]).unwrap(),
            nil
        );
        let mut large = map;
        let mut large_token = send(roots, b"retain:", &[large]).unwrap();
        for i in 0..128 {
            let next = send(large, b"at:put:", &[make_int(i), make_int(i * 2)]).unwrap();
            let token = send(roots, b"retain:", &[next]).unwrap();
            send(roots, b"release:", &[large_token]).unwrap();
            large = next;
            large_token = token;
        }
        assert_eq!(int_value(send(large, b"size", &[]).unwrap()), 128);
        send(gc, b"collectPrecisely", &[]).unwrap();
        for i in 0..128 {
            assert_eq!(
                int_value(send(large, b"at:", &[make_int(i)]).unwrap()),
                i * 2
            );
        }
        assert_eq!(int_value(send(single, b"size", &[]).unwrap()), 1);
        let map_meta = send(id.root, b"behaviorOf:", &[map_type]).unwrap();
        let private_meta = send(map_meta, b"delegated", &[]).unwrap();
        send(private_meta, b"addMethod", &[sel(b"allocate"), allocating]).unwrap();
        send(
            id.root,
            b"reassociateBehaviorOf:with:",
            &[map_type, private_meta],
        )
        .unwrap();
        send(
            id.root,
            b"reassociateBehaviorOf:with:",
            &[map_node, private_meta],
        )
        .unwrap();
        let key_type = keep(send(base, b"delegated", &[]).unwrap());
        let hashing = send(actor_type, b"lookup", &[sel(b"collisionHash")]).unwrap();
        send(key_type, b"addMethod", &[sel(b"collectionHash"), hashing]).unwrap();
        let mut collisions = map;
        let mut keys = Vec::new();
        for value in [one, two, three] {
            let key = keep(send(key_type, b"allocate", &[abi::size(0).unwrap()]).unwrap());
            keys.push(key);
            collisions = keep(send(collisions, b"at:put:", &[key, value]).unwrap());
        }
        assert_eq!(int_value(send(collisions, b"size", &[]).unwrap()), 3);
        for (&key, value) in keys.iter().zip([one, two, three]) {
            assert_eq!(send(collisions, b"at:", &[key]).unwrap(), value);
        }
        // Changing hashing requires explicit rehash; identity and aliases remain.
        send(
            actor,
            b"at:put:",
            &[abi::size(38).unwrap(), abi::size(1).unwrap()],
        )
        .unwrap();
        assert_eq!(send(collisions, b"at:", &[keys[0]]).unwrap(), nil);
        send(id.root, b"ensurePrivateVtableFor:", &[collisions]).unwrap();
        assert_eq!(
            send(map, b"rehashImage:", &[collisions]).unwrap(),
            collisions
        );
        for (&key, value) in keys.iter().zip([one, two, three]) {
            assert_eq!(send(collisions, b"at:", &[key]).unwrap(), value);
        }
        let collision_values = keep(send(collisions, b"values", &[]).unwrap());
        assert_eq!(int_value(send(collision_values, b"size", &[]).unwrap()), 3);
        assert_eq!(
            int_value(send(collisions, b"inject:into:", &[zero, reducer]).unwrap()),
            6
        );
        assert_eq!(send(collisions, b"do:", &[block]).unwrap(), collisions);
        assert_eq!(
            send(collisions, b"keysAndValuesDo:", &[absent]).is_err(),
            true
        );
        send(
            gc_type,
            b"addMethod",
            &[sel(b"chargeExternalAllocation:"), collecting_charge],
        )
        .unwrap();
        assert_eq!(
            text_bytes(send(single, b"printString", &[]).unwrap()),
            b"{1 -> 2}"
        );
        send(
            gc_type,
            b"addMethod",
            &[sel(b"chargeExternalAllocation:"), original_charge],
        )
        .unwrap();
        let vector_new = keep(send(vector_type, b"lookup", &[sel(b"new")]).unwrap());
        send(vector_type, b"addMethod", &[sel(b"new"), replacement]).unwrap();
        assert!(send(single, b"keys", &[]).is_err());
        send(vector_type, b"addMethod", &[sel(b"new"), vector_new]).unwrap();
        send(
            id.root,
            b"reassociateBehaviorOf:with:",
            &[map_type, map_meta],
        )
        .unwrap();
        send(
            id.root,
            b"reassociateBehaviorOf:with:",
            &[map_node, map_meta],
        )
        .unwrap();
        send(roots, b"release:", &[large_token]).unwrap();
        send(gc, b"collectPrecisely", &[]).unwrap();
        println!(
            "Map persistence, live key protocols, collisions, in-place rehash, Vector results and callback GC: passed"
        );
        let args = send(id.root, b"sequence", &[namespace]).unwrap();
        let system = id
            .send(modules[4], id.load, &[args])
            .expect("System constructed by Prelude");
        let system_type = send(namespace, b"at:", &[sel(b"SystemVt")]).unwrap();
        assert_eq!(
            send(id.root, b"behaviorOf:", &[system]).unwrap(),
            system_type
        );
        assert_eq!(send(id.root, b"ownerOf:", &[system_type]).unwrap(), system);
        assert_eq!(
            send(id.root, b"allocationSizeOf:", &[system]).unwrap(),
            abi::size(0).unwrap()
        );
        send(namespace, b"at:put:", &[sel(b"System"), nil]).unwrap();
        send(gc, b"collectPrecisely", &[]).unwrap();
        let args = send(id.root, b"sequence", &[namespace]).unwrap();
        assert_eq!(id.send(modules[4], id.load, &[args]).unwrap(), system);
        send(namespace, b"at:put:", &[sel(b"System"), system]).unwrap();
        println!(
            "Prelude System construction, ownership and repeated-load lifetime after namespace rebinding: passed"
        );

        let args = send(id.root, b"sequence", &[namespace]).unwrap();
        assert!(id.send(modules[5], id.load, &[args]).unwrap().is_null());
        let reader = keep(send(system_type, b"lookup", &[sel(b"readLine")]).unwrap());
        send(system_type, b"removeMethod:", &[sel(b"readLine")]).unwrap();
        send(gc, b"collectPrecisely", &[]).unwrap();
        let args = send(id.root, b"sequence", &[namespace]).unwrap();
        assert!(id.send(modules[5], id.load, &[args]).unwrap().is_null());
        assert!(
            send(system_type, b"lookup", &[sel(b"readLine")])
                .unwrap()
                .is_null()
        );
        send(
            gc_type,
            b"addMethod",
            &[sel(b"chargeExternalAllocation:"), collecting_charge],
        )
        .unwrap();
        let no_args = send(id.root, b"sequence", &[]).unwrap();
        let first = send(reader, b"invokeOn:arguments:", &[system, no_args]).unwrap();
        send(system_type, b"addMethod", &[sel(b"readLine"), reader]).unwrap();
        if std::env::args().nth(2).as_deref() == Some("numeric-input-error") {
            assert_eq!(
                send(first, b"at:", &[abi::size(0).unwrap()]).unwrap(),
                sel(b"ioError")
            );
            assert_eq!(
                text_bytes(send(first, b"at:", &[abi::size(1).unwrap()]).unwrap()),
                b"stdin read failed"
            );
            keep(error);
            send(
                namespace,
                b"at:put:",
                &[sel(b"Error"), core::ptr::null_mut()],
            )
            .unwrap();
            assert_eq!(send(system, b"readLine", &[]).unwrap(), nil);
            send(namespace, b"at:put:", &[sel(b"Error"), error]).unwrap();
            println!("SYSTEM_INPUT_ERROR_OK");
            return;
        }
        assert_eq!(text_bytes(first), b"a\0\xff\r");
        assert_eq!(send(system, b"readLine", &[one]).unwrap(), nil);
        assert_eq!(text_bytes(send(system, b"readLine", &[]).unwrap()), b"");
        let string_type = send(id.root, b"behaviorOf:", &[string]).unwrap();
        let factory = keep(send(string_type, b"lookup", &[sel(b"fromBytes:")]).unwrap());
        send(
            string_type,
            b"addMethod",
            &[sel(b"fromBytes:"), replacement],
        )
        .unwrap();
        assert_eq!(send(system, b"readLine", &[]).unwrap(), expected);
        send(string_type, b"addMethod", &[sel(b"fromBytes:"), factory]).unwrap();
        assert_eq!(text_bytes(send(system, b"readLine", &[]).unwrap()), b"last");
        assert_eq!(send(system, b"readLine", &[]).unwrap(), nil);
        send(
            gc_type,
            b"addMethod",
            &[sel(b"chargeExternalAllocation:"), original_charge],
        )
        .unwrap();
        println!(
            "System readLine binary input, EOF, detached Method, live constructor and callback GC: passed"
        );

        let args = send(id.root, b"sequence", &[namespace]).unwrap();
        assert!(id.send(modules[6], id.load, &[args]).unwrap().is_null());
        let getter = keep(send(system_type, b"lookup", &[sel(b"getEnv:")]).unwrap());
        let present = text(b"BIRNAM_OOP_TEST_PRESENT");
        send(
            gc_type,
            b"addMethod",
            &[sel(b"chargeExternalAllocation:"), collecting_charge],
        )
        .unwrap();
        assert_eq!(
            text_bytes(send(system, b"getEnv:", &[present]).unwrap()),
            "héllo".as_bytes()
        );
        assert_eq!(
            text_bytes(send(system, b"getEnv:", &[text(b"BIRNAM_OOP_TEST_EMPTY")]).unwrap()),
            b""
        );
        assert_eq!(
            send(system, b"getEnv:", &[text(b"BIRNAM_OOP_TEST_MISSING")]).unwrap(),
            nil
        );
        assert_eq!(send(system, b"getEnv:", &[]).unwrap(), nil);
        assert_eq!(send(system, b"getEnv:", &[one]).unwrap(), nil);
        for name in [text(b"\xff"), text(b"BIRNAM_OOP_TEST_BINARY")] {
            let e = keep(send(system, b"getEnv:", &[name]).unwrap());
            assert_eq!(
                send(e, b"at:", &[abi::size(0).unwrap()]).unwrap(),
                sel(b"ioError")
            );
            assert_eq!(
                text_bytes(send(e, b"at:", &[abi::size(1).unwrap()]).unwrap()),
                b"could not read environment"
            );
        }
        let original_bytes = keep(send(string_type, b"lookup", &[sel(b"bytes")]).unwrap());
        let absent_value = send(actor_type, b"lookup", &[sel(b"absentValue")]).unwrap();
        send(string_type, b"addMethod", &[sel(b"bytes"), absent_value]).unwrap();
        assert_eq!(send(system, b"getEnv:", &[present]).unwrap(), nil);
        assert!(send(system, b"getEnv:", &[core::ptr::null_mut()]).is_err());

        send(string_type, b"addMethod", &[sel(b"bytes"), replacement]).unwrap();
        assert_eq!(send(system, b"getEnv:", &[present]).unwrap(), nil);
        send(string_type, b"addMethod", &[sel(b"bytes"), original_bytes]).unwrap();
        send(id.root, b"ensurePrivateVtableFor:", &[present]).unwrap();
        assert_eq!(send(system, b"getEnv:", &[present]).unwrap(), nil);
        send(
            id.root,
            b"reassociateBehaviorOf:with:",
            &[present, string_type],
        )
        .unwrap();
        send(
            string_type,
            b"addMethod",
            &[sel(b"fromBytes:"), replacement],
        )
        .unwrap();
        assert_eq!(send(system, b"getEnv:", &[present]).unwrap(), expected);
        send(string_type, b"addMethod", &[sel(b"fromBytes:"), factory]).unwrap();
        let malformed = text(b"\xff");
        send(string_type,b"addMethod",&[sel(b"savedStringConstructor:"),factory]).unwrap();
        send(
            string_type,
            b"addMethod",
            &[sel(b"fromBytes:"), absent_value],
        )
        .unwrap();
        assert!(send(system, b"getEnv:", &[malformed]).is_err());
        send(string_type, b"addMethod", &[sel(b"fromBytes:"), factory]).unwrap();

        send(actor, b"at:put:", &[abi::size(39).unwrap(), factory]).unwrap();
        send(actor, b"at:put:", &[abi::size(40).unwrap(), error]).unwrap();
        send(
            namespace,
            b"at:put:",
            &[sel(b"Error"), core::ptr::null_mut()],
        )
        .unwrap();
        let publish = send(actor_type, b"lookup", &[sel(b"publishErrorString")]).unwrap();
        send(string_type, b"addMethod", &[sel(b"fromBytes:"), publish]).unwrap();
        let e = send(system, b"getEnv:", &[malformed]).unwrap();
        assert_eq!(
            send(e, b"at:", &[abi::size(0).unwrap()]).unwrap(),
            sel(b"ioError")
        );
        send(string_type, b"addMethod", &[sel(b"fromBytes:"), factory]).unwrap();
        send(system_type, b"removeMethod:", &[sel(b"getEnv:")]).unwrap();
        send(gc, b"collectPrecisely", &[]).unwrap();
        let args = send(id.root, b"sequence", &[namespace]).unwrap();
        assert!(id.send(modules[6], id.load, &[args]).unwrap().is_null());
        assert!(
            send(system_type, b"lookup", &[sel(b"getEnv:")])
                .unwrap()
                .is_null()
        );
        let args = send(id.root, b"sequence", &[present]).unwrap();
        assert_eq!(
            text_bytes(send(getter, b"invokeOn:arguments:", &[system, args]).unwrap()),
            "héllo".as_bytes()
        );
        send(system_type, b"addMethod", &[sel(b"getEnv:"), getter]).unwrap();
        send(
            gc_type,
            b"addMethod",
            &[sel(b"chargeExternalAllocation:"), original_charge],
        )
        .unwrap();
        println!(
            "Environment Oop byte providers, exact names, Unicode errors, lazy Error publication and captured Methods: passed"
        );

        let args = send(id.root, b"sequence", &[namespace]).unwrap();
        assert!(id.send(modules[7], id.load, &[args]).unwrap().is_null());
        let printer = keep(send(system_type, b"lookup", &[sel(b"println:")]).unwrap());
        send(system_type, b"removeMethod:", &[sel(b"println:")]).unwrap();
        send(gc, b"collectPrecisely", &[]).unwrap();
        let args = send(id.root, b"sequence", &[namespace]).unwrap();
        id.send(modules[7], id.load, &[args]).unwrap();
        assert!(
            send(system_type, b"lookup", &[sel(b"println:")])
                .unwrap()
                .is_null()
        );
        send(
            gc_type,
            b"addMethod",
            &[sel(b"chargeExternalAllocation:"), collecting_charge],
        )
        .unwrap();
        let args = send(id.root, b"sequence", &[text(b"OOP_STDOUT:\0\xff")]).unwrap();
        assert_eq!(
            send(printer, b"invokeOn:arguments:", &[system, args]).unwrap(),
            system
        );
        send(system_type, b"addMethod", &[sel(b"println:"), printer]).unwrap();
        assert_eq!(
            send(system, b"print:", &[text(b"OOP_NUMBER:")]).unwrap(),
            system
        );
        assert_eq!(send(system, b"println:", &[two]).unwrap(), system);
        assert_eq!(
            send(system, b"printErrorText:", &[text(b"OOP_STDERR:\0\xff")]).unwrap(),
            system
        );
        let rendered_type = keep(send(base, b"delegated", &[]).unwrap());
        let render = send(actor_type, b"lookup", &[sel(b"renderOutput")]).unwrap();
        send(rendered_type, b"addMethod", &[sel(b"printString"), render]).unwrap();
        let rendered = keep(send(rendered_type, b"allocate", &[abi::size(0).unwrap()]).unwrap());
        keep(expected);
        send(
            actor,
            b"at:put:",
            &[abi::size(6).unwrap(), text(b"OOP_REENTRY;")],
        )
        .unwrap();
        assert_eq!(send(system, b"println:", &[rendered]).unwrap(), system);
        send(actor, b"at:put:", &[abi::size(6).unwrap(), expected]).unwrap();
        assert_eq!(send(system, b"print:", &[]).unwrap(), nil);
        assert!(send(system, b"print:", &[core::ptr::null_mut()]).is_err());
        assert_eq!(send(system, b"printErrorText:", &[two]).unwrap(), nil);
        send(
            rendered_type,
            b"addMethod",
            &[sel(b"printString"), replacement],
        )
        .unwrap();
        assert_eq!(send(system, b"print:", &[rendered]).unwrap(), nil);
        let rejected = send(actor_type, b"lookup", &[sel(b"rethrow:")]).unwrap();
        send(
            rendered_type,
            b"addMethod",
            &[sel(b"printString"), rejected],
        )
        .unwrap();
        assert!(send(system, b"print:", &[rendered]).is_err());
        send(string_type, b"addMethod", &[sel(b"bytes"), absent_value]).unwrap();
        assert_eq!(send(system, b"print:", &[present]).unwrap(), nil);
        send(string_type, b"addMethod", &[sel(b"bytes"), original_bytes]).unwrap();
        send(id.root, b"ensurePrivateVtableFor:", &[present]).unwrap();
        assert_eq!(send(system, b"print:", &[present]).unwrap(), nil);
        send(
            id.root,
            b"reassociateBehaviorOf:with:",
            &[present, string_type],
        )
        .unwrap();
        send(
            gc_type,
            b"addMethod",
            &[sel(b"chargeExternalAllocation:"), original_charge],
        )
        .unwrap();
        println!(
            "System output binary/UTF-8-lossy streams, reentrant rendering, live bytes, failure propagation and captured Methods: passed"
        );

        let args = send(id.root, b"sequence", &[namespace]).unwrap();
        assert!(id.send(modules[8], id.load, &[args]).unwrap().is_null());
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("birnam-oop-files-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let file = directory.join("binary.dat");
        let filename = text(file.to_str().unwrap().as_bytes());
        let contents = text(b"a\0\xff\r\n");
        send(
            gc_type,
            b"addMethod",
            &[sel(b"chargeExternalAllocation:"), collecting_charge],
        )
        .unwrap();
        assert_eq!(
            send(system, b"writeFile:contents:", &[filename, contents]).unwrap(),
            system
        );
        assert_eq!(std::fs::read(&file).unwrap(), b"a\0\xff\r\n");
        let reader = keep(send(system_type, b"lookup", &[sel(b"readFile:")]).unwrap());
        send(system_type, b"removeMethod:", &[sel(b"readFile:")]).unwrap();
        send(gc, b"collectPrecisely", &[]).unwrap();
        let args = send(id.root, b"sequence", &[namespace]).unwrap();
        id.send(modules[8], id.load, &[args]).unwrap();
        assert!(
            send(system_type, b"lookup", &[sel(b"readFile:")])
                .unwrap()
                .is_null()
        );
        let args = send(id.root, b"sequence", &[filename]).unwrap();
        assert_eq!(
            text_bytes(send(reader, b"invokeOn:arguments:", &[system, args]).unwrap()),
            b"a\0\xff\r\n"
        );
        send(system_type, b"addMethod", &[sel(b"readFile:"), reader]).unwrap();
        let bytes_data = |v| {
            let n = abi::as_size(send(v, b"length", &[]).unwrap()).unwrap();
            (0..n)
                .map(|i| {
                    abi::as_size(send(v, b"byteAtWord:", &[abi::size(i).unwrap()]).unwrap())
                        .unwrap() as u8
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            bytes_data(send(system, b"embedFile:", &[filename]).unwrap()),
            b"a\0\xff\r\n"
        );
        let snapshot = keep(send(system, b"readFileSnapshot:", &[filename]).unwrap());
        assert_eq!(int_value(send(snapshot, b"size", &[]).unwrap()), 2);
        assert_eq!(
            bytes_data(send(snapshot, b"atWord:", &[abi::size(0).unwrap()]).unwrap()),
            file.to_str().unwrap().as_bytes()
        );
        assert_eq!(
            bytes_data(send(snapshot, b"atWord:", &[abi::size(1).unwrap()]).unwrap()),
            b"a\0\xff\r\n"
        );
        send(actor, b"at:put:", &[abi::size(41).unwrap(), original_bytes]).unwrap();
        send(actor, b"at:put:", &[abi::size(42).unwrap(), filename]).unwrap();
        send(actor, b"at:put:", &[abi::size(44).unwrap(), contents]).unwrap();
        let file_bytes = send(actor_type, b"lookup", &[sel(b"fileBytes")]).unwrap();
        send(string_type,b"addMethod",&[sel(b"savedStringBytes"),original_bytes]).unwrap();
        send(string_type, b"addMethod", &[sel(b"bytes"), file_bytes]).unwrap();
        assert_eq!(
            send(system, b"writeFile:contents:", &[filename, contents]).unwrap(),
            system
        );
        assert!(!directory.join("binary.dat.redirect").exists());
        send(
            actor,
            b"at:put:",
            &[abi::size(45).unwrap(), abi::size(1).unwrap()],
        )
        .unwrap();
        assert!(send(system, b"writeFile:contents:", &[malformed, contents]).is_err());
        send(
            actor,
            b"at:put:",
            &[abi::size(45).unwrap(), abi::size(0).unwrap()],
        )
        .unwrap();
        send(string_type, b"addMethod", &[sel(b"bytes"), original_bytes]).unwrap();
        assert_eq!(
            send(system, b"writeFile:contents:", &[malformed, one]).unwrap(),
            nil
        );
        let changed = keep(
            send(
                system,
                b"removeFile:ifContents:",
                &[filename, text(b"different")],
            )
            .unwrap(),
        );
        assert_eq!(
            text_bytes(send(changed, b"at:", &[abi::size(1).unwrap()]).unwrap()),
            b"file changed since candidate admission"
        );
        assert!(file.exists());
        assert_eq!(
            send(system, b"removeFile:ifContents:", &[filename, contents]).unwrap(),
            system
        );
        assert!(!file.exists());
        assert_eq!(
            send(system, b"removeFile:ifContents:", &[filename, contents]).unwrap(),
            system
        );
        assert_eq!(
            send(system, b"readFileSnapshot:", &[filename]).unwrap(),
            nil
        );
        let missing = keep(send(system, b"readFile:", &[filename]).unwrap());
        assert_eq!(
            send(missing, b"at:", &[abi::size(0).unwrap()]).unwrap(),
            sel(b"ioError")
        );
        assert_eq!(send(system, b"readFile:", &[one]).unwrap(), nil);
        let wrong = keep(send(system, b"embedFile:", &[one]).unwrap());
        assert_eq!(
            text_bytes(send(wrong, b"at:", &[abi::size(1).unwrap()]).unwrap()),
            b"embedFile: expected a String path"
        );
        let oversized = directory.join("too-large.bin");
        std::fs::File::create(&oversized)
            .unwrap()
            .set_len(16 * 1024 * 1024 + 1)
            .unwrap();
        let too_large = keep(
            send(
                system,
                b"embedFile:",
                &[text(oversized.to_str().unwrap().as_bytes())],
            )
            .unwrap(),
        );
        assert_eq!(
            text_bytes(send(too_large, b"at:", &[abi::size(1).unwrap()]).unwrap()),
            b"embedFile: could not read the file within the 16 MiB limit"
        );
        std::fs::write(&file, b"live factory").unwrap();
        send(
            string_type,
            b"addMethod",
            &[sel(b"fromBytes:"), replacement],
        )
        .unwrap();
        assert_eq!(send(system, b"readFile:", &[filename]).unwrap(), expected);
        send(string_type, b"addMethod", &[sel(b"fromBytes:"), factory]).unwrap();
        let bytes_factory = send(namespace, b"at:", &[sel(b"Bytes")]).unwrap();
        let bytes_type = send(id.root, b"behaviorOf:", &[bytes_factory]).unwrap();
        let bytes_constructor = keep(send(bytes_type, b"lookup", &[sel(b"fromBytes:")]).unwrap());
        // Keep filename transfer independent of the result factory under test.
        // String itself supplies byte application, so returning self is valid.
        let yourself = send(base, b"lookup", &[sel(b"yourself")]).unwrap();
        send(string_type, b"addMethod", &[sel(b"bytes"), yourself]).unwrap();
        send(bytes_type, b"addMethod", &[sel(b"fromBytes:"), replacement]).unwrap();
        assert_eq!(send(system, b"embedFile:", &[filename]).unwrap(), expected);
        send(string_type, b"addMethod", &[sel(b"bytes"), original_bytes]).unwrap();
        send(
            bytes_type,
            b"addMethod",
            &[sel(b"fromBytes:"), bytes_constructor],
        )
        .unwrap();
        send(
            gc_type,
            b"addMethod",
            &[sel(b"chargeExternalAllocation:"), original_charge],
        )
        .unwrap();
        std::fs::remove_dir_all(&directory).unwrap();
        println!(
            "File I/O binary transfers, filename snapshots, error ordering, bounded embedding, guarded removal and constructor GC: passed"
        );

        let args = send(id.root, b"sequence", &[namespace]).unwrap();
        let peg = id.send(modules[9], id.load, &[args]).expect("PEG loaded");
        let peg_types = [b"PegPatternVt".as_slice(), b"PegGrammarVt", b"PegCstVt"]
            .map(|name| send(namespace, b"at:", &[sel(name)]).unwrap());
        let peg_meta = send(id.root, b"behaviorOf:", &[peg_types[0]]).unwrap();
        let private_meta = send(peg_meta, b"delegated", &[]).unwrap();
        send(private_meta, b"addMethod", &[sel(b"allocate"), allocating]).unwrap();
        for vt in peg_types {
            send(id.root, b"reassociateBehaviorOf:with:", &[vt, private_meta]).unwrap();
        }
        let literal = keep(send(peg, b"literal:", &[text(b"ab\0\xff")]).unwrap());
        let grammar = keep(send(peg, b"grammar", &[]).unwrap());
        let root_name = text(b"root");
        send(grammar, b"rule:is:", &[root_name, literal]).unwrap();
        let tree = keep(send(grammar, b"parse:from:", &[text(b"ab\0\xff!"), root_name]).unwrap());
        assert_eq!(text_bytes(send(tree, b"ruleName", &[]).unwrap()), b"root");
        assert_eq!(int_value(send(tree, b"start", &[]).unwrap()), 0);
        assert_eq!(int_value(send(tree, b"end", &[]).unwrap()), 4);
        let digits = keep(send(peg, b"anyOf:", &[text(b"0123456789")]).unwrap());
        let digits = keep(send(digits, b"plus", &[]).unwrap());
        let digit_name = text(b"digits");
        send(grammar, b"rule:is:", &[digit_name, digits]).unwrap();
        let seq = keep(send(peg, b"seq", &[]).unwrap());
        let a = send(peg, b"literal:", &[text(b"a")]).unwrap();
        send(seq, b"add:", &[a]).unwrap();
        let reference = send(peg, b"ruleRef:", &[digit_name]).unwrap();
        send(seq, b"add:", &[reference]).unwrap();
        let eof = send(peg, b"eof", &[]).unwrap();
        send(seq, b"add:", &[eof]).unwrap();
        send(grammar, b"rule:is:", &[root_name, seq]).unwrap();
        let tree = keep(send(grammar, b"parse:from:", &[text(b"a123"), root_name]).unwrap());
        assert_eq!(int_value(send(tree, b"childCount", &[]).unwrap()), 1);
        let child = keep(send(tree, b"childAt:", &[zero]).unwrap());
        assert_eq!(
            text_bytes(send(child, b"ruleName", &[]).unwrap()),
            b"digits"
        );
        assert_eq!(int_value(send(child, b"start", &[]).unwrap()), 1);
        assert_eq!(int_value(send(child, b"end", &[]).unwrap()), 4);
        let children = send(tree, b"children", &[]).unwrap();
        assert_eq!(int_value(send(children, b"size", &[]).unwrap()), 1);
        assert_eq!(
            send(children, b"atWord:", &[abi::size(0).unwrap()]).unwrap(),
            child
        );
        let syntax = keep(send(grammar, b"parse:from:", &[text(b"aX"), root_name]).unwrap());
        assert_eq!(
            send(syntax, b"at:", &[abi::size(0).unwrap()]).unwrap(),
            sel(b"syntaxError")
        );
        assert_eq!(
            text_bytes(send(syntax, b"at:", &[abi::size(1).unwrap()]).unwrap()),
            b"syntax error at line 1, col 2"
        );
        assert_eq!(int_value(send(grammar, b"furthestFail", &[]).unwrap()), 1);
        let missing =
            keep(send(grammar, b"parse:from:", &[text(b""), text(b"missingRule")]).unwrap());
        assert_eq!(
            send(missing, b"at:", &[abi::size(0).unwrap()]).unwrap(),
            sel(b"undefinedRule")
        );
        assert_eq!(
            text_bytes(send(missing, b"at:", &[abi::size(1).unwrap()]).unwrap()),
            b"undefined rule: missingRule"
        );
        let loop_rule = text(b"loop");
        let recursive = send(peg, b"ruleRef:", &[loop_rule]).unwrap();
        send(grammar, b"rule:is:", &[loop_rule, recursive]).unwrap();
        let recursion = keep(send(grammar, b"parse:from:", &[text(b""), loop_rule]).unwrap());
        assert_eq!(
            send(recursion, b"at:", &[abi::size(0).unwrap()]).unwrap(),
            sel(b"leftRecursion")
        );
        let range = keep(send(peg, b"range:to:", &[text(b"A"), text(b"Z")]).unwrap());
        send(grammar, b"rule:is:", &[root_name, range]).unwrap();
        let tree = send(grammar, b"parse:from:", &[text(b"Q"), root_name]).unwrap();
        assert_eq!(int_value(send(tree, b"end", &[]).unwrap()), 1);
        let patterns = keep(send(vector, b"add:", &[range]).unwrap());
        let patterns = keep(send(patterns, b"add:", &[send(peg, b"eof", &[]).unwrap()]).unwrap());
        let sequence = send(peg, b"seqOf:", &[patterns]).unwrap();
        send(grammar, b"rule:is:", &[root_name, sequence]).unwrap();
        let tree = send(grammar, b"parse:from:", &[text(b"Z"), root_name]).unwrap();
        assert_eq!(int_value(send(tree, b"end", &[]).unwrap()), 1);
        let ephemeral = send(peg, b"literal:", &[text(b"discard")]).unwrap();
        send(gc, b"collectPrecisely", &[]).unwrap();
        assert!(send(id.root, b"identityOf:", &[ephemeral]).is_err());
        for _ in 0..24 {
            send(peg, b"literal:", &[text(b"reuse")]).unwrap();
            send(gc, b"collectPrecisely", &[]).unwrap();
        }
        send(string_type, b"addMethod", &[sel(b"bytes"), absent_value]).unwrap();
        assert!(send(peg, b"literal:", &[present]).is_err());
        send(string_type, b"addMethod", &[sel(b"bytes"), original_bytes]).unwrap();
        for vt in peg_types {
            send(id.root, b"reassociateBehaviorOf:with:", &[vt, peg_meta]).unwrap();
        }
        println!(
            "PEG binary literals, composed grammars, CSTs, errors, allocation-time GC and retirement: passed"
        );

        let args = send(id.root, b"sequence", &[namespace]).unwrap();
        let artifact = id
            .send(modules[10], id.load, &[args])
            .expect("Artifact loaded");
        fn field(out: &mut Vec<u8>, value: &[u8]) {
            out.extend_from_slice(&(value.len() as u32).to_le_bytes());
            out.extend_from_slice(value);
        }
        fn list(out: &mut Vec<u8>, values: &[&[u8]]) {
            out.extend_from_slice(&(values.len() as u32).to_le_bytes());
            for value in values {
                field(out, value);
            }
        }
        let artifact_bytes = |abi_name: &[u8], target: &[u8]| {
            let mut payload = Vec::new();
            for v in [
                b"module".as_slice(),
                b"candidate",
                abi_name,
                target,
                b"source-digest",
                b"revision-digest",
            ] {
                field(&mut payload, v);
            }
            list(&mut payload, &[b"Foo", b"Bar"]);
            list(&mut payload, &[b"libdep.dylib"]);
            list(&mut payload, &[b"activate", b"inspect"]);
            let mut bytes = b"not executable: inspection only\n".to_vec();
            bytes.extend_from_slice(b"BIRNAM-ARTIFACT-DESCRIPTOR-V1");
            bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            bytes.extend_from_slice(&payload);
            bytes
        };
        let fixture = artifact_bytes(b"birnam-artifact-generation-v1", b"aarch64-macos");
        let directory = std::env::temp_dir().join(format!(
            "birnam-oop-artifacts-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("candidate.dylib");
        std::fs::write(&path, &fixture).unwrap();
        std::fs::write(directory.join("libdep.dylib"), b"sibling marker").unwrap();
        let filename = text(path.to_str().unwrap().as_bytes());
        send(
            gc_type,
            b"addMethod",
            &[sel(b"chargeExternalAllocation:"), collecting_charge],
        )
        .unwrap();
        let descriptor = keep(send(artifact, b"inspect:", &[filename]).unwrap());
        let descriptor_type = send(namespace, b"at:", &[sel(b"ArtifactDescriptorVt")]).unwrap();
        assert_eq!(
            send(id.root, b"behaviorOf:", &[descriptor]).unwrap(),
            descriptor_type
        );
        assert_eq!(send(artifact, b"lastFailure", &[]).unwrap(), nil);
        let symbol_text = |symbol| text_bytes(send(string, b"fromBytes:", &[symbol]).unwrap());
        assert_eq!(
            symbol_text(send(descriptor, b"kind", &[]).unwrap()),
            b"module"
        );
        let identity = send(descriptor, b"identity", &[]).unwrap();
        assert_eq!(symbol_text(identity), b"candidate");
        assert_eq!(send(descriptor, b"rootAt:", &[zero]).unwrap(), identity);
        assert_eq!(int_value(send(descriptor, b"rootsCount", &[]).unwrap()), 1);
        for (selector, expected) in [
            (
                b"artifactAbi".as_slice(),
                b"birnam-artifact-generation-v1".as_slice(),
            ),
            (b"target", b"aarch64-macos"),
            (b"sourceDigest", b"source-digest"),
            (b"revisionDigest", b"revision-digest"),
        ] {
            assert_eq!(
                text_bytes(send(descriptor, selector, &[]).unwrap()),
                expected
            );
        }
        assert_eq!(
            int_value(send(descriptor, b"requiresCount", &[]).unwrap()),
            2
        );
        assert_eq!(
            symbol_text(send(descriptor, b"requireAt:", &[zero]).unwrap()),
            b"Foo"
        );
        assert_eq!(
            symbol_text(send(descriptor, b"requireAt:", &[one]).unwrap()),
            b"Bar"
        );
        assert_eq!(send(descriptor, b"requireAt:", &[minus]).unwrap(), nil);
        assert_eq!(send(descriptor, b"requireAt:", &[two]).unwrap(), nil);
        assert_eq!(
            text_bytes(send(descriptor, b"nativeRequireAt:", &[zero]).unwrap()),
            b"libdep.dylib"
        );
        assert_eq!(
            symbol_text(send(descriptor, b"capabilityAt:", &[one]).unwrap()),
            b"inspect"
        );
        let requires = keep(send(descriptor, b"requires", &[]).unwrap());
        assert_eq!(int_value(send(requires, b"size", &[]).unwrap()), 2);
        let bytes = text(&fixture);
        send(actor, b"at:put:", &[abi::size(42).unwrap(), filename]).unwrap();
        send(actor, b"at:put:", &[abi::size(44).unwrap(), bytes]).unwrap();
        send(string_type, b"addMethod", &[sel(b"bytes"), file_bytes]).unwrap();
        let snapshot = keep(send(artifact, b"inspectBytes:from:", &[bytes, filename]).unwrap());
        send(string_type, b"addMethod", &[sel(b"bytes"), original_bytes]).unwrap();
        assert_eq!(
            text_bytes(send(snapshot, b"path", &[]).unwrap()),
            path.to_str().unwrap().as_bytes()
        );
        assert_eq!(send(artifact, b"inspect:", &[one]).unwrap(), nil);
        assert_eq!(
            send(artifact, b"lastFailure", &[]).unwrap(),
            sel(b"artifactPathInvalid")
        );
        for (data, reason) in [
            (b"bad".to_vec(), b"artifactDescriptionMissing".as_slice()),
            (
                artifact_bytes(b"bad-abi", b"aarch64-macos"),
                b"artifactAbiMismatch",
            ),
            (
                artifact_bytes(b"birnam-artifact-generation-v1", b"bad-target"),
                b"artifactTargetMismatch",
            ),
        ] {
            assert_eq!(
                send(artifact, b"inspectBytes:from:", &[text(&data), filename]).unwrap(),
                nil
            );
            assert_eq!(send(artifact, b"lastFailure", &[]).unwrap(), sel(reason));
        }
        std::fs::remove_file(directory.join("libdep.dylib")).unwrap();
        assert_eq!(send(artifact, b"inspect:", &[filename]).unwrap(), nil);
        assert_eq!(
            send(artifact, b"lastFailure", &[]).unwrap(),
            sel(b"nativeRequirementUnavailable")
        );
        std::fs::remove_file(&path).unwrap();
        assert_eq!(send(artifact, b"inspect:", &[filename]).unwrap(), nil);
        assert_eq!(
            send(artifact, b"lastFailure", &[]).unwrap(),
            sel(b"artifactUnavailable")
        );
        send(gc, b"collectPrecisely", &[]).unwrap();
        assert_eq!(
            symbol_text(send(descriptor, b"identity", &[]).unwrap()),
            b"candidate"
        );
        send(vector_type, b"addMethod", &[sel(b"new"), replacement]).unwrap();
        assert!(send(descriptor, b"requires", &[]).is_err());
        send(vector_type, b"addMethod", &[sel(b"new"), vector_new]).unwrap();
        send(
            gc_type,
            b"addMethod",
            &[sel(b"chargeExternalAllocation:"), original_charge],
        )
        .unwrap();
        std::fs::remove_dir_all(&directory).unwrap();
        println!(
            "Artifact descriptors, live list access, byte snapshots, sibling checks, failure reasons and construction-time GC: passed"
        );
        super::tcc::check(id,send,sel,[modules[11],modules[12]],[namespace,roots,gc,string,base,actor]);
        super::compiler::check(id,send,sel,[modules[13],modules[14],modules[15]],[namespace,roots,gc,string,base,actor,integer]);
        println!("NUMERIC_INTEGRATION_OK");
    }
}
