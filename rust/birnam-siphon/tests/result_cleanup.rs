//! A result must stay covered through collecting custom cleanup callbacks.
mod support;
use birnam_abi::{Oop,id_protocol::{NativeEntry,Operand,Outcome,Protocol}};
use std::{cell::Cell,ffi::c_void,os::unix::ffi::OsStrExt};
use support::*;
struct Hook {id:Protocol,gc:Oop,constructor:NativeEntry,release:NativeEntry,last:Cell<Oop>,lost:Cell<bool>,calls:Cell<usize>,fail:Cell<bool>}
unsafe extern "C" fn factory(c:*mut c_void,r:Oop,a:*const Operand,n:usize)->Outcome{
 let h=unsafe{&*c.cast::<Hook>()};let answer=unsafe{(h.constructor.method.unwrap())(h.constructor.context,r,a,n)};
 if let Some(o)=answer.value.as_object(){h.last.set(o);}answer
}
unsafe extern "C" fn cleanup(c:*mut c_void,r:Oop,a:*const Operand,n:usize)->Outcome{
 let h=unsafe{&*c.cast::<Hook>()};let answer=unsafe{(h.release.method.unwrap())(h.release.context,r,a,n)};
 if !h.last.get().is_null(){unsafe{collect(h.id,h.gc)};h.calls.set(h.calls.get()+1);if !unsafe{present(h.id,h.last.get())}{h.lost.set(true);}}
 if h.fail.replace(false){Outcome::failed(811,35)}else{answer}
}
struct InventoryCleanup { id:Protocol, gc:Oop, add:NativeEntry, retain:NativeEntry, release:NativeEntry, result:Cell<Oop>, token:Cell<usize>, alive:Cell<bool>, calls:Cell<usize>, failure:Cell<bool> }
unsafe extern "C" fn inventory_add(c:*mut c_void,r:Oop,a:*const Operand,n:usize)->Outcome {
 let h=unsafe{&*c.cast::<InventoryCleanup>()};let answer=unsafe{(h.add.method.unwrap())(h.add.context,r,a,n)};
 if let Some(o)=answer.value.as_object(){h.result.set(o);} answer
}
unsafe extern "C" fn inventory_retain(c:*mut c_void,r:Oop,a:*const Operand,n:usize)->Outcome {
 let h=unsafe{&*c.cast::<InventoryCleanup>()};let answer=unsafe{(h.retain.method.unwrap())(h.retain.context,r,a,n)};
 if n==1 && unsafe{(*a).as_object()}==Some(h.result.get()){h.token.set(answer.value.as_word().unwrap());} answer
}
unsafe extern "C" fn inventory_release(c:*mut c_void,r:Oop,a:*const Operand,n:usize)->Outcome {
 let h=unsafe{&*c.cast::<InventoryCleanup>()};let answer=unsafe{(h.release.method.unwrap())(h.release.context,r,a,n)};
 if n==1 && unsafe{(*a).as_word()}==Some(h.token.get()) && h.token.replace(0)!=0 {
  unsafe{collect(h.id,h.gc)};h.calls.set(h.calls.get()+1);h.alive.set(unsafe{present(h.id,h.result.get())});
  if h.failure.replace(false){return Outcome::failed(812,36);}
 } answer
}
#[test]
#[ignore="requires production synchronous compiler"]
fn fresh_result_survives_collecting_cleanup_until_caller_handoff(){unsafe{
 let id=id();let ns=load(id,"bootstrap",&[Operand::bytes(directory().as_os_str().as_bytes())]);
 let named=|name:&[u8]|object(send(id,ns,b"at:",&[Operand::object(id.intern_name(name).unwrap())]));
 let roots=named(b"Roots");let gc=named(b"GC");
 let source=br#"let probe := [Object clone]; [Reflect defineMethod: #make: on: probe as: [Execution block: [:self :input | let unused := [Execution wordResultOf: input selector: #word]; [Execution wordResultOf: input selector: #word]] withNativeSignature: "oO"]]; [Globals at: #CleanupProbe put: probe]; true"#;
 let text=object(send(id,named(b"String"),b"fromBytes:",&[Operand::bytes(source)]));let tt=retain(id,roots,text);
 let path=std::env::temp_dir().join(format!("birnam-cleanup-result-{}.dylib",std::process::id()));
 let output=object(send(id,named(b"String"),b"fromBytes:",&[Operand::bytes(path.as_os_str().as_bytes())]));let ot=retain(id,roots,output);
 send(id,named(b"Codegen"),b"compile:output:from:synchronousBodies:",&[Operand::object(text),Operand::object(output),Operand::object(output),Operand::object(named(b"true"))]);
 send(id,named(b"Codegen"),b"load:",&[Operand::object(output)]);
 let probe=object(send(id,named(b"Globals"),b"at:",&[Operand::object(id.intern_name(b"CleanupProbe").unwrap())]));let pt=retain(id,roots,probe);
 let a=object(send(id,named(b"Integer"),b"fromWord:",&[Operand::word(123)]));let at=retain(id,roots,a);
 let run=||object(send(id,probe,b"make:",&[Operand::object(a)]));run(); // warm selector inventory
 let bind=|o,name:&[u8]|send(id,id.receiver,b"bind:to:",&[Operand::object(id.intern_name(name).unwrap()),Operand::object(o)]).as_entry().unwrap();
 let rv=object(send(id,id.receiver,b"behaviorOf:",&[Operand::object(roots)]));let wv=named(b"WordVt");
 let constructor=bind(named(b"Word"),b"fromBits:");let release_entry=bind(roots,b"release:");
 let mut h=Hook{id,gc,constructor,release:release_entry,last:Cell::new(std::ptr::null_mut()),lost:Cell::new(false),calls:Cell::new(0),fail:Cell::new(false)};
 install(id,wv,b"fromBits:",factory,(&raw mut h).cast());install(id,rv,b"release:",cleanup,(&raw mut h).cast());
 let result=run();
 install(id,rv,b"release:",release_entry.method.unwrap(),release_entry.context);install(id,wv,b"fromBits:",constructor.method.unwrap(),constructor.context);
 assert!(h.calls.get()>0,"custom cleanup must actually collect");assert!(!h.lost.get(),"fresh result was collected during cleanup before caller could receive it");assert!(present(id,result));
 let pin=retain(id,roots,result);assert_eq!(send(id,result,b"bits",&[]).as_word(),Some(123));
 // Failure still drains every requested token and the overlap scope.
 let x=object(send(id,named(b"Word"),b"fromBits:",&[Operand::word(1)]));let xt=retain(id,roots,x);
 let y=object(send(id,named(b"Word"),b"fromBits:",&[Operand::word(2)]));let yt=retain(id,roots,y);
 let tokens=[xt,yt];let bytes=core::slice::from_raw_parts(tokens.as_ptr().cast::<u8>(),size_of_val(&tokens));
 h.last.set(y);h.lost.set(false);h.fail.set(true);
 install(id,rv,b"release:",cleanup,(&raw mut h).cast());
 let error=id.send_named(roots,b"releaseMany:protecting:batch:",&[Operand::bytes(bytes),Operand::object(y),Operand::word(1)]).unwrap_err();
 assert_eq!((error.status,error.detail),(811,35));assert!(!h.lost.get());
 h.last.set(std::ptr::null_mut());install(id,rv,b"release:",release_entry.method.unwrap(),release_entry.context);
 assert_eq!(send(id,roots,b"release:",&[Operand::word(xt)]).as_word(),Some(0));
 assert_eq!(send(id,roots,b"release:",&[Operand::word(yt)]).as_word(),Some(0));
 collect(id,gc);assert!(!present(id,y),"completed failure must not leak private coverage");
 for t in [pin,at,pt,ot,tt]{release(id,roots,t);}std::fs::remove_file(path).unwrap();
 // Reflect owns a returned inventory until the caller receives it. Its cleanup
 // must not depend on Roots release: being non-collecting.
 let vector=named(b"Vector");let vv=named(b"VectorVt");
 let add=bind(vector,b"add:");let retain_entry=bind(roots,b"retain:");
 let mut h=InventoryCleanup{id,gc,add,retain:retain_entry,release:release_entry,result:Cell::new(std::ptr::null_mut()),token:Cell::new(0),alive:Cell::new(false),calls:Cell::new(0),failure:Cell::new(false)};
 for (selector, args) in [(b"directMethodsOf:".as_slice(), vec![Operand::object(named(b"System"))]), (b"allMethodsOf:".as_slice(), vec![Operand::object(named(b"System"))]), (b"allGlobalNames".as_slice(), vec![])] {
  install(id,vv,b"add:",inventory_add,(&raw mut h).cast());
  install(id,rv,b"retain:",inventory_retain,(&raw mut h).cast());
  install(id,rv,b"release:",inventory_release,(&raw mut h).cast());
  let result=object(send(id,named(b"Reflect"),selector,&args));
  install(id,rv,b"release:",release_entry.method.unwrap(),release_entry.context);
  install(id,rv,b"retain:",retain_entry.method.unwrap(),retain_entry.context);
  install(id,vv,b"add:",add.method.unwrap(),add.context);
  assert!(h.calls.get()>0,"collect exactly when the result token is released");
  assert!(h.alive.get() && present(id,result),"Reflect returned an inventory collected during cleanup");
  let token=retain(id,roots,result);
  assert!(send(id,result,b"size",&[]).as_object().is_some());
  release(id,roots,token);
  h.calls.set(0);h.alive.set(false);
 }
 install(id,vv,b"add:",inventory_add,(&raw mut h).cast());
 install(id,rv,b"retain:",inventory_retain,(&raw mut h).cast());
 install(id,rv,b"release:",inventory_release,(&raw mut h).cast());
 h.failure.set(true);
 let failure=id.send_named(named(b"Reflect"),b"allGlobalNames",&[]).unwrap_err();
 install(id,rv,b"release:",release_entry.method.unwrap(),release_entry.context);
 install(id,rv,b"retain:",retain_entry.method.unwrap(),retain_entry.context);
 install(id,vv,b"add:",add.method.unwrap(),add.context);
 assert_eq!((failure.status,failure.detail),(812,36));
 assert!(h.alive.get());
 collect(id,gc);assert!(!present(id,h.result.get()),"failed reflection cleanup leaked result");

}}
