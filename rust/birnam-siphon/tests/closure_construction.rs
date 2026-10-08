//! Direct capture construction and immutable shape queries through current Id messages.
mod support;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use birnam_artifact_codec::portable_program::{NodeSpec, ProgramSpec, StatementSpec, encode};
use std::{
    cell::{Cell, RefCell},
    ffi::c_void,
    os::unix::ffi::OsStrExt,
};
use support::*;
struct H {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    gc: Oop,
    tokens: RefCell<Vec<usize>>,
    _floor: CompiledFloor,
}
impl H {
    unsafe fn named(&self, name: &[u8]) -> Oop {
        unsafe {
            object(send(
                self.id,
                self.ns,
                b"at:",
                &[Operand::object(self.id.intern_name(name).unwrap())],
            ))
        }
    }
    unsafe fn keep(&self, value: Oop) -> Oop {
        self.tokens
            .borrow_mut()
            .push(unsafe { retain(self.id, self.roots, value) });
        value
    }
    unsafe fn call(&self, receiver: Oop, name: &[u8], args: &[Oop]) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                receiver,
                name,
                &args
                    .iter()
                    .copied()
                    .map(Operand::object)
                    .collect::<Vec<_>>(),
            )))
        }
    }
    unsafe fn int(&self, value: i64) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                self.named(b"Integer"),
                b"fromWord:",
                &[Operand::word(value as usize)],
            )))
        }
    }
    unsafe fn number(&self, value: Oop) -> usize {
        unsafe {
            send(
                self.id,
                self.named(b"Integer"),
                b"wordOf:",
                &[Operand::object(value)],
            )
            .as_word()
            .unwrap()
        }
    }
    unsafe fn vector(&self, values: &[Oop]) -> Oop {
        unsafe {
            let mut v = self.call(self.named(b"Vector"), b"new", &[]);
            for &value in values {
                v = self.call(v, b"add:", &[value]);
            }
            v
        }
    }
    unsafe fn admit(&self, bytes: &[u8]) -> Oop {
        unsafe {
            let bytes = self.keep(object(send(
                self.id,
                self.named(b"Bytes"),
                b"fromBytes:",
                &[Operand::bytes(bytes)],
            )));
            self.call(self.named(b"CodeObject"), b"fromCanonicalBytes:", &[bytes])
        }
    }
    unsafe fn materialize(&self, code: Oop, captures: &[Oop], resumable: bool) -> Oop {
        unsafe {
            self.call(
                self.named(b"Codegen"),
                if resumable {
                    b"materializeResumable:captures:"
                } else {
                    b"materializeSynchronous:captures:"
                },
                &[code, self.vector(captures)],
            )
        }
    }
    unsafe fn eval(&self, source: &[u8]) -> Oop {
        unsafe {
            let text = self.keep(object(send(
                self.id,
                self.named(b"String"),
                b"fromBytes:",
                &[Operand::bytes(source)],
            )));
            self.call(self.named(b"Codegen"), b"eval:", &[text])
        }
    }
    unsafe fn collect(&self) {
        unsafe {
            collect(self.id, self.gc);
            send(self.id, self.named(b"TCC"), b"checkpoint", &[]);
        }
    }
    unsafe fn clear_from(&self, index: usize) {
        let tokens = self.tokens.borrow_mut().split_off(index);
        for token in tokens {
            unsafe {
                release(self.id, self.roots, token);
            }
        }
    }
}
unsafe fn behavior_of(h: &H, value: Oop) -> Oop {
    unsafe {
        object(send(
            h.id,
            h.id.receiver,
            b"behaviorOf:",
            &[Operand::object(value)],
        ))
    }
}
unsafe fn binding(h: &H, vt: Oop, name: &[u8]) -> NativeEntry {
    unsafe {
        send(
            h.id,
            h.id.receiver,
            b"bindingOf:in:",
            &[
                Operand::object(h.id.intern_name(name).unwrap()),
                Operand::object(vt),
            ],
        )
        .as_entry()
        .unwrap()
    }
}
unsafe fn set(h: &H, vt: Oop, name: &[u8], entry: NativeEntry) {
    unsafe {
        send(
            h.id,
            vt,
            b"addMethod",
            &[
                Operand::object(h.id.intern_name(name).unwrap()),
                Operand::entry(entry),
            ],
        );
    }
}
fn literal_program(value: i64) -> Vec<u8> {
    encode(ProgramSpec {
        parameters: &[],
        captures: &[],
        lets: &[],
        statements: &[StatementSpec::Expression { root_node: 0 }],
        nodes: &[NodeSpec::Integer(value)],
    })
    .unwrap()
}

