//! Source compiler and real TCC mappings through production Bootstrap.
mod support;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use core::{ffi::c_void, ptr};
use std::{cell::Cell, os::unix::ffi::OsStrExt};
use support::*;
struct H {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    tokens: Vec<usize>,
}
impl H {
    unsafe fn named(&self, n: &[u8]) -> Oop {
        unsafe {
            object(send(
                self.id,
                self.ns,
                b"at:",
                &[Operand::object(self.id.intern_name(n).unwrap())],
            ))
        }
    }
    unsafe fn keep(&mut self, o: Oop) -> Oop {
        self.tokens.push(unsafe { retain(self.id, self.roots, o) });
        o
    }
    unsafe fn string(&mut self, b: &[u8]) -> Oop {
        let v = unsafe {
            object(send(
                self.id,
                self.named(b"String"),
                b"fromBytes:",
                &[Operand::bytes(b)],
            ))
        };
        unsafe { self.keep(v) }
    }
    unsafe fn eval(&mut self, b: &[u8]) -> Oop {
        unsafe {
            let text = self.string(b);
            let value = object(
                self.id
                    .send_named(self.named(b"Codegen"), b"eval:", &[Operand::object(text)])
                    .unwrap_or_else(|e| panic!("eval {}: {e:?}", String::from_utf8_lossy(b))),
            );
            self.keep(value)
        }
    }
    unsafe fn clear(&mut self) {
        for t in self.tokens.drain(..) {
            unsafe {
                release(self.id, self.roots, t);
            }
        }
    }
    unsafe fn collect(&self) {
        unsafe {
            collect(self.id, self.named(b"GC"));
            send(self.id, self.named(b"TCC"), b"checkpoint", &[]);
        }
    }
    unsafe fn native_bytes(&self) -> usize {
        unsafe {
            send(self.id, self.named(b"TCC"), b"liveNativeBytes", &[])
                .as_word()
                .unwrap()
        }
    }
    unsafe fn units(&self) -> usize {
        unsafe {
            send(self.id, self.named(b"TCC"), b"liveNativeUnits", &[])
                .as_word()
                .unwrap()
        }
    }
    unsafe fn bind(&self, n: &[u8], o: Oop) {
        unsafe {
            send(
                self.id,
                self.ns,
                b"at:put:",
                &[
                    Operand::object(self.id.intern_name(n).unwrap()),
                    Operand::object(o),
                ],
            );
        }
    }
    unsafe fn compile(&mut self, b: &[u8]) -> Oop {
        unsafe {
            let text = self.string(b);
            let v = object(send(
                self.id,
                self.named(b"Codegen"),
                b"compile:",
                &[Operand::object(text)],
            ));
            self.keep(v)
        }
    }
    unsafe fn block(&mut self, method: Oop) -> Oop {
        unsafe {
            let captures = object(send(self.id, self.named(b"Vector"), b"new", &[]));
            self.keep(captures);
            let block = object(send(
                self.id,
                self.named(b"Block"),
                b"fromMethod:captures:",
                &[Operand::object(method), Operand::object(captures)],
            ));
            self.keep(block)
        }
    }
    unsafe fn integer(&self, o: Oop) -> i64 {
        unsafe {
            send(
                self.id,
                self.named(b"Integer"),
                b"wordOf:",
                &[Operand::object(o)],
            )
            .as_word()
            .unwrap_or_else(|| panic!("not Integer: {:?}", send(self.id, o, b"kind", &[])))
                as i64
        }
    }
}
#[test]
#[ignore = "requires native prefix"]
fn source_compiles_and_runs_on_production_execution() {
    unsafe {
        let id = id();
        let ns = load(
            id,
            "bootstrap",
            &[Operand::bytes(directory().as_os_str().as_bytes())],
        );
        let roots = object(send(
            id,
            ns,
            b"at:",
            &[Operand::object(id.intern_name(b"Roots").unwrap())],
        ));
        let mut h = H {
            id,
            ns,
            roots,
            tokens: vec![],
        };
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
            let v = h.eval(source);
            assert_eq!(h.integer(v), expected);
        }
        let v =
            h.eval(b"[Reflect defineMethod: #answer on: Object as: [:self | 41]]; [Object answer]");
        assert_eq!(h.integer(v), 41);
        let old = h.eval(b"[Object methodFor: #answer]");
        let v =
            h.eval(b"[Reflect defineMethod: #answer on: Object as: [:self | 42]]; [Object answer]");
        assert_eq!(h.integer(v), 42);
        let v = object(send(
            id,
            old,
            b"invokeOn:",
            &[Operand::object(h.named(b"Object"))],
        ));
        assert_eq!(h.integer(v), 41);
        let vt = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(h.named(b"Object"))],
        ));
        send(
            id,
            vt,
            b"removeMethod:",
            &[Operand::object(id.intern_name(b"answer").unwrap())],
        );
        h.bind(b"adder", h.named(b"nil"));
        h.clear();
        h.collect();
        assert_eq!(h.units(), 0, "all initial compiled mappings reclaimed");
        rejected_compilation(&mut h);
        lifetimes(&mut h);
        compiled_control(&mut h);
        collecting_factories(&mut h);
        compiled_recovery(&mut h);
        h.clear();
        h.collect();
        assert_eq!(h.units(), 0);
        assert_eq!(
            h.native_bytes(),
            0,
            "actual libtcc relocated bytes reclaimed"
        );
    }
}

