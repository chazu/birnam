//! PortableCode ordinary objects through the production Id loader.
mod support;
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use core::{
    cell::{Cell, RefCell},
    ffi::c_void,
};
use std::os::unix::ffi::OsStrExt;
use support::*;
struct H {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    gc: Oop,
    tokens: RefCell<Vec<usize>>,
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
    unsafe fn keep(&self, o: Oop) -> Oop {
        self.tokens
            .borrow_mut()
            .push(unsafe { retain(self.id, self.roots, o) });
        o
    }
    unsafe fn string(&self, b: &[u8]) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                self.named(b"String"),
                b"fromBytes:",
                &[Operand::bytes(b)],
            )))
        }
    }
    unsafe fn call(&self, r: Oop, name: &[u8], args: &[Oop]) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                r,
                name,
                &args
                    .iter()
                    .copied()
                    .map(Operand::object)
                    .collect::<Vec<_>>(),
            )))
        }
    }
    unsafe fn number(&self, o: Oop) -> usize {
        unsafe {
            send(
                self.id,
                self.named(b"Integer"),
                b"wordOf:",
                &[Operand::object(o)],
            )
            .as_word()
            .unwrap()
        }
    }
    unsafe fn bind(&self, name: &[u8], o: Oop) {
        unsafe {
            send(
                self.id,
                self.ns,
                b"at:put:",
                &[
                    Operand::object(self.id.intern_name(name).unwrap()),
                    Operand::object(o),
                ],
            );
        }
    }
    unsafe fn bytes(&self, o: Oop) -> Vec<u8> {
        unsafe {
            let b = send(self.id, o, b"bytes", &[]).as_bytes().unwrap();
            borrowed_bytes(b.pointer, b.length).unwrap().to_vec()
        }
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
impl H {
    unsafe fn int(&self, n: i64) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                self.named(b"Integer"),
                b"fromWord:",
                &[Operand::word(n as usize)],
            )))
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
    unsafe fn program(&self, parameters: &[Oop], captures: &[Oop], statements: &[Oop]) -> Oop {
        unsafe {
            self.call(
                self.named(b"CodeProgram"),
                b"parameters:captures:statements:",
                &[
                    self.vector(parameters),
                    self.vector(captures),
                    self.vector(statements),
                ],
            )
        }
    }
    unsafe fn expression(&self, node: Oop) -> Oop {
        unsafe { self.call(self.named(b"CodeStatement"), b"expression:", &[node]) }
    }
    unsafe fn literal(&self, n: i64) -> Oop {
        unsafe { self.call(self.named(b"CodeNode"), b"integer:", &[self.int(n)]) }
    }
    unsafe fn digest(&self, program: Oop) -> Vec<u8> {
        unsafe {
            let code = self.call(self.named(b"CodeObject"), b"fromProgram:", &[program]);
            self.bytes(self.call(code, b"digest", &[]))
        }
    }
    unsafe fn kind(&self, r: Oop, kind: &[u8]) {
        unsafe {
            assert_eq!(
                self.call(r, b"kind", &[]),
                self.id.intern_name(kind).unwrap()
            );
        }
    }
    unsafe fn count(&self, r: Oop, selector: &[u8], count: usize) {
        unsafe {
            assert_eq!(self.number(self.call(r, selector, &[])), count);
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
use birnam_artifact_codec::portable_program as portable;
// Reify frozen fixture semantics using the public constructors, then check their
// digest against the independent, checked-in golden bytes' SHA256SUMS.
unsafe fn from_fixture(h: &H, bytes: &[u8]) -> Oop {
    unsafe {
        let p = portable::inspect(bytes).unwrap();
        let node = h.named(b"CodeNode");
        let mut nodes = Vec::new();
        for i in 0..p.node_count as usize {
            let n = p.node(i).unwrap();
            let value = match n.kind {
                1 => h.literal(n.payload as i64),
                2 => h.call(node, b"floatBits:", &[h.string(&n.payload.to_le_bytes())]),
                3 => h.call(node, b"string:", &[h.string(n.data)]),
                4 | 9 => h.call(
                    node,
                    if n.kind == 4 {
                        b"symbol:"
                    } else {
                        b"globalRead:"
                    },
                    &[h.keep(h.id.intern_name(n.data).unwrap())],
                ),
                5 => h.call(node, b"nilLiteral", &[]),
                6 => h.call(node, b"trueLiteral", &[]),
                7 => h.call(node, b"falseLiteral", &[]),
                8 => h.call(node, b"lexicalReadAt:", &[h.int(n.auxiliary_0 as i64)]),
                10 => {
                    let operands = (0..n.operand_count as usize)
                        .map(|j| nodes[p.node_operand(i, j).unwrap() as usize])
                        .collect::<Vec<_>>();
                    h.call(
                        node,
                        b"send:operands:",
                        &[
                            h.keep(h.id.intern_name(n.data).unwrap()),
                            h.vector(&operands),
                        ],
                    )
                }
                11 => {
                    let captures = (0..n.operand_count as usize)
                        .map(|j| h.int(p.node_operand(i, j).unwrap() as i64))
                        .collect::<Vec<_>>();
                    h.call(
                        node,
                        b"block:captureBindings:",
                        &[from_fixture(h, n.data), h.vector(&captures)],
                    )
                }
                _ => unreachable!(),
            };
            nodes.push(value);
        }
        let mut parameters = Vec::new();
        let mut captures = Vec::new();
        for i in 0..p.parameter_count + p.capture_count {
            let name = h.string(p.binding(i as usize).unwrap().name);
            if i < p.parameter_count {
                parameters.push(name);
            } else {
                captures.push(name);
            }
        }
        let statements = (0..p.statement_count as usize)
            .map(|i| {
                let st = p.statement(i).unwrap();
                let expression = nodes[st.root_node as usize];
                if let Some(index) = st.binding_index {
                    h.call(
                        h.named(b"CodeStatement"),
                        b"letNamed:expression:",
                        &[
                            h.string(p.binding(index as usize).unwrap().name),
                            expression,
                        ],
                    )
                } else {
                    h.expression(expression)
                }
            })
            .collect::<Vec<_>>();
        h.program(&parameters, &captures, &statements)
    }
}
unsafe fn check_node(h: &H, node: Oop, p: &portable::Program<'_>, index: usize) {
    unsafe {
        let n = p.node(index).unwrap();
        h.kind(
            node,
            [
                b"".as_slice(),
                b"integer",
                b"float",
                b"string",
                b"symbol",
                b"nil",
                b"true",
                b"false",
                b"lexicalRead",
                b"globalRead",
                b"send",
                b"block",
            ][n.kind as usize],
        );
        assert_eq!(h.call(node, b"clone", &[]), node);
        h.count(
            node,
            b"operandCount",
            if n.kind == 10 {
                n.operand_count as usize
            } else {
                0
            },
        );
        h.count(
            node,
            b"captureCount",
            if n.kind == 11 {
                n.operand_count as usize
            } else {
                0
            },
        );
        match n.kind {
            1 => assert_eq!(h.number(h.call(node, b"integer", &[])) as u64, n.payload),
            2 => assert_eq!(
                h.bytes(h.call(node, b"floatBits", &[])),
                n.payload.to_le_bytes()
            ),
            3 => assert_eq!(h.bytes(h.call(node, b"string", &[])), n.data),
            4 | 9 => assert_eq!(
                h.call(node, if n.kind == 4 { b"symbol" } else { b"global" }, &[]),
                h.keep(h.id.intern_name(n.data).unwrap())
            ),
            8 => h.count(node, b"bindingIndex", n.auxiliary_0 as usize),
            10 => {
                assert_eq!(
                    h.call(node, b"selector", &[]),
                    h.keep(h.id.intern_name(n.data).unwrap())
                );
                for j in 0..n.operand_count as usize {
                    check_node(
                        h,
                        h.call(node, b"operandAt:", &[h.int(j as i64)]),
                        p,
                        p.node_operand(index, j).unwrap() as usize,
                    );
                }
            }
            11 => {
                let nested = h.call(node, b"program", &[]);
                check_program(h, nested, n.data);
                for j in 0..n.operand_count as usize {
                    assert_eq!(
                        h.number(h.call(node, b"captureBindingAt:", &[h.int(j as i64)])),
                        p.node_operand(index, j).unwrap() as usize
                    );
                }
            }
            _ => {}
        }
    }
}
unsafe fn check_program(h: &H, program: Oop, bytes: &[u8]) {
    unsafe {
        let p = portable::inspect(bytes).unwrap();
        let nil = h.named(b"nil");
        assert_eq!(
            h.call(program, b"format", &[]),
            h.id.intern_name(b"portableProgramV1").unwrap()
        );
        assert_eq!(h.call(program, b"clone", &[]), program);
        for (sel, n) in [
            (b"arity".as_slice(), p.parameter_count),
            (b"parameterCount", p.parameter_count),
            (b"captureCount", p.capture_count),
            (
                b"letCount",
                p.binding_count - p.parameter_count - p.capture_count,
            ),
            (b"bindingCount", p.binding_count),
            (b"statementCount", p.statement_count),
        ] {
            h.count(program, sel, n as usize);
        }
        let mut params = Vec::new();
        let mut captures = Vec::new();
        for i in 0..p.binding_count {
            let expected = p.binding(i as usize).unwrap();
            let binding = h.call(program, b"bindingAt:", &[h.int(i as i64)]);
            let (selector, kind) = match expected.kind {
                portable::BindingKind::Parameter => {
                    (b"parameterAt:".as_slice(), b"parameter".as_slice())
                }
                portable::BindingKind::Capture => (b"captureAt:".as_slice(), b"capture".as_slice()),
                portable::BindingKind::Let => (b"letAt:".as_slice(), b"let".as_slice()),
            };
            let partitioned = h.call(program, selector, &[h.int(expected.ordinal as i64)]);
            for b in [binding, partitioned] {
                h.kind(b, kind);
                h.count(b, b"index", i as usize);
                h.count(b, b"ordinal", expected.ordinal as usize);
                assert_eq!(h.bytes(h.call(b, b"name", &[])), expected.name);
                assert_eq!(h.call(b, b"clone", &[]), b);
                let defining = h.call(b, b"definingStatementIndex", &[]);
                if let Some(index) = expected.defining_statement {
                    assert_eq!(h.number(defining), index as usize);
                } else {
                    assert_eq!(defining, nil);
                }
            }
            if i < p.parameter_count {
                params.push(h.call(binding, b"name", &[]));
            } else if i < p.parameter_count + p.capture_count {
                captures.push(h.call(binding, b"name", &[]));
            }
        }
        let mut statements = Vec::new();
        for i in 0..p.statement_count {
            let st = p.statement(i as usize).unwrap();
            let statement = h.call(program, b"statementAt:", &[h.int(i as i64)]);
            assert_eq!(h.call(statement, b"clone", &[]), statement);
            h.kind(
                statement,
                if st.binding_index.is_some() {
                    b"let"
                } else {
                    b"expression"
                },
            );
            let name = h.call(statement, b"bindingName", &[]);
            if let Some(binding) = st.binding_index {
                assert_eq!(h.bytes(name), p.binding(binding as usize).unwrap().name);
            } else {
                assert_eq!(name, nil);
            }
            check_node(
                h,
                h.call(statement, b"expression", &[]),
                &p,
                st.root_node as usize,
            );
            statements.push(statement);
        }
        let rebuilt = h.program(&params, &captures, &statements);
        assert_eq!(h.digest(rebuilt), h.digest(program));
        for sel in [
            b"statementAt:".as_slice(),
            b"bindingAt:",
            b"parameterAt:",
            b"captureAt:",
            b"letAt:",
        ] {
            for index in [nil, h.int(-1), h.int(i64::MAX)] {
                assert_eq!(h.call(program, sel, &[index]), nil);
            }
        }
    }
}
struct Forward {
    id: Protocol,
    gc: Oop,
    alias: Oop,
    calls: Cell<usize>,
    failure: Cell<bool>,
}
unsafe extern "C" fn collecting(
    context: *mut c_void,
    r: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    (|| unsafe {
        let f = &*context.cast::<Forward>();
        f.calls.set(f.calls.get() + 1);
        if f.failure.get() {
            return Err(Outcome::failed(0x73, 0x517));
        }
        let args = borrowed_slice(args, count).unwrap();
        // Native Bytes arguments also expire at the next send.
        let bytes = if let [a] = args {
            a.as_bytes()
                .map(|b| borrowed_bytes(b.pointer, b.length).unwrap().to_vec())
        } else {
            None
        };
        f.id.send_named(f.gc, b"collectPrecisely", &[])?;
        if let Some(b) = bytes {
            f.id.send(r, f.alias, &[Operand::bytes(&b)])
        } else {
            f.id.send(r, f.alias, args)
        }
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
#[test]
#[ignore = "requires production native modules"]
fn portable_code_ordinary_objects_use_id_sends() {
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
        };
        let nil = h.named(b"nil");
        // Frozen golden fixtures cover byte names, all node forms, nested Blocks,
        // duplicate captures, let scope, selector arities and exact float bits.
        let fixture_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/portable-code-v1");
        let sums = std::fs::read_to_string(fixture_dir.join("SHA256SUMS")).unwrap();
        for line in sums.lines() {
            let (digest, file) = line.split_once("  ").unwrap();
            if !file.starts_with("program-") {
                continue;
            }
            let checkpoint = h.tokens.borrow().len();
            let bytes = std::fs::read(fixture_dir.join(file)).unwrap();
            let program = from_fixture(&h, &bytes);
            let expected = (0..digest.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&digest[i..i + 2], 16).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(h.digest(program), expected, "{file}");
            check_program(&h, program, &bytes);
            h.clear_from(checkpoint);
            collect(id, h.gc);
        }
        let node = h.named(b"CodeNode");
        let statement = h.named(b"CodeStatement");
        let program = h.named(b"CodeProgram");
        let code = h.named(b"CodeObject");
        let literal = h.literal(7);
        let empty = h.program(&[], &[], &[]);
        let captures = h.program(
            &[h.string(b"x")],
            &[h.string(b"held"), h.string(b"held")],
            &[],
        );
        let provenance = h.vector(&[literal]);
        let co = h.call(code, b"fromProgram:provenance:", &[captures, provenance]);
        assert_eq!(h.call(co, b"program", &[]), captures);
        assert_eq!(h.call(co, b"sourceProvenance", &[]), provenance);
        h.count(co, b"arity", 1);
        assert_eq!(
            h.call(co, b"format", &[]),
            id.intern_name(b"portableProgramV1").unwrap()
        );
        assert_eq!(h.call(co, b"clone", &[]), co);
        let schema = h.call(co, b"captureSchema", &[]);
        h.count(schema, b"size", 2);
        for i in 0..2 {
            let b = h.call(schema, b"at:", &[h.int(i)]);
            h.kind(b, b"capture");
            h.count(b, b"index", (1 + i) as usize);
        }
        let stripped = h.call(co, b"withoutSourceProvenance", &[]);
        assert_eq!(h.call(stripped, b"sourceProvenance", &[]), nil);
        assert_eq!(
            h.bytes(h.call(co, b"digest", &[])),
            h.bytes(h.call(stripped, b"digest", &[]))
        );
        let updated = h.call(co, b"withProgram:", &[empty]);
        assert_eq!(h.call(updated, b"program", &[]), empty);
        assert_eq!(h.call(updated, b"sourceProvenance", &[]), provenance);
        for proto in [
            node,
            statement,
            program,
            h.named(b"CodeLexicalBinding"),
            code,
        ] {
            assert_eq!(h.call(proto, b"clone", &[]), proto);
            assert_eq!(h.call(proto, b"clone", &[nil]), nil);
        }
        let error = |receiver, selector: &[u8], args: &[Oop], kind: &[u8], message: &[u8]| {
            let e = h.call(receiver, selector, args);
            h.kind(e, kind);
            assert_eq!(h.bytes(h.call(e, b"message", &[])), message);
        };
        error(
            node,
            b"integer:",
            &[],
            b"typeMismatch",
            b"integer: needs one Integer",
        );
        error(
            node,
            b"integer:",
            &[nil],
            b"typeMismatch",
            b"integer: needs an Integer",
        );
        error(
            node,
            b"floatBits:",
            &[h.string(b"short")],
            b"invalidPortableProgram",
            b"floatBits: needs exactly eight bytes",
        );
        error(
            node,
            b"lexicalReadAt:",
            &[h.int(-1)],
            b"invalidPortableProgram",
            b"lexical binding index is outside u32",
        );
        error(
            node,
            b"send:operands:",
            &[id.intern_name(b"+").unwrap(), h.vector(&[literal])],
            b"invalidPortableProgram",
            b"send operand count does not match selector arity",
        );
        error(
            node,
            b"send:operands:",
            &[id.intern_name(b"run").unwrap(), h.vector(&[nil])],
            b"typeMismatch",
            b"send operands must contain only CodeNodes",
        );
        error(
            node,
            b"block:captureBindings:",
            &[captures, h.vector(&[])],
            b"invalidPortableProgram",
            b"capture binding count does not match nested program",
        );
        error(
            statement,
            b"letNamed:expression:",
            &[h.string(b"bad\0name"), literal],
            b"invalidPortableProgram",
            b"Let binding name must be nonempty and NUL-free",
        );
        error(
            code,
            b"fromProgram:",
            &[nil],
            b"typeMismatch",
            b"CodeObject program must be a CodeProgram",
        );
        assert_eq!(h.call(statement, b"expression:", &[]), nil);
        let lexical = h.call(node, b"lexicalReadAt:", &[h.int(0)]);
        error(
            program,
            b"parameters:captures:statements:",
            &[
                h.vector(&[]),
                h.vector(&[]),
                h.vector(&[h.expression(lexical)]),
            ],
            b"invalidPortableProgram",
            b"CodeProgram semantic validation failed",
        );

        // Replacement methods remain live. Every collecting forward calls the
        // original binding via another send, never by invoking its native entry.
        let mut replacements = Vec::new();
        for (vt, selector) in [
            (h.named(b"StringVt"), b"fromBytes:".as_slice()),
            (h.named(b"IntegerVt"), b"fromWord:"),
            (h.named(b"IntegerVt"), b"wordOf:"),
            (h.named(b"VectorVt"), b"at:"),
            (h.named(b"VectorVt"), b"add:"),
            (id.receiver, b"allocate"),
        ] {
            let original = binding(&h, vt, selector);
            let alias_name = [b"testPortableOriginal".as_slice(), selector].concat();
            let alias = id.intern_name(&alias_name).unwrap();
            set(&h, vt, &alias_name, original);
            let f = Box::into_raw(Box::new(Forward {
                id,
                gc: h.gc,
                alias,
                calls: Cell::new(0),
                failure: Cell::new(false),
            }));
            set(&h, vt, selector, NativeEntry::new(collecting, f.cast()));
            replacements.push((vt, selector, original, f));
        }
        let bytes = std::fs::read(fixture_dir.join("program-nonutf8-names-block.bin")).unwrap();
        let p = from_fixture(&h, &bytes);
        check_program(&h, p, &bytes);
        for (_, _, _, f) in &replacements {
            assert!((**f).calls.get() > 0);
        }
        // Native failure payloads propagate intact through both constructors and getters.
        let from_word = replacements
            .iter()
            .find(|(_, name, _, _)| *name == b"fromWord:")
            .unwrap()
            .3;
        (*from_word).failure.set(true);
        let fail = id.send_named(literal, b"integer", &[]).unwrap_err();
        assert_eq!((fail.status, fail.detail), (0x73, 0x517));
        (*from_word).failure.set(false);
        for (vt, name, original, f) in replacements {
            set(&h, vt, name, original);
            drop(Box::from_raw(f));
        }

        // Derived factories allocate with the receiver's behavior and retain all
        // owner edges through the inherited message-based tracing protocol.
        let derived_vt = h.keep(object(send(id, h.named(b"CodeNodeVt"), b"delegated", &[])));
        let derived = h.keep(allocate(id, derived_vt, 0));
        let derived_node = h.call(derived, b"integer:", &[h.int(91)]);
        assert_eq!(behavior_of(&h, derived_node), derived_vt);
        h.count(derived_node, b"integer", 91);
        // Raw allocations cannot impersonate an admitted private value.
        let raw = h.keep(allocate(id, h.named(b"CodeNodeVt"), size_of::<usize>()));
        assert_eq!(h.call(raw, b"kind", &[]), nil);
        error(
            statement,
            b"expression:",
            &[raw],
            b"typeMismatch",
            b"expression: needs a CodeNode",
        );

        let checkpoint = h.tokens.borrow().len();
        let child = h.literal(101);
        let send_node = h.call(
            derived,
            b"send:operands:",
            &[id.intern_name(b"run").unwrap(), h.vector(&[child])],
        );
        let st = h.expression(send_node);
        let nested = h.program(&[], &[], &[]);
        let block = h.call(node, b"block:captureBindings:", &[nested, h.vector(&[])]);
        let capture_program = h.program(&[], &[h.string(b"kept")], &[]);
        let lexical_binding = h.call(capture_program, b"captureAt:", &[h.int(0)]);
        let prov = h.string(b"retained only by CodeObject");
        let object_program = h.program(&[], &[], &[]);
        let object_code = h.call(code, b"fromProgram:provenance:", &[object_program, prov]);
        let anchors = [st, block, lexical_binding, object_code];
        let tokens = anchors.map(|o| retain(id, h.roots, o));
        h.clear_from(checkpoint);
        collect(id, h.gc);
        for value in [
            st,
            send_node,
            child,
            block,
            nested,
            lexical_binding,
            capture_program,
            object_code,
            object_program,
            prov,
        ] {
            assert!(present(id, value));
        }
        for token in tokens {
            release(id, h.roots, token);
        }
        collect(id, h.gc);
        for value in [
            st,
            send_node,
            child,
            block,
            nested,
            lexical_binding,
            capture_program,
            object_code,
            object_program,
            prov,
        ] {
            assert!(!present(id, value));
        }
        // Reload preserves explicit namespace replacements and native entry edits.
        h.bind(b"CodeNode", nil);
        let path = directory().join("libbirnam_portable_code.dylib");
        assert!(
            id.send(
                id.receiver,
                id.load,
                &[
                    Operand::bytes(path.as_os_str().as_bytes()),
                    Operand::object(ns)
                ]
            )
            .unwrap()
            .kind()
                == Operand::unit().kind()
        );
        assert_eq!(h.named(b"CodeNode"), nil);
        h.bind(b"CodeNode", node);
        h.clear_from(0);
        collect(id, h.gc);
    }
}
