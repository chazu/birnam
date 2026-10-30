#![allow(dead_code)]
#[path="support/minimal.rs"] mod minimal;
use birnam_host_support::{native,project::*};
use std::{ffi::OsString,os::unix::ffi::OsStringExt,path::PathBuf};
#[test] #[ignore="requires native module composition"]
fn command_records_use_objects_and_preserve_raw_paths_buffers_and_options(){
 let h=minimal::native_floor();let q=h.scope();
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
