use birnam_abi::{self as abi,Oop};
use birnam_host_support::{Context,Scope,Resources};
use birnam_codegen::{OwnedCst,CstNode};
use std::{path::PathBuf,os::unix::ffi::OsStrExt,time::Instant,hint::black_box};
fn context(prefix:PathBuf)->Context {
 let lib=prefix.join("lib");
 let id=birnam_siphon::load(lib.join("libbirnam_id.dylib").as_os_str().as_bytes(),&[lib.join("libbirnam_bootstrap.dylib").as_os_str().as_bytes(),lib.as_os_str().as_bytes(),b"nameWithBytes",b"sequence"]).unwrap();
 unsafe {
  let constructor=id.send(id.literal(2).unwrap(),id.intern,&[]).unwrap();
  let seq=id.send(id.literal(3).unwrap(),id.intern,&[]).unwrap();
  let args=id.send(id.root,seq,&[id.literal(1).unwrap()]).unwrap();
  let ns=id.send(id.literal(0).unwrap(),id.load,&[args]).unwrap();
  Context::from_namespace(id,ns,Resources{prefix:prefix.clone(),lib_dir:lib.clone(),id_dylib:lib.join("libbirnam_id.dylib"),rust_workspace_dir:prefix.join("share/birnam/src"),guide_dir:prefix.join("share/birnam/docs/guide")},constructor).unwrap()
 }
}
fn number(q:&Scope<'_>,h:&Context,o:Oop)->usize {q.word_bits(q.call(h.named(b"Integer").unwrap(),b"wordOf:",&[o]).unwrap()).unwrap()}
fn at(q:&Scope<'_>,h:&Context,o:Oop,i:usize)->Oop {let n=q.call(h.named(b"Integer").unwrap(),b"fromWord:",&[abi::size(i).unwrap()]).unwrap();q.call(o,b"at:",&[n]).unwrap()}
fn project(q:&Scope<'_>,h:&Context,n:Oop)->OwnedCst {
 let rule=q.string_bytes(q.call(n,b"ruleName",&[]).unwrap()).unwrap();
 let start=number(q,h,q.call(n,b"start",&[]).unwrap());let end=number(q,h,q.call(n,b"end",&[]).unwrap());
 let count=number(q,h,q.call(n,b"childCount",&[]).unwrap());
 let children=(0..count).map(|i|{let index=q.call(h.named(b"Integer").unwrap(),b"fromWord:",&[abi::size(i).unwrap()]).unwrap();let child=q.call(n,b"childAt:",&[index]).unwrap(); project(q,h,child)}).collect();
 OwnedCst::new(&rule,start,end,children)
}
fn rows(q:&Scope<'_>,h:&Context,v:Oop)->Vec<Vec<u8>> {(0..number(q,h,q.call(v,b"size",&[]).unwrap())).map(|i|q.string_bytes(at(q,h,v,i)).unwrap()).collect()}
fn nodes(n:&OwnedCst)->usize {1+(0..n.child_count()).map(|i|nodes(n.child(i).unwrap())).sum::<usize>()}
fn measure(mut f:impl FnMut(),warmup:usize,iterations:usize)->Vec<f64>{for _ in 0..warmup {f()};(0..iterations).map(|_|{let t=Instant::now();f();t.elapsed().as_secs_f64()*1e6}).collect()}


type Metrics=unsafe extern "C" fn(usize,usize,usize)->u64;
unsafe extern "C" {fn dlopen(p:*const std::ffi::c_char,flags:i32)->*mut std::ffi::c_void;fn dlsym(h:*mut std::ffi::c_void,n:*const std::ffi::c_char)->*mut std::ffi::c_void;}
fn metric(prefix:&str,lib:&str)->Option<Metrics>{unsafe{
 let p=std::ffi::CString::new(format!("{prefix}/lib/libbirnam_{lib}.dylib")).unwrap();
 let h=dlopen(p.as_ptr(),1);assert!(!h.is_null());let n=std::ffi::CString::new(format!("birnam_{lib}_probe")).unwrap();let f=dlsym(h,n.as_ptr());
 if f.is_null(){None}else{Some(std::mem::transmute::<*mut std::ffi::c_void,Metrics>(f))}
}}

fn main(){
 let args:Vec<_>=std::env::args().collect();let h=context(PathBuf::from(&args[1]));let q=h.scope();
 let codegen=h.named(b"Codegen").unwrap();let gc=h.named(b"GC").unwrap();
 let metrics=[metric(&args[1],"id"),metric(&args[1],"root_registry"),metric(&args[1],"collect")];
 let watched=["+","-","=",">=","wordOf:","fromWord:","wrappingAdd:","bitXor:","bitAnd:","unsignedLessThan:","retain:","release:","releaseTokensProtecting","integerBinary:operation:","integerBitsOf:","integerAddBits:with:","objectResultOf:selector:wordArgument:","optionalWordResultOf:selector:argument:resultFactory:","allocate:bytes:","fromMethod:captureOperands:","prepareCall:selector:","value","value:","value:value:","value:value:value:value:value:","ruleName","children","childAt:","childCount","fromByteValues","fromBytes:","captureWord:of:"];
 if let Some(read)=metrics[0]{for(i,name)in watched.iter().enumerate(){let key=h.intern(name.as_bytes()).unwrap();q.keep(key).unwrap();unsafe{read(2,i,key.addr());}}}
 let mode=std::env::var("WALK_MODE").unwrap_or_else(|_|"compileSynchronous:".into());
 for file in [&args[2],&args[3]] {
  let s=q.string(&std::fs::read(file).unwrap()).unwrap();let m=q.call(codegen,mode.as_bytes(),&[s]).unwrap();q.check_result(m).unwrap();
  let v=q.call(h.named(b"Vector").unwrap(),b"new",&[]).unwrap();let b=q.call(h.named(b"Block").unwrap(),b"fromMethod:captures:",&[m,v]).unwrap();
  let result=q.call(b,b"value",&[]).unwrap();q.check_result(result).unwrap();assert_eq!(result,codegen);
 }
 let source:String=(0..32).map(|i|format!("[Reflect defineMethod: #method{i} on: Object as: [:self | {i}]];\n")).collect();
 let text=q.string(source.as_bytes()).unwrap();let tree=q.call(h.named(b"Lang").unwrap(),b"parse:",&[text]).unwrap();q.check_result(tree).unwrap();
 let expected={let q=h.scope();nodes(&project(&q,&h,tree))};assert_eq!(expected,930);
 let epoch=h.intern(b"allocationEpoch").unwrap();q.keep(epoch).unwrap();let bytes=h.intern(b"allocationBytesTotal").unwrap();q.keep(bytes).unwrap();
 let a=q.call(h.named(b"Integer").unwrap(),b"fromWord:",&[abi::size(4).unwrap()]).unwrap();
 let b=q.call(h.named(b"Integer").unwrap(),b"fromWord:",&[abi::size(3).unwrap()]).unwrap();
 let count=|key| unsafe {abi::as_size(h.id.send(h.id.root,key,&[]).unwrap()).unwrap()};
 let repetitions=std::env::var("WALK_REPETITIONS").ok().map(|v|v.parse::<usize>().unwrap()).unwrap_or(1);
 let mut results=vec![];
 for automatic in [true,false] {
  q.call(gc,b"automaticCollection:",&[abi::size(usize::from(automatic)).unwrap()]).unwrap();
  for selector in [b"addPair:with:".as_slice(),b"subtractPair:with:",b"equalPair:with:",b"walkIndexed:",b"walkBulkCount:",b"walkBulkVisit:",b"walkRuleNames:",b"walkRules:",b"methodSourceTextsIn:source:"] {
   if let Ok(case)=std::env::var("WALK_CASE") {if selector!=case.as_bytes(){continue}}
   let pair=selector.ends_with(b"Pair:with:");
   let args=if pair {vec![a,b]} else if selector==b"methodSourceTextsIn:source:" {vec![tree,text]} else {vec![tree]};
   {let q=h.scope();let v=q.call(codegen,selector,&args).unwrap();q.check_result(v).unwrap();
    if selector==b"addPair:with:" {assert_eq!(number(&q,&h,v),7)}
    else if selector==b"subtractPair:with:" {assert_eq!(number(&q,&h,v),1)}
    else if selector==b"equalPair:with:" {assert_eq!(v,h.named(b"false").unwrap())}
    else if selector==b"walkIndexed:"||selector==b"walkBulkCount:" {assert_eq!(number(&q,&h,v),expected)}
    else if selector==b"methodSourceTextsIn:source:" {assert_eq!(rows(&q,&h,v).len(),32)}else{assert_eq!(v,codegen)}
   }
   let mut samples=vec![];
   for _ in 0..5 {
    q.call(gc,b"collectPrecisely",&[]).unwrap();
    let before=count(epoch);let charged=count(bytes);
    for read in metrics.iter().flatten(){unsafe{read(1,0,0);}}
    let start=Instant::now();
    for _ in 0..repetitions {let q=h.scope();black_box(q.call(codegen,selector,&args).unwrap());}
    let micros=start.elapsed().as_secs_f64()*1e6;let after=count(epoch);let byte_count=count(bytes)-charged;
    let counts:Vec<Vec<u64>>=metrics.iter().map(|f|f.map_or(vec![],|f|(0..16).map(|i|unsafe{f(0,i,0)}).collect())).collect();
    let sends:std::collections::BTreeMap<_,_>=watched.iter().enumerate().map(|(i,s)|(*s,metrics[0].map_or(0,|f|unsafe{f(3,i,0)}))).collect();
    samples.push(serde_json::json!({"us":micros/repetitions as f64,"allocations":(after-before) as f64/repetitions as f64,"id_bytes":byte_count as f64/repetitions as f64,"counts":counts,"sends":sends}));
   }
   let row=serde_json::json!({"selector":String::from_utf8_lossy(selector),"automatic_gc":automatic,"mode":mode,"nodes":expected,"instrumented":metrics[0].is_some(),"repetitions":repetitions,"samples":samples});
   println!("{}",row);results.push(row);
  }
 }
 std::fs::write(&args[4],serde_json::to_vec_pretty(&results).unwrap()).unwrap();
}
