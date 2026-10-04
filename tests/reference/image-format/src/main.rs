use std::io::{self, BufRead};
use birnam_image_format_reference::v8;
fn main() {
    for line in io::stdin().lock().lines() {
        let path = line.unwrap();
        let bytes = std::fs::read(path).unwrap();
        match v8::decode(&bytes, 1, 0xcdb6_41f9_7854_0d45) {
            Err(_) => println!("err"),
            Ok(v) => {
                let bytes = v8::compose(v8::ImageSpec {
                    architecture: 1, runtime_abi: 0xcdb6_41f9_7854_0d45,
                    programs: v.programs, bindings: v.bindings, selection: v.selection,
                    heaps: &v.heaps, facts: &v.facts, sources: v.sources,
                    behaviors: v.behaviors, code: v.code.map(|b| &b[72..]),
                }).unwrap();
                println!("ok\t{}", bytes.iter().map(|b| format!("{b:02x}")).collect::<String>());
            }
        }
    }
}
