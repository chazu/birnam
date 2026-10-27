//! Source language, compilation failures and native code lifetime through Oops.
#[path = "compiler/accounting.rs"]
mod accounting;
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use birnam_host_support::Context;
use std::cell::RefCell;
struct H<'a> {
    host: &'a Context,
    hooks: oop::hooks::Hooks<'a>,
    tokens: RefCell<Vec<Oop>>,
    retain: Oop,
    release: Oop,
    floor_units: usize,
    floor_bytes: usize,
}
impl H<'_> {
    fn named(&self, n: &[u8]) -> Oop {
        self.host.named(n).unwrap()
    }
    fn key(&self, n: &[u8]) -> Oop {
        self.hooks.symbol(n)
    }
    fn keep(&self, v: Oop) -> Oop {
        self.tokens
            .borrow_mut()
            .push(unsafe { self.host.id.send(self.host.roots, self.retain, &[v]) }.unwrap());
        v
    }
    fn call(&self, r: Oop, n: &[u8], a: &[Oop]) -> Oop {
        self.keep(self.host.send(r, n, a).unwrap())
    }
    fn string(&self, b: &[u8]) -> Oop {
        let q = self.host.scope();
        self.keep(q.string(b).unwrap())
    }
    fn eval(&self, b: &[u8]) -> Oop {
        self.call(self.named(b"Codegen"), b"eval:", &[self.string(b)])
    }
    fn compile(&self, b: &[u8]) -> Oop {
        self.call(self.named(b"Codegen"), b"compile:", &[self.string(b)])
    }
    fn block(&self, m: Oop) -> Oop {
        self.call(
            self.named(b"Block"),
            b"fromMethod:captures:",
            &[m, self.call(self.named(b"Vector"), b"new", &[])],
        )
    }
    fn integer(&self, v: Oop) -> i64 {
        self.host
            .scope()
            .word_bits(self.call(self.named(b"Integer"), b"wordOf:", &[v]))
            .unwrap() as i64
    }
    fn bind(&self, n: &[u8], v: Oop) {
        self.call(self.host.namespace, b"at:put:", &[self.key(n), v]);
    }
    fn publish(&self, n: &[u8], v: Oop) {
        self.call(self.named(b"Globals"), b"at:put:", &[self.key(n), v]);
    }
    fn vt(&self, v: Oop) -> Oop {
        self.call(self.host.id.root, b"behaviorOf:", &[v])
    }
    fn collect(&self) {
        self.call(self.named(b"GC"), b"collectPrecisely", &[]);
        self.call(self.named(b"TCC"), b"checkpoint", &[]);
    }
    fn units(&self) -> usize {
        as_size(self.call(self.named(b"TCC"), b"liveNativeUnits", &[]))
            .unwrap()
            .checked_sub(self.floor_units)
            .expect("compiled core units remain live")
    }
    fn native_bytes(&self) -> usize {
        as_size(self.call(self.named(b"TCC"), b"liveNativeBytes", &[]))
            .expect("TCC resource-metrics required")
            .checked_sub(self.floor_bytes)
            .expect("compiled core bytes remain live")
    }
    fn clear(&self) {
        for t in self.tokens.take().into_iter().rev() {
            unsafe { self.host.id.send(self.host.roots, self.release, &[t]) }.unwrap();
        }
    }
    fn reclaimed(&self) {
        self.clear();
        self.collect();
        assert_eq!(self.units(), 0);
        assert_eq!(
            self.native_bytes(),
            0,
            "actual libtcc relocated bytes reclaimed"
        );
    }
}
impl Drop for H<'_> {
    fn drop(&mut self) {
        self.clear();
    }
}
#[test]
#[ignore = "requires production Oop modules with TCC resource metrics"]
fn source_compiles_and_runs_on_production_execution() {
    let host = oop::bootstrap();
    let floor = host.scope();
    let gc = host.named(b"GC").unwrap();
    floor
        .call(gc, b"precisePayloads:", &[size(1).unwrap()])
        .unwrap();
    host.global(&floor, b"Module").unwrap();
    let lang = host.named(b"Lang").unwrap();
    for name in [
        b"buildGrammar".as_slice(),
        b"initializeGrammar",
        b"grammar",
        b"parse:",
        b"furthestFail",
    ] {
        floor
            .call(
                host.id.root,
                b"bind:to:",
                &[host.intern(name).unwrap(), lang],
            )
            .unwrap();
    }
    floor.call(gc, b"collectPrecisely", &[]).unwrap();
    let tcc = host.named(b"TCC").unwrap();
    floor.call(tcc, b"checkpoint", &[]).unwrap();
    let h = H {
        host: &host,
        hooks: oop::hooks::Hooks::new(&host),
        tokens: RefCell::new(vec![]),
        retain: host.intern(b"retain:").unwrap(),
        release: host.intern(b"release:").unwrap(),
        floor_units: as_size(floor.call(tcc, b"liveNativeUnits", &[]).unwrap()).unwrap(),
        floor_bytes: as_size(floor.call(tcc, b"liveNativeBytes", &[]).unwrap())
            .expect("TCC resource-metrics required"),
    };
    for source in [
        b"[[Reflect vtableOf: \"hello\"] == StringVt]".as_slice(),
        b"[[Reflect globalNamed: #System] == System]",
        b"[[[[Reflect allGlobalNames] indexOf: #System] isNil] not]",
        b"[[Reflect allMethodsOf: System] inject: false into: [:found :entry | [found | [[entry at: #selector] == #println:]]]]",
    ] {assert_eq!(h.eval(source),h.named(b"true"));}
    for (source, expected) in [
        (b"42".as_slice(), 42),
        (b"[35 + 7]", 42),
        (b"[true ifTrue: [ | 42] ifFalse: [ | 7]]", 42),
        (b"[1 => + 2 => * 14]", 42),
        (
            b"let adder := [:x | [:y | [x + y]]]; [[adder value: 35] value: 7]",
            42,
        ),
        (b"let x := 35; [[:y | [x + y]] value: 7]", 42),
    ] {
        assert_eq!(h.integer(h.eval(source)), expected);
    }
    assert_eq!(
        h.integer(
            h.eval(b"[Reflect defineMethod: #answer on: Object as: [:self | 41]]; [Object answer]")
        ),
        41
    );
    let old = h.eval(b"[Object methodFor: #answer]");
    assert_eq!(
        h.integer(
            h.eval(b"[Reflect defineMethod: #answer on: Object as: [:self | 42]]; [Object answer]")
        ),
        42
    );
    assert_eq!(
        h.integer(h.call(old, b"invokeOn:", &[h.named(b"Object")])),
        41
    );
    h.call(
        h.vt(h.named(b"Object")),
        b"removeMethod:",
        &[h.key(b"answer")],
    );
    h.publish(b"adder", h.named(b"nil"));
    h.reclaimed();
    rejected_compilation(&h);
    lifetimes(&h);
    compiled_control(&h);
    collecting_factories(&h);
    h.reclaimed();
    compiled_recovery(&h);
    h.reclaimed();
    accounting::check(&h);
}
fn rejected_compilation(h: &H<'_>) {
    let syntax = h.eval(b"let :=");
    assert_eq!(h.call(syntax, b"kind", &[]), h.key(b"syntaxError"));
    assert_eq!(h.units(), 0);
    let error = h.call(h.named(b"TCC"), b"compile:", &[h.string(b"this is not C;")]);
    assert_eq!(h.call(error, b"kind", &[]), h.key(b"compileError"));
    assert_eq!(h.units(), 0);
    assert_eq!(h.native_bytes(), 0);
    let unit = h.call(
        h.named(b"TCC"),
        b"compile:",
        &[h.string(b"int ordinary_symbol = 42;")],
    );
    assert!(h.host.send(unit, b"initialize", &[]).is_err());
    assert_eq!(h.units(), 1);
    h.reclaimed();
}
fn lifetimes(h: &H<'_>) {
    let target = h.call(h.named(b"Object"), b"clone", &[]);
    h.bind(b"Target", target);
    h.eval(b"[Reflect defineMethod: #answer on: Target as: [:self | 41]]");
    let captured = h.call(target, b"methodFor:", &[h.key(b"answer")]);
    let pin = h.host.scope();
    pin.keep(captured).unwrap();
    h.clear();
    h.collect();
    let old_units = h.units();
    assert!(old_units > 0);
    assert!(h.native_bytes() > 0);
    assert_eq!(h.integer(h.call(target, b"answer", &[])), 41);
    h.eval(b"[Reflect defineMethod: #answer on: Target as: [:self | 42]]");
    h.clear();
    h.collect();
    assert!(h.units() > old_units);
    assert_eq!(h.integer(h.call(captured, b"invokeOn:", &[target])), 41);
    assert_eq!(h.integer(h.call(target, b"answer", &[])), 42);
    drop(pin);
    h.collect();
    assert_eq!(h.units(), old_units);
    let machine = h.call(h.named(b"Machine"), b"new", &[]);
    h.bind(b"Paused", machine);
    h.eval(b"[Reflect defineMethod: #answer on: Target as: [:self | [Paused park]; 73]]");
    let block = h.block(h.compile(b"[Target answer]"));
    assert_eq!(h.call(machine, b"runBlock:", &[block]), machine);
    h.clear();
    h.collect();
    let waiting_units = h.units();
    assert!(waiting_units > old_units);
    h.eval(b"[Reflect defineMethod: #answer on: Target as: [:self | 99]]");
    h.clear();
    h.collect();
    assert!(h.units() > waiting_units);
    assert_eq!(
        h.integer(h.call(machine, b"resumeWith:", &[h.named(b"nil")])),
        73
    );
    h.bind(b"Paused", h.named(b"nil"));
    h.clear();
    h.collect();
    assert_eq!(h.units(), old_units);
    assert_eq!(h.integer(h.call(target, b"answer", &[])), 99);
    h.call(h.vt(target), b"removeMethod:", &[h.key(b"answer")]);
    h.bind(b"Target", h.named(b"nil"));
    h.reclaimed();
}
fn compiled_control(h: &H<'_>) {
    let machine = h.call(h.named(b"Machine"), b"new", &[]);
    h.bind(b"ControlMachine", machine);
    let method =
        h.compile(b"let result := [ControlMachine protectedBlock: [ | [1 + 2]; 99]]; [result + 1]");
    let block = h.block(method);
    let budget = h.eval(b"3");
    assert_eq!(
        h.call(machine, b"runBlock:budget:", &[block, budget]),
        machine
    );
    let delivered = h.eval(b"42");
    h.call(machine, b"exitRunnableBoundary:", &[delivered]);
    h.clear();
    h.collect();
    assert!(h.units() > 0, "boundary continuation retains compiled code");
    assert_eq!(
        h.call(machine, b"boundaryDeliveryPending", &[]),
        h.named(b"true")
    );
    let budget = h.eval(b"100");
    assert_eq!(
        h.integer(h.call(machine, b"continueWithBudget:", &[budget])),
        43,
        "exit delivers into the generated caller once"
    );
    h.bind(b"ControlMachine", h.named(b"nil"));
    h.reclaimed();
    let machine = h.call(h.named(b"Machine"), b"new", &[]);
    h.bind(b"ControlMachine", machine);
    let block = h.block(h.compile(b"[ControlMachine park]; 73"));
    assert_eq!(h.call(machine, b"runBlock:", &[block]), machine);
    h.clear();
    h.collect();
    assert!(h.units() > 0);
    h.call(machine, b"destroy", &[]);
    h.collect();
    assert_eq!(h.units(), 0, "abandoning a waiting frame releases its code");
    assert_eq!(h.native_bytes(), 0);
    h.bind(b"ControlMachine", h.named(b"nil"));
    h.reclaimed();
}
fn collecting_factories(h: &H<'_>) {
    let integer = h.named(b"Integer");
    let vt = h.vt(integer);
    let selector = h.key(b"fromWord:");
    let original = h.call(h.host.id.root, b"bind:to:", &[selector, integer]);
    let context = h
        .hooks
        .context(&[original, h.named(b"GC"), size(0).unwrap()]);
    h.call(
        vt,
        b"addMethod",
        &[selector, h.hooks.method(b"collectEvery", context)],
    );
    assert_eq!(
        h.integer(h.eval(b"let capture := 35; [[:x | [capture + x]] value: 7]")),
        42
    );
    assert!(h.hooks.count(context, 2) > 10);
    h.call(vt, b"addMethod", &[selector, original]);
}