unsafe fn rejected_compilation(h: &mut H) {
    unsafe {
        let syntax = h.eval(b"let :=");
        assert_eq!(
            object(send(h.id, syntax, b"kind", &[])),
            h.id.intern_name(b"syntaxError").unwrap()
        );
        assert_eq!(h.units(), 0, "syntax failure never creates a native Unit");
        let invalid_c = h.string(b"this is not C;");
        let error = object(send(
            h.id,
            h.named(b"TCC"),
            b"compile:",
            &[Operand::object(invalid_c)],
        ));
        h.keep(error);
        assert_eq!(
            object(send(h.id, error, b"kind", &[])),
            h.id.intern_name(b"compileError").unwrap()
        );
        assert_eq!(h.units(), 0);
        assert_eq!(h.native_bytes(), 0);
        // Compilation can succeed without a valid module initializer. Failed
        // admission must leave the owning Unit reclaimable, including its data.
        let source = h.string(b"int ordinary_symbol = 42;");
        let unit = object(send(
            h.id,
            h.named(b"TCC"),
            b"compile:",
            &[Operand::object(source)],
        ));
        h.keep(unit);
        assert!(h.id.send_named(unit, b"initialize", &[]).is_err());
        assert_eq!(h.units(), 1);
        h.clear();
        h.collect();
        assert_eq!(h.units(), 0);
        assert_eq!(h.native_bytes(), 0);
    }
}

