//! Targeted costs for the third simplification round; setup is outside timing.
#![allow(dead_code)]
#[path="../tests/support/minimal.rs"] mod minimal;
use std::time::Instant;
use birnam_abi::as_size;
fn main(){
 let h=minimal::native_floor();let q=h.scope();h.load_native(&q,&h.resources.lib_dir.join("libbirnam_peg.dylib"),&[h.namespace]).unwrap();
 let peg=h.named(b"Peg").unwrap();let grammar=q.call(peg,b"grammar",&[]).unwrap();
 let child=h.intern(b"child").unwrap();let root=h.intern(b"root").unwrap();let literal=q.call(peg,b"literal:",&[q.string(b"x").unwrap()]).unwrap();
 q.call(grammar,b"rule:is:",&[child,literal]).unwrap();let reference=q.call(peg,b"ruleRef:",&[child]).unwrap();let repeat=q.call(peg,b"star:",&[reference]).unwrap();q.call(grammar,b"rule:is:",&[root,repeat]).unwrap();
 let count=h.intern(b"childCount").unwrap();let at=h.intern(b"childAt:").unwrap();let integer=h.named(b"Integer").unwrap();let word=h.intern(b"word").unwrap();
 for n in [128usize,4096]{
  let tree=q.call(grammar,b"parse:from:",&[q.string(&vec![b'x';n]).unwrap(),root]).unwrap();let i=q.call(integer,b"fromWord:",&[birnam_abi::size(n-1).unwrap()]).unwrap();
  assert_eq!(as_size(unsafe{h.id.send(unsafe{h.id.send(tree,count,&[])}.unwrap(),word,&[])}.unwrap()),Some(n));
  for sample in 0..5{q.call(h.named(b"GC").unwrap(),b"collectPrecisely",&[]).unwrap();let start=Instant::now();
   for _ in 0..1000{std::hint::black_box(unsafe{h.id.send(tree,at,&[i])}.unwrap());}
   println!("{{\"operation\":\"cst_child\",\"n\":{n},\"sample\":{sample},\"ns\":{}}}",start.elapsed().as_nanos()/1000);
  }
 }
}
