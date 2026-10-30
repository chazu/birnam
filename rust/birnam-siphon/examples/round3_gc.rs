#![allow(dead_code)]
#[path="../tests/support/minimal.rs"] mod minimal;
use std::time::Instant;
fn main(){let h=minimal::native_floor();let q=h.scope();let gc=h.named(b"GC").unwrap();let collect=h.intern(b"collectPrecisely").unwrap();let parent=h.named(b"ObjectVt").unwrap();
 for n in [1024usize,16384]{let held=h.scope();for _ in 0..n{held.call(parent,b"allocate",&[birnam_abi::size(0).unwrap()]).unwrap();}q.call(gc,b"collectPrecisely",&[]).unwrap();for sample in 0..7{let start=Instant::now();unsafe{h.id.send(gc,collect,&[])}.unwrap();let ns=start.elapsed().as_nanos();println!("{{\"operation\":\"collect_live\",\"n\":{n},\"sample\":{sample},\"ns\":{ns}}}");}}
}