unsafe fn lifetimes(h: &mut H) {
    unsafe {
        let id = h.id;
        let target = object(send(id, h.named(b"Object"), b"clone", &[]));
        h.keep(target);
        h.bind(b"Target", target);
        h.eval(b"[Reflect defineMethod: #answer on: Target as: [:self | 41]]");
        let selector = id.intern_name(b"answer").unwrap();
        let captured = object(send(
            id,
            target,
            b"methodFor:",
            &[Operand::object(selector)],
        ));
        let pin = retain(id, h.roots, captured);
        h.clear();
        h.collect();
        let old_units = h.units();
        assert!(old_units > 0);
        assert!(h.native_bytes() > 0);
        assert_eq!(h.integer(object(send(id, target, b"answer", &[]))), 41);
        h.eval(b"[Reflect defineMethod: #answer on: Target as: [:self | 42]]");
        h.clear();
        h.collect();
        assert!(h.units() > old_units);
        assert_eq!(
            h.integer(object(send(
                id,
                captured,
                b"invokeOn:",
                &[Operand::object(target)]
            ))),
            41
        );
        assert_eq!(h.integer(object(send(id, target, b"answer", &[]))), 42);
        release(id, h.roots, pin);
        h.collect();
        assert_eq!(h.units(), old_units);
        // Compiled caller and body remain alive with only a waiting Machine and
        // current namespace/bindings. Replacement must not unmap the paused body.
        let machine = object(send(id, h.named(b"Machine"), b"new", &[]));
        h.bind(b"Paused", machine);
        h.eval(b"[Reflect defineMethod: #answer on: Target as: [:self | [Paused park]; 73]]");
        let caller = h.compile(b"[Target answer]");
        let block = h.block(caller);
        assert_eq!(
            object(send(id, machine, b"runBlock:", &[Operand::object(block)])),
            machine
        );
        h.clear();
        h.collect();
        let waiting_units = h.units();
        assert!(waiting_units > old_units);
        h.eval(b"[Reflect defineMethod: #answer on: Target as: [:self | 99]]");
        h.clear();
        h.collect();
        assert!(h.units() > waiting_units);
        let answer = object(send(
            id,
            machine,
            b"resumeWith:",
            &[Operand::object(h.named(b"nil"))],
        ));
        assert_eq!(h.integer(answer), 73);
        h.bind(b"Paused", h.named(b"nil"));
        h.collect();
        assert_eq!(h.units(), old_units);
        assert_eq!(h.integer(object(send(id, target, b"answer", &[]))), 99);
        let vt = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(target)],
        ));
        send(id, vt, b"removeMethod:", &[Operand::object(selector)]);
        h.bind(b"Target", h.named(b"nil"));
        h.collect();
        assert_eq!(h.units(), 0);
        assert_eq!(
            h.native_bytes(),
            0,
            "actual libtcc relocated bytes reclaimed"
        );
    }
}
unsafe fn compiled_control(h: &mut H) {
    unsafe {
        let id = h.id;
        let machine = object(send(id, h.named(b"Machine"), b"new", &[]));
        h.bind(b"ControlMachine", machine);
        let method = h.compile(
            b"let result := [ControlMachine protectedBlock: [ | [1 + 2]; 99]]; [result + 1]",
        );
        let block = h.block(method);
        let budget = h.eval(b"3");
        assert_eq!(
            object(send(
                id,
                machine,
                b"runBlock:budget:",
                &[Operand::object(block), Operand::object(budget)]
            )),
            machine
        );
        let delivered = h.eval(b"42");
        send(
            id,
            machine,
            b"exitRunnableBoundary:",
            &[Operand::object(delivered)],
        );
        h.clear();
        h.collect();
        assert!(h.units() > 0, "boundary continuation retains compiled code");
        assert_eq!(
            object(send(id, machine, b"boundaryDeliveryPending", &[])),
            h.named(b"true")
        );
        let budget = h.eval(b"100");
        let result = object(send(
            id,
            machine,
            b"continueWithBudget:",
            &[Operand::object(budget)],
        ));
        assert_eq!(
            h.integer(result),
            43,
            "exit delivers into the generated caller once"
        );
        h.bind(b"ControlMachine", h.named(b"nil"));
        h.clear();
        h.collect();
        assert_eq!(h.units(), 0);

        let machine = object(send(id, h.named(b"Machine"), b"new", &[]));
        h.bind(b"ControlMachine", machine);
        let method = h.compile(b"[ControlMachine park]; 73");
        let block = h.block(method);
        assert_eq!(
            object(send(id, machine, b"runBlock:", &[Operand::object(block)])),
            machine
        );
        h.clear();
        h.collect();
        assert!(h.units() > 0);
        send(id, machine, b"destroy", &[]);
        h.collect();
        assert_eq!(
            h.units(),
            0,
            "abandoning the waiting frame releases its compiled Unit"
        );
        assert_eq!(h.native_bytes(), 0);
        h.bind(b"ControlMachine", h.named(b"nil"));
    }
}

