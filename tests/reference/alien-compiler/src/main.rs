use birnam_alien_compiler_reference::{codegen::emit_native, parse_signature};
use std::io::{self, BufRead};
fn decode(s: &str) -> Vec<u8> {
    assert!(s.len() % 2 == 0);
    s.as_bytes()
        .chunks(2)
        .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
        .collect()
}
fn hex(s: &[u8]) -> String {
    s.iter().map(|b| format!("{b:02x}")).collect()
}
fn main() {
    for line in io::stdin().lock().lines() {
        let line = line.unwrap();
        let (name, signature) = line.split_once('\t').unwrap();
        let sig = decode(signature);
        let name = if name == "-" {
            None
        } else {
            Some(decode(name))
        };
        let answer = parse_signature(&sig)
            .map_err(|e| e.to_string())
            .and_then(|s| emit_native(name.as_deref(), &s).map_err(|e| e.to_string()));
        match answer {
            Ok(c) => println!("ok\t{}", hex(c.as_bytes())),
            Err(e) => println!("err\t{}", hex(e.as_bytes())),
        }
    }
}
