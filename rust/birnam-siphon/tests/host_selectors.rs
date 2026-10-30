//! Selector reuse does not freeze method or Namespace bindings.
#[allow(dead_code)] #[path="support/minimal.rs"] mod minimal;
#[allow(dead_code)] #[path="support/oop.rs"] mod oop;
use birnam_abi::size;
#[test] #[ignore="requires native modules"]
fn fixed_selectors_are_reused_while_bindings_stay_live(){
 let h=minimal::native_floor();let q=h.scope();let input=q.string(b"selector").unwrap();q.read_bytes(input).unwrap();let hooks=oop::hooks::Hooks::new(&h);
 let rootvt=q.call(h.id.root,b"behaviorOf:",&[h.id.root]).unwrap();let constructor=hooks.symbol(b"nameWithBytes");let add=hooks.symbol(b"addMethod");let saved=q.call(rootvt,b"lookup",&[constructor]).unwrap();
 let svt=h.named(b"StringVt").unwrap();let print=hooks.symbol(b"printString");let original=q.call(svt,b"lookup",&[print]).unwrap();
 let other=q.call(h.named(b"Object").unwrap(),b"clone",&[]).unwrap();let calls=hooks.context(&[other,size(0).unwrap()]);let replacement=hooks.method(b"fixedCount",calls);
 let denied=hooks.context(&[hooks.fault(71),size(0).unwrap()]);let deny=hooks.method(b"deny",denied);
 let integer=hooks.symbol(b"Integer");let original_integer=h.named(b"Integer").unwrap();let gc=h.named(b"GC").unwrap();
 q.call(rootvt,b"addMethod",&[constructor,deny]).unwrap();
 assert_eq!(q.call(input,b"printString",&[]).unwrap(),input);
 q.call(svt,b"addMethod",&[print,replacement]).unwrap();assert_eq!(q.call(input,b"printString",&[]).unwrap(),other);
 q.call(h.namespace,b"at:put:",&[integer,other]).unwrap();assert_eq!(h.named(b"Integer").unwrap(),other);
 q.call(gc,b"collectPrecisely",&[]).unwrap();assert_eq!(q.call(input,b"printString",&[]).unwrap(),other);
 assert!(h.intern(b"printString").is_err(),"explicit interning remains a current operation");
 unsafe{h.id.send(rootvt,add,&[constructor,saved])}.unwrap();
 q.call(svt,b"addMethod",&[print,original]).unwrap();q.call(h.namespace,b"at:put:",&[integer,original_integer]).unwrap();assert_eq!(hooks.count(denied,1),1);
}
