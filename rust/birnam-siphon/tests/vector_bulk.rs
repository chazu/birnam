//! Fresh bulk results preserve old persistent values and survive collecting allocators.
#[allow(dead_code)] #[path="support/minimal.rs"] mod minimal;
#[allow(dead_code)] #[path="support/oop.rs"] mod oop;
use birnam_abi::{size,as_size};
#[test] #[ignore="requires native modules"]
fn bulk_vectors_preserve_tree_boundaries_and_inputs(){
 let h=minimal::native_floor();let q=h.scope();let hooks=oop::hooks::Hooks::new(&h);
 let factory=h.named(b"Vector").unwrap();let gc=h.named(b"GC").unwrap();
 let add=h.intern(b"add:").unwrap();let at=h.intern(b"atWord:").unwrap();let put=h.intern(b"at:put:").unwrap();
 let model=h.named(b"ObjectModel").unwrap();let vt=q.call(h.id.root,b"behaviorOf:",&[model]).unwrap();let allocate=h.intern(b"allocate:bytes:").unwrap();let original=q.call(vt,b"lookup",&[allocate]).unwrap();
 for n in [0usize,1,31,32,33,1024,1025,1057,32769]{
  let scope=h.scope();let mut v=scope.call(factory,b"new",&[]).unwrap();let holder=scope.call(h.id.root,b"sequence",&[v]).unwrap();
  for i in 0..n{v=unsafe{h.id.send(v,add,&[size(i).unwrap()])}.unwrap();unsafe{h.id.send(holder,put,&[size(0).unwrap(),v])}.unwrap();}
  let state=hooks.context(&[original,gc,size(0).unwrap()]);
  scope.call(vt,b"addMethod",&[allocate,hooks.method(b"collectOnce",state)]).unwrap();
  let reverse=scope.call(v,b"reversed",&[]).unwrap();let joined=scope.call(v,b",",&[reverse]).unwrap();
  scope.call(vt,b"addMethod",&[allocate,original]).unwrap();scope.call(gc,b"collectPrecisely",&[]).unwrap();
  for i in 0..n{
   assert_eq!(as_size(unsafe{h.id.send(v,at,&[size(i).unwrap()])}.unwrap()),Some(i));
   assert_eq!(as_size(unsafe{h.id.send(reverse,at,&[size(i).unwrap()])}.unwrap()),Some(n-1-i));
   assert_eq!(as_size(unsafe{h.id.send(joined,at,&[size(n+i).unwrap()])}.unwrap()),Some(n-1-i));
  }
 }
}
