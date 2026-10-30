#![allow(dead_code)]
#[path="../tests/support/minimal.rs"] mod minimal;
use birnam_project_support::{values::Builder,ProjectDependencyObservation};
use std::time::Instant;
fn main(){let h=minimal::native_floor();for n in [128usize,2048]{let items=(0..n).map(|i|ProjectDependencyObservation{ name:format!("p{i}"),root:format!("/tmp/p{i}") }).collect::<Vec<_>>();for sample in 0..5 {let q=h.scope();let start=Instant::now();let result=Builder{q:&q}.dependencies(&items).unwrap();let ns=start.elapsed().as_nanos();assert_eq!(q.vector_size(result).unwrap(),n);println!("{{\"operation\":\"project_records\",\"n\":{n},\"sample\":{sample},\"ns\":{ns}}}");}}}
