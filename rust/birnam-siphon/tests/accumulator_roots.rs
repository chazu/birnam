#![allow(dead_code)]
#[path="support/minimal.rs"] mod minimal;
use birnam_abi::{as_size,size};
#[test] #[ignore="requires native module composition"]
fn replacing_one_temporary_root_preserves_independently_retained_aliases(){
 let h=minimal::native_floor();let outer=h.scope();let q=h.scope();let parent=h.named(b"ObjectVt").unwrap();
 let old=q.call(parent,b"allocate",&[size(0).unwrap()]).unwrap();outer.keep(old).unwrap();
 let next=q.call(parent,b"allocate",&[size(0).unwrap()]).unwrap();q.replace_kept(old,next).unwrap();
 q.call(h.named(b"GC").unwrap(),b"collectPrecisely",&[]).unwrap();
 assert_eq!(as_size(q.call(h.id.root,b"allocationSizeOf:",&[old]).unwrap()),Some(0));
 assert_eq!(as_size(q.call(h.id.root,b"allocationSizeOf:",&[next]).unwrap()),Some(0));
}
