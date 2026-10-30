use birnam_codegen::resumable::*;
use birnam_native_backend::oop_emission;
use birnam_artifact::portable_program::{self as p,NodeSpec,ProgramSpec,StatementSpec};
use std::{hint::black_box,time::Instant};
fn main(){
 for n in [256usize,4096]{
  let program=LogicalProgram{entry:ProgramCounter(0),argument_count:0,capture_count:0,value_count:1,binding_count:0,call_site_count:0,blocks:(0..n).map(|i|BasicBlock{pc:ProgramCounter((2*i) as u32),operations:vec![],terminator:if i+1<n{Terminator::Continue(ProgramCounter((2*(i+1)) as u32))}else{Terminator::Return(ValueId(0))}}).collect()};
  for sample in 0..5 {let start=Instant::now();let result=reference_liveness(&program);let ns=start.elapsed().as_nanos();assert_eq!(result[0].live_in,vec![ValueId(0)]);black_box(result);println!("{{\"operation\":\"liveness\",\"n\":{n},\"sample\":{sample},\"ns\":{ns}}}");}
  let names=(0..n).map(|i|format!("symbol_{i}").into_bytes()).collect::<Vec<_>>();let nodes=names.iter().map(|s|NodeSpec::Symbol(s)).collect::<Vec<_>>();let statements=(0..n).map(|i|StatementSpec::Expression{root_node:i as u32}).collect::<Vec<_>>();
  let bytes=p::encode(ProgramSpec{parameters:&[],captures:&[],lets:&[],statements:&statements,nodes:&nodes}).unwrap();
  for sample in 0..5 {let start=Instant::now();let result=oop_emission::compile(&bytes).unwrap();let ns=start.elapsed().as_nanos();black_box(result);println!("{{\"operation\":\"names\",\"n\":{n},\"sample\":{sample},\"ns\":{ns}}}");}
 }
}
