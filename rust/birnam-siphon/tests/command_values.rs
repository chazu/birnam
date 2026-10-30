#![allow(dead_code)]
#[path="support/minimal.rs"] mod minimal;
#[path="support/oop.rs"] mod oop;
use birnam_host_support::{native,project::*};
use std::{ffi::OsString,os::unix::ffi::OsStringExt,path::PathBuf};
#[test] #[ignore="requires native module composition"]
fn command_records_use_objects_and_preserve_raw_paths_buffers_and_options(){
 let h=minimal::native_floor();let q=h.scope();
 // Both command and project builders must use live add: and retain values
 // through collection inside that message; a failure must propagate.
 let hooks=oop::hooks::Hooks::new(&h);let vt=h.named(b"VectorVt").unwrap();let add=h.intern(b"add:").unwrap();
 let original=q.call(vt,b"lookup",&[add]).unwrap();
 let collecting=hooks.context(&[original,h.named(b"GC").unwrap(),birnam_abi::size(0).unwrap()]);
 q.call(vt,b"addMethod",&[add,hooks.method(b"collectEvery",collecting)]).unwrap();
 let strings=vec!["first".to_owned(),"second".to_owned()];
 let encoded=native::encode(&q,&strings).unwrap();
 assert_eq!(native::decode::<Vec<String>>(&q,&[encoded],0).unwrap(),strings);
 let dependencies=vec![birnam_project_support::ProjectDependencyObservation{name:"first".into(),root:"/tmp/first".into()}];
 let encoded=birnam_project_support::values::Builder{q:&q}.dependencies(&dependencies).unwrap();
 assert_eq!(birnam_project_support::values::decode_dependencies(&q,encoded).unwrap(),dependencies);
 q.call(vt,b"addMethod",&[add,original]).unwrap();assert!(hooks.count(collecting,2)>0);
 let denied=hooks.context(&[hooks.fault(71),birnam_abi::size(0).unwrap()]);
 q.call(vt,b"addMethod",&[add,hooks.method(b"deny",denied)]).unwrap();
 assert!(native::encode(&q,&strings).is_err());
 assert!(birnam_project_support::values::Builder{q:&q}.dependencies(&dependencies).is_err());
 q.call(vt,b"addMethod",&[add,original]).unwrap();
 let path=PathBuf::from(OsString::from_vec(b"/tmp/path-\xff".to_vec()));
 let options=RunOptions{directory:path.clone(),build:false,profile:"release".into(),repl:Some(ReplMode::Socket),socket_bind:Some("127.0.0.1".into()),socket_port:Some(0),arguments:vec![OsString::from_vec(b"arg-\xfe".to_vec()),OsString::new()]};
 let encoded=native::encode(&q,&options).unwrap();q.exact(encoded,b"MapVt").unwrap();q.call(h.named(b"GC").unwrap(),b"collectPrecisely",&[]).unwrap();
 assert_eq!(native::decode::<RunOptions>(&q,&[encoded],0).unwrap(),options);
 let build=ProjectBuildResult{output_root:path.clone(),module_root:path.clone(),configuration:path.clone(),application:Some(path),artifacts:vec![]};
 assert_eq!(native::decode::<ProjectBuildResult>(&q,&[native::encode(&q,&build).unwrap()],0).unwrap(),build);
 let source=ProjectSourceObservation{name:"test".into(),path:"test.birnam".into(),sources:vec![],project_root:"/tmp".into(),source:(0..=255).cycle().take(16384).collect(),requires:vec!["Object".into()],source_digest:"x".into(),revision_digest:"y".into()};
 let encoded=native::encode(&q,&source).unwrap();let bytes=q.call(encoded,b"at:",&[q.string(b"source").unwrap()]).unwrap();q.exact(bytes,b"BytesVt").unwrap();
 assert_eq!(q.read_bytes(bytes).unwrap(),source.source);
 assert_eq!(native::decode::<ProjectSourceObservation>(&q,&[encoded],0).unwrap(),source);
 assert!(native::decode::<RunOptions>(&q,&[q.string(b"{}").unwrap()],0).is_err(),"JSON text is not an object record");
}
