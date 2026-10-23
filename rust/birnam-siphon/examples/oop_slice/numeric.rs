//! Fresh-process integration of the actual Integer, Float and String owners.
use birnam_abi::{self as abi, Oop, id_protocol::Protocol};
pub(super) unsafe fn check(
    id: &Protocol,
    send: &impl Fn(Oop, &[u8], &[Oop]) -> Result<Oop, Oop>,
    sel: &impl Fn(&[u8]) -> Oop,
    modules: [Oop; 2],
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
        let private = send(id.root, b"ensurePrivateVtableFor:", &[from_word]).unwrap();
        send(private, b"addMethod", &[sel(b"apply"), replacement]).unwrap();
        assert_eq!(
            send(integer, b"canOmitIndexing", &[]).unwrap(),
            abi::size(0).unwrap()
        );
        assert!(fold().is_null());
        send(private, b"removeMethod:", &[sel(b"apply")]).unwrap();
        assert_eq!(fold(), truth);
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
        println!("NUMERIC_INTEGRATION_OK");
    }
}
