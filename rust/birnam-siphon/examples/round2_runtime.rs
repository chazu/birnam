//! Runtime costs. Setup and explicit GC are outside timing.
#![allow(dead_code)]
#[path="../tests/support/minimal.rs"] mod minimal;
use birnam_abi::{size, as_size};
use std::{hint::black_box,time::Instant};
use birnam_artifact_codec::portable_program::{self as p, NodeSpec, StatementSpec, ProgramSpec};
fn main(){
 let mode=std::env::args().nth(1).unwrap();let h=minimal::native_floor();let q=h.scope();
 q.read_bytes(q.string(b"initialize callbacks").unwrap()).unwrap();
 let gc=h.named(b"GC").unwrap();
 if mode=="emit" {
  let backend=h.named(b"NativeBackend").unwrap();let emit=h.intern(b"emitSynchronous:").unwrap();
  for n in [8192,262144] {
   let data=vec![b'a';n];let bytes=p::encode(ProgramSpec{parameters:&[],captures:&[],lets:&[],statements:&[StatementSpec::Expression{root_node:0}],nodes:&[NodeSpec::String(&data)]}).unwrap();
   let input=q.string(&bytes).unwrap();
   for sample in 0..5 {
    q.call(gc,b"collectPrecisely",&[]).unwrap();let start=Instant::now();
    let result=unsafe{h.id.send(backend,emit,&[input])}.unwrap();let elapsed=start.elapsed().as_nanos();
    assert!(q.read_bytes(result).unwrap().windows(6).any(|s|s==b"static"));
    println!("{{\"operation\":\"emit\",\"n\":{n},\"sample\":{sample},\"ns\":{elapsed}}}");
   }
  }
 }else if mode=="vector"{
  let factory=h.named(b"Vector").unwrap();let add=h.intern(b"add:").unwrap();let reversed=h.intern(b"reversed").unwrap();let size_word=h.intern(b"sizeWord").unwrap();
  for n in [1024,8192]{
   let scope=h.scope();let mut v=scope.call(factory,b"new",&[]).unwrap();
   let holder=scope.call(h.id.root,b"sequence",&[v]).unwrap();let put=h.intern(b"at:put:").unwrap();
   for i in 0..n{v=unsafe{h.id.send(v,add,&[size(i).unwrap()])}.unwrap();unsafe{h.id.send(holder,put,&[size(0).unwrap(),v])}.unwrap();}
   for sample in 0..5{
    q.call(gc,b"collectPrecisely",&[]).unwrap();let start=Instant::now();let result=unsafe{h.id.send(v,reversed,&[])}.unwrap();let elapsed=start.elapsed().as_nanos();
    assert_eq!(as_size(unsafe{h.id.send(result,size_word,&[])}.unwrap()),Some(n));black_box(result);
    println!("{{\"operation\":\"vector_reverse\",\"n\":{n},\"sample\":{sample},\"ns\":{elapsed}}}");
   }
  }
 }
}
