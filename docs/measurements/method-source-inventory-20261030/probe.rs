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
fn main(){
 let args:Vec<_>=std::env::args().collect(); let h=context(PathBuf::from(&args[1]));
 if std::env::var_os("INVENTORY_BOOTSTRAP_ONLY").is_some() {println!("BOOTSTRAP_COMPLETED");return}
 let q=h.scope();
 let codegen=h.named(b"Codegen").unwrap();let lang=h.named(b"Lang").unwrap();
 let source=q.string(&std::fs::read(&args[2]).unwrap()).unwrap();
 let mode=std::env::var("INVENTORY_COMPILE_MODE").unwrap_or_else(|_| "compile:".into());
 let method=q.call(codegen,mode.as_bytes(),&[source]).unwrap();q.check_result(method).unwrap();
 let empty=q.call(h.named(b"Vector").unwrap(),b"new",&[]).unwrap();
 let block=q.call(h.named(b"Block").unwrap(),b"fromMethod:captures:",&[method,empty]).unwrap();
 let installed=q.call(block,b"value",&[]).unwrap();q.check_result(installed).unwrap();assert_eq!(installed,codegen);
 let mut fixtures=vec![
  ("empty".to_owned(),"42".to_owned()),
  ("cases".to_owned(),r##"[Reflect defineMethod: #one on: Object as: [:self | 1]];
[| [Reflect defineMethod: #two on: Object as: [:self | 2] doc: "two"]];
let quoted := "[Reflect defineMethod: #ignored on: Object as: [:self | 3]]";
let dynamic := [:s | [Reflect defineMethod: s on: Object as: [:self | 4]]];
let shadow := [:Reflect | [Reflect defineMethod: #syntactic on: Object as: [:self | 5]]];
[Reflect defineMethod: #one on: Object as: [:self | 1]]"##.to_owned()),
  ("sparse".to_owned(),(0..50).map(|i|format!("let binding{i} := {i};\n")).collect()),
  ("dense".to_owned(),(0..32).map(|i|format!("[Reflect defineMethod: #method{i} on: Object as: [:self | {i}]];\n")).collect()),
  ("nested".to_owned(),format!("{}[Reflect defineMethod: #deep on: Object as: [:self | 1]]{}","[| ".repeat(32),"]".repeat(32)))
 ];
 fixtures.push(("registry_module".to_owned(),std::fs::read_to_string("modules/source-registry.birnam").unwrap()));
 let mut results=vec![];
 for (name,source) in fixtures {
  if let Ok(selected)=std::env::var("INVENTORY_CASE") {if name!=selected {continue}}
  let q=h.scope(); let text=q.string(source.as_bytes()).unwrap(); let tree=q.call(lang,b"parse:",&[text]).unwrap();q.check_result(tree).unwrap();
  let projected={let p=h.scope();project(&p,&h,tree)};let expected=birnam_codegen::source_revision::method_sources(&projected,source.as_bytes());
  {let p=h.scope();let got=p.call(codegen,b"methodSourceTextsIn:source:",&[tree,text]).unwrap();p.check_result(got).unwrap();assert_eq!(rows(&p,&h,got),expected,"{name}");}
  let rust_us:Vec<f64>=measure(||{for _ in 0..1000 {black_box(birnam_codegen::source_revision::method_sources(&projected,source.as_bytes()));}},2,7).into_iter().map(|v|v/1000.0).collect();
  if std::env::var_os("INVENTORY_PROFILE").is_some() {eprintln!("PROFILE_READY");}
  let authored_us=measure(||{let p=h.scope();let got=p.call(codegen,b"methodSourceTextsIn:source:",&[tree,text]).unwrap();black_box(got);},2,if std::env::var_os("INVENTORY_PROFILE").is_some() {20} else {7});
  let native_metadata_us=measure(||{let p=h.scope();let got=p.call(codegen,b"analyzeSourceMetadata:",&[text]).unwrap();p.check_result(got).unwrap();black_box(got);},2,7);
  let item=serde_json::json!({"compilation":mode,"name":name,"bytes":source.len(),"nodes":nodes(&projected),"declarations":expected.len(),"native_inventory_us":rust_us,"authored_inventory_us":authored_us,"native_metadata_us":native_metadata_us});
  println!("{}",item);results.push(item);
 }
 std::fs::write(&args[3],serde_json::to_vec_pretty(&results).unwrap()).unwrap();
}
