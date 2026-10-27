//! PortableCode ordinary objects through the production Oop loader.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use birnam_host_support::Context;
use std::{cell::RefCell, path::Path};
struct H<'a> {
    host: &'a Context,
    roots: Oop,
    gc: Oop,
    retain_key: Oop,
    release_key: Oop,
    tokens: RefCell<Vec<Oop>>,
    hooks: oop::hooks::Hooks<'a>,
    fixture: Oop,
}
impl H<'_> {
    fn named(&self, n: &[u8]) -> Oop {
        self.host.named(n).unwrap()
    }
    fn key(&self, n: &[u8]) -> Oop {
        self.hooks.symbol(n)
    }
    fn retain(&self, v: Oop) -> Oop {
        unsafe { self.host.id.send(self.roots, self.retain_key, &[v]) }.unwrap()
    }
    fn release(&self, t: Oop) {
        unsafe { self.host.id.send(self.roots, self.release_key, &[t]) }.unwrap();
    }
    fn keep(&self, v: Oop) -> Oop {
        let token = self.retain(v);
        self.tokens.borrow_mut().push(token);
        v
    }
    fn call(&self, r: Oop, n: &[u8], a: &[Oop]) -> Oop {
        self.keep(self.host.send(r, n, a).unwrap())
    }
    fn string(&self, b: &[u8]) -> Oop {
        let values = b
            .iter()
            .map(|&v| size(v as usize).unwrap())
            .collect::<Vec<_>>();
        let bytes = self.call(self.host.id.root, b"sequence", &values);
        self.call(self.named(b"String"), b"fromBytes:", &[bytes])
    }
    fn number(&self, v: Oop) -> usize {
        self.host
            .scope()
            .word_bits(self.call(self.named(b"Integer"), b"wordOf:", &[v]))
            .unwrap()
    }
    fn bind(&self, n: &[u8], v: Oop) {
        self.call(self.host.namespace, b"at:put:", &[self.key(n), v]);
    }
    fn bytes(&self, v: Oop) -> Vec<u8> {
        self.host
            .scope()
            .read_bytes(self.call(v, b"bytes", &[]))
            .unwrap()
    }
    fn int(&self, n: i64) -> Oop {
        let q = self.host.scope();
        let bits = q.word(n as usize).unwrap();
        self.call(self.named(b"Integer"), b"fromWord:", &[bits])
    }
    fn vector(&self, values: &[Oop]) -> Oop {
        let mut v = self.call(self.named(b"Vector"), b"new", &[]);
        for &x in values {
            v = self.call(v, b"add:", &[x]);
        }
        v
    }
    fn program(&self, p: &[Oop], c: &[Oop], s: &[Oop]) -> Oop {
        self.call(
            self.named(b"CodeProgram"),
            b"parameters:captures:statements:",
            &[self.vector(p), self.vector(c), self.vector(s)],
        )
    }
    fn expression(&self, n: Oop) -> Oop {
        self.call(self.named(b"CodeStatement"), b"expression:", &[n])
    }
    fn literal(&self, n: i64) -> Oop {
        self.call(self.named(b"CodeNode"), b"integer:", &[self.int(n)])
    }
    fn digest(&self, p: Oop) -> Vec<u8> {
        let c = self.call(self.named(b"CodeObject"), b"fromProgram:", &[p]);
        self.bytes(self.call(c, b"digest", &[]))
    }
    fn kind(&self, r: Oop, k: &[u8]) {
        assert_eq!(self.call(r, b"kind", &[]), self.key(k));
    }
    fn count(&self, r: Oop, n: &[u8], c: usize) {
        assert_eq!(self.number(self.call(r, n, &[])), c);
    }
    fn clear_from(&self, i: usize) {
        let tokens = self.tokens.borrow_mut().split_off(i);
        for token in tokens {
            self.release(token);
        }
    }
    fn collect(&self) {
        self.call(self.gc, b"collectPrecisely", &[]);
    }
    fn method(&self, n: &[u8], c: Oop) -> Oop {
        self.call(
            self.call(self.fixture, b"methodAt:", &[self.key(n)]),
            b"withContext:",
            &[c],
        )
    }
}
impl Drop for H<'_> {
    fn drop(&mut self) {
        self.clear_from(0);
    }
}
fn binding(h: &H<'_>, v: Oop, n: &[u8]) -> Oop {
    h.call(v, b"lookup", &[h.key(n)])
}
fn set(h: &H<'_>, v: Oop, n: &[u8], m: Oop) {
    h.call(v, b"addMethod", &[h.key(n), m]);
}
fn behavior_of(h: &H<'_>, v: Oop) -> Oop {
    h.call(h.host.id.root, b"behaviorOf:", &[v])
}
use birnam_artifact_codec::portable_program as portable;
// Reify frozen fixture semantics using the public constructors, then check their
// digest against the independent, checked-in golden bytes' SHA256SUMS.
fn from_fixture(h: &H<'_>, bytes: &[u8]) -> Oop {
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
                &[h.key(n.data)],
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
                    &[h.key(n.data), h.vector(&operands)],
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
fn check_node(h: &H<'_>, node: Oop, p: &portable::Program<'_>, index: usize) {
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
            h.key(n.data)
        ),
        8 => h.count(node, b"bindingIndex", n.auxiliary_0 as usize),
        10 => {
            assert_eq!(h.call(node, b"selector", &[]), h.key(n.data));
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
fn check_program(h: &H<'_>, program: Oop, bytes: &[u8]) {
    let p = portable::inspect(bytes).unwrap();
    let nil = h.named(b"nil");
    assert_eq!(h.call(program, b"format", &[]), h.key(b"portableProgramV1"));
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
#[test]
#[ignore = "requires production Oop modules"]
fn portable_code_ordinary_objects_use_id_sends() {
    let host = oop::bootstrap();
    let scope = host.scope();
    let path =
        std::env::temp_dir().join(format!("birnam-portable-code-{}.dylib", std::process::id()));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/portable-code.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = host.load_native(&scope, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let h = H {
        host: &host,
        roots: host.roots,
        gc: host.named(b"GC").unwrap(),
        retain_key: host.intern(b"retain:").unwrap(),
        release_key: host.intern(b"release:").unwrap(),
        tokens: RefCell::new(vec![]),
        hooks: oop::hooks::Hooks::new(&host),
        fixture,
    };
    h.call(h.gc, b"precisePayloads:", &[size(1).unwrap()]);
    let monitor = h.call(h.named(b"Object"), b"clone", &[]);
    let monitor_vt = h.call(host.id.root, b"ensurePrivateVtableFor:", &[monitor]);
    for n in [
        b"watch".as_slice(),
        b"remaining",
        b"allocation:bytes:identity:",
    ] {
        set(&h, monitor_vt, n, h.method(n, monitor));
    }
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
        h.collect();
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
    assert_eq!(h.call(co, b"format", &[]), h.key(b"portableProgramV1"));
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
        &[h.key(b"+"), h.vector(&[literal])],
        b"invalidPortableProgram",
        b"send operand count does not match selector arity",
    );
    error(
        node,
        b"send:operands:",
        &[h.key(b"run"), h.vector(&[nil])],
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
        (behavior_of(&h, h.named(b"CodeNodeVt")), b"allocate"),
    ] {
        let original = binding(&h, vt, selector);
        let alias_name = [b"testPortableOriginal".as_slice(), selector].concat();
        let alias = h.key(&alias_name);
        set(&h, vt, &alias_name, original);
        let state = h
            .hooks
            .context(&[h.gc, alias, size(0).unwrap(), std::ptr::null_mut()]);
        set(&h, vt, selector, h.method(b"forward", state));
        replacements.push((vt, selector, original, state));
    }
    let bytes = std::fs::read(fixture_dir.join("program-nonutf8-names-block.bin")).unwrap();
    let p = from_fixture(&h, &bytes);
    check_program(&h, p, &bytes);
    for (_, _, _, f) in &replacements {
        assert!(h.hooks.count(*f, 2) > 0);
    }
    // Native failure payloads propagate intact through both constructors and getters.
    let from_word = replacements
        .iter()
        .find(|(_, name, _, _)| *name == b"fromWord:")
        .unwrap()
        .3;
    let fault = h.hooks.fault(0x517);
    h.hooks.put(from_word, 3, fault);
    assert_eq!(
        unsafe { h.host.id.send(literal, h.key(b"integer"), &[]) }.unwrap_err(),
        fault
    );
    h.hooks.put(from_word, 3, std::ptr::null_mut());
    for (vt, name, original, _) in replacements {
        set(&h, vt, name, original);
        let alias = [b"testPortableOriginal".as_slice(), name].concat();
        h.call(vt, b"removeMethod:", &[h.key(&alias)]);
    }

    // Derived factories allocate with the receiver's behavior and retain all
    // owner edges through the inherited message-based tracing protocol.
    let derived_vt = h.call(h.named(b"CodeNodeVt"), b"delegated", &[]);
    let derived = h.call(derived_vt, b"allocate", &[size(0).unwrap()]);
    let derived_node = h.call(derived, b"integer:", &[h.int(91)]);
    assert_eq!(behavior_of(&h, derived_node), derived_vt);
    h.count(derived_node, b"integer", 91);
    // Raw allocations cannot impersonate an admitted private value.
    let raw = h.call(
        h.named(b"CodeNodeVt"),
        b"allocate",
        &[size(size_of::<usize>()).unwrap()],
    );
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
        &[h.key(b"run"), h.vector(&[child])],
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
    let tokens = anchors.map(|o| h.retain(o));
    let observed = [
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
    ];
    h.host.send(monitor, b"watch", &observed).unwrap();
    h.clear_from(checkpoint);
    h.collect();
    assert_eq!(
        as_size(h.call(monitor, b"remaining", &[])),
        Some(observed.len())
    );
    for token in tokens {
        h.release(token);
    }
    h.collect();
    assert_eq!(as_size(h.call(monitor, b"remaining", &[])), Some(0));
    // Reload preserves explicit namespace replacements and Method bindings.
    h.bind(b"CodeNode", nil);
    let scope = host.scope();
    assert_eq!(
        host.load_native(
            &scope,
            &host.resources.lib_dir.join("libbirnam_portable_code.dylib"),
            &[host.namespace],
        )
        .unwrap(),
        node
    );
    assert_eq!(h.named(b"CodeNode"), nil);
    h.bind(b"CodeNode", node);
    h.clear_from(0);
    h.collect();
}
