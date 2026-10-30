#![allow(dead_code)]
#[path="../tests/support/oop.rs"] mod oop;
use std::{path::Path,time::Instant,os::unix::ffi::OsStrExt};
fn main(){let h=oop::bootstrap();let q=h.scope();
 let r=&h.resources;let mut args=vec![h.namespace];
 for path in [&r.prefix,&r.lib_dir,&r.rust_workspace_dir,&r.guide_dir,&r.id_dylib]{args.push(q.string(path.as_os_str().as_bytes()).unwrap());}
 h.load_native(&q,&r.lib_dir.join("libbirnam_host_entry.dylib"),&args).unwrap();
 h.owner_call(&q,b"HostEntry",b"bootstrap:tools:",&[h.named(b"false").unwrap(),h.named(b"false").unwrap()]).unwrap();

 for file in ["modules/toml.birnam","modules/process.birnam"]{q.evaluate(&std::fs::read(file).unwrap(),Path::new(file),None).unwrap();}
 // Load only the definition under test; do not reset the Module registry.
 let module=std::fs::read_to_string("modules/modules.birnam").unwrap();let start=module.find("[Reflect defineMethod: #composeSources:").unwrap();let end=module[start..].find("\n// Inspect and validate").unwrap()+start;q.evaluate(module[start..end].as_bytes(),Path::new("<composeSources>"),None).unwrap();
 let gc=h.named(b"GC").unwrap();let unescape=h.intern(b"tomlUnescape").unwrap();
 for n in [128usize,4096]{let input=q.string(&vec![b'x';n]).unwrap();for sample in 0..5 {q.call(gc,b"collectPrecisely",&[]).unwrap();let start=Instant::now();let out=unsafe{h.id.send(input,unescape,&[])}.unwrap();let ns=start.elapsed().as_nanos();assert_eq!(q.string_bytes(out).unwrap(),vec![b'x';n]);println!("{{\"operation\":\"toml_plain\",\"n\":{n},\"sample\":{sample},\"ns\":{ns}}}");}}
 let temp=std::env::temp_dir().join(format!("birnam-round3-compose-{}",std::process::id()));std::fs::write(&temp,vec![b'x';4096]).unwrap();let path=q.string(temp.to_str().unwrap().as_bytes()).unwrap();let module=h.global(&q,b"Module").unwrap();let compose=h.intern(b"composeSources:").unwrap();
 for n in [16usize,256]{let mut paths=q.call(h.named(b"Vector").unwrap(),b"new",&[]).unwrap();for _ in 0..n{paths=q.call(paths,b"add:",&[path]).unwrap();}for sample in 0..5 {q.call(gc,b"collectPrecisely",&[]).unwrap();let start=Instant::now();let out=unsafe{h.id.send(module,compose,&[paths])}.unwrap();let ns=start.elapsed().as_nanos();assert_eq!(q.string_bytes(out).unwrap().len(),n*4097);println!("{{\"operation\":\"compose\",\"n\":{n},\"sample\":{sample},\"ns\":{ns}}}");}}
 std::fs::remove_file(temp).unwrap();
 let process=h.global(&q,b"Process").unwrap();let output=h.intern(b"output:").unwrap();for n in [8192usize,262144]{let cmd=q.string(format!("head -c {n} /dev/zero | tr '\\0' x").as_bytes()).unwrap();for sample in 0..5{q.call(gc,b"collectPrecisely",&[]).unwrap();let start=Instant::now();let out=unsafe{h.id.send(process,output,&[cmd])}.unwrap();let ns=start.elapsed().as_nanos();assert_eq!(q.string_bytes(out).unwrap(),vec![b'x';n]);println!("{{\"operation\":\"process_output\",\"n\":{n},\"sample\":{sample},\"ns\":{ns}}}");}}
}
