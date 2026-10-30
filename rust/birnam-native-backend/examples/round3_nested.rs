use birnam_artifact::{portable_program::{self as p,NodeSpec,ProgramSpec,StatementSpec},portable_inventory};
use std::{hint::black_box,time::Instant};
fn encode(nodes:&[NodeSpec<'_>])->Vec<u8>{p::encode(ProgramSpec{parameters:&[],captures:&[],lets:&[],statements:&[StatementSpec::Expression{root_node:0}],nodes}).unwrap()}
fn main(){for n in [8,64,128]{let mut bytes=encode(&[NodeSpec::Nil]);for _ in 0..n{bytes=encode(&[NodeSpec::BlockLiteral{program:&bytes,captures:&[]}]);}
for sample in 0..5 {for operation in ["inventory","emit_nested"]{let start=Instant::now();let result=if operation=="inventory" {portable_inventory::encode_closure(&[&bytes]).unwrap()}else{birnam_native_backend::oop_emission::compile(&bytes).unwrap()};let ns=start.elapsed().as_nanos();black_box(result);println!("{{\"operation\":\"{operation}\",\"n\":{n},\"sample\":{sample},\"ns\":{ns}}}");}}}}
