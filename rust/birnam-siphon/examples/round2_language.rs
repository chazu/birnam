//! Authored-operation timings on a complete compiled installation.
#![allow(dead_code)]
#[path="../tests/support/oop.rs"] mod oop;
use std::{os::unix::ffi::OsStrExt,time::Instant,path::Path};
fn main(){
 let h=oop::bootstrap();let q=h.scope();let r=&h.resources;let mut args=vec![h.namespace];
 for path in [&r.prefix,&r.lib_dir,&r.rust_workspace_dir,&r.guide_dir,&r.id_dylib]{args.push(q.string(path.as_os_str().as_bytes()).unwrap());}
 h.load_native(&q,&r.lib_dir.join("libbirnam_host_entry.dylib"),&args).unwrap();
 h.owner_call(&q,b"HostEntry",b"bootstrap:tools:",&[h.named(b"false").unwrap(),h.named(b"false").unwrap()]).unwrap();
 let mode=std::env::args().nth(1).unwrap_or_else(||"json".into());
 if mode=="json"{
  q.evaluate(include_bytes!("../../../modules/json.birnam"),Path::new("<json-benchmark>"),None).unwrap();
  let decode=h.intern(b"jsonUnescape").unwrap();let gc=h.named(b"GC").unwrap();
  for n in [128usize,1024,4096]{
   let text=format!("{}\\n","a".repeat(n));let input=q.string(text.as_bytes()).unwrap();
   for sample in 0..5{
    q.call(gc,b"collectPrecisely",&[]).unwrap();let start=Instant::now();let out=unsafe{h.id.send(input,decode,&[])}.unwrap();let elapsed=start.elapsed().as_nanos();
    assert_eq!(q.read_bytes(out).unwrap(),format!("{}\n","a".repeat(n)).as_bytes());
    println!("{{\"operation\":\"json_unescape\",\"n\":{n},\"sample\":{sample},\"ns\":{elapsed}}}");
   }
  }
 }else if mode=="json-test"{
  q.evaluate(include_bytes!("../../../modules/json-test.birnam"),Path::new("<json-test>"),None).unwrap();
  let result=q.call(h.global(&q,b"Test").unwrap(),b"runAllSuitesWithRunner:",&[h.global(&q,b"ConsoleRunner").unwrap()]).unwrap();q.check_result(result).unwrap();assert_eq!(q.call(result,b"allPassed",&[]).unwrap(),h.named(b"true").unwrap());
 }
}
