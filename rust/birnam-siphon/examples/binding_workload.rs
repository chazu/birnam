//! Rebind an admitted Method template; parsing/compilation/GC are untimed.
#[path = "../tests/support/mod.rs"] mod support;
#[path = "micro_support/clock.rs"] mod clock;
use birnam_abi::id_protocol::Operand;
use std::os::unix::ffi::OsStrExt;
use support::*;
// Live malloc traffic is independent of scheduling noise. Null zone sums all
// process zones (Darwin malloc.h); other platforms omit this diagnostic.
#[cfg(target_os="macos")]
fn heap_usage() -> Option<(usize,usize)> {
    #[repr(C)] #[derive(Default)] struct Statistics { blocks:u32, bytes:usize, high_water:usize, reserved:usize }
    unsafe extern "C" { fn malloc_zone_statistics(zone:*mut std::ffi::c_void, stats:*mut Statistics); }
    let mut stats=Statistics::default();
    unsafe { malloc_zone_statistics(std::ptr::null_mut(), &mut stats); }
    Some((stats.blocks as usize, stats.bytes))
}
#[cfg(not(target_os="macos"))]
fn heap_usage() -> Option<(usize,usize)> { None }
fn main() { unsafe {
    let id=id();let ns=load(id,"bootstrap",&[Operand::bytes(directory().as_os_str().as_bytes())]);
    let named=|name:&[u8]|object(send(id,ns,b"at:",&[Operand::object(id.intern_name(name).unwrap())]));
    let roots=named(b"Roots");let gc=named(b"GC");let nil=named(b"nil");
    let keep=|v|{retain(id,roots,v);v};
    let text=|s:&str|keep(object(send(id,named(b"String"),b"fromBytes:",&[Operand::bytes(s.as_bytes())])));
    for arity in [0,8,16] {
        let arity_object=keep(object(send(id,named(b"Integer"),b"fromWord:",&[Operand::word(arity)])));
        let source=keep(object(send(id,named(b"NativeBackend"),b"named:synchronous:",&[Operand::object(arity_object),Operand::object(named(b"false"))])));
        let unit=keep(object(send(id,named(b"TCC"),b"compile:",&[Operand::object(source)])));
        let template=keep(object(send(id,unit,b"initializeWith:",&[Operand::object(nil)])));
        let params=(0..arity).map(|n|format!(" :a{n}")).collect::<String>();
        let body=keep(object(send(id,named(b"Codegen"),b"eval:",&[Operand::object(text(&format!("[:self{params} | self]")))])));
        let selector=id.intern_name(b"withImplementation:").unwrap();let args=[Operand::object(body)];
        let mut values=keep(object(send(id,named(b"Vector"),b"new",&[])));
        for _ in 0..arity {values=keep(object(send(id,values,b"add:",&[Operand::object(nil)])));}
        let run=||object(id.send(template,selector,&args).unwrap());
        for _ in 0..10 {let method=run();assert_eq!(send(id,method,b"invokeOn:arguments:",&[Operand::object(nil),Operand::object(values)]).as_object(),Some(nil));}
        send(id,gc,b"automaticCollection:",&[Operand::word(0)]);
        let iterations=10000;let mut times=Vec::new();let mut heaps=Vec::new();
        for _ in 0..5 {
            collect(id,gc);let before=heap_usage();let start=clock::cpu_ns();let mut last=nil;
            for _ in 0..iterations {last=run();std::hint::black_box(last);}
            let elapsed=clock::cpu_ns()-start;
            let after=heap_usage();
            times.push(elapsed);
            heaps.push(before.zip(after).map(|(a,b)|serde_json::json!({"blocks":b.0 as i64-a.0 as i64,"bytes":b.1 as i64-a.1 as i64})));
            assert_eq!(send(id,last,b"invokeOn:arguments:",&[Operand::object(nil),Operand::object(values)]).as_object(),Some(nil));
        }
        println!("{}",serde_json::json!({"case":format!("bind-{arity}"),"iterations":iterations,"cpu_ns":times,"live_heap_delta":heaps}));
        send(id,gc,b"automaticCollection:",&[Operand::word(1)]);
    }
}}
