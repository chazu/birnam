//! A real Birnam replacement, including collection and nested stackless control.
use super::*;
use super::discarded_clone::{lookup, restore, vt};
struct Replacement { target: Oop, authored: NativeEntry, original: NativeEntry, calls: usize, active: bool, helper_calls: usize }
unsafe extern "C" fn replace(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        // A metacircular replacement still needs the bootstrap implementation
        // for its own ABI/factory helpers. It never forwards the selected target
        // transfer; that transfer is performed entirely by the authored body.
        let fixture = c.cast::<Replacement>();
        let target = r == (*fixture).target && !(*fixture).active;
        let entry = if target {
            (*fixture).calls += 1;
            (*fixture).active = true;
            (*fixture).authored
        } else {
            (*fixture).helper_calls += 1;
            (*fixture).original
        };
        let result = entry.method.unwrap()(entry.context, r, a, n);
        if target { (*fixture).active = false; }
        result
    }
}
unsafe extern "C" fn failed(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::failed(6, 71)
}
pub(super) unsafe fn check(h: &mut H) {
    unsafe {
        let id = h.id;
        let body = h.eval(include_bytes!("../../../../tests/fixtures/execution/prepared-call-reference.birnam"));
        h.bind(b"PreparedReference", body);
        let holder = h.eval(b"[[| let holder := [Object clone]; let method := [[TCC compile: [NativeBackend named: 2 synchronous: true]] initializeWith: PreparedReference]; [[Reflect vtableOf: holder] addMethod: method as: #prepareCall:selector:]; holder] value]");
        let authored = lookup(h, holder, b"prepareCall:selector:");
        let argument = h.eval(b"[Object clone]");
        h.bind(b"PreparedArgument", argument);
        let block = h.eval(b"[| let a := PreparedArgument; let b := a; let c := b; let first := [[:x :y :z | y] value: a value: b value: c]; let next := first; let last := next; [[:x :y | y] value: next value: last]]");
        let target = object(send(id, h.named(b"Machine"), b"new", &[]));
        h.keep(target);
        let behavior = vt(h, target);
        let original = lookup(h, target, b"prepareCall:selector:");
        let mut fixture = Replacement { target, authored, original, calls: 0, active: false, helper_calls: 0 };
        install(id, behavior, b"prepareCall:selector:", replace, ptr::from_mut(&mut fixture).cast());
        let result = id.send_named(target, b"runBlock:", &[Operand::object(block)]);
        restore(h, behavior, b"prepareCall:selector:", original);
        assert_eq!(object(result.unwrap()), argument, "authored transfer preserves exact identity");
        assert!(fixture.calls >= 2, "normal and tail calls must exercise the authored implementation");
        let failed_machine = object(send(id, h.named(b"Machine"), b"new", &[]));
        h.keep(failed_machine);
        install(id, behavior, b"prepareCall:selector:", failed, ptr::null_mut());
        let failure = id.send_named(failed_machine, b"runBlock:", &[Operand::object(block)]);
        restore(h, behavior, b"prepareCall:selector:", original);
        let failure = failure.unwrap_err();
        assert_eq!((failure.status, failure.detail), (6, 71), "entered coarse failure must not replay legacy preparation");
        // Replacement has gone away; a subsequent fresh run sees the native body.
        assert_eq!(discarded_clone::run(h, block, true), argument);
        send(id, vt(h, holder), b"removeMethod:", &[Operand::object(id.intern_name(b"prepareCall:selector:").unwrap())]);
        h.bind(b"PreparedReference", h.named(b"nil"));
        h.bind(b"PreparedArgument", h.named(b"nil"));
        send(id, h.named(b"Codegen"), b"evictMaterializations", &[]);
        h.clear();
        h.collect();
    }
}
