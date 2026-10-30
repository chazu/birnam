//! Provider indexes recognize known Methods/captures without interpreting Id data.
#[allow(dead_code)] #[path="support/minimal.rs"] mod minimal;
#[allow(dead_code)] #[path="support/oop.rs"] mod oop;
use birnam_abi::size;
#[test] #[ignore="requires native modules and Lang seed"]
fn known_methods_and_captures_do_not_scan_and_aliases_are_weak(){
 let h=minimal::native_floor();let q=h.scope();for name in ["peg","lang_seed"]{h.load_native(&q,&h.resources.lib_dir.join(format!("libbirnam_{name}.dylib")),&[h.namespace]).unwrap();}
 let hooks=oop::hooks::Hooks::new(&h);let cg=h.named(b"Codegen").unwrap();let gc=h.named(b"GC").unwrap();let exec=h.named(b"Execution").unwrap();
 let rootvt=q.call(h.id.root,b"behaviorOf:",&[h.id.root]).unwrap();let compare=hooks.symbol(b"sameNativeMethod:as:");let saved=q.call(rootvt,b"lookup",&[compare]).unwrap();
 let denied=hooks.context(&[hooks.fault(71),size(0).unwrap()]);let deny=hooks.method(b"deny",denied);
 for selector in [b"compileSynchronous:".as_slice(),b"compileResumable:"]{
  let method=q.call(cg,selector,&[q.string(b"[:self | 37]").unwrap()]).unwrap();
  let source=q.call(method,b"codeObject",&[]).unwrap();let alias=q.call(method,b"captured",&[]).unwrap();
  q.call(rootvt,b"addMethod",&[compare,deny]).unwrap();
  for value in [method,alias]{assert_eq!(q.call(value,b"codeObject",&[]).unwrap(),source);q.call(value,b"isSynchronous",&[]).unwrap();q.call(exec,b"bodyOfMethod:",&[value]).unwrap();}
  q.call(rootvt,b"addMethod",&[compare,saved]).unwrap();
  let expired={let local=h.scope();local.call(alias,b"captured",&[]).unwrap()};
  q.call(gc,b"collectPrecisely",&[]).unwrap();
  assert!(unsafe{h.id.send(h.id.root,h.intern(b"behaviorOf:").unwrap(),&[expired])}.is_err());
  assert_eq!(q.call(alias,b"codeObject",&[]).unwrap(),source);
 }
 assert_eq!(hooks.count(denied,1),0);
}
