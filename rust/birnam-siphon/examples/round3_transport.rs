#![allow(dead_code)]
#[path="../tests/support/minimal.rs"] mod minimal;
use birnam_host_support::{native,project::ProjectSourceObservation};
use std::time::Instant;
fn main(){let h=minimal::native_floor();for n in [8192usize,262144]{let value=ProjectSourceObservation{name:"bench".into(),path:"bench.birnam".into(),sources:vec!["bench.birnam".into()],project_root:"/tmp".into(),source:vec![b'x';n],requires:vec![],source_digest:"digest".into(),revision_digest:"revision".into()};for sample in 0..5{let q=h.scope();q.call(h.named(b"GC").unwrap(),b"collectPrecisely",&[]).unwrap();let start=Instant::now();let object=native::encode(&q,&value).unwrap();let decoded:ProjectSourceObservation=native::decode(&q,&[object],0).unwrap();let ns=start.elapsed().as_nanos();assert_eq!(decoded,value);println!("{{\"operation\":\"host_record_roundtrip\",\"n\":{n},\"sample\":{sample},\"ns\":{ns}}}");}}}
