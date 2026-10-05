//! Producer-proven alias returns reuse existing inline admission and fallback.
use super::*;
use super::discarded_clone::{lookup, restore, run, vt};
use std::time::Instant;
unsafe extern "C" fn baseline(_: *mut c_void, r: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::object(r))
}
struct Factory { original: NativeEntry, calls: Cell<usize> }
unsafe extern "C" fn factory(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    let f = unsafe { &*c.cast::<Factory>() }; f.calls.set(f.calls.get()+1);
    unsafe { f.original.method.unwrap()(f.original.context,r,a,n) }
}
pub(super) unsafe fn check(h: &mut H) {
    unsafe {
        h.clear();h.collect();
        let method = h.named(b"Method"); let mvt = vt(h,method); let attach = lookup(h,method,b"withInline:at:owner:");
        let candidate = h.eval(b"[:x | let alias := x; let another := alias; another]");
        install(h.id,mvt,b"withInline:at:owner:",baseline,ptr::null_mut());
        let plain = h.eval(b"[:x | let alias := x; let another := alias; another]");
        restore(h,mvt,b"withInline:at:owner:",attach);
        let argument = h.eval(b"[Object clone]");h.bind(b"InlineArgument",argument);
        h.bind(b"InlineReturn",candidate);
        let caller = h.compile(b"let answer := [InlineReturn value: InlineArgument]; answer");let block = h.block(caller);
        assert_eq!(run(h,block,false),argument);
        h.bind(b"InlineReturn",plain);assert_eq!(run(h,block,false),argument);
        // A replaced constructor's effects cannot be elided; both bodies run it.
        let integer = h.named(b"Integer");let ivt=vt(h,integer);
        let mut f=Factory { original:lookup(h,integer,b"fromWord:"),calls:Cell::new(0) };
        install(h.id,ivt,b"fromWord:",factory,ptr::from_mut(&mut f).cast());
        let mut counts=[0;2];
        for (i,body) in [plain,candidate].into_iter().enumerate() {
            h.bind(b"InlineReturn",body);f.calls.set(0);assert_eq!(run(h,block,false),argument);counts[i]=f.calls.get();
        }
        assert!(counts[0]>0);assert_eq!(counts[0],counts[1]);
        restore(h,ivt,b"fromWord:",f.original);
        // Changed frame access selects the ordinary frame/PC path through Id.
        let machine_proto=h.named(b"Machine");let machine_vt=vt(h,machine_proto);
        let mut reader=Factory { original:lookup(h,machine_proto,b"oopAt:"),calls:Cell::new(0) };
        install(h.id,machine_vt,b"oopAt:",factory,ptr::from_mut(&mut reader).cast());
        let mut reads=[0;2];
        for (i,body) in [plain,candidate].into_iter().enumerate() {
            h.bind(b"InlineReturn",body);reader.calls.set(0);assert_eq!(run(h,block,false),argument);reads[i]=reader.calls.get();
        }
        assert!(reads[0]>0);assert_eq!(reads[0],reads[1]);
        restore(h,machine_vt,b"oopAt:",reader.original);
        // A bounded drive exposes a real Frame rather than taking the shortcut.
        h.bind(b"InlineReturn",candidate);
        let machine=object(send(h.id,h.named(b"Machine"),b"new",&[]));h.keep(machine);
        let budget=h.eval(b"1");let mut result=object(send(h.id,machine,b"runBlock:budget:",&[Operand::object(block),Operand::object(budget)]));
        let mut observed=false;
        for _ in 0..32 {
            if result!=machine {break;}
            let view=object(send(h.id,machine,b"currentFrameView",&[]));h.keep(view);
            observed|=view!=h.named(b"nil");h.collect();
            result=object(send(h.id,machine,b"continueWithBudget:",&[Operand::object(budget)]));
        }
        assert!(observed);assert_eq!(result,argument);
        send(h.id,h.named(b"GC"),b"budget:",&[Operand::word(usize::MAX)]);
        let mut elapsed=[0;2];let mut allocations=[0;2];
        for (i,body) in [plain,candidate].into_iter().enumerate() {
            h.bind(b"InlineReturn",body);let start=Instant::now();
            for _ in 0..180 {
                let before=send(h.id,h.id.receiver,b"allocationCount",&[]).as_word().unwrap();
                assert_eq!(run(h,block,false),argument);
                allocations[i]+=send(h.id,h.id.receiver,b"allocationCount",&[]).as_word().unwrap()-before;
            }
            elapsed[i]=start.elapsed().as_nanos();
        }
        assert!(allocations[1]<allocations[0]);
        println!("inline-return: baseline {}ns/{} allocations; optimized {}ns/{} allocations",elapsed[0],allocations[0],elapsed[1],allocations[1]);
        h.bind(b"InlineReturn",h.named(b"nil"));h.bind(b"InlineArgument",h.named(b"nil"));h.clear();h.collect();
    }
}
