//! Diagnostic send counts for actual installed task policy. Requires Id's
//! dispatch-metrics feature; these counts are not wall/CPU measurements.
#[path = "../tests/support/mod.rs"] mod support;
use support::*;
use birnam_abi::{Oop,borrowed_bytes,id_protocol::Operand};
use std::os::unix::ffi::OsStrExt;
fn main() { unsafe {
 let id=id();let ns=load(id,"bootstrap",&[Operand::bytes(directory().as_os_str().as_bytes())]);
 let named=|name:&[u8]|object(send(id,ns,b"resolveGlobal:",&[Operand::object(id.intern_name(name).unwrap())]));
 let roots=named(b"Roots");let keep=|o:Oop|{retain(id,roots,o);o};
 let text=|b:&[u8]|keep(object(send(id,named(b"String"),b"fromBytes:",&[Operand::bytes(b)])));
 for module in ["stdlib","stacklessControl"] {
  let path=directory().join(format!("libbirnam_{module}.dylib"));
  let answer=object(send(id,named(b"Codegen"),b"load:",&[Operand::object(text(path.as_os_str().as_bytes()))]));
  assert_eq!(object(send(id,answer,b"isError",&[])),named(b"false"));
  send(id,named(b"Module"),b"markLoaded:",&[Operand::object(id.intern_name(module.as_bytes()).unwrap())]);
 }
 let body=keep(object(send(id,named(b"Codegen"),b"eval:",&[Operand::object(text(b"[:task | 42]"))])));
 let executor=named(b"TaskExecutor");let field=keep(object(send(id,executor,b"field:",&[Operand::object(id.intern_name(b"current").unwrap())])));
 let mut cases=vec![("current",executor,b"current".as_slice(),vec![]),
  ("cached-field",field,b"value",vec![]),
  ("field-handle",executor,b"field:",vec![Operand::object(id.intern_name(b"current").unwrap())]),
  ("task",named(b"TaskControl"),b"start:",vec![Operand::object(body)])];
 if named(b"TaskField")!=named(b"nil") {
  let index=keep(object(send(id,named(b"Integer"),b"fromWord:",&[Operand::word(3)])));
  cases.push(("direct-slot",named(b"ObjectModel"),b"slot:in:",vec![Operand::object(index),Operand::object(executor)]));
 }
 for (label,receiver,selector,args) in cases {
  send(id,receiver,selector,&args);
  collect(id,named(b"GC"));
  send(id,id.receiver,b"dispatchCountsBegin",&[]);
  let result=send(id,receiver,selector,&args);
  let raw=send(id,id.receiver,b"dispatchCountsEnd",&[]).as_bytes().unwrap();
  let bytes=borrowed_bytes(raw.pointer,raw.length).unwrap().to_vec();
  let returned=keep(object(result));
  if matches!(label,"current"|"cached-field"|"direct-slot") { assert_eq!(returned,named(b"nil")); }
  if label=="field-handle" { assert_eq!(object(send(id,returned,b"value",&[])),named(b"nil")); }
  if label=="task" {
   let outcome=keep(object(send(id,returned,b"pollOutcome",&[])));
   let at=|index|{let i=keep(object(send(id,named(b"Integer"),b"fromWord:",&[Operand::word(index)])));keep(object(send(id,outcome,b"at:",&[Operand::object(i)])))};
   assert_eq!(at(0),id.intern_name(b"success").unwrap());
   assert_eq!(send(id,named(b"Integer"),b"wordOf:",&[Operand::object(at(1))]).as_word(),Some(42));
  }
  let mut counts=std::collections::BTreeMap::new();
  for line in std::str::from_utf8(&bytes).unwrap().lines() {
   let (count,hex)=line.split_once('\t').unwrap();
   let name=String::from_utf8((0..hex.len()).step_by(2).map(|i|u8::from_str_radix(&hex[i..i+2],16).unwrap()).collect()).unwrap();
   if name!="dispatchCountsEnd" {counts.insert(name,count.parse::<usize>().unwrap());}
  }
  let total:usize=counts.values().sum();
  println!("{}",serde_json::json!({"case":label,"total_sends":total,"selectors":counts,"object_result":result.as_object().is_some()}));
 }
} }
