//! Compilation and generated String literals do not expand buffers into Sequences.
#[allow(dead_code)] #[path="support/minimal.rs"] mod minimal;
#[test] #[ignore="requires native modules and Lang seed"]
fn compilation_uses_bounded_byte_messages(){
 let h=minimal::native_floor();let q=h.scope();
 q.read_bytes(q.string(b"initialize").unwrap()).unwrap();
 for name in ["peg","lang_seed"]{h.load_native(&q,&h.resources.lib_dir.join(format!("libbirnam_{name}.dylib")),&[h.namespace]).unwrap();}
 let path=std::env::temp_dir().join(format!("birnam-bounded-sequence-{}.dylib",std::process::id()));
 assert!(std::process::Command::new("clang").args(["-dynamiclib","-O2","tests/fixtures/oop-host/bounded-sequence.c","-o"]).arg(&path).current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).status().unwrap().success());
 let fixture=h.load_native(&q,&path,&[]).unwrap();std::fs::remove_file(path).unwrap();
 let vt=q.call(h.id.root,b"behaviorOf:",&[h.id.root]).unwrap();let seq=h.intern(b"sequence").unwrap();let alias=h.intern(b"boundedOriginalSequence").unwrap();
 let original=q.call(vt,b"lookup",&[seq]).unwrap();q.call(vt,b"addMethod",&[alias,original]).unwrap();
 let guard=q.call(fixture,b"methodAt:",&[h.intern(b"guard").unwrap()]).unwrap();let guard=q.call(guard,b"withContext:",&[alias]).unwrap();
 q.call(vt,b"addMethod",&[seq,guard]).unwrap();
 let data="a".repeat(32768);let source=format!("\"{data}\"");
 for selector in [b"compileSynchronous:".as_slice(),b"compileResumable:"]{
  let method=q.call(h.named(b"Codegen").unwrap(),selector,&[q.string(source.as_bytes()).unwrap()]).unwrap();
  let block=q.call(h.named(b"Block").unwrap(),b"fromMethod:captureOperands:",&[method]).unwrap();
  let result=q.call(block,b"value",&[]).unwrap();assert_eq!(q.read_bytes(result).unwrap(),data.as_bytes());
 }
 q.call(vt,b"addMethod",&[seq,original]).unwrap();q.call(vt,b"removeMethod:",&[alias]).unwrap();
}
