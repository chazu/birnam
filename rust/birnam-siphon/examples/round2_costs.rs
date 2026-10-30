//! Reproducible microbenchmarks for the second simplification round.
use std::{hint::black_box, time::Instant};
use birnam_artifact_codec::portable_program::{self as p, NodeSpec, StatementSpec, ProgramSpec, ValidatedProgram};
fn main() {
    for n in [1000, 10000] {
        let nodes=vec![NodeSpec::String(b"abc"); n];
        let statements:Vec<_>=(0..n).map(|i|StatementSpec::Expression{root_node:i as u32}).collect();
        let bytes=p::encode(ProgramSpec{parameters:&[],captures:&[],lets:&[],statements:&statements,nodes:&nodes}).unwrap();
        let checked=ValidatedProgram::new(bytes.clone()).unwrap();
        let view=checked.view();
        for sample in 0..5 {
            let start=Instant::now();
            for _ in 0..5 { for i in 0..n { assert_eq!(black_box(&view).node(black_box(i)).unwrap().data,b"abc"); } }
            println!("{{\"operation\":\"portable_lookup\",\"n\":{n},\"sample\":{sample},\"ns\":{}}}",start.elapsed().as_nanos()/5);
            let start=Instant::now(); for _ in 0..5 {black_box(p::inspect(black_box(&bytes)).unwrap());}
            println!("{{\"operation\":\"portable_validate\",\"n\":{n},\"sample\":{sample},\"ns\":{}}}",start.elapsed().as_nanos()/5);
        }
    }
}
