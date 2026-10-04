# Alien compiler reference

This standalone, dependency-free Rust package freezes the compiler before its
Birnam rewrite. It is a test oracle: it is outside the production workspace,
exports no native module and is never installed. Keep the grammar, ABI classifier,
emitter and prelude stable so the authored implementation has an independent
behavioral comparison.

Run `cargo test --manifest-path tests/reference/alien-compiler/Cargo.toml --offline --locked`
for the original unit tests, or `mise exec -- python3 scripts/alien-compiler-test.py PREFIX`
for exact installed Birnam C/diagnostic comparison and ABI checks. The latter runs
without the standard library, outside the checkout, using the compiled artifact
and embedded C assets.