unsafe extern "C" fn admitted_constant(
    c: *mut c_void,
    _: Oop,
    _: *const Operand,
    n: usize,
) -> Outcome {
    assert_eq!(n, 0);
    Outcome::loaded(Operand::object(c.cast()))
}
struct ClosureQuery {
    id: Protocol,
    roots: Oop,
    gc: Oop,
    alias: Oop,
    calls: Cell<usize>,
    release_tokens: RefCell<Vec<usize>>,
    mode: Cell<u8>,
}
unsafe extern "C" fn closure_query(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let q = &*c.cast::<ClosureQuery>();
        q.calls.set(q.calls.get() + 1);
        for token in q.release_tokens.borrow_mut().drain(..) {
            release(q.id, q.roots, token);
        }
        collect(q.id, q.gc);
        match q.mode.get() {
            1 => Ok(Operand::word(4097)),
            2 => Ok(Operand::unit()),
            3 => Err(Outcome::failed(881, 73)),
            4 => Ok(Operand::word(0)),
            _ => q.id.send(r, q.alias, borrowed_slice(a, n).unwrap()),
        }
    })()
    .map_or_else(|e| e, Outcome::loaded)
}

#[test]
#[ignore = "requires rebuilt production modules"]
fn compact_shape_and_direct_captures_preserve_dispatch_aliases_and_callback_lifetime() {
    unsafe {
        let id = id();
        let ns = load(
            id,
            "bootstrap",
            &[Operand::bytes(directory().as_os_str().as_bytes())],
        );
        let named = |name: &[u8]| {
            object(send(
                id,
                ns,
                b"at:",
                &[Operand::object(id.intern_name(name).unwrap())],
            ))
        };
        let h = H {
            id,
            ns,
            roots: named(b"Roots"),
            gc: named(b"GC"),
            tokens: RefCell::new(Vec::new()),
            _floor: CompiledFloor::pin(id, ns, named(b"Roots")),
        };
        let bytes = encode(ProgramSpec {
            parameters: &[b"arg"],
            captures: &[b"a", b"b"],
            lets: &[],
            statements: &[StatementSpec::Expression { root_node: 0 }],
            nodes: &[NodeSpec::LexicalRead(1)],
        })
        .unwrap();
        let code = h.admit(&bytes);
        let factory = h.named(b"CodeObject");
        assert_eq!(
            send(id, factory, b"parameterCountOf:", &[Operand::object(code)]).as_word(),
            Some(1)
        );
        assert_eq!(
            send(id, factory, b"captureCountOf:", &[Operand::object(code)]).as_word(),
            Some(2)
        );
        for args in [
            vec![],
            vec![Operand::word(0)],
            vec![Operand::object(h.named(b"nil"))],
        ] {
            assert!(id.send_named(factory, b"parameterCountOf:", &args).is_err());
            assert!(id.send_named(factory, b"captureCountOf:", &args).is_err());
        }
        let first = h.int(31);
        let second = h.int(47);
        let owner = h.named(b"Block");
        let code_vt = behavior_of(&h, factory);
        let block_vt = behavior_of(&h, owner);
        let mut hooks = Vec::new();
        for (vt, name) in [
            (code_vt, b"parameterCountOf:".as_slice()),
            (code_vt, b"captureCountOf:"),
            (block_vt, b"fromMethod:captureOperands:"),
        ] {
            let original = binding(&h, vt, name);
            let alias_name = [b"closureOriginal".as_slice(), name].concat();
            let alias = h.keep(id.intern_name(&alias_name).unwrap());
            set(&h, vt, &alias_name, original);
            let hook = Box::into_raw(Box::new(ClosureQuery {
                id,
                roots: h.roots,
                gc: h.gc,
                alias,
                calls: Cell::new(0),
                release_tokens: RefCell::new(Vec::new()),
                mode: Cell::new(0),
            }));
            set(&h, vt, name, NativeEntry::new(closure_query, hook.cast()));
            hooks.push((vt, name, original, hook));
        }
        let answer = h.int(42);
        let method = h.keep(object(send(
            id,
            h.named(b"Method"),
            b"fromNative:owner:",
            &[
                Operand::entry(NativeEntry::new(admitted_constant, answer.cast())),
                Operand::object(answer),
            ],
        )));
        let admitted_code = h.admit(&literal_program(42));
        // Failed admission publishes no shape. A later valid admission may retry.
        (*hooks[1].3).mode.set(2);
        assert!(
            id.send_named(
                method,
                b"admitCodeObject:",
                &[Operand::object(admitted_code)]
            )
            .is_err()
        );
        assert!(
            id.send_named(
                owner,
                b"fromMethod:captureOperands:",
                &[Operand::object(method)]
            )
            .is_err()
        );
        (*hooks[1].3).mode.set(0);
        let token = h.tokens.borrow_mut().pop().unwrap();
        (*hooks[0].3).release_tokens.borrow_mut().push(token);
        assert_eq!(
            send(
                id,
                method,
                b"admitCodeObject:",
                &[Operand::object(admitted_code)]
            )
            .as_object(),
            Some(method)
        );
        assert!(present(id, admitted_code));
        h.collect();
        assert!(
            present(id, admitted_code),
            "Method must trace its admitted CodeObject"
        );
        let before = (*hooks[0].3).calls.get() + (*hooks[1].3).calls.get();
        send(
            id,
            method,
            b"admitCodeObject:",
            &[Operand::object(admitted_code)],
        );
        let direct = h.keep(object(send(
            id,
            owner,
            b"fromMethod:captureOperands:",
            &[Operand::object(method)],
        )));
        assert_eq!(h.call(direct, b"value", &[]), answer);
        assert_eq!(h.call(direct, b"codeObject", &[]), admitted_code);
        assert_eq!(
            (*hooks[0].3).calls.get() + (*hooks[1].3).calls.get(),
            before,
            "admitted reuse must issue no shape queries"
        );
        let other_code = h.admit(&literal_program(42));
        assert_ne!(admitted_code, other_code);
        assert!(
            id.send_named(method, b"admitCodeObject:", &[Operand::object(other_code)])
                .is_err()
        );
        let alias = h.call(direct, b"methodFor:", &[id.intern_name(b"value").unwrap()]);
        let alias_block = h.keep(object(send(
            id,
            owner,
            b"fromMethod:captureOperands:",
            &[Operand::object(alias)],
        )));
        assert_eq!(h.call(alias_block, b"codeObject", &[]), admitted_code);
        assert_eq!(h.call(alias_block, b"value", &[]), answer);
        let paired = h.call(
            owner,
            b"fromMethod:codeObject:captures:",
            &[method, other_code, h.vector(&[])],
        );
        assert_eq!(
            h.call(paired, b"codeObject", &[]),
            other_code,
            "external equal-byte code identity must be preserved"
        );
        for resumable in [false, true] {
            let original = h.materialize(code, &[first, second], resumable);
            let method = h.call(
                original,
                b"methodFor:",
                &[id.intern_name(b"value:").unwrap()],
            );
            let direct = |captures: &[Operand]| {
                let mut args = vec![Operand::object(method)];
                args.extend_from_slice(captures);
                id.send_named(owner, b"fromMethod:captureOperands:", &args)
            };
            let block = h.keep(object(
                direct(&[Operand::object(second), Operand::object(first)]).unwrap(),
            ));
            assert_eq!(h.call(block, b"value:", &[first]), second);
            assert_eq!(
                send(
                    id,
                    owner,
                    b"captureWord:of:",
                    &[Operand::word(1), Operand::object(block)]
                )
                .as_object(),
                Some(first)
            );
            assert!(direct(&[Operand::object(first)]).is_err());
            assert!(direct(&[Operand::object(first), Operand::word(0)]).is_err());
            assert!(
                direct(&[
                    Operand::object(first),
                    Operand::object(first),
                    Operand::object(first)
                ])
                .is_err()
            );
            let mut too_many = vec![Operand::object(method)];
            too_many.extend(std::iter::repeat_n(Operand::object(first), 4097));
            assert!(
                id.send_named(owner, b"fromMethod:captureOperands:", &too_many)
                    .is_err()
            );
            for bad in [
                vec![],
                vec![Operand::object(h.named(b"nil"))],
                vec![Operand::object(method), Operand::object(h.named(b"nil"))],
            ] {
                assert!(
                    id.send_named(owner, b"fromMethod:captureOperands:", &bad)
                        .is_err()
                );
            }
            let shell = h.keep(object(send(
                id,
                owner,
                b"imageShellFromMethod:codeObject:",
                &[Operand::object(method), Operand::object(code)],
            )));
            assert!(
                id.send_named(shell, b"value:", &[Operand::object(first)])
                    .is_err(),
                "pending image shell must not execute before capture restoration"
            );
            assert!(
                id.send_named(
                    owner,
                    b"restoreImageCaptures:of:",
                    &[Operand::object(h.vector(&[first])), Operand::object(shell)]
                )
                .is_err()
            );
            send(
                id,
                owner,
                b"restoreImageCaptures:of:",
                &[
                    Operand::object(h.vector(&[second, first])),
                    Operand::object(shell),
                ],
            );
            h.collect();
            assert_eq!(h.call(shell, b"value:", &[first]), second);
            let gc_vt = behavior_of(&h, h.gc);
            let prepare = binding(&h, gc_vt, b"prepareAllocation");
            let alias = h.keep(id.intern_name(b"closureOriginalPrepare").unwrap());
            set(&h, gc_vt, b"closureOriginalPrepare", prepare);
            let checkpoint = Box::into_raw(Box::new(ClosureQuery {
                id,
                roots: h.roots,
                gc: h.gc,
                alias,
                calls: Cell::new(0),
                release_tokens: RefCell::new(Vec::new()),
                mode: Cell::new(0),
            }));
            set(
                &h,
                gc_vt,
                b"prepareAllocation",
                NativeEntry::new(closure_query, checkpoint.cast()),
            );
            let fresh = allocate(id, behavior(id), 0);
            (*checkpoint)
                .release_tokens
                .borrow_mut()
                .push(retain(id, h.roots, fresh));
            let alias_block = h.keep(object(
                direct(&[Operand::object(fresh), Operand::object(fresh)]).unwrap(),
            ));
            assert!((*checkpoint).release_tokens.borrow().is_empty());
            assert!(
                present(id, fresh),
                "constructor scope protects captures before collecting admission"
            );
            set(&h, gc_vt, b"prepareAllocation", prepare);
            drop(Box::from_raw(checkpoint));
            h.collect();
            assert_eq!(h.call(alias_block, b"value:", &[first]), fresh);
            let clone = h.call(alias_block, b"clone", &[]);
            assert_eq!(h.call(clone, b"value:", &[first]), fresh);
            // Changes to shape readers affect explicit external pairing. They
            // cannot rewrite a Method's previously admitted immutable counts.
            for mode in [1, 2, 3, 4] {
                (*hooks[1].3).mode.set(mode);
                assert!(direct(&[Operand::object(first), Operand::object(second)]).is_ok());
                let failure = id
                    .send_named(
                        owner,
                        b"fromMethod:codeObject:captures:",
                        &[
                            Operand::object(method),
                            Operand::object(code),
                            Operand::object(h.vector(&[first, second])),
                        ],
                    )
                    .unwrap_err();
                if mode == 3 {
                    assert_eq!((failure.status, failure.detail), (881, 73));
                } else {
                    assert_eq!(failure.status, 1);
                }
            }
            (*hooks[1].3).mode.set(0);
        }
        // More than eight direct captures, zero captures, and nested literals in
        // both emitters. Ordered projections prevent a mistaken truncated carrier.
        let names = (0..12).map(|i| format!("c{i}")).collect::<Vec<_>>();
        let refs = names.iter().map(|s| s.as_bytes()).collect::<Vec<_>>();
        let inner = encode(ProgramSpec {
            parameters: &[],
            captures: &refs,
            lets: &[],
            statements: &[StatementSpec::Expression { root_node: 0 }],
            nodes: &[NodeSpec::LexicalRead(11)],
        })
        .unwrap();
        let indices = (0..12).collect::<Vec<_>>();
        let outer = encode(ProgramSpec {
            parameters: &[],
            captures: &refs,
            lets: &[],
            statements: &[StatementSpec::Expression { root_node: 0 }],
            nodes: &[NodeSpec::BlockLiteral {
                program: &inner,
                captures: &indices,
            }],
        })
        .unwrap();
        let outer = h.admit(&outer);
        let values = (0..12).map(|i| h.int(i + 100)).collect::<Vec<_>>();
        for resumable in [false, true] {
            let outer = h.materialize(outer, &values, resumable);
            let child = h.call(outer, b"value", &[]);
            assert_eq!(h.call(child, b"value", &[]), values[11]);
            for (i, value) in values.iter().enumerate() {
                assert_eq!(
                    send(
                        id,
                        owner,
                        b"captureWord:of:",
                        &[Operand::word(i), Operand::object(child)]
                    )
                    .as_object(),
                    Some(*value)
                );
            }
            let empty = h.materialize(h.admit(&literal_program(42)), &[], resumable);
            assert_eq!(h.number(h.call(empty, b"value", &[])), 42);
        }
        for (vt, name, original, hook) in hooks {
            assert!(
                (*hook).calls.get() > 0,
                "new operations must use current Id dispatch"
            );
            set(&h, vt, name, original);
            drop(Box::from_raw(hook));
        }
        // A bounded local concurrency probe: captured shared Cells survive waits,
        // collection, resumption, cancellation and cleanup at both quanta.
        h.eval(include_bytes!("../../../modules/modules.birnam"));
        h.eval(include_bytes!("../../../modules/stdlib.birnam"));
        h.eval(include_bytes!("../../../modules/stackless-control.birnam"));
        for quantum in [128, 1] {
            let source = format!(
                r#"
              [[TaskExecutor field: #quantum] set: {quantum}];
              let shared := [41 asCell]; let observed := [nil asCell]; let cleaned := [0 asCell];
              let task := [TaskControl start: [:task |
                [task ensure: [| [task wait]; [observed set: [shared value]]; [shared value]]
                  cleanup: [| [cleaned inc]]]]];
              let token := [task waitToken];
              [GC collectPrecisely]; [shared set: 42];
              let woke := [task resolve: token with: nil];
              let good := [[woke & [[observed value] = 42]] & [[cleaned value] = 1]];
              let outcome := [task pollOutcome];
              let cancelCleaned := [0 asCell];
              let cancelled := [TaskControl start: [:task |
                [task ensure: [| [task wait]; [observed set: 99]] cleanup: [| [cancelCleaned inc]]]]];
              [GC collectPrecisely]; [cancelled cancel: #stop];
              let cancelledOutcome := [cancelled pollOutcome];
              let success := [[good & [outcome = #[#success 42]]] & [[observed value] = 42]];
              [[success & [[cancelCleaned value] = 1]] & [cancelledOutcome = #[#cancelled #stop]]]
            "#
            );
            assert_eq!(
                h.eval(source.as_bytes()),
                h.named(b"true"),
                "capture task lifecycle at quantum {quantum}"
            );
        }
        h.clear_from(0);
        h.collect();
    }
}
