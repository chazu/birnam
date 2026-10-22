//! Isolate argument adaptation in owned Method invocation; setup is untimed.
#[path = "../tests/support/mod.rs"] mod support;
#[path = "micro_support/clock.rs"] mod clock;
use birnam_abi::{Oop, borrowed_slice, id_protocol::{NativeEntry, Operand, Outcome}};
use std::{ffi::c_void, os::unix::ffi::OsStrExt};
use support::*;
unsafe extern "C" fn last(_: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    Outcome::loaded(unsafe { borrowed_slice(a,n) }.unwrap()[n-1])
}
fn main() { unsafe {
    let id=id(); let ns=load(id,"bootstrap",&[Operand::bytes(directory().as_os_str().as_bytes())]);
    let named=|name:&[u8]|object(send(id,ns,b"at:",&[Operand::object(id.intern_name(name).unwrap())]));
    let roots=named(b"Roots");let nil=named(b"nil");let owner=named(b"Execution");
    let behavior=behavior(id);retain(id,roots,behavior);
    let receiver=allocate(id,behavior,0);retain(id,roots,receiver);
    let value=object(send(id,named(b"Integer"),b"fromWord:",&[Operand::word(71)]));retain(id,roots,value);
    for (label,count,nullable,authored) in [("source-one",1,false,true),("source-eight",8,false,true),("source-nullable",8,true,true),("native-eight",8,false,false)] {
        let mut construction=vec![Operand::entry(NativeEntry::new(last,std::ptr::null_mut())),Operand::object(owner)];
        if authored {construction.push(Operand::object(nil));}
        let method=object(send(id,named(b"Method"),if authored {b"fromNative:owner:implementation:"} else {b"fromNative:owner:"},&construction));retain(id,roots,method);
        let selector=id.intern_name(label.as_bytes()).unwrap();
        send(id,behavior,b"addMethod:as:",&[Operand::object(method),Operand::object(selector)]);
        let mut args=vec![Operand::object(value);count];if nullable {args[count-1]=Operand::object(std::ptr::null_mut());}
        let expected=if nullable {nil} else {value};
        for _ in 0..100 {assert_eq!(id.send(receiver,selector,&args).unwrap().as_object(),Some(expected));}
        let mut samples=Vec::new(); let iterations=200000;
        for _ in 0..5 {
            let start=clock::cpu_ns();
            for _ in 0..iterations {std::hint::black_box(id.send(receiver,selector,&args).unwrap());}
            samples.push(clock::cpu_ns()-start);
        }
        println!("{}",serde_json::json!({"case":label,"iterations":iterations,"cpu_ns":samples}));
    }
}}
