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
 }else if mode=="queue" || mode=="queue-test"{
  if let Some(path)=std::env::var_os("BIRNAM_QUEUE_SOURCE") {
   let path=std::path::PathBuf::from(path);q.evaluate(&std::fs::read(&path).unwrap(),&path,None).unwrap();
  } else if std::env::args().any(|a|a=="--current") {
   q.evaluate(include_bytes!("../../../modules/stackless-control.birnam"),Path::new("<stackless-control>"),None).unwrap();
  }
  if mode=="queue-test"{
   q.evaluate(include_bytes!("../../../tests/fixtures/task-queue.birnam"),Path::new("<task-queue>"),None).unwrap();
   let queue_failures=q.call(h.global(&q,b"QueueCheckFailures").unwrap(),b"value",&[]).unwrap();
   assert_eq!(q.word_bits(q.call(h.named(b"Integer").unwrap(),b"wordOf:",&[queue_failures]).unwrap()).unwrap(),0);
   let path=std::env::temp_dir().join(format!("birnam-task-native-{}.dylib",std::process::id()));
   assert!(std::process::Command::new("clang").args(["-dynamiclib","-O2","-Itests/fixtures/oop-continuation","tests/fixtures/oop-continuation/native.c","-o"]).arg(&path).status().unwrap().success());
   let image=h.load_native(&q,&path,&[]).unwrap();std::fs::remove_file(path).unwrap();
   let observer=q.call(h.named(b"Object").unwrap(),b"clone",&[]).unwrap();let cleanup=q.call(h.named(b"Codegen").unwrap(),b"eval:",&[q.string(b"[| nil]").unwrap()]).unwrap();
   let context=q.call(h.id.root,b"sequence",&[cleanup,h.intern(b"mustNotRecord").unwrap()]).unwrap();let behavior=q.call(h.id.root,b"ensurePrivateVtableFor:",&[observer]).unwrap();
   for name in [b"observe:".as_slice(),b"failTask:",b"ensureTask:body:"]{let key=h.intern(name).unwrap();let method=q.call(image,b"methodAt:",&[key]).unwrap();let method=q.call(method,b"withContext:",&[context]).unwrap();q.call(behavior,b"addMethod",&[key,method]).unwrap();}
   q.call(h.named(b"Globals").unwrap(),b"at:put:",&[h.intern(b"NativeTaskObserver").unwrap(),observer]).unwrap();
   q.evaluate(include_bytes!("../../../scripts/task-control-smoke.birnam"),Path::new("<task-control-smoke>"),None).unwrap();
   q.evaluate(b"[TaskCheck run]",Path::new("<task-control-run>"),None).unwrap();
   let failures=h.global(&q,b"TaskCheckFailures").unwrap();let count=q.call(failures,b"value",&[]).unwrap();
   assert_eq!(q.word_bits(q.call(h.named(b"Integer").unwrap(),b"wordOf:",&[count]).unwrap()).unwrap(),0);
  }else{
   let source=b"let BenchTask := [Object clone]; [Reflect defineMethod: #drive: on: BenchTask as: [:self :job | true]]; BenchTask";
   let task=q.call(h.named(b"Codegen").unwrap(),b"eval:",&[q.string(source).unwrap()]).unwrap();q.check_result(task).unwrap();
   let job=q.call(h.named(b"Vector").unwrap(),b"new",&[]).unwrap();let job=q.call(job,b"add:",&[task]).unwrap();
   let executor=h.global(&q,b"TaskExecutor").unwrap();let drive=h.intern(b"driveNext").unwrap();
   for n in [128usize,512,2048]{for sample in 0..5{
    for _ in 0..n{q.call(executor,b"enqueue:",&[job]).unwrap();}
    q.call(h.named(b"GC").unwrap(),b"collectPrecisely",&[]).unwrap();let start=Instant::now();
    for _ in 0..n{assert_eq!(unsafe{h.id.send(executor,drive,&[])}.unwrap(),h.named(b"true").unwrap());}
    let elapsed=start.elapsed().as_nanos();assert_eq!(q.call(executor,b"hasWork",&[]).unwrap(),h.named(b"false").unwrap());
    println!("{{\"operation\":\"queue_drain\",\"n\":{n},\"sample\":{sample},\"ns\":{elapsed}}}");
   }}
  }
 }else if mode=="sort"{
  let merge=std::env::args().any(|a|a=="--merge");let selector=h.intern(if merge{b"mergeSortBy:"}else{b"sortBy:"}).unwrap();
  let key=q.call(h.named(b"Codegen").unwrap(),b"eval:",&[q.string(b"[:x | x]").unwrap()]).unwrap();
  for n in [128usize]{
   let mut input=q.call(h.named(b"Vector").unwrap(),b"new",&[]).unwrap();
   for i in (0..n).rev(){input=q.call(input,b"add:",&[q.string(format!("item-{i:05}").as_bytes()).unwrap()]).unwrap();}
   for sample in 0..5{
    q.call(h.named(b"GC").unwrap(),b"collectPrecisely",&[]).unwrap();let start=Instant::now();let output=unsafe{h.id.send(input,selector,&[key])}.unwrap();let elapsed=start.elapsed().as_nanos();
    assert_eq!(q.vector_size(output).unwrap(),n); for i in 0..n {assert_eq!(q.string_bytes(q.vector_at(output,i).unwrap()).unwrap(),format!("item-{i:05}").as_bytes());}
    println!("{{\"operation\":\"name_sort\",\"n\":{n},\"sample\":{sample},\"ns\":{elapsed}}}");
   }
  }
 }else if mode=="json-test"{
  q.evaluate(include_bytes!("../../../modules/json-test.birnam"),Path::new("<json-test>"),None).unwrap();
  let result=q.call(h.global(&q,b"Test").unwrap(),b"runAllSuitesWithRunner:",&[h.global(&q,b"ConsoleRunner").unwrap()]).unwrap();q.check_result(result).unwrap();assert_eq!(q.call(result,b"allPassed",&[]).unwrap(),h.named(b"true").unwrap());
 }
}