struct Factory {
    id: Protocol,
    gc: Oop,
    entry: NativeEntry,
    calls: Cell<usize>,
}
unsafe extern "C" fn collecting(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let c = &*c.cast::<Factory>();
        let args = borrowed_slice(a, n).unwrap();
        c.calls.set(c.calls.get() + 1);
        collect(c.id, c.gc);
        c.entry.method.unwrap()(c.entry.context, r, args.as_ptr(), args.len())
    }
}
unsafe fn collecting_factories(h: &mut H) {
    unsafe {
        let id = h.id;
        let integer = h.named(b"Integer");
        let vt = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(integer)],
        ));
        let sel = id.intern_name(b"fromWord:").unwrap();
        let original = send(
            id,
            id.receiver,
            b"bind:to:",
            &[Operand::object(sel), Operand::object(integer)],
        )
        .as_entry()
        .unwrap();
        let factory = Box::leak(Box::new(Factory {
            id,
            gc: h.named(b"GC"),
            entry: original,
            calls: Cell::new(0),
        }));
        install(
            id,
            vt,
            b"fromWord:",
            collecting,
            ptr::from_mut(factory).cast(),
        );
        let result = h.eval(b"let capture := 35; [[:x | [capture + x]] value: 7]");
        assert_eq!(h.integer(result), 42);
        assert!(factory.calls.get() > 10);
        send(
            id,
            vt,
            b"addMethod",
            &[Operand::object(sel), Operand::entry(original)],
        );
    }
}

unsafe fn compiled_recovery(h: &mut H) {
    unsafe {
        h.clear();
        h.collect();
        assert_eq!(h.units(), 0);
        assert_eq!(
            h.native_bytes(),
            0,
            "actual libtcc relocated bytes reclaimed"
        );
        let id = h.id;
        let block = h.eval(b"[ | 42]");
        h.bind(b"RecoveryTarget", block);
        let baseline = object(send(
            id,
            block,
            b"methodFor:",
            &[Operand::object(id.intern_name(b"value").unwrap())],
        ));
        h.keep(baseline);
        let prefix = include_str!("../../../abi/fixtures/id-protocol-v1/id_protocol.h");
        let fixture = include_str!("../../../abi/fixtures/id-protocol-v1/compiled_recovery.c")
            .replace("#include \"id_protocol.h\"", "");
        let source = h.string(format!("{prefix}\n{fixture}").as_bytes());
        let unit = object(send(
            id,
            h.named(b"TCC"),
            b"compile:",
            &[Operand::object(source)],
        ));
        h.keep(unit);
        let candidate = object(send(
            id,
            unit,
            b"initializeWith:",
            &[Operand::object(baseline)],
        ));
        h.keep(candidate);
        assert_eq!(
            object(send(
                id,
                unit,
                b"initializeWith:",
                &[Operand::object(baseline)]
            )),
            candidate,
            "reinitialization preserves the exact captured result"
        );
        assert!(id.send_named(unit, b"initialize", &[]).is_err());
        assert!(
            id.send_named(unit, b"initializeWith:", &[Operand::object(block)])
                .is_err()
        );
        let vt = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(block)],
        ));
        send(
            id,
            vt,
            b"addMethod:as:",
            &[
                Operand::object(candidate),
                Operand::object(id.intern_name(b"value").unwrap()),
            ],
        );
        h.clear();
        h.collect();
        assert!(present(id, unit));
        let answer = h.eval(b"let recovered := [RecoveryTarget value]; [recovered + 1]");
        assert_eq!(h.integer(answer), 43);
        assert_eq!(h.named(b"Recovered"), h.named(b"true"));
        h.clear();
        h.collect();
        assert!(
            !present(id, unit),
            "candidate Unit reclaimed after entered recovery returns"
        );
        h.bind(b"RecoveryTarget", h.named(b"nil"));
        h.collect();
        assert_eq!(h.units(), 0);
        assert_eq!(
            h.native_bytes(),
            0,
            "actual libtcc relocated bytes reclaimed"
        );
    }
}
