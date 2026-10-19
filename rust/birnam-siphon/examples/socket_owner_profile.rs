//! Untimed message census of installed socket/Cell policies; requires Id dispatch-metrics.
#[path = "../tests/support/mod.rs"] mod support;
use support::*;
use birnam_abi::{Oop,borrowed_bytes,id_protocol::Operand};
use std::os::unix::ffi::OsStrExt;
fn main() { unsafe {
    let id=id(); let ns=load(id,"bootstrap",&[Operand::bytes(directory().as_os_str().as_bytes())]);
    let named=|name:&[u8]|object(send(id,ns,b"resolveGlobal:",&[Operand::object(id.intern_name(name).unwrap())]));
    let roots=named(b"Roots"); let keep=|o:Oop|{retain(id,roots,o);o};
    let text=|b:&[u8]|keep(object(send(id,named(b"String"),b"fromBytes:",&[Operand::bytes(b)])));
    for module in ["stdlib","stacklessControl","socket"] {
        let path=directory().join(format!("libbirnam_{module}.dylib"));
        let answer=object(send(id,named(b"Codegen"),b"load:",&[Operand::object(text(path.as_os_str().as_bytes()))]));
        assert_ne!(answer,named(b"nil"));
        assert_eq!(object(send(id,answer,b"isError",&[])),named(b"false"));
        send(id,named(b"Module"),b"markLoaded:",&[Operand::object(id.intern_name(module.as_bytes()).unwrap())]);
    }
    let eval=|s:&[u8]|keep(object(send(id,named(b"Codegen"),b"eval:",&[Operand::object(text(s))])));
    let invalid=eval(b"-1"); let pair=eval(b"#[nil nil]"); let cell=eval(b"[Cell with: #[nil nil]]");
    let socket=named(b"Socket");
    let conn=keep(object(send(id,socket,b"wrapFd:",&[Operand::object(invalid)])));
    for (label,receiver,selector,args) in [
        ("fd",conn,b"fd".as_slice(),vec![]),
        ("cell-value",cell,b"value",vec![]),
        ("cell-with",named(b"Cell"),b"with:",vec![Operand::object(pair)]),
        ("wrap",socket,b"wrapFd:",vec![Operand::object(invalid)]),
        ("close",conn,b"close",vec![]),
    ] {
        // Close needs a fresh owner for both warming and measurement.
        let warm=if label=="close" { keep(object(send(id,socket,b"wrapFd:",&[Operand::object(invalid)]))) } else { receiver };
        let warmed=keep(object(send(id,warm,selector,&args)));
        if label=="wrap" {send(id,warmed,b"close",&[]);}
        collect(id,named(b"GC"));
        send(id,id.receiver,b"dispatchCountsBegin",&[]);
        let result=send(id,receiver,selector,&args);
        let raw=send(id,id.receiver,b"dispatchCountsEnd",&[]).as_bytes().unwrap();
        let bytes=borrowed_bytes(raw.pointer,raw.length).unwrap().to_vec();
        let returned=keep(object(result));
        assert_eq!(object(send(id,returned,b"isError",&[])),named(b"false"));
        let number=|value|send(id,named(b"Integer"),b"wordOf:",&[Operand::object(value)]).as_word().unwrap();
        if matches!(label,"fd"|"close") {assert_eq!(number(returned),usize::MAX);}
        if label=="wrap" {assert_eq!(number(object(send(id,returned,b"fd",&[]))),usize::MAX);}
        if label=="cell-value" {assert_eq!(number(object(send(id,returned,b"size",&[]))),2);}
        if label=="cell-with" {assert_eq!(object(send(id,returned,b"value",&[])),pair);}
        if label=="wrap" {send(id,returned,b"close",&[]);}

        let mut counts=std::collections::BTreeMap::new();
        for line in std::str::from_utf8(&bytes).unwrap().lines() {
            let (count,hex)=line.split_once('\t').unwrap();
            let name=String::from_utf8((0..hex.len()).step_by(2).map(|i|u8::from_str_radix(&hex[i..i+2],16).unwrap()).collect()).unwrap();
            if name!="dispatchCountsEnd" {counts.insert(name,count.parse::<u64>().unwrap());}
        }
        let total:u64=counts.values().sum();
        println!("{}",serde_json::json!({"case":label,"total_sends":total,"selectors":counts}));
    }
} }