fn compiled_recovery(h: &H<'_>) {
    let block = h.eval(b"[ | 42]");
    h.bind(b"RecoveryTarget", block);
    let baseline = h.call(block, b"methodFor:", &[h.key(b"value")]);
    h.collect();
    let baseline_units = h.units();
    let baseline_bytes = h.native_bytes();
    let header = include_str!("../../../tests/fixtures/oop-id-v2/id.h");
    let fixture = include_str!("../../../tests/fixtures/oop-host/compiler-recovery.c")
        .replace("#include \"../oop-id-v2/id.h\"", "");
    let source = h.string(format!("{header}\n{fixture}").as_bytes());
    let unit = h.call(h.named(b"TCC"), b"compile:", &[source]);
    let candidate = h.call(unit, b"initializeWith:", &[baseline]);
    assert_eq!(h.call(unit, b"initializeWith:", &[baseline]), candidate);
    assert!(h.host.send(unit, b"initialize", &[]).is_err());
    assert!(h.host.send(unit, b"initializeWith:", &[block]).is_err());
    let target_vt = h.call(h.host.id.root, b"ensurePrivateVtableFor:", &[block]);
    h.call(target_vt, b"addMethod:as:", &[candidate, h.key(b"value")]);
    h.clear();
    h.collect();
    assert_eq!(h.units(), baseline_units + 1);
    let answer = h.eval(b"let recovered := [RecoveryTarget value]; [recovered + 1]");
    h.host.scope().check_result(answer).unwrap();
    assert_eq!(h.integer(answer), 43);
    assert_eq!(h.named(b"Recovered"), h.named(b"true"));
    h.clear();
    h.collect();
    assert_eq!(
        h.units(),
        baseline_units,
        "entered candidate mapping retires only after recovery returns"
    );
    assert_eq!(h.native_bytes(), baseline_bytes);
    h.bind(b"RecoveryTarget", h.named(b"nil"));
    h.reclaimed();
}
